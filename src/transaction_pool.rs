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
        // Validate transaction size
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

        let current_pool_size = self.pool_size();
        
        // Check pool capacity with enhanced logging
        if current_pool_size >= self.config.max_pool_size {
            log::warn!(
                "Transaction pool full: rejecting transaction (current: {}, max: {}). Consider increasing pool size or reducing transaction rate.",
                current_pool_size,
                self.config.max_pool_size
            );
            return Err(TransactionError::PoolFull.into());
        }
        
        // Log when pool is getting close to capacity (90% threshold)
        let capacity_ratio = current_pool_size as f64 / self.config.max_pool_size as f64;
        if capacity_ratio > 0.9 {
            log::warn!(
                "Transaction pool nearing capacity: {}/{} ({:.1}% full). System experiencing backpressure.",
                current_pool_size,
                self.config.max_pool_size,
                capacity_ratio * 100.0
            );
        }

        // Assign ID and timestamp
        let tx_id = self.transaction_counter.fetch_add(1, Ordering::SeqCst) + 1;
        transaction.id = tx_id;
        transaction.timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Insert based on ordering policy
        let mut pool = self.pool.write().unwrap();
        
        match self.config.ordering_policy {
            OrderingPolicy::Fifo => {
                pool.push_back(transaction.clone());
            }
            OrderingPolicy::NonceOrdered => {
                // For simplicity, still use FIFO but could implement nonce-based ordering
                pool.push_back(transaction.clone());
            }
            OrderingPolicy::FeeBased => {
                // Future enhancement for fee-based ordering
                pool.push_back(transaction.clone());
            }
        }

        Ok(tx_id)
    }

    /// Get current transaction pool size
    pub fn pool_size(&self) -> usize {
        self.pool.read().unwrap().len()
    }

    /// Drain transactions from pool for block creation
    pub fn drain_transactions(&self, max_count: usize) -> Vec<Transaction> {
        let mut pool = self.pool.write().unwrap();
        let count = std::cmp::min(max_count, pool.len());
        pool.drain(0..count).collect()
    }

    /// Peek at pending transactions without removing them
    pub fn peek_transactions(&self, max_count: usize) -> Vec<Transaction> {
        let pool = self.pool.read().unwrap();
        let count = std::cmp::min(max_count, pool.len());
        pool.iter().take(count).cloned().collect()
    }

    /// Remove specific transactions from pool (for failed validations)
    pub fn remove_transactions(&self, transaction_ids: &[u64]) -> usize {
        let mut pool = self.pool.write().unwrap();
        let original_size = pool.len();
        
        pool.retain(|tx| !transaction_ids.contains(&tx.id));
        
        original_size - pool.len()
    }

    /// Clear all transactions from the pool
    pub fn clear(&self) {
        let mut pool = self.pool.write().unwrap();
        pool.clear();
    }

    /// Get statistics about the transaction pool
    pub fn get_stats(&self) -> TransactionPoolStats {
        let pool = self.pool.read().unwrap();
        let total_data_size: usize = pool.iter().map(|tx| tx.data.len()).sum();
        
        TransactionPoolStats {
            total_transactions: pool.len() as u64,
            total_data_size_bytes: total_data_size as u64,
            capacity_utilization: (pool.len() as f64 / self.config.max_pool_size as f64) * 100.0,
            transactions_processed: self.transaction_counter.load(Ordering::SeqCst),
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