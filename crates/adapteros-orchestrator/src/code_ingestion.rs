//! Automated codebase ingestion and adapter training pipeline
//!
//! Provides a deterministic end-to-end workflow that scans a repository,
//! extracts symbol knowledge via adapteros-codegraph, builds Q&A training
//! samples, fine-tunes a Micro-LoRA adapter, packages it into a `.aos`
//! artifact, and optionally registers it in the adapter registry.

use adapteros_retrieval::codegraph::{CodeGraph, SymbolKind, SymbolNode, Visibility};
use adapteros_core::paths::AdapterPaths;
use adapteros_core::seed::{
    derive_seed_u64_from_inputs, is_strict_determinism_mode, maybe_stable_sort,
    should_use_stable_ordering, DeterminismConfig as CoreDeterminismConfig, DeterminismConfigGuard,
};
use adapteros_core::tenant::TenantId;
use adapteros_core::validation::validate_codebase_adapter_id;
use adapteros_core::{AosError, B3Hash, Result};

// Re-export normalization functions from shared crate for backwards compatibility
use adapteros_db::training_datasets::CreateDatasetHashInputsParams;
use adapteros_db::training_datasets::{CodebaseDatasetRowInput, SampleRole};
use adapteros_db::{AdapterRegistrationBuilder, Db};
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{
    AdapterPackager, LoRAQuantizer, MicroLoRATrainer, TrainingConfig, TrainingExample,
};
pub use adapteros_core::normalization::{
    normalize_path_segments, normalize_repo_id, normalize_repo_slug,
};
use adapteros_storage::platform::common::PlatformUtils;
use adapteros_storage::byte_store::{DatasetCategory, FsByteStorage};
use adapteros_types::training::{provenance_from_map, weight_from_metadata, ExampleMetadataV1};
use blake3::Hasher;
use chrono::{TimeZone, Utc};
use git2::Repository;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tokio::fs;
use tokio::task;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Category for code ingestion adapters - ensures consistency between metadata and registration.
/// Valid categories: 'code', 'framework', 'codebase', 'ephemeral' (see migration 0012).
const CODE_INGESTION_ADAPTER_CATEGORY: &str = "codebase";

/// Source repository specification for ingestion.
#[derive(Debug, Clone)]
pub enum CodeIngestionSource {
    /// Use a local path (automatically discovers the git root)
    LocalPath(PathBuf),
    /// Clone a remote git repository URL into a temporary workspace
    GitUrl(String),
}

/// Dataset generation tuning parameters
#[derive(Debug, Clone)]
pub struct CodeDatasetConfig {
    /// Maximum number of symbols to sample from the repository
    pub max_symbols: usize,
    /// Include non-public symbols
    pub include_private: bool,
    /// Weight assigned to knowledge samples
    pub positive_weight: f32,
    /// Weight assigned to abstention samples when documentation is missing.
    /// Must be non-negative; classification is via sample_role metadata, not weight sign.
    pub negative_weight: f32,
}

impl Default for CodeDatasetConfig {
    fn default() -> Self {
        Self {
            max_symbols: 64,
            include_private: false,
            positive_weight: 1.0,
            negative_weight: 0.5, // Non-negative weight; sample_role metadata classifies as abstention
        }
    }
}

/// Configuration for filtering repository scope during ingestion.
///
/// Used to selectively include or exclude files based on paths and extensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoScopeConfig {
    /// Paths to include (e.g., ["src/", "lib/"])
    #[serde(default)]
    pub include_paths: Vec<String>,
    /// Paths to exclude (e.g., ["tests/", "vendor/"])
    #[serde(default)]
    pub exclude_paths: Vec<String>,
    /// File extensions to include (e.g., ["rs", "py"])
    #[serde(default)]
    pub include_extensions: Vec<String>,
    /// File extensions to exclude (e.g., ["md", "txt"])
    #[serde(default)]
    pub exclude_extensions: Vec<String>,
}

impl RepoScopeConfig {
    /// Check if any filters are configured
    pub fn has_filters(&self) -> bool {
        !self.include_paths.is_empty()
            || !self.exclude_paths.is_empty()
            || !self.include_extensions.is_empty()
            || !self.exclude_extensions.is_empty()
    }

    fn normalized(&self) -> Self {
        let mut include_paths = self.include_paths.clone();
        include_paths.sort();
        let mut exclude_paths = self.exclude_paths.clone();
        exclude_paths.sort();
        let mut include_extensions = self.include_extensions.clone();
        include_extensions.sort();
        let mut exclude_extensions = self.exclude_extensions.clone();
        exclude_extensions.sort();

        Self {
            include_paths,
            exclude_paths,
            include_extensions,
            exclude_extensions,
        }
    }

    /// Convert to a deterministic map for metadata storage.
    ///
    /// Serializes the repo scope configuration into key-value pairs suitable
    /// for embedding in adapter metadata or training provenance records.
    /// This enables full traceability of what file filters were applied during ingestion.
    pub fn to_metadata_map(&self) -> BTreeMap<String, String> {
        let normalized = self.normalized();
        let mut map = BTreeMap::new();

        // Record whether any scope filters are active
        map.insert(
            "repo_scope_active".to_string(),
            normalized.has_filters().to_string(),
        );

        // Record filter counts for quick inspection
        if normalized.has_filters() {
            map.insert(
                "repo_scope_filter_count".to_string(),
                (normalized.include_paths.len()
                    + normalized.exclude_paths.len()
                    + normalized.include_extensions.len()
                    + normalized.exclude_extensions.len())
                .to_string(),
            );
        }

        if !normalized.include_paths.is_empty() {
            map.insert(
                "repo_scope_include_paths".to_string(),
                normalized.include_paths.join(","),
            );
        }
        if !normalized.exclude_paths.is_empty() {
            map.insert(
                "repo_scope_exclude_paths".to_string(),
                normalized.exclude_paths.join(","),
            );
        }
        if !normalized.include_extensions.is_empty() {
            map.insert(
                "repo_scope_include_extensions".to_string(),
                normalized.include_extensions.join(","),
            );
        }
        if !normalized.exclude_extensions.is_empty() {
            map.insert(
                "repo_scope_exclude_extensions".to_string(),
                normalized.exclude_extensions.join(","),
            );
        }

        map
    }

    /// Serialize to a compact JSON string for storage or logging.
    ///
    /// Returns a JSON representation of the repo scope configuration,
    /// useful for structured logging or database storage.
    pub fn to_json(&self) -> Result<String> {
        let normalized = self.normalized();
        serde_json::to_string(&normalized).map_err(AosError::Serialization)
    }

    /// Create a RepoScopeConfig from a JSON string.
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(AosError::Serialization)
    }
}

/// Stream output format for progress events during ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StreamFormat {
    /// JSON Lines format for machine parsing
    Json,
    /// Human-readable text format
    #[default]
    Text,
}

impl StreamFormat {
    /// Parse from string (case-insensitive)
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "json" | "jsonl" => Self::Json,
            _ => Self::Text,
        }
    }
}

/// Configuration for streaming progress events during ingestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Whether streaming is enabled
    pub enabled: bool,
    /// Output format for events
    pub format: StreamFormat,
    /// Minimum interval between events in milliseconds (0 = every event)
    pub interval_ms: u64,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self::disabled()
    }
}

impl StreamConfig {
    /// Create a new enabled stream config
    pub fn new(format: StreamFormat, interval_ms: u64) -> Self {
        Self {
            enabled: true,
            format,
            interval_ms,
        }
    }

    /// Create a disabled stream config
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            format: StreamFormat::Text,
            interval_ms: 0,
        }
    }
}

/// Metadata overrides for codebase scope.
///
/// Allows CLI or CI/CD to override auto-detected repository metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodebaseScopeMetadata {
    /// Override repository name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Override repository slug (normalized)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    /// Override repository identifier (normalized)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// Override branch name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Override commit SHA
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Override scan root path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_root: Option<String>,
    /// Override remote URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
}

impl CodebaseScopeMetadata {
    /// Check if any overrides are configured
    pub fn has_overrides(&self) -> bool {
        let normalized = self.normalized();
        normalized.repo.is_some()
            || normalized.repo_slug.is_some()
            || normalized.repo_id.is_some()
            || normalized.branch.is_some()
            || normalized.commit.is_some()
            || normalized.scan_root.is_some()
            || normalized.remote_url.is_some()
    }

    /// Normalize override values for deterministic storage.
    fn normalized(&self) -> Self {
        Self {
            repo: normalize_scope_optional(&self.repo),
            repo_slug: normalize_scope_optional_slug(&self.repo_slug),
            repo_id: normalize_scope_optional_repo_id(&self.repo_id),
            branch: normalize_scope_optional(&self.branch),
            commit: normalize_scope_optional_commit(&self.commit),
            scan_root: normalize_scope_optional_scan_root(&self.scan_root),
            remote_url: normalize_scope_optional(&self.remote_url),
        }
    }

    /// Convert to metadata key-value pairs for manifest storage.
    pub fn to_metadata_map(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        let normalized = self.normalized();

        if let Some(ref repo) = normalized.repo {
            map.insert("scope_repo".to_string(), repo.clone());
        }
        if let Some(ref repo_slug) = normalized.repo_slug {
            map.insert("repo_slug".to_string(), repo_slug.clone());
            map.insert("scope_repo_slug".to_string(), repo_slug.clone());
        }
        if let Some(ref repo_id) = normalized.repo_id {
            map.insert("scope_repo_id".to_string(), repo_id.clone());
        }
        if let Some(ref branch) = normalized.branch {
            map.insert("scope_branch".to_string(), branch.clone());
        }
        if let Some(ref commit) = normalized.commit {
            map.insert("scope_commit".to_string(), commit.clone());
        }
        if let Some(ref scan_root) = normalized.scan_root {
            map.insert("scope_scan_root".to_string(), scan_root.clone());
        }
        if let Some(ref remote_url) = normalized.remote_url {
            map.insert("scope_remote_url".to_string(), remote_url.clone());
        }

        map
    }
}

/// Adapter scope configuration for codebase ingestion.
///
/// Defines the scope boundaries for the adapter, controlling which codebase
/// context the adapter is trained on and how it should be identified.
/// This enables fine-grained control over adapter isolation, sharing, and
/// hierarchical organization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterScopeConfig {
    /// Scope identifier (e.g., "org/repo", "workspace:project", "tenant:team")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    /// Scope type classification (e.g., "repository", "workspace", "tenant", "global")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_type: Option<String>,
    /// Parent scope for hierarchical scope relationships
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_scope: Option<String>,
    /// Namespace within the scope (e.g., module path, package name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    /// Access visibility level (e.g., "private", "internal", "public")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
    /// Organization or team owner
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Additional scope-specific tags
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

const DEFAULT_ADAPTER_SCOPE_VISIBILITY: &str = "private";

impl AdapterScopeConfig {
    /// Create a new adapter scope with the given scope ID
    pub fn new(scope_id: impl Into<String>) -> Self {
        Self {
            scope_id: Some(scope_id.into()),
            visibility: Some(DEFAULT_ADAPTER_SCOPE_VISIBILITY.to_string()),
            ..Default::default()
        }
    }

    /// Create a repository-scoped adapter configuration
    pub fn repository(repo_id: impl Into<String>) -> Self {
        Self {
            scope_id: Some(repo_id.into()),
            scope_type: Some("repository".to_string()),
            visibility: Some(DEFAULT_ADAPTER_SCOPE_VISIBILITY.to_string()),
            ..Default::default()
        }
    }

    /// Create a workspace-scoped adapter configuration
    pub fn workspace(workspace_id: impl Into<String>) -> Self {
        Self {
            scope_id: Some(workspace_id.into()),
            scope_type: Some("workspace".to_string()),
            visibility: Some(DEFAULT_ADAPTER_SCOPE_VISIBILITY.to_string()),
            ..Default::default()
        }
    }

    /// Create a tenant-scoped adapter configuration
    pub fn tenant(tenant_id: impl Into<String>) -> Self {
        Self {
            scope_id: Some(tenant_id.into()),
            scope_type: Some("tenant".to_string()),
            visibility: Some(DEFAULT_ADAPTER_SCOPE_VISIBILITY.to_string()),
            ..Default::default()
        }
    }

    /// Set the scope type
    pub fn with_type(mut self, scope_type: impl Into<String>) -> Self {
        self.scope_type = Some(scope_type.into());
        self
    }

