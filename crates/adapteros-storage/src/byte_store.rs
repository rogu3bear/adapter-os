//! Lightweight byte-oriented storage abstraction with a filesystem backend.
//!
//! This provides a narrow interface that callers (datasets, training artifacts)
//! can depend on without committing to direct filesystem semantics. The
//! filesystem implementation preserves the current on-disk layout while making
//! it easy to swap for object storage later.
//!
//! # Dataset Storage Layout
//!
//! Files are organized according to the dataset storage conventions:
//!
//! ```text
//! {datasets_root}/
//! ├── files/                          # Finalized dataset files
//! │   └── {workspace_id}/
//! │       └── {dataset_id}/
//! │           ├── manifest.json
//! │           ├── data.jsonl
//! │           ├── samples/
//! │           │   └── {sample_file}
//! │           └── versions/
//! │               └── {version_id}/
//! │                   ├── manifest.json
//! │                   └── data.jsonl
//! │                   └── samples/
//! │                       └── {sample_file}
//! ├── canonical/                      # Content-addressable storage
//! │   └── {category}/
//! │       └── {hash_prefix}/
//! │           └── {content_hash}/
//! │               └── {file}
//! ├── temp/                           # Temporary uploads
//! │   └── {workspace_id}/
//! │       └── {dataset_id}/
//! └── chunked/                        # Chunked upload sessions
//!     └── {session_id}/
//! ```
//!
//! See [`DatasetStorageLayout`] for programmatic access to these paths.

use crate::ensure_free_space;
use adapteros_core::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::path::{Component, Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncReadExt;

/// Logical storage category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    DatasetFile,
    AdapterArtifact,
    /// Canonical content-addressable dataset storage
    CanonicalDataset,
}

/// Dataset category for canonical storage organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatasetCategory {
    /// Codebase-derived datasets (from code ingestion)
    Codebase,
    /// System metrics datasets
    Metrics,
    /// Synthetic/generated datasets
    Synthetic,
    /// User-uploaded datasets
    Upload,
    /// Custom category
    Custom(String),
}

impl DatasetCategory {
    /// Get the directory name for this category.
    pub fn as_dir_name(&self) -> &str {
        match self {
            DatasetCategory::Codebase => "codebase",
            DatasetCategory::Metrics => "metrics",
            DatasetCategory::Synthetic => "synthetic",
            DatasetCategory::Upload => "upload",
            DatasetCategory::Custom(name) => name.as_str(),
        }
    }

    /// Parse a category from a directory name.
    pub fn from_dir_name(name: &str) -> Option<Self> {
        match name {
            "codebase" => Some(Self::Codebase),
            "metrics" => Some(Self::Metrics),
            "synthetic" => Some(Self::Synthetic),
            "upload" => Some(Self::Upload),
            _ => Some(Self::Custom(name.to_string())),
        }
    }
}

// ============================================================================
// Dataset Storage Layout
// ============================================================================

/// Directory names for the dataset storage layout.
pub mod layout_dirs {
    /// Directory for finalized dataset files.
    pub const FILES: &str = "files";
    /// Directory for temporary uploads.
    pub const TEMP: &str = "temp";
    /// Directory for chunked upload sessions.
    pub const CHUNKED: &str = "chunked";
    /// Directory for operation logs.
    pub const LOGS: &str = "logs";
    /// Directory for dataset versions within a dataset.
    pub const VERSIONS: &str = "versions";
    /// Directory for sample artifacts within a dataset.
    pub const SAMPLES: &str = "samples";
    /// Directory for content-addressable canonical storage.
    pub const CANONICAL: &str = "canonical";
    /// Subdirectory for tenant isolation in canonical storage.
    pub const TENANTS: &str = "tenants";
}

/// Default filename for canonical dataset JSONL content.
pub const CANONICAL_DATA_FILENAME: &str = "canonical.jsonl";

fn validate_layout_segment(segment: &str, label: &str) -> Result<()> {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return Err(adapteros_core::AosError::Validation(format!(
            "{} cannot be empty",
            label
        )));
    }
    if segment.contains('/') || segment.contains('\\') {
        return Err(adapteros_core::AosError::Validation(format!(
            "{} must not contain path separators: {}",
            label, segment
        )));
    }
    let mut components = Path::new(segment).components();
    match components.next() {
        Some(Component::Normal(_)) => {}
        _ => {
            return Err(adapteros_core::AosError::Validation(format!(
                "{} must be a simple path segment: {}",
                label, segment
            )));
        }
    }
    if components.next().is_some() {
        return Err(adapteros_core::AosError::Validation(format!(
            "{} must be a single path segment: {}",
            label, segment
        )));
    }
    Ok(())
}

/// Dataset storage layout manager.
///
/// Provides path resolution for the standard dataset storage layout, supporting:
/// - Workspace-scoped dataset files
/// - Versioned dataset storage
/// - Content-addressable canonical storage
/// - Temporary and chunked upload staging
///
/// # Layout Structure
///
/// ```text
/// {root}/
/// ├── files/{workspace_id}/{dataset_id}/[versions/{version_id}/]{file}
/// ├── files/{workspace_id}/{dataset_id}/samples/{sample_file}
/// ├── files/{workspace_id}/{dataset_id}/versions/{version_id}/samples/{sample_file}
/// ├── canonical/{category}/{hash_prefix}/{hash}/[tenants/{tenant}/]{file}
/// ├── temp/{workspace_id}/{dataset_id}/{file}
/// └── chunked/{session_id}/{chunk_index}
/// ```
///
/// # Example
///
/// ```ignore
/// use adapteros_storage::byte_store::DatasetStorageLayout;
///
/// let layout = DatasetStorageLayout::new("/var/datasets");
///
/// // Get path for a dataset file
/// let path = layout.dataset_file_path("ws-123", "ds-456", "train.jsonl");
/// // -> /var/datasets/files/ws-123/ds-456/train.jsonl
///
/// // Get path for a versioned file
/// let path = layout.version_file_path("ws-123", "ds-456", "v1", "data.jsonl");
/// // -> /var/datasets/files/ws-123/ds-456/versions/v1/data.jsonl
/// ```
#[derive(Debug, Clone)]
pub struct DatasetStorageLayout {
    /// Root directory for all dataset storage.
    root: PathBuf,
}

