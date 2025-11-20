//! Comprehensive error handling for MLX FFI backend
//!
//! This module provides detailed error types, recovery mechanisms, and troubleshooting guidance.

use adapteros_core::AosError;
use std::fmt;
use thiserror::Error;

/// Detailed MLX FFI error types with context and recovery guidance
#[derive(Debug, Clone, Error)]
pub enum MlxError {
    /// Model loading errors
    #[error("Failed to load model from {path}: {message}")]
    ModelLoadError {
        path: String,
        message: String,
        recoverable: bool,
    },

    /// Model weight validation errors
    #[error("Invalid model weights: {reason}")]
    WeightValidationError {
        reason: String,
        expected: Option<String>,
        actual: Option<String>,
    },

    /// GPU out of memory errors
    #[error("GPU out of memory: {requested_mb:.2} MB requested, {available_mb:.2} MB available")]
    GpuOomError {
        requested_mb: f32,
        available_mb: f32,
        hint: String,
    },

    /// Tensor operation errors
    #[error("Tensor operation failed: {operation} - {reason}")]
    TensorOpError {
        operation: String,
        reason: String,
        tensor_shapes: Vec<Vec<usize>>,
    },

    /// Tensor shape mismatch
    #[error("Shape mismatch: expected {expected:?}, got {actual:?}")]
    ShapeMismatch {
        expected: Vec<usize>,
        actual: Vec<usize>,
        context: String,
    },

    /// Dtype mismatch
    #[error("Data type mismatch: expected {expected}, got {actual}")]
    DtypeMismatch {
        expected: String,
        actual: String,
        context: String,
    },

    /// Adapter loading errors
    #[error("Failed to load adapter '{adapter_id}': {reason}")]
    AdapterLoadError {
        adapter_id: String,
        reason: String,
        recoverable: bool,
    },

    /// Adapter not found
    #[error("Adapter {adapter_id} not found in registry")]
    AdapterNotFound { adapter_id: u16 },

    /// Module not found in adapter
    #[error("Module '{module_name}' not found in adapter '{adapter_id}'")]
    ModuleNotFound {
        adapter_id: String,
        module_name: String,
        available_modules: Vec<String>,
    },

    /// FFI boundary errors
    #[error("FFI call failed: {function} - {message}")]
    FfiError {
        function: String,
        message: String,
        c_error: Option<String>,
    },

    /// C++ exception caught
    #[error("C++ exception in {function}: {message}")]
    CppException {
        function: String,
        message: String,
        stack_trace: Option<String>,
    },

    /// Null pointer error
    #[error("Null pointer returned from {function}")]
    NullPointer {
        function: String,
        context: String,
    },

    /// Memory allocation failure
    #[error("Memory allocation failed: {size_mb:.2} MB requested")]
    AllocationFailed {
        size_mb: f32,
        total_allocated_mb: f32,
        hint: String,
    },

    /// Invalid configuration
    #[error("Invalid configuration: {field} - {reason}")]
    ConfigError { field: String, reason: String },

    /// Validation error
    #[error("Validation failed: {check} - {reason}")]
    ValidationError { check: String, reason: String },

    /// Resource cleanup error
    #[error("Failed to clean up resource: {resource} - {reason}")]
    CleanupError { resource: String, reason: String },

    /// Timeout error
    #[error("Operation timed out after {timeout_ms}ms: {operation}")]
    Timeout {
        operation: String,
        timeout_ms: u64,
    },

    /// Circuit breaker open
    #[error("Circuit breaker open for {operation}: {failures} consecutive failures")]
    CircuitBreakerOpen {
        operation: String,
        failures: usize,
        retry_after_ms: u64,
    },

    /// Retry exhausted
    #[error("Retry exhausted for {operation} after {attempts} attempts")]
    RetryExhausted {
        operation: String,
        attempts: usize,
        last_error: Box<MlxError>,
    },

