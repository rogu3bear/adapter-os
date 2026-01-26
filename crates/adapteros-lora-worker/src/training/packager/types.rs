//! Shared types for adapter packaging

use super::manifest::AdapterManifest;
use adapteros_core::normalize_repo_slug;
use adapteros_types::coreml::CoreMLPlacementSpec;
use adapteros_types::training::LoraTier;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Adapter packager.
/// Adapter-only invariant: only LoRA deltas are ever exported; base model
/// weights remain outside the package boundary.
#[derive(Debug)]
pub struct AdapterPackager {
    pub(super) repo_root: PathBuf,
}

/// Packaged adapter with all metadata
#[derive(Debug, Clone)]
pub struct PackagedAdapter {
    pub adapter_id: String,
    pub manifest: AdapterManifest,
    pub weights_path: PathBuf,
    pub hash_b3: String,
}

/// Placement specification for CoreML fusion/attach (canonical type).
pub type CoremlPlacementSpec = CoreMLPlacementSpec;

/// Per-layer hash entry keyed by canonical logical layer path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerHash {
    pub hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tensor_name: Option<String>,
}

/// CoreML-specific training metadata captured at packaging time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoremlTrainingMetadata {
    pub coreml_used: bool,
    #[serde(default)]
    pub coreml_device_type: Option<String>,
    #[serde(default)]
    pub coreml_precision_mode: Option<String>,
    #[serde(default)]
    pub coreml_compile_config_id: Option<String>,
}

/// Placement metadata describing how adapters map onto CoreML graph targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterPlacement {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<PlacementRecord>,
}

/// Individual placement record for CoreML graph targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlacementRecord {
    pub graph_target: String,
    pub rank: u32,
    pub direction: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha_override: Option<f32>,
}

/// Metadata for a scan root used during training package creation.
///
/// A scan root represents a directory path that was scanned during codebase
/// ingestion. When training combines content from multiple directories (e.g.,
/// monorepo workspaces, multi-module projects), each scanned location is
/// recorded here for provenance tracking.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanRootMetadata {
    /// Absolute or relative path to the scan root directory
    pub path: String,
    /// Optional label describing this scan root's role (e.g., "main", "lib", "tests")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Number of files processed from this scan root
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_count: Option<u64>,
    /// Total bytes ingested from this scan root
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<u64>,
    /// BLAKE3 hash of the scan root's content at ingestion time
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        alias = "content_hash_b3"
    )]
    pub content_hash: Option<String>,
    /// Timestamp when this scan root was processed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scanned_at: Option<String>,
}

/// Branch metadata for packaging context.
///
/// Contains git branch and commit information that should be included
/// in the packaged training artifacts for provenance tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BranchMetadata {
    /// Git branch name (e.g., "main", "feature/xyz")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Git commit SHA at the time of training
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Full commit SHA (if different from abbreviated)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_full: Option<String>,
    /// Repository name or identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    /// Normalized repository slug (e.g., "my_project")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    /// Remote URL of the repository
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    /// Whether the working tree was dirty (had uncommitted changes)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    /// Timestamp when the branch metadata was captured
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<String>,
}

impl BranchMetadata {
    /// Create new BranchMetadata with branch and commit info.
    pub fn new(branch: impl Into<String>, commit: impl Into<String>) -> Self {
        Self {
            branch: Some(branch.into()),
            commit: Some(commit.into()),
            ..Default::default()
        }
    }

    /// Create BranchMetadata with full commit SHA.
    pub fn with_full_commit(mut self, commit_full: impl Into<String>) -> Self {
        self.commit_full = Some(commit_full.into());
        self
    }

    /// Set repository name.
    pub fn with_repo_name(mut self, repo_name: impl Into<String>) -> Self {
        self.repo_name = Some(repo_name.into());
        self
    }

    /// Set repository slug.
    pub fn with_repo_slug(mut self, repo_slug: impl Into<String>) -> Self {
        let slug = repo_slug.into();
        self.repo_slug = Some(normalize_repo_slug(&slug));
        self
    }

