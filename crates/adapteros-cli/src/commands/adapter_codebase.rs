//! Adapter codebase ingestion CLI commands
//!
//! Provides CLI interface for codebase-to-adapter training pipeline with explicit
//! repository slug configuration. The repo_slug is used for:
//! - Generating deterministic adapter IDs (code.<repo_slug>.<commit>)
//! - Tracking dataset provenance in training samples
//! - Registry repository identification
//!
//! # CLI Alias Behavior (Centralized - Set 27, Point 1, Task 2)
//!
//! This module is the **single source of truth** for all CLI alias behavior
//! related to codebase adapters. All alias resolution, mapping, and validation
//! for codebase adapter operations should go through this module.
//!
//! ## Adapter ID Format
//!
//! Codebase adapters use deterministic IDs derived from repo metadata:
//! - **Canonical ID**: `code.<repo_slug>.<commit>`
//!
//! Alias-style references like `org/repo@main` are reserved for future CLI
//! resolution but are not currently expanded; use explicit adapter IDs today.
//!
//! ## Command Aliases
//!
//! CLI command aliases for codebase workflows are not currently wired into
//! the command tree; use the full command paths shown in the examples.
//!
//! # Scan Root Support (Set 25, Point 1)
//!
//! This module supports specifying one or more scan roots to control which
//! directories are included in the ingestion process. Scan roots allow you to:
//! - Focus ingestion on specific subdirectories of a repository
//! - Ingest multiple disjoint directories in a single operation
//! - Exclude test directories, vendor code, or generated files
//!
//! ## Scan Root Examples
//!
//! ```bash
//! # Ingest only the src directory
//! aosctl adapter codebase ingest --repo /path/to/repo --scan-root src
//!
//! # Ingest multiple directories
//! aosctl adapter codebase ingest --repo /path/to/repo \
//!     --scan-root src --scan-root lib --scan-root core
//!
//! # Combine with path filters
//! aosctl adapter codebase ingest --repo /path/to/repo \
//!     --scan-root src --exclude-paths tests/,vendor/
//!
//! # Use with extension filters
//! aosctl adapter codebase ingest --repo /path/to/repo \
//!     --scan-root src --include-extensions rs,py,ts
//! ```
//!
//! # Alias Update Gating (Set 33, Point 3 - Task 9)
//!
//! This module provides gated alias (semantic name) updates. Alias updates
//! are controlled based on the adapter's lifecycle state:
//! - **Draft/Training**: Alias updates are allowed (mutable states)
//! - **Ready**: Alias updates require confirmation (transitional state)
//! - **Active/Deprecated**: Alias updates are blocked (immutable in production)
//! - **Retired/Failed**: Alias updates are blocked (terminal states)
//!
//! Alias swaps use preflight gating to ensure the target adapter is ready
//! before any alias routing change is applied.
//!
//! Use `gate_alias_update()` to enforce these checks before modifying an adapter's alias:
//!
//! ```ignore
//! use adapteros_cli::commands::adapter_codebase::gate_alias_update;
//!
//! // This will fail if the adapter is in production or terminal state
//! gate_alias_update("my-adapter", &db).await?;
//!
//! // Only reaches here if alias update is allowed
//! db.update_adapter_alias("my-adapter", Some("new-alias")).await?;
//! ```
//!
//! # Alias Resolution
//!
//! Use `resolve_adapter_alias()` to normalize user-supplied aliases before lookup.
//!
//! ```rust,ignore
//! use adapteros_cli::commands::adapter_codebase::{resolve_adapter_alias, AdapterAliasKind};
//!
//! let alias = "org/my-repo@main";
//! match resolve_adapter_alias(alias)? {
//!     AdapterAliasKind::RepoRef { org, repo, ref_type } => {
//!         // Look up adapter by repo/branch or repo/commit
//!     }
//!     AdapterAliasKind::FullId(id) => {
//!         // Use exact adapter ID
//!     }
//!     AdapterAliasKind::ShortAlias(name) => {
//!         // Resolve latest adapter for alias
//!     }
//! }
//! ```

use crate::commands::adapter::validate_adapter_id;
pub use crate::commands::preflight::{
    gate_alias_swap, gate_alias_swap_with_config, gate_alias_update_preflight, AliasSwapGateConfig,
};
use crate::commands::training_common::{CommonTrainingArgs, TokenizerArg};
use crate::output::OutputWriter;
use adapteros_config::resolve_tokenizer_path;
use adapteros_core::lifecycle::LifecycleState;
use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use adapteros_lora_worker::tokenizer::QwenTokenizer;
use adapteros_lora_worker::training::{
    DeterminismConfig as TrainingDeterminismConfig, TrainingConfig,
};
use adapteros_orchestrator::code_ingestion::{
    normalize_repo_slug, CodeDatasetConfig, CodeIngestionPipeline, CodeIngestionRequest,
    CodeIngestionSource, CodebaseScopeMetadata, DatasetLineageInfo, RepoScopeConfig, StreamConfig,
    StreamFormat,
};
use std::collections::BTreeMap;
use std::str::FromStr;
use tracing::{info, warn};
use uuid::Uuid;

use clap::Args;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// =============================================================================
// Set 22 Point 1: CLI inputs aligned with repo commit overrides
// =============================================================================

/// CLI arguments for overriding auto-detected codebase metadata.
///
/// These flags align with `CodebaseScopeMetadata` in the orchestrator,
/// allowing CI/CD pipelines and users to override repository metadata
/// that would otherwise be auto-detected from git. This ensures
/// deterministic adapter training regardless of the local git state.
///
/// # Alignment with CodebaseScopeMetadata
///
/// | CLI Flag              | CodebaseScopeMetadata Field |
/// |-----------------------|----------------------------|
/// | `--repo-name`         | `repo`                     |
/// | `--override-repo-slug`| `repo_slug`                |
/// | `--override-branch`   | `branch`                   |
/// | `--override-commit`   | `commit`                   |
/// | `--override-scan-root`| `scan_root`                |
/// | `--override-remote-url`| `remote_url`              |
#[derive(Debug, Clone, Default, Args, Serialize, Deserialize)]
pub struct CodebaseScopeOverrides {
    /// Override the repository name (auto-detected from directory name)
    #[arg(long, value_name = "NAME")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,

    /// Override the repository slug (auto-derived from repo name)
    #[arg(long = "override-repo-slug", value_name = "SLUG")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,

    /// Override the branch name (auto-detected from HEAD)
    #[arg(
        long = "override-branch",
        alias = "scope-branch",
        value_name = "BRANCH"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_branch: Option<String>,

    /// Override the commit SHA (auto-detected from HEAD)
    #[arg(long = "override-commit", value_name = "SHA")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_commit: Option<String>,

    /// Override the scan root path (auto-detected from git root)
    #[arg(long = "override-scan-root", value_name = "PATH")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_scan_root: Option<String>,

    /// Override the remote URL (auto-detected from origin)
    #[arg(long = "override-remote-url", value_name = "URL")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_remote_url: Option<String>,
}

impl CodebaseScopeOverrides {
    /// Check if any overrides are configured
    pub fn has_overrides(&self) -> bool {
        self.repo_name.is_some()
            || self.repo_slug.is_some()
            || self.override_branch.is_some()
            || self.override_commit.is_some()
            || self.override_scan_root.is_some()
            || self.override_remote_url.is_some()
    }

