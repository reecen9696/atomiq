//! Route Definitions
//! 
//! Maps URLs to handlers with type-safe routing.

use super::{
    handlers::*, 
    websocket::{websocket_handler, transaction_websocket_handler},
    monitoring::metrics_handler,
    games_wrappers::{play_coinflip, get_game_result, verify_vrf, verify_game_by_id, list_supported_tokens},
    settlement::{get_pending_settlements, get_settlement_game, update_settlement_status, ingest_settlement_event},
};
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

/// Build the API router with all endpoints
pub fn create_router(state: Arc<AppState>) -> Router {
    let mut router = Router::new()
        // Health check (high priority)
        .route("/health", get(health_handler))
        
        // Status endpoint (with caching)
        .route("/status", get(status_handler))
        
        // Block endpoints (with caching)
        .route("/blocks", get(blocks_handler))
        .route("/block/:height", get(block_detail_handler))
        
        // Transaction endpoint (O(1) lookup)
        .route("/tx/:tx_id", get(transaction_handler))
        
        // WebSocket endpoints for real-time updates
        .route("/ws", get(websocket_handler))
        .route("/ws/tx/:tx_id", get(transaction_websocket_handler))
        
        // Metrics endpoint for Prometheus
        .route("/metrics", get(metrics_handler));
    
    // Add game endpoints if game processor is available
    if state.game_processor.is_some() && state.tx_sender.is_some() {
        router = router
            // Casino game endpoints
            .route("/api/coinflip/play", post(play_coinflip))
            .route("/api/game/:id", get(get_game_result))
            .route("/api/verify/vrf", post(verify_vrf))
            .route("/api/verify/game/:id", get(verify_game_by_id))
            .route("/api/tokens", get(list_supported_tokens))
            .route("/api/games/recent", get(recent_games_handler))
            // Casino statistics
            .route("/api/casino/stats", get(casino_stats_handler))
            // Settlement API endpoints
            .route("/api/settlement/pending", get(get_pending_settlements))
            .route(
                "/api/settlement/games/:tx_id",
                get(get_settlement_game).post(update_settlement_status),
            )
            .route("/api/settlement/ingest", post(ingest_settlement_event));
    }
    
    // Attach shared state
    router.with_state(state)
}

/// Build the game API router with all casino game endpoints (legacy function - kept for compatibility)
pub fn create_game_router(game_state: Arc<crate::api::games::GameApiState>) -> Router {
    use super::games::{
        play_coinflip as original_play_coinflip, 
        get_game_result as original_get_game_result,
        verify_vrf as original_verify_vrf, 
        verify_game_by_id as original_verify_game_by_id, 
        list_supported_tokens as original_list_supported_tokens,
        recent_games as original_recent_games,
    };
    
    Router::new()
        // Casino game endpoints
        .route("/api/coinflip/play", post(original_play_coinflip))
        .route("/api/game/:id", get(original_get_game_result))
        .route("/api/verify/vrf", post(original_verify_vrf))
        .route("/api/verify/game/:id", get(original_verify_game_by_id))
        .route("/api/tokens", get(original_list_supported_tokens))
        .route("/api/games/recent", get(original_recent_games))
        .with_state((*game_state).clone())
}

/// Convert AppState to GameApiState for game endpoints
pub fn app_state_to_game_state(app_state: &AppState) -> Option<crate::api::games::GameApiState> {
    if let (Some(game_processor), Some(tx_sender)) = (&app_state.game_processor, &app_state.tx_sender) {
        Some(crate::api::games::GameApiState {
            storage: app_state.storage.get_raw_storage(),
            game_processor: game_processor.clone(),
            tx_sender: tx_sender.clone(),
            finalization_waiter: app_state.finalization_waiter.clone(),
            fairness_waiter: app_state.fairness_waiter.clone(),
            websocket_manager: Some(app_state.websocket_manager.clone()),
        })
    } else {
        None
    }
}
