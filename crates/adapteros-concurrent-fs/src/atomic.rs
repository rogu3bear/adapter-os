//! Atomic filesystem operations
//!
//! Implements atomic filesystem operations for safe concurrent access.

use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Atomic operation manager
pub struct AtomicManager {
    /// Configuration
    config: AtomicConfig,
    /// Active operations
    active_operations: Mutex<Vec<ActiveOperation>>,
}

/// Atomic operation configuration
#[derive(Debug, Clone)]
pub struct AtomicConfig {
    /// Enable atomic operations
    pub enabled: bool,
    /// Operation timeout
    pub operation_timeout: Duration,
    /// Retry attempts
    pub retry_attempts: u32,
    /// Retry delay
    pub retry_delay: Duration,
    /// Enable rollback on failure
    pub enable_rollback: bool,
}

/// Active operation information
#[derive(Debug, Clone)]
pub struct ActiveOperation {
    /// Operation ID
    pub id: String,
    /// Operation type
    pub operation_type: AtomicOperationType,
    /// File path
    pub path: PathBuf,
    /// Start time
    pub start_time: SystemTime,
    /// Status
    pub status: OperationStatus,
}

/// Atomic operation type
#[derive(Debug, Clone)]
pub enum AtomicOperationType {
    /// File write operation
    FileWrite,
    /// File move operation
    FileMove,
    /// File copy operation
    FileCopy,
    /// Directory operation
    DirectoryOperation,
    /// Complex operation
    ComplexOperation,
}

/// Operation status
#[derive(Debug, Clone)]
pub enum OperationStatus {
    /// Operation in progress
    InProgress,
    /// Operation completed successfully
    Completed,
    /// Operation failed
    Failed,
    /// Operation rolled back
    RolledBack,
}

impl AtomicManager {
    /// Create a new atomic manager
    pub fn new(config: &crate::ConcurrentFsConfig) -> Result<Self> {
        let atomic_config = AtomicConfig {
            enabled: config.enable_atomic_operations,
            operation_timeout: Duration::from_secs(30),
            retry_attempts: config.retry_attempts,
            retry_delay: config.retry_delay,
            enable_rollback: true,
        };

        Ok(Self {
            config: atomic_config,
            active_operations: Mutex::new(Vec::new()),
        })
    }

    /// Perform an atomic operation
    pub async fn perform_atomic_operation<F, R>(&self, operation: F) -> Result<R>
    where
        F: Fn() -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        if !self.config.enabled {
            return operation();
        }

        let operation_id = format!(
            "atomic_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        // Register operation
        self.register_operation(
            &operation_id,
            AtomicOperationType::ComplexOperation,
            PathBuf::new(),
        )
        .await?;

        // Perform operation with retry
        let mut last_error = None;
        for attempt in 0..=self.config.retry_attempts {
            match operation() {
                Ok(result) => {
                    self.complete_operation(&operation_id).await?;
                    debug!("Atomic operation {} completed successfully", operation_id);
                    return Ok(result);
                }
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.config.retry_attempts {
                        warn!(
                            "Atomic operation {} failed (attempt {}), retrying in {:?}",
                            operation_id,
                            attempt + 1,
                            self.config.retry_delay
                        );
                        sleep(self.config.retry_delay).await;
                    }
                }
            }
        }

