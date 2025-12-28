//! Retry utilities for SQLite database operations.
//!
//! Provides exponential backoff retry logic for handling transient SQLite errors
//! like SQLITE_BUSY that occur under concurrent load.

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

/// Check if a SQLite error is retriable (transient lock contention)
pub fn is_retriable_sqlite_error(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(db_err) => {
            // Check for SQLITE_BUSY (error code 5) or SQLITE_LOCKED (error code 6)
            if let Some(code) = db_err.code() {
                let code_str = code.as_ref();
                if code_str == "5" || code_str == "6" || code_str == "SQLITE_BUSY" || code_str == "SQLITE_LOCKED" {
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

/// Execute an async operation with exponential backoff retry on transient SQLite errors.
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
    E: std::fmt::Display,
{
    let mut attempts = 0u32;

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

                // Check if we should retry
                let should_retry = attempts <= config.max_retries;

                if !should_retry {
                    warn!(
                        operation = %operation_name,
                        attempts = attempts,
                        error = %e,
                        "Operation failed after max retries"
                    );
                    return Err(e);
                }

                // Calculate delay with exponential backoff
                let delay_ms = (config.base_delay_ms * 2u64.pow(attempts - 1))
                    .min(config.max_delay_ms);

                warn!(
                    operation = %operation_name,
                    attempt = attempts,
                    max_retries = config.max_retries,
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
                let should_retry = is_retriable && attempts <= config.max_retries;

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
                let delay_ms = (config.base_delay_ms * 2u64.pow(attempts - 1))
                    .min(config.max_delay_ms);

                warn!(
                    operation = %operation_name,
                    attempt = attempts,
                    max_retries = config.max_retries,
                    delay_ms = delay_ms,
                    error = %e,
                    "SQLite busy/locked, retrying"
                );

                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_succeeds_first_try() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, &str> = with_retry(
            RetryConfig::default(),
            "test_op",
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(42)
                }
            },
        )
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, &str> = with_retry(
            RetryConfig::new(3, 10),
            "test_op",
            || {
                let c = counter_clone.clone();
                async move {
                    let count = c.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        Err("transient error")
                    } else {
                        Ok(42)
                    }
                }
            },
        )
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_fails_after_max_retries() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, &str> = with_retry(
            RetryConfig::new(2, 10),
            "test_op",
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("persistent error")
                }
            },
        )
        .await;

        assert!(result.is_err());
        // 1 initial + 2 retries = 3 attempts
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }
}
