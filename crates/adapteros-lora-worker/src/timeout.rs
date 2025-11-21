//! Request timeout and circuit breaker mechanisms
//!
//! Implements timeout protection and circuit breaker patterns to prevent runaway processes.
//! Aligns with Memory Ruleset #12 and Performance Ruleset #11 from policy enforcement.

use adapteros_core::{AosError, CircuitState, Result};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing::error;

/// Timeout configuration per request type
#[derive(Debug, Clone)]
pub struct TimeoutConfig {
    pub inference_timeout: Duration,
    pub evidence_timeout: Duration,
    pub router_timeout: Duration,
    pub policy_timeout: Duration,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            inference_timeout: Duration::from_secs(30),
            evidence_timeout: Duration::from_secs(5),
            router_timeout: Duration::from_millis(100),
            policy_timeout: Duration::from_millis(50),
        }
    }
}

// CircuitState is imported from adapteros_core::CircuitState

/// Circuit breaker for runaway detection
pub struct CircuitBreaker {
    state: CircuitState,
    failure_threshold: usize,
    failure_count: AtomicUsize,
    _timeout: Duration,
    last_failure: AtomicU64,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, timeout: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_threshold,
            failure_count: AtomicUsize::new(0),
            _timeout: timeout,
            last_failure: AtomicU64::new(0),
        }
    }

    pub async fn call<F, T>(&self, f: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        // Check circuit state
        match &self.state {
            CircuitState::Open { until } => {
                if Instant::now() < *until {
                    return Err(AosError::Worker("Circuit breaker open".to_string()));
                }
                // Transition to half-open (would need interior mutability in real implementation)
            }
            CircuitState::HalfOpen => {
                // Allow one request through
            }
            CircuitState::Closed => {
                // Normal operation
            }
        }

        let result = f.await;

        match result {
            Ok(value) => {
                self.on_success();
                Ok(value)
            }
            Err(e) => {
                self.on_failure();
                Err(e)
            }
        }
    }

    fn on_success(&self) {
        self.failure_count.store(0, Ordering::Relaxed);
        // In real implementation, would need interior mutability to update state
    }

    fn on_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::Relaxed) + 1;
        self.last_failure.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_secs(),
            Ordering::Relaxed,
        );

        if count >= self.failure_threshold {
            error!("Circuit breaker threshold reached: {} failures", count);
            // In real implementation, would transition to Open state
        }
    }

    pub fn is_open(&self) -> bool {
        matches!(self.state, CircuitState::Open { .. })
    }

    pub fn failure_count(&self) -> usize {
        self.failure_count.load(Ordering::Relaxed)
    }
}

/// Timeout wrapper for async operations
pub struct TimeoutWrapper {
    config: TimeoutConfig,
}

impl TimeoutWrapper {
    pub fn new(config: TimeoutConfig) -> Self {
        Self { config }
    }

    /// Wrap an async operation with timeout
    pub async fn with_timeout<F, T>(&self, operation: F, timeout_duration: Duration) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        timeout(timeout_duration, operation)
            .await
            .map_err(|_| AosError::Worker("Operation timeout".to_string()))?
    }

    /// Wrap inference with timeout
    pub async fn infer_with_timeout<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.with_timeout(operation, self.config.inference_timeout)
            .await
    }

    /// Wrap evidence retrieval with timeout
    pub async fn evidence_with_timeout<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.with_timeout(operation, self.config.evidence_timeout)
            .await
    }

    /// Wrap router operation with timeout
    pub async fn router_with_timeout<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.with_timeout(operation, self.config.router_timeout)
            .await
    }

    /// Wrap policy check with timeout
    pub async fn policy_with_timeout<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>>,
    {
        self.with_timeout(operation, self.config.policy_timeout)
            .await
    }
}

/// Timeout event for telemetry
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimeoutEvent {
    pub operation_type: String,
    pub timeout_duration_ms: u64,
    pub actual_duration_ms: u64,
    pub timed_out: bool,
    pub timestamp: u64,
}

impl TimeoutEvent {
    pub fn new(
        operation_type: String,
        timeout_duration: Duration,
        actual_duration: Duration,
        timed_out: bool,
    ) -> Self {
        Self {
            operation_type,
            timeout_duration_ms: timeout_duration.as_millis() as u64,
            actual_duration_ms: actual_duration.as_millis() as u64,
            timed_out,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_secs(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_circuit_breaker_success() {
        let breaker = CircuitBreaker::new(3, Duration::from_secs(10));

        let result = breaker.call(async { Ok("success") }).await;

        assert!(result.is_ok());
        assert_eq!(breaker.failure_count(), 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_failure() {
        let breaker = CircuitBreaker::new(2, Duration::from_secs(10));

        // First failure
        let result: Result<()> = breaker
            .call(async { Err(AosError::Worker("test failure".to_string())) })
            .await;
        assert!(result.is_err());
        assert_eq!(breaker.failure_count(), 1);

        // Second failure - should trigger threshold
        let result: Result<()> = breaker
            .call(async { Err(AosError::Worker("test failure".to_string())) })
            .await;
        assert!(result.is_err());
        assert_eq!(breaker.failure_count(), 2);
    }

    #[tokio::test]
    async fn test_timeout_wrapper() {
        let wrapper = TimeoutWrapper::new(TimeoutConfig::default());

        // Test successful operation
        let result = wrapper
            .with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok("success")
                },
                Duration::from_millis(100),
            )
            .await;

        assert!(result.is_ok());

        // Test timeout
        let result = wrapper
            .with_timeout(
                async {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Ok("success")
                },
                Duration::from_millis(100),
            )
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AosError::Worker(msg) if msg.contains("timeout")));
    }
}
