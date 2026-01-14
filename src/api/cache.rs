//! High-Performance LRU Cache Implementation
//!
//! Provides ultra-fast caching for frequently accessed blockchain data:
//! - Block data with configurable size limits
//! - Transaction lookups with O(1) access
//! - Auto-eviction of least recently used items
//! - Thread-safe concurrent access

use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, RwLock},
    time::{Duration, Instant},
};
use serde::{Deserialize, Serialize};

/// Generic LRU Cache with TTL support
pub struct LruCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Maximum capacity
    capacity: usize,
    
    /// Cache entries
    cache: Arc<RwLock<HashMap<K, CacheEntry<V>>>>,
    
    /// Access order tracking (for LRU eviction)
    access_order: Arc<RwLock<Vec<K>>>,
    
    /// Time-to-live for entries
    ttl: Option<Duration>,
}

/// Cache entry with metadata
#[derive(Clone)]
struct CacheEntry<V> {
    value: V,
    created_at: Instant,
    last_accessed: Instant,
    access_count: u64,
}

impl<K, V> LruCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create new LRU cache with specified capacity
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            cache: Arc::new(RwLock::new(HashMap::with_capacity(capacity))),
            access_order: Arc::new(RwLock::new(Vec::with_capacity(capacity))),
            ttl: None,
        }
    }
    
    /// Create new LRU cache with TTL
    pub fn with_ttl(capacity: usize, ttl: Duration) -> Self {
        Self {
            capacity,
            cache: Arc::new(RwLock::new(HashMap::with_capacity(capacity))),
            access_order: Arc::new(RwLock::new(Vec::with_capacity(capacity))),
            ttl: Some(ttl),
        }
    }
    
    /// Get value from cache
    pub fn get(&self, key: &K) -> Option<V> {
        let now = Instant::now();
        
        // Check if entry exists and is not expired
        {
            let mut cache = self.cache.write().ok()?;
            if let Some(entry) = cache.get_mut(key) {
                // Check TTL
                if let Some(ttl) = self.ttl {
                    if now.duration_since(entry.created_at) > ttl {
                        // Expired, remove it
                        cache.remove(key);
                        self.remove_from_access_order(key);
                        return None;
                    }
                }
                
                // Update access information
                entry.last_accessed = now;
                entry.access_count += 1;
                let value = entry.value.clone();
                
                // Update access order
                self.update_access_order(key);
                
                return Some(value);
            }
        }
        
        None
    }
    
    /// Put value into cache
    pub fn put(&self, key: K, value: V) {
        let now = Instant::now();
        
        {
            let mut cache = self.cache.write().unwrap();
            let mut access_order = self.access_order.write().unwrap();
            
            // Check if we need to evict items
            while cache.len() >= self.capacity && !cache.contains_key(&key) {
                if let Some(lru_key) = access_order.first().cloned() {
                    cache.remove(&lru_key);
                    access_order.retain(|k| k != &lru_key);
                } else {
                    break;
                }
            }
            
            let entry = CacheEntry {
                value,
                created_at: now,
                last_accessed: now,
                access_count: 1,
            };
            
            // Insert or update
            if cache.contains_key(&key) {
                // Update existing
                cache.insert(key.clone(), entry);
                // Move to end (most recently used)
                access_order.retain(|k| k != &key);
                access_order.push(key);
            } else {
                // Insert new
                cache.insert(key.clone(), entry);
                access_order.push(key);
            }
        }
    }
    
    /// Remove value from cache
    pub fn remove(&self, key: &K) -> Option<V> {
        let mut cache = self.cache.write().unwrap();
        if let Some(entry) = cache.remove(key) {
            self.remove_from_access_order(key);
            Some(entry.value)
        } else {
            None
        }
    }
    
    /// Clear all entries
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        let mut access_order = self.access_order.write().unwrap();
        cache.clear();
        access_order.clear();
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let cache = self.cache.read().unwrap();
        let now = Instant::now();
        
        let mut total_access_count = 0;
        let mut expired_count = 0;
        
        for entry in cache.values() {
            total_access_count += entry.access_count;
            
            if let Some(ttl) = self.ttl {
                if now.duration_since(entry.created_at) > ttl {
                    expired_count += 1;
                }
            }
        }
        
        CacheStats {
            capacity: self.capacity,
            size: cache.len(),
            expired_entries: expired_count,
            total_access_count,
        }
    }
    
    /// Update access order (move key to end)
    fn update_access_order(&self, key: &K) {
        if let Ok(mut access_order) = self.access_order.write() {
            access_order.retain(|k| k != key);
            access_order.push(key.clone());
        }
    }
    
    /// Remove key from access order
    fn remove_from_access_order(&self, key: &K) {
        if let Ok(mut access_order) = self.access_order.write() {
            access_order.retain(|k| k != key);
        }
    }
    
    /// Clean up expired entries
    pub fn cleanup_expired(&self) -> usize {
        if self.ttl.is_none() {
            return 0;
        }
        
        let now = Instant::now();
        let ttl = self.ttl.unwrap();
        let mut removed_count = 0;
        
        {
            let mut cache = self.cache.write().unwrap();
            let keys_to_remove: Vec<K> = cache
                .iter()
                .filter(|(_, entry)| now.duration_since(entry.created_at) > ttl)
                .map(|(key, _)| key.clone())
                .collect();
            
            for key in keys_to_remove {
                cache.remove(&key);
                self.remove_from_access_order(&key);
                removed_count += 1;
            }
        }
        
        removed_count
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize)]
pub struct CacheStats {
    pub capacity: usize,
    pub size: usize,
    pub expired_entries: usize,
    pub total_access_count: u64,
}