impl DatasetStorageLayout {
    /// Create a new layout manager with the given root directory.
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Get the root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    // =========================================================================
    // Files directory (finalized datasets)
    // =========================================================================

    /// Get the files directory for finalized datasets.
    pub fn files_dir(&self) -> PathBuf {
        self.root.join(layout_dirs::FILES)
    }

    /// Get the directory for a workspace's datasets.
    pub fn workspace_dir(&self, workspace_id: &str) -> PathBuf {
        self.files_dir().join(workspace_id)
    }

    /// Get the directory for a specific dataset.
    pub fn dataset_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.workspace_dir(workspace_id).join(dataset_id)
    }

    /// Get the path for a file within a dataset.
    ///
    /// # Security
    /// This function validates that `file_name` is a simple filename without
    /// path traversal components (e.g., "..", "/", or multiple path segments).
    ///
    /// # Errors
    /// Returns an error if `file_name` contains path traversal attempts.
    pub fn dataset_file_path(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        // SECURITY: Validate file_name is a simple filename without path traversal
        Self::validate_simple_filename(file_name)?;
        Ok(self.dataset_dir(workspace_id, dataset_id).join(file_name))
    }

    /// Validate that a filename is a simple filename without path components.
    ///
    /// # Security
    /// Prevents path traversal attacks by ensuring the filename:
    /// - Has exactly one Normal component
    /// - Contains no ".." or parent directory references
    /// - Contains no path separators (/ or \)
    fn validate_simple_filename(file_name: &str) -> Result<()> {
        use std::path::Component;

        let path = Path::new(file_name);
        let components: Vec<_> = path.components().collect();

        // Must have exactly one component and it must be a Normal component
        if components.len() != 1 {
            return Err(adapteros_core::AosError::Validation(format!(
                "Invalid filename: path has {} components, expected 1 (possible path traversal attempt)",
                components.len()
            )));
        }

        match components.first() {
            Some(Component::Normal(_)) => Ok(()),
            Some(Component::ParentDir) => Err(adapteros_core::AosError::Validation(
                "Invalid filename: contains parent directory reference '..' (path traversal attempt)"
                    .to_string(),
            )),
            Some(Component::CurDir) => Err(adapteros_core::AosError::Validation(
                "Invalid filename: contains current directory reference '.' (path traversal attempt)"
                    .to_string(),
            )),
            Some(Component::RootDir) => Err(adapteros_core::AosError::Validation(
                "Invalid filename: contains root directory reference (path traversal attempt)"
                    .to_string(),
            )),
            Some(Component::Prefix(_)) => Err(adapteros_core::AosError::Validation(
                "Invalid filename: contains path prefix (path traversal attempt)".to_string(),
            )),
            None => Err(adapteros_core::AosError::Validation(
                "Invalid filename: empty filename".to_string(),
            )),
        }
    }

    /// Get the versions directory for a dataset.
    pub fn versions_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.dataset_dir(workspace_id, dataset_id)
            .join(layout_dirs::VERSIONS)
    }

    /// Get the directory for a specific version.
    pub fn version_dir(&self, workspace_id: &str, dataset_id: &str, version_id: &str) -> PathBuf {
        self.versions_dir(workspace_id, dataset_id).join(version_id)
    }

    /// Get the path for a file within a dataset version.
    ///
    /// # Security
    /// This function validates that `file_name` is a simple filename without
    /// path traversal components.
    ///
    /// # Errors
    /// Returns an error if `file_name` contains path traversal attempts.
    pub fn version_file_path(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        // SECURITY: Validate file_name is a simple filename without path traversal
        Self::validate_simple_filename(file_name)?;
        Ok(self
            .version_dir(workspace_id, dataset_id, version_id)
            .join(file_name))
    }

    /// Get the samples directory for a dataset.
    pub fn samples_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.dataset_dir(workspace_id, dataset_id)
            .join(layout_dirs::SAMPLES)
    }

    /// Get the path for a sample artifact within a dataset.
    ///
    /// # Security
    /// This function validates that `file_name` is a simple filename without
    /// path traversal components.
    ///
    /// # Errors
    /// Returns an error if `file_name` contains path traversal attempts.
    pub fn sample_file_path(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        // SECURITY: Validate file_name is a simple filename without path traversal
        Self::validate_simple_filename(file_name)?;
        Ok(self.samples_dir(workspace_id, dataset_id).join(file_name))
    }

    /// Get the samples directory for a dataset version.
    pub fn version_samples_dir(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
    ) -> PathBuf {
        self.version_dir(workspace_id, dataset_id, version_id)
            .join(layout_dirs::SAMPLES)
    }

    /// Get the path for a sample artifact within a dataset version.
    ///
    /// # Security
    /// This function validates that `file_name` is a simple filename without
    /// path traversal components.
    ///
    /// # Errors
    /// Returns an error if `file_name` contains path traversal attempts.
    pub fn version_sample_file_path(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        // SECURITY: Validate file_name is a simple filename without path traversal
        Self::validate_simple_filename(file_name)?;
        Ok(self
            .version_samples_dir(workspace_id, dataset_id, version_id)
            .join(file_name))
    }

    // =========================================================================
    // Canonical directory (content-addressable storage)
    // =========================================================================

    /// Get the canonical storage directory.
    pub fn canonical_dir(&self) -> PathBuf {
        self.root.join(layout_dirs::CANONICAL)
    }

    /// Get the directory for a specific category in canonical storage.
    pub fn canonical_category_dir(&self, category: &DatasetCategory) -> PathBuf {
        self.canonical_dir().join(category.as_dir_name())
    }

    /// Get the path for a canonical file.
    ///
    /// Uses hash prefix sharding for filesystem performance.
    pub fn canonical_file_path(
        &self,
        category: &DatasetCategory,
        content_hash: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        self.canonical_file_path_with_tenant(category, content_hash, None, file_name)
    }

    /// Get the path for a canonical file with tenant isolation.
    pub fn canonical_file_path_with_tenant(
        &self,
        category: &DatasetCategory,
        content_hash: &str,
        tenant_id: Option<&str>,
        file_name: &str,
    ) -> Result<PathBuf> {
        let base = self.canonical_dir_path_with_tenant(category, content_hash, tenant_id)?;
        Ok(base.join(file_name))
    }

    /// Get the canonical directory path with optional tenant isolation.
    pub fn canonical_dir_path_with_tenant(
        &self,
        category: &DatasetCategory,
        content_hash: &str,
        tenant_id: Option<&str>,
    ) -> Result<PathBuf> {
        validate_layout_segment(category.as_dir_name(), "category")?;
        validate_layout_segment(content_hash, "content_hash")?;
        if let Some(tenant) = tenant_id {
            validate_layout_segment(tenant, "tenant_id")?;
        }

        // Validate hash length for prefix extraction
        if content_hash.len() < 4 {
            return Err(adapteros_core::AosError::Validation(format!(
                "Content hash too short for canonical storage: {} (minimum 4 characters)",
                content_hash
            )));
        }

        // Extract hash prefix for directory sharding (first 2 chars)
        let hash_prefix = &content_hash[..2];

        let mut path = self
            .canonical_category_dir(category)
            .join(hash_prefix)
            .join(content_hash);

        // Add tenant isolation if specified
        if let Some(tenant) = tenant_id {
            path = path.join(layout_dirs::TENANTS).join(tenant);
        }

        Ok(path)
    }

    // =========================================================================
    // Temp directory (uploads in progress)
    // =========================================================================

    /// Get the temp directory for uploads in progress.
    pub fn temp_dir(&self) -> PathBuf {
        self.root.join(layout_dirs::TEMP)
    }

    /// Get the temp directory for a specific dataset upload.
    pub fn dataset_temp_dir(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.temp_dir().join(workspace_id).join(dataset_id)
    }

    /// Get the path for a temp file during upload.
    ///
    /// # Security
    /// This function validates that `file_name` is a simple filename without
    /// path traversal components.
    ///
    /// # Errors
    /// Returns an error if `file_name` contains path traversal attempts.
    pub fn temp_file_path(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        file_name: &str,
    ) -> Result<PathBuf> {
        // SECURITY: Validate file_name is a simple filename without path traversal
        Self::validate_simple_filename(file_name)?;
        Ok(self
            .dataset_temp_dir(workspace_id, dataset_id)
            .join(file_name))
    }

    // =========================================================================
    // Chunked directory (chunked upload sessions)
    // =========================================================================

    /// Get the chunked uploads directory.
    pub fn chunked_dir(&self) -> PathBuf {
        self.root.join(layout_dirs::CHUNKED)
    }

    /// Get the directory for a chunked upload session.
    pub fn chunked_session_dir(&self, session_id: &str) -> PathBuf {
        self.chunked_dir().join(session_id)
    }

    /// Get the path for a chunk file.
    pub fn chunk_file_path(&self, session_id: &str, chunk_index: u32) -> PathBuf {
        self.chunked_session_dir(session_id)
            .join(format!("{:08}", chunk_index))
    }

    // =========================================================================
    // Logs directory
    // =========================================================================

    /// Get the logs directory.
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join(layout_dirs::LOGS)
    }

    // =========================================================================
    // Convenience methods for common file types
    // =========================================================================

    /// Get the manifest file path for a dataset.
    pub fn manifest_path(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.dataset_file_path(workspace_id, dataset_id, "manifest.json")
            .expect("manifest filename should be a simple path segment")
    }

    /// Get the manifest file path for a dataset version.
    pub fn version_manifest_path(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
    ) -> PathBuf {
        self.version_file_path(workspace_id, dataset_id, version_id, "manifest.json")
            .expect("manifest filename should be a simple path segment")
    }

    /// Get the canonical data file path (e.g., canonical.jsonl).
    pub fn canonical_data_path(&self, workspace_id: &str, dataset_id: &str) -> PathBuf {
        self.dataset_file_path(workspace_id, dataset_id, CANONICAL_DATA_FILENAME)
            .expect("canonical filename should be a simple path segment")
    }

    /// Get the canonical data file path for a version.
    pub fn version_canonical_data_path(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
    ) -> PathBuf {
        self.version_file_path(
            workspace_id,
            dataset_id,
            version_id,
            CANONICAL_DATA_FILENAME,
        )
        .expect("canonical filename should be a simple path segment")
    }

    // =========================================================================
    // StorageKey integration
    // =========================================================================

    /// Build a StorageKey for a dataset file.
    pub fn storage_key_for_file(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        file_name: &str,
    ) -> StorageKey {
        StorageKey::dataset_file(Some(workspace_id.to_string()), dataset_id, None, file_name)
    }

    /// Build a StorageKey for a versioned dataset file.
    pub fn storage_key_for_version_file(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
        file_name: &str,
    ) -> StorageKey {
        StorageKey::dataset_file(
            Some(workspace_id.to_string()),
            dataset_id,
            Some(version_id.to_string()),
            file_name,
        )
    }

    /// Build a StorageKey for a dataset sample artifact.
    pub fn storage_key_for_sample_file(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        file_name: &str,
    ) -> StorageKey {
        StorageKey::dataset_file(
            Some(workspace_id.to_string()),
            dataset_id,
            None,
            format!("{}/{}", layout_dirs::SAMPLES, file_name),
        )
    }

    /// Build a StorageKey for a versioned dataset sample artifact.
    pub fn storage_key_for_version_sample_file(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: &str,
        file_name: &str,
    ) -> StorageKey {
        StorageKey::dataset_file(
            Some(workspace_id.to_string()),
            dataset_id,
            Some(version_id.to_string()),
            format!("{}/{}", layout_dirs::SAMPLES, file_name),
        )
    }

    /// Build a StorageKey for canonical content-addressable storage.
    pub fn storage_key_for_canonical(
        &self,
        content_hash: &str,
        category: DatasetCategory,
        file_name: &str,
    ) -> StorageKey {
        StorageKey::canonical_dataset(content_hash, category, None, file_name)
    }

    /// Build a StorageKey for the canonical dataset JSONL file.
    pub fn storage_key_for_canonical_data(
        &self,
        content_hash: &str,
        category: DatasetCategory,
    ) -> StorageKey {
        self.storage_key_for_canonical(content_hash, category, CANONICAL_DATA_FILENAME)
    }

    /// Build a StorageKey for canonical storage with tenant isolation.
    pub fn storage_key_for_canonical_with_tenant(
        &self,
        tenant_id: &str,
        content_hash: &str,
        category: DatasetCategory,
        file_name: &str,
    ) -> StorageKey {
        StorageKey::canonical_dataset_with_tenant(
            tenant_id,
            content_hash,
            category,
            None,
            file_name,
        )
    }
}

