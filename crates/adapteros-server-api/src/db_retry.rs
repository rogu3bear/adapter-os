//! Database operation retry logic with configurable exponential backoff
//!
//! This module provides retry capabilities for transient database failures,
//! distinguishing between retryable errors (connection failures, locks) and
//! non-retryable errors (constraint violations, validation errors).

use adapteros_core::{AosError, Result};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Configuration for database retry behavior
#[derive(Debug, Clone)]
pub struct DbRetryConfig {
    /// Maximum number of retry attempts (total attempts = max_attempts + 1)
    pub max_attempts: u32,
    /// Base delay between retries
    pub base_delay: Duration,
    /// Maximum delay between retries (exponential backoff caps here)
    pub max_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_factor: f64,
    /// Enable jitter to avoid thundering herd
    pub enable_jitter: bool,
}

impl Default for DbRetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            backoff_factor: 2.0,
            enable_jitter: true,
        }
    }
}

impl DbRetryConfig {
    /// Create a fast retry config for quick operations
    pub fn fast() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(50),
            max_delay: Duration::from_millis(500),
            backoff_factor: 2.0,
            enable_jitter: true,
        }
    }

    /// Create a slow retry config for heavy operations
    pub fn slow() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(30),
            backoff_factor: 1.5,
            enable_jitter: true,
        }
    }

    /// Create a minimal retry config (for testing)
    pub fn minimal() -> Self {
        Self {
            max_attempts: 1,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_factor: 1.5,
            enable_jitter: false,
        }
    }
}

/// Statistics about a retry operation
#[derive(Debug, Clone)]
pub struct RetryStats {
    /// Total number of attempts made
    pub attempts: u32,
    /// Total time spent retrying (excluding successful first attempt)
    pub total_duration: Duration,
    /// Whether the operation succeeded
    pub succeeded: bool,
    /// Final error if failed
    pub final_error: Option<String>,
}

/// Determines if an error should trigger a retry
fn is_retryable_error(error: &AosError) -> bool {
    match error {
        // Retry network/connection errors
        AosError::Network(_) => true,
        AosError::Timeout { .. } => true,

        // Retry IO errors that are connection-related
        AosError::Io(msg) => {
            let lower = msg.to_lowercase();
            lower.contains("connection")
                || lower.contains("timeout")
                || lower.contains("deadlock")
                || lower.contains("busy")
                || lower.contains("locked")
        }

        // Retry database errors that are transient
        AosError::Sqlite(msg) => {
            let lower = msg.to_lowercase();
            // SQLite transient errors: BUSY, LOCKED
            lower.contains("database is locked")
                || lower.contains("database table is locked")
                || lower.contains("disk i/o error")
                || lower.contains("out of memory")
        }

        AosError::Sqlx(msg) => {
            let lower = msg.to_lowercase();
            // SQLx transient errors
            lower.contains("timeout") || lower.contains("connection") || lower.contains("pool")
        }

        // Don't retry constraint/validation errors - they won't succeed on retry
        AosError::Validation(_) => false,
        AosError::Config(_) => false,
        AosError::PolicyViolation(_) => false,
        AosError::DeterminismViolation(_) => false,
        AosError::EgressViolation(_) => false,
        AosError::IsolationViolation(_) => false,

        // Default: don't retry unknown errors
        _ => false,
    }
}

