//! Error classification for recovery decisions
//!
//! Provides traits and implementations for determining how to handle errors
//! in the recovery pipeline.

use crate::AosError;
use std::time::Duration;

/// Trait for classifying errors for recovery decisions
///
/// Implementors can provide nuanced classification to guide the recovery
/// orchestrator's behavior for different error types.
pub trait RecoveryClassifier {
    /// Check if this error should trigger a retry attempt
    ///
    /// Returns `true` for transient errors that may succeed on retry.
    fn is_retryable(&self) -> bool;

    /// Check if this error should be counted against the circuit breaker
    ///
    /// Returns `true` for errors that indicate service degradation.
    /// Client errors (validation, auth) typically return `false` since
    /// they indicate caller issues, not service issues.
    fn counts_as_failure(&self) -> bool;

    /// Recommended delay before retry (if retryable)
    ///
    /// Some errors may indicate a specific wait time is advisable.
    /// Returns `None` to use the default backoff calculation.
    fn recommended_delay(&self) -> Option<Duration>;

    /// Whether fallback should be attempted for this error
    ///
    /// Returns `true` if a fallback function should be invoked.
    fn should_fallback(&self) -> bool;
}

impl RecoveryClassifier for AosError {
    fn is_retryable(&self) -> bool {
        match self {
            // Network/connectivity errors are typically transient
            AosError::Network(_) => true,
            AosError::Timeout { .. } => true,

            // Circuit breaker half-open allows limited retries
            AosError::CircuitBreakerHalfOpen { .. } => true,

            // Service unavailability may be temporary
            AosError::Unavailable(_) => true,

            // Resource exhaustion may clear up
            AosError::ResourceExhaustion(_) => true,

            // IO errors that look like network/connection issues
            AosError::Io(err) => {
                let err_lower = err.to_lowercase();
                err_lower.contains("connection")
                    || err_lower.contains("timeout")
                    || err_lower.contains("network")
                    || err_lower.contains("refused")
                    || err_lower.contains("reset")
            }

            // Database lock contention is transient
            AosError::Database(err) => {
                let err_lower = err.to_lowercase();
                err_lower.contains("busy")
                    || err_lower.contains("locked")
                    || err_lower.contains("deadlock")
                    || err_lower.contains("timeout")
            }

            // Circuit breaker open requires waiting, not immediate retry
            AosError::CircuitBreakerOpen { .. } => false,

            // Most other errors are not retryable
            _ => false,
        }
    }

    fn counts_as_failure(&self) -> bool {
        // Most errors count as failures for circuit breaker purposes
        // Except client errors that indicate caller issues, not service degradation
        !matches!(
            self,
            AosError::Validation(_)
                | AosError::Auth(_)
                | AosError::Authz(_)
                | AosError::NotFound(_)
                | AosError::Parse(_)
                | AosError::Config(_)
        )
    }

    fn recommended_delay(&self) -> Option<Duration> {
        match self {
            // For timeouts, wait a fraction of the timeout duration
            AosError::Timeout { duration } => Some(*duration / 2),

            // Resource exhaustion may need more time to clear
            AosError::ResourceExhaustion(_) => Some(Duration::from_millis(500)),

            // Half-open circuit breaker: short delay before probe
            AosError::CircuitBreakerHalfOpen { .. } => Some(Duration::from_millis(100)),

            // Database contention: moderate delay
            AosError::Database(err) if err.to_lowercase().contains("busy") => {
                Some(Duration::from_millis(200))
            }

            // Use default backoff for other errors
            _ => None,
        }
    }

    fn should_fallback(&self) -> bool {
        // Fallback makes sense for errors where the primary path is unavailable
        // but the operation might still succeed via an alternative
        self.is_retryable()
            || matches!(
                self,
                AosError::CircuitBreakerOpen { .. } | AosError::ResourceExhaustion(_)
            )
    }
}

/// Extension trait for Result types with RecoveryClassifier errors
pub trait RecoveryClassifierExt<T, E: RecoveryClassifier> {
    /// Check if the error (if any) is retryable
    fn is_retryable_error(&self) -> bool;

    /// Check if the error (if any) counts as a failure
    fn counts_as_failure(&self) -> bool;
}

impl<T, E: RecoveryClassifier> RecoveryClassifierExt<T, E> for Result<T, E> {
    fn is_retryable_error(&self) -> bool {
        match self {
            Ok(_) => false,
            Err(e) => e.is_retryable(),
        }
    }

    fn counts_as_failure(&self) -> bool {
        match self {
            Ok(_) => false,
            Err(e) => e.counts_as_failure(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_errors_are_retryable() {
        let err = AosError::Network("connection refused".to_string());
        assert!(err.is_retryable());
        assert!(err.counts_as_failure());
        assert!(err.should_fallback());
    }

    #[test]
    fn test_timeout_errors_are_retryable() {
        let err = AosError::Timeout {
            duration: Duration::from_secs(5),
        };
        assert!(err.is_retryable());
        assert!(err.counts_as_failure());

        // Should recommend half the timeout duration
        let delay = err.recommended_delay().unwrap();
        assert_eq!(delay, Duration::from_millis(2500));
    }

    #[test]
    fn test_validation_errors_not_retryable() {
        let err = AosError::Validation("invalid input".to_string());
        assert!(!err.is_retryable());
        assert!(!err.counts_as_failure()); // Client error
        assert!(!err.should_fallback());
    }

    #[test]
    fn test_circuit_breaker_open_not_retryable() {
        let err = AosError::CircuitBreakerOpen {
            service: "db".to_string(),
        };
        assert!(!err.is_retryable()); // Must wait for timeout
        assert!(err.counts_as_failure());
        assert!(err.should_fallback()); // But fallback is appropriate
    }

    #[test]
    fn test_circuit_breaker_half_open_is_retryable() {
        let err = AosError::CircuitBreakerHalfOpen {
            service: "api".to_string(),
        };
        assert!(err.is_retryable()); // Can probe
        assert!(err.counts_as_failure());
    }

    #[test]
    fn test_io_connection_errors_are_retryable() {
        let err = AosError::Io("connection refused".to_string());
        assert!(err.is_retryable());

        let err = AosError::Io("connection reset by peer".to_string());
        assert!(err.is_retryable());

        let err = AosError::Io("file not found".to_string());
        assert!(!err.is_retryable()); // Not a connection issue
    }

    #[test]
    fn test_database_lock_errors_are_retryable() {
        let err = AosError::Database("database is locked".to_string());
        assert!(err.is_retryable());

        let err = AosError::Database("SQLITE_BUSY".to_string());
        assert!(err.is_retryable());

        let err = AosError::Database("constraint violation".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_resource_exhaustion_is_retryable() {
        let err = AosError::ResourceExhaustion("memory pressure".to_string());
        assert!(err.is_retryable());
        assert!(err.counts_as_failure());
        assert_eq!(err.recommended_delay(), Some(Duration::from_millis(500)));
    }

    #[test]
    fn test_result_extension() {
        let ok_result: Result<i32, AosError> = Ok(42);
        assert!(!ok_result.is_retryable_error());
        assert!(!ok_result.counts_as_failure());

        let err_result: Result<i32, AosError> =
            Err(AosError::Network("connection failed".to_string()));
        assert!(err_result.is_retryable_error());
        assert!(err_result.counts_as_failure());
    }
}