    /// Set the parent scope
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent_scope = Some(parent.into());
        self
    }

    /// Set the namespace
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Set the visibility level
    pub fn with_visibility(mut self, visibility: impl Into<String>) -> Self {
        self.visibility = Some(visibility.into());
        self
    }

    /// Set the owner
    pub fn with_owner(mut self, owner: impl Into<String>) -> Self {
        self.owner = Some(owner.into());
        self
    }

    /// Add tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Check if any scope configuration is present
    pub fn has_scope(&self) -> bool {
        self.scope_id.is_some()
            || self.scope_type.is_some()
            || self.parent_scope.is_some()
            || self.namespace.is_some()
            || self.visibility.is_some()
            || self.owner.is_some()
            || !self.tags.is_empty()
    }

    /// Convert scope config to metadata key-value pairs for storage
    pub fn to_metadata(&self) -> BTreeMap<String, String> {
        let mut metadata = BTreeMap::new();
        if let Some(ref id) = self.scope_id {
            metadata.insert("adapter_scope_id".to_string(), id.clone());
        }
        if let Some(ref t) = self.scope_type {
            metadata.insert("adapter_scope_type".to_string(), t.clone());
        }
        if let Some(ref p) = self.parent_scope {
            metadata.insert("adapter_scope_parent".to_string(), p.clone());
        }
        if let Some(ref n) = self.namespace {
            metadata.insert("adapter_scope_namespace".to_string(), n.clone());
        }
        if let Some(ref v) = self.visibility {
            metadata.insert("adapter_scope_visibility".to_string(), v.clone());
        }
        if let Some(ref o) = self.owner {
            metadata.insert("adapter_scope_owner".to_string(), o.clone());
        }
        if !self.tags.is_empty() {
            let mut tags = self.tags.clone();
            maybe_stable_sort(&mut tags);
            metadata.insert("adapter_scope_tags".to_string(), tags.join(","));
        }
        metadata
    }

    /// Derive a scope config from repository information
    pub fn from_repo(repo_slug: &str, remote_url: Option<&str>) -> Self {
        let scope_id = if let Some(url) = remote_url {
            normalize_repo_id(url)
        } else {
            format!("repo:{}", repo_slug)
        };
        Self {
            scope_id: Some(scope_id),
            scope_type: Some("repository".to_string()),
            visibility: Some(DEFAULT_ADAPTER_SCOPE_VISIBILITY.to_string()),
            ..Default::default()
        }
    }
}

/// Dataset lineage information for provenance tracking.
///
/// Links adapters to their training data sources.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetLineageInfo {
    /// Parent dataset ID for single-parent lineage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_dataset_id: Option<String>,
    /// Human-readable label for the lineage relationship
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_label: Option<String>,
    /// List of source dataset IDs this was derived from
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    /// Explicit version string
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Additional key-value metadata
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
}

impl DatasetLineageInfo {
    /// Check if any lineage information is present
    pub fn has_lineage(&self) -> bool {
        self.parent_dataset_id.is_some()
            || self.lineage_label.is_some()
            || !self.derived_from.is_empty()
            || self.version.is_some()
            || !self.metadata.is_empty()
    }

