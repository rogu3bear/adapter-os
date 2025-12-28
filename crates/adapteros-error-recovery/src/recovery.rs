//! Recovery engine
//!
//! Implements recovery mechanisms for corrupted files and directories.

use crate::{ErrorRecoveryConfig, RecoveryResult};
use adapteros_core::{AosError, Result};
use adapteros_platform::common::PlatformUtils;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;
use tracing::{debug, error, info, warn};

/// Recovery engine
pub struct RecoveryEngine {
    config: ErrorRecoveryConfig,
    backup_manager: BackupManager,
}

/// Backup manager
pub struct BackupManager {
    backup_dir: PathBuf,
    retention_count: u32,
}

impl RecoveryEngine {
    /// Create a new recovery engine
    pub fn new(config: &ErrorRecoveryConfig) -> Result<Self> {
        let backup_manager = BackupManager::new(config)?;

        Ok(Self {
            config: config.clone(),
            backup_manager,
        })
    }

    /// Restore file from backup
    pub async fn restore_from_backup(&self, path: &Path) -> Result<RecoveryResult> {
        if !self.config.enable_backup_restore {
            return Ok(RecoveryResult::Failed);
        }

        // Find the most recent backup
        let backup_path = self.backup_manager.find_latest_backup(path).await?;

        if backup_path.is_none() {
            warn!("No backup found for {}", path.display());
            return Ok(RecoveryResult::Failed);
        }

        let backup_path = backup_path.unwrap();

        // Restore the file
        match self.restore_file(&backup_path, path).await {
            Ok(_) => {
                info!("Successfully restored {} from backup", path.display());
                Ok(RecoveryResult::Success)
            }
            Err(e) => {
                error!("Failed to restore {} from backup: {}", path.display(), e);
                Ok(RecoveryResult::Failed)
            }
        }
    }

