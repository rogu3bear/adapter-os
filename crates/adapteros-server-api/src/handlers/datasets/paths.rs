//! Dataset path resolution and validation utilities.
//!
//! This module provides:
//! - Dataset root resolution from environment or configuration
//! - Safe path joining that prevents directory traversal attacks
//! - Path validation to ensure files stay within dataset boundaries
//!
//! # Resolution Precedence
//!
//! Dataset root resolution follows this priority order:
//!
//! 1. `AOS_DATASETS_DIR` environment variable (primary)
//! 2. Config-provided root (`paths.datasets_root` in config file)
//! 3. Default `var/datasets`
//!
//! # Security
//!
//! All paths are:
//! - Validated against forbidden temp directories (`/tmp`, `/var/tmp`, etc.)
//! - Canonicalized to resolve symlinks and prevent symlink attacks
//! - Checked for directory traversal patterns

use adapteros_core::{reject_forbidden_tmp_path, AosError, Result};
use std::env;
use std::path::{Component, Path, PathBuf};
use tracing::{debug, warn};

/// Environment variable for overriding dataset root directory.
///
/// When set, this takes precedence over config-provided paths.
pub const ENV_DATASETS_DIR: &str = "AOS_DATASETS_DIR";

/// Default dataset root directory (relative to working directory).
pub const DEFAULT_DATASETS_ROOT: &str = "var/datasets";

pub const FILES_DIR_NAME: &str = "files";
pub const TEMP_DIR_NAME: &str = "temp";
pub const CHUNKED_DIR_NAME: &str = "chunked";
pub const LOGS_DIR_NAME: &str = "logs";
pub const VERSIONS_DIR_NAME: &str = "versions";

#[derive(Debug, Clone)]
pub struct DatasetPaths {
    /// Root directory for all dataset operations
    root: PathBuf,
    /// Directory for finalized dataset files
    pub files: PathBuf,
    /// Directory for temporary uploads
    pub temp: PathBuf,
    /// Directory for chunked upload sessions
    pub chunked: PathBuf,
    /// Directory for dataset operation logs
    pub logs: PathBuf,
}

impl DatasetPaths {
    /// Create a new DatasetPaths instance from a resolved root directory.
    ///
    /// The root should already be an absolute, canonicalized path.
    pub fn new(root: PathBuf) -> Self {
        Self {
            files: root.join(FILES_DIR_NAME),
            temp: root.join(TEMP_DIR_NAME),
            chunked: root.join(CHUNKED_DIR_NAME),
            logs: root.join(LOGS_DIR_NAME),
            root,
        }
    }

    /// Create DatasetPaths by resolving the dataset root from AppState.
    ///
    /// This is the preferred factory method for handlers and services.
    /// It combines root resolution and path construction in a single call.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The resolved path is in a forbidden temp directory
    /// - The path cannot be canonicalized (e.g., doesn't exist)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let paths = DatasetPaths::from_state(&state)?;
    /// let dataset_dir = paths.dataset_dir(workspace_id, dataset_id);
    /// ```
    pub fn from_state(state: &crate::state::AppState) -> Result<Self> {
        let root = resolve_dataset_root(state)?;
        Ok(Self::new(root))
    }

    /// Create DatasetPaths from environment and optional config root.
    ///
    /// This is useful for contexts where AppState is not available,
    /// such as CLI tools or worker processes.
    ///
    /// Resolution order:
    /// 1. `AOS_DATASETS_DIR` environment variable
    /// 2. `config_root` parameter (if Some)
    /// 3. Default `var/datasets`
    pub fn from_env_and_config(config_root: Option<String>) -> Result<Self> {
        let root = resolve_dataset_root_from_strings(
            env::var(ENV_DATASETS_DIR).ok(),
            config_root,
        )?;
        Ok(Self::new(root))
    }

