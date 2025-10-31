//! Linux-specific filesystem operations
//!
//! Implements Linux-specific filesystem operations and features.

use crate::{FileMetadata, FileType, PlatformAttributes, PlatformHandler};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::debug;

/// Linux-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinuxSettings {
    /// Enable Linux-specific features
    pub enable_features: bool,
    /// Use Linux file locking
    pub use_file_locking: bool,
    /// Enable Linux security features
    pub enable_security: bool,
    /// Enable extended attributes
    pub enable_extended_attributes: bool,
    /// Enable access control lists
    pub enable_access_control_lists: bool,
    /// Enable capabilities
    pub enable_capabilities: bool,
    /// Default file mode
    pub default_file_mode: u32,
    /// Default directory mode
    pub default_dir_mode: u32,
}

/// Linux-specific attributes
#[derive(Debug, Clone)]
pub struct LinuxAttributes {
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
    /// Extended attributes
    pub extended_attributes: Vec<(String, Vec<u8>)>,
    /// Access control list
    pub access_control_list: Option<Vec<u8>>,
    /// Capabilities
    pub capabilities: Option<Vec<u8>>,
}

/// Linux platform handler
pub struct LinuxHandler {
    settings: LinuxSettings,
}

impl LinuxHandler {
    /// Create a new Linux handler
    pub fn new(settings: Option<&LinuxSettings>) -> Result<Self> {
        let settings = settings.cloned().unwrap_or_else(LinuxSettings::default);
        Ok(Self { settings })
    }
}

impl PlatformHandler for LinuxHandler {
    fn platform_name(&self) -> &str {
        "Linux"
    }

    fn is_feature_supported(&self, feature: &str) -> bool {
        match feature {
            "symlinks" => true,
            "hardlinks" => true,
            "file_locking" => self.settings.use_file_locking,
            "posix_permissions" => true,
            "case_sensitive" => true,
            "extended_attributes" => self.settings.enable_extended_attributes,
            "access_control_lists" => self.settings.enable_access_control_lists,
            "capabilities" => self.settings.enable_capabilities,
            "inotify" => true,
            "fanotify" => true,
            "seccomp" => true,
            _ => false,
        }
    }

    fn path_separator(&self) -> char {
        '/'
    }

    fn normalize_path(&self, path: &Path) -> Result<PathBuf> {
        // Linux path normalization
        let normalized = path.to_path_buf();

        // Convert to canonical path if possible
        if normalized.exists() {
            normalized.canonicalize().map_err(|e| {
                AosError::Platform(format!("Failed to canonicalize Linux path: {}", e))
            })
        } else {
            Ok(normalized)
        }
    }

