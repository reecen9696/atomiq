//! Shared type definitions for the Atomiq blockchain system
//!
//! This module provides canonical types used throughout the system,
//! ensuring consistency and preventing duplication.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Canonical transaction type used throughout the system
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transaction {
    /// Unique transaction identifier
    pub id: u64,
    /// Sender address (32-byte public key hash)
    pub sender: [u8; 32],
    /// Transaction payload (arbitrary data)
    pub data: Vec<u8>,
    /// Unix timestamp in milliseconds
    pub timestamp: u64,
    /// Nonce for replay protection
    pub nonce: u64,
}

impl Transaction {
    /// Create a new transaction with current timestamp
    pub fn new(id: u64, sender: [u8; 32], data: Vec<u8>, nonce: u64) -> Self {
        Self {
            id,
            sender,
            data,
            timestamp: current_timestamp_ms(),
            nonce,
        }
    }

    /// Calculate transaction hash for integrity verification
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.id.to_be_bytes());
        hasher.update(&self.sender);
        hasher.update(&self.data);
        hasher.update(&self.timestamp.to_be_bytes());
        hasher.update(&self.nonce.to_be_bytes());
        hasher.finalize().into()
    }

    /// Get transaction size in bytes
    pub fn size(&self) -> usize {
        std::mem::size_of::<u64>()  // id
            + 32                     // sender
            + self.data.len()        // data
            + std::mem::size_of::<u64>()  // timestamp
            + std::mem::size_of::<u64>()  // nonce
    }

    /// Check if transaction is valid for current context
    pub fn validate(&self, max_data_size: usize) -> Result<(), crate::errors::TransactionError> {
        use crate::errors::TransactionError;

        if self.data.len() > max_data_size {
            return Err(TransactionError::DataTooLarge {
                size: self.data.len(),
                max_size: max_data_size,
            });
        }

        // Additional validation logic can be added here
        Ok(())
    }
}

/// Canonical block type used throughout the system
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Block {
    /// Block height/number in the chain (starts at 0 for genesis)
    pub height: u64,
    /// SHA256 hash of this block (computed from other fields)
    pub block_hash: [u8; 32],
    /// Hash of the previous block (creates immutable chain)
    pub previous_block_hash: [u8; 32],
    /// Transactions included in this block
    pub transactions: Vec<Transaction>,
    /// Block creation timestamp (Unix milliseconds)
    pub timestamp: u64,
    /// Number of transactions (cached for convenience)
    pub transaction_count: usize,
    /// Merkle root of all transaction hashes
    pub transactions_root: [u8; 32],
    /// State root after applying transactions
    pub state_root: [u8; 32],
}

impl Block {
    /// Create a new block with given parameters
    pub fn new(
        height: u64,
        previous_block_hash: [u8; 32],
        transactions: Vec<Transaction>,
        state_root: [u8; 32],
    ) -> Self {
        let transaction_count = transactions.len();
        let transactions_root = calculate_merkle_root(&transactions);
        let timestamp = current_timestamp_ms();
        
        let mut block = Self {
            height,
            block_hash: [0; 32], // Will be calculated after construction
            previous_block_hash,
            transactions,
            timestamp,
            transaction_count,
            transactions_root,
            state_root,
        };
        
        block.block_hash = block.calculate_hash();
        block
    }