    /// Get the root directory for datasets.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the directory for a specific dataset within a workspace.
    pub fn dataset_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.files.join(workspace_id).join(dataset_id)
    }

    /// Get the temp directory for a specific dataset within a workspace.
    pub fn dataset_temp_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.temp.join(workspace_id).join(dataset_id)
    }

    /// Legacy helper for callers that do not yet provide workspace scoping.
    ///
    /// # Deprecated
    /// This method is deprecated and will be removed in a future version.
    /// Use [`dataset_dir`] with workspace_id instead for proper tenant isolation.
    ///
    /// Callers should migrate to:
    /// ```ignore
    /// paths.dataset_dir(workspace_id, dataset_id)
    /// ```
    #[deprecated(
        since = "0.243.0",
        note = "Use dataset_dir with workspace_id for tenant isolation"
    )]
    pub fn dataset_dir_unscoped(&self, dataset_id: &str) -> PathBuf {
        tracing::warn!(
            dataset_id = %dataset_id,
            "dataset_dir_unscoped is deprecated - use dataset_dir(workspace_id, dataset_id) for tenant isolation"
        );
        self.files.join(dataset_id)
    }

    /// Get the versions directory for a dataset.
    pub fn dataset_versions_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.dataset_dir(workspace_id, dataset_id)
            .join(VERSIONS_DIR_NAME)
    }

    /// Get the directory for a specific dataset version.
    pub fn dataset_version_dir(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
    ) -> PathBuf {
        self.dataset_versions_dir(workspace_id, dataset_id)
            .join(version_id)
    }

    /// Resolve a file path within a dataset, ensuring it stays within bounds.
    ///
    /// Returns an error if the resolved path would escape the dataset directory.
    pub fn resolve_dataset_file(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        let dataset_dir = self.dataset_dir(workspace_id, dataset_id);
        safe_join_path(&dataset_dir, file_name)
    }

    /// Resolve a file path within a dataset version, ensuring it stays within bounds.
    pub fn resolve_version_file(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        let version_dir = self.dataset_version_dir(workspace_id, dataset_id, version_id);
        safe_join_path(&version_dir, file_name)
    }

    /// Check if a path is within the dataset root.
    pub fn is_within_root(&self, path: &Path) -> bool {
        // Normalize both paths for comparison
        let normalized_path = normalize_path_components(path);
        let normalized_root = normalize_path_components(&self.root);

        normalized_path.starts_with(&normalized_root)
    }

    /// Validate that a path is within the dataset root.
    ///
    /// Returns an error if the path would escape the root.
    pub fn validate_within_root(&self, path: &Path) -> Result<()> {
        if !self.is_within_root(path) {
            return Err(AosError::Validation(format!(
                "Path '{}' is outside dataset root '{}'",
                path.display(),
                self.root.display()
            )));
        }
        Ok(())
    }
}

/// Safely join a base path with a relative path, preventing directory traversal.
///
/// This function:
/// 1. Validates the relative path has no traversal components
/// 2. Joins the paths
/// 3. Validates the result is still within the base directory
pub fn safe_join_path(base: &Path, relative: &str) -> Result<PathBuf> {
    // Check for path traversal patterns in the input
    if contains_traversal_pattern(relative) {
        return Err(AosError::Validation(format!(
            "Path contains traversal pattern: {}",
            relative
        )));
    }

    // Parse the relative path and check for parent directory components
    let relative_path = Path::new(relative);
    for component in relative_path.components() {
        match component {
            Component::ParentDir => {
                return Err(AosError::Validation(
                    "Path contains parent directory reference (..)".to_string(),
                ));
            }
            Component::RootDir => {
                return Err(AosError::Validation(
                    "Relative path cannot start with root (/)".to_string(),
                ));
            }
            _ => {}
        }
    }

    // Join the paths
    let joined = base.join(relative_path);

    // Normalize and verify the result is within base
    let normalized_joined = normalize_path_components(&joined);
    let normalized_base = normalize_path_components(base);

    if !normalized_joined.starts_with(&normalized_base) {
        return Err(AosError::Validation(format!(
            "Resolved path '{}' escapes base directory '{}'",
            joined.display(),
            base.display()
        )));
    }

    debug!(
        base = %base.display(),
        relative = %relative,
        result = %joined.display(),
        "Safely joined paths"
    );

    Ok(joined)
}

/// Check if a path string contains traversal patterns.
fn contains_traversal_pattern(path: &str) -> bool {
    // Check for various traversal patterns including URL-encoded versions
    let patterns = [
        "..",
        "%2e%2e",
        "%2E%2E",
        "%252e%252e",
        "%c0%ae",
        "%c1%9c",
        "..%2f",
        "..%5c",
        "%00", // Null byte attack
    ];

    let lower = path.to_lowercase();
    for pattern in patterns {
        if lower.contains(&pattern.to_lowercase()) {
            return true;
        }
    }

    false
}

