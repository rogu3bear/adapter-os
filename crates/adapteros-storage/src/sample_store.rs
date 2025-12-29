//! Sample artifact storage following the dataset file layout.
//!
//! This module provides specialized storage for training sample datasets,
//! following the conventions in `training/datasets/`.
//!
//! # Layout
//!
//! Sample artifacts follow the dataset file layout conventions:
//! ```text
//! {samples_root}/{category}/{dataset_name}/{file_name}
//! {samples_root}/{category}/{dataset_name}/{version}/{file_name}
//! ```
//!
//! Categories match the training/datasets taxonomy:
//! - behaviors, routing, stacks, replay, determinism, metrics, cli_contract, code_ingest, docs_derived
//!
//! Each sample dataset should include:
//! - `manifest.json` - Dataset metadata and configuration
//! - `*.jsonl` or `*.positive.jsonl` / `*.negative.jsonl` - Training examples
//! - `README.md` - Documentation (optional)

use adapteros_core::Result;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::ensure_free_space;

/// Sample dataset category matching training/datasets taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SampleCategory {
    /// Adapter behavior patterns and runtime characteristics
    Behaviors,
    /// K-sparse router decision training
    Routing,
    /// Adapter stack composition and workflows
    Stacks,
    /// Deterministic replay and verification
    Replay,
    /// Determinism guardrail training
    Determinism,
    /// Telemetry and observability patterns
    Metrics,
    /// CLI command patterns and contracts
    CliContract,
    /// Document/code ingestion training
    CodeIngest,
    /// Documentation-derived training data
    DocsDerived,
    /// Codebase analysis patterns
    Codebase,
}

impl SampleCategory {
    /// Get the directory name for this category
    pub fn as_dir_name(&self) -> &'static str {
        match self {
            Self::Behaviors => "behaviors",
            Self::Routing => "routing",
            Self::Stacks => "stacks",
            Self::Replay => "replay",
            Self::Determinism => "determinism",
            Self::Metrics => "metrics",
            Self::CliContract => "cli_contract",
            Self::CodeIngest => "code_ingest",
            Self::DocsDerived => "docs_derived",
            Self::Codebase => "codebase",
        }
    }

    /// Parse category from directory name
    pub fn from_dir_name(name: &str) -> Option<Self> {
        match name {
            "behaviors" => Some(Self::Behaviors),
            "routing" => Some(Self::Routing),
            "stacks" => Some(Self::Stacks),
            "replay" => Some(Self::Replay),
            "determinism" => Some(Self::Determinism),
            "metrics" => Some(Self::Metrics),
            "cli_contract" => Some(Self::CliContract),
            "code_ingest" => Some(Self::CodeIngest),
            "docs_derived" => Some(Self::DocsDerived),
            "codebase" => Some(Self::Codebase),
            _ => None,
        }
    }

    /// Get all available categories
    pub fn all() -> &'static [Self] {
        &[
            Self::Behaviors,
            Self::Routing,
            Self::Stacks,
            Self::Replay,
            Self::Determinism,
            Self::Metrics,
            Self::CliContract,
            Self::CodeIngest,
            Self::DocsDerived,
            Self::Codebase,
        ]
    }
}

impl std::fmt::Display for SampleCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_dir_name())
    }
}

