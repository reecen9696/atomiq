//! Fairness pipeline: ensures game fairness records are persisted asynchronously after block commit.
//!
//! Goals:
//! - Keep block commit path fast (no VRF work in consensus/commit hot path).
//! - Ensure API can wait for both (a) tx committed and (b) fairness record persisted.
//! - Ensure fairness records are eventually persisted even if no HTTP request triggers them.

use crate::{
    blockchain_game_processor::{BlockchainGameProcessor, GameBetData},
    common::types::{Transaction as CommonTransaction, TransactionType as CommonTransactionType},
    finalization::FinalizationWaiter,
    game_store,
    storage::OptimizedStorage,
    Block, Transaction,
};
use hotstuff_rs::block_tree::pluggables::KVGet;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{broadcast, oneshot, Semaphore};

const FAIRNESS_CURSOR_KEY: &[u8] = b"fairness:last_processed_height";

fn parse_u64_le(bytes: &[u8]) -> Option<u64> {
    let arr: [u8; 8] = bytes.try_into().ok()?;
    Some(u64::from_le_bytes(arr))
}

fn to_u64_le(value: u64) -> [u8; 8] {
    value.to_le_bytes()
}

fn load_latest_height(storage: &OptimizedStorage) -> u64 {
    storage
        .get(b"latest_height")
        .and_then(|b| parse_u64_le(&b))
        .unwrap_or(0)
}

fn load_cursor(storage: &OptimizedStorage) -> u64 {
    storage
        .get(FAIRNESS_CURSOR_KEY)
        .and_then(|b| parse_u64_le(&b))
        .unwrap_or(0)
}

fn store_cursor(storage: &OptimizedStorage, height: u64) {
    let _ = storage.put(FAIRNESS_CURSOR_KEY, &to_u64_le(height));
}

fn load_block(storage: &OptimizedStorage, height: u64) -> Option<Block> {
    let key = format!("block:height:{}", height);
    let bytes = storage.get(key.as_bytes())?;
    bincode::deserialize::<Block>(&bytes).ok()
}

fn is_game_bet_payload(data: &[u8]) -> bool {
    serde_json::from_slice::<GameBetData>(data).is_ok()
}

fn to_common_game_bet_tx(tx: &Transaction) -> Option<CommonTransaction> {
    if !is_game_bet_payload(&tx.data) {
        return None;
    }

    Some(CommonTransaction {
        id: tx.id,
        sender: tx.sender,
        data: tx.data.clone(),
        timestamp: tx.timestamp,
        nonce: tx.nonce,
        tx_type: CommonTransactionType::GameBet,
    })
}

/// Emitted after a fairness record has been persisted for a transaction.
#[derive(Clone, Debug)]
pub struct FairnessPersistedEvent {
    pub tx_id: u64,
    pub block_height: u64,
    pub block_hash: [u8; 32],
}

#[derive(Debug, thiserror::Error)]
pub enum FairnessError {
    #[error("Fairness record for tx {tx_id} not available within {timeout_ms}ms")]
    Timeout { tx_id: u64, timeout_ms: u64 },

    #[error("Fairness waiter cancelled")]
    Cancelled,

    #[error("Fairness record not found after notification for tx {tx_id}")]
    NotFound { tx_id: u64 },

    #[error("Fairness record does not match expected inclusion for tx {tx_id}")]
    InclusionMismatch { tx_id: u64 },
}

/// Waiter for fairness persistence.
///
/// This is intentionally DB-backed: notifications are a wake-up mechanism,
/// but the canonical source of truth is the RocksDB record.
#[derive(Clone)]
pub struct FairnessWaiter {
    storage: Arc<OptimizedStorage>,
    event_publisher: broadcast::Sender<FairnessPersistedEvent>,
    pending: Arc<dashmap::DashMap<u64, Vec<oneshot::Sender<FairnessPersistedEvent>>>>,
}

impl FairnessWaiter {
    pub fn new(storage: Arc<OptimizedStorage>) -> Self {
        let (event_publisher, _) = broadcast::channel(10_000);
        let waiter = Self {
            storage,
            event_publisher,
            pending: Arc::new(dashmap::DashMap::new()),
        };
        waiter.spawn_event_processor();
        waiter
    }

    pub fn publisher(&self) -> broadcast::Sender<FairnessPersistedEvent> {
        self.event_publisher.clone()
    }

