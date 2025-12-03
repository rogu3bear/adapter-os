//! Retry logic with exponential backoff

use std::future::Future;
use std::time::Duration;

use tracing::warn;

use crate::config::ClientConfig;
use crate::error::Result;

/// Execute an async operation with retry and exponential backoff
///
/// Will retry operations that return retryable errors up to `config.max_retries` times.
/// Uses exponential backoff starting at `config.initial_retry_delay`.
pub async fn with_retry<F, Fut, T>(config: &ClientConfig, operation_name: &str, f: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut attempt = 0;
    let mut delay = config.initial_retry_delay;

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(err) if err.is_retryable() && attempt < config.max_retries => {
                attempt += 1;

                warn!(
                    operation = %operation_name,
                    attempt = attempt,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %err,
                    "Retrying after transient error"
                );

                tokio::time::sleep(delay).await;

                // Calculate next delay with exponential backoff
                delay = Duration::from_secs_f64(
                    (delay.as_secs_f64() * config.backoff_multiplier)
                        .min(config.max_retry_delay.as_secs_f64()),
                );
            }
            Err(err) => {
                if attempt > 0 {
                    warn!(
                        operation = %operation_name,
                        attempts = attempt + 1,
                        error = %err,
                        "All retry attempts exhausted"
                    );
                }
                return Err(err);
            }
        }
    }
}

/// Calculate the delay for a given retry attempt
#[allow(dead_code)]
pub fn calculate_delay(
    attempt: u32,
    initial_delay: Duration,
    max_delay: Duration,
    multiplier: f64,
) -> Duration {
    let delay_secs = initial_delay.as_secs_f64() * multiplier.powi(attempt as i32);
    Duration::from_secs_f64(delay_secs.min(max_delay.as_secs_f64()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::WorkerCpError;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn test_config() -> ClientConfig {
        ClientConfig {
            initial_retry_delay: Duration::from_millis(1),
            max_retry_delay: Duration::from_millis(100),
            max_retries: 3,
            backoff_multiplier: 2.0,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_immediate_success() {
        let config = test_config();
        let result = with_retry(&config, "test", || async { Ok(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_retry_then_success() {
        let config = test_config();
        let attempt_count = Arc::new(AtomicU32::new(0));
        let attempt_count_clone = Arc::clone(&attempt_count);

        let result = with_retry(&config, "test", || {
            let count = attempt_count_clone.clone();
            async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    Err(WorkerCpError::network("transient", true))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(attempt_count.load(Ordering::SeqCst), 3); // 2 failures + 1 success
    }

    #[tokio::test]
    async fn test_non_retryable_error() {
        let config = test_config();
        let attempt_count = Arc::new(AtomicU32::new(0));
        let attempt_count_clone = Arc::clone(&attempt_count);

        let result: Result<i32> = with_retry(&config, "test", || {
            let count = attempt_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err(WorkerCpError::from_status(400, "bad request"))
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(attempt_count.load(Ordering::SeqCst), 1); // No retries for non-retryable
    }

    #[tokio::test]
    async fn test_max_retries_exhausted() {
        let config = test_config();
        let attempt_count = Arc::new(AtomicU32::new(0));
        let attempt_count_clone = Arc::clone(&attempt_count);

        let result: Result<i32> = with_retry(&config, "test", || {
            let count = attempt_count_clone.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err(WorkerCpError::network("always fails", true))
            }
        })
        .await;

        assert!(result.is_err());
        // 1 initial attempt + 3 retries = 4 total
        assert_eq!(attempt_count.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn test_calculate_delay() {
        let initial = Duration::from_millis(100);
        let max = Duration::from_secs(5);
        let multiplier = 2.0;

        assert_eq!(calculate_delay(0, initial, max, multiplier), initial);
        assert_eq!(
            calculate_delay(1, initial, max, multiplier),
            Duration::from_millis(200)
        );
        assert_eq!(
            calculate_delay(2, initial, max, multiplier),
            Duration::from_millis(400)
        );

        // Should cap at max
        assert_eq!(calculate_delay(10, initial, max, multiplier), max);
    }
}
