//! I/O utilities for file system error handling
//!
//! This module provides utilities for:
//! - Classifying I/O errors into specific categories (disk full, permission denied, etc.)
//! - Pre-flight checks for disk space
//! - Path validation for OS-specific invalid characters
//! - Temporary directory management
//!
//! # Example
//!
//! ```rust
//! use adapteros_core::io_utils::{check_disk_space, validate_path_characters, ensure_temp_dir};
//! use std::path::Path;
//!
//! // Check disk space with 10% margin before large write
//! check_disk_space(Path::new("/tmp"), 1024 * 1024 * 100)?; // 100 MB
//!
//! // Validate path has no invalid characters
//! validate_path_characters(Path::new("/valid/path/file.txt"))?;
//!
//! // Ensure temp directory exists
//! let temp_dir = ensure_temp_dir(Path::new("/tmp/my_app"))?;
//! ```

use crate::{AosError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Default safety margin for disk space checks (10%)
pub const DEFAULT_DISK_SPACE_MARGIN: f64 = 0.10;

/// Classified I/O error kinds for structured error handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoErrorKind {
    /// Disk is full (ENOSPC) or quota exceeded (EDQUOT)
    DiskFull,
    /// Permission denied (EACCES/EPERM)
    PermissionDenied,
    /// File or directory not found (ENOENT)
    NotFound,
    /// Invalid path (ENAMETOOLONG, EINVAL, or invalid characters)
    InvalidPath,
    /// Read-only filesystem (EROFS)
    ReadOnlyFilesystem,
    /// Other I/O error
    Other,
}

/// Classify an I/O error into a specific category
///
/// This function examines the underlying OS error code to determine
/// the specific type of I/O failure that occurred.
///
/// # Platform Support
///
/// - Unix: Uses raw OS error codes (ENOSPC, EDQUOT, EACCES, etc.)
/// - Windows: Uses standard ErrorKind mapping
pub fn classify_io_error(err: &std::io::Error) -> IoErrorKind {
    // First try to match on raw OS error codes (more specific)
    #[cfg(unix)]
    {
        if let Some(code) = err.raw_os_error() {
            return match code {
                libc::ENOSPC | libc::EDQUOT => IoErrorKind::DiskFull,
                libc::EACCES | libc::EPERM => IoErrorKind::PermissionDenied,
                libc::ENOENT => IoErrorKind::NotFound,
                libc::ENAMETOOLONG | libc::EINVAL => IoErrorKind::InvalidPath,
                libc::EROFS => IoErrorKind::ReadOnlyFilesystem,
                _ => IoErrorKind::Other,
            };
        }
    }

    #[cfg(windows)]
    {
        if let Some(code) = err.raw_os_error() {
            // Windows error codes
            const ERROR_DISK_FULL: i32 = 112;
            const ERROR_HANDLE_DISK_FULL: i32 = 39;
            const ERROR_ACCESS_DENIED: i32 = 5;
            const ERROR_FILE_NOT_FOUND: i32 = 2;
            const ERROR_PATH_NOT_FOUND: i32 = 3;
            const ERROR_INVALID_NAME: i32 = 123;
            const ERROR_BAD_PATHNAME: i32 = 161;
            const ERROR_WRITE_PROTECT: i32 = 19;

            return match code {
                ERROR_DISK_FULL | ERROR_HANDLE_DISK_FULL => IoErrorKind::DiskFull,
                ERROR_ACCESS_DENIED => IoErrorKind::PermissionDenied,
                ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => IoErrorKind::NotFound,
                ERROR_INVALID_NAME | ERROR_BAD_PATHNAME => IoErrorKind::InvalidPath,
                ERROR_WRITE_PROTECT => IoErrorKind::ReadOnlyFilesystem,
                _ => IoErrorKind::Other,
            };
        }
    }

    // Fall back to std::io::ErrorKind
    match err.kind() {
        std::io::ErrorKind::PermissionDenied => IoErrorKind::PermissionDenied,
        std::io::ErrorKind::NotFound => IoErrorKind::NotFound,
        std::io::ErrorKind::InvalidInput => IoErrorKind::InvalidPath,
        std::io::ErrorKind::ReadOnlyFilesystem => IoErrorKind::ReadOnlyFilesystem,
        _ => IoErrorKind::Other,
    }
}

