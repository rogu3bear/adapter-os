//! Cross-platform filesystem operations
//!
//! Provides cross-platform filesystem operations with platform-specific
//! optimizations and features for AdapterOS.

pub mod common;
pub mod linux;
pub mod macos;
pub mod unix;
pub mod windows;

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Platform-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformConfig {
    /// Target platform
    pub target_platform: Platform,
    /// Enable platform-specific optimizations
    pub enable_optimizations: bool,
    /// Enable platform-specific features
    pub enable_features: bool,
    /// Platform-specific settings
    pub platform_settings: PlatformSettings,
}

/// Supported platforms
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Platform {
    /// Windows platform
    Windows,
    /// macOS platform
    MacOS,
    /// Linux platform
    Linux,
    /// Unix-like platform (generic)
    Unix,
    /// Unknown platform
    Unknown,
}

/// Platform-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformSettings {
    /// Windows-specific settings
    pub windows: Option<windows::WindowsSettings>,
    /// Unix-specific settings
    pub unix: Option<unix::UnixSettings>,
    /// macOS-specific settings
    pub macos: Option<macos::MacOSSettings>,
    /// Linux-specific settings
    pub linux: Option<linux::LinuxSettings>,
}

/// Cross-platform filesystem manager
pub struct PlatformFsManager {
    config: PlatformConfig,
    platform_handler: Box<dyn PlatformHandler>,
}

/// Platform handler trait
pub trait PlatformHandler: Send + Sync {
    /// Get platform name
    fn platform_name(&self) -> &str;

    /// Check if feature is supported
    fn is_feature_supported(&self, feature: &str) -> bool;

    /// Get platform-specific path separator
    fn path_separator(&self) -> char;

    /// Normalize path for platform
    fn normalize_path(&self, path: &Path) -> Result<PathBuf>;

    /// Set file permissions
    fn set_file_permissions(&self, path: &Path, permissions: u32) -> Result<()>;

    /// Get file permissions
    fn get_file_permissions(&self, path: &Path) -> Result<u32>;

    /// Create symbolic link
    fn create_symlink(&self, target: &Path, link: &Path) -> Result<()>;

    /// Read symbolic link
    fn read_symlink(&self, link: &Path) -> Result<PathBuf>;

    /// Check if path is symbolic link
    fn is_symlink(&self, path: &Path) -> bool;

    /// Get file metadata
    fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata>;

    /// Set file metadata
    fn set_file_metadata(&self, path: &Path, metadata: &FileMetadata) -> Result<()>;
}

/// File metadata structure
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: u64,
    /// File permissions
    pub permissions: u32,
    /// Creation time
    pub created: SystemTime,
    /// Modification time
    pub modified: SystemTime,
    /// Access time
    pub accessed: SystemTime,
    /// File type
    pub file_type: FileType,
    /// Platform-specific attributes
    pub platform_attributes: PlatformAttributes,
}

/// File type
#[derive(Debug, Clone)]
pub enum FileType {
    /// Regular file
    File,
    /// Directory
    Directory,
    /// Symbolic link
    Symlink,
    /// Character device
    CharDevice,
    /// Block device
    BlockDevice,
    /// Named pipe
    NamedPipe,
    /// Socket
    Socket,
    /// Unknown type
    Unknown,
}

/// Platform-specific attributes
#[derive(Debug, Clone)]
pub enum PlatformAttributes {
    /// Windows attributes
    Windows(windows::WindowsAttributes),
    /// Unix attributes
    Unix(unix::UnixAttributes),
    /// macOS attributes
    MacOS(macos::MacOSAttributes),
    /// Linux attributes
    Linux(linux::LinuxAttributes),
}

impl PlatformFsManager {
    /// Create a new platform filesystem manager
    pub fn new(config: PlatformConfig) -> Result<Self> {
        let platform_handler = Self::create_platform_handler(&config)?;

        Ok(Self {
            config,
            platform_handler,
        })
    }

    /// Create platform-specific handler
    fn create_platform_handler(config: &PlatformConfig) -> Result<Box<dyn PlatformHandler>> {
        match config.target_platform {
            Platform::Windows => Ok(Box::new(windows::WindowsHandler::new(
                config.platform_settings.windows.as_ref(),
            )?)),
            Platform::MacOS => Ok(Box::new(macos::MacOSHandler::new(
                config.platform_settings.macos.as_ref(),
            )?)),
            Platform::Linux => Ok(Box::new(linux::LinuxHandler::new(
                config.platform_settings.linux.as_ref(),
            )?)),
            Platform::Unix => Ok(Box::new(unix::UnixHandler::new(
                config.platform_settings.unix.as_ref(),
            )?)),
            Platform::Unknown => Err(AosError::Platform("Unknown platform".to_string())),
        }
    }

    /// Detect platform
    pub fn detect_platform() -> Result<Platform> {
        #[cfg(target_os = "windows")]
        return Ok(Platform::Windows);

        #[cfg(target_os = "macos")]
        return Ok(Platform::MacOS);

        #[cfg(target_os = "linux")]
        return Ok(Platform::Linux);

        #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
        return Ok(Platform::Unix);

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux", unix)))]
        return Ok(Platform::Unknown);
    }

