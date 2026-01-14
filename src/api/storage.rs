//! Storage Query Layer
//! 
//! Provides indexed, read-only access to blockchain data.
//! All queries return only finalized/committed data.

use crate::{
    storage::OptimizedStorage,
    errors::AtomiqResult,
    Block, Transaction,
};
use hotstuff_rs::block_tree::pluggables::KVGet;
use std::sync::Arc;

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
    /// OPTIMIZED: Uses lightweight hash_idx -> height mapping
    pub fn get_block_by_hash(&self, hash: &[u8; 32]) -> AtomiqResult<Option<Block>> {
        // Use lightweight hash index to find height
        let hash_idx_key = format!("hash_idx:{}", hex::encode(hash));
        if let Some(height_bytes) = self.storage.get(hash_idx_key.as_bytes()) {
            let height = u64::from_le_bytes(height_bytes.try_into().map_err(|_| {
                crate::errors::AtomiqError::Storage(
                    crate::errors::StorageError::CorruptedData("Invalid height bytes in hash index".to_string())
                )
            })?);
            // Now get block by height
            return self.get_block_by_height(height);
        }
        Ok(None)
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

    /// Find transaction by ID using index for O(1) lookup
    /// OPTIMIZED: Uses tx_idx -> (height:index) -> extract from block pattern
    /// Returns (block_height, tx_index, transaction)
    pub fn find_transaction(&self, tx_id: u64) -> AtomiqResult<Option<(u64, usize, Transaction)>> {
        // Use the optimized transaction index (tx_idx:ID -> height:index)
        let tx_idx_key = format!("tx_idx:{}", tx_id);
        if let Some(location_bytes) = self.storage.get(tx_idx_key.as_bytes()) {
            let location_str = String::from_utf8(location_bytes).map_err(|_| {
                crate::errors::AtomiqError::Storage(
                    crate::errors::StorageError::CorruptedData("Invalid transaction index format".to_string())
                )
            })?;
            
            let parts: Vec<&str> = location_str.split(':').collect();
            if parts.len() == 2 {
                let height: u64 = parts[0].parse().map_err(|_| {
                    crate::errors::AtomiqError::Storage(
                        crate::errors::StorageError::CorruptedData("Invalid height in transaction index".to_string())
                    )
                })?;
                let tx_index: usize = parts[1].parse().map_err(|_| {
                    crate::errors::AtomiqError::Storage(
                        crate::errors::StorageError::CorruptedData("Invalid tx_index in transaction index".to_string())
                    )
                })?;
                
                // Get transaction from block (no separate tx_data storage)
                if let Some(block) = self.get_block_by_height(height)? {
                    if tx_index < block.transactions.len() {
                        let transaction = block.transactions[tx_index].clone();
                        return Ok(Some((height, tx_index, transaction)));
                    }
                }
            }
        }
        
        // Fallback to block scanning (for backwards compatibility with old data)
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

    /// Find transaction location by string ID (for new API)
    /// OPTIMIZED: Uses tx_idx:ID -> height:index mapping
    pub fn find_transaction_location(&self, tx_id: &str) -> AtomiqResult<Option<(u64, u32)>> {
        let tx_idx_key = format!("tx_idx:{}", tx_id);
        if let Some(location_bytes) = self.storage.get(tx_idx_key.as_bytes()) {
            let location_str = String::from_utf8(location_bytes).map_err(|_| {
                crate::errors::AtomiqError::Storage(
                    crate::errors::StorageError::CorruptedData("Invalid transaction index format".to_string())
                )
            })?;
            
            let parts: Vec<&str> = location_str.split(':').collect();
            if parts.len() == 2 {
                let height: u64 = parts[0].parse().map_err(|_| {
                    crate::errors::AtomiqError::Storage(
                        crate::errors::StorageError::CorruptedData("Invalid height in transaction index".to_string())
                    )
                })?;
                let tx_index: u32 = parts[1].parse().map_err(|_| {
                    crate::errors::AtomiqError::Storage(
                        crate::errors::StorageError::CorruptedData("Invalid tx_index in transaction index".to_string())
                    )
                })?;
                
                return Ok(Some((height, tx_index)));
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
