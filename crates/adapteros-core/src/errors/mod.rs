//! Error types for AdapterOS
//!
//! This module provides a structured error hierarchy with categorical sub-enums
//! for type-safe error handling without string parsing.
//!
//! ## Error Hierarchy
//!
//! ```text
//! AosError (top-level wrapper)
//! ├── Network    (HTTP, timeouts, circuit breakers, connectivity)
//! ├── Storage    (database, I/O, cache)
//! ├── Policy     (violations, egress, isolation, determinism)
//! ├── Crypto     (hashing, encryption, sealing)
//! ├── Adapter    (loading, hash verification, lifecycle)
//! ├── Model      (backends, cache, inference)
//! ├── Validation (parsing, config, manifest)
//! ├── Resource   (memory, quotas, exhaustion)
//! ├── Auth       (authentication, authorization)
//! ├── Operations (domain-specific: telemetry, federation, workers)
//! └── Internal   (system, platform - last resort)
//! ```
//!
//! ## Error Message Standards
//!
//! 1. **Capitalization**: Start with a capital letter
//! 2. **Format**: Use "Action failed: reason" or "Entity state: details"
//! 3. **Dynamic values**: Use `format!()` for interpolation
//! 4. **No trailing periods**: Error strings should not end with periods
//! 5. **Be specific and actionable**: Include enough context to debug

pub mod adapter;
pub mod auth;
pub mod crypto;
pub mod internal;
pub mod model;
pub mod network;
pub mod operations;
pub mod policy;
pub mod resource;
pub mod storage;
pub mod validation;

// Re-export sub-error types
pub use adapter::AosAdapterError;
pub use auth::AosAuthError;
pub use crypto::AosCryptoError;
pub use internal::AosInternalError;
pub use model::{AosModelError, CacheBudgetExceededInfo};
pub use network::AosNetworkError;
pub use operations::AosOperationsError;
pub use policy::AosPolicyError;
pub use resource::AosResourceError;
pub use storage::AosStorageError;
pub use validation::AosValidationError;

use thiserror::Error;
use zip::result::ZipError;

/// Result type alias for AosError
pub type Result<T> = std::result::Result<T, AosError>;