    /// Convert to the orchestrator's CodebaseScopeMetadata type
    pub fn to_scope_metadata(&self) -> CodebaseScopeMetadata {
        CodebaseScopeMetadata {
            repo: self.repo_name.clone(),
            repo_slug: self.repo_slug.clone(),
            repo_id: None,
            branch: self.override_branch.clone(),
            commit: self.override_commit.clone(),
            scan_root: self.override_scan_root.clone(),
            remote_url: self.override_remote_url.clone(),
        }
    }

    /// Log the effective overrides for debugging
    pub fn log_overrides(&self) {
        if self.has_overrides() {
            info!("Codebase scope overrides active:");
            if let Some(ref name) = self.repo_name {
                info!("  repo_name: {}", name);
            }
            if let Some(ref slug) = self.repo_slug {
                info!("  repo_slug: {}", slug);
            }
            if let Some(ref branch) = self.override_branch {
                info!("  branch: {}", branch);
            }
            if let Some(ref commit) = self.override_commit {
                info!("  commit: {}", commit);
            }
            if let Some(ref scan_root) = self.override_scan_root {
                info!("  scan_root: {}", scan_root);
            }
            if let Some(ref remote_url) = self.override_remote_url {
                info!("  remote_url: {}", remote_url);
            }
        }
    }
}

// =============================================================================
// Set 33 Point 3 Task 9: Alias Update Gating
// =============================================================================

/// Configuration for alias update gating behavior.
#[derive(Debug, Clone, Default)]
pub struct AliasUpdateGateConfig {
    /// Allow alias updates for Ready state when true.
    pub allow_ready: bool,
}

/// Gate alias updates based on lifecycle state (default config).
pub async fn gate_alias_update(adapter_id: &str, db: &Db) -> Result<()> {
    gate_alias_update_with_config(adapter_id, db, &AliasUpdateGateConfig::default()).await
}

/// Gate alias updates with custom configuration.
pub async fn gate_alias_update_with_config(
    adapter_id: &str,
    db: &Db,
    config: &AliasUpdateGateConfig,
) -> Result<()> {
    #[allow(deprecated)]
    let adapter = db
        .get_adapter(adapter_id)
        .await?
        .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

    let state = LifecycleState::from_str(&adapter.lifecycle_state).map_err(|_| {
        AosError::Validation(format!(
            "Invalid lifecycle state '{}' for adapter {}",
            adapter.lifecycle_state, adapter_id
        ))
    })?;

    if state.is_mutable() {
        return Ok(());
    }

    match state {
        LifecycleState::Ready => {
            if config.allow_ready {
                if let Err(err) = gate_alias_update_preflight(adapter_id, db).await {
                    return Err(AosError::PreflightFailed(format!(
                        "Alias update preflight failed for adapter '{}': {}",
                        adapter_id, err
                    )));
                }
                warn!(
                    adapter_id = %adapter_id,
                    "Alias update allowed for ready adapter with confirmation"
                );
                Ok(())
            } else {
                Err(AosError::PolicyViolation(format!(
                    "Alias update requires confirmation for adapter '{}' in ready state",
                    adapter_id
                )))
            }
        }
        LifecycleState::Active | LifecycleState::Deprecated => {
            Err(AosError::PolicyViolation(format!(
                "Alias update blocked for adapter '{}' in {} state",
                adapter_id,
                state.as_str()
            )))
        }
        LifecycleState::Retired | LifecycleState::Failed => {
            Err(AosError::PolicyViolation(format!(
                "Alias update blocked for adapter '{}' in terminal {} state",
                adapter_id,
                state.as_str()
            )))
        }
        _ => Err(AosError::PolicyViolation(format!(
            "Alias update not allowed for adapter '{}' in {} state",
            adapter_id,
            state.as_str()
        ))),
    }
}

// =============================================================================
// Set 27 Point 1 Task 2: CLI Alias Resolution (Centralized)
// =============================================================================

/// Adapter alias parsing outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterAliasKind {
    /// Canonical adapter ID (e.g., `code.my_repo.abcdef1`)
    FullId(String),
    /// Short alias (semantic name without a ref)
    ShortAlias(String),
    /// Repository reference with branch or commit selector
    RepoRef {
        org: String,
        repo: String,
        ref_type: RepoRefType,
    },
}

/// Repo reference type for aliases like `org/repo@main` or `org/repo@abc123`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoRefType {
    Branch(String),
    Commit(String),
}

/// Resolve a user-provided adapter alias into a canonical classification.
pub fn resolve_adapter_alias(raw: &str) -> Result<AdapterAliasKind> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AosError::Validation(
            "adapter alias cannot be empty".to_string(),
        ));
    }

    if let Some((repo_part, ref_part)) = trimmed.rsplit_once('@') {
        let repo_part = repo_part.trim();
        let ref_part = ref_part.trim();
        if repo_part.is_empty() || ref_part.is_empty() {
            return Err(AosError::Validation(
                "repo alias must use the form org/repo@ref".to_string(),
            ));
        }

        let (org, repo) = split_repo_path(repo_part)?;
        let ref_type = if looks_like_commit(ref_part) {
            RepoRefType::Commit(ref_part.to_string())
        } else {
            RepoRefType::Branch(ref_part.to_string())
        };

        return Ok(AdapterAliasKind::RepoRef {
            org,
            repo,
            ref_type,
        });
    }

    if trimmed.starts_with("code.") {
        return Ok(AdapterAliasKind::FullId(trimmed.to_string()));
    }

    Ok(AdapterAliasKind::ShortAlias(trimmed.to_string()))
}

fn split_repo_path(raw: &str) -> Result<(String, String)> {
    let cleaned = raw.trim().trim_matches('/');
    if cleaned.is_empty() {
        return Err(AosError::Validation(
            "repo alias must include a repository".to_string(),
        ));
    }

    let mut parts: Vec<&str> = cleaned.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Err(AosError::Validation(
            "repo alias must include a repository".to_string(),
        ));
    }

    let repo = parts.pop().unwrap_or_default().to_string();
    if repo.trim().is_empty() {
        return Err(AosError::Validation(
            "repo alias must include a repository".to_string(),
        ));
    }
    let org = if parts.is_empty() {
        "".to_string()
    } else {
        parts.join("/")
    };

    Ok((org, repo))
}

fn looks_like_commit(value: &str) -> bool {
    let len = value.len();
    if !(7..=40).contains(&len) {
        return false;
    }
    value.chars().all(|c| c.is_ascii_hexdigit())
}

// =============================================================================
// Set 33 Point 1 Task 2: CLI Scope Filtering Configuration
// =============================================================================

/// CLI arguments for filtering repository scope during ingestion.
///
/// This struct encapsulates path and extension filtering options that can be
/// flattened into command arguments. These filters control which files are
/// processed during ingestion, separate from metadata overrides.
///
/// # Usage
///
/// ```bash
/// # Include only specific paths
/// aosctl adapter codebase ingest --repo . --filter-include-paths src/,lib/
///
/// # Exclude specific paths
/// aosctl adapter codebase ingest --repo . --filter-exclude-paths tests/,vendor/
///
/// # Filter by extension
/// aosctl adapter codebase ingest --repo . --filter-include-extensions rs,py,ts
/// ```
#[derive(Debug, Clone, Default, Args, Serialize, Deserialize)]
pub struct CodebaseScopeFilters {
    /// Paths to include in ingestion (comma-separated, e.g., "src/,lib/").
    /// If specified, only files under these paths are processed.
    #[arg(
        long = "filter-include-paths",
        value_delimiter = ',',
        value_name = "PATHS"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_include_paths: Option<Vec<String>>,

