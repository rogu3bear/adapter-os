//! Recovery outcome types
//!
//! Defines the result and statistics types returned by the recovery orchestrator.

use crate::circuit_breaker::CircuitState;
use crate::AosError;
use std::time::Duration;
use thiserror::Error;

/// Detailed outcome of a recovery operation
#[derive(Debug)]
pub struct RecoveryOutcome<T, E = AosError> {
    /// The operation result
    pub result: Result<T, RecoveryError<E>>,
    /// Execution statistics
    pub stats: RecoveryStats,
}

impl<T, E> RecoveryOutcome<T, E> {
    /// Create a successful outcome
    pub fn success(value: T, stats: RecoveryStats) -> Self {
        Self {
            result: Ok(value),
            stats,
        }
    }

    /// Create a failed outcome
    pub fn failure(error: RecoveryError<E>, stats: RecoveryStats) -> Self {
        Self {
            result: Err(error),
            stats,
        }
    }

    /// Check if the operation was successful
    pub fn is_ok(&self) -> bool {
        self.result.is_ok()
    }

    /// Check if the operation failed
    pub fn is_err(&self) -> bool {
        self.result.is_err()
    }

    /// Map the success value
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> RecoveryOutcome<U, E> {
        RecoveryOutcome {
            result: self.result.map(f),
            stats: self.stats,
        }
    }
}

impl<T, E> RecoveryOutcome<T, E> {
    /// Unwrap the result, panicking if it was an error
    pub fn unwrap(self) -> T
    where
        E: std::fmt::Debug,
    {
        self.result.unwrap()
    }
}

/// Execution statistics from a recovery operation
#[derive(Debug, Clone, Default)]
pub struct RecoveryStats {
    /// Total execution time including retries
    pub total_duration: Duration,
    /// Number of retry attempts made (1 = first attempt only, no retries)
    pub retry_attempts: u32,
    /// Whether circuit breaker was checked
    pub circuit_breaker_checked: bool,
    /// Circuit breaker state at end of execution
    pub circuit_state: Option<CircuitState>,
    /// Whether SingleFlight deduplication occurred (waited for another request)
    pub was_deduplicated: bool,
    /// Whether fallback was invoked
    pub fallback_invoked: bool,
    /// Budget tokens consumed by this operation
    pub budget_tokens_consumed: u32,
}

impl RecoveryStats {
    /// Create new stats with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create stats for a first-attempt success
    pub fn first_attempt_success(duration: Duration) -> Self {
        Self {
            total_duration: duration,
            retry_attempts: 1,
            ..Default::default()
        }
    }
}

/// Recovery-specific error type that wraps underlying errors
#[derive(Debug, Error)]
pub enum RecoveryError<E = AosError> {
    /// Operation failed after all recovery attempts
    #[error("Operation failed after {attempts} attempt(s): {source}")]
    Exhausted {
        /// Number of attempts made
        attempts: u32,
        /// The last error encountered
        source: E,
    },

    /// Circuit breaker is open, operation not attempted
    #[error("Circuit breaker open for service '{service}'")]
    CircuitOpen {
        /// Name of the service with open circuit
        service: String,
    },

    /// Circuit breaker is half-open and at concurrent request capacity
    #[error("Circuit breaker half-open capacity reached for service '{service}'")]
    CircuitHalfOpenCapacity {
        /// Name of the service at capacity
        service: String,
    },

    /// Retry budget exhausted (too many concurrent retries or rate limit)
    #[error("Retry budget exhausted: {reason}")]
    BudgetExhausted {
        /// Reason for budget exhaustion
        reason: String,
    },

    /// Fallback was invoked but also failed
    #[error("Fallback failed: {source}")]
    FallbackFailed {
        /// The error from the fallback operation
        source: E,
    },

    /// Operation returned a non-retryable error
    #[error("Non-retryable error: {source}")]
    NonRetryable {
        /// The non-retryable error
        source: E,
    },

    /// Operation was cancelled or timed out at the orchestrator level
    #[error("Operation cancelled: {reason}")]
    Cancelled {
        /// Reason for cancellation
        reason: String,
    },
}

impl<E> RecoveryError<E> {
    /// Check if this error indicates the operation was never attempted
    pub fn operation_not_attempted(&self) -> bool {
        matches!(
            self,
            RecoveryError::CircuitOpen { .. }
                | RecoveryError::CircuitHalfOpenCapacity { .. }
                | RecoveryError::BudgetExhausted { .. }
                | RecoveryError::Cancelled { .. }
        )
    }