    /// Set remote URL.
    pub fn with_remote_url(mut self, remote_url: impl Into<String>) -> Self {
        self.remote_url = Some(remote_url.into());
        self
    }

    /// Set dirty flag indicating uncommitted changes.
    pub fn with_dirty(mut self, dirty: bool) -> Self {
        self.dirty = Some(dirty);
        self
    }

    /// Set capture timestamp.
    pub fn with_captured_at(mut self, captured_at: impl Into<String>) -> Self {
        self.captured_at = Some(captured_at.into());
        self
    }

    /// Convert to metadata HashMap entries for packaging.
    pub fn to_metadata_entries(&self) -> HashMap<String, String> {
        let mut entries = HashMap::new();
        if let Some(ref branch) = self.branch {
            entries.insert("scope_branch".to_string(), branch.clone());
        }
        if let Some(ref commit) = self.commit {
            entries.insert("scope_commit".to_string(), commit.clone());
        }
        if let Some(ref commit_full) = self.commit_full {
            entries.insert("scope_commit_full".to_string(), commit_full.clone());
        }
        if let Some(ref repo_name) = self.repo_name {
            entries.insert("scope_repo".to_string(), repo_name.clone());
        }
        if let Some(ref repo_slug) = self.repo_slug {
            entries.insert("repo_slug".to_string(), repo_slug.clone());
        }
        if let Some(ref remote_url) = self.remote_url {
            entries.insert("scope_remote_url".to_string(), remote_url.clone());
        }
        if let Some(dirty) = self.dirty {
            entries.insert("scope_dirty".to_string(), dirty.to_string());
        }
        if let Some(ref captured_at) = self.captured_at {
            entries.insert(
                "branch_metadata_captured_at".to_string(),
                captured_at.clone(),
            );
        }
        entries
    }

    /// Check if branch metadata is present (has at least branch or commit).
    pub fn is_present(&self) -> bool {
        self.branch.is_some() || self.commit.is_some()
    }

    /// Parse BranchMetadata from a metadata HashMap.
    pub fn from_metadata(metadata: &HashMap<String, String>) -> Self {
        use super::metadata::normalize_optional_str;
        Self {
            branch: normalize_optional_str(
                metadata
                    .get("scope_branch")
                    .or_else(|| metadata.get("repo_branch"))
                    .or_else(|| metadata.get("branch"))
                    .map(String::as_str),
            ),
            commit: normalize_optional_str(
                metadata
                    .get("scope_commit")
                    .or_else(|| metadata.get("repo_commit"))
                    .or_else(|| metadata.get("commit_sha"))
                    .or_else(|| metadata.get("commit_short_sha"))
                    .or_else(|| metadata.get("commit"))
                    .map(String::as_str),
            ),
            commit_full: normalize_optional_str(
                metadata
                    .get("scope_commit_full")
                    .or_else(|| metadata.get("commit_full"))
                    .or_else(|| metadata.get("commit_sha"))
                    .map(String::as_str),
            ),
            repo_name: normalize_optional_str(
                metadata
                    .get("scope_repo")
                    .or_else(|| metadata.get("repo_name"))
                    .map(String::as_str),
            ),
            repo_slug: normalize_optional_str(
                metadata
                    .get("repo_slug")
                    .or_else(|| metadata.get("scope_repo_slug"))
                    .map(String::as_str),
            )
            .map(|slug| normalize_repo_slug(&slug)),
            remote_url: normalize_optional_str(
                metadata
                    .get("scope_remote_url")
                    .or_else(|| metadata.get("repo_remote"))
                    .or_else(|| metadata.get("remote_url"))
                    .map(String::as_str),
            ),
            dirty: metadata
                .get("scope_dirty")
                .or_else(|| metadata.get("dirty"))
                .and_then(|v| v.parse().ok()),
            captured_at: normalize_optional_str(
                metadata
                    .get("branch_metadata_captured_at")
                    .or_else(|| metadata.get("captured_at"))
                    .map(String::as_str),
            ),
        }
    }
}
