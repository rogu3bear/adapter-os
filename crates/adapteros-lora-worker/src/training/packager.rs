//! Adapter packaging with safetensors and manifest generation
//!
//! Packages trained LoRA adapters into a format compatible with mplora-artifacts.

use super::quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
use super::trainer::{MoETrainingConfig, TrainingConfig};
use super::{
    LORA_Q15_DENOM, LORA_Q15_QUANTIZATION, LORA_Q15_VERSION, LORA_STRENGTH_DEFAULTS_VERSION,
    LORA_STRENGTH_DEFAULT_MAX, LORA_STRENGTH_DEFAULT_MICRO, LORA_STRENGTH_DEFAULT_STANDARD,
};
use adapteros_aos::{AosWriter, BackendTag};
use adapteros_core::{AosError, RepoAdapterPaths, Result};
use adapteros_crypto::Keypair;
use adapteros_lora_router::ROUTER_GATE_Q15_DENOM;
use adapteros_normalization::normalize_repo_slug;
use adapteros_storage::{ByteStorage, FsByteStorage, StorageKey};
use adapteros_types::coreml::{
    CoreMLOpKind, CoreMLPlacementBinding, CoreMLPlacementShape, CoreMLPlacementSpec,
    CoreMLProjection, CoreMLTargetRef,
};
use adapteros_types::training::LoraTier;
use safetensors::tensor::TensorView;
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use walkdir::WalkDir;

const DEFAULT_ARTIFACT_HARD_QUOTA_BYTES: u64 = 10 * 1024 * 1024 * 1024; // 10 GiB
const DEFAULT_ARTIFACT_SOFT_PCT: f64 = 0.8;

/// Adapter packager.
/// Adapter-only invariant: only LoRA deltas are ever exported; base model
/// weights remain outside the package boundary.
#[derive(Debug)]
pub struct AdapterPackager {
    repo_root: PathBuf,
}

/// Packaged adapter with all metadata
#[derive(Debug, Clone)]
pub struct PackagedAdapter {
    pub adapter_id: String,
    pub manifest: AdapterManifest,
    pub weights_path: PathBuf,
    pub hash_b3: String,
}

/// Adapter manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub version: String,
    pub rank: usize,
    pub base_model: String,
    #[serde(default)]
    pub base_model_hash: Option<String>,
    pub training_config: TrainingConfig,
    pub created_at: String,
    pub weights_hash: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default = "default_tier")]
    pub tier: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_layer_hashes: Option<std::collections::HashMap<String, LayerHash>>,
    #[serde(default)]
    pub training_backend: Option<String>,
    #[serde(default = "default_determinism_mode")]
    pub determinism: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora_tier: Option<LoraTier>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lora_strength: Option<f32>,
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default = "default_recommended_for_moe")]
    pub recommended_for_moe: bool,
    #[serde(default)]
    pub quantization: Option<String>,
    #[serde(default)]
    pub gate_q15_denominator: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml: Option<CoremlTrainingMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<AdapterPlacement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coreml_placement: Option<CoremlPlacementSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_backend_details: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset_version_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_spec_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_lineage_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub synthetic_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub training_slice_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_version: Option<String>,
    /// MoE (Mixture of Experts) training configuration (if trained for MoE model)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moe_config: Option<MoETrainingConfig>,
    /// Codebase scope: repository name or identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_repo: Option<String>,
    /// Codebase scope: branch name (e.g., "main", "feature/xyz")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_branch: Option<String>,
    /// Codebase scope: commit SHA at training time
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_commit: Option<String>,
    /// Codebase scope: primary scan root path used during ingestion
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_scan_root: Option<String>,
    /// Codebase scope: remote URL of the repository
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_remote_url: Option<String>,
    /// Codebase scope: normalized repository slug (e.g., "my_project" from "My-Project")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_slug: Option<String>,
    /// All scan root paths used during training package creation.
    /// Captures multiple roots when training combines content from different directories.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scan_roots: Vec<ScanRootMetadata>,
    /// Session identifier for correlating ingestion workflows
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Human-readable session name for ingestion workflows
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_name: Option<String>,
    /// Session tags for categorization (comma or JSON array in metadata)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_tags: Option<Vec<String>>,
    /// Whether stream mode was enabled during training/ingestion.
    /// Stream mode enables real-time progress updates during the training pipeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_mode: Option<bool>,
    pub metadata: std::collections::HashMap<String, String>,
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

fn default_determinism_mode() -> String {
    if cfg!(feature = "deterministic-only") {
        "deterministic-only".to_string()
    } else {
        "best-effort".to_string()
    }
}

fn default_category() -> String {
    "domain-adapter".to_string()
}

fn default_tier() -> String {
    "warm".to_string()
}

fn default_scope() -> String {
    "project".to_string()
}

fn default_recommended_for_moe() -> bool {
    true
}

fn normalize_optional_str(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
}

fn normalize_scan_roots(mut roots: Vec<ScanRootMetadata>) -> Vec<ScanRootMetadata> {
    for root in roots.iter_mut() {
        let trimmed = root.path.trim();
        if trimmed != root.path {
            root.path = trimmed.to_string();
        }
    }
    roots.retain(|root| !root.path.is_empty());
    roots
}

fn parse_lora_tier(metadata: &HashMap<String, String>) -> Option<LoraTier> {
    metadata.get("lora_tier").and_then(|v| match v.as_str() {
        "micro" => Some(LoraTier::Micro),
        "standard" => Some(LoraTier::Standard),
        "max" => Some(LoraTier::Max),
        _ => None,
    })
}

fn parse_metadata_bool(metadata: &HashMap<String, String>, key: &str) -> Option<bool> {
    metadata
        .get(key)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "y"))
}

fn default_strength_for_tier(tier: Option<LoraTier>) -> Option<f32> {
    match tier {
        Some(LoraTier::Micro) => Some(LORA_STRENGTH_DEFAULT_MICRO),
        Some(LoraTier::Standard) => Some(LORA_STRENGTH_DEFAULT_STANDARD),
        Some(LoraTier::Max) => Some(LORA_STRENGTH_DEFAULT_MAX),
        None => None,
    }
}

/// Parse scan-root metadata from the metadata HashMap.
///
/// Supports two formats:
/// 1. JSON array in `scan_roots` key: `[{"path": "src", "label": "main"}, ...]`
/// 2. Single scan root from canonical scan-root keys (scan_root_relative, scope_scan_root,
///    scan_root_path, repo_root_path, repo_path) with optional supporting fields
fn parse_scan_roots_from_metadata(metadata: &HashMap<String, String>) -> Vec<ScanRootMetadata> {
    // Try parsing JSON array first
    if let Some(raw) = metadata.get("scan_roots") {
        if let Ok(mut roots) = serde_json::from_str::<Vec<ScanRootMetadata>>(raw) {
            roots = normalize_scan_roots(roots);
            if !roots.is_empty() {
                return roots;
            }
        }
        if let Ok(paths) = serde_json::from_str::<Vec<String>>(raw) {
            let roots = normalize_scan_roots(
                paths
                    .into_iter()
                    .map(|path| ScanRootMetadata {
                        path,
                        label: None,
                        file_count: None,
                        byte_count: None,
                        content_hash: None,
                        scanned_at: None,
                    })
                    .collect(),
            );
            if !roots.is_empty() {
                return roots;
            }
        }
    }

    // Fall back to single scan root from scope_scan_root
    if let Some(path) = resolve_scan_root_from_metadata(metadata) {
        let root = ScanRootMetadata {
            path,
            label: metadata.get("scan_root_label").cloned(),
            file_count: metadata
                .get("scan_root_file_count")
                .and_then(|v| v.parse().ok()),
            byte_count: metadata
                .get("scan_root_byte_count")
                .and_then(|v| v.parse().ok()),
            content_hash: metadata.get("scan_root_content_hash").cloned(),
            scanned_at: metadata.get("scan_root_scanned_at").cloned(),
        };
        return vec![root];
    }

    Vec::new()
}

