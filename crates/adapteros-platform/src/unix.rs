//! Unix-like platform filesystem operations
//!
//! Implements Unix-like platform filesystem operations and features.

use crate::{FileMetadata, FileType, PlatformAttributes, PlatformHandler};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::debug;

/// Unix-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnixSettings {
    /// Enable Unix-specific features
    pub enable_features: bool,
    /// Use POSIX file locking
    pub use_posix_locking: bool,
    /// Enable Unix security features
    pub enable_security: bool,
    /// Default file mode
    pub default_file_mode: u32,
    /// Default directory mode
    pub default_dir_mode: u32,
}

/// Unix-specific attributes
#[derive(Debug, Clone)]
pub struct UnixAttributes {
    /// File mode (permissions)
    pub mode: u32,
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Device ID
    pub dev: u64,
    /// Inode number
    pub ino: u64,
    /// Number of hard links
    pub nlink: u64,
    /// Block size
    pub blksize: u64,
    /// Number of blocks
    pub blocks: u64,
}

/// Unix platform handler
pub struct UnixHandler {
    settings: UnixSettings,
}

impl UnixHandler {
    /// Create a new Unix handler
    pub fn new(settings: Option<&UnixSettings>) -> Result<Self> {
        let settings = settings.cloned().unwrap_or_else(UnixSettings::default);
        Ok(Self { settings })
    }
}

impl PlatformHandler for UnixHandler {
    fn platform_name(&self) -> &str {
        "Unix"
    }

    fn is_feature_supported(&self, feature: &str) -> bool {
        match feature {
            "symlinks" => true,
            "hardlinks" => true,
            "file_locking" => self.settings.use_posix_locking,
            "posix_permissions" => true,
            "case_sensitive" => true,
            "extended_attributes" => true,
            "access_control_lists" => true,
            _ => false,
        }
    }

    fn path_separator(&self) -> char {
        '/'
    }

    fn normalize_path(&self, path: &Path) -> Result<PathBuf> {
        // Unix path normalization
        let normalized = path.to_path_buf();

        // Convert to canonical path if possible
        if normalized.exists() {
            normalized
                .canonicalize()
                .map_err(|e| AosError::Platform(format!("Failed to canonicalize Unix path: {}", e)))
        } else {
            Ok(normalized)
        }
    }

