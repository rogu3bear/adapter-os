//! Error types for Control Plane client operations

use thiserror::Error;

/// Result type alias for Control Plane client operations
pub type Result<T> = std::result::Result<T, WorkerCpError>;

/// Errors that can occur during worker-to-control-plane communication
#[derive(Error, Debug)]
pub enum WorkerCpError {
    /// Network-level error (connection refused, DNS failure, etc.)
    #[error("Network error: {message}")]
    Network {
        message: String,
        /// Whether this error is transient and can be retried
        is_transient: bool,
    },

    /// Authentication/authorization error (401/403)
    #[error("Authentication error: {message} (status: {status_code})")]
    Auth { message: String, status_code: u16 },

    /// Invalid response from server (malformed JSON, unexpected format)
    #[error("Invalid response: {message}")]
    InvalidResponse { message: String, body: String },

    /// Server error (5xx responses)
    #[error("Server error: {message} (status: {status_code})")]
    ServerError {
        message: String,
        status_code: u16,
        /// Whether this error can be retried (502/503/504 are retryable)
        is_retryable: bool,
    },

    /// Client error (4xx responses, excluding auth)
    #[error("Client error: {message} (status: {status_code})")]
    ClientError { message: String, status_code: u16 },

    /// Request timeout
    #[error("Request timed out after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    /// Worker registration was rejected by control plane
    #[error("Registration rejected: {reason}")]
    Rejected { reason: String },

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Request building error
    #[error("Request error: {0}")]
    Request(String),
}

impl WorkerCpError {
    /// Returns true if this error is potentially transient and can be retried
    pub fn is_retryable(&self) -> bool {
        match self {
            WorkerCpError::Network { is_transient, .. } => *is_transient,
            WorkerCpError::Timeout { .. } => true,
            WorkerCpError::ServerError { is_retryable, .. } => *is_retryable,
            // Auth, client errors, rejection, config, and invalid responses are not retryable
            WorkerCpError::Auth { .. }
            | WorkerCpError::InvalidResponse { .. }
            | WorkerCpError::ClientError { .. }
            | WorkerCpError::Rejected { .. }
            | WorkerCpError::Config(_)
            | WorkerCpError::Request(_) => false,
        }
    }

    /// Create a network error
    pub fn network(message: impl Into<String>, is_transient: bool) -> Self {
        WorkerCpError::Network {
            message: message.into(),
            is_transient,
        }
    }

    /// Create a timeout error
    pub fn timeout(duration_ms: u64) -> Self {
        WorkerCpError::Timeout { duration_ms }
    }

    /// Create a server error from HTTP status code
    pub fn from_status(status_code: u16, message: impl Into<String>) -> Self {
        let message = message.into();

        match status_code {
            401 | 403 => WorkerCpError::Auth {
                message,
                status_code,
            },
            400..=499 => WorkerCpError::ClientError {
                message,
                status_code,
            },
            502 | 503 | 504 => WorkerCpError::ServerError {
                message,
                status_code,
                is_retryable: true,
            },
            500..=599 => WorkerCpError::ServerError {
                message,
                status_code,
                is_retryable: false,
            },
            _ => WorkerCpError::ServerError {
                message,
                status_code,
                is_retryable: false,
            },
        }
    }
}

impl From<reqwest::Error> for WorkerCpError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            WorkerCpError::Timeout {
                duration_ms: 0, // We don't have the exact duration from reqwest
            }
        } else if err.is_connect() {
            WorkerCpError::Network {
                message: format!("Connection failed: {}", err),
                is_transient: true,
            }
        } else if err.is_request() {
            WorkerCpError::Request(err.to_string())
        } else {
            WorkerCpError::Network {
                message: err.to_string(),
                is_transient: false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_retryable() {
        assert!(WorkerCpError::network("test", true).is_retryable());
        assert!(!WorkerCpError::network("test", false).is_retryable());
        assert!(WorkerCpError::timeout(1000).is_retryable());

        assert!(WorkerCpError::from_status(502, "bad gateway").is_retryable());
        assert!(WorkerCpError::from_status(503, "unavailable").is_retryable());
        assert!(WorkerCpError::from_status(504, "timeout").is_retryable());

        assert!(!WorkerCpError::from_status(500, "internal").is_retryable());
        assert!(!WorkerCpError::from_status(400, "bad request").is_retryable());
        assert!(!WorkerCpError::from_status(401, "unauthorized").is_retryable());
    }

    #[test]
    fn test_from_status() {
        let err = WorkerCpError::from_status(401, "unauthorized");
        assert!(matches!(
            err,
            WorkerCpError::Auth {
                status_code: 401,
                ..
            }
        ));

        let err = WorkerCpError::from_status(400, "bad request");
        assert!(matches!(
            err,
            WorkerCpError::ClientError {
                status_code: 400,
                ..
            }
        ));

        let err = WorkerCpError::from_status(503, "unavailable");
        assert!(matches!(
            err,
            WorkerCpError::ServerError {
                status_code: 503,
                is_retryable: true,
                ..
            }
        ));
    }
}
