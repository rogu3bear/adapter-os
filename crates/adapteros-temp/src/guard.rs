//! Temporary file guards for guaranteed cleanup
//!
//! Provides RAII guards for temporary files and directories with automatic cleanup.

use crate::TempManager;
use adapteros_core::{AosError, Result};
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

/// Guard for a temporary file with automatic cleanup
pub struct TempFileGuard<'a> {
    pub file_id: String,
    pub path: PathBuf,
    pub manager: &'a TempManager,
}

impl<'a> TempFileGuard<'a> {
    /// Get the file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Write data to the file
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        fs::write(&self.path, data)
            .await
            .map_err(|e| AosError::Io(format!("Failed to write to temp file: {}", e)))?;
        Ok(())
    }

    /// Read data from the file
    pub async fn read(&self) -> Result<Vec<u8>> {
        fs::read(&self.path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read temp file: {}", e)))?;
        Ok(fs::read(&self.path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to read temp file: {}", e)))?)
    }

    /// Get file metadata
    pub async fn metadata(&self) -> Result<std::fs::Metadata> {
        fs::metadata(&self.path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to get temp file metadata: {}", e)))?;
        Ok(fs::metadata(&self.path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to get temp file metadata: {}", e)))?)
    }

    /// Move the file to a permanent location
    pub async fn move_to(self, destination: PathBuf) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| AosError::Io(format!("Failed to create parent directory: {}", e)))?;
        }

        // Move the file
        fs::rename(&self.path, &destination).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to move temp file to {}: {}",
                destination.display(),
                e
            ))
        })?;

        debug!(
            "Moved temp file {} to {}",
            self.path.display(),
            destination.display()
        );

        // Don't clean up since we moved it
        std::mem::forget(self);
        Ok(())
    }

    /// Copy the file to a permanent location
    pub async fn copy_to(&self, destination: PathBuf) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| AosError::Io(format!("Failed to create parent directory: {}", e)))?;
        }

        // Copy the file
        fs::copy(&self.path, &destination).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to copy temp file to {}: {}",
                destination.display(),
                e
            ))
        })?;

        debug!(
            "Copied temp file {} to {}",
            self.path.display(),
            destination.display()
        );
        Ok(())
    }
}

impl<'a> Drop for TempFileGuard<'a> {
    fn drop(&mut self) {
        // Note: We can't await in Drop, so we use blocking operations
        if self.path.exists() {
            if let Err(e) = std::fs::remove_file(&self.path) {
                tracing::warn!("Failed to remove temp file {}: {}", self.path.display(), e);
            } else {
                debug!("Cleaned up temp file: {}", self.path.display());
            }
        }

        // Note: We can't remove from active files in Drop due to lifetime constraints
        // The cleanup task will handle this
    }
}

/// Guard for a temporary directory with automatic cleanup
pub struct TempDirGuard<'a> {
    pub dir_id: String,
    pub path: PathBuf,
    pub manager: &'a TempManager,
}

impl<'a> TempDirGuard<'a> {
    /// Get the directory path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Create a file in the temporary directory
    pub async fn create_file(&self, name: &str) -> Result<TempFileGuard<'a>> {
        let file_path = self.path.join(name);
        let file_id = format!("{}_{}", self.dir_id, name);

        // Create the file
        fs::write(&file_path, b"")
            .await
            .map_err(|e| AosError::Io(format!("Failed to create file in temp directory: {}", e)))?;

        // Set secure permissions if enabled
        if self.manager.config.secure_permissions {
            self.manager.set_secure_permissions(&file_path).await?;
        }

        debug!(
            "Created file {} in temp directory {}",
            name,
            self.path.display()
        );

        Ok(TempFileGuard {
            file_id,
            path: file_path,
            manager: self.manager,
        })
    }

    /// Create a subdirectory in the temporary directory
    pub async fn create_subdir(&self, name: &str) -> Result<TempDirGuard<'a>> {
        let subdir_path = self.path.join(name);

        // Create the subdirectory
        fs::create_dir_all(&subdir_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to create subdirectory in temp directory: {}",
                e
            ))
        })?;

        // Set secure permissions if enabled
        if self.manager.config.secure_permissions {
            self.manager.set_secure_permissions(&subdir_path).await?;
        }

        debug!(
            "Created subdirectory {} in temp directory {}",
            name,
            self.path.display()
        );

        Ok(TempDirGuard {
            dir_id: format!("{}_{}", self.dir_id, name),
            path: subdir_path,
            manager: self.manager,
        })
    }

    /// Move the directory to a permanent location
    pub async fn move_to(self, destination: PathBuf) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| AosError::Io(format!("Failed to create parent directory: {}", e)))?;
        }

        // Move the directory
        fs::rename(&self.path, &destination).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to move temp directory to {}: {}",
                destination.display(),
                e
            ))
        })?;

        debug!(
            "Moved temp directory {} to {}",
            self.path.display(),
            destination.display()
        );

        // Don't clean up since we moved it
        std::mem::forget(self);
        Ok(())
    }

    /// Copy the directory to a permanent location
    pub async fn copy_to(&self, destination: PathBuf) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| AosError::Io(format!("Failed to create parent directory: {}", e)))?;
        }

        // Copy the directory recursively
        self.copy_directory_recursive(&self.path, &destination)
            .await?;

        debug!(
            "Copied temp directory {} to {}",
            self.path.display(),
            destination.display()
        );
        Ok(())
    }

    /// Copy directory recursively
    async fn copy_directory_recursive(&self, src: &PathBuf, dst: &PathBuf) -> Result<()> {
        Box::pin(async move {
            fs::create_dir_all(dst).await.map_err(|e| {
                AosError::Io(format!("Failed to create destination directory: {}", e))
            })?;

            let mut entries = fs::read_dir(src)
                .await
                .map_err(|e| AosError::Io(format!("Failed to read source directory: {}", e)))?;

            while let Some(entry) = entries
                .next_entry()
                .await
                .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?
            {
                let entry_path = entry.path();
                let dst_path = dst.join(entry.file_name());

                if entry_path.is_file() {
                    fs::copy(&entry_path, &dst_path)
                        .await
                        .map_err(|e| AosError::Io(format!("Failed to copy file: {}", e)))?;
                } else if entry_path.is_dir() {
                    self.copy_directory_recursive(&entry_path, &dst_path)
                        .await?;
                }
            }

            Ok(())
        })
        .await
    }
}

impl<'a> Drop for TempDirGuard<'a> {
    fn drop(&mut self) {
        // Note: We can't await in Drop, so we use blocking operations
        if self.path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&self.path) {
                tracing::warn!(
                    "Failed to remove temp directory {}: {}",
                    self.path.display(),
                    e
                );
            } else {
                debug!("Cleaned up temp directory: {}", self.path.display());
            }
        }

        // Note: We can't remove from active directories in Drop due to lifetime constraints
        // The cleanup task will handle this
    }
}