    /// Paths to exclude from ingestion (comma-separated, e.g., "tests/,vendor/").
    /// Files under these paths are skipped even if they match include patterns.
    #[arg(
        long = "filter-exclude-paths",
        value_delimiter = ',',
        value_name = "PATHS"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_exclude_paths: Option<Vec<String>>,

    /// File extensions to include (comma-separated, e.g., "rs,py,ts").
    /// If specified, only files with these extensions are processed.
    #[arg(
        long = "filter-include-extensions",
        value_delimiter = ',',
        value_name = "EXTS"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_include_extensions: Option<Vec<String>>,

    /// File extensions to exclude (comma-separated, e.g., "md,txt,json").
    /// Files with these extensions are skipped even if they match include patterns.
    #[arg(
        long = "filter-exclude-extensions",
        value_delimiter = ',',
        value_name = "EXTS"
    )]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_exclude_extensions: Option<Vec<String>>,
}

impl CodebaseScopeFilters {
    /// Check if any filters are configured.
    pub fn has_filters(&self) -> bool {
        self.filter_include_paths.is_some()
            || self.filter_exclude_paths.is_some()
            || self.filter_include_extensions.is_some()
            || self.filter_exclude_extensions.is_some()
    }

    /// Convert to the orchestrator's RepoScopeConfig type.
    pub fn to_repo_scope_config(&self) -> RepoScopeConfig {
        RepoScopeConfig {
            include_paths: self.filter_include_paths.clone().unwrap_or_default(),
            exclude_paths: self.filter_exclude_paths.clone().unwrap_or_default(),
            include_extensions: self.filter_include_extensions.clone().unwrap_or_default(),
            exclude_extensions: self.filter_exclude_extensions.clone().unwrap_or_default(),
        }
    }

    /// Log the effective filters for debugging.
    pub fn log_filters(&self) {
        if self.has_filters() {
            info!("Codebase scope filters active:");
            if let Some(ref paths) = self.filter_include_paths {
                info!("  include_paths: {:?}", paths);
            }
            if let Some(ref paths) = self.filter_exclude_paths {
                info!("  exclude_paths: {:?}", paths);
            }
            if let Some(ref exts) = self.filter_include_extensions {
                info!("  include_extensions: {:?}", exts);
            }
            if let Some(ref exts) = self.filter_exclude_extensions {
                info!("  exclude_extensions: {:?}", exts);
            }
        }
    }
}

// =============================================================================
// Set 32 Point 1: Advanced Determinism Configuration
// =============================================================================

/// CLI arguments for advanced determinism controls during training.
///
/// These flags provide fine-grained control over determinism for reproducible builds.
/// All settings are optional and work alongside the basic `--seed` flag.
///
/// # Usage
///
/// ```bash
/// # Basic deterministic training with fixed seed
/// aosctl adapter codebase ingest --repo . --seed 42
///
/// # Full determinism with fixed timestamp (for CI/CD reproducibility)
/// aosctl adapter codebase ingest --repo . --seed 42 \
///     --fixed-timestamp="2025-01-15T10:00:00Z" \
///     --stable-ordering --strict-determinism
///
/// # Trace seed derivations for debugging
/// aosctl adapter codebase ingest --repo . --seed 42 --trace-seeds
/// ```
#[derive(Debug, Clone, Default, Args, Serialize, Deserialize)]
pub struct DeterminismArgs {
    /// Fixed timestamp for reproducible builds (ISO 8601 format).
    /// When set, all time-dependent operations use this fixed timestamp.
    /// Example: --fixed-timestamp="2025-01-15T10:00:00Z"
    #[arg(long, value_name = "TIMESTAMP")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_timestamp: Option<String>,

    /// Enable stable ordering for all hash and sort operations.
    /// Forces deterministic iteration order for collections.
    #[arg(long)]
    #[serde(default)]
    pub stable_ordering: bool,

    /// Enable strict determinism validation.
    /// Errors on any operation that could introduce non-determinism.
    #[arg(long)]
    #[serde(default)]
    pub strict_determinism: bool,

    /// Trace all seed derivations for debugging.
    /// Logs detailed information about how seeds are derived.
    #[arg(long)]
    #[serde(default)]
    pub trace_seeds: bool,
}

impl DeterminismArgs {
    /// Check if any determinism settings are configured.
    pub fn is_configured(&self) -> bool {
        self.fixed_timestamp.is_some()
            || self.stable_ordering
            || self.strict_determinism
            || self.trace_seeds
    }

    /// Log the effective determinism settings for debugging.
    pub fn log_settings(&self, seed: Option<u64>) {
        if self.is_configured() || seed.is_some() {
            info!("Determinism settings active:");
            if let Some(s) = seed {
                info!("  seed: {}", s);
            }
            if let Some(ref ts) = self.fixed_timestamp {
                info!("  fixed_timestamp: {}", ts);
            }
            if self.stable_ordering {
                info!("  stable_ordering: true");
            }
            if self.strict_determinism {
                info!("  strict_determinism: true");
            }
            if self.trace_seeds {
                info!("  trace_seeds: true");
            }
        }
    }

    /// Convert to DeterminismConfig for the core seed system.
    ///
    /// Returns None if no determinism settings are configured.
    /// Returns Err if fixed_timestamp is provided but malformed.
    pub fn to_determinism_config(
        &self,
        seed: Option<u64>,
    ) -> Result<Option<adapteros_core::seed::DeterminismConfig>> {
        use adapteros_core::seed::DeterminismConfig;
        use chrono::DateTime;

        // If nothing configured, return None (use defaults)
        if !self.is_configured() && seed.is_none() {
            return Ok(None);
        }

        let mut config = DeterminismConfig::new();

        if let Some(seed_value) = seed {
            config.fixed_seed = Some(seed_value);
        }

        if let Some(ref ts_str) = self.fixed_timestamp {
            let ts = DateTime::parse_from_rfc3339(ts_str)
                .map_err(|e| {
                    AosError::Validation(format!(
                        "Invalid --fixed-timestamp format '{}': {}. Expected ISO 8601 (e.g., 2025-01-15T10:00:00Z)",
                        ts_str, e
                    ))
                })?
                .with_timezone(&chrono::Utc);
            config.fixed_timestamp = Some(ts);
        }

        config.stable_ordering = self.stable_ordering;
        config.strict_mode = self.strict_determinism;
        config.trace_seeds = self.trace_seeds;

        Ok(Some(config))
    }
}

// =============================================================================
// Set 8 Point 1: Dataset Lineage CLI Parameters
// =============================================================================

/// CLI arguments for dataset lineage provenance tracking.
#[derive(Debug, Clone, Default, Args, Serialize, Deserialize)]
pub struct DatasetLineageArgs {
    /// Parent dataset ID (single-parent lineage).
    #[arg(long = "parent-dataset-id", value_name = "DATASET_ID")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_dataset_id: Option<String>,

    /// Human-readable label for the lineage relationship.
    #[arg(long = "lineage-label", value_name = "LABEL")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_label: Option<String>,

