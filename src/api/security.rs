//! Rate Limiting & Security
//!
//! Comprehensive security features including:
//! - Configurable rate limiting per IP/API key
//! - API key authentication and authorization
//! - DDoS protection mechanisms  
//! - Request validation and sanitization

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::RwLock;
use axum::{
    extract::{ConnectInfo, Request},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn, error};
use uuid::Uuid;

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per minute for unauthenticated users
    pub requests_per_minute_anonymous: u32,
    /// Requests per minute for authenticated users
    pub requests_per_minute_authenticated: u32,
    /// Requests per minute for premium API keys
    pub requests_per_minute_premium: u32,
    /// Window size for rate limiting in seconds
    pub window_size_seconds: u64,
    /// Maximum burst requests allowed
    pub burst_capacity: u32,
    /// IP whitelist for unlimited access
    pub ip_whitelist: Vec<IpAddr>,
    /// Enable DDoS protection
    pub enable_ddos_protection: bool,
    /// Maximum requests per second from single IP (DDoS threshold)
    pub ddos_threshold_rps: u32,
    /// DDoS ban duration in seconds
    pub ddos_ban_duration_seconds: u64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute_anonymous: 60,      // 1 req/sec for anonymous
            requests_per_minute_authenticated: 600,  // 10 req/sec for authenticated
            requests_per_minute_premium: 6000,      // 100 req/sec for premium
            window_size_seconds: 60,
            burst_capacity: 10,
            ip_whitelist: vec![
                "127.0.0.1".parse().unwrap(),
                "::1".parse().unwrap(),
            ],
            enable_ddos_protection: true,
            ddos_threshold_rps: 100,               // 100 req/sec threshold
            ddos_ban_duration_seconds: 300,       // 5 minute ban
        }
    }
}

/// API key configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub key_hash: String, // SHA256 hash of the actual key
    pub name: String,
    pub tier: ApiKeyTier,
    pub created_at: u64,
    pub last_used: Option<u64>,
    pub request_count: u64,
    pub enabled: bool,
    pub rate_limit_override: Option<u32>,
    pub ip_restrictions: Vec<IpAddr>,
    pub endpoint_permissions: Vec<String>,
}

/// API key tiers with different privileges
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiKeyTier {
    Basic,
    Premium,
    Enterprise,
}

impl ApiKeyTier {
    pub fn default_rate_limit(&self) -> u32 {
        match self {
            ApiKeyTier::Basic => 600,      // 10 req/sec
            ApiKeyTier::Premium => 6000,   // 100 req/sec  
            ApiKeyTier::Enterprise => 60000, // 1000 req/sec
        }
    }
}

/// Rate limiter using token bucket algorithm
#[derive(Debug)]
struct TokenBucket {
    tokens: f64,
    last_refill: SystemTime,
    capacity: f64,
    refill_rate: f64, // tokens per second
}

impl TokenBucket {
    fn new(capacity: u32, refill_rate_per_minute: u32) -> Self {
        let refill_rate = refill_rate_per_minute as f64 / 60.0; // Convert to per second
        Self {
            tokens: capacity as f64,
            last_refill: SystemTime::now(),
            capacity: capacity as f64,
            refill_rate,
        }
    }
    
    fn try_consume(&mut self, tokens: f64) -> bool {
        self.refill();
        
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
    
    fn refill(&mut self) {
        let now = SystemTime::now();
        let elapsed = now.duration_since(self.last_refill).unwrap_or_default().as_secs_f64();
        
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
    }
    
    fn tokens_available(&mut self) -> f64 {
        self.refill();
        self.tokens
    }
}

/// Security manager for rate limiting and authentication
#[derive(Clone)]
pub struct SecurityManager {
    config: RateLimitConfig,
    
    /// Rate limiters by IP address
    ip_limiters: Arc<DashMap<IpAddr, TokenBucket>>,
    
    /// Rate limiters by API key
    api_key_limiters: Arc<DashMap<String, TokenBucket>>,
    
    /// API keys database
    api_keys: Arc<RwLock<HashMap<String, ApiKey>>>,
    
    /// DDoS protection - banned IPs
    banned_ips: Arc<DashMap<IpAddr, SystemTime>>,
    
