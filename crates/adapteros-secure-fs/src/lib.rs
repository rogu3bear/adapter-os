//! Secure filesystem operations with cap-std integration
//!
//! Provides secure filesystem operations with capability-based access control,
//! symlink protection, and path traversal prevention for AdapterOS.

pub mod caps;
pub mod content;
pub mod permissions;
pub mod symlink;
pub mod traversal;

use adapteros_core::{AosError, Result};
use cap_std::fs::{Dir, File};
use serde::{Deserialize, Serialize};
use std::path::Path as StdPath;
use std::path::PathBuf;
use tracing::info;

/// Secure filesystem configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecureFsConfig {
    /// Enable capability-based access control
    pub enable_caps: bool,
    /// Enable symlink protection
    pub enable_symlink_protection: bool,
    /// Enable path traversal protection
    pub enable_traversal_protection: bool,
    /// Maximum path depth
    pub max_path_depth: u32,
    /// Allowed file extensions
    pub allowed_extensions: Vec<String>,
    /// Blocked file extensions
    pub blocked_extensions: Vec<String>,
    /// Default file permissions (octal)
    pub default_file_permissions: u32,
    /// Default directory permissions (octal)
    pub default_dir_permissions: u32,
    /// Enable encryption by default
    pub enable_encryption: bool,
    /// Key provider configuration
    pub key_provider: adapteros_crypto::KeyProviderConfig,
}

/// Secure filesystem manager
pub struct SecureFsManager {
    config: SecureFsConfig,
    root_dir: Option<Dir>,
    key_provider: Option<Box<dyn adapteros_crypto::KeyProvider + Send + Sync>>,
}

impl SecureFsManager {
    /// Create a new secure filesystem manager
    pub fn new(config: SecureFsConfig) -> Result<Self> {
        Ok(Self {
            config,
            root_dir: None,
            key_provider: None,
        })
    }

    /// Initialize the key provider (async operation)
    pub async fn init_key_provider(&mut self) -> Result<()> {
        if self.config.enable_encryption {
            // Create the appropriate key provider based on config
            let provider: Box<dyn adapteros_crypto::KeyProvider + Send + Sync> =
                match self.config.key_provider.mode {
                    adapteros_crypto::KeyProviderMode::Keychain => Box::new(
                        adapteros_crypto::KeychainProvider::new(self.config.key_provider.clone())?,
                    ),
                    adapteros_crypto::KeyProviderMode::Kms => {
                        return Err(adapteros_core::AosError::Crypto(
                            "KMS provider not yet implemented".to_string(),
                        ));
                    }
                    adapteros_crypto::KeyProviderMode::File => {
                        return Err(adapteros_core::AosError::Crypto(
                            "File provider not allowed in production".to_string(),
                        ));
                    }
                };

            self.key_provider = Some(provider);
            info!("Initialized key provider for encrypted filesystem operations");
        }

        Ok(())
    }

    /// Set the root directory for capability-based access
    pub fn set_root_dir(&mut self, path: impl AsRef<StdPath>) -> Result<()> {
        if self.config.enable_caps {
            let root_dir = Dir::open_ambient_dir(path, cap_std::ambient_authority())
                .map_err(|e| AosError::Io(format!("Failed to open root directory: {}", e)))?;

            self.root_dir = Some(root_dir);
            info!("Set root directory for capability-based access");
        }
        Ok(())
    }

    /// Open a file securely
    pub fn open_file(&self, path: impl AsRef<StdPath>) -> Result<File> {
        let path = path.as_ref();

        // Validate path
        self.validate_path(path)?;

        // Check symlink protection
        if self.config.enable_symlink_protection {
            symlink::check_symlink_safety(path)?;
        }

        // Open file with capabilities if enabled
        if let Some(ref root_dir) = self.root_dir {
            let relative_path = self.make_relative_path(path)?;
            let file = root_dir.open(relative_path).map_err(|e| {
                AosError::Io(format!("Failed to open file with capabilities: {}", e))
            })?;
            Ok(file)
        } else {
            // Fallback to standard file operations
            let file = std::fs::File::open(path)
                .map_err(|e| AosError::Io(format!("Failed to open file: {}", e)))?;
            Ok(File::from_std(file))
        }
    }

    /// Create a file securely
    pub fn create_file(&self, path: impl AsRef<StdPath>) -> Result<File> {
        let path = path.as_ref();

        // Validate path
        self.validate_path(path)?;

        // Check symlink protection
        if self.config.enable_symlink_protection {
            symlink::check_symlink_safety(path)?;
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent)?;
        }