    /// Detect platform automatically
    pub fn detect_platform_config() -> Result<PlatformConfig> {
        let platform = Self::detect_platform()?;

        let config = match platform {
            Platform::Windows => PlatformConfig {
                target_platform: platform,
                enable_optimizations: true,
                enable_features: true,
                platform_settings: PlatformSettings {
                    windows: Some(windows::WindowsSettings::default()),
                    unix: None,
                    macos: None,
                    linux: None,
                },
            },
            Platform::MacOS => PlatformConfig {
                target_platform: platform,
                enable_optimizations: true,
                enable_features: true,
                platform_settings: PlatformSettings {
                    windows: None,
                    unix: Some(unix::UnixSettings::default()),
                    macos: Some(macos::MacOSSettings::default()),
                    linux: None,
                },
            },
            Platform::Linux => PlatformConfig {
                target_platform: platform,
                enable_optimizations: true,
                enable_features: true,
                platform_settings: PlatformSettings {
                    windows: None,
                    unix: Some(unix::UnixSettings::default()),
                    macos: None,
                    linux: Some(linux::LinuxSettings::default()),
                },
            },
            Platform::Unix => PlatformConfig {
                target_platform: platform,
                enable_optimizations: true,
                enable_features: true,
                platform_settings: PlatformSettings {
                    windows: None,
                    unix: Some(unix::UnixSettings::default()),
                    macos: None,
                    linux: None,
                },
            },
            Platform::Unknown => {
                return Err(AosError::Platform("Unsupported platform".to_string()));
            }
        };

        Ok(config)
    }

    /// Get platform name
    pub fn platform_name(&self) -> &str {
        self.platform_handler.platform_name()
    }

    /// Check if feature is supported
    pub fn is_feature_supported(&self, feature: &str) -> bool {
        self.platform_handler.is_feature_supported(feature)
    }

    /// Get path separator
    pub fn path_separator(&self) -> char {
        self.platform_handler.path_separator()
    }

    /// Normalize path
    pub fn normalize_path(&self, path: &Path) -> Result<PathBuf> {
        self.platform_handler.normalize_path(path)
    }

    /// Set file permissions
    pub fn set_file_permissions(&self, path: &Path, permissions: u32) -> Result<()> {
        self.platform_handler
            .set_file_permissions(path, permissions)
    }

    /// Get file permissions
    pub fn get_file_permissions(&self, path: &Path) -> Result<u32> {
        self.platform_handler.get_file_permissions(path)
    }

    /// Create symbolic link
    pub fn create_symlink(&self, target: &Path, link: &Path) -> Result<()> {
        self.platform_handler.create_symlink(target, link)
    }

    /// Read symbolic link
    pub fn read_symlink(&self, link: &Path) -> Result<PathBuf> {
        self.platform_handler.read_symlink(link)
    }

    /// Check if path is symbolic link
    pub fn is_symlink(&self, path: &Path) -> bool {
        self.platform_handler.is_symlink(path)
    }

    /// Get file metadata
    pub fn get_file_metadata(&self, path: &Path) -> Result<FileMetadata> {
        self.platform_handler.get_file_metadata(path)
    }

    /// Set file metadata
    pub fn set_file_metadata(&self, path: &Path, metadata: &FileMetadata) -> Result<()> {
        self.platform_handler.set_file_metadata(path, metadata)
    }

    /// Get configuration
    pub fn config(&self) -> &PlatformConfig {
        &self.config
    }
}

impl Default for PlatformConfig {
    fn default() -> Self {
        PlatformFsManager::detect_platform_config().unwrap_or(PlatformConfig {
            target_platform: Platform::Unknown,
            enable_optimizations: false,
            enable_features: false,
            platform_settings: PlatformSettings {
                windows: None,
                unix: None,
                macos: None,
                linux: None,
            },
        })
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
    fn test_platform_detection() {
        let platform = PlatformFsManager::detect_platform().unwrap();
        assert_ne!(platform, Platform::Unknown);
    }

    #[test]
    fn test_platform_config() -> Result<()> {
        let config = PlatformFsManager::detect_platform_config()?;
        assert!(config.enable_optimizations);
        assert!(config.enable_features);
        Ok(())
    }

    #[test]
    fn test_platform_manager() -> Result<()> {
        let config = PlatformFsManager::detect_platform_config()?;
        let manager = PlatformFsManager::new(config)?;

        assert!(!manager.platform_name().is_empty());
        assert_eq!(manager.path_separator(), std::path::MAIN_SEPARATOR);

        Ok(())
    }

    #[test]
    fn test_path_normalization() -> Result<()> {
        let config = PlatformFsManager::detect_platform_config()?;
        let manager = PlatformFsManager::new(config)?;

        let temp_dir = new_test_tempdir()?;
        let test_path = temp_dir.path().join("test.txt");

        let normalized = manager.normalize_path(&test_path)?;
        assert_eq!(normalized, test_path);

        Ok(())
    }
}
