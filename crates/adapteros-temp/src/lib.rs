//! Temporary file management with guaranteed cleanup
//!
//! Provides atomic temporary file operations with guaranteed cleanup
//! and secure permissions for AdapterOS training and adapter operations.

pub mod atomic;
pub mod cleanup;
pub mod guard;
pub mod manager;

use adapteros_core::{AosError, Result};
use adapteros_platform::common::PlatformUtils;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

// Re-export guard types
pub use guard::{TempDirGuard, TempFileGuard};

/// Temporary file configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempConfig {
    /// Base directory for temporary files
    pub base_dir: PathBuf,
    /// Maximum file size in bytes
    pub max_file_size_bytes: u64,
    /// Default file permissions (octal)
    pub default_permissions: u32,
    /// Cleanup interval
    pub cleanup_interval: Duration,
    /// File age threshold for cleanup
    pub age_threshold: Duration,
    /// Enable secure permissions
    pub secure_permissions: bool,
    /// Enable atomic operations
    pub atomic_operations: bool,
}

/// Temporary file manager
pub struct TempManager {
    config: TempConfig,
    active_files: Arc<RwLock<std::collections::HashMap<String, TempFileInfo>>>,
    cleanup_task: Option<JoinHandle<()>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

/// Information about a temporary file
#[derive(Debug, Clone)]
pub struct TempFileInfo {
    /// File path
    pub path: PathBuf,
    /// Creation time
    pub created_at: SystemTime,
    /// File size
    pub size: u64,
    /// File permissions
    pub permissions: u32,
    /// Owner process ID
    pub owner_pid: u32,
}

impl TempManager {
    /// Create a new temporary file manager
    pub fn new(config: TempConfig) -> Result<Self> {
        // Create base directory if it doesn't exist
        if !config.base_dir.exists() {
            std::fs::create_dir_all(&config.base_dir)
                .map_err(|e| AosError::Io(format!("Failed to create temp directory: {}", e)))?;
        }

        Ok(Self {
            config,
            active_files: Arc::new(RwLock::new(std::collections::HashMap::new())),
            cleanup_task: None,
            shutdown_tx: None,
        })
    }

