//! Error recovery and corruption detection
//!
//! Provides error recovery mechanisms, corruption detection, and automatic
//! retry mechanisms for adapterOS filesystem operations.

pub mod corruption;
pub mod recovery;
pub mod retry;
pub mod validation;

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Error recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorRecoveryConfig {
    /// Enable error recovery
    pub enabled: bool,
    /// Enable corruption detection
    pub enable_corruption_detection: bool,
    /// Enable automatic retry
    pub enable_automatic_retry: bool,
    /// Maximum retry attempts
    pub max_retry_attempts: u32,
    /// Retry delay between attempts
    pub retry_delay: Duration,
    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
    /// Maximum retry delay
    pub max_retry_delay: Duration,
    /// Enable partial recovery
    pub enable_partial_recovery: bool,
    /// Enable backup and restore
    pub enable_backup_restore: bool,
    /// Backup retention count
    pub backup_retention_count: u32,
}

/// Error recovery manager
pub struct ErrorRecoveryManager {
    config: ErrorRecoveryConfig,
    corruption_detector: corruption::CorruptionDetector,
    recovery_engine: recovery::RecoveryEngine,
    retry_manager: retry::RetryManager,
    validation_engine: validation::ValidationEngine,
    recovery_history: RwLock<Vec<RecoveryRecord>>,
}

/// Recovery record
#[derive(Debug, Clone)]
pub struct RecoveryRecord {
    /// Recovery ID
    pub id: String,
    /// File path
    pub path: PathBuf,
    /// Error type
    pub error_type: ErrorType,
    /// Recovery strategy
    pub strategy: RecoveryStrategy,
    /// Recovery result
    pub result: RecoveryResult,
    /// Timestamp
    pub timestamp: SystemTime,
    /// Duration
    pub duration: Duration,
    /// Retry count
    pub retry_count: u32,
}

/// Error type
#[derive(Debug, Clone)]
pub enum ErrorType {
    /// File corruption
    FileCorruption,
    /// Directory corruption
    DirectoryCorruption,
    /// Permission error
    PermissionError,
    /// Disk space error
    DiskSpaceError,
    /// Network error
    NetworkError,
    /// Timeout error
    TimeoutError,
    /// Lock error
    LockError,
    /// Unknown error
    Unknown,
}

/// Recovery strategy
#[derive(Debug, Clone)]
pub enum RecoveryStrategy {
    /// Retry operation
    Retry,
    /// Restore from backup
    RestoreFromBackup,
    /// Recreate file
    RecreateFile,
    /// Recreate directory
    RecreateDirectory,
    /// Skip operation
    Skip,
    /// Manual intervention required
    Manual,
}

/// Recovery result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryResult {
    /// Recovery successful
    Success,
    /// Recovery partially successful
    PartialSuccess,
    /// Recovery failed
    Failed,
    /// Recovery skipped
    Skipped,
    /// Manual intervention required
    ManualRequired,
}

impl ErrorRecoveryManager {
    /// Create a new error recovery manager
    pub fn new(config: ErrorRecoveryConfig) -> Result<Self> {
        let corruption_detector = corruption::CorruptionDetector::new()?;
        let recovery_engine = recovery::RecoveryEngine::new(&config)?;
        let retry_manager = retry::RetryManager::new(&config)?;
        let validation_engine = validation::ValidationEngine::new()?;

        Ok(Self {
            config,
            corruption_detector,
            recovery_engine,
            retry_manager,
            validation_engine,
            recovery_history: RwLock::new(Vec::new()),
        })
    }

