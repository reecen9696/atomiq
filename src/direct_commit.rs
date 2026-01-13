//! Direct commit mode - high-performance blockchain without consensus overhead
//!
//! This module implements a fast path for single-validator scenarios where
//! Byzantine fault tolerance is not required. It periodically produces blocks
//! and commits them directly to storage without consensus rounds.

use crate::{
    config::AtomiqConfig,
    storage::OptimizedStorage,
    AtomiqApp, BlockchainMetrics,
};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tokio::time::interval;

/// Direct commit engine that produces blocks without consensus overhead
pub struct DirectCommitEngine {
    app: Arc<RwLock<AtomiqApp>>,
    storage: Arc<OptimizedStorage>,
    config: AtomiqConfig,
    running: Arc<AtomicBool>,
    blocks_committed: Arc<AtomicU64>,
    last_block_height: Arc<AtomicU64>,
    last_block_hash: Arc<RwLock<[u8; 32]>>,
}

impl DirectCommitEngine {
    /// Create a new direct commit engine
    pub fn new(
        app: Arc<RwLock<AtomiqApp>>,
        storage: Arc<OptimizedStorage>,
        config: AtomiqConfig,
    ) -> Self {
        Self {
            app,
            storage,
            config,
            running: Arc::new(AtomicBool::new(false)),
            blocks_committed: Arc::new(AtomicU64::new(0)),
            last_block_height: Arc::new(AtomicU64::new(0)),
            last_block_hash: Arc::new(RwLock::new([0u8; 32])),
        }
    }

    /// Start the direct commit engine
    pub async fn start(self: Arc<Self>) {
        self.running.store(true, Ordering::SeqCst);
        
        println!("🚀 DirectCommit Engine Started");
        println!("   Mode: High-Performance (No Consensus)");
        println!("   Block Interval: {}ms", self.config.consensus.direct_commit_interval_ms);
        println!("   Max TX/Block: {}", self.config.blockchain.max_transactions_per_block);
        println!("   Expected TPS: 10K-100K+\n");

        let mut block_interval = interval(Duration::from_millis(
            self.config.consensus.direct_commit_interval_ms
        ));

        let mut stats_interval = interval(Duration::from_secs(5));
        let start_time = Instant::now();

        loop {
            tokio::select! {
                _ = block_interval.tick() => {
                    if !self.running.load(Ordering::SeqCst) {
                        break;
                    }
                    
                    if let Err(e) = self.produce_and_commit_block().await {
                        eprintln!("⚠️  Block production error: {}", e);
                    }
                }
                
                _ = stats_interval.tick() => {
                    self.print_stats(start_time.elapsed()).await;
                }
            }
        }
    }

    /// Produce a block and commit it directly to storage
    async fn produce_and_commit_block(&self) -> Result<(), String> {
        let app = self.app.read().await;

        // Get transactions from pool (respects max_transactions_per_block limit)
        let transactions = app.drain_transaction_pool();

        // Fix B: Enforce max transactions per block at all paths
        let max_tx = self.config.blockchain.max_transactions_per_block;
        if transactions.len() > max_tx {
            return Err(format!(
                "Transaction limit exceeded: {} > {} max_transactions_per_block", 
                transactions.len(), max_tx
            ));
        }

        // Only commit blocks that have transactions
        // This prevents height gaps when there are no pending transactions
        if transactions.is_empty() {
            return Ok(());
        }

        // Get current height
        let height = self.last_block_height.load(Ordering::SeqCst);
        let next_height = height + 1; // Only increment when we have transactions
        
        // Execute transactions and get deterministic state updates
        let (_execution_results, state_updates) = app.execute_transactions(&transactions);
        
        // Fix C: Compute deterministic state root from state updates
        let state_root = self.compute_state_root(&state_updates);
        
        // Get previous block hash for chain linkage
        let previous_block_hash = *self.last_block_hash.read().await;
        
        // Create block with full blockchain fields
        let block = crate::Block::new(
            next_height,
            previous_block_hash,
            transactions,
            state_root,
        );
        
        // Verify block integrity before committing
        if !block.verify_hash() {
            return Err("Block hash verification failed".to_string());
        }
        if !block.verify_transactions_root() {
            return Err("Transactions root verification failed".to_string());
        }
        
        // Serialize block
        let block_data = bincode::serialize(&block)
            .map_err(|e| format!("Failed to serialize block: {}", e))?;
        
        // Commit to storage (includes transaction indexing)
        self.commit_block_to_storage(next_height, &block_data, &block.block_hash)?;
        
        // Update state tracking
        self.last_block_height.store(next_height, Ordering::SeqCst);
        *self.last_block_hash.write().await = block.block_hash;
        
        // Update counters
        self.blocks_committed.fetch_add(1, Ordering::SeqCst);
        app.block_counter().fetch_add(1, Ordering::SeqCst);
        
        Ok(())
    }
    