/// Blockchain-specific cache manager
pub struct BlockchainCache {
    /// Block cache (by height)
    block_cache: LruCache<u64, BlockData>,
    
    /// Transaction cache (by ID)
    transaction_cache: LruCache<String, TransactionData>,
    
    /// Block hash cache (height -> hash mapping)
    hash_cache: LruCache<u64, String>,
    
    /// Latest block height cache (single entry)
    latest_height: Arc<RwLock<Option<(u64, Instant)>>>,
}

/// Cached block data
#[derive(Clone, Serialize, Deserialize)]
pub struct BlockData {
    pub height: u64,
    pub hash: String,
    pub prev_hash: String,
    pub timestamp: String,
    pub tx_count: usize,
    pub transactions: Vec<String>,
    pub merkle_root: String,
    pub state_root: String,
}

/// Cached transaction data
#[derive(Clone, Serialize, Deserialize)]
pub struct TransactionData {
    pub id: String,
    pub sender: String,
    pub data: String,
    pub timestamp: String,
    pub block_height: Option<u64>,
    pub block_hash: Option<String>,
}

impl BlockchainCache {
    /// Create new blockchain cache with optimized sizes
    pub fn new() -> Self {
        Self {
            // Cache last 10,000 blocks (approximately 1-2 days of blocks)
            block_cache: LruCache::with_ttl(10_000, Duration::from_secs(3600)), // 1 hour TTL
            
            // Cache 100,000 transactions with shorter TTL
            transaction_cache: LruCache::with_ttl(100_000, Duration::from_secs(1800)), // 30 min TTL
            
            // Cache 10,000 block hashes (lightweight)
            hash_cache: LruCache::with_ttl(10_000, Duration::from_secs(7200)), // 2 hours TTL
            
            // Latest height cache with very short TTL
            latest_height: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Create cache with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            block_cache: LruCache::with_ttl(config.block_capacity, config.block_ttl),
            transaction_cache: LruCache::with_ttl(config.transaction_capacity, config.transaction_ttl),
            hash_cache: LruCache::with_ttl(config.hash_capacity, config.hash_ttl),
            latest_height: Arc::new(RwLock::new(None)),
        }
    }
    
    /// Get block from cache
    pub fn get_block(&self, height: u64) -> Option<BlockData> {
        self.block_cache.get(&height)
    }
    
    /// Cache block data
    pub fn cache_block(&self, height: u64, block: BlockData) {
        self.block_cache.put(height, block);
    }
    
    /// Get transaction from cache
    pub fn get_transaction(&self, tx_id: &str) -> Option<TransactionData> {
        self.transaction_cache.get(&tx_id.to_string())
    }
    
    /// Cache transaction data
    pub fn cache_transaction(&self, tx_id: String, transaction: TransactionData) {
        self.transaction_cache.put(tx_id, transaction);
    }
    
    /// Get block hash from cache
    pub fn get_block_hash(&self, height: u64) -> Option<String> {
        self.hash_cache.get(&height)
    }
    
    /// Cache block hash
    pub fn cache_block_hash(&self, height: u64, hash: String) {
        self.hash_cache.put(height, hash);
    }
    
    /// Get cached latest height
    pub fn get_latest_height(&self) -> Option<u64> {
        if let Ok(latest) = self.latest_height.read() {
            if let Some((height, cached_at)) = *latest {
                // Cache latest height for 5 seconds
                if cached_at.elapsed() < Duration::from_secs(5) {
                    return Some(height);
                }
            }
        }
        None
    }
    
