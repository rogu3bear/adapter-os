//! Error types for base LLM operations
//!
//! Defines error types following the patterns established in adapteros-core.

use thiserror::Error;

/// Base LLM specific errors
#[derive(Error, Debug)]
pub enum BaseLLMError {
    #[error("Model not initialized: {0}")]
    NotInitialized(String),

    #[error("Model loading failed: {0}")]
    LoadingFailed(String),

    #[error("Forward pass failed: {0}")]
    ForwardFailed(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("State management error: {0}")]
    StateError(String),

    #[error("Checkpoint error: {0}")]
    CheckpointError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Model verification failed: {0}")]
    VerificationFailed(String),
}

/// Result type for base LLM operations
pub type Result<T> = std::result::Result<T, BaseLLMError>;

impl From<serde_json::Error> for BaseLLMError {
    fn from(err: serde_json::Error) -> Self {
        BaseLLMError::SerializationError(err.to_string())
    }
}

impl From<std::io::Error> for BaseLLMError {
    fn from(err: std::io::Error) -> Self {
        BaseLLMError::LoadingFailed(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_types() {
        let err = BaseLLMError::NotInitialized("test model".to_string());
        assert!(err.to_string().contains("test model"));

        let err = BaseLLMError::LoadingFailed("disk error".to_string());
        assert!(err.to_string().contains("disk error"));
    }

    #[test]
    fn test_error_conversions() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let base_err: BaseLLMError = json_err.into();
        assert!(matches!(base_err, BaseLLMError::SerializationError(_)));

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let base_err: BaseLLMError = io_err.into();
        assert!(matches!(base_err, BaseLLMError::LoadingFailed(_)));
    }
}
