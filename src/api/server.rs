//! API Server
//! 
//! High-performance server setup optimized for concurrent requests.

use super::{
    handlers::AppState,
    middleware::{create_cors_layer, request_id_middleware, response_time_middleware},
    routes::create_router,
    storage::ApiStorage,
    websocket::WebSocketManager,
    monitoring,
};
use crate::{
    storage::OptimizedStorage,
    blockchain_game_processor::BlockchainGameProcessor,
    fairness::{FairnessWaiter, FairnessWorker},
    common::types::Transaction,
    finalization::FinalizationWaiter,
    TransactionSender,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{signal, sync::mpsc};
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};
use tracing::{info, warn};


/// API server configuration
#[derive(Debug, Clone)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    pub allowed_origins: Vec<String>,
    pub request_timeout_secs: u64,
    pub node_id: String,
    pub network: String,
    pub version: String,
    pub tls_enabled: bool,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    
    // High-performance settings
    pub max_concurrent_requests: usize,
    pub enable_metrics: bool,
    pub preload_recent_blocks: usize,
    
    // Game settings
    pub enable_games: bool,

    // Backpressure settings
    pub tx_queue_capacity: usize,

    /// Optional pinned VRF public key (hex). If set, the node will refuse to start unless
    /// the persistent keypair matches this value.
    pub pinned_vrf_public_key_hex: Option<String>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            allowed_origins: vec!["*".to_string()],
            request_timeout_secs: 30,
            node_id: "atomiq-node-1".to_string(),
            network: "atomiq-mainnet".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            tls_enabled: false,
            cert_path: None,
            key_path: None,
            
            // High-performance defaults
            max_concurrent_requests: 50_000, // Support 50K concurrent requests
            enable_metrics: true,
            preload_recent_blocks: 1_000, // Cache last 1K blocks on startup
            
            // Game defaults
            enable_games: true, // Enable casino games by default

            // Backpressure defaults
            tx_queue_capacity: 50_000,

            pinned_vrf_public_key_hex: None,
        }
    }
}

/// High-performance API server
pub struct ApiServer {
    config: ApiConfig,
    storage: Arc<OptimizedStorage>,
    finalization_waiter: Option<Arc<FinalizationWaiter>>,
    blockchain_tx_sender: Option<TransactionSender>,
}

impl ApiServer {
    pub fn new(config: ApiConfig, storage: Arc<OptimizedStorage>) -> Self {
        Self { 
            config, 
            storage,
            finalization_waiter: None,
            blockchain_tx_sender: None,
        }
    }

    /// Create ApiServer with finalization support
    pub fn with_finalization(
        config: ApiConfig, 
        storage: Arc<OptimizedStorage>,
        finalization_waiter: Arc<FinalizationWaiter>,
        blockchain_tx_sender: TransactionSender,
    ) -> Self {
        Self {
            config,
            storage,
            finalization_waiter: Some(finalization_waiter),
            blockchain_tx_sender: Some(blockchain_tx_sender),
        }
    }