    fn normalized(&self) -> Self {
        let mut derived_from = self.derived_from.clone();
        maybe_stable_sort(&mut derived_from);
        Self {
            parent_dataset_id: self.parent_dataset_id.clone(),
            lineage_label: self.lineage_label.clone(),
            derived_from,
            version: self.version.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

/// Git commit metadata captured during code ingestion.
///
/// Provides full commit provenance including author information, timestamps,
/// and message for traceability and reproducibility of training runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommitMetadata {
    /// Full 40-character commit SHA
    pub sha: String,
    /// Short SHA (first 8 characters) for display purposes
    pub short_sha: String,
    /// Commit author name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_name: Option<String>,
    /// Commit author email
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author_email: Option<String>,
    /// Commit timestamp in ISO 8601 format (UTC)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_date: Option<String>,
    /// Unix timestamp of the commit (seconds since epoch)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_timestamp: Option<i64>,
    /// First line of the commit message (summary)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_summary: Option<String>,
    /// Full commit message body
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_body: Option<String>,
    /// Committer name (may differ from author in rebased commits)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_name: Option<String>,
    /// Committer email
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committer_email: Option<String>,
    /// Parent commit SHA(s) - empty for initial commits
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parent_shas: Vec<String>,
}

impl CommitMetadata {
    /// Create a new CommitMetadata with just the SHA
    pub fn new(sha: String) -> Self {
        let short_sha = sha.get(0..8).unwrap_or(&sha).to_string();
        Self {
            sha,
            short_sha,
            ..Default::default()
        }
    }

    /// Check if full metadata is available (beyond just SHA)
    pub fn has_full_metadata(&self) -> bool {
        self.author_name.is_some() || self.commit_date.is_some() || self.message_summary.is_some()
    }

    /// Convert to a deterministic map for metadata storage
    pub fn to_metadata_map(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert("commit_sha".to_string(), self.sha.clone());
        map.insert("commit_short_sha".to_string(), self.short_sha.clone());

        if let Some(ref name) = self.author_name {
            map.insert("commit_author_name".to_string(), name.clone());
        }
        if let Some(ref email) = self.author_email {
            map.insert("commit_author_email".to_string(), email.clone());
        }
        if let Some(ref date) = self.commit_date {
            map.insert("commit_date".to_string(), date.clone());
        }
        if let Some(ts) = self.commit_timestamp {
            map.insert("commit_timestamp".to_string(), ts.to_string());
        }
        if let Some(ref summary) = self.message_summary {
            map.insert("commit_message_summary".to_string(), summary.clone());
        }
        if let Some(ref body) = self.message_body {
            map.insert("commit_message_body".to_string(), body.clone());
        }
        if let Some(ref committer) = self.committer_name {
            map.insert("commit_committer_name".to_string(), committer.clone());
        }
        if let Some(ref committer_email) = self.committer_email {
            map.insert(
                "commit_committer_email".to_string(),
                committer_email.clone(),
            );
        }
        if !self.parent_shas.is_empty() {
            let mut parent_shas = self.parent_shas.clone();
            maybe_stable_sort(&mut parent_shas);
            map.insert("commit_parent_shas".to_string(), parent_shas.join(","));
        }

        map
    }
}

fn build_commit_metadata(commit: &git2::Commit<'_>) -> CommitMetadata {
    let commit_sha = commit.id().to_string();
    let summary = commit.summary().unwrap_or("").to_string();
    let author = commit.author();
    let committer = commit.committer();
    let commit_time = commit.time();
    let commit_timestamp = commit_time.seconds();
    let commit_date = Utc
        .timestamp_opt(commit_timestamp, 0)
        .single()
        .map(|dt| dt.to_rfc3339());
    let mut parent_shas: Vec<String> = commit.parent_ids().map(|id| id.to_string()).collect();
    maybe_stable_sort(&mut parent_shas);

    CommitMetadata {
        sha: commit_sha.clone(),
        short_sha: commit_sha.get(0..8).unwrap_or(&commit_sha).to_string(),
        author_name: Some(author.name().unwrap_or("").to_string()),
        author_email: Some(author.email().unwrap_or("").to_string()),
        commit_date,
        commit_timestamp: Some(commit_timestamp),
        message_summary: Some(summary),
        message_body: commit.message().map(|s| s.to_string()),
        committer_name: Some(committer.name().unwrap_or("").to_string()),
        committer_email: Some(committer.email().unwrap_or("").to_string()),
        parent_shas,
    }
}

fn resolve_commit_metadata(repo_root: &Path, commit_ref: &str) -> Result<CommitMetadata> {
    let repo = Repository::open(repo_root).map_err(|e| {
        AosError::Git(format!(
            "Failed to open repository for commit override: {}",
            e
        ))
    })?;
    let object = repo.revparse_single(commit_ref).map_err(|e| {
        AosError::Git(format!(
            "Failed to resolve commit '{}' in repository: {}",
            commit_ref, e
        ))
    })?;
    let commit = object.peel_to_commit().map_err(|e| {
        AosError::Git(format!(
            "Failed to read commit '{}' in repository: {}",
            commit_ref, e
        ))
    })?;
    Ok(build_commit_metadata(&commit))
}

/// Request to train an adapter directly from a codebase
#[derive(Debug, Clone)]
pub struct CodeIngestionRequest {
    pub source: CodeIngestionSource,
    pub tokenizer_path: PathBuf,
    pub training_config: TrainingConfig,
    pub dataset: CodeDatasetConfig,
    pub output_dir: PathBuf,
    pub adapter_id: Option<String>,
    pub base_model: String,
    pub register: bool,
    pub tier: i32,
    pub repo_id: Option<String>,
    pub project_name: Option<String>,
    pub seed: Option<u64>,
    /// Optional determinism config applied for this ingestion run.
    pub determinism_config: Option<CoreDeterminismConfig>,
    /// Human-readable name for the ingestion session (e.g., "nightly-build", "pr-123")
    pub session_name: Option<String>,
    /// Arbitrary tags for categorizing the session (e.g., ["ci", "production"])
    pub session_tags: Option<Vec<String>>,
    /// Unique identifier for this ingestion session, used for correlation across pipeline stages
    pub session_id: Option<Uuid>,
    /// Repository scope filtering configuration (include/exclude paths and extensions)
    pub repo_scope: Option<RepoScopeConfig>,
    /// Scan root paths within the repository (relative or absolute).
    pub scan_roots: Vec<String>,
    /// Streaming configuration for real-time progress updates
    pub stream: Option<StreamConfig>,
    /// Codebase scope metadata (repo, branch, commit, etc.)
    pub scope_metadata: Option<CodebaseScopeMetadata>,
    /// Dataset lineage information for provenance tracking
    pub lineage: Option<DatasetLineageInfo>,
    /// Adapter scope configuration for controlling codebase adapter boundaries
    pub adapter_scope: Option<AdapterScopeConfig>,
    /// Repository slug override for adapter naming and provenance tracking.
    /// Used to generate adapter IDs in format: code.<repo_slug>.<commit>
    /// If not provided, auto-derived from repository name using normalize_repo_slug.
    pub repo_slug: Option<String>,
}

/// Result of a code ingestion training run
#[derive(Debug, Clone)]
pub struct CodeIngestionResult {
    pub adapter_id: String,
    pub repo_name: String,
    pub repo_slug: String,
    pub repo_identifier: String,
    /// Git branch name at time of ingestion (e.g., "main", "feature/xyz")
    pub branch: Option<String>,
    pub commit_sha: String,
    pub short_commit_sha: String,
    /// Full commit metadata including author, date, and message
    pub commit_metadata: CommitMetadata,
    /// Absolute path to the git repository root
    pub repo_root_path: PathBuf,
    /// Absolute path to the scan root (the directory being scanned)
    pub scan_root_path: PathBuf,
    /// Scan root path relative to repo root (empty string if same as repo root)
    pub scan_root_relative: String,
    pub dataset_examples: usize,
    pub positive_examples: usize,
    pub negative_examples: usize,
    pub dataset_hash: String,
    pub aos_path: PathBuf,
    pub aos_hash_b3: String,
    pub registry_id: Option<String>,
    /// Adapter scope that was applied during ingestion
    pub adapter_scope: Option<AdapterScopeConfig>,
    /// Repository scope configuration used during ingestion (file/path filters applied)
    pub repo_scope: Option<RepoScopeConfig>,
}

#[derive(Debug, Clone, Serialize)]
struct ScanRootEntry {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
}

#[derive(Debug, Clone)]
struct MetadataPaths {
    repo_root: String,
    scan_root: String,
    scan_root_relative: String,
}

struct DatasetArtifact {
    file_hash_b3: String,
    storage_path: String,
}

fn should_normalize_metadata_paths() -> bool {
    should_use_stable_ordering() || is_strict_determinism_mode()
}

fn resolve_metadata_paths(prepared_repo: &PreparedRepo) -> MetadataPaths {
    let scan_root_relative = normalize_scope_scan_root(&prepared_repo.scan_root_relative);
    let scan_root = if should_normalize_metadata_paths() {
        if scan_root_relative.is_empty() {
            ".".to_string()
        } else {
            scan_root_relative.clone()
        }
    } else {
        prepared_repo.scan_root.display().to_string()
    };
    let repo_root = if should_normalize_metadata_paths() {
        ".".to_string()
    } else {
        prepared_repo.root.display().to_string()
    };

    MetadataPaths {
        repo_root,
        scan_root,
        scan_root_relative,
    }
}

fn normalize_scan_root_list(scan_roots: &[String]) -> Vec<String> {
    let mut roots: Vec<String> = scan_roots
        .iter()
        .map(|root| normalize_scope_scan_root(root))
        .filter(|root| !root.trim().is_empty())
        .collect();
    roots.sort();
    roots.dedup();
    roots
}

fn build_scan_root_entries(
    prepared_repo: &PreparedRepo,
    repo_scope: Option<&RepoScopeConfig>,
) -> Vec<ScanRootEntry> {
    let mut candidates = Vec::new();
    if let Some(scope) = repo_scope {
        candidates.extend(
            scope
                .include_paths
                .iter()
                .map(|path| normalize_scope_scan_root(path)),
        );
    }
    candidates.sort();
    candidates.retain(|path| !path.trim().is_empty());
    candidates.dedup();

    if !candidates.is_empty() {
        let label = if candidates.len() == 1 {
            Some("primary".to_string())
        } else {
            None
        };
        return candidates
            .into_iter()
            .map(|path| ScanRootEntry {
                path,
                label: label.clone(),
            })
            .collect();
    }

    let primary_path = if should_normalize_metadata_paths() {
        let normalized = normalize_scope_scan_root(&prepared_repo.scan_root_relative);
        if normalized.is_empty() {
            ".".to_string()
        } else {
            normalized
        }
    } else if prepared_repo.scan_root_relative.is_empty() {
        prepared_repo.scan_root.display().to_string()
    } else {
        prepared_repo.scan_root_relative.clone()
    };

    vec![ScanRootEntry {
        path: primary_path,
        label: Some("primary".to_string()),
    }]
}

/// Primary entry point for the ingestion pipeline
#[derive(Debug, Default, Clone)]
pub struct CodeIngestionPipeline;

impl CodeIngestionPipeline {
    pub fn new() -> Self {
        Self
    }

    /// Execute ingestion + training end-to-end
    pub async fn run(&self, request: CodeIngestionRequest) -> Result<CodeIngestionResult> {
        let _determinism_guard = request
            .determinism_config
            .clone()
            .map(DeterminismConfigGuard::new);
        let normalized_repo_slug = request
            .repo_slug
            .as_ref()
            .map(|slug| slug.trim())
            .filter(|slug| !slug.is_empty())
            .map(normalize_repo_slug);
        let mut normalized_repo_scope = request.repo_scope.as_ref().map(|scope| scope.normalized());
        let normalized_scan_roots = normalize_scan_root_list(&request.scan_roots);
        if !normalized_scan_roots.is_empty() {
            let mut scope = normalized_repo_scope.unwrap_or_default();
            scope.include_paths = scope
                .include_paths
                .iter()
                .map(|path| normalize_scope_scan_root(path))
                .filter(|path| !path.trim().is_empty())
                .collect();
            scope
                .include_paths
                .extend(normalized_scan_roots.iter().cloned());
            scope.include_paths.sort();
            scope.include_paths.retain(|path| !path.trim().is_empty());
            scope.include_paths.dedup();
            normalized_repo_scope = Some(scope);
        }
        let normalized_scope_overrides = request
            .scope_metadata
            .as_ref()
            .map(|scope| scope.normalized());
        let normalized_lineage = request.lineage.as_ref().map(|lineage| lineage.normalized());

        let mut prepared_repo = prepare_repo(&request.source, normalized_repo_slug.clone()).await?;
        let naming_repo_slug = normalized_repo_slug
            .clone()
            .unwrap_or_else(|| prepared_repo.repo_slug.clone());
        if let Some(scope_overrides) = &normalized_scope_overrides {
            apply_scope_metadata_overrides(
                &mut prepared_repo,
                scope_overrides,
                normalized_repo_slug.as_deref(),
            );
        }
        let explicit_scan_root = normalized_scope_overrides
            .as_ref()
            .and_then(|meta| meta.scan_root.as_ref())
            .filter(|value| !value.trim().is_empty());
        if explicit_scan_root.is_none() && normalized_scan_roots.len() == 1 {
            apply_scan_root_override(&mut prepared_repo, &normalized_scan_roots[0]);
        }
        validate_scope_override_consistency(
            &prepared_repo,
            normalized_repo_slug.as_deref(),
            normalized_scope_overrides.as_ref(),
        )?;
        let project_name = request
            .project_name
            .clone()
            .unwrap_or_else(|| prepared_repo.repo_name.clone());

        let adapter_id = request.adapter_id.clone().unwrap_or_else(|| {
            format!(
                "code.{}.{}",
                naming_repo_slug,
                prepared_repo.short_sha().to_ascii_lowercase()
            )
        });
        let adapter_id = normalize_codebase_adapter_id(&adapter_id)?;
        validate_codebase_adapter_id(&adapter_id)?;

        let repo_id_override = request
            .repo_id
            .as_ref()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(|id| id.to_string())
            .or_else(|| {
                normalized_scope_overrides
                    .as_ref()
                    .and_then(|meta| meta.repo_id.as_ref())
                    .map(|id| id.trim())
                    .filter(|id| !id.is_empty())
                    .map(|id| id.to_string())
            });
        let mut repo_identifier = if let Some(ref repo_id) = repo_id_override {
            normalize_repo_id(repo_id)
        } else if let Some(ref remote_url) = prepared_repo.remote_url {
            normalize_repo_id(remote_url)
        } else {
            normalize_repo_id(&format!("repo:{}", prepared_repo.repo_slug))
        };
        if repo_id_override.is_none() && !prepared_repo.scan_root_relative.is_empty() {
            let joined = format!(
                "{}/{}",
                repo_identifier.trim_end_matches('/'),
                prepared_repo.scan_root_relative
            );
            repo_identifier = normalize_repo_id(&joined);
        }

        let mut adapter_scope = request.adapter_scope.clone().unwrap_or_else(|| {
            AdapterScopeConfig::from_repo(
                &prepared_repo.repo_slug,
                prepared_repo.remote_url.as_deref(),
            )
        });
        if request.adapter_scope.is_none()
            || adapter_scope
                .scope_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            adapter_scope.scope_id = Some(repo_identifier.clone());
        }
        let adapter_scope_id = adapter_scope
            .scope_id
            .clone()
            .unwrap_or_else(|| repo_identifier.clone());

        info!(
            repo_root = %prepared_repo.root.display(),
            scan_root = %prepared_repo.scan_root.display(),
            scan_root_relative = %prepared_repo.scan_root_relative,
            commit = %prepared_repo.commit_sha,
            project = %project_name,
            adapter_id = %adapter_id,
            repo_identifier = %repo_identifier,
            "Starting code ingestion pipeline",
        );

        // Build CodeGraph from the configured scan root
        let codegraph = CodeGraph::from_directory(&prepared_repo.scan_root, None).await?;
        debug!(
            symbol_count = codegraph.symbols.len(),
            "Parsed repository into CodeGraph"
        );

        let metadata_paths = resolve_metadata_paths(&prepared_repo);
        let samples = build_symbol_samples(
            &codegraph,
            &prepared_repo,
            &metadata_paths,
            &project_name,
            &request.dataset,
            &repo_identifier,
            normalized_repo_scope.as_ref(),
        );
        if samples.is_empty() {
            return Err(AosError::Training(
                "Code ingestion did not produce any training samples".to_string(),
            ));
        }

        let sample_stats = SampleStats::from(samples.as_slice());
        let dataset_hash = compute_dataset_hash(&samples);
        let training_config_hash = compute_training_config_hash(&request.training_config);
        let training_config_hash_inputs =
            build_training_config_hash_inputs(&request.training_config)?;
        let training_config_hash_inputs_json =
            serde_json::to_string(&training_config_hash_inputs).map_err(AosError::Serialization)?;
        let seed_inputs = SeedInputs {
            commit_sha: &prepared_repo.commit_sha,
            dataset_hash_b3: &dataset_hash,
            training_config_hash: &training_config_hash,
            base_model_id: &request.base_model,
            repo_slug: &naming_repo_slug,
        };
        let seed_inputs_json = serialize_seed_inputs(&seed_inputs)?;
        let derived_seed = derive_seed_from_inputs(&seed_inputs_json)?;
        let seed_override = request.seed.or_else(|| {
            request
                .training_config
                .determinism
                .as_ref()
                .and_then(|d| d.seed)
        });
        let seed_source = if request.seed.is_some() {
            "explicit"
        } else if request
            .training_config
            .determinism
            .as_ref()
            .and_then(|d| d.seed)
            .is_some()
        {
            "config"
        } else {
            "derived"
        };
        let seed = seed_override.unwrap_or(derived_seed);

        let tokenizer = QwenTokenizer::from_file(&request.tokenizer_path).map_err(|e| {
            AosError::Training(format!(
                "Failed to load tokenizer {}: {}",
                request.tokenizer_path.display(),
                e
            ))
        })?;
        let training_examples = encode_samples(&tokenizer, &samples)?;

        let stats = SampleStats::from_examples(&training_examples);
        info!(
            samples = training_examples.len(),
            positives = stats.positive,
            negatives = stats.negative,
            hash = %dataset_hash,
            "Constructed training dataset"
        );

        let scan_root_entries =
            build_scan_root_entries(&prepared_repo, normalized_repo_scope.as_ref());
        let repo_scope_metadata = normalized_repo_scope.clone().unwrap_or_default();
        let primary_scan_root = scan_root_entries
            .iter()
            .find(|entry| entry.label.as_deref() == Some("primary"))
            .or_else(|| scan_root_entries.first())
            .map(|entry| entry.path.clone())
            .unwrap_or_else(|| {
                if metadata_paths.scan_root_relative.is_empty() {
                    metadata_paths.scan_root.clone()
                } else {
                    metadata_paths.scan_root_relative.clone()
                }
            });

        let mut dataset_id: Option<String> = None;
        let mut dataset_version_id: Option<String> = None;
        let mut db: Option<Db> = None;
        let dataset_artifact = if request.register {
            Some(store_codebase_dataset_artifact(&samples, &dataset_hash).await?)
        } else {
            None
        };

        if request.register {
            let db_instance = Db::connect_env().await?;
            db_instance.migrate().await?;
            let dataset_artifact = dataset_artifact.as_ref().ok_or_else(|| {
                AosError::Training("Dataset artifact missing for registered ingestion".to_string())
            })?;
            let dataset_file_hash = dataset_artifact.file_hash_b3.as_str();
            let dataset_storage_path = dataset_artifact.storage_path.as_str();

            let seed_inputs_value = serde_json::from_str::<serde_json::Value>(&seed_inputs_json)
                .unwrap_or(serde_json::Value::Null);
            let mut session_tags = request.session_tags.clone().unwrap_or_default();
            maybe_stable_sort(&mut session_tags);
            let dataset_metadata_json = serde_json::to_string(&serde_json::json!({
                "dataset_hash_b3": dataset_hash,
                "dataset_file_hash_b3": dataset_file_hash,
                "training_config_hash": training_config_hash,
                "training_config_hash_inputs": training_config_hash_inputs,
                "dataset_examples": training_examples.len(),
                "dataset_positive_examples": stats.positive,
                "dataset_negative_examples": stats.negative,
                "seed_inputs": seed_inputs_value,
                "project": project_name,
                "repo_branch": prepared_repo.branch.as_deref(),
                "repo_commit": prepared_repo.commit_sha.clone(),
                "commit_short_sha": prepared_repo.short_sha(),
                "commit_metadata": &prepared_repo.commit_metadata,
                "repo_identifier": repo_identifier,
                "scan_roots": &scan_root_entries,
                "repo_root_path": &metadata_paths.repo_root,
                "repo_path": &metadata_paths.scan_root,
                "scan_root_path": &metadata_paths.scan_root,
                "session_id": request.session_id.as_ref().map(|id| id.to_string()),
                "session_name": request.session_name.as_ref(),
                "session_tags": if session_tags.is_empty() { None } else { Some(session_tags) },
                "dataset_config": {
                    "max_symbols": request.dataset.max_symbols,
                    "include_private": request.dataset.include_private,
                    "positive_weight": request.dataset.positive_weight,
                    "negative_weight": request.dataset.negative_weight,
                },
                "repo_scope": &repo_scope_metadata,
                "lineage": normalized_lineage.as_ref(),
            }))
            .map_err(AosError::Serialization)?;

            let scan_root_path = metadata_paths.scan_root.clone();
            let (ds_id, version_id) = db_instance
                .create_codebase_dataset_from_repo_with_hashes(
                    &prepared_repo.repo_name,
                    prepared_repo.remote_url.as_deref(),
                    &scan_root_path,
                    &prepared_repo.commit_sha,
                    prepared_repo.branch.as_deref(),
                    Some(&prepared_repo.repo_slug),
                    &dataset_hash,
                    dataset_file_hash,
                    dataset_storage_path,
                    Some(&dataset_metadata_json),
                    None,
                    None,
                )
                .await?;

            let scope_config_json = normalized_repo_scope
                .as_ref()
                .filter(|scope| scope.has_filters())
                .and_then(|scope| scope.to_json().ok());

            let mut additional_inputs = serde_json::Map::new();
            additional_inputs.insert(
                "project_name".to_string(),
                serde_json::Value::String(project_name.clone()),
            );
            additional_inputs.insert(
                "repo_name".to_string(),
                serde_json::Value::String(prepared_repo.repo_name.clone()),
            );
            if !metadata_paths.scan_root_relative.is_empty() {
                additional_inputs.insert(
                    "scan_root_relative".to_string(),
                    serde_json::Value::String(metadata_paths.scan_root_relative.clone()),
                );
            }
            if scan_root_entries.len() > 1 {
                if let Ok(value) = serde_json::to_value(&scan_root_entries) {
                    additional_inputs.insert("scan_roots".to_string(), value);
                }
            }
            if let Some(session_id) = request.session_id.as_ref() {
                additional_inputs.insert(
                    "session_id".to_string(),
                    serde_json::Value::String(session_id.to_string()),
                );
            }
            if let Some(session_name) = request.session_name.as_ref() {
                additional_inputs.insert(
                    "session_name".to_string(),
                    serde_json::Value::String(session_name.clone()),
                );
            }
            if let Some(tags) = request.session_tags.as_ref() {
                let mut tags = tags.clone();
                maybe_stable_sort(&mut tags);
                additional_inputs.insert(
                    "session_tags".to_string(),
                    serde_json::Value::Array(
                        tags.into_iter().map(serde_json::Value::String).collect(),
                    ),
                );
            }
            if let Some(lineage) = normalized_lineage.as_ref() {
                if let Ok(value) = serde_json::to_value(lineage) {
                    additional_inputs.insert("lineage".to_string(), value);
                }
            }
            let additional_inputs_json = if additional_inputs.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_string(&serde_json::Value::Object(additional_inputs))
                        .map_err(AosError::Serialization)?,
                )
            };

            let mut hash_inputs = CreateDatasetHashInputsParams::new(
                dataset_hash.clone(),
                samples.len() as i64,
                sample_stats.positive as i64,
                sample_stats.negative as i64,
            );
            hash_inputs.dataset_id = Some(ds_id.clone());
            hash_inputs.repo_id = Some(repo_identifier.clone());
            hash_inputs.repo_slug = Some(prepared_repo.repo_slug.clone());
            hash_inputs.commit_sha = Some(prepared_repo.commit_sha.clone());
            hash_inputs.branch = prepared_repo.branch.clone();
            hash_inputs.scan_root_path = Some(scan_root_path.clone());
            hash_inputs.remote_url = prepared_repo.remote_url.clone();
            hash_inputs.max_symbols = Some(request.dataset.max_symbols as i64);
            hash_inputs.include_private = Some(request.dataset.include_private);
            hash_inputs.positive_weight = Some(f64::from(request.dataset.positive_weight));
            hash_inputs.negative_weight = Some(f64::from(request.dataset.negative_weight));
            hash_inputs.scope_config_json = scope_config_json;
            hash_inputs.additional_inputs_json = additional_inputs_json;

            db_instance.record_dataset_hash_inputs(&hash_inputs).await?;

            if let Some(session_id) = request.session_id.as_ref() {
                let session_id = session_id.to_string();
                db_instance
                    .ensure_dataset_collection_session(
                        &session_id,
                        request.session_name.as_deref(),
                        request.session_tags.as_deref(),
                        None,
                    )
                    .await?;
                db_instance
                    .link_dataset_to_collection_session(&session_id, &ds_id, Some("created"), None)
                    .await?;
            }

            dataset_id = Some(ds_id);
            dataset_version_id = Some(version_id);
            db = Some(db_instance);
        }

