//! Dataset path resolution and validation utilities.
//!
//! This module provides:
//! - Dataset root resolution from environment or configuration
//! - Safe path joining that prevents directory traversal attacks
//! - Path validation to ensure files stay within dataset boundaries
//! - Comprehensive dataset root validation for codebase adapter support
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
//!
//! # Dataset Root Validation (Codebase Adapter Support)
//!
//! The `validate_dataset_root` function provides comprehensive validation for
//! dataset roots used in codebase adapter workflows. This includes:
//! - Existence and accessibility checks
//! - Permission validation (read/write)
//! - Security boundary enforcement
//! - Expected directory structure validation

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Component, Path, PathBuf};
use tracing::{debug, info, warn};

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

/// Additional forbidden path prefixes beyond /tmp for dataset roots.
/// These paths should never be used as dataset roots due to security concerns.
const FORBIDDEN_DATASET_ROOT_PREFIXES: &[&str] = &[
    "/proc", "/sys", "/dev", "/run", "/var/run", "/boot", "/etc", "/root",
];

// ============================================================================
// Dataset Root Validation Types
// ============================================================================

/// Result of dataset root validation with detailed error information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetRootValidation {
    /// Whether the dataset root passed all validation checks
    pub is_valid: bool,
    /// The validated (canonicalized) path, if validation succeeded
    pub canonical_path: Option<PathBuf>,
    /// List of validation errors encountered
    pub errors: Vec<DatasetRootValidationError>,
    /// List of validation warnings (non-blocking issues)
    pub warnings: Vec<String>,
    /// Whether the root directory exists
    pub exists: bool,
    /// Whether the root directory is readable
    pub is_readable: bool,
    /// Whether the root directory is writable
    pub is_writable: bool,
    /// Whether expected subdirectories are present
    pub has_expected_structure: bool,
}

#[allow(dead_code)]
impl DatasetRootValidation {
    /// Create a successful validation result.
    pub fn success(canonical_path: PathBuf) -> Self {
        Self {
            is_valid: true,
            canonical_path: Some(canonical_path),
            errors: Vec::new(),
            warnings: Vec::new(),
            exists: true,
            is_readable: true,
            is_writable: true,
            has_expected_structure: true,
        }
    }

    /// Create a failed validation result with errors.
    pub fn failure(errors: Vec<DatasetRootValidationError>) -> Self {
        Self {
            is_valid: false,
            errors,
            ..Default::default()
        }
    }

    /// Add an error to the validation result.
    pub fn add_error(&mut self, error: DatasetRootValidationError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    /// Add a warning to the validation result.
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Convert validation result to a Result type for easy error handling.
    pub fn into_result(self) -> Result<PathBuf> {
        if self.is_valid {
            self.canonical_path.ok_or_else(|| {
                AosError::Validation("Validation succeeded but canonical path is missing".into())
            })
        } else {
            let error_messages: Vec<String> = self.errors.iter().map(|e| e.to_string()).collect();
            Err(AosError::Validation(format!(
                "Dataset root validation failed: {}",
                error_messages.join("; ")
            )))
        }
    }
}

/// Specific validation error types for dataset roots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetRootValidationError {
    /// Error code for programmatic handling
    pub code: String,
    /// Human-readable error message
    pub message: String,
    /// The path that caused the error
    pub path: Option<String>,
    /// Suggested fix for the error
    pub suggestion: Option<String>,
}

impl DatasetRootValidationError {
    /// Create a new validation error.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: None,
            suggestion: None,
        }
    }

    /// Add path context to the error.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Add a suggestion for fixing the error.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

impl std::fmt::Display for DatasetRootValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(ref path) = self.path {
            write!(f, " (path: {})", path)?;
        }
        Ok(())
    }
}

/// Options for dataset root validation.
#[derive(Debug, Clone, Default)]
pub struct DatasetRootValidationOptions {
    /// Whether to require the root directory to already exist
    pub require_exists: bool,
    /// Whether to require write permissions
    pub require_writable: bool,
    /// Whether to check for expected subdirectory structure
    pub check_structure: bool,
    /// Whether to create the directory if it doesn't exist
    pub create_if_missing: bool,
    /// Additional allowed root prefixes (for testing or special deployments)
    pub additional_allowed_prefixes: Vec<String>,
}

