//! Retry logic with exponential backoff and jitter for resilient operations

use adapteros_core::CircuitState;
use crate::state::OperationRetryConfig;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Result of a retry attempt
#[derive(Debug, Clone)]
pub enum RetryResult<T, E> {
    /// Operation succeeded
    Success(T),
    /// Operation failed after all retries
    Failed(E),
    /// Operation timed out
    Timeout,
}

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
    pub jitter: f64,
}

impl From<&OperationRetryConfig> for RetryConfig {
    fn from(config: &OperationRetryConfig) -> Self {
        Self {
            max_attempts: config.max_retries + 1, // +1 for initial attempt
            initial_delay: Duration::from_millis(config.initial_retry_delay_ms),
            max_delay: Duration::from_millis(config.max_retry_delay_ms),
            backoff_multiplier: config.backoff_multiplier,
            jitter: config.jitter,
        }
    }
}

/// Execute an operation with retry logic
pub async fn retry_with_backoff<F, Fut, T, E>(
    operation: F,
    config: &RetryConfig,
    timeout: Duration,
) -> RetryResult<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Debug + Clone,
{
    let start_time = std::time::Instant::now();
    let mut attempt = 0;
    let mut current_delay = config.initial_delay;

    loop {
        attempt += 1;

        // Check if we've exceeded timeout
        if start_time.elapsed() >= timeout {
            debug!("Operation timed out after {} attempts", attempt - 1);
            return RetryResult::Timeout;
        }

        // Execute the operation
        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    debug!("Operation succeeded on attempt {}", attempt);
                }
                return RetryResult::Success(result);
            }
            Err(error) => {
                // Check if we've exhausted all retries
                if attempt >= config.max_attempts {
                    debug!("Operation failed after {} attempts: {:?}", attempt, error);
                    return RetryResult::Failed(error);
                }

                // Calculate next delay with exponential backoff and jitter
                let base_delay = current_delay.as_millis() as f64;
                let jitter_offset =
                    base_delay * config.jitter * (rand::random::<f64>() - 0.5) * 2.0;
                let next_delay = ((base_delay + jitter_offset) * config.backoff_multiplier) as u64;
                current_delay =
                    Duration::from_millis(next_delay.min(config.max_delay.as_millis() as u64));

                warn!(
                    attempt = attempt,
                    max_attempts = config.max_attempts,
                    delay_ms = current_delay.as_millis(),
                    error = ?error,
                    "Operation failed, retrying"
                );

                // Wait before retrying
                sleep(current_delay).await;
            }
        }
    }
}

// CircuitState is imported from adapteros_core::CircuitState

