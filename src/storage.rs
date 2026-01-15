//! Optimized RocksDB storage layer for high-performance blockchain

use crate::config::{StorageConfig, CompressionType};
use rocksdb::{DB, Direction, IteratorMode, Options, WriteBatch};
use hotstuff_rs::block_tree::pluggables::{KVStore, KVGet};
use std::path::Path;

/// High-performance storage using RocksDB with optimized settings
#[derive(Clone)]
pub struct OptimizedStorage {
    db: std::sync::Arc<DB>,
}

impl OptimizedStorage {
    /// Create new optimized storage instance with default settings
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, rocksdb::Error> {
        let config = StorageConfig {
            data_directory: path.as_ref().to_string_lossy().to_string(),
            ..Default::default()
        };
        Self::new_with_config(&config)
    }

    /// Create new optimized storage instance with custom configuration
    pub fn new_with_config(config: &StorageConfig) -> Result<Self, rocksdb::Error> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        
        // Apply configuration settings
        opts.set_write_buffer_size((config.write_buffer_size_mb * 1024 * 1024) as usize);
        opts.set_max_write_buffer_number(config.max_write_buffer_number as i32);
        opts.set_target_file_size_base((config.target_file_size_mb * 1024 * 1024) as u64);
        
        // Set compression type
        let compression = match config.compression_type {
            CompressionType::None => rocksdb::DBCompressionType::None,
            CompressionType::Snappy => rocksdb::DBCompressionType::Snappy,
            CompressionType::Lz4 => rocksdb::DBCompressionType::Lz4,
            CompressionType::Zstd => rocksdb::DBCompressionType::Zstd,
        };
        opts.set_compression_type(compression);

        // Additional performance optimizations
        opts.set_level_compaction_dynamic_level_bytes(true);
        opts.set_max_background_jobs(4);
        opts.optimize_for_point_lookup(1024);

        let db = DB::open(&opts, &config.data_directory)?;
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

    /// Write a single key/value pair.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<(), rocksdb::Error> {
        self.db.put(key, value)
    }

    /// Delete a key.
    pub fn delete(&self, key: &[u8]) -> Result<(), rocksdb::Error> {
        self.db.delete(key)
    }

    /// Scan keys with the given prefix in lexicographic order.
    ///
    /// If `cursor` is provided, scanning resumes *after* the cursor key.
    pub fn scan_prefix(
        &self,
        prefix: &[u8],
        cursor: Option<&[u8]>,
        limit: usize,
    ) -> Vec<(Vec<u8>, Vec<u8>)> {
        let start = cursor.unwrap_or(prefix);
        let mut out = Vec::new();

        for item in self.db.iterator(IteratorMode::From(start, Direction::Forward)) {
            let Ok((key, value)) = item else {
                continue;
            };

            if cursor.is_some() && key.as_ref() == start {
                continue;
            }

            if !key.as_ref().starts_with(prefix) {
                break;
            }

            out.push((key.to_vec(), value.to_vec()));
            if out.len() >= limit {
                break;
            }
        }

        out
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