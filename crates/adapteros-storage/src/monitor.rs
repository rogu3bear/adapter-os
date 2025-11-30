//! Storage monitoring and alerting
//!
//! Implements storage usage monitoring and alerting for tenant storage.

use crate::{StorageConfig, StorageUsage};
use adapteros_core::{AosError, Result};
use adapteros_telemetry::TelemetryWriter;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Storage monitor for tracking usage and sending alerts
pub struct StorageMonitor {
    config: StorageConfig,
    root_path: PathBuf,
    telemetry: TelemetryWriter,
    monitoring_task: Option<tokio::task::JoinHandle<()>>,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    last_alert_level: Arc<RwLock<Option<AlertLevel>>>,
}

impl Clone for StorageMonitor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            root_path: self.root_path.clone(),
            telemetry: self.telemetry.clone(),
            monitoring_task: None, // Cannot clone JoinHandle
            shutdown_tx: None, // Cannot clone oneshot::Sender
            last_alert_level: Arc::clone(&self.last_alert_level),
        }
    }
}

/// Alert levels for storage monitoring
#[derive(Debug, Clone, PartialEq)]
pub enum AlertLevel {
    Warning,
    Critical,
    Emergency,
}

impl StorageMonitor {
    /// Create a new storage monitor
    pub fn new(config: &StorageConfig, root_path: &Path) -> Result<Self> {
        let telemetry = TelemetryWriter::new("storage_monitor", 1000, 1024 * 1024)?;

        Ok(Self {
            config: config.clone(),
            root_path: root_path.to_path_buf(),
            telemetry,
            monitoring_task: None,
            shutdown_tx: None,
            last_alert_level: Arc::new(RwLock::new(None)),
        })
    }

