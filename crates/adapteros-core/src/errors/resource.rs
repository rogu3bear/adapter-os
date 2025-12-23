//! Resource management errors
//!
//! Covers memory pressure, quota limits, and resource exhaustion.

use thiserror::Error;

/// Resource management errors
#[derive(Error, Debug)]
pub enum AosResourceError {
    /// Generic resource exhaustion
    #[error("Resource exhaustion: {0}")]
    Exhaustion(String),

    /// Memory pressure detected
    #[error("Memory pressure: {0}")]
    MemoryPressure(String),

    /// Memory allocation or management error
    #[error("Memory error: {0}")]
    Memory(String),

    /// Quota exceeded for a specific resource
    #[error("Quota exceeded for resource '{resource}'")]
    QuotaExceeded {
        resource: String,
        /// Failure code string (e.g., "KV_QUOTA_EXCEEDED")
        failure_code: Option<String>,
    },

    /// Resource temporarily unavailable
    #[error("Resource unavailable: {0}")]
    Unavailable(String),
}

impl AosResourceError {
    /// Check if this error indicates the system should back off
    pub fn should_backoff(&self) -> bool {
        matches!(
            self,
            Self::MemoryPressure(_) | Self::QuotaExceeded { .. } | Self::Exhaustion(_)
        )
    }
}
