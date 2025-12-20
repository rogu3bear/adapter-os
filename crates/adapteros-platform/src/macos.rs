//! macOS-specific filesystem operations
//!
//! Implements macOS-specific filesystem operations and features.

use crate::{FileMetadata, FileType, PlatformAttributes, PlatformHandler};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::debug;

/// macOS-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacOSSettings {
    /// Enable macOS-specific features
    pub enable_features: bool,
    /// Use macOS file locking
    pub use_file_locking: bool,
    /// Enable macOS security features
    pub enable_security: bool,
    /// Enable extended attributes
    pub enable_extended_attributes: bool,
    /// Enable resource forks
    pub enable_resource_forks: bool,
    /// Default file mode
    pub default_file_mode: u32,
    /// Default directory mode
    pub default_dir_mode: u32,
}

/// macOS-specific attributes
#[derive(Debug, Clone)]
pub struct MacOSAttributes {
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
    /// Resource fork size
    pub resource_fork_size: u64,
}

/// macOS platform handler
pub struct MacOSHandler {
    settings: MacOSSettings,
}

impl MacOSHandler {
    /// Create a new macOS handler
    pub fn new(settings: Option<&MacOSSettings>) -> Result<Self> {
        let settings = settings.cloned().unwrap_or_else(MacOSSettings::default);
        Ok(Self { settings })
    }
}

impl PlatformHandler for MacOSHandler {
    fn platform_name(&self) -> &str {
        "macOS"
    }

    fn is_feature_supported(&self, feature: &str) -> bool {
        match feature {
            "symlinks" => true,
            "hardlinks" => true,
            "file_locking" => self.settings.use_file_locking,
            "posix_permissions" => true,
            "case_sensitive" => false, // macOS is case-insensitive by default
            "extended_attributes" => self.settings.enable_extended_attributes,
            "resource_forks" => self.settings.enable_resource_forks,
            "access_control_lists" => true,
            "quarantine" => true,
            "gatekeeper" => true,
            _ => false,
        }
    }

    fn path_separator(&self) -> char {
        '/'
    }

    fn normalize_path(&self, path: &Path) -> Result<PathBuf> {
        // macOS path normalization
        let normalized = path.to_path_buf();

        // Convert to canonical path if possible
        if normalized.exists() {
            normalized.canonicalize().map_err(|e| {
                AosError::Platform(format!("Failed to canonicalize macOS path: {}", e))
            })
        } else {
            Ok(normalized)
        }
    }

