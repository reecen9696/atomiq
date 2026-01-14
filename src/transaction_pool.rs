//! Transaction pool management with configurable policies
//!
//! Separated transaction pool concerns from the main blockchain logic

use crate::{
    config::BlockchainConfig,
    errors::{AtomiqResult, TransactionError},
    Transaction,
};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

/// Transaction pool with configurable capacity and ordering policies
#[derive(Clone)]
pub struct TransactionPool {
    pool: Arc<RwLock<VecDeque<Transaction>>>,
    transaction_counter: Arc<AtomicU64>,
    config: TransactionPoolConfig,
}

/// Configuration for transaction pool behavior
#[derive(Clone, Debug)]
pub struct TransactionPoolConfig {
    pub max_pool_size: usize,
    pub max_transaction_data_size: usize,
    pub enable_nonce_validation: bool,
    pub ordering_policy: OrderingPolicy,
}

/// Policy for ordering transactions in the pool
#[derive(Clone, Debug)]
pub enum OrderingPolicy {
    /// First-in-first-out (simple queue)
    Fifo,
    /// Order by nonce for each sender
    NonceOrdered,
    /// Priority based on fees (future enhancement)
    FeeBased,
}

impl Default for TransactionPoolConfig {
    fn default() -> Self {
        Self {
            max_pool_size: 100_000,
            max_transaction_data_size: 1024 * 1024, // 1MB
            enable_nonce_validation: true,
            ordering_policy: OrderingPolicy::Fifo,
        }
    }
}

impl TransactionPool {
    /// Create new transaction pool with default configuration
    pub fn new() -> Self {
        Self::new_with_config(TransactionPoolConfig::default())
    }

    /// Create new transaction pool with custom configuration
    pub fn new_with_config(config: TransactionPoolConfig) -> Self {
        Self {
            pool: Arc::new(RwLock::new(VecDeque::new())),
            transaction_counter: Arc::new(AtomicU64::new(0)),
            config,
        }
    }

    /// Submit transaction to pool with validation and backpressure handling
    pub fn submit_transaction(&self, mut transaction: Transaction) -> AtomiqResult<u64> {
        // Validate transaction size (early return pattern)
        self.validate_transaction_size(&transaction)?;

        // Check pool capacity with logging
        let current_pool_size = self.pool_size();
        self.check_pool_capacity(current_pool_size)?;
        self.log_capacity_warnings(current_pool_size);

        // Assign ID if not already set, otherwise preserve existing ID
        let tx_id = if transaction.id == 0 {
            let new_id = self.assign_transaction_id();
            transaction.id = new_id;
            new_id
        } else {
            // Preserve existing ID (e.g., from external systems)
            transaction.id
        };
        
        // Update timestamp
        transaction.timestamp = Self::get_current_timestamp_ms()?;

        // Insert transaction using policy
        self.insert_transaction(transaction)?;

        Ok(tx_id)
    }

    /// Validate transaction data size against configuration limits
    fn validate_transaction_size(&self, transaction: &Transaction) -> AtomiqResult<()> {
        if transaction.data.len() > self.config.max_transaction_data_size {
            log::warn!(
                "Transaction rejected: data too large ({} bytes > {} max)",
                transaction.data.len(),
                self.config.max_transaction_data_size
            );
            return Err(TransactionError::DataTooLarge {
                size: transaction.data.len(),
                max_size: self.config.max_transaction_data_size,
            }.into());
        }
        Ok(())
    }

    /// Check if pool has capacity for new transaction
    fn check_pool_capacity(&self, current_size: usize) -> AtomiqResult<()> {
        if current_size >= self.config.max_pool_size {
            log::warn!(
                "Transaction pool full: rejecting transaction (current: {}, max: {})",
                current_size,
                self.config.max_pool_size
            );
            return Err(TransactionError::PoolFull.into());
        }
        Ok(())
    }

    /// Log warnings when pool is approaching capacity
    fn log_capacity_warnings(&self, current_size: usize) {
        const HIGH_CAPACITY_THRESHOLD: f64 = 0.9;
        let capacity_ratio = current_size as f64 / self.config.max_pool_size as f64;
        
        if capacity_ratio > HIGH_CAPACITY_THRESHOLD {
            log::warn!(
                "Transaction pool nearing capacity: {}/{} ({:.1}% full)",
                current_size,
                self.config.max_pool_size,
                capacity_ratio * 100.0
            );
        }
    }

