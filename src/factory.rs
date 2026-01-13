//! Factory patterns for blockchain initialization and configuration
//!
//! Centralizes the complex initialization logic that's duplicated across binaries

use crate::{
    config::{AtomiqConfig, NetworkMode},
    errors::{AtomiqError, AtomiqResult, BlockchainError, StorageError},
    storage::OptimizedStorage,
    AtomiqApp,
};
use hotstuff_rs::{
    replica::{Configuration, Replica, ReplicaSpec},
    types::{
        crypto_primitives::{SigningKey, VerifyingKey},
        data_types::{ChainID, BufferSize, EpochLength, Power},
        update_sets::{AppStateUpdates, ValidatorSetUpdates},
        validator_set::{ValidatorSet, ValidatorSetState},
    },
    networking::network::Network,
};
use std::{
    sync::{mpsc, Arc, Mutex},
    fs,
};
use rand_core::OsRng;

/// Network wrapper enum to handle different network implementations
#[derive(Clone)]
pub enum NetworkWrapper {
    SingleValidator(SingleValidatorNetwork),
    Mock(crate::network::MockNetwork),
}

impl hotstuff_rs::networking::network::Network for NetworkWrapper {
    fn init_validator_set(&mut self, validator_set: hotstuff_rs::types::validator_set::ValidatorSet) {
        match self {
            NetworkWrapper::SingleValidator(n) => n.init_validator_set(validator_set),
            NetworkWrapper::Mock(n) => n.init_validator_set(validator_set),
        }
    }

    fn update_validator_set(&mut self, updates: hotstuff_rs::types::update_sets::ValidatorSetUpdates) {
        match self {
            NetworkWrapper::SingleValidator(n) => n.update_validator_set(updates),
            NetworkWrapper::Mock(n) => n.update_validator_set(updates),
        }
    }

    fn send(&mut self, peer: hotstuff_rs::types::crypto_primitives::VerifyingKey, message: hotstuff_rs::networking::messages::Message) {
        match self {
            NetworkWrapper::SingleValidator(n) => n.send(peer, message),
            NetworkWrapper::Mock(n) => n.send(peer, message),
        }
    }

    fn broadcast(&mut self, message: hotstuff_rs::networking::messages::Message) {
        match self {
            NetworkWrapper::SingleValidator(n) => n.broadcast(message),
            NetworkWrapper::Mock(n) => n.broadcast(message),
        }
    }

    fn recv(&mut self) -> Option<(hotstuff_rs::types::crypto_primitives::VerifyingKey, hotstuff_rs::networking::messages::Message)> {
        match self {
            NetworkWrapper::SingleValidator(n) => n.recv(),
            NetworkWrapper::Mock(n) => n.recv(),
        }
    }
}

/// Factory for creating blockchain instances with different configurations  
pub struct BlockchainFactory;

impl BlockchainFactory {
    /// Create a fully configured blockchain instance
    pub async fn create_blockchain(
        config: AtomiqConfig,
    ) -> AtomiqResult<(Arc<AtomiqApp>, Box<dyn BlockchainHandle>)> {
        // Validate configuration
        config.validate().map_err(|e| AtomiqError::Configuration(
            crate::errors::ConfigurationError::ValidationFailed(e.to_string())
        ))?;

        // Clean up old data if requested
        if config.storage.data_directory != ":memory:" {
            let _ = fs::remove_dir_all(&config.storage.data_directory);
        }

        // Create components
        let (signing_key, verifying_key) = Self::create_keypair();
        let app = Arc::new(AtomiqApp::new(config.blockchain.clone()));
        let storage = Self::create_storage(&config)?;
        let network = Self::create_network(&config, verifying_key)?;

        // Initialize validator set
        let validator_set_state = Self::create_validator_set(verifying_key, &config)?;

        // Initialize replica
        Replica::initialize(storage.clone(), AppStateUpdates::new(), validator_set_state);

        // Create replica configuration
        let replica_config = Self::create_replica_configuration(signing_key, &config);

        // Start the appropriate blockchain type
        let handle = match config.network.mode {
            NetworkMode::SingleValidator => {
                let replica = ReplicaSpec::builder()
                    .app((*app).clone())
                    .network(network)
                    .kv_store(storage)
                    .configuration(replica_config)
                    .build()
                    .start();

                Box::new(SingleValidatorHandle { replica }) as Box<dyn BlockchainHandle>
            }
            NetworkMode::MultiValidator => {
                return Err(AtomiqError::Blockchain(BlockchainError::InitializationFailed(
                    "Multi-validator mode not yet implemented".to_string()
                )));
            }
            NetworkMode::Mock => {
                Box::new(MockHandle { app: app.clone() }) as Box<dyn BlockchainHandle>
            }
        };

        // Brief initialization delay
        tokio::time::sleep(config.max_view_time() / 10).await;

        Ok((app, handle))
    }

