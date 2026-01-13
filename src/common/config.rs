//! Configuration management for the Atomiq blockchain system
//!
//! This module provides a centralized configuration system with validation,
//! defaults, and environment variable support.

use crate::errors::{AtomiqResult, ConfigurationError};
use crate::common::traits::{AtomiqConfig, NetworkConfig, ConsensusConfig, StorageConfig, ApiConfig, MetricsConfig};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::env;

/// Default configuration builder with sensible defaults
impl Default for AtomiqConfig {
    fn default() -> Self {
        Self {
            network: NetworkConfig::default(),
            consensus: ConsensusConfig::default(),
            storage: StorageConfig::default(),
            api: ApiConfig::default(),
            metrics: MetricsConfig::default(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0".to_string(),
            port: 8545,
            peers: vec![],
            max_connections: 50,
        }
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            algorithm: "hotstuff".to_string(),
            timeout_ms: 1000,
            max_block_size: 1_048_576, // 1MB
            max_transactions_per_block: 1000,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: "./blockchain_data".to_string(),
            cache_size: 134_217_728, // 128MB
            sync_writes: true,
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            listen_address: "0.0.0.0".to_string(),
            port: 8080,
            cors_origins: vec!["*".to_string()],
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval_ms: 5000,
            retention_duration_hours: 24,
        }
    }
}

/// Configuration loader with environment variable support
pub struct ConfigLoader {
    config_path: Option<String>,
}

impl ConfigLoader {
    /// Create a new config loader
    pub fn new() -> Self {
        Self {
            config_path: None,
        }
    }

