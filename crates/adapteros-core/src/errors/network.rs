//! Network-related errors
//!
//! Covers HTTP, TCP, UDS, timeouts, circuit breakers, and connectivity issues.

use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

/// Network and connectivity errors
#[derive(Error, Debug)]
pub enum AosNetworkError {
    /// Generic HTTP error
    #[error("HTTP error: {0}")]
    Http(String),

    /// Generic network error
    #[error("Network error: {0}")]
    Network(String),

    /// Request timeout
    #[error("Timeout waiting for response after {duration:?}")]
    Timeout { duration: Duration },

    /// Circuit breaker is open (rejecting requests)
    #[error("Circuit breaker is open for service '{service}'")]
    CircuitBreakerOpen { service: String },

    /// Circuit breaker is half-open (testing recovery)
    #[error("Circuit breaker is half-open for service '{service}'")]
    CircuitBreakerHalfOpen { service: String },

    /// Unix domain socket connection failed
    #[error("UDS connection failed: {path}")]
    UdsConnectionFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Worker process not responding
    #[error("Worker not responding at {path}")]
    WorkerNotResponding { path: PathBuf },

    /// Invalid response from remote service
    #[error("Invalid response from worker: {reason}")]
    InvalidResponse { reason: String },

    /// Download operation failed
    #[error("Download failed for {repo_id}: {reason}")]
    DownloadFailed {
        repo_id: String,
        reason: String,
        is_resumable: bool,
    },

    /// Health check failed
    #[error("Health check failed for model {model_id}: {reason} (attempt {retry_count})")]
    HealthCheckFailed {
        model_id: String,
        reason: String,
        retry_count: u32,
    },

    /// Service unavailable (503-like errors)
    #[error("Service unavailable: {0}")]
    Unavailable(String),
}

impl AosNetworkError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Timeout { .. } => true,
            Self::CircuitBreakerHalfOpen { .. } => true,
            Self::DownloadFailed { is_resumable, .. } => *is_resumable,
            Self::HealthCheckFailed { .. } => true,
            Self::UdsConnectionFailed { .. } => true,
            Self::WorkerNotResponding { .. } => true,
            Self::CircuitBreakerOpen { .. } => false, // Must wait for timeout
            Self::Unavailable(_) => true,             // Service might become available
            Self::Http(_) | Self::Network(_) | Self::InvalidResponse { .. } => false,
        }
    }
}
