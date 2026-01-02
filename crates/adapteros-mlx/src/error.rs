//! MLX error types

use thiserror::Error;

/// Errors from MLX operations
#[derive(Error, Debug)]
pub enum MlxError {
    /// Array operation failed
    #[error("Array operation failed: {0}")]
    ArrayOp(String),

    /// Shape mismatch
    #[error("Shape mismatch: expected {expected:?}, got {actual:?}")]
    ShapeMismatch {
        expected: Vec<i32>,
        actual: Vec<i32>,
    },

    /// Invalid dtype
    #[error("Invalid dtype: {0}")]
    InvalidDtype(String),

    /// Device error
    #[error("Device error: {0}")]
    Device(String),

    /// Not available (stub mode)
    #[error("MLX not available: {0}")]
    NotAvailable(String),

    /// Upstream mlx-rs error
    #[error("MLX error: {0}")]
    Upstream(String),

    /// Initialization failed (e.g., Metal device not available)
    #[error("MLX initialization failed: {0}")]
    InitializationFailed(String),

    /// CoreML/ANE operation error
    #[error("CoreML error: {0}")]
    CoreMLError(String),
}

impl From<adapteros_core::AosError> for MlxError {
    fn from(e: adapteros_core::AosError) -> Self {
        MlxError::Upstream(e.to_string())
    }
}

impl From<MlxError> for adapteros_core::AosError {
    fn from(e: MlxError) -> Self {
        adapteros_core::AosError::Mlx(e.to_string())
    }
}
