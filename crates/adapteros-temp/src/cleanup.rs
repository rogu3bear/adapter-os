//! Cleanup utilities for temporary files
//!
//! Provides utilities for cleaning up temporary files and directories.

use adapteros_core::{AosError, Result};
use std::path::Path;
use tokio::fs;

/// Cleanup utilities
pub struct CleanupUtils;

impl CleanupUtils {
    /// Remove a temporary file or directory
    pub async fn remove_temp_path(path: &Path) -> Result<()> {
        if path.is_file() {
            fs::remove_file(path).await.map_err(|e| {
                AosError::Io(format!(
                    "Failed to remove temp file {}: {}",
                    path.display(),
                    e
                ))
            })?;
        } else if path.is_dir() {
            fs::remove_dir_all(path).await.map_err(|e| {
                AosError::Io(format!(
                    "Failed to remove temp directory {}: {}",
                    path.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }

    /// Clean up old temporary files
    pub async fn cleanup_old_files(temp_dir: &Path, max_age_seconds: u64) -> Result<()> {
        let mut entries = fs::read_dir(temp_dir).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to read temp directory {}: {}",
                temp_dir.display(),
                e
            ))
        })?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?
        {
            let entry_path = entry.path();
            let metadata = entry.metadata().await.map_err(|e| {
                AosError::Io(format!(
                    "Failed to get metadata for {}: {}",
                    entry_path.display(),
                    e
                ))
            })?;

            if let Ok(modified) = metadata.modified() {
                if let Ok(age) = std::time::SystemTime::now().duration_since(modified) {
                    if age.as_secs() > max_age_seconds {
                        Self::remove_temp_path(&entry_path).await?;
                    }
                }
            }
        }

        Ok(())
    }
}
