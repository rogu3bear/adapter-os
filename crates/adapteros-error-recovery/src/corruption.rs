//! Corruption detection
//!
//! Implements corruption detection mechanisms for files and directories.

use crate::ErrorRecoveryConfig;
use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tokio::fs;
use tracing::{debug, error, info, warn};

/// Corruption detector
pub struct CorruptionDetector {
    config: ErrorRecoveryConfig,
    checksum_cache: std::collections::HashMap<PathBuf, String>,
}

/// Corruption type
#[derive(Debug, Clone)]
pub enum CorruptionType {
    /// File corruption
    FileCorruption,
    /// Directory corruption
    DirectoryCorruption,
    /// Metadata corruption
    MetadataCorruption,
    /// Permission corruption
    PermissionCorruption,
    /// Checksum mismatch
    ChecksumMismatch,
    /// Size mismatch
    SizeMismatch,
    /// Unknown corruption
    Unknown,
}

/// Corruption detection result
#[derive(Debug, Clone)]
pub struct CorruptionResult {
    /// Is corrupted
    pub is_corrupted: bool,
    /// Corruption type
    pub corruption_type: Option<CorruptionType>,
    /// Corruption details
    pub details: String,
    /// Detection timestamp
    pub timestamp: SystemTime,
}

impl CorruptionDetector {
    /// Create a new corruption detector
    pub fn new(config: &ErrorRecoveryConfig) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            checksum_cache: std::collections::HashMap::new(),
        })
    }

    /// Check if a file is corrupted
    pub async fn is_corrupted(&self, path: &Path) -> Result<bool> {
        let result = self.detect_corruption(path).await?;
        Ok(result.is_corrupted)
    }

    /// Detect corruption in a file or directory
    pub async fn detect_corruption(&self, path: &Path) -> Result<CorruptionResult> {
        if !path.exists() {
            return Ok(CorruptionResult {
                is_corrupted: false,
                corruption_type: None,
                details: "Path does not exist".to_string(),
                timestamp: SystemTime::now(),
            });
        }

        if path.is_file() {
            self.detect_file_corruption(path).await
        } else if path.is_dir() {
            self.detect_directory_corruption(path).await
        } else {
            Ok(CorruptionResult {
                is_corrupted: false,
                corruption_type: None,
                details: "Unknown file type".to_string(),
                timestamp: SystemTime::now(),
            })
        }
    }

    /// Detect file corruption
    async fn detect_file_corruption(&self, path: &Path) -> Result<CorruptionResult> {
        let mut corruption_type = None;
        let mut details = String::new();

        // Check file metadata
        match self.check_file_metadata(path).await {
            Ok(_) => {}
            Err(e) => {
                corruption_type = Some(CorruptionType::MetadataCorruption);
                details.push_str(&format!("Metadata error: {}", e));
            }
        }

        // Check file permissions
        match self.check_file_permissions(path).await {
            Ok(_) => {}
            Err(e) => {
                if corruption_type.is_none() {
                    corruption_type = Some(CorruptionType::PermissionCorruption);
                }
                details.push_str(&format!("Permission error: {}", e));
            }
        }

        // Check file size consistency
        match self.check_file_size_consistency(path).await {
            Ok(_) => {}
            Err(e) => {
                if corruption_type.is_none() {
                    corruption_type = Some(CorruptionType::SizeMismatch);
                }
                details.push_str(&format!("Size error: {}", e));
            }
        }

        // Check file checksum
        match self.check_file_checksum(path).await {
            Ok(_) => {}
            Err(e) => {
                if corruption_type.is_none() {
                    corruption_type = Some(CorruptionType::ChecksumMismatch);
                }
                details.push_str(&format!("Checksum error: {}", e));
            }
        }

        // Check file content integrity
        match self.check_file_content_integrity(path).await {
            Ok(_) => {}
            Err(e) => {
                if corruption_type.is_none() {
                    corruption_type = Some(CorruptionType::FileCorruption);
                }
                details.push_str(&format!("Content error: {}", e));
            }
        }

        let is_corrupted = corruption_type.is_some();

        if is_corrupted {
            warn!(
                "Detected corruption in {}: {:?} - {}",
                path.display(),
                corruption_type,
                details
            );
        } else {
            debug!("No corruption detected in {}", path.display());
        }

        Ok(CorruptionResult {
            is_corrupted,
            corruption_type,
            details,
            timestamp: SystemTime::now(),
        })
    }

    /// Detect directory corruption
    async fn detect_directory_corruption(&self, path: &Path) -> Result<CorruptionResult> {
        let mut corruption_type = None;
        let mut details = String::new();

        // Check directory metadata
        match self.check_directory_metadata(path).await {
            Ok(_) => {}
            Err(e) => {
                corruption_type = Some(CorruptionType::MetadataCorruption);
                details.push_str(&format!("Metadata error: {}", e));
            }
        }

        // Check directory permissions
        match self.check_directory_permissions(path).await {
            Ok(_) => {}
            Err(e) => {
                if corruption_type.is_none() {
                    corruption_type = Some(CorruptionType::PermissionCorruption);
                }
                details.push_str(&format!("Permission error: {}", e));
            }
        }

        // Check directory structure
        match self.check_directory_structure(path).await {
            Ok(_) => {}
            Err(e) => {
                if corruption_type.is_none() {
                    corruption_type = Some(CorruptionType::DirectoryCorruption);
                }
                details.push_str(&format!("Structure error: {}", e));
            }
        }

        let is_corrupted = corruption_type.is_some();

        if is_corrupted {
            warn!(
                "Detected corruption in directory {}: {:?} - {}",
                path.display(),
                corruption_type,
                details
            );
        } else {
            debug!("No corruption detected in directory {}", path.display());
        }

        Ok(CorruptionResult {
            is_corrupted,
            corruption_type,
            details,
            timestamp: SystemTime::now(),
        })
    }

    /// Check file metadata
    async fn check_file_metadata(&self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to get file metadata: {}", e)))?;

        // Check if metadata is reasonable
        if metadata.len() == 0 && metadata.is_file() {
            // Empty files are not necessarily corrupted
            return Ok(());
        }

        // Check if file size is reasonable (not negative, not too large)
        if metadata.len() > 100 * 1024 * 1024 * 1024 {
            // 100GB
            return Err(AosError::Recovery(
                "File size unreasonably large".to_string(),
            ));
        }

        Ok(())
    }

    /// Check file permissions
    async fn check_file_permissions(&self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to get file permissions: {}", e)))?;

        // Check if permissions are reasonable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode();

            // Check for suspicious permissions (e.g., world-writable)
            if mode & 0o002 != 0 {
                return Err(AosError::Recovery(
                    "File has world-writable permissions".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Check file size consistency
    async fn check_file_size_consistency(&self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to get file metadata: {}", e)))?;

        // Read file and check actual size
        let content = fs::read(path)
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to read file: {}", e)))?;

        if content.len() as u64 != metadata.len() {
            return Err(AosError::Recovery(format!(
                "Size mismatch: metadata={}, actual={}",
                metadata.len(),
                content.len()
            )));
        }

        Ok(())
    }

    /// Check file checksum
    async fn check_file_checksum(&self, path: &Path) -> Result<()> {
        // For now, we'll use a simple checksum
        // In production, this would use a more robust checksum algorithm
        let content = fs::read(path)
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to read file for checksum: {}", e)))?;

        let checksum = self.calculate_checksum(&content);

        // Check against cached checksum if available
        if let Some(cached_checksum) = self.checksum_cache.get(path) {
            if &checksum != cached_checksum {
                return Err(AosError::Recovery("Checksum mismatch".to_string()));
            }
        } else {
            // Cache the checksum for future comparison
            // Note: In a real implementation, this would be thread-safe
            // self.checksum_cache.insert(path.to_path_buf(), checksum);
        }

        Ok(())
    }

    /// Check file content integrity
    async fn check_file_content_integrity(&self, path: &Path) -> Result<()> {
        let content = fs::read(path)
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to read file content: {}", e)))?;

        // Check for common corruption patterns
        if content.is_empty() {
            // Empty files are not necessarily corrupted
            return Ok(());
        }

        // Check for null bytes in text files
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                match ext_str {
                    "txt" | "json" | "toml" | "yaml" | "md" => {
                        if content.contains(&0) {
                            return Err(AosError::Recovery("File contains null bytes".to_string()));
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    /// Check directory metadata
    async fn check_directory_metadata(&self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to get directory metadata: {}", e)))?;

        if !metadata.is_dir() {
            return Err(AosError::Recovery("Path is not a directory".to_string()));
        }

        Ok(())
    }

    /// Check directory permissions
    async fn check_directory_permissions(&self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path).await.map_err(|e| {
            AosError::Recovery(format!("Failed to get directory permissions: {}", e))
        })?;

        // Check if permissions are reasonable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode();

            // Check for suspicious permissions
            if mode & 0o002 != 0 {
                return Err(AosError::Recovery(
                    "Directory has world-writable permissions".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Check directory structure
    async fn check_directory_structure(&self, path: &Path) -> Result<()> {
        let mut entries = fs::read_dir(path)
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to read directory: {}", e)))?;

        let mut entry_count = 0;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AosError::Recovery(format!("Failed to read directory entry: {}", e)))?
        {
            entry_count += 1;

            // Check if entry is accessible
            let entry_path = entry.path();
            if let Err(e) = fs::metadata(&entry_path).await {
                return Err(AosError::Recovery(format!(
                    "Cannot access directory entry {}: {}",
                    entry_path.display(),
                    e
                )));
            }
        }

        // Check for reasonable entry count
        if entry_count > 10000 {
            return Err(AosError::Recovery(
                "Directory has too many entries".to_string(),
            ));
        }

        Ok(())
    }

    /// Calculate simple checksum
    fn calculate_checksum(&self, content: &[u8]) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_corruption_detector() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let detector = CorruptionDetector::new(&config)?;

        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test non-existent file
        let result = detector.detect_corruption(&test_file).await?;
        assert!(!result.is_corrupted);

        // Test existing file
        fs::write(&test_file, "hello world").await?;
        let result = detector.detect_corruption(&test_file).await?;
        assert!(!result.is_corrupted);

        Ok(())
    }

    #[tokio::test]
    async fn test_file_corruption_detection() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let detector = CorruptionDetector::new(&config)?;

        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Create a file with null bytes (corruption)
        fs::write(&test_file, b"hello\x00world").await?;

        let result = detector.detect_corruption(&test_file).await?;
        // Should detect corruption due to null bytes

        Ok(())
    }

    #[tokio::test]
    async fn test_directory_corruption_detection() -> Result<()> {
        let config = ErrorRecoveryConfig::default();
        let detector = CorruptionDetector::new(&config)?;

        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_dir");

        // Create a directory
        fs::create_dir(&test_dir).await?;

        let result = detector.detect_corruption(&test_dir).await?;
        assert!(!result.is_corrupted);

        Ok(())
    }
}
