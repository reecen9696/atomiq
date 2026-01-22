//! Blockchain API Service
//! 
//! Production-ready HTTP API for blockchain explorer and external integrations.
//! Provides read-only access to finalized blockchain data with optimized
//! concurrent request handling, real-time WebSocket updates, and advanced caching.

pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod server;
pub mod storage;
pub mod websocket;
pub mod cache;
pub mod games;
pub mod games_wrappers;
pub mod settlement;

// High-performance modules (Stage 2 features)
pub mod concurrent_handler;
pub mod lock_free_storage;
pub mod monitoring;
pub mod security;
pub mod load_balancing;

pub use server::ApiServer;
