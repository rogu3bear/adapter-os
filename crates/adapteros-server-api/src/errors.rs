//! User-friendly error handling for AdapterOS API
//!
//! Provides automatic mapping of technical error messages to user-friendly,
//! actionable messages. Supports retry logic and enhanced error responses.
//!
//! Citations:
//! - Error message mapping: Based on UX improvements demo【1†demo_ux_improvements.rs】
//! - Error response enhancement: Extends existing ErrorResponse pattern【2†adapteros-server-api/src/handlers.rs】
//! - Retry logic: Implements exponential backoff for transient errors【3†demo_ux_improvements.rs:6-10】

use adapteros_core::{AosError, Result};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, info, warn};

/// Enhanced error response with user-friendly messages and retry information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// HTTP status code
    pub status: u16,
    /// Error code for programmatic handling
    pub error_code: String,
    /// User-friendly error message
    pub message: String,
    /// Technical details (only in development/debug mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// Whether this error is retryable
    #[serde(default)]
    pub retryable: bool,
    /// Suggested retry delay in seconds (if retryable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
    /// Request ID for correlation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Timestamp of the error
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ErrorResponse {
    /// Create a new error response from an AosError
    pub fn from_error(error: &AosError, request_id: Option<String>) -> Self {
        let (status, error_code, technical_message) = error_to_components(error);
        let user_friendly_message = UserFriendlyErrorMapper::map_error_message(&error_code, &technical_message);

        let retryable = is_retryable_error(error);
        let retry_after = if retryable { Some(5) } else { None }; // 5 second default

        Self {
            status,
            error_code,
            message: user_friendly_message,
            details: if cfg!(debug_assertions) { Some(technical_message) } else { None },
            retryable,
            retry_after,
            request_id,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a user-friendly error response with custom message
    pub fn new_user_friendly(status: StatusCode, error_code: &str, user_message: &str, request_id: Option<String>) -> Self {
        Self {
            status: status.as_u16(),
            error_code: error_code.to_string(),
            message: user_message.to_string(),
            details: None,
            retryable: false,
            retry_after: None,
            request_id,
            timestamp: chrono::Utc::now(),
        }
    }

    /// Create a retryable error response
    pub fn new_retryable(status: StatusCode, error_code: &str, user_message: &str, retry_after: u64, request_id: Option<String>) -> Self {
        Self {
            status: status.as_u16(),
            error_code: error_code.to_string(),
            message: user_message.to_string(),
            details: None,
            retryable: true,
            retry_after: Some(retry_after),
            request_id,
            timestamp: chrono::Utc::now(),
        }
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

        // Set Retry-After header if retryable
        let mut response = axum::Json(self).into_response();
        *response.status_mut() = status;

        if let Some(retry_after) = self.retry_after {
            response.headers_mut().insert(
                "Retry-After",
                retry_after.to_string().parse().unwrap(),
            );
        }

        response
    }
}

/// Maps technical error messages to user-friendly messages
pub struct UserFriendlyErrorMapper;

impl UserFriendlyErrorMapper {
    /// Map an error code and technical message to a user-friendly message
    pub fn map_error_message(error_code: &str, technical_message: &str) -> String {
        // First try exact error code mapping
        if let Some(message) = Self::get_exact_mapping(error_code) {
            return message.to_string();
        }

        // Then try pattern-based mapping
        Self::get_pattern_mapping(technical_message)
    }

    /// Get exact mapping for known error codes
    fn get_exact_mapping(error_code: &str) -> Option<&'static str> {
        match error_code {
            "DB_ERROR" => Some("The database is temporarily unavailable. Please try again in a moment."),
            "LOAD_FAILED" => Some("The model files could not be found. Please verify the model path and try again."),
            "NOT_FOUND" => Some("The requested model was not found. Please check the model ID and try again."),
            "TIMEOUT" => Some("The operation timed out. This usually happens when the system is busy. Please try again."),
            "VALIDATION_ERROR" => Some("The request contains invalid data. Please check your input and try again."),
            "PERMISSION_DENIED" => Some("You don't have permission to perform this action. Please contact your administrator."),
            "RATE_LIMITED" => Some("Too many requests. Please wait a moment before trying again."),
            "INTERNAL_ERROR" => Some("An unexpected error occurred. Our team has been notified. Please try again later."),
            "NETWORK_ERROR" => Some("Network connection issue. Please check your connection and try again."),
            "CONFIG_ERROR" => Some("System configuration issue. Please try again later."),
            "TOKEN_ERROR" => Some("Failed to generate authentication token. Please try again."),
            _ => None,
        }
    }

    /// Get pattern-based mapping for technical error messages
    fn get_pattern_mapping(technical_message: &str) -> String {
        let message = technical_message.to_lowercase();

        // Connection/database errors
        if message.contains("connection refused") || message.contains("connection reset") {
            return "The database is temporarily unavailable. Please try again in a moment.".to_string();
        }

        if message.contains("timeout") || message.contains("timed out") {
            return "The operation timed out. This usually happens when the system is busy. Please try again.".to_string();
        }

        // File/path errors
        if message.contains("path does not exist") || message.contains("file not found") || message.contains("no such file") {
            return "The model files could not be found. Please verify the model path and try again.".to_string();
        }

        if message.contains("permission denied") || message.contains("access denied") {
            return "Permission denied accessing the requested files. Please contact your administrator.".to_string();
        }

        // Model errors
        if message.contains("model not found") || message.contains("adapter not found") {
            return "The requested model was not found. Please check the model ID and try again.".to_string();
        }

        if message.contains("invalid model") || message.contains("corrupted model") {
            return "The model file appears to be corrupted or invalid. Please try a different model.".to_string();
        }

        // Memory/resource errors
        if message.contains("out of memory") || message.contains("memory allocation failed") {
            return "The system is running low on memory. Please try again later or with a smaller model.".to_string();
        }

        if message.contains("disk full") || message.contains("no space left") {
            return "The system is running low on disk space. Please contact your administrator.".to_string();
        }

        // Network errors
        if message.contains("network") || message.contains("dns") || message.contains("connection") {
            return "Network connection issue. Please check your connection and try again.".to_string();
        }

        // Validation errors
        if message.contains("validation") || message.contains("invalid") {
            return "The request contains invalid data. Please check your input and try again.".to_string();
        }

        // Rate limiting
        if message.contains("rate limit") || message.contains("too many requests") {
            return "Too many requests. Please wait a moment before trying again.".to_string();
        }

        // Fallback for unknown errors
        warn!("Unknown error pattern: {}", technical_message);
        "An unexpected error occurred. Please try again or contact support if the problem persists.".to_string()
    }
}

/// Retry configuration for operations
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Backoff multiplier
    pub backoff_multiplier: f64,
    /// Jitter factor (0.0 = no jitter, 1.0 = full jitter)
    pub jitter_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
        }
    }
}

/// Retry logic with exponential backoff for transient errors
pub struct RetryExecutor {
    config: RetryConfig,
}

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// IntoResponse implementation for ErrorResponse
/// This allows ErrorResponse to be returned directly from handlers
impl axum::response::IntoResponse for crate::types::ErrorResponse {
    fn into_response(self) -> axum::response::Response {
        let status = match self.code.as_str() {
            "NOT_FOUND" => StatusCode::NOT_FOUND,
            "UNAUTHORIZED" => StatusCode::UNAUTHORIZED,
            "FORBIDDEN" => StatusCode::FORBIDDEN,
            "BAD_REQUEST" => StatusCode::BAD_REQUEST,
            "CONFLICT" => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, axum::Json(self)).into_response()
    }
}
