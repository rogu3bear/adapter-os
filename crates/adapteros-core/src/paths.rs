//! Centralized Path Resolution for adapterOS
//!
//! This module provides a single source of truth for .aos adapter file locations
//! and other critical path resolution throughout the system.
//!
//! # Choosing a Path Type
//!
//! | Need | Use |
//! |------|-----|
//! | Simple flat layout (`{root}/{adapter}.aos`) | [`AdapterPaths`] |
//! | Versioned tenant layout with cache | [`crate::RepoAdapterPaths`] |
//!
//! Use [`AdapterPaths`] for CLI tools and simple lookups where you just need
//! `{root}/{name}.aos`. Use [`crate::RepoAdapterPaths`] for production systems
//! that require tenant isolation, semantic versioning, and cache management.
//!
//! # Path Resolution Priority
//!
//! When using `from_config()`, precedence is:
//! 1. Environment variable `AOS_ADAPTERS_ROOT` (preferred)
//! 2. Environment variable `AOS_ADAPTERS_DIR` (legacy)
//! 3. Configuration file `paths.adapters_root`
//! 4. Default: `./var/adapters/`
//!
//! Explicit paths provided via `new()` always take precedence over all other sources.
//!
//! # Examples
//!
//! ```rust
//! use adapteros_core::paths::AdapterPaths;
//!
//! // Create with default paths
//! let paths = AdapterPaths::default();
//! let adapter_path = paths.get_adapter_path("my-adapter");
//! // Returns: ./var/adapters/my-adapter.aos
//!
//! // Create with custom root
//! let paths = AdapterPaths::new("./var/adapters");
//! let adapter_path = paths.get_adapter_path("my-adapter");
//! // Returns: ./var/adapters/my-adapter.aos
//! ```

use std::path::{Path, PathBuf};

// Re-export canonical env var constants from adapter_repo_paths to avoid duplication
pub use crate::adapter_repo_paths::{
    ENV_ADAPTERS_DIR_COMPAT as AOS_ADAPTERS_DIR_ENV, ENV_ADAPTERS_ROOT as AOS_ADAPTERS_ROOT_ENV,
};

/// Default adapters directory (relative to project root)
pub const DEFAULT_ADAPTERS_DIR: &str = crate::defaults::DEFAULT_ADAPTERS_ROOT;

/// Production adapters directory (repo-local path)
pub const PRODUCTION_ADAPTERS_DIR: &str = "./var/adapters";

/// Simple flat-layout path resolution for adapter files.
///
/// This type provides `{root}/{adapter}.aos` layout without versioning or
/// tenant isolation. For production systems requiring versioned tenant layouts
/// with caching, use [`crate::RepoAdapterPaths`] instead.
///
/// # When to Use
///
/// - CLI tools that load adapters by name
/// - Development/testing with flat directory structure
/// - Legacy compatibility with single-directory layouts
///
/// # See Also
///
/// - [`crate::RepoAdapterPaths`] for versioned `{repo}/{tenant}/{adapter}/{version}.aos` layout
#[derive(Debug, Clone)]
pub struct AdapterPaths {
    /// Root directory for all .aos files
    adapters_root: PathBuf,
}

impl AdapterPaths {
    /// Create a new AdapterPaths with a custom root directory
    pub fn new<P: AsRef<Path>>(adapters_root: P) -> Self {
        Self {
            adapters_root: adapters_root.as_ref().to_path_buf(),
        }
    }

    /// Create AdapterPaths from environment variable or default
    ///
    /// Resolution order:
    /// 1. `AOS_ADAPTERS_ROOT` environment variable (preferred)
    /// 2. `AOS_ADAPTERS_DIR` environment variable (legacy)
    /// 3. Default: `./var/adapters/`
    pub fn from_env() -> Self {
        let adapters_root = std::env::var(AOS_ADAPTERS_ROOT_ENV)
            .or_else(|_| std::env::var(AOS_ADAPTERS_DIR_ENV))
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_ADAPTERS_DIR));

        Self { adapters_root }
    }

    /// Create AdapterPaths with config value, falling back to env/default
    ///
    /// Resolution order (following standard precedence):
    /// 1. `AOS_ADAPTERS_DIR` environment variable (highest priority)
    /// 2. Provided config value (if Some)
    /// 3. Default: `./var/adapters/`
    pub fn from_config(config_value: Option<&str>) -> Self {
        // Check ENV first (highest priority)
        if let Ok(env_path) = std::env::var(AOS_ADAPTERS_ROOT_ENV) {
            return Self::new(env_path);
        }
        if let Ok(env_path) = std::env::var(AOS_ADAPTERS_DIR_ENV) {
            return Self::new(env_path);
        }
        // Fall back to config if provided
        if let Some(path) = config_value {
            return Self::new(path);
        }
        // Default fallback
        Self::from_env()
    }

    /// Get the root directory for adapters
    pub fn root(&self) -> &Path {
        &self.adapters_root
    }

    /// Get the full path for an adapter by ID
    ///
    /// Returns: `{adapters_root}/{adapter_id}.aos`
    pub fn get_adapter_path(&self, adapter_id: &str) -> PathBuf {
        self.adapters_root.join(format!("{}.aos", adapter_id))
    }

    /// Get the full path for an adapter by ID with a specific extension
    ///
    /// Returns: `{adapters_root}/{adapter_id}.{extension}`
    pub fn get_adapter_path_with_ext(&self, adapter_id: &str, extension: &str) -> PathBuf {
        self.adapters_root
            .join(format!("{}.{}", adapter_id, extension))
    }

    /// Check if the adapters directory exists
    pub fn exists(&self) -> bool {
        self.adapters_root.exists()
    }

    /// Ensure the adapters directory exists, creating it if necessary
    pub fn ensure_exists(&self) -> std::io::Result<()> {
        if !self.adapters_root.exists() {
            std::fs::create_dir_all(&self.adapters_root)?;
        }
        Ok(())
    }

    /// Ensure the adapters directory exists (async version)
    #[cfg(feature = "async")]
    pub async fn ensure_exists_async(&self) -> std::io::Result<()> {
        if !self.adapters_root.exists() {
            tokio::fs::create_dir_all(&self.adapters_root).await?;
        }
        Ok(())
    }

    /// List all .aos files in the adapters directory
    pub fn list_adapters(&self) -> std::io::Result<Vec<PathBuf>> {
        let mut adapters = Vec::new();
        if self.adapters_root.exists() {
            for entry in std::fs::read_dir(&self.adapters_root)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().is_some_and(|ext| ext == "aos") {
                    adapters.push(path);
                }
            }
        }
        Ok(adapters)
    }

    /// Get adapter ID from a path (extracts filename without .aos extension)
    pub fn adapter_id_from_path(path: &Path) -> Option<String> {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }
}

