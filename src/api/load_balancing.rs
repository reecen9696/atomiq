//! Load Balancing & Horizontal Scaling Support
//!
//! Features for running multiple API server instances:
//! - Health check endpoints for load balancers  
//! - Session affinity and sticky sessions
//! - Distributed rate limiting coordination
//! - Instance discovery and registration
//! - Graceful shutdown coordination

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    net::SocketAddr,
};
use tokio::{
    sync::{broadcast, RwLock},
    time::interval,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};
use uuid::Uuid;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json, response::Response,
};

/// Configuration for load balancing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancerConfig {
    /// Unique instance ID
    pub instance_id: String,
    
    /// Instance name for identification
    pub instance_name: String,
    
    /// Address this instance is listening on
    pub listen_address: SocketAddr,
    
    /// Health check interval in seconds
    pub health_check_interval_secs: u64,
    
    /// Health check timeout in seconds
    pub health_check_timeout_secs: u64,
    
    /// Maximum unhealthy duration before marking as failed
    pub max_unhealthy_duration_secs: u64,
    
    /// Enable distributed rate limiting
    pub enable_distributed_rate_limiting: bool,
    
    /// Redis URL for distributed coordination (optional)
    pub redis_url: Option<String>,
    
    /// Instance weight for load balancing (1-100)
    pub instance_weight: u8,
    
    /// Maximum concurrent connections this instance can handle
    pub max_connections: u32,
    
    /// Preferred instance for sticky sessions
    pub enable_session_affinity: bool,
}

impl Default for LoadBalancerConfig {
    fn default() -> Self {
        Self {
            instance_id: Uuid::new_v4().to_string(),
            instance_name: format!("atomiq-api-{}", hostname()),
            listen_address: "0.0.0.0:8080".parse().unwrap(),
            health_check_interval_secs: 30,
            health_check_timeout_secs: 5,
            max_unhealthy_duration_secs: 180, // 3 minutes
            enable_distributed_rate_limiting: false,
            redis_url: None,
            instance_weight: 100,
            max_connections: 10_000,
            enable_session_affinity: false,
        }
    }
}

/// Instance health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceHealth {
    pub status: HealthStatus,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub active_connections: u32,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub requests_per_second: f64,
    pub error_rate_percent: f64,
    pub version: String,
    pub features: Vec<String>,
}

/// Health status enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Maintenance,
}

/// Load balancer manager
#[derive(Clone)]
pub struct LoadBalancerManager {
    config: LoadBalancerConfig,
    
    /// This instance's health status
    local_health: Arc<RwLock<InstanceHealth>>,
    
    /// Known peer instances
    peer_instances: Arc<RwLock<HashMap<String, PeerInstance>>>,
    
    /// Graceful shutdown signal
    shutdown_signal: broadcast::Sender<()>,
    
    /// Instance start time
    start_time: Instant,
    
    /// Connection counter
    active_connections: Arc<AtomicU64>,
    
    /// Request statistics
    total_requests: Arc<AtomicU64>,
    successful_requests: Arc<AtomicU64>,
    
    /// Maintenance mode
    maintenance_mode: Arc<AtomicBool>,
}

/// Peer instance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInstance {
    pub id: String,
    pub name: String,
    pub address: SocketAddr,
    pub health: InstanceHealth,
    pub last_seen: u64,
    pub weight: u8,
}

