//! WebSocket Support for Real-time Blockchain Events
//!
//! Provides real-time updates for:
//! - New block notifications
//! - Transaction confirmations
//! - Network status changes
//! - Performance metrics

use super::handlers::AppState;
use crate::storage::OptimizedStorage;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{atomic::AtomicU64, Arc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    sync::{broadcast, RwLock, Mutex},
    time::interval,
};
use tracing::{debug, error, info, warn};

/// WebSocket event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsEvent {
    /// New block created
    #[serde(rename = "new_block")]
    NewBlock {
        height: u64,
        hash: String,
        tx_count: usize,
        timestamp: u64,
        transactions: Vec<String>, // Transaction IDs
    },
    
    /// Transaction confirmed
    #[serde(rename = "transaction_confirmed")]
    TransactionConfirmed {
        tx_id: String,
        block_height: u64,
        block_hash: String,
        timestamp: u64,
    },
    
    /// Real-time performance metrics
    #[serde(rename = "metrics")]
    Metrics {
        timestamp: u64,
        tps: f64,
        pending_transactions: usize,
        total_blocks: u64,
        total_transactions: u64,
        memory_usage_mb: f64,
    },
    
    /// Network status update
    #[serde(rename = "status")]
    Status {
        status: String,
        connected_peers: usize,
        latest_block: u64,
        sync_status: String,
    },
    
    /// Heartbeat to keep connection alive
    #[serde(rename = "heartbeat")]
    Heartbeat { timestamp: u64 },
    
    /// Error event
    #[serde(rename = "error")]
    Error {
        message: String,
        code: Option<String>,
    },
    
    /// Casino game win event
    #[serde(rename = "casino_win")]
    CasinoWin {
        game_type: String,
        wallet: String,
        amount_won: f64,
        currency: String,
        timestamp: u64,
        tx_id: String,
        block_height: u64,
    },
    
    /// Casino statistics update
    #[serde(rename = "casino_stats")]
    CasinoStats {
        total_wagered: f64,
        gross_rtp: f64,
        bet_count: u64,
        bankroll: f64,
        wins_24h: u64,
        wagered_24h: f64,
        timestamp: u64,
    },
}

/// WebSocket subscription filters
#[derive(Debug, Clone, Deserialize)]
pub struct WsSubscription {
    /// Subscribe to new block events
    #[serde(default)]
    pub blocks: bool,
    
    /// Subscribe to transaction confirmations
    #[serde(default)]
    pub transactions: bool,
    
    /// Subscribe to specific transaction IDs
    #[serde(default)]
    pub transaction_ids: HashSet<String>,
    
    /// Subscribe to performance metrics
    #[serde(default)]
    pub metrics: bool,
    
    /// Subscribe to status updates
    #[serde(default)]
    pub status: bool,
    
    /// Subscribe to casino win events
    #[serde(default)]
    pub casino: bool,
    
    /// Metrics update interval in seconds
    #[serde(default = "default_metrics_interval")]
    pub metrics_interval_secs: u64,
}

fn default_metrics_interval() -> u64 {
    5 // 5 seconds
}

impl Default for WsSubscription {
    fn default() -> Self {
        Self {
            blocks: true,
            transactions: false,
            transaction_ids: HashSet::new(),
            metrics: false,
            status: false,
            casino: false,
            metrics_interval_secs: 5,
        }
    }
}

/// WebSocket connection manager
#[derive(Clone)]
pub struct WebSocketManager {
    /// Broadcast sender for events
    tx: broadcast::Sender<WsEvent>,
    
    /// Keep-alive receiver to prevent channel closure
    /// This ensures the broadcast channel stays open even when no clients are connected
    _keep_alive_rx: Arc<Mutex<broadcast::Receiver<WsEvent>>>,
    
    /// Connected clients counter
    client_count: Arc<AtomicU64>,
    
    /// Client subscriptions
    subscriptions: Arc<RwLock<HashMap<String, WsSubscription>>>,
    
    /// Storage access
    storage: Arc<OptimizedStorage>,
}

impl WebSocketManager {
    /// Create new WebSocket manager
    pub fn new(storage: Arc<OptimizedStorage>) -> Self {
        let (tx, rx) = broadcast::channel(1024);
        
        Self {
            tx,
            _keep_alive_rx: Arc::new(Mutex::new(rx)),
            client_count: Arc::new(AtomicU64::new(0)),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            storage,
        }
    }
    