    fn spawn_event_processor(&self) {
        let mut rx = self.event_publisher.subscribe();
        let pending = self.pending.clone();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Some((_, senders)) = pending.remove(&event.tx_id) {
                            for sender in senders {
                                let _ = sender.send(event.clone());
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!("Fairness waiter lagged; skipped {} events", skipped);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub async fn wait_for_game_result(
        &self,
        tx_id: u64,
        expected_block_height: u64,
        expected_block_hash: [u8; 32],
        timeout: Duration,
    ) -> Result<crate::blockchain_game_processor::BlockchainGameResult, FairnessError> {
        if let Ok(Some(existing)) = game_store::load_game_result(self.storage.as_ref(), tx_id) {
            if existing.block_height == expected_block_height && existing.block_hash == expected_block_hash {
                return Ok(existing);
            }
            return Err(FairnessError::InclusionMismatch { tx_id });
        }

        let (tx, rx) = oneshot::channel();
        self.pending.entry(tx_id).or_insert_with(Vec::new).push(tx);

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(_event)) => {
                let record = game_store::load_game_result(self.storage.as_ref(), tx_id)
                    .ok()
                    .flatten()
                    .ok_or(FairnessError::NotFound { tx_id })?;

                if record.block_height != expected_block_height || record.block_hash != expected_block_hash {
                    return Err(FairnessError::InclusionMismatch { tx_id });
                }

                Ok(record)
            }
            Ok(Err(_)) => Err(FairnessError::Cancelled),
            Err(_) => {
                self.pending.remove(&tx_id);
                Err(FairnessError::Timeout {
                    tx_id,
                    timeout_ms: timeout.as_millis() as u64,
                })
            }
        }
    }
}

/// Background worker that ensures fairness records exist for committed game bets.
///
/// Runs off the block-commit hot path. Uses a durable cursor in RocksDB so it can
/// backfill on restart.
pub struct FairnessWorker {
    storage: Arc<OptimizedStorage>,
    processor: Arc<BlockchainGameProcessor>,
    finalization_waiter: Arc<FinalizationWaiter>,
    fairness_publisher: broadcast::Sender<FairnessPersistedEvent>,
    max_concurrency: usize,
    running: Arc<AtomicBool>,
}

impl FairnessWorker {
    pub fn spawn(
        storage: Arc<OptimizedStorage>,
        processor: Arc<BlockchainGameProcessor>,
        finalization_waiter: Arc<FinalizationWaiter>,
        fairness_publisher: broadcast::Sender<FairnessPersistedEvent>,
        max_concurrency: usize,
    ) -> Arc<Self> {
        let worker = Arc::new(Self {
            storage,
            processor,
            finalization_waiter,
            fairness_publisher,
            max_concurrency: max_concurrency.max(1),
            running: Arc::new(AtomicBool::new(true)),
        });

        worker.clone().spawn_task();
        worker
    }

    fn spawn_task(self: Arc<Self>) {
        tokio::spawn(async move {
            // Initial backfill.
            if let Err(e) = self.catch_up_once().await {
                tracing::warn!("FairnessWorker initial catch-up failed: {}", e);
            }

            let mut rx = self.finalization_waiter.subscribe();
            let mut tick = tokio::time::interval(Duration::from_millis(100));

            while self.running.load(Ordering::SeqCst) {
                tokio::select! {
                    biased;
                    _ = tick.tick() => {
                        let _ = self.catch_up_once().await;
                    }
                    recv = rx.recv() => {
                        match recv {
                            Ok(event) => {
                                // Process by height (DB is truth).
                                let _ = self.process_height(event.height).await;
                            }
                            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                                tracing::warn!("FairnessWorker lagged; skipped {} finalization events", skipped);
                                // Fall back to scanning by height on next tick.
                            }
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        });
    }

    async fn catch_up_once(&self) -> Result<(), String> {
        let latest = load_latest_height(self.storage.as_ref());
        if latest == 0 {
            return Ok(());
        }

        let mut cursor = load_cursor(self.storage.as_ref());
        if cursor > latest {
            cursor = latest;
            store_cursor(self.storage.as_ref(), cursor);
        }

        let mut next = cursor.saturating_add(1);
        while next <= latest {
            self.process_height(next).await?;
            store_cursor(self.storage.as_ref(), next);
            next += 1;
        }

        Ok(())
    }

    async fn process_height(&self, height: u64) -> Result<(), String> {
        let Some(block) = load_block(self.storage.as_ref(), height) else {
            return Ok(());
        };

        let semaphore = Arc::new(Semaphore::new(self.max_concurrency));
        let mut handles = Vec::new();

        for tx in block.transactions {
            let Some(common_tx) = to_common_game_bet_tx(&tx) else {
                continue;
            };

            let processor = self.processor.clone();
            let publisher = self.fairness_publisher.clone();
            let permit = semaphore.clone().acquire_owned().await.map_err(|e| e.to_string())?;
            let block_hash = block.block_hash;

            handles.push(tokio::task::spawn_blocking(move || {
                let _permit = permit;

                match processor.process_game_transaction(&common_tx, block_hash, height) {
                    Ok(_result) => {
                        let _ = publisher.send(FairnessPersistedEvent {
                            tx_id: common_tx.id,
                            block_height: height,
                            block_hash,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("Failed to persist fairness for tx {} at height {}: {}", common_tx.id, height, e);
                    }
                }
            }));
        }

        for h in handles {
            let _ = h.await;
        }

        Ok(())
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}
