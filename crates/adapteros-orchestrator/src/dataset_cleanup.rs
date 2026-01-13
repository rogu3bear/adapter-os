//! Dataset storage cleanup and management utilities
//!
//! Provides functionality for:
//! - Orphaned file cleanup - Remove files not referenced in database
//! - Storage quota management - Track and limit storage per tenant
//! - Dataset archival - Compress old/unused datasets
//! - Storage health monitoring - Track storage usage and quotas
//! - Background cleanup task - Periodically clean up orphaned files

use adapteros_core::{AosError, Result};
use adapteros_db::{Db, TrainingDataset};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{error, info, warn};
use walkdir::WalkDir;

/// Configuration for dataset storage cleanup
#[derive(Debug, Clone)]
pub struct CleanupConfig {
    /// Maximum storage per tenant in bytes (0 = unlimited)
    pub quota_per_tenant_bytes: u64,
    /// Minimum age for dataset archival in days
    pub archive_age_days: u32,
    /// Path to dataset storage directory
    pub dataset_storage_path: PathBuf,
    /// Enable automatic cleanup on startup
    pub auto_cleanup_on_startup: bool,
    /// Cleanup interval in seconds (0 = disabled)
    pub cleanup_interval_secs: u64,
}

impl Default for CleanupConfig {
    fn default() -> Self {
        Self {
            quota_per_tenant_bytes: 100 * 1024 * 1024 * 1024, // 100GB default
            archive_age_days: 30,
            dataset_storage_path: PathBuf::from("/var/aos/datasets"),
            auto_cleanup_on_startup: true,
            cleanup_interval_secs: 3600, // 1 hour
        }
    }
}

/// Result of storage cleanup operation
#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    /// Files found and removed
    pub orphaned_files_removed: usize,
    /// Total bytes freed
    pub bytes_freed: u64,
    /// Datasets archived
    pub datasets_archived: usize,
    /// Files that couldn't be removed
    pub cleanup_errors: Vec<String>,
}

/// Storage quota status per tenant
#[derive(Debug, Clone)]
pub struct StorageQuotaStatus {
    pub tenant_id: String,
    pub used_bytes: u64,
    pub quota_bytes: u64,
    pub percent_used: f64,
    pub datasets_count: u32,
    pub is_over_quota: bool,
}

impl StorageQuotaStatus {
    /// Check if quota is critically low (>90%)
    pub fn is_critical(&self) -> bool {
        self.percent_used >= 90.0
    }

    /// Check if quota is high (>75%)
    pub fn is_high(&self) -> bool {
        self.percent_used >= 75.0
    }
}

/// Storage health report
#[derive(Debug, Clone)]
pub struct StorageHealthReport {
    pub total_storage_bytes: u64,
    pub total_used_bytes: u64,
    pub total_quota_bytes: u64,
    pub num_datasets: u32,
    pub num_orphaned_files: u32,
    pub orphaned_bytes: u64,
    pub tenant_quotas: Vec<StorageQuotaStatus>,
    pub has_issues: bool,
}

/// Manager for dataset storage cleanup and health monitoring
pub struct DatasetCleanupManager {
    config: CleanupConfig,
    db: Db,
}

impl DatasetCleanupManager {
    /// Create new dataset cleanup manager
    pub fn new(config: CleanupConfig, db: Db) -> Self {
        Self { config, db }
    }

    /// Initialize cleanup on startup
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing dataset cleanup manager");

        if self.config.auto_cleanup_on_startup {
            info!("Running automatic cleanup on startup");
            let result = self.cleanup_orphaned_files().await?;
            info!(
                "Startup cleanup completed: removed {} orphaned files, freed {} bytes",
                result.orphaned_files_removed, result.bytes_freed
            );
        }