impl LoadBalancerManager {
    /// Create new load balancer manager
    pub fn new(config: LoadBalancerConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(16);
        
        let initial_health = InstanceHealth {
            status: HealthStatus::Healthy,
            timestamp: current_timestamp(),
            uptime_seconds: 0,
            active_connections: 0,
            cpu_usage_percent: 0.0,
            memory_usage_mb: 0.0,
            requests_per_second: 0.0,
            error_rate_percent: 0.0,
            version: env!("CARGO_PKG_VERSION").to_string(),
            features: vec![
                "websockets".to_string(),
                "rate_limiting".to_string(),
                "caching".to_string(),
                "metrics".to_string(),
            ],
        };
        
        Self {
            config,
            local_health: Arc::new(RwLock::new(initial_health)),
            peer_instances: Arc::new(RwLock::new(HashMap::new())),
            shutdown_signal: shutdown_tx,
            start_time: Instant::now(),
            active_connections: Arc::new(AtomicU64::new(0)),
            total_requests: Arc::new(AtomicU64::new(0)),
            successful_requests: Arc::new(AtomicU64::new(0)),
            maintenance_mode: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Start background tasks
    pub fn start_background_tasks(&self) {
        self.start_health_update_task();
        self.start_peer_discovery_task();
        self.start_metrics_collection_task();
    }
    
    /// Get detailed health check for load balancer
    pub async fn get_health_check(&self) -> LoadBalancerHealthCheck {
        let local_health = self.local_health.read().await;
        let peer_instances = self.peer_instances.read().await;
        
        let healthy_peers = peer_instances.values()
            .filter(|peer| matches!(peer.health.status, HealthStatus::Healthy))
            .count();
        
        let total_peers = peer_instances.len();
        
        LoadBalancerHealthCheck {
            instance_id: self.config.instance_id.clone(),
            instance_name: self.config.instance_name.clone(),
            status: local_health.status.clone(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            active_connections: self.active_connections.load(Ordering::Relaxed) as u32,
            max_connections: self.config.max_connections,
            connection_capacity_percent: {
                let active = self.active_connections.load(Ordering::Relaxed) as f64;
                let max = self.config.max_connections as f64;
                (active / max * 100.0).min(100.0)
            },
            requests_per_second: local_health.requests_per_second,
            error_rate_percent: local_health.error_rate_percent,
            memory_usage_mb: local_health.memory_usage_mb,
            cpu_usage_percent: local_health.cpu_usage_percent,
            maintenance_mode: self.maintenance_mode.load(Ordering::Relaxed),
            peer_count: total_peers,
            healthy_peer_count: healthy_peers,
            version: local_health.version.clone(),
            timestamp: current_timestamp(),
        }
    }
    
    /// Lightweight health check for frequent load balancer polling
    pub async fn get_simple_health(&self) -> SimpleHealthResponse {
        let is_healthy = !self.maintenance_mode.load(Ordering::Relaxed) &&
                        self.active_connections.load(Ordering::Relaxed) < self.config.max_connections as u64;
        
        SimpleHealthResponse {
            status: if is_healthy { "up" } else { "down" }.to_string(),
            timestamp: current_timestamp(),
            connections: self.active_connections.load(Ordering::Relaxed) as u32,
            max_connections: self.config.max_connections,
        }
    }
    
    /// Enable/disable maintenance mode
    pub async fn set_maintenance_mode(&self, enabled: bool) {
        self.maintenance_mode.store(enabled, Ordering::Relaxed);
        
        let mut health = self.local_health.write().await;
        health.status = if enabled {
            HealthStatus::Maintenance
        } else {
            HealthStatus::Healthy
        };
        
        if enabled {
            warn!("Instance {} entering maintenance mode", self.config.instance_id);
        } else {
            info!("Instance {} exiting maintenance mode", self.config.instance_id);
        }
    }
    
    /// Record connection start
    pub fn connection_started(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record connection end
    pub fn connection_ended(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }
    
    /// Record request (for RPS calculation)
    pub fn record_request(&self, success: bool) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        if success {
            self.successful_requests.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Get instance statistics for load balancer
    pub async fn get_instance_stats(&self) -> InstanceStats {
        let peer_instances = self.peer_instances.read().await;
        let local_health = self.local_health.read().await;
        
        InstanceStats {
            instance_id: self.config.instance_id.clone(),
            instance_name: self.config.instance_name.clone(),
            weight: self.config.instance_weight,
            active_connections: self.active_connections.load(Ordering::Relaxed) as u32,
            max_connections: self.config.max_connections,
            total_requests: self.total_requests.load(Ordering::Relaxed),
            successful_requests: self.successful_requests.load(Ordering::Relaxed),
            uptime_seconds: self.start_time.elapsed().as_secs(),
            cpu_usage_percent: local_health.cpu_usage_percent,
            memory_usage_mb: local_health.memory_usage_mb,
            peer_count: peer_instances.len(),
            features: local_health.features.clone(),
        }
    }
    
    /// Start graceful shutdown
    pub async fn initiate_graceful_shutdown(&self) {
        info!("Initiating graceful shutdown for instance {}", self.config.instance_id);
        
        // Enter maintenance mode
        self.set_maintenance_mode(true).await;
        
        // Send shutdown signal
        let _ = self.shutdown_signal.send(());
        
        // Wait for connections to drain
        let mut wait_time = 0;
        let max_wait = 30; // 30 seconds max
        
        while self.active_connections.load(Ordering::Relaxed) > 0 && wait_time < max_wait {
            info!("Waiting for {} connections to drain...", 
                  self.active_connections.load(Ordering::Relaxed));
            tokio::time::sleep(Duration::from_secs(1)).await;
            wait_time += 1;
        }
        
        info!("Graceful shutdown completed");
    }
    
    /// Start health update task
    fn start_health_update_task(&self) {
        let manager = self.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(manager.config.health_check_interval_secs));
            
            loop {
                interval.tick().await;
                
                // Update health metrics
                let (cpu_percent, memory_mb) = collect_system_metrics();
                let uptime = manager.start_time.elapsed().as_secs();
                let active_conns = manager.active_connections.load(Ordering::Relaxed) as u32;
                
                // Calculate RPS (requests per second) over last interval
                let total_requests = manager.total_requests.load(Ordering::Relaxed);
                let successful_requests = manager.successful_requests.load(Ordering::Relaxed);
                let error_rate = if total_requests > 0 {
                    ((total_requests - successful_requests) as f64 / total_requests as f64) * 100.0
                } else {
                    0.0
                };
                
                // Determine health status
                let status = if manager.maintenance_mode.load(Ordering::Relaxed) {
                    HealthStatus::Maintenance
                } else if active_conns > manager.config.max_connections * 95 / 100 {
                    HealthStatus::Degraded
                } else if cpu_percent > 90.0 || memory_mb > 8192.0 || error_rate > 5.0 {
                    HealthStatus::Degraded
                } else {
                    HealthStatus::Healthy
                };
                
                let mut health = manager.local_health.write().await;
                health.status = status;
                health.timestamp = current_timestamp();
                health.uptime_seconds = uptime;
                health.active_connections = active_conns;
                health.cpu_usage_percent = cpu_percent;
                health.memory_usage_mb = memory_mb;
                health.error_rate_percent = error_rate;
                
                // Log health summary
                debug!(
                    "Health update: {:?}, connections: {}/{}, CPU: {:.1}%, Memory: {:.1}MB",
                    health.status, active_conns, manager.config.max_connections, cpu_percent, memory_mb
                );
            }
        });
    }
    
    /// Start peer discovery task
    fn start_peer_discovery_task(&self) {
        let _manager = self.clone();
        
        tokio::spawn(async move {
            let mut _interval = interval(Duration::from_secs(60)); // Every minute
            
            // TODO: Implement peer discovery via service registry or DNS
            // For now, this is a placeholder
            loop {
                _interval.tick().await;
                
                // In a real implementation, this would:
                // 1. Query service registry for other instances
                // 2. Perform health checks on peer instances
                // 3. Update peer_instances map
                
                debug!("Peer discovery task running (placeholder)");
            }
        });
    }
    
    /// Start metrics collection task  
    fn start_metrics_collection_task(&self) {
        let manager = self.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10)); // Every 10 seconds
            let mut last_request_count = 0u64;
            
            loop {
                interval.tick().await;
                
                let current_requests = manager.total_requests.load(Ordering::Relaxed);
                let rps = (current_requests - last_request_count) as f64 / 10.0;
                last_request_count = current_requests;
                
                // Update RPS in health
                let mut health = manager.local_health.write().await;
                health.requests_per_second = rps;
            }
        });
    }
}

