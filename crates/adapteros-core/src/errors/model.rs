//! Model-related errors
//!
//! Covers model loading, backends (CoreML, MLX, Metal), cache management, and inference.

use thiserror::Error;

/// Model and inference backend errors
#[derive(Error, Debug)]
pub enum AosModelError {
    /// Model not found
    #[error("Model not found: {model_id}")]
    NotFound { model_id: String },

    /// Model acquisition (download) in progress
    #[error("Model acquisition in progress: {model_id} is {state}")]
    AcquisitionInProgress { model_id: String, state: String },

    /// Base LLM error
    #[error("Base LLM error: {0}")]
    BaseLLM(String),

    /// Model cache budget exceeded during eviction
    #[error("Model cache budget exceeded: needed {needed_mb} MB, freed {freed_mb} MB (pinned={pinned_count}, active={active_count}), max {max_mb} MB")]
    CacheBudgetExceeded {
        needed_mb: u64,
        freed_mb: u64,
        pinned_count: usize,
        active_count: usize,
        max_mb: u64,
        model_key: Option<String>,
    },

    /// CoreML backend error
    #[error("CoreML error: {0}")]
    CoreML(String),

    /// MLX backend error
    #[error("MLX error: {0}")]
    Mlx(String),

    /// Metal GPU error
    #[error("Metal error: {0}")]
    Metal(String),

    /// Kernel operation error
    #[error("Kernel error: {0}")]
    Kernel(String),

    /// Kernel tensor layout mismatch
    #[error("Kernel layout mismatch for tensor '{tensor}': expected {expected}, got {got}")]
    KernelLayoutMismatch {
        tensor: String,
        expected: String,
        got: String,
    },

    /// Quantization error
    #[error("Quantization error: {0}")]
    Quantization(String),

    /// Training error
    #[error("Training error: {0}")]
    Training(String),

    /// Autograd error
    #[error("Autograd error: {0}")]
    Autograd(String),
}

/// Serializable representation of cache budget exceeded error
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CacheBudgetExceededInfo {
    pub needed_mb: u64,
    pub freed_mb: u64,
    pub pinned_count: usize,
    pub active_count: usize,
    pub max_mb: u64,
    pub model_key: Option<String>,
}

impl CacheBudgetExceededInfo {
    /// Extract info from an `AosModelError::CacheBudgetExceeded` variant
    pub fn from_error(e: &AosModelError) -> Option<Self> {
        match e {
            AosModelError::CacheBudgetExceeded {
                needed_mb,
                freed_mb,
                pinned_count,
                active_count,
                max_mb,
                model_key,
            } => Some(Self {
                needed_mb: *needed_mb,
                freed_mb: *freed_mb,
                pinned_count: *pinned_count,
                active_count: *active_count,
                max_mb: *max_mb,
                model_key: model_key.clone(),
            }),
            _ => None,
        }
    }
}