/// Sample manifest following the dataset manifest schema.
///
/// This schema matches the manifests used in `training/datasets/*/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleManifest {
    /// Unique dataset identifier
    pub name: String,
    /// Human-readable description
    #[serde(default)]
    pub description: String,
    /// Semantic version
    #[serde(default = "default_version")]
    pub version: String,
    /// Dataset category (matches SampleCategory::as_dir_name())
    pub category: String,
    /// Dataset scope (global, tenant, workspace)
    #[serde(default)]
    pub scope: Option<String>,
    /// Dataset tier (test, staging, production, critical)
    #[serde(default)]
    pub tier: Option<String>,
    /// LoRA rank for training
    #[serde(default)]
    pub rank: Option<u32>,
    /// LoRA alpha for training
    #[serde(default)]
    pub alpha: Option<f32>,
    /// Target modules for LoRA training
    #[serde(default)]
    pub target_modules: Vec<String>,
    /// Dataset entries (files)
    #[serde(default)]
    pub entries: Vec<SampleEntry>,
    /// Provenance information
    #[serde(default)]
    pub provenance: Option<SampleProvenance>,
    /// Evaluation gates (quality thresholds)
    #[serde(default)]
    pub evaluation_gates: Vec<String>,
    /// Dataset intent/purpose
    #[serde(default)]
    pub intent: Option<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl SampleManifest {
    /// Create a new manifest with minimal required fields.
    pub fn new(name: impl Into<String>, category: SampleCategory) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            version: default_version(),
            category: category.as_dir_name().to_string(),
            scope: None,
            tier: None,
            rank: None,
            alpha: None,
            target_modules: Vec::new(),
            entries: Vec::new(),
            provenance: None,
            evaluation_gates: Vec::new(),
            intent: None,
        }
    }

    /// Set the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// Set the version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Add an entry.
    pub fn with_entry(mut self, entry: SampleEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Set LoRA training parameters.
    pub fn with_lora_params(mut self, rank: u32, alpha: f32, target_modules: Vec<String>) -> Self {
        self.rank = Some(rank);
        self.alpha = Some(alpha);
        self.target_modules = target_modules;
        self
    }
}

/// Entry in a sample manifest pointing to a data file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleEntry {
    /// Relative path to the file
    pub path: String,
    /// File format (jsonl, csv, etc.)
    #[serde(default = "default_format")]
    pub format: String,
    /// Weight for training (0.0-1.0)
    #[serde(default = "default_weight")]
    pub weight: f32,
    /// Role (positive, negative, adversarial)
    #[serde(default)]
    pub role: Option<String>,
    /// Additional notes
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_format() -> String {
    "jsonl".to_string()
}

fn default_weight() -> f32 {
    1.0
}

impl SampleEntry {
    /// Create a new entry with minimal required fields.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            format: default_format(),
            weight: default_weight(),
            role: None,
            notes: None,
        }
    }

    /// Set the format.
    pub fn with_format(mut self, format: impl Into<String>) -> Self {
        self.format = format.into();
        self
    }

    /// Set the role.
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    /// Set the weight.
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// Set notes.
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }
}

/// Provenance information for a sample dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleProvenance {
    /// Reference to source documentation sections
    #[serde(default)]
    pub masterplan_sections: Vec<String>,
    /// Creator identifier
    #[serde(default)]
    pub created_by: Option<String>,
    /// Creation timestamp (ISO 8601)
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last review timestamp
    #[serde(default)]
    pub last_reviewed_at: Option<String>,
    /// Review notes
    #[serde(default)]
    pub review_notes: Option<String>,
}

impl SampleProvenance {
    /// Create new provenance with creator.
    pub fn new(created_by: impl Into<String>) -> Self {
        Self {
            masterplan_sections: Vec::new(),
            created_by: Some(created_by.into()),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            last_reviewed_at: None,
            review_notes: None,
        }
    }
}

/// Location and metadata for a stored sample artifact.
#[derive(Debug, Clone)]
pub struct SampleLocation {
    /// Absolute path to the file
    pub path: PathBuf,
    /// Size in bytes
    pub size_bytes: u64,
}

/// Filesystem-backed sample artifact store.
///
/// Provides storage operations for sample datasets following the
/// `training/datasets/` layout conventions.
#[derive(Debug, Clone)]
pub struct SampleStore {
    /// Root directory for samples (e.g., `training/datasets`)
    samples_root: PathBuf,
}

impl SampleStore {
    /// Create a new SampleStore with the given root directory.
    pub fn new(samples_root: impl AsRef<Path>) -> Self {
        Self {
            samples_root: samples_root.as_ref().to_path_buf(),
        }
    }

