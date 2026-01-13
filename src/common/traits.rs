//! Shared traits and interfaces
//!
//! This module defines common traits used throughout the system for better
//! abstraction and testability.

use crate::errors::AtomiqResult;
use crate::common::types::{Block, Transaction};
use async_trait::async_trait;

/// Generic storage interface for blockchain data
#[async_trait]
pub trait BlockchainStorage: Send + Sync {
    /// Store a block in the blockchain
    async fn store_block(&self, block: &Block) -> AtomiqResult<()>;
    
    /// Retrieve a block by its height
    async fn get_block_by_height(&self, height: u64) -> AtomiqResult<Option<Block>>;
    
    /// Retrieve a block by its hash
    async fn get_block_by_hash(&self, hash: &[u8; 32]) -> AtomiqResult<Option<Block>>;
    
    /// Get the latest block height
    async fn get_latest_height(&self) -> AtomiqResult<u64>;
    
    /// Get blocks in a range
    async fn get_blocks_range(&self, from: u64, to: u64) -> AtomiqResult<Vec<Block>>;
    
    /// Store a transaction
    async fn store_transaction(&self, tx: &Transaction, block_height: u64) -> AtomiqResult<()>;
    
    /// Retrieve a transaction by ID
    async fn get_transaction(&self, tx_id: u64) -> AtomiqResult<Option<Transaction>>;
    
    /// Get all transactions in a block
    async fn get_block_transactions(&self, block_height: u64) -> AtomiqResult<Vec<Transaction>>;
}

/// Network interface for blockchain communication
#[async_trait]
pub trait NetworkInterface: Send + Sync {
    /// Broadcast a block to the network
    async fn broadcast_block(&self, block: &Block) -> AtomiqResult<()>;
    
    /// Broadcast a transaction to the network
    async fn broadcast_transaction(&self, tx: &Transaction) -> AtomiqResult<()>;
    
    /// Receive incoming messages from peers
    async fn receive_message(&self) -> AtomiqResult<NetworkMessage>;
    
    /// Connect to a peer
    async fn connect_peer(&self, peer_id: &str) -> AtomiqResult<()>;
    
    /// Get list of connected peers
    async fn get_peers(&self) -> AtomiqResult<Vec<String>>;
}

/// Consensus interface for block validation and agreement
#[async_trait]
pub trait ConsensusEngine: Send + Sync {
    /// Validate a proposed block
    async fn validate_block(&self, block: &Block) -> AtomiqResult<bool>;
    
    /// Propose a new block
    async fn propose_block(&self, transactions: Vec<Transaction>) -> AtomiqResult<Block>;
    
    /// Process consensus messages
    async fn process_consensus_message(&self, message: ConsensusMessage) -> AtomiqResult<()>;
    
    /// Get current consensus state
    async fn get_consensus_state(&self) -> AtomiqResult<ConsensusState>;
}

/// Configuration management interface
pub trait ConfigManager: Send + Sync {
    /// Load configuration from source
    fn load_config(&self) -> AtomiqResult<AtomiqConfig>;
    
    /// Save configuration to source
    fn save_config(&self, config: &AtomiqConfig) -> AtomiqResult<()>;
    
    /// Validate configuration
    fn validate_config(&self, config: &AtomiqConfig) -> AtomiqResult<()>;
}

/// Metrics collection interface
#[async_trait]
pub trait MetricsCollector: Send + Sync {
    /// Record a metric value
    async fn record_metric(&self, name: &str, value: f64) -> AtomiqResult<()>;
    
    /// Record a counter increment
    async fn increment_counter(&self, name: &str) -> AtomiqResult<()>;
    
    /// Record timing information
    async fn record_timing(&self, name: &str, duration_ms: u64) -> AtomiqResult<()>;
    
    /// Get collected metrics
    async fn get_metrics(&self) -> AtomiqResult<MetricsSnapshot>;
}

/// Network message types
#[derive(Clone, Debug)]
pub enum NetworkMessage {
    Block(Block),
    Transaction(Transaction),
    Consensus(ConsensusMessage),
    Sync(SyncMessage),
}