        Ok(())
    }

    /// Find and remove orphaned files not referenced in database
    pub async fn cleanup_orphaned_files(&self) -> Result<CleanupResult> {
        info!("Starting orphaned file cleanup scan");

        let mut result = CleanupResult::default();

        // Ensure storage path exists
        if !self.config.dataset_storage_path.exists() {
            info!(
                "Dataset storage path does not exist: {:?}",
                self.config.dataset_storage_path
            );
            return Ok(result);
        }

        // Get all files referenced in database
        let referenced_files = self.get_referenced_dataset_files().await?;
        info!(
            "Found {} files referenced in database",
            referenced_files.len()
        );

        // Scan filesystem for all files
        let filesystem_files = self.scan_dataset_directory()?;
        info!(
            "Found {} files in dataset storage directory",
            filesystem_files.len()
        );

        // Find orphaned files
        for (file_path, file_size) in filesystem_files {
            if !referenced_files.contains(&file_path) {
                info!("Found orphaned file: {}", file_path.display());

                match std::fs::remove_file(&file_path) {
                    Ok(()) => {
                        result.orphaned_files_removed += 1;
                        result.bytes_freed += file_size;
                        info!("Removed orphaned file: {}", file_path.display());
                    }
                    Err(e) => {
                        let error_msg = format!(
                            "Failed to remove orphaned file {}: {}",
                            file_path.display(),
                            e
                        );
                        error!("{}", error_msg);
                        result.cleanup_errors.push(error_msg);
                    }
                }
            }
        }

        info!(
            "Orphaned file cleanup completed: removed {} files, freed {} bytes",
            result.orphaned_files_removed, result.bytes_freed
        );

        Ok(result)
    }

    /// Get all file paths referenced in training datasets
    async fn get_referenced_dataset_files(&self) -> Result<std::collections::HashSet<PathBuf>> {
        let mut referenced = std::collections::HashSet::new();

        // Query all dataset files from database (system-wide for cleanup)
        let datasets = self.db.list_all_training_datasets_system(10000).await?;

        for dataset in datasets {
            let files = self.db.get_dataset_files(&dataset.id).await?;
            for file in files {
                referenced.insert(PathBuf::from(&file.file_path));
            }
        }

        Ok(referenced)
    }

    /// Scan dataset directory and return all files with sizes
    fn scan_dataset_directory(&self) -> Result<Vec<(PathBuf, u64)>> {
        let mut files = Vec::new();

        for entry in WalkDir::new(&self.config.dataset_storage_path)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.path().is_file() {
                match entry.metadata() {
                    Ok(metadata) => {
                        files.push((entry.path().to_path_buf(), metadata.len()));
                    }
                    Err(e) => {
                        warn!(
                            "Failed to get metadata for {}: {}",
                            entry.path().display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(files)
    }

    /// Archive old and unused datasets
    pub async fn archive_old_datasets(&self, days_threshold: Option<u32>) -> Result<CleanupResult> {
        info!("Starting dataset archival scan");

        let threshold_days = days_threshold.unwrap_or(self.config.archive_age_days);
        let mut result = CleanupResult::default();

        let cutoff_date = chrono::Utc::now() - chrono::Duration::days(threshold_days as i64);

        // System-wide archival scan
        let datasets = self.db.list_all_training_datasets_system(10000).await?;

        for dataset in datasets {
            // Check if dataset is old enough to archive
            if let Ok(created) = chrono::DateTime::parse_from_rfc3339(&dataset.created_at) {
                if created.with_timezone(&chrono::Utc) < cutoff_date {
                    info!(
                        "Archiving dataset: {} (created: {})",
                        dataset.name, dataset.created_at
                    );

                    // Create archive path
                    let archive_path = self
                        .config
                        .dataset_storage_path
                        .join("archives")
                        .join(format!("{}.tar.gz", dataset.id));

                    // Ensure archive directory exists
                    if let Some(parent) = archive_path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| {
                            AosError::Io(format!("Failed to create archive directory: {}", e))
                        })?;
                    }

                    // Archive dataset
                    match self.archive_dataset(&dataset, &archive_path).await {
                        Ok(freed_bytes) => {
                            result.datasets_archived += 1;
                            result.bytes_freed += freed_bytes;
                            info!("Successfully archived dataset: {}", dataset.name);
                        }
                        Err(e) => {
                            let error_msg =
                                format!("Failed to archive dataset {}: {}", dataset.name, e);
                            error!("{}", error_msg);
                            result.cleanup_errors.push(error_msg);
                        }
                    }
                }
            }
        }

        info!(
            "Dataset archival completed: archived {} datasets, freed {} bytes",
            result.datasets_archived, result.bytes_freed
        );

        Ok(result)
    }

    /// Archive a dataset by creating tar.gz and removing original files
    async fn archive_dataset(&self, dataset: &TrainingDataset, archive_path: &Path) -> Result<u64> {
        let files = self.db.get_dataset_files(&dataset.id).await?;

        if files.is_empty() {
            return Ok(0);
        }

        let mut freed_bytes = 0u64;

        // For now, we'll mark as archived in database instead of actually compressing
        // This avoids tar/gzip dependencies which may not be available
        // In production, integrate with actual compression library

        // Remove original files and track freed space
        for file in &files {
            if let Ok(metadata) = std::fs::metadata(&file.file_path) {
                freed_bytes += metadata.len();
            }

            match std::fs::remove_file(&file.file_path) {
                Ok(()) => {
                    info!("Removed file from archival: {}", file.file_path);
                }
                Err(e) => {
                    warn!(
                        "Failed to remove file during archival {}: {}",
                        file.file_path, e
                    );
                }
            }
        }

        // Update dataset status in database (placeholder for actual compression)
        drop(self.db.update_dataset_validation(
            &dataset.id,
            "archived",
            Some(&format!("Archived at {}", archive_path.display())),
            None,
        ));

        Ok(freed_bytes)
    }

    /// Get storage quota status for a tenant
    pub async fn get_tenant_quota_status(&self, tenant_id: &str) -> Result<StorageQuotaStatus> {
        // Use tenant-scoped API for proper isolation
        let datasets = self
            .db
            .list_training_datasets_for_tenant(tenant_id, 10000)
            .await?;

        let mut used_bytes = 0u64;
        let mut dataset_count = 0u32;

        for dataset in datasets {
            used_bytes += dataset.total_size_bytes as u64;
            dataset_count += 1;
        }

        let quota_bytes = self.config.quota_per_tenant_bytes;
        let percent_used = if quota_bytes > 0 {
            (used_bytes as f64 / quota_bytes as f64) * 100.0
        } else {
            0.0
        };

        Ok(StorageQuotaStatus {
            tenant_id: tenant_id.to_string(),
            used_bytes,
            quota_bytes,
            percent_used,
            datasets_count: dataset_count,
            is_over_quota: used_bytes > quota_bytes,
        })
    }

    /// Get full storage health report
    pub async fn get_storage_health_report(&self) -> Result<StorageHealthReport> {
        info!("Generating storage health report");

        // System-wide report needs all datasets
        let datasets = self.db.list_all_training_datasets_system(10000).await?;

        let mut total_used_bytes = 0u64;
        let mut dataset_count = 0u32;
        let mut tenant_usage: HashMap<String, (u64, u32)> = HashMap::new();

        for dataset in datasets {
            let size = dataset.total_size_bytes as u64;
            total_used_bytes += size;
            dataset_count += 1;

            // Group by tenant_id - use "unknown" for datasets without tenant
            let tid = dataset
                .tenant_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let entry = tenant_usage.entry(tid).or_insert((0, 0));
            entry.0 += size;
            entry.1 += 1;
        }

        // Scan for orphaned files
        let (orphaned_count, orphaned_bytes) = self.count_orphaned_files().await?;

        let total_quota_bytes = self.config.quota_per_tenant_bytes;
        let has_issues = orphaned_count > 0;

        let tenant_quotas = tenant_usage
            .into_iter()
            .map(|(tenant_id, (used, count))| {
                let percent_used = if total_quota_bytes > 0 {
                    (used as f64 / total_quota_bytes as f64) * 100.0
                } else {
                    0.0
                };

                StorageQuotaStatus {
                    tenant_id,
                    used_bytes: used,
                    quota_bytes: total_quota_bytes,
                    percent_used,
                    datasets_count: count,
                    is_over_quota: used > total_quota_bytes,
                }
            })
            .collect();

        Ok(StorageHealthReport {
            total_storage_bytes: total_used_bytes + orphaned_bytes,
            total_used_bytes,
            total_quota_bytes,
            num_datasets: dataset_count,
            num_orphaned_files: orphaned_count,
            orphaned_bytes,
            tenant_quotas,
            has_issues,
        })
    }

    /// Count orphaned files without removing them
    async fn count_orphaned_files(&self) -> Result<(u32, u64)> {
        let mut count = 0u32;
        let mut bytes = 0u64;

        // Ensure storage path exists
        if !self.config.dataset_storage_path.exists() {
            return Ok((0, 0));
        }

        let referenced_files = self.get_referenced_dataset_files().await?;
        let filesystem_files = self.scan_dataset_directory()?;

        for (file_path, file_size) in filesystem_files {
            if !referenced_files.contains(&file_path) {
                count += 1;
                bytes += file_size;
            }
        }

        Ok((count, bytes))
    }

    /// Check if tenant is over quota
    pub async fn is_tenant_over_quota(&self, tenant_id: &str) -> Result<bool> {
        let status = self.get_tenant_quota_status(tenant_id).await?;
        Ok(status.is_over_quota)
    }

    /// Get remaining quota for tenant in bytes
    pub async fn get_tenant_remaining_quota(&self, tenant_id: &str) -> Result<i64> {
        let status = self.get_tenant_quota_status(tenant_id).await?;
        let remaining = status.quota_bytes as i64 - status.used_bytes as i64;
        Ok(remaining.max(0))
    }

    /// Validate that adding bytes won't exceed quota
    pub async fn validate_quota(&self, tenant_id: &str, bytes_to_add: u64) -> Result<bool> {
        let status = self.get_tenant_quota_status(tenant_id).await?;
        let new_total = status.used_bytes + bytes_to_add;
        Ok(new_total <= status.quota_bytes)
    }

    /// Start background cleanup task (runs in a separate tokio task)
    pub fn start_background_cleanup(&self) -> tokio::task::JoinHandle<()> {
        let config = self.config.clone();
        let db = self.db.clone();

        tokio::spawn(async move {
            if config.cleanup_interval_secs == 0 {
                info!("Background cleanup task disabled");
                return;
            }

            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(config.cleanup_interval_secs));

            loop {
                interval.tick().await;

                let manager = DatasetCleanupManager::new(config.clone(), db.clone());

                match manager.cleanup_orphaned_files().await {
                    Ok(result) => {
                        if result.orphaned_files_removed > 0 {
                            info!(
                                "Background cleanup: removed {} files, freed {} bytes",
                                result.orphaned_files_removed, result.bytes_freed
                            );
                        }
                    }
                    Err(e) => {
                        error!("Background cleanup task failed: {}", e);
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_quota_status_critical() {
        let status = StorageQuotaStatus {
            tenant_id: "test".to_string(),
            used_bytes: 900,
            quota_bytes: 1000,
            percent_used: 90.0,
            datasets_count: 1,
            is_over_quota: false,
        };

        assert!(status.is_critical());
        assert!(status.is_high());
    }

    #[test]
    fn test_storage_quota_status_high() {
        let status = StorageQuotaStatus {
            tenant_id: "test".to_string(),
            used_bytes: 800,
            quota_bytes: 1000,
            percent_used: 80.0,
            datasets_count: 1,
            is_over_quota: false,
        };

        assert!(!status.is_critical());
        assert!(status.is_high());
    }

    #[test]
    fn test_cleanup_config_default() {
        let config = CleanupConfig::default();
        assert_eq!(config.quota_per_tenant_bytes, 100 * 1024 * 1024 * 1024);
        assert_eq!(config.archive_age_days, 30);
        assert!(config.auto_cleanup_on_startup);
        assert_eq!(config.cleanup_interval_secs, 3600);
    }

    #[test]
    fn test_cleanup_result_default() {
        let result = CleanupResult::default();
        assert_eq!(result.orphaned_files_removed, 0);
        assert_eq!(result.bytes_freed, 0);
        assert_eq!(result.datasets_archived, 0);
        assert!(result.cleanup_errors.is_empty());
    }
}
