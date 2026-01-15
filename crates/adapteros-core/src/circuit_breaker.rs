//! Standardized Circuit Breaker Interface
//!
//! Provides a unified circuit breaker pattern across adapterOS services for protecting
//! against cascading failures. Implements configurable thresholds, automatic recovery,
//! and comprehensive metrics.
//!
//! # Architecture
//!
//! Circuit breakers protect critical services by:
//! - **Closed State**: Normal operation, requests flow through
//! - **Open State**: Service is failing, requests are rejected immediately
//! - **Half-Open State**: Testing recovery, limited requests allowed
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_core::{CircuitBreaker, CircuitBreakerConfig, CircuitState, StandardCircuitBreaker};
//!
//! // Create a circuit breaker with custom config
//! let config = CircuitBreakerConfig {
//!     failure_threshold: 5,
//!     success_threshold: 3,
//!     timeout_ms: 60000,
//!     half_open_max_requests: 10,
//! };
//!
//! let breaker = StandardCircuitBreaker::new("database".to_string(), config);
//!
//! // Use in async operations
//! async {
//!     let result = breaker.call(async {
//!         // Your operation here
//!         Ok::<_, adapteros_core::AosError>("success")
//!     }).await;
//! };
//! ```

use crate::{AosError, Result};
use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Circuit breaker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation - requests flow through
    Closed,
    /// Service is failing - requests are rejected
    Open { until: Instant },
    /// Testing recovery - limited requests allowed
    HalfOpen,
}

impl fmt::Display for CircuitState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "closed"),
            CircuitState::Open { .. } => write!(f, "open"),
            CircuitState::HalfOpen => write!(f, "half_open"),
        }
    }
}

/// Circuit breaker configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct CircuitBreakerConfig {
    /// Number of consecutive failures before opening circuit
    pub failure_threshold: usize,
    /// Number of consecutive successes needed to close circuit from half-open
    pub success_threshold: usize,
    /// Time in milliseconds to wait before transitioning to half-open
    pub timeout_ms: u64,
    /// Maximum requests allowed in half-open state before considering success/failure
    pub half_open_max_requests: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            timeout_ms: 60000, // 1 minute
            half_open_max_requests: 10,
        }
    }
}

/// Circuit breaker metrics for monitoring
#[derive(Debug, Clone)]
pub struct CircuitBreakerMetrics {
    /// Current state of the circuit breaker
    pub state: CircuitState,
    /// Total number of requests processed
    pub requests_total: u64,
    /// Total number of successful requests
    pub successes_total: u64,
    /// Total number of failed requests
    pub failures_total: u64,
    /// Number of times circuit has opened
    pub opens_total: u64,
    /// Number of times circuit has closed
    pub closes_total: u64,
    /// Number of transitions to half-open state
    pub half_opens_total: u64,
    /// Timestamp of last state change (Unix timestamp in seconds)
    pub last_state_change: u64,
}

