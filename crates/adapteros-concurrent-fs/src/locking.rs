//! File locking for concurrent access
//!
//! Implements file locking mechanisms for safe concurrent access to files.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// File lock manager
pub struct LockManager {
    /// File locks
    locks: Arc<RwLock<HashMap<PathBuf, Arc<FileLockInfo>>>>,
    /// Configuration
    config: LockConfig,
}

/// File lock information
#[derive(Debug)]
pub struct FileLockInfo {
    /// Path being locked
    path: PathBuf,
    /// Lock type
    lock_type: LockType,
    /// Lock holder
    holder: String,
    /// Lock timestamp
    timestamp: SystemTime,
    /// Read lock count
    read_count: Mutex<u32>,
    /// Write lock flag
    write_locked: Mutex<bool>,
}

/// Lock type
#[derive(Debug, Clone, PartialEq)]
pub enum LockType {
    /// Read lock (shared)
    Read,
    /// Write lock (exclusive)
    Write,
}

/// Lock configuration
#[derive(Debug, Clone)]
pub struct LockConfig {
    /// Lock timeout duration
    pub timeout: Duration,
    /// Enable deadlock detection
    pub enable_deadlock_detection: bool,
    /// Deadlock detection interval
    pub deadlock_detection_interval: Duration,
    /// Maximum lock wait time
    pub max_wait_time: Duration,
}

/// File lock handle
pub struct FileLock {
    /// Lock information
    info: Arc<FileLockInfo>,
    /// Lock manager
    manager: Arc<LockManager>,
    /// Lock type
    lock_type: LockType,
}

