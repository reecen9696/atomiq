//! API Server with Blockchain Finalization
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
    info!("🚀 Starting Atomiq with API + Finalization");

    // Create a high-performance blockchain with DirectCommit
    info!("📦 Creating DirectCommit blockchain...");
    let (app, handle) = BlockchainFactory::create_high_performance_persistent().await?;
    
    // Get the DirectCommit engine from the handle
    let engine = if let Some(direct_commit_handle) = handle.as_any().downcast_ref::<DirectCommitHandle>() {
        direct_commit_handle.engine.clone()
    } else {
        return Err("Failed to get DirectCommit engine - wrong blockchain mode".into());
    };

    // IMPORTANT: Use the blockchain's actual storage.
    // The query endpoints depend on indices written during block commit.
    let storage = engine.storage();

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
        tx_queue_capacity: 50_000,
        pinned_vrf_public_key_hex: None,
    };

    // Create API server with finalization support
    info!("🌐 Starting API server with finalization...");
    let server = ApiServer::with_finalization(api_config, storage, finalization_waiter, tx_sender);
    
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
