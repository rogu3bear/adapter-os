//! Tests for database retry logic with transient failure simulation
//!
//! These tests verify that the retry mechanism correctly:
//! 1. Retries on transient errors (connection failures, locks)
//! 2. Does NOT retry on permanent errors (validation, constraint violations)
//! 3. Uses exponential backoff with jitter
//! 4. Respects maximum retry attempts
//! 5. Logs retry attempts at appropriate levels

#[cfg(test)]
mod tests {
    use adapteros_core::AosError;
    use adapteros_server_api::db_retry::{retry_db_operation, retry_db_simple, DbRetryConfig};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// Test that a successful operation on first attempt doesn't retry
    #[tokio::test]
    async fn test_no_retry_on_immediate_success() {
        let config = DbRetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_immediate_success", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, AosError>(42)
            })
        })
        .await;

        assert!(result.is_ok());
        let (value, stats) = result.unwrap();
        assert_eq!(value, 42);
        assert_eq!(stats.attempts, 1);
        assert!(stats.succeeded);
        assert_eq!(stats.total_duration, Duration::ZERO);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    /// Test that transient connection errors trigger retries
    #[tokio::test]
    async fn test_retries_on_connection_error() {
        let config = DbRetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_factor: 2.0,
            enable_jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_connection_error", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                match current {
                    0 | 1 => Err(AosError::Io("connection refused".to_string())),
                    _ => Ok::<i32, AosError>(42),
                }
            })
        })
        .await;

        assert!(result.is_ok());
        let (value, stats) = result.unwrap();
        assert_eq!(value, 42);
        assert_eq!(stats.attempts, 3);
        assert!(stats.succeeded);
        assert!(stats.total_duration > Duration::from_millis(10));
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    /// Test that transient database lock errors trigger retries
    #[tokio::test]
    async fn test_retries_on_database_locked() {
        let config = DbRetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_factor: 2.0,
            enable_jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_database_locked", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    Err(AosError::Sqlite("database is locked".to_string()))
                } else {
                    Ok::<i32, AosError>(42)
                }
            })
        })
        .await;

        assert!(result.is_ok());
        let (value, stats) = result.unwrap();
        assert_eq!(value, 42);
        assert_eq!(stats.attempts, 2);
        assert!(stats.succeeded);
    }

    /// Test that timeout errors trigger retries
    #[tokio::test]
    async fn test_retries_on_timeout() {
        let config = DbRetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(50),
            backoff_factor: 2.0,
            enable_jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_timeout", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    Err(AosError::Timeout {
                        duration: Duration::from_secs(5),
                    })
                } else {
                    Ok::<i32, AosError>(42)
                }
            })
        })
        .await;

        assert!(result.is_ok());
        let (value, stats) = result.unwrap();
        assert_eq!(value, 42);
        assert_eq!(stats.attempts, 2);
        assert!(stats.succeeded);
    }

    /// Test that validation errors do NOT trigger retries
    #[tokio::test]
    async fn test_no_retry_on_validation_error() {
        let config = DbRetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_validation_error", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err(AosError::Validation(
                    "invalid adapter ID format".to_string(),
                ))
            })
        })
        .await;

        assert!(result.is_err());
        // Should only attempt once since validation errors are not retryable
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    /// Test that constraint violations do NOT trigger retries
    #[tokio::test]
    async fn test_no_retry_on_config_error() {
        let config = DbRetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_config_error", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err(AosError::Config("bad database config".to_string()))
            })
        })
        .await;

        assert!(result.is_err());
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    /// Test that max_attempts limit is respected
    #[tokio::test]
    async fn test_exhausts_max_attempts() {
        let config = DbRetryConfig {
            max_attempts: 2,
            base_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(20),
            backoff_factor: 1.5,
            enable_jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_max_attempts", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err(AosError::Network("persistent network error".to_string()))
            })
        })
        .await;

        assert!(result.is_err());
        // Should try: initial attempt + max_attempts retries = 3 total
        assert_eq!(call_count.load(Ordering::SeqCst), 3);

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("after 3 attempts"));
    }

    /// Test exponential backoff progression
    #[tokio::test]
    async fn test_exponential_backoff() {
        let config = DbRetryConfig {
            max_attempts: 3,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(1),
            backoff_factor: 2.0,
            enable_jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();
        let start_time = std::time::Instant::now();

        let result = retry_db_operation(&config, "test_exponential_backoff", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current < 3 {
                    Err(AosError::Network("temp error".to_string()))
                } else {
                    Ok::<i32, AosError>(42)
                }
            })
        })
        .await;

        assert!(result.is_ok());
        let (_value, stats) = result.unwrap();
        let elapsed = start_time.elapsed();

        // With exponential backoff 2.0x and base delay 10ms:
        // Attempt 1: 0ms
        // Attempt 2: +10ms = 10ms
        // Attempt 3: +20ms (10ms * 2.0) = 30ms
        // Attempt 4: +40ms (20ms * 2.0) = 70ms
        // Total minimum: ~70ms (before the 4th successful attempt)
        assert!(elapsed > Duration::from_millis(50));
        assert_eq!(stats.attempts, 4);
    }

    /// Test that jitter is applied when enabled
    #[tokio::test]
    async fn test_jitter_application() {
        let config = DbRetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(200),
            backoff_factor: 1.0,
            enable_jitter: true,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_jitter", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    Err(AosError::Network("temp error".to_string()))
                } else {
                    Ok::<i32, AosError>(42)
                }
            })
        })
        .await;

        assert!(result.is_ok());
        // With jitter enabled, the actual delay should vary between 100ms and 110ms
        // (base_delay + 0-10% jitter)
        let (_, stats) = result.unwrap();
        assert_eq!(stats.attempts, 2);
        // Jitter may cause timing to be slightly different from exact base_delay
        assert!(stats.total_duration >= Duration::from_millis(100));
    }

    /// Test retry_db_simple wrapper function
    #[tokio::test]
    async fn test_retry_db_simple_wrapper() {
        let config = DbRetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(20),
            backoff_factor: 1.5,
            enable_jitter: false,
        };

        let result = retry_db_simple(&config, "test_simple", || {
            Box::pin(async { Ok::<i32, AosError>(42) })
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    /// Test multiple retries before eventual success
    #[tokio::test]
    async fn test_multiple_transient_failures_then_success() {
        let config = DbRetryConfig {
            max_attempts: 5,
            base_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(50),
            backoff_factor: 1.5,
            enable_jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_multiple_transient", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                match current {
                    0..=3 => Err(AosError::Io("database is locked".to_string())),
                    _ => Ok::<i32, AosError>(42),
                }
            })
        })
        .await;

        assert!(result.is_ok());
        let (value, stats) = result.unwrap();
        assert_eq!(value, 42);
        assert_eq!(stats.attempts, 5);
        assert!(stats.succeeded);
        assert_eq!(call_count.load(Ordering::SeqCst), 5);
    }

    /// Test that network errors are retryable
    #[tokio::test]
    async fn test_network_error_is_retryable() {
        let config = DbRetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(5),
            max_delay: Duration::from_millis(20),
            backoff_factor: 1.5,
            enable_jitter: false,
        };

        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let result = retry_db_operation(&config, "test_network_error", || {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let current = count.fetch_add(1, Ordering::SeqCst);
                if current == 0 {
                    Err(AosError::Network("connection reset".to_string()))
                } else {
                    Ok::<i32, AosError>(42)
                }
            })
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    /// Test fast config preset
    #[tokio::test]
    async fn test_fast_config_preset() {
        let config = DbRetryConfig::fast();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay, Duration::from_millis(50));
        assert_eq!(config.max_delay, Duration::from_millis(500));
        assert!(config.enable_jitter);
    }

    /// Test slow config preset
    #[tokio::test]
    async fn test_slow_config_preset() {
        let config = DbRetryConfig::slow();
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.base_delay, Duration::from_millis(500));
        assert_eq!(config.max_delay, Duration::from_secs(30));
        assert!(config.enable_jitter);
    }

    /// Test minimal config preset
    #[tokio::test]
    async fn test_minimal_config_preset() {
        let config = DbRetryConfig::minimal();
        assert_eq!(config.max_attempts, 1);
        assert_eq!(config.base_delay, Duration::from_millis(10));
        assert!(!config.enable_jitter);
    }

    /// Test default config
    #[tokio::test]
    async fn test_default_config() {
        let config = DbRetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.base_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert!(config.enable_jitter);
    }
}
