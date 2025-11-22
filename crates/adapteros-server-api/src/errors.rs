//! User-friendly error handling for AdapterOS API
//!
//! Provides automatic mapping of technical error messages to user-friendly,
//! actionable messages. Supports retry logic and enhanced error responses.
//!
//! Citations:
//! - Error message mapping: Based on UX improvements demo【1†demo_ux_improvements.rs】
//! - Error response enhancement: Extends existing ErrorResponse pattern【2†adapteros-server-api/src/handlers.rs】
//! - Retry logic: Implements exponential backoff for transient errors【3†demo_ux_improvements.rs:6-10】

use adapteros_api_types::{ErrorResponse, API_SCHEMA_VERSION};
use adapteros_core::AosError;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{Map, Value};
use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

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
            return "The database is temporarily unavailable. Please try again in a moment."
                .to_string();
        }

        if message.contains("timeout") || message.contains("timed out") {
            return "The operation timed out. This usually happens when the system is busy. Please try again.".to_string();
        }

        // File/path errors
        if message.contains("path does not exist")
            || message.contains("file not found")
            || message.contains("no such file")
        {
            return "The model files could not be found. Please verify the model path and try again.".to_string();
        }

        if message.contains("permission denied") || message.contains("access denied") {
            return "Permission denied accessing the requested files. Please contact your administrator.".to_string();
        }

        // Model errors
        if message.contains("model not found") || message.contains("adapter not found") {
            return "The requested model was not found. Please check the model ID and try again."
                .to_string();
        }

        if message.contains("invalid model") || message.contains("corrupted model") {
            return "The model file appears to be corrupted or invalid. Please try a different model.".to_string();
        }

        // Memory/resource errors
        if message.contains("out of memory") || message.contains("memory allocation failed") {
            return "The system is running low on memory. Please try again later or with a smaller model.".to_string();
        }

        if message.contains("disk full") || message.contains("no space left") {
            return "The system is running low on disk space. Please contact your administrator."
                .to_string();
        }

        // Network errors
        if message.contains("network") || message.contains("dns") || message.contains("connection")
        {
            return "Network connection issue. Please check your connection and try again."
                .to_string();
        }

        // Validation errors
        if message.contains("validation") || message.contains("invalid") {
            return "The request contains invalid data. Please check your input and try again."
                .to_string();
        }

        // Rate limiting
        if message.contains("rate limit") || message.contains("too many requests") {
            return "Too many requests. Please wait a moment before trying again.".to_string();
        }

        // Fallback for unknown errors
        warn!("Unknown error pattern: {}", technical_message);
        "An unexpected error occurred. Please try again or contact support if the problem persists."
            .to_string()
    }
}

/// Extension methods for creating adapteros_api_types::ErrorResponse values
pub trait ErrorResponseExt {
    fn new_user_friendly<S: Into<String>>(
        error_code: &str,
        technical_message: S,
    ) -> adapteros_api_types::ErrorResponse;

    fn from_error(
        error: &AosError,
        request_id: Option<String>,
    ) -> adapteros_api_types::ErrorResponse;

    fn with_message<S: Into<String>>(
        status: StatusCode,
        error_code: &str,
        user_message: S,
        request_id: Option<String>,
    ) -> adapteros_api_types::ErrorResponse;
}

impl ErrorResponseExt for adapteros_api_types::ErrorResponse {
    fn new_user_friendly<S: Into<String>>(
        error_code: &str,
        technical_message: S,
    ) -> adapteros_api_types::ErrorResponse {
        let technical_message = technical_message.into();
        let user_friendly_message =
            UserFriendlyErrorMapper::map_error_message(error_code, &technical_message);

        let mut details = Map::new();
        details.insert(
            "technical_details".to_string(),
            Value::String(technical_message),
        );
        details.insert("user_friendly".to_string(), Value::Bool(true));

        adapteros_api_types::ErrorResponse {
            schema_version: API_SCHEMA_VERSION.to_string(),
            error: user_friendly_message,
            code: error_code.to_string(),
            details: Some(Value::Object(details)),
        }
    }

    fn from_error(
        error: &AosError,
        request_id: Option<String>,
    ) -> adapteros_api_types::ErrorResponse {
        let (status, error_code, technical_message) = error_to_components(error);
        let user_message =
            UserFriendlyErrorMapper::map_error_message(&error_code, &technical_message);

        let mut details = Map::new();
        details.insert(
            "status".to_string(),
            Value::Number(serde_json::Number::from(status.as_u16() as u64)),
        );
        details.insert(
            "technical_details".to_string(),
            Value::String(technical_message),
        );
        if let Some(request_id) = request_id {
            details.insert("request_id".to_string(), Value::String(request_id));
        }

        adapteros_api_types::ErrorResponse {
            schema_version: API_SCHEMA_VERSION.to_string(),
            error: user_message,
            code: error_code,
            details: Some(Value::Object(details)),
        }
    }

    fn with_message<S: Into<String>>(
        status: StatusCode,
        error_code: &str,
        user_message: S,
        request_id: Option<String>,
    ) -> adapteros_api_types::ErrorResponse {
        let mut details = Map::new();
        details.insert(
            "status".to_string(),
            Value::Number(serde_json::Number::from(status.as_u16() as u64)),
        );
        if let Some(request_id) = request_id {
            details.insert("request_id".to_string(), Value::String(request_id));
        }

        adapteros_api_types::ErrorResponse {
            schema_version: API_SCHEMA_VERSION.to_string(),
            error: user_message.into(),
            code: error_code.to_string(),
            details: if details.is_empty() {
                None
            } else {
                Some(Value::Object(details))
            },
        }
    }
}

