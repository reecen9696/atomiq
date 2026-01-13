//! Blockchain API Service
//! 
//! Production-ready HTTP API for blockchain explorer and external integrations.
//! Provides read-only access to finalized blockchain data.

pub mod errors;
pub mod handlers;
pub mod middleware;
pub mod models;
pub mod routes;
pub mod server;
pub mod storage;

pub use server::ApiServer;