    /// Start background tasks
    pub fn start_background_tasks(&self) {
        self.start_heartbeat_task();
        self.start_metrics_broadcast_task();
        self.start_receiver_monitor_task();
    }
    
    /// Handle WebSocket upgrade
    pub async fn handle_upgrade(
        &self,
        ws: WebSocketUpgrade,
        subscription: Option<WsSubscription>,
    ) -> Response {
        let manager = self.clone();
        let sub = subscription.unwrap_or_default();
        
        ws.on_upgrade(move |socket| async move {
            manager.handle_connection(socket, sub).await
        })
    }
    
    /// Handle individual WebSocket connection
    async fn handle_connection(&self, socket: WebSocket, subscription: WsSubscription) {
        let client_id = generate_client_id();
        info!("🔵 handle_connection START for {}", client_id);
        let client_count = self.client_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        
        info!("🔌 WebSocket client {} connected (total: {})", client_id, client_count);
        
        // Store subscription
        self.subscriptions.write().await.insert(client_id.clone(), subscription.clone());
        
        let (mut sender, mut receiver) = socket.split();
        let mut rx = self.tx.subscribe();
        info!("📡 Created broadcast receiver for client {}", client_id);
        
        // Test broadcast immediately
        let test_count = self.tx.receiver_count();
        info!("🧪 TEST: Receiver count right after subscribe: {}", test_count);
        
        // Send welcome message
        if let Err(e) = sender.send(Message::Text(
            serde_json::to_string(&WsEvent::Status {
                status: "connected".to_string(),
                connected_peers: 1, // Single node
                latest_block: self.get_latest_block_height().await.unwrap_or(0),
                sync_status: "synced".to_string(),
            }).unwrap()
        )).await {
            warn!("Failed to send welcome message to client {}: {}", client_id, e);
            return;
        }
        
        let client_id_clone = client_id.clone();
        let client_id_for_send = client_id.clone();
        let subscriptions = self.subscriptions.clone();
        
        // Task to handle incoming messages from client
        let receive_task = tokio::spawn(async move {
            while let Some(msg) = receiver.next().await {
                match msg {
                    Ok(Message::Text(text)) => {
                        debug!("Received message from client {}: {}", client_id_clone, text);
                        // Handle subscription updates here if needed
                    }
                    Ok(Message::Close(_)) => {
                        info!("Client {} requested close", client_id_clone);
                        break;
                    }
                    Ok(Message::Pong(_)) => {
                        debug!("Received pong from client {}", client_id_clone);
                    }
                    Err(e) => {
                        warn!("WebSocket error from client {}: {}", client_id_clone, e);
                        break;
                    }
                    _ => {}
                }
            }
            
            info!("Client {} receive_task ending (stream closed)", client_id_clone);
            
            // Clean up subscription
            subscriptions.write().await.remove(&client_id_clone);
        });
        
        // Task to send events to client
        let send_task = tokio::spawn(async move {
            info!("🟢 send_task STARTED for {}", client_id_for_send);
            
            // Periodic alive check
            let client_id_alive = client_id_for_send.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    info!("💚 send_task for {} still alive", client_id_alive);
                }
            });
            
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        info!("📥 send_task for {} received event", client_id_for_send);
                        // Check if client is subscribed to this event type
                        if !should_send_event(&event, &subscription) {
                            continue;
                        }
                        
                        let message = match serde_json::to_string(&event) {
                            Ok(msg) => Message::Text(msg),
                            Err(e) => {
                                error!("Failed to serialize event: {}", e);
                                continue;
                            }
                        };
                        
