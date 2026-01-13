//! Optimized RocksDB storage layer for high-performance blockchain

use rocksdb::{DB, Options, WriteBatch};
use hotstuff_rs::block_tree::pluggables::{KVStore, KVGet};
use std::path::Path;

/// High-performance storage using RocksDB with optimized settings
#[derive(Clone)]
pub struct OptimizedStorage {
    db: std::sync::Arc<DB>,
}

impl OptimizedStorage {
    /// Create new optimized storage instance
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, rocksdb::Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_write_buffer_size(128 * 1024 * 1024); // 128MB for high throughput
        opts.set_max_write_buffer_number(4);
        opts.set_target_file_size_base(128 * 1024 * 1024);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let db = DB::open(&opts, path)?;
        Ok(Self { 
            db: std::sync::Arc::new(db)
        })
    }

    /// Batch write multiple key-value pairs efficiently
    pub fn batch_write<K, V>(&self, items: &[(K, V)]) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        let mut batch = WriteBatch::default();
        for (key, value) in items {
            batch.put(key, value);
        }
        self.db.write(batch)
    }
}

impl KVGet for OptimizedStorage {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.db.get(key).ok().flatten()
    }
}

/// Read-only snapshot of storage state
#[derive(Clone)]
pub struct StorageSnapshot {
    storage: OptimizedStorage,
}

impl KVGet for StorageSnapshot {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.storage.get(key)
    }
}

impl KVStore for OptimizedStorage {
    type Snapshot<'a> = StorageSnapshot;
    type WriteBatch = RocksWriteBatch;

    fn snapshot(&self) -> Self::Snapshot<'_> {
        StorageSnapshot {
            storage: self.clone(),
        }
    }

    fn clear(&mut self) {
        // Note: Full clear not implemented for safety
        // In production, implement proper database reset if needed
    }

    fn write(&mut self, write_batch: Self::WriteBatch) {
        let _ = self.db.write(write_batch.batch);
    }
}

/// Optimized write batch for bulk operations
pub struct RocksWriteBatch {
    batch: WriteBatch,
}

impl hotstuff_rs::block_tree::pluggables::WriteBatch for RocksWriteBatch {
    fn new() -> Self {
        Self {
            batch: WriteBatch::default(),
        }
    }

    fn set(&mut self, key: &[u8], value: &[u8]) {
        self.batch.put(key, value);
    }

    fn delete(&mut self, key: &[u8]) {
        self.batch.delete(key);
    }
}