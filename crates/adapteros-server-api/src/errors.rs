//! Authentication and API error types
//!
//! This module provides comprehensive error handling for authentication
//! and API operations with proper HTTP status code mapping.
//!
//! Citations:
//! - CLAUDE.md: Error handling and validation patterns
//! - crates/adapteros-core/src/lib.rs: Core error types

use crate::types::ErrorResponse;
use axum::http::StatusCode;
use axum::Json;
use thiserror::Error;

/// Authentication-specific errors
#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Invalid token: {0}")]
    InvalidToken(String),

    #[error("Token expired at {0}")]
    TokenExpired(String),

    #[error("Token refresh failed: {0}")]
    RefreshFailed(String),

    #[error("Authentication required")]
    AuthenticationRequired,

    #[error("Insufficient permissions: required {required}, has {actual}")]
    InsufficientPermissions { required: String, actual: String },

    #[error("Rate limit exceeded: {0} attempts")]
    RateLimitExceeded(u32),

    #[error("Account locked until {0}")]
    AccountLocked(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Missing authorization header")]
    MissingAuthHeader,

    #[error("Invalid authorization format: {0}")]
    InvalidAuthFormat(String),
}

impl AuthError {
    /// Convert to HTTP status code
    pub fn status_code(&self) -> StatusCode {
        match self {
            AuthError::InvalidToken(_) => StatusCode::UNAUTHORIZED,
            AuthError::TokenExpired(_) => StatusCode::UNAUTHORIZED,
            AuthError::RefreshFailed(_) => StatusCode::UNAUTHORIZED,
            AuthError::AuthenticationRequired => StatusCode::UNAUTHORIZED,
            AuthError::InsufficientPermissions { .. } => StatusCode::FORBIDDEN,
            AuthError::RateLimitExceeded(_) => StatusCode::TOO_MANY_REQUESTS,
            AuthError::AccountLocked(_) => StatusCode::FORBIDDEN,
            AuthError::InvalidCredentials => StatusCode::UNAUTHORIZED,
            AuthError::MissingAuthHeader => StatusCode::UNAUTHORIZED,
            AuthError::InvalidAuthFormat(_) => StatusCode::BAD_REQUEST,
        }
    }

    /// Convert to error code for API responses
    pub fn error_code(&self) -> &'static str {
        match self {
            AuthError::InvalidToken(_) => "INVALID_TOKEN",
            AuthError::TokenExpired(_) => "TOKEN_EXPIRED",
            AuthError::RefreshFailed(_) => "REFRESH_FAILED",
            AuthError::AuthenticationRequired => "AUTH_REQUIRED",
            AuthError::InsufficientPermissions { .. } => "INSUFFICIENT_PERMISSIONS",
            AuthError::RateLimitExceeded(_) => "RATE_LIMIT_EXCEEDED",
            AuthError::AccountLocked(_) => "ACCOUNT_LOCKED",
            AuthError::InvalidCredentials => "INVALID_CREDENTIALS",
            AuthError::MissingAuthHeader => "MISSING_AUTH_HEADER",
            AuthError::InvalidAuthFormat(_) => "INVALID_AUTH_FORMAT",
        }
    }
}

/// Convert AuthError to HTTP response
impl From<AuthError> for (StatusCode, Json<ErrorResponse>) {
    fn from(err: AuthError) -> Self {
        let status = err.status_code();
        let response = ErrorResponse::new(&err.to_string())
            .with_code(err.error_code())
            .with_string_details(&err.to_string());

        (status, Json(response))
    }
}

/// API validation errors
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid field value: {field} - {reason}")]
    InvalidValue { field: String, reason: String },

    #[error("Validation failed: {0}")]
    ValidationFailed(String),
}

impl ValidationError {
    pub fn status_code(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }

    pub fn error_code(&self) -> &'static str {
        "VALIDATION_ERROR"
    }
}

impl From<ValidationError> for (StatusCode, Json<ErrorResponse>) {
    fn from(err: ValidationError) -> Self {
        let status = err.status_code();
        let response = ErrorResponse::new(&err.to_string())
            .with_code(err.error_code())
            .with_string_details(&err.to_string());

        (status, Json(response))
    }
}

/// Result type for authentication operations
pub type AuthResult<T> = Result<T, AuthError>;

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
