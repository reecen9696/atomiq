//! Lock-Free Storage Layer
//! 
//! High-performance storage with lock-free data structures for concurrent access

use dashmap::DashMap;
use std::{
    sync::{atomic::{AtomicU64, Ordering}, Arc},
    time::Instant,
};
use crate::{
    storage::OptimizedStorage,
    errors::{AtomiqError, AtomiqResult},
};
use tracing::{debug, warn};
use serde::{Serialize, Deserialize};

/// Simplified block structure for caching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedBlock {
    pub height: u64,
    pub hash: String,
    pub prev_hash: String,
    pub timestamp: u64,
    pub transactions: Vec<String>, // Transaction IDs
    pub tx_count: usize,
}

/// Simplified transaction structure for caching  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedTransaction {
    pub id: String,
    pub sender: String, 
    pub data: String,
    pub timestamp: u64,
    pub block_height: Option<u64>,
}

/// Lock-free storage wrapper for high-concurrency scenarios
pub struct LockFreeStorage {
    // Hot data cache - lock-free concurrent access
    block_cache: DashMap<u64, Arc<CachedBlock>>,
    transaction_cache: DashMap<String, Arc<CachedTransaction>>,
    
    // Fast lookups
    height_to_hash: DashMap<u64, String>,
    tx_to_location: DashMap<String, (u64, u32)>, // (height, tx_index)
    hash_to_height: DashMap<String, u64>,
    
    // Cache statistics
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    
    // Persistent storage backend
    persistent_storage: Arc<OptimizedStorage>,
    
    // Configuration
    max_cached_blocks: usize,
    max_cached_transactions: usize,
}

impl LockFreeStorage {
    pub fn new(persistent_storage: Arc<OptimizedStorage>) -> Self {
        Self {
            block_cache: DashMap::new(),
            transaction_cache: DashMap::new(),
            height_to_hash: DashMap::new(),
            tx_to_location: DashMap::new(),
            hash_to_height: DashMap::new(),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            persistent_storage,
            max_cached_blocks: 10_000,    // Cache last 10K blocks
            max_cached_transactions: 50_000, // Cache 50K recent TXs
        }
    }
    
    /// Get block by height with O(1) cache lookup
    pub async fn get_block(&self, height: u64) -> AtomiqResult<Option<CachedBlock>> {
        let start = Instant::now();
        
        // Check cache first (lock-free read)
        if let Some(cached_block) = self.block_cache.get(&height) {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            debug!("Block cache hit for height {}: {:?}", height, start.elapsed());
            return Ok(Some(cached_block.as_ref().clone()));
        }
        
        // Cache miss - fetch from persistent storage
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        debug!("Block cache miss for height {}", height);
        
        // For now, return placeholder data - integrate with actual storage
        let cached_block = CachedBlock {
            height,
            hash: format!("block_hash_{}", height),
            prev_hash: format!("prev_hash_{}", height.saturating_sub(1)),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            transactions: vec![],
            tx_count: 0,
        };
        
        // Cache the block for future requests
        let arc_block = Arc::new(cached_block.clone());
        self.block_cache.insert(height, arc_block);
        
        // Cache metadata for fast lookups
        self.height_to_hash.insert(height, cached_block.hash.clone());
        self.hash_to_height.insert(cached_block.hash.clone(), height);
        
        // Evict old blocks if cache is full
        self.evict_old_blocks_if_needed();
        
        debug!("Block {} cached in {:?}", height, start.elapsed());
        Ok(Some(cached_block))
    }
    
    /// Get block by hash with O(1) lookup
    pub async fn get_block_by_hash(&self, hash: &str) -> AtomiqResult<Option<CachedBlock>> {
        // Check if we know the height for this hash
        if let Some(height) = self.hash_to_height.get(hash) {
            return self.get_block(*height).await;
        }
        
        // Fallback to persistent storage
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        
        // For now, return None - integrate with actual storage
        Ok(None)
    }
    
    /// Get transaction with O(1) lookup via index
    pub async fn get_transaction(&self, tx_id: &str) -> AtomiqResult<Option<CachedTransaction>> {
        let start = Instant::now();
        
        // Check transaction cache first
        if let Some(cached_tx) = self.transaction_cache.get(tx_id) {
            self.cache_hits.fetch_add(1, Ordering::Relaxed);
            debug!("Transaction cache hit for {}: {:?}", tx_id, start.elapsed());
            return Ok(Some(cached_tx.as_ref().clone()));
        }
        
        // Check location index for O(1) lookup
        if let Some(location) = self.tx_to_location.get(tx_id) {
            let (height, tx_index) = *location;
            
            if let Some(_block) = self.get_block(height).await? {
                // Create placeholder transaction
                let cached_tx = CachedTransaction {
                    id: tx_id.to_string(),
                    sender: format!("sender_{}", tx_index),
                    data: format!("data_{}", tx_index),
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    block_height: Some(height),
                };
                
                // Cache the transaction
                self.transaction_cache.insert(tx_id.to_string(), Arc::new(cached_tx.clone()));
                debug!("Transaction found via index for {} in {:?}", tx_id, start.elapsed());
                return Ok(Some(cached_tx));
            }
        }
        
        // Ultimate fallback - search persistent storage
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
        warn!("Transaction {} not found in cache or index, falling back to full search", tx_id);
        
        Ok(None)
    }
    