    /// Create a keypair for the validator
    fn create_keypair() -> (SigningKey, VerifyingKey) {
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    /// Create optimized storage instance
    fn create_storage(config: &AtomiqConfig) -> AtomiqResult<OptimizedStorage> {
        OptimizedStorage::new_with_config(&config.storage)
            .map_err(|e| AtomiqError::Storage(StorageError::DatabaseOpenFailed(e.to_string())))
    }

    /// Create network based on configuration
    fn create_network(
        config: &AtomiqConfig,
        verifying_key: VerifyingKey,
    ) -> AtomiqResult<NetworkWrapper> {
        let network = match config.network.mode {
            NetworkMode::SingleValidator => {
                NetworkWrapper::SingleValidator(SingleValidatorNetwork::new(verifying_key))
            }
            NetworkMode::Mock => {
                NetworkWrapper::Mock(crate::network::MockNetwork::new(verifying_key))
            }
            NetworkMode::MultiValidator => {
                return Err(AtomiqError::Blockchain(BlockchainError::InitializationFailed(
                    "Multi-validator network not yet implemented".to_string()
                )));
            }
        };

        Ok(network)
    }

    /// Create validator set for the configuration
    fn create_validator_set(
        verifying_key: VerifyingKey,
        _config: &AtomiqConfig,
    ) -> AtomiqResult<ValidatorSetState> {
        let mut validator_set_updates = ValidatorSetUpdates::new();
        validator_set_updates.insert(verifying_key, Power::new(1));

        let mut initial_validator_set = ValidatorSet::new();
        initial_validator_set.apply_updates(&validator_set_updates);

        Ok(ValidatorSetState::new(
            initial_validator_set.clone(),
            initial_validator_set,
            None,
            true, // Mark as decided for single validator
        ))
    }

    /// Create replica configuration from atomiq config
    fn create_replica_configuration(
        signing_key: SigningKey,
        config: &AtomiqConfig,
    ) -> Configuration {
        Configuration::builder()
            .me(signing_key)
            .chain_id(ChainID::new(config.blockchain.chain_id))
            .block_sync_request_limit(config.consensus.block_sync_request_limit.try_into().unwrap_or(100))
            .block_sync_server_advertise_time(config.max_view_time())
            .block_sync_response_timeout(config.max_view_time() / 2)
            .block_sync_blacklist_expiry_time(config.max_view_time() * 5)
            .block_sync_trigger_min_view_difference(config.consensus.block_sync_trigger_min_view_difference.try_into().unwrap_or(2))
            .block_sync_trigger_timeout(config.max_view_time() * 10)
            .progress_msg_buffer_capacity(BufferSize::new(config.consensus.progress_msg_buffer_capacity.try_into().unwrap_or(10240)))
            .epoch_length(EpochLength::new(config.consensus.epoch_length.try_into().unwrap_or(100)))
            .max_view_time(config.max_view_time())
            .log_events(config.monitoring.enable_logging)
            .build()
    }
}

/// Trait representing a running blockchain instance
pub trait BlockchainHandle: Send + Sync {
    /// Get a textual description of the blockchain type
    fn blockchain_type(&self) -> &'static str;
    
    /// Gracefully shutdown the blockchain
    fn shutdown(&mut self) -> AtomiqResult<()>;
}

/// Handle for single validator blockchain
pub struct SingleValidatorHandle {
    replica: Replica<OptimizedStorage>,
}

impl BlockchainHandle for SingleValidatorHandle {
    fn blockchain_type(&self) -> &'static str {
        "SingleValidator"
    }

