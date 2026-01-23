//! Persistent game/fairness records stored in RocksDB.

use crate::{
    blockchain_game_processor::{BlockchainGameResult, SettlementStatus},
    errors::{AtomiqError, AtomiqResult, StorageError},
    storage::OptimizedStorage,
};
use hex;
use hotstuff_rs::block_tree::pluggables::KVGet;
use serde::{Deserialize, Serialize};

const GAME_RESULT_PREFIX: &str = "game:result:tx:";
const RECENT_GAMES_PREFIX: &[u8] = b"game:index:recent:";
const SETTLEMENT_PENDING_PREFIX: &[u8] = b"settlement:pending:";

/// Lightweight settlement summary for efficient queries
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SettlementSummary {
    pub transaction_id: u64,
    pub player_address: String,
    pub bet_amount: u64,
    pub payout: u64,
    pub version: u64,
    pub block_height: u64,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(default)]
    pub next_retry_after: Option<i64>,
}

fn game_result_key(tx_id: u64) -> Vec<u8> {
    format!("{}{}", GAME_RESULT_PREFIX, tx_id).into_bytes()
}

fn recent_game_index_key(block_height: u64, tx_id: u64) -> Vec<u8> {
    // Sort newest-first by using an inverted height as the primary sort key.
    // Key layout: prefix | inv_height(be) | tx_id(be)
    let inv_height = u64::MAX - block_height;
    let mut key = Vec::with_capacity(RECENT_GAMES_PREFIX.len() + 16);
    key.extend_from_slice(RECENT_GAMES_PREFIX);
    key.extend_from_slice(&inv_height.to_be_bytes());
    key.extend_from_slice(&tx_id.to_be_bytes());
    key
}

fn settlement_pending_key(tx_id: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(SETTLEMENT_PENDING_PREFIX.len() + 8);
    key.extend_from_slice(SETTLEMENT_PENDING_PREFIX);
    key.extend_from_slice(&tx_id.to_be_bytes());
    key
}

pub fn load_recent_game_tx_ids(
    storage: &OptimizedStorage,
    cursor_hex: Option<&str>,
    limit: usize,
) -> AtomiqResult<(Vec<u64>, Option<String>)> {
    let cursor_bytes = match cursor_hex {
        Some(c) => Some(hex::decode(c).map_err(|e| {
            AtomiqError::Storage(StorageError::CorruptedData(format!(
                "Invalid cursor hex: {}",
                e
            )))
        })?),
        None => None,
    };

    let rows = storage.scan_prefix(
        RECENT_GAMES_PREFIX,
        cursor_bytes.as_deref(),
        limit.max(1),
    );

    let mut tx_ids = Vec::with_capacity(rows.len());
    let mut next_cursor: Option<String> = None;

    for (key, _value) in rows {
        if !key.starts_with(RECENT_GAMES_PREFIX) || key.len() < RECENT_GAMES_PREFIX.len() + 16 {
            continue;
        }

        let tx_id_off = key.len() - 8;
        let tx_id_bytes: [u8; 8] = key[tx_id_off..].try_into().unwrap_or([0u8; 8]);
        tx_ids.push(u64::from_be_bytes(tx_id_bytes));
        next_cursor = Some(hex::encode(key));
    }

    Ok((tx_ids, next_cursor))
}

pub fn load_game_result(storage: &OptimizedStorage, tx_id: u64) -> AtomiqResult<Option<BlockchainGameResult>> {
    let key = game_result_key(tx_id);
    let Some(bytes) = storage.get(&key) else {
        return Ok(None);
    };

    let result: BlockchainGameResult = serde_json::from_slice(&bytes).map_err(|e| {
        AtomiqError::Storage(StorageError::CorruptedData(format!(
            "Failed to decode game result for tx {}: {}",
            tx_id, e
        )))
    })?;

    Ok(Some(result))
}

