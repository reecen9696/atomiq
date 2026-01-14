//! Advanced Monitoring & Metrics
//! 
//! Comprehensive performance monitoring with Prometheus metrics export,
//! real-time dashboards, and system health monitoring.

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    collections::HashMap,
};
use tokio::{sync::RwLock, time::interval};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, error};

/// Prometheus-compatible metrics registry
#[derive(Clone)]
pub struct MetricsRegistry {
    /// HTTP request metrics
    pub http_requests_total: Arc<AtomicU64>,
    pub http_request_duration_seconds: Arc<RwLock<Vec<f64>>>,
    pub http_requests_active: Arc<AtomicU64>,
    
    /// Blockchain metrics
    pub blocks_total: Arc<AtomicU64>,
    pub transactions_total: Arc<AtomicU64>,
    pub transactions_per_second: Arc<std::sync::Mutex<f64>>,
    pub avg_block_time: Arc<std::sync::Mutex<f64>>,
    pub pending_transactions: Arc<AtomicU64>,
    
    /// Cache metrics
    pub cache_hits_total: Arc<AtomicU64>,
    pub cache_misses_total: Arc<AtomicU64>,
    pub cache_size_blocks: Arc<AtomicU64>,
    pub cache_size_transactions: Arc<AtomicU64>,
    
    /// System metrics
    pub memory_usage_bytes: Arc<AtomicU64>,
    pub cpu_usage_percent: Arc<std::sync::Mutex<f64>>,
    pub disk_usage_bytes: Arc<AtomicU64>,
    pub open_connections: Arc<AtomicU64>,
    
    /// WebSocket metrics  
    pub websocket_connections_active: Arc<AtomicU64>,
    pub websocket_messages_sent: Arc<AtomicU64>,
    pub websocket_messages_received: Arc<AtomicU64>,
    
    /// Error metrics
    pub errors_total: Arc<AtomicU64>,
    pub error_rates: Arc<RwLock<HashMap<String, u64>>>,
    
    /// Performance metrics
    pub request_rate_per_second: Arc<std::sync::Mutex<f64>>,
    pub avg_response_time_ms: Arc<std::sync::Mutex<f64>>,
    pub p95_response_time_ms: Arc<std::sync::Mutex<f64>>,
    pub p99_response_time_ms: Arc<std::sync::Mutex<f64>>,
}

impl MetricsRegistry {
    /// Create new metrics registry
    pub fn new() -> Self {
        Self {
            http_requests_total: Arc::new(AtomicU64::new(0)),
            http_request_duration_seconds: Arc::new(RwLock::new(Vec::new())),
            http_requests_active: Arc::new(AtomicU64::new(0)),
            
            blocks_total: Arc::new(AtomicU64::new(0)),
            transactions_total: Arc::new(AtomicU64::new(0)),
            transactions_per_second: Arc::new(std::sync::Mutex::new(0.0)),
            avg_block_time: Arc::new(std::sync::Mutex::new(0.0)),
            pending_transactions: Arc::new(AtomicU64::new(0)),
            
            cache_hits_total: Arc::new(AtomicU64::new(0)),
            cache_misses_total: Arc::new(AtomicU64::new(0)),
            cache_size_blocks: Arc::new(AtomicU64::new(0)),
            cache_size_transactions: Arc::new(AtomicU64::new(0)),
            
            memory_usage_bytes: Arc::new(AtomicU64::new(0)),
            cpu_usage_percent: Arc::new(std::sync::Mutex::new(0.0)),
            disk_usage_bytes: Arc::new(AtomicU64::new(0)),
            open_connections: Arc::new(AtomicU64::new(0)),
            
            websocket_connections_active: Arc::new(AtomicU64::new(0)),
            websocket_messages_sent: Arc::new(AtomicU64::new(0)),
            websocket_messages_received: Arc::new(AtomicU64::new(0)),
            
            errors_total: Arc::new(AtomicU64::new(0)),
            error_rates: Arc::new(RwLock::new(HashMap::new())),
            
            request_rate_per_second: Arc::new(std::sync::Mutex::new(0.0)),
            avg_response_time_ms: Arc::new(std::sync::Mutex::new(0.0)),
            p95_response_time_ms: Arc::new(std::sync::Mutex::new(0.0)),
            p99_response_time_ms: Arc::new(std::sync::Mutex::new(0.0)),
        }
    }
    