/// Consensus message types
#[derive(Clone, Debug)]
pub enum ConsensusMessage {
    Propose { block: Block, round: u64 },
    Vote { block_hash: [u8; 32], round: u64 },
    Commit { block_hash: [u8; 32], round: u64 },
}

/// Sync message types
#[derive(Clone, Debug)]
pub enum SyncMessage {
    RequestBlocks { from_height: u64, count: u32 },
    ResponseBlocks { blocks: Vec<Block> },
    RequestState { height: u64 },
    ResponseState { state: StateSnapshot },
}

/// Consensus state information
#[derive(Clone, Debug)]
pub struct ConsensusState {
    pub current_round: u64,
    pub current_height: u64,
    pub leader: String,
    pub participants: Vec<String>,
}

/// Configuration structure
#[derive(Clone, Debug)]
pub struct AtomiqConfig {
    pub network: NetworkConfig,
    pub consensus: ConsensusConfig,
    pub storage: StorageConfig,
    pub api: ApiConfig,
    pub metrics: MetricsConfig,
}

#[derive(Clone, Debug)]
pub struct NetworkConfig {
    pub listen_address: String,
    pub port: u16,
    pub peers: Vec<String>,
    pub max_connections: u32,
}

#[derive(Clone, Debug)]
pub struct ConsensusConfig {
    pub algorithm: String,
    pub timeout_ms: u64,
    pub max_block_size: usize,
    pub max_transactions_per_block: usize,
}

#[derive(Clone, Debug)]
pub struct StorageConfig {
    pub data_dir: String,
    pub cache_size: usize,
    pub sync_writes: bool,
}

#[derive(Clone, Debug)]
pub struct ApiConfig {
    pub enabled: bool,
    pub listen_address: String,
    pub port: u16,
    pub cors_origins: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub collection_interval_ms: u64,
    pub retention_duration_hours: u64,
}

/// Metrics snapshot
#[derive(Clone, Debug)]
pub struct MetricsSnapshot {
    pub timestamp: u64,
    pub counters: std::collections::HashMap<String, u64>,
    pub gauges: std::collections::HashMap<String, f64>,
    pub timings: std::collections::HashMap<String, Vec<u64>>,
}

/// State snapshot for synchronization
#[derive(Clone, Debug)]
pub struct StateSnapshot {
    pub height: u64,
    pub root_hash: [u8; 32],
    pub data: Vec<u8>,
}

/// Result wrapper for asynchronous operations
pub type AsyncResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(test)]
mod tests {
    use super::*;
    
    // Mock implementations for testing
    struct MockStorage;
    
    #[async_trait]
    impl BlockchainStorage for MockStorage {
        async fn store_block(&self, _block: &Block) -> AtomiqResult<()> {
            Ok(())
        }
        
        async fn get_block_by_height(&self, _height: u64) -> AtomiqResult<Option<Block>> {
            Ok(None)
        }
        
        async fn get_block_by_hash(&self, _hash: &[u8; 32]) -> AtomiqResult<Option<Block>> {
            Ok(None)
        }
        
        async fn get_latest_height(&self) -> AtomiqResult<u64> {
            Ok(0)
        }
        
        async fn get_blocks_range(&self, _from: u64, _to: u64) -> AtomiqResult<Vec<Block>> {
            Ok(vec![])
        }
        
        async fn store_transaction(&self, _tx: &Transaction, _block_height: u64) -> AtomiqResult<()> {
            Ok(())
        }
        
        async fn get_transaction(&self, _tx_id: u64) -> AtomiqResult<Option<Transaction>> {
            Ok(None)
        }
        
        async fn get_block_transactions(&self, _block_height: u64) -> AtomiqResult<Vec<Transaction>> {
            Ok(vec![])
        }
    }
    
    #[tokio::test]
    async fn test_mock_storage() {
        let storage = MockStorage;
        assert_eq!(storage.get_latest_height().await.unwrap(), 0);
    }
}