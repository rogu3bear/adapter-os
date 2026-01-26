//! Secure file permissions
//!
//! Implements secure file and directory permissions for adapterOS.
//!
//! ## Permission Recovery
//!
//! This module provides functions that automatically attempt to fix permissions
//! before failing. When a permission denied error occurs, the functions will:
//!
//! 1. Attempt to chmod the file/directory to the configured permissions
//! 2. Retry the original operation once
//! 3. Only fail if the retry also fails
//!
//! This is useful for recovering from permission issues caused by:
//! - Files created by other processes with restrictive permissions
//! - Permission drift over time
//! - Manual permission changes by administrators

use adapteros_core::{AosError, Result};
use std::fs::File;
use std::path::Path;
use tracing::{debug, error, info, warn};

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
            .map_err(|e| AosError::Io(format!("Failed to set file permissions: {}", e)))?;

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
        std::fs::set_permissions(path, perms)
            .map_err(|e| AosError::Io(format!("Failed to set directory permissions: {}", e)))?;

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
            .map_err(|e| AosError::Io(format!("Failed to get file metadata: {}", e)))?;

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
            .map_err(|e| AosError::Io(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?;
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
            .map_err(|e| AosError::Io(format!("Failed to read directory: {}", e)))?;

        for entry in entries {
            let entry = entry
                .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();

            check_and_fix_permissions_recursive(&entry_path, config)?;
        }
    }

    Ok(())
}

/// Attempt to open a file, auto-fixing permissions on EACCES
///
/// On permission denied:
/// 1. Try to chmod the file to 0o600 (owner read/write)
/// 2. Retry the open operation once
/// 3. Only fail if retry also fails
///
/// # Example
///
/// ```rust
/// use crate::secure_fs::permissions::{try_open_with_permission_fix, PermissionConfig};
/// use std::path::Path;
///
/// let config = PermissionConfig::default();
/// let file = try_open_with_permission_fix(Path::new("/path/to/file"), &config)?;
/// ```
pub fn try_open_with_permission_fix(
    path: impl AsRef<Path>,
    config: &PermissionConfig,
) -> Result<File> {
    let path = path.as_ref();

    match File::open(path) {
        Ok(f) => {
            debug!(path = %path.display(), "File opened successfully");
            Ok(f)
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            info!(
                path = %path.display(),
                "Permission denied, attempting chmod fix"
            );

            // Attempt to fix permissions (chmod to configured default)
            if let Err(chmod_err) = set_secure_file_permissions(path, config) {
                warn!(
                    path = %path.display(),
                    error = %chmod_err,
                    "Failed to fix file permissions, will try open anyway"
                );
            } else {
                debug!(
                    path = %path.display(),
                    mode = format!("{:o}", config.default_file_permissions),
                    "Fixed file permissions"
                );
            }

            // Retry open after chmod attempt
            File::open(path).map_err(|retry_err| {
                error!(
                    path = %path.display(),
                    original_error = %e,
                    retry_error = %retry_err,
                    "Permission denied after chmod retry"
                );
                AosError::PermissionDenied {
                    path: path.display().to_string(),
                    operation: "open".to_string(),
                    reason: format!("Permission denied (chmod attempted): {}", retry_err),
                }
            })
        }
        Err(e) => Err(AosError::Io(format!(
            "Failed to open file {}: {}",
            path.display(),
            e
        ))),
    }
}

