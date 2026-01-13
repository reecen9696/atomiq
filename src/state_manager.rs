//! State management with validation and optimized operations
//!
//! Extracted state management logic for better separation of concerns

use crate::{
    config::BlockchainConfig,

    Transaction,
};
use hotstuff_rs::types::update_sets::AppStateUpdates;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

/// State change applied by a transaction
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateChange {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

/// Result of executing a transaction with state changes
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub tx_id: u64,
    pub success: bool,
    pub state_changes: Vec<StateChange>,
    pub error_message: Option<String>,
}

/// State manager with validation and optimized operations
pub struct StateManager {
    state: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
    config: StateManagerConfig,
}

/// Configuration for state manager behavior
#[derive(Clone, Debug)]
pub struct StateManagerConfig {
    pub enable_validation: bool,
    pub max_state_size_mb: usize,
    pub enable_snapshots: bool,
    pub validation_mode: ValidationMode,
}

/// Validation modes for state operations
#[derive(Clone, Debug)]
pub enum ValidationMode {
    /// No validation for maximum performance
    None,
    /// Basic validation (nonce, structure)
    Basic,
    /// Full validation including business logic
    Full,
}

impl Default for StateManagerConfig {
    fn default() -> Self {
        Self {
            enable_validation: true,
            max_state_size_mb: 1024, // 1GB
            enable_snapshots: false,
            validation_mode: ValidationMode::Basic,
        }
    }
}

impl StateManager {
    /// Create new state manager with default configuration
    pub fn new() -> Self {
        Self::new_with_config(StateManagerConfig::default())
    }