    fn set_file_permissions(&self, _path: &Path, _permissions: u32) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(_permissions);
            std::fs::set_permissions(_path, perms).map_err(|e| {
                AosError::Platform(format!("Failed to set Linux file permissions: {}", e))
            })?;
            debug!("Set Linux file permissions: {:o}", _permissions);
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(AosError::Platform(
                "Linux permissions not available on this platform".to_string(),
            ))
        }
    }

    fn get_file_permissions(&self, _path: &Path) -> Result<u32> {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(_path).map_err(|e| {
                AosError::Platform(format!("Failed to get Linux file metadata: {}", e))
            })?;
            Ok(metadata.permissions().mode())
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(AosError::Platform(
                "Linux permissions not available on this platform".to_string(),
            ))
        }
    }

    fn create_symlink(&self, _target: &Path, _link: &Path) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            std::os::unix::fs::symlink(_target, _link).map_err(|e| {
                AosError::Platform(format!("Failed to create Linux symlink: {}", e))
            })?;
            debug!(
                "Created Linux symlink: {} -> {}",
                _link.display(),
                _target.display()
            );
            Ok(())
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(AosError::Platform(
                "Linux symlinks not available on this platform".to_string(),
            ))
        }
    }

    fn read_symlink(&self, _link: &Path) -> Result<PathBuf> {
        #[cfg(target_os = "linux")]
        {
            std::fs::read_link(_link)
                .map_err(|e| AosError::Platform(format!("Failed to read Linux symlink: {}", e)))
        }

        #[cfg(not(target_os = "linux"))]
        {
            Err(AosError::Platform(
                "Linux symlinks not available on this platform".to_string(),
            ))
        }
    }

    fn is_symlink(&self, _path: &Path) -> bool {
        #[cfg(target_os = "linux")]
        {
            _path.is_symlink()
        }

        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| AosError::Platform(format!("Failed to get Linux file metadata: {}", e)))?;

        let file_type = if metadata.is_file() {
            FileType::File
        } else if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::Symlink
        } else {
            FileType::Unknown
        };

        #[cfg(target_os = "linux")]
        let platform_attributes = {
            use std::os::unix::fs::MetadataExt;
            PlatformAttributes::Linux(LinuxAttributes {
                mode: metadata.mode(),
                uid: metadata.uid(),
                gid: metadata.gid(),
                dev: metadata.dev(),
                ino: metadata.ino(),
                nlink: metadata.nlink(),
                blksize: metadata.blksize(),
                blocks: metadata.blocks(),
                extended_attributes: Vec::new(), // Would need Linux API to get this
                access_control_list: None,       // Would need Linux API to get this
                capabilities: None,              // Would need Linux API to get this
            })
        };

        #[cfg(not(target_os = "linux"))]
        let platform_attributes = PlatformAttributes::Linux(LinuxAttributes {
            mode: 0,
            uid: 0,
            gid: 0,
            dev: 0,
            ino: 0,
            nlink: 0,
            blksize: 0,
            blocks: 0,
            extended_attributes: Vec::new(),
            access_control_list: None,
            capabilities: None,
        });

        Ok(FileMetadata {
            size: metadata.len(),
            permissions: self.get_file_permissions(path)?,
            created: metadata.created().unwrap_or(SystemTime::UNIX_EPOCH),
            modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            accessed: metadata.accessed().unwrap_or(SystemTime::UNIX_EPOCH),
            file_type,
            platform_attributes,
        })
    }

    fn set_file_metadata(&self, path: &Path, metadata: &FileMetadata) -> Result<()> {
        // Set file permissions
        self.set_file_permissions(path, metadata.permissions)?;

        // Set file times
        #[cfg(target_os = "linux")]
        {
            use std::fs::OpenOptions;
            use std::os::unix::fs::MetadataExt;
            use std::os::unix::fs::OpenOptionsExt;

            let file = OpenOptions::new().write(true).open(path).map_err(|e| {
                AosError::Platform(format!("Failed to open file for metadata update: {}", e))
            })?;

            // Set file times using Linux system calls
            // This would require additional Linux API bindings
            debug!("Linux file metadata update not fully implemented");
        }

        #[cfg(not(target_os = "linux"))]
        {
            debug!("Linux file metadata update not available on this platform");
        }

        Ok(())
    }
}

impl Default for LinuxSettings {
    fn default() -> Self {
        Self {
            enable_features: true,
            use_file_locking: true,
            enable_security: true,
            enable_extended_attributes: true,
            enable_access_control_lists: true,
            enable_capabilities: true,
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
    fn test_linux_handler() -> Result<()> {
        let handler = LinuxHandler::new(None)?;

        assert_eq!(handler.platform_name(), "Linux");
        assert_eq!(handler.path_separator(), '/');
        assert!(handler.is_feature_supported("symlinks"));
        assert!(handler.is_feature_supported("hardlinks"));
        assert!(handler.is_feature_supported("posix_permissions"));
        assert!(handler.is_feature_supported("case_sensitive"));
        assert!(handler.is_feature_supported("extended_attributes"));
        assert!(handler.is_feature_supported("access_control_lists"));
        assert!(handler.is_feature_supported("capabilities"));

        Ok(())
    }

    #[test]
    fn test_linux_path_normalization() -> Result<()> {
        let handler = LinuxHandler::new(None)?;
        let temp_dir = TempDir::new()?;
        let test_path = temp_dir.path().join("test.txt");

        let normalized = handler.normalize_path(&test_path)?;
        assert_eq!(normalized, test_path);

        Ok(())
    }

    #[test]
    fn test_linux_file_metadata() -> Result<()> {
        let handler = LinuxHandler::new(None)?;
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello")?;

        let metadata = handler.get_file_metadata(&test_file)?;
        assert_eq!(metadata.size, 5);
        assert!(matches!(metadata.file_type, FileType::File));

        Ok(())
    }
}