        // Create file with capabilities if enabled
        if let Some(ref root_dir) = self.root_dir {
            let relative_path = self.make_relative_path(path)?;
            let file = root_dir.create(relative_path).map_err(|e| {
                AosError::Io(format!("Failed to create file with capabilities: {}", e))
            })?;
            Ok(file)
        } else {
            // Fallback to standard file operations
            let file = std::fs::File::create(path)
                .map_err(|e| AosError::Io(format!("Failed to create file: {}", e)))?;
            Ok(File::from_std(file))
        }
    }

    /// Open a directory securely
    pub fn open_dir(&self, path: impl AsRef<StdPath>) -> Result<Dir> {
        let path = path.as_ref();

        // Validate path
        self.validate_path(path)?;

        // Check symlink protection
        if self.config.enable_symlink_protection {
            symlink::check_symlink_safety(path)?;
        }

        // Open directory with capabilities if enabled
        if let Some(ref root_dir) = self.root_dir {
            let relative_path = self.make_relative_path(path)?;
            let dir = root_dir.open_dir(relative_path).map_err(|e| {
                AosError::Io(format!("Failed to open directory with capabilities: {}", e))
            })?;
            Ok(dir)
        } else {
            // Fallback to standard directory operations
            let _dir = std::fs::read_dir(path)
                .map_err(|e| AosError::Io(format!("Failed to open directory: {}", e)))?;
            Ok(
                Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(|e| {
                    AosError::Io(format!("Failed to open directory with capabilities: {}", e))
                })?,
            )
        }
    }

    /// Create a directory securely
    pub fn create_dir(&self, path: impl AsRef<StdPath>) -> Result<()> {
        let path = path.as_ref();

        // Validate path
        self.validate_path(path)?;

        // Check symlink protection
        if self.config.enable_symlink_protection {
            symlink::check_symlink_safety(path)?;
        }

        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent)?;
        }

        // Create directory with capabilities if enabled
        if let Some(ref root_dir) = self.root_dir {
            let relative_path = self.make_relative_path(path)?;
            root_dir.create_dir(relative_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create directory with capabilities: {}",
                    e
                ))
            })?;
        } else {
            // Fallback to standard directory operations
            std::fs::create_dir(path)
                .map_err(|e| AosError::Io(format!("Failed to create directory: {}", e)))?;
        }

        // Set secure permissions
        self.set_secure_permissions(path, true)?;

        Ok(())
    }

    /// Create directory and all parents securely
    pub fn create_dir_all(&self, path: impl AsRef<StdPath>) -> Result<()> {
        let path = path.as_ref();

        // Validate path
        self.validate_path(path)?;

        // Check symlink protection
        if self.config.enable_symlink_protection {
            symlink::check_symlink_safety(path)?;
        }

        // Create directory with capabilities if enabled
        if let Some(ref root_dir) = self.root_dir {
            let relative_path = self.make_relative_path(path)?;
            root_dir.create_dir_all(relative_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to create directory with capabilities: {}",
                    e
                ))
            })?;
        } else {
            // Fallback to standard directory operations
            std::fs::create_dir_all(path)
                .map_err(|e| AosError::Io(format!("Failed to create directory: {}", e)))?;
        }

        // Set secure permissions
        self.set_secure_permissions(path, true)?;

        Ok(())
    }

    /// Remove a file securely
    pub fn remove_file(&self, path: impl AsRef<StdPath>) -> Result<()> {
        let path = path.as_ref();

        // Validate path
        self.validate_path(path)?;

        // Check symlink protection
        if self.config.enable_symlink_protection {
            symlink::check_symlink_safety(path)?;
        }

        // Remove file with capabilities if enabled
        if let Some(ref root_dir) = self.root_dir {
            let relative_path = self.make_relative_path(path)?;
            root_dir.remove_file(relative_path).map_err(|e| {
                AosError::Io(format!("Failed to remove file with capabilities: {}", e))
            })?;
        } else {
            // Fallback to standard file operations
            std::fs::remove_file(path)
                .map_err(|e| AosError::Io(format!("Failed to remove file: {}", e)))?;
        }

        Ok(())
    }

    /// Remove a directory securely
    pub fn remove_dir(&self, path: impl AsRef<StdPath>) -> Result<()> {
        let path = path.as_ref();

        // Validate path
        self.validate_path(path)?;

        // Check symlink protection
        if self.config.enable_symlink_protection {
            symlink::check_symlink_safety(path)?;
        }

        // Remove directory with capabilities if enabled
        if let Some(ref root_dir) = self.root_dir {
            let relative_path = self.make_relative_path(path)?;
            root_dir.remove_dir(relative_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to remove directory with capabilities: {}",
                    e
                ))
            })?;
        } else {
            // Fallback to standard directory operations
            std::fs::remove_dir(path)
                .map_err(|e| AosError::Io(format!("Failed to remove directory: {}", e)))?;
        }

        Ok(())
    }

    /// Remove directory and all contents securely
    pub fn remove_dir_all(&self, path: impl AsRef<StdPath>) -> Result<()> {
        let path = path.as_ref();

        // Validate path
        self.validate_path(path)?;

        // Check symlink protection
        if self.config.enable_symlink_protection {
            symlink::check_symlink_safety(path)?;
        }

        // Remove directory with capabilities if enabled
        if let Some(ref root_dir) = self.root_dir {
            let relative_path = self.make_relative_path(path)?;
            root_dir.remove_dir_all(relative_path).map_err(|e| {
                AosError::Io(format!(
                    "Failed to remove directory with capabilities: {}",
                    e
                ))
            })?;
        } else {
            // Fallback to standard directory operations
            std::fs::remove_dir_all(path)
                .map_err(|e| AosError::Io(format!("Failed to remove directory: {}", e)))?;
        }

        Ok(())
    }

    /// Validate a path for security
    fn validate_path(&self, path: impl AsRef<StdPath>) -> Result<()> {
        let path = path.as_ref();

        // Check path traversal protection
        if self.config.enable_traversal_protection {
            traversal::check_path_traversal(path)?;
        }

        // Check path depth
        let depth = path.components().count();
        if depth > self.config.max_path_depth as usize {
            return Err(AosError::Io(format!(
                "Path depth {} exceeds maximum {}",
                depth, self.config.max_path_depth
            )));
        }

        // Check file extension if it's a file
        if path.is_file() {
            if let Some(extension) = path.extension() {
                let ext_str = extension.to_string_lossy().to_string();

                // Check blocked extensions
                if self.config.blocked_extensions.contains(&ext_str) {
                    return Err(AosError::Io(format!(
                        "File extension {} is blocked by security policy",
                        ext_str
                    )));
                }

                // Check allowed extensions (if specified)
                if !self.config.allowed_extensions.is_empty()
                    && !self.config.allowed_extensions.contains(&ext_str)
                {
                    return Err(AosError::Io(format!(
                        "File extension {} is not allowed by security policy",
                        ext_str
                    )));
                }
            }
        }

        Ok(())
    }

    /// Make a path relative to the root directory
    fn make_relative_path(&self, path: impl AsRef<StdPath>) -> Result<PathBuf> {
        let path = path.as_ref();

        // In a real implementation, this would make the path relative to the root directory
        // For now, we'll just convert it to a PathBuf
        Ok(PathBuf::from(path))
    }

    /// Set secure permissions for a file or directory
    fn set_secure_permissions(&self, path: impl AsRef<StdPath>, is_dir: bool) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = if is_dir {
                self.config.default_dir_permissions
            } else {
                self.config.default_file_permissions
            };
            let perms = std::fs::Permissions::from_mode(permissions);
            std::fs::set_permissions(path, perms)
                .map_err(|e| AosError::Io(format!("Failed to set permissions: {}", e)))?;
        }

        #[cfg(windows)]
        {
            // Windows permission handling would go here
            debug!("Windows permissions not implemented yet");
        }

        Ok(())
    }
}

impl Default for SecureFsConfig {
    fn default() -> Self {
        Self {
            enable_caps: true,
            enable_symlink_protection: true,
            enable_traversal_protection: true,
            max_path_depth: 20,
            allowed_extensions: vec![
                "txt".to_string(),
                "json".to_string(),
                "jsonl".to_string(),
                "toml".to_string(),
                "yaml".to_string(),
                "aos".to_string(),
                "safetensors".to_string(),
            ],
            blocked_extensions: vec![
                "exe".to_string(),
                "bat".to_string(),
                "sh".to_string(),
                "ps1".to_string(),
                "cmd".to_string(),
            ],
            default_file_permissions: 0o600, // Owner read/write only
            default_dir_permissions: 0o700,  // Owner read/write/execute only
            enable_encryption: true,         // Encryption enabled by default
            key_provider: adapteros_crypto::KeyProviderConfig::default(),
        }
    }
}