/// Core error type for AdapterOS operations
///
/// This enum wraps categorical sub-enums for type-safe error handling.
/// Each category preserves full structure for programmatic matching.
#[derive(Error, Debug)]
pub enum AosError {
    /// Network and connectivity errors
    #[error(transparent)]
    Network(#[from] AosNetworkError),

    /// Storage and database errors
    #[error(transparent)]
    Storage(#[from] AosStorageError),

    /// Policy and security errors
    #[error(transparent)]
    Policy(#[from] AosPolicyError),

    /// Cryptographic errors
    #[error(transparent)]
    Crypto(#[from] AosCryptoError),

    /// Adapter errors
    #[error(transparent)]
    Adapter(#[from] AosAdapterError),

    /// Model and inference errors
    #[error(transparent)]
    Model(#[from] AosModelError),

    /// Validation and parsing errors
    #[error(transparent)]
    Validation(#[from] AosValidationError),

    /// Resource management errors
    #[error(transparent)]
    Resource(#[from] AosResourceError),

    /// Authentication and authorization errors
    #[error(transparent)]
    Auth(#[from] AosAuthError),

    /// Domain-specific operational errors
    #[error(transparent)]
    Operations(#[from] AosOperationsError),

    /// Internal system errors (last resort)
    #[error(transparent)]
    Internal(#[from] AosInternalError),

    /// Context wrapper for error chains
    #[error("{context}: {source}")]
    WithContext {
        context: String,
        #[source]
        source: Box<AosError>,
    },
}

// ============================================================================
// Convenience conversions from external types
// ============================================================================

impl From<std::io::Error> for AosError {
    fn from(err: std::io::Error) -> Self {
        AosError::Storage(AosStorageError::Io(err.to_string()))
    }
}

impl From<rusqlite::Error> for AosError {
    fn from(err: rusqlite::Error) -> Self {
        AosError::Storage(AosStorageError::Sqlite(err.to_string()))
    }
}

#[cfg(feature = "sqlx")]
impl From<sqlx::Error> for AosError {
    fn from(err: sqlx::Error) -> Self {
        AosError::Storage(AosStorageError::Sqlx(err.to_string()))
    }
}

impl From<serde_json::Error> for AosError {
    fn from(err: serde_json::Error) -> Self {
        AosError::Validation(AosValidationError::Serialization(err.to_string()))
    }
}

impl From<ZipError> for AosError {
    fn from(err: ZipError) -> Self {
        AosError::Storage(AosStorageError::Io(format!(
            "Zip operation failed: {}",
            err
        )))
    }
}

// ============================================================================
// Context extension trait
// ============================================================================

/// Extension trait to attach context to results without disrupting error types
pub trait ResultExt<T> {
    /// Add context to an error
    fn context(self, ctx: impl Into<String>) -> Result<T>;

    /// Add context to an error using a closure (lazy evaluation)
    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn context(self, ctx: impl Into<String>) -> Result<T> {
        self.map_err(|e| AosError::WithContext {
            context: ctx.into(),
            source: Box::new(e),
        })
    }

    fn with_context<F: FnOnce() -> String>(self, f: F) -> Result<T> {
        self.map_err(|e| AosError::WithContext {
            context: f(),
            source: Box::new(e),
        })
    }
}

// ============================================================================
// Convenience constructors (backward compatibility)
// ============================================================================

impl AosError {
    // ----- Network errors -----

    /// HTTP error
    pub fn http(msg: impl Into<String>) -> Self {
        AosError::Network(AosNetworkError::Http(msg.into()))
    }

    /// Network error
    pub fn network(msg: impl Into<String>) -> Self {
        AosError::Network(AosNetworkError::Network(msg.into()))
    }

    /// Timeout error
    pub fn timeout(duration: std::time::Duration) -> Self {
        AosError::Network(AosNetworkError::Timeout { duration })
    }

    /// Circuit breaker open
    pub fn circuit_breaker_open(service: impl Into<String>) -> Self {
        AosError::Network(AosNetworkError::CircuitBreakerOpen {
            service: service.into(),
        })
    }

    /// Circuit breaker half-open
    pub fn circuit_breaker_half_open(service: impl Into<String>) -> Self {
        AosError::Network(AosNetworkError::CircuitBreakerHalfOpen {
            service: service.into(),
        })
    }

    /// Service unavailable
    pub fn unavailable(msg: impl Into<String>) -> Self {
        AosError::Network(AosNetworkError::Unavailable(msg.into()))
    }

    // ----- Storage errors -----

    /// Database error
    pub fn database(msg: impl Into<String>) -> Self {
        AosError::Storage(AosStorageError::Database(msg.into()))
    }

    /// I/O error
    pub fn io(msg: impl Into<String>) -> Self {
        AosError::Storage(AosStorageError::Io(msg.into()))
    }

    /// Dual-write inconsistency error (SQL committed but KV failed and rollback unavailable)
    pub fn dual_write_inconsistency(
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        AosError::Storage(AosStorageError::DualWriteInconsistency {
            entity_type: entity_type.into(),
            entity_id: entity_id.into(),
            reason: reason.into(),
        })
    }

    /// Artifact storage error
    pub fn artifact(msg: impl Into<String>) -> Self {
        AosError::Storage(AosStorageError::Artifact(msg.into()))
    }

    /// Registry error
    pub fn registry(msg: impl Into<String>) -> Self {
        AosError::Storage(AosStorageError::Registry(msg.into()))
    }

    // ----- Validation errors -----

    /// Configuration error
    pub fn config(msg: impl Into<String>) -> Self {
        AosError::Validation(AosValidationError::Config(msg.into()))
    }

    /// Validation error
    pub fn validation(msg: impl Into<String>) -> Self {
        AosError::Validation(AosValidationError::Validation(msg.into()))
    }

    /// Parse error
    pub fn parse(msg: impl Into<String>) -> Self {
        AosError::Validation(AosValidationError::Parse(msg.into()))
    }

    // ----- Resource errors -----

    /// Resource exhaustion
    pub fn resource_exhaustion(msg: impl Into<String>) -> Self {
        AosError::Resource(AosResourceError::Exhaustion(msg.into()))
    }

    /// Memory pressure
    pub fn memory_pressure(msg: impl Into<String>) -> Self {
        AosError::Resource(AosResourceError::MemoryPressure(msg.into()))
    }

    // ----- Internal errors -----

    /// Internal error
    pub fn internal(msg: impl Into<String>) -> Self {
        AosError::Internal(AosInternalError::Internal(msg.into()))
    }

    /// System error
    pub fn system(msg: impl Into<String>) -> Self {
        AosError::Internal(AosInternalError::System(msg.into()))
    }

    // ----- Auth errors -----

    /// Authentication error
    pub fn auth(msg: impl Into<String>) -> Self {
        AosError::Auth(AosAuthError::Authentication(msg.into()))
    }

    /// Authorization error
    pub fn authz(msg: impl Into<String>) -> Self {
        AosError::Auth(AosAuthError::Authorization(msg.into()))
    }

    // ----- Crypto errors -----

    /// Cryptographic error
    pub fn crypto(msg: impl Into<String>) -> Self {
        AosError::Crypto(AosCryptoError::Crypto(msg.into()))
    }

    /// Invalid hash format
    pub fn invalid_hash(msg: impl Into<String>) -> Self {
        AosError::Crypto(AosCryptoError::InvalidHash(msg.into()))
    }

    /// Invalid CPID format
    pub fn invalid_cpid(msg: impl Into<String>) -> Self {
        AosError::Validation(AosValidationError::InvalidCPID(msg.into()))
    }

    // ----- Policy errors -----

    /// Policy violation
    pub fn policy_violation(msg: impl Into<String>) -> Self {
        AosError::Policy(AosPolicyError::Violation(msg.into()))
    }

    /// Determinism violation
    pub fn determinism_violation(msg: impl Into<String>) -> Self {
        AosError::Policy(AosPolicyError::DeterminismViolation(msg.into()))
    }

    // ----- Adapter errors -----

    /// Adapter lifecycle error
    pub fn lifecycle(msg: impl Into<String>) -> Self {
        AosError::Adapter(AosAdapterError::Lifecycle(msg.into()))
    }

    // ----- Operations errors -----

    /// Not found error
    pub fn not_found(msg: impl Into<String>) -> Self {
        AosError::Operations(AosOperationsError::NotFound(msg.into()))
    }

    /// Worker error
    pub fn worker(msg: impl Into<String>) -> Self {
        AosError::Operations(AosOperationsError::Worker(msg.into()))
    }
}

// ============================================================================
// Helper methods on AosError
// ============================================================================

impl AosError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        match self {
            AosError::Network(e) => e.is_retryable(),
            AosError::Resource(e) => e.should_backoff(),
            AosError::WithContext { source, .. } => source.is_retryable(),
            _ => false,
        }
    }

    /// Check if this is a security-critical error
    pub fn is_security_critical(&self) -> bool {
        match self {
            AosError::Policy(e) => e.is_security_critical(),
            AosError::Auth(_) => true,
            AosError::WithContext { source, .. } => source.is_security_critical(),
            _ => false,
        }
    }

    /// Get the innermost error (unwrap all WithContext layers)
    pub fn root_cause(&self) -> &AosError {
        match self {
            AosError::WithContext { source, .. } => source.root_cause(),
            other => other,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_chaining() {
        let base: Result<()> = Err(AosError::internal("boom"));

        let err = base
            .context("while doing A")
            .with_context(|| "processing request".to_string())
            .unwrap_err();

        // Validate the chain structure
        match &err {
            AosError::WithContext { context, source } => {
                assert_eq!(context, "processing request");
                match source.as_ref() {
                    AosError::WithContext { context, source } => {
                        assert_eq!(context, "while doing A");
                        assert!(matches!(source.as_ref(), AosError::Internal(_)));
                    }
                    _ => panic!("expected inner WithContext"),
                }
            }
            _ => panic!("expected outer WithContext"),
        }

        // Validate root_cause
        assert!(matches!(err.root_cause(), AosError::Internal(_)));
    }

    #[test]
    fn test_is_retryable() {
        let network_err = AosError::Network(AosNetworkError::Timeout {
            duration: std::time::Duration::from_secs(30),
        });
        assert!(network_err.is_retryable());

        let policy_err = AosError::Policy(AosPolicyError::Violation("test".to_string()));
        assert!(!policy_err.is_retryable());
    }

    #[test]
    fn test_from_conversions() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let aos_err: AosError = io_err.into();
        assert!(matches!(aos_err, AosError::Storage(AosStorageError::Io(_))));
    }
}
