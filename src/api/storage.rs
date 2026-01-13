//! Storage Query Layer
//! 
//! Provides indexed, read-only access to blockchain data.
//! All queries return only finalized/committed data.

use crate::{
    storage::OptimizedStorage,
    errors::AtomiqResult,
};
use hotstuff_rs::block_tree::pluggables::KVGet;
use std::sync::Arc;
use serde::{Deserialize, Serialize};

// Local type definitions matching the actual blockchain data structures
#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

/// API-specific storage interface
pub struct ApiStorage {
    storage: Arc<OptimizedStorage>,
}

impl ApiStorage {
    pub fn new(storage: Arc<OptimizedStorage>) -> Self {
        Self { storage }
    }

    /// Get the latest finalized block height
    pub fn get_latest_height(&self) -> AtomiqResult<u64> {
        // Try to get latest_height key
        if let Some(bytes) = self.storage.get(b"latest_height") {
            let height = u64::from_le_bytes(bytes.try_into().map_err(|_| {
                crate::errors::AtomiqError::Storage(
                    crate::errors::StorageError::CorruptedData("Invalid height bytes".to_string())
                )
            })?);
            Ok(height)
        } else {
            // No blocks yet
            Ok(0)
        }
    }

    /// Get the latest finalized block hash
    pub fn get_latest_hash(&self) -> AtomiqResult<[u8; 32]> {
        if let Some(bytes) = self.storage.get(b"latest_hash") {
            let hash: [u8; 32] = bytes.try_into().map_err(|_| {
                crate::errors::AtomiqError::Storage(
                    crate::errors::StorageError::CorruptedData("Invalid hash bytes".to_string())
                )
            })?;
            Ok(hash)
        } else {
            Ok([0u8; 32]) // Genesis
        }
    }

    /// Get a block by height (finalized only)
    pub fn get_block_by_height(&self, height: u64) -> AtomiqResult<Option<Block>> {
        let key = format!("block:height:{}", height);
        if let Some(bytes) = self.storage.get(key.as_bytes()) {
            let block: Block = bincode::deserialize(&bytes).map_err(|e| {
                crate::errors::AtomiqError::Storage(
                    crate::errors::StorageError::CorruptedData(format!("Failed to deserialize block: {}", e))
                )
            })?;
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }

    /// Get a block by hash (finalized only)
    pub fn get_block_by_hash(&self, hash: &[u8; 32]) -> AtomiqResult<Option<Block>> {
        let key = format!("block:hash:{}", hex::encode(hash));
        if let Some(bytes) = self.storage.get(key.as_bytes()) {
            let block: Block = bincode::deserialize(&bytes).map_err(|e| {
                crate::errors::AtomiqError::Storage(
                    crate::errors::StorageError::CorruptedData(format!("Failed to deserialize block: {}", e))
                )
            })?;
            Ok(Some(block))
        } else {
            Ok(None)
        }
    }

    /// Get a range of blocks (finalized only)
    /// Returns blocks in descending order (newest first)
    pub fn get_block_range(&self, from: u64, to: u64, limit: usize) -> AtomiqResult<Vec<Block>> {
        let mut blocks = Vec::new();
        let start = from.min(to);
        let end = from.max(to);
        
        // Iterate from highest to lowest height (newest first)
        for height in (start..=end).rev().take(limit) {
            if let Some(block) = self.get_block_by_height(height)? {
                blocks.push(block);
            }
        }
        
        Ok(blocks)
    }

    /// Find transaction by ID across all blocks
    /// Returns (block_height, tx_index, transaction)
    pub fn find_transaction(&self, tx_id: u64) -> AtomiqResult<Option<(u64, usize, Transaction)>> {
        let latest_height = self.get_latest_height()?;
        
        // Search backwards from latest block (most recent transactions first)
        for height in (0..=latest_height).rev() {
            if let Some(block) = self.get_block_by_height(height)? {
                for (idx, tx) in block.transactions.iter().enumerate() {
                    if tx.id == tx_id {
                        return Ok(Some((height, idx, tx.clone())));
                    }
                }
            }
        }
        
        Ok(None)
    }

    /// Check if the node is catching up (always false for single node)
    pub fn is_catching_up(&self) -> bool {
        // For single-node DirectCommit mode, we're never "catching up"
        // This would check sync status in a multi-node setup
        false
    }
}