fn resolve_scan_root_from_metadata(metadata: &HashMap<String, String>) -> Option<String> {
    let candidates = [
        metadata.get("scan_root_relative"),
        metadata.get("scope_scan_root"),
        metadata.get("scan_root_path"),
        metadata.get("repo_root_path"),
        metadata.get("repo_path"),
    ];

    for path in candidates.into_iter().flatten() {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

fn parse_bool_strict(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

fn metadata_has_scan_root_keys(metadata: &HashMap<String, String>) -> bool {
    let keys = [
        "scan_roots",
        "scan_root_relative",
        "scope_scan_root",
        "scan_root_path",
        "repo_root_path",
        "repo_path",
    ];
    keys.iter().any(|key| {
        metadata
            .get(*key)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    })
}

fn metadata_indicates_codebase(metadata: &HashMap<String, String>) -> bool {
    if metadata_has_scan_root_keys(metadata) {
        return true;
    }

    let keys = [
        "codebase_scope",
        "repo_identifier",
        "scope_repo_id",
        "repo_id",
    ];
    keys.iter().any(|key| {
        metadata
            .get(*key)
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
    })
}

fn parse_scan_roots_strict(
    metadata: &HashMap<String, String>,
) -> Result<Option<Vec<ScanRootMetadata>>> {
    if let Some(raw) = metadata.get("scan_roots") {
        let value: serde_json::Value = serde_json::from_str(raw).map_err(|e| {
            AosError::InvalidManifest(format!("scan_roots metadata is invalid JSON: {}", e))
        })?;
        if let Ok(roots) = serde_json::from_value::<Vec<ScanRootMetadata>>(value.clone()) {
            let roots = normalize_scan_roots(roots);
            if roots.is_empty() {
                return Err(AosError::InvalidManifest(
                    "scan_roots metadata provided but empty".to_string(),
                ));
            }
            return Ok(Some(roots));
        }
        if let Ok(paths) = serde_json::from_value::<Vec<String>>(value) {
            let roots = normalize_scan_roots(
                paths
                    .into_iter()
                    .map(|path| ScanRootMetadata {
                        path,
                        label: None,
                        file_count: None,
                        byte_count: None,
                        content_hash: None,
                        scanned_at: None,
                    })
                    .collect(),
            );
            if roots.is_empty() {
                return Err(AosError::InvalidManifest(
                    "scan_roots metadata provided but empty".to_string(),
                ));
            }
            return Ok(Some(roots));
        }
        return Err(AosError::InvalidManifest(
            "scan_roots metadata is invalid JSON array".to_string(),
        ));
    }
    Ok(None)
}

fn expected_repo_slug_from_metadata(metadata: &HashMap<String, String>) -> Option<String> {
    metadata
        .get("repo_slug")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .or_else(|| {
            metadata
                .get("scope_repo_slug")
                .map(|v| v.trim())
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
        })
        .or_else(|| {
            let candidates = [
                metadata.get("scope_repo"),
                metadata.get("scope_repo_id"),
                metadata.get("repo_identifier"),
                metadata.get("repo_id"),
                metadata.get("repo_name"),
            ];
            for value in candidates.into_iter().flatten() {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    return Some(normalize_repo_slug(trimmed));
                }
            }
            None
        })
}

/// Codebase scope metadata extracted from the metadata HashMap.
#[derive(Debug, Clone, Default)]
struct ScopeMetadataExtract {
    scope_repo: Option<String>,
    scope_branch: Option<String>,
    scope_commit: Option<String>,
    scope_scan_root: Option<String>,
    scope_remote_url: Option<String>,
    repo_slug: Option<String>,
    session_id: Option<String>,
    session_name: Option<String>,
    session_tags: Option<Vec<String>>,
    scan_roots: Vec<ScanRootMetadata>,
}

/// Extracted manifest fields from metadata HashMap.
/// Centralizes parsing logic to ensure consistency across packaging methods.
#[derive(Debug, Clone)]
struct ManifestFieldsExtract {
    lora_tier: Option<LoraTier>,
    lora_strength: Option<f32>,
    category: String,
    tier: String,
    dataset_version_ids: Option<Vec<String>>,
    data_spec_hash: Option<String>,
    data_lineage_mode: Option<String>,
    synthetic_mode: Option<bool>,
    training_slice_id: Option<String>,
    backend_policy: Option<String>,
    recommended_for_moe: bool,
    stream_mode: Option<bool>,
    scope_meta: ScopeMetadataExtract,
}

/// Extract manifest fields from metadata HashMap.
/// Ensures consistent parsing across all packaging methods.
fn extract_manifest_fields(metadata: &HashMap<String, String>) -> ManifestFieldsExtract {
    let lora_tier = parse_lora_tier(metadata);
    let lora_strength = metadata
        .get("lora_strength")
        .and_then(|v| v.parse::<f32>().ok())
        .or_else(|| default_strength_for_tier(lora_tier));
    let category = metadata
        .get("category")
        .cloned()
        .unwrap_or_else(default_category);
    let tier = metadata.get("tier").cloned().unwrap_or_else(default_tier);

    let dataset_version_ids = metadata.get("dataset_version_ids").and_then(|raw| {
        serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|val| {
                let arr = val.as_array()?;
                let ids: Vec<String> = arr
                    .iter()
                    .filter_map(|v| {
                        if let Some(id) = v.get("dataset_version_id").and_then(|s| s.as_str()) {
                            Some(id.to_string())
                        } else {
                            v.as_str().map(|s| s.to_string())
                        }
                    })
                    .collect();
                if ids.is_empty() {
                    None
                } else {
                    Some(ids)
                }
            })
    });

    let data_spec_hash = metadata.get("data_spec_hash").cloned();
    let data_lineage_mode = metadata.get("data_lineage_mode").cloned();
    let synthetic_mode = metadata
        .get("synthetic_mode")
        .map(|v| v == "true" || v == "1");
    let training_slice_id = metadata.get("training_slice_id").cloned();
    let backend_policy = metadata.get("backend_policy").cloned();
    let recommended_for_moe = metadata
        .get("recommended_for_moe")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);
    let stream_mode = parse_metadata_bool(metadata, "stream_mode");

    let scope_meta = extract_scope_metadata(metadata);

    ManifestFieldsExtract {
        lora_tier,
        lora_strength,
        category,
        tier,
        dataset_version_ids,
        data_spec_hash,
        data_lineage_mode,
        synthetic_mode,
        training_slice_id,
        backend_policy,
        recommended_for_moe,
        stream_mode,
        scope_meta,
    }
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

fn apply_branch_metadata_defaults(metadata: &mut HashMap<String, String>) {
    let branch_meta = BranchMetadata::from_metadata(metadata);
    if let Some(branch) = branch_meta.branch {
        metadata.entry("scope_branch".to_string()).or_insert(branch);
    }
    if let Some(commit) = branch_meta.commit {
        metadata.entry("scope_commit".to_string()).or_insert(commit);
    }
    if let Some(repo_name) = branch_meta.repo_name {
        metadata
            .entry("scope_repo".to_string())
            .or_insert(repo_name);
    }
    if let Some(repo_slug) = branch_meta.repo_slug {
        metadata.entry("repo_slug".to_string()).or_insert(repo_slug);
    }
    if let Some(remote_url) = branch_meta.remote_url {
        metadata
            .entry("scope_remote_url".to_string())
            .or_insert(remote_url);
    }
}

fn normalize_commit_metadata(metadata: &mut HashMap<String, String>) {
    let commit_full = metadata
        .get("commit_sha")
        .or_else(|| metadata.get("scope_commit_full"))
        .or_else(|| metadata.get("commit_full"))
        .or_else(|| metadata.get("repo_commit"))
        .or_else(|| metadata.get("scope_commit"))
        .or_else(|| metadata.get("commit"))
        .cloned();

    if let Some(full_sha) = commit_full {
        metadata
            .entry("commit_sha".to_string())
            .or_insert_with(|| full_sha.clone());
        metadata
            .entry("scope_commit_full".to_string())
            .or_insert_with(|| full_sha.clone());
        let short = full_sha.get(0..8).unwrap_or(&full_sha).to_string();
        metadata
            .entry("commit_short_sha".to_string())
            .or_insert(short);
    }
}

fn apply_codebase_scope_defaults(metadata: &mut HashMap<String, String>) {
    let existing = metadata
        .get("codebase_scope")
        .map(|v| v.trim())
        .unwrap_or("");
    if !existing.is_empty() {
        return;
    }

    let scope_candidate = metadata
        .get("repo_identifier")
        .or_else(|| metadata.get("repo_id"))
        .or_else(|| metadata.get("scope_repo_id"))
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());

    if let Some(scope) = scope_candidate {
        metadata.insert("codebase_scope".to_string(), scope.to_string());
        return;
    }

    let slug_candidate = metadata
        .get("repo_slug")
        .or_else(|| metadata.get("scope_repo_slug"))
        .map(|v| v.trim())
        .filter(|v| !v.is_empty());

    if let Some(slug) = slug_candidate {
        let normalized = normalize_repo_slug(slug);
        metadata.insert("codebase_scope".to_string(), format!("repo:{}", normalized));
    }
}

/// Extract codebase scope metadata from the metadata HashMap.
fn extract_scope_metadata(metadata: &HashMap<String, String>) -> ScopeMetadataExtract {
    let repo_slug = normalize_optional_str(metadata.get("repo_slug").map(String::as_str))
        .or_else(|| normalize_optional_str(metadata.get("scope_repo_slug").map(String::as_str)))
        .map(|slug| normalize_repo_slug(&slug))
        .or_else(|| {
            normalize_optional_str(
                metadata
                    .get("scope_repo")
                    .or_else(|| metadata.get("repo_identifier"))
                    .or_else(|| metadata.get("scope_repo_id"))
                    .or_else(|| metadata.get("repo_name"))
                    .map(String::as_str),
            )
            .map(|v| normalize_repo_slug(&v))
        });
    let scan_roots = parse_scan_roots_from_metadata(metadata);
    let scope_scan_root = resolve_scan_root_from_metadata(metadata)
        .or_else(|| scan_roots.first().map(|root| root.path.clone()));

    ScopeMetadataExtract {
        scope_repo: normalize_optional_str(
            metadata
                .get("scope_repo")
                .or_else(|| metadata.get("repo_name"))
                .map(String::as_str),
        ),
        scope_branch: normalize_optional_str(
            metadata
                .get("scope_branch")
                .or_else(|| metadata.get("repo_branch"))
                .map(String::as_str),
        ),
        scope_commit: normalize_optional_str(
            metadata
                .get("scope_commit")
                .or_else(|| metadata.get("repo_commit"))
                .or_else(|| metadata.get("commit_sha"))
                .or_else(|| metadata.get("commit_short_sha"))
                .or_else(|| metadata.get("commit"))
                .map(String::as_str),
        ),
        scope_scan_root,
        scope_remote_url: normalize_optional_str(
            metadata
                .get("scope_remote_url")
                .or_else(|| metadata.get("repo_remote"))
                .map(String::as_str),
        ),
        repo_slug,
        session_id: normalize_optional_str(metadata.get("session_id").map(String::as_str)),
        session_name: normalize_optional_str(metadata.get("session_name").map(String::as_str)),
        session_tags: parse_session_tags(metadata.get("session_tags")),
        scan_roots,
    }
}

fn persist_scope_metadata(
    metadata: &mut HashMap<String, String>,
    scope_meta: &ScopeMetadataExtract,
) {
    fn insert_if_missing(
        metadata: &mut HashMap<String, String>,
        key: &str,
        value: Option<&String>,
    ) {
        let Some(value) = value else {
            return;
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return;
        }
        let should_insert = metadata
            .get(key)
            .map(|existing| existing.trim().is_empty())
            .unwrap_or(true);
        if should_insert {
            metadata.insert(key.to_string(), trimmed.to_string());
        }
    }

    insert_if_missing(metadata, "scope_repo", scope_meta.scope_repo.as_ref());
    insert_if_missing(metadata, "scope_branch", scope_meta.scope_branch.as_ref());
    insert_if_missing(metadata, "scope_commit", scope_meta.scope_commit.as_ref());
    insert_if_missing(
        metadata,
        "scope_scan_root",
        scope_meta.scope_scan_root.as_ref(),
    );
    insert_if_missing(
        metadata,
        "scope_remote_url",
        scope_meta.scope_remote_url.as_ref(),
    );
    insert_if_missing(metadata, "repo_slug", scope_meta.repo_slug.as_ref());
    insert_if_missing(metadata, "session_id", scope_meta.session_id.as_ref());
    insert_if_missing(metadata, "session_name", scope_meta.session_name.as_ref());

    if let Some(tags) = scope_meta.session_tags.as_ref().filter(|t| !t.is_empty()) {
        let should_insert = metadata
            .get("session_tags")
            .map(|existing| existing.trim().is_empty())
            .unwrap_or(true);
        if should_insert {
            let value = serde_json::to_string(tags).unwrap_or_else(|_| tags.join(","));
            metadata.insert("session_tags".to_string(), value);
        }
    }

    if !scope_meta.scan_roots.is_empty() {
        let should_insert = metadata
            .get("scan_roots")
            .map(|existing| existing.trim().is_empty())
            .unwrap_or(true);
        if should_insert {
            if let Ok(serialized) = serde_json::to_string(&scope_meta.scan_roots) {
                metadata.insert("scan_roots".to_string(), serialized);
            }
        }
    }
}

fn parse_session_tags(raw: Option<&String>) -> Option<Vec<String>> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with('[') {
        if let Ok(mut tags) = serde_json::from_str::<Vec<String>>(trimmed) {
            normalize_session_tags(&mut tags);
            return if tags.is_empty() { None } else { Some(tags) };
        }
    }

    let mut tags: Vec<String> = trimmed
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    normalize_session_tags(&mut tags);

    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

fn normalize_session_tags(tags: &mut Vec<String>) {
    if tags.is_empty() {
        return;
    }
    for tag in tags.iter_mut() {
        let trimmed = tag.trim();
        if trimmed != tag.as_str() {
            *tag = trimmed.to_string();
        }
    }
    tags.retain(|t| !t.is_empty());
    if tags.len() > 1 {
        tags.sort();
        tags.dedup();
    }
}

// normalize_repo_slug is now imported from adapteros_normalization crate

fn canonicalize_backend_label(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.contains("coreml") {
        "coreml".to_string()
    } else if lower.contains("mlx") {
        "mlx".to_string()
    } else if lower.contains("metal") {
        "metal".to_string()
    } else if lower.contains("cpu") {
        "cpu".to_string()
    } else {
        lower
    }
}

fn is_valid_graph_target(target: &str) -> bool {
    !target.trim().is_empty()
        && target.len() <= 256
        && target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
}

fn infer_op_kind_from_target(target: &str) -> CoreMLOpKind {
    let lower = target.to_ascii_lowercase();
    if lower.contains("q_proj") || lower.contains(".q_proj") || lower.contains("query") {
        CoreMLOpKind::AttentionQ
    } else if lower.contains("k_proj") || lower.contains(".k_proj") || lower.contains("key") {
        CoreMLOpKind::AttentionK
    } else if lower.contains("v_proj") || lower.contains(".v_proj") || lower.contains("value") {
        CoreMLOpKind::AttentionV
    } else if lower.contains("o_proj") || lower.contains(".o_proj") || lower.contains("out_proj") {
        CoreMLOpKind::AttentionO
    } else if lower.contains("gate") {
        CoreMLOpKind::MlpGate
    } else if lower.contains("up_proj") {
        CoreMLOpKind::MlpUp
    } else if lower.contains("down_proj") {
        CoreMLOpKind::MlpDown
    } else {
        CoreMLOpKind::AttentionO
    }
}

impl AdapterManifest {
    fn validate(&self) -> Result<()> {
        if let Some(coreml) = &self.coreml {
            if coreml.coreml_used {
                let backend = self
                    .training_backend
                    .as_deref()
                    .or(self.training_backend_details.as_deref())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if !backend.contains("coreml") {
                    return Err(AosError::InvalidManifest(
                        "coreml_used requires training_backend coreml".to_string(),
                    ));
                }
            }

            if let Some(device) = &coreml.coreml_device_type {
                let device_lc = device.to_ascii_lowercase();
                if !matches!(device_lc.as_str(), "ane" | "gpu" | "cpu" | "unknown") {
                    return Err(AosError::InvalidManifest(format!(
                        "Invalid coreml_device_type: {}",
                        device
                    )));
                }
            }
        }

        let scope_id_ok = self
            .metadata
            .get("adapter_scope_id")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let scope_type_ok = self
            .metadata
            .get("adapter_scope_type")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let scope_visibility_ok = self
            .metadata
            .get("adapter_scope_visibility")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);
        let scope_required = matches!(self.category.as_str(), "code" | "codebase");
        if (scope_required || scope_id_ok || scope_type_ok || scope_visibility_ok)
            && !(scope_id_ok && scope_type_ok && scope_visibility_ok)
        {
            return Err(AosError::InvalidManifest(
                "adapter_scope_id, adapter_scope_type, and adapter_scope_visibility are required for code adapters"
                    .to_string(),
            ));
        }