        if let (Some(db_ref), Some(ds_id)) = (db.as_ref(), dataset_id.as_ref()) {
            let row_inputs = build_codebase_dataset_row_inputs(&samples);
            if !row_inputs.is_empty() {
                let session_id = request.session_id.as_ref().map(|id| id.to_string());
                let inserted = db_ref
                    .insert_codebase_dataset_rows_for_training_config(
                        ds_id,
                        dataset_version_id.as_deref(),
                        session_id.as_deref(),
                        Some(&prepared_repo.repo_name),
                        Some(&prepared_repo.repo_slug),
                        Some(&repo_identifier),
                        Some(&project_name),
                        Some(&prepared_repo.commit_sha),
                        &training_config_hash,
                        &row_inputs,
                        None,
                    )
                    .await?;
                info!(
                    dataset_id = %ds_id,
                    inserted,
                    "Inserted codebase dataset rows for ingestion run"
                );
            }
        }

        let stream_enabled = request
            .stream
            .as_ref()
            .map(|stream| stream.enabled)
            .unwrap_or(false);

        let mut training_config = request.training_config.clone();
        let mut determinism = training_config.determinism.unwrap_or_default();
        determinism.seed = Some(seed);
        if let Some(version_id) = dataset_version_id.as_ref() {
            determinism.dataset_version_id = Some(version_id.clone());
        }
        training_config.determinism = Some(determinism);

        let mut trainer = MicroLoRATrainer::new(training_config.clone())?;
        info!(seed, seed_source, "Using deterministic training seed");

        let mut training_result = trainer.train(&training_examples).await?;
        training_result.adapter_id = adapter_id.clone();

        let mut metadata = BTreeMap::new();
        metadata.insert("repo_name".to_string(), prepared_repo.repo_name.clone());
        metadata.insert("repo_slug".to_string(), prepared_repo.repo_slug.clone());
        metadata.insert("codebase_scope".to_string(), repo_identifier.clone());
        metadata.insert("scope".to_string(), adapter_scope_id.clone());
        metadata.insert("repo_identifier".to_string(), repo_identifier.clone());
        metadata.insert("scope_repo_id".to_string(), repo_identifier.clone());
        metadata.insert("repo_commit".to_string(), prepared_repo.commit_sha.clone());
        metadata.insert(
            "repo_short_commit".to_string(),
            prepared_repo.short_sha().to_string(),
        );
        metadata.insert(
            "repo_root_path".to_string(),
            metadata_paths.repo_root.clone(),
        );
        metadata.insert("repo_path".to_string(), metadata_paths.scan_root.clone());
        // Record scan root path and its relative path to repo root
        metadata.insert(
            "scan_root_path".to_string(),
            metadata_paths.scan_root.clone(),
        );
        if !metadata_paths.scan_root_relative.is_empty() {
            metadata.insert(
                "scan_root_relative".to_string(),
                metadata_paths.scan_root_relative.clone(),
            );
        }
        if let Ok(scan_roots_json) = serde_json::to_string(&scan_root_entries) {
            metadata.insert("scan_roots".to_string(), scan_roots_json);
        }
        if let Some(remote) = &prepared_repo.remote_url {
            metadata.insert("repo_remote".to_string(), remote.clone());
        }
        // Record branch name if available
        if let Some(branch) = &prepared_repo.branch {
            metadata.insert("repo_branch".to_string(), branch.clone());
        }

        // Capture canonical scope metadata for manifest provenance
        let scope_meta = CodebaseScopeMetadata {
            repo: Some(prepared_repo.repo_name.clone()),
            repo_slug: Some(prepared_repo.repo_slug.clone()),
            repo_id: Some(repo_identifier.clone()),
            branch: prepared_repo.branch.clone(),
            commit: Some(prepared_repo.commit_sha.clone()),
            scan_root: Some(primary_scan_root.clone()),
            remote_url: prepared_repo.remote_url.clone(),
        };
        for (key, value) in scope_meta.to_metadata_map() {
            metadata.insert(key, value);
        }

        // Include repo scope metadata (always include active flag; include JSON when filtered)
        for (key, value) in repo_scope_metadata.to_metadata_map() {
            metadata.insert(key, value);
        }
        if repo_scope_metadata.has_filters() {
            if let Ok(scope_json) = repo_scope_metadata.to_json() {
                metadata.insert("repo_scope_json".to_string(), scope_json);
            }
        }
        // Include full commit metadata
        for (key, value) in prepared_repo.commit_metadata.to_metadata_map() {
            metadata.insert(key, value);
        }
        metadata.insert("dataset_hash".to_string(), dataset_hash.clone());
        metadata.insert("dataset_hash_b3".to_string(), dataset_hash.clone());
        metadata.insert(
            "training_config_hash".to_string(),
            training_config_hash.clone(),
        );
        metadata.insert(
            "training_config_hash_inputs".to_string(),
            training_config_hash_inputs_json.clone(),
        );
        metadata.insert("seed_inputs_json".to_string(), seed_inputs_json.clone());
        metadata.insert("determinism_seed".to_string(), seed.to_string());
        metadata.insert("seed_source".to_string(), seed_source.to_string());
        metadata.insert(
            "dataset_examples".to_string(),
            training_examples.len().to_string(),
        );
        metadata.insert(
            "dataset_positive_examples".to_string(),
            stats.positive.to_string(),
        );
        metadata.insert(
            "dataset_negative_examples".to_string(),
            stats.negative.to_string(),
        );
        metadata.insert("project".to_string(), project_name.clone());
        metadata.insert("base_model_id".to_string(), request.base_model.clone());
        if let Some(ref ds_id) = dataset_id {
            metadata.insert("dataset_id".to_string(), ds_id.clone());
        }
        if let Some(ref version_id) = dataset_version_id {
            if let Ok(json) = serde_json::to_string(&vec![version_id]) {
                metadata.insert("dataset_version_ids".to_string(), json);
            }
            metadata.insert("data_spec_hash".to_string(), dataset_hash.clone());
        }
        if dataset_version_id.is_some() {
            metadata.insert("synthetic_mode".to_string(), "false".to_string());
            metadata.insert("data_lineage_mode".to_string(), "versioned".to_string());
        } else {
            metadata.insert("synthetic_mode".to_string(), "true".to_string());
            metadata.insert("data_lineage_mode".to_string(), "synthetic".to_string());
        }
        metadata.insert(
            "generator".to_string(),
            "code_ingestion_pipeline".to_string(),
        );
        metadata.insert(
            "category".to_string(),
            CODE_INGESTION_ADAPTER_CATEGORY.to_string(),
        );

        // Include session context metadata if provided
        if let Some(session_name) = &request.session_name {
            metadata.insert("session_name".to_string(), session_name.clone());
        }
        if let Some(session_tags) = &request.session_tags {
            let mut tags = session_tags.clone();
            maybe_stable_sort(&mut tags);
            metadata.insert("session_tags".to_string(), tags.join(","));
        }
        if let Some(session_id) = &request.session_id {
            metadata.insert("session_id".to_string(), session_id.to_string());
        }
        metadata.insert("stream_mode".to_string(), stream_enabled.to_string());
        if let Some(stream) = &request.stream {
            let format = match stream.format {
                StreamFormat::Json => "json",
                StreamFormat::Text => "text",
            };
            metadata.insert("stream_format".to_string(), format.to_string());
            metadata.insert(
                "stream_interval_ms".to_string(),
                stream.interval_ms.to_string(),
            );
        }

        validate_adapter_scope_metadata(&adapter_scope)?;

        // Include adapter scope metadata in the package
        if adapter_scope.has_scope() {
            for (key, value) in adapter_scope.to_metadata() {
                metadata.insert(key, value);
            }
        }

        let mut package_metadata: HashMap<String, String> = metadata
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if let Some(ref backend) = training_result.backend {
            package_metadata.insert("training_backend".to_string(), backend.clone());
        }
        if let Some(ref device) = training_result.backend_device {
            package_metadata.insert("training_backend_device".to_string(), device.clone());
        }

        let quantized = LoRAQuantizer::quantize_to_q15(&training_result.weights);
        let packager = AdapterPackager::new(&request.output_dir);
        let packaged = packager
            .package_aos_with_metadata(
                "default",
                &adapter_id,
                &quantized,
                &training_config,
                &request.base_model,
                package_metadata,
            )
            .await?;

        let aos_path = packaged.weights_path;
        let aos_bytes = fs::read(&aos_path).await.map_err(|e| {
            AosError::Io(format!(
                "Failed to read {} for hashing: {}",
                aos_path.display(),
                e
            ))
        })?;
        let aos_hash = blake3::hash(&aos_bytes).to_hex().to_string();

        info!(
            path = %aos_path.display(),
            hash = %aos_hash,
            "Packaged SingleFile adapter"
        );

        if let (Some(db_ref), Some(ds_id)) = (db.as_ref(), dataset_id.as_ref()) {
            db_ref.update_dataset_status(ds_id, "ready").await?;
        }

        let registry_id = if request.register {
            register_adapter(
                &adapter_id,
                &aos_hash,
                &packaged.hash_b3,
                &aos_path,
                request.tier,
                &training_config,
                &adapter_scope_id,
                &repo_identifier,
                &prepared_repo.commit_sha,
                dataset_version_id.as_deref(),
                &request.base_model,
                &prepared_repo.scan_root,
                Some(packaged.manifest.version.as_str()),
                &metadata,
            )
            .await?
        } else {
            None
        };

        Ok(CodeIngestionResult {
            adapter_id,
            repo_name: prepared_repo.repo_name.clone(),
            repo_slug: naming_repo_slug,
            repo_identifier,
            branch: prepared_repo.branch.clone(),
            commit_sha: prepared_repo.commit_sha.clone(),
            short_commit_sha: prepared_repo.short_sha().to_string(),
            commit_metadata: prepared_repo.commit_metadata.clone(),
            repo_root_path: prepared_repo.root.clone(),
            scan_root_path: prepared_repo.scan_root.clone(),
            scan_root_relative: prepared_repo.scan_root_relative.clone(),
            dataset_examples: training_examples.len(),
            positive_examples: stats.positive,
            negative_examples: stats.negative,
            dataset_hash,
            aos_path,
            aos_hash_b3: aos_hash,
            registry_id,
            adapter_scope: Some(adapter_scope),
            repo_scope: normalized_repo_scope.clone(),
        })
    }
}

