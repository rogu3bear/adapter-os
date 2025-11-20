//! Retry logic and circuit breaker pattern for resilient MLX operations

use crate::error::MlxError;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Retry configuration with exponential backoff
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: usize,
    /// Initial backoff duration (milliseconds)
    pub initial_backoff_ms: u64,
    /// Maximum backoff duration (milliseconds)
    pub max_backoff_ms: u64,
    /// Backoff multiplier
    pub backoff_multiplier: f32,
    /// Whether to add jitter to backoff
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

impl RetryConfig {
    /// Create config for transient errors (network, temporary failures)
    pub fn transient() -> Self {
        Self {
            max_attempts: 5,
            initial_backoff_ms: 50,
            max_backoff_ms: 2000,
            backoff_multiplier: 1.5,
            jitter: true,
        }
    }

    /// Create config for resource exhaustion (OOM, GPU busy)
    pub fn resource_exhaustion() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_ms: 500,
            max_backoff_ms: 10000,
            backoff_multiplier: 2.5,
            jitter: false,
        }
    }

    /// Create config for model loading (slower operations)
    pub fn model_loading() -> Self {
        Self {
            max_attempts: 2,
            initial_backoff_ms: 1000,
            max_backoff_ms: 15000,
            backoff_multiplier: 3.0,
            jitter: false,
        }
    }

    /// Calculate backoff duration for attempt number
    pub fn backoff_duration(&self, attempt: usize) -> Duration {
        let base_ms = self.initial_backoff_ms as f32
            * self.backoff_multiplier.powi(attempt.saturating_sub(1) as i32);
        let clamped_ms = base_ms.min(self.max_backoff_ms as f32);

        let final_ms = if self.jitter {
            // Add ±25% jitter
            let jitter_factor = 1.0 + (rand::random::<f32>() - 0.5) * 0.5;
            clamped_ms * jitter_factor
        } else {
            clamped_ms
        };

        Duration::from_millis(final_ms as u64)
    }
}

