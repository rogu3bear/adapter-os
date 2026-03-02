//! Standardized retry policy implementation
//!
//! Provides a unified retry policy system with exponential backoff, jitter,
//! circuit breaker integration, and retry budget management.

use crate::circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, StandardCircuitBreaker};
use crate::{AosError, Result};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Retry policy configuration
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts (excluding initial attempt)
    pub max_attempts: u32,
    /// Base delay between retry attempts
    pub base_delay: Duration,
    /// Maximum delay between retry attempts
    pub max_delay: Duration,
    /// Backoff factor for exponential backoff
    pub backoff_factor: f64,
    /// Jitter factor (0.0 = no jitter, 1.0 = full jitter)
    pub jitter: bool,
    /// Use deterministic jitter (for deterministic contexts like inference/training)
    pub deterministic_jitter: bool,
    /// Circuit breaker configuration
    pub circuit_breaker: Option<CircuitBreakerConfig>,
    /// Retry budget configuration
    pub budget: Option<RetryBudgetConfig>,
    /// Service type for metrics categorization
    pub service_type: String,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
            jitter: true,
            deterministic_jitter: false,
            circuit_breaker: Some(CircuitBreakerConfig::default()),
            budget: Some(RetryBudgetConfig::default()),
            service_type: "default".to_string(),
        }
    }
}

impl RetryPolicy {
    /// Create a fast retry policy for quick operations
    pub fn fast(service_type: &str) -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(50),
            max_delay: Duration::from_millis(500),
            backoff_factor: 2.0,
            jitter: true,
            deterministic_jitter: false,
            circuit_breaker: Some(CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 2,
                timeout_ms: 30000,
                half_open_max_requests: 2,
            }),
            budget: Some(RetryBudgetConfig::default()),
            service_type: service_type.to_string(),
        }
    }

    /// Create a slow retry policy for expensive operations
    pub fn slow(service_type: &str) -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            backoff_factor: 1.5,
            jitter: true,
            deterministic_jitter: false,
            circuit_breaker: Some(CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 2,
                timeout_ms: 120000,
                half_open_max_requests: 1,
            }),
            budget: Some(RetryBudgetConfig::default()),
            service_type: service_type.to_string(),
        }
    }

    /// Create a database retry policy
    pub fn database(service_type: &str) -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(10),
            backoff_factor: 1.5,
            jitter: true,
            deterministic_jitter: false,
            circuit_breaker: Some(CircuitBreakerConfig {
                failure_threshold: 5,
                success_threshold: 3,
                timeout_ms: 60000,
                half_open_max_requests: 2,
            }),
            budget: Some(RetryBudgetConfig {
                max_concurrent_retries: 10,
                max_retry_rate_per_second: 20.0,
                budget_window: Duration::from_secs(60),
                max_budget_tokens: 100,
            }),
            service_type: service_type.to_string(),
        }
    }

    /// Create a network retry policy
    pub fn network(service_type: &str) -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
            jitter: true,
            deterministic_jitter: false,
            circuit_breaker: Some(CircuitBreakerConfig {
                failure_threshold: 3,
                success_threshold: 2,
                timeout_ms: 45000,
                half_open_max_requests: 3,
            }),
            budget: Some(RetryBudgetConfig::default()),
            service_type: service_type.to_string(),
        }
    }

    /// Create a policy without circuit breaker
    pub fn without_circuit_breaker(service_type: &str) -> Self {
        Self {
            circuit_breaker: None,
            service_type: service_type.to_string(),
            ..Default::default()
        }
    }

    /// Create a policy without retry budget
    pub fn without_budget(service_type: &str) -> Self {
        Self {
            budget: None,
            service_type: service_type.to_string(),
            ..Default::default()
        }
    }
}