                        if sender.send(message).await.is_err() {
                            info!("Client {} send failed, closing send_task", client_id_for_send);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Client {} lagged by {} messages, continuing", client_id_for_send, n);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Broadcast channel closed for client {}, exiting send_task", client_id_for_send);
                        break;
                    }
                }
            }
            
            info!("Client {} send_task ending", client_id_for_send);
        });
        
        info!("✅ Tasks spawned for client {}, waiting in select...", client_id);
        
        // TEST: Send a test broadcast immediately
        let tx_test = self.tx.clone();
        let client_id_test = client_id.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let test_event = WsEvent::Heartbeat { timestamp: current_timestamp() };
            let count = tx_test.receiver_count();
            info!("🧪 TEST BROADCAST for {}: Sending to {} receivers", client_id_test, count);
            if let Err(e) = tx_test.send(test_event) {
                warn!("🧪 TEST BROADCAST FAILED: {}", e);
            } else {
                info!("🧪 TEST BROADCAST SUCCESS");
            }
        });
        
        // Wait for either task to complete
        tokio::select! {
            _ = receive_task => {
                info!("Receive task completed for client {}", client_id);
            }
            _ = send_task => {
                info!("Send task completed for client {}", client_id);
            }
        }
        
        // Cleanup
        self.subscriptions.write().await.remove(&client_id);
        let remaining = self.client_count.fetch_sub(1, std::sync::atomic::Ordering::SeqCst) - 1;
        info!("🔌 WebSocket client {} disconnected (remaining: {})", client_id, remaining);
        info!("🔴 handle_connection END for {}", client_id);
    }
    
    /// Broadcast new block event
    pub async fn broadcast_new_block(&self, height: u64, hash: String, transactions: Vec<String>) {
        let receiver_count = self.tx.receiver_count();
        info!("📤 Broadcasting new block (height: {}) to {} receivers", height, receiver_count);
        
        let event = WsEvent::NewBlock {
            height,
            hash,
            tx_count: transactions.len(),
            timestamp: current_timestamp(),
            transactions,
        };
        
        if let Err(e) = self.tx.send(event) {
            warn!("Failed to broadcast new block event (receivers: {}): {}", receiver_count, e);
        } else {
            info!("✅ Successfully broadcast new block to {} clients", receiver_count);
        }
    }
    
    /// Broadcast transaction confirmation
    pub async fn broadcast_transaction_confirmed(&self, tx_id: String, block_height: u64, block_hash: String) {
        let event = WsEvent::TransactionConfirmed {
            tx_id,
            block_height,
            block_hash,
            timestamp: current_timestamp(),
        };
        
        if let Err(e) = self.tx.send(event) {
            debug!("No WebSocket clients to receive transaction event: {}", e);
        }
    }
    
    /// Broadcast error event
    pub async fn broadcast_error(&self, message: String, code: Option<String>) {
        let event = WsEvent::Error { message, code };
        
        if let Err(e) = self.tx.send(event) {
            debug!("No WebSocket clients to receive error event: {}", e);
        }
    }
    
    /// Broadcast casino win event
    pub async fn broadcast_casino_win(
        &self,
        game_type: String,
        wallet: String,
        amount_won: f64,
        currency: String,
        tx_id: String,
        block_height: u64,
    ) {
        let receiver_count = self.tx.receiver_count();
        info!("🎰 Broadcasting casino win to {} receivers: amount_won={} {}", 
            receiver_count, amount_won, currency);
        
        let event = WsEvent::CasinoWin {
            game_type,
            wallet,
            amount_won,
            currency,
            timestamp: current_timestamp(),
            tx_id,
            block_height,
        };
        
        // Debug: log the serialized event
        if let Ok(json) = serde_json::to_string(&event) {
            info!("📤 Serialized casino win event: {}", json);
        }
        
        if let Err(e) = self.tx.send(event) {
            warn!("Failed to broadcast casino win (receivers: {}): {}", receiver_count, e);
        } else {
            info!("✅ Successfully broadcast casino win to {} clients", receiver_count);
        }
    }
    
    /// Broadcast casino statistics
    pub async fn broadcast_casino_stats(
        &self,
        total_wagered: f64,
        gross_rtp: f64,
        bet_count: u64,
        bankroll: f64,
        wins_24h: u64,
        wagered_24h: f64,
    ) {
        let event = WsEvent::CasinoStats {
            total_wagered,
            gross_rtp,
            bet_count,
            bankroll,
            wins_24h,
            wagered_24h,
            timestamp: current_timestamp(),
        };
        
        if let Err(e) = self.tx.send(event) {
            debug!("No WebSocket clients to receive casino stats event: {}", e);
        }
    }
    
    /// Get current client count
    pub fn client_count(&self) -> u64 {
        self.client_count.load(std::sync::atomic::Ordering::SeqCst)
    }
    
    /// Start heartbeat task to keep connections alive
    fn start_heartbeat_task(&self) {
        let tx = self.tx.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30)); // Every 30 seconds
            
            loop {
                interval.tick().await;
                
                let heartbeat = WsEvent::Heartbeat {
                    timestamp: current_timestamp(),
                };
                
                if let Err(_) = tx.send(heartbeat) {
                    // No receivers, continue
                }
            }
        });
    }
    
    /// Start metrics broadcast task
    fn start_metrics_broadcast_task(&self) {
        let tx = self.tx.clone();
        let storage = self.storage.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5)); // Every 5 seconds
            
            loop {
                interval.tick().await;
                
                // Get current metrics from storage
                if let Ok(metrics) = get_current_metrics(&storage).await {
                    let event = WsEvent::Metrics {
                        timestamp: current_timestamp(),
                        tps: metrics.tps,
                        pending_transactions: metrics.pending_transactions,
                        total_blocks: metrics.total_blocks,
                        total_transactions: metrics.total_transactions,
                        memory_usage_mb: metrics.memory_usage_mb,
                    };
                    
                    if let Err(_) = tx.send(event) {
                        // No receivers, continue
                    }
                }
            }
        });
    }
    
    /// Monitor receiver count for debugging
    fn start_receiver_monitor_task(&self) {
        let tx = self.tx.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5)); // Every 5 seconds
            
            loop {
                interval.tick().await;
                let count = tx.receiver_count();
                info!("🔍 Receiver count monitor: {} active receivers", count);
            }
        });
    }
    
    /// Get latest block height
    async fn get_latest_block_height(&self) -> Result<u64, Box<dyn std::error::Error>> {
        // Implementation depends on storage interface
        // This is a placeholder
        Ok(0)
    }
}

