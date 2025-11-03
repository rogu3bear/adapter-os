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

/// User-friendly error message mapper
pub struct UserFriendlyErrorMapper;

impl UserFriendlyErrorMapper {
    /// Map technical error messages to user-friendly messages
    pub fn map_error_message(error_code: &str, technical_message: &str) -> String {
        match error_code {
            "DB_ERROR" => Self::map_database_errors(technical_message),
            "LOAD_FAILED" => Self::map_load_errors(technical_message),
            "TIMEOUT" => Self::map_timeout_errors(technical_message),
            "INSUFFICIENT_MEMORY" => Self::map_memory_errors(technical_message),
            "GLOBAL_MODEL_LIMIT_EXCEEDED" | "TENANT_MODEL_LIMIT_EXCEEDED" => {
                Self::map_limit_errors(technical_message, error_code)
            }
            "OPERATION_IN_PROGRESS" => Self::map_operation_in_progress_errors(technical_message),
            "NOT_FOUND" => Self::map_not_found_errors(technical_message),
            "UNAUTHORIZED" => Self::map_auth_errors(technical_message),
            "VALIDATION_ERROR" => Self::map_validation_errors(technical_message),
            _ => Self::map_generic_errors(technical_message),
        }
    }

    fn map_database_errors(technical_message: &str) -> String {
        if technical_message.contains("Connection refused") {
            "The database is temporarily unavailable. Please try again in a moment.".to_string()
        } else if technical_message.contains("timeout") {
            "The database operation timed out. Please try again.".to_string()
        } else if technical_message.contains("deadlock") {
            "The system is busy processing other requests. Please try again.".to_string()
        } else {
            "A database error occurred. Please try again or contact support if the problem persists.".to_string()
        }
    }

    fn map_load_errors(technical_message: &str) -> String {
        if technical_message.contains("path does not exist") {
            "The model files could not be found. Please verify the model path and try again.".to_string()
        } else if technical_message.contains("insufficient memory") || technical_message.contains("out of memory") {
            "Not enough memory available to load this model. Try unloading other models first or use a smaller model.".to_string()
        } else if technical_message.contains("timeout") {
            "Loading the model took too long. This might be due to a large model or system load. Please try again.".to_string()
        } else {
            "Failed to load the model. Please check that the model files are valid and try again.".to_string()
        }
    }

    fn map_timeout_errors(_technical_message: &str) -> String {
        "The operation timed out. This usually happens when the system is busy. Please try again.".to_string()
    }

    fn map_memory_errors(technical_message: &str) -> String {
        if technical_message.contains("estimated") {
            "Not enough memory available to load this model. Try unloading other models first.".to_string()
        } else {
            "Insufficient memory to complete this operation. Please free up system resources and try again.".to_string()
        }
    }

    fn map_limit_errors(technical_message: &str, error_code: &str) -> String {
        let limit_type = if error_code.contains("GLOBAL") { "global model" } else { "tenant model" };
        format!("You've reached the maximum number of {} models that can be loaded. Please unload some models before loading new ones.", limit_type)
    }

    fn map_operation_in_progress_errors(technical_message: &str) -> String {
        if technical_message.contains("loading") {
            "This model is currently being loaded. Please wait for the loading to complete.".to_string()
        } else if technical_message.contains("unloading") {
            "This model is currently being unloaded. Please wait for the unloading to complete.".to_string()
        } else {
            "This operation is already in progress. Please wait for it to complete before trying again.".to_string()
        }
    }

    fn map_not_found_errors(technical_message: &str) -> String {
        if technical_message.contains("model") {
            "The requested model was not found. Please check the model ID and try again.".to_string()
        } else if technical_message.contains("tenant") {
            "Your account or tenant was not found. Please contact support.".to_string()
        } else {
            "The requested resource was not found. Please check your request and try again.".to_string()
        }
    }

    fn map_auth_errors(technical_message: &str) -> String {
        if technical_message.contains("expired") {
            "Your session has expired. Please log in again.".to_string()
        } else if technical_message.contains("invalid") {
            "Your credentials are invalid. Please check and try again.".to_string()
        } else if technical_message.contains("permission") || technical_message.contains("role") {
            "You don't have permission to perform this action. Please contact your administrator.".to_string()
        } else {
            "Authentication failed. Please log in and try again.".to_string()
        }
    }

    fn map_validation_errors(technical_message: &str) -> String {
        if technical_message.contains("field") {
            "Some required information is missing or invalid. Please check your input and try again.".to_string()
        } else if technical_message.contains("format") {
            "The provided data is in an invalid format. Please check the documentation and try again.".to_string()
        } else {
            "The provided information is invalid. Please check your input and try again.".to_string()
        }
    }

    fn map_generic_errors(technical_message: &str) -> String {
        if technical_message.len() < 50 {
            format!("An error occurred: {}. Please try again or contact support.", technical_message.to_lowercase())
        } else {
            "An unexpected error occurred. Please try again, and contact support if the problem persists.".to_string()
        }
    }
}

