//! Configuration management with validation and defaults
//!
//! Centralized configuration system following enterprise patterns

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Comprehensive blockchain configuration with validation
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AtomiqConfig {
    pub blockchain: BlockchainConfig,
    pub consensus: ConsensusConfig,
    pub network: NetworkConfig,
    pub performance: PerformanceConfig,
    pub storage: StorageConfig,
    pub monitoring: MonitoringConfig,
}

impl Default for AtomiqConfig {
    fn default() -> Self {
        Self {
            blockchain: BlockchainConfig::default(),
            consensus: ConsensusConfig::default(),
            network: NetworkConfig::default(),
            performance: PerformanceConfig::default(),
            storage: StorageConfig::default(),
            monitoring: MonitoringConfig::default(),
        }
    }
}

/// Core blockchain configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockchainConfig {
    pub max_transactions_per_block: usize,
    pub max_block_time_ms: u64,
    pub enable_state_validation: bool,
    pub batch_size_threshold: usize,
    pub chain_id: u64,
}

impl Default for BlockchainConfig {
    fn default() -> Self {
        Self {
            max_transactions_per_block: 10_000,
            max_block_time_ms: 10,
            enable_state_validation: true,
            batch_size_threshold: 1_000,
            chain_id: 1,
        }
    }
}

/// Consensus mode selection
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConsensusMode {
    /// Full HotStuff BFT consensus - for multi-validator networks
    /// Slow (~10 TPS) but Byzantine fault tolerant
    FullHotStuff,
    
    /// Direct commit mode - for single trusted validator
    /// Fast (100K+ TPS) with <10ms latency, no consensus overhead
    DirectCommit,
}

/// HotStuff consensus configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsensusConfig {
    pub mode: ConsensusMode,
    pub max_view_time_ms: u64,
    pub epoch_length: usize,
    pub block_sync_request_limit: usize,
    pub block_sync_trigger_min_view_difference: usize,
    pub progress_msg_buffer_capacity: usize,
    /// Block production interval for DirectCommit mode (milliseconds)
    pub direct_commit_interval_ms: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            // PRODUCTION DEFAULT: DirectCommit for single-validator high performance
            // Switch to HotStuff only if you need multi-validator consensus
            mode: ConsensusMode::DirectCommit,
            max_view_time_ms: 2000,
            epoch_length: 100,
            block_sync_request_limit: 100,
            block_sync_trigger_min_view_difference: 2,
            progress_msg_buffer_capacity: 10240,
            direct_commit_interval_ms: 10, // 10ms = 100 blocks/sec potential
        }
    }
}

/// Network configuration for different deployment modes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub mode: NetworkMode,
    pub bind_address: String,
    pub bind_port: u16,
    pub peers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkMode {
    SingleValidator,
    MultiValidator,
    Mock,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: NetworkMode::SingleValidator,
            bind_address: "127.0.0.1".to_string(),
            bind_port: 8080,
            peers: vec![],
        }
    }
}

/// Performance and benchmark configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub target_tps: Option<u64>,
    pub benchmark_duration_seconds: u64,
    pub concurrent_submitters: usize,
    pub batch_size: usize,
    pub warmup_duration_seconds: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            target_tps: None,
            benchmark_duration_seconds: 60,
            concurrent_submitters: 4,
            batch_size: 100,
            warmup_duration_seconds: 5,
        }
    }
}

/// Storage configuration with optimization settings
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StorageConfig {
    pub data_directory: String,
    pub write_buffer_size_mb: usize,
    pub max_write_buffer_number: usize,
    pub target_file_size_mb: usize,
    pub compression_type: CompressionType,
    /// Whether to clear database on startup (testing only!)
    pub clear_on_start: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CompressionType {
    None,
    Snappy,
    Lz4,
    Zstd,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_directory: "./DB/blockchain_data".to_string(),
            write_buffer_size_mb: 128,
            max_write_buffer_number: 4,
            target_file_size_mb: 128,
            compression_type: CompressionType::Lz4,
            clear_on_start: false,  // Production default: preserve data
        }
    }
}

/// Monitoring and metrics configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub enable_metrics: bool,
    pub metrics_interval_seconds: u64,
    pub enable_logging: bool,
    pub log_level: LogLevel,
    pub enable_progress_reporting: bool,
    pub progress_report_interval_seconds: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            metrics_interval_seconds: 1,
            enable_logging: true,
            log_level: LogLevel::Info,
            enable_progress_reporting: true,
            progress_report_interval_seconds: 2,
        }
    }
}