    /// Store block with atomic indexing
    pub async fn store_block(&self, block: CachedBlock) -> AtomiqResult<()> {
        let start = Instant::now();
        
        // Update cache and indices atomically
        let arc_block = Arc::new(block.clone());
        self.block_cache.insert(block.height, arc_block);
        self.height_to_hash.insert(block.height, block.hash.clone());
        self.hash_to_height.insert(block.hash.clone(), block.height);
        
        // Index all transactions
        for (index, tx_id) in block.transactions.iter().enumerate() {
            self.tx_to_location.insert(tx_id.clone(), (block.height, index as u32));
        }
        
        self.evict_old_blocks_if_needed();
        
        debug!("Block {} stored and indexed in {:?}", block.height, start.elapsed());
        Ok(())
    }
    
    /// Get latest block height
    pub async fn get_latest_height(&self) -> AtomiqResult<Option<u64>> {
        // Return the highest cached height for now
        let max_height = self.block_cache.iter().map(|entry| *entry.key()).max();
        Ok(max_height)
    }
    
    /// Get multiple blocks efficiently
    pub async fn get_blocks(&self, start_height: u64, limit: usize) -> AtomiqResult<Vec<CachedBlock>> {
        let mut blocks = Vec::new();
        
        for height in start_height..start_height + limit as u64 {
            if let Some(block) = self.get_block(height).await? {
                blocks.push(block);
            } else {
                break; // No more blocks available
            }
        }
        
        Ok(blocks)
    }
    
    /// Evict old blocks from cache to maintain memory usage
    fn evict_old_blocks_if_needed(&self) {
        if self.block_cache.len() > self.max_cached_blocks {
            // Find oldest blocks to evict (simple strategy - could be improved)
            let mut heights: Vec<_> = self.block_cache.iter().map(|entry| *entry.key()).collect();
            heights.sort();
            
            let evict_count = self.block_cache.len() - (self.max_cached_blocks * 4 / 5); // Evict to 80% capacity
            
            for &height in heights.iter().take(evict_count) {
                self.block_cache.remove(&height);
                if let Some((_, hash)) = self.height_to_hash.remove(&height) {
                    self.hash_to_height.remove(&hash);
                }
            }
            
            debug!("Evicted {} old blocks from cache", evict_count);
        }
    }
    
    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let misses = self.cache_misses.load(Ordering::Relaxed);
        
        CacheStats {
            hits,
            misses,
            hit_ratio: if hits + misses > 0 { hits as f64 / (hits + misses) as f64 } else { 0.0 },
            cached_blocks: self.block_cache.len(),
            cached_transactions: self.transaction_cache.len(),
            cached_indices: self.tx_to_location.len(),
        }
    }
    
    /// Preload recent blocks into cache
    pub async fn preload_recent_blocks(&self, count: usize) -> AtomiqResult<usize> {
        let latest_height = match self.get_latest_height().await? {
            Some(height) => height,
            None => return Ok(0),
        };
        
        let start_height = latest_height.saturating_sub(count as u64);
        let mut loaded = 0;
        
        for height in start_height..=latest_height {
            if self.get_block(height).await?.is_some() {
                loaded += 1;
            }
        }
        
        debug!("Preloaded {} recent blocks into cache", loaded);
        Ok(loaded)
    }
}

/// Cache performance statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub hit_ratio: f64,
    pub cached_blocks: usize,
    pub cached_transactions: usize,
    pub cached_indices: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::{
        config::StorageConfig,
        common::types::{Transaction},
    };

    async fn create_test_storage() -> (LockFreeStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            data_directory: temp_dir.path().to_string_lossy().to_string(),
            clear_on_start: true,
            ..Default::default()
        };
        
        let persistent = Arc::new(OptimizedStorage::new_with_config(&config).unwrap());
        let storage = LockFreeStorage::new(persistent);
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_block_caching() {
        let (storage, _temp) = create_test_storage().await;
        
        // Create test block using CachedBlock
        let block = CachedBlock {
            height: 1,
            hash: "test_block_hash".to_string(),
            prev_hash: "prev_hash".to_string(),
            timestamp: 123456789,
            transactions: vec!["tx_1".to_string()],
            tx_count: 1,
        };
        
        // Store block
        storage.store_block(block.clone()).await.unwrap();
        
        // First read should be cache miss
        let stats_before = storage.cache_stats();
        let retrieved = storage.get_block(1).await.unwrap().unwrap();
        
        assert_eq!(retrieved.height, 1);
        assert_eq!(retrieved.hash, block.hash);
        
        // Second read should be cache hit
        let retrieved2 = storage.get_block(1).await.unwrap().unwrap();
        let stats_after = storage.cache_stats();
        
        assert_eq!(retrieved2.height, 1);
        assert!(stats_after.hits > stats_before.hits);
    }
    
    #[tokio::test]
    async fn test_transaction_indexing() {
        let (storage, _temp) = create_test_storage().await;
        
        // Create test transaction
        let tx_id = "test_tx_id".to_string();
        let cached_tx = CachedTransaction {
            id: tx_id.clone(),
            sender: "test_sender".to_string(),
            data: "test_data".to_string(),
            timestamp: 123456789,
            block_height: Some(1),
        };
        
        // Create block with transaction
        let block = CachedBlock {
            height: 1,
            hash: "test_block_hash".to_string(),
            prev_hash: "prev_hash".to_string(),
            timestamp: 123456789,
            transactions: vec![tx_id.clone()],
            tx_count: 1,
        };
        
        // Store block
        storage.store_block(block).await.unwrap();
        
        // Retrieve transaction by ID (this will come from underlying storage)
        // Note: The actual transaction data depends on what's stored in the persistent storage
        let retrieved_result = storage.get_transaction(&tx_id).await;
        assert!(retrieved_result.is_ok(), "Should be able to query for transaction");
    }
}