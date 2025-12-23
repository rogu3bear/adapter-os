//! Internal and system errors
//!
//! Last-resort errors for truly unexpected conditions. Prefer specific error categories.

use thiserror::Error;

/// Internal system errors (use sparingly - prefer specific categories)
#[derive(Error, Debug)]
pub enum AosInternalError {
    /// Internal error (unexpected condition)
    #[error("Internal error: {0}")]
    Internal(String),

    /// System-level error
    #[error("System error: {0}")]
    System(String),

    /// Platform-specific error
    #[error("Platform error: {0}")]
    Platform(String),

    /// Toolchain error (build, compile)
    #[error("Toolchain error: {0}")]
    Toolchain(String),

    /// Deterministic executor error
    #[error("Deterministic executor error: {0}")]
    DeterministicExecutor(String),

    /// Anomaly detected (unexpected behavior)
    #[error("Anomaly detected: {0}")]
    Anomaly(String),
}