/// Extension trait to convert `AosError` into user-friendly responses.
pub trait AosErrorExt {
    fn to_user_friendly_response(&self) -> (StatusCode, Json<adapteros_api_types::ErrorResponse>);
}

impl AosErrorExt for AosError {
    fn to_user_friendly_response(&self) -> (StatusCode, Json<adapteros_api_types::ErrorResponse>) {
        let (status, _, _) = error_to_components(self);
        let response = adapteros_api_types::ErrorResponse::from_error(self, None);
        (status, Json(response))
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

    pub async fn execute<F, Fut, T>(
        &self,
        mut operation: F,
    ) -> std::result::Result<T, anyhow::Error>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = std::result::Result<T, anyhow::Error>>,
    {
        let mut attempt = 0u32;
        let mut delay = self.config.initial_delay;

        loop {
            attempt += 1;
            match operation().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    if attempt >= self.config.max_attempts {
                        return Err(err);
                    }

                    let err_message = err.to_string();
                    warn!(
                        attempt,
                        max_attempts = self.config.max_attempts,
                        error = %err_message,
                        "Retryable operation failed; scheduling retry"
                    );

                    let jitter_ratio = self.config.jitter_factor.clamp(0.0, 1.0);
                    let jitter_direction = if attempt.is_multiple_of(2) { -1.0 } else { 1.0 };
                    let jitter_multiplier = 1.0 + (jitter_ratio * 0.5 * jitter_direction);
                    let sleep_duration =
                        delay.mul_f64(jitter_multiplier).min(self.config.max_delay);

                    sleep(sleep_duration).await;

                    let next_delay = delay.mul_f64(self.config.backoff_multiplier);
                    delay = next_delay.min(self.config.max_delay);
                }
            }
        }
    }

    /// Execute operation with progress callback support
    ///
    /// # Citations
    /// - Progress tracking: [source: crates/adapteros-server-api/src/operation_tracker.rs L315-340]
    /// - Retry logic: [source: crates/adapteros-server-api/src/errors.rs L277-315]
    pub async fn execute_with_progress<F, Fut, T, P>(
        &self,
        mut operation: P,
    ) -> std::result::Result<T, anyhow::Error>
    where
        P: FnMut(u32, u32) -> F,
        F: Future<Output = std::result::Result<T, anyhow::Error>>,
    {
        let mut attempt = 0u32;
        let mut delay = self.config.initial_delay;

        loop {
            attempt += 1;
            match operation(attempt, self.config.max_attempts).await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    if attempt >= self.config.max_attempts {
                        return Err(err);
                    }

                    let err_message = err.to_string();
                    warn!(
                        attempt,
                        max_attempts = self.config.max_attempts,
                        error = %err_message,
                        "Retryable operation failed; scheduling retry"
                    );

                    let jitter_ratio = self.config.jitter_factor.clamp(0.0, 1.0);
                    let jitter_direction = if attempt.is_multiple_of(2) { -1.0 } else { 1.0 };
                    let jitter_multiplier = 1.0 + (jitter_ratio * 0.5 * jitter_direction);
                    let sleep_duration =
                        delay.mul_f64(jitter_multiplier).min(self.config.max_delay);

                    sleep(sleep_duration).await;

                    let next_delay = delay.mul_f64(self.config.backoff_multiplier);
                    delay = next_delay.min(self.config.max_delay);
                }
            }
        }
    }
}

/// Result type for validation operations
pub type ValidationResult<T> =
    std::result::Result<T, (axum::http::StatusCode, axum::Json<ErrorResponse>)>;

// Note: IntoResponse implementation for ErrorResponse should be in adapteros_api_types crate

/// Extract status code, error code, and technical message from an AosError
pub fn error_to_components(error: &AosError) -> (axum::http::StatusCode, String, String) {
    use axum::http::StatusCode;

    match error {
        AosError::PolicyViolation(_) | AosError::Policy(_) => (
            StatusCode::FORBIDDEN,
            "POLICY_VIOLATION".to_string(),
            error.to_string(),
        ),
        AosError::Validation(_) => (
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR".to_string(),
            error.to_string(),
        ),
        AosError::NotFound(_) => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND".to_string(),
            error.to_string(),
        ),
        AosError::Sqlx(_) | AosError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "DATABASE_ERROR".to_string(),
            error.to_string(),
        ),
        AosError::Io(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "IO_ERROR".to_string(),
            error.to_string(),
        ),
        AosError::Crypto(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "CRYPTO_ERROR".to_string(),
            error.to_string(),
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL_ERROR".to_string(),
            error.to_string(),
        ),
    }
}

/// Determine if an error is retryable
pub fn is_retryable_error(error: &AosError) -> bool {
    match error {
        // Database errors might be transient
        AosError::Sqlx(_) | AosError::Database(_) => true,
        // IO errors might be transient
        AosError::Io(_) => true,
        // Network-related errors might be transient
        AosError::Network(_) => true,
        // Other errors are not retryable
        _ => false,
    }
}
