//! Request Handlers
//! 
//! High-performance request handlers optimized for concurrent access.

use super::{
    errors::ApiError,
    middleware::RequestId,
    models::*,
    storage::ApiStorage,
    websocket::WebSocketManager,
};
use crate::{
    blockchain_game_processor::{load_vrf_public_key, BlockchainGameProcessor, GameBetData},
    fairness::FairnessWaiter,
    finalization::FinalizationWaiter,
    game_store,
};
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Shared application state
pub struct AppState {
    pub storage: ApiStorage,
    pub node_id: String,
    pub network: String,
    pub version: String,
    pub websocket_manager: Arc<WebSocketManager>,
    pub metrics: Arc<super::monitoring::MetricsRegistry>,
    
    // Game-related components for casino functionality
    pub game_processor: Option<Arc<BlockchainGameProcessor>>,
    pub tx_sender: Option<mpsc::UnboundedSender<crate::common::types::Transaction>>,
    pub finalization_waiter: Option<Arc<FinalizationWaiter>>,
    pub fairness_waiter: Option<Arc<FairnessWaiter>>,
}

/// Health check handler - minimal response time
/// GET /health
pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "Running".to_string(),
    })
}

/// Status handler with caching potential
/// GET /status
pub async fn status_handler(
    Extension(request_id): Extension<RequestId>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<StatusResponse>, ApiError> {
    let latest_height = state.storage.get_latest_height()
        .map_err(|e| ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to get height: {}", e)
        ))?;
    
    let latest_hash = state.storage.get_latest_hash()
        .map_err(|e| ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to get hash: {}", e)
        ))?;

    // Height 0 means: no finalized blocks have been committed yet.
    // Treat this as genesis and return a valid status payload.
    let latest_time = if latest_height == 0 {
        Utc::now()
    } else {
        let latest_block = state.storage.get_block_by_height(latest_height)
            .map_err(|e| ApiError::internal_error(
                request_id.0.clone(),
                format!("Failed to get block: {}", e)
            ))?
            .ok_or_else(|| ApiError::internal_error(
                request_id.0.clone(),
                "Latest block not found".to_string()
            ))?;

        DateTime::from_timestamp_millis(latest_block.timestamp as i64)
            .unwrap_or_else(|| Utc::now())
    };
    
    Ok(Json(StatusResponse {
        node_info: NodeInfo {
            id: state.node_id.clone(),
            network: state.network.clone(),
            version: state.version.clone(),
        },
        sync_info: SyncInfo {
            latest_block_height: latest_height,
            latest_block_hash: hex::encode(latest_hash),
            latest_block_time: latest_time,
            catching_up: state.storage.is_catching_up(),
        },
    }))
}

/// Block list query parameters
#[derive(Debug, Deserialize)]
pub struct BlocksQuery {
    #[serde(default)]
    pub from: Option<u64>,
    #[serde(default)]
    pub to: Option<u64>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    50
}

/// Block list handler with caching
/// GET /blocks?from={height}&to={height}&limit={n}
pub async fn blocks_handler(
    Extension(request_id): Extension<RequestId>,
    State(state): State<Arc<AppState>>,
    Query(params): Query<BlocksQuery>,
) -> Result<Json<BlocksResponse>, ApiError> {
    // Enforce maximum limit
    let limit = params.limit.min(100);
    
    let latest_height = state.storage.get_latest_height()
        .map_err(|e| ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to get height: {}", e)
        ))?;
    
    // Default range: last N blocks
    let to = params.to.unwrap_or(latest_height);
    let from = params.from.unwrap_or_else(|| to.saturating_sub(limit as u64));
    
    // Validate range
    if from > to {
        return Err(ApiError::bad_request(
            request_id.0,
            "'from' must be <= 'to'".to_string()
        ));
    }
    
    let blocks = state.storage.get_block_range(from, to, limit)
        .map_err(|e| ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to get blocks: {}", e)
        ))?;
    
    let summaries: Vec<BlockSummary> = blocks.into_iter().map(|block| {
        BlockSummary {
            height: block.height,
            hash: hex::encode(block.block_hash),
            time: DateTime::from_timestamp_millis(block.timestamp as i64).unwrap_or_else(|| Utc::now()),
            tx_count: block.transaction_count,
        }
    }).collect();
    
    let total_returned = summaries.len();
    
    Ok(Json(BlocksResponse {
        blocks: summaries,
        pagination: PaginationInfo {
            from,
            to,
            total_returned,
        },
    }))
}