impl Default for CircuitBreakerMetrics {
    fn default() -> Self {
        Self {
            state: CircuitState::Closed,
            requests_total: 0,
            successes_total: 0,
            failures_total: 0,
            opens_total: 0,
            closes_total: 0,
            half_opens_total: 0,
            last_state_change: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

/// Standardized circuit breaker trait
#[async_trait::async_trait]
pub trait CircuitBreaker: Send + Sync {
    /// Execute an async operation through the circuit breaker
    ///
    /// Returns the operation result if successful, or an error if the circuit is open
    /// or the operation fails.
    async fn call<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>> + Send,
        T: Send;

    /// Get current circuit breaker state
    ///
    /// Returns the current circuit state. In case of lock contention,
    /// returns CircuitState::Closed as a safe default to allow operations to continue.
    fn state(&self) -> CircuitState;

    /// Get current circuit breaker metrics
    fn metrics(&self) -> CircuitBreakerMetrics;

    /// Get circuit breaker name for identification
    fn name(&self) -> &str;

    /// Execute a boxed future through the circuit breaker
    async fn call_boxed(
        &self,
        _operation: std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send>,
        >,
    ) -> Result<serde_json::Value> {
        // Default implementation - concrete types should override
        Err(AosError::Internal("call_boxed not implemented".to_string()))
    }
}

/// Standard circuit breaker implementation
pub struct StandardCircuitBreaker {
    name: String,
    config: CircuitBreakerConfig,
    state: Mutex<CircuitState>,
    /// Atomic cache for non-blocking sync state access.
    /// Encoding: bits 0-1 = state (0=Closed, 1=Open, 2=HalfOpen)
    ///           bits 2-63 = deadline seconds for Open state
    cached_state: AtomicU64,
    consecutive_failures: AtomicUsize,
    consecutive_successes: AtomicUsize,
    half_open_requests: AtomicUsize,
    requests_total: AtomicU64,
    successes_total: AtomicU64,
    failures_total: AtomicU64,
    opens_total: AtomicU64,
    closes_total: AtomicU64,
    half_opens_total: AtomicU64,
    last_state_change: AtomicU64,
}

impl Clone for StandardCircuitBreaker {
    fn clone(&self) -> Self {
        // For testing purposes, we need to copy the current state
        // This is safe because tests don't run operations concurrently during cloning
        let current_state = self
            .state
            .try_lock()
            .map(|guard| *guard)
            .unwrap_or(CircuitState::Closed);

        Self {
            name: self.name.clone(),
            config: self.config.clone(),
            state: Mutex::new(current_state),
            cached_state: AtomicU64::new(self.cached_state.load(Ordering::Acquire)),
            consecutive_failures: AtomicUsize::new(
                self.consecutive_failures.load(Ordering::Relaxed),
            ),
            consecutive_successes: AtomicUsize::new(
                self.consecutive_successes.load(Ordering::Relaxed),
            ),
            half_open_requests: AtomicUsize::new(self.half_open_requests.load(Ordering::Relaxed)),
            requests_total: AtomicU64::new(self.requests_total.load(Ordering::Relaxed)),
            successes_total: AtomicU64::new(self.successes_total.load(Ordering::Relaxed)),
            failures_total: AtomicU64::new(self.failures_total.load(Ordering::Relaxed)),
            opens_total: AtomicU64::new(self.opens_total.load(Ordering::Relaxed)),
            closes_total: AtomicU64::new(self.closes_total.load(Ordering::Relaxed)),
            half_opens_total: AtomicU64::new(self.half_opens_total.load(Ordering::Relaxed)),
            last_state_change: AtomicU64::new(self.last_state_change.load(Ordering::Relaxed)),
        }
    }
}

impl StandardCircuitBreaker {
    /// Create a new circuit breaker with the given configuration
    pub fn new(name: String, config: CircuitBreakerConfig) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            name,
            config,
            state: Mutex::new(CircuitState::Closed),
            cached_state: AtomicU64::new(0), // 0 = Closed
            consecutive_failures: AtomicUsize::new(0),
            consecutive_successes: AtomicUsize::new(0),
            half_open_requests: AtomicUsize::new(0),
            requests_total: AtomicU64::new(0),
            successes_total: AtomicU64::new(0),
            failures_total: AtomicU64::new(0),
            opens_total: AtomicU64::new(0),
            closes_total: AtomicU64::new(0),
            half_opens_total: AtomicU64::new(0),
            last_state_change: AtomicU64::new(now),
        }
    }

    /// Encode a circuit state into an atomic u64 for cache storage.
    /// Bits 0-1: state (0=Closed, 1=Open, 2=HalfOpen)
    /// Bits 2-63: deadline seconds since UNIX epoch for Open state
    fn encode_state_for_cache(state: CircuitState) -> u64 {
        match state {
            CircuitState::Closed => 0,
            CircuitState::Open { until } => {
                // Store deadline as seconds from now (approximate)
                let deadline_secs = until
                    .checked_duration_since(Instant::now())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                1 | (deadline_secs << 2)
            }
            CircuitState::HalfOpen => 2,
        }
    }

    /// Update the cached state atomically for non-blocking sync access.
    fn update_state_cache(&self, state: CircuitState) {
        let encoded = Self::encode_state_for_cache(state);
        self.cached_state.store(encoded, Ordering::Release);
    }

    /// Decode cached state into a CircuitState.
    fn decode_cached_state(&self) -> CircuitState {
        let cached = self.cached_state.load(Ordering::Acquire);
        let state_bits = (cached & 0x3) as u8;
        match state_bits {
            0 => CircuitState::Closed,
            1 => {
                let deadline_secs = cached >> 2;
                if deadline_secs == 0 {
                    // Timeout expired or no deadline stored
                    CircuitState::HalfOpen
                } else {
                    CircuitState::Open {
                        until: Instant::now() + Duration::from_secs(deadline_secs),
                    }
                }
            }
            _ => CircuitState::HalfOpen,
        }
    }

    /// Update the last state change timestamp
    fn update_state_change_time(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_state_change.store(now, Ordering::Relaxed);
    }

    /// Transition to a new state
    async fn transition_to(&self, new_state: CircuitState) {
        let mut state = self.state.lock().await;
        let old_state = *state;
        *state = new_state;
        drop(state); // Release lock before updating timestamp
        self.update_state_change_time();
        self.update_state_cache(new_state);

        let failures_total = self.failures_total.load(Ordering::Relaxed);
        let timeout_secs = Duration::from_millis(self.config.timeout_ms).as_secs();

        // Emit telemetry events for state transitions
        match new_state {
            CircuitState::Open { .. } => {
                self.opens_total.fetch_add(1, Ordering::Relaxed);
                self.consecutive_failures.store(0, Ordering::Relaxed);
                self.consecutive_successes.store(0, Ordering::Relaxed);

                // Only emit if transitioning from closed/half-open to open
                if !matches!(old_state, CircuitState::Open { .. }) {
                    tracing::warn!(
                        breaker_name = %self.name,
                        from_state = %old_state,
                        to_state = "open",
                        timeout_secs = timeout_secs,
                        failures_total = failures_total,
                        failure_threshold = self.config.failure_threshold,
                        "Circuit breaker opened - service protection engaged"
                    );
                    self.emit_telemetry_event(new_state).await;
                }
            }
            CircuitState::Closed => {
                self.closes_total.fetch_add(1, Ordering::Relaxed);
                self.consecutive_failures.store(0, Ordering::Relaxed);
                self.consecutive_successes.store(0, Ordering::Relaxed);
                self.half_open_requests.store(0, Ordering::Relaxed);

                // Only emit if transitioning to closed from open/half-open
                if matches!(
                    old_state,
                    CircuitState::Open { .. } | CircuitState::HalfOpen
                ) {
                    tracing::info!(
                        breaker_name = %self.name,
                        from_state = %old_state,
                        to_state = "closed",
                        timeout_secs = timeout_secs,
                        failures_total = failures_total,
                        success_threshold = self.config.success_threshold,
                        "Circuit breaker closed - service recovered"
                    );
                    self.emit_telemetry_event(new_state).await;
                }
            }
            CircuitState::HalfOpen => {
                self.half_opens_total.fetch_add(1, Ordering::Relaxed);
                self.half_open_requests.store(0, Ordering::Relaxed);

                tracing::info!(
                    breaker_name = %self.name,
                    from_state = %old_state,
                    to_state = "half_open",
                    timeout_secs = timeout_secs,
                    failures_total = failures_total,
                    half_open_max_requests = self.config.half_open_max_requests,
                    "Circuit breaker half-open - testing service recovery"
                );
                // Always emit half-open transitions
                self.emit_telemetry_event(new_state).await;
            }
        }
    }

    /// Emit telemetry event for state transition
    async fn emit_telemetry_event(&self, _new_state: CircuitState) {
        // Telemetry emission would be handled by the caller or through a callback
        // to avoid circular dependencies between core and telemetry crates
    }

    /// Handle a successful operation
    async fn on_success(&self) {
        let current_state = self.state_async().await;
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.successes_total.fetch_add(1, Ordering::Relaxed);

        match current_state {
            CircuitState::Closed => {
                // Reset failure counter on success in closed state
                self.consecutive_failures.store(0, Ordering::Relaxed);
            }
            CircuitState::HalfOpen => {
                let successes = self.consecutive_successes.fetch_add(1, Ordering::Relaxed) + 1;
                // Decrement concurrent request counter with bounds checking
                let _ = self.half_open_requests.fetch_update(
                    Ordering::AcqRel,
                    Ordering::Acquire,
                    |current| {
                        if current > 0 {
                            Some(current - 1)
                        } else {
                            Some(0)
                        }
                    },
                );

                // Close circuit only if we have enough consecutive successes
                if successes >= self.config.success_threshold {
                    self.transition_to(CircuitState::Closed).await;
                }
            }
            CircuitState::Open { .. } => {
                // This shouldn't happen, but reset counters if it does
                self.consecutive_failures.store(0, Ordering::Relaxed);
            }
        }
    }

    /// Calculate the timeout instant for opening the circuit
    fn calculate_timeout(&self) -> Instant {
        Instant::now() + Duration::from_millis(self.config.timeout_ms)
    }

    /// Handle a failed operation
    async fn on_failure(&self) {
        let current_state = self.state_async().await;
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        self.failures_total.fetch_add(1, Ordering::Relaxed);

        match current_state {
            CircuitState::Closed => {
                let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
                if failures >= self.config.failure_threshold {
                    let until = self.calculate_timeout();
                    self.transition_to(CircuitState::Open { until }).await;
                }
            }
            CircuitState::HalfOpen => {
                // Decrement concurrent request counter with bounds checking
                let _ = self.half_open_requests.fetch_update(
                    Ordering::AcqRel,
                    Ordering::Acquire,
                    |current| {
                        if current > 0 {
                            Some(current - 1)
                        } else {
                            Some(0)
                        }
                    },
                );

                // Any failure in half-open immediately re-opens (traditional circuit breaker behavior)
                let until = self.calculate_timeout();
                self.transition_to(CircuitState::Open { until }).await;
            }
            CircuitState::Open { .. } => {
                // Already open, just increment counters
            }
        }
    }

    /// Get current state (async version for internal use)
    async fn state_async(&self) -> CircuitState {
        let mut state = self.state.lock().await;

        // Check if we should transition from Open to HalfOpen
        if let CircuitState::Open { until } = *state {
            if Instant::now() >= until {
                *state = CircuitState::HalfOpen;
                drop(state);
                self.update_state_change_time();
                self.update_state_cache(CircuitState::HalfOpen);
                // Note: half_opens_total is incremented in transition_to() to avoid duplicates
                self.half_opens_total.fetch_add(1, Ordering::Relaxed);
                return CircuitState::HalfOpen;
            }
        }

        *state
    }
}

#[async_trait::async_trait]
impl CircuitBreaker for StandardCircuitBreaker {
    async fn call<F, T>(&self, operation: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T>> + Send,
        T: Send,
    {
        // Check current state
        let current_state = self.state_async().await;

        match current_state {
            CircuitState::Open { until } => {
                if Instant::now() < until {
                    return Err(AosError::CircuitBreakerOpen {
                        service: self.name.clone(),
                    });
                }
                // Should have transitioned to HalfOpen above, but handle just in case
                return Err(AosError::CircuitBreakerOpen {
                    service: self.name.clone(),
                });
            }
            CircuitState::HalfOpen => {
                // Atomically check and increment concurrent requests
                let mut current_requests = self.half_open_requests.load(Ordering::Acquire);
                loop {
                    if current_requests >= self.config.half_open_max_requests {
                        return Err(AosError::CircuitBreakerHalfOpen {
                            service: self.name.clone(),
                        });
                    }

                    match self.half_open_requests.compare_exchange(
                        current_requests,
                        current_requests + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    ) {
                        Ok(_) => break, // Successfully incremented
                        Err(actual) => {
                            current_requests = actual; // Retry with updated value
                        }
                    }
                }
            }
            CircuitState::Closed => {
                // Normal operation
            }
        }

        // Execute the operation
        let result = operation.await;

        match result {
            Ok(value) => {
                self.on_success().await;
                Ok(value)
            }
            Err(e) => {
                self.on_failure().await;
                Err(e)
            }
        }
    }

    fn state(&self) -> CircuitState {
        // For sync access, try to get fresh state from lock first
        if let Ok(state_guard) = self.state.try_lock() {
            let current_state = *state_guard;

            // Check if we should transition from Open to HalfOpen
            match current_state {
                CircuitState::Open { until } => {
                    if Instant::now() >= until {
                        CircuitState::HalfOpen
                    } else {
                        CircuitState::Open { until }
                    }
                }
                state => state,
            }
        } else {
            // Lock is contended - use cached state instead of defaulting to Closed.
            // This ensures we don't bypass protection when circuit is actually Open.
            // Cache is updated by transition_to() and state_async() atomically.
            self.decode_cached_state()
        }
    }

    fn metrics(&self) -> CircuitBreakerMetrics {
        CircuitBreakerMetrics {
            state: self.state(),
            requests_total: self.requests_total.load(Ordering::Relaxed),
            successes_total: self.successes_total.load(Ordering::Relaxed),
            failures_total: self.failures_total.load(Ordering::Relaxed),
            opens_total: self.opens_total.load(Ordering::Relaxed),
            closes_total: self.closes_total.load(Ordering::Relaxed),
            half_opens_total: self.half_opens_total.load(Ordering::Relaxed),
            last_state_change: self.last_state_change.load(Ordering::Relaxed),
        }
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Thread-safe circuit breaker wrapper for sharing across threads
pub type SharedCircuitBreaker = Arc<dyn CircuitBreaker>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_circuit_breaker_closed_state() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 1000,
            half_open_max_requests: 5,
        };

        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Test successful operations in closed state
        for _ in 0..5 {
            let result = breaker.call(async { Ok("success") }).await;
            assert!(result.is_ok());
        }

        assert_eq!(breaker.state(), CircuitState::Closed);
        let metrics = breaker.metrics();
        assert_eq!(metrics.requests_total, 5);
        assert_eq!(metrics.successes_total, 5);
        assert_eq!(metrics.failures_total, 0);
    }

