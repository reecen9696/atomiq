//! Example: API Server with Blockchain Finalization
//! 
//! This demonstrates the complete integration of the DirectCommit blockchain
//! with the API server using the finalization guarantee system. Game endpoints
//! will wait for transaction finalization before returning results.

use atomiq::{
    api::server::{ApiServer, ApiConfig},
    factory::BlockchainFactory,
    finalization::FinalizationWaiter,
    DirectCommitHandle,
};
use std::sync::Arc;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging only if not already initialized
    let _ = tracing_subscriber::fmt()
        .with_env_filter("atomiq=info,api_with_finalization=info")
        .try_init();

    info!("🚀 Starting Atomiq with API + Finalization");

    // Create a high-performance blockchain with DirectCommit
    info!("📦 Creating DirectCommit blockchain...");
    let (app, handle) = BlockchainFactory::create_high_performance().await?;
    
    // Get the DirectCommit engine from the handle
    let engine = if let Some(direct_commit_handle) = handle.as_any().downcast_ref::<DirectCommitHandle>() {
        direct_commit_handle.engine.clone()
    } else {
        return Err("Failed to get DirectCommit engine - wrong blockchain mode".into());
    };

    // Use the same storage as the blockchain engine so API queries reflect chain state.
    let storage = engine.storage();

    // Create FinalizationWaiter from the DirectCommit engine's event publisher
    info!("🔔 Setting up finalization notifications...");
    let event_publisher = engine.event_publisher();
    let finalization_waiter = Arc::new(FinalizationWaiter::new(event_publisher));
    
    // Create API configuration
    let api_config = ApiConfig {
        host: "127.0.0.1".to_string(),
        port: 3000,
        allowed_origins: vec!["*".to_string()],
        request_timeout_secs: 30,
        node_id: "finalized_node".to_string(),
        network: "devnet".to_string(),
        version: "1.0.0".to_string(),
        tls_enabled: false,
        cert_path: None,
        key_path: None,
        enable_metrics: true,
        max_concurrent_requests: 5000,
        preload_recent_blocks: 100,
        enable_games: true, // Enable casino games
    };

    // Create API server with finalization support
    info!("🌐 Starting API server with finalization...");
    let blockchain_tx_sender = app.transaction_sender();
    let server = ApiServer::with_finalization(
        api_config,
        storage,
        finalization_waiter,
        blockchain_tx_sender,
    );
    
    info!("✅ System ready!");
    info!("📡 API: http://127.0.0.1:3000");
    info!("🎮 Games: POST http://127.0.0.1:3000/api/coinflip/play");
    info!("⏱️  Block time: 10ms (DirectCommit mode)");
    info!("🔒 Finalization: Enabled (responses wait for block commits)");
    info!("");
    info!("Try playing a game:");
    info!(r#"  curl -X POST http://127.0.0.1:3000/api/coinflip/play \
    -H "Content-Type: application/json" \
    -d '{{"bet_amount": 100, "coin_choice": "Heads", "token": "ATOM"}}'
"#);

    // Run server
    server.run().await?;

    Ok(())
}
