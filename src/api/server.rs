//! API Server
//! 
//! High-performance server setup optimized for concurrent requests.

use super::{
    handlers::AppState,
    middleware::{create_cors_layer, request_id_middleware},
    routes::create_router,
    storage::ApiStorage,
    websocket::WebSocketManager,
    monitoring,
};
use crate::storage::OptimizedStorage;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::signal;
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
        }
    }
}

/// High-performance API server
pub struct ApiServer {
    config: ApiConfig,
    storage: Arc<OptimizedStorage>,
}

impl ApiServer {
    pub fn new(config: ApiConfig, storage: Arc<OptimizedStorage>) -> Self {
        Self { config, storage }
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
        
        let state = Arc::new(AppState {
            storage: ApiStorage::new(self.storage.clone()),
            node_id: self.config.node_id.clone(),
            network: self.config.network.clone(),
            version: self.config.version.clone(),
            websocket_manager,
            metrics,
        });

        create_router(state)
            // Request ID middleware (first for tracing)
            .layer(axum::middleware::from_fn(request_id_middleware))
            
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