pub fn store_game_result(storage: &OptimizedStorage, result: &BlockchainGameResult) -> AtomiqResult<()> {
    let key = game_result_key(result.transaction_id);
    let bytes = serde_json::to_vec(result).map_err(|e| {
        AtomiqError::Storage(StorageError::WriteFailed(format!(
            "Failed to encode game result for tx {}: {}",
            result.transaction_id, e
        )))
    })?;

    // Prepare settlement index entry - only write if actually pending
    let settlement_key = settlement_pending_key(result.transaction_id);
    let index_key = recent_game_index_key(result.block_height, result.transaction_id);
    
    let mut items: Vec<(Vec<u8>, Vec<u8>)> = vec![
        (key, bytes),
        (index_key, Vec::new()),
    ];

    // Conditionally add settlement index entry
    if result.settlement_status == SettlementStatus::PendingSettlement {
        let summary = SettlementSummary {
            transaction_id: result.transaction_id,
            player_address: result.player_address.clone(),
            bet_amount: result.bet_amount,
            payout: result.payout,
            version: result.version,
            block_height: result.block_height,
            retry_count: result.retry_count,
            next_retry_after: result.next_retry_after,
        };
        let settlement_bytes = serde_json::to_vec(&summary).map_err(|e| {
            AtomiqError::Storage(StorageError::WriteFailed(format!(
                "Failed to encode settlement summary for tx {}: {}",
                result.transaction_id, e
            )))
        })?;
        items.push((settlement_key.clone(), settlement_bytes));
        tracing::debug!(
            tx_id = result.transaction_id,
            retry_count = result.retry_count,
            "Writing settlement index entry for pending/retryable settlement"
        );
    } else {
        // Explicitly delete settlement index entry if complete or permanently failed
        storage.delete(&settlement_key).ok(); // Ignore errors if key doesn't exist
        tracing::debug!(
            tx_id = result.transaction_id,
            status = ?result.settlement_status,
            "Removing settlement index entry (complete/permanent status)"
        );
    }

    storage
        .batch_write(&items)
        .map_err(|e| AtomiqError::Storage(StorageError::WriteFailed(e.to_string())))?;

    Ok(())
}

