//! Settlement API handlers
//!
//! Provides endpoints for managing game settlement status between blockchain
//! and transaction processor, with optimistic locking and cursor-based pagination.

use crate::api::{
    errors::{ApiError, ApiErrorKind},
    handlers::AppState,
    middleware::RequestId,
};
use crate::blockchain_game_processor::{BlockchainGameResult, SettlementStatus};
use crate::game_store;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::Json,
    Extension,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Simple API key check for settlement endpoints
fn validate_settlement_api_key(headers: &HeaderMap, request_id: &str) -> Result<(), ApiError> {
    if let Ok(expected_key) = std::env::var("SETTLEMENT_API_KEY") {
        if let Some(provided_key) = headers.get("X-API-Key") {
            if provided_key.to_str().unwrap_or("") == expected_key {
                return Ok(());
            }
        }
        Err(ApiError::bad_request(
            request_id.to_string(),
            "Invalid or missing settlement API key".to_string(),
        ))
    } else {
        // No API key configured - allow for development
        Ok(())
    }
}

/// Query parameters for pending settlements endpoint
#[derive(Debug, Deserialize)]
pub struct SettlementQuery {
    /// Maximum number of games to return (default: 50, max: 500)
    pub limit: Option<usize>,
    /// Pagination cursor for scanning through results
    #[serde(default)]
    pub cursor: Option<String>,
}

/// Settlement update request body
#[derive(Debug, Deserialize)]
pub struct SettlementUpdateRequest {
    /// New settlement status
    pub status: SettlementStatus,
    /// Solana transaction hash when submitted
    #[serde(default)]
    pub solana_tx_id: Option<String>,
    /// Current version for optimistic locking
    pub expected_version: u64,
    /// Error description if settlement failed
    #[serde(default)]
    pub error_message: Option<String>,
}

/// Settlement update response
#[derive(Debug, Serialize)]
pub struct SettlementUpdateResponse {
    pub success: bool,
    pub new_version: u64,
}

/// Settlement information for API responses
#[derive(Debug, Serialize)]
pub struct GameSettlementInfo {
    pub transaction_id: u64,
    pub player_address: String,
    pub game_type: String,
    pub bet_amount: u64,
    pub token: String,
    pub outcome: String,
    pub payout: u64,
    pub vrf_proof: String,
    pub vrf_output: String,
    pub block_height: u64,
    pub version: u64,
}

/// Response for pending settlements endpoint
#[derive(Debug, Serialize)]
pub struct PendingSettlementsResponse {
    pub games: Vec<GameSettlementInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Settlement event for ingest endpoint
#[derive(Debug, Deserialize)]
pub struct SettlementEvent {
    pub transaction_id: u64,
    pub player_address: String,
    pub game_type: String,
    pub bet_amount: u64,
    pub token: String,
    pub outcome: String,
    pub payout: u64,
    pub vrf_proof: String,
    pub vrf_output: String,
    pub block_height: u64,
    pub block_hash: String,
    pub timestamp: u64,
}

impl From<BlockchainGameResult> for GameSettlementInfo {
    fn from(result: BlockchainGameResult) -> Self {
        Self {
            transaction_id: result.transaction_id,
            player_address: result.player_address,
            game_type: format!("{:?}", result.game_type),
            bet_amount: result.bet_amount,
            token: format!("{:?}", result.token),
            outcome: format!("{:?}", result.outcome),
            payout: result.payout,
            vrf_proof: hex::encode(result.vrf_proof),
            vrf_output: hex::encode(result.vrf_output),
            block_height: result.block_height,
            version: result.version,
        }
    }
}

/// GET /api/settlement/pending - Retrieve games awaiting settlement
pub async fn get_pending_settlements(
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Query(params): Query<SettlementQuery>,
) -> Result<Json<PendingSettlementsResponse>, ApiError> {
    // Validate API key
    validate_settlement_api_key(&headers, &request_id.0)?;
    
    // Limit query size to prevent abuse
    let limit = params.limit.unwrap_or(50).min(200);

    let (game_results, next_cursor) = game_store::load_pending_settlements(
        state.storage.get_raw_storage().as_ref(),
        params.cursor.as_deref(),
        limit,
    )
    .map_err(|e| {
        ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to load pending settlements: {}", e),
        )
    })?;

    let games: Vec<GameSettlementInfo> = game_results
        .into_iter()
        .map(GameSettlementInfo::from)
        .collect();

    Ok(Json(PendingSettlementsResponse { games, next_cursor }))
}

/// POST /api/settlement/games/:tx_id - Update settlement status with optimistic locking
pub async fn update_settlement_status(
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Path(tx_id): Path<u64>,
    Json(update): Json<SettlementUpdateRequest>,
) -> Result<Json<SettlementUpdateResponse>, ApiError> {
    // Validate API key
    validate_settlement_api_key(&headers, &request_id.0)?;
    
    // Load current game result
    let mut game_result = game_store::load_game_result(state.storage.get_raw_storage().as_ref(), tx_id)
        .map_err(|e| {
            ApiError::internal_error(
                request_id.0.clone(),
                format!("Failed to load game result: {}", e),
            )
        })?
        .ok_or_else(|| {
            ApiError::not_found(
                request_id.0.clone(),
                format!("Game with transaction ID {} not found", tx_id),
            )
        })?;

    // Check version for optimistic locking - return 409 Conflict for version mismatch
    if game_result.version != update.expected_version {
        return Err(ApiError::bad_request(
            request_id.0.clone(),
            format!(
                "Version mismatch: expected {}, found {} (409 Conflict)",
                update.expected_version, game_result.version
            ),
        ));
    }

    // Update settlement fields
    game_result.settlement_status = update.status.clone();
    game_result.version += 1;
    game_result.solana_tx_id = update.solana_tx_id.clone();
    game_result.settlement_error = update.error_message.clone();

    // Set completion timestamp if settled
    if update.status == SettlementStatus::SettlementComplete {
        game_result.settlement_completed_at = Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }

    // Store updated result
    game_store::store_game_result(state.storage.get_raw_storage().as_ref(), &game_result).map_err(|e| {
        ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to update game result: {}", e),
        )
    })?;

    Ok(Json(SettlementUpdateResponse {
        success: true,
        new_version: game_result.version,
    }))
}

/// POST /api/settlement/ingest - Ingest settlement events from outbox dispatcher
pub async fn ingest_settlement_event(
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    State(_state): State<Arc<AppState>>,
    Json(_event): Json<SettlementEvent>,
) -> Result<(), ApiError> {
    // Validate API key
    validate_settlement_api_key(&headers, &request_id.0)?;
    
    // This endpoint is idempotent and serves as a receiver for event-driven architectures.
    // Currently, actual settlement processing is handled by the polling mechanism via
    // /api/settlement/pending. Future implementations may process events here.
    
    // For now, we simply accept the event and return 202 Accepted
    Ok(())
}