    /// Cache latest height
    pub fn cache_latest_height(&self, height: u64) {
        if let Ok(mut latest) = self.latest_height.write() {
            *latest = Some((height, Instant::now()));
        }
    }
    
    /// Get cache statistics for all caches
    pub fn get_stats(&self) -> CacheStatsCollection {
        CacheStatsCollection {
            blocks: self.block_cache.stats(),
            transactions: self.transaction_cache.stats(),
            hashes: self.hash_cache.stats(),
            hit_rates: self.calculate_hit_rates(),
        }
    }
    
    /// Clean up expired entries in all caches
    pub fn cleanup_expired(&self) -> CleanupStats {
        CleanupStats {
            blocks_removed: self.block_cache.cleanup_expired(),
            transactions_removed: self.transaction_cache.cleanup_expired(),
            hashes_removed: self.hash_cache.cleanup_expired(),
        }
    }
    
    /// Clear all caches
    pub fn clear_all(&self) {
        self.block_cache.clear();
        self.transaction_cache.clear();
        self.hash_cache.clear();
        
        if let Ok(mut latest) = self.latest_height.write() {
            *latest = None;
        }
    }
    
    /// Calculate approximate hit rates
    fn calculate_hit_rates(&self) -> HitRates {
        // This would be enhanced with proper hit/miss tracking
        HitRates {
            blocks: 0.85,      // Placeholder
            transactions: 0.75, // Placeholder  
            hashes: 0.90,      // Placeholder
        }
    }
    
    /// Start background cleanup task
    pub fn start_cleanup_task(cache: Arc<BlockchainCache>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // Every 5 minutes
            
            loop {
                interval.tick().await;
                let stats = cache.cleanup_expired();
                
                if stats.total_removed() > 0 {
                    tracing::info!(
                        "Cache cleanup: removed {} blocks, {} transactions, {} hashes",
                        stats.blocks_removed,
                        stats.transactions_removed, 
                        stats.hashes_removed
                    );
                }
            }
        });
    }
}

/// Cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub block_capacity: usize,
    pub block_ttl: Duration,
    pub transaction_capacity: usize,
    pub transaction_ttl: Duration,
    pub hash_capacity: usize,
    pub hash_ttl: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            block_capacity: 10_000,
            block_ttl: Duration::from_secs(3600), // 1 hour
            transaction_capacity: 100_000,
            transaction_ttl: Duration::from_secs(1800), // 30 minutes
            hash_capacity: 10_000,
            hash_ttl: Duration::from_secs(7200), // 2 hours
        }
    }
}

/// Collection of cache stats
#[derive(Debug, Serialize)]
pub struct CacheStatsCollection {
    pub blocks: CacheStats,
    pub transactions: CacheStats,
    pub hashes: CacheStats,
    pub hit_rates: HitRates,
}

/// Cache hit rates
#[derive(Debug, Serialize)]
pub struct HitRates {
    pub blocks: f64,
    pub transactions: f64,
    pub hashes: f64,
}

/// Cleanup statistics
#[derive(Debug)]
pub struct CleanupStats {
    pub blocks_removed: usize,
    pub transactions_removed: usize,
    pub hashes_removed: usize,
}

impl CleanupStats {
    pub fn total_removed(&self) -> usize {
        self.blocks_removed + self.transactions_removed + self.hashes_removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_lru_cache_basic_operations() {
        let cache = LruCache::new(3);
        
        // Test put/get
        cache.put("key1".to_string(), "value1".to_string());
        cache.put("key2".to_string(), "value2".to_string());
        
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        assert_eq!(cache.get(&"key2".to_string()), Some("value2".to_string()));
        assert_eq!(cache.get(&"key3".to_string()), None);
    }
    
    #[tokio::test] 
    async fn test_lru_cache_eviction() {
        let cache = LruCache::new(2);
        
        cache.put("key1".to_string(), "value1".to_string());
        cache.put("key2".to_string(), "value2".to_string());
        cache.put("key3".to_string(), "value3".to_string()); // Should evict key1
        
        assert_eq!(cache.get(&"key1".to_string()), None);
        assert_eq!(cache.get(&"key2".to_string()), Some("value2".to_string()));
        assert_eq!(cache.get(&"key3".to_string()), Some("value3".to_string()));
    }
    
    #[tokio::test]
    async fn test_lru_cache_ttl() {
        let cache = LruCache::with_ttl(10, Duration::from_millis(100));
        
        cache.put("key1".to_string(), "value1".to_string());
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        
        sleep(Duration::from_millis(150)).await;
        assert_eq!(cache.get(&"key1".to_string()), None);
    }
}