    /// Check if a fallback should be considered for this error
    pub fn should_try_fallback(&self) -> bool {
        matches!(
            self,
            RecoveryError::Exhausted { .. }
                | RecoveryError::CircuitOpen { .. }
                | RecoveryError::BudgetExhausted { .. }
        )
    }

    /// Get the underlying AosError if available
    pub fn source_error(&self) -> Option<&E> {
        match self {
            RecoveryError::Exhausted { source, .. } => Some(source),
            RecoveryError::FallbackFailed { source } => Some(source),
            RecoveryError::NonRetryable { source } => Some(source),
            _ => None,
        }
    }

    /// Consume the error and return the underlying source error
    pub fn into_source(self) -> Option<E> {
        match self {
            RecoveryError::Exhausted { source, .. } => Some(source),
            RecoveryError::FallbackFailed { source } => Some(source),
            RecoveryError::NonRetryable { source } => Some(source),
            _ => None,
        }
    }
}

impl RecoveryError<AosError> {
    /// Convert to AosError for integration with existing error handling
    pub fn into_aos_error(self) -> AosError {
        match self {
            RecoveryError::Exhausted { source, .. } => source,
            RecoveryError::CircuitOpen { service } => AosError::CircuitBreakerOpen { service },
            RecoveryError::CircuitHalfOpenCapacity { service } => {
                AosError::CircuitBreakerHalfOpen { service }
            }
            RecoveryError::BudgetExhausted { reason } => AosError::ResourceExhaustion(reason),
            RecoveryError::FallbackFailed { source } => source,
            RecoveryError::NonRetryable { source } => source,
            RecoveryError::Cancelled { reason } => {
                AosError::Internal(format!("Operation cancelled: {}", reason))
            }
        }
    }
}

impl From<RecoveryError<AosError>> for AosError {
    fn from(err: RecoveryError<AosError>) -> Self {
        err.into_aos_error()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_outcome_success() {
        let stats = RecoveryStats::first_attempt_success(Duration::from_millis(100));
        let outcome: RecoveryOutcome<i32, AosError> = RecoveryOutcome::success(42, stats);

        assert!(outcome.is_ok());
        assert!(!outcome.is_err());
        assert_eq!(outcome.stats.retry_attempts, 1);
        assert_eq!(outcome.unwrap(), 42);
    }

    #[test]
    fn test_recovery_outcome_failure() {
        let stats = RecoveryStats {
            retry_attempts: 3,
            ..Default::default()
        };
        let error = RecoveryError::<AosError>::Exhausted {
            attempts: 3,
            source: AosError::Network("connection failed".to_string()),
        };
        let outcome: RecoveryOutcome<i32, AosError> = RecoveryOutcome::failure(error, stats);

        assert!(!outcome.is_ok());
        assert!(outcome.is_err());
        assert_eq!(outcome.stats.retry_attempts, 3);
    }

    #[test]
    fn test_recovery_error_operation_not_attempted() {
        assert!(RecoveryError::<AosError>::CircuitOpen {
            service: "test".to_string()
        }
        .operation_not_attempted());

        assert!(RecoveryError::<AosError>::BudgetExhausted {
            reason: "rate limit".to_string()
        }
        .operation_not_attempted());

        assert!(!RecoveryError::<AosError>::Exhausted {
            attempts: 3,
            source: AosError::Network("fail".to_string())
        }
        .operation_not_attempted());
    }

    #[test]
    fn test_recovery_error_should_try_fallback() {
        assert!(RecoveryError::<AosError>::Exhausted {
            attempts: 3,
            source: AosError::Network("fail".to_string())
        }
        .should_try_fallback());

        assert!(RecoveryError::<AosError>::CircuitOpen {
            service: "test".to_string()
        }
        .should_try_fallback());

        assert!(!RecoveryError::<AosError>::NonRetryable {
            source: AosError::Validation("bad input".to_string())
        }
        .should_try_fallback());
    }

    #[test]
    fn test_recovery_error_into_aos_error() {
        let err = RecoveryError::<AosError>::CircuitOpen {
            service: "db".to_string(),
        };
        let aos_err: AosError = err.into();
        assert!(matches!(aos_err, AosError::CircuitBreakerOpen { .. }));
    }

    #[test]
    fn test_recovery_outcome_map() {
        let stats = RecoveryStats::first_attempt_success(Duration::from_millis(50));
        let outcome: RecoveryOutcome<i32, AosError> = RecoveryOutcome::success(21, stats);
        let mapped = outcome.map(|x| x * 2);

        assert_eq!(mapped.unwrap(), 42);
    }
}
