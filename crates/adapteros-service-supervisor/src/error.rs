//! Error types for the service supervisor

use adapteros_core::AosError;
use std::fmt;

/// Result type alias for supervisor operations
pub type Result<T> = std::result::Result<T, SupervisorError>;

/// Comprehensive error types for the service supervisor
#[derive(Debug, thiserror::Error)]
pub enum SupervisorError {
    #[error("Authentication failed: {0}")]
    Authentication(String),

    #[error("Authorization failed: {0}")]
    Authorization(String),

    #[error("Service not found: {0}")]
    ServiceNotFound(String),

    #[error("Service operation failed: {0}")]
    ServiceOperation(String),

    #[error("Process management error: {0}")]
    Process(String),

    #[error("Health check failed: {0}")]
    HealthCheck(String),

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Circuit breaker open: {0}")]
    CircuitBreaker(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl SupervisorError {
    /// Check if this error should trigger a retry
    pub fn should_retry(&self) -> bool {
        matches!(
            self,
            SupervisorError::Timeout(_) |
            SupervisorError::CircuitBreaker(_) |
            SupervisorError::Http(_) |
            SupervisorError::Io(_) |
            SupervisorError::Process(_)
        )
    }

    /// Get the retry delay for this error type
    pub fn retry_delay(&self) -> std::time::Duration {
        match self {
            SupervisorError::Timeout(_) => std::time::Duration::from_millis(100),
            SupervisorError::CircuitBreaker(_) => std::time::Duration::from_millis(500),
            SupervisorError::Http(_) => std::time::Duration::from_millis(200),
            SupervisorError::Io(_) => std::time::Duration::from_millis(300),
            SupervisorError::Process(_) => std::time::Duration::from_millis(500),
            _ => std::time::Duration::from_millis(1000),
        }
    }
}

/// Convert from anyhow::Error for compatibility
impl From<anyhow::Error> for SupervisorError {
    fn from(err: anyhow::Error) -> Self {
        SupervisorError::Internal(err.to_string())
    }
}

// Note: sysinfo may not have Error type in current version
// impl From<sysinfo::Error> for SupervisorError {
//     fn from(err: sysinfo::Error) -> Self {
//         SupervisorError::Process(err.to_string())
//     }
// }

impl From<SupervisorError> for AosError {
    fn from(err: SupervisorError) -> Self {
        match err {
            SupervisorError::Authentication(msg) => AosError::Auth(format!("Supervisor authentication failed: {}", msg)),
            SupervisorError::Authorization(msg) => AosError::Authz(format!("Supervisor authorization failed: {}", msg)),
            SupervisorError::ServiceNotFound(msg) => AosError::NotFound(format!("Service not found: {}", msg)),
            SupervisorError::ServiceOperation(msg) => AosError::Internal(format!("Service operation failed: {}", msg)),
            SupervisorError::Process(msg) => AosError::System(format!("Process management error: {}", msg)),
            SupervisorError::HealthCheck(msg) => AosError::Internal(format!("Health check failed: {}", msg)),
            SupervisorError::Configuration(msg) => AosError::Config(format!("Supervisor configuration error: {}", msg)),
            SupervisorError::Io(e) => AosError::Io(format!("Supervisor IO error: {}", e)),
            SupervisorError::Json(e) => AosError::Serialization(e),
            SupervisorError::Jwt(e) => AosError::Auth(format!("JWT error: {}", e)),
            SupervisorError::Http(msg) => AosError::Http(format!("Supervisor HTTP error: {}", msg)),
            SupervisorError::CircuitBreaker(msg) => AosError::CircuitBreakerOpen { service: msg },
            SupervisorError::Timeout(msg) => AosError::Timeout { duration: std::time::Duration::from_secs(0) },
            SupervisorError::Internal(msg) => AosError::Internal(format!("Supervisor internal error: {}", msg)),
        }
    }
}