    /// Assign unique transaction ID using atomic counter
    fn assign_transaction_id(&self) -> u64 {
        self.transaction_counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Get current system timestamp in milliseconds
    fn get_current_timestamp_ms() -> AtomiqResult<u64> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| TransactionError::ExecutionFailed(
                format!("Failed to get system time: {}", e)
            ))?;
        Ok(duration.as_millis() as u64)
    }

    /// Insert transaction into pool based on ordering policy
    fn insert_transaction(&self, transaction: Transaction) -> AtomiqResult<()> {
        let mut pool = self.pool.write()
            .map_err(|e| TransactionError::ExecutionFailed(
                format!("Failed to acquire pool lock: {}", e)
            ))?;
        
        // All policies currently use FIFO insertion
        // NonceOrdered and FeeBased are placeholders for future enhancement
        pool.push_back(transaction);
        Ok(())
    }

    /// Get current transaction pool size
    ///
    /// Returns 0 if unable to acquire read lock
    pub fn pool_size(&self) -> usize {
        self.pool.read()
            .map(|pool| pool.len())
            .unwrap_or_else(|e| {
                log::error!("Failed to read pool size: {}", e);
                0
            })
    }

    /// Drain transactions from pool for block creation
    ///
    /// Returns empty vector if unable to acquire write lock
    pub fn drain_transactions(&self, max_count: usize) -> Vec<Transaction> {
        match self.pool.write() {
            Ok(mut pool) => {
                let count = std::cmp::min(max_count, pool.len());
                pool.drain(0..count).collect()
            }
            Err(e) => {
                log::error!("Failed to drain transactions: {}", e);
                Vec::new()
            }
        }
    }

    /// Peek at pending transactions without removing them
    ///
    /// Returns empty vector if unable to acquire read lock
    pub fn peek_transactions(&self, max_count: usize) -> Vec<Transaction> {
        match self.pool.read() {
            Ok(pool) => {
                let count = std::cmp::min(max_count, pool.len());
                pool.iter().take(count).cloned().collect()
            }
            Err(e) => {
                log::error!("Failed to peek transactions: {}", e);
                Vec::new()
            }
        }
    }

    /// Remove specific transactions from pool (for failed validations)
    ///
    /// Returns number of transactions removed, or 0 if unable to acquire lock
    pub fn remove_transactions(&self, transaction_ids: &[u64]) -> usize {
        match self.pool.write() {
            Ok(mut pool) => {
                let original_size = pool.len();
                pool.retain(|tx| !transaction_ids.contains(&tx.id));
                original_size - pool.len()
            }
            Err(e) => {
                log::error!("Failed to remove transactions: {}", e);
                0
            }
        }
    }

    /// Clear all transactions from the pool
    ///
    /// Logs error if unable to acquire write lock
    pub fn clear(&self) {
        match self.pool.write() {
            Ok(mut pool) => pool.clear(),
            Err(e) => log::error!("Failed to clear transaction pool: {}", e),
        }
    }

    /// Get statistics about the transaction pool
    ///
    /// Returns zero stats if unable to acquire read lock
    pub fn get_stats(&self) -> TransactionPoolStats {
        match self.pool.read() {
            Ok(pool) => {
                let total_data_size: usize = pool.iter().map(|tx| tx.data.len()).sum();
                
                TransactionPoolStats {
                    total_transactions: pool.len() as u64,
                    total_data_size_bytes: total_data_size as u64,
                    capacity_utilization: (pool.len() as f64 / self.config.max_pool_size as f64) * 100.0,
                    transactions_processed: self.transaction_counter.load(Ordering::SeqCst),
                }
            }
            Err(e) => {
                log::error!("Failed to get pool stats: {}", e);
                TransactionPoolStats {
                    total_transactions: 0,
                    total_data_size_bytes: 0,
                    capacity_utilization: 0.0,
                    transactions_processed: self.transaction_counter.load(Ordering::SeqCst),
                }
            }
        }
    }

    /// Access to transaction counter for monitoring
    pub fn transaction_counter(&self) -> &Arc<AtomicU64> {
        &self.transaction_counter
    }

    /// Validate transactions for nonce consistency (if enabled)
    fn validate_transaction_nonces(&self, transactions: &[Transaction]) -> AtomiqResult<()> {
        if !self.config.enable_nonce_validation {
            return Ok(());
        }

        // Group transactions by sender and validate nonce sequence
        let mut sender_nonces: HashMap<[u8; 32], u64> = HashMap::new();
        
        for tx in transactions {
            let current_nonce = sender_nonces.entry(tx.sender).or_insert(0);
            
            if tx.nonce != *current_nonce + 1 {
                return Err(TransactionError::NonceError {
                    expected: *current_nonce + 1,
                    actual: tx.nonce,
                }.into());
            }
            
            *current_nonce = tx.nonce;
        }
        
        Ok(())
    }
}