struct PreparedRepo {
    /// Git repository root (the directory containing `.git`)
    root: PathBuf,
    /// Scan root path (the directory being scanned, may differ from repo root)
    scan_root: PathBuf,
    /// Scan root path relative to the repo root (empty string if same as repo root)
    scan_root_relative: String,
    repo_name: String,
    repo_slug: String,
    /// Current branch name (e.g., "main", "feature/xyz")
    branch: Option<String>,
    commit_sha: String,
    commit_summary: String,
    /// Full commit metadata including author, date, and message
    commit_metadata: CommitMetadata,
    remote_url: Option<String>,
    _temp_dir: Option<TempDir>,
}

impl PreparedRepo {
    fn short_sha(&self) -> &str {
        self.commit_metadata.short_sha.as_str()
    }
}

#[derive(Default)]
struct SymbolSample {
    prompt: String,
    response: String,
    metadata: BTreeMap<String, String>,
    weight: f32,
    qualified_name: String,
    symbol_kind: String,
    language: String,
    file_path: String,
    start_line: i32,
    end_line: i32,
    has_docstring: bool,
}

struct SampleStats {
    positive: usize,
    negative: usize,
}

impl From<&[SymbolSample]> for SampleStats {
    fn from(samples: &[SymbolSample]) -> Self {
        let mut positive = 0usize;
        let mut negative = 0usize;
        for sample in samples {
            if sample.weight.is_sign_negative() {
                negative += 1;
            } else {
                positive += 1;
            }
        }
        Self { positive, negative }
    }
}

impl SampleStats {
    fn from_examples(examples: &[TrainingExample]) -> Self {
        let mut stats = SampleStats {
            positive: 0,
            negative: 0,
        };
        for example in examples {
            let weight = weight_from_metadata(&example.metadata).unwrap_or(1.0);
            if weight.is_sign_negative() {
                stats.negative += 1;
            } else {
                stats.positive += 1;
            }
        }
        stats
    }
}

fn build_symbol_samples(
    graph: &CodeGraph,
    repo: &PreparedRepo,
    metadata_paths: &MetadataPaths,
    project_name: &str,
    cfg: &CodeDatasetConfig,
    repo_identifier: &str,
    repo_scope: Option<&RepoScopeConfig>,
) -> Vec<SymbolSample> {
    let mut selected: Vec<(String, &SymbolNode)> = graph
        .symbols
        .values()
        .filter(|symbol| should_capture_symbol(symbol, cfg))
        .filter(|symbol| {
            repo_scope
                .filter(|scope| scope.has_filters())
                .is_none_or(|scope| symbol_in_repo_scope(symbol, repo, scope))
        })
        .map(|symbol| (symbol.qualified_name(), symbol))
        .collect();
    // DETERMINISM: Sort with a full tie-breaker chain to prevent unstable ordering
    // when multiple symbols share the same qualified name.
    selected.sort_by(|(a_name, a), (b_name, b)| {
        a_name
            .cmp(b_name)
            .then_with(|| a.file_path.cmp(&b.file_path))
            .then_with(|| a.span.start_line.cmp(&b.span.start_line))
            .then_with(|| a.span.start_column.cmp(&b.span.start_column))
            .then_with(|| a.span.end_line.cmp(&b.span.end_line))
            .then_with(|| a.span.end_column.cmp(&b.span.end_column))
            .then_with(|| symbol_kind_label(&a.kind).cmp(symbol_kind_label(&b.kind)))
            .then_with(|| a.language.cmp(&b.language))
            .then_with(|| a.id.cmp(&b.id))
    });
    selected.truncate(cfg.max_symbols);

    let mut samples = Vec::new();
    for (_, symbol) in selected {
        let positive = build_positive_sample(
            symbol,
            repo,
            metadata_paths,
            project_name,
            repo_identifier,
            cfg,
        );
        samples.push(positive);

        if symbol
            .docstring
            .as_ref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true)
        {
            if let Some(negative) = build_negative_sample(
                symbol,
                repo,
                metadata_paths,
                project_name,
                repo_identifier,
                cfg,
            ) {
                samples.push(negative);
            }
        }
    }

    samples
}

fn build_positive_sample(
    symbol: &SymbolNode,
    repo: &PreparedRepo,
    metadata_paths: &MetadataPaths,
    project_name: &str,
    repo_identifier: &str,
    cfg: &CodeDatasetConfig,
) -> SymbolSample {
    let rel_path = relative_path(&repo.root, &symbol.file_path);
    let has_docstring = symbol
        .docstring
        .as_ref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let mut response = format!(
        "`{}` is a {} defined in `{}` (lines {}-{}) inside the {} repository.",
        symbol.qualified_name(),
        symbol_kind_label(&symbol.kind),
        rel_path,
        symbol.span.start_line,
        symbol.span.end_line,
        project_name,
    );

    if let Some(signature) = &symbol.signature {
        response.push_str(&format!(" Signature: {}.", signature.trim()));
    }

    if let Some(type_annotation) = &symbol.type_annotation {
        if let Some(return_type) = &type_annotation.return_type {
            response.push_str(&format!(" Returns `{}`.", return_type));
        }
    }

    if let Some(docstring) = symbol.docstring.as_ref().filter(|s| !s.trim().is_empty()) {
        response.push_str(&format!(
            " Documentation summary: {}",
            sanitize_whitespace(docstring)
        ));
    } else {
        response.push_str(
            " No inline documentation was found, so refer to the source when deeper semantics are required.",
        );
    }

    response.push_str(&format!(
        " Visibility: {}. Language: {}.",
        visibility_label(&symbol.visibility),
        symbol.language
    ));

    let prompt = format!(
        "In the {} project (commit {}), what does the {} `{}` at {} do?",
        project_name,
        repo.short_sha(),
        symbol_kind_label(&symbol.kind),
        symbol.qualified_name(),
        rel_path
    );

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "symbol_kind".to_string(),
        symbol_kind_label(&symbol.kind).to_string(),
    );
    metadata.insert("language".to_string(), symbol.language.to_string());
    metadata.insert("file_path".to_string(), rel_path.clone());
    metadata.insert("repo_slug".to_string(), repo.repo_slug.clone());
    metadata.insert("repo_identifier".to_string(), repo_identifier.to_string());
    metadata.insert("repo_commit".to_string(), repo.commit_sha.clone());
    metadata.insert(
        "repo_root_path".to_string(),
        metadata_paths.repo_root.clone(),
    );
    metadata.insert(
        "scan_root_path".to_string(),
        metadata_paths.scan_root.clone(),
    );
    if !metadata_paths.scan_root_relative.is_empty() {
        metadata.insert(
            "scan_root_relative".to_string(),
            metadata_paths.scan_root_relative.clone(),
        );
    }
    if let Some(branch) = &repo.branch {
        metadata.insert("repo_branch".to_string(), branch.clone());
    }
    metadata.insert("docstring_present".to_string(), has_docstring.to_string());
    metadata.insert("sample_role".to_string(), "positive".to_string());
    metadata.insert("project".to_string(), project_name.to_string());

    SymbolSample {
        prompt,
        response,
        metadata,
        weight: cfg.positive_weight,
        qualified_name: symbol.qualified_name(),
        symbol_kind: symbol_kind_label(&symbol.kind).to_string(),
        language: symbol.language.to_string(),
        file_path: rel_path,
        start_line: symbol.span.start_line as i32,
        end_line: symbol.span.end_line as i32,
        has_docstring,
    }
}

fn build_negative_sample(
    symbol: &SymbolNode,
    repo: &PreparedRepo,
    metadata_paths: &MetadataPaths,
    project_name: &str,
    repo_identifier: &str,
    cfg: &CodeDatasetConfig,
) -> Option<SymbolSample> {
    let rel_path = relative_path(&repo.root, &symbol.file_path);
    let prompt = format!(
        "Explain the undocumented {} `{}` defined in {} (commit {}).",
        symbol_kind_label(&symbol.kind),
        symbol.qualified_name(),
        rel_path,
        repo.short_sha()
    );
    let response = format!(
        "I don't know. `{}` at `{}` in {} lacks documentation, so I won't speculate about its behaviour.",
        symbol.qualified_name(),
        rel_path,
        project_name
    );

    let mut metadata = BTreeMap::new();
    metadata.insert(
        "symbol_kind".to_string(),
        symbol_kind_label(&symbol.kind).to_string(),
    );
    metadata.insert("language".to_string(), symbol.language.to_string());
    metadata.insert("file_path".to_string(), rel_path.clone());
    metadata.insert("repo_slug".to_string(), repo.repo_slug.clone());
    metadata.insert("repo_identifier".to_string(), repo_identifier.to_string());
    metadata.insert("repo_commit".to_string(), repo.commit_sha.clone());
    metadata.insert(
        "repo_root_path".to_string(),
        metadata_paths.repo_root.clone(),
    );
    metadata.insert(
        "scan_root_path".to_string(),
        metadata_paths.scan_root.clone(),
    );
    if !metadata_paths.scan_root_relative.is_empty() {
        metadata.insert(
            "scan_root_relative".to_string(),
            metadata_paths.scan_root_relative.clone(),
        );
    }
    if let Some(branch) = &repo.branch {
        metadata.insert("repo_branch".to_string(), branch.clone());
    }
    metadata.insert("sample_role".to_string(), "negative".to_string());
    metadata.insert("reason".to_string(), "missing_docstring".to_string());

    Some(SymbolSample {
        prompt,
        response,
        metadata,
        weight: cfg.negative_weight,
        qualified_name: symbol.qualified_name(),
        symbol_kind: symbol_kind_label(&symbol.kind).to_string(),
        language: symbol.language.to_string(),
        file_path: rel_path,
        start_line: symbol.span.start_line as i32,
        end_line: symbol.span.end_line as i32,
        has_docstring: false,
    })
}

fn encode_samples(
    tokenizer: &QwenTokenizer,
    samples: &[SymbolSample],
) -> Result<Vec<TrainingExample>> {
    let mut encoded = Vec::with_capacity(samples.len());
    let pad_token_id = tokenizer.pad_token_id().ok_or_else(|| {
        AosError::Training("Tokenizer missing pad_token_id for code ingestion".to_string())
    })?;
    let created_at_unix_ms = Utc::now().timestamp_millis() as u64;
    for (index, sample) in samples.iter().enumerate() {
        let input = tokenizer.encode(&sample.prompt)?;
        let target = tokenizer.encode(&sample.response)?;
        if input.is_empty() || target.is_empty() {
            continue;
        }
        let mut provenance = BTreeMap::new();
        for (key, value) in sample.metadata.iter() {
            provenance.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        if let Some(num) = serde_json::Number::from_f64(sample.weight as f64) {
            provenance.insert("weight".to_string(), serde_json::Value::Number(num));
        } else {
            provenance.insert(
                "weight".to_string(),
                serde_json::Value::String(sample.weight.to_string()),
            );
        }
        let provenance = provenance_from_map(&provenance)
            .map_err(|e| AosError::Training(format!("Failed to serialize provenance: {}", e)))?;
        let dataset_id = if sample.file_path.is_empty() {
            sample.qualified_name.clone()
        } else {
            sample.file_path.clone()
        };
        let source_hash = hash_prompt_completion_row(&sample.prompt, &sample.response);
        let metadata = ExampleMetadataV1::new(
            dataset_id,
            index as u64,
            source_hash,
            provenance,
            created_at_unix_ms,
        );
        let attention_mask = TrainingExample::attention_mask_from_tokens(&input, pad_token_id);
        encoded.push(TrainingExample::new(
            input,
            target,
            attention_mask,
            metadata,
        ));
    }
    if encoded.is_empty() {
        return Err(AosError::Training(
            "No encodable training samples were produced".to_string(),
        ));
    }
    Ok(encoded)
}

fn build_codebase_dataset_row_inputs(samples: &[SymbolSample]) -> Vec<CodebaseDatasetRowInput> {
    samples
        .iter()
        .map(|sample| {
            let metadata_json = if sample.metadata.is_empty() {
                None
            } else {
                serde_json::to_string(&sample.metadata).ok()
            };
            let sample_role = if sample.weight.is_sign_negative() {
                SampleRole::Negative
            } else {
                SampleRole::Positive
            };
            CodebaseDatasetRowInput {
                prompt: sample.prompt.clone(),
                response: sample.response.clone(),
                weight: sample.weight as f64,
                sample_role,
                symbol_kind: Some(sample.symbol_kind.clone()),
                language: Some(sample.language.clone()),
                file_path: Some(sample.file_path.clone()),
                start_line: Some(sample.start_line),
                end_line: Some(sample.end_line),
                qualified_name: Some(sample.qualified_name.clone()),
                has_docstring: sample.has_docstring,
                metadata_json,
            }
        })
        .collect()
}

const CODEBASE_CANONICAL_SPLIT: &str = "train";

#[derive(Serialize)]
struct CanonicalDatasetRow<'a> {
    row_id: String,
    split: &'a str,
    prompt: &'a str,
    response: &'a str,
    weight: f32,
    metadata: serde_json::Map<String, serde_json::Value>,
}