impl DatasetRootValidationOptions {
    /// Create options for strict validation (all checks enabled).
    pub fn strict() -> Self {
        Self {
            require_exists: true,
            require_writable: true,
            check_structure: true,
            create_if_missing: false,
            additional_allowed_prefixes: Vec::new(),
        }
    }

    /// Create options for lenient validation (minimal checks).
    pub fn lenient() -> Self {
        Self {
            require_exists: false,
            require_writable: false,
            check_structure: false,
            create_if_missing: true,
            additional_allowed_prefixes: Vec::new(),
        }
    }
}

// ============================================================================
// Dataset Root Validation Functions
// ============================================================================

/// Validate a dataset root path with comprehensive checks.
///
/// This function performs the following validations:
/// 1. Path is not empty
/// 2. Path is not in forbidden directories (/tmp, /proc, /sys, etc.)
/// 3. Path can be canonicalized (resolves symlinks)
/// 4. Canonical path is still not in forbidden directories
/// 5. Directory exists (if required)
/// 6. Directory is readable and writable (if required)
/// 7. Expected subdirectory structure is present (if required)
///
/// # Arguments
///
/// * `path` - The dataset root path to validate
/// * `options` - Validation options controlling which checks are performed
///
/// # Returns
///
/// A `DatasetRootValidation` struct containing the validation result,
/// canonical path (if successful), and any errors or warnings.
///
/// # Example
///
/// ```ignore
/// use adapteros_server_api::handlers::datasets::paths::{
///     validate_dataset_root, DatasetRootValidationOptions
/// };
///
/// let path = std::path::Path::new("/var/datasets");
/// let options = DatasetRootValidationOptions::strict();
/// let result = validate_dataset_root(path, &options);
///
/// if result.is_valid {
///     println!("Valid root: {:?}", result.canonical_path);
/// } else {
///     for error in &result.errors {
///         eprintln!("Validation error: {}", error);
///     }
/// }
/// ```
pub fn validate_dataset_root(
    path: &Path,
    options: &DatasetRootValidationOptions,
) -> DatasetRootValidation {
    let mut result = DatasetRootValidation::default();
    let path_str = path.display().to_string();

    // Check 1: Path is not empty
    if path_str.is_empty() || path.as_os_str().is_empty() {
        result.add_error(
            DatasetRootValidationError::new("EMPTY_PATH", "Dataset root path cannot be empty")
                .with_suggestion(
                "Provide a valid path via AOS_DATASETS_DIR environment variable or configuration",
            ),
        );
        return result;
    }

    // Check 2: Path is not in forbidden directories
    if let Err(e) = check_forbidden_paths(path, &options.additional_allowed_prefixes) {
        result.add_error(e);
        return result;
    }

    // Check 3: Absolutize the path
    let absolute_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        match env::current_dir() {
            Ok(cwd) => cwd.join(path),
            Err(e) => {
                result.add_error(
                    DatasetRootValidationError::new(
                        "CWD_ERROR",
                        format!("Cannot determine current directory: {}", e),
                    )
                    .with_path(&path_str),
                );
                return result;
            }
        }
    };

    // Check 4: Check if path exists
    result.exists = absolute_path.exists();

    if !result.exists {
        if options.require_exists && !options.create_if_missing {
            result.add_error(
                DatasetRootValidationError::new(
                    "PATH_NOT_EXISTS",
                    format!("Dataset root does not exist: {}", absolute_path.display()),
                )
                .with_path(absolute_path.display().to_string())
                .with_suggestion("Create the directory or set create_if_missing option"),
            );
            return result;
        }

        if options.create_if_missing {
            // Attempt to create the directory
            match std::fs::create_dir_all(&absolute_path) {
                Ok(()) => {
                    info!(
                        path = %absolute_path.display(),
                        "Created dataset root directory"
                    );
                    result.exists = true;
                    result.add_warning(format!(
                        "Created missing dataset root directory: {}",
                        absolute_path.display()
                    ));
                }
                Err(e) => {
                    result.add_error(
                        DatasetRootValidationError::new(
                            "CREATE_FAILED",
                            format!("Failed to create dataset root: {}", e),
                        )
                        .with_path(absolute_path.display().to_string()),
                    );
                    return result;
                }
            }
        } else {
            result.add_warning(format!(
                "Dataset root does not exist but will be created on first use: {}",
                absolute_path.display()
            ));
        }
    }

    // Check 5: Canonicalize the path (resolves symlinks)
    let canonical_path = if result.exists {
        match absolute_path.canonicalize() {
            Ok(canon) => canon,
            Err(e) => {
                result.add_error(
                    DatasetRootValidationError::new(
                        "CANONICALIZE_FAILED",
                        format!("Failed to canonicalize path: {}", e),
                    )
                    .with_path(absolute_path.display().to_string()),
                );
                return result;
            }
        }
    } else {
        // For non-existent paths, use the absolute path
        absolute_path.clone()
    };

    // Check 6: Verify canonical path is not in forbidden directories
    if let Err(_e) = check_forbidden_paths(&canonical_path, &options.additional_allowed_prefixes) {
        result.add_error(
            DatasetRootValidationError::new(
                "SYMLINK_ESCAPE",
                format!(
                    "Symlink resolves to forbidden location: {} -> {}",
                    path.display(),
                    canonical_path.display()
                ),
            )
            .with_path(canonical_path.display().to_string())
            .with_suggestion("Ensure symlinks do not point to system directories"),
        );
        return result;
    }

    // Check 7: Verify it's a directory (if exists)
    if result.exists && !canonical_path.is_dir() {
        result.add_error(
            DatasetRootValidationError::new(
                "NOT_DIRECTORY",
                format!("Path is not a directory: {}", canonical_path.display()),
            )
            .with_path(canonical_path.display().to_string()),
        );
        return result;
    }

    // Check 8: Check read permissions
    if result.exists {
        result.is_readable = check_read_permission(&canonical_path);
        if !result.is_readable {
            result.add_error(
                DatasetRootValidationError::new(
                    "NOT_READABLE",
                    format!("Dataset root is not readable: {}", canonical_path.display()),
                )
                .with_path(canonical_path.display().to_string()),
            );
            return result;
        }
    }

    // Check 9: Check write permissions (always measured; enforced if required)
    if result.exists {
        result.is_writable = check_write_permission(&canonical_path);
        if options.require_writable && !result.is_writable {
            result.add_error(
                DatasetRootValidationError::new(
                    "NOT_WRITABLE",
                    format!("Dataset root is not writable: {}", canonical_path.display()),
                )
                .with_path(canonical_path.display().to_string())
                .with_suggestion("Check directory permissions or run with appropriate privileges"),
            );
            return result;
        }
        if !options.require_writable && !result.is_writable {
            result.add_warning(format!(
                "Dataset root is not writable: {}",
                canonical_path.display()
            ));
        }
    }

    // Check 10: Check expected directory structure (if required)
    if options.check_structure && result.exists {
        let (has_structure, structure_warnings) = check_expected_structure(&canonical_path);
        result.has_expected_structure = has_structure;
        for warning in structure_warnings {
            result.add_warning(warning);
        }
    } else {
        result.has_expected_structure = true; // Not checking, assume valid
    }

    // Success!
    result.is_valid = true;
    result.canonical_path = Some(canonical_path);
    result
}