    fn set_file_permissions(&self, _path: &Path, _permissions: u32) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(_permissions);
            std::fs::set_permissions(_path, perms).map_err(|e| {
                AosError::Platform(format!("Failed to set macOS file permissions: {}", e))
            })?;
        }

        #[cfg(not(target_os = "macos"))]
        {
            return Err(AosError::Platform(
                "macOS permissions not available on this platform".to_string(),
            ));
        }

        debug!("Set macOS file permissions: {:o}", _permissions);
        Ok(())
    }

    fn get_file_permissions(&self, _path: &Path) -> Result<u32> {
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = std::fs::metadata(_path).map_err(|e| {
                AosError::Platform(format!("Failed to get macOS file metadata: {}", e))
            })?;
            Ok(metadata.permissions().mode())
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(AosError::Platform(
                "macOS permissions not available on this platform".to_string(),
            ))
        }
    }

    fn create_symlink(&self, _target: &Path, _link: &Path) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            std::os::unix::fs::symlink(_target, _link).map_err(|e| {
                AosError::Platform(format!("Failed to create macOS symlink: {}", e))
            })?;
        }

        #[cfg(not(target_os = "macos"))]
        {
            return Err(AosError::Platform(
                "macOS symlinks not available on this platform".to_string(),
            ));
        }

        debug!(
            "Created macOS symlink: {} -> {}",
            _link.display(),
            _target.display()
        );
        Ok(())
    }

    fn read_symlink(&self, _link: &Path) -> Result<PathBuf> {
        #[cfg(target_os = "macos")]
        {
            std::fs::read_link(_link)
                .map_err(|e| AosError::Platform(format!("Failed to read macOS symlink: {}", e)))
        }

        #[cfg(not(target_os = "macos"))]
        {
            Err(AosError::Platform(
                "macOS symlinks not available on this platform".to_string(),
            ))
        }
    }

    fn is_symlink(&self, _path: &Path) -> bool {
        #[cfg(target_os = "macos")]
        {
            _path.is_symlink()
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = std::fs::metadata(path)
            .map_err(|e| AosError::Platform(format!("Failed to get macOS file metadata: {}", e)))?;

        let file_type = if metadata.is_file() {
            FileType::File
        } else if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::Symlink
        } else {
            FileType::Unknown
        };

        #[cfg(target_os = "macos")]
        let platform_attributes = {
            use std::os::unix::fs::MetadataExt;
            PlatformAttributes::MacOS(MacOSAttributes {
                mode: metadata.mode(),
                uid: metadata.uid(),
                gid: metadata.gid(),
                dev: metadata.dev(),
                ino: metadata.ino(),
                nlink: metadata.nlink(),
                blksize: metadata.blksize(),
                blocks: metadata.blocks(),
                extended_attributes: Vec::new(), // Would need macOS API to get this
                resource_fork_size: 0,           // Would need macOS API to get this
            })
        };

        #[cfg(not(target_os = "macos"))]
        let platform_attributes = PlatformAttributes::MacOS(MacOSAttributes {
            mode: 0,
            uid: 0,
            gid: 0,
            dev: 0,
            ino: 0,
            nlink: 0,
            blksize: 0,
            blocks: 0,
            extended_attributes: Vec::new(),
            resource_fork_size: 0,
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
        #[cfg(target_os = "macos")]
        {
            use std::fs::OpenOptions;

            let _file = OpenOptions::new().write(true).open(path).map_err(|e| {
                AosError::Platform(format!("Failed to open file for metadata update: {}", e))
            })?;

            // Set file times using macOS system calls
            // This would require additional macOS API bindings
            debug!("macOS file metadata update not fully implemented");
        }

        #[cfg(not(target_os = "macos"))]
        {
            debug!("macOS file metadata update not available on this platform");
        }

        Ok(())
    }
}

impl Default for MacOSSettings {
    fn default() -> Self {
        Self {
            enable_features: true,
            use_file_locking: true,
            enable_security: true,
            enable_extended_attributes: true,
            enable_resource_forks: true,
            default_file_mode: 0o644,
            default_dir_mode: 0o755,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::PlatformUtils;
    use tempfile::TempDir;

    fn new_test_tempdir() -> Result<TempDir> {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root)?;
        Ok(TempDir::new_in(&root)?)
    }

    #[test]
    fn test_macos_handler() -> Result<()> {
        let handler = MacOSHandler::new(None)?;

        assert_eq!(handler.platform_name(), "macOS");
        assert_eq!(handler.path_separator(), '/');
        assert!(handler.is_feature_supported("symlinks"));
        assert!(handler.is_feature_supported("hardlinks"));
        assert!(handler.is_feature_supported("posix_permissions"));
        assert!(!handler.is_feature_supported("case_sensitive"));
        assert!(handler.is_feature_supported("extended_attributes"));
        assert!(handler.is_feature_supported("resource_forks"));

        Ok(())
    }

    #[test]
    fn test_macos_path_normalization() -> Result<()> {
        let handler = MacOSHandler::new(None)?;
        let temp_dir = new_test_tempdir()?;
        let test_path = temp_dir.path().join("test.txt");

        let normalized = handler.normalize_path(&test_path)?;
        assert_eq!(normalized, test_path);

        Ok(())
    }

    #[test]
    fn test_macos_file_metadata() -> Result<()> {
        let handler = MacOSHandler::new(None)?;
        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello")?;

        let metadata = handler.get_file_metadata(&test_file)?;
        assert_eq!(metadata.size, 5);
        assert!(matches!(metadata.file_type, FileType::File));

        Ok(())
    }
}
