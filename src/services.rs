//! Service layer providing dependency injection and clean separation of concerns
//!
//! This module implements the Dependency Injection pattern to create loosely
//! coupled, testable components.

use crate::{
    common::{
        traits::{AtomiqConfig, BlockchainStorage, NetworkInterface, ConsensusEngine, MetricsCollector},
        config::{ConfigLoader},
    },
    errors::AtomiqResult,
    storage::OptimizedStorage,
    api::storage::ApiStorage,
};
use std::sync::Arc;

/// Service container for dependency injection
pub struct ServiceContainer {
    config: AtomiqConfig,
    storage: Arc<dyn BlockchainStorage>,
    network: Arc<dyn NetworkInterface>,
    consensus: Arc<dyn ConsensusEngine>,
    metrics: Arc<dyn MetricsCollector>,
}

impl ServiceContainer {
    /// Create a new service container with the given configuration
    pub async fn new(config: AtomiqConfig) -> AtomiqResult<Self> {
        let storage = Self::create_storage(&config).await?;
        let network = Self::create_network(&config).await?;
        let consensus = Self::create_consensus(&config).await?;
        let metrics = Self::create_metrics(&config).await?;

        Ok(Self {
            config,
            storage,
            network,
            consensus,
            metrics,
        })
    }

    /// Get the configuration
    pub fn config(&self) -> &AtomiqConfig {
        &self.config
    }

    /// Get the storage service
    pub fn storage(&self) -> Arc<dyn BlockchainStorage> {
        Arc::clone(&self.storage)
    }

    /// Get the network service
    pub fn network(&self) -> Arc<dyn NetworkInterface> {
        Arc::clone(&self.network)
    }

    /// Get the consensus service
    pub fn consensus(&self) -> Arc<dyn ConsensusEngine> {
        Arc::clone(&self.consensus)
    }

    /// Get the metrics service
    pub fn metrics(&self) -> Arc<dyn MetricsCollector> {
        Arc::clone(&self.metrics)
    }

    /// Create API storage service
    pub fn create_api_storage(&self) -> ApiStorage {
        // For now, we'll create a basic ApiStorage
        // In a real implementation, this would use the DI container
        ApiStorage::new(Arc::new(
            OptimizedStorage::new(&self.config.storage.data_dir)
                .expect("Failed to open storage")
        ))
    }

    // Private factory methods
    async fn create_storage(config: &AtomiqConfig) -> AtomiqResult<Arc<dyn BlockchainStorage>> {
        // Create storage implementation based on config
        let storage = MockStorageAdapter::new(&config.storage.data_dir).await?;
        Ok(Arc::new(storage))
    }

    async fn create_network(config: &AtomiqConfig) -> AtomiqResult<Arc<dyn NetworkInterface>> {
        // Create network implementation based on config
        let network = MockNetworkAdapter::new(&config.network).await?;
        Ok(Arc::new(network))
    }

    async fn create_consensus(config: &AtomiqConfig) -> AtomiqResult<Arc<dyn ConsensusEngine>> {
        // Create consensus implementation based on config
        let consensus = MockConsensusAdapter::new(&config.consensus).await?;
        Ok(Arc::new(consensus))
    }

    async fn create_metrics(config: &AtomiqConfig) -> AtomiqResult<Arc<dyn MetricsCollector>> {
        // Create metrics implementation based on config
        let metrics = MockMetricsAdapter::new(&config.metrics).await?;
        Ok(Arc::new(metrics))
    }
}

/// Service builder for creating configured service containers
pub struct ServiceBuilder {
    config_path: Option<String>,
    storage_override: Option<Arc<dyn BlockchainStorage>>,
    network_override: Option<Arc<dyn NetworkInterface>>,
}

impl ServiceBuilder {
    /// Create a new service builder
    pub fn new() -> Self {
        Self {
            config_path: None,
            storage_override: None,
            network_override: None,
        }
    }

    /// Set the configuration file path
    pub fn with_config_path(mut self, path: String) -> Self {
        self.config_path = Some(path);
        self
    }

    /// Override the storage implementation (useful for testing)
    pub fn with_storage(mut self, storage: Arc<dyn BlockchainStorage>) -> Self {
        self.storage_override = Some(storage);
        self
    }

    /// Override the network implementation (useful for testing)
    pub fn with_network(mut self, network: Arc<dyn NetworkInterface>) -> Self {
        self.network_override = Some(network);
        self
    }

    /// Build the service container
    pub async fn build(self) -> AtomiqResult<ServiceContainer> {
        let config = if let Some(path) = self.config_path {
            ConfigLoader::new().with_path(path).load()?
        } else {
            AtomiqConfig::default()
        };

        let mut container = ServiceContainer::new(config).await?;

        // Apply overrides
        if let Some(storage) = self.storage_override {
            container.storage = storage;
        }
        if let Some(network) = self.network_override {
            container.network = network;
        }

        Ok(container)
    }
}

impl Default for ServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// Mock implementations for the interim (these will be replaced with real implementations)

use crate::common::traits::{
    NetworkMessage, ConsensusMessage, ConsensusState, MetricsSnapshot,
    NetworkConfig, ConsensusConfig, MetricsConfig,
};
use async_trait::async_trait;

