//! Temporary file manager
//!
//! Manages temporary files and directories with automatic cleanup.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::sync::RwLock;

/// Information about a temporary file
#[derive(Debug, Clone)]
pub struct TempFileInfo {
    pub path: PathBuf,
    pub created_at: SystemTime,
    pub size: u64,
}

/// Temporary file manager
pub struct TempFileManager {
    temp_dir: PathBuf,
    active_files: RwLock<HashMap<String, TempFileInfo>>,
    cleanup_interval: Duration,
}

impl TempFileManager {
    /// Create a new temporary file manager
    pub fn new(temp_dir: PathBuf) -> Result<Self> {
        Ok(Self {
            temp_dir,
            active_files: RwLock::new(HashMap::new()),
            cleanup_interval: Duration::from_secs(3600), // 1 hour
        })
    }

    /// Get the temporary directory path
    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }

    /// Create a temporary file
    pub async fn create_temp_file(&self, prefix: &str, suffix: &str) -> Result<PathBuf> {
        let file_name = format!(
            "{}_{}{}",
            prefix,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            suffix
        );
        let file_path = self.temp_dir.join(file_name);

        // Create the file
        fs::File::create(&file_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to create temp file {}: {}",
                file_path.display(),
                e
            ))
        })?;

        // Track the file
        let file_id = file_path.to_string_lossy().to_string();
        let file_info = TempFileInfo {
            path: file_path.clone(),
            created_at: SystemTime::now(),
            size: 0,
        };

        {
            let mut files = self.active_files.write().await;
            files.insert(file_id, file_info);
        }

        Ok(file_path)
    }

    /// Remove a temporary file
    pub async fn remove_temp_file(&self, file_path: &Path) -> Result<()> {
        // Remove from tracking
        let file_id = file_path.to_string_lossy().to_string();
        {
            let mut files = self.active_files.write().await;
            files.remove(&file_id);
        }

        // Remove the actual file
        fs::remove_file(file_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to remove temp file {}: {}",
                file_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    /// Get information about active temporary files
    pub async fn get_active_files(&self) -> HashMap<String, TempFileInfo> {
        self.active_files.read().await.clone()
    }

    /// Clean up old temporary files
    pub async fn cleanup_old_files(&self, max_age: Duration) -> Result<()> {
        let now = SystemTime::now();
        let mut files_to_remove = Vec::new();

        {
            let files = self.active_files.read().await;
            for (file_id, file_info) in files.iter() {
                if let Ok(age) = now.duration_since(file_info.created_at) {
                    if age > max_age {
                        files_to_remove.push(file_id.clone());
                    }
                }
            }
        }

        // Remove old files
        for file_id in files_to_remove {
            if let Some(file_info) = self.active_files.write().await.remove(&file_id) {
                let _ = fs::remove_file(&file_info.path).await; // Ignore errors
            }
        }

        Ok(())
    }
}