/// Canonical key for a stored object.
#[derive(Debug, Clone)]
pub struct StorageKey {
    pub tenant_id: Option<String>,
    pub object_id: String,
    pub version_id: Option<String>,
    pub file_name: String,
    pub kind: StorageKind,
    /// Content hash for canonical storage (BLAKE3 hex string)
    pub content_hash: Option<String>,
    /// Dataset category for canonical storage
    pub category: Option<DatasetCategory>,
}

impl StorageKey {
    /// Create a storage key for a standard dataset file.
    pub fn dataset_file(
        tenant_id: Option<String>,
        dataset_id: impl Into<String>,
        version_id: Option<String>,
        file_name: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            object_id: dataset_id.into(),
            version_id,
            file_name: file_name.into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        }
    }

    /// Create a storage key for an adapter artifact.
    pub fn adapter_artifact(
        tenant_id: Option<String>,
        adapter_id: impl Into<String>,
        version_id: Option<String>,
        file_name: impl Into<String>,
    ) -> Self {
        Self {
            tenant_id,
            object_id: adapter_id.into(),
            version_id,
            file_name: file_name.into(),
            kind: StorageKind::AdapterArtifact,
            content_hash: None,
            category: None,
        }
    }

    /// Create a canonical content-addressable storage key for a dataset file.
    ///
    /// Path scheme: `{datasets_root}/canonical/{category}/{hash_prefix}/{content_hash}/{version?}/{file_name}`
    pub fn canonical_dataset(
        content_hash: impl Into<String>,
        category: DatasetCategory,
        version_id: Option<String>,
        file_name: impl Into<String>,
    ) -> Self {
        let hash = content_hash.into();
        Self {
            tenant_id: None,
            object_id: hash.clone(),
            version_id,
            file_name: file_name.into(),
            kind: StorageKind::CanonicalDataset,
            content_hash: Some(hash),
            category: Some(category),
        }
    }

    /// Create a canonical dataset key with tenant isolation.
    pub fn canonical_dataset_with_tenant(
        tenant_id: impl Into<String>,
        content_hash: impl Into<String>,
        category: DatasetCategory,
        version_id: Option<String>,
        file_name: impl Into<String>,
    ) -> Self {
        let hash = content_hash.into();
        Self {
            tenant_id: Some(tenant_id.into()),
            object_id: hash.clone(),
            version_id,
            file_name: file_name.into(),
            kind: StorageKind::CanonicalDataset,
            content_hash: Some(hash),
            category: Some(category),
        }
    }
}

