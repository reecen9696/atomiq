//! Game API Wrapper Handlers
//! 
//! These handlers convert from AppState to GameApiState to integrate
//! game endpoints with the main API server state management.

use super::{
    handlers::AppState,
    games::{GameApiState, play_coinflip as inner_play_coinflip, get_game_result as inner_get_game_result, 
           verify_vrf as inner_verify_vrf, verify_game_by_id as inner_verify_game_by_id, 
           list_supported_tokens as inner_list_supported_tokens},
};
use crate::games::types::{CoinFlipPlayRequest, VerifyVRFRequest, Token, GameResponse, VerifyVRFResponse};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use std::sync::Arc;

/// Wrapper for coinflip play that converts AppState to GameApiState
/// POST /api/coinflip/play
pub async fn play_coinflip(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<CoinFlipPlayRequest>,
) -> Result<Json<GameResponse>, (StatusCode, String)> {
    // Check if game components are available
    let game_processor = app_state.game_processor.as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "Game processor not available".to_string()))?;
    
    let tx_sender = app_state.tx_sender.as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "Transaction sender not available".to_string()))?;
    
    // Create game state
    let game_state = GameApiState {
        storage: app_state.storage.get_raw_storage(),
        game_processor: game_processor.clone(),
        tx_sender: tx_sender.clone(),
        finalization_waiter: app_state.finalization_waiter.clone(),
        fairness_waiter: app_state.fairness_waiter.clone(),
    };
    
    // Call the original handler
    inner_play_coinflip(State(game_state), Json(request)).await
}

/// Wrapper for get game result that converts AppState to GameApiState  
/// GET /api/game/:id
pub async fn get_game_result(
    Path(game_id): Path<String>,
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<GameResponse>, (StatusCode, String)> {
    // Check if game components are available
    let game_processor = app_state.game_processor.as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "Game processor not available".to_string()))?;
    
    let tx_sender = app_state.tx_sender.as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "Transaction sender not available".to_string()))?;
    
    // Create game state
    let game_state = GameApiState {
        storage: app_state.storage.get_raw_storage(),
        game_processor: game_processor.clone(),
        tx_sender: tx_sender.clone(),
        finalization_waiter: app_state.finalization_waiter.clone(),
        fairness_waiter: app_state.fairness_waiter.clone(),
    };
    
    // Call the original handler
    inner_get_game_result(Path(game_id), State(game_state)).await
}

/// Wrapper for VRF verification that converts AppState to GameApiState
/// POST /api/verify/vrf
pub async fn verify_vrf(
    State(app_state): State<Arc<AppState>>,
    Json(request): Json<VerifyVRFRequest>,
) -> Result<Json<VerifyVRFResponse>, (StatusCode, String)> {
    // Check if game components are available
    let game_processor = app_state.game_processor.as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "Game processor not available".to_string()))?;
    
    let tx_sender = app_state.tx_sender.as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "Transaction sender not available".to_string()))?;
    
    // Create game state
    let game_state = GameApiState {
        storage: app_state.storage.get_raw_storage(),
        game_processor: game_processor.clone(),
        tx_sender: tx_sender.clone(),
        finalization_waiter: app_state.finalization_waiter.clone(),
        fairness_waiter: app_state.fairness_waiter.clone(),
    };
    
    // Call the original handler
    inner_verify_vrf(State(game_state), Json(request)).await
}

/// Wrapper for game verification by ID that converts AppState to GameApiState
/// GET /api/verify/game/:id
pub async fn verify_game_by_id(
    Path(game_id): Path<String>,
    State(app_state): State<Arc<AppState>>,
) -> Result<Json<VerifyVRFResponse>, (StatusCode, String)> {
    // Check if game components are available
    let game_processor = app_state.game_processor.as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "Game processor not available".to_string()))?;
    
    let tx_sender = app_state.tx_sender.as_ref()
        .ok_or_else(|| (StatusCode::SERVICE_UNAVAILABLE, "Transaction sender not available".to_string()))?;
    
    // Create game state
    let game_state = GameApiState {
        storage: app_state.storage.get_raw_storage(),
        game_processor: game_processor.clone(),
        finalization_waiter: app_state.finalization_waiter.clone(),
        fairness_waiter: app_state.fairness_waiter.clone(),
        tx_sender: tx_sender.clone(),
    };
    
    // Call the original handler
    inner_verify_game_by_id(Path(game_id), State(game_state)).await
}

/// Wrapper for listing supported tokens (no state conversion needed)
/// GET /api/tokens  
pub async fn list_supported_tokens() -> Json<Vec<Token>> {
    inner_list_supported_tokens().await
}