    /// Create new state manager with custom configuration
    pub fn new_with_config(config: StateManagerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Execute batch of transactions with validation and state updates
    pub fn execute_transactions(
        &self, 
        transactions: &[Transaction]
    ) -> (Vec<ExecutionResult>, AppStateUpdates) {
        match self.config.validation_mode {
            ValidationMode::None => self.execute_fast_path(transactions),
            ValidationMode::Basic | ValidationMode::Full => self.execute_with_validation(transactions),
        }
    }

    /// Fast execution path without state validation
    fn execute_fast_path(&self, transactions: &[Transaction]) -> (Vec<ExecutionResult>, AppStateUpdates) {
        let results = transactions
            .iter()
            .map(|tx| ExecutionResult {
                tx_id: tx.id,
                success: true,
                state_changes: vec![],
                error_message: None,
            })
            .collect();

        (results, AppStateUpdates::new())
    }

    /// Execute with full validation and state tracking
    fn execute_with_validation(&self, transactions: &[Transaction]) -> (Vec<ExecutionResult>, AppStateUpdates) {
        let mut results = Vec::with_capacity(transactions.len());
        let mut app_state_updates = AppStateUpdates::new();
        let mut state = self.state.write().unwrap();

        for tx in transactions {
            let result = self.execute_single_transaction(tx, &mut state, &mut app_state_updates);
            results.push(result);
        }

        (results, app_state_updates)
    }

    /// Execute single transaction with nonce validation
    fn execute_single_transaction(
        &self,
        tx: &Transaction,
        state: &mut HashMap<Vec<u8>, Vec<u8>>,
        app_state_updates: &mut AppStateUpdates,
    ) -> ExecutionResult {
        // Basic transaction structure validation
        if let Err(error) = self.validate_transaction_structure(tx) {
            return ExecutionResult {
                tx_id: tx.id,
                success: false,
                state_changes: vec![],
                error_message: Some(error),
            };
        }

        // Nonce validation (if enabled)
        if matches!(self.config.validation_mode, ValidationMode::Basic | ValidationMode::Full) {
            if let Err(error) = self.validate_transaction_nonce(tx, state) {
                return ExecutionResult {
                    tx_id: tx.id,
                    success: false,
                    state_changes: vec![],
                    error_message: Some(error),
                };
            }
        }

        // Apply state changes
        match self.apply_transaction_state_changes(tx, state) {
            Ok(changes) => {
                // Update app state for HotStuff-rs
                for change in &changes {
                    app_state_updates.insert(change.key.clone(), change.value.clone());
                }

                ExecutionResult {
                    tx_id: tx.id,
                    success: true,
                    state_changes: changes,
                    error_message: None,
                }
            }
            Err(error) => ExecutionResult {
                tx_id: tx.id,
                success: false,
                state_changes: vec![],
                error_message: Some(error),
            },
        }
    }

    /// Validate basic transaction structure
    fn validate_transaction_structure(&self, tx: &Transaction) -> Result<(), String> {
        if tx.data.is_empty() {
            return Err("Transaction data cannot be empty".to_string());
        }

        if tx.data.len() > 1024 * 1024 { // 1MB limit
            return Err(format!("Transaction data too large: {} bytes", tx.data.len()));
        }

        Ok(())
    }

    /// Validate transaction nonce against current state
    fn validate_transaction_nonce(
        &self,
        tx: &Transaction,
        state: &HashMap<Vec<u8>, Vec<u8>>,
    ) -> Result<(), String> {
        let nonce_key = self.build_nonce_key(&tx.sender);
        let current_nonce = self.get_current_nonce(state, &nonce_key);

        if tx.nonce != current_nonce + 1 {
            return Err(format!(
                "Invalid nonce: expected {}, got {}",
                current_nonce + 1,
                tx.nonce
            ));
        }

        Ok(())
    }

    /// Apply state changes for successful transaction
    fn apply_transaction_state_changes(
        &self,
        tx: &Transaction,
        state: &mut HashMap<Vec<u8>, Vec<u8>>,
    ) -> Result<Vec<StateChange>, String> {
        let mut changes = Vec::new();

        // Update nonce
        let nonce_key = self.build_nonce_key(&tx.sender);
        let new_nonce = self.get_current_nonce(state, &nonce_key) + 1;
        let nonce_bytes = new_nonce.to_le_bytes().to_vec();
        state.insert(nonce_key.clone(), nonce_bytes.clone());
        changes.push(StateChange {
            key: nonce_key,
            value: nonce_bytes,
        });

        // Store transaction data
        let tx_key = self.build_transaction_key(tx.id);
        state.insert(tx_key.clone(), tx.data.clone());
        changes.push(StateChange {
            key: tx_key,
            value: tx.data.clone(),
        });

        // Check state size limits
        if self.config.enable_validation {
            self.check_state_size_limits(state)?;
        }

        Ok(changes)
    }

    /// Check if state size is within limits
    fn check_state_size_limits(&self, state: &HashMap<Vec<u8>, Vec<u8>>) -> Result<(), String> {
        let total_size: usize = state
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum();

        let max_size = self.config.max_state_size_mb * 1024 * 1024;
        if total_size > max_size {
            return Err(format!(
                "State size {} bytes exceeds limit {} bytes",
                total_size, max_size
            ));
        }

        Ok(())
    }

    /// Build nonce key for sender
    fn build_nonce_key(&self, sender: &[u8; 32]) -> Vec<u8> {
        let mut key = Vec::with_capacity(38); // "nonce_" + 32 bytes
        key.extend_from_slice(b"nonce_");
        key.extend_from_slice(sender);
        key
    }

    /// Get current nonce for sender
    fn get_current_nonce(&self, state: &HashMap<Vec<u8>, Vec<u8>>, nonce_key: &[u8]) -> u64 {
        state
            .get(nonce_key)
            .and_then(|bytes| {
                if bytes.len() == 8 {
                    let array: Result<[u8; 8], _> = bytes.as_slice().try_into();
                    array.ok().map(u64::from_le_bytes)
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }

    /// Build transaction storage key
    fn build_transaction_key(&self, tx_id: u64) -> Vec<u8> {
        let mut key = Vec::with_capacity(11); // "tx_" + 8 bytes
        key.extend_from_slice(b"tx_");
        key.extend_from_slice(&tx_id.to_le_bytes());
        key
    }

    /// Validate block transactions without state modification
    pub fn validate_block_transactions(&self, transactions: &[Transaction]) -> bool {
        if matches!(self.config.validation_mode, ValidationMode::None) {
            return true;
        }

        let state = self.state.read().unwrap();

        for tx in transactions {
            if !self.validate_single_transaction(tx, &state) {
                return false;
            }
        }

        true
    }

    /// Validate single transaction against current state
    fn validate_single_transaction(&self, tx: &Transaction, state: &HashMap<Vec<u8>, Vec<u8>>) -> bool {
        if self.validate_transaction_structure(tx).is_err() {
            return false;
        }

        if matches!(self.config.validation_mode, ValidationMode::Basic | ValidationMode::Full) {
            if self.validate_transaction_nonce(tx, state).is_err() {
                return false;
            }
        }

        true
    }

    /// Get current state statistics
    pub fn get_state_stats(&self) -> StateStats {
        let state = self.state.read().unwrap();
        let total_size: usize = state.iter().map(|(k, v)| k.len() + v.len()).sum();
        let max_size = self.config.max_state_size_mb * 1024 * 1024;

        StateStats {
            total_entries: state.len() as u64,
            total_size_bytes: total_size as u64,
            size_utilization: (total_size as f64 / max_size as f64) * 100.0,
        }
    }

    /// Clear all state (for testing)
    pub fn clear_state(&self) {
        let mut state = self.state.write().unwrap();
        state.clear();
    }
}

/// Statistics about the state manager
#[derive(Debug, Clone)]
pub struct StateStats {
    pub total_entries: u64,
    pub total_size_bytes: u64,
    pub size_utilization: f64,
}

impl From<BlockchainConfig> for StateManagerConfig {
    fn from(blockchain_config: BlockchainConfig) -> Self {
        let validation_mode = if blockchain_config.enable_state_validation {
            ValidationMode::Basic
        } else {
            ValidationMode::None
        };

        Self {
            enable_validation: blockchain_config.enable_state_validation,
            validation_mode,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_manager_creation() {
        let manager = StateManager::new();
        let stats = manager.get_state_stats();
        assert_eq!(stats.total_entries, 0);
    }

    #[test]
    fn test_fast_path_execution() {
        let config = StateManagerConfig {
            validation_mode: ValidationMode::None,
            ..Default::default()
        };
        let manager = StateManager::new_with_config(config);

        let transactions = vec![
            Transaction {
                id: 1,
                sender: [1; 32],
                data: b"test".to_vec(),
                timestamp: 123,
                nonce: 1,
            }
        ];

        let (results, _) = manager.execute_transactions(&transactions);
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(results[0].state_changes.is_empty());
    }

    #[test]
    fn test_validated_execution() {
        let config = StateManagerConfig {
            validation_mode: ValidationMode::Basic,
            ..Default::default()
        };
        let manager = StateManager::new_with_config(config);

        let transactions = vec![
            Transaction {
                id: 1,
                sender: [1; 32],
                data: b"test".to_vec(),
                timestamp: 123,
                nonce: 1, // First nonce should be 1
            }
        ];

        let (results, _) = manager.execute_transactions(&transactions);
        assert_eq!(results.len(), 1);
        assert!(results[0].success);
        assert!(!results[0].state_changes.is_empty());
    }

    #[test]
    fn test_invalid_nonce() {
        let config = StateManagerConfig {
            validation_mode: ValidationMode::Basic,
            ..Default::default()
        };
        let manager = StateManager::new_with_config(config);

        let transactions = vec![
            Transaction {
                id: 1,
                sender: [1; 32],
                data: b"test".to_vec(),
                timestamp: 123,
                nonce: 5, // Wrong nonce (should be 1)
            }
        ];

        let (results, _) = manager.execute_transactions(&transactions);
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].error_message.is_some());
    }

    #[test]
    fn test_empty_transaction_data() {
        let manager = StateManager::new();

        let transactions = vec![
            Transaction {
                id: 1,
                sender: [1; 32],
                data: Vec::new(), // Empty data
                timestamp: 123,
                nonce: 1,
            }
        ];

        let (results, _) = manager.execute_transactions(&transactions);
        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
        assert!(results[0].error_message.as_ref().unwrap().contains("empty"));
    }
}