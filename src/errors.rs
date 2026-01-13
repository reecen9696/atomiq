//! Comprehensive error types for the Atomiq blockchain system
//!
//! Enterprise-grade error handling with proper context and error chains

use std::fmt;
use std::error::Error as StdError;

/// Root error type for all Atomiq operations
#[derive(Debug)]
pub enum AtomiqError {
    /// Configuration related errors
    Configuration(ConfigurationError),
    
    /// Blockchain operation errors
    Blockchain(BlockchainError),
    
    /// Network communication errors
    Network(NetworkError),
    
    /// Storage system errors
    Storage(StorageError),
    
    /// Consensus mechanism errors
    Consensus(ConsensusError),
    
    /// Performance monitoring errors
    Monitoring(MonitoringError),
    
    /// Transaction processing errors
    Transaction(TransactionError),
}

/// Configuration and validation errors
#[derive(Debug)]
pub enum ConfigurationError {
    ValidationFailed(String),
    MissingRequired(String),
    InvalidValue { field: String, value: String, reason: String },
    LoadFailed(String),
    SaveFailed(String),
}

/// Blockchain operation errors
#[derive(Debug)]
pub enum BlockchainError {
    InitializationFailed(String),
    StateCorrupted(String),
    BlockProductionFailed(String),
    BlockValidationFailed(String),
    ChainSyncFailed(String),
    InvalidGenesis(String),
}

/// Network communication errors
#[derive(Debug)]
pub enum NetworkError {
    ConnectionFailed(String),
    PeerUnreachable(String),
    MessageSerializationFailed(String),
    MessageDeserializationFailed(String),
    BroadcastFailed(String),
    NetworkPartition,
}

/// Storage system errors
#[derive(Debug)]
pub enum StorageError {
    DatabaseOpenFailed(String),
    ReadFailed(String),
    WriteFailed(String),
    CorruptedData(String),
    InsufficientSpace,
    PermissionDenied(String),
}

/// Consensus mechanism errors
#[derive(Debug)]
pub enum ConsensusError {
    ViewTimeout,
    LeaderElectionFailed,
    QuorumNotReached,
    InvalidProposal(String),
    ConsensusStalled,
    EpochTransitionFailed(String),
}

/// Performance monitoring errors
#[derive(Debug)]
pub enum MonitoringError {
    MetricsCollectionFailed(String),
    ReportingFailed(String),
    InvalidMetrics(String),
}

/// Transaction processing errors
#[derive(Debug)]
pub enum TransactionError {
    InvalidSignature,
    InsufficientFunds,
    NonceError { expected: u64, actual: u64 },
    DataTooLarge { size: usize, max_size: usize },
    ExecutionFailed(String),
    PoolFull,
    DuplicateTransaction(u64),
}

// Display implementations
impl fmt::Display for AtomiqError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AtomiqError::Configuration(e) => write!(f, "Configuration error: {}", e),
            AtomiqError::Blockchain(e) => write!(f, "Blockchain error: {}", e),
            AtomiqError::Network(e) => write!(f, "Network error: {}", e),
            AtomiqError::Storage(e) => write!(f, "Storage error: {}", e),
            AtomiqError::Consensus(e) => write!(f, "Consensus error: {}", e),
            AtomiqError::Monitoring(e) => write!(f, "Monitoring error: {}", e),
            AtomiqError::Transaction(e) => write!(f, "Transaction error: {}", e),
        }
    }
}

impl fmt::Display for ConfigurationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigurationError::ValidationFailed(msg) => write!(f, "Validation failed: {}", msg),
            ConfigurationError::MissingRequired(field) => write!(f, "Missing required field: {}", field),
            ConfigurationError::InvalidValue { field, value, reason } => {
                write!(f, "Invalid value for {}: '{}' ({})", field, value, reason)
            }
            ConfigurationError::LoadFailed(msg) => write!(f, "Failed to load configuration: {}", msg),
            ConfigurationError::SaveFailed(msg) => write!(f, "Failed to save configuration: {}", msg),
        }
    }
}

