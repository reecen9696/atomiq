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
use crate::games::types::{CoinChoice, CoinFlipResult, GameOutcome, GameType, Token};
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
    /// Retry counter for failed settlements
    #[serde(default)]
    pub retry_count: Option<u32>,
    /// Unix timestamp (ms) when next retry should be attempted
    #[serde(default)]
    pub next_retry_after: Option<i64>,
}

/// Settlement update response
#[derive(Debug, Serialize)]
pub struct SettlementUpdateResponse {
    pub success: bool,
    pub new_version: u64,
}

/// Detailed settlement information for API responses
#[derive(Debug, Serialize)]
pub struct GameSettlementDetail {
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
    pub version: u64,
    pub settlement_status: SettlementStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solana_tx_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settlement_completed_at: Option<u64>,
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
    #[serde(default)]
    pub retry_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_after: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solana_tx_id: Option<String>,
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
            retry_count: result.retry_count,
            next_retry_after: result.next_retry_after,
            solana_tx_id: result.solana_tx_id,
        }
    }
}

impl From<BlockchainGameResult> for GameSettlementDetail {
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
            block_hash: hex::encode(result.block_hash),
            version: result.version,
            settlement_status: result.settlement_status,
            solana_tx_id: result.solana_tx_id,
            settlement_error: result.settlement_error,
            settlement_completed_at: result.settlement_completed_at,
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

/// GET /api/settlement/games/:tx_id - Retrieve settlement details for a game
pub async fn get_settlement_game(
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Path(tx_id): Path<u64>,
) -> Result<Json<GameSettlementDetail>, ApiError> {
    validate_settlement_api_key(&headers, &request_id.0)?;

    let game_result = game_store::load_game_result(state.storage.get_raw_storage().as_ref(), tx_id)
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

    Ok(Json(GameSettlementDetail::from(game_result)))
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
    
    // Update retry fields if provided
    if let Some(retry_count) = update.retry_count {
        game_result.retry_count = retry_count;
    }
    if update.next_retry_after.is_some() {
        game_result.next_retry_after = update.next_retry_after;
    }

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
    State(state): State<Arc<AppState>>,
    Json(event): Json<SettlementEvent>,
) -> Result<(), ApiError> {
    // Validate API key
    validate_settlement_api_key(&headers, &request_id.0)?;

    fn decode_hex_or_bytes(value: &str) -> Vec<u8> {
        let trimmed = value.trim().trim_start_matches("0x");
        let is_hex = trimmed.len() % 2 == 0 && trimmed.chars().all(|c| c.is_ascii_hexdigit());

        if is_hex {
            hex::decode(trimmed).unwrap_or_else(|_| value.as_bytes().to_vec())
        } else {
            value.as_bytes().to_vec()
        }
    }

    fn decode_32_byte_hash_or_zero(value: &str) -> [u8; 32] {
        let bytes = decode_hex_or_bytes(value);
        if bytes.len() == 32 {
            let mut out = [0u8; 32];
            out.copy_from_slice(&bytes);
            out
        } else {
            [0u8; 32]
        }
    }

    let game_type = match event.game_type.trim().to_lowercase().as_str() {
        "coinflip" | "coin_flip" | "coin-flip" => GameType::CoinFlip,
        other => {
            return Err(ApiError::bad_request(
                request_id.0.clone(),
                format!("Unsupported game_type for ingest: {}", other),
            ));
        }
    };

    let outcome = match event.outcome.trim().to_lowercase().as_str() {
        "win" => GameOutcome::Win,
        "loss" | "lose" => GameOutcome::Loss,
        other => {
            return Err(ApiError::bad_request(
                request_id.0.clone(),
                format!("Unsupported outcome for ingest: {}", other),
            ));
        }
    };

    // Ingest is primarily a testing/debug endpoint. If the event does not
    // carry full game details (e.g. coin result), we fill minimal placeholders.
    let (player_choice, coin_result) = match outcome {
        GameOutcome::Win => (CoinChoice::Heads, CoinFlipResult::Heads),
        GameOutcome::Loss => (CoinChoice::Heads, CoinFlipResult::Tails),
    };

    let token = Token {
        symbol: event.token.clone(),
        mint_address: None,
    };

    let vrf_proof = decode_hex_or_bytes(&event.vrf_proof);
    let vrf_output = decode_hex_or_bytes(&event.vrf_output);
    let block_hash = decode_32_byte_hash_or_zero(&event.block_hash);

    let player_address = event.player_address.clone();

    let game_result = BlockchainGameResult {
        transaction_id: event.transaction_id,
        player_address,
        game_type,
        bet_amount: event.bet_amount,
        token,
        player_choice,
        coin_result,
        outcome,
        vrf_proof,
        vrf_output,
        vrf_input_message: format!(
            "ingest:{}:{}:{}",
            event.transaction_id,
            format!("{}", game_type),
            event.player_address
        ),
        payout: event.payout,
        timestamp: event.timestamp,
        block_height: event.block_height,
        block_hash,
        settlement_status: SettlementStatus::PendingSettlement,
        version: 1,
        solana_tx_id: None,
        settlement_error: None,
        settlement_completed_at: None,
        retry_count: 0,
        next_retry_after: None,
    };

    game_store::store_game_result(state.storage.get_raw_storage().as_ref(), &game_result).map_err(|e| {
        ApiError::internal_error(
            request_id.0.clone(),
            format!("Failed to ingest settlement event: {}", e),
        )
    })?;

    Ok(())
}