/// Location + size metadata for a stored object.
#[derive(Debug, Clone)]
pub struct StorageLocation {
    pub path: PathBuf,
    pub size_bytes: u64,
}

// ============================================================================
// Deterministic Artifact Path Computation
// ============================================================================

/// Computes a deterministic storage path based on content hash.
///
/// This ensures identical content always maps to the same path, enabling
/// content-addressable storage for artifacts. The path structure uses hash
/// prefixes for efficient filesystem sharding.
///
/// # Path Structure
///
/// ```text
/// Dataset files:
/// `{root}/canonical/general/{hash_prefix}/{content_hash}/canonical.jsonl`
///
/// Adapter artifacts:
/// `{root}/artifacts/{prefix_2}/{prefix_6}/canonical.aos`
/// ```
///
/// Where:
/// - `hash_prefix`: First 2 characters of the hash (256 possible directories)
/// - `prefix_6`: Characters 2-8 of the hash (16M possible subdirectories, adapters only)
/// - Dataset category defaults to `general`
/// - Extension: `.jsonl` for datasets, `.aos` for adapters
///
/// # Determinism Guarantees
///
/// - Same content hash always produces the same path
/// - Path is independent of upload time, user, or session
/// - Suitable for deduplication and caching scenarios
///
/// # Arguments
///
/// * `root` - Base storage directory
/// * `content_hash` - 64-character hex BLAKE3 hash of the content
/// * `kind` - Type of artifact (dataset or adapter)
///
/// # Panics
///
/// Panics if `content_hash` is shorter than 8 characters.
pub fn compute_deterministic_path(root: &Path, content_hash: &str, kind: StorageKind) -> PathBuf {
    // Use hash prefix for sharding: first 2 chars / next 6 chars
    let hash_prefix = &content_hash[0..2];
    let hash_suffix = &content_hash[2..8];

    match kind {
        StorageKind::DatasetFile | StorageKind::CanonicalDataset => {
            let layout = DatasetStorageLayout::new(root);
            layout
                .canonical_file_path(
                    &DatasetCategory::Custom("general".to_string()),
                    content_hash,
                    CANONICAL_DATA_FILENAME,
                )
                .expect("canonical dataset path should be valid")
        }
        StorageKind::AdapterArtifact => root
            .join("artifacts")
            .join(hash_prefix)
            .join(hash_suffix)
            .join("canonical.aos"),
    }
}

#[async_trait]
pub trait ByteStorage: Send + Sync {
    /// Resolve the absolute path for a logical key.
    fn path_for(&self, key: &StorageKey) -> Result<PathBuf>;

    /// Store bytes at the resolved location, overwriting if present.
    async fn store_bytes(&self, key: &StorageKey, data: &[u8]) -> Result<StorageLocation>;

    /// Open and read bytes for a key.
    async fn open_bytes(&self, key: &StorageKey) -> Result<Bytes>;

    /// Delete bytes for a key (no error if missing).
    async fn delete(&self, key: &StorageKey) -> Result<()>;

    /// Stat a key, returning size and path.
    async fn stat(&self, key: &StorageKey) -> Result<StorageLocation>;
}