impl fmt::Display for BlockchainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlockchainError::InitializationFailed(msg) => write!(f, "Initialization failed: {}", msg),
            BlockchainError::StateCorrupted(msg) => write!(f, "State corrupted: {}", msg),
            BlockchainError::BlockProductionFailed(msg) => write!(f, "Block production failed: {}", msg),
            BlockchainError::BlockValidationFailed(msg) => write!(f, "Block validation failed: {}", msg),
            BlockchainError::ChainSyncFailed(msg) => write!(f, "Chain sync failed: {}", msg),
            BlockchainError::InvalidGenesis(msg) => write!(f, "Invalid genesis: {}", msg),
        }
    }
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            NetworkError::PeerUnreachable(peer) => write!(f, "Peer unreachable: {}", peer),
            NetworkError::MessageSerializationFailed(msg) => write!(f, "Message serialization failed: {}", msg),
            NetworkError::MessageDeserializationFailed(msg) => write!(f, "Message deserialization failed: {}", msg),
            NetworkError::BroadcastFailed(msg) => write!(f, "Broadcast failed: {}", msg),
            NetworkError::NetworkPartition => write!(f, "Network partition detected"),
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StorageError::DatabaseOpenFailed(msg) => write!(f, "Database open failed: {}", msg),
            StorageError::ReadFailed(msg) => write!(f, "Read failed: {}", msg),
            StorageError::WriteFailed(msg) => write!(f, "Write failed: {}", msg),
            StorageError::CorruptedData(msg) => write!(f, "Corrupted data: {}", msg),
            StorageError::InsufficientSpace => write!(f, "Insufficient storage space"),
            StorageError::PermissionDenied(path) => write!(f, "Permission denied: {}", path),
        }
    }
}

impl fmt::Display for ConsensusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConsensusError::ViewTimeout => write!(f, "View timeout"),
            ConsensusError::LeaderElectionFailed => write!(f, "Leader election failed"),
            ConsensusError::QuorumNotReached => write!(f, "Quorum not reached"),
            ConsensusError::InvalidProposal(msg) => write!(f, "Invalid proposal: {}", msg),
            ConsensusError::ConsensusStalled => write!(f, "Consensus stalled"),
            ConsensusError::EpochTransitionFailed(msg) => write!(f, "Epoch transition failed: {}", msg),
        }
    }
}

impl fmt::Display for MonitoringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MonitoringError::MetricsCollectionFailed(msg) => write!(f, "Metrics collection failed: {}", msg),
            MonitoringError::ReportingFailed(msg) => write!(f, "Reporting failed: {}", msg),
            MonitoringError::InvalidMetrics(msg) => write!(f, "Invalid metrics: {}", msg),
        }
    }
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransactionError::InvalidSignature => write!(f, "Invalid signature"),
            TransactionError::InsufficientFunds => write!(f, "Insufficient funds"),
            TransactionError::NonceError { expected, actual } => {
                write!(f, "Nonce error: expected {}, got {}", expected, actual)
            }
            TransactionError::DataTooLarge { size, max_size } => {
                write!(f, "Data too large: {} bytes (max {})", size, max_size)
            }
            TransactionError::ExecutionFailed(msg) => write!(f, "Execution failed: {}", msg),
            TransactionError::PoolFull => write!(f, "Transaction pool full"),
            TransactionError::DuplicateTransaction(id) => write!(f, "Duplicate transaction: {}", id),
        }
    }
}

// Standard Error trait implementations
impl std::error::Error for AtomiqError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AtomiqError::Configuration(e) => Some(e),
            AtomiqError::Blockchain(e) => Some(e),
            AtomiqError::Network(e) => Some(e),
            AtomiqError::Storage(e) => Some(e),
            AtomiqError::Consensus(e) => Some(e),
            AtomiqError::Monitoring(e) => Some(e),
            AtomiqError::Transaction(e) => Some(e),
        }
    }
}

