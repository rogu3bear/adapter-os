//! Windows-specific filesystem operations
//!
//! Implements Windows-specific filesystem operations and features.

use super::{FileMetadata, FileType, PlatformAttributes, PlatformHandler};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
#[cfg(target_os = "windows")]
use tracing::debug;

/// Windows-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsSettings {
    /// Enable Windows-specific features
    pub enable_features: bool,
    /// Use Windows file locking
    pub use_file_locking: bool,
    /// Enable Windows security features
    pub enable_security: bool,
    /// Default file attributes
    pub default_attributes: u32,
}

/// Windows-specific attributes
#[derive(Debug, Clone)]
pub struct WindowsAttributes {
    /// File attributes (FILE_ATTRIBUTE_*)
    pub attributes: u32,
    /// Security descriptor
    pub security_descriptor: Option<Vec<u8>>,
    /// Alternate data streams
    pub alternate_streams: Vec<String>,
}

/// Windows platform handler
pub struct WindowsHandler {
    settings: WindowsSettings,
}

impl WindowsHandler {
    /// Create a new Windows handler
    pub fn new(settings: Option<&WindowsSettings>) -> Result<Self> {
        let settings = settings.cloned().unwrap_or_else(WindowsSettings::default);
        Ok(Self { settings })
    }
}

impl PlatformHandler for WindowsHandler {
    fn platform_name(&self) -> &str {
        "Windows"
    }

    fn is_feature_supported(&self, feature: &str) -> bool {
        match feature {
            "symlinks" => true,
            "hardlinks" => true,
            "file_locking" => self.settings.use_file_locking,
            "security_descriptors" => self.settings.enable_security,
            "alternate_streams" => true,
            "case_sensitive" => false,
            "posix_permissions" => false,
            _ => false,
        }
    }

    fn path_separator(&self) -> char {
        '\\'
    }

    fn normalize_path(&self, path: &Path) -> Result<PathBuf> {
        // Windows path normalization
        let normalized = path.to_path_buf();

        // Convert to canonical path if possible
        if normalized.exists() {
            normalized.canonicalize().map_err(|e| {
                AosError::Platform(format!("Failed to canonicalize Windows path: {}", e))
            })
        } else {
            Ok(normalized)
        }
    }

    fn set_file_permissions(&self, _path: &Path, _permissions: u32) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(_permissions);
            std::fs::set_permissions(_path, perms).map_err(|e| {
                AosError::Platform(format!("Failed to set Windows file permissions: {}", e))
            })?;
            debug!("Set Windows file permissions: {:o}", _permissions);
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err(AosError::Platform(
                "Windows permissions not available on this platform".to_string(),
            ))
        }
    }

    fn get_file_permissions(&self, _path: &Path) -> Result<u32> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::PermissionsExt;
            let metadata = std::fs::metadata(_path).map_err(|e| {
                AosError::Platform(format!("Failed to get Windows file metadata: {}", e))
            })?;
            Ok(metadata.permissions().mode())
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err(AosError::Platform(
                "Windows permissions not available on this platform".to_string(),
            ))
        }
    }

    fn create_symlink(&self, _target: &Path, _link: &Path) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::fs::symlink_dir;
            use std::os::windows::fs::symlink_file;

            if _target.is_file() {
                symlink_file(_target, _link).map_err(|e| {
                    AosError::Platform(format!("Failed to create Windows file symlink: {}", e))
                })?;
            } else if _target.is_dir() {
                symlink_dir(_target, _link).map_err(|e| {
                    AosError::Platform(format!("Failed to create Windows directory symlink: {}", e))
                })?;
            } else {
                return Err(AosError::Platform(
                    "Target must be a file or directory".to_string(),
                ));
            }

            debug!(
                "Created Windows symlink: {} -> {}",
                _link.display(),
                _target.display()
            );
            Ok(())
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err(AosError::Platform(
                "Windows symlinks not available on this platform".to_string(),
            ))
        }
    }

    fn read_symlink(&self, _link: &Path) -> Result<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            std::fs::read_link(_link)
                .map_err(|e| AosError::Platform(format!("Failed to read Windows symlink: {}", e)))
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err(AosError::Platform(
                "Windows symlinks not available on this platform".to_string(),
            ))
        }
    }

    fn is_symlink(&self, _path: &Path) -> bool {
        #[cfg(target_os = "windows")]
        {
            _path.is_symlink()
        }

        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        let metadata = std::fs::metadata(path).map_err(|e| {
            AosError::Platform(format!("Failed to get Windows file metadata: {}", e))
        })?;

        let file_type = if metadata.is_file() {
            FileType::File
        } else if metadata.is_dir() {
            FileType::Directory
        } else if metadata.is_symlink() {
            FileType::Symlink
        } else {
            FileType::Unknown
        };

        #[cfg(target_os = "windows")]
        let platform_attributes = {
            use std::os::windows::fs::MetadataExt;
            PlatformAttributes::Windows(WindowsAttributes {
                attributes: metadata.file_attributes(),
                security_descriptor: None, // Would need Windows API to get this
                alternate_streams: Vec::new(), // Would need Windows API to get this
            })
        };

        #[cfg(not(target_os = "windows"))]
        let platform_attributes = PlatformAttributes::Windows(WindowsAttributes {
            attributes: 0,
            security_descriptor: None,
            alternate_streams: Vec::new(),
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

    fn set_file_metadata(&self, _path: &Path, _metadata: &FileMetadata) -> Result<()> {
        // Fail fast - full metadata update not implemented
        #[cfg(target_os = "windows")]
        {
            return Err(AosError::Platform(
                "Windows file metadata update not implemented (file times require Windows API bindings)".to_string(),
            ));
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err(AosError::Platform(
                "Windows file metadata not available on this platform".to_string(),
            ))
        }
    }
}

impl Default for WindowsSettings {
    fn default() -> Self {
        Self {
            enable_features: true,
            use_file_locking: true,
            enable_security: true,
            default_attributes: 0x20, // FILE_ATTRIBUTE_ARCHIVE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::common::PlatformUtils;
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> Result<TempDir> {
        let root = PlatformUtils::temp_dir();
        std::fs::create_dir_all(&root)?;
        Ok(TempDir::new_in(&root)?)
    }

    #[test]
    fn test_windows_handler() -> Result<()> {
        let handler = WindowsHandler::new(None)?;

        assert_eq!(handler.platform_name(), "Windows");
        assert_eq!(handler.path_separator(), '\\');
        assert!(handler.is_feature_supported("symlinks"));
        assert!(handler.is_feature_supported("hardlinks"));
        assert!(!handler.is_feature_supported("case_sensitive"));

        Ok(())
    }

    #[test]
    fn test_windows_path_normalization() -> Result<()> {
        let handler = WindowsHandler::new(None)?;
        let temp_dir = new_test_tempdir()?;
        let test_path = temp_dir.path().join("test.txt");

        let normalized = handler.normalize_path(&test_path)?;
        assert_eq!(normalized, test_path);

        Ok(())
    }

    #[test]
    fn test_windows_file_metadata() -> Result<()> {
        let handler = WindowsHandler::new(None)?;
        let temp_dir = new_test_tempdir()?;
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello")?;

        let metadata = handler.get_file_metadata(&test_file)?;
        assert_eq!(metadata.size, 5);
        assert!(matches!(metadata.file_type, FileType::File));

        Ok(())
    }
}
