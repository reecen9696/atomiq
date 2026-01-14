//! Blockchain finalization notification system
//!
//! This module provides an event-driven system for notifying HTTP handlers and
//! other services when blocks are committed to the blockchain. This ensures that
//! game results and other responses are only returned after blockchain finalization.

use crate::common::types::Transaction;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot};
use dashmap::DashMap;

/// Event emitted when a block is committed to the blockchain
#[derive(Clone, Debug)]
pub struct BlockCommittedEvent {
    /// Block height
    pub height: u64,
    
    /// Block hash
    pub hash: [u8; 32],
    
    /// Transactions included in the block
    pub transactions: Vec<Transaction>,
    
    /// Unix timestamp (milliseconds)
    pub timestamp: u64,
}

impl BlockCommittedEvent {
    /// Create a new block committed event
    pub fn new(height: u64, hash: [u8; 32], transactions: Vec<Transaction>, timestamp: u64) -> Self {
        Self {
            height,
            hash,
            transactions,
            timestamp,
        }
    }
    
    /// Check if a transaction ID is included in this block
    pub fn contains_transaction(&self, tx_id: u64) -> bool {
        self.transactions.iter().any(|tx| tx.id == tx_id)
    }
}

/// Service for waiting on blockchain finalization
///
/// This allows HTTP handlers to await block commits asynchronously, ensuring
/// that responses are only sent after transactions are committed to the blockchain.
#[derive(Clone)]
pub struct FinalizationWaiter {
    /// Channel for receiving block commit events
    event_receiver: broadcast::Sender<BlockCommittedEvent>,
    
    /// Pending transaction waiters (tx_id -> oneshot sender)
    pending_tx_waiters: Arc<DashMap<u64, Vec<oneshot::Sender<BlockCommittedEvent>>>>,
    
    /// Pending height waiters (height -> oneshot senders)
    pending_height_waiters: Arc<DashMap<u64, Vec<oneshot::Sender<BlockCommittedEvent>>>>,
}

impl FinalizationWaiter {
    /// Create a new finalization waiter
    ///
    /// # Arguments
    /// * `event_receiver` - Broadcast channel for receiving block commit events
    pub fn new(event_receiver: broadcast::Sender<BlockCommittedEvent>) -> Self {
        let waiter = Self {
            event_receiver,
            pending_tx_waiters: Arc::new(DashMap::new()),
            pending_height_waiters: Arc::new(DashMap::new()),
        };
        
        // Spawn background task to process events
        waiter.spawn_event_processor();
        
        waiter
    }

    /// Subscribe to the stream of committed-block events.
    ///
    /// This is intended for internal background workers (e.g. fairness persistence)
    /// that need a low-latency signal, while still treating RocksDB as the source of truth.
    pub fn subscribe(&self) -> broadcast::Receiver<BlockCommittedEvent> {
        self.event_receiver.subscribe()
    }
    
    /// Spawn background task to process block commit events
    fn spawn_event_processor(&self) {
        let mut event_rx = self.event_receiver.subscribe();
        let tx_waiters = self.pending_tx_waiters.clone();
        let height_waiters = self.pending_height_waiters.clone();
        
        tokio::spawn(async move {
            tracing::debug!("Finalization event processor started");
            loop {
                let event = match event_rx.recv().await {
                    Ok(event) => event,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        // Under high throughput we may lag; do NOT exit the loop.
                        tracing::warn!("Finalization waiter lagged; skipped {} events", skipped);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                };

                tracing::trace!("Received finalization event for block {} with {} transactions", event.height, event.transactions.len());
                // Process transaction waiters
                let mut completed_tx_ids = Vec::new();
                
                for tx in &event.transactions {
                    if let Some((_, senders)) = tx_waiters.remove(&tx.id) {
                        tracing::debug!("Notifying {} waiters for transaction {}", senders.len(), tx.id);
                        for sender in senders {
                            if sender.send(event.clone()).is_err() {
                                tracing::trace!("Waiter for transaction {} already dropped", tx.id);
                            }
                        }
                        completed_tx_ids.push(tx.id);
                    }
                }
                
                // Process height waiters
                if let Some((_, senders)) = height_waiters.remove(&event.height) {
                    for sender in senders {
                        let _ = sender.send(event.clone());
                    }
                }
                
                // Log completion
                if !completed_tx_ids.is_empty() {
                    tracing::info!(
                        "Finalized {} pending transactions in block {} (height: {})",
                        completed_tx_ids.len(),
                        hex::encode(&event.hash[..8]),
                        event.height
                    );
                } else if !event.transactions.is_empty() {
                    tracing::trace!(
                        "Block {} finalized with {} transactions but no pending waiters",
                        event.height,
                        event.transactions.len()
                    );
                }
            }
        });
    }
    