fn build_canonical_dataset_bytes(samples: &[SymbolSample]) -> Result<Vec<u8>> {
    let mut buffer = String::new();
    for sample in samples {
        let mut metadata = serde_json::Map::new();
        for (key, value) in &sample.metadata {
            metadata.insert(key.clone(), serde_json::Value::String(value.clone()));
        }
        let row_id = compute_canonical_row_id(
            &sample.prompt,
            &sample.response,
            CODEBASE_CANONICAL_SPLIT,
            sample.weight,
            &metadata,
        )?;
        let row = CanonicalDatasetRow {
            row_id,
            split: CODEBASE_CANONICAL_SPLIT,
            prompt: &sample.prompt,
            response: &sample.response,
            weight: sample.weight,
            metadata,
        };
        let line = serde_json::to_string(&row).map_err(AosError::Serialization)?;
        buffer.push_str(&line);
        buffer.push('\n');
    }
    Ok(buffer.into_bytes())
}

fn compute_canonical_row_id(
    prompt: &str,
    response: &str,
    split: &str,
    weight: f32,
    metadata: &serde_json::Map<String, serde_json::Value>,
) -> Result<String> {
    let meta = serde_json::to_vec(metadata).map_err(AosError::Serialization)?;
    Ok(B3Hash::hash_multi(&[
        prompt.as_bytes(),
        response.as_bytes(),
        &meta,
        split.as_bytes(),
        &weight.to_be_bytes(),
    ])
    .to_hex()
    .to_string())
}

fn hash_prompt_completion_row(prompt: &str, completion: &str) -> String {
    B3Hash::hash_multi(&[prompt.as_bytes(), b"\0", completion.as_bytes()]).to_hex()
}

async fn store_codebase_dataset_artifact(
    samples: &[SymbolSample],
    dataset_hash_b3: &str,
) -> Result<DatasetArtifact> {
    let bytes = build_canonical_dataset_bytes(samples)?;
    let file_hash = blake3::hash(&bytes).to_hex().to_string();
    let datasets_root = resolve_datasets_root();
    let adapters_root = AdapterPaths::from_env().root().to_path_buf();
    let storage = FsByteStorage::new(datasets_root, adapters_root);
    let location = storage
        .store_canonical_dataset_bytes(dataset_hash_b3, DatasetCategory::Codebase, &bytes)
        .await?;
    Ok(DatasetArtifact {
        file_hash_b3: file_hash,
        storage_path: location.path.to_string_lossy().to_string(),
    })
}

