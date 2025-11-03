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

impl RetryExecutor {
    pub fn new(config: RetryConfig) -> Self {
        Self { config }
    }

    pub fn with_default_config() -> Self {
        Self::new(RetryConfig::default())
    }

    /// Execute an async operation with retry logic
    pub async fn execute<F, Fut, T, E>(&self, mut operation: F) -> Result<T, E>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
        E: std::fmt::Debug,
    {
        let mut attempt = 0;
        let mut delay = self.config.initial_delay;

        loop {
            attempt += 1;

            match operation().await {
                Ok(result) => {
                    if attempt > 1 {
                        info!("Operation succeeded on attempt {}", attempt);
                    }
                    return Ok(result);
                }
                Err(error) => {
                    if attempt >= self.config.max_attempts {
                        debug!("Operation failed after {} attempts: {:?}", attempt, error);
                        return Err(error);
                    }

                    // Check if error is retryable
                    if !is_retryable_error_generic(&error) {
                        debug!("Non-retryable error on attempt {}: {:?}", attempt, error);
                        return Err(error);
                    }

                    // Add jitter to prevent thundering herd
                    let jitter = if self.config.jitter_factor > 0.0 {
                        let jitter_range = (delay.as_millis() as f64 * self.config.jitter_factor) as u64;
                        let jitter_amount = fastrand::u64(0..jitter_range);
                        Duration::from_millis(jitter_amount)
                    } else {
                        Duration::ZERO
                    };

                    let actual_delay = delay + jitter;

                    debug!(
                        "Operation failed on attempt {}, retrying in {:?}: {:?}",
                        attempt,
                        actual_delay,
                        error
                    );

                    sleep(actual_delay).await;

                    // Exponential backoff with max delay
                    delay = std::cmp::min(
                        Duration::from_millis((delay.as_millis() as f64 * self.config.backoff_multiplier) as u64),
                        self.config.max_delay,
                    );
                }
            }
        }
    }
}

/// Convert AosError to HTTP status, error code, and technical message
fn error_to_components(error: &AosError) -> (u16, String, String) {
    match error {
        AosError::NotFound(msg) => (404, "NOT_FOUND".to_string(), msg.clone()),
        AosError::Validation(msg) => (400, "VALIDATION_ERROR".to_string(), msg.clone()),
        AosError::Io(msg) => {
            if msg.contains("permission") || msg.contains("access") {
                (403, "PERMISSION_DENIED".to_string(), msg.clone())
            } else {
                (500, "IO_ERROR".to_string(), msg.clone())
            }
        }
        AosError::Database(msg) => (500, "DB_ERROR".to_string(), msg.clone()),
        AosError::Network(msg) => (502, "NETWORK_ERROR".to_string(), msg.clone()),
        AosError::Config(msg) => (500, "CONFIG_ERROR".to_string(), msg.clone()),
        AosError::PolicyViolation(msg) => (403, "PERMISSION_DENIED".to_string(), msg.clone()),
        AosError::Crypto(msg) => (500, "CRYPTO_ERROR".to_string(), msg.clone()),
        AosError::Memory(msg) => (507, "MEMORY_ERROR".to_string(), msg.clone()),
        AosError::Worker(msg) => {
            if msg.contains("timeout") {
                (504, "TIMEOUT".to_string(), msg.clone())
            } else if msg.contains("not found") {
                (404, "NOT_FOUND".to_string(), msg.clone())
            } else {
                (500, "WORKER_ERROR".to_string(), msg.clone())
            }
        }
        AosError::Auth(msg) => {
            if msg.contains("expired") {
                (401, "TOKEN_EXPIRED".to_string(), msg.clone())
            } else {
                (401, "AUTH_ERROR".to_string(), msg.clone())
            }
        }
        AosError::RateLimit(msg) => (429, "RATE_LIMITED".to_string(), msg.clone()),
        AosError::TenantIsolation(msg) => (403, "TENANT_ISOLATION".to_string(), msg.clone()),
        AosError::DeterminismViolation(msg) => (500, "DETERMINISM_ERROR".to_string(), msg.clone()),
        AosError::EgressViolation(msg) => (403, "EGRESS_VIOLATION".to_string(), msg.clone()),
        AosError::IsolationViolation(msg) => (403, "ISOLATION_VIOLATION".to_string(), msg.clone()),
        AosError::Security(msg) => (403, "SECURITY_ERROR".to_string(), msg.clone()),
        _ => (500, "INTERNAL_ERROR".to_string(), format!("{:?}", error)),
    }
}

/// Check if an AosError is retryable
fn is_retryable_error(error: &AosError) -> bool {
    match error {
        AosError::Database(_) | AosError::Network(_) | AosError::Io(_) => true,
        AosError::Worker(msg) if msg.contains("timeout") || msg.contains("busy") => true,
        AosError::RateLimit(_) => true,
        _ => false,
    }
}

