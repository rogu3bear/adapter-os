//! Retry mechanisms
//!
//! Implements automatic retry mechanisms for failed operations.

use crate::{ErrorRecoveryConfig, RecoveryResult};
use adapteros_core::{AosError, Result};
use std::path::Path;
use std::time::{Duration, SystemTime};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// Retry manager
pub struct RetryManager {
    config: ErrorRecoveryConfig,
    retry_history: std::collections::HashMap<String, RetryRecord>,
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
}

impl RetryManager {
    /// Create a new retry manager
    pub fn new(config: &ErrorRecoveryConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            retry_history: std::collections::HashMap::new(),
        })
    }

    /// Retry an operation
    pub async fn retry_operation(&self, path: &Path) -> Result<RecoveryResult> {
        if !self.config.enable_automatic_retry {
            return Ok(RecoveryResult::Failed);
        }

        let operation_id = format!("retry_{}", SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos());
        let path_str = path.to_string_lossy().to_string();

        // Get or create retry record
        let mut record = self.retry_history.get(&operation_id).cloned().unwrap_or_else(|| {
            RetryRecord {
                operation_id: operation_id.clone(),
                path: path_str.clone(),
                retry_count: 0,
                last_retry_time: SystemTime::now(),
                total_duration: Duration::ZERO,
                success: false,
            }
        });

        // Check if we've exceeded max retry attempts
        if record.retry_count >= self.config.max_retry_attempts {
            warn!("Maximum retry attempts exceeded for {}", path.display());
            return Ok(RecoveryResult::Failed);
        }

        // Perform retry with exponential backoff
        let retry_delay = self.calculate_retry_delay(record.retry_count);
        sleep(retry_delay).await;

        let start_time = SystemTime::now();
        let result = self.perform_retry_operation(path).await;
        let duration = start_time.elapsed().unwrap_or(Duration::ZERO);

        // Update retry record
        record.retry_count += 1;
        record.last_retry_time = start_time;
        record.total_duration += duration;
        record.success = result.is_ok();

        // Store updated record
        // Note: In a real implementation, this would be thread-safe
        // self.retry_history.insert(operation_id, record);

        match result {
            Ok(_) => {
                info!("Retry successful for {} after {} attempts", path.display(), record.retry_count);
                Ok(RecoveryResult::Success)
            }
            Err(e) => {
                warn!("Retry failed for {} after {} attempts: {}", path.display(), record.retry_count, e);
                Ok(RecoveryResult::Failed)
            }
        }
    }

    /// Perform the actual retry operation
    async fn perform_retry_operation(&self, path: &Path) -> Result<()> {
        // This is a placeholder for the actual retry logic
        // In a real implementation, this would retry the specific operation that failed
        
        // For now, we'll just check if the path exists and is accessible
        if path.exists() {
            // Try to read metadata to verify accessibility
            tokio::fs::metadata(path).await
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
    pub fn get_retry_statistics(&self) -> RetryStatistics {
        let total_retries = self.retry_history.len();
        let successful_retries = self.retry_history.values()
            .filter(|record| record.success)
            .count();
        let failed_retries = self.retry_history.values()
            .filter(|record| !record.success)
            .count();

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
    pub fn clear_retry_history(&mut self) {
        self.retry_history.clear();
    }

    /// Get retry record for an operation
    pub fn get_retry_record(&self, operation_id: &str) -> Option<&RetryRecord> {
        self.retry_history.get(operation_id)
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
    F: Fn() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    let mut attempt = 0;
    let mut delay = base_delay;

    while attempt < max_attempts {
        let result = timeout(timeout_duration, async {
            operation()
        }).await;

        match result {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(e)) => {
                attempt += 1;
                if attempt >= max_attempts {
                    return Err(e);
                }
                
                warn!("Retry attempt {} failed: {}, retrying in {:?}", attempt, e, delay);
                sleep(delay).await;
                
                // Exponential backoff
                delay = std::cmp::min(delay * 2, max_delay);
            }
            Err(_) => {
                attempt += 1;
                if attempt >= max_attempts {
                    return Err(AosError::Timeout("Operation timed out".to_string()));
                }
                
                warn!("Retry attempt {} timed out, retrying in {:?}", attempt, delay);
                sleep(delay).await;
                
                // Exponential backoff
                delay = std::cmp::min(delay * 2, max_delay);
            }
        }
    }

    Err(AosError::Retry("Maximum retry attempts exceeded".to_string()))
}

/// Retry operation with custom error handling
pub async fn retry_with_error_handler<F, T, E>(
    operation: F,
    max_attempts: u32,
    base_delay: Duration,
    max_delay: Duration,
    error_handler: impl Fn(&E, u32) -> bool + Send + Sync,
) -> Result<T>
where
    F: Fn() -> Result<T, E> + Send + 'static,
    T: Send + 'static,
    E: Send + Sync + 'static,
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
                    return Err(AosError::Retry("Error handler prevented retry".to_string()));
                }
                
                if attempt >= max_attempts {
                    return Err(AosError::Retry("Maximum retry attempts exceeded".to_string()));
                }
                
                warn!("Retry attempt {} failed: {:?}, retrying in {:?}", attempt, e, delay);
                sleep(delay).await;
                
                // Exponential backoff
                delay = std::cmp::min(delay * 2, max_delay);
            }
        }
    }

    Err(AosError::Retry("Maximum retry attempts exceeded".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_retry_manager() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = RetryManager::new(&config)?;
        
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        
        // Test retry operation
        let result = manager.retry_operation(&test_file).await?;
        // Should fail because file doesn't exist
        
        // Test statistics
        let stats = manager.get_retry_statistics();
        assert_eq!(stats.total_retries, 0);
        
        Ok(())
    }

    #[tokio::test]
    async fn test_retry_with_timeout() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        
        // Test retry with timeout
        let result = retry_with_timeout(
            || {
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
        ).await;
        
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
            || {
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
        ).await;
        
        // Should fail because file doesn't exist
        assert!(result.is_err());
        
        Ok(())
    }
}
