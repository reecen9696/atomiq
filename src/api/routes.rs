//! Route Definitions
//! 
//! Maps URLs to handlers with type-safe routing.

use super::{
    handlers::*, 
    websocket::{websocket_handler, transaction_websocket_handler},
    monitoring::metrics_handler,
    games::{play_coinflip, get_game_result, verify_vrf, verify_game_by_id, list_supported_tokens},
};
use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

/// Build the API router with all endpoints
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
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
        .route("/metrics", get(metrics_handler))
        
        // Attach shared state
        .with_state(state)
}

/// Build the game API router with all casino game endpoints
pub fn create_game_router(game_state: Arc<crate::api::games::GameApiState>) -> Router {
    Router::new()
        // Casino game endpoints
        .route("/api/coinflip/play", post(play_coinflip))
        .route("/api/game/:id", get(get_game_result))
        .route("/api/verify/vrf", post(verify_vrf))
        .route("/api/verify/game/:id", get(verify_game_by_id))
        .route("/api/tokens", get(list_supported_tokens))
        .with_state((*game_state).clone())
}