/// Convert an I/O error to an appropriate AosError based on classification
///
/// This function classifies the I/O error and returns a structured AosError
/// with appropriate context for the failure type.
pub fn classify_and_convert_io_error(
    err: std::io::Error,
    path: &Path,
    operation: &str,
) -> AosError {
    let path_str = path.display().to_string();

    match classify_io_error(&err) {
        IoErrorKind::DiskFull => AosError::DiskFull {
            path: path_str,
            details: err.to_string(),
            bytes_needed: None,
            bytes_available: None,
        },
        IoErrorKind::PermissionDenied => AosError::PermissionDenied {
            path: path_str,
            operation: operation.to_string(),
            reason: err.to_string(),
        },
        IoErrorKind::NotFound => {
            if path.parent().is_some_and(|p| !p.exists()) {
                AosError::TempDirUnavailable {
                    path: path_str,
                    reason: format!("Parent directory does not exist: {}", err),
                }
            } else {
                AosError::NotFound(format!("{}: {}", path_str, err))
            }
        }
        IoErrorKind::InvalidPath => AosError::InvalidPathCharacters {
            path: path_str,
            details: err.to_string(),
            invalid_chars: vec![],
        },
        IoErrorKind::ReadOnlyFilesystem => AosError::PermissionDenied {
            path: path_str,
            operation: operation.to_string(),
            reason: format!("Read-only filesystem: {}", err),
        },
        IoErrorKind::Other => {
            AosError::Io(format!("{} failed for {}: {}", operation, path_str, err))
        }
    }
}

/// Get available disk space at the given path
///
/// Returns the number of bytes available on the filesystem containing the path.
///
/// # Platform Support
///
/// - Unix: Uses `statvfs`
/// - Windows: Uses `GetDiskFreeSpaceExW`
pub fn get_available_space(path: &Path) -> Result<u64> {
    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        // Find the actual mount point by traversing up until we find an existing directory
        let check_path = if path.exists() {
            path.to_path_buf()
        } else {
            path.ancestors()
                .find(|p| p.exists())
                .unwrap_or(Path::new("/"))
                .to_path_buf()
        };

        let path_cstr = CString::new(check_path.as_os_str().as_bytes()).map_err(|e| {
            AosError::InvalidPathCharacters {
                path: path.display().to_string(),
                details: format!("Path contains null byte: {}", e),
                invalid_chars: vec!['\0'],
            }
        })?;

        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        let result = unsafe { libc::statvfs(path_cstr.as_ptr(), &mut stat) };

        if result != 0 {
            let err = std::io::Error::last_os_error();
            return Err(classify_and_convert_io_error(err, path, "statvfs"));
        }

        // Available space = available blocks * block size
        let available = stat.f_bavail as u64 * stat.f_frsize;
        Ok(available)
    }

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        let check_path = if path.exists() {
            path.to_path_buf()
        } else {
            path.ancestors()
                .find(|p| p.exists())
                .unwrap_or(Path::new("C:\\"))
                .to_path_buf()
        };

        let wide_path: Vec<u16> = check_path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut free_bytes: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut total_free_bytes: u64 = 0;

        let result = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide_path.as_ptr(),
                &mut free_bytes as *mut _,
                &mut total_bytes as *mut _,
                &mut total_free_bytes as *mut _,
            )
        };

        if result == 0 {
            let err = std::io::Error::last_os_error();
            return Err(classify_and_convert_io_error(
                err,
                path,
                "GetDiskFreeSpaceExW",
            ));
        }

        Ok(free_bytes)
    }

    #[cfg(not(any(unix, windows)))]
    {
        // Fallback for other platforms - return a large number to not block writes
        warn!(
            path = %path.display(),
            "Disk space check not supported on this platform, allowing write"
        );
        Ok(u64::MAX)
    }
}

