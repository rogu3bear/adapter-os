//! Secure file permissions
//!
//! Implements secure file and directory permissions for AdapterOS.

use adapteros_core::{AosError, Result};
use std::path::Path;
use tracing::debug;

/// Permission configuration
#[derive(Debug, Clone)]
pub struct PermissionConfig {
    /// Default file permissions (octal)
    pub default_file_permissions: u32,
    /// Default directory permissions (octal)
    pub default_dir_permissions: u32,
    /// Enable strict permissions
    pub strict_permissions: bool,
    /// Enable permission inheritance
    pub inherit_permissions: bool,
}

impl Default for PermissionConfig {
    fn default() -> Self {
        Self {
            default_file_permissions: 0o600, // Owner read/write only
            default_dir_permissions: 0o700,  // Owner read/write/execute only
            strict_permissions: true,
            inherit_permissions: true,
        }
    }
}

/// Set secure permissions for a file
pub fn set_secure_file_permissions(
    path: impl AsRef<Path>,
    config: &PermissionConfig,
) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(config.default_file_permissions);
        std::fs::set_permissions(path, perms)
            .map_err(|e| AosError::Security(format!("Failed to set file permissions: {}", e)))?;

        debug!(
            "Set secure file permissions: {:o}",
            config.default_file_permissions
        );
    }

    #[cfg(windows)]
    {
        // Windows permission handling would go here
        debug!("Windows file permissions not implemented yet");
    }

    Ok(())
}

/// Set secure permissions for a directory
pub fn set_secure_dir_permissions(path: impl AsRef<Path>, config: &PermissionConfig) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(config.default_dir_permissions);
        std::fs::set_permissions(path, perms).map_err(|e| {
            AosError::Security(format!("Failed to set directory permissions: {}", e))
        })?;

        debug!(
            "Set secure directory permissions: {:o}",
            config.default_dir_permissions
        );
    }

    #[cfg(windows)]
    {
        // Windows permission handling would go here
        debug!("Windows directory permissions not implemented yet");
    }

    Ok(())
}

/// Check if permissions are secure
pub fn check_secure_permissions(
    path: impl AsRef<Path>,
    _config: &PermissionConfig,
) -> Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&path)
            .map_err(|e| AosError::Security(format!("Failed to get file metadata: {}", e)))?;

        let permissions = metadata.permissions();
        let mode = permissions.mode();

        if path.as_ref().is_file() {
            // Check if file permissions are too permissive
            if mode & 0o077 != 0 {
                return Ok(false);
            }
        } else if path.as_ref().is_dir() {
            // Check if directory permissions are too permissive
            if mode & 0o077 != 0 {
                return Ok(false);
            }
        }

        Ok(true)
    }

    #[cfg(windows)]
    {
        // Windows permission checking would go here
        debug!("Windows permission checking not implemented yet");
        Ok(true)
    }
}

/// Fix insecure permissions
pub fn fix_insecure_permissions(path: impl AsRef<Path>, config: &PermissionConfig) -> Result<()> {
    if !check_secure_permissions(&path, config)? {
        if path.as_ref().is_file() {
            set_secure_file_permissions(&path, config)?;
        } else if path.as_ref().is_dir() {
            set_secure_dir_permissions(&path, config)?;
        }

        debug!(
            "Fixed insecure permissions for: {}",
            path.as_ref().display()
        );
    }

    Ok(())
}

/// Recursively set secure permissions
pub fn set_secure_permissions_recursive(
    path: impl AsRef<Path>,
    config: &PermissionConfig,
) -> Result<()> {
    let path = path.as_ref();

    if path.is_file() {
        set_secure_file_permissions(path, config)?;
    } else if path.is_dir() {
        set_secure_dir_permissions(path, config)?;

        // Recursively set permissions for all entries
        let entries = std::fs::read_dir(path)
            .map_err(|e| AosError::Security(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                AosError::Security(format!("Failed to read directory entry: {}", e))
            })?;
            let entry_path = entry.path();

            set_secure_permissions_recursive(&entry_path, config)?;
        }
    }

    Ok(())
}

/// Check and fix permissions recursively
pub fn check_and_fix_permissions_recursive(
    path: impl AsRef<Path>,
    config: &PermissionConfig,
) -> Result<()> {
    let path = path.as_ref();

    if path.is_file() {
        fix_insecure_permissions(path, config)?;
    } else if path.is_dir() {
        fix_insecure_permissions(path, config)?;

        // Recursively check and fix permissions for all entries
        let entries = std::fs::read_dir(path)
            .map_err(|e| AosError::Security(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                AosError::Security(format!("Failed to read directory entry: {}", e))
            })?;
            let entry_path = entry.path();

            check_and_fix_permissions_recursive(&entry_path, config)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_secure_permissions() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "hello")?;

        let config = PermissionConfig::default();

        // Set secure permissions
        set_secure_file_permissions(&test_file, &config)?;

        // Check if permissions are secure
        assert!(check_secure_permissions(&test_file, &config)?);

        Ok(())
    }

    #[test]
    fn test_recursive_permissions() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_dir");
        std::fs::create_dir_all(&test_dir)?;

        let test_file = test_dir.join("test.txt");
        std::fs::write(&test_file, "hello")?;

        let config = PermissionConfig::default();

        // Set secure permissions recursively
        set_secure_permissions_recursive(&test_dir, &config)?;

        // Check if permissions are secure
        assert!(check_secure_permissions(&test_file, &config)?);
        assert!(check_secure_permissions(&test_dir, &config)?);

        Ok(())
    }
}
