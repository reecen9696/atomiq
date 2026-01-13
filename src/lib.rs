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
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub mod storage;
pub mod network;
pub mod metrics;

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

/// Result of executing a transaction with state changes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub tx_id: u64,
    pub success: bool,
    pub state_changes: Vec<StateChange>,
}

/// State change applied by a transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateChange {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Blockchain configuration for performance optimization
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
            max_transactions_per_block: 10_000,
            max_block_time_ms: 10,
            enable_state_validation: true,
            batch_size_threshold: 1_000,
        }
    }
}

/// High-performance blockchain app with single validator consensus
#[derive(Clone)]
pub struct AtomiqApp {
    config: BlockchainConfig,
    transaction_pool: Arc<RwLock<VecDeque<Transaction>>>,
    state: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
    transaction_counter: Arc<AtomicU64>,
    block_counter: Arc<AtomicU64>,
    last_block_time: Arc<RwLock<SystemTime>>,
}

impl AtomiqApp {
    pub fn new(config: BlockchainConfig) -> Self {
        Self {
            config,
            transaction_pool: Arc::new(RwLock::new(VecDeque::new())),
            state: Arc::new(RwLock::new(HashMap::new())),
            transaction_counter: Arc::new(AtomicU64::new(0)),
            block_counter: Arc::new(AtomicU64::new(0)),
            last_block_time: Arc::new(RwLock::new(SystemTime::now())),
        }
    }

    /// Submit transaction to pool with auto-assigned ID and timestamp
    pub fn submit_transaction(&self, mut transaction: Transaction) -> u64 {
        transaction.id = self.transaction_counter.fetch_add(1, Ordering::SeqCst);
        transaction.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        if let Ok(mut pool) = self.transaction_pool.write() {
            pool.push_back(transaction.clone());
        }

        transaction.id
    }

    /// Get current transaction pool size  
    pub fn pool_size(&self) -> usize {
        self.transaction_pool.read().map(|pool| pool.len()).unwrap_or(0)
    }

    /// Get current performance metrics
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

    /// Drain transactions from pool for block creation
    pub fn drain_transaction_pool(&self) -> Vec<Transaction> {
        let mut pool = self.transaction_pool.write().unwrap();
        let batch_size = std::cmp::min(self.config.max_transactions_per_block, pool.len());
        pool.drain(0..batch_size).collect()
    }

    /// Access to transaction counter for monitoring  
    pub fn transaction_counter(&self) -> &Arc<AtomicU64> {
        &self.transaction_counter
    }

    /// Access to block counter for monitoring
    pub fn block_counter(&self) -> &Arc<AtomicU64> {
        &self.block_counter
    }

    /// Execute batch of transactions with validation and state updates
    pub fn execute_transactions(&self, transactions: &[Transaction]) -> (Vec<ExecutionResult>, AppStateUpdates) {
        if !self.config.enable_state_validation {
            return self.execute_fast_path(transactions);
        }
        
        self.execute_with_validation(transactions)
    }

    /// Fast execution path without state validation  
    fn execute_fast_path(&self, transactions: &[Transaction]) -> (Vec<ExecutionResult>, AppStateUpdates) {
        let results = transactions
            .iter()
            .map(|tx| ExecutionResult {
                tx_id: tx.id,
                success: true,
                state_changes: vec![],
            })
            .collect();
        
        (results, AppStateUpdates::new())
    }

    /// Execute with full validation and state tracking
    fn execute_with_validation(&self, transactions: &[Transaction]) -> (Vec<ExecutionResult>, AppStateUpdates) {
        let mut results = Vec::with_capacity(transactions.len());
        let mut app_state_updates = AppStateUpdates::new();
        let mut state = self.state.write().unwrap();
        
        for tx in transactions {
            let result = self.execute_single_transaction(tx, &mut state, &mut app_state_updates);
            results.push(result);
        }
        
        (results, app_state_updates)
    }