/// Retry budget configuration to prevent resource exhaustion
#[derive(Debug, Clone)]
pub struct RetryBudgetConfig {
    /// Maximum number of concurrent retries
    pub max_concurrent_retries: usize,
    /// Maximum retry rate per second
    pub max_retry_rate_per_second: f64,
    /// Time window for rate limiting
    pub budget_window: Duration,
    /// Maximum budget tokens
    pub max_budget_tokens: usize,
}

impl Default for RetryBudgetConfig {
    fn default() -> Self {
        Self {
            max_concurrent_retries: 50,
            max_retry_rate_per_second: 100.0,
            budget_window: Duration::from_secs(60),
            max_budget_tokens: 500,
        }
    }
}

/// Retry budget manager
#[derive(Clone)]
pub struct RetryBudget {
    config: RetryBudgetConfig,
    active_retries: Arc<std::sync::atomic::AtomicUsize>,
    rate_limiter: Arc<Mutex<RateLimiter>>,
}

impl RetryBudget {
    fn new(config: RetryBudgetConfig) -> Self {
        let rate_limiter = Arc::new(Mutex::new(RateLimiter::new(
            config.max_retry_rate_per_second,
            config.budget_window,
        )));
        Self {
            config,
            active_retries: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            rate_limiter,
        }
    }

    /// Check if retry is allowed by budget
    async fn check_budget(&self) -> Result<()> {
        // Check concurrent retry limit
        let active = self
            .active_retries
            .load(std::sync::atomic::Ordering::SeqCst);
        if active >= self.config.max_concurrent_retries {
            return Err(AosError::ResourceExhaustion(
                "Retry budget exceeded: too many concurrent retries".to_string(),
            ));
        }

        // Check rate limit
        let mut rate_limiter = self.rate_limiter.lock().await;
        if !rate_limiter.allow()? {
            return Err(AosError::ResourceExhaustion(
                "Retry budget exceeded: rate limit".to_string(),
            ));
        }

        Ok(())
    }

    /// Acquire budget for a retry operation
    async fn acquire(&self) -> Result<RetryBudgetGuard> {
        self.check_budget().await?;
        self.active_retries
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(RetryBudgetGuard {
            budget: self.clone(),
        })
    }
}

/// Guard that releases budget when dropped
pub struct RetryBudgetGuard {
    budget: RetryBudget,
}