    /// Source dataset IDs this dataset is derived from (comma-separated).
    #[arg(
        long = "derived-from",
        value_name = "DATASET_ID",
        value_delimiter = ','
    )]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,

    /// Explicit lineage version tag.
    #[arg(long = "lineage-version", value_name = "VERSION")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_version: Option<String>,

    /// Additional lineage metadata (repeatable key=value pairs).
    #[arg(long = "lineage-metadata", value_name = "KEY=VALUE")]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage_metadata: Vec<String>,
}

impl DatasetLineageArgs {
    pub fn has_lineage(&self) -> bool {
        self.parent_dataset_id.is_some()
            || self.lineage_label.is_some()
            || !self.derived_from.is_empty()
            || self.lineage_version.is_some()
            || !self.lineage_metadata.is_empty()
    }

    pub fn build_lineage(&self) -> Result<Option<DatasetLineageInfo>> {
        if !self.has_lineage() {
            return Ok(None);
        }

        let parent_dataset_id = self
            .parent_dataset_id
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string());
        let lineage_label = self
            .lineage_label
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string());
        let lineage_version = self
            .lineage_version
            .as_ref()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string());
        let derived_from = self
            .derived_from
            .iter()
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .map(|v| v.to_string())
            .collect::<Vec<_>>();
        let metadata = parse_lineage_metadata(&self.lineage_metadata)?;

        let lineage = DatasetLineageInfo {
            parent_dataset_id,
            lineage_label,
            derived_from,
            version: lineage_version,
            metadata,
        };

        Ok(lineage.has_lineage().then_some(lineage))
    }
}

fn parse_lineage_metadata(entries: &[String]) -> Result<BTreeMap<String, String>> {
    let mut metadata = BTreeMap::new();
    for entry in entries {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(2, '=');
        let key = parts.next().unwrap_or("").trim();
        let value = parts.next().unwrap_or("").trim();
        if key.is_empty() || value.is_empty() {
            return Err(AosError::Validation(format!(
                "Invalid --lineage-metadata entry '{}': expected key=value",
                entry
            )));
        }
        if metadata.contains_key(key) {
            return Err(AosError::Validation(format!(
                "Duplicate --lineage-metadata key '{}'",
                key
            )));
        }
        metadata.insert(key.to_string(), value.to_string());
    }
    Ok(metadata)
}

// =============================================================================
// Main Codebase Ingest Command Arguments
// =============================================================================

/// Arguments for codebase ingestion and adapter training
#[derive(Debug, Clone, Args)]
pub struct CodebaseIngestArgs {
    /// Repository path or git URL
    #[arg(long)]
    pub repo: String,

    /// Repository slug for adapter naming and provenance tracking.
    /// Used to generate adapter IDs in format: code.<repo_slug>.<commit>
    /// If not provided, auto-derived from repository name.
    #[arg(long = "repo-slug", value_name = "SLUG")]
    pub repo_slug: Option<String>,

    /// Adapter ID override (defaults to code.<repo_slug>.<commit>)
    #[arg(long)]
    pub adapter_id: Option<String>,

    /// Logical project name for metadata
    #[arg(long)]
    pub project_name: Option<String>,

    /// Registry repo identifier override
    #[arg(long)]
    pub repo_id: Option<String>,

    /// Git branch to use for ingestion.
    /// If not specified, uses the current branch of the repository.
    #[arg(long, value_name = "BRANCH")]
    pub branch: Option<String>,

    /// Git commit SHA to use for ingestion.
    /// If not specified, uses the current commit of the repository.
    #[arg(long, value_name = "SHA")]
    pub commit: Option<String>,

    /// Output directory for `.aos` artifacts
    #[arg(long, default_value = "./adapters")]
    pub output_dir: PathBuf,

    /// Base model name for metadata
    #[arg(long, default_value = "qwen2.5-7b")]
    pub base_model: String,

    /// Maximum number of symbols to sample per repo
    #[arg(long, default_value_t = 64)]
    pub max_symbols: usize,

    /// Include private symbols in dataset
    #[arg(long)]
    pub include_private: bool,

    /// Positive sample weight
    #[arg(long, default_value_t = 1.0)]
    pub positive_weight: f32,

    /// Negative sample weight for abstention pairs
    #[arg(long, default_value_t = -0.5)]
    pub negative_weight: f32,

    /// Skip registry registration
    #[arg(long)]
    pub skip_register: bool,

    /// Registry tier (integer)
    #[arg(long, default_value_t = 1)]
    pub tier: i32,

    /// Deterministic seed override
    #[arg(long)]
    pub seed: Option<u64>,

    /// Tokenizer configuration
    #[command(flatten)]
    pub tokenizer_arg: TokenizerArg,

    /// Common training hyperparameters
    #[command(flatten)]
    pub common: CommonTrainingArgs,

    /// Advanced determinism configuration
    #[command(flatten)]
    pub determinism: DeterminismArgs,

    /// Scope metadata overrides (repo/branch/commit/scan root/remote URL)
    #[command(flatten)]
    pub scope_overrides: CodebaseScopeOverrides,

    // === Scope Configuration ===
    /// Paths to include in ingestion (comma-separated, e.g., "src/,lib/")
    #[arg(long, value_delimiter = ',')]
    pub include_paths: Option<Vec<String>>,

    /// Paths to exclude from ingestion (comma-separated, e.g., "tests/,vendor/")
    #[arg(long, value_delimiter = ',')]
    pub exclude_paths: Option<Vec<String>>,

    /// File extensions to include (comma-separated, e.g., "rs,py,ts")
    #[arg(long, value_delimiter = ',')]
    pub include_extensions: Option<Vec<String>>,

    /// File extensions to exclude (comma-separated, e.g., "md,txt,json")
    #[arg(long, value_delimiter = ',')]
    pub exclude_extensions: Option<Vec<String>>,

    // === Streaming Configuration ===
    /// Enable streaming progress output
    #[arg(long)]
    pub stream: bool,

    /// Stream output format: json or text (default: text)
    #[arg(long, default_value = "text")]
    pub stream_format: String,

    /// Minimum interval between stream events in milliseconds (0 = every event)
    #[arg(long, default_value_t = 0)]
    pub stream_interval: u64,

    // === Session Configuration ===
    /// Session ID for correlating ingestion workflows.
    /// Auto-generated when session metadata or streaming is enabled.
    #[arg(long)]
    pub session_id: Option<String>,

    /// Human-readable session name for identification
    #[arg(long)]
    pub session_name: Option<String>,

    /// Session tags for categorization (comma-separated, e.g., "ci,nightly")
    #[arg(long, value_delimiter = ',')]
    pub session_tags: Option<Vec<String>>,

    /// Dataset lineage metadata for provenance tracking
    #[command(flatten)]
    pub lineage: DatasetLineageArgs,

    /// Scan root paths within the repository (can be specified multiple times).
    /// Relative paths from the repository root that define which directories to scan.
    /// If not specified, the entire repository is scanned.
    ///
    /// Examples:
    ///   --scan-root src
    ///   --scan-root src --scan-root lib --scan-root core
    #[arg(long = "scan-root", value_name = "PATH")]
    pub scan_roots: Vec<String>,

    /// Remote URL override for provenance tracking
    #[arg(long)]
    pub remote_url: Option<String>,
}