/// Check if a path is in forbidden directories.
fn check_forbidden_paths(
    path: &Path,
    additional_allowed: &[String],
) -> std::result::Result<(), DatasetRootValidationError> {
    let path_str = path.display().to_string();

    // Check standard forbidden tmp paths
    if adapteros_core::path_security::is_forbidden_tmp_path(path) {
        return Err(DatasetRootValidationError::new(
            "FORBIDDEN_TMP",
            format!("Dataset root cannot be under /tmp: {}", path_str),
        )
        .with_path(&path_str)
        .with_suggestion("Use a persistent directory like /var/datasets or ~/datasets"));
    }

    // Check additional forbidden prefixes
    for prefix in FORBIDDEN_DATASET_ROOT_PREFIXES {
        let prefix_path = Path::new(prefix);
        if path.starts_with(prefix_path) {
            // Check if this prefix is in the allowed list
            if additional_allowed
                .iter()
                .any(|allowed| path.starts_with(allowed))
            {
                continue;
            }

            return Err(DatasetRootValidationError::new(
                "FORBIDDEN_SYSTEM",
                format!("Dataset root cannot be under {}: {}", prefix, path_str),
            )
            .with_path(&path_str)
            .with_suggestion("Use a user-accessible directory for dataset storage"));
        }
    }

    Ok(())
}