/// Statistics about the transaction pool
#[derive(Debug, Clone)]
pub struct TransactionPoolStats {
    pub total_transactions: u64,
    pub total_data_size_bytes: u64,
    pub capacity_utilization: f64,
    pub transactions_processed: u64,
}

impl From<BlockchainConfig> for TransactionPoolConfig {
    fn from(blockchain_config: BlockchainConfig) -> Self {
        Self {
            max_pool_size: blockchain_config.max_transactions_per_block * 10,
            enable_nonce_validation: blockchain_config.enable_state_validation,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_pool_creation() {
        let pool = TransactionPool::new();
        assert_eq!(pool.pool_size(), 0);
        assert_eq!(pool.transaction_counter().load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_submit_transaction() {
        let pool = TransactionPool::new();
        
        let tx = Transaction {
            id: 0, // Will be assigned
            sender: [1u8; 32],
            data: b"test data".to_vec(),
            timestamp: 0, // Will be assigned
            nonce: 1,
        };
        
        let result = pool.submit_transaction(tx);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
        assert_eq!(pool.pool_size(), 1);
    }

    #[test]
    fn test_drain_transactions() {
        let pool = TransactionPool::new();
        
        // Submit test transactions
        for i in 0..5 {
            let tx = Transaction {
                id: 0,
                sender: [i; 32],
                data: format!("test {}", i).into_bytes(),
                timestamp: 0,
                nonce: 1,
            };
            pool.submit_transaction(tx).unwrap();
        }
        
        assert_eq!(pool.pool_size(), 5);
        
        let drained = pool.drain_transactions(3);
        assert_eq!(drained.len(), 3);
        assert_eq!(pool.pool_size(), 2);
    }

    #[test]
    fn test_pool_capacity_limit() {
        let config = TransactionPoolConfig {
            max_pool_size: 2,
            ..Default::default()
        };
        let pool = TransactionPool::new_with_config(config);
        
        // Fill pool to capacity
        for i in 0..2 {
            let tx = Transaction {
                id: 0,
                sender: [i; 32],
                data: b"test".to_vec(),
                timestamp: 0,
                nonce: 1,
            };
            assert!(pool.submit_transaction(tx).is_ok());
        }
        
        // Next transaction should fail
        let tx = Transaction {
            id: 0,
            sender: [3; 32],
            data: b"test".to_vec(),
            timestamp: 0,
            nonce: 1,
        };
        assert!(pool.submit_transaction(tx).is_err());
    }

    #[test]
    fn test_transaction_size_limit() {
        let config = TransactionPoolConfig {
            max_transaction_data_size: 10,
            ..Default::default()
        };
        let pool = TransactionPool::new_with_config(config);
        
        let tx = Transaction {
            id: 0,
            sender: [1; 32],
            data: vec![0u8; 20], // Exceeds limit
            timestamp: 0,
            nonce: 1,
        };
        
        let result = pool.submit_transaction(tx);
        assert!(result.is_err());
    }

    #[test]
    fn test_pool_stats() {
        let pool = TransactionPool::new();
        
        let tx = Transaction {
            id: 0,
            sender: [1; 32],
            data: b"test data".to_vec(),
            timestamp: 0,
            nonce: 1,
        };
        pool.submit_transaction(tx).unwrap();
        
        let stats = pool.get_stats();
        assert_eq!(stats.total_transactions, 1);
        assert_eq!(stats.total_data_size_bytes, 9); // "test data".len()
        assert!(stats.capacity_utilization > 0.0);
    }
}