/// Filesystem-backed implementation that mirrors current layout conventions.
///
/// Uses [`DatasetStorageLayout`] internally for consistent path resolution
/// across the codebase.
#[derive(Debug, Clone)]
pub struct FsByteStorage {
    datasets_root: PathBuf,
    adapters_root: PathBuf,
    /// Internal layout manager for dataset paths.
    layout: DatasetStorageLayout,
}

impl FsByteStorage {
    /// Create a new byte storage instance.
    ///
    /// # Arguments
    ///
    /// * `datasets_root` - Root directory for dataset storage
    /// * `adapters_root` - Root directory for adapter artifact storage
    pub fn new(datasets_root: PathBuf, adapters_root: PathBuf) -> Self {
        let layout = DatasetStorageLayout::new(&datasets_root);
        Self {
            datasets_root,
            adapters_root,
            layout,
        }
    }

    /// Get a reference to the dataset storage layout manager.
    ///
    /// This provides access to the full layout API for advanced path resolution.
    pub fn layout(&self) -> &DatasetStorageLayout {
        &self.layout
    }

    /// Get the datasets root directory.
    pub fn datasets_root(&self) -> &Path {
        &self.datasets_root
    }

    /// Get the adapters root directory.
    pub fn adapters_root(&self) -> &Path {
        &self.adapters_root
    }

    /// Compute a deterministic path for a dataset artifact based on content hash.
    ///
    /// This method provides content-addressable storage paths that are:
    /// - Deterministic: same hash always produces the same path
    /// - Independent of upload context (time, user, session)
    /// - Suitable for deduplication and caching
    ///
    /// # Path Structure
    ///
    /// ```text
    /// {datasets_root}/canonical/general/{hash_prefix}/{content_hash}/canonical.jsonl
    /// ```
    ///
    /// # Arguments
    ///
    /// * `content_hash` - 64-character hex BLAKE3 hash of the content
    ///
    /// # Panics
    ///
    /// Panics if `content_hash` is shorter than 8 characters.
    pub fn deterministic_dataset_path(&self, content_hash: &str) -> PathBuf {
        compute_deterministic_path(&self.datasets_root, content_hash, StorageKind::DatasetFile)
    }

    /// Store canonical dataset bytes using the default JSONL filename.
    pub async fn store_canonical_dataset_bytes(
        &self,
        content_hash: &str,
        category: DatasetCategory,
        data: &[u8],
    ) -> Result<StorageLocation> {
        let key = self
            .layout
            .storage_key_for_canonical_data(content_hash, category);
        self.store_bytes(&key, data).await
    }

    /// Store sample artifact bytes using the dataset file layout.
    pub async fn store_sample_artifact_bytes(
        &self,
        workspace_id: &str,
        dataset_id: &str,
        version_id: Option<&str>,
        file_name: &str,
        data: &[u8],
    ) -> Result<StorageLocation> {
        let key = match version_id {
            Some(version_id) => self.layout.storage_key_for_version_sample_file(
                workspace_id,
                dataset_id,
                version_id,
                file_name,
            ),
            None => self
                .layout
                .storage_key_for_sample_file(workspace_id, dataset_id, file_name),
        };
        self.store_bytes(&key, data).await
    }