    /// Wait for a specific transaction to be committed
    ///
    /// Returns the BlockCommittedEvent containing the transaction, or an error on timeout
    ///
    /// # Arguments
    /// * `tx_id` - Transaction ID to wait for
    /// * `timeout` - Maximum time to wait
    pub async fn wait_for_transaction(
        &self,
        tx_id: u64,
        timeout: std::time::Duration,
    ) -> Result<BlockCommittedEvent, FinalizationError> {
        let (tx, rx) = oneshot::channel();
        
        // Register waiter
        tracing::trace!("Registering waiter for transaction {}", tx_id);
        self.pending_tx_waiters
            .entry(tx_id)
            .or_insert_with(Vec::new)
            .push(tx);
        let total_waiters = self.pending_tx_waiters.len();
        tracing::trace!("Total pending transaction waiters: {}", total_waiters);
        
        // Wait with timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(event)) => Ok(event),
            Ok(Err(_)) => Err(FinalizationError::Cancelled),
            Err(_) => {
                // Timeout - clean up waiter
                self.pending_tx_waiters.remove(&tx_id);
                Err(FinalizationError::Timeout {
                    tx_id,
                    timeout_ms: timeout.as_millis() as u64,
                })
            }
        }
    }
    
    /// Wait for a specific block height to be committed
    ///
    /// Returns the BlockCommittedEvent for that height, or an error on timeout
    ///
    /// # Arguments
    /// * `height` - Block height to wait for
    /// * `timeout` - Maximum time to wait
    pub async fn wait_for_height(
        &self,
        height: u64,
        timeout: std::time::Duration,
    ) -> Result<BlockCommittedEvent, FinalizationError> {
        let (tx, rx) = oneshot::channel();
        
        // Register waiter
        self.pending_height_waiters
            .entry(height)
            .or_insert_with(Vec::new)
            .push(tx);
        
        // Wait with timeout
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(event)) => Ok(event),
            Ok(Err(_)) => Err(FinalizationError::Cancelled),
            Err(_) => {
                // Timeout - clean up waiter
                self.pending_height_waiters.remove(&height);
                Err(FinalizationError::Timeout {
                    tx_id: 0,
                    timeout_ms: timeout.as_millis() as u64,
                })
            }
        }
    }
    
    /// Wait for the next block commit (any block)
    ///
    /// This is useful when you don't care about a specific transaction,
    /// just want to wait for the next blockchain update.
    pub async fn wait_for_next_commit(
        &self,
        timeout: std::time::Duration,
    ) -> Result<BlockCommittedEvent, FinalizationError> {
        let mut event_rx = self.event_receiver.subscribe();
        
        match tokio::time::timeout(timeout, event_rx.recv()).await {
            Ok(Ok(event)) => Ok(event),
            Ok(Err(_)) => Err(FinalizationError::Cancelled),
            Err(_) => Err(FinalizationError::Timeout {
                tx_id: 0,
                timeout_ms: timeout.as_millis() as u64,
            }),
        }
    }
    
    /// Get statistics about pending waiters
    pub fn get_stats(&self) -> FinalizationStats {
        FinalizationStats {
            pending_tx_waiters: self.pending_tx_waiters.len(),
            pending_height_waiters: self.pending_height_waiters.len(),
        }
    }
}

/// Statistics about finalization waiters
#[derive(Debug, Clone)]
pub struct FinalizationStats {
    /// Number of pending transaction waiters
    pub pending_tx_waiters: usize,
    
    /// Number of pending height waiters
    pub pending_height_waiters: usize,
}

/// Errors that can occur during finalization waiting
#[derive(Debug, thiserror::Error)]
pub enum FinalizationError {
    /// Transaction not committed within timeout
    #[error("Transaction {tx_id} not finalized within {timeout_ms}ms")]
    Timeout { tx_id: u64, timeout_ms: u64 },
    
    /// Wait was cancelled
    #[error("Finalization wait cancelled")]
    Cancelled,
    
    /// Transaction not found in committed block
    #[error("Transaction {tx_id} not found in committed block {height}")]
    TransactionNotFound { tx_id: u64, height: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_finalization_waiter_creation() {
        let (tx, _rx) = broadcast::channel(100);
        let waiter = FinalizationWaiter::new(tx);
        
        let stats = waiter.get_stats();
        assert_eq!(stats.pending_tx_waiters, 0);
        assert_eq!(stats.pending_height_waiters, 0);
    }
    
    #[tokio::test]
    async fn test_wait_for_transaction() {
        let (tx, _rx) = broadcast::channel(100);
        let waiter = FinalizationWaiter::new(tx.clone());
        
        // Spawn a task to send an event after a short delay
        let event_tx = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            let event = BlockCommittedEvent {
                height: 1,
                hash: [1u8; 32],
                transactions: vec![Transaction {
                    id: 123,
                    sender: [0u8; 32],
                    data: vec![],
                    timestamp: 0,
                    nonce: 1,
                    tx_type: crate::common::types::TransactionType::Standard,
                }],
                timestamp: 1000,
            };
            
            let _ = event_tx.send(event);
        });
        
        // Wait for the transaction
        let result = waiter.wait_for_transaction(123, Duration::from_secs(1)).await;
        assert!(result.is_ok());
        
        let event = result.unwrap();
        assert_eq!(event.height, 1);
        assert!(event.contains_transaction(123));
    }
    
    #[tokio::test]
    async fn test_wait_timeout() {
        let (tx, _rx) = broadcast::channel(100);
        let waiter = FinalizationWaiter::new(tx);
        
        // Wait for transaction that never arrives
        let result = waiter.wait_for_transaction(999, Duration::from_millis(100)).await;
        assert!(result.is_err());
        
        match result {
            Err(FinalizationError::Timeout { tx_id, .. }) => {
                assert_eq!(tx_id, 999);
            }
            _ => panic!("Expected timeout error"),
        }
    }
    
    #[tokio::test]
    async fn test_wait_for_next_commit() {
        let (tx, _rx) = broadcast::channel(100);
        let waiter = FinalizationWaiter::new(tx.clone());
        
        // Spawn a task to send an event
        let event_tx = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            let event = BlockCommittedEvent {
                height: 42,
                hash: [2u8; 32],
                transactions: vec![],
                timestamp: 2000,
            };
            
            let _ = event_tx.send(event);
        });
        
        // Wait for next commit
        let result = waiter.wait_for_next_commit(Duration::from_secs(1)).await;
        assert!(result.is_ok());
        
        let event = result.unwrap();
        assert_eq!(event.height, 42);
    }
}