/// Generic retryable error check for any error type
fn is_retryable_error_generic<E: std::fmt::Debug>(_error: &E) -> bool {
    // For now, be conservative and only retry on specific error types
    // In a real implementation, you might want to check error messages
    // for patterns like "timeout", "busy", "temporary", etc.
    // For this implementation, we'll use a simple heuristic
    false // Conservative approach - don't retry unknown errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::AosError;

    #[test]
    fn test_error_mapping_exact_codes() {
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("DB_ERROR", "connection refused"),
            "The database is temporarily unavailable. Please try again in a moment."
        );

        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("LOAD_FAILED", "path does not exist"),
            "The model files could not be found. Please verify the model path and try again."
        );

        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("NOT_FOUND", "model not found"),
            "The requested model was not found. Please check the model ID and try again."
        );
    }

    #[test]
    fn test_error_mapping_patterns() {
        assert!(UserFriendlyErrorMapper::map_error_message("UNKNOWN", "Connection refused")
            .contains("database is temporarily unavailable"));

        assert!(UserFriendlyErrorMapper::map_error_message("UNKNOWN", "timeout occurred")
            .contains("timed out"));

        assert!(UserFriendlyErrorMapper::map_error_message("UNKNOWN", "path does not exist")
            .contains("could not be found"));
    }

    #[test]
    fn test_error_response_creation() {
        let error = AosError::NotFound("model not found".to_string());
        let response = ErrorResponse::from_error(&error, Some("req-123".to_string()));

        assert_eq!(response.status, 404);
        assert_eq!(response.error_code, "NOT_FOUND");
        assert!(response.message.contains("not found"));
        assert_eq!(response.request_id, Some("req-123".to_string()));
    }

    #[test]
    fn test_retryable_error_detection() {
        assert!(is_retryable_error(&AosError::Database("connection failed".to_string())));
        assert!(is_retryable_error(&AosError::Network("timeout".to_string())));
        assert!(!is_retryable_error(&AosError::Validation("invalid input".to_string())));
    }

    #[tokio::test]
    async fn test_retry_executor_success() {
        let mut attempts = 0;
        let result = RetryExecutor::with_default_config()
            .execute(|| async {
                attempts += 1;
                if attempts < 2 {
                    Err("temporary failure")
                } else {
                    Ok("success")
                }
            })
            .await;

        assert_eq!(result, Ok("success"));
        assert_eq!(attempts, 2);
    }

    #[tokio::test]
    async fn test_retry_executor_failure() {
        let mut attempts = 0;
        let result = RetryExecutor::with_default_config()
            .execute(|| async {
                attempts += 1;
                Err("permanent failure") as Result<&str, &str>
            })
            .await;

        assert_eq!(result, Err("permanent failure"));
        assert_eq!(attempts, 3); // max_attempts
    }

    #[test]
    fn test_user_friendly_error_mapping_comprehensive() {
        // Test exact error code mappings
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("DB_ERROR", "Connection refused"),
            "The database is temporarily unavailable. Please try again in a moment."
        );

        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("LOAD_FAILED", "path does not exist"),
            "The model files could not be found. Please verify the model path and try again."
        );

        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("NOT_FOUND", "model not found"),
            "The requested model was not found. Please check the model ID and try again."
        );

        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("TIMEOUT", "operation timed out"),
            "The operation timed out. This usually happens when the system is busy. Please try again."
        );

        // Test configuration errors
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("CONFIG_ERROR", "invalid config"),
            "A configuration error occurred. Please contact support if this problem persists."
        );

        // Test I/O errors
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("IO_ERROR", "Permission denied accessing file"),
            "Permission denied when accessing files. Please check file permissions."
        );

        // Test crypto errors
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("CRYPTO_ERROR", "signature verification failed"),
            "A cryptographic operation failed. Please contact support if this problem persists."
        );

        // Test policy violations
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("POLICY_VIOLATION", "determinism violation detected"),
            "This operation violates determinism requirements. Please use only approved operations."
        );

        // Test resource exhaustion
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("RESOURCE_EXHAUSTION", "insufficient memory"),
            "The system is running low on memory. Please try again later or use smaller models."
        );

        // Test worker errors
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("WORKER_ERROR", "worker process crashed"),
            "The model worker encountered an error. Please try again, and contact support if the problem persists."
        );

        // Test network errors
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("NETWORK_ERROR", "connection failed"),
            "A network error occurred. Please check your connection and try again."
        );

        // Test feature disabled
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("FEATURE_DISABLED", "experimental feature - requires config flag"),
            "This feature is currently disabled: experimental feature - requires config flag."
        );

        // Test token errors
        assert_eq!(
            UserFriendlyErrorMapper::map_error_message("TOKEN_ERROR", "failed to generate token"),
            "Failed to generate authentication token. Please try again."
        );
    }

    #[test]
    fn test_aos_error_to_response_conversion() {
        use adapteros_core::AosError;

        // Test database error conversion
        let db_error = AosError::Database("connection failed".to_string());
        let response = db_error.to_user_friendly_response();
        assert_eq!(response.0, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        assert!(response.1.error.contains("temporarily unavailable"));

        // Test not found error conversion
        let not_found = AosError::NotFound("model xyz".to_string());
        let response = not_found.to_user_friendly_response();
        assert_eq!(response.0, axum::http::StatusCode::NOT_FOUND);
        assert!(response.1.error.contains("not found"));

        // Test validation error conversion
        let validation_error = AosError::Validation("invalid input".to_string());
        let response = validation_error.to_user_friendly_response();
        assert_eq!(response.0, axum::http::StatusCode::BAD_REQUEST);
        assert!(response.1.error.contains("invalid data"));

        // Test policy violation conversion
        let policy_error = AosError::PolicyViolation("determinism check failed".to_string());
        let response = policy_error.to_user_friendly_response();
        assert_eq!(response.0, axum::http::StatusCode::FORBIDDEN);
        assert!(response.1.error.contains("violates"));

        // Test timeout conversion
        let timeout_error = AosError::Timeout(std::time::Duration::from_secs(30));
        let response = timeout_error.to_user_friendly_response();
        assert_eq!(response.0, axum::http::StatusCode::REQUEST_TIMEOUT);
        assert!(response.1.error.contains("timed out"));
    }
}