/// Circuit breaker for protecting against cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: tokio::sync::RwLock<CircuitState>,
    failure_count: tokio::sync::RwLock<u32>,
    success_count: tokio::sync::RwLock<u32>,
    next_attempt: tokio::sync::RwLock<std::time::Instant>,
    failure_threshold: u32,
    success_threshold: u32,
    timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, success_threshold: u32, timeout: Duration) -> Self {
        Self {
            state: tokio::sync::RwLock::new(CircuitState::Closed),
            failure_count: tokio::sync::RwLock::new(0),
            success_count: tokio::sync::RwLock::new(0),
            next_attempt: tokio::sync::RwLock::new(std::time::Instant::now()),
            failure_threshold,
            success_threshold,
            timeout,
        }
    }

    /// Execute an operation through the circuit breaker
    pub async fn execute<F, Fut, T, E>(&self, operation: F) -> Result<T, CircuitBreakerError<E>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, E>>,
    {
        let state = self.state.read().await.clone();

        match state {
            CircuitState::Open { .. } => {
                let next_attempt = *self.next_attempt.read().await;
                if std::time::Instant::now() < next_attempt {
                    return Err(CircuitBreakerError::CircuitOpen);
                } else {
                    // Transition to half-open
                    *self.state.write().await = CircuitState::HalfOpen;
                    *self.success_count.write().await = 0;
                }
            }
            CircuitState::HalfOpen => {
                // Allow the request to test recovery
            }
            CircuitState::Closed => {
                // Normal operation
            }
        }

        // Execute the operation
        match operation().await {
            Ok(result) => {
                self.record_success().await;
                Ok(result)
            }
            Err(error) => {
                self.record_failure().await;
                Err(CircuitBreakerError::OperationFailed(error))
            }
        }
    }

    async fn record_success(&self) {
        let mut state = self.state.write().await;
        let mut success_count = self.success_count.write().await;

        match *state {
            CircuitState::HalfOpen => {
                *success_count += 1;
                if *success_count >= self.success_threshold {
                    debug!("Circuit breaker closed - service recovered");
                    *state = CircuitState::Closed;
                    *self.failure_count.write().await = 0;
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success
                *self.failure_count.write().await = 0;
            }
            CircuitState::Open { .. } => {
                // Shouldn't happen, but reset if we get here
                *state = CircuitState::Closed;
                *self.failure_count.write().await = 0;
            }
        }
    }

    async fn record_failure(&self) {
        let mut state = self.state.write().await;
        let mut failure_count = self.failure_count.write().await;

        *failure_count += 1;

        if *failure_count >= self.failure_threshold {
            debug!("Circuit breaker opened - too many failures");
            let until = std::time::Instant::now() + self.timeout;
            *state = CircuitState::Open { until };
            *self.next_attempt.write().await = until;
        }
    }
}

/// Error types for circuit breaker operations
#[derive(Debug, Clone, thiserror::Error)]
pub enum CircuitBreakerError<E: std::fmt::Display> {
    #[error("Circuit breaker is open")]
    CircuitOpen,
    #[error("Operation failed: {0}")]
    OperationFailed(E),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success_first_attempt() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            jitter: 0.0,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(
            || async {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Ok::<_, String>("success".to_string())
            },
            &config,
            Duration::from_secs(1),
        )
        .await;

        assert!(matches!(result, RetryResult::Success(_)));
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_eventual_success() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            jitter: 0.0,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(
            || async {
                let count = call_count_clone.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err::<String, _>("temporary failure".to_string())
                } else {
                    Ok("success".to_string())
                }
            },
            &config,
            Duration::from_secs(1),
        )
        .await;

        assert!(matches!(result, RetryResult::Success(_)));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhaustion() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            jitter: 0.0,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_with_backoff(
            || async {
                call_count_clone.fetch_add(1, Ordering::SeqCst);
                Err::<String, _>("persistent failure".to_string())
            },
            &config,
            Duration::from_secs(1),
        )
        .await;

        assert!(matches!(result, RetryResult::Failed(_)));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_circuit_breaker() {
        let circuit_breaker = CircuitBreaker::new(2, 1, Duration::from_millis(100));

        // First failure
        let result1 = circuit_breaker
            .execute(|| async { Err::<(), _>("failure") })
            .await;
        assert!(matches!(
            result1,
            Err(CircuitBreakerError::OperationFailed(_))
        ));

        // Second failure - should open circuit
        let result2 = circuit_breaker
            .execute(|| async { Err::<(), _>("failure") })
            .await;
        assert!(matches!(
            result2,
            Err(CircuitBreakerError::OperationFailed(_))
        ));

        // Third attempt - should be rejected by open circuit
        let result3 = circuit_breaker
            .execute(|| async { Err::<(), &str>("failure") })
            .await;
        assert!(matches!(result3, Err(CircuitBreakerError::CircuitOpen)));

        // Wait for timeout and try success
        tokio::time::sleep(Duration::from_millis(150)).await;
        let result4: Result<(), CircuitBreakerError<&str>> =
            circuit_breaker.execute(|| async { Ok(()) }).await;
        assert!(matches!(result4, Ok(())));
    }
}