impl LockManager {
    /// Create a new lock manager
    pub fn new(config: &crate::ConcurrentFsConfig) -> Result<Self> {
        let lock_config = LockConfig {
            timeout: config.lock_timeout,
            enable_deadlock_detection: true,
            deadlock_detection_interval: Duration::from_secs(5),
            max_wait_time: Duration::from_secs(60),
        };

        Ok(Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
            config: lock_config,
        })
    }

    /// Acquire a read lock on a file
    pub async fn acquire_read_lock(&self, path: impl AsRef<Path>) -> Result<FileLock> {
        let path = path.as_ref().to_path_buf();
        let holder = format!(
            "read_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        // Try to acquire lock with timeout
        let result = timeout(
            self.config.max_wait_time,
            self.acquire_read_lock_internal(&path, &holder),
        )
        .await;

        match result {
            Ok(lock) => lock,
            Err(_) => Err(AosError::Concurrency(format!(
                "Timeout waiting for read lock on {}",
                path.display()
            ))),
        }
    }

    /// Acquire a write lock on a file
    pub async fn acquire_write_lock(&self, path: impl AsRef<Path>) -> Result<FileLock> {
        let path = path.as_ref().to_path_buf();
        let holder = format!(
            "write_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        // Try to acquire lock with timeout
        let result = timeout(
            self.config.max_wait_time,
            self.acquire_write_lock_internal(&path, &holder),
        )
        .await;

        match result {
            Ok(lock) => lock,
            Err(_) => Err(AosError::Concurrency(format!(
                "Timeout waiting for write lock on {}",
                path.display()
            ))),
        }
    }

    /// Internal method to acquire read lock
    async fn acquire_read_lock_internal(&self, path: &PathBuf, holder: &str) -> Result<FileLock> {
        let mut locks = self.locks.write().await;

        // Check if file is already locked for writing
        if let Some(lock_info) = locks.get(path) {
            let write_locked = *lock_info.write_locked.lock().await;
            if write_locked {
                // Wait for write lock to be released
                drop(locks);
                sleep(Duration::from_millis(10)).await;
                return Box::pin(
                    async move { self.acquire_read_lock_internal(path, holder).await },
                )
                .await;
            }
        }

        // Get or create lock info
        let lock_info = locks.entry(path.clone()).or_insert_with(|| {
            Arc::new(FileLockInfo {
                path: path.clone(),
                lock_type: LockType::Read,
                holder: holder.to_string(),
                timestamp: SystemTime::now(),
                read_count: Mutex::new(0),
                write_locked: Mutex::new(false),
            })
        });

        // Increment read count
        let mut read_count = lock_info.read_count.lock().await;
        *read_count += 1;

        debug!(
            "Acquired read lock on {} (count: {})",
            path.display(),
            *read_count
        );

        Ok(FileLock {
            info: lock_info.clone(),
            manager: Arc::new(self.clone()),
            lock_type: LockType::Read,
        })
    }

    /// Internal method to acquire write lock
    async fn acquire_write_lock_internal(&self, path: &PathBuf, holder: &str) -> Result<FileLock> {
        let mut locks = self.locks.write().await;

        // Check if file is already locked
        if let Some(lock_info) = locks.get(path) {
            let read_count = *lock_info.read_count.lock().await;
            let write_locked = *lock_info.write_locked.lock().await;

            if read_count > 0 || write_locked {
                // Wait for locks to be released
                drop(locks);
                sleep(Duration::from_millis(10)).await;
                return Box::pin(
                    async move { self.acquire_write_lock_internal(path, holder).await },
                )
                .await;
            }
        }

        // Get or create lock info
        let lock_info = locks.entry(path.clone()).or_insert_with(|| {
            Arc::new(FileLockInfo {
                path: path.clone(),
                lock_type: LockType::Write,
                holder: holder.to_string(),
                timestamp: SystemTime::now(),
                read_count: Mutex::new(0),
                write_locked: Mutex::new(false),
            })
        });

        // Set write lock flag
        let mut write_locked = lock_info.write_locked.lock().await;
        *write_locked = true;

        debug!("Acquired write lock on {}", path.display());

        Ok(FileLock {
            info: lock_info.clone(),
            manager: Arc::new(self.clone()),
            lock_type: LockType::Write,
        })
    }

    /// Release a file lock
    pub async fn release_lock(&self, path: &PathBuf, lock_type: LockType) -> Result<()> {
        let mut locks = self.locks.write().await;

        if let Some(lock_info) = locks.get(path) {
            match lock_type {
                LockType::Read => {
                    let mut read_count = lock_info.read_count.lock().await;
                    if *read_count > 0 {
                        *read_count -= 1;
                        debug!(
                            "Released read lock on {} (count: {})",
                            path.display(),
                            *read_count
                        );

                        // Remove lock if no more readers
                        if *read_count == 0 {
                            drop(read_count); // Release the guard before removing
                            locks.remove(path);
                        }
                    }
                }
                LockType::Write => {
                    let mut write_locked = lock_info.write_locked.lock().await;
                    *write_locked = false;
                    debug!("Released write lock on {}", path.display());

                    // Remove lock
                    drop(write_locked); // Release the guard before removing
                    locks.remove(path);
                }
            }
        }

        Ok(())
    }

    /// Get lock status for a file
    pub async fn get_lock_status(&self, path: impl AsRef<Path>) -> Option<LockStatus> {
        let path = path.as_ref().to_path_buf();
        let locks = self.locks.read().await;

        if let Some(lock_info) = locks.get(&path) {
            let read_count = *lock_info.read_count.lock().await;
            let write_locked = *lock_info.write_locked.lock().await;

            Some(LockStatus {
                path,
                read_count,
                write_locked,
                holder: lock_info.holder.clone(),
                timestamp: lock_info.timestamp,
            })
        } else {
            None
        }
    }

    /// List all active locks
    pub async fn list_active_locks(&self) -> Vec<LockStatus> {
        let locks = self.locks.read().await;
        let mut statuses = Vec::new();

        for (path, lock_info) in locks.iter() {
            let read_count = *lock_info.read_count.lock().await;
            let write_locked = *lock_info.write_locked.lock().await;

            if read_count > 0 || write_locked {
                statuses.push(LockStatus {
                    path: path.clone(),
                    read_count,
                    write_locked,
                    holder: lock_info.holder.clone(),
                    timestamp: lock_info.timestamp,
                });
            }
        }

        statuses
    }
}

impl Clone for LockManager {
    fn clone(&self) -> Self {
        Self {
            locks: self.locks.clone(),
            config: self.config.clone(),
        }
    }
}

/// Lock status information
#[derive(Debug, Clone)]
pub struct LockStatus {
    /// File path
    pub path: PathBuf,
    /// Read lock count
    pub read_count: u32,
    /// Write lock flag
    pub write_locked: bool,
    /// Lock holder
    pub holder: String,
    /// Lock timestamp
    pub timestamp: SystemTime,
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let path = self.info.path.clone();
        let lock_type = self.lock_type.clone();
        let manager = self.manager.clone();

        // Release lock asynchronously
        tokio::spawn(async move {
            if let Err(e) = manager.release_lock(&path, lock_type).await {
                error!("Failed to release file lock: {}", e);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_locking() -> Result<()> {
        let config = crate::ConcurrentFsConfig::default();
        let manager = LockManager::new(&config)?;

        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test read lock
        let read_lock = manager.acquire_read_lock(&test_file).await?;
        assert!(read_lock.lock_type == LockType::Read);

        // Test multiple read locks
        let read_lock2 = manager.acquire_read_lock(&test_file).await?;
        assert!(read_lock2.lock_type == LockType::Read);

        // Check lock status
        let status = manager.get_lock_status(&test_file).await;
        assert!(status.is_some());
        let status = status.unwrap();
        assert_eq!(status.read_count, 2);
        assert!(!status.write_locked);

        // Release locks
        drop(read_lock);
        drop(read_lock2);

        // Wait a bit for async cleanup
        sleep(Duration::from_millis(100)).await;

        // Check that lock is released
        let status = manager.get_lock_status(&test_file).await;
        assert!(status.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_write_locking() -> Result<()> {
        let config = crate::ConcurrentFsConfig::default();
        let manager = LockManager::new(&config)?;

        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test write lock
        let write_lock = manager.acquire_write_lock(&test_file).await?;
        assert!(write_lock.lock_type == LockType::Write);

        // Check lock status
        let status = manager.get_lock_status(&test_file).await;
        assert!(status.is_some());
        let status = status.unwrap();
        assert_eq!(status.read_count, 0);
        assert!(status.write_locked);

        // Release lock
        drop(write_lock);

        // Wait a bit for async cleanup
        sleep(Duration::from_millis(100)).await;

        // Check that lock is released
        let status = manager.get_lock_status(&test_file).await;
        assert!(status.is_none());

        Ok(())
    }
}