/// Retry a database operation with exponential backoff
///
/// # Arguments
/// * `config` - Retry configuration
/// * `operation_name` - Human-readable name for logging
/// * `operation` - Async operation to retry
///
/// # Returns
/// Returns the operation result along with retry statistics
pub async fn retry_db_operation<F, T>(
    config: &DbRetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<(T, RetryStats)>
where
    F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
    T: Send,
{
    let mut attempt = 0;
    let mut delay = config.base_delay;
    let start_time = std::time::Instant::now();
    let mut total_wait_time = Duration::ZERO;

    loop {
        attempt += 1;

        match operation().await {
            Ok(result) => {
                if attempt > 1 {
                    info!(
                        operation = operation_name,
                        attempts = attempt,
                        total_duration_ms = total_wait_time.as_millis(),
                        "Database operation succeeded after retries"
                    );
                }

                return Ok((
                    result,
                    RetryStats {
                        attempts: attempt,
                        total_duration: total_wait_time,
                        succeeded: true,
                        final_error: None,
                    },
                ));
            }
            Err(err) => {
                // Check if this error is worth retrying
                if !is_retryable_error(&err) {
                    error!(
                        operation = operation_name,
                        error = %err,
                        attempt = attempt,
                        "Non-retryable database error encountered"
                    );

                    return Err(err);
                }

                // Check if we've exhausted retry attempts
                if attempt > config.max_attempts {
                    error!(
                        operation = operation_name,
                        error = %err,
                        attempts = attempt,
                        max_attempts = config.max_attempts,
                        total_duration_ms = total_wait_time.as_millis(),
                        "Database operation failed after exhausting retries"
                    );

                    return Err(AosError::Database(format!(
                        "Database operation '{}' failed after {} attempts: {}",
                        operation_name, attempt, err
                    )));
                }

                // Log the retry attempt
                warn!(
                    operation = operation_name,
                    error = %err,
                    attempt = attempt,
                    max_attempts = config.max_attempts,
                    next_delay_ms = delay.as_millis(),
                    "Transient database error, retrying..."
                );

                // Calculate next delay with jitter
                let mut next_delay = delay;

                if config.enable_jitter {
                    // Add up to 10% random jitter
                    let jitter_range = (delay.as_millis() as f64 * 0.1) as u64;
                    if jitter_range > 0 {
                        let jitter = fastrand::u64(0..jitter_range);
                        next_delay = Duration::from_millis(delay.as_millis() as u64 + jitter);
                    }
                }

                // Apply exponential backoff for next attempt
                let exponential_delay = Duration::from_millis(
                    (next_delay.as_millis() as f64 * config.backoff_factor) as u64,
                );
                delay = std::cmp::min(exponential_delay, config.max_delay);

                // Sleep before retry
                tokio::time::sleep(next_delay).await;
                total_wait_time += next_delay;

                debug!(
                    operation = operation_name,
                    attempt = attempt,
                    next_attempt_delay_ms = delay.as_millis(),
                    "Retrying database operation"
                );
            }
        }
    }
}

/// Convenience wrapper for simple operations without stats
pub async fn retry_db_simple<F, T>(
    config: &DbRetryConfig,
    operation_name: &str,
    operation: F,
) -> Result<T>
where
    F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T>> + Send>>,
    T: Send,
{
    let (result, _stats) = retry_db_operation(config, operation_name, operation).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_is_retryable_error() {
        // Network errors are retryable
        assert!(is_retryable_error(&AosError::Network(
            "connection failed".to_string()
        )));

        // Timeout errors are retryable
        assert!(is_retryable_error(&AosError::Timeout {
            duration: Duration::from_secs(5)
        }));

        // IO errors with "connection" are retryable
        assert!(is_retryable_error(&AosError::Io(
            "connection refused".to_string()
        )));
        assert!(is_retryable_error(&AosError::Io("timeout".to_string())));
        assert!(is_retryable_error(&AosError::Io(
            "deadlock detected".to_string()
        )));
        assert!(is_retryable_error(&AosError::Io(
            "database is locked".to_string()
        )));

        // SQLite transient errors are retryable
        assert!(is_retryable_error(&AosError::Sqlite(
            "database is locked".to_string()
        )));
        assert!(is_retryable_error(&AosError::Sqlite(
            "disk i/o error".to_string()
        )));

        // Validation errors are NOT retryable
        assert!(!is_retryable_error(&AosError::Validation(
            "invalid input".to_string()
        )));

        // Config errors are NOT retryable
        assert!(!is_retryable_error(&AosError::Config(
            "bad config".to_string()
        )));

        // Policy violations are NOT retryable
        assert!(!is_retryable_error(&AosError::PolicyViolation(
            "policy failed".to_string()
        )));
    }

    #[test]
    fn test_retry_config_variants() {
        let fast = DbRetryConfig::fast();
        assert_eq!(fast.max_attempts, 3);
        assert_eq!(fast.base_delay, Duration::from_millis(50));
        assert!(fast.enable_jitter);

        let slow = DbRetryConfig::slow();
        assert_eq!(slow.max_attempts, 5);
        assert_eq!(slow.base_delay, Duration::from_millis(500));
        assert!(slow.enable_jitter);

        let minimal = DbRetryConfig::minimal();
        assert_eq!(minimal.max_attempts, 1);
        assert!(!minimal.enable_jitter);
    }

    #[tokio::test]
    async fn test_successful_operation_no_retry() {
        let config = DbRetryConfig::default();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_db_operation(&config, "test_op", || {
            let attempts = attempts_clone.clone();
            Box::pin(async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Ok::<_, AosError>("success")
            })
        })
        .await;

        assert!(result.is_ok());
        let (value, stats) = result.unwrap();
        assert_eq!(value, "success");
        assert_eq!(stats.attempts, 1);
        assert!(stats.succeeded);
        assert_eq!(stats.total_duration, Duration::ZERO);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retries_transient_error() {
        let config = DbRetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_factor: 2.0,
            enable_jitter: false,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_db_operation(&config, "test_op", || {
            let attempts = attempts_clone.clone();
            Box::pin(async move {
                let current = attempts.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    Err(AosError::Io("connection timeout".to_string()))
                } else {
                    Ok("success")
                }
            })
        })
        .await;

        assert!(result.is_ok());
        let (value, stats) = result.unwrap();
        assert_eq!(value, "success");
        assert_eq!(stats.attempts, 3);
        assert!(stats.succeeded);
        assert!(stats.total_duration > Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_stops_on_non_retryable_error() {
        let config = DbRetryConfig::default();
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_db_operation(&config, "test_op", || {
            let attempts = attempts_clone.clone();
            Box::pin(async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(AosError::Validation("invalid input".to_string()))
            })
        })
        .await;

        assert!(result.is_err());
        // Should only try once since validation errors are not retryable
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_exhausts_retries() {
        let config = DbRetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(20),
            backoff_factor: 1.5,
            enable_jitter: false,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result = retry_db_operation(&config, "test_op", || {
            let attempts = attempts_clone.clone();
            Box::pin(async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(AosError::Network("connection failed".to_string()))
            })
        })
        .await;

        assert!(result.is_err());
        // Should try initial + max_attempts = 3 times total
        assert_eq!(attempts.load(Ordering::SeqCst), 3);

        let err = result.unwrap_err();
        assert!(err.to_string().contains("after 3 attempts"));
    }

    #[tokio::test]
    async fn test_retry_simple_wrapper() {
        let config = DbRetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(20),
            backoff_factor: 1.5,
            enable_jitter: false,
        };

        let result = retry_db_simple(&config, "test_op", || {
            Box::pin(async { Ok::<_, AosError>("success") })
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "success");
    }
}
