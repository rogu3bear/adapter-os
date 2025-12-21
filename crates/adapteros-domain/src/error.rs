//! Error types for the Domain Adapter Layer

use adapteros_core::AosError;
use thiserror::Error;

/// Error types for domain adapter operations
#[derive(Error, Debug)]
pub enum DomainAdapterError {
    #[error("Failed to load manifest from {path}: {source}")]
    ManifestLoadError {
        path: String,
        source: std::io::Error,
    },

    #[error("Invalid manifest: {reason}")]
    InvalidManifest { reason: String },

    #[error("Tensor shape mismatch: expected {expected:?}, got {actual:?}")]
    TensorShapeMismatch {
        expected: Vec<usize>,
        actual: Vec<usize>,
    },

    #[error("Unsupported input format: {format}")]
    UnsupportedInputFormat { format: String },

    #[error("Unsupported output format: {format}")]
    UnsupportedOutputFormat { format: String },

    #[error("Adapter not initialized: {adapter_name}")]
    AdapterNotInitialized { adapter_name: String },

    #[error("Determinism violation detected: {details}")]
    DeterminismViolation { details: String },

    #[error("Numerical error exceeds threshold: {error} > {threshold}")]
    NumericalErrorThreshold { error: f64, threshold: f64 },

    #[error("Model file not found: {path}")]
    ModelFileNotFound { path: String },

    #[error("Hash verification failed: expected {expected}, got {actual}")]
    HashVerificationFailed { expected: String, actual: String },

    #[error("Tokenization error: {details}")]
    TokenizationError { details: String },

    #[error("Image processing error: {details}")]
    ImageProcessingError { details: String },

    #[error("Telemetry error: {details}")]
    TelemetryError { details: String },

    #[error("Executor error: {0}")]
    ExecutorError(#[from] adapteros_deterministic_exec::DeterministicExecutorError),

    #[error("Numerics error: {0}")]
    NumericsError(#[from] adapteros_numerics::noise::NumericsError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("TOML error: {0}")]
    TomlError(#[from] toml::de::Error),
}

/// Result type for domain adapter operations
pub type Result<T> = std::result::Result<T, DomainAdapterError>;