    /// Handle an error with automatic recovery
    pub async fn handle_error(&self, error: AosError, path: &Path) -> Result<()> {
        if !self.config.enabled {
            return Err(error);
        }

        let error_type = self.classify_error(&error);
        let recovery_id = format!(
            "recovery_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );

        // Determine recovery strategy
        let strategy = self.determine_recovery_strategy(&error_type, path).await?;

        // Attempt recovery
        let start_time = SystemTime::now();
        let result = self.attempt_recovery(&strategy, path).await?;
        let duration = start_time.elapsed().unwrap_or(Duration::ZERO);

        // Record recovery attempt
        let record = RecoveryRecord {
            id: recovery_id,
            path: path.to_path_buf(),
            error_type,
            strategy,
            result: result.clone(),
            timestamp: start_time,
            duration,
            retry_count: 0, // Would be tracked by retry manager
        };

        self.record_recovery(record).await?;

        // Return appropriate result
        match result {
            RecoveryResult::Success => Ok(()),
            RecoveryResult::PartialSuccess => {
                warn!("Partial recovery successful for {}", path.display());
                Ok(())
            }
            RecoveryResult::Failed => Err(error),
            RecoveryResult::Skipped => {
                info!("Recovery skipped for {}", path.display());
                Ok(())
            }
            RecoveryResult::ManualRequired => {
                error!("Manual intervention required for {}", path.display());
                Err(AosError::Io(
                    "Recovery: Manual intervention required".to_string(),
                ))
            }
        }
    }

    /// Classify an error type with more granular IO error detection
    fn classify_error(&self, error: &AosError) -> ErrorType {
        match error {
            AosError::Io(msg) => {
                let msg_lower = msg.to_lowercase();
                if msg_lower.contains("corrupt")
                    || msg_lower.contains("invalid data")
                    || msg_lower.contains("bad format")
                {
                    ErrorType::FileCorruption
                } else if msg_lower.contains("permission")
                    || msg_lower.contains("access denied")
                    || msg_lower.contains("operation not permitted")
                {
                    ErrorType::PermissionError
                } else if msg_lower.contains("no space")
                    || msg_lower.contains("disk full")
                    || msg_lower.contains("quota exceeded")
                {
                    ErrorType::DiskSpaceError
                } else if msg_lower.contains("not found")
                    || msg_lower.contains("no such file")
                    || msg_lower.contains("does not exist")
                {
                    // Missing files are not corruption - likely a logic error or race condition
                    ErrorType::Unknown
                } else if msg_lower.contains("lock")
                    || msg_lower.contains("busy")
                    || msg_lower.contains("in use")
                {
                    ErrorType::LockError
                } else {
                    // Default IO errors that don't match specific patterns
                    // could be corruption, hardware issues, etc.
                    ErrorType::FileCorruption
                }
            }
            AosError::Timeout { duration: _ } => ErrorType::TimeoutError,
            AosError::Network(_) => ErrorType::NetworkError,
            AosError::Authz(_) => ErrorType::PermissionError,
            AosError::ResourceExhaustion(_) => ErrorType::DiskSpaceError,
            _ => ErrorType::Unknown,
        }
    }

    /// Determine recovery strategy
    async fn determine_recovery_strategy(
        &self,
        error_type: &ErrorType,
        path: &Path,
    ) -> Result<RecoveryStrategy> {
        // Check if file exists and is corrupted
        if path.exists()
            && self.config.enable_corruption_detection
            && self.corruption_detector.is_corrupted(path).await?
        {
            return Ok(RecoveryStrategy::RestoreFromBackup);
        }

        // Determine strategy based on error type
        match error_type {
            ErrorType::FileCorruption => {
                if self.config.enable_backup_restore {
                    Ok(RecoveryStrategy::RestoreFromBackup)
                } else {
                    Ok(RecoveryStrategy::RecreateFile)
                }
            }
            ErrorType::DirectoryCorruption => {
                if self.config.enable_backup_restore {
                    Ok(RecoveryStrategy::RestoreFromBackup)
                } else {
                    Ok(RecoveryStrategy::RecreateDirectory)
                }
            }
            ErrorType::PermissionError => Ok(RecoveryStrategy::Manual),
            ErrorType::DiskSpaceError => Ok(RecoveryStrategy::Manual),
            ErrorType::NetworkError => Ok(RecoveryStrategy::Retry),
            ErrorType::TimeoutError => Ok(RecoveryStrategy::Retry),
            ErrorType::LockError => Ok(RecoveryStrategy::Retry),
            ErrorType::Unknown => Ok(RecoveryStrategy::Manual),
        }
    }

    /// Attempt recovery using specified strategy
    async fn attempt_recovery(
        &self,
        strategy: &RecoveryStrategy,
        path: &Path,
    ) -> Result<RecoveryResult> {
        match strategy {
            RecoveryStrategy::Retry => self.retry_manager.retry_operation(path).await,
            RecoveryStrategy::RestoreFromBackup => {
                self.recovery_engine.restore_from_backup(path).await
            }
            RecoveryStrategy::RecreateFile => self.recovery_engine.recreate_file(path).await,
            RecoveryStrategy::RecreateDirectory => {
                self.recovery_engine.recreate_directory(path).await
            }
            RecoveryStrategy::Skip => Ok(RecoveryResult::Skipped),
            RecoveryStrategy::Manual => Ok(RecoveryResult::ManualRequired),
        }
    }

    /// Record recovery attempt
    async fn record_recovery(&self, record: RecoveryRecord) -> Result<()> {
        let mut history = self.recovery_history.write().await;

        // Add to history
        history.push(record.clone());

        // Trim history if too large
        if history.len() > 1000 {
            let target_len = history.len() - 1000;
            history.drain(0..target_len);
        }

        debug!("Recorded recovery: {} -> {:?}", record.id, record.result);
        Ok(())
    }

    /// Get recovery history
    pub async fn get_recovery_history(&self) -> Vec<RecoveryRecord> {
        let history = self.recovery_history.read().await;
        history.clone()
    }

    /// Get recovery statistics
    pub async fn get_recovery_statistics(&self) -> RecoveryStatistics {
        let history = self.recovery_history.read().await;

        let total_recoveries = history.len();
        let successful_recoveries = history
            .iter()
            .filter(|r| matches!(r.result, RecoveryResult::Success))
            .count();
        let failed_recoveries = history
            .iter()
            .filter(|r| matches!(r.result, RecoveryResult::Failed))
            .count();
        let partial_recoveries = history
            .iter()
            .filter(|r| matches!(r.result, RecoveryResult::PartialSuccess))
            .count();

        let success_rate = if total_recoveries > 0 {
            successful_recoveries as f32 / total_recoveries as f32
        } else {
            0.0
        };

        RecoveryStatistics {
            total_recoveries,
            successful_recoveries,
            failed_recoveries,
            partial_recoveries,
            success_rate,
        }
    }

    /// Validate file integrity
    pub async fn validate_file_integrity(&mut self, path: &Path) -> Result<bool> {
        self.validation_engine.validate_file(path).await
    }

    /// Validate directory integrity
    pub async fn validate_directory_integrity(&self, path: &Path) -> Result<bool> {
        self.validation_engine.validate_directory(path).await
    }

    /// Get configuration
    pub fn config(&self) -> &ErrorRecoveryConfig {
        &self.config
    }
}

/// Recovery statistics
#[derive(Debug, Clone)]
pub struct RecoveryStatistics {
    /// Total number of recovery attempts
    pub total_recoveries: usize,
    /// Number of successful recoveries
    pub successful_recoveries: usize,
    /// Number of failed recoveries
    pub failed_recoveries: usize,
    /// Number of partial recoveries
    pub partial_recoveries: usize,
    /// Success rate (0.0 to 1.0)
    pub success_rate: f32,
}

impl Default for ErrorRecoveryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_corruption_detection: true,
            enable_automatic_retry: true,
            max_retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
            backoff_multiplier: 2.0,
            max_retry_delay: Duration::from_secs(30),
            enable_partial_recovery: true,
            enable_backup_restore: true,
            backup_retention_count: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_platform::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> Result<TempDir> {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root)?;
        Ok(TempDir::new_in(&root)?)
    }

    #[tokio::test]
    async fn test_error_recovery_manager() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test error handling
        let error = AosError::Io("Test error".to_string());
        let result = manager.handle_error(error, &test_file).await;

        // Test statistics
        let stats = manager.get_recovery_statistics().await;
        assert_eq!(stats.total_recoveries, 1);
        if result.is_ok() {
            assert_eq!(stats.successful_recoveries, 1);
        } else {
            assert_eq!(stats.failed_recoveries, 1);
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_recovery_statistics() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = ErrorRecoveryManager::new(config)?;

        let stats = manager.get_recovery_statistics().await;
        assert_eq!(stats.total_recoveries, 0);
        assert_eq!(stats.successful_recoveries, 0);
        assert_eq!(stats.failed_recoveries, 0);
        assert_eq!(stats.partial_recoveries, 0);
        assert_eq!(stats.success_rate, 0.0);

        Ok(())
    }
}