    /// Execute single transaction with nonce validation
    fn execute_single_transaction(
        &self,
        tx: &Transaction,
        state: &mut HashMap<Vec<u8>, Vec<u8>>,
        app_state_updates: &mut AppStateUpdates,
    ) -> ExecutionResult {
        // Validate transaction structure
        if tx.data.is_empty() {
            return ExecutionResult {
                tx_id: tx.id,
                success: false,
                state_changes: vec![],
            };
        }

        // Validate nonce
        let nonce_key = self.build_nonce_key(&tx.sender);
        let current_nonce = self.get_current_nonce(state, &nonce_key);
        
        if tx.nonce != current_nonce + 1 {
            return ExecutionResult {
                tx_id: tx.id,
                success: false,
                state_changes: vec![],
            };
        }

        // Apply state changes
        let changes = self.apply_transaction_state_changes(tx, state, &nonce_key, current_nonce + 1);
        
        // Update app state for HotStuff-rs
        for change in &changes {
            app_state_updates.insert(change.key.clone(), change.value.clone());
        }

        ExecutionResult {
            tx_id: tx.id,
            success: true,
            state_changes: changes,
        }
    }

    /// Build nonce key for sender
    fn build_nonce_key(&self, sender: &[u8; 32]) -> Vec<u8> {
        let mut key = Vec::with_capacity(38); // "nonce_" + 32 bytes
        key.extend_from_slice(b"nonce_");
        key.extend_from_slice(sender);
        key
    }

    /// Get current nonce for sender
    fn get_current_nonce(&self, state: &HashMap<Vec<u8>, Vec<u8>>, nonce_key: &[u8]) -> u64 {
        state
            .get(nonce_key)
            .and_then(|bytes| {
                if bytes.len() == 8 {
                    let array: Result<[u8; 8], _> = bytes.as_slice().try_into();
                    array.ok().map(u64::from_le_bytes)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    /// Apply state changes for successful transaction
    fn apply_transaction_state_changes(
        &self,
        tx: &Transaction,
        state: &mut HashMap<Vec<u8>, Vec<u8>>,
        nonce_key: &[u8],
        new_nonce: u64,
    ) -> Vec<StateChange> {
        let mut changes = Vec::new();

        // Update nonce
        let nonce_bytes = new_nonce.to_le_bytes().to_vec();
        state.insert(nonce_key.to_vec(), nonce_bytes.clone());
        changes.push(StateChange {
            key: nonce_key.to_vec(),
            value: nonce_bytes,
        });

        // Store transaction data
        let tx_key = self.build_transaction_key(tx.id);
        state.insert(tx_key.clone(), tx.data.clone());
        changes.push(StateChange {
            key: tx_key,
            value: tx.data.clone(),
        });

        changes
    }

    /// Build transaction storage key
    fn build_transaction_key(&self, tx_id: u64) -> Vec<u8> {
        let mut key = Vec::with_capacity(11); // "tx_" + 8 bytes
        key.extend_from_slice(b"tx_");
        key.extend_from_slice(&tx_id.to_le_bytes());
        key
    }

    /// Validate block transactions without state modification
    fn validate_block_transactions(&self, transactions: &[Transaction]) -> bool {
        if !self.config.enable_state_validation {
            return true;
        }
        
        let state = self.state.read().unwrap();
        
        for tx in transactions {
            if !self.validate_single_transaction(tx, &state) {
                return false;
            }
        }
        
        true
    }

    /// Validate single transaction against current state
    fn validate_single_transaction(&self, tx: &Transaction, state: &HashMap<Vec<u8>, Vec<u8>>) -> bool {
        if tx.data.is_empty() {
            return false;
        }
        
        let nonce_key = self.build_nonce_key(&tx.sender);
        let current_nonce = self.get_current_nonce(state, &nonce_key);
        
        tx.nonce == current_nonce + 1
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

        if !self.validate_block_transactions(&block.transactions) {
            return Err("Transaction validation failed".to_string());
        }

        Ok(block)
    }

    /// Process valid block and execute transactions
    fn process_valid_block(&self, block: &Block) -> ValidateBlockResponse {
        let (_, app_state_updates) = if self.config.enable_state_validation {
            self.execute_transactions(&block.transactions)
        } else {
            (Vec::new(), AppStateUpdates::new())
        };

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

/// Real-time performance metrics
#[derive(Debug, Clone)]
pub struct BlockchainMetrics {
    pub total_transactions: u64,
    pub total_blocks: u64,
    pub pending_transactions: u64,
    pub time_since_last_block_ms: u64,
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
        assert_eq!(tx_id, 1);
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
    }

    #[test]
    fn test_blockchain_config() {
        let config = BlockchainConfig::default();
        assert_eq!(config.max_transactions_per_block, 10_000);
        assert_eq!(config.max_block_time_ms, 10);
        assert!(config.enable_state_validation);
        assert_eq!(config.batch_size_threshold, 1_000);
    }
}