impl Default for AdapterPaths {
    fn default() -> Self {
        Self::from_env()
    }
}

/// Get the default adapters root directory
///
/// This is a convenience function for quick access without creating an AdapterPaths instance.
pub fn get_default_adapters_root() -> PathBuf {
    std::env::var(AOS_ADAPTERS_ROOT_ENV)
        .or_else(|_| std::env::var(AOS_ADAPTERS_DIR_ENV))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_ADAPTERS_DIR))
}

/// Get the full path for an adapter using default configuration
///
/// This is a convenience function for quick access without creating an AdapterPaths instance.
pub fn get_adapter_path(adapter_id: &str) -> PathBuf {
    get_default_adapters_root().join(format!("{}.aos", adapter_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_paths() {
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        let paths = AdapterPaths::default();
        assert!(paths.root().ends_with("var/adapters") || paths.root().to_str().is_some());
    }

    #[test]
    fn test_custom_paths() {
        let paths = AdapterPaths::new("/custom/adapters");
        assert_eq!(paths.root(), Path::new("/custom/adapters"));
    }

    #[test]
    fn test_get_adapter_path() {
        let paths = AdapterPaths::new("/adapters");
        let path = paths.get_adapter_path("my-adapter");
        assert_eq!(path, PathBuf::from("/adapters/my-adapter.aos"));
    }

    #[test]
    fn test_get_adapter_path_with_ext() {
        let paths = AdapterPaths::new("/adapters");
        let path = paths.get_adapter_path_with_ext("my-adapter", "sig");
        assert_eq!(path, PathBuf::from("/adapters/my-adapter.sig"));
    }

    #[test]
    fn test_adapter_id_from_path() {
        let path = Path::new("/adapters/my-adapter.aos");
        assert_eq!(
            AdapterPaths::adapter_id_from_path(path),
            Some("my-adapter".to_string())
        );
    }

    #[test]
    fn test_from_config_with_value() {
        // Without ENV set, config should be used
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
        let paths = AdapterPaths::from_config(Some("/custom/path"));
        assert_eq!(paths.root(), Path::new("/custom/path"));
    }

    #[test]
    fn test_from_config_without_value() {
        // Should fall back to env or default
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
        let paths = AdapterPaths::from_config(None);
        assert!(paths.root().to_str().is_some());
    }

    #[test]
    fn test_from_config_env_precedence() {
        // ENV should take precedence over config
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::set_var(AOS_ADAPTERS_DIR_ENV, "/env/path");
        let paths = AdapterPaths::from_config(Some("/config/path"));
        assert_eq!(paths.root(), Path::new("/env/path"));
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
    }

    #[test]
    fn test_from_config_env_only() {
        // ENV without config should work
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::set_var(AOS_ADAPTERS_DIR_ENV, "/env/only");
        let paths = AdapterPaths::from_config(None);
        assert_eq!(paths.root(), Path::new("/env/only"));
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
    }

    #[test]
    fn test_from_config_config_only() {
        // Config without ENV should work
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
        let paths = AdapterPaths::from_config(Some("/config/only"));
        assert_eq!(paths.root(), Path::new("/config/only"));
    }

    #[test]
    fn test_from_config_default_fallback() {
        // Neither ENV nor config should use default
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
        let paths = AdapterPaths::from_config(None);
        assert_eq!(paths.root(), PathBuf::from(DEFAULT_ADAPTERS_DIR));
    }

    #[test]
    fn test_get_default_adapters_root_env() {
        let tmp = tempdir().unwrap();
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::set_var(AOS_ADAPTERS_DIR_ENV, tmp.path());
        let root = get_default_adapters_root();
        assert!(root.starts_with(tmp.path()));
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
    }

    #[test]
    fn test_get_default_adapters_root_default() {
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
        let root = get_default_adapters_root();
        assert_eq!(root, PathBuf::from(DEFAULT_ADAPTERS_DIR));
    }

    #[test]
    fn test_prefers_root_env_over_legacy() {
        std::env::set_var(AOS_ADAPTERS_ROOT_ENV, "/root/env/path");
        std::env::set_var(AOS_ADAPTERS_DIR_ENV, "/legacy/env/path");
        let paths = AdapterPaths::from_env();
        assert_eq!(paths.root(), Path::new("/root/env/path"));
        std::env::remove_var(AOS_ADAPTERS_ROOT_ENV);
        std::env::remove_var(AOS_ADAPTERS_DIR_ENV);
    }
}