    /// Start the API server
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // Initialize enhanced tracing for performance monitoring
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "atomiq_api=info,tower_http=info,atomiq::api=debug".into())
            )
            .init();

        info!("🚀 Starting High-Performance Atomiq API Server");
        info!("   Max concurrent requests: {}", self.config.max_concurrent_requests);
        info!("   Tx ingest queue capacity: {}", self.config.tx_queue_capacity);

        if self.config.tls_enabled {
            warn!("⚠️  HTTPS/TLS support is planned but not yet implemented");
            warn!("   Use a reverse proxy (Nginx/Caddy) for production HTTPS");
            warn!("   Continuing with HTTP...");
        }

        self.run_http().await
    }

    /// Run HTTP server with performance optimizations
    async fn run_http(self) -> Result<(), Box<dyn std::error::Error>> {
        let app = self.create_app();
        let addr = self.get_socket_addr()?;

        info!("🌐 Starting Atomiq API Server (HTTP)");
        info!("   Listen: http://{}", addr);
        self.log_server_info();

        let listener = tokio::net::TcpListener::bind(addr).await?;
        
        info!("✅ High-Performance API Server running");
        info!("🔥 Ready to handle {} concurrent requests", self.config.max_concurrent_requests);

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        info!("🛑 API Server stopped gracefully");
        Ok(())
    }

    /// Create the application with optimized middleware stack
    fn create_app(&self) -> axum::Router {
        let websocket_manager = Arc::new(WebSocketManager::new(self.storage.clone()));
        let metrics = Arc::new(monitoring::MetricsRegistry::new());
        
        // Initialize game components if enabled
        let (game_processor, tx_sender) = if self.config.enable_games {
            info!("🎮 Initializing casino game components...");
            
            // Load-or-create a persistent VRF keypair seed from RocksDB.
            // This keeps `vrf.public_key` stable across restarts.
            let processor = Arc::new(
                BlockchainGameProcessor::new_with_persistent_key(self.storage.clone())
                    .expect("Failed to initialize persistent VRF key"),
            );

            if let Some(pinned_hex) = &self.config.pinned_vrf_public_key_hex {
                let pinned = hex::decode(pinned_hex.trim_start_matches("0x"))
                    .expect("Invalid pinned_vrf_public_key_hex (must be hex)");
                if pinned != processor.get_public_key() {
                    panic!(
                        "Pinned VRF public key does not match persistent key (configured={}, actual={})",
                        pinned_hex,
                        hex::encode(processor.get_public_key())
                    );
                }
                info!("🔐 Pinned VRF public key verified: {}", pinned_hex);
            }
            
            // Use real blockchain connection if available, otherwise create dummy channel
            let sender = if let Some(blockchain_sender) = &self.blockchain_tx_sender {
                info!("🔗 Connected to blockchain for transaction submission");
                let sender_clone = blockchain_sender.clone();
                let (tx, mut rx) = mpsc::channel::<Transaction>(self.config.tx_queue_capacity);
                
                // Forward transactions to blockchain
                tokio::spawn(async move {
                    while let Some(transaction) = rx.recv().await {
                        if let Err(e) = sender_clone.send(transaction) {
                            tracing::error!("Failed to submit transaction to blockchain: {}", e);
                        }
                    }
                });
                
                tx
            } else {
                info!("⚠️  No blockchain connection - transactions will not be processed");
                let (sender, mut receiver) = mpsc::channel::<Transaction>(self.config.tx_queue_capacity);
                
                // Dummy handler that discards transactions
                tokio::spawn(async move {
                    while let Some(_transaction) = receiver.recv().await {
                        // Transactions are discarded in standalone mode
                    }
                });
                
                sender
            };
            
            info!("✅ Casino game components initialized");
            (Some(processor), Some(sender))
        } else {
            info!("⏭️ Casino games disabled");
            (None, None)
        };

        // Fairness pipeline: compute+persist outcomes asynchronously after commit.
        // This keeps commit latency low while allowing the API to wait for fairness persistence.
        let fairness_waiter = if self.config.enable_games {
            Some(Arc::new(FairnessWaiter::new(self.storage.clone())))
        } else {
            None
        };

        if self.config.enable_games {
            if let (Some(processor), Some(finalization_waiter), Some(waiter)) = (
                game_processor.clone(),
                self.finalization_waiter.clone(),
                fairness_waiter.clone(),
            ) {
                tracing::info!("🧮 Starting fairness worker (async persistence)");

                // Bounded concurrency: keep CPU use predictable under load.
                let max_concurrency = 64;
                let publisher = waiter.publisher();

                let _worker = FairnessWorker::spawn(
                    self.storage.clone(),
                    processor,
                    finalization_waiter,
                    publisher,
                    max_concurrency,
                );
            } else if game_processor.is_some() {
                tracing::warn!("Fairness worker not started (missing finalization support)");
            }
        }
        
        let state = Arc::new(AppState {
            storage: ApiStorage::new(self.storage.clone()),
            node_id: self.config.node_id.clone(),
            network: self.config.network.clone(),
            version: self.config.version.clone(),
            websocket_manager,
            metrics,
            game_processor,
            tx_sender,
            finalization_waiter: self.finalization_waiter.clone(),
            fairness_waiter,
        });

        create_router(state)
            // Request ID middleware (first for tracing)
            .layer(axum::middleware::from_fn(request_id_middleware))

            // Response time header for frontend diagnostics
            .layer(axum::middleware::from_fn(response_time_middleware))
            
            // CORS layer (before timeout to handle preflight)
            .layer(create_cors_layer(self.config.allowed_origins.clone()))
            
            // Timeout layer (shorter for high-performance requirements)
            .layer(TimeoutLayer::new(Duration::from_secs(self.config.request_timeout_secs)))
            
            // Tracing layer (last for complete request tracing)
            .layer(TraceLayer::new_for_http())
    }

    /// Get socket address from config
    fn get_socket_addr(&self) -> Result<SocketAddr, Box<dyn std::error::Error>> {
        Ok(SocketAddr::from((
            self.config.host.parse::<std::net::IpAddr>()?,
            self.config.port,
        )))
    }

    /// Log server information
    fn log_server_info(&self) {
        info!("📋 Server Configuration:");
        info!("   Network: {}", self.config.network);
        info!("   Version: {}", self.config.version);
        info!("   Node ID: {}", self.config.node_id);
        info!("   CORS: {:?}", self.config.allowed_origins);
        info!("   Request timeout: {}s", self.config.request_timeout_secs);
        info!("   Metrics enabled: {}", self.config.enable_metrics);
        
        if self.config.tls_enabled {
            info!("   TLS: configured (reverse proxy recommended)");
        }
        
        info!("🔧 Performance Settings:");
        info!("   Max concurrent requests: {}", self.config.max_concurrent_requests);
        info!("   Preloaded blocks: {}", self.config.preload_recent_blocks);
        
        info!("📊 Available endpoints:");
        info!("   GET  /health          - Health check");
        info!("   GET  /status          - Node status");
        info!("   GET  /blocks          - Block list");
        info!("   GET  /block/:height   - Block details");
        info!("   GET  /tx/:id          - Transaction lookup");
        
        if self.config.enable_games {
            info!("🎮 Casino game endpoints:");
            info!("   POST /api/coinflip/play     - Play coinflip");
            info!("   GET  /api/game/:id          - Get game result");
            info!("   POST /api/verify/vrf        - Verify VRF proof");
            info!("   GET  /api/verify/game/:id   - Verify game by ID");
            info!("   GET  /api/tokens            - List supported tokens");
        }
    }
}

/// Wait for shutdown signal
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received Ctrl+C signal");
        },
        _ = terminate => {
            info!("Received terminate signal");
        },
    }
}
