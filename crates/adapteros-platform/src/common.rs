//! Common platform utilities
//!
//! Provides common utilities and helpers for cross-platform operations.

use adapteros_core::{AosError, Result};
use std::path::{Path, PathBuf};

/// Common platform utilities
pub struct PlatformUtils;

impl PlatformUtils {
    /// Get the current platform
    pub fn current_platform() -> &'static str {
        #[cfg(target_os = "windows")]
        return "windows";

        #[cfg(target_os = "macos")]
        return "macos";

        #[cfg(target_os = "linux")]
        return "linux";

        #[cfg(all(unix, not(any(target_os = "macos", target_os = "linux"))))]
        return "unix";

        #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux", unix)))]
        return "unknown";
    }

    /// Check if running on Windows
    pub fn is_windows() -> bool {
        cfg!(target_os = "windows")
    }

    /// Check if running on macOS
    pub fn is_macos() -> bool {
        cfg!(target_os = "macos")
    }

    /// Check if running on Linux
    pub fn is_linux() -> bool {
        cfg!(target_os = "linux")
    }

    /// Check if running on Unix-like system
    pub fn is_unix() -> bool {
        cfg!(unix)
    }

    /// Get the path separator for the current platform
    pub fn path_separator() -> char {
        std::path::MAIN_SEPARATOR
    }

    /// Join paths using the current platform's separator
    pub fn join_paths(paths: &[&str]) -> PathBuf {
        let mut result = PathBuf::new();
        for path in paths {
            result.push(path);
        }
        result
    }

    /// Normalize path separators for the current platform
    pub fn normalize_path_separators(path: &str) -> String {
        if Self::is_windows() {
            path.replace('/', "\\")
        } else {
            path.replace('\\', "/")
        }
    }

    /// Get the home directory for the current user
    pub fn home_dir() -> Result<PathBuf> {
        dirs::home_dir()
            .ok_or_else(|| AosError::Platform("Failed to get home directory".to_string()))
    }

    /// Expand a path, resolving `~` to the home directory
    ///
    /// This function handles tilde expansion for paths:
    /// - `~` or `~/...` expands to the user's home directory
    /// - Paths without tilde are returned as-is
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use adapteros_platform::common::PlatformUtils;
    ///
    /// // Expands ~ to home directory
    /// let path = PlatformUtils::expand_path("~/.cache/adapteros").unwrap();
    /// // Returns something like /Users/username/.cache/adapteros
    ///
    /// // Non-tilde paths pass through unchanged
    /// let path = PlatformUtils::expand_path("var/cache").unwrap();
    /// // Returns var/cache
    /// ```
    pub fn expand_path<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
        let path = path.as_ref();
        let path_str = path.to_string_lossy();

        if path_str.starts_with("~/") {
            let home = Self::home_dir()?;
            Ok(home.join(&path_str[2..]))
        } else if path_str == "~" {
            Self::home_dir()
        } else {
            Ok(path.to_path_buf())
        }
    }

    // =========================================================================
    // AdapterOS Directory Functions
    // =========================================================================
    //
    // API Design Note:
    // - Functions returning `PathBuf`: Cannot fail (env vars with defaults)
    // - Functions returning `Result<PathBuf>`: Can fail (require home dir expansion)
    //
    // This asymmetry is intentional - relative paths always work, but `~`
    // expansion requires a home directory which may not exist on all systems.
    // =========================================================================

    /// Get the AdapterOS var directory (runtime data)
    ///
    /// This is the canonical location for all runtime data including:
    /// - `var/model-cache/` - Downloaded models from HuggingFace
    /// - `var/adapters/` - LoRA adapter weights
    /// - `var/artifacts/` - Training artifacts
    /// - `var/bundles/` - Telemetry bundles
    /// - `var/alerts/` - Alert logs
    /// - `var/aos-cp.sqlite3` - Database
    ///
    /// Respects `AOS_VAR_DIR` env var, defaults to `var/` relative to cwd.
    pub fn aos_var_dir() -> PathBuf {
        std::env::var("AOS_VAR_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("var"))
    }

    /// Get the model cache directory
    ///
    /// Returns the directory where downloaded models are cached.
    /// Respects `AOS_MODEL_CACHE_DIR` env var, defaults to `var/model-cache`.
    pub fn aos_model_cache_dir() -> PathBuf {
        std::env::var("AOS_MODEL_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| Self::aos_var_dir().join("model-cache"))
    }

    /// Get the adapters directory
    ///
    /// Returns the directory where LoRA adapter weights are stored.
    /// Delegates to `AdapterPaths` for consistent path resolution.
    /// Respects `AOS_ADAPTERS_DIR` env var, defaults to `var/adapters`.
    ///
    /// Note: For config-aware path resolution, use `AdapterPaths::from_config()` directly.
    pub fn aos_adapters_dir() -> PathBuf {
        use adapteros_core::paths::AdapterPaths;
        AdapterPaths::from_config(None).root().to_path_buf()
    }

    /// Get the artifacts directory
    ///
    /// Returns the directory where training artifacts are stored.
    /// Respects `AOS_ARTIFACTS_DIR` env var, defaults to `var/artifacts`.
    pub fn aos_artifacts_dir() -> PathBuf {
        std::env::var("AOS_ARTIFACTS_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| Self::aos_var_dir().join("artifacts"))
    }

    /// Get the user-specific AdapterOS cache directory (optional)
    ///
    /// Returns `~/.cache/adapteros` on Unix systems (expanded to absolute path).
    /// Use this for user-specific cached data that should persist across projects.
    /// For project-local runtime data, use `aos_var_dir()` instead.
    pub fn aos_user_cache_dir() -> Result<PathBuf> {
        Self::expand_path("~/.cache/adapteros")
    }

    /// Get the user-specific AdapterOS config directory
    ///
    /// Returns `~/.config/adapteros` on Unix systems (expanded to absolute path).
    /// This is the location for user-specific configuration files.
    pub fn aos_user_config_dir() -> Result<PathBuf> {
        Self::expand_path("~/.config/adapteros")
    }

    /// Get the cache directory for the current user
    pub fn cache_dir() -> Result<PathBuf> {
        dirs::cache_dir()
            .ok_or_else(|| AosError::Platform("Failed to get cache directory".to_string()))
    }

    /// Get the config directory for the current user
    pub fn config_dir() -> Result<PathBuf> {
        dirs::config_dir()
            .ok_or_else(|| AosError::Platform("Failed to get config directory".to_string()))
    }

    /// Get the data directory for the current user
    pub fn data_dir() -> Result<PathBuf> {
        dirs::data_dir()
            .ok_or_else(|| AosError::Platform("Failed to get data directory".to_string()))
    }

    /// Get the temp directory
    pub fn temp_dir() -> PathBuf {
        std::env::temp_dir()
    }

    /// Check if a path is absolute
    pub fn is_absolute_path(path: &Path) -> bool {
        path.is_absolute()
    }

    /// Check if a path is relative
    pub fn is_relative_path(path: &Path) -> bool {
        path.is_relative()
    }

    /// Get the file extension from a path
    pub fn get_file_extension(path: &Path) -> Option<String> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_string())
    }

    /// Get the file stem from a path
    pub fn get_file_stem(path: &Path) -> Option<String> {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(|s| s.to_string())
    }

    /// Get the file name from a path
    pub fn get_file_name(path: &Path) -> Option<String> {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|s| s.to_string())
    }

    /// Get the parent directory from a path
    pub fn get_parent_dir(path: &Path) -> Option<PathBuf> {
        path.parent().map(|p| p.to_path_buf())
    }

    /// Check if a path exists
    pub fn path_exists(path: &Path) -> bool {
        path.exists()
    }

    /// Check if a path is a file
    pub fn is_file(path: &Path) -> bool {
        path.is_file()
    }

    /// Check if a path is a directory
    pub fn is_directory(path: &Path) -> bool {
        path.is_dir()
    }

    /// Check if a path is a symbolic link
    pub fn is_symlink(path: &Path) -> bool {
        path.is_symlink()
    }

    /// Get the file size
    pub fn get_file_size(path: &Path) -> Result<u64> {
        std::fs::metadata(path)
            .map(|metadata| metadata.len())
            .map_err(|e| AosError::Platform(format!("Failed to get file size: {}", e)))
    }

    /// Create a directory
    pub fn create_dir(path: &Path) -> Result<()> {
        std::fs::create_dir(path)
            .map_err(|e| AosError::Platform(format!("Failed to create directory: {}", e)))
    }

    /// Create a directory and all parents
    pub fn create_dir_all(path: &Path) -> Result<()> {
        std::fs::create_dir_all(path)
            .map_err(|e| AosError::Platform(format!("Failed to create directory: {}", e)))
    }

    /// Remove a file
    pub fn remove_file(path: &Path) -> Result<()> {
        std::fs::remove_file(path)
            .map_err(|e| AosError::Platform(format!("Failed to remove file: {}", e)))
    }

    /// Remove a directory
    pub fn remove_dir(path: &Path) -> Result<()> {
        std::fs::remove_dir(path)
            .map_err(|e| AosError::Platform(format!("Failed to remove directory: {}", e)))
    }

    /// Remove a directory and all contents
    pub fn remove_dir_all(path: &Path) -> Result<()> {
        std::fs::remove_dir_all(path)
            .map_err(|e| AosError::Platform(format!("Failed to remove directory: {}", e)))
    }

    /// Copy a file
    pub fn copy_file(src: &Path, dst: &Path) -> Result<u64> {
        std::fs::copy(src, dst)
            .map_err(|e| AosError::Platform(format!("Failed to copy file: {}", e)))
    }

    /// Move a file
    pub fn move_file(src: &Path, dst: &Path) -> Result<()> {
        std::fs::rename(src, dst)
            .map_err(|e| AosError::Platform(format!("Failed to move file: {}", e)))
    }

    /// Read file contents
    pub fn read_file(path: &Path) -> Result<Vec<u8>> {
        std::fs::read(path).map_err(|e| AosError::Platform(format!("Failed to read file: {}", e)))
    }

    /// Read file contents as string
    pub fn read_file_string(path: &Path) -> Result<String> {
        std::fs::read_to_string(path)
            .map_err(|e| AosError::Platform(format!("Failed to read file as string: {}", e)))
    }

    /// Write file contents
    pub fn write_file(path: &Path, contents: &[u8]) -> Result<()> {
        std::fs::write(path, contents)
            .map_err(|e| AosError::Platform(format!("Failed to write file: {}", e)))
    }

    /// Write file contents as string
    pub fn write_file_string(path: &Path, contents: &str) -> Result<()> {
        std::fs::write(path, contents)
            .map_err(|e| AosError::Platform(format!("Failed to write file as string: {}", e)))
    }

    /// Get environment variable
    pub fn get_env_var(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    /// Set environment variable
    pub fn set_env_var(name: &str, value: &str) -> Result<()> {
        std::env::set_var(name, value);
        Ok(())
    }

    /// Get current working directory
    pub fn current_dir() -> Result<PathBuf> {
        std::env::current_dir()
            .map_err(|e| AosError::Platform(format!("Failed to get current directory: {}", e)))
    }

    /// Change current working directory
    pub fn change_dir(path: &Path) -> Result<()> {
        std::env::set_current_dir(path)
            .map_err(|e| AosError::Platform(format!("Failed to change directory: {}", e)))
    }

    /// Get the process ID
    pub fn process_id() -> u32 {
        std::process::id()
    }

    /// Get the user ID (Unix only)
    pub fn user_id() -> Option<u32> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(".").ok().map(|metadata| metadata.uid())
        }
        #[cfg(not(unix))]
        None
    }

    /// Get the group ID (Unix only)
    pub fn group_id() -> Option<u32> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(".").ok().map(|metadata| metadata.gid())
        }
        #[cfg(not(unix))]
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_platform_detection() {
        let platform = PlatformUtils::current_platform();
        assert!(!platform.is_empty());

        assert_eq!(PlatformUtils::is_windows(), cfg!(target_os = "windows"));
        assert_eq!(PlatformUtils::is_macos(), cfg!(target_os = "macos"));
        assert_eq!(PlatformUtils::is_linux(), cfg!(target_os = "linux"));
        assert_eq!(PlatformUtils::is_unix(), cfg!(unix));
    }

    #[test]
    fn test_path_operations() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_file = temp_dir.path().join("test.txt");

        // Test file operations
        PlatformUtils::write_file_string(&test_file, "hello world")?;
        assert!(PlatformUtils::path_exists(&test_file));
        assert!(PlatformUtils::is_file(&test_file));
        assert!(!PlatformUtils::is_directory(&test_file));

        let content = PlatformUtils::read_file_string(&test_file)?;
        assert_eq!(content, "hello world");

        let size = PlatformUtils::get_file_size(&test_file)?;
        assert_eq!(size, 11);

        Ok(())
    }

    #[test]
    fn test_directory_operations() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let test_dir = temp_dir.path().join("test_dir");

        // Test directory operations
        PlatformUtils::create_dir(&test_dir)?;
        assert!(PlatformUtils::path_exists(&test_dir));
        assert!(PlatformUtils::is_directory(&test_dir));
        assert!(!PlatformUtils::is_file(&test_dir));

        PlatformUtils::remove_dir(&test_dir)?;
        assert!(!PlatformUtils::path_exists(&test_dir));

        Ok(())
    }

    #[test]
    fn test_path_utilities() {
        let path = PathBuf::from("test/file.txt");

        assert_eq!(
            PlatformUtils::get_file_extension(&path),
            Some("txt".to_string())
        );
        assert_eq!(
            PlatformUtils::get_file_stem(&path),
            Some("file".to_string())
        );
        assert_eq!(
            PlatformUtils::get_file_name(&path),
            Some("file.txt".to_string())
        );
        assert_eq!(
            PlatformUtils::get_parent_dir(&path),
            Some(PathBuf::from("test"))
        );
    }

    #[test]
    fn test_expand_path_tilde() -> Result<()> {
        // Test tilde expansion
        let expanded = PlatformUtils::expand_path("~/.cache/adapteros")?;
        assert!(expanded.is_absolute());
        assert!(expanded.to_string_lossy().ends_with(".cache/adapteros"));
        assert!(!expanded.to_string_lossy().contains('~'));

        // Test standalone tilde
        let home = PlatformUtils::expand_path("~")?;
        assert!(home.is_absolute());
        assert_eq!(home, PlatformUtils::home_dir()?);

        // Test non-tilde paths pass through
        let regular = PlatformUtils::expand_path("var/cache")?;
        assert_eq!(regular, PathBuf::from("var/cache"));

        // Test absolute paths pass through
        let absolute = PlatformUtils::expand_path("/tmp/test")?;
        assert_eq!(absolute, PathBuf::from("/tmp/test"));

        Ok(())
    }

    #[test]
    fn test_aos_var_directories() {
        // Test var directory defaults (relative paths)
        let var_dir = PlatformUtils::aos_var_dir();
        assert_eq!(var_dir, PathBuf::from("var"));

        let model_cache_dir = PlatformUtils::aos_model_cache_dir();
        assert_eq!(model_cache_dir, PathBuf::from("var/model-cache"));

        let adapters_dir = PlatformUtils::aos_adapters_dir();
        assert_eq!(adapters_dir, PathBuf::from("var/adapters"));

        let artifacts_dir = PlatformUtils::aos_artifacts_dir();
        assert_eq!(artifacts_dir, PathBuf::from("var/artifacts"));
    }

    #[test]
    fn test_aos_user_directories() -> Result<()> {
        // Test user-specific directories (absolute paths with tilde expansion)
        let user_cache_dir = PlatformUtils::aos_user_cache_dir()?;
        assert!(user_cache_dir.is_absolute());
        assert!(user_cache_dir.to_string_lossy().ends_with(".cache/adapteros"));

        let user_config_dir = PlatformUtils::aos_user_config_dir()?;
        assert!(user_config_dir.is_absolute());
        assert!(user_config_dir.to_string_lossy().ends_with(".config/adapteros"));

        Ok(())
    }
}