        let synthetic_mode = self
            .metadata
            .get("synthetic_mode")
            .map(|v| v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let lineage_mode = self
            .metadata
            .get("data_lineage_mode")
            .map(|s| s.as_str())
            .unwrap_or("versioned");
        let dataset_ids_present = self
            .dataset_version_ids
            .as_ref()
            .map(|ids| !ids.is_empty())
            .unwrap_or(false);

        debug!(
            lineage_mode = lineage_mode,
            dataset_ids_present = dataset_ids_present,
            "evaluated lineage mode"
        );

        if !synthetic_mode {
            if let Some(ids) = &self.dataset_version_ids {
                if ids.is_empty() {
                    return Err(AosError::InvalidManifest(
                        "dataset_version_ids cannot be empty".to_string(),
                    ));
                }
            }

            if !dataset_ids_present && lineage_mode != "legacy_unpinned" {
                return Err(AosError::InvalidManifest(
                    "dataset_version_ids required unless synthetic_mode=true or lineage_mode=legacy_unpinned"
                        .to_string(),
                ));
            }

            if dataset_ids_present && self.data_spec_hash.is_none() {
                return Err(AosError::InvalidManifest(
                    "data_spec_hash is required when dataset_version_ids are present".to_string(),
                ));
            }
        }

        if let Some(placement) = &self.placement {
            for record in &placement.records {
                if record.rank == 0 {
                    return Err(AosError::InvalidManifest(
                        "Placement record rank must be > 0".to_string(),
                    ));
                }

                if !is_valid_graph_target(&record.graph_target) {
                    return Err(AosError::InvalidManifest(format!(
                        "Invalid placement graph_target: {}",
                        record.graph_target
                    )));
                }

                if record.direction.trim().is_empty() {
                    return Err(AosError::InvalidManifest(
                        "Placement direction must be non-empty".to_string(),
                    ));
                }

                if let Some(alpha) = record.alpha_override {
                    if alpha <= 0.0 {
                        return Err(AosError::InvalidManifest(
                            "Placement alpha_override must be positive".to_string(),
                        ));
                    }
                }
            }
        }

        if let Some(spec) = &self.coreml_placement {
            if spec.version == 0 {
                return Err(AosError::InvalidManifest(
                    "coreml_placement.version must be > 0".to_string(),
                ));
            }
            for binding in &spec.bindings {
                if binding.rank == 0 {
                    return Err(AosError::InvalidManifest(format!(
                        "coreml_placement binding {} has rank 0",
                        binding.binding_id
                    )));
                }
                if binding.shape.input_dim == 0 || binding.shape.output_dim == 0 {
                    return Err(AosError::InvalidManifest(format!(
                        "coreml_placement binding {} has zero-dimension shape",
                        binding.binding_id
                    )));
                }
                if binding.target.layer.trim().is_empty() {
                    return Err(AosError::InvalidManifest(
                        "coreml_placement binding missing target layer".to_string(),
                    ));
                }
            }
        }

        self.validate_scope_metadata()?;
        self.validate_quantization_metadata()?;

        Ok(())
    }

