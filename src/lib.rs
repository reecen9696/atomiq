//! Atomiq - High-Performance Single-Validator Blockchain
//!
//! Clean, minimal blockchain implementation optimized for maximum TPS.
//! Uses HotStuff-rs consensus with single validator for performance.

use hotstuff_rs::{
    app::{App, ProduceBlockRequest, ProduceBlockResponse, ValidateBlockRequest, ValidateBlockResponse},
    block_tree::pluggables::KVStore,
    types::{
        crypto_primitives::{CryptoHasher, Digest},
        data_types::{CryptoHash, Data, Datum},
        update_sets::AppStateUpdates,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

// Export all modules for external use
pub mod config;
pub mod errors;
pub mod factory;
pub mod storage;
pub mod network;
pub mod metrics;
pub mod benchmark;
pub mod transaction_pool;
pub mod state_manager;

// Re-export commonly used types
pub use config::{AtomiqConfig, BlockchainConfig};
pub use errors::{AtomiqError, AtomiqResult};
pub use factory::{BlockchainFactory, BlockchainHandle};

/// Transaction with minimal required fields for performance testing
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Transaction {
    pub id: u64,
    pub sender: [u8; 32],
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub nonce: u64,
}

/// Block containing a batch of validated transactions  
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub transactions: Vec<Transaction>,
    pub timestamp: u64,
    pub transaction_count: usize,
}

/// Refactored high-performance blockchain app with modular architecture
#[derive(Clone)]
pub struct AtomiqApp {
    config: BlockchainConfig,
    transaction_pool: crate::transaction_pool::TransactionPool,
    state_manager: Arc<crate::state_manager::StateManager>,
    block_counter: Arc<AtomicU64>,
    last_block_time: Arc<RwLock<SystemTime>>,
}

impl AtomiqApp {
    pub fn new(config: BlockchainConfig) -> Self {
        let transaction_pool = crate::transaction_pool::TransactionPool::new_with_config(
            config.clone().into()
        );
        let state_manager = Arc::new(crate::state_manager::StateManager::new_with_config(
            config.clone().into()
        ));
        
        Self {
            config,
            transaction_pool,
            state_manager,
            block_counter: Arc::new(AtomicU64::new(0)),
            last_block_time: Arc::new(RwLock::new(SystemTime::now())),
        }
    }

    /// Submit transaction to pool with auto-assigned ID and timestamp
    pub fn submit_transaction(&self, transaction: Transaction) -> AtomiqResult<u64> {
        self.transaction_pool.submit_transaction(transaction)
            .map_err(|e| e.into())
    }

    /// Get current transaction pool size  
    pub fn pool_size(&self) -> usize {
        self.transaction_pool.pool_size()
    }

    /// Get current performance metrics
    pub fn get_metrics(&self) -> BlockchainMetrics {
        let last_block_time = *self.last_block_time.read().unwrap();
        let time_since_last_block = SystemTime::now()
            .duration_since(last_block_time)
            .unwrap_or_default()
            .as_millis() as u64;

        let pool_stats = self.transaction_pool.get_stats();
        let state_stats = self.state_manager.get_state_stats();

        BlockchainMetrics {
            total_transactions: pool_stats.transactions_processed,
            total_blocks: self.block_counter.load(Ordering::SeqCst),
            pending_transactions: pool_stats.total_transactions,
            time_since_last_block_ms: time_since_last_block,
            state_entries: state_stats.total_entries,
            state_size_bytes: state_stats.total_size_bytes,
        }
    }

    /// Drain transactions from pool for block creation
    pub fn drain_transaction_pool(&self) -> Vec<Transaction> {
        self.transaction_pool.drain_transactions(self.config.max_transactions_per_block)
    }

    /// Access to transaction counter for monitoring  
    pub fn transaction_counter(&self) -> Arc<AtomicU64> {
        self.transaction_pool.transaction_counter().clone()
    }

    /// Access to block counter for monitoring
    pub fn block_counter(&self) -> &Arc<AtomicU64> {
        &self.block_counter
    }

    /// Execute batch of transactions using state manager
    pub fn execute_transactions(&self, transactions: &[Transaction]) -> (Vec<crate::state_manager::ExecutionResult>, AppStateUpdates) {
        self.state_manager.execute_transactions(transactions)
    }

    /// Create a block from transactions
    fn create_block(&self, transactions: Vec<Transaction>) -> Block {
        let transaction_count = transactions.len();
        Block {
            transactions,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            transaction_count,
        }
    }

    /// Serialize block and compute hash
    fn serialize_block(&self, block: &Block) -> (CryptoHash, Data) {
        let block_bytes = bincode::serialize(block).expect("Failed to serialize block");
        
        let data_hash = {
            let mut hasher = CryptoHasher::new();
            hasher.update(&block_bytes);
            CryptoHash::new(hasher.finalize().into())
        };

        (data_hash, Data::new(vec![Datum::new(block_bytes)]))
    }

    /// Update block production metrics
    fn update_block_metrics(&self) {
        self.block_counter.fetch_add(1, Ordering::SeqCst);
        *self.last_block_time.write().unwrap() = SystemTime::now();
    }

    /// Deserialize and validate block structure and hash
    fn deserialize_and_validate_block<K: KVStore>(&self, request: &ValidateBlockRequest<K>) -> Result<Block, String> {
        let block_data = &request.proposed_block().data;
        let block_bytes = &block_data.vec()[0].bytes();
        
        let block: Block = bincode::deserialize(block_bytes)
            .map_err(|e| format!("Failed to deserialize block: {}", e))?;

        let computed_hash = {
            let mut hasher = CryptoHasher::new();
            hasher.update(block_bytes);
            CryptoHash::new(hasher.finalize().into())
        };

        if computed_hash != request.proposed_block().data_hash {
            return Err("Block hash mismatch".to_string());
        }

        if !self.state_manager.validate_block_transactions(&block.transactions) {
            return Err("Transaction validation failed".to_string());
        }

        Ok(block)
    }

    /// Process valid block and execute transactions
    fn process_valid_block(&self, block: &Block) -> ValidateBlockResponse {
        let (_, app_state_updates) = self.state_manager.execute_transactions(&block.transactions);

        ValidateBlockResponse::Valid {
            app_state_updates: Some(app_state_updates),
            validator_set_updates: None,
        }
    }
}

impl<K: KVStore> App<K> for AtomiqApp {
    fn produce_block(&mut self, _request: ProduceBlockRequest<K>) -> ProduceBlockResponse {
        let transactions = self.drain_transaction_pool();
        
        if transactions.is_empty() {
            return ProduceBlockResponse {
                data_hash: CryptoHash::new([0u8; 32]),
                data: Data::new(vec![]),
                app_state_updates: None,
                validator_set_updates: None,
            };
        }

        let (_execution_results, app_state_updates) = self.execute_transactions(&transactions);
        let block = self.create_block(transactions);
        let (data_hash, data) = self.serialize_block(&block);

        self.update_block_metrics();

        ProduceBlockResponse {
            data_hash,
            data,
            app_state_updates: Some(app_state_updates),
            validator_set_updates: None,
        }
    }

    fn validate_block(&mut self, request: ValidateBlockRequest<K>) -> ValidateBlockResponse {
        let block_data = &request.proposed_block().data;
        
        if block_data.vec().is_empty() {
            return ValidateBlockResponse::Valid {
                app_state_updates: None,
                validator_set_updates: None,
            };
        }
        
        match self.deserialize_and_validate_block(&request) {
            Ok(block) => self.process_valid_block(&block),
            Err(_) => ValidateBlockResponse::Invalid,
        }
    }

    fn validate_block_for_sync(&mut self, request: ValidateBlockRequest<K>) -> ValidateBlockResponse {
        self.validate_block(request)
    }
}

/// Enhanced performance metrics with additional state information
#[derive(Debug, Clone)]
pub struct BlockchainMetrics {
    pub total_transactions: u64,
    pub total_blocks: u64,
    pub pending_transactions: u64,
    pub time_since_last_block_ms: u64,
    pub state_entries: u64,
    pub state_size_bytes: u64,
}

impl BlockchainMetrics {
    /// Calculate estimated TPS based on current metrics
    pub fn estimated_tps(&self) -> f64 {
        if self.total_blocks == 0 {
            return 0.0;
        }
        
        // Rough calculation assuming 10ms average block time
        self.total_transactions as f64 / self.total_blocks as f64 * 100.0
    }

    /// Calculate state utilization metrics
    pub fn state_utilization_mb(&self) -> f64 {
        self.state_size_bytes as f64 / (1024.0 * 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let tx = Transaction {
            id: 1,
            sender: [1u8; 32],
            data: b"test data".to_vec(),
            timestamp: 123_456_789,
            nonce: 1,
        };
        
        assert_eq!(tx.id, 1);
        assert_eq!(tx.data, b"test data");
    }

    #[test]
    fn test_app_creation_and_transaction_submission() {
        let config = BlockchainConfig::default();
        let app = AtomiqApp::new(config);
        
        let tx = Transaction {
            id: 0, // Will be overwritten
            sender: [1u8; 32],
            data: b"test".to_vec(),
            timestamp: 0, // Will be overwritten
            nonce: 1,
        };
        
        let tx_id = app.submit_transaction(tx);
        assert!(tx_id.is_ok());
        assert_eq!(app.pool_size(), 1);
    }

    #[test]
    fn test_metrics() {
        let config = BlockchainConfig::default();
        let app = AtomiqApp::new(config);
        
        let metrics = app.get_metrics();
        assert_eq!(metrics.total_transactions, 0);
        assert_eq!(metrics.total_blocks, 0);
        assert_eq!(metrics.pending_transactions, 0);
        assert_eq!(metrics.state_entries, 0);
    }

    #[test]
    fn test_blockchain_config() {
        let config = BlockchainConfig::default();
        assert_eq!(config.max_transactions_per_block, 10_000);
        assert_eq!(config.max_block_time_ms, 10);
        assert!(config.enable_state_validation);
        assert_eq!(config.batch_size_threshold, 1_000);
    }

    #[test]
    fn test_enhanced_metrics() {
        let config = BlockchainConfig::default();
        let app = AtomiqApp::new(config);
        let metrics = app.get_metrics();
        
        assert_eq!(metrics.estimated_tps(), 0.0); // No blocks yet
        assert_eq!(metrics.state_utilization_mb(), 0.0); // No state yet
    }
}