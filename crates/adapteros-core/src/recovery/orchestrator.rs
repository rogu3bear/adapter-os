//! Recovery orchestrator implementation
//!
//! Provides a unified handler that coordinates retry, circuit breaker,
//! and fallback mechanisms for resilient operation execution.

use super::classifier::RecoveryClassifier;
use super::config::{FallbackConfig, LogLevel, RecoveryConfig, TelemetryConfig};
use super::outcome::{RecoveryError, RecoveryOutcome, RecoveryStats};
use crate::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitState, StandardCircuitBreaker,
};
use crate::circuit_breaker_registry::CircuitBreakerRegistry;
use crate::retry_policy::{RetryBudgetConfig, RetryMetricsReporter, RetryPolicy};
use crate::AosError;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Unified recovery orchestrator
///
/// Coordinates retry, circuit breaker, budget, and fallback mechanisms
/// to provide resilient operation execution.
///
/// # Example
///
/// ```rust,ignore
/// use adapteros_core::recovery::{RecoveryOrchestrator, RecoveryConfig};
///
/// let orchestrator = RecoveryOrchestrator::new(RecoveryConfig::database("user-db"));
///
/// let outcome = orchestrator.execute(|| {
///     Box::pin(async { db.query("SELECT * FROM users").await })
/// }).await;
///
/// if outcome.is_ok() {
///     println!("Query succeeded after {} attempts", outcome.stats.retry_attempts);
/// }
/// ```
pub struct RecoveryOrchestrator {
    /// Configuration
    config: RecoveryConfig,

    /// Circuit breaker instance (local or from registry)
    circuit_breaker: Option<Arc<StandardCircuitBreaker>>,

    /// Retry budget (internal implementation)
    budget: Option<SimpleBudget>,

    /// Metrics reporter
    metrics: Option<Arc<dyn RetryMetricsReporter + Send + Sync>>,
}

/// Simple budget implementation for the orchestrator
struct SimpleBudget {
    max_concurrent: usize,
    active: AtomicUsize,
}

impl SimpleBudget {
    fn new(config: &RetryBudgetConfig) -> Self {
        Self {
            max_concurrent: config.max_concurrent_retries,
            active: AtomicUsize::new(0),
        }
    }

    fn try_acquire(&self) -> Option<SimpleBudgetGuard<'_>> {
        let current = self.active.load(Ordering::Acquire);
        if current >= self.max_concurrent {
            return None;
        }

        // Try to atomically increment
        match self.active.compare_exchange(
            current,
            current + 1,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Some(SimpleBudgetGuard { budget: self }),
            Err(_) => {
                // Race condition, try again or fail
                None
            }
        }
    }
}

struct SimpleBudgetGuard<'a> {
    budget: &'a SimpleBudget,
}

impl<'a> Drop for SimpleBudgetGuard<'a> {
    fn drop(&mut self) {
        self.budget.active.fetch_sub(1, Ordering::AcqRel);
    }
}

impl RecoveryOrchestrator {
    /// Create a new orchestrator with the given configuration
    pub fn new(config: RecoveryConfig) -> Self {
        let circuit_breaker = if config.use_global_circuit_breaker {
            Some(
                CircuitBreakerRegistry::global().get_or_create(
                    &config.service_name,
                    config
                        .retry_policy
                        .circuit_breaker
                        .clone()
                        .unwrap_or_default(),
                ),
            )
        } else if let Some(cb_config) = &config.retry_policy.circuit_breaker {
            Some(Arc::new(StandardCircuitBreaker::new(
                config.service_name.clone(),
                cb_config.clone(),
            )))
        } else {
            None
        };

        let budget = config.retry_policy.budget.as_ref().map(SimpleBudget::new);

        Self {
            config,
            circuit_breaker,
            budget,
            metrics: None,
        }
    }