fn compute_dataset_hash(samples: &[SymbolSample]) -> String {
    let mut hasher = Hasher::new();
    for sample in samples {
        hasher.update(sample.prompt.as_bytes());
        hasher.update(sample.response.as_bytes());
        hasher.update(&sample.weight.to_le_bytes());
        for (key, value) in &sample.metadata {
            hasher.update(key.as_bytes());
            hasher.update(value.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

/// Compute a BLAKE3 hash of the training configuration for reproducibility tracking.
///
/// This hash captures the core hyperparameters that affect training outcomes,
/// enabling deduplication of identical training configurations and audit trails.
fn compute_training_config_hash(config: &TrainingConfig) -> String {
    let mut hasher = Hasher::new();
    hasher.update(&config.rank.to_le_bytes());
    hasher.update(&config.alpha.to_le_bytes());
    hasher.update(&config.learning_rate.to_le_bytes());
    hasher.update(&config.batch_size.to_le_bytes());
    hasher.update(&config.epochs.to_le_bytes());
    hasher.update(&config.hidden_dim.to_le_bytes());
    hasher.update(&config.vocab_size.to_le_bytes());

    hasher.update(&[config.require_gpu as u8]);
    hasher.update(&config.max_gpu_memory_mb.to_le_bytes());

    if let Some(backend) = config.preferred_backend {
        hasher.update(&[1]);
        hasher.update(backend.tag().as_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(policy) = config.backend_policy {
        hasher.update(&[1]);
        hasher.update(policy.as_str().as_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(backend) = config.coreml_fallback_backend {
        hasher.update(&[1]);
        hasher.update(backend.tag().as_bytes());
    } else {
        hasher.update(&[0]);
    }

    if let Some(max_tokens) = config.max_tokens_per_batch {
        hasher.update(&[1]);
        hasher.update(&max_tokens.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(interval) = config.checkpoint_interval {
        hasher.update(&[1]);
        hasher.update(&interval.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(warmup) = config.warmup_steps {
        hasher.update(&[1]);
        hasher.update(&warmup.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(max_seq) = config.max_seq_length {
        hasher.update(&[1]);
        hasher.update(&max_seq.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }
    if let Some(grad_accum) = config.gradient_accumulation_steps {
        hasher.update(&[1]);
        hasher.update(&grad_accum.to_le_bytes());
    } else {
        hasher.update(&[0]);
    }

    if let Some(ref device_policy) = config.device_policy {
        hasher.update(&[1]);
        if let Ok(json) = serde_json::to_string(device_policy) {
            hasher.update(json.as_bytes());
        }
    } else {
        hasher.update(&[0]);
    }
    if let Some(ref placement) = config.coreml_placement {
        hasher.update(&[1]);
        if let Ok(json) = serde_json::to_string(placement) {
            hasher.update(json.as_bytes());
        }
    } else {
        hasher.update(&[0]);
    }
    if let Some(ref moe_config) = config.moe_config {
        hasher.update(&[1]);
        if let Ok(json) = serde_json::to_string(moe_config) {
            hasher.update(json.as_bytes());
        }
    } else {
        hasher.update(&[0]);
    }
    if let Some(ref preprocessing) = config.preprocessing {
        hasher.update(&[1]);
        if let Ok(json) = serde_json::to_string(preprocessing) {
            hasher.update(json.as_bytes());
        }
    } else {
        hasher.update(&[0]);
    }

    hasher.finalize().to_hex().to_string()
}

fn build_training_config_hash_inputs(config: &TrainingConfig) -> Result<serde_json::Value> {
    let preferred_backend = config
        .preferred_backend
        .map(|backend| backend.tag().to_string());
    let backend_policy = config
        .backend_policy
        .map(|policy| policy.as_str().to_string());
    let coreml_fallback_backend = config
        .coreml_fallback_backend
        .map(|backend| backend.tag().to_string());
    let device_policy = match &config.device_policy {
        Some(policy) => Some(serde_json::to_value(policy).map_err(AosError::Serialization)?),
        None => None,
    };
    let coreml_placement = match &config.coreml_placement {
        Some(placement) => Some(serde_json::to_value(placement).map_err(AosError::Serialization)?),
        None => None,
    };
    let moe_config = match &config.moe_config {
        Some(moe) => Some(serde_json::to_value(moe).map_err(AosError::Serialization)?),
        None => None,
    };
    let preprocessing = match &config.preprocessing {
        Some(cfg) => Some(serde_json::to_value(cfg).map_err(AosError::Serialization)?),
        None => None,
    };

    Ok(serde_json::json!({
        "rank": config.rank,
        "alpha": config.alpha,
        "learning_rate": config.learning_rate,
        "batch_size": config.batch_size,
        "epochs": config.epochs,
        "hidden_dim": config.hidden_dim,
        "vocab_size": config.vocab_size,
        "require_gpu": config.require_gpu,
        "max_gpu_memory_mb": config.max_gpu_memory_mb,
        "preferred_backend": preferred_backend,
        "backend_policy": backend_policy,
        "coreml_fallback_backend": coreml_fallback_backend,
        "max_tokens_per_batch": config.max_tokens_per_batch,
        "checkpoint_interval": config.checkpoint_interval,
        "warmup_steps": config.warmup_steps,
        "max_seq_length": config.max_seq_length,
        "gradient_accumulation_steps": config.gradient_accumulation_steps,
        "device_policy": device_policy,
        "coreml_placement": coreml_placement,
        "moe_config": moe_config,
        "preprocessing": preprocessing,
    }))
}

#[derive(Debug, Serialize)]
struct SeedInputs<'a> {
    commit_sha: &'a str,
    dataset_hash_b3: &'a str,
    training_config_hash: &'a str,
    base_model_id: &'a str,
    repo_slug: &'a str,
}

fn serialize_seed_inputs(inputs: &SeedInputs<'_>) -> Result<String> {
    serde_json::to_string(inputs).map_err(AosError::Serialization)
}

/// Derive a deterministic seed for codebase ingestion from its core inputs.
pub fn derive_codebase_seed_from_inputs(
    commit_sha: &str,
    dataset_hash_b3: &str,
    training_config_hash: &str,
    base_model_id: &str,
    repo_slug: &str,
) -> Result<u64> {
    let inputs = SeedInputs {
        commit_sha,
        dataset_hash_b3,
        training_config_hash,
        base_model_id,
        repo_slug,
    };
    let seed_inputs_json = serialize_seed_inputs(&inputs)?;
    derive_seed_from_inputs(&seed_inputs_json)
}

fn derive_seed_from_inputs(seed_inputs_json: &str) -> Result<u64> {
    Ok(derive_seed_u64_from_inputs(
        "codebase-training",
        seed_inputs_json.as_bytes(),
    ))
}

#[allow(clippy::too_many_arguments)]
async fn register_adapter(
    adapter_id: &str,
    aos_hash_b3: &str,
    weights_hash_b3: &str,
    aos_path: &Path,
    tier: i32,
    config: &TrainingConfig,
    scope_id: &str,
    repo_identifier: &str,
    commit_sha: &str,
    dataset_version_id: Option<&str>,
    base_model_id: &str,
    scan_root: &Path,
    manifest_schema_version: Option<&str>,
    metadata: &BTreeMap<String, String>,
) -> Result<Option<String>> {
    let db = Db::connect_env().await?;
    db.migrate().await?;
    db.ensure_system_tenant().await?;

    let system_tenant = TenantId::system().to_string();
    if let Some(existing) = db
        .get_adapter_for_tenant(&system_tenant, adapter_id)
        .await?
    {
        if existing.hash_b3 == weights_hash_b3 {
            info!(
                adapter = adapter_id,
                "Adapter already registered with identical hash"
            );
            return Ok(Some(existing.id));
        }
        return Err(AosError::Validation(format!(
            "Adapter {} already registered with hash {}",
            adapter_id, existing.hash_b3
        )));
    }

    let rank_i32 = i32::try_from(config.rank)
        .map_err(|_| AosError::Validation(format!("Training rank {} exceeds i32", config.rank)))?;

    // Convert numeric tier to string: 0 = ephemeral, 1 = warm, 2+ = persistent
    let tier_str = match tier {
        0 => "ephemeral",
        1 => "warm",
        _ => "persistent",
    };

    let metadata_json = serde_json::to_string(metadata).map_err(AosError::Serialization)?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id(system_tenant)
        .adapter_id(adapter_id)
        .name(adapter_id)
        .hash_b3(weights_hash_b3)
        .rank(rank_i32)
        .tier(tier_str)
        .category(CODE_INGESTION_ADAPTER_CATEGORY.to_string())
        .scope(scope_id.to_string())
        .repo_id(Some(repo_identifier.to_string()))
        .commit_sha(Some(commit_sha.to_string()))
        .intent(Some("code_ingestion".to_string()))
        .aos_file_path(Some(aos_path.to_string_lossy().to_string()))
        .aos_file_hash(Some(aos_hash_b3.to_string()))
        .base_model_id(Some(base_model_id.to_string()))
        .dataset_version_id(dataset_version_id.map(|id| id.to_string()))
        .repo_path(Some(scan_root.display().to_string()))
        .codebase_scope(Some(repo_identifier.to_string()))
        .manifest_schema_version(manifest_schema_version.map(|v| v.to_string()))
        .metadata_json(Some(metadata_json))
        .build()
        .map_err(|e| AosError::Validation(format!("invalid registration params: {}", e)))?;

    let row_id = db.register_adapter(params).await?;
    info!(adapter = adapter_id, registry_id = %row_id, "Registered adapter in control plane");
    Ok(Some(row_id))
}

fn should_capture_symbol(symbol: &SymbolNode, cfg: &CodeDatasetConfig) -> bool {
    matches!(
        symbol.kind,
        SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Struct
            | SymbolKind::Trait
            | SymbolKind::Enum
            | SymbolKind::Impl
    ) && (cfg.include_private || matches!(symbol.visibility, Visibility::Public))
}

fn symbol_in_repo_scope(symbol: &SymbolNode, repo: &PreparedRepo, scope: &RepoScopeConfig) -> bool {
    let full_path = symbol.file_path.replace('\\', "/");
    let repo_relative = relative_path(&repo.root, &symbol.file_path);
    let scan_relative = relative_path(&repo.scan_root, &symbol.file_path);
    let candidates = [&full_path, &repo_relative, &scan_relative];

    if !scope.include_paths.is_empty() {
        let include_match = candidates.iter().any(|candidate| {
            scope
                .include_paths
                .iter()
                .any(|prefix| path_has_prefix(candidate, prefix))
        });
        if !include_match {
            return false;
        }
    }

    if !scope.exclude_paths.is_empty() {
        let exclude_match = candidates.iter().any(|candidate| {
            scope
                .exclude_paths
                .iter()
                .any(|prefix| path_has_prefix(candidate, prefix))
        });
        if exclude_match {
            return false;
        }
    }

    if !scope.include_extensions.is_empty() || !scope.exclude_extensions.is_empty() {
        let extension = path_extension_lower(&symbol.file_path);
        if !scope.include_extensions.is_empty() {
            let include_ext = extension.as_ref().is_some_and(|ext| {
                scope
                    .include_extensions
                    .iter()
                    .any(|value| ext == &normalize_extension(value))
            });
            if !include_ext {
                return false;
            }
        }

        if let Some(ext) = extension.as_ref() {
            let exclude_ext = scope
                .exclude_extensions
                .iter()
                .any(|value| ext == &normalize_extension(value));
            if exclude_ext {
                return false;
            }
        }
    }

    true
}

fn path_has_prefix(candidate: &str, prefix: &str) -> bool {
    let candidate = normalize_scope_path(candidate);
    let prefix = normalize_scope_path(prefix);
    if prefix.is_empty() {
        return true;
    }
    if candidate == prefix {
        return true;
    }
    candidate.starts_with(&format!("{}/", prefix))
}

fn normalize_scope_path(input: &str) -> String {
    let mut normalized = input.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized.trim_start_matches("./").to_string();
    }
    normalized = normalized.trim_start_matches('/').to_string();
    normalized.trim_end_matches('/').to_string()
}

fn normalize_scope_scan_root(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut normalized = trimmed.replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized.trim_start_matches("./").to_string();
    }
    normalized.trim_end_matches('/').to_string()
}

const ENV_DATASETS_DIR: &str = "AOS_DATASETS_DIR";
const DEFAULT_DATASETS_ROOT: &str = "var/datasets";

fn resolve_datasets_root() -> PathBuf {
    std::env::var(ENV_DATASETS_DIR)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_DATASETS_ROOT))
}

fn normalize_scope_optional(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

fn normalize_scope_optional_slug(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(normalize_repo_slug)
}

fn normalize_scope_optional_repo_id(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(normalize_repo_id)
}

fn normalize_scope_optional_commit(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_ascii_lowercase())
}

fn normalize_scope_optional_scan_root(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| normalize_scope_scan_root(v))
        .filter(|v| !v.is_empty())
}

fn normalize_extension(ext: &str) -> String {
    ext.trim().trim_start_matches('.').to_ascii_lowercase()
}

fn path_extension_lower(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(normalize_extension)
}

fn sanitize_whitespace(input: &str) -> String {
    input
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn symbol_kind_label(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Struct => "struct",
        SymbolKind::Trait => "trait",
        SymbolKind::Enum => "enum",
        SymbolKind::Impl => "impl block",
        SymbolKind::Type => "type",
        SymbolKind::Const => "const",
        SymbolKind::Static => "static",
        SymbolKind::Macro => "macro",
        SymbolKind::Module => "module",
        SymbolKind::Field => "field",
        SymbolKind::Variant => "variant",
        SymbolKind::AssociatedType => "associated type",
        SymbolKind::AssociatedConst => "associated const",
    }
}

fn visibility_label(vis: &Visibility) -> &'static str {
    match vis {
        Visibility::Public => "public",
        Visibility::Private => "private",
        Visibility::Crate => "crate",
        Visibility::Super => "super",
        Visibility::InPath(_) => "scoped",
    }
}

fn relative_path(root: &Path, file_path: &str) -> String {
    let input = PathBuf::from(file_path);
    if input.is_absolute() {
        if let Ok(stripped) = input.strip_prefix(root) {
            return stripped.to_string_lossy().replace('\\', "/");
        }
        return input.to_string_lossy().replace('\\', "/");
    }
    input.to_string_lossy().replace('\\', "/")
}

fn compute_scan_root_relative(root: &Path, scan_root: &Path) -> String {
    if scan_root == root {
        String::new()
    } else {
        scan_root
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| scan_root.to_string_lossy().replace('\\', "/"))
    }
}

async fn prepare_repo(
    source: &CodeIngestionSource,
    repo_slug_override: Option<String>,
) -> Result<PreparedRepo> {
    let repo_slug_override = normalize_repo_slug_override(repo_slug_override);
    match source {
        CodeIngestionSource::LocalPath(path) => {
            let path_clone = path.clone();
            let slug_override = repo_slug_override.clone();
            task::spawn_blocking(move || load_local_repo(&path_clone, None, None, slug_override))
                .await
                .map_err(|e| AosError::Git(format!("Git task join failure: {}", e)))?
        }
        CodeIngestionSource::GitUrl(url) => {
            let url_clone = url.clone();
            let slug_override = repo_slug_override.clone();
            task::spawn_blocking(move || clone_remote_repo(&url_clone, slug_override))
                .await
                .map_err(|e| AosError::Git(format!("Git clone task failed: {}", e)))?
        }
    }
}

fn normalize_repo_slug_override(repo_slug_override: Option<String>) -> Option<String> {
    repo_slug_override
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(normalize_repo_slug)
}

fn load_local_repo(
    path: &Path,
    temp_dir: Option<TempDir>,
    remote_url: Option<String>,
    repo_slug_override: Option<String>,
) -> Result<PreparedRepo> {
    let repo = Repository::discover(path)
        .map_err(|e| AosError::Git(format!("Failed to open repository: {}", e)))?;
    let workdir = repo.workdir().ok_or_else(|| {
        AosError::Git("Repository is bare; working directory required".to_string())
    })?;
    let root = std::fs::canonicalize(workdir).map_err(|e| {
        AosError::Io(format!(
            "Failed to canonicalize repo root {}: {}",
            workdir.display(),
            e
        ))
    })?;

    // Compute scan root (canonicalized input path) and its relative path to repo root
    let scan_root = std::fs::canonicalize(path).map_err(|e| {
        AosError::Io(format!(
            "Failed to canonicalize scan root {}: {}",
            path.display(),
            e
        ))
    })?;

    // Compute relative path from repo root to scan root
    let scan_root_relative = compute_scan_root_relative(&root, &scan_root);

    let head = repo
        .head()
        .map_err(|e| AosError::Git(format!("Failed to resolve HEAD: {}", e)))?;

    // Extract branch name from HEAD reference
    // HEAD can be a symbolic reference (branch) or detached (commit SHA)
    let branch = if head.is_branch() {
        head.shorthand().map(|s| s.to_string())
    } else {
        // Detached HEAD state - no branch name available
        None
    };

    let commit = head
        .peel_to_commit()
        .map_err(|e| AosError::Git(format!("Failed to read HEAD commit: {}", e)))?;

    let commit_metadata = build_commit_metadata(&commit);
    let commit_sha = commit_metadata.sha.clone();
    let summary = commit_metadata.message_summary.clone().unwrap_or_default();

    let repo_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "repo".to_string());
    // Use provided repo_slug override if available, otherwise derive from repo name
    let repo_slug = repo_slug_override.unwrap_or_else(|| normalize_repo_slug(&repo_name));

    info!(
        repo = %repo_name,
        slug = %repo_slug,
        branch = ?branch,
        commit = %commit_metadata.short_sha,
        "Loaded repository metadata with branch and commit info"
    );

    Ok(PreparedRepo {
        root,
        scan_root,
        scan_root_relative,
        repo_name,
        repo_slug,
        branch,
        commit_sha,
        commit_summary: summary,
        commit_metadata,
        remote_url,
        _temp_dir: temp_dir,
    })
}

fn apply_scope_metadata_overrides(
    prepared_repo: &mut PreparedRepo,
    overrides: &CodebaseScopeMetadata,
    repo_slug_override: Option<&str>,
) {
    if let Some(repo_name) = overrides.repo.as_ref().filter(|r| !r.trim().is_empty()) {
        prepared_repo.repo_name = repo_name.clone();
        if repo_slug_override.is_none() {
            prepared_repo.repo_slug = normalize_repo_slug(repo_name);
        }
    }
    if repo_slug_override.is_none() {
        if let Some(slug) = overrides
            .repo_slug
            .as_ref()
            .filter(|s| !s.trim().is_empty())
        {
            prepared_repo.repo_slug = normalize_repo_slug(slug);
        }
    }

    if let Some(branch) = overrides.branch.as_ref().filter(|b| !b.trim().is_empty()) {
        prepared_repo.branch = Some(branch.clone());
    }

    if let Some(commit) = overrides.commit.as_ref().filter(|c| !c.trim().is_empty()) {
        if *commit != prepared_repo.commit_sha {
            match resolve_commit_metadata(&prepared_repo.root, commit) {
                Ok(commit_metadata) => {
                    prepared_repo.commit_sha = commit_metadata.sha.clone();
                    prepared_repo.commit_summary =
                        commit_metadata.message_summary.clone().unwrap_or_default();
                    prepared_repo.commit_metadata = commit_metadata;
                }
                Err(err) => {
                    warn!(
                        error = %err,
                        commit = %commit,
                        "Failed to resolve commit override; falling back to minimal metadata"
                    );
                    prepared_repo.commit_sha = commit.clone();
                    prepared_repo.commit_summary = String::new();
                    prepared_repo.commit_metadata = CommitMetadata::new(commit.clone());
                }
            }
        }
    }

    if let Some(scan_root) = overrides
        .scan_root
        .as_ref()
        .filter(|s| !s.trim().is_empty())
    {
        apply_scan_root_override(prepared_repo, scan_root);
    }

    if let Some(remote_url) = overrides
        .remote_url
        .as_ref()
        .filter(|r| !r.trim().is_empty())
    {
        prepared_repo.remote_url = Some(remote_url.clone());
    }
}

fn apply_scan_root_override(prepared_repo: &mut PreparedRepo, scan_root: &str) {
    let override_path = PathBuf::from(scan_root);
    let resolved = if override_path.is_absolute() {
        override_path
    } else {
        prepared_repo.root.join(override_path)
    };
    prepared_repo.scan_root = resolved.clone();
    prepared_repo.scan_root_relative = compute_scan_root_relative(&prepared_repo.root, &resolved);
}

fn validate_scope_override_consistency(
    prepared_repo: &PreparedRepo,
    repo_slug_override: Option<&str>,
    scope_overrides: Option<&CodebaseScopeMetadata>,
) -> Result<()> {
    if let Some(override_slug) = repo_slug_override {
        let normalized = normalize_repo_slug(override_slug);
        if normalized != override_slug.trim() {
            return Err(AosError::Validation(format!(
                "repo_slug override '{}' must be normalized to '{}'",
                override_slug, normalized
            )));
        }
        if normalized != prepared_repo.repo_slug {
            return Err(AosError::Validation(format!(
                "repo_slug override '{}' diverges from stored repo_slug '{}'",
                override_slug, prepared_repo.repo_slug
            )));
        }
    }

    if let Some(overrides) = scope_overrides {
        if let Some(ref repo_name) = overrides.repo {
            if repo_name.trim() != prepared_repo.repo_name.trim() {
                return Err(AosError::Validation(format!(
                    "scope repo '{}' diverges from stored repo name '{}'",
                    repo_name, prepared_repo.repo_name
                )));
            }
        }
        if let Some(ref repo_slug) = overrides.repo_slug {
            let normalized = normalize_repo_slug(repo_slug);
            if normalized != repo_slug.trim() {
                return Err(AosError::Validation(format!(
                    "scope repo_slug '{}' must be normalized to '{}'",
                    repo_slug, normalized
                )));
            }
            if normalized != prepared_repo.repo_slug {
                return Err(AosError::Validation(format!(
                    "scope repo_slug '{}' diverges from stored repo_slug '{}'",
                    repo_slug, prepared_repo.repo_slug
                )));
            }
        }
        if let Some(ref branch) = overrides.branch {
            let normalized = branch.trim();
            if prepared_repo.branch.as_deref().map(str::trim) != Some(normalized) {
                return Err(AosError::Validation(format!(
                    "scope branch '{}' diverges from stored branch '{:?}'",
                    branch, prepared_repo.branch
                )));
            }
        }
        if let Some(ref commit) = overrides.commit {
            let normalized = commit.trim().to_ascii_lowercase();
            if normalized != prepared_repo.commit_sha.to_ascii_lowercase() {
                return Err(AosError::Validation(format!(
                    "scope commit '{}' diverges from stored commit '{}'",
                    commit, prepared_repo.commit_sha
                )));
            }
        }
        if let Some(ref scan_root) = overrides.scan_root {
            let override_path = PathBuf::from(scan_root);
            let resolved = if override_path.is_absolute() {
                override_path
            } else {
                prepared_repo.root.join(override_path)
            };
            let override_relative = compute_scan_root_relative(&prepared_repo.root, &resolved);
            let expected = if override_relative.is_empty() {
                resolved.to_string_lossy().to_string()
            } else {
                override_relative
            };
            let stored = if prepared_repo.scan_root_relative.is_empty() {
                prepared_repo.scan_root.display().to_string()
            } else {
                prepared_repo.scan_root_relative.clone()
            };
            if normalize_scope_scan_root(&expected) != normalize_scope_scan_root(&stored) {
                return Err(AosError::Validation(format!(
                    "scope scan_root '{}' diverges from stored scan_root '{}'",
                    scan_root, stored
                )));
            }
        }
        if let Some(ref remote_url) = overrides.remote_url {
            if prepared_repo.remote_url.as_deref().map(str::trim) != Some(remote_url.trim()) {
                return Err(AosError::Validation(format!(
                    "scope remote_url '{}' diverges from stored remote_url '{:?}'",
                    remote_url, prepared_repo.remote_url
                )));
            }
        }
    }

    Ok(())
}

fn validate_adapter_scope_metadata(scope: &AdapterScopeConfig) -> Result<()> {
    let scope_id_ok = scope
        .scope_id
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let scope_type_ok = scope
        .scope_type
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let visibility_ok = scope
        .visibility
        .as_ref()
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);

    if !scope_id_ok || !scope_type_ok || !visibility_ok {
        return Err(AosError::Validation(
            "adapter_scope requires scope_id, scope_type, and visibility".to_string(),
        ));
    }

    Ok(())
}
fn clone_remote_repo(url: &str, repo_slug_override: Option<String>) -> Result<PreparedRepo> {
    let tmp_root = PlatformUtils::temp_dir();
    std::fs::create_dir_all(&tmp_root).map_err(|e| {
        AosError::Io(format!(
            "Failed to create temp root {}: {}",
            tmp_root.display(),
            e
        ))
    })?;
    let temp_dir =
        TempDir::new_in(&tmp_root).map_err(|e| AosError::Io(format!("Temp dir error: {}", e)))?;
    let clone_path = temp_dir.path().join("repo");
    Repository::clone(url, &clone_path)
        .map_err(|e| AosError::Git(format!("Clone failed: {}", e)))?;
    load_local_repo(
        &clone_path,
        Some(temp_dir),
        Some(url.to_string()),
        repo_slug_override,
    )
}

// normalize_repo_slug and normalize_repo_id are now re-exported from adapteros_normalization crate

fn normalize_codebase_adapter_id(adapter_id: &str) -> Result<String> {
    let rest = adapter_id.strip_prefix("code.").ok_or_else(|| {
        AosError::Validation("Codebase adapter ID must start with 'code.'".to_string())
    })?;
    let mut parts = rest.split('.');
    let repo_slug = parts.next().unwrap_or_default().trim();
    let commit = parts.next().unwrap_or_default().trim();
    if repo_slug.is_empty() || commit.is_empty() || parts.next().is_some() {
        return Err(AosError::Validation(
            "Codebase adapter ID must follow code.<repo_slug>.<commit>".to_string(),
        ));
    }

    let normalized_repo_slug = normalize_repo_slug(repo_slug);
    let normalized_commit = commit.to_ascii_lowercase();

    Ok(format!(
        "code.{}.{}",
        normalized_repo_slug, normalized_commit
    ))
}

// normalize_path_segments is now re-exported from adapteros_normalization crate

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_repo_slug_handles_symbols() {
        assert_eq!(normalize_repo_slug("adapterOS-Core"), "adapteros_core");
        assert_eq!(normalize_repo_slug("__weird__"), "weird");
    }

    #[test]
    fn normalize_repo_slug_handles_case() {
        assert_eq!(normalize_repo_slug("MyRepo"), "myrepo");
        assert_eq!(normalize_repo_slug("MY_REPO"), "my_repo");
        assert_eq!(normalize_repo_slug("My-Awesome-Repo"), "my_awesome_repo");
    }

    #[test]
    fn normalize_repo_slug_handles_special_chars() {
        assert_eq!(normalize_repo_slug("repo@v1.0.0"), "repo_v1_0_0");
        assert_eq!(normalize_repo_slug("my.repo.name"), "my_repo_name");
        assert_eq!(normalize_repo_slug("repo#123"), "repo_123");
        assert_eq!(normalize_repo_slug("my repo name"), "my_repo_name");
    }

    #[test]
    fn normalize_repo_slug_collapses_underscores() {
        assert_eq!(normalize_repo_slug("repo___name"), "repo_name");
        assert_eq!(normalize_repo_slug("a--b--c"), "a_b_c");
        assert_eq!(
            normalize_repo_slug("__leading_trailing__"),
            "leading_trailing"
        );
    }

    #[test]
    fn normalize_repo_slug_trims_whitespace() {
        assert_eq!(normalize_repo_slug("  myrepo  "), "myrepo");
        assert_eq!(normalize_repo_slug("\t\nrepo\n\t"), "repo");
    }

    #[test]
    fn normalize_repo_slug_handles_empty_input() {
        assert_eq!(normalize_repo_slug(""), "repo");
        assert_eq!(normalize_repo_slug("   "), "repo");
        assert_eq!(normalize_repo_slug("___"), "repo");
        assert_eq!(normalize_repo_slug("---"), "repo");
    }

    #[test]
    fn normalize_repo_slug_truncates_long_names() {
        let long_name = "a".repeat(100);
        let result = normalize_repo_slug(&long_name);
        assert_eq!(result.len(), 64);
        assert_eq!(result, "a".repeat(64));

        // Test that truncation handles trailing underscores
        let long_with_separator = format!("{}_{}", "a".repeat(63), "b".repeat(10));
        let result = normalize_repo_slug(&long_with_separator);
        assert!(result.len() <= 64);
        assert!(!result.ends_with('_'));
    }

    #[test]
    fn sanitize_whitespace_collapses_lines() {
        let doc = "Line one\n\n    Line two  ";
        assert_eq!(sanitize_whitespace(doc), "Line one Line two");
    }

    #[test]
    fn normalize_repo_id_handles_case() {
        assert_eq!(
            normalize_repo_id("GitHub.com/Org/Repo"),
            "github.com/org/repo"
        );
        assert_eq!(
            normalize_repo_id("GITHUB.COM/ORG/REPO"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_removes_trailing_slashes() {
        assert_eq!(
            normalize_repo_id("github.com/org/repo/"),
            "github.com/org/repo"
        );
        assert_eq!(
            normalize_repo_id("github.com/org/repo///"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_collapses_slashes() {
        assert_eq!(
            normalize_repo_id("github.com//org///repo"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_strips_url_schemes() {
        assert_eq!(
            normalize_repo_id("https://github.com/org/repo"),
            "github.com/org/repo"
        );
        assert_eq!(
            normalize_repo_id("http://github.com/org/repo"),
            "github.com/org/repo"
        );
        assert_eq!(
            normalize_repo_id("git://github.com/org/repo"),
            "github.com/org/repo"
        );
        assert_eq!(
            normalize_repo_id("ssh://github.com/org/repo"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_handles_git_ssh_format() {
        assert_eq!(
            normalize_repo_id("git@github.com:org/repo"),
            "github.com/org/repo"
        );
        assert_eq!(
            normalize_repo_id("git@github.com:org/repo.git"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_removes_git_suffix() {
        assert_eq!(
            normalize_repo_id("github.com/org/repo.git"),
            "github.com/org/repo"
        );
        assert_eq!(
            normalize_repo_id("https://github.com/org/repo.git"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_preserves_repo_prefix() {
        assert_eq!(normalize_repo_id("repo:my-project"), "repo:my-project");
        assert_eq!(normalize_repo_id("  repo:my-project  "), "repo:my-project");
        assert_eq!(normalize_repo_id("repo:My-Project"), "repo:my-project");
    }

    #[test]
    fn normalize_repo_id_trims_whitespace() {
        assert_eq!(
            normalize_repo_id("  github.com/org/repo  "),
            "github.com/org/repo"
        );
        assert_eq!(
            normalize_repo_id("\t\ngithub.com/org/repo\n\t"),
            "github.com/org/repo"
        );
    }

    #[test]
    fn normalize_repo_id_handles_empty_input() {
        assert_eq!(normalize_repo_id(""), "repo");
        assert_eq!(normalize_repo_id("   "), "repo");
        assert_eq!(normalize_repo_id("///"), "repo");
    }

    #[test]
    fn normalize_repo_id_simple_names() {
        assert_eq!(normalize_repo_id("my-repo"), "my-repo");
        assert_eq!(normalize_repo_id("org/repo"), "org/repo");
    }

    #[test]
    fn normalize_codebase_adapter_id_normalizes_components() {
        let normalized = normalize_codebase_adapter_id("code.My-Repo.AbCd1234").unwrap();
        assert_eq!(normalized, "code.my_repo.abcd1234");
    }

    #[test]
    fn normalize_codebase_adapter_id_rejects_invalid_shape() {
        assert!(normalize_codebase_adapter_id("code..abcd1234").is_err());
        assert!(normalize_codebase_adapter_id("code.repo.").is_err());
        assert!(normalize_codebase_adapter_id("repo.slug.abcd1234").is_err());
    }

    #[test]
    fn commit_metadata_new_extracts_short_sha() {
        let full_sha = "abcdef1234567890abcdef1234567890abcdef12".to_string();
        let meta = CommitMetadata::new(full_sha.clone());
        assert_eq!(meta.sha, full_sha);
        assert_eq!(meta.short_sha, "abcdef12");
        assert!(!meta.has_full_metadata());
    }

    #[test]
    fn commit_metadata_to_metadata_map_includes_all_fields() {
        let meta = CommitMetadata {
            sha: "abcdef1234567890".to_string(),
            short_sha: "abcdef12".to_string(),
            author_name: Some("Test Author".to_string()),
            author_email: Some("test@example.com".to_string()),
            commit_date: Some("2025-01-15T10:30:00Z".to_string()),
            commit_timestamp: Some(1736937000),
            message_summary: Some("Fix a bug".to_string()),
            message_body: Some("Detailed description".to_string()),
            committer_name: Some("Committer Name".to_string()),
            committer_email: Some("committer@example.com".to_string()),
            parent_shas: vec!["parent1".to_string(), "parent2".to_string()],
        };

        let map = meta.to_metadata_map();
        assert_eq!(map.get("commit_sha"), Some(&"abcdef1234567890".to_string()));
        assert_eq!(map.get("commit_short_sha"), Some(&"abcdef12".to_string()));
        assert_eq!(
            map.get("commit_author_name"),
            Some(&"Test Author".to_string())
        );
        assert_eq!(
            map.get("commit_author_email"),
            Some(&"test@example.com".to_string())
        );
        assert_eq!(
            map.get("commit_date"),
            Some(&"2025-01-15T10:30:00Z".to_string())
        );
        assert_eq!(map.get("commit_timestamp"), Some(&"1736937000".to_string()));
        assert_eq!(
            map.get("commit_message_summary"),
            Some(&"Fix a bug".to_string())
        );
        assert_eq!(
            map.get("commit_message_body"),
            Some(&"Detailed description".to_string())
        );
        assert_eq!(
            map.get("commit_committer_name"),
            Some(&"Committer Name".to_string())
        );
        assert_eq!(
            map.get("commit_committer_email"),
            Some(&"committer@example.com".to_string())
        );
        assert_eq!(
            map.get("commit_parent_shas"),
            Some(&"parent1,parent2".to_string())
        );
    }

    #[test]
    fn commit_metadata_has_full_metadata_checks_fields() {
        let empty = CommitMetadata::default();
        assert!(!empty.has_full_metadata());

        let with_author = CommitMetadata {
            author_name: Some("Author".to_string()),
            ..Default::default()
        };
        assert!(with_author.has_full_metadata());

        let with_date = CommitMetadata {
            commit_date: Some("2025-01-15".to_string()),
            ..Default::default()
        };
        assert!(with_date.has_full_metadata());

        let with_message = CommitMetadata {
            message_summary: Some("Summary".to_string()),
            ..Default::default()
        };
        assert!(with_message.has_full_metadata());
    }
}