    #[tokio::test]
    async fn test_circuit_breaker_open_on_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 1000,
            half_open_max_requests: 5,
        };

        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Cause failures to open circuit
        for _ in 0..3 {
            let result: Result<()> = breaker
                .call(async { Err(AosError::Unavailable("test failure".to_string())) })
                .await;
            assert!(result.is_err());
        }

        // Check that circuit is open
        match breaker.state() {
            CircuitState::Open { .. } => {}
            _ => panic!("Expected circuit to be open"),
        }

        // Requests should be rejected
        let result = breaker.call(async { Ok("should fail") }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 100, // Short timeout for test
            half_open_max_requests: 5,
        };

        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Open the circuit
        for _ in 0..3 {
            let _result: Result<()> = breaker
                .call(async { Err(AosError::Unavailable("test failure".to_string())) })
                .await;
        }

        // Wait for timeout
        sleep(Duration::from_millis(150)).await;

        // Should be in half-open state
        let state = breaker.state_async().await;
        match state {
            CircuitState::HalfOpen => {}
            _ => panic!("Expected circuit to be half-open, got {:?}", state),
        }

        // Successful operations should close the circuit
        for _ in 0..2 {
            let result = breaker.call(async { Ok("success") }).await;
            assert!(result.is_ok());
        }

        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            timeout_ms: 100,
            half_open_max_requests: 5,
        };

        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Open the circuit
        for _ in 0..3 {
            let _result: Result<()> = breaker
                .call(async { Err(AosError::Unavailable("test failure".to_string())) })
                .await;
        }

        // Wait for timeout
        sleep(Duration::from_millis(150)).await;

        // Failure in half-open should immediately reopen
        let result: Result<()> = breaker
            .call(async { Err(AosError::Unavailable("half-open failure".to_string())) })
            .await;
        assert!(result.is_err());

        match breaker.state() {
            CircuitState::Open { .. } => {}
            _ => panic!("Expected circuit to reopen after half-open failure"),
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_concurrent_limit() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout_ms: 100,
            half_open_max_requests: 2, // Very low limit for testing
        };

        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Open the circuit
        for _ in 0..2 {
            let _result: Result<()> = breaker
                .call(async { Err(AosError::Unavailable("test failure".to_string())) })
                .await;
        }

        // Wait for timeout
        sleep(Duration::from_millis(150)).await;

        // Should be in half-open state
        match breaker.state_async().await {
            CircuitState::HalfOpen => {}
            _ => panic!("Expected circuit to be half-open"),
        }

        // First request should succeed (starts pending)
        let handle1 = tokio::spawn({
            let breaker = Arc::clone(&breaker);
            async move {
                breaker
                    .call(async {
                        // Simulate long-running operation
                        sleep(Duration::from_millis(50)).await;
                        Ok("success1")
                    })
                    .await
            }
        });

        // Second request should succeed (starts pending)
        let handle2 = tokio::spawn({
            let breaker = Arc::clone(&breaker);
            async move {
                breaker
                    .call(async {
                        sleep(Duration::from_millis(50)).await;
                        Ok("success2")
                    })
                    .await
            }
        });

        // Small delay to ensure first two requests have started and incremented counter
        sleep(Duration::from_millis(10)).await;

        // Third request should be rejected due to concurrent limit
        let result3 = breaker.call(async { Ok("should be rejected") }).await;
        assert!(result3.is_err());
        assert!(matches!(
            result3.unwrap_err(),
            AosError::CircuitBreakerHalfOpen { .. }
        ));

        // Wait for the first two requests to complete
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        // After two successes, circuit should be closed
        assert_eq!(breaker.state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_concurrent_state_access() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(StandardCircuitBreaker::new(
            "concurrent_test".to_string(),
            config,
        ));

        // Spawn multiple tasks that will try to access state concurrently
        let mut handles = vec![];

        for _ in 0..10 {
            let breaker_clone = breaker.clone();
            let handle = tokio::spawn(async move {
                // This should not panic even when lock is contended
                let _state_result = breaker_clone.state();
                // Also test metrics access which internally calls state()
                let _metrics_result = breaker_clone.metrics();
            });
            handles.push(handle);
        }

        // Also do some operations that will hold the lock
        for _ in 0..5 {
            let breaker_clone = breaker.clone();
            let handle = tokio::spawn(async move {
                let _ = breaker_clone.call(async { Ok("success") }).await;
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify the circuit breaker is still functional
        let final_state = breaker.state();
        assert!(matches!(final_state, CircuitState::Closed));
    }

    #[tokio::test]
    async fn test_circuit_breaker_state_unchecked_fallback() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Hold the lock to force contention
        let lock_guard = breaker.state.lock().await;

        // Spawn a task that tries to get state while lock is held
        let breaker_clone = breaker.clone();
        let handle = tokio::spawn(async move {
            // state() should return cached state when lock is contended
            // Since circuit was initialized as Closed, cached state is also Closed
            let state = breaker_clone.state();
            assert_eq!(state, CircuitState::Closed);

            // metrics() should work fine (state() is infallible now)
            let metrics = breaker_clone.metrics();
            assert_eq!(metrics.state, CircuitState::Closed);
        });

        // Wait for the spawned task
        handle.await.unwrap();

        // Release the lock
        drop(lock_guard);
    }

    #[tokio::test]
    async fn test_circuit_breaker_lock_contention_fallback() {
        let config = CircuitBreakerConfig::default();
        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Test contended lock - state() should return cached state (not hardcoded Closed)
        let lock_guard = breaker.state.lock().await;

        let breaker_clone = breaker.clone();
        let handle = tokio::spawn(async move {
            let state = breaker_clone.state();
            // Returns cached state (Closed, since that's the initial state)
            assert_eq!(state, CircuitState::Closed);
        });

        handle.await.unwrap();
        drop(lock_guard);

        // Verify we can get real state after lock is released
        let final_state = breaker.state();
        assert_eq!(final_state, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_cached_state_preserves_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            timeout_ms: 60000, // Long timeout to stay Open
            half_open_max_requests: 5,
        };

        let breaker = Arc::new(StandardCircuitBreaker::new("test".to_string(), config));

        // Open the circuit
        for _ in 0..2 {
            let _result: Result<()> = breaker
                .call(async { Err(AosError::Unavailable("test failure".to_string())) })
                .await;
        }

        // Verify it's Open
        match breaker.state() {
            CircuitState::Open { .. } => {}
            state => panic!("Expected Open, got {:?}", state),
        }

        // Hold the lock to force contention
        let lock_guard = breaker.state.lock().await;

        // Check that cached state still returns Open (not Closed!)
        let breaker_clone = breaker.clone();
        let handle = tokio::spawn(async move {
            let state = breaker_clone.state();
            // Critical: should return Open from cache, not Closed!
            // This ensures we don't bypass protection when lock is contended
            match state {
                CircuitState::Open { .. } => {} // Expected
                state => panic!("Expected cached Open state, got {:?}", state),
            }
        });

        handle.await.unwrap();
        drop(lock_guard);
    }
}
