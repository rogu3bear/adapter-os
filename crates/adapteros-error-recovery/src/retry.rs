//! Retry mechanisms
//!
//! Implements automatic retry mechanisms for failed operations.

use crate::{ErrorRecoveryConfig, RecoveryResult};
use adapteros_core::{AosError, Result};
use std::future::Future;
use std::path::Path;
use std::time::{Duration, SystemTime};
use tokio::time::{sleep, timeout};
use tracing::{info, warn};

/// Retry manager
pub struct RetryManager {
    config: ErrorRecoveryConfig,
    retry_history:
        std::sync::Arc<tokio::sync::Mutex<std::collections::HashMap<String, RetryRecord>>>,
}

/// Retry record
#[derive(Debug, Clone)]
pub struct RetryRecord {
    /// Operation ID
    pub operation_id: String,
    /// File path
    pub path: String,
    /// Retry count
    pub retry_count: u32,
    /// Last retry time
    pub last_retry_time: SystemTime,
    /// Total retry duration
    pub total_duration: Duration,
    /// Success flag
    pub success: bool,
    /// Last error message (if any)
    pub last_error: Option<String>,
}

impl RetryManager {
    /// Create a new retry manager
    pub fn new(config: &ErrorRecoveryConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            retry_history: std::sync::Arc::new(tokio::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
        })
    }

    /// Retry an operation identified by a key using a custom async action.
    pub async fn retry_with<F, Fut>(&self, key: &str, mut operation: F) -> Result<RecoveryResult>
    where
        F: FnMut() -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        if !self.config.enable_automatic_retry {
            return Ok(RecoveryResult::Failed);
        }

        let key = key.to_string();

        let mut record = {
            let history = self.retry_history.lock().await;
            history.get(&key).cloned()
        }
        .unwrap_or_else(|| RetryRecord {
            operation_id: format!(
                "retry_{}",
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            path: key.clone(),
            retry_count: 0,
            last_retry_time: SystemTime::now(),
            total_duration: Duration::ZERO,
            success: false,
            last_error: None,
        });

        if record.retry_count >= self.config.max_retry_attempts {
            warn!("Maximum retry attempts exceeded for {}", key);
            return Ok(RecoveryResult::Failed);
        }

        let retry_delay = self.calculate_retry_delay(record.retry_count);
        sleep(retry_delay).await;

        let start_time = SystemTime::now();
        let result = operation().await;
        let duration = start_time.elapsed().unwrap_or(Duration::ZERO);

        record.retry_count += 1;
        record.last_retry_time = start_time;
        record.total_duration += duration;
        match &result {
            Ok(_) => {
                record.success = true;
                record.last_error = None;
            }
            Err(err) => {
                record.success = false;
                record.last_error = Some(err.to_string());
            }
        }

        {
            let mut history = self.retry_history.lock().await;
            history.insert(key.clone(), record.clone());
        }

        match &result {
            Ok(_) => {
                info!(
                    "Retry successful for {} after {} attempts",
                    key, record.retry_count
                );
                Ok(RecoveryResult::Success)
            }
            Err(e) => {
                warn!(
                    "Retry failed for {} after {} attempts: {}",
                    key, record.retry_count, e
                );
                Ok(RecoveryResult::Failed)
            }
        }
    }

    /// Retry an operation
    pub async fn retry_operation(&self, path: &Path) -> Result<RecoveryResult> {
        let key = path.to_string_lossy().to_string();
        let path_buf = path.to_path_buf();

        let this = self;
        self.retry_with(&key, move || {
            let path_buf = path_buf.clone();
            async move { this.perform_retry_operation(&path_buf).await }
        })
        .await
    }

    /// Perform the actual retry operation
    async fn perform_retry_operation(&self, path: &Path) -> Result<()> {
        if path.exists() {
            tokio::fs::metadata(path)
                .await
                .map_err(|e| AosError::Recovery(format!("Path not accessible: {}", e)))?;
            Ok(())
        } else {
            Err(AosError::Recovery("Path does not exist".to_string()))
        }
    }

    /// Calculate retry delay with exponential backoff
    fn calculate_retry_delay(&self, retry_count: u32) -> Duration {
        let base_delay = self.config.retry_delay;
        let multiplier = self.config.backoff_multiplier;

        let delay_ms = base_delay.as_millis() as f64 * multiplier.powi(retry_count as i32);
        let delay_ms = delay_ms.min(self.config.max_retry_delay.as_millis() as f64) as u64;

        Duration::from_millis(delay_ms)
    }

    /// Get retry statistics
    pub async fn get_retry_statistics(&self) -> RetryStatistics {
        let history = self.retry_history.lock().await;
        let total_retries = history.len();
        let successful_retries = history.values().filter(|record| record.success).count();
        let failed_retries = history.values().filter(|record| !record.success).count();

        let success_rate = if total_retries > 0 {
            successful_retries as f32 / total_retries as f32
        } else {
            0.0
        };

        RetryStatistics {
            total_retries,
            successful_retries,
            failed_retries,
            success_rate,
        }
    }

    /// Clear retry history
    pub async fn clear_retry_history(&self) {
        self.retry_history.lock().await.clear();
    }

    /// Get retry record for a path
    pub async fn get_retry_record(&self, path: &str) -> Option<RetryRecord> {
        self.retry_history.lock().await.get(path).cloned()
    }
}

/// Retry statistics
#[derive(Debug, Clone)]
pub struct RetryStatistics {
    /// Total number of retry attempts
    pub total_retries: usize,
    /// Number of successful retries
    pub successful_retries: usize,
    /// Number of failed retries
    pub failed_retries: usize,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f32,
}