    fn validate_dataset_file_name(&self, file_name: &str) -> Result<()> {
        let trimmed = file_name.trim();
        if trimmed.is_empty() {
            return Err(adapteros_core::AosError::Validation(
                "Dataset file name cannot be empty".to_string(),
            ));
        }

        let path = Path::new(trimmed);
        if path.is_absolute() {
            return Err(adapteros_core::AosError::Validation(format!(
                "Dataset file name must be a relative path: {}",
                file_name
            )));
        }

        for component in path.components() {
            match component {
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(adapteros_core::AosError::Validation(format!(
                        "Dataset file name must stay within dataset root: {}",
                        file_name
                    )));
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn validate_path_segment(&self, segment: &str, label: &str) -> Result<()> {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            return Err(adapteros_core::AosError::Validation(format!(
                "{} cannot be empty",
                label
            )));
        }

        if segment.contains('/') || segment.contains('\\') {
            return Err(adapteros_core::AosError::Validation(format!(
                "{} must not contain path separators: {}",
                label, segment
            )));
        }

        let mut components = Path::new(segment).components();
        match components.next() {
            Some(Component::Normal(_)) => {}
            _ => {
                return Err(adapteros_core::AosError::Validation(format!(
                    "{} must be a simple path segment: {}",
                    label, segment
                )));
            }
        }

        if components.next().is_some() {
            return Err(adapteros_core::AosError::Validation(format!(
                "{} must be a single path segment: {}",
                label, segment
            )));
        }

        Ok(())
    }

    fn validate_optional_segment(&self, segment: Option<&str>, label: &str) -> Result<()> {
        if let Some(value) = segment {
            self.validate_path_segment(value, label)?;
        }
        Ok(())
    }

    fn dataset_path(&self, key: &StorageKey) -> PathBuf {
        // Use the layout manager for consistent path resolution
        // tenant_id maps to workspace_id for workspace scoping
        match (&key.tenant_id, &key.version_id) {
            (Some(workspace_id), Some(version_id)) => self
                .layout
                .version_dir(workspace_id, &key.object_id, version_id)
                .join(&key.file_name),
            (Some(workspace_id), None) => self
                .layout
                .dataset_dir(workspace_id, &key.object_id)
                .join(&key.file_name),
            (None, Some(version_id)) => {
                // Legacy fallback: no workspace, but has version
                self.layout
                    .files_dir()
                    .join(&key.object_id)
                    .join(layout_dirs::VERSIONS)
                    .join(version_id)
                    .join(&key.file_name)
            }
            (None, None) => {
                // Legacy fallback: no workspace scoping
                self.layout
                    .files_dir()
                    .join(&key.object_id)
                    .join(&key.file_name)
            }
        }
    }

    fn adapter_path(&self, key: &StorageKey) -> PathBuf {
        // Preserve adapter repo layout: {adapters_root}/{tenant}/{adapter}/{file}
        // If no tenant is provided, fall back to top-level.
        let mut base = if let Some(tenant) = &key.tenant_id {
            self.adapters_root.join(tenant)
        } else {
            self.adapters_root.clone()
        };
        base = base.join(&key.object_id);
        if let Some(ver) = &key.version_id {
            base = base.join(ver);
        }
        base.join(&key.file_name)
    }

    /// Build canonical content-addressable path for a dataset.
    ///
    /// Path scheme: `{datasets_root}/canonical/{category}/{hash_prefix}/{content_hash}/{version?}/{tenant?}/{file_name}`
    fn canonical_dataset_path(&self, key: &StorageKey) -> Result<PathBuf> {
        let hash = key.content_hash.as_ref().ok_or_else(|| {
            adapteros_core::AosError::Validation(
                "Canonical dataset key requires content_hash".to_string(),
            )
        })?;

        let category = key.category.as_ref().ok_or_else(|| {
            adapteros_core::AosError::Validation(
                "Canonical dataset key requires category".to_string(),
            )
        })?;

        self.validate_path_segment(hash, "content_hash")?;
        self.validate_path_segment(category.as_dir_name(), "category")?;
        self.validate_optional_segment(key.tenant_id.as_deref(), "tenant_id")?;
        self.validate_optional_segment(key.version_id.as_deref(), "version_id")?;

        // Use layout manager for path construction with optional version handling
        let base_path = self.layout.canonical_file_path_with_tenant(
            category,
            hash,
            key.tenant_id.as_deref(),
            &key.file_name,
        )?;

        // If version is specified, insert it before the file name
        if let Some(ver) = &key.version_id {
            let parent = base_path.parent().unwrap_or(Path::new("."));
            Ok(parent.join(ver).join(&key.file_name))
        } else {
            Ok(base_path)
        }
    }

    async fn ensure_parent(path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                adapteros_core::AosError::Io(format!(
                    "Failed to create parent dir {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }
}

#[async_trait]
impl ByteStorage for FsByteStorage {
    fn path_for(&self, key: &StorageKey) -> Result<PathBuf> {
        let path = match key.kind {
            StorageKind::DatasetFile => {
                self.validate_dataset_file_name(&key.file_name)?;
                self.validate_path_segment(&key.object_id, "dataset_id")?;
                self.validate_optional_segment(key.tenant_id.as_deref(), "workspace_id")?;
                self.validate_optional_segment(key.version_id.as_deref(), "version_id")?;
                self.dataset_path(key)
            }
            StorageKind::AdapterArtifact => {
                self.validate_path_segment(&key.object_id, "adapter_id")?;
                self.validate_optional_segment(key.tenant_id.as_deref(), "tenant_id")?;
                self.validate_optional_segment(key.version_id.as_deref(), "version_id")?;
                self.adapter_path(key)
            }
            StorageKind::CanonicalDataset => {
                self.validate_dataset_file_name(&key.file_name)?;
                return self.canonical_dataset_path(key);
            }
        };
        let abs = if path.is_absolute() {
            path
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("/"))
                .join(path)
        };
        Ok(abs)
    }

    async fn store_bytes(&self, key: &StorageKey, data: &[u8]) -> Result<StorageLocation> {
        let path = self.path_for(key)?;
        Self::ensure_parent(&path).await?;
        let parent = path.parent().unwrap_or(Path::new("."));
        ensure_free_space(parent, "byte store write").map_err(|e| {
            adapteros_core::AosError::Io(format!(
                "Failed to ensure free space for {}: {}",
                path.display(),
                e
            ))
        })?;
        fs::write(&path, data).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to write {}: {}", path.display(), e))
        })?;
        let size_bytes = data.len() as u64;
        Ok(StorageLocation { path, size_bytes })
    }

    async fn open_bytes(&self, key: &StorageKey) -> Result<Bytes> {
        let path = self.path_for(key)?;
        let mut file = fs::File::open(&path).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to open {}: {}", path.display(), e))
        })?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;
        Ok(Bytes::from(buf))
    }

    async fn delete(&self, key: &StorageKey) -> Result<()> {
        let path = self.path_for(key)?;
        if fs::remove_file(&path).await.is_err() {
            // Best-effort; missing is not fatal.
        }
        Ok(())
    }