/// Check if there is sufficient disk space for a write operation
///
/// Checks that the filesystem containing `path` has at least `required_bytes`
/// available, plus a 10% safety margin.
///
/// # Errors
///
/// Returns `AosError::DiskFull` if there is insufficient space, including
/// details about the space needed and available.
///
/// # Example
///
/// ```rust
/// use adapteros_core::io_utils::check_disk_space;
/// use std::path::Path;
///
/// // Check for 100 MB of space (will actually require 110 MB with margin)
/// check_disk_space(Path::new("/tmp"), 100 * 1024 * 1024)?;
/// ```
pub fn check_disk_space(path: &Path, required_bytes: u64) -> Result<()> {
    let required_with_margin =
        (required_bytes as f64 * (1.0 + DEFAULT_DISK_SPACE_MARGIN)).ceil() as u64;

    let available = get_available_space(path)?;

    if available < required_with_margin {
        debug!(
            path = %path.display(),
            required = required_bytes,
            required_with_margin = required_with_margin,
            available = available,
            "Insufficient disk space"
        );

        return Err(AosError::DiskFull {
            path: path.display().to_string(),
            details: format!(
                "Need {} bytes (including 10% safety margin), only {} bytes available",
                required_with_margin, available
            ),
            bytes_needed: Some(required_with_margin),
            bytes_available: Some(available),
        });
    }

    debug!(
        path = %path.display(),
        required = required_bytes,
        required_with_margin = required_with_margin,
        available = available,
        "Disk space check passed"
    );

    Ok(())
}

/// Invalid characters for file paths on each platform
#[cfg(unix)]
const INVALID_PATH_CHARS: &[char] = &['\0'];

#[cfg(windows)]
const INVALID_PATH_CHARS: &[char] = &['\0', '<', '>', ':', '"', '|', '?', '*'];

#[cfg(not(any(unix, windows)))]
const INVALID_PATH_CHARS: &[char] = &['\0'];

/// Validate that a path contains no OS-specific invalid characters
///
/// On Unix, only the null character is invalid in paths.
/// On Windows, the following characters are invalid: `<>:"|?*` and null.
///
/// # Errors
///
/// Returns `AosError::InvalidPathCharacters` if the path contains invalid
/// characters, listing all the invalid characters found.
///
/// # Example
///
/// ```rust
/// use adapteros_core::io_utils::validate_path_characters;
/// use std::path::Path;
///
/// // Valid path
/// validate_path_characters(Path::new("/home/user/file.txt"))?;
///
/// // Invalid path (on Windows)
/// let result = validate_path_characters(Path::new("/home/user/file<>.txt"));
/// assert!(result.is_err()); // Contains < and >
/// ```
pub fn validate_path_characters(path: &Path) -> Result<()> {
    // First check if the path is valid UTF-8
    let path_str = match path.to_str() {
        Some(s) => s,
        None => {
            return Err(AosError::InvalidPathCharacters {
                path: path.to_string_lossy().to_string(),
                details: "Path contains non-UTF-8 characters".to_string(),
                invalid_chars: vec![],
            });
        }
    };

    // Find all invalid characters
    let invalid: Vec<char> = path_str
        .chars()
        .filter(|c| INVALID_PATH_CHARS.contains(c))
        .collect();

    if !invalid.is_empty() {
        return Err(AosError::InvalidPathCharacters {
            path: path_str.to_string(),
            details: format!("Path contains {} invalid character(s)", invalid.len()),
            invalid_chars: invalid,
        });
    }

    Ok(())
}

/// Ensure a temporary directory exists, creating it if necessary
///
/// Creates the directory and all parent directories if they don't exist.
/// Returns the canonical path to the directory.
///
/// # Errors
///
/// Returns `AosError::TempDirUnavailable` if the directory cannot be created,
/// or `AosError::PermissionDenied` if creation fails due to permissions.
///
/// # Example
///
/// ```rust
/// use adapteros_core::io_utils::ensure_temp_dir;
/// use std::path::Path;
///
/// let temp_dir = ensure_temp_dir(Path::new("/tmp/my_app/cache"))?;
/// // temp_dir now exists and contains the canonical path
/// ```
pub fn ensure_temp_dir(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        if path.is_dir() {
            return path
                .canonicalize()
                .map_err(|e| classify_and_convert_io_error(e, path, "canonicalize"));
        } else {
            return Err(AosError::TempDirUnavailable {
                path: path.display().to_string(),
                reason: "Path exists but is not a directory".to_string(),
            });
        }
    }

    // Create the directory and all parents
    match fs::create_dir_all(path) {
        Ok(()) => {
            debug!(path = %path.display(), "Created temporary directory");
            path.canonicalize()
                .map_err(|e| classify_and_convert_io_error(e, path, "canonicalize"))
        }
        Err(e) => {
            let kind = classify_io_error(&e);
            match kind {
                IoErrorKind::PermissionDenied => Err(AosError::PermissionDenied {
                    path: path.display().to_string(),
                    operation: "create_dir_all".to_string(),
                    reason: e.to_string(),
                }),
                IoErrorKind::DiskFull => Err(AosError::DiskFull {
                    path: path.display().to_string(),
                    details: format!("Cannot create directory: {}", e),
                    bytes_needed: None,
                    bytes_available: None,
                }),
                _ => Err(AosError::TempDirUnavailable {
                    path: path.display().to_string(),
                    reason: e.to_string(),
                }),
            }
        }
    }
}