/// Attempt to create a file, auto-fixing parent directory permissions on EACCES
///
/// On permission denied:
/// 1. Try to chmod the parent directory to 0o700 (owner read/write/execute)
/// 2. Retry the create operation once
/// 3. Only fail if retry also fails
///
/// # Example
///
/// ```rust
/// use crate::secure_fs::permissions::{try_create_with_permission_fix, PermissionConfig};
/// use std::path::Path;
///
/// let config = PermissionConfig::default();
/// let file = try_create_with_permission_fix(Path::new("/path/to/file"), &config)?;
/// ```
pub fn try_create_with_permission_fix(
    path: impl AsRef<Path>,
    config: &PermissionConfig,
) -> Result<File> {
    let path = path.as_ref();

    match File::create(path) {
        Ok(f) => {
            debug!(path = %path.display(), "File created successfully");
            // Set secure permissions on the newly created file
            if let Err(e) = set_secure_file_permissions(path, config) {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to set permissions on new file"
                );
            }
            Ok(f)
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            info!(
                path = %path.display(),
                "Permission denied for file creation, attempting to fix parent directory"
            );

            // Try to fix parent directory permissions
            if let Some(parent) = path.parent() {
                if let Err(chmod_err) = set_secure_dir_permissions(parent, config) {
                    warn!(
                        path = %parent.display(),
                        error = %chmod_err,
                        "Failed to fix parent directory permissions"
                    );
                } else {
                    debug!(
                        path = %parent.display(),
                        mode = format!("{:o}", config.default_dir_permissions),
                        "Fixed parent directory permissions"
                    );
                }
            }

            // Retry create after chmod attempt
            match File::create(path) {
                Ok(f) => {
                    // Set secure permissions on the newly created file
                    if let Err(perm_err) = set_secure_file_permissions(path, config) {
                        warn!(
                            path = %path.display(),
                            error = %perm_err,
                            "Failed to set permissions on new file after retry"
                        );
                    }
                    Ok(f)
                }
                Err(retry_err) => {
                    error!(
                        path = %path.display(),
                        original_error = %e,
                        retry_error = %retry_err,
                        "Permission denied after chmod retry"
                    );
                    Err(AosError::PermissionDenied {
                        path: path.display().to_string(),
                        operation: "create".to_string(),
                        reason: format!("Permission denied (chmod attempted): {}", retry_err),
                    })
                }
            }
        }
        Err(e) => Err(AosError::Io(format!(
            "Failed to create file {}: {}",
            path.display(),
            e
        ))),
    }
}

/// Attempt to read a directory, auto-fixing permissions on EACCES
///
/// On permission denied:
/// 1. Try to chmod the directory to 0o700 (owner read/write/execute)
/// 2. Retry the read_dir operation once
/// 3. Only fail if retry also fails
pub fn try_read_dir_with_permission_fix(
    path: impl AsRef<Path>,
    config: &PermissionConfig,
) -> Result<std::fs::ReadDir> {
    let path = path.as_ref();

    match std::fs::read_dir(path) {
        Ok(entries) => {
            debug!(path = %path.display(), "Directory read successfully");
            Ok(entries)
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            info!(
                path = %path.display(),
                "Permission denied, attempting chmod fix for directory"
            );

            // Attempt to fix directory permissions
            if let Err(chmod_err) = set_secure_dir_permissions(path, config) {
                warn!(
                    path = %path.display(),
                    error = %chmod_err,
                    "Failed to fix directory permissions"
                );
            } else {
                debug!(
                    path = %path.display(),
                    mode = format!("{:o}", config.default_dir_permissions),
                    "Fixed directory permissions"
                );
            }

            // Retry read_dir after chmod attempt
            std::fs::read_dir(path).map_err(|retry_err| {
                error!(
                    path = %path.display(),
                    original_error = %e,
                    retry_error = %retry_err,
                    "Permission denied after chmod retry"
                );
                AosError::PermissionDenied {
                    path: path.display().to_string(),
                    operation: "read_dir".to_string(),
                    reason: format!("Permission denied (chmod attempted): {}", retry_err),
                }
            })
        }
        Err(e) => Err(AosError::Io(format!(
            "Failed to read directory {}: {}",
            path.display(),
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> Result<TempDir> {
        let root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root)?;
        Ok(TempDir::new_in(&root)?)
    }

    #[test]
    fn test_secure_permissions() -> Result<()> {
        let temp_dir = new_test_tempdir()?;
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
        let temp_dir = new_test_tempdir()?;
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
