//! Retry utilities for database operations.
//!
//! Provides exponential backoff retry logic for handling transient database errors
//! like SQLITE_BUSY, PostgreSQL deadlocks, and connection pool exhaustion.
#![allow(clippy::manual_range_contains)]

use crate::error_classification::{classify_sqlx_error, DatabaseBackend, DbErrorClass, Retriable};
use std::future::Future;
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Base delay in milliseconds (doubles with each retry)
    pub base_delay_ms: u64,
    /// Maximum delay cap in milliseconds
    pub max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 100,
            max_delay_ms: 2000,
        }
    }
}

impl RetryConfig {
    /// Create a new retry config with specified parameters
    pub fn new(max_retries: u32, base_delay_ms: u64) -> Self {
        Self {
            max_retries,
            base_delay_ms,
            max_delay_ms: 5000,
        }
    }

    /// Configuration for critical operations that need more retries
    pub fn critical() -> Self {
        Self {
            max_retries: 5,
            base_delay_ms: 100,
            max_delay_ms: 5000,
        }
    }
}

#[derive(Debug, Clone)]
struct RetryEnvelopeAdapter {
    policy: adapteros_core::retry_policy::RetryPolicy,
}

impl RetryEnvelopeAdapter {
    fn from_config(config: &RetryConfig, service_type: &str) -> Self {
        let mut policy = adapteros_core::retry_policy::RetryPolicy::without_budget(service_type);
        policy.circuit_breaker = None;
        policy.max_attempts = config.max_retries;
        policy.base_delay = Duration::from_millis(config.base_delay_ms);
        policy.max_delay = Duration::from_millis(config.max_delay_ms.max(config.base_delay_ms));
        policy.backoff_factor = 2.0;
        policy.jitter = false;
        policy.deterministic_jitter = false;
        Self { policy }
    }

    fn max_attempts(&self) -> u32 {
        self.policy.max_attempts
    }

    fn retries_exhausted(&self, attempts: u32) -> bool {
        attempts > self.policy.max_attempts
    }

    fn max_delay_ms(&self) -> u64 {
        self.policy.max_delay.as_millis() as u64
    }

    fn exponential_factor(&self, attempt: u32) -> u64 {
        2u64.saturating_pow(attempt.saturating_sub(1))
    }

    fn delay_ms(&self, base_delay_ms: u64, attempt: u32) -> u64 {
        base_delay_ms
            .saturating_mul(self.exponential_factor(attempt))
            .min(self.max_delay_ms())
    }
}

/// Check if a SQLite error is retriable (transient lock contention)
pub fn is_retriable_sqlite_error(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(db_err) => {
            // Check for SQLITE_BUSY (error code 5) or SQLITE_LOCKED (error code 6)
            if let Some(code) = db_err.code() {
                let code_str = code.as_ref();
                if code_str == "5"
                    || code_str == "6"
                    || code_str == "SQLITE_BUSY"
                    || code_str == "SQLITE_LOCKED"
                {
                    return true;
                }
            }
            // Also check the message for busy/locked indicators
            let msg = db_err.message().to_lowercase();
            msg.contains("database is locked") || msg.contains("busy")
        }
        sqlx::Error::PoolTimedOut => true,
        sqlx::Error::PoolClosed => false,
        _ => false,
    }
}