impl CodebaseIngestArgs {
    /// Validate command arguments
    pub fn validate(&self) -> Result<()> {
        if let Some(adapter_id) = &self.adapter_id {
            validate_adapter_id(adapter_id)?;
        }

        self.common.validate()?;

        if let Some(slug) = &self.repo_slug {
            validate_repo_slug(slug)?;
        }
        if let Some(slug) = &self.scope_overrides.repo_slug {
            validate_repo_slug(slug)?;
        }
        self.validate_scope_override_consistency()?;

        if self.max_symbols == 0 {
            return Err(AosError::Validation(
                "--max-symbols must be greater than zero".to_string(),
            ));
        }

        if self.tier <= 0 {
            return Err(AosError::Validation(
                "--tier must be a positive integer".to_string(),
            ));
        }

        if (self.positive_weight - 0.0).abs() < f32::EPSILON {
            return Err(AosError::Validation(
                "--positive-weight cannot be zero".to_string(),
            ));
        }

        self.lineage.build_lineage()?;

        Ok(())
    }

    fn validate_scope_override_consistency(&self) -> Result<()> {
        if let (Some(repo_slug), Some(override_slug)) =
            (&self.repo_slug, &self.scope_overrides.repo_slug)
        {
            let normalized_repo = normalize_repo_slug(repo_slug);
            let normalized_override = normalize_repo_slug(override_slug);
            if normalized_repo != normalized_override {
                return Err(AosError::Validation(format!(
                    "--repo-slug '{}' conflicts with --override-repo-slug '{}'",
                    repo_slug, override_slug
                )));
            }
        }

        if let (Some(branch), Some(override_branch)) =
            (&self.branch, &self.scope_overrides.override_branch)
        {
            if branch.trim() != override_branch.trim() {
                return Err(AosError::Validation(format!(
                    "--branch '{}' conflicts with --override-branch '{}'",
                    branch, override_branch
                )));
            }
        }

        if let (Some(commit), Some(override_commit)) =
            (&self.commit, &self.scope_overrides.override_commit)
        {
            let normalized_commit = commit.trim().to_ascii_lowercase();
            let normalized_override = override_commit.trim().to_ascii_lowercase();
            if normalized_commit != normalized_override {
                return Err(AosError::Validation(format!(
                    "--commit '{}' conflicts with --override-commit '{}'",
                    commit, override_commit
                )));
            }
        }

        if let (Some(remote_url), Some(override_remote_url)) =
            (&self.remote_url, &self.scope_overrides.override_remote_url)
        {
            if remote_url.trim() != override_remote_url.trim() {
                return Err(AosError::Validation(format!(
                    "--remote-url '{}' conflicts with --override-remote-url '{}'",
                    remote_url, override_remote_url
                )));
            }
        }

        if let Some(override_scan_root) = &self.scope_overrides.override_scan_root {
            if !self.scan_roots.is_empty() {
                if self.scan_roots.len() == 1 {
                    let configured = Self::normalize_scan_root_for_compare(&self.scan_roots[0]);
                    let override_value = Self::normalize_scan_root_for_compare(override_scan_root);
                    if configured != override_value {
                        return Err(AosError::Validation(format!(
                            "--scan-root '{}' conflicts with --override-scan-root '{}'",
                            self.scan_roots[0], override_scan_root
                        )));
                    }
                } else {
                    return Err(AosError::Validation(
                        "--override-scan-root cannot be combined with multiple --scan-root values"
                            .to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    fn normalize_scan_root_for_compare(value: &str) -> String {
        let mut normalized = value.trim().replace('\\', "/");
        while normalized.starts_with("./") {
            normalized = normalized.trim_start_matches("./").to_string();
        }
        normalized.trim_end_matches('/').to_string()
    }

    // =========================================================================
    // Set 33 Point 1 Task 2: Scope Configuration Helpers
    // =========================================================================

    /// Build RepoScopeConfig from CLI arguments.
    ///
    /// Merges path and extension filters from command-line arguments
    /// into a single RepoScopeConfig for the orchestrator.
    pub fn build_repo_scope_config(&self) -> RepoScopeConfig {
        RepoScopeConfig {
            include_paths: self.include_paths.clone().unwrap_or_default(),
            exclude_paths: self.exclude_paths.clone().unwrap_or_default(),
            include_extensions: self.include_extensions.clone().unwrap_or_default(),
            exclude_extensions: self.exclude_extensions.clone().unwrap_or_default(),
        }
    }

    /// Check if any scope filters are configured.
    pub fn has_scope_filters(&self) -> bool {
        self.include_paths.is_some()
            || self.exclude_paths.is_some()
            || self.include_extensions.is_some()
            || self.exclude_extensions.is_some()
    }

    /// Build CodebaseScopeMetadata from CLI arguments.
    ///
    /// Creates scope metadata using branch and remote_url overrides
    /// from command-line arguments.
    pub fn build_scope_metadata(&self) -> CodebaseScopeMetadata {
        let mut metadata = self.scope_overrides.to_scope_metadata();
        if metadata.branch.is_none() {
            metadata.branch = self.branch.clone();
        }
        if metadata.commit.is_none() {
            metadata.commit = self.commit.clone();
        }
        if metadata.repo_slug.is_none() {
            metadata.repo_slug = self.repo_slug.clone();
        }
        if metadata.repo_id.is_none() {
            metadata.repo_id = self.repo_id.clone();
        }
        if metadata.scan_root.is_none() && self.scan_roots.len() == 1 {
            metadata.scan_root = self.scan_roots.first().cloned();
        }
        if metadata.remote_url.is_none() {
            metadata.remote_url = self.remote_url.clone();
        }
        metadata
    }

    /// Check if any scope metadata overrides are configured.
    pub fn has_scope_metadata(&self) -> bool {
        self.scope_overrides.has_overrides()
            || self.branch.is_some()
            || self.commit.is_some()
            || self.repo_slug.is_some()
            || self.repo_id.is_some()
            || self.remote_url.is_some()
            || !self.scan_roots.is_empty()
    }

    /// Build StreamConfig from CLI arguments.
    pub fn build_stream_config(&self) -> StreamConfig {
        let format = StreamFormat::parse(&self.stream_format);
        let interval_ms = self.stream_interval;
        if self.stream {
            StreamConfig::new(format, interval_ms)
        } else {
            StreamConfig {
                enabled: false,
                format,
                interval_ms,
            }
        }
    }

    /// Build CodeDatasetConfig from CLI arguments.
    pub fn build_dataset_config(&self) -> CodeDatasetConfig {
        CodeDatasetConfig {
            max_symbols: self.max_symbols,
            include_private: self.include_private,
            positive_weight: self.positive_weight,
            negative_weight: self.negative_weight,
        }
    }

    /// Log scope configuration for debugging/audit.
    pub fn log_scope_config(&self) {
        if self.has_scope_filters() {
            info!("Scope filters configured:");
            if let Some(ref paths) = self.include_paths {
                info!("  include_paths: {:?}", paths);
            }
            if let Some(ref paths) = self.exclude_paths {
                info!("  exclude_paths: {:?}", paths);
            }
            if let Some(ref exts) = self.include_extensions {
                info!("  include_extensions: {:?}", exts);
            }
            if let Some(ref exts) = self.exclude_extensions {
                info!("  exclude_extensions: {:?}", exts);
            }
        }

        if self.has_scope_metadata() {
            self.scope_overrides.log_overrides();
            let mut _logged_defaults = false;
            if self.scope_overrides.repo_slug.is_none() {
                if let Some(ref slug) = self.repo_slug {
                    if !_logged_defaults {
                        info!("Scope metadata defaults:");
                        _logged_defaults = true;
                    }
                    info!("  repo_slug: {}", slug);
                }
            }
            if let Some(ref repo_id) = self.repo_id {
                if !_logged_defaults {
                    info!("Scope metadata defaults:");
                    _logged_defaults = true;
                }
                info!("  repo_id: {}", repo_id);
            }
            if self.scope_overrides.override_branch.is_none() {
                if let Some(ref branch) = self.branch {
                    if !_logged_defaults {
                        info!("Scope metadata defaults:");
                        _logged_defaults = true;
                    }
                    info!("  branch: {}", branch);
                }
            }
            if self.scope_overrides.override_commit.is_none() {
                if let Some(ref commit) = self.commit {
                    if !_logged_defaults {
                        info!("Scope metadata defaults:");
                        _logged_defaults = true;
                    }
                    info!("  commit: {}", commit);
                }
            }
            if self.scope_overrides.override_remote_url.is_none() {
                if let Some(ref remote) = self.remote_url {
                    if !_logged_defaults {
                        info!("Scope metadata defaults:");
                        _logged_defaults = true;
                    }
                    info!("  remote_url: {}", remote);
                }
            }
            if self.scope_overrides.override_scan_root.is_none() && !self.scan_roots.is_empty() {
                if !_logged_defaults {
                    info!("Scope metadata defaults:");
                    _logged_defaults = true;
                }
                info!("  scan_roots: {:?}", self.scan_roots);
            }
        }
    }
}

/// Validate repo_slug format
///
/// Repo slugs must be:
/// - 1-64 characters
/// - Lowercase alphanumeric with underscores
/// - No leading/trailing underscores
/// - No consecutive underscores
pub fn validate_repo_slug(slug: &str) -> Result<()> {
    if slug.is_empty() {
        return Err(AosError::Validation(
            "--repo-slug cannot be empty".to_string(),
        ));
    }

    if slug.len() > 64 {
        return Err(AosError::Validation(
            "--repo-slug cannot exceed 64 characters".to_string(),
        ));
    }

    if slug.starts_with('_') || slug.ends_with('_') {
        return Err(AosError::Validation(
            "--repo-slug cannot start or end with underscore".to_string(),
        ));
    }

    if slug.contains("__") {
        return Err(AosError::Validation(
            "--repo-slug cannot contain consecutive underscores".to_string(),
        ));
    }

    for c in slug.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '_' {
            return Err(AosError::Validation(format!(
                "--repo-slug contains invalid character '{}': must be lowercase alphanumeric or underscore",
                c
            )));
        }
    }

    Ok(())
}

/// Run the codebase ingestion command
pub async fn run(args: &CodebaseIngestArgs, output: &OutputWriter) -> Result<()> {
    args.validate()?;

    let effective_repo_slug = args
        .scope_overrides
        .repo_slug
        .clone()
        .or_else(|| args.repo_slug.clone());

    let determinism_config = args.determinism.to_determinism_config(args.seed)?;
    if let Some(ref config) = determinism_config {
        adapteros_core::seed::set_determinism_config(config.clone());
    }

    let lineage = args.lineage.build_lineage()?;

    if args.negative_weight >= 0.0 {
        output.warning("--negative-weight is non-negative; abstention training may be ineffective");
    }

    // Display configuration
    output.info("Codebase ingestion configuration:");
    output.kv("Repository", &args.repo);
    if let Some(branch) = &args.branch {
        output.kv("Branch", branch);
    } else {
        output.kv("Branch", "(current branch)");
    }
    if let Some(commit) = &args.commit {
        output.kv("Commit", commit);
    } else {
        output.kv("Commit", "(current commit)");
    }
    if let Some(slug) = &effective_repo_slug {
        output.kv("Repo slug", slug);
    } else {
        output.kv("Repo slug", "(auto-derived from repo name)");
    }
    if let Some(adapter_id) = &args.adapter_id {
        output.kv("Adapter ID", adapter_id);
    } else {
        output.kv("Adapter ID", "(auto-generated: code.<slug>.<commit>)");
    }
    output.kv("Max symbols", &args.max_symbols.to_string());
    output.kv("Include private", &args.include_private.to_string());
    if args.determinism.is_configured() || args.seed.is_some() {
        output.info("Determinism overrides:");
        if let Some(seed) = args.seed {
            output.kv("Seed", &seed.to_string());
        }
        if let Some(ref ts) = args.determinism.fixed_timestamp {
            output.kv("Fixed timestamp", ts);
        }
        if args.determinism.stable_ordering {
            output.kv("Stable ordering", "true");
        }
        if args.determinism.strict_determinism {
            output.kv("Strict determinism", "true");
        }
        if args.determinism.trace_seeds {
            output.kv("Trace seeds", "true");
        }
    }

    // Display scope configuration (Set 33, Point 1, Task 2)
    if args.has_scope_filters() {
        output.info("Scope filters:");
        if let Some(ref paths) = args.include_paths {
            output.kv("Include paths", &paths.join(", "));
        }
        if let Some(ref paths) = args.exclude_paths {
            output.kv("Exclude paths", &paths.join(", "));
        }
        if let Some(ref exts) = args.include_extensions {
            output.kv("Include extensions", &exts.join(", "));
        }
        if let Some(ref exts) = args.exclude_extensions {
            output.kv("Exclude extensions", &exts.join(", "));
        }
    }

    // Display scope metadata overrides
    if args.has_scope_metadata() {
        output.info("Scope metadata overrides:");
        if let Some(ref name) = args.scope_overrides.repo_name {
            output.kv("Repo name", name);
        }
        if let Some(ref slug) = args.scope_overrides.repo_slug {
            output.kv("Repo slug override", slug);
        }
        if let Some(ref branch) = args.scope_overrides.override_branch {
            output.kv("Branch override", branch);
        } else if let Some(ref branch) = args.branch {
            output.kv("Branch", branch);
        }
        if let Some(ref commit) = args.scope_overrides.override_commit {
            output.kv("Commit override", commit);
        } else if let Some(ref commit) = args.commit {
            output.kv("Commit", commit);
        }
        if let Some(ref scan_root) = args.scope_overrides.override_scan_root {
            output.kv("Scan root override", scan_root);
        } else if !args.scan_roots.is_empty() {
            output.kv("Scan roots", &args.scan_roots.join(", "));
        }
        if let Some(ref remote) = args.scope_overrides.override_remote_url {
            output.kv("Remote URL override", remote);
        } else if let Some(ref remote) = args.remote_url {
            output.kv("Remote URL", remote);
        }
    }

    if let Some(ref lineage) = lineage {
        output.info("Dataset lineage:");
        if let Some(ref parent) = lineage.parent_dataset_id {
            output.kv("Parent dataset", parent);
        }
        if !lineage.derived_from.is_empty() {
            output.kv("Derived from", &lineage.derived_from.join(", "));
        }
        if let Some(ref label) = lineage.lineage_label {
            output.kv("Lineage label", label);
        }
        if let Some(ref version) = lineage.version {
            output.kv("Lineage version", version);
        }
        if !lineage.metadata.is_empty() {
            let pairs = lineage
                .metadata
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>();
            output.kv("Lineage metadata", &pairs.join(", "));
        }
    }

    // Log scope configuration for tracing
    args.log_scope_config();
    args.determinism.log_settings(args.seed);

    let tokenizer_path = resolve_tokenizer_path(args.tokenizer_arg.tokenizer.as_ref())?;
    let tokenizer = QwenTokenizer::from_file(&tokenizer_path)?;
    let pad_token_id = tokenizer.pad_token_id().ok_or_else(|| {
        AosError::Validation("Tokenizer missing pad_token_id for codebase training".to_string())
    })?;
    let vocab_size = tokenizer.vocab_size(true);
    let ignore_index = i32::try_from(pad_token_id)
        .map_err(|_| AosError::Validation("pad_token_id exceeds i32 range".to_string()))?;

    let source = resolve_repo_source(&args.repo)?;
    let mut training_config = TrainingConfig {
        rank: args.common.rank,
        alpha: args.common.alpha,
        learning_rate: args.common.learning_rate,
        batch_size: args.common.batch_size,
        epochs: args.common.epochs,
        hidden_dim: args.common.hidden_dim,
        vocab_size,
        pad_token_id,
        ignore_index,
        ..TrainingConfig::default()
    };

    if args.seed.is_some() {
        training_config.determinism = Some(TrainingDeterminismConfig {
            seed: args.seed,
            ..Default::default()
        });
    }

    let mut repo_scope = args.build_repo_scope_config();
    if !args.scan_roots.is_empty() {
        if repo_scope.include_paths.is_empty() {
            repo_scope.include_paths = args.scan_roots.clone();
        } else {
            repo_scope
                .include_paths
                .extend(args.scan_roots.iter().cloned());
        }
    }
    let repo_scope = Some(repo_scope);

    let scope_metadata = if args.has_scope_metadata() {
        Some(args.build_scope_metadata())
    } else {
        None
    };

    let stream = Some(args.build_stream_config());

    let session_id =
        match args.session_id.as_ref() {
            Some(raw) => Some(Uuid::parse_str(raw).map_err(|e| {
                AosError::Validation(format!("Invalid --session-id '{}': {}", raw, e))
            })?),
            None => {
                if args.session_name.is_some() || args.session_tags.is_some() || args.stream {
                    Some(Uuid::now_v7())
                } else {
                    None
                }
            }
        };

    let request = CodeIngestionRequest {
        source,
        tokenizer_path,
        training_config,
        dataset: args.build_dataset_config(),
        output_dir: args.output_dir.clone(),
        adapter_id: args.adapter_id.clone(),
        base_model: args.base_model.clone(),
        register: !args.skip_register,
        tier: args.tier,
        repo_id: args.repo_id.clone(),
        project_name: args.project_name.clone(),
        seed: args.seed,
        determinism_config,
        session_name: args.session_name.clone(),
        session_tags: args.session_tags.clone(),
        session_id,
        repo_scope,
        scan_roots: args.scan_roots.clone(),
        stream,
        scope_metadata,
        lineage,
        adapter_scope: None,
        repo_slug: effective_repo_slug,
    };

    let pipeline = CodeIngestionPipeline::new();
    let result = pipeline.run(request).await?;

    output.success("Codebase ingestion completed");
    output.kv("Adapter ID", &result.adapter_id);
    output.kv(
        "Repo",
        &format!("{} ({})", result.repo_name, result.repo_slug),
    );
    output.kv("Repo identifier", &result.repo_identifier);
    if let Some(branch) = &result.branch {
        output.kv("Branch", branch);
    } else {
        output.kv("Branch", "(detached)");
    }
    output.kv("Commit", &result.commit_sha);
    output.kv("Dataset hash", &result.dataset_hash);
    output.kv("Examples", &result.dataset_examples.to_string());
    output.kv("AOS path", &result.aos_path.display().to_string());
    output.kv("AOS hash", &result.aos_hash_b3);
    if let Some(registry_id) = &result.registry_id {
        output.kv("Registry ID", registry_id);
    }

    Ok(())
}

/// Resolve repository source from path or URL
fn resolve_repo_source(repo: &str) -> Result<CodeIngestionSource> {
    let path_candidate = Path::new(repo);
    if path_candidate.exists() {
        let absolute = std::fs::canonicalize(path_candidate).map_err(|e| {
            AosError::Io(format!("Failed to canonicalize repo path {}: {}", repo, e))
        })?;
        Ok(CodeIngestionSource::LocalPath(absolute))
    } else {
        Ok(CodeIngestionSource::GitUrl(repo.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_ingest_args() -> CodebaseIngestArgs {
        CodebaseIngestArgs {
            repo: ".".to_string(),
            repo_slug: None,
            adapter_id: None,
            project_name: None,
            repo_id: None,
            branch: None,
            commit: None,
            output_dir: PathBuf::from("./adapters"),
            base_model: "qwen2.5-7b".to_string(),
            max_symbols: 64,
            include_private: false,
            positive_weight: 1.0,
            negative_weight: -0.5,
            skip_register: false,
            tier: 1,
            seed: None,
            tokenizer_arg: TokenizerArg { tokenizer: None },
            common: CommonTrainingArgs {
                rank: 16,
                alpha: 32.0,
                learning_rate: 0.0001,
                batch_size: 8,
                epochs: 3,
                hidden_dim: 768,
            },
            determinism: DeterminismArgs::default(),
            scope_overrides: CodebaseScopeOverrides::default(),
            include_paths: None,
            exclude_paths: None,
            include_extensions: None,
            exclude_extensions: None,
            stream: false,
            stream_format: "text".to_string(),
            stream_interval: 0,
            session_id: None,
            session_name: None,
            session_tags: None,
            lineage: DatasetLineageArgs::default(),
            scan_roots: Vec::new(),
            remote_url: None,
        }
    }

    #[test]
    fn test_resolve_adapter_alias_repo_branch() {
        let alias = resolve_adapter_alias("org/repo@main").unwrap();
        assert_eq!(
            alias,
            AdapterAliasKind::RepoRef {
                org: "org".to_string(),
                repo: "repo".to_string(),
                ref_type: RepoRefType::Branch("main".to_string()),
            }
        );
    }

    #[test]
    fn test_resolve_adapter_alias_repo_commit() {
        let alias = resolve_adapter_alias("org/repo@abcdef1").unwrap();
        assert_eq!(
            alias,
            AdapterAliasKind::RepoRef {
                org: "org".to_string(),
                repo: "repo".to_string(),
                ref_type: RepoRefType::Commit("abcdef1".to_string()),
            }
        );
    }

    #[test]
    fn test_resolve_adapter_alias_full_id() {
        let alias = resolve_adapter_alias("code.my_repo.abcdef1").unwrap();
        assert_eq!(
            alias,
            AdapterAliasKind::FullId("code.my_repo.abcdef1".to_string())
        );
    }

    #[test]
    fn test_resolve_adapter_alias_short_alias() {
        let alias = resolve_adapter_alias("codebase-demo").unwrap();
        assert_eq!(
            alias,
            AdapterAliasKind::ShortAlias("codebase-demo".to_string())
        );
    }

    #[test]
    fn test_resolve_adapter_alias_empty() {
        let err = resolve_adapter_alias(" ").unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_repo_slug_valid() {
        assert!(validate_repo_slug("myrepo").is_ok());
        assert!(validate_repo_slug("my_repo").is_ok());
        assert!(validate_repo_slug("repo123").is_ok());
        assert!(validate_repo_slug("my_repo_123").is_ok());
        assert!(validate_repo_slug("a").is_ok());
    }

    #[test]
    fn test_validate_repo_slug_empty() {
        let err = validate_repo_slug("").unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_repo_slug_too_long() {
        let long_slug = "a".repeat(65);
        let err = validate_repo_slug(&long_slug).unwrap_err();
        assert!(err.to_string().contains("64 characters"));
    }

    #[test]
    fn test_validate_repo_slug_leading_underscore() {
        let err = validate_repo_slug("_myrepo").unwrap_err();
        assert!(err.to_string().contains("start or end with underscore"));
    }

    #[test]
    fn test_validate_repo_slug_trailing_underscore() {
        let err = validate_repo_slug("myrepo_").unwrap_err();
        assert!(err.to_string().contains("start or end with underscore"));
    }

    #[test]
    fn test_validate_repo_slug_consecutive_underscores() {
        let err = validate_repo_slug("my__repo").unwrap_err();
        assert!(err.to_string().contains("consecutive underscores"));
    }

    #[test]
    fn test_validate_repo_slug_uppercase() {
        let err = validate_repo_slug("MyRepo").unwrap_err();
        assert!(err.to_string().contains("invalid character"));
    }

    #[test]
    fn test_validate_repo_slug_special_chars() {
        assert!(validate_repo_slug("my-repo").is_err());
        assert!(validate_repo_slug("my.repo").is_err());
        assert!(validate_repo_slug("my@repo").is_err());
        assert!(validate_repo_slug("my repo").is_err());
    }

    // =========================================================================
    // Set 22 Point 1: CodebaseScopeOverrides tests
    // =========================================================================

    #[test]
    fn test_scope_overrides_has_overrides_empty() {
        let overrides = CodebaseScopeOverrides::default();
        assert!(!overrides.has_overrides());
    }

    #[test]
    fn test_scope_overrides_has_overrides_with_commit() {
        let overrides = CodebaseScopeOverrides {
            override_commit: Some("abc123".to_string()),
            ..Default::default()
        };
        assert!(overrides.has_overrides());
    }

    #[test]
    fn test_scope_overrides_has_overrides_all_fields() {
        let overrides = CodebaseScopeOverrides {
            repo_name: Some("my-repo".to_string()),
            repo_slug: Some("my_repo".to_string()),
            override_branch: Some("main".to_string()),
            override_commit: Some("abc123def".to_string()),
            override_scan_root: Some("src/".to_string()),
            override_remote_url: Some("https://github.com/org/repo".to_string()),
        };
        assert!(overrides.has_overrides());
    }

    #[test]
    fn test_scope_overrides_to_scope_metadata() {
        let overrides = CodebaseScopeOverrides {
            repo_name: Some("test-repo".to_string()),
            repo_slug: Some("test_repo".to_string()),
            override_branch: Some("feature".to_string()),
            override_commit: Some("abc123".to_string()),
            override_scan_root: Some("src/".to_string()),
            override_remote_url: Some("https://github.com/org/repo".to_string()),
        };

        let metadata = overrides.to_scope_metadata();
        assert_eq!(metadata.repo, Some("test-repo".to_string()));
        assert_eq!(metadata.repo_slug, Some("test_repo".to_string()));
        assert_eq!(metadata.branch, Some("feature".to_string()));
        assert_eq!(metadata.commit, Some("abc123".to_string()));
        assert_eq!(metadata.scan_root, Some("src/".to_string()));
        assert_eq!(
            metadata.remote_url,
            Some("https://github.com/org/repo".to_string())
        );
    }

    #[test]
    fn test_scope_overrides_serialization() {
        let overrides = CodebaseScopeOverrides {
            repo_name: Some("test-repo".to_string()),
            repo_slug: Some("test_repo".to_string()),
            override_commit: Some("abc123".to_string()),
            ..Default::default()
        };

        let json = serde_json::to_string(&overrides).unwrap();
        let deserialized: CodebaseScopeOverrides = serde_json::from_str(&json).unwrap();

        assert_eq!(overrides.repo_name, deserialized.repo_name);
        assert_eq!(overrides.repo_slug, deserialized.repo_slug);
        assert_eq!(overrides.override_commit, deserialized.override_commit);
        assert_eq!(overrides.override_branch, deserialized.override_branch);
    }

    #[test]
    fn test_scope_override_commit_conflict() {
        let mut args = sample_ingest_args();
        args.commit = Some("ABC123".to_string());
        args.scope_overrides.override_commit = Some("def456".to_string());

        let err = args.validate_scope_override_consistency().unwrap_err();
        assert!(err.to_string().contains("--commit"));
    }

    #[test]
    fn test_scope_override_commit_case_insensitive_match() {
        let mut args = sample_ingest_args();
        args.commit = Some("ABC123".to_string());
        args.scope_overrides.override_commit = Some("abc123".to_string());

        assert!(args.validate_scope_override_consistency().is_ok());
    }

    // =========================================================================
    // Set 33 Point 1 Task 2: CodebaseScopeFilters tests
    // =========================================================================

    #[test]
    fn test_scope_filters_has_filters_empty() {
        let filters = CodebaseScopeFilters::default();
        assert!(!filters.has_filters());
    }

    #[test]
    fn test_scope_filters_has_filters_with_include_paths() {
        let filters = CodebaseScopeFilters {
            filter_include_paths: Some(vec!["src/".to_string()]),
            ..Default::default()
        };
        assert!(filters.has_filters());
    }

    #[test]
    fn test_scope_filters_has_filters_with_exclude_extensions() {
        let filters = CodebaseScopeFilters {
            filter_exclude_extensions: Some(vec!["md".to_string(), "txt".to_string()]),
            ..Default::default()
        };
        assert!(filters.has_filters());
    }

    #[test]
    fn test_scope_filters_to_repo_scope_config() {
        let filters = CodebaseScopeFilters {
            filter_include_paths: Some(vec!["src/".to_string(), "lib/".to_string()]),
            filter_exclude_paths: Some(vec!["tests/".to_string()]),
            filter_include_extensions: Some(vec!["rs".to_string(), "py".to_string()]),
            filter_exclude_extensions: Some(vec!["md".to_string()]),
        };

        let config = filters.to_repo_scope_config();
        assert_eq!(
            config.include_paths,
            vec!["src/".to_string(), "lib/".to_string()]
        );
        assert_eq!(config.exclude_paths, vec!["tests/".to_string()]);
        assert_eq!(
            config.include_extensions,
            vec!["rs".to_string(), "py".to_string()]
        );
        assert_eq!(config.exclude_extensions, vec!["md".to_string()]);
    }

    #[test]
    fn test_scope_filters_to_repo_scope_config_empty() {
        let filters = CodebaseScopeFilters::default();
        let config = filters.to_repo_scope_config();

        assert!(config.include_paths.is_empty());
        assert!(config.exclude_paths.is_empty());
        assert!(config.include_extensions.is_empty());
        assert!(config.exclude_extensions.is_empty());
    }

    #[test]
    fn test_scope_filters_serialization() {
        let filters = CodebaseScopeFilters {
            filter_include_paths: Some(vec!["src/".to_string()]),
            filter_exclude_extensions: Some(vec!["md".to_string()]),
            ..Default::default()
        };

        let json = serde_json::to_string(&filters).unwrap();
        let deserialized: CodebaseScopeFilters = serde_json::from_str(&json).unwrap();

        assert_eq!(
            filters.filter_include_paths,
            deserialized.filter_include_paths
        );
        assert_eq!(
            filters.filter_exclude_extensions,
            deserialized.filter_exclude_extensions
        );
        assert_eq!(
            filters.filter_exclude_paths,
            deserialized.filter_exclude_paths
        );
    }
}