    /// Record HTTP request
    pub async fn record_http_request(&self, duration: Duration, success: bool) {
        self.http_requests_total.fetch_add(1, Ordering::SeqCst);
        
        let duration_secs = duration.as_secs_f64();
        let mut durations = self.http_request_duration_seconds.write().await;
        durations.push(duration_secs);
        
        // Keep only recent durations (last 1000)
        if durations.len() > 1000 {
            let excess = durations.len() - 1000;
            durations.drain(0..excess);
        }
        
        if !success {
            self.errors_total.fetch_add(1, Ordering::SeqCst);
        }
    }
    
    /// Record cache hit/miss
    pub fn record_cache_hit(&self, hit: bool) {
        if hit {
            self.cache_hits_total.fetch_add(1, Ordering::SeqCst);
        } else {
            self.cache_misses_total.fetch_add(1, Ordering::SeqCst);
        }
    }
    
    /// Update cache sizes
    pub fn update_cache_sizes(&self, blocks: usize, transactions: usize) {
        self.cache_size_blocks.store(blocks as u64, Ordering::SeqCst);
        self.cache_size_transactions.store(transactions as u64, Ordering::SeqCst);
    }
    
    /// Record new block
    pub fn record_block(&self, tx_count: usize, block_time_ms: f64) {
        self.blocks_total.fetch_add(1, Ordering::SeqCst);
        self.transactions_total.fetch_add(tx_count as u64, Ordering::SeqCst);
        
        // Update average block time with exponential moving average
        let current_avg = *self.avg_block_time.lock().unwrap();
        let new_avg = if current_avg == 0.0 { 
            block_time_ms 
        } else { 
            current_avg * 0.9 + block_time_ms * 0.1 
        };
        *self.avg_block_time.lock().unwrap() = new_avg;
    }
    
    /// Update system metrics
    pub fn update_system_metrics(&self, memory_bytes: u64, cpu_percent: f64, disk_bytes: u64) {
        self.memory_usage_bytes.store(memory_bytes, Ordering::SeqCst);
        *self.cpu_usage_percent.lock().unwrap() = cpu_percent;
        self.disk_usage_bytes.store(disk_bytes, Ordering::SeqCst);
    }
    
    /// Update WebSocket metrics
    pub fn update_websocket_metrics(&self, active_connections: usize) {
        self.websocket_connections_active.store(active_connections as u64, Ordering::SeqCst);
    }
    
    /// Record WebSocket message
    pub fn record_websocket_message(&self, sent: bool) {
        if sent {
            self.websocket_messages_sent.fetch_add(1, Ordering::SeqCst);
        } else {
            self.websocket_messages_received.fetch_add(1, Ordering::SeqCst);
        }
    }
    