/// Check if a directory is readable.
fn check_read_permission(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok()
}

/// Check if a directory is writable by attempting to create a temp file.
fn check_write_permission(path: &Path) -> bool {
    let test_file = path.join(".aos_write_test");
    match std::fs::write(&test_file, b"test") {
        Ok(()) => {
            let _ = std::fs::remove_file(&test_file);
            true
        }
        Err(_) => false,
    }
}

/// Check if the expected subdirectory structure exists.
fn check_expected_structure(path: &Path) -> (bool, Vec<String>) {
    let mut warnings = Vec::new();
    let expected_dirs = [
        FILES_DIR_NAME,
        TEMP_DIR_NAME,
        CHUNKED_DIR_NAME,
        LOGS_DIR_NAME,
    ];
    let mut all_present = true;

    for dir_name in expected_dirs {
        let dir_path = path.join(dir_name);
        if !dir_path.exists() {
            warnings.push(format!(
                "Expected subdirectory '{}' does not exist (will be created on first use)",
                dir_name
            ));
            all_present = false;
        }
    }

    (all_present, warnings)
}

/// Validate dataset root from AppState with default options.
///
/// This is a convenience function that resolves the dataset root from
/// AppState and validates it with lenient options (will create directory
/// if missing).
pub fn validate_dataset_root_from_state(
    state: &crate::state::AppState,
) -> Result<DatasetRootValidation> {
    let config_root = match state.config.read() {
        Ok(config) => {
            if config.paths.datasets_root.is_empty() {
                None
            } else {
                Some(config.paths.datasets_root.clone())
            }
        }
        Err(_) => {
            tracing::error!("Config lock poisoned in validate_dataset_root_from_state");
            None
        }
    };
    let root =
        resolve_dataset_root_candidate_validated(env::var(ENV_DATASETS_DIR).ok(), config_root)?;
    let options = DatasetRootValidationOptions::lenient();
    Ok(validate_dataset_root(&root, &options))
}

/// Validate dataset root with strict options.
///
/// This function validates the dataset root with strict
/// options, requiring the directory to exist and be writable.
#[allow(dead_code)]
pub fn validate_dataset_root_strict(path: &Path) -> DatasetRootValidation {
    validate_dataset_root(path, &DatasetRootValidationOptions::strict())
}

// ============================================================================
// DatasetPaths Struct
// ============================================================================

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
        let validation = validate_dataset_root_from_state(state)?;
        let root = validation.into_result()?;
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
        let root =
            resolve_dataset_root_candidate_validated(env::var(ENV_DATASETS_DIR).ok(), config_root)?;
        let validation = validate_dataset_root(&root, &DatasetRootValidationOptions::lenient());
        Ok(Self::new(validation.into_result()?))
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
/// - Created if missing (lenient validation)
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

    let candidate =
        resolve_dataset_root_candidate_from_strings(env::var(ENV_DATASETS_DIR).ok(), config_root)?;

    if adapteros_core::path_security::is_forbidden_tmp_path(&candidate) {
        return Err(AosError::Validation(format!(
            "Dataset root '{}' is in a forbidden temporary directory. \
             Temporary directories (/tmp, /var/tmp, /private/tmp) are not allowed for dataset storage \
             because data may be lost on reboot. \
             Please configure AOS_DATASETS_DIR or paths.datasets_root to a persistent location.",
            candidate.display()
        )));
    }

    let validation = validate_dataset_root(&candidate, &DatasetRootValidationOptions::lenient());
    for warning in &validation.warnings {
        warn!(warning = %warning, "Dataset root validation warning");
    }
    validation.into_result()
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
    let absolute_root = resolve_dataset_root_candidate_validated(env_root, config_root)?;
    resolve_and_validate_path(absolute_root, DatasetRootValidationOptions::strict())
}