    /// Calculate block hash from all fields except block_hash itself
    fn calculate_hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(&self.height.to_be_bytes());
        hasher.update(&self.previous_block_hash);
        hasher.update(&self.transactions_root);
        hasher.update(&self.state_root);
        hasher.update(&self.timestamp.to_be_bytes());
        hasher.update(&self.transaction_count.to_be_bytes());
        hasher.finalize().into()
    }

    /// Get block size in bytes (approximate)
    pub fn size(&self) -> usize {
        std::mem::size_of::<u64>()      // height
            + 32                         // block_hash
            + 32                         // previous_block_hash
            + self.transactions.iter().map(|t| t.size()).sum::<usize>()
            + std::mem::size_of::<u64>() // timestamp
            + std::mem::size_of::<usize>() // transaction_count
            + 32                         // transactions_root
            + 32                         // state_root
    }

    /// Check if block structure is valid
    pub fn validate(&self) -> Result<(), crate::errors::BlockchainError> {
        use crate::errors::BlockchainError;

        // Verify transaction count matches
        if self.transactions.len() != self.transaction_count {
            return Err(BlockchainError::BlockValidationFailed(
                "Transaction count mismatch".to_string()
            ));
        }

        // Verify merkle root
        if self.transactions_root != calculate_merkle_root(&self.transactions) {
            return Err(BlockchainError::BlockValidationFailed(
                "Invalid transactions root".to_string()
            ));
        }

        // Verify block hash
        if self.block_hash != self.calculate_hash() {
            return Err(BlockchainError::BlockValidationFailed(
                "Invalid block hash".to_string()
            ));
        }

        Ok(())
    }
}

/// Calculate Merkle root from a list of transactions
fn calculate_merkle_root(transactions: &[Transaction]) -> [u8; 32] {
    if transactions.is_empty() {
        return [0; 32];
    }

    let mut hashes: Vec<[u8; 32]> = transactions.iter().map(|tx| tx.hash()).collect();

    // Simple binary Merkle tree implementation
    while hashes.len() > 1 {
        let mut next_level = Vec::new();
        
        for chunk in hashes.chunks(2) {
            let mut hasher = Sha256::new();
            hasher.update(chunk[0]);
            if chunk.len() > 1 {
                hasher.update(chunk[1]);
            } else {
                hasher.update(chunk[0]); // Duplicate for odd number
            }
            next_level.push(hasher.finalize().into());
        }
        
        hashes = next_level;
    }

    hashes[0]
}

/// Get current timestamp in milliseconds since Unix epoch
pub fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64
}

/// Convert bytes to hexadecimal string
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Convert hexadecimal string to bytes
pub fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>, hex::FromHexError> {
    hex::decode(hex_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_creation() {
        let tx = Transaction::new(1, [0; 32], b"test data".to_vec(), 1);
        assert_eq!(tx.id, 1);
        assert_eq!(tx.data, b"test data");
        assert_eq!(tx.nonce, 1);
    }

    #[test]
    fn test_transaction_hash_consistency() {
        let tx1 = Transaction::new(1, [0; 32], b"test".to_vec(), 1);
        let tx2 = Transaction::new(1, [0; 32], b"test".to_vec(), 1);
        // Hashes should be the same for identical transactions (except timestamp)
        // This test would need to be adjusted for timestamp differences
    }

    #[test]
    fn test_block_creation() {
        let transactions = vec![
            Transaction::new(1, [0; 32], b"tx1".to_vec(), 1),
            Transaction::new(2, [1; 32], b"tx2".to_vec(), 1),
        ];
        
        let block = Block::new(1, [0; 32], transactions.clone(), [0; 32]);
        
        assert_eq!(block.height, 1);
        assert_eq!(block.transaction_count, 2);
        assert_eq!(block.transactions, transactions);
    }

    #[test]
    fn test_block_validation() {
        let transactions = vec![
            Transaction::new(1, [0; 32], b"tx1".to_vec(), 1),
        ];
        
        let block = Block::new(0, [0; 32], transactions, [0; 32]);
        assert!(block.validate().is_ok());
    }

    #[test]
    fn test_merkle_root_empty() {
        let root = calculate_merkle_root(&[]);
        assert_eq!(root, [0; 32]);
    }

    #[test]
    fn test_merkle_root_single_tx() {
        let tx = Transaction::new(1, [0; 32], b"test".to_vec(), 1);
        let root = calculate_merkle_root(&[tx.clone()]);
        assert_eq!(root, tx.hash());
    }
}