/// Configuration validation and factory methods
impl AtomiqConfig {
    /// Create configuration optimized for high-performance testing
    pub fn high_performance() -> Self {
        Self {
            blockchain: BlockchainConfig {
                max_transactions_per_block: 50_000,
                max_block_time_ms: 5,
                enable_state_validation: false, // Disabled for max throughput
                batch_size_threshold: 5_000,
                chain_id: 1,
            },
            consensus: ConsensusConfig {
                mode: ConsensusMode::DirectCommit,
                max_view_time_ms: 100, // Aggressive timing
                epoch_length: 1000,
                block_sync_request_limit: 1000,
                block_sync_trigger_min_view_difference: 1,
                progress_msg_buffer_capacity: 50000,
                direct_commit_interval_ms: 5, // Ultra-fast for high performance
            },
            performance: PerformanceConfig {
                target_tps: Some(100_000),
                concurrent_submitters: 8,
                batch_size: 1000,
                ..Default::default()
            },
            storage: StorageConfig {
                write_buffer_size_mb: 512,
                max_write_buffer_number: 8,
                target_file_size_mb: 256,
                clear_on_start: true,  // Testing mode: clear DB
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Create configuration optimized for consensus correctness testing
    pub fn consensus_testing() -> Self {
        Self {
            blockchain: BlockchainConfig {
                max_transactions_per_block: 100,
                max_block_time_ms: 1000,
                enable_state_validation: true,
                batch_size_threshold: 50,
                chain_id: 1,
            },
            consensus: ConsensusConfig {
                mode: ConsensusMode::FullHotStuff, // Use full consensus for testing
                max_view_time_ms: 5000, // Conservative for correctness
                epoch_length: 50,
                block_sync_request_limit: 10,
                block_sync_trigger_min_view_difference: 2,
                progress_msg_buffer_capacity: 1024,
                direct_commit_interval_ms: 10,
            },
            performance: PerformanceConfig {
                target_tps: Some(1000),
                concurrent_submitters: 2,
                batch_size: 25,
                benchmark_duration_seconds: 30,
                ..Default::default()
            },
            storage: StorageConfig {
                clear_on_start: true,  // Testing mode: clear DB
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Create configuration for production deployment with persistence
    pub fn production() -> Self {
        Self {
            blockchain: BlockchainConfig {
                max_transactions_per_block: 10_000,
                max_block_time_ms: 100,
                enable_state_validation: true,
                batch_size_threshold: 1_000,
                chain_id: 1,
            },
            consensus: ConsensusConfig {
                mode: ConsensusMode::DirectCommit, // Fast mode for production
                max_view_time_ms: 2000,
                epoch_length: 100,
                block_sync_request_limit: 100,
                block_sync_trigger_min_view_difference: 2,
                progress_msg_buffer_capacity: 10240,
                direct_commit_interval_ms: 10, // 10ms blocks
            },
            storage: StorageConfig {
                data_directory: "./DB/blockchain_data".to_string(),
                write_buffer_size_mb: 256,
                max_write_buffer_number: 6,
                target_file_size_mb: 256,
                compression_type: CompressionType::Lz4,
                clear_on_start: false,  // Production: preserve blockchain data
            },
            monitoring: MonitoringConfig {
                enable_logging: true,
                log_level: LogLevel::Info,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Validate configuration for logical consistency
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate blockchain config
        if self.blockchain.max_transactions_per_block == 0 {
            return Err(ConfigValidationError::InvalidValue(
                "max_transactions_per_block must be > 0".to_string()
            ));
        }

        if self.blockchain.max_block_time_ms == 0 {
            return Err(ConfigValidationError::InvalidValue(
                "max_block_time_ms must be > 0".to_string()
            ));
        }

        // Validate performance config
        if self.performance.concurrent_submitters == 0 {
            return Err(ConfigValidationError::InvalidValue(
                "concurrent_submitters must be > 0".to_string()
            ));
        }

        if self.performance.batch_size == 0 {
            return Err(ConfigValidationError::InvalidValue(
                "batch_size must be > 0".to_string()
            ));
        }

        // Validate consensus timing relationships
        let expected_block_rate = 1000.0 / self.blockchain.max_block_time_ms as f64;
        let view_timeout_rate = 1000.0 / self.consensus.max_view_time_ms as f64;
        
        if view_timeout_rate > expected_block_rate * 2.0 {
            return Err(ConfigValidationError::LogicalInconsistency(
                "View timeout is too aggressive for target block time".to_string()
            ));
        }

        Ok(())
    }

    /// Convert to duration types for internal use
    pub fn max_view_time(&self) -> Duration {
        Duration::from_millis(self.consensus.max_view_time_ms)
    }

    pub fn max_block_time(&self) -> Duration {
        Duration::from_millis(self.blockchain.max_block_time_ms)
    }

    pub fn metrics_interval(&self) -> Duration {
        Duration::from_secs(self.monitoring.metrics_interval_seconds)
    }

    pub fn progress_report_interval(&self) -> Duration {
        Duration::from_secs(self.monitoring.progress_report_interval_seconds)
    }
}

/// Configuration validation errors
#[derive(Debug, Clone)]
pub enum ConfigValidationError {
    InvalidValue(String),
    LogicalInconsistency(String),
    MissingRequired(String),
}

impl std::fmt::Display for ConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigValidationError::InvalidValue(msg) => write!(f, "Invalid configuration value: {}", msg),
            ConfigValidationError::LogicalInconsistency(msg) => write!(f, "Configuration logical inconsistency: {}", msg),
            ConfigValidationError::MissingRequired(msg) => write!(f, "Missing required configuration: {}", msg),
        }
    }
}

impl std::error::Error for ConfigValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = AtomiqConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_high_performance_config_is_valid() {
        let config = AtomiqConfig::high_performance();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_consensus_testing_config_is_valid() {
        let config = AtomiqConfig::consensus_testing();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_invalid_config_validation() {
        let mut config = AtomiqConfig::default();
        config.blockchain.max_transactions_per_block = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_timing_consistency_validation() {
        let mut config = AtomiqConfig::default();
        config.consensus.max_view_time_ms = 1; // Too aggressive
        config.blockchain.max_block_time_ms = 1000;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_duration_conversions() {
        let config = AtomiqConfig::default();
        assert_eq!(config.max_view_time(), Duration::from_millis(2000));
        assert_eq!(config.max_block_time(), Duration::from_millis(10));
    }
}