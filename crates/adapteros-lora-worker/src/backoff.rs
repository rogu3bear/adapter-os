//! Exponential backoff and circuit breaker patterns for background tasks
//!
//! This module provides utilities for adding resilience to background tasks
//! that handle errors in loops. It implements:
//! - Exponential backoff with configurable parameters
//! - Circuit breaker pattern for detecting and handling repeated failures
//!
//! # Usage
//!
//! ```rust
//! use adapteros_lora_worker::backoff::{BackoffConfig, CircuitBreaker};
//! use std::time::Duration;
//!
//! let backoff = BackoffConfig::default();
//! let circuit_breaker = CircuitBreaker::new(5, Duration::from_secs(60));
//!
//! loop {
//!     if circuit_breaker.is_open() {
//!         tracing::warn!("Circuit breaker is open, pausing operation");
//!         tokio::time::sleep(circuit_breaker.reset_timeout()).await;
//!         continue;
//!     }
//!
//!     match do_work().await {
//!         Ok(_) => {
//!             circuit_breaker.record_success();
//!             // Reset backoff on success
//!         }
//!         Err(e) => {
//!             circuit_breaker.record_failure();
//!             tracing::error!("Operation failed: {}", e);
//!
//!             let delay = backoff.next_delay(attempt);
//!             tokio::time::sleep(delay).await;
//!         }
//!     }
//! }
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

/// Exponential backoff configuration
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub multiplier: f64,
    pub max_retries: u32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            multiplier: 2.0,
            max_retries: 10,
        }
    }
}

impl BackoffConfig {
    /// Create a new backoff configuration
    pub fn new(
        initial_delay: Duration,
        max_delay: Duration,
        multiplier: f64,
        max_retries: u32,
    ) -> Self {
        Self {
            initial_delay,
            max_delay,
            multiplier,
            max_retries,
        }
    }

    /// Calculate the delay for a given attempt number
    pub fn next_delay(&self, attempt: u32) -> Duration {
        if attempt >= self.max_retries {
            return self.max_delay;
        }

        let delay_ms = self.initial_delay.as_millis() as f64
            * self.multiplier.powi(attempt as i32);

        let delay = Duration::from_millis(delay_ms as u64);

        std::cmp::min(delay, self.max_delay)
    }

    /// Check if we've exceeded max retries
    pub fn should_give_up(&self, attempt: u32) -> bool {
        attempt >= self.max_retries
    }
}

/// Circuit breaker state
pub struct CircuitBreaker {
    failure_count: AtomicU32,
    last_failure: AtomicU64,
    threshold: u32,
    reset_timeout: Duration,
}

impl CircuitBreaker {
    /// Create a new circuit breaker
    ///
    /// # Arguments
    /// * `threshold` - Number of consecutive failures before opening the circuit
    /// * `reset_timeout` - Duration to wait before attempting to close the circuit
    pub fn new(threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_count: AtomicU32::new(0),
            last_failure: AtomicU64::new(0),
            threshold,
            reset_timeout,
        }
    }

    /// Record a failure and increment the failure count
    pub fn record_failure(&self) {
        self.failure_count.fetch_add(1, Ordering::SeqCst);
        self.last_failure.store(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::SeqCst,
        );
    }

    /// Check if the circuit breaker is open (preventing operations)
    pub fn is_open(&self) -> bool {
        let failures = self.failure_count.load(Ordering::SeqCst);
        if failures < self.threshold {
            return false;
        }

        // Check if reset timeout has passed
        let last = self.last_failure.load(Ordering::SeqCst);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        now - last < self.reset_timeout.as_secs()
    }

    /// Record a successful operation and reset the failure count
    pub fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
    }

    /// Get the current failure count
    pub fn failure_count(&self) -> u32 {
        self.failure_count.load(Ordering::SeqCst)
    }

    /// Get the reset timeout duration
    pub fn reset_timeout(&self) -> Duration {
        self.reset_timeout
    }

    /// Check if the circuit breaker can be reset (timeout has passed)
    pub fn can_reset(&self) -> bool {
        let last = self.last_failure.load(Ordering::SeqCst);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        now - last >= self.reset_timeout.as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_config_default() {
        let config = BackoffConfig::default();
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert_eq!(config.multiplier, 2.0);
        assert_eq!(config.max_retries, 10);
    }

    #[test]
    fn test_backoff_delay_calculation() {
        let config = BackoffConfig::default();

        // First attempt: 100ms
        assert_eq!(config.next_delay(0), Duration::from_millis(100));

        // Second attempt: 200ms
        assert_eq!(config.next_delay(1), Duration::from_millis(200));

        // Third attempt: 400ms
        assert_eq!(config.next_delay(2), Duration::from_millis(400));

        // Fourth attempt: 800ms
        assert_eq!(config.next_delay(3), Duration::from_millis(800));
    }

    #[test]
    fn test_backoff_max_delay() {
        let config = BackoffConfig::default();

        // After many attempts, should cap at max_delay
        let delay = config.next_delay(20);
        assert_eq!(delay, config.max_delay);
    }

    #[test]
    fn test_backoff_should_give_up() {
        let config = BackoffConfig::default();

        assert!(!config.should_give_up(5));
        assert!(config.should_give_up(10));
        assert!(config.should_give_up(15));
    }

    #[test]
    fn test_circuit_breaker_threshold() {
        let breaker = CircuitBreaker::new(3, Duration::from_secs(60));

        // Should be closed initially
        assert!(!breaker.is_open());

        // Record failures
        breaker.record_failure();
        assert!(!breaker.is_open()); // 1 failure

        breaker.record_failure();
        assert!(!breaker.is_open()); // 2 failures

        breaker.record_failure();
        assert!(breaker.is_open()); // 3 failures - threshold reached
    }

    #[test]
    fn test_circuit_breaker_success_reset() {
        let breaker = CircuitBreaker::new(3, Duration::from_secs(60));

        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.failure_count(), 2);

        // Success should reset
        breaker.record_success();
        assert_eq!(breaker.failure_count(), 0);
        assert!(!breaker.is_open());
    }

    #[tokio::test]
    async fn test_circuit_breaker_timeout() {
        let breaker = CircuitBreaker::new(2, Duration::from_millis(100));

        // Open the circuit
        breaker.record_failure();
        breaker.record_failure();
        assert!(breaker.is_open());

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Circuit should allow reset attempt
        assert!(breaker.can_reset());
        assert!(!breaker.is_open());
    }
}