    /// Create a SampleStore using the default training/datasets path.
    pub fn default_location() -> Self {
        let samples_root = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("training")
            .join("datasets");
        Self::new(samples_root)
    }

    /// Get the samples root directory.
    pub fn root(&self) -> &Path {
        &self.samples_root
    }

    /// Get the path for a category directory.
    pub fn category_path(&self, category: SampleCategory) -> PathBuf {
        self.samples_root.join(category.as_dir_name())
    }

    /// Get the path for a sample dataset directory.
    pub fn dataset_path(&self, category: SampleCategory, dataset_name: &str) -> PathBuf {
        self.category_path(category).join(dataset_name)
    }

    /// Get the path for a sample dataset file.
    pub fn file_path(
        &self,
        category: SampleCategory,
        dataset_name: &str,
        file_name: &str,
    ) -> PathBuf {
        self.dataset_path(category, dataset_name).join(file_name)
    }

    /// Get the path for a versioned sample dataset file.
    pub fn versioned_file_path(
        &self,
        category: SampleCategory,
        dataset_name: &str,
        version: &str,
        file_name: &str,
    ) -> PathBuf {
        self.dataset_path(category, dataset_name)
            .join(version)
            .join(file_name)
    }

    /// Check if a sample dataset exists.
    pub fn dataset_exists(&self, category: SampleCategory, dataset_name: &str) -> bool {
        self.dataset_path(category, dataset_name)
            .join("manifest.json")
            .exists()
    }