/// Retry operation with timeout
pub async fn retry_with_timeout<F, T>(
    operation: F,
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
    timeout_duration: Duration,
) -> Result<T>
where
    F: Fn() -> Result<T> + Send + Sync + 'static,
    T: Send + 'static,
{
    let mut attempt = 0;
    let mut delay = base_delay;

    while attempt < max_attempts {
        let result = timeout(timeout_duration, async { operation() }).await;

        match result {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(e)) => {
                attempt += 1;
                if attempt >= max_attempts {
                    return Err(e);
                }

                warn!(
                    "Retry attempt {} failed: {}, retrying in {:?}",
                    attempt, e, delay
                );
                sleep(delay).await;

                // Exponential backoff
                delay = std::cmp::min(delay * 2, max_delay);
            }
            Err(_) => {
                attempt += 1;
                if attempt >= max_attempts {
                    return Err(AosError::Timeout {
                        duration: timeout_duration,
                    });
                }

                warn!(
                    "Retry attempt {} timed out, retrying in {:?}",
                    attempt, delay
                );
                sleep(delay).await;

                // Exponential backoff
                delay = std::cmp::min(delay * 2, max_delay);
            }
        }
    }

    // This should never be reached due to the loop logic above
    unreachable!("Maximum retry attempts exceeded")
}

/// Retry operation with custom error handling
pub async fn retry_with_error_handler<F, T, E>(
    operation: F,
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
    error_handler: impl Fn(&E, u32) -> bool + Send + Sync,
) -> std::result::Result<T, E>
where
    F: Fn() -> std::result::Result<T, E> + Send + 'static,
    T: Send + 'static,
    E: Send + Sync + 'static + std::fmt::Debug,
{
    let mut attempt = 0;
    let mut delay = base_delay;

    while attempt < max_attempts {
        match operation() {
            Ok(value) => return Ok(value),
            Err(e) => {
                attempt += 1;

                // Check if we should retry based on error
                if !error_handler(&e, attempt) {
                    return Err(e);
                }

                if attempt >= max_attempts {
                    return Err(e);
                }

                warn!(
                    "Retry attempt {} failed: {:?}, retrying in {:?}",
                    attempt, e, delay
                );
                sleep(delay).await;

                // Exponential backoff
                delay = std::cmp::min(delay * 2, max_delay);
            }
        }
    }

    // This should never be reached due to the loop logic above
    unreachable!("Maximum retry attempts exceeded")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_retry_manager() -> Result<()> {
        let mut config = ErrorRecoveryConfig::default();
        config.retry_delay = Duration::from_millis(5);
        config.max_retry_delay = Duration::from_millis(20);
        let manager = RetryManager::new(&config)?;

        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        let path_str = test_file.to_string_lossy().to_string();

        // Test retry operation on non-existent file
        let result = manager.retry_operation(&test_file).await?;
        assert_eq!(
            result,
            RecoveryResult::Failed,
            "Should fail for non-existent file"
        );

        // Verify retry was recorded
        let stats = manager.get_retry_statistics().await;
        assert_eq!(stats.total_retries, 1, "Should have 1 retry recorded");
        assert_eq!(stats.failed_retries, 1, "Should have 1 failed retry");
        assert_eq!(stats.success_rate, 0.0, "Success rate should be 0");

        let failed_record = manager.get_retry_record(&path_str).await.unwrap();
        assert!(!failed_record.success);
        assert!(
            failed_record.last_error.is_some(),
            "Last error should be recorded on failure"
        );

        // Create the file and retry
        std::fs::write(&test_file, "test content")?;
        let result = manager.retry_operation(&test_file).await?;
        assert_eq!(
            result,
            RecoveryResult::Success,
            "Should succeed for existing file"
        );

        // Verify updated statistics
        let stats = manager.get_retry_statistics().await;
        assert_eq!(
            stats.total_retries, 1,
            "Should still have 1 entry (same path)"
        );
        assert_eq!(
            stats.successful_retries, 1,
            "Should have 1 successful retry"
        );
        assert_eq!(stats.success_rate, 1.0, "Success rate should be 1.0");

        // Test get_retry_record
        let record = manager.get_retry_record(&path_str).await;
        assert!(record.is_some(), "Should have retry record for path");
        let record = record.unwrap();
        assert_eq!(record.path, path_str);
        assert!(record.success, "Record should show success");
        assert!(
            record.last_error.is_none(),
            "Last error should be cleared after success"
        );
        assert_eq!(record.retry_count, 2, "Should have 2 retry attempts total");

        // Test clear_retry_history
        manager.clear_retry_history().await;
        let stats = manager.get_retry_statistics().await;
        assert_eq!(stats.total_retries, 0, "Should have 0 retries after clear");

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_with_timeout() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test retry with timeout
        let result = retry_with_timeout(
            move || {
                if test_file.exists() {
                    Ok("success")
                } else {
                    Err(AosError::Retry("File not found".to_string()))
                }
            },
            3,
            Duration::from_millis(10),
            Duration::from_millis(100),
            Duration::from_millis(50),
        )
        .await;

        // Should fail because file doesn't exist
        assert!(result.is_err());

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_with_error_handler() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test retry with error handler
        let result = retry_with_error_handler(
            move || {
                if test_file.exists() {
                    Ok("success")
                } else {
                    Err(AosError::Retry("File not found".to_string()))
                }
            },
            3,
            Duration::from_millis(10),
            Duration::from_millis(100),
            |_e, attempt| attempt < 2, // Only retry twice
        )
        .await;

        // Should fail because file doesn't exist
        assert!(result.is_err());

        Ok(())
    }
}