    /// Start the cleanup task
    pub async fn start_cleanup_task(&mut self) -> Result<()> {
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let config = self.config.clone();
        let active_files = Arc::clone(&self.active_files);

        let cleanup_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.cleanup_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::run_cleanup(&config, &active_files).await {
                            warn!("Temp file cleanup failed: {}", e);
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("Temp file cleanup task shutting down");
                        break;
                    }
                }
            }
        });

        self.cleanup_task = Some(cleanup_task);
        info!(
            "Started temp file cleanup task with interval {:?}",
            self.config.cleanup_interval
        );
        Ok(())
    }

    /// Stop the cleanup task
    pub async fn stop_cleanup_task(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(task) = self.cleanup_task.take() {
            task.abort();
            let _ = task.await;
        }

        info!("Stopped temp file cleanup task");
        Ok(())
    }

    /// Create a temporary file with guaranteed cleanup
    pub async fn create_temp_file(&self, prefix: &str, suffix: &str) -> Result<TempFileGuard<'_>> {
        let file_id = format!(
            "{}_{}_{}",
            prefix,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        );
        let temp_path = self.config.base_dir.join(format!("{}{}", file_id, suffix));

        // Create the file
        fs::write(&temp_path, b"")
            .await
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        // Set secure permissions if enabled
        if self.config.secure_permissions {
            self.set_secure_permissions(&temp_path).await?;
        }

        // Get file metadata
        let metadata = fs::metadata(&temp_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to get temp file metadata: {}", e)))?;

        let file_info = TempFileInfo {
            path: temp_path.clone(),
            created_at: SystemTime::now(),
            size: metadata.len(),
            permissions: self.config.default_permissions,
            owner_pid: std::process::id(),
        };

        // Register the file
        {
            let mut active_files = self.active_files.write().await;
            active_files.insert(file_id.clone(), file_info);
        }

        debug!("Created temp file: {}", temp_path.display());

        Ok(TempFileGuard {
            file_id,
            path: temp_path,
            manager: self,
        })
    }

    /// Create a temporary directory with guaranteed cleanup
    pub async fn create_temp_dir(&self, prefix: &str) -> Result<TempDirGuard<'_>> {
        let dir_id = format!(
            "{}_{}_{}",
            prefix,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id()
        );
        let temp_path = self.config.base_dir.join(&dir_id);

        // Create the directory
        fs::create_dir_all(&temp_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to create temp directory: {}", e)))?;

        // Set secure permissions if enabled
        if self.config.secure_permissions {
            self.set_secure_permissions(&temp_path).await?;
        }

        debug!("Created temp directory: {}", temp_path.display());

        Ok(TempDirGuard {
            dir_id,
            path: temp_path,
            manager: self,
        })
    }

    /// Remove a temporary file
    pub async fn remove_temp_file(&self, file_id: &str) -> Result<()> {
        let file_info = {
            let mut active_files = self.active_files.write().await;
            active_files.remove(file_id)
        };

        if let Some(file_info) = file_info {
            if file_info.path.exists() {
                fs::remove_file(&file_info.path).await.map_err(|e| {
                    AosError::Io(format!(
                        "Failed to remove temp file {}: {}",
                        file_info.path.display(),
                        e
                    ))
                })?;
                debug!("Removed temp file: {}", file_info.path.display());
            }
        }

        Ok(())
    }

    /// Remove a temporary directory
    pub async fn remove_temp_dir(&self, dir_id: &str) -> Result<()> {
        let dir_path = self.config.base_dir.join(dir_id);

        if dir_path.exists() {
            fs::remove_dir_all(&dir_path).await.map_err(|e| {
                AosError::Io(format!(
                    "Failed to remove temp directory {}: {}",
                    dir_path.display(),
                    e
                ))
            })?;
            debug!("Removed temp directory: {}", dir_path.display());
        }

        Ok(())
    }

    /// Set secure permissions for a file or directory
    async fn set_secure_permissions(&self, path: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(self.config.default_permissions);
            fs::set_permissions(path, perms)
                .await
                .map_err(|e| AosError::Io(format!("Failed to set permissions: {}", e)))?;
        }

        #[cfg(windows)]
        {
            // Windows permission handling would go here
            // For now, just log that we're on Windows
            debug!("Windows permissions not implemented yet");
        }

        Ok(())
    }

    /// Run cleanup of old temporary files
    async fn run_cleanup(
        config: &TempConfig,
        active_files: &Arc<RwLock<std::collections::HashMap<String, TempFileInfo>>>,
    ) -> Result<()> {
        let mut cleaned_count = 0;
        let now = SystemTime::now();

        // Clean up old files
        {
            let mut files = active_files.write().await;
            files.retain(|_file_id, file_info| {
                if now
                    .duration_since(file_info.created_at)
                    .unwrap_or(Duration::MAX)
                    > config.age_threshold
                {
                    // Remove the file
                    if file_info.path.exists() {
                        if let Err(e) = std::fs::remove_file(&file_info.path) {
                            warn!(
                                "Failed to remove old temp file {}: {}",
                                file_info.path.display(),
                                e
                            );
                        } else {
                            cleaned_count += 1;
                            debug!("Cleaned up old temp file: {}", file_info.path.display());
                        }
                    }
                    false // Remove from active files
                } else {
                    true // Keep in active files
                }
            });
        }

        if cleaned_count > 0 {
            info!("Cleaned up {} old temporary files", cleaned_count);
        }

        Ok(())
    }

    /// Get current temporary file usage
    pub async fn get_usage(&self) -> Result<TempUsage> {
        let files = self.active_files.read().await;
        let mut total_size = 0u64;
        let mut file_count = 0u32;

        for file_info in files.values() {
            total_size += file_info.size;
            file_count += 1;
        }

        Ok(TempUsage {
            file_count,
            total_size,
            base_dir: self.config.base_dir.clone(),
        })
    }
}

/// Temporary file usage information
#[derive(Debug, Clone)]
pub struct TempUsage {
    /// Number of active temporary files
    pub file_count: u32,
    /// Total size of temporary files in bytes
    pub total_size: u64,
    /// Base directory for temporary files
    pub base_dir: PathBuf,
}

impl Default for TempConfig {
    fn default() -> Self {
        Self {
            base_dir: PlatformUtils::temp_dir().join("adapteros"),
            max_file_size_bytes: 100 * 1024 * 1024, // 100MB
            default_permissions: 0o600,             // Owner read/write only
            cleanup_interval: Duration::from_secs(3600), // 1 hour
            age_threshold: Duration::from_secs(24 * 3600), // 24 hours
            secure_permissions: true,
            atomic_operations: true,
        }
    }
}

impl Drop for TempManager {
    fn drop(&mut self) {
        if self.cleanup_task.is_some() {
            // Note: We can't await in Drop, so we just abort the task
            if let Some(task) = self.cleanup_task.take() {
                task.abort();
            }
        }
    }
}