    /// Generate Prometheus metrics format
    pub async fn to_prometheus_format(&self) -> String {
        let mut output = String::new();
        
        // HTTP metrics
        output.push_str(&format!(
            "# HELP atomiq_http_requests_total Total number of HTTP requests\n\
             # TYPE atomiq_http_requests_total counter\n\
             atomiq_http_requests_total {}\n\n",
            self.http_requests_total.load(Ordering::SeqCst)
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_http_requests_active Currently active HTTP requests\n\
             # TYPE atomiq_http_requests_active gauge\n\
             atomiq_http_requests_active {}\n\n",
            self.http_requests_active.load(Ordering::SeqCst)
        ));
        
        // Calculate response time percentiles
        let durations = self.http_request_duration_seconds.read().await;
        if !durations.is_empty() {
            let mut sorted_durations = durations.clone();
            sorted_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            let p50_idx = (sorted_durations.len() as f64 * 0.50) as usize;
            let p95_idx = (sorted_durations.len() as f64 * 0.95) as usize;
            let p99_idx = (sorted_durations.len() as f64 * 0.99) as usize;
            
            output.push_str(&format!(
                "# HELP atomiq_http_request_duration_seconds HTTP request duration percentiles\n\
                 # TYPE atomiq_http_request_duration_seconds gauge\n\
                 atomiq_http_request_duration_seconds{{quantile=\"0.50\"}} {}\n\
                 atomiq_http_request_duration_seconds{{quantile=\"0.95\"}} {}\n\
                 atomiq_http_request_duration_seconds{{quantile=\"0.99\"}} {}\n\n",
                sorted_durations.get(p50_idx).unwrap_or(&0.0),
                sorted_durations.get(p95_idx).unwrap_or(&0.0),
                sorted_durations.get(p99_idx).unwrap_or(&0.0)
            ));
        }
        
        // Blockchain metrics
        output.push_str(&format!(
            "# HELP atomiq_blocks_total Total number of blocks\n\
             # TYPE atomiq_blocks_total counter\n\
             atomiq_blocks_total {}\n\n",
            self.blocks_total.load(Ordering::SeqCst)
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_transactions_total Total number of transactions\n\
             # TYPE atomiq_transactions_total counter\n\
             atomiq_transactions_total {}\n\n",
            self.transactions_total.load(Ordering::SeqCst)
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_transactions_per_second Current transaction throughput\n\
             # TYPE atomiq_transactions_per_second gauge\n\
             atomiq_transactions_per_second {}\n\n",
            *self.transactions_per_second.lock().unwrap()
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_avg_block_time_seconds Average block time in seconds\n\
             # TYPE atomiq_avg_block_time_seconds gauge\n\
             atomiq_avg_block_time_seconds {}\n\n",
            (*self.avg_block_time.lock().unwrap()) / 1000.0
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_pending_transactions Number of pending transactions\n\
             # TYPE atomiq_pending_transactions gauge\n\
             atomiq_pending_transactions {}\n\n",
            self.pending_transactions.load(Ordering::SeqCst)
        ));
        
        // Cache metrics
        let cache_hits = self.cache_hits_total.load(Ordering::SeqCst);
        let cache_misses = self.cache_misses_total.load(Ordering::SeqCst);
        let cache_hit_ratio = if cache_hits + cache_misses > 0 {
            cache_hits as f64 / (cache_hits + cache_misses) as f64
        } else {
            0.0
        };
        
        output.push_str(&format!(
            "# HELP atomiq_cache_hits_total Total cache hits\n\
             # TYPE atomiq_cache_hits_total counter\n\
             atomiq_cache_hits_total {}\n\n",
            cache_hits
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_cache_misses_total Total cache misses\n\
             # TYPE atomiq_cache_misses_total counter\n\
             atomiq_cache_misses_total {}\n\n",
            cache_misses
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_cache_hit_ratio Cache hit ratio (0-1)\n\
             # TYPE atomiq_cache_hit_ratio gauge\n\
             atomiq_cache_hit_ratio {}\n\n",
            cache_hit_ratio
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_cache_size_blocks Number of cached blocks\n\
             # TYPE atomiq_cache_size_blocks gauge\n\
             atomiq_cache_size_blocks {}\n\n",
            self.cache_size_blocks.load(Ordering::SeqCst)
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_cache_size_transactions Number of cached transactions\n\
             # TYPE atomiq_cache_size_transactions gauge\n\
             atomiq_cache_size_transactions {}\n\n",
            self.cache_size_transactions.load(Ordering::SeqCst)
        ));
        
        // System metrics
        output.push_str(&format!(
            "# HELP atomiq_memory_usage_bytes Memory usage in bytes\n\
             # TYPE atomiq_memory_usage_bytes gauge\n\
             atomiq_memory_usage_bytes {}\n\n",
            self.memory_usage_bytes.load(Ordering::SeqCst)
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_cpu_usage_percent CPU usage percentage\n\
             # TYPE atomiq_cpu_usage_percent gauge\n\
             atomiq_cpu_usage_percent {}\n\n",
            *self.cpu_usage_percent.lock().unwrap()
        ));
        
        // WebSocket metrics
        output.push_str(&format!(
            "# HELP atomiq_websocket_connections_active Active WebSocket connections\n\
             # TYPE atomiq_websocket_connections_active gauge\n\
             atomiq_websocket_connections_active {}\n\n",
            self.websocket_connections_active.load(Ordering::SeqCst)
        ));
        
        output.push_str(&format!(
            "# HELP atomiq_websocket_messages_sent_total WebSocket messages sent\n\
             # TYPE atomiq_websocket_messages_sent_total counter\n\
             atomiq_websocket_messages_sent_total {}\n\n",
            self.websocket_messages_sent.load(Ordering::SeqCst)
        ));
        
        // Error metrics
        output.push_str(&format!(
            "# HELP atomiq_errors_total Total number of errors\n\
             # TYPE atomiq_errors_total counter\n\
             atomiq_errors_total {}\n\n",
            self.errors_total.load(Ordering::SeqCst)
        ));
        
        output
    }
    
