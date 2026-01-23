//! API Server with Blockchain Finalization
//! 
//! This demonstrates the complete integration of the DirectCommit blockchain
//! with the API server using the finalization guarantee system. Game endpoints
//! will wait for transaction finalization before returning results.

use atomiq::{
    api::server::{ApiServer, ApiConfig},
    api::websocket::WebSocketManager,
    factory::BlockchainFactory,
    finalization::FinalizationWaiter,
    DirectCommitHandle,
};
use atomiq::config::AtomiqConfig;
use clap::Parser;
use std::sync::Arc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "api-finalized")]
#[command(about = "Atomiq API Server with blockchain finalization (DirectCommit)", long_about = None)]
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
    #[arg(long, default_value = "finalized_node")]
    node_id: String,

    /// Network name
    #[arg(long, default_value = "devnet")]
    network: String,

    /// Enable casino game + settlement endpoints
    #[arg(long, default_value = "true")]
    enable_games: bool,

    /// Tx ingest queue capacity (bounded backpressure)
    #[arg(long, default_value = "50000")]
    tx_queue_capacity: usize,

    /// Optional pinned VRF public key hex; server refuses to start if mismatch
    #[arg(long)]
    pinned_vrf_public_key_hex: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    info!("🚀 Starting Atomiq with API + Finalization");

    // Create a high-performance blockchain with DirectCommit
    info!("📦 Creating DirectCommit blockchain...");
    let mut config = AtomiqConfig::high_performance();
    config.storage.clear_on_start = false;
    config.storage.data_directory = args.db_path.clone();
    let (_app, handle) = BlockchainFactory::create_blockchain(config).await?;
    
    // Get the DirectCommit engine from the handle
    let engine = if let Some(direct_commit_handle) = handle.as_any().downcast_ref::<DirectCommitHandle>() {
        direct_commit_handle.engine.clone()
    } else {
        return Err("Failed to get DirectCommit engine - wrong blockchain mode".into());
    };

    // IMPORTANT: Use the blockchain's actual storage.
    // The query endpoints depend on indices written during block commit.
    let storage = engine.storage();

    // Create WebSocketManager for real-time events
    info!("📡 Setting up WebSocket manager...");
    let websocket_manager = Arc::new(WebSocketManager::new(storage.clone()));
    websocket_manager.start_background_tasks();
    
    // Connect WebSocketManager to DirectCommitEngine for casino win broadcasts
    engine.set_websocket_manager(websocket_manager.clone()).await;
    info!("✅ WebSocket manager initialized and connected to blockchain");

    // Create FinalizationWaiter from the DirectCommit engine's event publisher
    info!("🔔 Setting up finalization notifications...");
    let event_publisher = engine.event_publisher();
    let finalization_waiter = Arc::new(FinalizationWaiter::new(event_publisher));
    info!("✅ Finalization system initialized with 10s timeout");
    
    // Get blockchain transaction sender from app
    info!("🔗 Connecting API to blockchain...");
    let app_clone = if let Some(direct_commit_handle) = handle.as_any().downcast_ref::<DirectCommitHandle>() {
        direct_commit_handle.app().clone()
    } else {
        return Err("Failed to get app reference".into());
    };
    let tx_sender = app_clone.read().await.transaction_sender();
    
    // Create API configuration
    let allowed_origins: Vec<String> = args
        .cors_origins
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();

    let api_host_for_log = args.host.clone();
    let api_port_for_log = args.port;

    let api_config = ApiConfig {
        host: args.host,
        port: args.port,
        allowed_origins,
        request_timeout_secs: args.timeout,
        node_id: args.node_id,
        network: args.network,
        version: env!("CARGO_PKG_VERSION").to_string(),
        tls_enabled: false,
        cert_path: None,
        key_path: None,
        enable_metrics: true,
        max_concurrent_requests: 5000,
        preload_recent_blocks: 100,
        enable_games: args.enable_games,
        tx_queue_capacity: args.tx_queue_capacity,
        pinned_vrf_public_key_hex: args.pinned_vrf_public_key_hex,
    };

    // Create API server with finalization support and inject the WebSocketManager
    info!("🌐 Starting API server with finalization...");
    let server = ApiServer::with_finalization(api_config, storage, finalization_waiter, tx_sender)
        .with_websocket_manager(websocket_manager.clone());
    
    info!("✅ System ready!");
    info!("📡 API: http://{}:{}", api_host_for_log, api_port_for_log);
    info!("🎮 Games: POST http://{}:{}/api/coinflip/play", api_host_for_log, api_port_for_log);
    info!("⏱️  Block time: 10ms (DirectCommit mode)");
    info!("🔒 Finalization: Enabled (responses wait for block commits)");
    info!("");
    info!("Try playing a game:");
    info!(r#"  curl -X POST http://127.0.0.1:8080/api/coinflip/play \
    -H "Content-Type: application/json" \
    -d '{{"bet_amount": 100, "coin_choice": "Heads", "token": "ATOM"}}'
"#);

    // Run server
    server.run().await?;

    Ok(())
}