impl Drop for RetryBudgetGuard {
    fn drop(&mut self) {
        self.budget
            .active_retries
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Simple rate limiter implementation
struct RateLimiter {
    max_rate: f64,
    window: Duration,
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    fn new(max_rate: f64, window: Duration) -> Self {
        Self {
            max_rate,
            window,
            tokens: max_rate,
            last_refill: Instant::now(),
        }
    }

    fn allow(&mut self) -> Result<bool> {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let refill_amount = (elapsed.as_secs_f64() / self.window.as_secs_f64()) * self.max_rate;

        self.tokens = (self.tokens + refill_amount).min(self.max_rate);
        self.last_refill = now;
    }
}

/// Retry manager with standardized retry logic
#[derive(Clone)]
pub struct RetryManager {
    circuit_breaker: Option<Arc<StandardCircuitBreaker>>,
    budget: Option<RetryBudget>,
    metrics: Option<Arc<dyn RetryMetricsReporter + Send + Sync>>,
}

impl RetryManager {
    /// Create a new retry manager
    pub fn new() -> Self {
        Self {
            circuit_breaker: None,
            budget: None,
            metrics: None,
        }
    }
}

impl Default for RetryManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RetryManager {
    /// Create retry manager with circuit breaker
    pub fn with_circuit_breaker(config: CircuitBreakerConfig) -> Self {
        Self {
            circuit_breaker: Some(Arc::new(StandardCircuitBreaker::new(
                "retry-manager".to_string(),
                config,
            ))),
            budget: None,
            metrics: None,
        }
    }

    /// Create retry manager with budget
    pub fn with_budget(config: RetryBudgetConfig) -> Self {
        Self {
            circuit_breaker: None,
            budget: Some(RetryBudget::new(config)),
            metrics: None,
        }
    }

    /// Create retry manager with metrics
    pub fn with_metrics(metrics: Arc<dyn RetryMetricsReporter + Send + Sync>) -> Self {
        Self {
            circuit_breaker: None,
            budget: None,
            metrics: Some(metrics),
        }
    }

    /// Execute an operation with retry policy
    pub async fn execute_with_policy<F, T>(&self, policy: &RetryPolicy, operation: F) -> Result<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>
            + Send
            + Sync,
        T: Send,
    {
        let start_time = Instant::now();

        // Record attempt start
        if let Some(metrics) = &self.metrics {
            metrics.record_retry_start(&policy.service_type);
        }

        // Check budget if configured
        let budget_guard = if let Some(_budget_config) = &policy.budget {
            let manager_budget = self.budget.as_ref().ok_or_else(|| {
                AosError::Config("Retry budget requested but not configured".to_string())
            })?;
            Some(manager_budget.acquire().await?)
        } else {
            None
        };

        // Ensure budget guard is used (held for duration of retry operation)
        let _ = &budget_guard;

        // Execute with circuit breaker if configured, holding budget guard throughout
        let result = if let Some(cb) = &self.circuit_breaker {
            self.execute_with_circuit_breaker(cb, policy, operation, budget_guard)
                .await
        } else {
            self.execute_with_retry(policy, operation, budget_guard)
                .await
        };

        // Record metrics
        let duration = start_time.elapsed();
        if let Some(metrics) = &self.metrics {
            match &result {
                Ok(_) => metrics.record_retry_success(&policy.service_type, duration),
                Err(_) => metrics.record_retry_failure(&policy.service_type, duration),
            }
        }

        // Budget guard is automatically dropped here if it wasn't passed to retry methods,
        // releasing the budget

        result
    }

    /// Execute operation with circuit breaker
    async fn execute_with_circuit_breaker<F, T>(
        &self,
        circuit_breaker: &Arc<StandardCircuitBreaker>,
        policy: &RetryPolicy,
        operation: F,
        budget_guard: Option<RetryBudgetGuard>,
    ) -> Result<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>
            + Send
            + Sync,
        T: Send,
    {
        // Hold the budget guard for the entire circuit breaker operation duration
        let _budget_guard = budget_guard;
        let retry_future = self.execute_with_retry(policy, operation, _budget_guard);
        let cb_result = circuit_breaker.call(retry_future).await;

        cb_result
    }

    /// Generate deterministic or random jitter based on policy
    fn generate_jitter(policy: &RetryPolicy, delay: Duration, attempt: u32) -> Duration {
        let jitter_range = (delay.as_millis() as f64 * 0.1) as u64; // 10% jitter
        if jitter_range == 0 {
            return Duration::ZERO;
        }

        let jitter_amount = if policy.deterministic_jitter {
            // Use HKDF-based deterministic jitter
            use hkdf::Hkdf;
            use sha2::Sha256;
            let label = format!("retry_jitter:{}:{}", policy.service_type, attempt);
            let hk = Hkdf::<Sha256>::new(Some(label.as_bytes()), b"adapteros-retry");
            let mut seed_bytes = [0u8; 8];
            // HKDF-SHA256 can expand to 8160 bytes (255 * 32); 8 bytes is safe
            hk.expand(&[], &mut seed_bytes)
                .expect("HKDF expand for 8 bytes always succeeds");
            u64::from_le_bytes(seed_bytes) % jitter_range
        } else {
            fastrand::Rng::new().u64(0..jitter_range)
        };

        Duration::from_millis(jitter_amount)
    }

    /// Execute operation with retry logic
    async fn execute_with_retry<F, T>(
        &self,
        policy: &RetryPolicy,
        operation: F,
        budget_guard: Option<RetryBudgetGuard>,
    ) -> Result<T>
    where
        F: Fn() -> std::pin::Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>
            + Send
            + Sync,
        T: Send,
    {
        // Hold the budget guard for the entire retry operation duration
        let _budget_guard = budget_guard;
        let mut attempt = 0;
        let mut delay = policy.base_delay;

        loop {
            attempt += 1;

            match operation().await {
                Ok(result) => {
                    // Record success on retry
                    if attempt > 1 {
                        if let Some(metrics) = &self.metrics {
                            metrics.record_retry_success(
                                &policy.service_type,
                                Duration::from_millis(0),
                            );
                        }
                    }
                    return Ok(result);
                }
                Err(err) => {
                    // Check if we should retry this error
                    if attempt > policy.max_attempts || !self.should_retry(&err) {
                        return Err(err);
                    }

                    // Calculate next delay
                    if attempt <= policy.max_attempts {
                        // Record retry attempt
                        if let Some(metrics) = &self.metrics {
                            metrics.record_retry_attempt(&policy.service_type, attempt);
                        }

                        // Apply jitter if enabled
                        if policy.jitter {
                            delay += Self::generate_jitter(policy, delay, attempt);
                        }

                        // Apply exponential backoff
                        delay = std::cmp::min(
                            Duration::from_millis(
                                (delay.as_millis() as f64 * policy.backoff_factor) as u64,
                            ),
                            policy.max_delay,
                        );

                        // Sleep before retry
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
    }

    /// Determine if an error should be retried
    fn should_retry(&self, error: &AosError) -> bool {
        match error {
            // Explicit network/timeout classes
            AosError::Network(_) => true,
            AosError::Timeout { .. } => true,
            // Retry IO errors that look like network/connection issues
            AosError::Io(err) => {
                let err_lower = err.to_lowercase();
                err_lower.contains("connection")
                    || err_lower.contains("timeout")
                    || err_lower.contains("network")
            }
            // Don't retry other errors
            _ => false,
        }
    }

    /// Get circuit breaker metrics
    pub fn circuit_breaker_metrics(&self) -> Option<crate::circuit_breaker::CircuitBreakerMetrics> {
        Some(self.circuit_breaker.as_ref()?.metrics())
    }
}

/// Metrics reporter trait for retry operations
pub trait RetryMetricsReporter {
    /// Record that a retry operation started
    fn record_retry_start(&self, service_type: &str);

    /// Record a retry attempt
    fn record_retry_attempt(&self, service_type: &str, attempt: u32);

    /// Record a successful retry
    fn record_retry_success(&self, service_type: &str, duration: Duration);

    /// Record a failed retry
    fn record_retry_failure(&self, service_type: &str, duration: Duration);
}

/// No-op metrics reporter for when metrics are not needed
pub struct NoOpRetryMetrics;

impl RetryMetricsReporter for NoOpRetryMetrics {
    fn record_retry_start(&self, _service_type: &str) {}
    fn record_retry_attempt(&self, _service_type: &str, _attempt: u32) {}
    fn record_retry_success(&self, _service_type: &str, _duration: Duration) {}
    fn record_retry_failure(&self, _service_type: &str, _duration: Duration) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

    #[derive(Clone)]
    struct TestMetrics {
        starts: Arc<AtomicU32>,
        attempts: Arc<AtomicU32>,
        successes: Arc<AtomicU32>,
        failures: Arc<AtomicU32>,
    }

    impl TestMetrics {
        fn new() -> Self {
            Self {
                starts: Arc::new(AtomicU32::new(0)),
                attempts: Arc::new(AtomicU32::new(0)),
                successes: Arc::new(AtomicU32::new(0)),
                failures: Arc::new(AtomicU32::new(0)),
            }
        }
    }

    impl RetryMetricsReporter for TestMetrics {
        fn record_retry_start(&self, _service_type: &str) {
            self.starts.fetch_add(1, Ordering::SeqCst);
        }
        fn record_retry_attempt(&self, _service_type: &str, _attempt: u32) {
            self.attempts.fetch_add(1, Ordering::SeqCst);
        }
        fn record_retry_success(&self, _service_type: &str, _duration: Duration) {
            self.successes.fetch_add(1, Ordering::SeqCst);
        }
        fn record_retry_failure(&self, _service_type: &str, _duration: Duration) {
            self.failures.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn test_retry_policy_success_first_attempt() {
        let policy = RetryPolicy {
            budget: None, // Disable budget for this test
            ..RetryPolicy::fast("test")
        };
        let manager = RetryManager::new();
        let attempts = Arc::new(AtomicU32::new(0));

        let result = manager
            .execute_with_policy(&policy, || {
                let attempts = attempts.clone();
                Box::pin(async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, AosError>("success")
                })
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_policy_with_retries() {
        let policy = RetryPolicy {
            max_attempts: 2,
            base_delay: Duration::from_millis(10),
            budget: None, // Disable budget for this test
            ..RetryPolicy::fast("test")
        };
        let manager = RetryManager::new();
        let attempts = Arc::new(AtomicU32::new(0));

        let result = manager
            .execute_with_policy(&policy, || {
                let attempts = attempts.clone();
                Box::pin(async move {
                    let current = attempts.fetch_add(1, Ordering::SeqCst);
                    if current < 2 {
                        Err(AosError::Network("temporary failure".to_string()))
                    } else {
                        Ok("success")
                    }
                })
            })
            .await;

        assert!(result.is_ok());
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // initial + 2 retries
    }

    #[tokio::test]
    async fn test_retry_policy_exhausts_attempts() {
        let policy = RetryPolicy {
            max_attempts: 1,
            base_delay: Duration::from_millis(10),
            budget: None, // Disable budget for this test
            ..RetryPolicy::fast("test")
        };
        let manager = RetryManager::new();
        let attempts = Arc::new(AtomicU32::new(0));

        let result = manager
            .execute_with_policy(&policy, || {
                let attempts = attempts.clone();
                Box::pin(async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>(AosError::Network("persistent failure".to_string()))
                })
            })
            .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 2); // initial + 1 retry
    }

    #[tokio::test]
    async fn test_retry_policy_with_metrics() {
        let policy = RetryPolicy {
            budget: None, // Disable budget for this test
            ..RetryPolicy::fast("test")
        };
        let metrics = Arc::new(TestMetrics::new());
        let manager = RetryManager::with_metrics(metrics.clone());
        let attempts = Arc::new(AtomicU32::new(0));

        let _result = manager
            .execute_with_policy(&policy, || {
                let attempts = attempts.clone();
                Box::pin(async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Err::<(), _>(AosError::Network("failure".to_string()))
                })
            })
            .await;

        // Should have recorded start and attempts
        assert_eq!(metrics.starts.load(Ordering::SeqCst), 1);
        assert_eq!(metrics.attempts.load(Ordering::SeqCst), 3); // 3 retry attempts
        assert_eq!(metrics.failures.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_should_retry() {
        let manager = RetryManager::new();

        // Should retry network errors
        assert!(manager.should_retry(&AosError::Network("connection failed".to_string())));

        // Should retry timeout errors
        assert!(manager.should_retry(&AosError::Timeout {
            duration: Duration::from_secs(5)
        }));

        // Should retry connection-related IO errors
        assert!(manager.should_retry(&AosError::Io(
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection refused")
                .to_string()
        )));

        // Should not retry other errors
        assert!(!manager.should_retry(&AosError::Validation("invalid input".to_string())));
        assert!(!manager.should_retry(&AosError::Config("bad config".to_string())));
    }

    #[tokio::test]
    async fn test_retry_budget_integration() {
        let budget_config = RetryBudgetConfig {
            max_concurrent_retries: 2,
            max_retry_rate_per_second: 10.0,
            budget_window: Duration::from_secs(1),
            max_budget_tokens: 10,
        };
        let manager = RetryManager::with_budget(budget_config.clone());
        let policy = RetryPolicy {
            max_attempts: 3,
            base_delay: Duration::from_millis(10),
            budget: Some(budget_config),
            ..RetryPolicy::fast("test")
        };

        // Track concurrent operations
        let active_operations = Arc::new(AtomicUsize::new(0));
        let max_concurrent_seen = Arc::new(AtomicUsize::new(0));

        // Create multiple concurrent retry operations
        let mut handles = vec![];

        for i in 0..5 {
            let manager = manager.clone();
            let policy = policy.clone();
            let active_operations = active_operations.clone();
            let max_concurrent_seen = max_concurrent_seen.clone();

            let handle = tokio::spawn(async move {
                let result = manager
                    .execute_with_policy(&policy, || {
                        let active_operations = active_operations.clone();
                        let max_concurrent_seen = max_concurrent_seen.clone();
                        Box::pin(async move {
                            let current = active_operations.fetch_add(1, Ordering::SeqCst) + 1;
                            max_concurrent_seen.fetch_max(current, Ordering::SeqCst);

                            // Simulate some work that always fails to trigger retries
                            tokio::time::sleep(Duration::from_millis(5)).await;

                            active_operations.fetch_sub(1, Ordering::SeqCst);
                            Err::<(), _>(AosError::Network(format!("operation {} failed", i)))
                        })
                    })
                    .await;

                result
            });

            handles.push(handle);
        }

        // Wait for all operations to complete
        let mut success_count = 0;
        let mut budget_exhaustion_count = 0;

        for handle in handles {
            match handle.await.unwrap() {
                Ok(_) => success_count += 1,
                Err(AosError::ResourceExhaustion(msg)) if msg.contains("budget") => {
                    budget_exhaustion_count += 1;
                }
                Err(_) => {} // Other errors are expected
            }
        }

        // Verify budget constraints were enforced
        assert!(
            budget_exhaustion_count > 0,
            "Budget should have been exhausted for some operations"
        );
        assert!(
            success_count == 0,
            "No operations should succeed (they all fail)"
        );
        assert!(
            max_concurrent_seen.load(Ordering::SeqCst) <= 2,
            "Should not exceed max_concurrent_retries limit"
        );
    }

    #[tokio::test]
    async fn test_retry_budget_rate_limiting() {
        let budget_config = RetryBudgetConfig {
            max_concurrent_retries: 10,
            max_retry_rate_per_second: 2.0, // Very low rate limit
            budget_window: Duration::from_secs(1),
            max_budget_tokens: 5,
        };
        let manager = RetryManager::with_budget(budget_config.clone());
        let policy = RetryPolicy {
            max_attempts: 1,
            base_delay: Duration::from_millis(1),
            budget: Some(budget_config),
            ..RetryPolicy::fast("test")
        };

        let start_time = Instant::now();
        let mut handles = vec![];

        // Try to execute many operations in parallel
        for i in 0..10 {
            let manager = manager.clone();
            let policy = policy.clone();

            let handle = tokio::spawn(async move {
                manager
                    .execute_with_policy(&policy, || {
                        Box::pin(async move {
                            // Always fail to avoid retries consuming budget
                            Err::<(), _>(AosError::Network(format!("operation {} failed", i)))
                        })
                    })
                    .await
            });

            handles.push(handle);
        }

        // Wait for all operations to complete
        let mut results = vec![];
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        let _elapsed = start_time.elapsed();

        // Count budget exhaustion errors
        let budget_errors = results.iter()
            .filter(|r| matches!(r, Err(AosError::ResourceExhaustion(msg)) if msg.contains("rate limit")))
            .count();

        // With such a low rate limit (2 per second) and 10 operations,
        // we should see rate limiting kick in
        // Note: The rate limiting may not cause delays if operations complete very fast
        // The important thing is that some operations are rejected due to rate limits
        assert!(
            budget_errors > 0,
            "Rate limiting should have occurred: got {} budget errors",
            budget_errors
        );
    }
}