    /// Create orchestrator with a metrics reporter
    pub fn with_metrics(mut self, metrics: Arc<dyn RetryMetricsReporter + Send + Sync>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Get the service name
    pub fn service_name(&self) -> &str {
        &self.config.service_name
    }

    /// Get the current circuit breaker state
    pub fn circuit_state(&self) -> Option<CircuitState> {
        self.circuit_breaker.as_ref().map(|cb| cb.state())
    }

    /// Execute an async operation with full recovery pipeline
    ///
    /// Pipeline order:
    /// 1. Check budget
    /// 2. Check circuit breaker
    /// 3. Execute with retry loop
    /// 4. Update circuit breaker
    /// 5. Return outcome
    pub async fn execute<F, Fut, T>(&self, operation: F) -> RecoveryOutcome<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T, AosError>> + Send,
        T: Send,
    {
        let start_time = Instant::now();
        let mut stats = RecoveryStats::new();

        // Record start if metrics enabled
        if let Some(metrics) = &self.metrics {
            metrics.record_retry_start(&self.config.service_name);
        }

        // Step 1: Check budget
        let _budget_guard = if let Some(ref budget) = self.budget {
            match budget.try_acquire() {
                Some(guard) => Some(guard),
                None => {
                    stats.total_duration = start_time.elapsed();
                    return RecoveryOutcome::failure(
                        RecoveryError::BudgetExhausted {
                            reason: "Too many concurrent retries".to_string(),
                        },
                        stats,
                    );
                }
            }
        } else {
            None
        };

        // Step 2: Check circuit breaker
        stats.circuit_breaker_checked = self.circuit_breaker.is_some();
        if let Some(ref cb) = self.circuit_breaker {
            let state = cb.state();
            stats.circuit_state = Some(state);

            match state {
                CircuitState::Open { .. } => {
                    self.log_event(LogLevel::Warn, "Circuit breaker is open, rejecting request");
                    stats.total_duration = start_time.elapsed();
                    return RecoveryOutcome::failure(
                        RecoveryError::CircuitOpen {
                            service: self.config.service_name.clone(),
                        },
                        stats,
                    );
                }
                CircuitState::HalfOpen => {
                    self.log_event(LogLevel::Debug, "Circuit breaker is half-open, probing");
                }
                CircuitState::Closed => {}
            }
        }

        // Step 3: Execute with retry loop
        let result = self.execute_with_retry(&operation, &mut stats).await;

        stats.total_duration = start_time.elapsed();

        // Record final metrics
        if let Some(metrics) = &self.metrics {
            match &result {
                Ok(_) => {
                    metrics.record_retry_success(&self.config.service_name, stats.total_duration)
                }
                Err(_) => {
                    metrics.record_retry_failure(&self.config.service_name, stats.total_duration)
                }
            }
        }

        RecoveryOutcome { result, stats }
    }

    /// Execute with a fallback function
    ///
    /// The fallback is invoked when:
    /// - All retries are exhausted (if `on_exhausted` is true)
    /// - Circuit breaker is open (if `on_circuit_open` is true)
    /// - Budget is exhausted (if `on_budget_exhausted` is true)
    pub async fn execute_with_fallback<F, Fut, FB, FutB, T>(
        &self,
        operation: F,
        fallback: FB,
    ) -> RecoveryOutcome<T>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T, AosError>> + Send,
        FB: FnOnce(&RecoveryError) -> FutB + Send,
        FutB: Future<Output = Result<T, AosError>> + Send,
        T: Send,
    {
        let mut outcome = self.execute(operation).await;

        if let Err(ref recovery_err) = outcome.result {
            let fallback_config = self.config.fallback.as_ref();

            let should_fallback = match (fallback_config, recovery_err) {
                (Some(fc), RecoveryError::Exhausted { .. }) => fc.on_exhausted,
                (Some(fc), RecoveryError::CircuitOpen { .. }) => fc.on_circuit_open,
                (Some(fc), RecoveryError::BudgetExhausted { .. }) => fc.on_budget_exhausted,
                (None, _) => true, // Default: always try fallback if provided
                _ => false,
            };

            if should_fallback {
                outcome.stats.fallback_invoked = true;
                self.log_event(LogLevel::Info, "Invoking fallback function");

                match fallback(recovery_err).await {
                    Ok(value) => {
                        outcome.result = Ok(value);
                    }
                    Err(fb_err) => {
                        self.log_event(LogLevel::Warn, "Fallback also failed");
                        outcome.result = Err(RecoveryError::FallbackFailed { source: fb_err });
                    }
                }
            }
        }

        outcome
    }

    /// Execute operation with retry loop
    async fn execute_with_retry<F, Fut, T>(
        &self,
        operation: &F,
        stats: &mut RecoveryStats,
    ) -> Result<T, RecoveryError>
    where
        F: Fn() -> Fut + Send + Sync,
        Fut: Future<Output = Result<T, AosError>> + Send,
        T: Send,
    {
        let policy = &self.config.retry_policy;
        let mut attempt = 0u32;
        let mut delay = policy.base_delay;

        loop {
            attempt += 1;
            stats.retry_attempts = attempt;
            stats.budget_tokens_consumed += 1;

            // Execute the operation
            let result = operation().await;

            match result {
                Ok(value) => {
                    if attempt > 1 {
                        self.log_event(
                            LogLevel::Info,
                            &format!("Operation succeeded after {} attempts", attempt),
                        );
                    }

                    return Ok(value);
                }
                Err(err) => {
                    // Check if error is retryable
                    if !err.is_retryable() {
                        self.log_event(LogLevel::Debug, &format!("Non-retryable error: {}", err));
                        return Err(RecoveryError::NonRetryable { source: err });
                    }

                    // Check if we've exhausted attempts
                    if attempt > policy.max_attempts {
                        self.log_event(
                            LogLevel::Warn,
                            &format!("Exhausted {} retry attempts, last error: {}", attempt, err),
                        );
                        return Err(RecoveryError::Exhausted {
                            attempts: attempt,
                            source: err,
                        });
                    }

                    // Calculate next delay
                    delay = self.calculate_delay(delay, attempt, &err);

                    // Record retry attempt
                    if let Some(metrics) = &self.metrics {
                        metrics.record_retry_attempt(&self.config.service_name, attempt);
                    }

                    self.log_event(
                        LogLevel::Debug,
                        &format!(
                            "Retry attempt {} of {}, waiting {:?}",
                            attempt, policy.max_attempts, delay
                        ),
                    );

                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// Calculate delay for next retry
    fn calculate_delay(&self, current_delay: Duration, attempt: u32, error: &AosError) -> Duration {
        let policy = &self.config.retry_policy;

        // Start with exponential backoff
        let base_delay = (current_delay.as_millis() as f64 * policy.backoff_factor) as u64;
        let mut delay_ms = base_delay.min(policy.max_delay.as_millis() as u64);

        // Apply error-specific adjustment if recommended
        if let Some(recommended) = error.recommended_delay() {
            delay_ms = delay_ms.max(recommended.as_millis() as u64);
        }

        // Apply jitter
        if policy.jitter {
            delay_ms = self.apply_jitter(delay_ms, attempt);
        }

        Duration::from_millis(delay_ms)
    }

    /// Apply jitter to delay
    fn apply_jitter(&self, delay_ms: u64, attempt: u32) -> u64 {
        let jitter_range = (delay_ms as f64 * 0.1) as u64; // 10% jitter
        if jitter_range == 0 {
            return delay_ms;
        }

        let jitter = if self.config.retry_policy.deterministic_jitter {
            // Use HKDF-based deterministic jitter for reproducibility
            use hkdf::Hkdf;
            use sha2::Sha256;

            let label = format!("recovery_jitter:{}:{}", self.config.service_name, attempt);
            let hk = Hkdf::<Sha256>::new(Some(label.as_bytes()), b"adapteros-recovery");
            let mut seed_bytes = [0u8; 8];
            let _ = hk.expand(&[], &mut seed_bytes);
            u64::from_le_bytes(seed_bytes) % jitter_range
        } else {
            fastrand::Rng::new().u64(0..jitter_range)
        };

        delay_ms + jitter
    }

    /// Log an event based on telemetry config
    fn log_event(&self, level: LogLevel, message: &str) {
        if !self.config.telemetry.log_events {
            return;
        }

        // Only log if level >= configured level
        let should_log = matches!(
            (level, self.config.telemetry.log_level),
            (LogLevel::Trace, LogLevel::Trace)
                | (LogLevel::Debug, LogLevel::Trace | LogLevel::Debug)
                | (
                    LogLevel::Info,
                    LogLevel::Trace | LogLevel::Debug | LogLevel::Info
                )
                | (LogLevel::Warn, _)
        );

        if !should_log {
            return;
        }

        match level {
            LogLevel::Trace => {
                tracing::trace!(service = %self.config.service_name, "{}", message)
            }
            LogLevel::Debug => {
                tracing::debug!(service = %self.config.service_name, "{}", message)
            }
            LogLevel::Info => tracing::info!(service = %self.config.service_name, "{}", message),
            LogLevel::Warn => tracing::warn!(service = %self.config.service_name, "{}", message),
        }
    }
}

/// Builder for RecoveryOrchestrator
///
/// Provides a fluent API for configuring the orchestrator.
///
/// # Example
///
/// ```rust,ignore
/// let orchestrator = RecoveryOrchestratorBuilder::new("api-client")
///     .with_retry_policy(RetryPolicy::network("api"))
///     .use_global_circuit_breaker()
///     .deterministic_jitter(true)
///     .build();
/// ```
pub struct RecoveryOrchestratorBuilder {
    config: RecoveryConfig,
    metrics: Option<Arc<dyn RetryMetricsReporter + Send + Sync>>,
}

impl RecoveryOrchestratorBuilder {
    /// Create a new builder with the given service name
    pub fn new(service_name: impl Into<String>) -> Self {
        Self {
            config: RecoveryConfig::new(service_name),
            metrics: None,
        }
    }

    /// Set the retry policy
    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.config.retry_policy = policy;
        self
    }

    /// Configure circuit breaker
    pub fn with_circuit_breaker(mut self, config: CircuitBreakerConfig) -> Self {
        self.config.retry_policy.circuit_breaker = Some(config);
        self
    }

    /// Use the global circuit breaker registry
    pub fn use_global_circuit_breaker(mut self) -> Self {
        self.config.use_global_circuit_breaker = true;
        self
    }

    /// Configure retry budget
    pub fn with_budget(mut self, config: RetryBudgetConfig) -> Self {
        self.config.retry_policy.budget = Some(config);
        self
    }

    /// Enable SingleFlight deduplication
    pub fn with_singleflight(mut self) -> Self {
        self.config.enable_singleflight = true;
        self
    }

    /// Configure fallback behavior
    pub fn with_fallback(mut self, config: FallbackConfig) -> Self {
        self.config.fallback = Some(config);
        self
    }

    /// Enable deterministic jitter for reproducible operations
    pub fn deterministic_jitter(mut self, enabled: bool) -> Self {
        self.config.retry_policy.deterministic_jitter = enabled;
        self
    }

    /// Set metrics reporter
    pub fn with_metrics(mut self, metrics: Arc<dyn RetryMetricsReporter + Send + Sync>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Configure telemetry
    pub fn with_telemetry(mut self, telemetry: TelemetryConfig) -> Self {
        self.config.telemetry = telemetry;
        self
    }

    /// Build the orchestrator
    pub fn build(self) -> RecoveryOrchestrator {
        let mut orchestrator = RecoveryOrchestrator::new(self.config);
        if let Some(metrics) = self.metrics {
            orchestrator = orchestrator.with_metrics(metrics);
        }
        orchestrator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_successful_first_attempt() {
        let orchestrator =
            RecoveryOrchestrator::new(RecoveryConfig::new("test").deterministic_jitter(true));

        let outcome = orchestrator
            .execute(|| async { Ok::<_, AosError>("success") })
            .await;

        assert!(outcome.is_ok());
        assert_eq!(outcome.stats.retry_attempts, 1);
        assert!(!outcome.stats.fallback_invoked);
    }

    #[tokio::test]
    async fn test_retry_then_success() {
        let attempts = Arc::new(AtomicU32::new(0));

        let config = RecoveryConfig {
            service_name: "test".to_string(),
            retry_policy: RetryPolicy {
                max_attempts: 3,
                base_delay: Duration::from_millis(1),
                max_delay: Duration::from_millis(10),
                jitter: false,
                circuit_breaker: None,
                budget: None,
                ..Default::default()
            },
            ..Default::default()
        };

        let orchestrator = RecoveryOrchestrator::new(config);
        let attempts_clone = attempts.clone();

        let outcome = orchestrator
            .execute(move || {
                let a = attempts_clone.clone();
                async move {
                    let current = a.fetch_add(1, Ordering::SeqCst);
                    if current < 2 {
                        Err(AosError::Network("transient failure".to_string()))
                    } else {
                        Ok("success")
                    }
                }
            })
            .await;

        assert!(outcome.is_ok());
        assert_eq!(outcome.stats.retry_attempts, 3); // 1 initial + 2 retries
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_exhausted_retries() {
        let config = RecoveryConfig {
            service_name: "test".to_string(),
            retry_policy: RetryPolicy {
                max_attempts: 2,
                base_delay: Duration::from_millis(1),
                jitter: false,
                circuit_breaker: None,
                budget: None,
                ..Default::default()
            },
            ..Default::default()
        };

        let orchestrator = RecoveryOrchestrator::new(config);

        let outcome = orchestrator
            .execute(|| async { Err::<(), _>(AosError::Network("persistent failure".to_string())) })
            .await;

        assert!(outcome.is_err());
        assert!(matches!(
            outcome.result,
            Err(RecoveryError::Exhausted { attempts: 3, .. })
        ));
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let orchestrator = RecoveryOrchestrator::new(RecoveryConfig::new("test"));

        let outcome = orchestrator
            .execute(|| async { Err::<(), _>(AosError::Validation("bad input".to_string())) })
            .await;

        assert!(outcome.is_err());
        assert!(matches!(
            outcome.result,
            Err(RecoveryError::NonRetryable { .. })
        ));
        assert_eq!(outcome.stats.retry_attempts, 1); // No retries for non-retryable
    }

    #[tokio::test]
    async fn test_fallback_on_exhausted() {
        let config = RecoveryConfig {
            service_name: "test".to_string(),
            retry_policy: RetryPolicy {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                jitter: false,
                circuit_breaker: None,
                budget: None,
                ..Default::default()
            },
            fallback: Some(FallbackConfig::on_exhausted_only()),
            ..Default::default()
        };

        let orchestrator = RecoveryOrchestrator::new(config);

        let outcome = orchestrator
            .execute_with_fallback(
                || async { Err::<i32, _>(AosError::Network("fail".to_string())) },
                |_err| async { Ok(42) },
            )
            .await;

        assert!(outcome.is_ok());
        assert_eq!(outcome.result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_fallback_also_fails() {
        let config = RecoveryConfig {
            service_name: "test".to_string(),
            retry_policy: RetryPolicy {
                max_attempts: 1,
                base_delay: Duration::from_millis(1),
                jitter: false,
                circuit_breaker: None,
                budget: None,
                ..Default::default()
            },
            fallback: Some(FallbackConfig::on_exhausted_only()),
            ..Default::default()
        };

        let orchestrator = RecoveryOrchestrator::new(config);

        let outcome = orchestrator
            .execute_with_fallback(
                || async { Err::<i32, _>(AosError::Network("primary fail".to_string())) },
                |_err| async { Err(AosError::Network("fallback fail".to_string())) },
            )
            .await;

        assert!(outcome.is_err());
        assert!(matches!(
            outcome.result,
            Err(RecoveryError::FallbackFailed { .. })
        ));
    }

    #[tokio::test]
    async fn test_builder_pattern() {
        let orchestrator = RecoveryOrchestratorBuilder::new("api-client")
            .with_retry_policy(RetryPolicy::network("api"))
            .deterministic_jitter(true)
            .build();

        assert_eq!(orchestrator.service_name(), "api-client");
        assert!(orchestrator.config.retry_policy.deterministic_jitter);
    }

    #[tokio::test]
    async fn test_deterministic_jitter_reproducibility() {
        let orchestrator = RecoveryOrchestrator::new(
            RecoveryConfig::new("determinism-test").deterministic_jitter(true),
        );

        // Same inputs should produce same jitter
        let jitter1 = orchestrator.apply_jitter(1000, 1);
        let jitter2 = orchestrator.apply_jitter(1000, 1);

        assert_eq!(
            jitter1, jitter2,
            "Deterministic jitter should be reproducible"
        );
    }

    #[tokio::test]
    async fn test_budget_exhaustion() {
        let config = RecoveryConfig {
            service_name: "test".to_string(),
            retry_policy: RetryPolicy {
                max_attempts: 10,
                base_delay: Duration::from_millis(1),
                jitter: false,
                circuit_breaker: None,
                budget: Some(RetryBudgetConfig {
                    max_concurrent_retries: 1, // Very low
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        };

        let orchestrator = Arc::new(RecoveryOrchestrator::new(config));

        // First request holds the budget
        let orch1 = orchestrator.clone();
        let handle1 = tokio::spawn(async move {
            orch1
                .execute(|| async {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    Ok::<_, AosError>("done")
                })
                .await
        });

        // Give first request time to acquire budget
        tokio::time::sleep(Duration::from_millis(5)).await;

        // Second request should fail on budget
        let outcome = orchestrator
            .execute(|| async { Ok::<_, AosError>("should not run") })
            .await;

        assert!(outcome.is_err());
        assert!(matches!(
            outcome.result,
            Err(RecoveryError::BudgetExhausted { .. })
        ));

        // Clean up first request
        let _ = handle1.await;
    }
}