impl std::error::Error for ConfigurationError {}
impl std::error::Error for BlockchainError {}
impl std::error::Error for NetworkError {}
impl std::error::Error for StorageError {}
impl std::error::Error for ConsensusError {}
impl std::error::Error for MonitoringError {}
impl std::error::Error for TransactionError {}

// From implementations for easy conversion
impl From<ConfigurationError> for AtomiqError {
    fn from(e: ConfigurationError) -> Self {
        AtomiqError::Configuration(e)
    }
}

impl From<BlockchainError> for AtomiqError {
    fn from(e: BlockchainError) -> Self {
        AtomiqError::Blockchain(e)
    }
}

impl From<NetworkError> for AtomiqError {
    fn from(e: NetworkError) -> Self {
        AtomiqError::Network(e)
    }
}

impl From<StorageError> for AtomiqError {
    fn from(e: StorageError) -> Self {
        AtomiqError::Storage(e)
    }
}

impl From<ConsensusError> for AtomiqError {
    fn from(e: ConsensusError) -> Self {
        AtomiqError::Consensus(e)
    }
}

impl From<MonitoringError> for AtomiqError {
    fn from(e: MonitoringError) -> Self {
        AtomiqError::Monitoring(e)
    }
}

impl From<TransactionError> for AtomiqError {
    fn from(e: TransactionError) -> Self {
        AtomiqError::Transaction(e)
    }
}

// External error conversions
impl From<rocksdb::Error> for AtomiqError {
    fn from(e: rocksdb::Error) -> Self {
        AtomiqError::Storage(StorageError::WriteFailed(e.to_string()))
    }
}

impl From<std::io::Error> for AtomiqError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::PermissionDenied => {
                AtomiqError::Storage(StorageError::PermissionDenied(e.to_string()))
            }
            std::io::ErrorKind::NotFound => {
                AtomiqError::Storage(StorageError::ReadFailed(e.to_string()))
            }
            _ => AtomiqError::Storage(StorageError::ReadFailed(e.to_string())),
        }
    }
}

impl From<serde_json::Error> for AtomiqError {
    fn from(e: serde_json::Error) -> Self {
        AtomiqError::Configuration(ConfigurationError::LoadFailed(e.to_string()))
    }
}

// Convenience type alias for Results
pub type AtomiqResult<T> = Result<T, AtomiqError>;

/// Macro for creating context-aware errors
#[macro_export]
macro_rules! atomiq_error {
    ($variant:expr, $msg:expr) => {
        AtomiqError::from($variant($msg.to_string()))
    };
    ($variant:expr, $fmt:expr, $($args:tt)*) => {
        AtomiqError::from($variant(format!($fmt, $($args)*)))
    };
}

/// Macro for creating contextual errors with file/line information
#[macro_export]
macro_rules! context_error {
    ($variant:expr, $msg:expr) => {
        AtomiqError::from($variant(format!("{} at {}:{}", $msg, file!(), line!())))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let config_error = ConfigurationError::ValidationFailed("test".to_string());
        let atomiq_error = AtomiqError::Configuration(config_error);
        
        assert!(atomiq_error.to_string().contains("Configuration error"));
        assert!(atomiq_error.to_string().contains("test"));
    }

    #[test]
    fn test_transaction_error_details() {
        let tx_error = TransactionError::NonceError {
            expected: 5,
            actual: 3,
        };
        
        assert!(tx_error.to_string().contains("expected 5"));
        assert!(tx_error.to_string().contains("got 3"));
    }

    #[test]
    fn test_error_conversion() {
        let config_error = ConfigurationError::ValidationFailed("test".to_string());
        let atomiq_error: AtomiqError = config_error.into();
        
        match atomiq_error {
            AtomiqError::Configuration(_) => {},
            _ => panic!("Expected configuration error"),
        }
    }

    #[test]
    fn test_error_source() {
        let config_error = ConfigurationError::ValidationFailed("test".to_string());
        let atomiq_error = AtomiqError::Configuration(config_error);
        
        assert!(atomiq_error.source().is_some());
    }
}