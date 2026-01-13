//! Middleware Components
//! 
//! CORS, rate limiting, request tracking, and other cross-cutting concerns.

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use tower_http::cors::{Any, CorsLayer};
use tower_http::cors::ExposeHeaders;
use axum::http::HeaderName;
use uuid::Uuid;

/// Request ID header key
pub const REQUEST_ID_HEADER: &str = "x-request-id";

/// Create CORS middleware with configurable origins
pub fn create_cors_layer(allowed_origins: Vec<String>) -> CorsLayer {
    if allowed_origins.is_empty() || allowed_origins.contains(&"*".to_string()) {
        // Development mode: allow all origins
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
            .expose_headers(ExposeHeaders::list([HeaderName::from_static(REQUEST_ID_HEADER)]))
    } else {
        // Production mode: specific origins
        CorsLayer::new()
            .allow_origin(
                allowed_origins.into_iter()
                    .filter_map(|o| o.parse().ok())
                    .collect::<Vec<_>>()
            )
            .allow_methods([axum::http::Method::GET])
            .allow_headers(Any)
            .expose_headers(ExposeHeaders::list([HeaderName::from_static(REQUEST_ID_HEADER)]))
    }
}

/// Middleware to add request ID to all requests
pub async fn request_id_middleware(
    mut request: Request,
    next: Next,
) -> Response {
    // Check if request already has an ID from client
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // Store request ID in extensions for handlers to access
    request.extensions_mut().insert(RequestId(request_id.clone()));

    // Call next middleware/handler
    let mut response = next.run(request).await;

    // Add request ID to response headers
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        request_id.parse().unwrap(),
    );

    response
}

/// Request ID wrapper for extracting in handlers
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Rate limiting placeholder
/// TODO: Implement actual rate limiting when needed
pub struct RateLimitConfig {
    pub requests_per_second: u32,
    pub burst_size: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 100,
            burst_size: 200,
        }
    }
}
