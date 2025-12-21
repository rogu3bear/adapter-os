//! Concurrent filesystem operations
//!
//! Provides file locking, atomic operations, and conflict resolution
//! for concurrent access to filesystem resources in AdapterOS.

pub mod atomic;
pub mod conflict;
pub mod locking;
pub mod manager;

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use tracing::debug;

/// Concurrent filesystem configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrentFsConfig {
    /// Enable file locking
    pub enable_file_locking: bool,
    /// Enable atomic operations
    pub enable_atomic_operations: bool,
    /// Enable conflict resolution
    pub enable_conflict_resolution: bool,
    /// Lock timeout duration
    pub lock_timeout: Duration,
    /// Retry attempts for failed operations
    pub retry_attempts: u32,
    /// Retry delay between attempts
    pub retry_delay: Duration,
    /// Maximum concurrent operations
    pub max_concurrent_operations: u32,
}

/// Concurrent filesystem manager
pub struct ConcurrentFsManager {
    config: ConcurrentFsConfig,
    lock_manager: locking::LockManager,
    atomic_manager: atomic::AtomicManager,
    conflict_resolver: conflict::ConflictResolver,
    operation_counter: Mutex<u32>,
}

impl ConcurrentFsManager {
    /// Create a new concurrent filesystem manager
    pub fn new(config: ConcurrentFsConfig) -> Result<Self> {
        let lock_manager = locking::LockManager::new(&config)?;
        let atomic_manager = atomic::AtomicManager::new(&config)?;
        let conflict_resolver = conflict::ConflictResolver::new(&config)?;

        Ok(Self {
            config,
            lock_manager,
            atomic_manager,
            conflict_resolver,
            operation_counter: Mutex::new(0),
        })
    }

    /// Acquire a read lock on a file
    pub async fn acquire_read_lock(&self, path: impl AsRef<Path>) -> Result<locking::FileLock> {
        self.check_concurrent_limit().await?;
        self.lock_manager.acquire_read_lock(path).await
    }

    /// Acquire a write lock on a file
    pub async fn acquire_write_lock(&self, path: impl AsRef<Path>) -> Result<locking::FileLock> {
        self.check_concurrent_limit().await?;
        self.lock_manager.acquire_write_lock(path).await
    }

    /// Perform an atomic file operation
    pub async fn perform_atomic_operation<F, R>(&self, operation: F) -> Result<R>
    where
        F: Fn() -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        self.check_concurrent_limit().await?;
        self.atomic_manager
            .perform_atomic_operation(operation)
            .await
    }

    /// Resolve a file conflict
    pub async fn resolve_conflict(
        &self,
        conflict: conflict::FileConflict,
    ) -> Result<conflict::ConflictResolution> {
        self.conflict_resolver.resolve_conflict(conflict).await
    }

    /// Check if we're within concurrent operation limits
    async fn check_concurrent_limit(&self) -> Result<()> {
        let mut counter = self.operation_counter.lock().await;
        if *counter >= self.config.max_concurrent_operations {
            return Err(AosError::Concurrency(
                "Maximum concurrent operations exceeded".to_string(),
            ));
        }
        *counter += 1;
        Ok(())
    }

    /// Release an operation counter
    pub async fn release_operation(&self) {
        let mut counter = self.operation_counter.lock().await;
        if *counter > 0 {
            *counter -= 1;
        }
    }

    /// Get current operation count
    pub async fn get_operation_count(&self) -> u32 {
        let counter = self.operation_counter.lock().await;
        *counter
    }

    /// Begin a tracked concurrent operation
    pub async fn begin_operation(self: &Arc<Self>) -> Result<ConcurrentOperationGuard> {
        self.check_concurrent_limit().await?;
        Ok(ConcurrentOperationGuard::new(self.clone()))
    }

    /// Get configuration
    pub fn config(&self) -> &ConcurrentFsConfig {
        &self.config
    }
}

impl Default for ConcurrentFsConfig {
    fn default() -> Self {
        Self {
            enable_file_locking: true,
            enable_atomic_operations: true,
            enable_conflict_resolution: true,
            lock_timeout: Duration::from_secs(30),
            retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
            max_concurrent_operations: 100,
        }
    }
}

/// File operation result
#[derive(Debug, Clone)]
pub struct FileOperationResult {
    /// Operation success
    pub success: bool,
    /// Operation duration
    pub duration: Duration,
    /// Error message if failed
    pub error: Option<String>,
    /// Retry count
    pub retry_count: u32,
}

/// Concurrent operation guard
pub struct ConcurrentOperationGuard {
    manager: Arc<ConcurrentFsManager>,
    start_time: SystemTime,
    retry_count: u32,
}

impl ConcurrentOperationGuard {
    /// Create a new concurrent operation guard
    pub fn new(manager: Arc<ConcurrentFsManager>) -> Self {
        Self {
            manager,
            start_time: SystemTime::now(),
            retry_count: 0,
        }
    }

    /// Increment retry count
    pub fn increment_retry(&mut self) {
        self.retry_count += 1;
    }

    /// Get retry count
    pub fn retry_count(&self) -> u32 {
        self.retry_count
    }

    /// Check if retry limit exceeded
    pub fn retry_limit_exceeded(&self) -> bool {
        self.retry_count >= self.manager.config.retry_attempts
    }
}

impl Drop for ConcurrentOperationGuard {
    fn drop(&mut self) {
        let duration = self.start_time.elapsed().unwrap_or(Duration::ZERO);

        let manager = Arc::clone(&self.manager);
        tokio::spawn(async move {
            manager.release_operation().await;
        });

        debug!(
            "Concurrent operation completed in {:?} (retries: {})",
            duration, self.retry_count
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_concurrent_fs_manager() -> Result<()> {
        let config = ConcurrentFsConfig::default();
        let manager = Arc::new(ConcurrentFsManager::new(config)?);

        // Test operation counting
        assert_eq!(manager.get_operation_count().await, 0);

        // Test concurrent limit
        let guard = manager.clone().begin_operation().await?;
        assert_eq!(manager.get_operation_count().await, 1);

        drop(guard);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert_eq!(manager.get_operation_count().await, 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_retry_limit() -> Result<()> {
        let config = ConcurrentFsConfig::default();
        let manager = Arc::new(ConcurrentFsManager::new(config)?);

        let mut guard = manager.clone().begin_operation().await?;

        // Test retry counting
        assert_eq!(guard.retry_count(), 0);
        assert!(!guard.retry_limit_exceeded());

        guard.increment_retry();
        assert_eq!(guard.retry_count(), 1);
        assert!(!guard.retry_limit_exceeded());

        // Increment to retry limit
        for _ in 0..manager.config.retry_attempts {
            guard.increment_retry();
        }
        assert!(guard.retry_limit_exceeded());

        Ok(())
    }
}