    /// Request statistics for DDoS detection
    request_stats: Arc<DashMap<IpAddr, Vec<SystemTime>>>,
}

impl SecurityManager {
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            config,
            ip_limiters: Arc::new(DashMap::new()),
            api_key_limiters: Arc::new(DashMap::new()),
            api_keys: Arc::new(RwLock::new(HashMap::new())),
            banned_ips: Arc::new(DashMap::new()),
            request_stats: Arc::new(DashMap::new()),
        }
    }
    
    /// Create new API key
    pub async fn create_api_key(
        &self,
        name: String,
        tier: ApiKeyTier,
        ip_restrictions: Vec<IpAddr>,
        endpoint_permissions: Vec<String>,
    ) -> ApiKey {
        let key_id = Uuid::new_v4().to_string();
        let raw_key = format!("atomiq_{}", Uuid::new_v4().simple());
        let key_hash = hash_api_key(&raw_key);
        
        let api_key = ApiKey {
            id: key_id.clone(),
            key_hash,
            name,
            tier,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            last_used: None,
            request_count: 0,
            enabled: true,
            rate_limit_override: None,
            ip_restrictions,
            endpoint_permissions,
        };
        
        self.api_keys.write().await.insert(key_id, api_key.clone());
        
        debug!("Created new API key: {} ({})", api_key.name, api_key.id);
        api_key
    }
    
    /// Validate API key
    async fn validate_api_key(&self, key: &str, client_ip: IpAddr) -> Option<ApiKey> {
        let key_hash = hash_api_key(key);
        let api_keys = self.api_keys.read().await;
        
        for api_key in api_keys.values() {
            if api_key.key_hash == key_hash && api_key.enabled {
                // Check IP restrictions
                if !api_key.ip_restrictions.is_empty() && 
                   !api_key.ip_restrictions.contains(&client_ip) {
                    warn!("API key {} used from unauthorized IP: {}", api_key.id, client_ip);
                    return None;
                }
                
                return Some(api_key.clone());
            }
        }
        
        None
    }
    
    /// Check if request is allowed (rate limiting + DDoS protection)
    pub async fn check_request_allowed(
        &self,
        client_ip: IpAddr,
        api_key: Option<&str>,
        endpoint: &str,
    ) -> Result<AuthContext, SecurityError> {
        // Check if IP is banned (DDoS protection)
        if let Some(banned_until) = self.banned_ips.get(&client_ip) {
                if banned_until.duration_since(SystemTime::now()).is_ok() {
                return Err(SecurityError::IpBanned {
                    ip: client_ip,
                    reason: "DDoS protection".to_string(),
                });
            } else {
                // Ban expired, remove it
                self.banned_ips.remove(&client_ip);
            }
        }
        
        // Check IP whitelist
        if self.config.ip_whitelist.contains(&client_ip) {
            return Ok(AuthContext {
                client_ip,
                api_key_id: None,
                tier: None,
                rate_limit_remaining: u32::MAX, // Unlimited for whitelisted IPs
            });
        }
        
        // DDoS detection
        if self.config.enable_ddos_protection {
            self.update_request_stats(client_ip);
            if self.is_ddos_attack(client_ip) {
                self.banned_ips.insert(client_ip, SystemTime::now());
                warn!("IP {} banned for DDoS attack", client_ip);
                return Err(SecurityError::DDoSDetected { ip: client_ip });
            }
        }
        
        // Validate API key if provided
        let validated_key = if let Some(key) = api_key {
            match self.validate_api_key(key, client_ip).await {
                Some(api_key) => {
                    // Check endpoint permissions
                    if !api_key.endpoint_permissions.is_empty() && 
                       !api_key.endpoint_permissions.contains(&endpoint.to_string()) &&
                       !api_key.endpoint_permissions.contains(&"*".to_string()) {
                        return Err(SecurityError::InsufficientPermissions {
                            endpoint: endpoint.to_string(),
                            required_permissions: vec![endpoint.to_string()],
                        });
                    }
                    Some(api_key)
                }
                None => {
                    return Err(SecurityError::InvalidApiKey);
                }
            }
        } else {
            None
        };
        
        // Determine rate limit
        let (rate_limit, limiter_key, tier) = if let Some(ref api_key) = validated_key {
            let limit = api_key.rate_limit_override
                .unwrap_or(api_key.tier.default_rate_limit());
            (limit, format!("api:{}", api_key.id), Some(api_key.tier.clone()))
        } else {
            (self.config.requests_per_minute_anonymous, format!("ip:{}", client_ip), None)
        };
        
        // Check rate limit
        let tokens_remaining = if let Some(ref api_key) = validated_key {
            // Use API key rate limiter
            let mut api_limiters = self.api_key_limiters.entry(api_key.id.clone())
                .or_insert_with(|| TokenBucket::new(self.config.burst_capacity, rate_limit));
            
            if !api_limiters.try_consume(1.0) {
                return Err(SecurityError::RateLimitExceeded {
                    limit: rate_limit,
                    window_seconds: self.config.window_size_seconds,
                    retry_after_seconds: 60,
                });
            }
            
            api_limiters.tokens_available() as u32
        } else {
            // Use IP rate limiter
            let mut ip_limiters = self.ip_limiters.entry(client_ip)
                .or_insert_with(|| TokenBucket::new(self.config.burst_capacity, rate_limit));
            
            if !ip_limiters.try_consume(1.0) {
                return Err(SecurityError::RateLimitExceeded {
                    limit: rate_limit,
                    window_seconds: self.config.window_size_seconds,
                    retry_after_seconds: 60,
                });
            }
            
            ip_limiters.tokens_available() as u32
        };
        
        // Update API key usage statistics
        if let Some(ref api_key) = validated_key {
            // Update last used time and request count
            // This would normally update the database
            debug!("API key {} used by IP {}", api_key.id, client_ip);
        }
        
        Ok(AuthContext {
            client_ip,
            api_key_id: validated_key.map(|k| k.id),
            tier,
            rate_limit_remaining: tokens_remaining,
        })
    }
    
    /// Update request statistics for DDoS detection
    fn update_request_stats(&self, ip: IpAddr) {
        let now = SystemTime::now();
        let window = Duration::from_secs(1); // 1 second window for RPS calculation
        
        self.request_stats.entry(ip)
            .and_modify(|times| {
                // Remove old entries
                times.retain(|&time| now.duration_since(time).unwrap_or_default() <= window);
                times.push(now);
            })
            .or_insert_with(|| vec![now]);
    }
    
    /// Check if IP is performing DDoS attack
    fn is_ddos_attack(&self, ip: IpAddr) -> bool {
        if let Some(stats) = self.request_stats.get(&ip) {
            stats.len() > self.config.ddos_threshold_rps as usize
        } else {
            false
        }
    }
    
    /// Get security statistics
    pub async fn get_statistics(&self) -> SecurityStatistics {
        let api_keys = self.api_keys.read().await;
        
        SecurityStatistics {
            active_api_keys: api_keys.values().filter(|k| k.enabled).count(),
            total_api_keys: api_keys.len(),
            banned_ips: self.banned_ips.len(),
            active_rate_limiters: self.ip_limiters.len() + self.api_key_limiters.len(),
            total_requests_blocked: 0, // Would track this in production
        }
    }
    
    /// Start cleanup task for expired entries
    pub fn start_cleanup_task(security_manager: Arc<SecurityManager>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // Every 5 minutes
            
            loop {
                interval.tick().await;
                
                // Clean up expired ban entries
                let expired_bans: Vec<IpAddr> = security_manager.banned_ips.iter()
                    .filter_map(|entry| {
                        let ip = *entry.key();
                        let banned_at = *entry.value();
                        if SystemTime::now().duration_since(banned_at).unwrap_or_default() > Duration::from_secs(security_manager.config.ddos_ban_duration_seconds) {
                            Some(ip)
                        } else {
                            None
                        }
                    })
                    .collect();
                
                for ip in expired_bans {
                    security_manager.banned_ips.remove(&ip);
                    debug!("Removed expired ban for IP: {}", ip);
                }
                
                // Clean up old request stats
                let cutoff = SystemTime::now() - Duration::from_secs(60);
                security_manager.request_stats.retain(|_, times| {
                    times.retain(|&time| time > cutoff);
                    !times.is_empty()
                });
                
                debug!("Completed security cleanup task");
            }
        });
    }
}