/// Retry an operation with exponential backoff
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation_name` - Human-readable operation name for logging
/// * `operation` - Closure to retry
///
/// # Returns
/// Result from successful operation or final error after exhaustion
pub async fn retry_with_backoff<F, T, E>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, MlxError>
where
    F: FnMut() -> Result<T, E>,
    E: Into<MlxError>,
{
    let mut last_error: Option<MlxError> = None;

    for attempt in 1..=config.max_attempts {
        match operation() {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(
                        operation = %operation_name,
                        attempt = attempt,
                        "Operation succeeded after retry"
                    );
                }
                return Ok(result);
            }
            Err(error) => {
                let mlx_error = error.into();

                // Don't retry non-recoverable errors
                if !mlx_error.is_recoverable() {
                    tracing::error!(
                        operation = %operation_name,
                        attempt = attempt,
                        error = %mlx_error,
                        "Non-recoverable error, aborting retry"
                    );
                    return Err(mlx_error);
                }

                last_error = Some(mlx_error.clone());

                if attempt < config.max_attempts {
                    let backoff = config.backoff_duration(attempt);
                    tracing::warn!(
                        operation = %operation_name,
                        attempt = attempt,
                        max_attempts = config.max_attempts,
                        backoff_ms = backoff.as_millis(),
                        error = %mlx_error,
                        severity = %mlx_error.severity(),
                        hint = mlx_error.recovery_hint(),
                        "Operation failed, retrying after backoff"
                    );

                    tokio::time::sleep(backoff).await;
                } else {
                    tracing::error!(
                        operation = %operation_name,
                        attempts = attempt,
                        error = %mlx_error,
                        "Retry exhausted"
                    );
                }
            }
        }
    }

    Err(MlxError::RetryExhausted {
        operation: operation_name.to_string(),
        attempts: config.max_attempts,
        last_error: Box::new(last_error.unwrap()),
    })
}

/// Synchronous version of retry_with_backoff
pub fn retry_with_backoff_sync<F, T, E>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, MlxError>
where
    F: FnMut() -> Result<T, E>,
    E: Into<MlxError>,
{
    let mut last_error: Option<MlxError> = None;

    for attempt in 1..=config.max_attempts {
        match operation() {
            Ok(result) => {
                if attempt > 1 {
                    tracing::info!(
                        operation = %operation_name,
                        attempt = attempt,
                        "Operation succeeded after retry"
                    );
                }
                return Ok(result);
            }
            Err(error) => {
                let mlx_error = error.into();

                if !mlx_error.is_recoverable() {
                    tracing::error!(
                        operation = %operation_name,
                        attempt = attempt,
                        error = %mlx_error,
                        "Non-recoverable error, aborting retry"
                    );
                    return Err(mlx_error);
                }

                last_error = Some(mlx_error.clone());

                if attempt < config.max_attempts {
                    let backoff = config.backoff_duration(attempt);
                    tracing::warn!(
                        operation = %operation_name,
                        attempt = attempt,
                        max_attempts = config.max_attempts,
                        backoff_ms = backoff.as_millis(),
                        error = %mlx_error,
                        "Operation failed, retrying after backoff"
                    );

                    std::thread::sleep(backoff);
                }
            }
        }
    }

    Err(MlxError::RetryExhausted {
        operation: operation_name.to_string(),
        attempts: config.max_attempts,
        last_error: Box::new(last_error.unwrap()),
    })
}

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitState {
    Closed,  // Normal operation
    Open,    // Failures exceeded threshold, blocking requests
    HalfOpen, // Testing if service recovered
}

/// Circuit breaker for preventing cascade failures
pub struct CircuitBreaker {
    /// Operation name
    operation: String,
    /// Current state
    state: Arc<Mutex<CircuitState>>,
    /// Consecutive failure count
    failure_count: AtomicUsize,
    /// Success count in half-open state
    success_count: AtomicUsize,
    /// Failure threshold to open circuit
    failure_threshold: usize,
    /// Success threshold to close circuit from half-open
    success_threshold: usize,
    /// Timeout before trying half-open (milliseconds)
    timeout_ms: AtomicU64,
    /// Last failure timestamp
    last_failure: Arc<Mutex<Option<Instant>>>,
}

impl CircuitBreaker {
    /// Create new circuit breaker
    pub fn new(operation: impl Into<String>, failure_threshold: usize, timeout_ms: u64) -> Self {
        Self {
            operation: operation.into(),
            state: Arc::new(Mutex::new(CircuitState::Closed)),
            failure_count: AtomicUsize::new(0),
            success_count: AtomicUsize::new(0),
            failure_threshold,
            success_threshold: 2, // Require 2 successes to close from half-open
            timeout_ms: AtomicU64::new(timeout_ms),
            last_failure: Arc::new(Mutex::new(None)),
        }
    }

    /// Execute operation through circuit breaker
    pub fn call<F, T, E>(&self, operation: F) -> Result<T, MlxError>
    where
        F: FnOnce() -> Result<T, E>,
        E: Into<MlxError>,
    {
        // Check if circuit should transition to half-open
        self.check_half_open_transition();

        // Check current state
        let state = *self.state.lock().unwrap();

        match state {
            CircuitState::Open => {
                let failures = self.failure_count.load(Ordering::Relaxed);
                let timeout = self.timeout_ms.load(Ordering::Relaxed);
                Err(MlxError::CircuitBreakerOpen {
                    operation: self.operation.clone(),
                    failures,
                    retry_after_ms: timeout,
                })
            }
            CircuitState::Closed | CircuitState::HalfOpen => {
                match operation() {
                    Ok(result) => {
                        self.record_success();
                        Ok(result)
                    }
                    Err(error) => {
                        let mlx_error = error.into();
                        self.record_failure();
                        Err(mlx_error)
                    }
                }
            }
        }
    }

    /// Check if circuit should transition to half-open
    fn check_half_open_transition(&self) {
        let state = *self.state.lock().unwrap();
        if state != CircuitState::Open {
            return;
        }

        let last_failure = self.last_failure.lock().unwrap();
        if let Some(instant) = *last_failure {
            let timeout = Duration::from_millis(self.timeout_ms.load(Ordering::Relaxed));
            if instant.elapsed() >= timeout {
                drop(last_failure);
                *self.state.lock().unwrap() = CircuitState::HalfOpen;
                self.success_count.store(0, Ordering::Relaxed);
                tracing::info!(
                    operation = %self.operation,
                    "Circuit breaker transitioning to half-open"
                );
            }
        }
    }

    /// Record successful operation
    fn record_success(&self) {
        let state = *self.state.lock().unwrap();

        match state {
            CircuitState::HalfOpen => {
                let successes = self.success_count.fetch_add(1, Ordering::Relaxed) + 1;
                if successes >= self.success_threshold {
                    *self.state.lock().unwrap() = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::Relaxed);
                    tracing::info!(
                        operation = %self.operation,
                        "Circuit breaker closed after successful recovery"
                    );
                }
            }
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count.store(0, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// Record failed operation
    fn record_failure(&self) {
        let failures = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        *self.last_failure.lock().unwrap() = Some(Instant::now());

        let state = *self.state.lock().unwrap();

        match state {
            CircuitState::Closed if failures >= self.failure_threshold => {
                *self.state.lock().unwrap() = CircuitState::Open;
                tracing::warn!(
                    operation = %self.operation,
                    failures = failures,
                    threshold = self.failure_threshold,
                    "Circuit breaker opened due to consecutive failures"
                );
            }
            CircuitState::HalfOpen => {
                // Failed in half-open, go back to open
                *self.state.lock().unwrap() = CircuitState::Open;
                self.success_count.store(0, Ordering::Relaxed);
                tracing::warn!(
                    operation = %self.operation,
                    "Circuit breaker re-opened after failure in half-open state"
                );
            }
            _ => {}
        }
    }

    /// Get current state (for monitoring)
    pub fn state(&self) -> String {
        let state = *self.state.lock().unwrap();
        format!("{:?}", state)
    }

    /// Get failure count (for monitoring)
    pub fn failure_count(&self) -> usize {
        self.failure_count.load(Ordering::Relaxed)
    }

    /// Manually reset circuit breaker
    pub fn reset(&self) {
        *self.state.lock().unwrap() = CircuitState::Closed;
        self.failure_count.store(0, Ordering::Relaxed);
        self.success_count.store(0, Ordering::Relaxed);
        *self.last_failure.lock().unwrap() = None;
        tracing::info!(
            operation = %self.operation,
            "Circuit breaker manually reset"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_backoff() {
        let config = RetryConfig::default();

        let backoff1 = config.backoff_duration(1);
        let backoff2 = config.backoff_duration(2);
        let backoff3 = config.backoff_duration(3);

        // Backoff should increase
        assert!(backoff2 > backoff1);
        assert!(backoff3 > backoff2);

        // Should not exceed max
        let backoff_large = config.backoff_duration(100);
        assert!(backoff_large.as_millis() <= config.max_backoff_ms as u128);
    }

    #[test]
    fn test_circuit_breaker_opens() {
        let breaker = CircuitBreaker::new("test_op", 3, 1000);

        // Record failures
        for _ in 0..3 {
            let result: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::Internal {
                    message: "test".to_string(),
                })
            });
            assert!(result.is_err());
        }

        // Circuit should be open
        let result: Result<(), MlxError> = breaker.call(|| {
            Ok::<(), MlxError>(())
        });
        assert!(matches!(result, Err(MlxError::CircuitBreakerOpen { .. })));
    }

    #[test]
    fn test_circuit_breaker_closes() {
        let breaker = CircuitBreaker::new("test_op", 3, 100);

        // Open the circuit
        for _ in 0..3 {
            let _: Result<(), MlxError> = breaker.call(|| {
                Err(MlxError::Internal {
                    message: "test".to_string(),
                })
            });
        }

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(150));

        // Should transition to half-open and allow test
        let result: Result<(), MlxError> = breaker.call(|| {
            Ok::<(), MlxError>(())
        });
        assert!(result.is_ok());

        // Record another success to close
        let result: Result<(), MlxError> = breaker.call(|| {
            Ok::<(), MlxError>(())
        });
        assert!(result.is_ok());

        // Circuit should be closed
        assert_eq!(breaker.state(), "Closed");
    }

    #[test]
    fn test_retry_sync_success() {
        let config = RetryConfig::default();
        let mut attempts = 0;

        let result = retry_with_backoff_sync(&config, "test_op", || {
            attempts += 1;
            if attempts < 2 {
                Err(MlxError::Internal {
                    message: "transient".to_string(),
                })
            } else {
                Ok(42)
            }
        });

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempts, 2);
    }

    #[test]
    fn test_retry_sync_exhausted() {
        let config = RetryConfig {
            max_attempts: 2,
            ..Default::default()
        };

        let result: Result<(), MlxError> = retry_with_backoff_sync(&config, "test_op", || {
            Err(MlxError::GpuOomError {
                requested_mb: 100.0,
                available_mb: 50.0,
                hint: "test".to_string(),
            })
        });

        assert!(matches!(result, Err(MlxError::RetryExhausted { .. })));
    }

    #[test]
    fn test_retry_non_recoverable() {
        let config = RetryConfig::default();

        let result: Result<(), MlxError> = retry_with_backoff_sync(&config, "test_op", || {
            Err(MlxError::ValidationError {
                check: "test".to_string(),
                reason: "invalid".to_string(),
            })
        });

        // Should fail immediately without retry
        assert!(matches!(result, Err(MlxError::ValidationError { .. })));
    }
}