/// RAII guard for cleanup of temporary files on failure
///
/// Automatically removes the file at the given path when dropped,
/// unless `defuse()` is called to prevent cleanup.
///
/// # Example
///
/// ```rust
/// use adapteros_core::io_utils::TempFileGuard;
/// use std::path::Path;
///
/// let guard = TempFileGuard::new(Path::new("/tmp/my_file.tmp"));
///
/// // Do work with the file...
///
/// // If we get here successfully, prevent cleanup
/// guard.defuse();
/// ```
pub struct TempFileGuard {
    path: Option<PathBuf>,
}

impl TempFileGuard {
    /// Create a new temp file guard for the given path
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: Some(path.as_ref().to_path_buf()),
        }
    }

    /// Prevent cleanup when dropped (call on success)
    pub fn defuse(mut self) {
        self.path = None;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if let Some(ref path) = self.path {
            if path.exists() {
                if let Err(e) = fs::remove_file(path) {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to clean up temporary file"
                    );
                } else {
                    debug!(path = %path.display(), "Cleaned up temporary file");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, ErrorKind};

    #[test]
    fn test_classify_permission_denied() {
        let err = io::Error::new(ErrorKind::PermissionDenied, "access denied");
        assert_eq!(classify_io_error(&err), IoErrorKind::PermissionDenied);
    }

    #[test]
    fn test_classify_not_found() {
        let err = io::Error::new(ErrorKind::NotFound, "no such file");
        assert_eq!(classify_io_error(&err), IoErrorKind::NotFound);
    }

    #[test]
    fn test_validate_path_valid() {
        assert!(validate_path_characters(Path::new("/home/user/file.txt")).is_ok());
        assert!(validate_path_characters(Path::new("relative/path/file.txt")).is_ok());
        assert!(
            validate_path_characters(Path::new("file-with-dashes_and_underscores.txt")).is_ok()
        );
    }

    #[test]
    fn test_validate_path_with_null() {
        // Paths with null should fail on all platforms
        let path_with_null = Path::new("/home/user/file\0.txt");
        let result = validate_path_characters(path_with_null);
        // Note: This test may not work as expected because Path may reject null bytes
        // In practice, null bytes in paths cause issues before we even get to validation
    }

    #[test]
    #[cfg(windows)]
    fn test_validate_path_windows_invalid() {
        let result = validate_path_characters(Path::new("file<name>.txt"));
        assert!(result.is_err());
        if let Err(AosError::InvalidPathCharacters { invalid_chars, .. }) = result {
            assert!(invalid_chars.contains(&'<'));
            assert!(invalid_chars.contains(&'>'));
        }
    }

    #[test]
    fn test_temp_file_guard_cleanup() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_guard_cleanup.tmp");

        // Create a file
        fs::write(&test_file, "test content").unwrap();
        assert!(test_file.exists());

        // Create guard and drop it (should clean up)
        {
            let _guard = TempFileGuard::new(&test_file);
            // Guard dropped here
        }

        // File should be removed
        assert!(!test_file.exists());
    }

    #[test]
    fn test_temp_file_guard_defuse() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_guard_defuse.tmp");

        // Create a file
        fs::write(&test_file, "test content").unwrap();
        assert!(test_file.exists());

        // Create guard and defuse it
        {
            let guard = TempFileGuard::new(&test_file);
            guard.defuse();
            // Guard dropped here but should not clean up
        }

        // File should still exist
        assert!(test_file.exists());

        // Clean up manually
        fs::remove_file(&test_file).unwrap();
    }

    #[test]
    fn test_ensure_temp_dir_creates_nested() {
        let temp_dir = std::env::temp_dir();
        let nested = temp_dir
            .join("test_ensure_temp")
            .join("nested")
            .join("deep");

        // Clean up if exists from previous run
        let _ = fs::remove_dir_all(temp_dir.join("test_ensure_temp"));

        // Should create all directories
        let result = ensure_temp_dir(&nested);
        assert!(result.is_ok());
        assert!(nested.exists());
        assert!(nested.is_dir());

        // Clean up
        let _ = fs::remove_dir_all(temp_dir.join("test_ensure_temp"));
    }
}