    /// Start monitoring
    pub async fn start_monitoring(&mut self) -> Result<()> {
        if !self.config.monitoring.enabled {
            debug!("Storage monitoring is disabled");
            return Ok(());
        }

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel();
        self.shutdown_tx = Some(shutdown_tx);

        let config = self.config.clone();
        let root_path = self.root_path.clone();
        let telemetry = self.telemetry.clone();
        let last_alert_level = Arc::clone(&self.last_alert_level);

        let monitoring_task = tokio::spawn(async move {
            let mut interval = interval(config.monitoring.check_interval);

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::check_storage_usage(&config, &root_path, &telemetry, &last_alert_level).await {
                            error!("Storage monitoring check failed: {}", e);
                        }
                    }
                    _ = &mut shutdown_rx => {
                        info!("Storage monitoring task shutting down");
                        break;
                    }
                }
            }
        });

        self.monitoring_task = Some(monitoring_task);
        info!(
            "Started storage monitoring with interval {:?}",
            self.config.monitoring.check_interval
        );
        Ok(())
    }

    /// Stop monitoring
    pub async fn stop_monitoring(&mut self) -> Result<()> {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }

        if let Some(task) = self.monitoring_task.take() {
            task.abort();
            let _ = task.await;
        }

        info!("Stopped storage monitoring");
        Ok(())
    }

    /// Get current storage usage
    pub async fn get_usage(&self) -> Result<StorageUsage> {
        self.calculate_usage().await
    }

    /// Check storage usage and send alerts if needed
    async fn check_storage_usage(
        config: &StorageConfig,
        root_path: &Path,
        telemetry: &TelemetryWriter,
        last_alert_level: &RwLock<Option<AlertLevel>>,
    ) -> Result<()> {
        let usage = Self::calculate_usage_internal(config, root_path).await?;
        let thresholds = &config.monitoring.alert_thresholds;

        let current_alert_level = if usage.exceeds_threshold(thresholds.emergency_pct) {
            Some(AlertLevel::Emergency)
        } else if usage.exceeds_threshold(thresholds.critical_pct) {
            Some(AlertLevel::Critical)
        } else if usage.exceeds_threshold(thresholds.warning_pct) {
            Some(AlertLevel::Warning)
        } else {
            None
        };

        // Check if alert level changed
        let mut last_level = last_alert_level.write().await;
        let should_send_alert = *last_level != current_alert_level;
        *last_level = current_alert_level.clone();

        if should_send_alert {
            if let Some(alert_level) = &current_alert_level {
                Self::send_alert(telemetry, alert_level, &usage).await?;
            } else {
                // Usage is back to normal
                Self::send_recovery_alert(telemetry, &usage).await?;
            }
        }

        // Log usage info
        debug!(
            "Storage usage: {:.1}% ({} bytes / {} bytes)",
            usage.usage_pct, usage.used_bytes, usage.available_bytes
        );

        Ok(())
    }

    /// Send storage alert
    async fn send_alert(
        telemetry: &TelemetryWriter,
        alert_level: &AlertLevel,
        usage: &StorageUsage,
    ) -> Result<()> {
        let alert_data = serde_json::json!({
            "type": "storage_alert",
            "level": format!("{:?}", alert_level),
            "usage_pct": usage.usage_pct,
            "used_bytes": usage.used_bytes,
            "available_bytes": usage.available_bytes,
            "file_count": usage.file_count,
            "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
        });

        telemetry
            .log("storage_monitor", alert_data)
            .map_err(|e| AosError::Telemetry(format!("Failed to send storage alert: {}", e)))?;

        match alert_level {
            AlertLevel::Warning => warn!("Storage usage warning: {:.1}%", usage.usage_pct),
            AlertLevel::Critical => error!("Storage usage critical: {:.1}%", usage.usage_pct),
            AlertLevel::Emergency => error!("Storage usage emergency: {:.1}%", usage.usage_pct),
        }

        Ok(())
    }

    /// Send recovery alert
    async fn send_recovery_alert(telemetry: &TelemetryWriter, usage: &StorageUsage) -> Result<()> {
        let recovery_data = serde_json::json!({
            "type": "storage_recovery",
            "usage_pct": usage.usage_pct,
            "used_bytes": usage.used_bytes,
            "available_bytes": usage.available_bytes,
            "file_count": usage.file_count,
            "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
        });

        telemetry
            .log("storage_monitor", recovery_data)
            .map_err(|e| {
                AosError::Telemetry(format!("Failed to send storage recovery alert: {}", e))
            })?;

        info!("Storage usage recovered: {:.1}%", usage.usage_pct);
        Ok(())
    }

    /// Calculate current storage usage
    async fn calculate_usage(&self) -> Result<StorageUsage> {
        Self::calculate_usage_internal(&self.config, &self.root_path).await
    }

    /// Calculate storage usage for a given config and path
    async fn calculate_usage_internal(
        config: &StorageConfig,
        root_path: &Path,
    ) -> Result<StorageUsage> {
        let mut used_bytes = 0u64;
        let mut file_count = 0u32;

        if !root_path.exists() {
            return Ok(StorageUsage {
                used_bytes: 0,
                available_bytes: config.max_disk_space_bytes,
                file_count: 0,
                usage_pct: 0.0,
                last_updated: SystemTime::now(),
            });
        }

        // Walk directory tree
        Self::walk_directory(root_path, &mut used_bytes, &mut file_count).await?;

        let usage_pct = (used_bytes as f32 / config.max_disk_space_bytes as f32) * 100.0;

        Ok(StorageUsage {
            used_bytes,
            available_bytes: config.max_disk_space_bytes,
            file_count,
            usage_pct,
            last_updated: SystemTime::now(),
        })
    }

    /// Walk directory tree to calculate usage
    async fn walk_directory(path: &Path, used_bytes: &mut u64, file_count: &mut u32) -> Result<()> {
        Box::pin(async move {
            let mut entries = tokio::fs::read_dir(path).await.map_err(|e| {
                AosError::Io(format!(
                    "Failed to read directory {}: {}",
                    path.display(),
                    e
                ))
            })?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?
            {
                let entry_path = entry.path();

                if entry_path.is_file() {
                    let metadata = entry.metadata().await.map_err(|e| {
                        AosError::Io(format!(
                            "Failed to read file metadata {}: {}",
                            entry_path.display(),
                            e
                        ))
                    })?;

                    *used_bytes += metadata.len();
                    *file_count += 1;
                } else if entry_path.is_dir() {
                    Self::walk_directory(entry_path.as_path(), used_bytes, file_count).await?;
                }
            }

            Ok(())
        })
        .await
    }
}

impl Drop for StorageMonitor {
    fn drop(&mut self) {
        if self.monitoring_task.is_some() {
            // Note: We can't await in Drop, so we just abort the task
            if let Some(task) = self.monitoring_task.take() {
                task.abort();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AlertThresholds, StorageMonitoring};
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_storage_monitor() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = StorageConfig {
            max_disk_space_bytes: 1000,
            max_files: 100,
            monitoring: StorageMonitoring {
                enabled: true,
                check_interval: Duration::from_secs(1),
                alert_thresholds: AlertThresholds {
                    warning_pct: 50.0,
                    critical_pct: 80.0,
                    emergency_pct: 95.0,
                },
            },
            ..Default::default()
        };

        let monitor = StorageMonitor::new(&config, &temp_dir.path())?;

        // Create test files
        let test_file1 = temp_dir.path().join("test1.txt");
        fs::write(&test_file1, "hello")?;

        let test_file2 = temp_dir.path().join("test2.txt");
        fs::write(&test_file2, "world")?;

        // Check usage
        let usage = monitor.get_usage().await?;
        assert!(usage.used_bytes > 0);
        assert_eq!(usage.file_count, 2);
        assert!(usage.usage_pct > 0.0);

        Ok(())
    }
}