        // Operation failed after all retries
        self.fail_operation(&operation_id).await?;
        Err(last_error
            .unwrap_or_else(|| AosError::Concurrency("Atomic operation failed".to_string())))
    }

    /// Perform atomic file write
    pub async fn atomic_file_write(&self, path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        let operation_id = format!(
            "write_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        // Register operation
        self.register_operation(&operation_id, AtomicOperationType::FileWrite, path.clone())
            .await?;

        // Create temporary file
        let temp_path = path.with_extension(format!(
            "{}.tmp.{}",
            path.extension().unwrap_or_default().to_string_lossy(),
            operation_id
        ));

        // Write to temporary file
        fs::write(&temp_path, data).await.map_err(|e| {
            AosError::Concurrency(format!("Failed to write to temporary file: {}", e))
        })?;

        // Atomic rename
        fs::rename(&temp_path, &path).await.map_err(|e| {
            // Clean up temporary file on failure
            let _ = std::fs::remove_file(&temp_path);
            AosError::Concurrency(format!("Failed to atomically rename file: {}", e))
        })?;

        self.complete_operation(&operation_id).await?;
        debug!("Atomic file write completed: {}", path.display());
        Ok(())
    }

    /// Perform atomic file move
    pub async fn atomic_file_move(
        &self,
        src: impl AsRef<Path>,
        dst: impl AsRef<Path>,
    ) -> Result<()> {
        let src = src.as_ref().to_path_buf();
        let dst = dst.as_ref().to_path_buf();
        let operation_id = format!(
            "move_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        // Register operation
        self.register_operation(&operation_id, AtomicOperationType::FileMove, dst.clone())
            .await?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                AosError::Concurrency(format!("Failed to create parent directory: {}", e))
            })?;
        }

        // Atomic move
        fs::rename(&src, &dst)
            .await
            .map_err(|e| AosError::Concurrency(format!("Failed to atomically move file: {}", e)))?;

        self.complete_operation(&operation_id).await?;
        debug!(
            "Atomic file move completed: {} -> {}",
            src.display(),
            dst.display()
        );
        Ok(())
    }

    /// Perform atomic file copy
    pub async fn atomic_file_copy(
        &self,
        src: impl AsRef<Path>,
        dst: impl AsRef<Path>,
    ) -> Result<()> {
        let src = src.as_ref().to_path_buf();
        let dst = dst.as_ref().to_path_buf();
        let operation_id = format!(
            "copy_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        // Register operation
        self.register_operation(&operation_id, AtomicOperationType::FileCopy, dst.clone())
            .await?;

        // Create parent directory if it doesn't exist
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                AosError::Concurrency(format!("Failed to create parent directory: {}", e))
            })?;
        }

        // Create temporary file
        let temp_path = dst.with_extension(format!(
            "{}.tmp.{}",
            dst.extension().unwrap_or_default().to_string_lossy(),
            operation_id
        ));

        // Copy to temporary file
        fs::copy(&src, &temp_path).await.map_err(|e| {
            AosError::Concurrency(format!("Failed to copy to temporary file: {}", e))
        })?;

        // Atomic rename
        fs::rename(&temp_path, &dst).await.map_err(|e| {
            // Clean up temporary file on failure
            let _ = std::fs::remove_file(&temp_path);
            AosError::Concurrency(format!("Failed to atomically rename copied file: {}", e))
        })?;

        self.complete_operation(&operation_id).await?;
        debug!(
            "Atomic file copy completed: {} -> {}",
            src.display(),
            dst.display()
        );
        Ok(())
    }

    /// Register an operation
    async fn register_operation(
        &self,
        operation_id: &str,
        operation_type: AtomicOperationType,
        path: PathBuf,
    ) -> Result<()> {
        let mut operations = self.active_operations.lock().await;

        let operation = ActiveOperation {
            id: operation_id.to_string(),
            operation_type,
            path,
            start_time: SystemTime::now(),
            status: OperationStatus::InProgress,
        };

        operations.push(operation);
        debug!("Registered atomic operation: {}", operation_id);
        Ok(())
    }

    /// Complete an operation
    async fn complete_operation(&self, operation_id: &str) -> Result<()> {
        let mut operations = self.active_operations.lock().await;

        if let Some(operation) = operations.iter_mut().find(|op| op.id == operation_id) {
            operation.status = OperationStatus::Completed;
            debug!("Completed atomic operation: {}", operation_id);
        }

        Ok(())
    }

    /// Fail an operation
    async fn fail_operation(&self, operation_id: &str) -> Result<()> {
        let mut operations = self.active_operations.lock().await;

        if let Some(operation) = operations.iter_mut().find(|op| op.id == operation_id) {
            operation.status = OperationStatus::Failed;
            error!("Failed atomic operation: {}", operation_id);
        }

        Ok(())
    }

    /// Get active operations
    pub async fn get_active_operations(&self) -> Vec<ActiveOperation> {
        let operations = self.active_operations.lock().await;
        operations.clone()
    }

    /// Clean up completed operations
    pub async fn cleanup_completed_operations(&self) -> Result<()> {
        let mut operations = self.active_operations.lock().await;
        let now = SystemTime::now();

        operations.retain(|op| {
            match op.status {
                OperationStatus::InProgress => {
                    // Keep in-progress operations that haven't timed out
                    now.duration_since(op.start_time).unwrap_or(Duration::ZERO)
                        < self.config.operation_timeout
                }
                OperationStatus::Completed
                | OperationStatus::Failed
                | OperationStatus::RolledBack => {
                    // Remove completed/failed operations after a delay
                    now.duration_since(op.start_time).unwrap_or(Duration::ZERO)
                        > Duration::from_secs(60)
                }
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_atomic_file_write() -> Result<()> {
        let config = crate::ConcurrentFsConfig::default();
        let manager = AtomicManager::new(&config)?;

        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test atomic file write
        manager
            .atomic_file_write(&test_file, b"hello world")
            .await?;

        // Verify file was written
        let content = fs::read_to_string(&test_file).await?;
        assert_eq!(content, "hello world");

        Ok(())
    }

    #[tokio::test]
    async fn test_atomic_file_move() -> Result<()> {
        let config = crate::ConcurrentFsConfig::default();
        let manager = AtomicManager::new(&config)?;

        let temp_dir = TempDir::new()?;
        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");

        // Create source file
        fs::write(&src_file, b"hello world").await?;

        // Test atomic file move
        manager.atomic_file_move(&src_file, &dst_file).await?;

        // Verify file was moved
        assert!(!src_file.exists());
        assert!(dst_file.exists());

        let content = fs::read_to_string(&dst_file).await?;
        assert_eq!(content, "hello world");

        Ok(())
    }

    #[tokio::test]
    async fn test_atomic_file_copy() -> Result<()> {
        let config = crate::ConcurrentFsConfig::default();
        let manager = AtomicManager::new(&config)?;

        let temp_dir = TempDir::new()?;
        let src_file = temp_dir.path().join("src.txt");
        let dst_file = temp_dir.path().join("dst.txt");

        // Create source file
        fs::write(&src_file, b"hello world").await?;

        // Test atomic file copy
        manager.atomic_file_copy(&src_file, &dst_file).await?;

        // Verify file was copied
        assert!(src_file.exists());
        assert!(dst_file.exists());

        let src_content = fs::read_to_string(&src_file).await?;
        let dst_content = fs::read_to_string(&dst_file).await?;
        assert_eq!(src_content, dst_content);
        assert_eq!(src_content, "hello world");

        Ok(())
    }
}