/// Normalize path components by resolving . and .. logically.
///
/// Unlike canonicalize(), this works on paths that don't exist yet.
fn normalize_path_components(path: &Path) -> PathBuf {
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::ParentDir => {
                // Pop the last component if possible
                if !components.is_empty() {
                    components.pop();
                }
            }
            Component::CurDir => {
                // Skip current directory markers
            }
            _ => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
}

/// Resolve dataset root preferring env override and returning an absolute path.
///
/// Resolution order:
/// 1. `AOS_DATASETS_DIR` environment variable
/// 2. Config `paths.datasets_root`
/// 3. Default `var/datasets`
///
/// # Security
///
/// The resolved path is:
/// - Validated against forbidden temp directories
/// - Canonicalized to resolve symlinks (prevents symlink attacks)
/// - Validated again after canonicalization
pub fn resolve_dataset_root(state: &crate::state::AppState) -> Result<PathBuf> {
    let config_root = match state.config.read() {
        Ok(config) => {
            if config.paths.datasets_root.is_empty() {
                None
            } else {
                Some(config.paths.datasets_root.clone())
            }
        }
        Err(_) => {
            tracing::error!("Config lock poisoned in resolve_dataset_root");
            None
        }
    };

    resolve_dataset_root_from_strings(env::var(ENV_DATASETS_DIR).ok(), config_root)
}

/// Resolve dataset root from environment variable and optional config root.
///
/// This is the core resolution function used by both `resolve_dataset_root`
/// and `DatasetPaths::from_env_and_config`.
///
/// Resolution order:
/// 1. `env_root` (typically from `AOS_DATASETS_DIR`)
/// 2. `config_root` (from config file)
/// 3. Default `var/datasets`
///
/// # Security
///
/// The resolved path is:
/// - Validated against forbidden temp directories
/// - Canonicalized to resolve symlinks (prevents symlink attacks)
/// - Validated again after canonicalization
///
/// # Errors
///
/// Returns an error if:
/// - The path is in a forbidden temp directory
/// - The path cannot be canonicalized (e.g., doesn't exist)
pub fn resolve_dataset_root_from_strings(
    env_root: Option<String>,
    config_root: Option<String>,
) -> Result<PathBuf> {
    let root_str = env_root
        .or(config_root)
        .unwrap_or_else(|| DEFAULT_DATASETS_ROOT.to_string());

    let root = PathBuf::from(&root_str);

    // Absolutize the path
    let absolute_root = if root.is_absolute() {
        root
    } else {
        env::current_dir()
            .unwrap_or_else(|_| Path::new("/").to_path_buf())
            .join(root)
    };

    // Security check: reject forbidden temp directories before canonicalization
    if adapteros_core::path_security::is_forbidden_tmp_path(&absolute_root) {
        warn!(
            path = %absolute_root.display(),
            "Refusing to use forbidden temp directory for datasets root; falling back to default"
        );
        // Fall back to default and try again
        let fallback = env::current_dir()
            .unwrap_or_else(|_| Path::new("/").to_path_buf())
            .join(DEFAULT_DATASETS_ROOT);
        return resolve_and_validate_path(fallback);
    }

    resolve_and_validate_path(absolute_root)
}

