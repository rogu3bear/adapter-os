//! API error types
//!
//! Unified error handling for API requests.

use adapteros_api_types::{ErrorResponse, FailureCode};
use thiserror::Error;

/// Result type for API operations
pub type ApiResult<T> = Result<T, ApiError>;

/// API error types
#[derive(Debug, Error, Clone)]
pub enum ApiError {
    /// Request was aborted (user cancelled)
    #[error("Request aborted")]
    Aborted,

    /// Network error (connection failed, timeout, etc.)
    #[error("Network error: {0}")]
    Network(String),

    /// HTTP error with status code
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String },

    /// Authentication error (401)
    #[error("Authentication required")]
    Unauthorized,

    /// Authorization error (403)
    #[error("Access denied: {0}")]
    Forbidden(String),

    /// Not found error (404)
    #[error("Not found: {0}")]
    NotFound(String),

    /// Validation error (400/422)
    #[error("Validation error: {0}")]
    Validation(String),

    /// Server error (5xx)
    #[error("Server error: {0}")]
    Server(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Rate limited (429)
    #[error("Rate limited, retry after {retry_after:?}ms")]
    RateLimited { retry_after: Option<u64> },

    /// Structured error response from server
    #[error("{error}")]
    Structured {
        error: String,
        code: String,
        failure_code: Option<FailureCode>,
        details: Option<serde_json::Value>,
    },
}

impl ApiError {
    /// Create from HTTP status and body
    pub fn from_response(status: u16, body: &str) -> Self {
        // Try to parse as ErrorResponse
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(body) {
            return Self::Structured {
                error: err.error,
                code: err.code.clone(),
                failure_code: err
                    .failure_code
                    .or_else(|| FailureCode::parse_code(&err.code)),
                details: err.details,
            };
        }

        match status {
            401 => Self::Unauthorized,
            403 => Self::Forbidden(body.to_string()),
            404 => Self::NotFound(body.to_string()),
            400 | 422 => Self::Validation(body.to_string()),
            429 => Self::RateLimited { retry_after: None },
            500..=599 => Self::Server(body.to_string()),
            _ => Self::Http {
                status,
                message: body.to_string(),
            },
        }
    }

    /// Check if this error indicates the user should re-authenticate
    pub fn requires_auth(&self) -> bool {
        matches!(self, Self::Unauthorized)
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Network(_) | Self::RateLimited { .. } | Self::Server(_)
        )
    }

    /// Check if this error indicates the request was aborted
    pub fn is_aborted(&self) -> bool {
        matches!(self, Self::Aborted)
    }

    /// Get the error code if available
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Structured { code, .. } => Some(code),
            Self::Unauthorized => Some("UNAUTHORIZED"),
            Self::Forbidden(_) => Some("FORBIDDEN"),
            Self::NotFound(_) => Some("NOT_FOUND"),
            Self::Validation(_) => Some("VALIDATION_ERROR"),
            Self::RateLimited { .. } => Some("RATE_LIMITED"),
            Self::Server(_) => Some("SERVER_ERROR"),
            _ => None,
        }
    }

    /// Get the structured failure code if available
    pub fn failure_code(&self) -> Option<FailureCode> {
        match self {
            Self::Structured { failure_code, .. } => *failure_code,
            _ => None,
        }
    }

    /// Check if this error has a specific failure code
    pub fn has_failure_code(&self, code: FailureCode) -> bool {
        self.failure_code() == Some(code)
    }
}

impl From<gloo_net::Error> for ApiError {
    fn from(err: gloo_net::Error) -> Self {
        let msg = err.to_string();
        if msg.contains("AbortError") {
            Self::Aborted
        } else {
            Self::Network(msg)
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_response_401() {
        let error = ApiError::from_response(401, "Unauthorized");
        assert!(matches!(error, ApiError::Unauthorized));
        assert!(error.requires_auth());
        assert_eq!(error.code(), Some("UNAUTHORIZED"));
    }

    #[test]
    fn test_from_response_403() {
        let error = ApiError::from_response(403, "Access denied");
        assert!(matches!(error, ApiError::Forbidden(_)));
        assert!(!error.requires_auth());
        assert_eq!(error.code(), Some("FORBIDDEN"));
    }

    #[test]
    fn test_from_response_404() {
        let error = ApiError::from_response(404, "Resource not found");
        assert!(matches!(error, ApiError::NotFound(_)));
        assert_eq!(error.code(), Some("NOT_FOUND"));
    }

    #[test]
    fn test_from_response_429() {
        let error = ApiError::from_response(429, "Too many requests");
        assert!(matches!(error, ApiError::RateLimited { .. }));
        assert!(error.is_retryable());
    }

    #[test]
    fn test_from_response_500() {
        let error = ApiError::from_response(500, "Internal server error");
        assert!(matches!(error, ApiError::Server(_)));
        assert!(error.is_retryable());
        assert_eq!(error.code(), Some("SERVER_ERROR"));
    }

    #[test]
    fn test_from_response_structured() {
        // FailureCode uses SCREAMING_SNAKE_CASE serde format
        let body = r#"{"error":"Worker is overloaded","code":"WORKER_OVERLOADED","failure_code":"WORKER_OVERLOADED"}"#;
        let error = ApiError::from_response(503, body);

        assert!(matches!(error, ApiError::Structured { .. }));
        if let ApiError::Structured {
            error,
            code,
            failure_code,
            ..
        } = &error
        {
            assert_eq!(error, "Worker is overloaded");
            assert_eq!(code, "WORKER_OVERLOADED");
            assert_eq!(*failure_code, Some(FailureCode::WorkerOverloaded));
        }
    }

    #[test]
    fn test_from_response_structured_without_failure_code() {
        let body = r#"{"error":"Invalid request","code":"VALIDATION_FAILED"}"#;
        let error = ApiError::from_response(400, body);

        assert!(matches!(error, ApiError::Structured { .. }));
        if let ApiError::Structured { error, code, .. } = &error {
            assert_eq!(error, "Invalid request");
            assert_eq!(code, "VALIDATION_FAILED");
        }
    }

    #[test]
    fn test_network_error_retryable() {
        let error = ApiError::Network("Connection refused".to_string());
        assert!(error.is_retryable());
    }

    #[test]
    fn test_validation_error_not_retryable() {
        let error = ApiError::Validation("Invalid email".to_string());
        assert!(!error.is_retryable());
    }
}