/// Detailed health check response
#[derive(Debug, Serialize)]
pub struct LoadBalancerHealthCheck {
    pub instance_id: String,
    pub instance_name: String,
    pub status: HealthStatus,
    pub uptime_seconds: u64,
    pub active_connections: u32,
    pub max_connections: u32,
    pub connection_capacity_percent: f64,
    pub requests_per_second: f64,
    pub error_rate_percent: f64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    pub maintenance_mode: bool,
    pub peer_count: usize,
    pub healthy_peer_count: usize,
    pub version: String,
    pub timestamp: u64,
}

/// Simple health response for load balancer
#[derive(Debug, Serialize)]
pub struct SimpleHealthResponse {
    pub status: String,
    pub timestamp: u64,
    pub connections: u32,
    pub max_connections: u32,
}

/// Instance statistics
#[derive(Debug, Serialize)]
pub struct InstanceStats {
    pub instance_id: String,
    pub instance_name: String,
    pub weight: u8,
    pub active_connections: u32,
    pub max_connections: u32,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub uptime_seconds: u64,
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub peer_count: usize,
    pub features: Vec<String>,
}

/// Query parameters for health endpoints
#[derive(Debug, Deserialize)]
pub struct HealthQuery {
    #[serde(default)]
    pub format: HealthFormat,
}