struct MockStorageAdapter {
    _data_dir: String,
}

impl MockStorageAdapter {
    async fn new(data_dir: &str) -> AtomiqResult<Self> {
        Ok(Self {
            _data_dir: data_dir.to_string(),
        })
    }
}

#[async_trait]
impl BlockchainStorage for MockStorageAdapter {
    async fn store_block(&self, _block: &crate::common::types::Block) -> AtomiqResult<()> {
        Ok(())
    }

    async fn get_block_by_height(&self, _height: u64) -> AtomiqResult<Option<crate::common::types::Block>> {
        Ok(None)
    }

    async fn get_block_by_hash(&self, _hash: &[u8; 32]) -> AtomiqResult<Option<crate::common::types::Block>> {
        Ok(None)
    }

    async fn get_latest_height(&self) -> AtomiqResult<u64> {
        Ok(0)
    }

    async fn get_blocks_range(&self, _from: u64, _to: u64) -> AtomiqResult<Vec<crate::common::types::Block>> {
        Ok(vec![])
    }

    async fn store_transaction(&self, _tx: &crate::common::types::Transaction, _block_height: u64) -> AtomiqResult<()> {
        Ok(())
    }

    async fn get_transaction(&self, _tx_id: u64) -> AtomiqResult<Option<crate::common::types::Transaction>> {
        Ok(None)
    }

    async fn get_block_transactions(&self, _block_height: u64) -> AtomiqResult<Vec<crate::common::types::Transaction>> {
        Ok(vec![])
    }
}

struct MockNetworkAdapter {
    _config: NetworkConfig,
}

impl MockNetworkAdapter {
    async fn new(config: &NetworkConfig) -> AtomiqResult<Self> {
        Ok(Self {
            _config: config.clone(),
        })
    }
}

#[async_trait]
impl NetworkInterface for MockNetworkAdapter {
    async fn broadcast_block(&self, _block: &crate::common::types::Block) -> AtomiqResult<()> {
        Ok(())
    }

    async fn broadcast_transaction(&self, _tx: &crate::common::types::Transaction) -> AtomiqResult<()> {
        Ok(())
    }

    async fn receive_message(&self) -> AtomiqResult<NetworkMessage> {
        // This would block waiting for real messages
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        Err(crate::errors::NetworkError::ConnectionFailed("No messages".to_string()).into())
    }

    async fn connect_peer(&self, _peer_id: &str) -> AtomiqResult<()> {
        Ok(())
    }

    async fn get_peers(&self) -> AtomiqResult<Vec<String>> {
        Ok(vec![])
    }
}

struct MockConsensusAdapter {
    _config: ConsensusConfig,
}

impl MockConsensusAdapter {
    async fn new(config: &ConsensusConfig) -> AtomiqResult<Self> {
        Ok(Self {
            _config: config.clone(),
        })
    }
}

#[async_trait]
impl ConsensusEngine for MockConsensusAdapter {
    async fn validate_block(&self, _block: &crate::common::types::Block) -> AtomiqResult<bool> {
        Ok(true)
    }

    async fn propose_block(&self, _transactions: Vec<crate::common::types::Transaction>) -> AtomiqResult<crate::common::types::Block> {
        let block = crate::common::types::Block::new(0, [0; 32], _transactions, [0; 32]);
        Ok(block)
    }

    async fn process_consensus_message(&self, _message: ConsensusMessage) -> AtomiqResult<()> {
        Ok(())
    }

    async fn get_consensus_state(&self) -> AtomiqResult<ConsensusState> {
        Ok(ConsensusState {
            current_round: 0,
            current_height: 0,
            leader: "mock_leader".to_string(),
            participants: vec![],
        })
    }
}

struct MockMetricsAdapter {
    _config: MetricsConfig,
}

impl MockMetricsAdapter {
    async fn new(config: &MetricsConfig) -> AtomiqResult<Self> {
        Ok(Self {
            _config: config.clone(),
        })
    }
}

#[async_trait]
impl MetricsCollector for MockMetricsAdapter {
    async fn record_metric(&self, _name: &str, _value: f64) -> AtomiqResult<()> {
        Ok(())
    }

    async fn increment_counter(&self, _name: &str) -> AtomiqResult<()> {
        Ok(())
    }

    async fn record_timing(&self, _name: &str, _duration_ms: u64) -> AtomiqResult<()> {
        Ok(())
    }

    async fn get_metrics(&self) -> AtomiqResult<MetricsSnapshot> {
        Ok(MetricsSnapshot {
            timestamp: crate::common::types::current_timestamp_ms(),
            counters: std::collections::HashMap::new(),
            gauges: std::collections::HashMap::new(),
            timings: std::collections::HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_container_creation() {
        let config = AtomiqConfig::default();
        let container = ServiceContainer::new(config).await.unwrap();
        
        assert!(container.storage().get_latest_height().await.is_ok());
    }

    #[tokio::test]
    async fn test_service_builder() {
        let container = ServiceBuilder::new()
            .build()
            .await
            .unwrap();
        
        let state = container.consensus().get_consensus_state().await.unwrap();
        assert_eq!(state.current_height, 0);
    }
}