    /// Internal error
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl MlxError {
    /// Check if error is recoverable with retry
    pub fn is_recoverable(&self) -> bool {
        match self {
            MlxError::GpuOomError { .. } => true,
            MlxError::Timeout { .. } => true,
            MlxError::AllocationFailed { .. } => true,
            MlxError::FfiError { .. } => true,
            MlxError::ModelLoadError { recoverable, .. } => *recoverable,
            MlxError::AdapterLoadError { recoverable, .. } => *recoverable,
            MlxError::CircuitBreakerOpen { .. } => false,
            MlxError::RetryExhausted { .. } => false,
            MlxError::ShapeMismatch { .. } => false,
            MlxError::DtypeMismatch { .. } => false,
            MlxError::ValidationError { .. } => false,
            _ => false,
        }
    }

    /// Get suggested recovery action
    pub fn recovery_hint(&self) -> &str {
        match self {
            MlxError::GpuOomError { hint, .. } => hint,
            MlxError::AllocationFailed { hint, .. } => hint,
            MlxError::CircuitBreakerOpen { .. } => {
                "Wait for circuit breaker to reset before retrying"
            }
            MlxError::Timeout { .. } => "Retry the operation or increase timeout threshold",
            MlxError::AdapterLoadError { .. } => "Check adapter file integrity and format",
            MlxError::ModelLoadError { .. } => {
                "Verify model path and ensure all required files are present"
            }
            MlxError::ShapeMismatch { .. } => "Check tensor dimensions match expected shapes",
            MlxError::ValidationError { .. } => "Review input parameters and configuration",
            _ => "See error details for troubleshooting steps",
        }
    }

    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            MlxError::Internal { .. } => ErrorSeverity::Critical,
            MlxError::GpuOomError { .. } => ErrorSeverity::High,
            MlxError::ModelLoadError { .. } => ErrorSeverity::High,
            MlxError::CircuitBreakerOpen { .. } => ErrorSeverity::High,
            MlxError::AllocationFailed { .. } => ErrorSeverity::High,
            MlxError::RetryExhausted { .. } => ErrorSeverity::Medium,
            MlxError::Timeout { .. } => ErrorSeverity::Medium,
            MlxError::AdapterLoadError { .. } => ErrorSeverity::Medium,
            MlxError::ValidationError { .. } => ErrorSeverity::Low,
            MlxError::ShapeMismatch { .. } => ErrorSeverity::Low,
            _ => ErrorSeverity::Medium,
        }
    }

    /// Convert to AosError for compatibility
    pub fn into_aos_error(self) -> AosError {
        match self {
            MlxError::ModelLoadError { path, message, .. } => {
                AosError::Mlx(format!("Failed to load model from {}: {}", path, message))
            }
            MlxError::GpuOomError {
                requested_mb,
                available_mb,
                ..
            } => AosError::Mlx(format!(
                "GPU OOM: {:.2}MB requested, {:.2}MB available",
                requested_mb, available_mb
            )),
            MlxError::AdapterNotFound { adapter_id } => {
                AosError::Lifecycle(format!("Adapter {} not found", adapter_id))
            }
            MlxError::ValidationError { check, reason } => {
                AosError::Validation(format!("{}: {}", check, reason))
            }
            other => AosError::Mlx(other.to_string()),
        }
    }
}

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorSeverity::Low => write!(f, "LOW"),
            ErrorSeverity::Medium => write!(f, "MEDIUM"),
            ErrorSeverity::High => write!(f, "HIGH"),
            ErrorSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Error context builder for adding detailed information
pub struct ErrorContext {
    operation: String,
    details: Vec<(String, String)>,
}

impl ErrorContext {
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
            details: Vec::new(),
        }
    }

    pub fn add(mut self, key: impl Into<String>, value: impl fmt::Display) -> Self {
        self.details.push((key.into(), value.to_string()));
        self
    }

    pub fn build(self, error: MlxError) -> MlxError {
        tracing::error!(
            operation = %self.operation,
            error = %error,
            severity = %error.severity(),
            recoverable = error.is_recoverable(),
            hint = error.recovery_hint(),
            details = ?self.details,
            "MLX error occurred"
        );
        error
    }
}

/// Helper macro for creating errors with context
#[macro_export]
macro_rules! mlx_error {
    ($error:expr, $op:expr, $($key:expr => $value:expr),* $(,)?) => {{
        let mut ctx = $crate::error::ErrorContext::new($op);
        $(
            ctx = ctx.add($key, $value);
        )*
        ctx.build($error)
    }};
}

/// Implement From<AosError> for MlxError to allow error conversion
impl From<AosError> for MlxError {
    fn from(err: AosError) -> Self {
        MlxError::Internal {
            message: format!("AosError: {}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_recoverability() {
        let oom_error = MlxError::GpuOomError {
            requested_mb: 2048.0,
            available_mb: 1024.0,
            hint: "Reduce batch size".to_string(),
        };
        assert!(oom_error.is_recoverable());

        let shape_error = MlxError::ShapeMismatch {
            expected: vec![2, 2],
            actual: vec![3, 3],
            context: "matmul".to_string(),
        };
        assert!(!shape_error.is_recoverable());
    }

    #[test]
    fn test_error_severity() {
        let internal = MlxError::Internal {
            message: "test".to_string(),
        };
        assert_eq!(internal.severity(), ErrorSeverity::Critical);

        let validation = MlxError::ValidationError {
            check: "test".to_string(),
            reason: "invalid".to_string(),
        };
        assert_eq!(validation.severity(), ErrorSeverity::Low);
    }

    #[test]
    fn test_error_conversion() {
        let mlx_error = MlxError::AdapterNotFound { adapter_id: 42 };
        let aos_error = mlx_error.into_aos_error();
        let msg = format!("{:?}", aos_error);
        assert!(msg.contains("42"));
    }

    #[test]
    fn test_error_context() {
        let error = MlxError::TensorOpError {
            operation: "matmul".to_string(),
            reason: "invalid shape".to_string(),
            tensor_shapes: vec![vec![2, 2], vec![3, 3]],
        };

        let ctx = ErrorContext::new("test_operation")
            .add("tensor1", "2x2")
            .add("tensor2", "3x3")
            .build(error);

        // Context building should succeed
        assert!(matches!(ctx, MlxError::TensorOpError { .. }));
    }
}
