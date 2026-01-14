//! Route Definitions
//! 
//! Maps URLs to handlers with type-safe routing.

use super::{
    handlers::*, 
    websocket::{websocket_handler, transaction_websocket_handler},
    monitoring::metrics_handler,
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