/// Authentication context extracted from middleware
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub client_ip: IpAddr,
    pub api_key_id: Option<String>,
    pub tier: Option<ApiKeyTier>,
    pub rate_limit_remaining: u32,
}

/// Security-related errors
#[derive(Debug)]
pub enum SecurityError {
    RateLimitExceeded {
        limit: u32,
        window_seconds: u64,
        retry_after_seconds: u64,
    },
    InvalidApiKey,
    IpBanned {
        ip: IpAddr,
        reason: String,
    },
    DDoSDetected {
        ip: IpAddr,
    },
    InsufficientPermissions {
        endpoint: String,
        required_permissions: Vec<String>,
    },
}

impl SecurityError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            SecurityError::RateLimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
            SecurityError::InvalidApiKey => StatusCode::UNAUTHORIZED,
            SecurityError::IpBanned { .. } => StatusCode::FORBIDDEN,
            SecurityError::DDoSDetected { .. } => StatusCode::FORBIDDEN,
            SecurityError::InsufficientPermissions { .. } => StatusCode::FORBIDDEN,
        }
    }
    
    pub fn message(&self) -> String {
        match self {
            SecurityError::RateLimitExceeded { limit, window_seconds, .. } => {
                format!("Rate limit exceeded: {} requests per {} seconds", limit, window_seconds)
            }
            SecurityError::InvalidApiKey => "Invalid API key".to_string(),
            SecurityError::IpBanned { reason, .. } => format!("IP banned: {}", reason),
            SecurityError::DDoSDetected { .. } => "DDoS attack detected".to_string(),
            SecurityError::InsufficientPermissions { endpoint, .. } => {
                format!("Insufficient permissions for endpoint: {}", endpoint)
            }
        }
    }
}