/// Load pending settlements using efficient settlement index
pub fn load_pending_settlements(
    storage: &OptimizedStorage,
    cursor_hex: Option<&str>,
    limit: usize,
) -> AtomiqResult<(Vec<BlockchainGameResult>, Option<String>)> {
    let cursor_bytes = match cursor_hex {
        Some(c) => Some(hex::decode(c).map_err(|e| {
            AtomiqError::Storage(StorageError::CorruptedData(format!(
                "Invalid cursor hex: {}",
                e
            )))
        })?),
        None => None,
    };

    // Scan settlement pending index directly
    // We scan 10x the limit to account for empty/deleted entries
    // Empty entries are left in place for efficient deletion without compaction
    let scan_limit = (limit * 10).max(50);
    let rows = storage.scan_prefix(
        SETTLEMENT_PENDING_PREFIX,
        cursor_bytes.as_deref(),
        scan_limit,
    );

    let rows_count = rows.len();
    tracing::info!(
        "load_pending_settlements: settlement index returned {} rows, cursor={:?}",
        rows_count,
        cursor_hex
    );

    // FALLBACK: If settlement index is empty (games created before index was added),
    // scan recent games to find pending settlements
    if rows.is_empty() && cursor_hex.is_none() {
        tracing::info!("load_pending_settlements: Using fallback - scanning recent games");
        let (tx_ids, _) = load_recent_game_tx_ids(storage, None, limit * 2)?;
        tracing::info!("load_pending_settlements: Found {} recent game tx_ids", tx_ids.len());
        let mut pending_games = Vec::new();
        
        for tx_id in tx_ids {
            if let Ok(Some(game_result)) = load_game_result(storage, tx_id) {
                let is_pending = game_result.settlement_status == SettlementStatus::PendingSettlement;
                let is_retryable = game_result.settlement_status == SettlementStatus::SettlementFailed
                    && game_result.retry_count < 3
                    && game_result.next_retry_after.map_or(true, |retry_after| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as i64;
                        retry_after <= now
                    });
                
                if is_pending || is_retryable {
                    pending_games.push(game_result);
                    if pending_games.len() >= limit {
                        break;
                    }
                }
            }
        }
        
        tracing::info!("load_pending_settlements: Fallback returned {} pending games", pending_games.len());
        return Ok((pending_games, None));
    }

    let mut pending_games = Vec::with_capacity(rows.len());
    let mut next_cursor: Option<String> = None;
    let mut empty_count = 0;
    let mut parse_errors = 0;
    let mut load_errors = 0;
    let mut status_mismatch = 0;

    for (key, value) in rows {
        // Stop if we've collected enough valid results
        if pending_games.len() >= limit {
            break;
        }

        // Skip empty values (removed settlements)
        if value.is_empty() {
            empty_count += 1;
            continue;
        }

        // Parse settlement summary to get transaction ID
        match serde_json::from_slice::<SettlementSummary>(&value) {
            Ok(summary) => {
                // Load full game result
                match load_game_result(storage, summary.transaction_id) {
                    Ok(Some(game_result)) => {
                        // Include PendingSettlement and retryable SettlementFailed
                        let is_pending = game_result.settlement_status == SettlementStatus::PendingSettlement;
                        let is_retryable_failed = game_result.settlement_status == SettlementStatus::SettlementFailed
                            && game_result.retry_count < 3
                            && game_result.next_retry_after.map_or(true, |retry_after| {
                                let now = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis() as i64;
                                retry_after <= now
                            });
                        
                        if is_pending || is_retryable_failed {
                            pending_games.push(game_result);
                        } else {
                            status_mismatch += 1;
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("Settlement index has tx_id {} but game result not found", summary.transaction_id);
                        load_errors += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load game result for tx_id {}: {}", summary.transaction_id, e);
                        load_errors += 1;
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to parse settlement summary: {}", e);
                parse_errors += 1;
            }
        }

        // Set next cursor to the last key processed
        next_cursor = Some(hex::encode(&key));
    }

    tracing::info!(
        "load_pending_settlements: Processed {} settlement index entries - empty:{}, parse_errors:{}, load_errors:{}, status_mismatch:{}, returned:{}",
        rows_count, empty_count, parse_errors, load_errors, status_mismatch, pending_games.len()
    );

    // Only return cursor if we might have more results
    let final_cursor = if pending_games.len() >= limit {
        next_cursor
    } else {
        None
    };

    Ok((pending_games, final_cursor))
}

// ============================================================================
// Casino Statistics Tracking
// ============================================================================

const CASINO_STATS_KEY: &[u8] = b"casino:stats";

/// Casino statistics for tracking overall performance
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CasinoStats {
    pub total_wagered: f64,
    pub total_paid_out: f64,
    pub bet_count: u64,
    pub wins_24h: u64,
    pub wagered_24h: f64,
    pub last_24h_reset: i64, // Unix timestamp in seconds
}

impl Default for CasinoStats {
    fn default() -> Self {
        Self {
            total_wagered: 0.0,
            total_paid_out: 0.0,
            bet_count: 0,
            wins_24h: 0,
            wagered_24h: 0.0,
            last_24h_reset: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }
    }
}

/// Load casino statistics from storage
pub fn load_casino_stats(storage: &OptimizedStorage) -> AtomiqResult<CasinoStats> {
    match storage.get(CASINO_STATS_KEY) {
        Some(bytes) => {
            let stats: CasinoStats = serde_json::from_slice(&bytes).map_err(|e| {
                AtomiqError::Storage(StorageError::CorruptedData(format!(
                    "Failed to decode casino stats: {}",
                    e
                )))
            })?;
            Ok(stats)
        }
        None => Ok(CasinoStats::default()),
    }
}

/// Update casino statistics with a new game result
pub fn update_casino_stats(
    storage: &OptimizedStorage,
    game_result: &BlockchainGameResult,
) -> AtomiqResult<()> {
    let mut stats = load_casino_stats(storage)?;
    
    // Convert lamports to SOL (assuming bet amounts are in lamports)
    let bet_amount_sol = game_result.bet_amount as f64 / 1_000_000_000.0;
    let payout_sol = game_result.payout as f64 / 1_000_000_000.0;
    
    // Update cumulative stats
    stats.total_wagered += bet_amount_sol;
    stats.total_paid_out += payout_sol;
    stats.bet_count += 1;
    
    // Check if we need to reset 24h stats
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    
    let seconds_in_24h = 86400;
    if now - stats.last_24h_reset >= seconds_in_24h {
        stats.wins_24h = 0;
        stats.wagered_24h = 0.0;
        stats.last_24h_reset = now;
    }
    
    // Update 24h stats
    stats.wagered_24h += bet_amount_sol;
    if payout_sol > bet_amount_sol {
        stats.wins_24h += 1;
    }
    
    // Save updated stats
    let bytes = serde_json::to_vec(&stats).map_err(|e| {
        AtomiqError::Storage(StorageError::WriteFailed(format!(
            "Failed to encode casino stats: {}",
            e
        )))
    })?;
    
    storage.put(CASINO_STATS_KEY, &bytes)
        .map_err(|e| AtomiqError::Storage(StorageError::WriteFailed(e.to_string())))?;
    
    Ok(())
}