/// Block detail handler
/// GET /block/{height}
/// Supports both numeric heights (e.g., /block/123) and "latest" (e.g., /block/latest)
pub async fn block_detail_handler(
    Extension(request_id): Extension<RequestId>,
    State(state): State<Arc<AppState>>,
    Path(height_param): Path<String>,
) -> Result<Json<BlockDetailResponse>, ApiError> {
    // Parse height parameter - support both numeric and "latest"
    let height = if height_param == "latest" {
        state.storage.get_latest_height()
            .map_err(|e| ApiError::internal_error(
                request_id.0.clone(),
                format!("Failed to get latest height: {}", e)
            ))?
    } else {
        height_param.parse::<u64>()
            .map_err(|_| ApiError::bad_request(
                request_id.0.clone(),
                format!("Invalid block height: '{}'. Use a number or 'latest'", height_param)
            ))?
    };
    
    let block = state.storage.get_block_by_height(height)
        .map_err(|e| ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to get block: {}", e)
        ))?
        .ok_or_else(|| ApiError::not_found(
            request_id.0.clone(),
            format!("Block {} not found", height)
        ))?;
    
    let tx_ids: Vec<String> = block.transactions.iter()
        .map(|tx| hex::encode(tx.hash()))
        .collect();
    
    Ok(Json(BlockDetailResponse {
        height: block.height,
        hash: hex::encode(block.block_hash),
        prev_hash: hex::encode(block.previous_block_hash),
        time: DateTime::from_timestamp_millis(block.timestamp as i64)
            .unwrap_or_else(|| Utc::now()),
        tx_count: block.transaction_count,
        tx_ids,
        transactions_root: hex::encode(block.transactions_root),
        state_root: hex::encode(block.state_root),
    }))
}

/// O(1) transaction handler
/// GET /tx/{tx_id}
pub async fn transaction_handler(
    Extension(request_id): Extension<RequestId>,
    State(state): State<Arc<AppState>>,
    Path(tx_id): Path<String>,
) -> Result<Json<TransactionResponse>, ApiError> {
    // For now, we'll use the existing transaction lookup
    // TODO: Implement hash-based lookup when available
    if let Ok(tx_id_u64) = tx_id.parse::<u64>() {
        let result = state.storage.find_transaction(tx_id_u64)
            .map_err(|e| ApiError::internal_error(
                request_id.0.clone(),
                format!("Failed to find transaction: {}", e)
            ))?
            .ok_or_else(|| ApiError::not_found(
                request_id.0.clone(),
                format!("Transaction {} not found", tx_id)
            ))?;
        
        let (block_height, tx_index, tx) = result;
        
        // Get block hash for inclusion info
        let block = state.storage.get_block_by_height(block_height)
            .map_err(|e| ApiError::internal_error(
                request_id.0.clone(),
                format!("Failed to get block: {}", e)
            ))?
            .ok_or_else(|| ApiError::internal_error(
                request_id.0.clone(),
                "Block not found for transaction".to_string()
            ))?;

        // Best-effort decode of game bet payload.
        let decoded_game_bet: Option<GameBetData> = serde_json::from_slice(&tx.data).ok();

        // Best-effort load of persisted game result (DB is source of truth).
        let raw_storage = state.storage.get_raw_storage();
        let persisted_game_result = game_store::load_game_result(raw_storage.as_ref(), tx_id_u64)
            .unwrap_or(None);

        // Prefer the in-memory processor's public key; fall back to DB seed.
        let public_key_hex = if let Some(processor) = &state.game_processor {
            hex::encode(processor.get_public_key())
        } else {
            load_vrf_public_key(raw_storage.as_ref())
                .ok()
                .flatten()
                .map(hex::encode)
                .unwrap_or_default()
        };

        let fairness = if decoded_game_bet.is_some() || persisted_game_result.is_some() {
            let game_result = persisted_game_result.map(|r| PersistedGameResult {
                transaction_id: r.transaction_id,
                player_address: r.player_address,
                game_type: r.game_type,
                bet_amount: r.bet_amount,
                token: r.token,
                player_choice: r.player_choice,
                coin_result: r.coin_result,
                outcome: r.outcome,
                vrf: crate::games::types::VRFBundle {
                    vrf_output: hex::encode(r.vrf_output),
                    vrf_proof: hex::encode(r.vrf_proof),
                    public_key: public_key_hex.clone(),
                    input_message: r.vrf_input_message,
                },
                payout: r.payout,
                timestamp: r.timestamp,
                block_height: r.block_height,
                block_hash: hex::encode(r.block_hash),
            });

            Some(FairnessRecord {
                game_bet: decoded_game_bet,
                game_result,
            })
        } else {
            None
        };

        let tx_type = if fairness.is_some() {
            "GAME_BET".to_string()
        } else {
            "GENERIC".to_string()
        };
        
        Ok(Json(TransactionResponse {
            tx_id: tx_id.clone(),
            included_in: InclusionInfo {
                block_height,
                block_hash: hex::encode(block.block_hash),
                index: tx_index,
            },
            tx_type,
            data: TransactionData {
                sender: hex::encode(tx.sender),
                data: hex::encode(&tx.data),
                timestamp: tx.timestamp,
                nonce: tx.nonce,
            },
            fairness,
        }))
    } else {
        Err(ApiError::bad_request(
            request_id.0,
            "Invalid transaction ID format".to_string()
        ))
    }
}