    fn validate_scope_metadata(&self) -> Result<()> {
        let is_codebase = metadata_indicates_codebase(&self.metadata);
        let expected_slug = expected_repo_slug_from_metadata(&self.metadata);

        if is_codebase {
            let has_repo_slug_metadata = self
                .metadata
                .get("repo_slug")
                .map(|slug| !slug.trim().is_empty())
                .unwrap_or(false)
                || self
                    .metadata
                    .get("scope_repo_slug")
                    .map(|slug| !slug.trim().is_empty())
                    .unwrap_or(false);
            if !has_repo_slug_metadata {
                return Err(AosError::InvalidManifest(
                    "repo_slug missing from ingestion metadata for codebase adapter".to_string(),
                ));
            }
        }

        if is_codebase
            && self
                .repo_slug
                .as_deref()
                .map(|slug| slug.trim().is_empty())
                .unwrap_or(true)
        {
            return Err(AosError::InvalidManifest(
                "repo_slug missing for codebase adapter".to_string(),
            ));
        }

        if let Some(expected_slug) = expected_slug {
            match self.repo_slug.as_deref() {
                Some(actual) if actual == expected_slug => {}
                Some(actual) => {
                    return Err(AosError::InvalidManifest(format!(
                        "repo_slug '{}' does not match ingestion metadata '{}'",
                        actual, expected_slug
                    )));
                }
                None => {
                    return Err(AosError::InvalidManifest(
                        "repo_slug missing but ingestion metadata provides one".to_string(),
                    ));
                }
            }
        }

        if is_codebase && !metadata_has_scan_root_keys(&self.metadata) {
            return Err(AosError::InvalidManifest(
                "scan_roots missing from ingestion metadata for codebase adapter".to_string(),
            ));
        }

        if metadata_has_scan_root_keys(&self.metadata) {
            let expected_roots = parse_scan_roots_strict(&self.metadata)?
                .unwrap_or_else(|| parse_scan_roots_from_metadata(&self.metadata));
            if expected_roots.is_empty() {
                return Err(AosError::InvalidManifest(
                    "scan_roots metadata provided but no scan roots parsed".to_string(),
                ));
            }
            if self.scan_roots.is_empty() {
                return Err(AosError::InvalidManifest(
                    "manifest missing scan_roots for codebase ingestion".to_string(),
                ));
            }
            if self.scan_roots != expected_roots {
                return Err(AosError::InvalidManifest(
                    "manifest scan_roots do not match ingestion metadata".to_string(),
                ));
            }

            if let Some(expected_root) = resolve_scan_root_from_metadata(&self.metadata) {
                if self.scope_scan_root.as_deref() != Some(expected_root.as_str()) {
                    return Err(AosError::InvalidManifest(format!(
                        "scope_scan_root '{}' does not match ingestion metadata '{}'",
                        self.scope_scan_root.as_deref().unwrap_or("<missing>"),
                        expected_root
                    )));
                }
            }
        }

        if is_codebase && !self.metadata.contains_key("stream_mode") {
            return Err(AosError::InvalidManifest(
                "stream_mode missing from ingestion metadata for codebase adapter".to_string(),
            ));
        }

        if let Some(raw) = self.metadata.get("stream_mode") {
            let expected = parse_bool_strict(raw).ok_or_else(|| {
                AosError::InvalidManifest(format!("Invalid stream_mode value '{}'", raw))
            })?;
            if self.stream_mode != Some(expected) {
                return Err(AosError::InvalidManifest(
                    "stream_mode does not match ingestion metadata".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn validate_quantization_metadata(&self) -> Result<()> {
        let quant = self.quantization.as_deref().unwrap_or_default();
        if quant.is_empty() {
            return Err(AosError::InvalidManifest(
                "quantization is required for packaged adapters".to_string(),
            ));
        }
        if quant != LORA_Q15_QUANTIZATION {
            return Err(AosError::InvalidManifest(format!(
                "Unsupported quantization '{}': expected {}",
                quant, LORA_Q15_QUANTIZATION
            )));
        }

        let version = self
            .metadata
            .get("quantization_version")
            .map(|v| v.as_str())
            .unwrap_or("");
        if version != LORA_Q15_VERSION {
            return Err(AosError::InvalidManifest(format!(
                "quantization_version '{}' does not match expected {}",
                version, LORA_Q15_VERSION
            )));
        }

        let strength_version = self
            .metadata
            .get("lora_strength_defaults_version")
            .map(|v| v.as_str())
            .unwrap_or("");
        if strength_version != LORA_STRENGTH_DEFAULTS_VERSION {
            return Err(AosError::InvalidManifest(format!(
                "lora_strength_defaults_version '{}' does not match expected {}",
                strength_version, LORA_STRENGTH_DEFAULTS_VERSION
            )));
        }

        let denom_raw = self.metadata.get("lora_q15_denom").ok_or_else(|| {
            AosError::InvalidManifest("lora_q15_denom missing from metadata".to_string())
        })?;
        let denom = denom_raw.parse::<f32>().map_err(|_| {
            AosError::InvalidManifest(format!(
                "lora_q15_denom '{}' is not a valid number",
                denom_raw
            ))
        })?;
        if (denom - LORA_Q15_DENOM).abs() > f32::EPSILON {
            return Err(AosError::InvalidManifest(format!(
                "lora_q15_denom {} does not match expected {}",
                denom, LORA_Q15_DENOM
            )));
        }

        if self.lora_tier.is_some() && self.lora_strength.is_none() {
            return Err(AosError::InvalidManifest(
                "lora_strength missing for tiered adapter".to_string(),
            ));
        }

        if !self.metadata.contains_key("lora_strength") {
            if let Some(tier) = self.lora_tier {
                if let Some(expected) = default_strength_for_tier(Some(tier)) {
                    if self
                        .lora_strength
                        .map(|strength| (strength - expected).abs() > f32::EPSILON)
                        .unwrap_or(true)
                    {
                        return Err(AosError::InvalidManifest(
                            "lora_strength default does not match expected tier strength"
                                .to_string(),
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}

impl AdapterPackager {
    /// Create a new packager with output directory
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Self {
        Self {
            repo_root: output_dir.as_ref().to_path_buf(),
        }
    }

    /// Create a packager using the default adapters directory
    ///
    /// Uses the centralized path from `adapteros_core::RepoAdapterPaths`,
    /// which resolves from environment variable `AOS_ADAPTERS_ROOT`
    /// (with `AOS_ADAPTERS_DIR` compatibility) or defaults to `var/adapters/repo`.
    pub fn with_default_path() -> Self {
        Self {
            repo_root: RepoAdapterPaths::from_env_and_config(None)
                .repo_root
                .to_path_buf(),
        }
    }

    /// Create a packager from config value, falling back to default
    pub fn from_config(adapters_root: Option<&str>) -> Self {
        Self {
            repo_root: RepoAdapterPaths::from_env_and_config(adapters_root.map(|s| s.to_string()))
                .repo_root
                .to_path_buf(),
        }
    }

    fn artifact_quota_limits() -> (u64, u64) {
        let hard = std::env::var("AOS_ARTIFACT_HARD_QUOTA_BYTES")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_ARTIFACT_HARD_QUOTA_BYTES);
        let soft = std::env::var("AOS_ARTIFACT_SOFT_QUOTA_BYTES")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or((hard as f64 * DEFAULT_ARTIFACT_SOFT_PCT) as u64);
        (soft, hard)
    }

    async fn current_artifact_usage(&self, tenant_id: &str) -> Result<u64> {
        let tenant_dir = self.repo_root.join(tenant_id);
        if !tenant_dir.exists() {
            return Ok(0);
        }
        let mut total: u64 = 0;
        for entry in WalkDir::new(&tenant_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "aos" {
                    if let Ok(meta) = tokio::fs::metadata(path).await {
                        total = total.saturating_add(meta.len());
                    }
                }
            }
        }
        Ok(total)
    }

    async fn enforce_artifact_quota(&self, tenant_id: &str, incoming_bytes: u64) -> Result<()> {
        let (soft, hard) = Self::artifact_quota_limits();
        let current = self.current_artifact_usage(tenant_id).await?;
        let predicted = current.saturating_add(incoming_bytes);
        if predicted > hard {
            return Err(AosError::Training(format!(
                "Artifact storage quota exceeded for tenant {}: {} > {} bytes",
                tenant_id, predicted, hard
            )));
        }
        if predicted > soft {
            warn!(
                tenant_id = %tenant_id,
                predicted,
                soft,
                "Artifact storage soft quota exceeded"
            );
        }
        Ok(())
    }

    /// Enrich metadata with deterministic defaults and backend/quantization hints.
    fn build_manifest_metadata(
        metadata: HashMap<String, String>,
        config: &TrainingConfig,
        scope: &str,
    ) -> (HashMap<String, String>, Option<String>, String, String) {
        let mut manifest_metadata = metadata;
        apply_branch_metadata_defaults(&mut manifest_metadata);
        normalize_commit_metadata(&mut manifest_metadata);
        apply_codebase_scope_defaults(&mut manifest_metadata);

        // runtime-only knob; exclude from persisted .aos metadata
        manifest_metadata.remove("routing_determinism_mode");

        // Standard quantization + determinism annotations
        manifest_metadata
            .entry("quantization".to_string())
            .or_insert_with(|| LORA_Q15_QUANTIZATION.to_string());
        manifest_metadata
            .entry("quantization_version".to_string())
            .or_insert_with(|| LORA_Q15_VERSION.to_string());
        manifest_metadata
            .entry("lora_strength_defaults_version".to_string())
            .or_insert_with(|| LORA_STRENGTH_DEFAULTS_VERSION.to_string());
        manifest_metadata
            .entry("lora_q15_denom".to_string())
            .or_insert_with(|| LORA_Q15_DENOM.to_string());
        manifest_metadata
            .entry("gate_q15_denominator".to_string())
            .or_insert_with(|| ROUTER_GATE_Q15_DENOM.to_string());

        let determinism = manifest_metadata
            .entry("determinism".to_string())
            .or_insert_with(default_determinism_mode)
            .clone();

        // Prefer caller-provided backend (actual executed), otherwise derive from config preference
        let training_backend = manifest_metadata
            .get("training_backend")
            .cloned()
            .or_else(|| config.preferred_backend.map(|b| b.tag().to_string()))
            .map(|b| canonicalize_backend_label(&b));

        if let Some(ref backend) = training_backend {
            manifest_metadata.insert("training_backend".to_string(), backend.clone());
        }

        // Persist scope early so hierarchy defaults can use it.
        manifest_metadata
            .entry("scope".to_string())
            .or_insert_with(|| scope.to_string());

        // Hierarchical adapter metadata: derive when missing or placeholder.
        // domain <- category (adapter type), group <- scope (project/tenant), operation <- adapter_name/action.
        let domain = manifest_metadata
            .get("domain")
            .filter(|v| v != &&"unspecified".to_string())
            .cloned()
            .or_else(|| manifest_metadata.get("category").cloned())
            .unwrap_or_else(|| "unspecified".to_string());
        manifest_metadata.insert("domain".to_string(), domain.clone());

        let group = manifest_metadata
            .get("group")
            .filter(|v| v != &&"unspecified".to_string())
            .cloned()
            .or_else(|| manifest_metadata.get("project").cloned())
            .or_else(|| manifest_metadata.get("scope").cloned())
            .unwrap_or_else(|| "unspecified".to_string());
        manifest_metadata.insert("group".to_string(), group.clone());

        let operation = manifest_metadata
            .get("operation")
            .filter(|v| v != &&"unspecified".to_string())
            .cloned()
            .or_else(|| manifest_metadata.get("adapter_name").cloned())
            .or_else(|| manifest_metadata.get("training_action").cloned())
            .unwrap_or_else(|| "unspecified".to_string());
        manifest_metadata.insert("operation".to_string(), operation.clone());

        let scope_path = format!("{}/{}/{}/{}", domain, group, scope, operation);
        manifest_metadata
            .entry("scope_path".to_string())
            .or_insert_with(|| scope_path.clone());

        if !manifest_metadata.contains_key("scope_repo_id") {
            if let Some(repo_id) = manifest_metadata
                .get("repo_identifier")
                .or_else(|| manifest_metadata.get("repo_id"))
                .cloned()
            {
                manifest_metadata.insert("scope_repo_id".to_string(), repo_id);
            }
        }
        if !manifest_metadata.contains_key("repo_identifier") {
            if let Some(repo_id) = manifest_metadata
                .get("scope_repo_id")
                .or_else(|| manifest_metadata.get("repo_id"))
                .cloned()
            {
                manifest_metadata.insert("repo_identifier".to_string(), repo_id);
            }
        }

        (manifest_metadata, training_backend, determinism, scope_path)
    }

    fn validate_quantized_shapes(
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
    ) -> Result<(usize, usize, usize, usize)> {
        let a_rows = weights.lora_a_q15.len();
        let a_cols = weights
            .lora_a_q15
            .first()
            .map(|r| r.len())
            .unwrap_or_default();
        let b_rows = weights.lora_b_q15.len();
        let b_cols = weights
            .lora_b_q15
            .first()
            .map(|r| r.len())
            .unwrap_or_default();

        if a_rows == 0 || a_cols == 0 || b_rows == 0 || b_cols == 0 {
            return Err(AosError::Validation(
                "Quantized weights are empty; aborting packaging".to_string(),
            ));
        }

        if a_rows != config.rank || b_cols != config.rank {
            return Err(AosError::Validation(format!(
                "LoRA rank mismatch for CoreML placement: expected {}, got A rows {} / B cols {}",
                config.rank, a_rows, b_cols
            )));
        }

        if a_cols != config.hidden_dim || b_rows != config.hidden_dim {
            return Err(AosError::Validation(format!(
                "Hidden dimension mismatch for CoreML placement: expected {}, got A cols {} / B rows {}",
                config.hidden_dim, a_cols, b_rows
            )));
        }

        Ok((a_rows, a_cols, b_rows, b_cols))
    }

    fn parse_coreml_placement_from_metadata(
        metadata: &HashMap<String, String>,
    ) -> Result<Option<CoremlPlacementSpec>> {
        if let Some(raw) = metadata.get("coreml_placement") {
            let spec: CoremlPlacementSpec = serde_json::from_str(raw).map_err(|e| {
                AosError::Validation(format!("Invalid CoreML placement spec JSON: {}", e))
            })?;
            return Ok(Some(spec));
        }
        Ok(None)
    }

    fn default_coreml_placement_spec(
        modules: &[&str],
        rank: usize,
        hidden_dim: usize,
    ) -> CoremlPlacementSpec {
        CoremlPlacementSpec {
            version: 1,
            graph_id: Some("coreml-default".to_string()),
            bindings: modules
                .iter()
                .map(|m| CoreMLPlacementBinding {
                    binding_id: m.to_string(),
                    target: CoreMLTargetRef {
                        layer: m.to_string(),
                        op_kind: infer_op_kind_from_target(m),
                        path_hint: None,
                    },
                    projection: CoreMLProjection::InputToHidden,
                    rank: rank as u32,
                    alpha: None,
                    scale: None,
                    gating: None,
                    shape: CoreMLPlacementShape {
                        input_dim: hidden_dim as u32,
                        output_dim: hidden_dim as u32,
                    },
                })
                .collect(),
        }
    }

    fn validate_coreml_placement_spec(
        spec: &CoremlPlacementSpec,
        modules: &[&str],
        rank: usize,
        hidden_dim: usize,
    ) -> Result<()> {
        if spec.version == 0 {
            return Err(AosError::Validation(
                "CoreML placement spec version must be > 0".to_string(),
            ));
        }
        if spec.bindings.is_empty() {
            return Err(AosError::Validation(
                "CoreML placement spec must include at least one entry".to_string(),
            ));
        }

        let mut seen = HashSet::new();
        let allowed: HashSet<String> = modules.iter().map(|m| m.to_string()).collect();

        for binding in &spec.bindings {
            if !seen.insert(binding.binding_id.clone()) {
                return Err(AosError::Validation(format!(
                    "Duplicate CoreML placement target '{}'",
                    binding.binding_id
                )));
            }

            if !allowed.contains(&binding.target.layer) {
                return Err(AosError::Validation(format!(
                    "Unknown CoreML placement target '{}' (expected one of {:?})",
                    binding.target.layer, modules
                )));
            }

            if binding.rank as usize != rank {
                return Err(AosError::Validation(format!(
                    "CoreML placement rank mismatch for '{}': expected {}, got {}",
                    binding.binding_id, rank, binding.rank
                )));
            }
            if binding.shape.input_dim != hidden_dim as u32
                || binding.shape.output_dim != hidden_dim as u32
            {
                return Err(AosError::Validation(format!(
                    "CoreML placement shape mismatch for '{}': expected {}x{}, got {}x{}",
                    binding.binding_id,
                    hidden_dim,
                    hidden_dim,
                    binding.shape.output_dim,
                    binding.shape.input_dim
                )));
            }
        }

        Ok(())
    }

    fn resolve_coreml_placement_spec(
        metadata: &HashMap<String, String>,
        modules: &[&str],
        rank: usize,
        hidden_dim: usize,
    ) -> Result<CoremlPlacementSpec> {
        if let Some(spec) = Self::parse_coreml_placement_from_metadata(metadata)? {
            Self::validate_coreml_placement_spec(&spec, modules, rank, hidden_dim)?;
            return Ok(spec);
        }

        let spec = Self::default_coreml_placement_spec(modules, rank, hidden_dim);
        Self::validate_coreml_placement_spec(&spec, modules, rank, hidden_dim)?;
        Ok(spec)
    }

    fn build_coreml_sections(
        metadata: &HashMap<String, String>,
        training_backend: Option<&str>,
        rank: usize,
    ) -> Result<(
        Option<CoremlTrainingMetadata>,
        Option<AdapterPlacement>,
        Option<String>,
    )> {
        let mut training_backend_details = metadata
            .get("training_backend_details")
            .cloned()
            .or_else(|| training_backend.map(|b| format!("{b}_train")));

        let coreml_requested = metadata
            .get("coreml_used")
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false)
            || matches!(training_backend, Some("coreml"));

        let coreml_metadata = if coreml_requested {
            if training_backend.is_none() && training_backend_details.is_none() {
                training_backend_details = Some("coreml_train".to_string());
            }
            let device_type = metadata
                .get("coreml_device_type")
                .or_else(|| metadata.get("coreml_device"))
                .cloned()
                .or_else(|| {
                    training_backend.map(|b| {
                        if b == "coreml" {
                            "ane".to_string()
                        } else {
                            "unknown".to_string()
                        }
                    })
                })
                .unwrap_or_else(|| "unknown".to_string());

            Some(CoremlTrainingMetadata {
                coreml_used: true,
                coreml_device_type: Some(device_type.to_ascii_lowercase()),
                coreml_precision_mode: metadata.get("coreml_precision_mode").cloned(),
                coreml_compile_config_id: metadata.get("coreml_compile_config_id").cloned(),
            })
        } else {
            None
        };

        let placement_records = if let Some(raw) = metadata.get("coreml_placement_records") {
            let parsed: Vec<PlacementRecord> = serde_json::from_str(raw).map_err(|e| {
                AosError::InvalidManifest(format!(
                    "coreml_placement_records is not valid JSON array: {}",
                    e
                ))
            })?;
            Some(parsed)
        } else if let Some(target) = metadata.get("coreml_graph_target") {
            let direction = metadata
                .get("coreml_projection")
                .cloned()
                .unwrap_or_else(|| "projection".to_string());
            let alpha_override = metadata
                .get("coreml_alpha_override")
                .and_then(|v| v.parse::<f32>().ok());
            Some(vec![PlacementRecord {
                graph_target: target.clone(),
                rank: rank as u32,
                direction,
                alpha_override,
            }])
        } else {
            None
        };

        let placement = placement_records.and_then(|records| {
            if records.is_empty() {
                None
            } else {
                Some(AdapterPlacement { records })
            }
        });

        Ok((coreml_metadata, placement, training_backend_details))
    }

    fn adapter_dir(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> std::result::Result<PathBuf, adapteros_core::ResolveError> {
        adapteros_core::adapter_fs_path_with_root(&self.repo_root, tenant_id, adapter_id)
    }

    async fn artifact_usage_for_tenant(&self, tenant_id: &str) -> Result<u64> {
        let root = self.repo_root.join(tenant_id);
        Self::dir_size(&root).await
    }

    async fn dir_size(root: &Path) -> Result<u64> {
        let mut total: u64 = 0;
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await.map_err(|e| {
                AosError::Io(format!("Failed to read dir {}: {}", dir.display(), e))
            })?;
            while let Some(entry) = entries.next_entry().await.map_err(|e| {
                AosError::Io(format!("Failed to read dir entry {}: {}", dir.display(), e))
            })? {
                let path = entry.path();
                let meta = entry.metadata().await.map_err(|e| {
                    AosError::Io(format!("Failed to stat {}: {}", path.display(), e))
                })?;
                if meta.is_dir() {
                    stack.push(path);
                } else {
                    total = total.saturating_add(meta.len());
                }
            }
        }
        Ok(total)
    }

    /// Package adapter with weights and manifest
    pub async fn package(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
    ) -> Result<PackagedAdapter> {
        self.package_with_metadata(
            tenant_id,
            adapter_id,
            weights,
            config,
            base_model,
            HashMap::new(),
        )
        .await
    }

    /// Package adapter with weights, manifest, and metadata
    pub async fn package_with_metadata(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
        metadata: HashMap<String, String>,
    ) -> Result<PackagedAdapter> {
        info!("Packaging adapter: {}", adapter_id);

        // Create adapter directory (canonical tenant-aware path)
        let adapter_dir = self
            .adapter_dir(tenant_id, adapter_id)
            .map_err(|e| AosError::Validation(format!("Invalid adapter path: {}", e)))?;
        tokio::fs::create_dir_all(&adapter_dir).await.map_err(|e| {
            AosError::Training(format!("Failed to create adapter directory: {}", e))
        })?;

        // Serialize weights to safetensors format (adapter-only deltas)
        let weights_path = adapter_dir.join("weights.safetensors");
        let weights_bytes = self
            .save_weights_safetensors(&weights_path, weights)
            .await?;

        // Compute whole-adapter hash + per-layer hashes from the in-memory bytes
        let hash_b3 = blake3::hash(&weights_bytes).to_hex().to_string();
        let per_layer_hashes = Self::compute_per_layer_hashes_from_bytes(&weights_bytes)?;

        let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];
        let (rank, hidden_dim, _, _) = Self::validate_quantized_shapes(weights, config)?;
        let coreml_placement =
            Self::resolve_coreml_placement_spec(&metadata, &modules, rank, hidden_dim)?;
        let base_model_hash = metadata.get("base_model_hash").cloned();

        let scope_value = metadata.get("scope").cloned().unwrap_or_else(default_scope);
        let (mut metadata, training_backend, determinism, _scope_path) =
            Self::build_manifest_metadata(metadata, config, &scope_value);

        // Use centralized extraction for consistent metadata parsing
        let fields = extract_manifest_fields(&metadata);
        persist_scope_metadata(&mut metadata, &fields.scope_meta);
        let (coreml, placement, training_backend_details) =
            Self::build_coreml_sections(&metadata, training_backend.as_deref(), config.rank)?;

        // Create manifest with consistent version format (semver)
        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            rank: config.rank,
            base_model: base_model.to_string(),
            base_model_hash,
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            category: fields.category,
            tier: fields.tier,
            per_layer_hashes: Some(per_layer_hashes),
            training_backend,
            determinism,
            coreml_placement: Some(coreml_placement.clone()),
            lora_tier: fields.lora_tier,
            lora_strength: fields.lora_strength,
            scope: scope_value,
            recommended_for_moe: fields.recommended_for_moe,
            quantization: Some(LORA_Q15_QUANTIZATION.to_string()),
            gate_q15_denominator: Some(ROUTER_GATE_Q15_DENOM as u32),
            coreml,
            placement,
            training_backend_details,
            dataset_version_ids: fields.dataset_version_ids,
            data_spec_hash: fields.data_spec_hash,
            data_lineage_mode: fields.data_lineage_mode,
            synthetic_mode: fields.synthetic_mode,
            training_slice_id: fields.training_slice_id,
            backend_policy: fields.backend_policy,
            kernel_version: Some(adapteros_core::version::VERSION.to_string()),
            moe_config: config.moe_config.clone(),
            scope_repo: fields.scope_meta.scope_repo,
            scope_branch: fields.scope_meta.scope_branch,
            scope_commit: fields.scope_meta.scope_commit,
            scope_scan_root: fields.scope_meta.scope_scan_root,
            scope_remote_url: fields.scope_meta.scope_remote_url,
            repo_slug: fields.scope_meta.repo_slug,
            scan_roots: fields.scope_meta.scan_roots,
            session_id: fields.scope_meta.session_id,
            session_name: fields.scope_meta.session_name,
            session_tags: fields.scope_meta.session_tags,
            stream_mode: fields.stream_mode,
            metadata,
        };

        manifest.validate()?;

        // Serialize manifest once for deterministic signing
        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| AosError::Training(format!("Failed to serialize manifest: {}", e)))?;

        // Save manifest
        let manifest_path = adapter_dir.join("manifest.json");
        tokio::fs::write(&manifest_path, &manifest_bytes)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write manifest: {}", e)))?;

        // Deterministic manifest signing (seeded by manifest bytes + adapter_id)
        self.sign_manifest(&adapter_dir, adapter_id, &manifest_bytes)
            .await?;

        info!("Adapter packaged successfully: {}", adapter_id);

        Ok(PackagedAdapter {
            adapter_id: adapter_id.to_string(),
            manifest,
            weights_path,
            hash_b3,
        })
    }

    /// Package adapter as single .aos archive file for a specific tenant.
    ///
    /// Creates a single-file .aos archive containing manifest + weights.
    /// This is the preferred format for distribution and loading into Worker.
    pub async fn package_aos_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
    ) -> Result<PackagedAdapter> {
        self.package_aos_with_metadata(
            tenant_id,
            adapter_id,
            weights,
            config,
            base_model,
            HashMap::new(),
        )
        .await
    }

    /// Package adapter as single .aos archive file (legacy wrapper).
    ///
    /// Uses the default tenant ("default") to preserve compatibility with
    /// existing call sites that are not yet tenant-aware.
    pub async fn package_aos(
        &self,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
    ) -> Result<PackagedAdapter> {
        self.package_aos_for_tenant("default", adapter_id, weights, config, base_model)
            .await
    }

    /// Package adapter as single .aos archive file with metadata
    pub async fn package_aos_with_metadata(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
        metadata: HashMap<String, String>,
    ) -> Result<PackagedAdapter> {
        info!("Packaging adapter as .aos archive: {}", adapter_id);

        let adapter_dir = self
            .adapter_dir(tenant_id, adapter_id)
            .map_err(|e| AosError::Validation(format!("Invalid adapter path: {}", e)))?;
        tokio::fs::create_dir_all(&adapter_dir).await.map_err(|e| {
            AosError::Training(format!("Failed to create adapter directory: {}", e))
        })?;
        let storage = FsByteStorage::new(PathBuf::from("var/datasets"), self.repo_root.clone());

        // Serialize weights to in-memory safetensors buffer (matches loader expectations)
        let weights_data = Self::build_safetensors_bytes(weights)?;
        let estimate_bytes = weights_data.len() as u64;
        self.enforce_artifact_quota(tenant_id, estimate_bytes)
            .await?;

        // Compute BLAKE3 hash of weights and per-layer hashes
        let hash_b3 = blake3::hash(&weights_data).to_hex().to_string();
        let per_layer_hashes = Self::compute_per_layer_hashes_from_bytes(&weights_data)?;

        let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];
        let (rank, hidden_dim, _, _) = Self::validate_quantized_shapes(weights, config)?;
        let coreml_placement =
            Self::resolve_coreml_placement_spec(&metadata, &modules, rank, hidden_dim)?;
        let base_model_hash = metadata.get("base_model_hash").cloned();

        let scope_value = metadata.get("scope").cloned().unwrap_or_else(default_scope);
        let (mut metadata, training_backend, determinism, scope_path) =
            Self::build_manifest_metadata(metadata, config, &scope_value);

        let fields = extract_manifest_fields(&metadata);
        persist_scope_metadata(&mut metadata, &fields.scope_meta);
        let (coreml, placement, training_backend_details) =
            Self::build_coreml_sections(&metadata, training_backend.as_deref(), config.rank)?;

        // Create manifest
        let manifest = AdapterManifest {
            version: "2.0".to_string(), // AOS 2.0 format
            rank: config.rank,
            base_model: base_model.to_string(),
            base_model_hash,
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            category: fields.category,
            tier: fields.tier,
            per_layer_hashes: Some(per_layer_hashes),
            training_backend,
            determinism,
            coreml_placement: Some(coreml_placement.clone()),
            lora_tier: fields.lora_tier,
            lora_strength: fields.lora_strength,
            scope: scope_value.clone(),
            recommended_for_moe: fields.recommended_for_moe,
            quantization: Some(LORA_Q15_QUANTIZATION.to_string()),
            gate_q15_denominator: Some(ROUTER_GATE_Q15_DENOM as u32),
            coreml,
            placement,
            training_backend_details,
            dataset_version_ids: fields.dataset_version_ids,
            data_spec_hash: fields.data_spec_hash,
            data_lineage_mode: fields.data_lineage_mode,
            synthetic_mode: fields.synthetic_mode,
            training_slice_id: fields.training_slice_id,
            backend_policy: fields.backend_policy,
            kernel_version: Some(adapteros_core::version::VERSION.to_string()),
            moe_config: config.moe_config.clone(),
            scope_repo: fields.scope_meta.scope_repo,
            scope_branch: fields.scope_meta.scope_branch,
            scope_commit: fields.scope_meta.scope_commit,
            scope_scan_root: fields.scope_meta.scope_scan_root,
            scope_remote_url: fields.scope_meta.scope_remote_url,
            repo_slug: fields.scope_meta.repo_slug,
            scan_roots: fields.scope_meta.scan_roots,
            session_id: fields.scope_meta.session_id,
            session_name: fields.scope_meta.session_name,
            session_tags: fields.scope_meta.session_tags,
            stream_mode: fields.stream_mode,
            metadata,
        };

        manifest.validate()?;

        let manifest_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| AosError::Training(format!("Failed to serialize manifest: {}", e)))?;

        let (soft_quota, hard_quota) = Self::artifact_quota_limits();
        let current_usage = self.artifact_usage_for_tenant(tenant_id).await.unwrap_or(0);
        let expected_bytes = weights_data.len() as u64 + manifest_bytes.len() as u64;
        let predicted = current_usage + expected_bytes;
        if predicted > hard_quota {
            return Err(AosError::Validation(format!(
                "Artifact storage quota exceeded: {} > {} bytes",
                predicted, hard_quota
            )));
        }
        if predicted > soft_quota {
            warn!(
                tenant_id = %tenant_id,
                predicted,
                soft_quota,
                "Artifact storage soft quota exceeded"
            );
        }

        // Resolve archive path via storage abstraction (tenant-scoped)
        let aos_key = StorageKey::adapter_artifact(
            Some(tenant_id.to_string()),
            adapter_id,
            None,
            format!("{}.aos", adapter_id),
        );
        let aos_path = storage.path_for(&aos_key)?;
        let mut writer = AosWriter::new();
        writer.add_segment(
            BackendTag::Canonical,
            Some(scope_path.clone()),
            &weights_data,
        )?;
        writer.write_archive(&aos_path, &manifest)?;

        // Deterministic signature for the archive to allow reproducible verification
        self.sign_archive(&aos_path, adapter_id).await?;

        info!(
            path = %aos_path.display(),
            size_kb = weights_data.len() / 1024,
            "AOS archive created successfully"
        );

        Ok(PackagedAdapter {
            adapter_id: adapter_id.to_string(),
            manifest,
            weights_path: aos_path,
            hash_b3,
        })
    }

    /// Package adapter as single .aos archive with branch metadata.
    ///
    /// This method provides a more ergonomic way to include branch and commit
    /// information in the packaged training artifacts. The branch metadata is
    /// merged into the packaging metadata and preserved in the manifest.
    ///
    /// # Arguments
    ///
    /// * `tenant_id` - Tenant identifier for multi-tenant isolation
    /// * `adapter_id` - Unique identifier for this adapter
    /// * `weights` - Quantized LoRA weights to package
    /// * `config` - Training configuration used to produce these weights
    /// * `base_model` - Base model identifier this adapter is trained against
    /// * `branch_metadata` - Git branch and commit information for provenance
    /// * `metadata` - Additional metadata to include in the manifest
    ///
    /// # Example
    ///
    /// ```ignore
    /// let branch_meta = BranchMetadata::new("main", "abc123def")
    ///     .with_repo_name("my-repo")
    ///     .with_remote_url("https://github.com/org/repo");
    ///
    /// let packaged = packager
    ///     .package_aos_with_branch_metadata(
    ///         "tenant-1",
    ///         "adapter-001",
    ///         &weights,
    ///         &config,
    ///         "llama-3.2",
    ///         &branch_meta,
    ///         HashMap::new(),
    ///     )
    ///     .await?;
    /// ```
    #[allow(clippy::too_many_arguments)]
    pub async fn package_aos_with_branch_metadata(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        weights: &QuantizedLoRAWeights,
        config: &TrainingConfig,
        base_model: &str,
        branch_metadata: &BranchMetadata,
        metadata: HashMap<String, String>,
    ) -> Result<PackagedAdapter> {
        // Merge branch metadata into the packaging metadata
        let mut enriched_metadata = metadata;
        for (key, value) in branch_metadata.to_metadata_entries() {
            // Only insert if not already present (explicit metadata takes precedence)
            enriched_metadata.entry(key).or_insert(value);
        }

        // Log branch metadata inclusion for auditability
        if branch_metadata.is_present() {
            info!(
                adapter_id = %adapter_id,
                branch = ?branch_metadata.branch,
                commit = ?branch_metadata.commit,
                repo = ?branch_metadata.repo_name,
                "Including branch metadata in packaged adapter"
            );
        }

        self.package_aos_with_metadata(
            tenant_id,
            adapter_id,
            weights,
            config,
            base_model,
            enriched_metadata,
        )
        .await
    }

    /// Save weights in safetensors format
    async fn save_weights_safetensors(
        &self,
        path: &Path,
        weights: &QuantizedLoRAWeights,
    ) -> Result<Vec<u8>> {
        let data = Self::build_safetensors_bytes(weights)?;

        tokio::fs::write(path, &data)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write weights: {}", e)))?;

        Ok(data)
    }

    /// Compute BLAKE3 hash of file
    async fn compute_hash(&self, path: &Path) -> Result<String> {
        let data = tokio::fs::read(path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read file for hashing: {}", e)))?;

        let hash = blake3::hash(&data);
        Ok(hash.to_hex().to_string())
    }

    /// Canonical logical layer path for manifest keys (e.g., transformer.layer_12.attn.q_proj.lora_A)
    fn canonical_layer_id(tensor_name: &str) -> String {
        let mut segments = Vec::new();
        let mut iter = tensor_name.split(['.', '/']).peekable();

        while let Some(seg) = iter.next() {
            if seg.is_empty() {
                continue;
            }

            let lower = seg.to_lowercase();
            if lower == "weight" {
                continue;
            }

            if lower == "model" || lower == "transformer" {
                if segments.is_empty() {
                    segments.push("transformer".to_string());
                }
                continue;
            }

            if lower == "layers" || lower == "layer" {
                if let Some(next) = iter.peek() {
                    if let Ok(idx) = next.parse::<usize>() {
                        segments.push(format!("layer_{}", idx));
                        iter.next();
                        continue;
                    }
                }
            }

            let normalized = match lower.as_str() {
                "lora_a" => "lora_A".to_string(),
                "lora_b" => "lora_B".to_string(),
                other => other.to_string(),
            };

            segments.push(normalized);
        }

        if segments.is_empty() {
            return tensor_name.to_string();
        }

        if segments.first().map(|s| s.as_str()) != Some("transformer") {
            let mut prefixed = vec!["transformer".to_string()];
            prefixed.extend(segments);
            segments = prefixed;
        }

        segments.join(".")
    }

    /// Serialize quantized weights into safetensors bytes (adapter-only, no base weights)
    fn build_safetensors_bytes(weights: &QuantizedLoRAWeights) -> Result<Vec<u8>> {
        // Dequantize to f32 for runtime backends
        let deq = LoRAQuantizer::dequantize_from_q15(weights);

        // Default module list; future: make configurable
        let modules = ["q_proj", "k_proj", "v_proj", "o_proj"];

        // Build tensor views by reusing the same weights for each module
        let mut tensors: Vec<(String, TensorView)> = Vec::new();

        // Flatten helpers
        fn flatten_2d(m: &Vec<Vec<f32>>) -> Vec<u8> {
            let mut out = Vec::with_capacity(m.len() * m.first().map(|r| r.len()).unwrap_or(0) * 4);
            for row in m {
                for &v in row {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
            out
        }

        let a_rows = deq.lora_a.len(); // rank
        let a_cols = deq.lora_a.first().map(|r| r.len()).unwrap_or(0); // hidden_dim
        let b_rows = deq.lora_b.len(); // hidden_dim
        let b_cols = deq.lora_b.first().map(|r| r.len()).unwrap_or(0); // rank

        let a_bytes = flatten_2d(&deq.lora_a);
        let b_bytes = flatten_2d(&deq.lora_b);

        for name in modules.iter() {
            let a_view = TensorView::new(
                safetensors::Dtype::F32,
                vec![a_rows, a_cols],
                a_bytes.as_slice(),
            )
            .map_err(|e| AosError::Training(format!("safetensors A view error: {}", e)))?;
            let b_view = TensorView::new(
                safetensors::Dtype::F32,
                vec![b_rows, b_cols],
                b_bytes.as_slice(),
            )
            .map_err(|e| AosError::Training(format!("safetensors B view error: {}", e)))?;
            tensors.push((format!("lora_a.{}", name), a_view));
            tensors.push((format!("lora_b.{}", name), b_view));
        }

        debug_assert!(
            tensors
                .iter()
                .all(|(name, _)| name.starts_with("lora_a.") || name.starts_with("lora_b.")),
            "packager must only serialize LoRA tensors; base weights are excluded"
        );

        safetensors::serialize(tensors, &Default::default())
            .map_err(|e| AosError::Training(format!("safetensors serialize error: {}", e)))
    }

    fn compute_per_layer_hashes_from_bytes(
        weights_bytes: &[u8],
    ) -> Result<std::collections::HashMap<String, LayerHash>> {
        let tensors = SafeTensors::deserialize(weights_bytes).map_err(|e| {
            AosError::Training(format!(
                "Failed to parse safetensors for per-layer hashing: {e}"
            ))
        })?;

        let mut hashes = std::collections::HashMap::new();
        for (name, tensor) in tensors.tensors() {
            let canonical = Self::canonical_layer_id(&name);
            let hash = blake3::hash(tensor.data()).to_hex().to_string();
            if hashes
                .insert(
                    canonical.clone(),
                    LayerHash {
                        hash,
                        tensor_name: Some(name.to_string()),
                    },
                )
                .is_some()
            {
                return Err(AosError::Training(format!(
                    "Duplicate canonical layer id detected while hashing: {}",
                    canonical
                )));
            }
        }

        Ok(hashes)
    }

    /// Deterministic manifest signing using Ed25519 seeded from manifest bytes
    async fn sign_manifest(
        &self,
        adapter_dir: &Path,
        adapter_id: &str,
        manifest_bytes: &[u8],
    ) -> Result<()> {
        let keypair = Self::load_signing_keypair("manifest", adapter_id, manifest_bytes)?;
        let signature = keypair.sign(manifest_bytes);

        // Save signature
        let sig_path = adapter_dir.join("signature.sig");
        tokio::fs::write(&sig_path, signature.to_bytes())
            .await
            .map_err(|e| AosError::Training(format!("Failed to write signature: {}", e)))?;

        // Save public key (hex-encoded)
        let pubkey_path = adapter_dir.join("public_key.pem");
        let pubkey_hex = hex::encode(keypair.public_key().to_bytes());
        tokio::fs::write(&pubkey_path, pubkey_hex)
            .await
            .map_err(|e| AosError::Training(format!("Failed to write public key: {}", e)))?;

        info!("Adapter manifest signed deterministically");
        Ok(())
    }

    /// Deterministic archive signing for .aos outputs
    async fn sign_archive(&self, aos_path: &Path, adapter_id: &str) -> Result<()> {
        let archive_bytes = tokio::fs::read(aos_path).await.map_err(|e| {
            AosError::Training(format!("Failed to read archive for signing: {}", e))
        })?;
        let keypair = Self::load_signing_keypair("aos-archive", adapter_id, &archive_bytes)?;
        let signature = keypair.sign(&archive_bytes);

        let sig_path = aos_path.with_extension("aos.sig");
        tokio::fs::write(&sig_path, signature.to_bytes())
            .await
            .map_err(|e| AosError::Training(format!("Failed to write archive signature: {}", e)))?;

        let pubkey_path = aos_path.with_extension("aos.pub");
        let pubkey_hex = hex::encode(keypair.public_key().to_bytes());
        tokio::fs::write(&pubkey_path, pubkey_hex)
            .await
            .map_err(|e| {
                AosError::Training(format!("Failed to write archive public key: {}", e))
            })?;

        info!(
            path = %aos_path.display(),
            sig = %sig_path.display(),
            "AOS archive signed deterministically"
        );

        Ok(())
    }

    fn deterministic_keypair(label: &str, adapter_id: &str, material: &[u8]) -> Keypair {
        let mut hasher = blake3::Hasher::new();
        hasher.update(label.as_bytes());
        hasher.update(adapter_id.as_bytes());
        hasher.update(material);
        let hash = hasher.finalize();
        Keypair::from_bytes(hash.as_bytes())
    }

    /// Load signing keypair: prefer env-provided Ed25519 seed (32-byte hex), fall back to deterministic.
    fn load_signing_keypair(label: &str, adapter_id: &str, material: &[u8]) -> Result<Keypair> {
        if let Ok(hex_seed) = std::env::var("AOS_SIGNING_KEY_HEX") {
            let bytes = hex::decode(hex_seed.trim())
                .map_err(|e| AosError::Training(format!("Invalid AOS_SIGNING_KEY_HEX: {}", e)))?;
            if bytes.len() != 32 {
                return Err(AosError::Training(
                    "AOS_SIGNING_KEY_HEX must be 32 bytes".to_string(),
                ));
            }
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            return Ok(Keypair::from_bytes(&seed));
        }

        // Deterministic fallback (test/dev only)
        Ok(Self::deterministic_keypair(label, adapter_id, material))
    }

    /// Verify adapter signature
    pub async fn verify_signature(&self, adapter_dir: &Path) -> Result<bool> {
        // Read manifest
        let manifest_path = adapter_dir.join("manifest.json");
        let manifest_data = tokio::fs::read(&manifest_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read manifest: {}", e)))?;

        // Read signature
        let sig_path = adapter_dir.join("signature.sig");
        let sig_bytes = tokio::fs::read(&sig_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read signature: {}", e)))?;

        let sig_array: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| AosError::Training("Invalid signature length".to_string()))?;

        let signature = adapteros_crypto::Signature::from_bytes(&sig_array)
            .map_err(|e| AosError::Training(format!("Invalid signature: {}", e)))?;

        // Read public key
        let pubkey_path = adapter_dir.join("public_key.pem");
        let pubkey_hex = tokio::fs::read_to_string(&pubkey_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read public key: {}", e)))?;

        let pubkey_bytes = hex::decode(pubkey_hex.trim())
            .map_err(|e| AosError::Training(format!("Invalid public key hex: {}", e)))?;

        let pubkey_array: [u8; 32] = pubkey_bytes
            .try_into()
            .map_err(|_| AosError::Training("Invalid public key length".to_string()))?;

        let public_key = adapteros_crypto::PublicKey::from_bytes(&pubkey_array)
            .map_err(|e| AosError::Training(format!("Invalid public key: {}", e)))?;

        // Verify signature
        public_key
            .verify(&manifest_data, &signature)
            .map_err(|e| AosError::Training(format!("Signature verification failed: {}", e)))?;

        Ok(true)
    }

    /// Load packaged adapter
    pub async fn load(&self, tenant_id: &str, adapter_id: &str) -> Result<PackagedAdapter> {
        let adapter_dir = self
            .adapter_dir(tenant_id, adapter_id)
            .map_err(|e| AosError::Validation(format!("Invalid adapter path: {}", e)))?;

        // Verify signature first
        if !self.verify_signature(&adapter_dir).await? {
            return Err(AosError::Training(format!(
                "Signature verification failed for adapter: {}",
                adapter_id
            )));
        }

        // Load manifest
        let manifest_path = adapter_dir.join("manifest.json");
        let manifest_data = tokio::fs::read(&manifest_path)
            .await
            .map_err(|e| AosError::Training(format!("Failed to read manifest: {}", e)))?;

        let manifest: AdapterManifest = serde_json::from_slice(&manifest_data)
            .map_err(|e| AosError::Training(format!("Failed to parse manifest: {}", e)))?;

        let weights_path = adapter_dir.join("weights.safetensors");
        let hash_b3 = self.compute_hash(&weights_path).await?;

        // Verify hash matches manifest
        if hash_b3 != manifest.weights_hash {
            return Err(AosError::Training(format!(
                "Hash mismatch: expected {}, got {}",
                manifest.weights_hash, hash_b3
            )));
        }

        Ok(PackagedAdapter {
            adapter_id: adapter_id.to_string(),
            manifest,
            weights_path,
            hash_b3,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_compute_hash() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let temp_dir = tempfile::tempdir_in(&tmp_root).expect("tempdir");
        let test_file = temp_dir.path().join("test.txt");
        tokio::fs::write(&test_file, b"hello world").await.unwrap();

        let packager = AdapterPackager::new(temp_dir.path());
        let hash = packager.compute_hash(&test_file).await.unwrap();

        assert_eq!(hash.len(), 64); // BLAKE3 produces 256-bit hash (64 hex chars)
    }

    #[tokio::test]
    async fn test_save_load_manifest() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let temp_dir = tempfile::tempdir_in(&tmp_root).expect("tempdir");
        let manifest_path = temp_dir.path().join("manifest.json");

        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            rank: 4,
            base_model: "test-model".to_string(),
            base_model_hash: None,
            training_config: TrainingConfig::default(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: "test_hash".to_string(),
            category: default_category(),
            tier: default_tier(),
            per_layer_hashes: None,
            training_backend: Some("cpu".to_string()),
            determinism: default_determinism_mode(),
            lora_tier: None,
            lora_strength: None,
            scope: default_scope(),
            recommended_for_moe: true,
            quantization: Some(LORA_Q15_QUANTIZATION.to_string()),
            gate_q15_denominator: Some(ROUTER_GATE_Q15_DENOM as u32),
            coreml: None,
            placement: None,
            training_backend_details: None,
            coreml_placement: None,
            dataset_version_ids: None,
            data_spec_hash: None,
            data_lineage_mode: None,
            synthetic_mode: None,
            training_slice_id: None,
            backend_policy: None,
            kernel_version: None,
            moe_config: None,
            scope_repo: None,
            scope_branch: None,
            scope_commit: None,
            scope_scan_root: None,
            scope_remote_url: None,
            repo_slug: None,
            scan_roots: Vec::new(),
            session_id: None,
            session_name: None,
            session_tags: None,
            stream_mode: None,
            metadata: std::collections::HashMap::new(),
        };

        let _packager = AdapterPackager::new(temp_dir.path());
        let manifest_bytes = serde_json::to_vec_pretty(&manifest).unwrap();
        tokio::fs::write(&manifest_path, manifest_bytes)
            .await
            .unwrap();

        // Load and verify
        let loaded_data = tokio::fs::read(&manifest_path).await.unwrap();
        let loaded_manifest: AdapterManifest = serde_json::from_slice(&loaded_data).unwrap();

        assert_eq!(loaded_manifest.rank, 4);
        assert_eq!(loaded_manifest.base_model, "test-model");
    }

    #[tokio::test]
    async fn artifact_quota_enforces_hard_limit() {
        let tmp_root = std::path::PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&tmp_root).expect("create var/tmp");
        let temp_dir = tempfile::tempdir_in(&tmp_root).expect("tempdir");
        let tenant_dir = temp_dir.path().join("tenant1").join("adapter");
        tokio::fs::create_dir_all(&tenant_dir).await.unwrap();
        let existing = tenant_dir.join("v1.aos");
        tokio::fs::write(&existing, vec![0u8; 8]).await.unwrap();

        std::env::set_var("AOS_ARTIFACT_HARD_QUOTA_BYTES", "10");
        std::env::set_var("AOS_ARTIFACT_SOFT_QUOTA_BYTES", "8");

        let packager = AdapterPackager::new(temp_dir.path());
        let result = packager.enforce_artifact_quota("tenant1", 5).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_per_layer_hashes_use_canonical_ids() {
        use safetensors::tensor::TensorView;

        let lora_bytes: Vec<u8> = vec![0.1f32, 0.2, 0.3, 0.4]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let tensors = [(
            "model.layers.0.attn.q_proj.lora_A.weight".to_string(),
            TensorView::new(safetensors::Dtype::F32, vec![2, 2], &lora_bytes).unwrap(),
        )];

        let serialized = safetensors::tensor::serialize(tensors, &None).unwrap();
        let hashes =
            AdapterPackager::compute_per_layer_hashes_from_bytes(&serialized).expect("hashing ok");

        let canonical = "transformer.layer_0.attn.q_proj.lora_A";
        let entry = hashes
            .get(canonical)
            .expect("canonical layer entry should exist");
        assert_eq!(
            entry.tensor_name.as_deref(),
            Some("model.layers.0.attn.q_proj.lora_A.weight")
        );
        assert!(!entry.hash.is_empty());
    }

    #[test]
    fn manifest_prefers_actual_backend_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("training_backend".to_string(), "mlx".to_string());
        metadata.insert(
            "training_backend_reason".to_string(),
            "coreml_unavailable".to_string(),
        );
        let (_meta, training_backend, _determinism, _scope) =
            AdapterPackager::build_manifest_metadata(
                metadata,
                &TrainingConfig::default(),
                "project",
            );

        assert_eq!(training_backend.as_deref(), Some("mlx"));
    }

    #[test]
    fn manifest_keeps_backend_reason_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("training_backend".to_string(), "cpu".to_string());
        metadata.insert(
            "training_backend_reason".to_string(),
            "coreml_unavailable".to_string(),
        );
        let config = TrainingConfig::default();
        let (meta, _backend, _determinism, _scope) =
            AdapterPackager::build_manifest_metadata(metadata, &config, "project");

        assert_eq!(
            meta.get("training_backend_reason").map(String::as_str),
            Some("coreml_unavailable")
        );
    }

    #[test]
    fn derives_domain_group_operation_from_defaults() {
        let mut metadata = HashMap::new();
        metadata.insert("adapter_name".to_string(), "my-adapter".to_string());
        metadata.insert("category".to_string(), "code".to_string());
        metadata.insert("scope".to_string(), "project".to_string());

        let (enriched, _, _, scope_path) = AdapterPackager::build_manifest_metadata(
            metadata,
            &TrainingConfig::default(),
            "tenant",
        );

        assert_eq!(enriched.get("domain").unwrap(), "code");
        assert_eq!(enriched.get("group").unwrap(), "project");
        assert_eq!(enriched.get("operation").unwrap(), "my-adapter");
        assert_eq!(scope_path, "code/project/tenant/my-adapter");
    }

    #[test]
    fn respects_provided_hierarchy_overrides() {
        let mut metadata = HashMap::new();
        metadata.insert("domain".to_string(), "custom-domain".to_string());
        metadata.insert("group".to_string(), "custom-group".to_string());
        metadata.insert("operation".to_string(), "custom-op".to_string());

        let (enriched, _, _, scope_path) = AdapterPackager::build_manifest_metadata(
            metadata,
            &TrainingConfig::default(),
            "tenant",
        );

        assert_eq!(enriched.get("domain").unwrap(), "custom-domain");
        assert_eq!(enriched.get("group").unwrap(), "custom-group");
        assert_eq!(enriched.get("operation").unwrap(), "custom-op");
        assert_eq!(scope_path, "custom-domain/custom-group/tenant/custom-op");
    }

    #[test]
    fn invalid_coreml_placement_is_rejected() {
        let mut metadata = HashMap::new();
        let bad_spec = CoremlPlacementSpec {
            version: 1,
            graph_id: Some("graph".to_string()),
            bindings: vec![CoreMLPlacementBinding {
                binding_id: "q_proj".to_string(),
                target: CoreMLTargetRef {
                    layer: "q_proj".to_string(),
                    op_kind: CoreMLOpKind::AttentionQ,
                    path_hint: None,
                },
                projection: CoreMLProjection::InputToHidden,
                rank: 8, // wrong rank/hidden_dim for this test
                alpha: None,
                scale: None,
                gating: None,
                shape: CoreMLPlacementShape {
                    input_dim: 16,
                    output_dim: 16,
                },
            }],
        };
        metadata.insert(
            "coreml_placement".to_string(),
            serde_json::to_string(&bad_spec).unwrap(),
        );

        let config = TrainingConfig {
            rank: 4,
            hidden_dim: 32,
            ..Default::default()
        };

        let err = AdapterPackager::resolve_coreml_placement_spec(
            &metadata,
            &["q_proj"],
            config.rank,
            config.hidden_dim,
        );

        assert!(err.is_err());
    }

    #[test]
    fn default_coreml_placement_covers_modules() {
        let modules = ["q_proj", "o_proj"];
        let spec = AdapterPackager::default_coreml_placement_spec(&modules, 4, 32);
        assert_eq!(spec.version, 1);
        assert_eq!(spec.bindings.len(), modules.len());
        for binding in spec.bindings {
            assert_eq!(binding.rank, 4);
            assert_eq!(binding.shape.input_dim, 32);
            assert_eq!(binding.shape.output_dim, 32);
        }
    }

    #[test]
    fn artifact_quota_limits_respect_env() {
        std::env::set_var("AOS_ARTIFACT_HARD_QUOTA_BYTES", "1000");
        std::env::set_var("AOS_ARTIFACT_SOFT_QUOTA_BYTES", "800");
        let (soft, hard) = AdapterPackager::artifact_quota_limits();
        assert_eq!(hard, 1000);
        assert_eq!(soft, 800);
        std::env::remove_var("AOS_ARTIFACT_HARD_QUOTA_BYTES");
        std::env::remove_var("AOS_ARTIFACT_SOFT_QUOTA_BYTES");
    }

    #[test]
    fn parse_scan_roots_from_json_array() {
        let mut metadata = HashMap::new();
        let scan_roots_json = r#"[
            {"path": "src", "label": "main", "file_count": 100, "byte_count": 50000},
            {"path": "lib", "label": "library"}
        ]"#;
        metadata.insert("scan_roots".to_string(), scan_roots_json.to_string());

        let roots = parse_scan_roots_from_metadata(&metadata);
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].path, "src");
        assert_eq!(roots[0].label, Some("main".to_string()));
        assert_eq!(roots[0].file_count, Some(100));
        assert_eq!(roots[0].byte_count, Some(50000));
        assert_eq!(roots[1].path, "lib");
        assert_eq!(roots[1].label, Some("library".to_string()));
        assert_eq!(roots[1].file_count, None);
    }

    #[test]
    fn parse_scan_roots_from_scope_scan_root_fallback() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_scan_root".to_string(), "/project/src".to_string());
        metadata.insert("scan_root_label".to_string(), "primary".to_string());
        metadata.insert("scan_root_file_count".to_string(), "42".to_string());
        metadata.insert("scan_root_content_hash".to_string(), "abc123".to_string());

        let roots = parse_scan_roots_from_metadata(&metadata);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/project/src");
        assert_eq!(roots[0].label, Some("primary".to_string()));
        assert_eq!(roots[0].file_count, Some(42));
        assert_eq!(roots[0].content_hash, Some("abc123".to_string()));
    }

    #[test]
    fn parse_scan_roots_prefers_relative_paths() {
        let mut metadata = HashMap::new();
        metadata.insert(
            "scan_root_relative".to_string(),
            "packages/core".to_string(),
        );
        metadata.insert(
            "scan_root_path".to_string(),
            "/repo/packages/core".to_string(),
        );

        let roots = parse_scan_roots_from_metadata(&metadata);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "packages/core");
    }

    #[test]
    fn parse_scan_roots_returns_empty_for_no_data() {
        let metadata = HashMap::new();
        let roots = parse_scan_roots_from_metadata(&metadata);
        assert!(roots.is_empty());
    }

    #[test]
    fn extract_scope_metadata_from_canonical_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_repo".to_string(), "my-repo".to_string());
        metadata.insert("repo_slug".to_string(), "my_repo".to_string());
        metadata.insert("scope_branch".to_string(), "main".to_string());
        metadata.insert("scope_commit".to_string(), "abc123".to_string());
        metadata.insert(
            "scope_remote_url".to_string(),
            "https://github.com/org/repo".to_string(),
        );
        metadata.insert("session_id".to_string(), "session-001".to_string());
        metadata.insert("session_name".to_string(), "nightly-run".to_string());
        metadata.insert("session_tags".to_string(), "ci,nightly".to_string());

        let scope = extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_repo, Some("my-repo".to_string()));
        assert_eq!(scope.repo_slug, Some("my_repo".to_string()));
        assert_eq!(scope.scope_branch, Some("main".to_string()));
        assert_eq!(scope.scope_commit, Some("abc123".to_string()));
        assert_eq!(
            scope.scope_remote_url,
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(scope.session_id, Some("session-001".to_string()));
        assert_eq!(scope.session_name, Some("nightly-run".to_string()));
        assert_eq!(
            scope.session_tags,
            Some(vec!["ci".to_string(), "nightly".to_string()])
        );
    }

    #[test]
    fn extract_scope_metadata_falls_back_to_repo_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("repo_name".to_string(), "fallback-repo".to_string());
        metadata.insert("repo_branch".to_string(), "develop".to_string());
        metadata.insert("repo_commit".to_string(), "def456".to_string());
        metadata.insert("repo_path".to_string(), "/home/user/project".to_string());
        metadata.insert(
            "repo_remote".to_string(),
            "git@github.com:org/repo.git".to_string(),
        );

        let scope = extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_repo, Some("fallback-repo".to_string()));
        assert_eq!(scope.scope_branch, Some("develop".to_string()));
        assert_eq!(scope.scope_commit, Some("def456".to_string()));
        assert_eq!(
            scope.scope_scan_root,
            Some("/home/user/project".to_string())
        );
        assert_eq!(
            scope.scope_remote_url,
            Some("git@github.com:org/repo.git".to_string())
        );
    }

    #[test]
    fn extract_scope_metadata_falls_back_to_commit_sha() {
        let mut metadata = HashMap::new();
        metadata.insert("commit_sha".to_string(), "abc123def4567890".to_string());

        let scope = extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_commit, Some("abc123def4567890".to_string()));
    }

    #[test]
    fn branch_metadata_from_metadata_commit_sha_fallback() {
        let mut metadata = HashMap::new();
        metadata.insert("commit_sha".to_string(), "abc123def4567890".to_string());

        let meta = BranchMetadata::from_metadata(&metadata);
        assert_eq!(meta.commit, Some("abc123def4567890".to_string()));
        assert_eq!(meta.commit_full, Some("abc123def4567890".to_string()));
    }

    #[test]
    fn extract_scope_metadata_prefers_canonical_over_fallback() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_repo".to_string(), "canonical-repo".to_string());
        metadata.insert("repo_name".to_string(), "fallback-repo".to_string());

        let scope = extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_repo, Some("canonical-repo".to_string()));
    }

    #[test]
    fn scan_root_metadata_serialization_roundtrip() {
        let root = ScanRootMetadata {
            path: "/project/src".to_string(),
            label: Some("main".to_string()),
            file_count: Some(100),
            byte_count: Some(50000),
            content_hash: Some("blake3hash".to_string()),
            scanned_at: Some("2024-01-15T10:30:00Z".to_string()),
        };

        let json = serde_json::to_string(&root).unwrap();
        let parsed: ScanRootMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(root, parsed);
    }

    #[test]
    fn branch_metadata_new_creates_basic_instance() {
        let meta = BranchMetadata::new("main", "abc123");
        assert_eq!(meta.branch, Some("main".to_string()));
        assert_eq!(meta.commit, Some("abc123".to_string()));
        assert!(meta.is_present());
    }

    #[test]
    fn branch_metadata_builder_pattern() {
        let meta = BranchMetadata::new("feature/xyz", "def456")
            .with_full_commit("def456789abcdef0123456789abcdef012345678")
            .with_repo_name("my-repo")
            .with_repo_slug("my_repo")
            .with_remote_url("https://github.com/org/repo")
            .with_dirty(true)
            .with_captured_at("2024-01-15T10:30:00Z");

        assert_eq!(meta.branch, Some("feature/xyz".to_string()));
        assert_eq!(meta.commit, Some("def456".to_string()));
        assert_eq!(
            meta.commit_full,
            Some("def456789abcdef0123456789abcdef012345678".to_string())
        );
        assert_eq!(meta.repo_name, Some("my-repo".to_string()));
        assert_eq!(meta.repo_slug, Some("my_repo".to_string()));
        assert_eq!(
            meta.remote_url,
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(meta.dirty, Some(true));
        assert_eq!(meta.captured_at, Some("2024-01-15T10:30:00Z".to_string()));
    }

    #[test]
    fn branch_metadata_to_metadata_entries() {
        let meta = BranchMetadata::new("main", "abc123")
            .with_repo_name("test-repo")
            .with_repo_slug("test_repo")
            .with_remote_url("git@github.com:org/repo.git");

        let entries = meta.to_metadata_entries();
        assert_eq!(entries.get("scope_branch"), Some(&"main".to_string()));
        assert_eq!(entries.get("scope_commit"), Some(&"abc123".to_string()));
        assert_eq!(entries.get("scope_repo"), Some(&"test-repo".to_string()));
        assert_eq!(entries.get("repo_slug"), Some(&"test_repo".to_string()));
        assert_eq!(
            entries.get("scope_remote_url"),
            Some(&"git@github.com:org/repo.git".to_string())
        );
    }

    #[test]
    fn branch_metadata_from_metadata_canonical_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_branch".to_string(), "develop".to_string());
        metadata.insert("scope_commit".to_string(), "xyz789".to_string());
        metadata.insert("scope_repo".to_string(), "canonical-repo".to_string());
        metadata.insert("repo_slug".to_string(), "canonical_repo".to_string());
        metadata.insert(
            "scope_remote_url".to_string(),
            "https://github.com/org/repo".to_string(),
        );
        metadata.insert("scope_dirty".to_string(), "true".to_string());

        let meta = BranchMetadata::from_metadata(&metadata);
        assert_eq!(meta.branch, Some("develop".to_string()));
        assert_eq!(meta.commit, Some("xyz789".to_string()));
        assert_eq!(meta.repo_name, Some("canonical-repo".to_string()));
        assert_eq!(meta.repo_slug, Some("canonical_repo".to_string()));
        assert_eq!(
            meta.remote_url,
            Some("https://github.com/org/repo".to_string())
        );
        assert_eq!(meta.dirty, Some(true));
    }

    #[test]
    fn branch_metadata_from_metadata_fallback_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("repo_branch".to_string(), "fallback-branch".to_string());
        metadata.insert("repo_commit".to_string(), "fallback-commit".to_string());
        metadata.insert("repo_name".to_string(), "fallback-repo".to_string());
        metadata.insert("repo_remote".to_string(), "git@fallback.git".to_string());

        let meta = BranchMetadata::from_metadata(&metadata);
        assert_eq!(meta.branch, Some("fallback-branch".to_string()));
        assert_eq!(meta.commit, Some("fallback-commit".to_string()));
        assert_eq!(meta.repo_name, Some("fallback-repo".to_string()));
        assert_eq!(meta.remote_url, Some("git@fallback.git".to_string()));
    }

    #[test]
    fn branch_metadata_prefers_canonical_over_fallback() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_branch".to_string(), "canonical".to_string());
        metadata.insert("repo_branch".to_string(), "fallback".to_string());
        metadata.insert("branch".to_string(), "base".to_string());

        let meta = BranchMetadata::from_metadata(&metadata);
        assert_eq!(meta.branch, Some("canonical".to_string()));
    }

    #[test]
    fn branch_metadata_is_present_checks_branch_or_commit() {
        let empty = BranchMetadata::default();
        assert!(!empty.is_present());

        let with_branch = BranchMetadata {
            branch: Some("main".to_string()),
            ..Default::default()
        };
        assert!(with_branch.is_present());

        let with_commit = BranchMetadata {
            commit: Some("abc123".to_string()),
            ..Default::default()
        };
        assert!(with_commit.is_present());
    }

    #[test]
    fn branch_metadata_serialization_roundtrip() {
        let meta = BranchMetadata::new("main", "abc123")
            .with_full_commit("abc123456789")
            .with_repo_name("test-repo")
            .with_repo_slug("test_repo")
            .with_remote_url("https://github.com/org/repo")
            .with_dirty(false)
            .with_captured_at("2024-01-15T10:30:00Z");

        let json = serde_json::to_string(&meta).unwrap();
        let parsed: BranchMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta.branch, parsed.branch);
        assert_eq!(meta.commit, parsed.commit);
        assert_eq!(meta.commit_full, parsed.commit_full);
        assert_eq!(meta.repo_name, parsed.repo_name);
        assert_eq!(meta.repo_slug, parsed.repo_slug);
        assert_eq!(meta.remote_url, parsed.remote_url);
        assert_eq!(meta.dirty, parsed.dirty);
        assert_eq!(meta.captured_at, parsed.captured_at);
    }

    #[test]
    fn branch_metadata_entries_exclude_none_values() {
        let meta = BranchMetadata {
            branch: Some("main".to_string()),
            commit: None,
            ..Default::default()
        };

        let entries = meta.to_metadata_entries();
        assert!(entries.contains_key("scope_branch"));
        assert!(!entries.contains_key("scope_commit"));
        assert!(!entries.contains_key("scope_repo"));
    }
    #[test]
    fn extract_manifest_fields_includes_scope_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("scope_repo".to_string(), "test-repo".to_string());
        metadata.insert("scope_branch".to_string(), "feature-branch".to_string());
        metadata.insert("scope_commit".to_string(), "abc123def456".to_string());
        metadata.insert(
            "scope_scan_root".to_string(),
            "/path/to/project".to_string(),
        );
        metadata.insert(
            "scope_remote_url".to_string(),
            "https://github.com/org/test-repo".to_string(),
        );
        metadata.insert("session_id".to_string(), "session-xyz".to_string());
        metadata.insert("session_name".to_string(), "release-run".to_string());
        metadata.insert("session_tags".to_string(), "release,prod".to_string());
        metadata.insert("category".to_string(), "code".to_string());
        metadata.insert("tier".to_string(), "warm".to_string());

        let fields = extract_manifest_fields(&metadata);

        // Verify scope metadata is extracted correctly
        assert_eq!(fields.scope_meta.scope_repo, Some("test-repo".to_string()));
        assert_eq!(
            fields.scope_meta.scope_branch,
            Some("feature-branch".to_string())
        );
        assert_eq!(
            fields.scope_meta.scope_commit,
            Some("abc123def456".to_string())
        );
        assert_eq!(
            fields.scope_meta.scope_scan_root,
            Some("/path/to/project".to_string())
        );
        assert_eq!(
            fields.scope_meta.scope_remote_url,
            Some("https://github.com/org/test-repo".to_string())
        );
        assert_eq!(
            fields.scope_meta.session_id,
            Some("session-xyz".to_string())
        );
        assert_eq!(
            fields.scope_meta.session_name,
            Some("release-run".to_string())
        );
        assert_eq!(
            fields.scope_meta.session_tags,
            Some(vec!["prod".to_string(), "release".to_string()])
        );

        // Verify other fields are also extracted
        assert_eq!(fields.category, "code");
        assert_eq!(fields.tier, "warm");
    }

    #[test]
    fn extract_manifest_fields_with_scan_roots_json() {
        let mut metadata = HashMap::new();
        let scan_roots_json = r#"[
            {"path": "src", "label": "main", "file_count": 150},
            {"path": "tests", "label": "tests", "file_count": 50}
        ]"#;
        metadata.insert("scan_roots".to_string(), scan_roots_json.to_string());
        metadata.insert("scope_repo".to_string(), "multi-root-repo".to_string());

        let fields = extract_manifest_fields(&metadata);

        assert_eq!(fields.scope_meta.scan_roots.len(), 2);
        assert_eq!(fields.scope_meta.scan_roots[0].path, "src");
        assert_eq!(
            fields.scope_meta.scan_roots[0].label,
            Some("main".to_string())
        );
        assert_eq!(fields.scope_meta.scan_roots[0].file_count, Some(150));
        assert_eq!(fields.scope_meta.scan_roots[1].path, "tests");
        assert_eq!(
            fields.scope_meta.scope_repo,
            Some("multi-root-repo".to_string())
        );
    }
}