/// Security statistics
#[derive(Debug, Serialize)]
pub struct SecurityStatistics {
    pub active_api_keys: usize,
    pub total_api_keys: usize,
    pub banned_ips: usize,
    pub active_rate_limiters: usize,
    pub total_requests_blocked: u64,
}

/// Axum middleware for rate limiting and authentication
pub async fn security_middleware(
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let client_ip = addr.ip();
    
    // Extract API key from Authorization header
    let api_key = headers.get("Authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|auth| {
            if auth.starts_with("Bearer ") {
                Some(&auth[7..])
            } else {
                None
            }
        });
    
    // Get endpoint path
    let endpoint = request.uri().path();
    
    // TODO: Extract security manager from app state
    // For now, create a default one
    let security_manager = SecurityManager::new(RateLimitConfig::default());
    
    // Check if request is allowed
    match security_manager.check_request_allowed(client_ip, api_key, endpoint).await {
        Ok(auth_context) => {
            // Add auth context to request extensions
            let mut request = request;
            request.extensions_mut().insert(auth_context);
            
            // Continue to next middleware/handler
            Ok(next.run(request).await)
        }
        Err(security_error) => {
            warn!("Security check failed for {}: {:?}", client_ip, security_error);
            Err(security_error.status_code())
        }
    }
}

/// Hash API key using SHA256
fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extract client IP from request, handling proxies
pub fn extract_client_ip(headers: &HeaderMap, connect_info: Option<std::net::SocketAddr>) -> IpAddr {
    // Check X-Forwarded-For header (from load balancer/proxy)
    if let Some(forwarded) = headers.get("X-Forwarded-For") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(first_ip) = forwarded_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                    return ip;
                }
            }
        }
    }
    
    // Check X-Real-IP header (from Nginx)
    if let Some(real_ip) = headers.get("X-Real-IP") {
        if let Ok(real_ip_str) = real_ip.to_str() {
            if let Ok(ip) = real_ip_str.parse::<IpAddr>() {
                return ip;
            }
        }
    }
    
    // Fall back to direct connection IP
    connect_info.map(|addr| addr.ip()).unwrap_or_else(|| "127.0.0.1".parse().unwrap())
}