    /// Set the configuration file path
    pub fn with_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.config_path = Some(path.as_ref().to_string_lossy().to_string());
        self
    }

    /// Load configuration from file and environment variables
    pub fn load(&self) -> AtomiqResult<AtomiqConfig> {
        let mut config = if let Some(ref path) = self.config_path {
            self.load_from_file(path)?
        } else {
            AtomiqConfig::default()
        };

        // Override with environment variables
        self.apply_env_overrides(&mut config)?;

        // Validate the final configuration
        self.validate(&config)?;

        Ok(config)
    }

    /// Load configuration from TOML file
    fn load_from_file(&self, path: &str) -> AtomiqResult<AtomiqConfig> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigurationError::LoadFailed(format!("Failed to read {}: {}", path, e)))?;

        toml::from_str(&content)
            .map_err(|e| ConfigurationError::LoadFailed(format!("Failed to parse TOML: {}", e)).into())
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&self, config: &mut AtomiqConfig) -> AtomiqResult<()> {
        // Network overrides
        if let Ok(addr) = env::var("ATOMIQ_NETWORK_ADDRESS") {
            config.network.listen_address = addr;
        }
        if let Ok(port) = env::var("ATOMIQ_NETWORK_PORT") {
            config.network.port = port.parse()
                .map_err(|_| ConfigurationError::InvalidValue {
                    field: "ATOMIQ_NETWORK_PORT".to_string(),
                    value: port,
                    reason: "Invalid port number".to_string(),
                })?;
        }

        // API overrides
        if let Ok(enabled) = env::var("ATOMIQ_API_ENABLED") {
            config.api.enabled = enabled.parse()
                .map_err(|_| ConfigurationError::InvalidValue {
                    field: "ATOMIQ_API_ENABLED".to_string(),
                    value: enabled,
                    reason: "Invalid boolean value".to_string(),
                })?;
        }
        if let Ok(port) = env::var("ATOMIQ_API_PORT") {
            config.api.port = port.parse()
                .map_err(|_| ConfigurationError::InvalidValue {
                    field: "ATOMIQ_API_PORT".to_string(),
                    value: port,
                    reason: "Invalid port number".to_string(),
                })?;
        }

        // Storage overrides
        if let Ok(data_dir) = env::var("ATOMIQ_DATA_DIR") {
            config.storage.data_dir = data_dir;
        }

        // Consensus overrides
        if let Ok(timeout) = env::var("ATOMIQ_CONSENSUS_TIMEOUT") {
            config.consensus.timeout_ms = timeout.parse()
                .map_err(|_| ConfigurationError::InvalidValue {
                    field: "ATOMIQ_CONSENSUS_TIMEOUT".to_string(),
                    value: timeout,
                    reason: "Invalid timeout value".to_string(),
                })?;
        }

        Ok(())
    }

    /// Validate configuration values
    fn validate(&self, config: &AtomiqConfig) -> AtomiqResult<()> {
        // Validate network configuration
        if config.network.port == 0 {
            return Err(ConfigurationError::InvalidValue {
                field: "network.port".to_string(),
                value: "0".to_string(),
                reason: "Port cannot be zero".to_string(),
            }.into());
        }

        if config.network.max_connections == 0 {
            return Err(ConfigurationError::InvalidValue {
                field: "network.max_connections".to_string(),
                value: "0".to_string(),
                reason: "Max connections cannot be zero".to_string(),
            }.into());
        }

        // Validate API configuration
        if config.api.enabled && config.api.port == 0 {
            return Err(ConfigurationError::InvalidValue {
                field: "api.port".to_string(),
                value: "0".to_string(),
                reason: "API port cannot be zero when API is enabled".to_string(),
            }.into());
        }

        // Validate consensus configuration
        if config.consensus.timeout_ms < 100 {
            return Err(ConfigurationError::InvalidValue {
                field: "consensus.timeout_ms".to_string(),
                value: config.consensus.timeout_ms.to_string(),
                reason: "Timeout must be at least 100ms".to_string(),
            }.into());
        }

        if config.consensus.max_block_size == 0 {
            return Err(ConfigurationError::InvalidValue {
                field: "consensus.max_block_size".to_string(),
                value: "0".to_string(),
                reason: "Max block size cannot be zero".to_string(),
            }.into());
        }

        // Validate storage configuration
        if config.storage.data_dir.is_empty() {
            return Err(ConfigurationError::MissingRequired("storage.data_dir".to_string()).into());
        }

        if config.storage.cache_size == 0 {
            return Err(ConfigurationError::InvalidValue {
                field: "storage.cache_size".to_string(),
                value: "0".to_string(),
                reason: "Cache size cannot be zero".to_string(),
            }.into());
        }

        Ok(())
    }

    /// Save configuration to file
    pub fn save(&self, config: &AtomiqConfig, path: &str) -> AtomiqResult<()> {
        let toml_string = toml::to_string_pretty(config)
            .map_err(|e| ConfigurationError::SaveFailed(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(path, toml_string)
            .map_err(|e| ConfigurationError::SaveFailed(format!("Failed to write to {}: {}", path, e)).into())
    }
}

/// Builder pattern for creating configurations
pub struct ConfigBuilder {
    config: AtomiqConfig,
}

impl ConfigBuilder {
    /// Create a new config builder with defaults
    pub fn new() -> Self {
        Self {
            config: AtomiqConfig::default(),
        }
    }

    /// Set network configuration
    pub fn network(mut self, network: NetworkConfig) -> Self {
        self.config.network = network;
        self
    }

    /// Set consensus configuration
    pub fn consensus(mut self, consensus: ConsensusConfig) -> Self {
        self.config.consensus = consensus;
        self
    }

    /// Set storage configuration
    pub fn storage(mut self, storage: StorageConfig) -> Self {
        self.config.storage = storage;
        self
    }

    /// Set API configuration
    pub fn api(mut self, api: ApiConfig) -> Self {
        self.config.api = api;
        self
    }

    /// Set metrics configuration
    pub fn metrics(mut self, metrics: MetricsConfig) -> Self {
        self.config.metrics = metrics;
        self
    }

    /// Build the final configuration
    pub fn build(self) -> AtomiqConfig {
        self.config
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a sample configuration file
pub fn generate_sample_config(path: &str) -> AtomiqResult<()> {
    let config = AtomiqConfig::default();
    let loader = ConfigLoader::new();
    loader.save(&config, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_config() {
        let config = AtomiqConfig::default();
        assert_eq!(config.network.port, 8545);
        assert_eq!(config.api.port, 8080);
        assert!(config.api.enabled);
    }

    #[test]
    fn test_config_validation() {
        let loader = ConfigLoader::new();
        let mut config = AtomiqConfig::default();
        
        // Valid config should pass
        assert!(loader.validate(&config).is_ok());
        
        // Invalid port should fail
        config.network.port = 0;
        assert!(loader.validate(&config).is_err());
    }

    #[test]
    fn test_config_builder() {
        let config = ConfigBuilder::new()
            .network(NetworkConfig {
                listen_address: "127.0.0.1".to_string(),
                port: 9000,
                peers: vec!["peer1".to_string()],
                max_connections: 100,
            })
            .build();

        assert_eq!(config.network.listen_address, "127.0.0.1");
        assert_eq!(config.network.port, 9000);
        assert_eq!(config.network.peers.len(), 1);
    }

    #[test]
    fn test_save_and_load_config() -> AtomiqResult<()> {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap();

        let original_config = AtomiqConfig::default();
        
        // Save config
        let loader = ConfigLoader::new();
        loader.save(&original_config, path)?;

        // Load config
        let loaded_config = ConfigLoader::new().with_path(path).load()?;

        // Compare key fields
        assert_eq!(loaded_config.network.port, original_config.network.port);
        assert_eq!(loaded_config.api.enabled, original_config.api.enabled);

        Ok(())
    }
}