//! Atomiq API Server Binary
//! 
//! Standalone HTTP API for blockchain explorer and external integrations.

mod api;
mod config;
mod errors;
mod storage;

use api::server::{ApiConfig, ApiServer};
use clap::Parser;
use config::StorageConfig;
use storage::OptimizedStorage;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "atomiq-api")]
#[command(about = "Atomiq Blockchain API Server", long_about = None)]
struct Args {
    /// API server host
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// API server port
    #[arg(long, default_value = "8080")]
    port: u16,

    /// Database directory
    #[arg(long, default_value = "./DB/blockchain_data")]
    db_path: String,

    /// Allowed CORS origins (comma-separated, use * for all)
    #[arg(long, default_value = "*")]
    cors_origins: String,

    /// Request timeout in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,

    /// Node ID
    #[arg(long, default_value = "atomiq-node-1")]
    node_id: String,

    /// Network name
    #[arg(long, default_value = "atomiq-mainnet")]
    network: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Open database (read-only mode)
    let storage_config = StorageConfig {
        data_directory: args.db_path.clone(),
        clear_on_start: false,
        ..Default::default()
    };

    println!("📂 Opening blockchain database: {}", storage_config.data_directory);
    let storage = Arc::new(OptimizedStorage::new_with_config(&storage_config)?);
    println!("✅ Database opened successfully");

    // Parse CORS origins
    let allowed_origins: Vec<String> = args.cors_origins
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    // Create API configuration
    let api_config = ApiConfig {
        host: args.host,
        port: args.port,
        allowed_origins,
        request_timeout_secs: args.timeout,
        node_id: args.node_id,
        network: args.network,
        version: env!("CARGO_PKG_VERSION").to_string(),
    };

    // Create and run server
    let server = ApiServer::new(api_config, storage);
    server.run().await?;

    Ok(())
}
