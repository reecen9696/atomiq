//! Lean High-Performance Blockchain using HotStuff-rs
//! 
//! Focused on maximum TPS with minimal complexity.
//! No trading logic, just pure transaction processing performance.

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
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub mod storage;
pub mod network;
pub mod metrics;

/// Simple transaction structure optimized for throughput testing
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Transaction {
    pub id: u64,
    pub sender: [u8; 32],
    pub data: Vec<u8>,
    pub timestamp: u64,
    pub nonce: u64,
}

/// Block containing batched transactions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block {
    pub transactions: Vec<Transaction>,
    pub timestamp: u64,
    pub transaction_count: usize,
}

/// Execution result for a transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub tx_id: u64,
    pub success: bool,
    pub state_changes: Vec<StateChange>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateChange {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Configuration for the lean blockchain
#[derive(Clone, Debug)]
pub struct BlockchainConfig {
    pub max_transactions_per_block: usize,
    pub max_block_time_ms: u64,
    pub enable_state_validation: bool,
    pub batch_size_threshold: usize,
}

impl Default for BlockchainConfig {
    fn default() -> Self {
        Self {
            max_transactions_per_block: 10000, // High throughput batching
            max_block_time_ms: 10,             // 10ms block times
            enable_state_validation: true,     // Full validation for real TPS
            batch_size_threshold: 1000,        // Create block when we hit 1000 txs
        }
    }
}

/// Lean blockchain app focused on maximum throughput
#[derive(Clone)]
pub struct LeanBlockchainApp {
    /// Configuration
    config: BlockchainConfig,
    
    /// Transaction pool for batching
    transaction_pool: Arc<RwLock<VecDeque<Transaction>>>,
    
    /// Simple key-value state for validation
    state: Arc<RwLock<std::collections::HashMap<Vec<u8>, Vec<u8>>>>,
    
    /// Metrics
    transaction_counter: Arc<AtomicU64>,
    block_counter: Arc<AtomicU64>,
    
    /// Performance tracking
    last_block_time: Arc<RwLock<SystemTime>>,
}

impl LeanBlockchainApp {
    pub fn new(config: BlockchainConfig) -> Self {
        Self {
            config,
            transaction_pool: Arc::new(RwLock::new(VecDeque::new())),
            state: Arc::new(RwLock::new(std::collections::HashMap::new())),
            transaction_counter: Arc::new(AtomicU64::new(0)),
            block_counter: Arc::new(AtomicU64::new(0)),
            last_block_time: Arc::new(RwLock::new(SystemTime::now())),
        }
    }

    /// Submit a transaction to the pool (non-blocking)
    pub fn submit_transaction(&self, mut transaction: Transaction) -> u64 {
        // Assign unique ID
        transaction.id = self.transaction_counter.fetch_add(1, Ordering::SeqCst);
        transaction.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Add to pool
        if let Ok(mut pool) = self.transaction_pool.write() {
            pool.push_back(transaction.clone());
        }

        transaction.id
    }

    /// Get current transaction pool size
    pub fn pool_size(&self) -> usize {
        self.transaction_pool.read().map(|pool| pool.len()).unwrap_or(0)
    }

    /// Get performance metrics
    pub fn get_metrics(&self) -> BlockchainMetrics {
        let last_block_time = *self.last_block_time.read().unwrap();
        let time_since_last_block = SystemTime::now()
            .duration_since(last_block_time)
            .unwrap_or_default()
            .as_millis() as u64;

        BlockchainMetrics {
            total_transactions: self.transaction_counter.load(Ordering::SeqCst),
            total_blocks: self.block_counter.load(Ordering::SeqCst),
            pending_transactions: self.pool_size() as u64,
            time_since_last_block_ms: time_since_last_block,
        }
    }

    /// Drain all transactions from pool (for manual block creation)
    pub fn drain_transaction_pool(&self) -> Vec<Transaction> {
        let mut pool = self.transaction_pool.write().unwrap();
        let batch_size = std::cmp::min(
            self.config.max_transactions_per_block,
            pool.len()
        );
        pool.drain(0..batch_size).collect()
    }
    
    /// Get current transaction counter
    pub fn transaction_counter(&self) -> &Arc<std::sync::atomic::AtomicU64> {
        &self.transaction_counter
    }
    
    /// Get current block counter
    pub fn block_counter(&self) -> &Arc<std::sync::atomic::AtomicU64> {
        &self.block_counter
    }

    /// Execute a batch of transactions with full validation
    pub fn execute_transactions(&self, transactions: &[Transaction]) -> (Vec<ExecutionResult>, AppStateUpdates) {
        let mut results = Vec::with_capacity(transactions.len());
        let mut app_state_updates = AppStateUpdates::new();
        
        if !self.config.enable_state_validation {
            // Fast path - minimal validation
            for tx in transactions {
                results.push(ExecutionResult {
                    tx_id: tx.id,
                    success: true,
                    state_changes: vec![],
                });
            }
            return (results, app_state_updates);
        }

        // Full validation path for real TPS measurement
        let mut state = self.state.write().unwrap();
        
        for tx in transactions {
            let execution_start = std::time::Instant::now();
            
            // Simulate real validation work
            let mut success = true;
            let mut changes = Vec::new();
            
            // 1. Validate transaction structure
            if tx.data.is_empty() {
                success = false;
            }
            
            // 2. Validate nonce (simple incrementing nonce per sender)
            let mut nonce_key = Vec::with_capacity(6 + 32);
            nonce_key.extend_from_slice(b"nonce_");
            nonce_key.extend_from_slice(&tx.sender);
            let current_nonce = state.get(&nonce_key)
                .and_then(|bytes| {
                    if bytes.len() == 8 {
                        let array: Result<[u8; 8], _> = bytes.as_slice().try_into();
                        array.ok().map(u64::from_le_bytes)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
                
            if tx.nonce != current_nonce + 1 {
                success = false;
            }
            
            if success {
                // 3. Apply state changes
                
                // Update nonce
                let new_nonce = (current_nonce + 1).to_le_bytes().to_vec();
                state.insert(nonce_key.clone(), new_nonce.clone());
                changes.push(StateChange {
                    key: nonce_key,
                    value: new_nonce,
                });
                
                // Store transaction data as state
                let mut tx_key = Vec::with_capacity(3 + 8);
                tx_key.extend_from_slice(b"tx_");
                tx_key.extend_from_slice(&tx.id.to_le_bytes());
                state.insert(tx_key.clone(), tx.data.clone());
                changes.push(StateChange {
                    key: tx_key,
                    value: tx.data.clone(),
                });
                
                // Update app state updates for HotStuff-rs
                for change in &changes {
                    app_state_updates.insert(change.key.clone(), change.value.clone());
                }
            }
            
            results.push(ExecutionResult {
                tx_id: tx.id,
                success,
                state_changes: changes,
            });
            
            // Log slow transactions (for performance debugging)
            let execution_time = execution_start.elapsed();
            if execution_time.as_micros() > 100 {
                log::warn!("Slow transaction {} took {:?}", tx.id, execution_time);
            }
        }
        
        (results, app_state_updates)
    }

    /// Validate a block's transactions
    fn validate_block_transactions(&self, transactions: &[Transaction]) -> bool {
        if !self.config.enable_state_validation {
            return true; // Skip validation in fast mode
        }
        
        // Perform validation without modifying state
        let state = self.state.read().unwrap();
        
        for tx in transactions {
            // Basic structure validation
            if tx.data.is_empty() {
                return false;
            }
            
            // Nonce validation
            let mut nonce_key = Vec::with_capacity(6 + 32);
            nonce_key.extend_from_slice(b"nonce_");
            nonce_key.extend_from_slice(&tx.sender);
            let current_nonce = state.get(&nonce_key)
                .and_then(|bytes| {
                    if bytes.len() == 8 {
                        let array: Result<[u8; 8], _> = bytes.as_slice().try_into();
                        array.ok().map(u64::from_le_bytes)
                    } else {
                        None
                    }
                })
                .unwrap_or(0);
                
            if tx.nonce != current_nonce + 1 {
                return false;
            }
            
            // Additional validation could go here
        }
        
        true
    }
}

impl<K: KVStore> App<K> for LeanBlockchainApp {
    fn produce_block(&mut self, _request: ProduceBlockRequest<K>) -> ProduceBlockResponse {
        let block_start = std::time::Instant::now();
        
        // Drain transactions from pool
        let transactions = self.drain_transaction_pool();
        
        if transactions.is_empty() {
            // Return empty block quickly
            return ProduceBlockResponse {
                data_hash: CryptoHash::new([0u8; 32]),
                data: Data::new(vec![]),
                app_state_updates: None,
                validator_set_updates: None,
            };
        }

        // Execute transactions with full validation
        let (_execution_results, app_state_updates) = self.execute_transactions(&transactions);
        
        // Create block
        let block = Block {
            transactions: transactions.clone(),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            transaction_count: transactions.len(),
        };

        // Serialize block
        let block_bytes = bincode::serialize(&block).expect("Failed to serialize block");
        
        // Compute hash
        let data_hash = {
            let mut hasher = CryptoHasher::new();
            hasher.update(&block_bytes);
            CryptoHash::new(hasher.finalize().into())
        };

        // Update metrics
        self.block_counter.fetch_add(1, Ordering::SeqCst);
        *self.last_block_time.write().unwrap() = SystemTime::now();

        let block_time = block_start.elapsed();
        log::info!(
            "Produced block {} with {} transactions in {:?}",
            self.block_counter.load(Ordering::SeqCst),
            transactions.len(),
            block_time
        );

        // Report performance metrics
        if block_time.as_millis() > self.config.max_block_time_ms as u128 {
            log::warn!("Block production took {:?}, exceeding target of {}ms", 
                block_time, self.config.max_block_time_ms);
        }

        ProduceBlockResponse {
            data_hash,
            data: Data::new(vec![Datum::new(block_bytes)]),
            app_state_updates: Some(app_state_updates),
            validator_set_updates: None,
        }
    }

    fn validate_block(&mut self, request: ValidateBlockRequest<K>) -> ValidateBlockResponse {
        let validation_start = std::time::Instant::now();
        
        // Deserialize block
        let block_data = &request.proposed_block().data;
        if block_data.vec().is_empty() {
            return ValidateBlockResponse::Valid {
                app_state_updates: None,
                validator_set_updates: None,
            };
        }
        
        let block: Block = match bincode::deserialize(&block_data.vec()[0].bytes()) {
            Ok(block) => block,
            Err(e) => {
                log::error!("Failed to deserialize block: {}", e);
                return ValidateBlockResponse::Invalid;
            }
        };

        // Verify data hash
        let block_bytes = &block_data.vec()[0].bytes();
        let computed_hash = {
            let mut hasher = CryptoHasher::new();
            hasher.update(block_bytes);
            CryptoHash::new(hasher.finalize().into())
        };

        if computed_hash != request.proposed_block().data_hash {
            log::error!("Block hash mismatch");
            return ValidateBlockResponse::Invalid;
        }

        // Validate transactions
        if !self.validate_block_transactions(&block.transactions) {
            log::error!("Transaction validation failed");
            return ValidateBlockResponse::Invalid;
        }

        // Execute transactions to get state updates (if validation enabled)
        let (_, app_state_updates) = if self.config.enable_state_validation {
            self.execute_transactions(&block.transactions)
        } else {
            (Vec::new(), AppStateUpdates::new())
        };

        let validation_time = validation_start.elapsed();
        log::debug!(
            "Validated block with {} transactions in {:?}",
            block.transactions.len(),
            validation_time
        );

        // Report validation performance
        if validation_time.as_millis() > self.config.max_block_time_ms as u128 {
            log::warn!("Block validation took {:?}, exceeding target", validation_time);
        }

        ValidateBlockResponse::Valid {
            app_state_updates: Some(app_state_updates),
            validator_set_updates: None,
        }
    }

    fn validate_block_for_sync(&mut self, request: ValidateBlockRequest<K>) -> ValidateBlockResponse {
        // For sync, use same validation logic
        self.validate_block(request)
    }
}

/// Performance metrics for monitoring
#[derive(Debug, Clone)]
pub struct BlockchainMetrics {
    pub total_transactions: u64,
    pub total_blocks: u64,
    pub pending_transactions: u64,
    pub time_since_last_block_ms: u64,
}

impl BlockchainMetrics {
    pub fn transactions_per_second(&self) -> f64 {
        if self.total_blocks == 0 {
            return 0.0;
        }
        
        // Rough TPS calculation (would need more sophisticated timing in production)
        self.total_transactions as f64 / self.total_blocks as f64 * 100.0 // Assuming ~10ms blocks
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
            timestamp: 123456789,
            nonce: 1,
        };
        
        assert_eq!(tx.id, 1);
        assert_eq!(tx.data, b"test data");
    }

    #[test]
    fn test_app_transaction_submission() {
        let config = BlockchainConfig::default();
        let app = LeanBlockchainApp::new(config);
        
        let tx = Transaction {
            id: 0, // Will be overwritten
            sender: [1u8; 32],
            data: b"test".to_vec(),
            timestamp: 0, // Will be overwritten
            nonce: 1,
        };
        
        let tx_id = app.submit_transaction(tx);
        assert_eq!(tx_id, 1); // First transaction gets ID 1
        assert_eq!(app.pool_size(), 1);
    }

    #[test]
    fn test_metrics() {
        let config = BlockchainConfig::default();
        let app = LeanBlockchainApp::new(config);
        
        let metrics = app.get_metrics();
        assert_eq!(metrics.total_transactions, 0);
        assert_eq!(metrics.total_blocks, 0);
        assert_eq!(metrics.pending_transactions, 0);
    }
}