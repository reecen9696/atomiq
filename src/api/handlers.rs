//! Request Handlers
//! 
//! Handle HTTP requests and return properly formatted responses.

use super::{
    errors::ApiError,
    middleware::RequestId,
    models::*,
    storage::ApiStorage,
};
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::sync::Arc;

/// Shared application state
pub struct AppState {
    pub storage: ApiStorage,
    pub node_id: String,
    pub network: String,
    pub version: String,
}

/// Health check handler
/// GET /health
pub async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "Running".to_string(),
    })
}

/// Status handler
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
    
    // Get latest block for timestamp
    let latest_block = state.storage.get_block_by_height(latest_height)
        .map_err(|e| ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to get block: {}", e)
        ))?
        .ok_or_else(|| ApiError::internal_error(
            request_id.0.clone(),
            "Latest block not found".to_string()
        ))?;
    
    let latest_time = DateTime::from_timestamp_millis(latest_block.timestamp as i64)
        .unwrap_or_else(|| Utc::now());
    
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

/// Block list handler
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
pub async fn block_detail_handler(
    Extension(request_id): Extension<RequestId>,
    State(state): State<Arc<AppState>>,
    Path(height): Path<u64>,
) -> Result<Json<BlockDetailResponse>, ApiError> {
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
        .map(|tx| tx.id.to_string())
        .collect();
    
    Ok(Json(BlockDetailResponse {
        height: block.height,
        hash: hex::encode(block.block_hash),
        prev_hash: hex::encode(block.previous_block_hash),
        time: DateTime::from_timestamp_millis(block.timestamp as i64).unwrap_or_else(|| Utc::now()),
        tx_count: block.transaction_count,
        tx_ids,
        transactions_root: hex::encode(block.transactions_root),
        state_root: hex::encode(block.state_root),
    }))
}

/// Transaction detail handler
/// GET /tx/{tx_id}
pub async fn transaction_handler(
    Extension(request_id): Extension<RequestId>,
    State(state): State<Arc<AppState>>,
    Path(tx_id): Path<u64>,
) -> Result<Json<TransactionResponse>, ApiError> {
    let result = state.storage.find_transaction(tx_id)
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
    
    Ok(Json(TransactionResponse {
        tx_id: tx.id.to_string(),
        included_in: InclusionInfo {
            block_height,
            block_hash: hex::encode(block.block_hash),
            index: tx_index,
        },
        tx_type: "GENERIC".to_string(),
        data: TransactionData {
            sender: hex::encode(tx.sender),
            data: hex::encode(&tx.data),
            timestamp: tx.timestamp,
            nonce: tx.nonce,
        },
    }))
}