    fn shutdown(&mut self) -> AtomiqResult<()> {
        // HotStuff replica handles shutdown automatically
        Ok(())
    }
}

/// Handle for mock blockchain (no consensus)
pub struct MockHandle {
    app: Arc<AtomiqApp>,
}

impl BlockchainHandle for MockHandle {
    fn blockchain_type(&self) -> &'static str {
        "Mock"
    }

    fn shutdown(&mut self) -> AtomiqResult<()> {
        Ok(())
    }
}

/// Single validator network implementation (extracted from duplicated code)
#[derive(Clone)]
pub struct SingleValidatorNetwork {
    my_verifying_key: VerifyingKey,
    sender: mpsc::Sender<(VerifyingKey, hotstuff_rs::networking::messages::Message)>,
    receiver: Arc<Mutex<mpsc::Receiver<(VerifyingKey, hotstuff_rs::networking::messages::Message)>>>,
}

impl SingleValidatorNetwork {
    pub fn new(validator_key: VerifyingKey) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            my_verifying_key: validator_key,
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
        }
    }
}

impl Network for SingleValidatorNetwork {
    fn init_validator_set(&mut self, _: ValidatorSet) {
        // No-op for single validator
    }

    fn update_validator_set(&mut self, _: ValidatorSetUpdates) {
        // No-op for single validator  
    }

    fn send(&mut self, _peer: VerifyingKey, message: hotstuff_rs::networking::messages::Message) {
        // For single validator, send to self
        let _ = self.sender.send((self.my_verifying_key, message));
    }

    fn broadcast(&mut self, message: hotstuff_rs::networking::messages::Message) {
        // For single validator, broadcast to self
        let _ = self.sender.send((self.my_verifying_key, message));
    }

    fn recv(&mut self) -> Option<(VerifyingKey, hotstuff_rs::networking::messages::Message)> {
        if let Ok(receiver) = self.receiver.lock() {
            match receiver.try_recv() {
                Ok(message) => Some(message),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => None,
            }
        } else {
            None
        }
    }
}

/// Configuration-specific factory methods
impl BlockchainFactory {
    /// Create blockchain optimized for high-performance testing
    pub async fn create_high_performance() -> AtomiqResult<(Arc<AtomiqApp>, Box<dyn BlockchainHandle>)> {
        Self::create_blockchain(AtomiqConfig::high_performance()).await
    }

    /// Create blockchain optimized for consensus testing
    pub async fn create_consensus_testing() -> AtomiqResult<(Arc<AtomiqApp>, Box<dyn BlockchainHandle>)> {
        Self::create_blockchain(AtomiqConfig::consensus_testing()).await
    }

    /// Create mock blockchain for unit testing
    pub async fn create_mock() -> AtomiqResult<(Arc<AtomiqApp>, Box<dyn BlockchainHandle>)> {
        let mut config = AtomiqConfig::default();
        config.network.mode = NetworkMode::Mock;
        Self::create_blockchain(config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_mock_blockchain() {
        let result = BlockchainFactory::create_mock().await;
        assert!(result.is_ok());
        
        let (_app, handle) = result.unwrap();
        assert_eq!(handle.blockchain_type(), "Mock");
    }

    #[tokio::test]
    async fn test_create_high_performance_blockchain() {
        let result = BlockchainFactory::create_high_performance().await;
        assert!(result.is_ok());
        
        let (_app, handle) = result.unwrap();
        assert_eq!(handle.blockchain_type(), "SingleValidator");
    }

    #[tokio::test] 
    async fn test_create_consensus_testing_blockchain() {
        let result = BlockchainFactory::create_consensus_testing().await;
        assert!(result.is_ok());
        
        let (_app, handle) = result.unwrap();
        assert_eq!(handle.blockchain_type(), "SingleValidator");
    }

    #[test]
    fn test_keypair_generation() {
        let (signing_key, verifying_key) = BlockchainFactory::create_keypair();
        assert_eq!(signing_key.verifying_key(), verifying_key);
    }

    #[test]
    fn test_single_validator_network() {
        let key = SigningKey::generate(&mut OsRng).verifying_key();
        let mut network = SingleValidatorNetwork::new(key);
        
        // Should start empty
        assert!(network.recv().is_none());
    }
}