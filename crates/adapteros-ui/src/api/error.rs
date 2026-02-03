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
        hint: Option<String>,
        details: Option<serde_json::Value>,
    },
}

impl ApiError {
    /// Create from HTTP status and body
    pub fn from_response(status: u16, body: &str) -> Self {
        // Try to parse as ErrorResponse
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(body) {
            return Self::Structured {
                error: err.message,
                code: err.code.clone(),
                failure_code: err
                    .failure_code
                    .or_else(|| FailureCode::parse_code(&err.code)),
                hint: err.hint,
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
        match self {
            Self::Unauthorized => true,
            Self::Structured { code, .. } => matches!(
                code.as_str(),
                "UNAUTHORIZED"
                    | "TOKEN_EXPIRED"
                    | "TOKEN_REVOKED"
                    | "INVALID_TOKEN"
                    | "MISSING_AUTH"
                    | "AUTHENTICATION_ERROR"
            ),
            _ => false,
        }
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Network(_) | Self::RateLimited { .. } | Self::Server(_) => true,
            Self::Structured {
                failure_code: Some(code),
                ..
            } => code.is_retryable(),
            _ => false,
        }
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

    /// Get any server-provided hint for the error.
    pub fn hint(&self) -> Option<&str> {
        match self {
            Self::Structured { hint, .. } => hint.as_deref(),
            _ => None,
        }
    }

    /// Check if this error has a specific failure code
    pub fn has_failure_code(&self, code: FailureCode) -> bool {
        self.failure_code() == Some(code)
    }

    /// Check if this is an in-flight adapter error (HTTP 409 with ADAPTER_IN_FLIGHT code)
    pub fn is_adapter_in_flight(&self) -> bool {
        self.code() == Some("ADAPTER_IN_FLIGHT")
    }

    /// Get user-friendly message for display.
    ///
    /// Returns a context-appropriate message for specific error codes,
    /// falling back to the standard error message for others. This method is the
    /// single source of truth for user-facing API error copy.
    pub fn user_message(&self) -> String {
        if self.is_adapter_in_flight() {
            "This adapter is currently in use for inference. Please wait for active requests to complete before making changes.".to_string()
        } else {
            match self {
                Self::Unauthorized => "Session expired. Log in again.".to_string(),
                Self::Forbidden(_) => {
                    "You don't have access to this action. Contact an admin if you need access."
                        .to_string()
                }
                Self::NotFound(_) => "Not found. Check the URL or try again.".to_string(),
                Self::Validation(_) => {
                    "Some fields are invalid. Fix the highlighted fields and retry.".to_string()
                }
                Self::Network(_) => {
                    "Can't reach the server. Check your connection and retry.".to_string()
                }
                Self::RateLimited { retry_after } => retry_after
                    .map(|ms| {
                        let secs = (ms / 1000).max(1);
                        format!("Too many requests. Retry in {}s.", secs)
                    })
                    .unwrap_or_else(|| "Too many requests. Retry in a moment.".to_string()),
                Self::Server(_) => "Server error. Retry in a moment.".to_string(),
                Self::Http { status, .. } => match status {
                    401 => "Session expired. Log in again.".to_string(),
                    403 => {
                        "You don't have access to this action. Contact an admin if you need access."
                            .to_string()
                    }
                    404 => "Not found. Check the URL or try again.".to_string(),
                    502 => "Upstream service unavailable. Retry in a moment.".to_string(),
                    503 => "Service temporarily unavailable. Retry in a moment.".to_string(),
                    504 => "Request timed out. Retry in a moment.".to_string(),
                    _ => self.to_string(),
                },
                Self::Structured {
                    error,
                    code,
                    failure_code,
                    hint,
                    ..
                } => {
                    let base = user_message_for_code(code, *failure_code, error);
                    apply_hint(base, hint.as_deref())
                }
                _ => self.to_string(),
            }
        }
    }
}

fn apply_hint(mut message: String, hint: Option<&str>) -> String {
    let Some(hint) = hint.map(str::trim).filter(|h| !h.is_empty()) else {
        return message;
    };
    if !message.ends_with('.') {
        message.push('.');
    }
    message.push_str(" Next: ");
    message.push_str(hint);
    message
}

fn user_message_for_code(
    code: &str,
    failure_code: Option<FailureCode>,
    error: &str,
) -> String {
    let message = match code {
        "UNAUTHORIZED" | "TOKEN_EXPIRED" | "TOKEN_REVOKED" | "INVALID_TOKEN" | "MISSING_AUTH" => {
            "Session expired. Log in again.".to_string()
        }
        "FORBIDDEN" | "PERMISSION_DENIED" | "AUTHORIZATION_ERROR" | "POLICY_VIOLATION" => {
            "You don't have access to this action. Contact an admin if you need access.".to_string()
        }
        "NOT_FOUND" | "ENDPOINT_NOT_FOUND" | "MODEL_NOT_FOUND" | "ADAPTER_NOT_FOUND" => {
            "Not found. Check the URL or try again.".to_string()
        }
        "WORKER_NOT_RESPONDING" | "NO_COMPATIBLE_WORKER" | "WORKER_DEGRADED" => {
            "Worker unavailable. Retry in a moment or check worker health.".to_string()
        }
        "MODEL_NOT_READY" | "ADAPTER_NOT_LOADED" | "ADAPTER_NOT_LOADABLE" => {
            "Model not ready. Retry in a moment or check model loading status.".to_string()
        }
        "CACHE_BUDGET_EXCEEDED" => {
            "Model cache is full. Free resources or retry later.".to_string()
        }
        "BACKPRESSURE" | "MEMORY_PRESSURE" | "OUT_OF_MEMORY" => {
            "System is under memory pressure. Reduce request size or retry later.".to_string()
        }
        "GPU_UNAVAILABLE" => "GPU unavailable. Retry in a moment or check worker health.".to_string(),
        "SERVICE_UNAVAILABLE" | "BAD_GATEWAY" | "NETWORK_ERROR" | "CIRCUIT_BREAKER_OPEN" => {
            "Service temporarily unavailable. Retry in a moment.".to_string()
        }
        "REQUEST_TIMEOUT" | "GATEWAY_TIMEOUT" | "TIMEOUT" => {
            "Request timed out. Retry in a moment.".to_string()
        }
        _ => {
            let lower = error.to_lowercase();
            if lower.contains("inference failed") {
                "Inference failed. Retry in a moment or check worker health.".to_string()
            } else {
                error.to_string()
            }
        }
    };

    if let Some(code) = failure_code {
        match code {
            FailureCode::OutOfMemory => {
                return "System is out of memory. Reduce request size or retry later.".to_string();
            }
            FailureCode::WorkerOverloaded
            | FailureCode::CpuThrottled
            | FailureCode::ThreadPoolSaturated
            | FailureCode::GpuUnavailable => {
                return "Workers are at capacity. Retry in a moment or check worker health."
                    .to_string();
            }
            FailureCode::TenantAccessDenied => {
                return "You don't have access to this workspace. Contact an admin if you need access."
                    .to_string();
            }
            FailureCode::ModelLoadFailed => {
                return "Model failed to load. Retry in a moment or check worker logs.".to_string();
            }
            FailureCode::BackendFallback => {
                return "No compatible worker available. Check model availability or retry later."
                    .to_string();
            }
            _ => {}
        }
    }

    message
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

    #[test]
    fn test_is_adapter_in_flight() {
        let body = r#"{"error":"Adapter is currently in use","code":"ADAPTER_IN_FLIGHT"}"#;
        let error = ApiError::from_response(409, body);

        assert!(error.is_adapter_in_flight());
        assert!(error
            .user_message()
            .contains("currently in use for inference"));
    }

    #[test]
    fn test_is_adapter_in_flight_false_for_other_errors() {
        let error = ApiError::Validation("Invalid input".to_string());
        assert!(!error.is_adapter_in_flight());
    }

    #[test]
    fn test_user_message_regular_error() {
        let error = ApiError::NotFound("Resource not found".to_string());
        assert_eq!(
            error.user_message(),
            "Not found. Check the URL or try again."
        );
    }

    #[test]
    fn test_requires_auth_for_structured_unauthorized() {
        let error = ApiError::Structured {
            error: "unauthorized".to_string(),
            code: "UNAUTHORIZED".to_string(),
            failure_code: None,
            hint: None,
            details: None,
        };
        assert!(error.requires_auth());
    }

    #[test]
    fn test_user_message_applies_hint() {
        let error = ApiError::Structured {
            error: "Service unavailable".to_string(),
            code: "SERVICE_UNAVAILABLE".to_string(),
            failure_code: None,
            hint: Some("retry in a moment".to_string()),
            details: None,
        };
        assert_eq!(
            error.user_message(),
            "Service temporarily unavailable. Retry in a moment. Next: retry in a moment"
        );
    }
}
