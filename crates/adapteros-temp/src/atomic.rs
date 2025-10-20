//! Atomic file operations
//!
//! Provides atomic file operations for safe concurrent access.

use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// Atomic file writer
pub struct AtomicFileWriter {
    temp_path: PathBuf,
    final_path: PathBuf,
}

impl AtomicFileWriter {
    /// Create a new atomic file writer
    pub fn new(final_path: PathBuf) -> Result<Self> {
        let temp_path = final_path.with_extension(format!(
            "{}.tmp.{}",
            final_path.extension().unwrap_or_default().to_string_lossy(),
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
        ));

        Ok(Self {
            temp_path,
            final_path,
        })
    }

    /// Write data to the temporary file
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        fs::write(&self.temp_path, data).await
            .map_err(|e| AosError::Io(format!("Failed to write to temp file: {}", e)))?;
        Ok(())
    }

    /// Append data to the temporary file
    pub async fn append(&self, data: &[u8]) -> Result<()> {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.temp_path)
            .await
            .map_err(|e| AosError::Io(format!("Failed to open temp file for append: {}", e)))?
            .write_all(data)
            .await
            .map_err(|e| AosError::Io(format!("Failed to append to temp file: {}", e)))?;
        Ok(())
    }

    /// Commit the file atomically
    pub async fn commit(self) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = self.final_path.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| AosError::Io(format!("Failed to create parent directory: {}", e)))?;
        }

        // Atomic rename
        fs::rename(&self.temp_path, &self.final_path).await
            .map_err(|e| AosError::Io(format!("Failed to commit file atomically: {}", e)))?;

        debug!("Atomically committed file: {}", self.final_path.display());
        Ok(())
    }

    /// Abort the operation and clean up
    pub async fn abort(self) -> Result<()> {
        if self.temp_path.exists() {
            fs::remove_file(&self.temp_path).await
                .map_err(|e| AosError::Io(format!("Failed to remove temp file: {}", e)))?;
            debug!("Aborted atomic file operation: {}", self.temp_path.display());
        }
        Ok(())
    }
}

/// Atomic file reader
pub struct AtomicFileReader {
    path: PathBuf,
    backup_path: Option<PathBuf>,
}

impl AtomicFileReader {
    /// Create a new atomic file reader
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            backup_path: None,
        }
    }

    /// Create a backup of the file before reading
    pub async fn with_backup(mut self) -> Result<Self> {
        let backup_path = self.path.with_extension(format!(
            "{}.backup.{}",
            self.path.extension().unwrap_or_default().to_string_lossy(),
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos()
        ));

        fs::copy(&self.path, &backup_path).await
            .map_err(|e| AosError::Io(format!("Failed to create backup: {}", e)))?;

        self.backup_path = Some(backup_path);
        Ok(self)
    }

    /// Read the file
    pub async fn read(&self) -> Result<Vec<u8>> {
        fs::read(&self.path).await
            .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?;
        Ok(fs::read(&self.path).await
            .map_err(|e| AosError::Io(format!("Failed to read file: {}", e)))?)
    }

    /// Read the file as a string
    pub async fn read_string(&self) -> Result<String> {
        fs::read_to_string(&self.path).await
            .map_err(|e| AosError::Io(format!("Failed to read file as string: {}", e)))?;
        Ok(fs::read_to_string(&self.path).await
            .map_err(|e| AosError::Io(format!("Failed to read file as string: {}", e)))?)
    }

    /// Get file metadata
    pub async fn metadata(&self) -> Result<std::fs::Metadata> {
        fs::metadata(&self.path).await
            .map_err(|e| AosError::Io(format!("Failed to get file metadata: {}", e)))?;
        Ok(fs::metadata(&self.path).await
            .map_err(|e| AosError::Io(format!("Failed to get file metadata: {}", e)))?)
    }
}

impl Drop for AtomicFileReader {
    fn drop(&mut self) {
        // Clean up backup if it exists
        if let Some(backup_path) = &self.backup_path {
            if backup_path.exists() {
                if let Err(e) = std::fs::remove_file(backup_path) {
                    tracing::warn!("Failed to remove backup file {}: {}", backup_path.display(), e);
                } else {
                    debug!("Cleaned up backup file: {}", backup_path.display());
                }
            }
        }
    }
}

/// Atomic directory operations
pub struct AtomicDirOperations;

impl AtomicDirOperations {
    /// Move a directory atomically
    pub async fn move_atomic(src: PathBuf, dst: PathBuf) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| AosError::Io(format!("Failed to create parent directory: {}", e)))?;
        }

        // Atomic rename
        fs::rename(&src, &dst).await
            .map_err(|e| AosError::Io(format!("Failed to move directory atomically: {}", e)))?;

        debug!("Atomically moved directory: {} -> {}", src.display(), dst.display());
        Ok(())
    }

    /// Copy a directory atomically
    pub async fn copy_atomic(src: PathBuf, dst: PathBuf) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).await
                .map_err(|e| AosError::Io(format!("Failed to create parent directory: {}", e)))?;
        }

        // Copy directory recursively
        Self::copy_directory_recursive(&src, &dst).await?;

        debug!("Atomically copied directory: {} -> {}", src.display(), dst.display());
        Ok(())
    }

    /// Copy directory recursively
    async fn copy_directory_recursive(src: &Path, dst: &Path) -> Result<()> {
        fs::create_dir_all(dst).await
            .map_err(|e| AosError::Io(format!("Failed to create destination directory: {}", e)))?;

        let mut entries = fs::read_dir(src).await
            .map_err(|e| AosError::Io(format!("Failed to read source directory: {}", e)))?;

        while let Some(entry) = entries.next_entry().await
            .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))? {
            let entry_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if entry_path.is_file() {
                fs::copy(&entry_path, &dst_path).await
                    .map_err(|e| AosError::Io(format!("Failed to copy file: {}", e)))?;
            } else if entry_path.is_dir() {
                Self::copy_directory_recursive(&entry_path, &dst_path).await?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_atomic_file_writer() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");

        let writer = AtomicFileWriter::new(file_path.clone())?;
        writer.write(b"hello world").await?;
        writer.commit().await?;

        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).await?;
        assert_eq!(content, "hello world");

        Ok(())
    }

    #[tokio::test]
    async fn test_atomic_file_reader() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"hello world").await?;

        let reader = AtomicFileReader::new(file_path);
        let content = reader.read().await?;
        assert_eq!(content, b"hello world");

        Ok(())
    }

    #[tokio::test]
    async fn test_atomic_directory_operations() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let src_dir = temp_dir.path().join("src");
        let dst_dir = temp_dir.path().join("dst");

        fs::create_dir_all(&src_dir).await?;
        fs::write(src_dir.join("file1.txt"), b"content1").await?;
        fs::write(src_dir.join("file2.txt"), b"content2").await?;

        AtomicDirOperations::move_atomic(src_dir.clone(), dst_dir.clone()).await?;

        assert!(!src_dir.exists());
        assert!(dst_dir.exists());
        assert!(dst_dir.join("file1.txt").exists());
        assert!(dst_dir.join("file2.txt").exists());

        Ok(())
    }
}