    /// Recreate a file with backup preservation.
    /// Attempts to backup the corrupted file before recreation so data can potentially be recovered.
    pub async fn recreate_file(&self, path: &Path) -> Result<RecoveryResult> {
        // Try to backup existing corrupted file first (preserves data for potential recovery)
        if path.exists() {
            match self.backup_manager.create_backup(path).await {
                Ok(backup_path) => {
                    info!(
                        "Backed up corrupted file to {} before recreation",
                        backup_path.display()
                    );
                }
                Err(e) => {
                    warn!(
                        "Could not backup corrupted file {} before recreation: {}",
                        path.display(),
                        e
                    );
                }
            }

            if let Err(e) = fs::remove_file(path).await {
                warn!("Failed to remove corrupted file {}: {}", path.display(), e);
            }
        }

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent).await;
        }

        // Create a new empty file
        // Note: Caller is responsible for repopulating content
        match fs::File::create(path).await {
            Ok(_) => {
                info!(
                    "Recreated file {} (backup available for data recovery if needed)",
                    path.display()
                );
                // Return PartialSuccess since original data was lost (even if backed up)
                Ok(RecoveryResult::PartialSuccess)
            }
            Err(e) => {
                error!("Failed to recreate file {}: {}", path.display(), e);
                Ok(RecoveryResult::Failed)
            }
        }
    }

    /// Recreate a directory
    pub async fn recreate_directory(&self, path: &Path) -> Result<RecoveryResult> {
        // Remove the corrupted directory
        if path.exists() {
            if let Err(e) = fs::remove_dir_all(path).await {
                warn!(
                    "Failed to remove corrupted directory {}: {}",
                    path.display(),
                    e
                );
            }
        }

        // Create a new directory
        match fs::create_dir_all(path).await {
            Ok(_) => {
                info!("Successfully recreated directory {}", path.display());
                Ok(RecoveryResult::Success)
            }
            Err(e) => {
                error!("Failed to recreate directory {}: {}", path.display(), e);
                Ok(RecoveryResult::Failed)
            }
        }
    }

    /// Restore a file from backup
    async fn restore_file(&self, backup_path: &Path, target_path: &Path) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| AosError::Io(format!("Failed to create parent directory: {}", e)))?;
        }

        // Copy backup to target location
        fs::copy(backup_path, target_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to restore file: {}", e)))?;

        debug!(
            "Restored file {} from backup {}",
            target_path.display(),
            backup_path.display()
        );
        Ok(())
    }
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new(config: &ErrorRecoveryConfig) -> Result<Self> {
        let backup_dir = PlatformUtils::aos_var_dir()
            .join("backups")
            .join("error-recovery");

        // Create backup directory if it doesn't exist
        if !backup_dir.exists() {
            std::fs::create_dir_all(&backup_dir)
                .map_err(|e| AosError::Io(format!("Failed to create backup directory: {}", e)))?;
        }

        Ok(Self {
            backup_dir,
            retention_count: config.backup_retention_count,
        })
    }

    /// Create a backup of a file
    pub async fn create_backup(&self, path: &Path) -> Result<PathBuf> {
        if !path.exists() {
            return Err(AosError::Io("File does not exist".to_string()));
        }

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let backup_filename = format!(
            "{}_{}_{}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            timestamp,
            path.to_string_lossy().replace(['/', '\\'], "_")
        );

        let backup_path = self.backup_dir.join(backup_filename);

        // Copy file to backup location
        fs::copy(path, &backup_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to create backup: {}", e)))?;

        // Clean up old backups
        self.cleanup_old_backups(path).await?;

        debug!("Created backup: {}", backup_path.display());
        Ok(backup_path)
    }

    /// Find the latest backup for a file
    pub async fn find_latest_backup(&self, path: &Path) -> Result<Option<PathBuf>> {
        let file_name = path
            .file_name()
            .ok_or_else(|| AosError::Io("Invalid file path".to_string()))?
            .to_string_lossy();

        let mut entries = fs::read_dir(&self.backup_dir)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read backup directory: {}", e)))?;

        let mut latest_backup: Option<(PathBuf, SystemTime)> = None;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read backup entry: {}", e)))?
        {
            let entry_path = entry.path();

            if let Some(entry_name) = entry_path.file_name() {
                if entry_name.to_string_lossy().starts_with(&*file_name) {
                    if let Ok(metadata) = entry.metadata().await {
                        if let Ok(modified) = metadata.modified() {
                            if latest_backup.is_none()
                                || modified > latest_backup.as_ref().unwrap().1
                            {
                                latest_backup = Some((entry_path, modified));
                            }
                        }
                    }
                }
            }
        }

        Ok(latest_backup.map(|(path, _)| path))
    }

    /// Clean up old backups
    async fn cleanup_old_backups(&self, path: &Path) -> Result<()> {
        let file_name = path
            .file_name()
            .ok_or_else(|| AosError::Io("Invalid file path".to_string()))?
            .to_string_lossy();

        let mut entries = fs::read_dir(&self.backup_dir)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read backup directory: {}", e)))?;

        let mut backups: Vec<(PathBuf, SystemTime)> = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read backup entry: {}", e)))?
        {
            let entry_path = entry.path();

            if let Some(entry_name) = entry_path.file_name() {
                if entry_name.to_string_lossy().starts_with(&*file_name) {
                    if let Ok(metadata) = entry.metadata().await {
                        if let Ok(modified) = metadata.modified() {
                            backups.push((entry_path, modified));
                        }
                    }
                }
            }
        }

        // Sort by modification time (newest first)
        backups.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove old backups
        for (backup_path, _) in backups.into_iter().skip(self.retention_count as usize) {
            if let Err(e) = fs::remove_file(&backup_path).await {
                warn!(
                    "Failed to remove old backup {}: {}",
                    backup_path.display(),
                    e
                );
            } else {
                debug!("Removed old backup: {}", backup_path.display());
            }
        }

        Ok(())
    }

    /// List all backups for a file
    pub async fn list_backups(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let file_name = path
            .file_name()
            .ok_or_else(|| AosError::Io("Invalid file path".to_string()))?
            .to_string_lossy();

        let mut entries = fs::read_dir(&self.backup_dir)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read backup directory: {}", e)))?;

        let mut backups = Vec::new();

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read backup entry: {}", e)))?
        {
            let entry_path = entry.path();

            if let Some(entry_name) = entry_path.file_name() {
                if entry_name.to_string_lossy().starts_with(&*file_name) {
                    backups.push(entry_path);
                }
            }
        }

        Ok(backups)
    }

    /// Get backup directory
    pub fn backup_dir(&self) -> &PathBuf {
        &self.backup_dir
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
    async fn test_recovery_engine() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let engine = RecoveryEngine::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test file recreation
        fs::write(&test_file, "hello world").await?;
        let result = engine.recreate_file(&test_file).await?;
        assert!(matches!(result, RecoveryResult::Success));

        // Verify file was recreated
        let content = fs::read_to_string(&test_file).await?;
        assert_eq!(content, "");

        Ok(())
    }

    #[tokio::test]
    async fn test_backup_manager() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let manager = BackupManager::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "hello world").await?;

        // Test backup creation
        let backup_path = manager.create_backup(&test_file).await?;
        assert!(backup_path.exists());

        // Test finding latest backup
        let latest_backup = manager.find_latest_backup(&test_file).await?;
        assert!(latest_backup.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_directory_recreation() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let engine = RecoveryEngine::new(&config)?;

        let temp_dir = new_test_tempdir()?;
        let test_dir = temp_dir.path().join("test_dir");

        // Test directory recreation
        fs::create_dir(&test_dir).await?;
        let result = engine.recreate_directory(&test_dir).await?;
        assert!(matches!(result, RecoveryResult::Success));

        // Verify directory was recreated
        assert!(test_dir.exists());
        assert!(test_dir.is_dir());

        Ok(())
    }
}