    fn set_file_permissions(&self, path: &Path, permissions: u32) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(permissions);
            std::fs::set_permissions(path, perms).map_err(|e| {
                AosError::Platform(format!("Failed to set Unix file permissions: {}", e))
            })?;
        }

        #[cfg(not(unix))]
        {
            return Err(AosError::Platform(
                "Unix permissions not available on this platform".to_string(),
            ));
        }

        debug!("Set Unix file permissions: {:o}", permissions);
        Ok(())
    }

    fn get_file_permissions(&self, path: &Path) -> Result<u32> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(path).map_err(|e| {
                AosError::Platform(format!("Failed to get Unix file metadata: {}", e))
            })?;
            Ok(metadata.permissions().mode())
        }

        #[cfg(not(unix))]
        {
            Err(AosError::Platform(
                "Unix permissions not available on this platform".to_string(),
            ))
        }
    }

    fn create_symlink(&self, target: &Path, link: &Path) -> Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
                .map_err(|e| AosError::Platform(format!("Failed to create Unix symlink: {}", e)))?;
        }

        #[cfg(not(unix))]
        {
            return Err(AosError::Platform(
                "Unix symlinks not available on this platform".to_string(),
            ));
        }

        debug!(
            "Created Unix symlink: {} -> {}",
            link.display(),
            target.display()
        );
        Ok(())
    }

    fn read_symlink(&self, link: &Path) -> Result<PathBuf> {
        #[cfg(unix)]
        {
            std::fs::read_link(link)
                .map_err(|e| AosError::Platform(format!("Failed to read Unix symlink: {}", e)))
        }

        #[cfg(not(unix))]
        {
            Err(AosError::Platform(
                "Unix symlinks not available on this platform".to_string(),
            ))
        }
    }

    fn is_symlink(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            path.is_symlink()
        }

        #[cfg(not(unix))]
        {
            false
        }
    }

    fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| AosError::Platform(format!("Failed to get Unix file metadata: {}", e)))?;

        let file_type = if metadata.is_file() {
            FileType::File
        } else if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::Symlink
        } else {
            FileType::Unknown
        };

        #[cfg(unix)]
        let platform_attributes = {
            use std::os::unix::fs::MetadataExt;
            PlatformAttributes::Unix(UnixAttributes {
                mode: metadata.mode(),
                uid: metadata.uid(),
                gid: metadata.gid(),
                dev: metadata.dev(),
                ino: metadata.ino(),
                nlink: metadata.nlink(),
                blksize: metadata.blksize(),
                blocks: metadata.blocks(),
            })
        };

        #[cfg(not(unix))]
        let platform_attributes = PlatformAttributes::Unix(UnixAttributes {
            mode: 0,
            uid: 0,
            gid: 0,
            dev: 0,
            ino: 0,
            nlink: 0,
            blksize: 0,
            blocks: 0,
        });

        Ok(FileMetadata {
            size: metadata.len(),
            permissions: self.get_file_permissions(path)?,
            created: metadata
                .created()
                .unwrap_or_else(|_| SystemTime::UNIX_EPOCH),
            modified: metadata
                .modified()
                .unwrap_or_else(|_| SystemTime::UNIX_EPOCH),
            accessed: metadata
                .accessed()
                .unwrap_or_else(|_| SystemTime::UNIX_EPOCH),
            file_type,
            platform_attributes,
        })
    }

    fn set_file_metadata(&self, path: &Path, metadata: &FileMetadata) -> Result<()> {
        // Set file permissions
        self.set_file_permissions(path, metadata.permissions)?;

        // Set file times
        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            use std::os::unix::fs::MetadataExt;
            use std::os::unix::fs::OpenOptionsExt;

            let file = OpenOptions::new().write(true).open(path).map_err(|e| {
                AosError::Platform(format!("Failed to open file for metadata update: {}", e))
            })?;

            // Set file times using Unix system calls
            // This would require additional Unix API bindings
            debug!("Unix file metadata update not fully implemented");
        }

        #[cfg(not(unix))]
        {
            debug!("Unix file metadata update not available on this platform");
        }

        Ok(())
    }
}

impl Default for UnixSettings {
    fn default() -> Self {
        Self {
            enable_features: true,
            use_posix_locking: true,
            enable_security: true,
            default_file_mode: 0o644,
            default_dir_mode: 0o755,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_unix_handler() -> Result<()> {
        let handler = UnixHandler::new(None)?;

        assert_eq!(handler.platform_name(), "Unix");
        assert_eq!(handler.path_separator(), '/');
        assert!(handler.is_feature_supported("symlinks"));
        assert!(handler.is_feature_supported("hardlinks"));
        assert!(handler.is_feature_supported("posix_permissions"));
        assert!(handler.is_feature_supported("case_sensitive"));

        Ok(())
    }

    #[test]
    fn test_unix_path_normalization() -> Result<()> {
        let handler = UnixHandler::new(None)?;
        let temp_dir = TempDir::new()?;
        let test_path = temp_dir.path().join("test.txt");

        let normalized = handler.normalize_path(&test_path)?;
        assert_eq!(normalized, test_path);

        Ok(())
    }

    #[test]
    fn test_unix_file_metadata() -> Result<()> {
        let handler = UnixHandler::new(None)?;
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello")?;

        let metadata = handler.get_file_metadata(&test_file)?;
        assert_eq!(metadata.size, 5);
        assert!(matches!(metadata.file_type, FileType::File));

        Ok(())
    }
}