/// Health response format
#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthFormat {
    Simple,
    Detailed,
}

impl Default for HealthFormat {
    fn default() -> Self {
        HealthFormat::Simple
    }
}

/// Load balancer health endpoint handler
pub async fn lb_health_handler(
    Query(params): Query<HealthQuery>,
    State(manager): State<Arc<LoadBalancerManager>>,
) -> Result<Response<String>, StatusCode> {
    match params.format {
        HealthFormat::Simple => {
            let health = manager.get_simple_health().await;
            let json = serde_json::to_string(&health).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            
            let status_code = if health.status == "up" {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            
            Ok(axum::response::Response::builder()
                .status(status_code)
                .header("Content-Type", "application/json")
                .body(json)
                .unwrap())
        }
        HealthFormat::Detailed => {
            let health = manager.get_health_check().await;
            let json = serde_json::to_string(&health).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            
            let status_code = match health.status {
                HealthStatus::Healthy => StatusCode::OK,
                HealthStatus::Degraded => StatusCode::OK, // Still serving traffic
                HealthStatus::Unhealthy | HealthStatus::Maintenance => StatusCode::SERVICE_UNAVAILABLE,
            };
            
            Ok(axum::response::Response::builder()
                .status(status_code)
                .header("Content-Type", "application/json")
                .body(json)
                .unwrap())
        }
    }
}

/// Instance stats endpoint handler
pub async fn instance_stats_handler(
    State(manager): State<Arc<LoadBalancerManager>>,
) -> Json<InstanceStats> {
    let stats = manager.get_instance_stats().await;
    Json(stats)
}

/// Maintenance mode control endpoint
pub async fn maintenance_mode_handler(
    Query(params): Query<MaintenanceQuery>,
    State(manager): State<Arc<LoadBalancerManager>>,
) -> Result<Json<MaintenanceResponse>, StatusCode> {
    match params.action.as_deref() {
        Some("enable") => {
            manager.set_maintenance_mode(true).await;
            Ok(Json(MaintenanceResponse {
                maintenance_mode: true,
                message: "Maintenance mode enabled".to_string(),
            }))
        }
        Some("disable") => {
            manager.set_maintenance_mode(false).await;
            Ok(Json(MaintenanceResponse {
                maintenance_mode: false,
                message: "Maintenance mode disabled".to_string(),
            }))
        }
        _ => {
            let mode = manager.maintenance_mode.load(Ordering::Relaxed);
            Ok(Json(MaintenanceResponse {
                maintenance_mode: mode,
                message: format!("Maintenance mode is {}", if mode { "enabled" } else { "disabled" }),
            }))
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct MaintenanceQuery {
    pub action: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MaintenanceResponse {
    pub maintenance_mode: bool,
    pub message: String,
}

/// Utility functions
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("HOST"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn collect_system_metrics() -> (f64, f64) {
    // Placeholder implementation
    // In production, this would collect real metrics
    (15.5, 1024.0) // CPU %, Memory MB
}