    async fn stat(&self, key: &StorageKey) -> Result<StorageLocation> {
        let path = self.path_for(key)?;
        let meta = fs::metadata(&path).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to stat {}: {}", path.display(), e))
        })?;
        Ok(StorageLocation {
            path,
            size_bytes: meta.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // =========================================================================
    // DatasetStorageLayout tests
    // =========================================================================

    #[test]
    fn layout_files_directory_structure() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        assert_eq!(layout.root(), Path::new("/var/datasets"));
        assert_eq!(layout.files_dir(), PathBuf::from("/var/datasets/files"));
        assert_eq!(
            layout.workspace_dir("ws-123"),
            PathBuf::from("/var/datasets/files/ws-123")
        );
        assert_eq!(
            layout.dataset_dir("ws-123", "ds-456"),
            PathBuf::from("/var/datasets/files/ws-123/ds-456")
        );
    }

    #[test]
    fn layout_dataset_file_path() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        let path = layout
            .dataset_file_path("ws-123", "ds-456", "train.jsonl")
            .expect("valid dataset filename");
        assert_eq!(
            path,
            PathBuf::from("/var/datasets/files/ws-123/ds-456/train.jsonl")
        );
    }

    #[test]
    fn layout_versioned_file_path() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        let path = layout
            .version_file_path("ws-123", "ds-456", "v1", "data.jsonl")
            .expect("valid dataset filename");
        assert_eq!(
            path,
            PathBuf::from("/var/datasets/files/ws-123/ds-456/versions/v1/data.jsonl")
        );
    }

    #[test]
    fn layout_sample_file_path() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        let path = layout.sample_file_path("ws-123", "ds-456", "preview.jsonl").unwrap();
        assert_eq!(
            path,
            PathBuf::from("/var/datasets/files/ws-123/ds-456/samples/preview.jsonl")
        );
    }

    #[test]
    fn layout_version_sample_file_path() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        let path = layout.version_sample_file_path("ws-123", "ds-456", "v1", "preview.jsonl").unwrap();
        assert_eq!(
            path,
            PathBuf::from("/var/datasets/files/ws-123/ds-456/versions/v1/samples/preview.jsonl")
        );
    }

    #[test]
    fn layout_canonical_file_path() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        let path = layout
            .canonical_file_path(&DatasetCategory::Codebase, "abcd1234567890", "data.jsonl")
            .unwrap();
        assert_eq!(
            path,
            PathBuf::from("/var/datasets/canonical/codebase/ab/abcd1234567890/data.jsonl")
        );
    }

    #[test]
    fn layout_canonical_file_path_with_tenant() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        let path = layout
            .canonical_file_path_with_tenant(
                &DatasetCategory::Upload,
                "efgh5678901234",
                Some("tenant-a"),
                "metadata.json",
            )
            .unwrap();
        assert_eq!(
            path,
            PathBuf::from(
                "/var/datasets/canonical/upload/ef/efgh5678901234/tenants/tenant-a/metadata.json"
            )
        );

        let dir = layout
            .canonical_dir_path_with_tenant(
                &DatasetCategory::Upload,
                "efgh5678901234",
                Some("tenant-a"),
            )
            .unwrap();
        assert_eq!(
            dir,
            PathBuf::from("/var/datasets/canonical/upload/ef/efgh5678901234/tenants/tenant-a")
        );
    }

    #[test]
    fn layout_canonical_hash_too_short() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        let result = layout.canonical_file_path(&DatasetCategory::Metrics, "abc", "data.jsonl");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn layout_temp_and_chunked_paths() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        assert_eq!(layout.temp_dir(), PathBuf::from("/var/datasets/temp"));
        assert_eq!(
            layout.dataset_temp_dir("ws-123", "ds-456"),
            PathBuf::from("/var/datasets/temp/ws-123/ds-456")
        );
        assert_eq!(
            layout.temp_file_path("ws-123", "ds-456", "upload.tmp").unwrap(),
            PathBuf::from("/var/datasets/temp/ws-123/ds-456/upload.tmp")
        );

        assert_eq!(layout.chunked_dir(), PathBuf::from("/var/datasets/chunked"));
        assert_eq!(
            layout.chunked_session_dir("session-abc"),
            PathBuf::from("/var/datasets/chunked/session-abc")
        );
        assert_eq!(
            layout.chunk_file_path("session-abc", 5),
            PathBuf::from("/var/datasets/chunked/session-abc/00000005")
        );
    }

    #[test]
    fn layout_convenience_paths() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        assert_eq!(
            layout.manifest_path("ws-123", "ds-456"),
            PathBuf::from("/var/datasets/files/ws-123/ds-456/manifest.json")
        );
        assert_eq!(
            layout.version_manifest_path("ws-123", "ds-456", "v1"),
            PathBuf::from("/var/datasets/files/ws-123/ds-456/versions/v1/manifest.json")
        );
        assert_eq!(
            layout.canonical_data_path("ws-123", "ds-456"),
            PathBuf::from("/var/datasets/files/ws-123/ds-456/canonical.jsonl")
        );
    }

    #[test]
    fn layout_storage_key_builders() {
        let layout = DatasetStorageLayout::new("/var/datasets");

        let key = layout.storage_key_for_file("ws-123", "ds-456", "train.jsonl");
        assert_eq!(key.tenant_id, Some("ws-123".to_string()));
        assert_eq!(key.object_id, "ds-456");
        assert_eq!(key.file_name, "train.jsonl");
        assert!(matches!(key.kind, StorageKind::DatasetFile));

        let key = layout.storage_key_for_version_file("ws-123", "ds-456", "v1", "data.jsonl");
        assert_eq!(key.version_id, Some("v1".to_string()));

        let key =
            layout.storage_key_for_canonical("abcd1234", DatasetCategory::Codebase, "file.json");
        assert!(matches!(key.kind, StorageKind::CanonicalDataset));
        assert_eq!(key.content_hash, Some("abcd1234".to_string()));

        let key = layout.storage_key_for_sample_file("ws-123", "ds-456", "preview.jsonl");
        assert_eq!(key.tenant_id, Some("ws-123".to_string()));
        assert_eq!(key.object_id, "ds-456");
        assert_eq!(key.file_name, "samples/preview.jsonl");
        assert!(matches!(key.kind, StorageKind::DatasetFile));
    }

    #[test]
    fn dataset_category_from_dir_name() {
        assert_eq!(
            DatasetCategory::from_dir_name("codebase"),
            Some(DatasetCategory::Codebase)
        );
        assert_eq!(
            DatasetCategory::from_dir_name("metrics"),
            Some(DatasetCategory::Metrics)
        );
        assert_eq!(
            DatasetCategory::from_dir_name("custom_name"),
            Some(DatasetCategory::Custom("custom_name".to_string()))
        );
    }

    // =========================================================================
    // FsByteStorage tests
    // =========================================================================

    #[tokio::test]
    async fn fs_byte_storage_roundtrip() {
        let dir = tempdir().unwrap();
        let ds_root = dir.path().join("datasets");
        let ad_root = dir.path().join("adapters");
        let store = FsByteStorage::new(ds_root.clone(), ad_root.clone());

        let key = StorageKey {
            tenant_id: Some("t1".into()),
            object_id: "obj".into(),
            version_id: Some("v1".into()),
            file_name: "file.bin".into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        };

        let location = store.store_bytes(&key, b"hello").await.unwrap();
        assert!(location.path.exists());
        assert_eq!(location.size_bytes, 5);

        let bytes = store.open_bytes(&key).await.unwrap();
        assert_eq!(bytes, Bytes::from_static(b"hello"));

        let stat = store.stat(&key).await.unwrap();
        assert_eq!(stat.size_bytes, 5);

        store.delete(&key).await.unwrap();
        assert!(!location.path.exists());
    }

    #[test]
    fn fs_byte_storage_layout_access() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        // Verify layout is accessible and matches datasets_root
        assert_eq!(store.layout().root(), Path::new("/data/datasets"));
        assert_eq!(store.datasets_root(), Path::new("/data/datasets"));
        assert_eq!(store.adapters_root(), Path::new("/data/adapters"));
    }

    #[test]
    fn dataset_path_with_tenant_and_version() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        // Full key with tenant_id and version_id
        let key = StorageKey {
            tenant_id: Some("workspace-123".into()),
            object_id: "dataset-456".into(),
            version_id: Some("v2".into()),
            file_name: "train.jsonl".into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        };

        let path = store.dataset_path(&key);
        // Canonical layout: files/{workspace_id}/{dataset_id}/versions/{version_id}/{file}
        assert_eq!(
            path,
            PathBuf::from("/data/datasets/files/workspace-123/dataset-456/versions/v2/train.jsonl")
        );
    }

    #[test]
    fn dataset_path_with_tenant_no_version() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        // Key with tenant_id but no version_id
        let key = StorageKey {
            tenant_id: Some("workspace-123".into()),
            object_id: "dataset-456".into(),
            version_id: None,
            file_name: "train.jsonl".into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        };

        let path = store.dataset_path(&key);
        // Layout without version: files/{workspace_id}/{dataset_id}/{file}
        assert_eq!(
            path,
            PathBuf::from("/data/datasets/files/workspace-123/dataset-456/train.jsonl")
        );
    }

    #[test]
    fn dataset_path_without_tenant() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        // Key without tenant_id (legacy/unscoped)
        let key = StorageKey {
            tenant_id: None,
            object_id: "dataset-456".into(),
            version_id: None,
            file_name: "train.jsonl".into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        };

        let path = store.dataset_path(&key);
        // Fallback layout without workspace: files/{dataset_id}/{file}
        assert_eq!(
            path,
            PathBuf::from("/data/datasets/files/dataset-456/train.jsonl")
        );
    }

    #[test]
    fn dataset_path_no_tenant_with_version() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        // Key without tenant_id but with version (legacy path)
        let key = StorageKey {
            tenant_id: None,
            object_id: "dataset-456".into(),
            version_id: Some("v1".into()),
            file_name: "train.jsonl".into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        };

        let path = store.dataset_path(&key);
        // Legacy layout without workspace but with version
        assert_eq!(
            path,
            PathBuf::from("/data/datasets/files/dataset-456/versions/v1/train.jsonl")
        );
    }

    #[test]
    fn dataset_path_rejects_absolute_file_name() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());
        let key = StorageKey {
            tenant_id: Some("workspace-123".into()),
            object_id: "dataset-456".into(),
            version_id: None,
            file_name: "/tmp/evil.jsonl".into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        };

        let err = store.path_for(&key).unwrap_err();
        assert!(err.to_string().contains("relative path"));
    }

    #[test]
    fn dataset_path_rejects_parent_dir_file_name() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());
        let key = StorageKey {
            tenant_id: Some("workspace-123".into()),
            object_id: "dataset-456".into(),
            version_id: None,
            file_name: "../evil.jsonl".into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        };

        let err = store.path_for(&key).unwrap_err();
        assert!(err.to_string().contains("dataset root"));
    }

    #[test]
    fn dataset_path_rejects_workspace_with_separator() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());
        let key = StorageKey {
            tenant_id: Some("workspace/evil".into()),
            object_id: "dataset-456".into(),
            version_id: None,
            file_name: "train.jsonl".into(),
            kind: StorageKind::DatasetFile,
            content_hash: None,
            category: None,
        };

        let err = store.path_for(&key).unwrap_err();
        assert!(err.to_string().contains("workspace_id"));
    }

    #[test]
    fn canonical_dataset_path_via_storage() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        let key = StorageKey::canonical_dataset(
            "abcd1234567890fedcba",
            DatasetCategory::Codebase,
            None,
            "data.jsonl",
        );

        let path = store.path_for(&key).unwrap();
        assert_eq!(
            path,
            PathBuf::from("/data/datasets/canonical/codebase/ab/abcd1234567890fedcba/data.jsonl")
        );
    }

    #[test]
    fn canonical_dataset_path_with_tenant_via_storage() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());

        let key = StorageKey::canonical_dataset_with_tenant(
            "tenant-x",
            "efgh5678901234567890",
            DatasetCategory::Upload,
            None,
            "metadata.json",
        );

        let path = store.path_for(&key).unwrap();
        assert_eq!(
            path,
            PathBuf::from(
                "/data/datasets/canonical/upload/ef/efgh5678901234567890/tenants/tenant-x/metadata.json"
            )
        );
    }

    #[test]
    fn compute_deterministic_path_for_dataset() {
        // 64-char BLAKE3 hash example
        let hash = "a1b2c3d4e5f6789012345678901234567890123456789012345678901234abcd";

        let path = compute_deterministic_path(Path::new("/data"), hash, StorageKind::DatasetFile);
        // Path: canonical/general/{hash_prefix}/{content_hash}/canonical.jsonl
        assert_eq!(
            path,
            PathBuf::from(
                "/data/canonical/general/a1/a1b2c3d4e5f6789012345678901234567890123456789012345678901234abcd/canonical.jsonl"
            )
        );
    }

    #[test]
    fn compute_deterministic_path_for_adapter() {
        let hash = "ff00112233445566778899aabbccddeeff00112233445566778899aabbccddee";

        let path =
            compute_deterministic_path(Path::new("/adapters"), hash, StorageKind::AdapterArtifact);
        assert_eq!(
            path,
            PathBuf::from("/adapters/artifacts/ff/001122/canonical.aos")
        );
    }

    #[test]
    fn deterministic_dataset_path_uses_datasets_root() {
        let store = FsByteStorage::new("/data/datasets".into(), "/data/adapters".into());
        let hash = "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";

        let path = store.deterministic_dataset_path(hash);
        assert_eq!(
            path,
            PathBuf::from(
                "/data/datasets/canonical/general/ab/abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890/canonical.jsonl"
            )
        );
    }

    #[test]
    fn deterministic_path_is_deterministic() {
        // Same hash should always produce the same path
        let hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

        let path1 = compute_deterministic_path(Path::new("/root"), hash, StorageKind::DatasetFile);
        let path2 = compute_deterministic_path(Path::new("/root"), hash, StorageKind::DatasetFile);

        assert_eq!(path1, path2);
    }
}
