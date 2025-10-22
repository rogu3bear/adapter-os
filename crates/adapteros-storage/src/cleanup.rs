//! Storage cleanup management
//!
//! Implements automatic cleanup policies for tenant storage.

use crate::{StorageConfig, StorageUsage};
use adapteros_core::{AosError, Result};
use glob::glob;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Cleanup manager for automatic storage cleanup
pub struct CleanupManager {
    config: StorageConfig,
    root_path: PathBuf,
    cleanup_task: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl CleanupManager {
    /// Create a new cleanup manager
    pub fn new(config: &StorageConfig, root_path: &PathBuf) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            root_path: root_path.clone(),
            cleanup_task: None,
            shutdown_tx: None,
        })
    }

    /// Start the cleanup task
    pub async fn start_cleanup_task(&mut self) -> Result<()> {
        if !self.config.cleanup_policy.enabled {
            debug!("Cleanup policy is disabled");
            return Ok(());
        }

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let config = self.config.clone();
        let root_path = self.root_path.clone();

        let cleanup_task = tokio::spawn(async move {
            let mut interval = interval(config.cleanup_policy.interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::run_cleanup(&config, &root_path).await {
                            error!("Cleanup failed: {}", e);
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("Cleanup task shutting down");
                        break;
                    }
                }
            }
        });

        self.cleanup_task = Some(cleanup_task);
        info!(
            "Started cleanup task with interval {:?}",
            self.config.cleanup_policy.interval
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

        info!("Stopped cleanup task");
        Ok(())
    }

    /// Run cleanup if needed based on usage thresholds
    pub async fn cleanup_if_needed(&self) -> Result<()> {
        if !self.config.cleanup_policy.enabled {
            return Ok(());
        }

        // Check current usage
        let usage = self.get_current_usage().await?;

        if usage.exceeds_threshold(self.config.cleanup_policy.usage_threshold_pct) {
            info!(
                "Storage usage {:.1}% exceeds threshold {:.1}%, running cleanup",
                usage.usage_pct, self.config.cleanup_policy.usage_threshold_pct
            );
            Self::run_cleanup(&self.config, &self.root_path).await?;
        }

        Ok(())
    }

    /// Run cleanup based on policy
    async fn run_cleanup(config: &StorageConfig, root_path: &PathBuf) -> Result<()> {
        let mut cleaned_bytes = 0u64;
        let mut cleaned_files = 0u32;

        // Clean up files matching patterns
        for pattern in &config.cleanup_policy.patterns {
            let glob_pattern = root_path
                .join("**")
                .join(pattern)
                .to_string_lossy()
                .to_string();

            match glob(&glob_pattern) {
                Ok(paths) => {
                    for path in paths {
                        match path {
                            Ok(file_path) => {
                                if let Ok(metadata) = fs::metadata(&file_path).await {
                                    // Check file age
                                    if let Ok(created) = metadata.created() {
                                        if SystemTime::now()
                                            .duration_since(created)
                                            .unwrap_or(Duration::MAX)
                                            > config.cleanup_policy.age_threshold
                                        {
                                            if let Err(e) = fs::remove_file(&file_path).await {
                                                warn!(
                                                    "Failed to remove file {}: {}",
                                                    file_path.display(),
                                                    e
                                                );
                                            } else {
                                                cleaned_bytes += metadata.len();
                                                cleaned_files += 1;
                                                debug!("Cleaned up file: {}", file_path.display());
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Glob pattern error: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Invalid glob pattern {}: {}", pattern, e);
                }
            }
        }

        // Clean up old .aos files
        Self::cleanup_old_aos_files(config, root_path, &mut cleaned_bytes, &mut cleaned_files)
            .await?;

        if cleaned_files > 0 {
            info!(
                "Cleanup completed: {} files, {} bytes",
                cleaned_files, cleaned_bytes
            );
        }

        Ok(())
    }

    /// Clean up old .aos files
    async fn cleanup_old_aos_files(
        config: &StorageConfig,
        root_path: &PathBuf,
        cleaned_bytes: &mut u64,
        cleaned_files: &mut u32,
    ) -> Result<()> {
        let aos_pattern = root_path
            .join("**")
            .join("*.aos")
            .to_string_lossy()
            .to_string();

        match glob(&aos_pattern) {
            Ok(paths) => {
                for path in paths {
                    match path {
                        Ok(file_path) => {
                            if let Ok(metadata) = fs::metadata(&file_path).await {
                                // Check file age
                                if let Ok(created) = metadata.created() {
                                    if SystemTime::now()
                                        .duration_since(created)
                                        .unwrap_or(Duration::MAX)
                                        > config.cleanup_policy.age_threshold
                                    {
                                        if let Err(e) = fs::remove_file(&file_path).await {
                                            warn!(
                                                "Failed to remove .aos file {}: {}",
                                                file_path.display(),
                                                e
                                            );
                                        } else {
                                            *cleaned_bytes += metadata.len();
                                            *cleaned_files += 1;
                                            debug!("Cleaned up .aos file: {}", file_path.display());
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Glob pattern error: {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Invalid .aos glob pattern: {}", e);
            }
        }

        Ok(())
    }

    /// Get current storage usage
    async fn get_current_usage(&self) -> Result<StorageUsage> {
        let mut used_bytes = 0u64;
        let mut file_count = 0u32;

        if !self.root_path.exists() {
            return Ok(StorageUsage {
                used_bytes: 0,
                available_bytes: self.config.max_disk_space_bytes,
                file_count: 0,
                usage_pct: 0.0,
                last_updated: SystemTime::now(),
            });
        }

        // Walk directory tree
        self.walk_directory(&self.root_path, &mut used_bytes, &mut file_count)
            .await?;

        let usage_pct = (used_bytes as f32 / self.config.max_disk_space_bytes as f32) * 100.0;

        Ok(StorageUsage {
            used_bytes,
            available_bytes: self.config.max_disk_space_bytes,
            file_count,
            usage_pct,
            last_updated: SystemTime::now(),
        })
    }

    /// Walk directory tree to calculate usage
    async fn walk_directory(
        &self,
        path: &PathBuf,
        used_bytes: &mut u64,
        file_count: &mut u32,
    ) -> Result<()> {
        Box::pin(async move {
            let mut entries = fs::read_dir(path).await.map_err(|e| {
                AosError::Storage(format!(
                    "Failed to read directory {}: {}",
                    path.display(),
                    e
                ))
            })?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| AosError::Storage(format!("Failed to read directory entry: {}", e)))?
            {
                let entry_path = entry.path();

                if entry_path.is_file() {
                    let metadata = entry.metadata().await.map_err(|e| {
                        AosError::Storage(format!(
                            "Failed to read file metadata {}: {}",
                            entry_path.display(),
                            e
                        ))
                    })?;

                    *used_bytes += metadata.len();
                    *file_count += 1;
                } else if entry_path.is_dir() {
                    self.walk_directory(&entry_path, used_bytes, file_count)
                        .await?;
                }
            }

            Ok(())
        })
        .await
    }
}

impl Drop for CleanupManager {
    fn drop(&mut self) {
        if self.cleanup_task.is_some() {
            // Note: We can't await in Drop, so we just abort the task
            if let Some(task) = self.cleanup_task.take() {
                task.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_cleanup_manager() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = StorageConfig {
            max_disk_space_bytes: 1000,
            max_files: 100,
            cleanup_policy: CleanupPolicy {
                enabled: true,
                interval: Duration::from_secs(1),
                age_threshold: Duration::from_secs(0), // Clean up immediately
                usage_threshold_pct: 50.0,
                patterns: vec!["*.tmp".to_string()],
            },
            ..Default::default()
        };

        let mut cleanup_manager = CleanupManager::new(&config, &temp_dir.path().to_path_buf())?;

        // Create test files
        let test_file1 = temp_dir.path().join("test1.tmp");
        fs::write(&test_file1, "hello")?;

        let test_file2 = temp_dir.path().join("test2.tmp");
        fs::write(&test_file2, "world")?;

        // Run cleanup
        cleanup_manager.cleanup_if_needed().await?;

        // Check that files were cleaned up
        assert!(!test_file1.exists());
        assert!(!test_file2.exists());

        Ok(())
    }
}