    /// Store a sample manifest.
    pub async fn store_manifest(
        &self,
        category: SampleCategory,
        dataset_name: &str,
        manifest: &SampleManifest,
    ) -> Result<SampleLocation> {
        let path = self.file_path(category, dataset_name, "manifest.json");
        let json = serde_json::to_string_pretty(manifest).map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to serialize manifest: {}", e))
        })?;
        self.store_bytes_at(&path, json.as_bytes()).await
    }

    /// Load a sample manifest.
    pub async fn load_manifest(
        &self,
        category: SampleCategory,
        dataset_name: &str,
    ) -> Result<SampleManifest> {
        let path = self.file_path(category, dataset_name, "manifest.json");
        let bytes = self.load_bytes_at(&path).await?;
        serde_json::from_slice(&bytes).map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to parse manifest: {}", e))
        })
    }

    /// Store a sample artifact file.
    pub async fn store_artifact(
        &self,
        category: SampleCategory,
        dataset_name: &str,
        file_name: &str,
        data: &[u8],
    ) -> Result<SampleLocation> {
        let path = self.file_path(category, dataset_name, file_name);
        self.store_bytes_at(&path, data).await
    }

    /// Load a sample artifact file.
    pub async fn load_artifact(
        &self,
        category: SampleCategory,
        dataset_name: &str,
        file_name: &str,
    ) -> Result<Bytes> {
        let path = self.file_path(category, dataset_name, file_name);
        self.load_bytes_at(&path).await
    }

    /// Create a new sample dataset with manifest.
    pub async fn create_dataset(
        &self,
        category: SampleCategory,
        manifest: &SampleManifest,
    ) -> Result<PathBuf> {
        let dataset_dir = self.dataset_path(category, &manifest.name);

        // Create directory
        fs::create_dir_all(&dataset_dir).await.map_err(|e| {
            adapteros_core::AosError::Io(format!(
                "Failed to create dataset dir {}: {}",
                dataset_dir.display(),
                e
            ))
        })?;

        // Write manifest
        self.store_manifest(category, &manifest.name, manifest)
            .await?;

        Ok(dataset_dir)
    }

    /// Delete a sample dataset and all its files.
    pub async fn delete_dataset(
        &self,
        category: SampleCategory,
        dataset_name: &str,
    ) -> Result<()> {
        let dataset_dir = self.dataset_path(category, dataset_name);
        if dataset_dir.exists() {
            fs::remove_dir_all(&dataset_dir).await.map_err(|e| {
                adapteros_core::AosError::Io(format!(
                    "Failed to delete dataset dir {}: {}",
                    dataset_dir.display(),
                    e
                ))
            })?;
        }
        Ok(())
    }

    /// List all sample datasets in a category.
    pub async fn list_datasets(&self, category: SampleCategory) -> Result<Vec<String>> {
        let category_dir = self.category_path(category);
        if !category_dir.exists() {
            return Ok(Vec::new());
        }

        let mut datasets = Vec::new();
        let mut entries = fs::read_dir(&category_dir).await.map_err(|e| {
            adapteros_core::AosError::Io(format!(
                "Failed to read category dir {}: {}",
                category_dir.display(),
                e
            ))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to read dir entry: {}", e))
        })? {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Only include directories that have a manifest.json
                    if path.join("manifest.json").exists() {
                        datasets.push(name.to_string());
                    }
                }
            }
        }

        datasets.sort();
        Ok(datasets)
    }

    /// List all files in a sample dataset.
    pub async fn list_files(
        &self,
        category: SampleCategory,
        dataset_name: &str,
    ) -> Result<Vec<String>> {
        let dataset_dir = self.dataset_path(category, dataset_name);
        if !dataset_dir.exists() {
            return Ok(Vec::new());
        }

        let mut files = Vec::new();
        let mut entries = fs::read_dir(&dataset_dir).await.map_err(|e| {
            adapteros_core::AosError::Io(format!(
                "Failed to read dataset dir {}: {}",
                dataset_dir.display(),
                e
            ))
        })?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to read dir entry: {}", e))
        })? {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    files.push(name.to_string());
                }
            }
        }

        files.sort();
        Ok(files)
    }

    /// List all categories that have at least one dataset.
    pub async fn list_categories(&self) -> Result<Vec<SampleCategory>> {
        let mut categories = Vec::new();

        for cat in SampleCategory::all() {
            let cat_dir = self.category_path(*cat);
            if cat_dir.exists() {
                // Check if there are any subdirectories with manifests
                if let Ok(mut entries) = fs::read_dir(&cat_dir).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let path = entry.path();
                        if path.is_dir() && path.join("manifest.json").exists() {
                            categories.push(*cat);
                            break;
                        }
                    }
                }
            }
        }

        Ok(categories)
    }

    // =========================================================================
    // Private helpers
    // =========================================================================

    async fn store_bytes_at(&self, path: &Path, data: &[u8]) -> Result<SampleLocation> {
        // Create parent directories
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.map_err(|e| {
                adapteros_core::AosError::Io(format!(
                    "Failed to create parent dir {}: {}",
                    parent.display(),
                    e
                ))
            })?;

            // Check disk space
            ensure_free_space(parent, "sample store write").map_err(|e| {
                adapteros_core::AosError::Io(format!(
                    "Failed to ensure free space for {}: {}",
                    path.display(),
                    e
                ))
            })?;
        }

        // Write file
        let mut file = fs::File::create(path).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to create {}: {}", path.display(), e))
        })?;
        file.write_all(data).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to write {}: {}", path.display(), e))
        })?;

        Ok(SampleLocation {
            path: path.to_path_buf(),
            size_bytes: data.len() as u64,
        })
    }

    async fn load_bytes_at(&self, path: &Path) -> Result<Bytes> {
        let bytes = fs::read(path).await.map_err(|e| {
            adapteros_core::AosError::Io(format!("Failed to read {}: {}", path.display(), e))
        })?;
        Ok(Bytes::from(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn sample_store_roundtrip() {
        let dir = tempdir().unwrap();
        let store = SampleStore::new(dir.path());

        // Create a manifest
        let manifest = SampleManifest::new("test_dataset", SampleCategory::Routing)
            .with_description("A test sample dataset")
            .with_entry(SampleEntry::new("data.jsonl").with_role("positive"));

        // Store manifest
        let location = store
            .store_manifest(SampleCategory::Routing, "test_dataset", &manifest)
            .await
            .unwrap();
        assert!(location.path.exists());
        assert!(location.path.to_string_lossy().contains("routing"));
        assert!(location.path.to_string_lossy().contains("test_dataset"));

        // Load manifest
        let loaded = store
            .load_manifest(SampleCategory::Routing, "test_dataset")
            .await
            .unwrap();
        assert_eq!(loaded.name, "test_dataset");
        assert_eq!(loaded.category, "routing");

        // Store artifact
        let data = b"{\"input\": \"test\", \"target\": \"result\"}";
        let artifact_location = store
            .store_artifact(SampleCategory::Routing, "test_dataset", "data.jsonl", data)
            .await
            .unwrap();
        assert!(artifact_location.path.exists());
        assert_eq!(artifact_location.size_bytes, data.len() as u64);

        // Load artifact
        let loaded_data = store
            .load_artifact(SampleCategory::Routing, "test_dataset", "data.jsonl")
            .await
            .unwrap();
        assert_eq!(loaded_data.as_ref(), data);

        // Check dataset exists
        assert!(store.dataset_exists(SampleCategory::Routing, "test_dataset"));

        // List files
        let files = store
            .list_files(SampleCategory::Routing, "test_dataset")
            .await
            .unwrap();
        assert!(files.contains(&"manifest.json".to_string()));
        assert!(files.contains(&"data.jsonl".to_string()));

        // Delete dataset
        store
            .delete_dataset(SampleCategory::Routing, "test_dataset")
            .await
            .unwrap();
        assert!(!store.dataset_exists(SampleCategory::Routing, "test_dataset"));
    }

    #[tokio::test]
    async fn create_dataset_creates_dir_and_manifest() {
        let dir = tempdir().unwrap();
        let store = SampleStore::new(dir.path());

        let manifest = SampleManifest::new("new_dataset", SampleCategory::Determinism)
            .with_description("Created dataset");

        let dataset_dir = store
            .create_dataset(SampleCategory::Determinism, &manifest)
            .await
            .unwrap();

        assert!(dataset_dir.exists());
        assert!(dataset_dir.join("manifest.json").exists());
        assert!(store.dataset_exists(SampleCategory::Determinism, "new_dataset"));
    }

    #[tokio::test]
    async fn list_datasets_returns_only_valid_datasets() {
        let dir = tempdir().unwrap();
        let store = SampleStore::new(dir.path());

        // Create a valid dataset
        let manifest = SampleManifest::new("valid_dataset", SampleCategory::Routing);
        store
            .create_dataset(SampleCategory::Routing, &manifest)
            .await
            .unwrap();

        // Create a directory without manifest (should not be listed)
        let invalid_dir = store.dataset_path(SampleCategory::Routing, "invalid_dataset");
        fs::create_dir_all(&invalid_dir).await.unwrap();

        let datasets = store.list_datasets(SampleCategory::Routing).await.unwrap();
        assert_eq!(datasets, vec!["valid_dataset".to_string()]);
    }

    #[test]
    fn sample_category_dir_name_roundtrip() {
        for cat in SampleCategory::all() {
            let dir_name = cat.as_dir_name();
            let parsed = SampleCategory::from_dir_name(dir_name).unwrap();
            assert_eq!(*cat, parsed);
        }
    }

    #[test]
    fn manifest_builder() {
        let manifest = SampleManifest::new("test", SampleCategory::Behaviors)
            .with_description("Test dataset")
            .with_version("2.0.0")
            .with_lora_params(4, 8.0, vec!["gate_proj".to_string()])
            .with_entry(
                SampleEntry::new("data.jsonl")
                    .with_role("positive")
                    .with_weight(0.8),
            );

        assert_eq!(manifest.name, "test");
        assert_eq!(manifest.category, "behaviors");
        assert_eq!(manifest.version, "2.0.0");
        assert_eq!(manifest.rank, Some(4));
        assert_eq!(manifest.alpha, Some(8.0));
        assert_eq!(manifest.entries.len(), 1);
        assert_eq!(manifest.entries[0].role, Some("positive".to_string()));
    }
}