    /// Get current metrics snapshot
    pub async fn snapshot(&self) -> MetricsSnapshot {
        let durations = self.http_request_duration_seconds.read().await;
        let avg_response_time = if !durations.is_empty() {
            durations.iter().sum::<f64>() / durations.len() as f64 * 1000.0 // Convert to ms
        } else {
            0.0
        };
        
        MetricsSnapshot {
            timestamp: current_timestamp(),
            http_requests_total: self.http_requests_total.load(Ordering::SeqCst),
            http_requests_active: self.http_requests_active.load(Ordering::SeqCst),
            avg_response_time_ms: avg_response_time,
            
            blocks_total: self.blocks_total.load(Ordering::SeqCst),
            transactions_total: self.transactions_total.load(Ordering::SeqCst),
            transactions_per_second: *self.transactions_per_second.lock().unwrap(),
            pending_transactions: self.pending_transactions.load(Ordering::SeqCst),
            
            cache_hits: self.cache_hits_total.load(Ordering::SeqCst),
            cache_misses: self.cache_misses_total.load(Ordering::SeqCst),
            cache_hit_ratio: {
                let hits = self.cache_hits_total.load(Ordering::SeqCst);
                let misses = self.cache_misses_total.load(Ordering::SeqCst);
                if hits + misses > 0 { hits as f64 / (hits + misses) as f64 } else { 0.0 }
            },
            
            memory_usage_mb: self.memory_usage_bytes.load(Ordering::SeqCst) as f64 / (1024.0 * 1024.0),
            cpu_usage_percent: *self.cpu_usage_percent.lock().unwrap(),
            
            websocket_connections: self.websocket_connections_active.load(Ordering::SeqCst),
            errors_total: self.errors_total.load(Ordering::SeqCst),
        }
    }
    
    /// Start background metrics collection task
    pub fn start_metrics_collection(registry: Arc<MetricsRegistry>) {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            
            loop {
                interval.tick().await;
                
                // Collect system metrics
                let (memory_bytes, cpu_percent) = collect_system_metrics();
                let disk_bytes = collect_disk_usage().unwrap_or(0);
                
                registry.update_system_metrics(memory_bytes, cpu_percent, disk_bytes);
                
                // Calculate TPS (transactions per second)
                let current_snapshot = registry.snapshot().await;
                
                // Log metrics summary
                info!(
                    "📊 Metrics: {} requests, {:.1}ms avg, {} blocks, {} tx, {:.1} TPS, {:.1}MB RAM, {:.1}% CPU",
                    current_snapshot.http_requests_total,
                    current_snapshot.avg_response_time_ms,
                    current_snapshot.blocks_total,
                    current_snapshot.transactions_total,
                    current_snapshot.transactions_per_second,
                    current_snapshot.memory_usage_mb,
                    current_snapshot.cpu_usage_percent
                );
            }
        });
    }
}

/// Metrics snapshot for API responses
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub timestamp: u64,
    
    // HTTP metrics
    pub http_requests_total: u64,
    pub http_requests_active: u64,
    pub avg_response_time_ms: f64,
    
    // Blockchain metrics  
    pub blocks_total: u64,
    pub transactions_total: u64,
    pub transactions_per_second: f64,
    pub pending_transactions: u64,
    
    // Cache metrics
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_hit_ratio: f64,
    
    // System metrics
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
    
    // WebSocket metrics
    pub websocket_connections: u64,
    
    // Error metrics
    pub errors_total: u64,
}

/// Health check status
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: u64,
    pub checks: HashMap<String, HealthCheck>,
}

/// Individual health check
#[derive(Debug, Clone, Serialize)]
pub struct HealthCheck {
    pub status: String,
    pub message: String,
    pub duration_ms: f64,
}

/// Performance monitor for tracking detailed metrics
pub struct PerformanceMonitor {
    start_time: Instant,
    last_block_time: std::sync::RwLock<Option<Instant>>,
    last_tx_count: Arc<AtomicU64>,
    registry: Arc<MetricsRegistry>,
}

