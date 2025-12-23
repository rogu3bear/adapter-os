//! Validation and parsing errors
//!
//! Covers input validation, manifest parsing, configuration, and serialization.

use thiserror::Error;

/// Validation and parsing errors
#[derive(Error, Debug)]
pub enum AosValidationError {
    /// Generic validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Invalid manifest format or content
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    /// Parse error
    #[error("Parse error: {0}")]
    Parse(String),

    /// JSON serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Invalid CPID format
    #[error("Invalid CPID: {0}")]
    InvalidCPID(String),

    /// Chat template error
    #[error("Chat template error: {0}")]
    ChatTemplate(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Invalid input data
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

impl From<serde_json::Error> for AosValidationError {
    fn from(err: serde_json::Error) -> Self {
        AosValidationError::Serialization(err.to_string())
    }
}