/// Execute an async operation with exponential backoff retry on retriable errors.
///
/// Only retries errors where `Retriable::is_retriable()` returns true.
/// Non-retriable errors (auth failures, permission denied, etc.) are returned immediately.
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation_name` - Name for logging purposes
/// * `operation` - The async operation to execute
///
/// # Example
/// ```ignore
/// use adapteros_db::retry::{with_retry, RetryConfig};
///
/// let result = with_retry(
///     RetryConfig::default(),
///     "update_model_status",
///     || async {
///         db.update_base_model_status(tenant_id, model_id, status, None, None).await
///     }
/// ).await?;
/// ```
pub async fn with_retry<T, E, F, Fut>(
    config: RetryConfig,
    operation_name: &str,
    operation: F,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display + Retriable,
{
    let mut attempts = 0u32;
    let envelope = RetryEnvelopeAdapter::from_config(&config, operation_name);

    loop {
        match operation().await {
            Ok(result) => {
                if attempts > 0 {
                    debug!(
                        operation = %operation_name,
                        attempts = attempts + 1,
                        "Operation succeeded after retries"
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                attempts += 1;

                // Check if this error is retriable at all
                if !e.is_retriable() {
                    debug!(
                        operation = %operation_name,
                        error = %e,
                        "Non-retriable error, returning immediately"
                    );
                    return Err(e);
                }

                // Check if we've exhausted retries
                if envelope.retries_exhausted(attempts) {
                    warn!(
                        operation = %operation_name,
                        attempts = attempts,
                        error = %e,
                        "Operation failed after max retries"
                    );
                    return Err(e);
                }

                // Calculate delay with exponential backoff
                let delay_ms = envelope.delay_ms(config.base_delay_ms, attempts);

                warn!(
                    operation = %operation_name,
                    attempt = attempts,
                    max_retries = envelope.max_attempts(),
                    delay_ms = delay_ms,
                    error = %e,
                    "Transient error, retrying"
                );

                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

/// Execute an async operation with retry, only retrying on SQLite-specific transient errors.
///
/// This is a more targeted version that only retries on SQLITE_BUSY/SQLITE_LOCKED errors.
pub async fn with_sqlite_retry<T, F, Fut>(
    config: RetryConfig,
    operation_name: &str,
    operation: F,
) -> Result<T, sqlx::Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    let mut attempts = 0u32;
    let envelope = RetryEnvelopeAdapter::from_config(&config, operation_name);

    loop {
        match operation().await {
            Ok(result) => {
                if attempts > 0 {
                    debug!(
                        operation = %operation_name,
                        attempts = attempts + 1,
                        "SQLite operation succeeded after retries"
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                attempts += 1;

                // Only retry on transient SQLite errors
                let is_retriable = is_retriable_sqlite_error(&e);
                let should_retry = is_retriable && !envelope.retries_exhausted(attempts);

                if !should_retry {
                    if is_retriable {
                        warn!(
                            operation = %operation_name,
                            attempts = attempts,
                            error = %e,
                            "SQLite operation failed after max retries"
                        );
                    }
                    return Err(e);
                }

                // Calculate delay with exponential backoff
                let delay_ms = envelope.delay_ms(config.base_delay_ms, attempts);

                warn!(
                    operation = %operation_name,
                    attempt = attempts,
                    max_retries = envelope.max_attempts(),
                    delay_ms = delay_ms,
                    error = %e,
                    "SQLite busy/locked, retrying"
                );

                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

/// Extended retry configuration with database-specific options
///
/// Provides more sophisticated retry behavior including:
/// - Backend-aware error classification
/// - Adaptive backoff for lock contention
/// - Total duration circuit breaker
/// - Jitter for thundering herd prevention
#[derive(Debug, Clone)]
pub struct DbRetryConfig {
    /// Base retry configuration
    pub base: RetryConfig,
    /// Database backend type for error classification
    pub backend: DatabaseBackend,
    /// Additional delay multiplier for consecutive lock contention errors
    pub lock_contention_multiplier: f64,
    /// Maximum total retry duration in milliseconds (circuit breaker)
    pub max_total_duration_ms: u64,
    /// Jitter factor (0.0 to 1.0) for randomizing delays
    pub jitter_factor: f64,
}

impl Default for DbRetryConfig {
    fn default() -> Self {
        Self {
            base: RetryConfig::default(),
            backend: DatabaseBackend::Sqlite,
            lock_contention_multiplier: 1.5,
            max_total_duration_ms: 30_000, // 30 seconds max
            jitter_factor: 0.1,
        }
    }
}

impl DbRetryConfig {
    /// Configuration optimized for PostgreSQL
    ///
    /// Uses longer timeouts and more retries suitable for network databases.
    pub fn postgres() -> Self {
        Self {
            base: RetryConfig {
                max_retries: 5,
                base_delay_ms: 50,
                max_delay_ms: 5000,
            },
            backend: DatabaseBackend::Postgres,
            lock_contention_multiplier: 2.0,
            max_total_duration_ms: 60_000, // 60 seconds
            jitter_factor: 0.15,
        }
    }

    /// Configuration optimized for SQLite
    ///
    /// Uses shorter timeouts suitable for local embedded databases.
    pub fn sqlite() -> Self {
        Self {
            base: RetryConfig::default(),
            backend: DatabaseBackend::Sqlite,
            lock_contention_multiplier: 1.5,
            max_total_duration_ms: 30_000, // 30 seconds
            jitter_factor: 0.1,
        }
    }

    /// Configuration for critical operations
    ///
    /// Uses more aggressive retry with longer total duration.
    pub fn critical(backend: DatabaseBackend) -> Self {
        Self {
            base: RetryConfig::critical(),
            backend,
            lock_contention_multiplier: 2.0,
            max_total_duration_ms: 120_000, // 2 minutes
            jitter_factor: 0.2,
        }
    }
}

/// Check if a database error is retriable based on classification
pub fn is_retriable_db_error(err: &sqlx::Error, backend: DatabaseBackend) -> bool {
    classify_sqlx_error(err, backend).is_retryable()
}

/// Execute a database operation with intelligent retry based on error classification
///
/// This function provides sophisticated retry behavior:
/// - Classifies errors to determine if they are retryable
/// - Uses exponential backoff with jitter
/// - Applies additional delay for sustained lock contention
/// - Enforces a total duration circuit breaker
///
/// # Arguments
/// * `config` - Database retry configuration
/// * `operation_name` - Name for logging purposes
/// * `operation` - The async operation to execute
///
/// # Example
/// ```ignore
/// use adapteros_db::retry::{with_db_retry, DbRetryConfig};
///
/// let result = with_db_retry(
///     DbRetryConfig::sqlite(),
///     "update_model_status",
///     || async {
///         db.update_base_model_status(tenant_id, model_id, status, None, None).await
///     }
/// ).await?;
/// ```
pub async fn with_db_retry<T, F, Fut>(
    config: DbRetryConfig,
    operation_name: &str,
    operation: F,
) -> Result<T, sqlx::Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    let start_time = std::time::Instant::now();
    let mut attempts = 0u32;
    let mut consecutive_lock_errors = 0u32;
    let envelope = RetryEnvelopeAdapter::from_config(&config.base, operation_name);

    loop {
        match operation().await {
            Ok(result) => {
                if attempts > 0 {
                    debug!(
                        operation = %operation_name,
                        attempts = attempts + 1,
                        elapsed_ms = start_time.elapsed().as_millis() as u64,
                        "Database operation succeeded after retries"
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                attempts += 1;
                let error_class = classify_sqlx_error(&e, config.backend);

                // Check if error is retriable
                if !error_class.is_retryable() {
                    debug!(
                        operation = %operation_name,
                        error_class = ?error_class,
                        error = %e,
                        "Non-retriable database error"
                    );
                    return Err(e);
                }

                // Check retry limits
                let elapsed_ms = start_time.elapsed().as_millis() as u64;
                if envelope.retries_exhausted(attempts) {
                    warn!(
                        operation = %operation_name,
                        attempts = attempts,
                        elapsed_ms = elapsed_ms,
                        error = %e,
                        "Database operation failed after max retries"
                    );
                    return Err(e);
                }

                if elapsed_ms >= config.max_total_duration_ms {
                    warn!(
                        operation = %operation_name,
                        attempts = attempts,
                        elapsed_ms = elapsed_ms,
                        max_duration_ms = config.max_total_duration_ms,
                        error = %e,
                        "Database operation failed - total duration exceeded"
                    );
                    return Err(e);
                }

                // Track consecutive lock errors for adaptive backoff
                if matches!(
                    error_class,
                    DbErrorClass::LockContention | DbErrorClass::MigrationLocked
                ) {
                    consecutive_lock_errors += 1;
                } else {
                    consecutive_lock_errors = 0;
                }

                // Calculate delay with exponential backoff
                let base_delay = error_class
                    .recommended_delay_ms()
                    .max(config.base.base_delay_ms);
                let mut delay_ms = envelope.delay_ms(base_delay, attempts);

                // Apply lock contention multiplier for sustained contention
                if consecutive_lock_errors > 1 {
                    let multiplier = config
                        .lock_contention_multiplier
                        .powi(consecutive_lock_errors.saturating_sub(1) as i32);
                    delay_ms = (delay_ms as f64 * multiplier) as u64;
                }

                // Cap at max delay
                delay_ms = delay_ms.min(envelope.max_delay_ms());

                // Add jitter to prevent thundering herd (uses deterministic RNG when configured)
                if config.jitter_factor > 0.0 {
                    delay_ms = adapteros_core::compute_jitter_delay(delay_ms, config.jitter_factor);
                }

                warn!(
                    operation = %operation_name,
                    attempt = attempts,
                    max_retries = envelope.max_attempts(),
                    error_class = ?error_class,
                    delay_ms = delay_ms,
                    consecutive_lock_errors = consecutive_lock_errors,
                    error = %e,
                    "Retriable database error, backing off"
                );

                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_classification::Retriable;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    /// Test error type for retry tests
    #[derive(Debug, Clone)]
    enum TestError {
        /// Transient error that should be retried
        Transient(String),
        /// Permanent error that should NOT be retried
        Permanent(String),
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                TestError::Transient(msg) => write!(f, "transient: {}", msg),
                TestError::Permanent(msg) => write!(f, "permanent: {}", msg),
            }
        }
    }

    impl Retriable for TestError {
        fn is_retriable(&self) -> bool {
            matches!(self, TestError::Transient(_))
        }
    }

    #[tokio::test]
    async fn test_retry_succeeds_first_try() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, TestError> = with_retry(RetryConfig::default(), "test_op", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, TestError> = with_retry(RetryConfig::new(3, 10), "test_op", || {
            let c = counter_clone.clone();
            async move {
                let count = c.fetch_add(1, Ordering::SeqCst);
                if count < 2 {
                    Err(TestError::Transient("try again".into()))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, TestError> = with_retry(RetryConfig::new(2, 10), "test_op", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(TestError::Transient("always fails".into()))
            }
        })
        .await;

        assert!(result.is_err());
        // 1 initial + 2 retries = 3 attempts
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_non_retriable_error_returns_immediately() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, TestError> = with_retry(RetryConfig::new(5, 10), "test_op", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(TestError::Permanent("auth failed".into()))
            }
        })
        .await;

        assert!(result.is_err());
        // Should only try once, no retries for permanent errors
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // Retry Configuration Validation Tests
    // =========================================================================

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 2000);
    }

    #[test]
    fn test_retry_config_new() {
        let config = RetryConfig::new(5, 200);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 200);
        assert_eq!(config.max_delay_ms, 5000); // Default max_delay_ms in new()
    }

    #[test]
    fn test_retry_config_critical() {
        let config = RetryConfig::critical();
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.base_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 5000);
    }

    #[test]
    fn test_db_retry_config_default() {
        let config = DbRetryConfig::default();
        assert_eq!(config.base.max_retries, 3);
        assert_eq!(config.base.base_delay_ms, 100);
        assert_eq!(config.base.max_delay_ms, 2000);
        assert_eq!(config.backend, DatabaseBackend::Sqlite);
        assert!((config.lock_contention_multiplier - 1.5).abs() < f64::EPSILON);
        assert_eq!(config.max_total_duration_ms, 30_000);
        assert!((config.jitter_factor - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_db_retry_config_postgres() {
        let config = DbRetryConfig::postgres();
        assert_eq!(config.base.max_retries, 5);
        assert_eq!(config.base.base_delay_ms, 50);
        assert_eq!(config.base.max_delay_ms, 5000);
        assert_eq!(config.backend, DatabaseBackend::Postgres);
        assert!((config.lock_contention_multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.max_total_duration_ms, 60_000);
        assert!((config.jitter_factor - 0.15).abs() < f64::EPSILON);
    }

    #[test]
    fn test_db_retry_config_sqlite() {
        let config = DbRetryConfig::sqlite();
        assert_eq!(config.backend, DatabaseBackend::Sqlite);
        assert!((config.lock_contention_multiplier - 1.5).abs() < f64::EPSILON);
        assert_eq!(config.max_total_duration_ms, 30_000);
    }

    #[test]
    fn test_db_retry_config_critical() {
        let config = DbRetryConfig::critical(DatabaseBackend::Postgres);
        assert_eq!(config.base.max_retries, 5);
        assert_eq!(config.backend, DatabaseBackend::Postgres);
        assert!((config.lock_contention_multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.max_total_duration_ms, 120_000);
        assert!((config.jitter_factor - 0.2).abs() < f64::EPSILON);

        let config_sqlite = DbRetryConfig::critical(DatabaseBackend::Sqlite);
        assert_eq!(config_sqlite.backend, DatabaseBackend::Sqlite);
    }

    #[test]
    fn test_retry_config_zero_retries() {
        let config = RetryConfig::new(0, 100);
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn test_retry_config_zero_delay() {
        let config = RetryConfig::new(3, 0);
        assert_eq!(config.base_delay_ms, 0);
    }

    // =========================================================================
    // Backoff Calculation Tests
    // =========================================================================

    #[test]
    fn test_backoff_calculation_exponential() {
        // Test that backoff follows exponential pattern: base_delay * 2^(attempt-1)
        let base_delay_ms: u64 = 100;
        let max_delay_ms: u64 = 10000;

        // attempt 1: 100 * 2^0 = 100
        let delay_1 = (base_delay_ms * 2u64.pow(0)).min(max_delay_ms);
        assert_eq!(delay_1, 100);

        // attempt 2: 100 * 2^1 = 200
        let delay_2 = (base_delay_ms * 2u64.pow(1)).min(max_delay_ms);
        assert_eq!(delay_2, 200);

        // attempt 3: 100 * 2^2 = 400
        let delay_3 = (base_delay_ms * 2u64.pow(2)).min(max_delay_ms);
        assert_eq!(delay_3, 400);

        // attempt 4: 100 * 2^3 = 800
        let delay_4 = (base_delay_ms * 2u64.pow(3)).min(max_delay_ms);
        assert_eq!(delay_4, 800);
    }

    #[test]
    fn test_backoff_respects_max_delay() {
        let base_delay_ms: u64 = 100;
        let max_delay_ms: u64 = 500;

        // attempt 3: 100 * 2^2 = 400 (under max)
        let delay_3 = (base_delay_ms * 2u64.pow(2)).min(max_delay_ms);
        assert_eq!(delay_3, 400);

        // attempt 4: 100 * 2^3 = 800, but capped at 500
        let delay_4 = (base_delay_ms * 2u64.pow(3)).min(max_delay_ms);
        assert_eq!(delay_4, 500);

        // attempt 5: 100 * 2^4 = 1600, but capped at 500
        let delay_5 = (base_delay_ms * 2u64.pow(4)).min(max_delay_ms);
        assert_eq!(delay_5, 500);
    }

    #[test]
    fn test_backoff_with_very_small_base() {
        let base_delay_ms: u64 = 1;
        let max_delay_ms: u64 = 100;

        // attempt 1: 1 * 2^0 = 1
        assert_eq!((base_delay_ms * 2u64.pow(0)).min(max_delay_ms), 1);
        // attempt 7: 1 * 2^6 = 64
        assert_eq!((base_delay_ms * 2u64.pow(6)).min(max_delay_ms), 64);
        // attempt 8: 1 * 2^7 = 128 -> capped at 100
        assert_eq!((base_delay_ms * 2u64.pow(7)).min(max_delay_ms), 100);
    }

    #[test]
    fn test_lock_contention_multiplier_effect() {
        let base_delay: u64 = 100;
        let multiplier = 1.5_f64;

        // First lock error: no multiplier (consecutive_lock_errors = 1)
        let delay_first = base_delay;
        assert_eq!(delay_first, 100);

        // Second consecutive lock error: multiplier^1
        let delay_second = (base_delay as f64 * multiplier.powi(1)) as u64;
        assert_eq!(delay_second, 150);

        // Third consecutive lock error: multiplier^2
        let delay_third = (base_delay as f64 * multiplier.powi(2)) as u64;
        assert_eq!(delay_third, 225);

        // Fourth consecutive lock error: multiplier^3
        let delay_fourth = (base_delay as f64 * multiplier.powi(3)) as u64;
        assert_eq!(delay_fourth, 337); // 100 * 3.375 = 337.5 truncated
    }

    // =========================================================================
    // Max Retry Limit Enforcement Tests
    // =========================================================================

    #[tokio::test]
    async fn test_max_retries_zero_means_one_attempt() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, TestError> = with_retry(RetryConfig::new(0, 10), "test_op", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(TestError::Transient("fail".into()))
            }
        })
        .await;

        assert!(result.is_err());
        // With max_retries=0, only the initial attempt runs
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_max_retries_exact_count() {
        // Test that we get exactly max_retries + 1 attempts (1 initial + max_retries)
        for max_retries in 1..=5 {
            let counter = Arc::new(AtomicU32::new(0));
            let counter_clone = counter.clone();

            let _: Result<i32, TestError> =
                with_retry(RetryConfig::new(max_retries, 1), "test_op", || {
                    let c = counter_clone.clone();
                    async move {
                        c.fetch_add(1, Ordering::SeqCst);
                        Err(TestError::Transient("fail".into()))
                    }
                })
                .await;

            assert_eq!(
                counter.load(Ordering::SeqCst),
                max_retries + 1,
                "max_retries={} should result in {} total attempts",
                max_retries,
                max_retries + 1
            );
        }
    }

    #[tokio::test]
    async fn test_success_on_last_retry() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();
        let max_retries = 3u32;

        let result: Result<i32, TestError> =
            with_retry(RetryConfig::new(max_retries, 1), "test_op", || {
                let c = counter_clone.clone();
                async move {
                    let count = c.fetch_add(1, Ordering::SeqCst);
                    // Succeed on the last attempt (attempt index = max_retries)
                    if count == max_retries {
                        Ok(42)
                    } else {
                        Err(TestError::Transient("not yet".into()))
                    }
                }
            })
            .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), max_retries + 1);
    }

    #[tokio::test]
    async fn test_sqlite_retry_respects_max_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), sqlx::Error> =
            with_sqlite_retry(RetryConfig::new(2, 1), "test_sqlite_op", || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(sqlx::Error::PoolTimedOut)
                }
            })
            .await;

        assert!(result.is_err());
        // 1 initial + 2 retries = 3
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_sqlite_retry_non_retriable_no_retry() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), sqlx::Error> =
            with_sqlite_retry(RetryConfig::new(5, 1), "test_sqlite_op", || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    // PoolClosed is not retriable for sqlite_retry
                    Err(sqlx::Error::PoolClosed)
                }
            })
            .await;

        assert!(result.is_err());
        // Should only try once since PoolClosed is not a retriable SQLite error
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // =========================================================================
    // Jitter Application Tests
    // =========================================================================

    #[test]
    fn test_jitter_stays_within_bounds() {
        let delay_ms: u64 = 1000;
        let jitter_factor = 0.1;

        // Run multiple iterations to test randomness bounds
        for _ in 0..100 {
            let jitter_range = delay_ms as f64 * jitter_factor;
            let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
            let adjusted_delay = ((delay_ms as f64 + jitter).max(1.0)) as u64;

            // With jitter_factor=0.1, delay should be in range [900, 1100]
            // But due to .max(1.0), minimum is 1
            assert!(
                adjusted_delay >= 900 && adjusted_delay <= 1100,
                "Jittered delay {} outside expected range [900, 1100]",
                adjusted_delay
            );
        }
    }

    #[test]
    fn test_jitter_with_zero_factor() {
        let delay_ms: u64 = 1000;
        let jitter_factor = 0.0;

        // With zero jitter factor, delay should remain unchanged
        let jitter_range = delay_ms as f64 * jitter_factor;
        let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
        let adjusted_delay = ((delay_ms as f64 + jitter).max(1.0)) as u64;

        assert_eq!(adjusted_delay, 1000);
    }

    #[test]
    fn test_jitter_with_small_delay_never_goes_below_one() {
        let delay_ms: u64 = 1;
        let jitter_factor = 0.5; // 50% jitter on a 1ms delay

        // Even with aggressive jitter, should never go below 1
        for _ in 0..100 {
            let jitter_range = delay_ms as f64 * jitter_factor;
            let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
            let adjusted_delay = ((delay_ms as f64 + jitter).max(1.0)) as u64;

            assert!(
                adjusted_delay >= 1,
                "Jittered delay {} went below 1",
                adjusted_delay
            );
        }
    }

    #[test]
    fn test_jitter_distribution_centered() {
        // Statistical test: verify jitter is roughly centered around the original delay
        let delay_ms: u64 = 1000;
        let jitter_factor = 0.1;
        let iterations = 1000;

        let mut total: f64 = 0.0;
        for _ in 0..iterations {
            let jitter_range = delay_ms as f64 * jitter_factor;
            let jitter = (rand::random::<f64>() - 0.5) * 2.0 * jitter_range;
            let adjusted_delay = (delay_ms as f64 + jitter).max(1.0);
            total += adjusted_delay;
        }

        let average = total / iterations as f64;
        // Average should be close to original delay (within 5% tolerance)
        let tolerance = delay_ms as f64 * 0.05;
        assert!(
            (average - delay_ms as f64).abs() < tolerance,
            "Average jittered delay {} deviates too much from expected {}",
            average,
            delay_ms
        );
    }

    #[test]
    fn test_db_retry_config_jitter_values() {
        // Verify jitter factors for different configs are reasonable (0.0 to 1.0)
        let default_jitter = DbRetryConfig::default().jitter_factor;
        assert!(default_jitter >= 0.0 && default_jitter <= 1.0);

        let postgres_jitter = DbRetryConfig::postgres().jitter_factor;
        assert!(postgres_jitter >= 0.0 && postgres_jitter <= 1.0);

        let sqlite_jitter = DbRetryConfig::sqlite().jitter_factor;
        assert!(sqlite_jitter >= 0.0 && sqlite_jitter <= 1.0);

        let critical_jitter = DbRetryConfig::critical(DatabaseBackend::Sqlite).jitter_factor;
        assert!(critical_jitter >= 0.0 && critical_jitter <= 1.0);
    }

    // =========================================================================
    // Integration Tests for is_retriable_sqlite_error
    // =========================================================================

    #[test]
    fn test_is_retriable_sqlite_error_pool_timeout() {
        let err = sqlx::Error::PoolTimedOut;
        assert!(is_retriable_sqlite_error(&err));
    }

    #[test]
    fn test_is_retriable_sqlite_error_pool_closed() {
        let err = sqlx::Error::PoolClosed;
        assert!(!is_retriable_sqlite_error(&err));
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[tokio::test]
    async fn test_retry_with_alternating_errors() {
        // Test that retry works correctly when error types alternate
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, TestError> = with_retry(RetryConfig::new(5, 1), "test_op", || {
            let c = counter_clone.clone();
            async move {
                let count = c.fetch_add(1, Ordering::SeqCst);
                match count {
                    0 => Err(TestError::Transient("first".into())),
                    1 => Err(TestError::Transient("second".into())),
                    2 => Ok(42), // Succeed on third attempt
                    _ => Err(TestError::Transient("should not reach".into())),
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_stops_on_permanent_error_after_transient() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, TestError> = with_retry(RetryConfig::new(5, 1), "test_op", || {
            let c = counter_clone.clone();
            async move {
                let count = c.fetch_add(1, Ordering::SeqCst);
                match count {
                    0 => Err(TestError::Transient("retry this".into())),
                    1 => Err(TestError::Permanent("stop here".into())), // Should stop
                    _ => Ok(42),                                        // Should never reach
                }
            }
        })
        .await;

        assert!(matches!(result, Err(TestError::Permanent(_))));
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn test_retry_config_clone() {
        let config = RetryConfig::new(3, 100);
        let cloned = config.clone();
        assert_eq!(config.max_retries, cloned.max_retries);
        assert_eq!(config.base_delay_ms, cloned.base_delay_ms);
        assert_eq!(config.max_delay_ms, cloned.max_delay_ms);
    }

    #[test]
    fn test_db_retry_config_clone() {
        let config = DbRetryConfig::postgres();
        let cloned = config.clone();
        assert_eq!(config.base.max_retries, cloned.base.max_retries);
        assert_eq!(config.backend, cloned.backend);
        assert!(
            (config.lock_contention_multiplier - cloned.lock_contention_multiplier).abs()
                < f64::EPSILON
        );
    }
}