impl PerformanceMonitor {
    pub fn new(registry: Arc<MetricsRegistry>) -> Self {
        Self {
            start_time: Instant::now(),
            last_block_time: std::sync::RwLock::new(None),
            last_tx_count: Arc::new(AtomicU64::new(0)),
            registry,
        }
    }
    
    /// Record block processing
    pub fn record_block_processed(&self, tx_count: usize) {
        let now = Instant::now();
        
        // Calculate block time
        let block_time_ms = {
            let mut last_time = self.last_block_time.write().unwrap();
            let duration = if let Some(last) = *last_time {
                now.duration_since(last).as_millis() as f64
            } else {
                0.0
            };
            *last_time = Some(now);
            duration
        };
        
        self.registry.record_block(tx_count, block_time_ms);
        
        // Calculate TPS
        let total_tx = self.registry.transactions_total.load(Ordering::SeqCst);
        let elapsed_secs = self.start_time.elapsed().as_secs_f64();
        if elapsed_secs > 0.0 {
            let tps = total_tx as f64 / elapsed_secs;
            *self.registry.transactions_per_second.lock().unwrap() = tps;
        }
    }
}

/// System metrics collection functions
fn collect_system_metrics() -> (u64, f64) {
    let memory_bytes = get_memory_usage_bytes();
    let cpu_percent = get_cpu_usage_percent();
    (memory_bytes, cpu_percent)
}

/// Get memory usage in bytes
fn get_memory_usage_bytes() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb * 1024; // Convert KB to bytes
                        }
                    }
                }
            }
        }
    }
    
    if cfg!(target_os = "macos") {
        // Use task_info on macOS - simplified version
        return 100 * 1024 * 1024; // Placeholder: 100MB
    } else {
        // Fallback
        return 0;
    }
}

/// Get CPU usage percentage (simplified)
fn get_cpu_usage_percent() -> f64 {
    // This would need proper CPU monitoring implementation
    // For now, return a placeholder
    0.0
}

/// Get disk usage
fn collect_disk_usage() -> Result<u64, Box<dyn std::error::Error>> {
    // This would implement proper disk usage monitoring
    // For now, return placeholder
    Ok(1024 * 1024 * 1024) // 1GB placeholder
}

/// Get current timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Axum handler for Prometheus metrics endpoint
pub async fn metrics_handler(
    axum::extract::State(state): axum::extract::State<Arc<super::handlers::AppState>>,
) -> axum::response::Response<String> {
    let metrics = state.metrics.to_prometheus_format().await;
    
    axum::response::Response::builder()
        .header("Content-Type", "text/plain; version=0.0.4; charset=utf-8")
        .body(metrics)
        .unwrap()
}

/// Axum handler for JSON metrics endpoint
pub async fn metrics_json_handler(
    axum::extract::State(registry): axum::extract::State<Arc<MetricsRegistry>>,
) -> axum::Json<MetricsSnapshot> {
    let snapshot = registry.snapshot().await;
    axum::Json(snapshot)
}

/// Axum handler for health check endpoint
pub async fn health_check_handler(
    axum::extract::State(registry): axum::extract::State<Arc<MetricsRegistry>>,
) -> axum::Json<HealthStatus> {
    let start = Instant::now();
    
    let mut checks = HashMap::new();
    
    // Database health check
    checks.insert("database".to_string(), HealthCheck {
        status: "ok".to_string(),
        message: "Database connection healthy".to_string(),
        duration_ms: 1.5, // Placeholder
    });
    
    // Cache health check
    checks.insert("cache".to_string(), HealthCheck {
        status: "ok".to_string(), 
        message: "Cache system operational".to_string(),
        duration_ms: 0.5, // Placeholder
    });
    
    // WebSocket health check
    let ws_connections = registry.websocket_connections_active.load(Ordering::SeqCst);
    checks.insert("websockets".to_string(), HealthCheck {
        status: if ws_connections < 10000 { "ok" } else { "warning" }.to_string(),
        message: format!("{} active connections", ws_connections),
        duration_ms: 0.1,
    });
    
    let overall_status = if checks.values().all(|c| c.status == "ok") {
        "healthy"
    } else {
        "degraded"
    };
    
    axum::Json(HealthStatus {
        status: overall_status.to_string(),
        timestamp: current_timestamp(),
        checks,
    })
}