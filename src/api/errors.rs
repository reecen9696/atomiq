//! API Error Handling
//! 
//! Structured error responses with proper HTTP status codes and request tracking.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Top-level API error response with request tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub request_id: String,
    pub error: ErrorBody,
}

/// Error body with structured information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorBody {
    /// Error code (NOT_FOUND, BAD_REQUEST, INTERNAL_ERROR, etc.)
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// Optional additional details (can be any JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// API error types with request tracking
#[derive(Debug)]
pub struct ApiError {
    pub kind: ApiErrorKind,
    pub request_id: String,
}

#[derive(Debug)]
pub enum ApiErrorKind {
    NotFound(String),
    BadRequest(String),
    InternalError(String),
    ServiceUnavailable(String),
}

impl ApiError {
    pub fn not_found(request_id: String, message: String) -> Self {
        Self {
            kind: ApiErrorKind::NotFound(message),
            request_id,
        }
    }

    pub fn bad_request(request_id: String, message: String) -> Self {
        Self {
            kind: ApiErrorKind::BadRequest(message),
            request_id,
        }
    }

    pub fn internal_error(request_id: String, message: String) -> Self {
        Self {
            kind: ApiErrorKind::InternalError(message),
            request_id,
        }
    }

    pub fn service_unavailable(request_id: String, message: String) -> Self {
        Self {
            kind: ApiErrorKind::ServiceUnavailable(message),
            request_id,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ApiErrorKind::NotFound(msg) => write!(f, "[{}] Not Found: {}", self.request_id, msg),
            ApiErrorKind::BadRequest(msg) => write!(f, "[{}] Bad Request: {}", self.request_id, msg),
            ApiErrorKind::InternalError(msg) => write!(f, "[{}] Internal Error: {}", self.request_id, msg),
            ApiErrorKind::ServiceUnavailable(msg) => write!(f, "[{}] Service Unavailable: {}", self.request_id, msg),
        }
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self.kind {
            ApiErrorKind::NotFound(msg) => {
                (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone())
            }
            ApiErrorKind::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone())
            }
            ApiErrorKind::InternalError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg.clone())
            }
            ApiErrorKind::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE", msg.clone())
            }
        };

        let body = Json(ErrorResponse {
            request_id: self.request_id.clone(),
            error: ErrorBody {
                code: code.to_string(),
                message,
                details: None,
            },
        });

        (status, body).into_response()
    }
}