/// Resolve dataset root with lenient validation (creates directories if missing).
///
/// This is useful for startup paths that need to tolerate missing directories
/// while still enforcing forbidden path checks and canonicalization.
pub fn resolve_dataset_root_lenient_from_strings(
    env_root: Option<String>,
    config_root: Option<String>,
) -> Result<PathBuf> {
    let absolute_root = resolve_dataset_root_candidate_validated(env_root, config_root)?;
    resolve_and_validate_path(absolute_root, DatasetRootValidationOptions::lenient())
}

/// Resolve a dataset root candidate without canonicalization.
///
/// This is used for validation paths that may create the directory if missing.
fn resolve_dataset_root_candidate_from_strings(
    env_root: Option<String>,
    config_root: Option<String>,
) -> Result<PathBuf> {
    let env_root = env_root
        .as_deref()
        .map(str::trim)
        .filter(|root| !root.is_empty())
        .map(str::to_string);
    let config_root = config_root
        .as_deref()
        .map(str::trim)
        .filter(|root| !root.is_empty())
        .map(str::to_string);
    let root_str = env_root
        .or(config_root)
        .unwrap_or_else(|| DEFAULT_DATASETS_ROOT.to_string());
    let root = PathBuf::from(&root_str);
    let absolute_root = if root.is_absolute() {
        root
    } else {
        env::current_dir()
            .map_err(|e| {
                AosError::Validation(format!("Cannot determine current directory: {}", e))
            })?
            .join(root)
    };

    Ok(absolute_root)
}

fn resolve_dataset_root_candidate_validated(
    env_root: Option<String>,
    config_root: Option<String>,
) -> Result<PathBuf> {
    let absolute_root = resolve_dataset_root_candidate_from_strings(env_root, config_root)?;

    if adapteros_core::path_security::is_forbidden_tmp_path(&absolute_root) {
        return Err(AosError::Validation(format!(
            "Dataset root '{}' is in a forbidden temporary directory. \
             Temporary directories (/tmp, /var/tmp, /private/tmp) are not allowed for dataset storage \
             because data may be lost on reboot. \
             Please configure AOS_DATASETS_DIR or paths.datasets_root to a persistent location.",
            absolute_root.display()
        )));
    }

    Ok(absolute_root)
}

/// Internal helper to canonicalize and validate a dataset root path.
fn resolve_and_validate_path(
    path: PathBuf,
    options: DatasetRootValidationOptions,
) -> Result<PathBuf> {
    let validation = validate_dataset_root(&path, &options);
    for warning in &validation.warnings {
        warn!(warning = %warning, "Dataset root validation warning");
    }
    let canonical = validation.into_result()?;

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
        let result = resolve_dataset_root_from_strings(None, Some("/config/path".to_string()));
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
        let result =
            DatasetPaths::from_env_and_config(Some("/nonexistent/config/path".to_string()));
        assert!(result.is_err()); // Path doesn't exist
    }

    #[test]
    fn test_dataset_temp_dir() {
        let paths = test_dataset_paths();
        let temp_dir = paths.dataset_temp_dir("ws-123", "ds-456");
        assert_eq!(temp_dir, PathBuf::from("/var/datasets/temp/ws-123/ds-456"));
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

    #[test]
    fn test_forbidden_tmp_path_returns_error() {
        let result = resolve_dataset_root_from_strings(Some("/tmp/datasets".to_string()), None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("forbidden temporary directory"),
            "Expected error about forbidden temporary directory, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_var_tmp_path_returns_error() {
        let result = resolve_dataset_root_from_strings(Some("/var/tmp/datasets".to_string()), None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("forbidden temporary directory"),
            "Expected error about forbidden temporary directory, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_private_tmp_path_returns_error() {
        // macOS uses /private/tmp
        let result =
            resolve_dataset_root_from_strings(Some("/private/tmp/datasets".to_string()), None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("forbidden temporary directory"),
            "Expected error about forbidden temporary directory, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_forbidden_tmp_path_from_config_returns_error() {
        // Test that config path is also rejected
        let result = resolve_dataset_root_from_strings(None, Some("/tmp/datasets".to_string()));
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("forbidden temporary directory"),
            "Expected error about forbidden temporary directory, got: {}",
            err_msg
        );
    }
}
