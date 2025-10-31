//! Secure Enclave abstraction with platform-specific implementations.

use thiserror::Error;

/// Result type for enclave operations
pub type Result<T> = std::result::Result<T, EnclaveError>;

/// Errors produced by Secure Enclave helpers
#[derive(Debug, Error)]
pub enum EnclaveError {
    #[error("Security framework error: {0}")]
    Security(String),
    #[error("Key not found: {0}")]
    KeyNotFound(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
mod stub;
#[cfg(not(target_os = "macos"))]
pub use stub::*;