impl UserFriendlyErrorMapper {
    /// Map technical error messages to user-friendly messages
    pub fn map_error_message(error_code: &str, technical_message: &str) -> String {
        match error_code {
            "DB_ERROR" => Self::map_database_errors(technical_message),
            "LOAD_FAILED" => Self::map_load_errors(technical_message),
            "TIMEOUT" => Self::map_timeout_errors(technical_message),
            "INSUFFICIENT_MEMORY" => Self::map_memory_errors(technical_message),
            "GLOBAL_MODEL_LIMIT_EXCEEDED" => Self::map_limit_errors(technical_message, "global model"),
            "TENANT_MODEL_LIMIT_EXCEEDED" => Self::map_limit_errors(technical_message, "tenant model"),
            "OPERATION_IN_PROGRESS" => Self::map_operation_in_progress_errors(technical_message),
            "NOT_FOUND" => Self::map_not_found_errors(technical_message),
            "UNAUTHORIZED" => Self::map_auth_errors(technical_message),
            "VALIDATION_ERROR" => Self::map_validation_errors(technical_message),
            _ => Self::map_generic_errors(technical_message),
        }
    }

    /// Map database-related errors to user-friendly messages
    fn map_database_errors(technical_message: &str) -> String {
        if technical_message.contains("Connection refused") {
            "The database is temporarily unavailable. Please try again in a moment.".to_string()
        } else if technical_message.contains("timeout") {
            "The database operation timed out. Please try again.".to_string()
        } else if technical_message.contains("deadlock") {
            "The system is busy processing other requests. Please try again.".to_string()
        } else if technical_message.contains("constraint") {
            "A data validation error occurred. Please check your input and try again.".to_string()
        } else {
            "A database error occurred. Please try again or contact support if the problem persists.".to_string()
        }
    }

    /// Map model loading errors to user-friendly messages
    fn map_load_errors(technical_message: &str) -> String {
        if technical_message.contains("path does not exist") {
            "The model files could not be found. Please verify the model path and try again.".to_string()
        } else if technical_message.contains("insufficient memory") || technical_message.contains("out of memory") {
            "Not enough memory available to load this model. Try unloading other models first or use a smaller model.".to_string()
        } else if technical_message.contains("timeout") {
            "Loading the model took too long. This might be due to a large model or system load. Please try again.".to_string()
        } else if technical_message.contains("corrupted") || technical_message.contains("invalid") {
            "The model file appears to be corrupted or invalid. Please verify the model integrity.".to_string()
        } else if technical_message.contains("permission") {
            "Permission denied when trying to access model files. Please check file permissions.".to_string()
        } else {
            "Failed to load the model. Please check that the model files are valid and try again.".to_string()
        }
    }

    /// Map timeout errors to user-friendly messages
    fn map_timeout_errors(_technical_message: &str) -> String {
        "The operation timed out. This usually happens when the system is busy. Please try again.".to_string()
    }

    /// Map memory-related errors to user-friendly messages
    fn map_memory_errors(technical_message: &str) -> String {
        if technical_message.contains("estimated") {
            "Not enough memory available to load this model. Try unloading other models first.".to_string()
        } else {
            "Insufficient memory to complete this operation. Please free up system resources and try again.".to_string()
        }
    }

    /// Map limit-related errors to user-friendly messages
    fn map_limit_errors(technical_message: &str, limit_type: &str) -> String {
        format!("You've reached the maximum number of {} models that can be loaded. Please unload some models before loading new ones.", limit_type)
    }

    /// Map operation in progress errors to user-friendly messages
    fn map_operation_in_progress_errors(technical_message: &str) -> String {
        if technical_message.contains("loading") {
            "This model is currently being loaded. Please wait for the loading to complete.".to_string()
        } else if technical_message.contains("unloading") {
            "This model is currently being unloaded. Please wait for the unloading to complete.".to_string()
        } else {
            "This operation is already in progress. Please wait for it to complete before trying again.".to_string()
        }
    }

    /// Map not found errors to user-friendly messages
    fn map_not_found_errors(technical_message: &str) -> String {
        if technical_message.contains("model") {
            "The requested model was not found. Please check the model ID and try again.".to_string()
        } else if technical_message.contains("tenant") {
            "Your account or tenant was not found. Please contact support.".to_string()
        } else {
            "The requested resource was not found. Please check your request and try again.".to_string()
        }
    }

    /// Map authentication errors to user-friendly messages
    fn map_auth_errors(technical_message: &str) -> String {
        if technical_message.contains("expired") {
            "Your session has expired. Please log in again.".to_string()
        } else if technical_message.contains("invalid") {
            "Your credentials are invalid. Please check and try again.".to_string()
        } else if technical_message.contains("permission") || technical_message.contains("role") {
            "You don't have permission to perform this action. Please contact your administrator.".to_string()
        } else {
            "Authentication failed. Please log in and try again.".to_string()
        }
    }

    /// Map validation errors to user-friendly messages
    fn map_validation_errors(technical_message: &str) -> String {
        if technical_message.contains("field") {
            "Some required information is missing or invalid. Please check your input and try again.".to_string()
        } else if technical_message.contains("format") {
            "The provided data is in an invalid format. Please check the documentation and try again.".to_string()
        } else {
            "The provided information is invalid. Please check your input and try again.".to_string()
        }
    }

    /// Map generic errors to user-friendly messages
    fn map_generic_errors(technical_message: &str) -> String {
        // For errors we don't specifically handle, provide a generic but helpful message
        if technical_message.len() < 50 {
            // If the technical message is short, we might keep it but make it friendlier
            format!("An error occurred: {}. Please try again or contact support.", technical_message.to_lowercase())
        } else {
            "An unexpected error occurred. Please try again, and contact support if the problem persists.".to_string()
        }
    }
}