/// Check if event should be sent to client based on subscription
fn should_send_event(event: &WsEvent, subscription: &WsSubscription) -> bool {
    match event {
        WsEvent::NewBlock { .. } => subscription.blocks,
        WsEvent::TransactionConfirmed { tx_id, .. } => {
            subscription.transactions || subscription.transaction_ids.contains(tx_id)
        }
        WsEvent::Metrics { .. } => subscription.metrics,
        WsEvent::Status { .. } => subscription.status,
        WsEvent::Heartbeat { .. } => true, // Always send heartbeats
        WsEvent::Error { .. } => true, // Always send errors
        WsEvent::CasinoWin { .. } => subscription.casino,
        WsEvent::CasinoStats { .. } => subscription.casino,
    }
}

/// Generate unique client ID
fn generate_client_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    
    format!("ws_{}", COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Current metrics structure
struct CurrentMetrics {
    tps: f64,
    pending_transactions: usize,
    total_blocks: u64,
    total_transactions: u64,
    memory_usage_mb: f64,
}

/// Get current system metrics
async fn get_current_metrics(storage: &OptimizedStorage) -> Result<CurrentMetrics, Box<dyn std::error::Error>> {
    // This would integrate with your actual metrics system
    // Placeholder implementation
    Ok(CurrentMetrics {
        tps: 0.0,
        pending_transactions: 0,
        total_blocks: 0,
        total_transactions: 0,
        memory_usage_mb: get_memory_usage(),
    })
}

/// Get current memory usage in MB
fn get_memory_usage() -> f64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(contents) = std::fs::read_to_string("/proc/self/status") {
            for line in contents.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<f64>() {
                            return kb / 1024.0; // Convert KB to MB
                        }
                    }
                }
            }
        }
    }
    
    // Fallback for other platforms
    0.0
}

/// WebSocket query parameters for subscription
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    #[serde(default)]
    pub blocks: bool,
    
    #[serde(default)]
    pub transactions: bool,
    
    #[serde(default)]
    pub metrics: bool,
    
    #[serde(default)]
    pub status: bool,
    
    #[serde(default)]
    pub casino: bool,
    
    #[serde(default = "default_metrics_interval")]
    pub metrics_interval: u64,
}

impl From<WsQuery> for WsSubscription {
    fn from(query: WsQuery) -> Self {
        Self {
            blocks: query.blocks,
            transactions: query.transactions,
            transaction_ids: HashSet::new(),
            metrics: query.metrics,
            status: query.status,
            casino: query.casino,
            metrics_interval_secs: query.metrics_interval,
        }
    }
}

/// WebSocket endpoint handler
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    let subscription = WsSubscription::from(params);
    state.websocket_manager.handle_upgrade(ws, Some(subscription)).await
}

/// Subscribe to specific transaction endpoint
pub async fn transaction_websocket_handler(
    ws: WebSocketUpgrade,
    Path(tx_id): Path<String>,
    Query(mut params): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    // Enable transaction subscription and add specific tx ID
    params.transactions = true;
    let mut subscription = WsSubscription::from(params);
    subscription.transaction_ids.insert(tx_id);
    
    state.websocket_manager.handle_upgrade(ws, Some(subscription)).await
}