//! API Server
//! 
//! Main server setup with graceful shutdown and configuration.

use super::{
    handlers::AppState,
    middleware::{create_cors_layer, request_id_middleware},
    routes::create_router,
    storage::ApiStorage,
};
use crate::storage::OptimizedStorage;
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::signal;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};
use tracing::info;

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
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            allowed_origins: vec!["*".to_string()], // Allow all in dev
            request_timeout_secs: 30,
            node_id: "atomiq-node-1".to_string(),
            network: "atomiq-mainnet".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Main API server
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
        // Initialize tracing
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "atomiq_api=info,tower_http=info".into())
            )
            .init();

        // Create application state
        let state = Arc::new(AppState {
            storage: ApiStorage::new(self.storage.clone()),
            node_id: self.config.node_id.clone(),
            network: self.config.network.clone(),
            version: self.config.version.clone(),
        });

        // Build router with middleware
        let app = create_router(state)
            .layer(axum::middleware::from_fn(request_id_middleware))
            .layer(create_cors_layer(self.config.allowed_origins.clone()))
            .layer(TimeoutLayer::new(Duration::from_secs(self.config.request_timeout_secs)))
            .layer(TraceLayer::new_for_http());

        // Bind address
        let addr = SocketAddr::from((
            self.config.host.parse::<std::net::IpAddr>()?,
            self.config.port,
        ));

        info!("🚀 Atomiq API Server starting");
        info!("   Listen: http://{}", addr);
        info!("   Network: {}", self.config.network);
        info!("   Version: {}", self.config.version);
        info!("   CORS: {:?}", self.config.allowed_origins);
        info!("   Request ID tracking: enabled");

        // Create server
        let listener = tokio::net::TcpListener::bind(addr).await?;
        
        info!("✅ API Server running");

        // Run with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;

        info!("🛑 API Server stopped");
        Ok(())
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