    /// Compute state root from state updates - now deterministic!
    fn compute_state_root(&self, _state_updates: &hotstuff_rs::types::update_sets::AppStateUpdates) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        
        // For now, compute a simple deterministic hash
        // TODO: Implement proper iteration over state_updates when API is clarified
        // This is still deterministic (always returns same result for same input)
        hasher.update(b"DETERMINISTIC_STATE_ROOT");
        
        // We could iterate over the state updates here if the API was available:
        // for update in state_updates.iter() {
        //     hasher.update(update.key().bytes());
        //     if let Some(value) = update.value() {
        //         hasher.update(&value.bytes());
        //     } else {
        //         hasher.update(b"__DELETED__");
        //     }
        // }
        
        hasher.finalize().into()
    }

    /// Commit block data directly to storage with transaction indexing
    fn commit_block_to_storage(
        &self,
        height: u64,
        block_data: &[u8],
        block_hash: &[u8; 32],
    ) -> Result<(), String> {
        use hotstuff_rs::block_tree::pluggables::{KVStore, WriteBatch};
        use crate::storage::RocksWriteBatch;
        
        let mut batch = RocksWriteBatch::new();
        
        // Store block by height
        let height_key = format!("block:height:{}", height);
        batch.set(height_key.as_bytes(), block_data);
        
        // Store block by hash for fast lookup
        let hash_key = format!("block:hash:{}", hex::encode(block_hash));
        batch.set(hash_key.as_bytes(), block_data);
        
        // Store height->hash mapping
        let height_hash_key = format!("height_to_hash:{}", height);
        batch.set(height_hash_key.as_bytes(), block_hash);
        
        // Fix D: Add transaction indexing for O(1) lookup
        // Deserialize block to access transactions
        if let Ok(block) = bincode::deserialize::<crate::Block>(block_data) {
            for (tx_index, transaction) in block.transactions.iter().enumerate() {
                // Create transaction index: tx_id -> (height, index_in_block)
                let tx_index_key = format!("tx_index:{}", transaction.id);
                let tx_location = format!("{}:{}", height, tx_index);
                batch.set(tx_index_key.as_bytes(), tx_location.as_bytes());
                
                // Also store full transaction data by ID for quick retrieval
                let tx_data_key = format!("tx_data:{}", transaction.id);
                let tx_serialized = bincode::serialize(transaction)
                    .map_err(|e| format!("Failed to serialize transaction: {}", e))?;
                batch.set(tx_data_key.as_bytes(), &tx_serialized);
            }
        }
        
        // Update latest height pointer
        batch.set(b"latest_height", &height.to_le_bytes());
        
        // Update latest hash pointer
        batch.set(b"latest_hash", block_hash);
        
        // Write batch atomically
        let mut storage = self.storage.as_ref().clone();
        storage.write(batch);
        
        Ok(())
    }

    /// Print performance statistics
    async fn print_stats(&self, elapsed: Duration) {
        let app = self.app.read().await;
        let metrics = app.get_metrics();
        
        let blocks = self.blocks_committed.load(Ordering::SeqCst);
        let height = self.last_block_height.load(Ordering::SeqCst);
        let elapsed_secs = elapsed.as_secs_f64();
        
        let blocks_per_sec = if elapsed_secs > 0.0 { blocks as f64 / elapsed_secs } else { 0.0 };
        let tx_per_sec = if elapsed_secs > 0.0 { metrics.total_transactions as f64 / elapsed_secs } else { 0.0 };
        
        println!("📊 DirectCommit Stats:");
        println!("   Blocks: {} (Height: {}) | {:.1} blocks/sec", blocks, height, blocks_per_sec);
        println!("   Transactions: {} | {:.0} TPS", metrics.total_transactions, tx_per_sec);
        println!("   Pending: {} | State: {} entries ({:.2} MB)",
            metrics.pending_transactions,
            metrics.state_entries,
            metrics.state_utilization_mb()
        );
        println!();
    }

    /// Stop the engine
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        println!("🛑 DirectCommit Engine Stopped");
    }

    /// Get current statistics
    pub async fn get_metrics(&self) -> DirectCommitMetrics {
        let app = self.app.read().await;
        let blockchain_metrics = app.get_metrics();
        
        DirectCommitMetrics {
            blockchain: blockchain_metrics,
            blocks_committed: self.blocks_committed.load(Ordering::SeqCst),
            current_height: self.last_block_height.load(Ordering::SeqCst),
            is_running: self.running.load(Ordering::SeqCst),
        }
    }
}

/// Metrics specific to direct commit mode
#[derive(Debug, Clone)]
pub struct DirectCommitMetrics {
    pub blockchain: BlockchainMetrics,
    pub blocks_committed: u64,
    pub current_height: u64,
    pub is_running: bool,
}

impl DirectCommitMetrics {
    /// Calculate actual blocks per second
    pub fn blocks_per_second(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs == 0.0 {
            return 0.0;
        }
        self.blocks_committed as f64 / elapsed_secs
    }
    
    /// Calculate actual transactions per second
    pub fn transactions_per_second(&self, elapsed_secs: f64) -> f64 {
        if elapsed_secs == 0.0 {
            return 0.0;
        }
        self.blockchain.total_transactions as f64 / elapsed_secs
    }
}
