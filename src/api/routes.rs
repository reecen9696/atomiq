//! Route Definitions
//! 
//! Maps URLs to handlers with type-safe routing.

use super::handlers::*;
use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;

/// Build the API router with all endpoints
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        // Health check (no state needed)
        .route("/health", get(health_handler))
        
        // Status endpoint
        .route("/status", get(status_handler))
        
        // Block endpoints
        .route("/blocks", get(blocks_handler))
        .route("/block/:height", get(block_detail_handler))
        
        // Transaction endpoint
        .route("/tx/:tx_id", get(transaction_handler))
        
        // Attach shared state
        .with_state(state)
}