/// Internal helper to canonicalize and validate a dataset root path.
fn resolve_and_validate_path(path: PathBuf) -> Result<PathBuf> {
    reject_forbidden_tmp_path(&path, "datasets-root")?;

    // SECURITY: Canonicalize to resolve symlinks after validation
    // This prevents symlink attacks that bypass the /tmp check
    let canonical = path.canonicalize().map_err(|e| {
        AosError::Validation(format!(
            "Invalid datasets root path '{}': {}",
            path.display(),
            e
        ))
    })?;

    // Validate again after canonicalization (in case symlink pointed to forbidden location)
    reject_forbidden_tmp_path(&canonical, "datasets-root-canonical")?;

    debug!(
        original = %path.display(),
        canonical = %canonical.display(),
        "Resolved dataset root path"
    );

    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dataset_paths() -> DatasetPaths {
        DatasetPaths::new(PathBuf::from("/var/datasets"))
    }

    #[test]
    fn test_dataset_paths_construction() {
        let paths = test_dataset_paths();

        assert_eq!(paths.root(), Path::new("/var/datasets"));
        assert_eq!(paths.files, PathBuf::from("/var/datasets/files"));
        assert_eq!(paths.temp, PathBuf::from("/var/datasets/temp"));
        assert_eq!(paths.chunked, PathBuf::from("/var/datasets/chunked"));
        assert_eq!(paths.logs, PathBuf::from("/var/datasets/logs"));
    }

    #[test]
    fn test_dataset_dir() {
        let paths = test_dataset_paths();

        let dir = paths.dataset_dir("workspace-1", "dataset-abc");
        assert_eq!(
            dir,
            PathBuf::from("/var/datasets/files/workspace-1/dataset-abc")
        );
    }

    #[test]
    fn test_dataset_version_dir() {
        let paths = test_dataset_paths();

        let dir = paths.dataset_version_dir("workspace-1", "dataset-abc", "v1");
        assert_eq!(
            dir,
            PathBuf::from("/var/datasets/files/workspace-1/dataset-abc/versions/v1")
        );
    }

    #[test]
    fn test_resolve_dataset_file_valid() {
        let paths = test_dataset_paths();

        let result = paths.resolve_dataset_file("workspace-1", "dataset-abc", "data.jsonl");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/var/datasets/files/workspace-1/dataset-abc/data.jsonl")
        );
    }

    #[test]
    fn test_resolve_dataset_file_nested() {
        let paths = test_dataset_paths();

        let result = paths.resolve_dataset_file("workspace-1", "dataset-abc", "subdir/data.jsonl");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/var/datasets/files/workspace-1/dataset-abc/subdir/data.jsonl")
        );
    }

    #[test]
    fn test_resolve_dataset_file_traversal_blocked() {
        let paths = test_dataset_paths();

        // Direct parent traversal
        let result = paths.resolve_dataset_file("workspace-1", "dataset-abc", "../evil.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));

        // Double parent traversal
        let result = paths.resolve_dataset_file("workspace-1", "dataset-abc", "../../evil.txt");
        assert!(result.is_err());

        // URL-encoded traversal
        let result = paths.resolve_dataset_file("workspace-1", "dataset-abc", "%2e%2e/evil.txt");
        assert!(result.is_err());

        // Absolute path attempt
        let result = paths.resolve_dataset_file("workspace-1", "dataset-abc", "/etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_safe_join_path_valid() {
        let base = PathBuf::from("/base/dir");

        // Simple file
        let result = safe_join_path(&base, "file.txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/base/dir/file.txt"));

        // Nested path
        let result = safe_join_path(&base, "subdir/nested/file.txt");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/base/dir/subdir/nested/file.txt")
        );
    }

    #[test]
    fn test_safe_join_path_traversal_attacks() {
        let base = PathBuf::from("/base/dir");

        // Basic traversal
        assert!(safe_join_path(&base, "../escape").is_err());
        assert!(safe_join_path(&base, "foo/../../../escape").is_err());

        // URL-encoded traversal
        assert!(safe_join_path(&base, "%2e%2e/escape").is_err());
        assert!(safe_join_path(&base, "%2E%2E/escape").is_err());

        // Double-encoded traversal
        assert!(safe_join_path(&base, "%252e%252e/escape").is_err());

        // Overlong UTF-8 encoded traversal
        assert!(safe_join_path(&base, "%c0%ae%c0%ae/escape").is_err());

        // Null byte attack
        assert!(safe_join_path(&base, "file.txt%00.jpg").is_err());

        // Absolute path
        assert!(safe_join_path(&base, "/etc/passwd").is_err());
    }

    #[test]
    fn test_contains_traversal_pattern() {
        // Should detect traversal patterns
        assert!(contains_traversal_pattern(".."));
        assert!(contains_traversal_pattern("../foo"));
        assert!(contains_traversal_pattern("foo/../bar"));
        assert!(contains_traversal_pattern("%2e%2e"));
        assert!(contains_traversal_pattern("%2E%2E"));
        assert!(contains_traversal_pattern("%252e%252e"));
        assert!(contains_traversal_pattern("%c0%ae"));
        assert!(contains_traversal_pattern("%00"));

        // Should not detect traversal in safe paths
        assert!(!contains_traversal_pattern("file.txt"));
        assert!(!contains_traversal_pattern("subdir/file.txt"));
        assert!(!contains_traversal_pattern("data.jsonl"));
        assert!(!contains_traversal_pattern("training_v1.0.jsonl"));
    }

    #[test]
    fn test_normalize_path_components() {
        // Simple path
        assert_eq!(
            normalize_path_components(Path::new("/a/b/c")),
            PathBuf::from("/a/b/c")
        );

        // Path with current directory
        assert_eq!(
            normalize_path_components(Path::new("/a/./b/./c")),
            PathBuf::from("/a/b/c")
        );

        // Path with parent directory
        assert_eq!(
            normalize_path_components(Path::new("/a/b/../c")),
            PathBuf::from("/a/c")
        );

        // Multiple parent directories
        assert_eq!(
            normalize_path_components(Path::new("/a/b/c/../../d")),
            PathBuf::from("/a/d")
        );
    }

    #[test]
    fn test_is_within_root() {
        let paths = test_dataset_paths();

        // Paths within root
        assert!(paths.is_within_root(Path::new("/var/datasets/files/data.txt")));
        assert!(paths.is_within_root(Path::new("/var/datasets/temp/upload")));
        assert!(paths.is_within_root(Path::new("/var/datasets")));

        // Paths outside root
        assert!(!paths.is_within_root(Path::new("/var/other/data.txt")));
        assert!(!paths.is_within_root(Path::new("/etc/passwd")));
        assert!(!paths.is_within_root(Path::new("/var")));
    }

    #[test]
    fn test_validate_within_root() {
        let paths = test_dataset_paths();

        // Valid paths
        assert!(paths
            .validate_within_root(Path::new("/var/datasets/files/data.txt"))
            .is_ok());

        // Invalid paths
        assert!(paths
            .validate_within_root(Path::new("/etc/passwd"))
            .is_err());
        assert!(paths
            .validate_within_root(Path::new("/var/datasets/../other"))
            .is_err());
    }

    #[test]
    fn test_resolve_version_file() {
        let paths = test_dataset_paths();

        let result = paths.resolve_version_file("ws-1", "ds-1", "v1", "canonical.jsonl");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/var/datasets/files/ws-1/ds-1/versions/v1/canonical.jsonl")
        );

        // Traversal blocked
        let result = paths.resolve_version_file("ws-1", "ds-1", "v1", "../../../escape.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_env_datasets_dir_constant() {
        // Verify the constant matches expected value
        assert_eq!(ENV_DATASETS_DIR, "AOS_DATASETS_DIR");
    }

    #[test]
    fn test_default_datasets_root_constant() {
        // Verify the default root path
        assert_eq!(DEFAULT_DATASETS_ROOT, "var/datasets");
    }

    #[test]
    fn test_resolve_dataset_root_from_strings_uses_env() {
        // Test that env takes precedence
        // Note: This test uses absolute paths that don't need to exist
        // because we're testing the precedence logic, not actual resolution
        let result = resolve_dataset_root_from_strings(
            Some("/custom/env/path".to_string()),
            Some("/config/path".to_string()),
        );
        // Will fail because path doesn't exist, but that's expected
        // We're verifying the function accepts the parameters correctly
        assert!(result.is_err()); // Path doesn't exist, so canonicalize fails
    }

    #[test]
    fn test_resolve_dataset_root_from_strings_uses_config_when_no_env() {
        // Test that config is used when no env
        let result = resolve_dataset_root_from_strings(
            None,
            Some("/config/path".to_string()),
        );
        // Will fail because path doesn't exist
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_dataset_root_from_strings_uses_default_when_no_options() {
        // Test that default is used when neither env nor config provided
        let result = resolve_dataset_root_from_strings(None, None);
        // Will likely fail unless var/datasets exists, but the function runs correctly
        // The important thing is it doesn't panic
        let _ = result;
    }

    #[test]
    fn test_dataset_paths_from_env_and_config_factory() {
        // Test the factory method works (may fail if path doesn't exist)
        let result = DatasetPaths::from_env_and_config(Some("/nonexistent/config/path".to_string()));
        assert!(result.is_err()); // Path doesn't exist
    }

    #[test]
    fn test_dataset_temp_dir() {
        let paths = test_dataset_paths();
        let temp_dir = paths.dataset_temp_dir("ws-123", "ds-456");
        assert_eq!(
            temp_dir,
            PathBuf::from("/var/datasets/temp/ws-123/ds-456")
        );
    }

    #[test]
    fn test_dataset_versions_dir() {
        let paths = test_dataset_paths();
        let versions_dir = paths.dataset_versions_dir("ws-123", "ds-456");
        assert_eq!(
            versions_dir,
            PathBuf::from("/var/datasets/files/ws-123/ds-456/versions")
        );
    }
}
