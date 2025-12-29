//! Adapter packaging with safetensors and manifest generation
//!
//! Packages trained LoRA adapters into a format compatible with mplora-artifacts.

use super::quantizer::{LoRAQuantizer, QuantizedLoRAWeights};
use super::trainer::{MoETrainingConfig, TrainingConfig};
use adapteros_aos::{AosWriter, BackendTag};
use adapteros_core::{AosError, RepoAdapterPaths, Result};
use adapteros_crypto::Keypair;
use adapteros_lora_router::ROUTER_GATE_Q15_DENOM;
use adapteros_storage::{ByteStorage, FsByteStorage, StorageKey, StorageKind};
use adapteros_types::coreml::{
    CoreMLOpKind, CoreMLPlacementBinding, CoreMLPlacementShape, CoreMLPlacementSpec,
    CoreMLProjection, CoreMLTargetRef,
};
use adapteros_types::training::LoraTier;
use chrono::Utc;
use safetensors::tensor::TensorView;
use safetensors::SafeTensors;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
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
    /// All scan root paths used during training package creation.
    /// Captures multiple roots when training combines content from different directories.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scan_roots: Vec<ScanRootMetadata>,
    /// Session identifier for correlating ingestion workflows
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

fn parse_lora_tier(metadata: &HashMap<String, String>) -> Option<LoraTier> {
    metadata.get("lora_tier").and_then(|v| match v.as_str() {
        "micro" => Some(LoraTier::Micro),
        "standard" => Some(LoraTier::Standard),
        "max" => Some(LoraTier::Max),
        _ => None,
    })
}

fn default_strength_for_tier(tier: Option<LoraTier>) -> Option<f32> {
    match tier {
        Some(LoraTier::Micro) => Some(0.25),
        Some(LoraTier::Standard) => Some(0.5),
        Some(LoraTier::Max) => Some(1.0),
        None => None,
    }
}

/// Parse scan-root metadata from the metadata HashMap.
///
/// Supports two formats:
/// 1. JSON array in `scan_roots` key: `[{"path": "src", "label": "main"}, ...]`
/// 2. Single scan root from `scope_scan_root` key with optional supporting fields
fn parse_scan_roots_from_metadata(metadata: &HashMap<String, String>) -> Vec<ScanRootMetadata> {
    // Try parsing JSON array first
    if let Some(raw) = metadata.get("scan_roots") {
        if let Ok(roots) = serde_json::from_str::<Vec<ScanRootMetadata>>(raw) {
            if !roots.is_empty() {
                return roots;
            }
        }
    }

    // Fall back to single scan root from scope_scan_root
    if let Some(path) = metadata.get("scope_scan_root") {
        if !path.trim().is_empty() {
            let root = ScanRootMetadata {
                path: path.clone(),
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
    }

    Vec::new()
}

/// Codebase scope metadata extracted from the metadata HashMap.
#[derive(Debug, Clone, Default)]
struct ScopeMetadataExtract {
    scope_repo: Option<String>,
    scope_branch: Option<String>,
    scope_commit: Option<String>,
    scope_scan_root: Option<String>,
    scope_remote_url: Option<String>,
    session_id: Option<String>,
    scan_roots: Vec<ScanRootMetadata>,
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
        if let Some(ref remote_url) = self.remote_url {
            entries.insert("scope_remote_url".to_string(), remote_url.clone());
        }
        if let Some(dirty) = self.dirty {
            entries.insert("scope_dirty".to_string(), dirty.to_string());
        }
        if let Some(ref captured_at) = self.captured_at {
            entries.insert("branch_metadata_captured_at".to_string(), captured_at.clone());
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
            branch: metadata
                .get("scope_branch")
                .or_else(|| metadata.get("repo_branch"))
                .or_else(|| metadata.get("branch"))
                .cloned(),
            commit: metadata
                .get("scope_commit")
                .or_else(|| metadata.get("repo_commit"))
                .or_else(|| metadata.get("commit"))
                .cloned(),
            commit_full: metadata
                .get("scope_commit_full")
                .or_else(|| metadata.get("commit_full"))
                .cloned(),
            repo_name: metadata
                .get("scope_repo")
                .or_else(|| metadata.get("repo_name"))
                .cloned(),
            remote_url: metadata
                .get("scope_remote_url")
                .or_else(|| metadata.get("repo_remote"))
                .or_else(|| metadata.get("remote_url"))
                .cloned(),
            dirty: metadata
                .get("scope_dirty")
                .or_else(|| metadata.get("dirty"))
                .and_then(|v| v.parse().ok()),
            captured_at: metadata
                .get("branch_metadata_captured_at")
                .or_else(|| metadata.get("captured_at"))
                .cloned(),
        }
    }
}

/// Extract codebase scope metadata from the metadata HashMap.
fn extract_scope_metadata(metadata: &HashMap<String, String>) -> ScopeMetadataExtract {
    ScopeMetadataExtract {
        scope_repo: metadata
            .get("scope_repo")
            .or_else(|| metadata.get("repo_name"))
            .cloned(),
        scope_branch: metadata
            .get("scope_branch")
            .or_else(|| metadata.get("repo_branch"))
            .cloned(),
        scope_commit: metadata
            .get("scope_commit")
            .or_else(|| metadata.get("repo_commit"))
            .cloned(),
        scope_scan_root: metadata
            .get("scope_scan_root")
            .or_else(|| metadata.get("repo_path"))
            .cloned(),
        scope_remote_url: metadata
            .get("scope_remote_url")
            .or_else(|| metadata.get("repo_remote"))
            .cloned(),
        session_id: metadata.get("session_id").cloned(),
        scan_roots: parse_scan_roots_from_metadata(metadata),
    }
}

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

        // #region agent log
        if let Ok(mut f) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("/Users/mln-dev/Dev/adapter-os/.cursor/debug.log")
        {
            let _ = writeln!(
                f,
                r#"{{"sessionId":"debug-session","runId":"pre-fix","hypothesisId":"H2","location":"training/packager.rs:lineage_mode","message":"evaluated lineage mode","data":{{"lineage_mode":"{}","dataset_ids_present":{}}},"timestamp":{}}}"#,
                lineage_mode,
                dataset_ids_present,
                Utc::now().timestamp_millis()
            );
        }
        // #endregion

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

        // runtime-only knob; exclude from persisted .aos metadata
        manifest_metadata.remove("routing_determinism_mode");

        // Standard quantization + determinism annotations
        manifest_metadata
            .entry("quantization".to_string())
            .or_insert_with(|| "q15".to_string());
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
        let (metadata, training_backend, determinism, _scope_path) =
            Self::build_manifest_metadata(metadata, config, &scope_value);
        let lora_tier = parse_lora_tier(&metadata);
        let lora_strength = metadata
            .get("lora_strength")
            .and_then(|v| v.parse::<f32>().ok())
            .or_else(|| default_strength_for_tier(lora_tier));
        let category = metadata
            .get("category")
            .cloned()
            .unwrap_or_else(default_category);
        let tier = metadata.get("tier").cloned().unwrap_or_else(default_tier);
        let (coreml, placement, training_backend_details) =
            Self::build_coreml_sections(&metadata, training_backend.as_deref(), config.rank)?;

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
        let training_slice_id = metadata.get("training_slice_id").cloned();
        let backend_policy = metadata.get("backend_policy").cloned();
        let data_lineage_mode = metadata.get("data_lineage_mode").cloned();
        let synthetic_mode = metadata
            .get("synthetic_mode")
            .map(|v| v == "true" || v == "1");
        let recommended_for_moe = metadata
            .get("recommended_for_moe")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);

        // Extract codebase scope metadata (scan roots, repo info, session)
        let scope_meta = extract_scope_metadata(&metadata);

        // Create manifest
        let manifest = AdapterManifest {
            version: "1.0.0".to_string(),
            rank: config.rank,
            base_model: base_model.to_string(),
            base_model_hash,
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            category,
            tier,
            per_layer_hashes: Some(per_layer_hashes),
            training_backend,
            determinism,
            coreml_placement: Some(coreml_placement.clone()),
            lora_tier,
            lora_strength,
            scope: scope_value,
            recommended_for_moe,
            quantization: Some("q15".to_string()),
            gate_q15_denominator: Some(ROUTER_GATE_Q15_DENOM as u32),
            coreml,
            placement,
            training_backend_details,
            dataset_version_ids,
            data_spec_hash,
            data_lineage_mode,
            synthetic_mode,
            training_slice_id,
            backend_policy,
            kernel_version: Some(adapteros_core::version::VERSION.to_string()),
            moe_config: config.moe_config.clone(),
            scope_repo: scope_meta.scope_repo,
            scope_branch: scope_meta.scope_branch,
            scope_commit: scope_meta.scope_commit,
            scope_scan_root: scope_meta.scope_scan_root,
            scope_remote_url: scope_meta.scope_remote_url,
            scan_roots: scope_meta.scan_roots,
            session_id: scope_meta.session_id,
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
        let (metadata, training_backend, determinism, scope_path) =
            Self::build_manifest_metadata(metadata, config, &scope_value);
        let lora_tier = parse_lora_tier(&metadata);
        let lora_strength = metadata
            .get("lora_strength")
            .and_then(|v| v.parse::<f32>().ok())
            .or_else(|| default_strength_for_tier(lora_tier));
        let category = metadata
            .get("category")
            .cloned()
            .unwrap_or_else(default_category);
        let tier = metadata.get("tier").cloned().unwrap_or_else(default_tier);
        let (coreml, placement, training_backend_details) =
            Self::build_coreml_sections(&metadata, training_backend.as_deref(), config.rank)?;
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
        let training_slice_id = metadata.get("training_slice_id").cloned();
        let backend_policy = metadata.get("backend_policy").cloned();
        let data_lineage_mode = metadata.get("data_lineage_mode").cloned();
        let synthetic_mode = metadata
            .get("synthetic_mode")
            .map(|v| v == "true" || v == "1");
        let recommended_for_moe = metadata
            .get("recommended_for_moe")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);

        // Extract codebase scope metadata (scan roots, repo info, session)
        let scope_meta = extract_scope_metadata(&metadata);

        // Create manifest
        let manifest = AdapterManifest {
            version: "2.0".to_string(), // AOS 2.0 format
            rank: config.rank,
            base_model: base_model.to_string(),
            base_model_hash,
            training_config: config.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: hash_b3.clone(),
            category,
            tier,
            per_layer_hashes: Some(per_layer_hashes),
            training_backend,
            determinism,
            coreml_placement: Some(coreml_placement.clone()),
            lora_tier,
            lora_strength,
            scope: scope_value.clone(),
            recommended_for_moe,
            quantization: Some("q15".to_string()),
            gate_q15_denominator: Some(ROUTER_GATE_Q15_DENOM as u32),
            coreml,
            placement,
            training_backend_details,
            dataset_version_ids,
            data_spec_hash,
            data_lineage_mode,
            synthetic_mode,
            training_slice_id,
            backend_policy,
            kernel_version: Some(adapteros_core::version::VERSION.to_string()),
            moe_config: config.moe_config.clone(),
            scope_repo: scope_meta.scope_repo,
            scope_branch: scope_meta.scope_branch,
            scope_commit: scope_meta.scope_commit,
            scope_scan_root: scope_meta.scope_scan_root,
            scope_remote_url: scope_meta.scope_remote_url,
            scan_roots: scope_meta.scan_roots,
            session_id: scope_meta.session_id,
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
        let aos_key = StorageKey {
            tenant_id: Some(tenant_id.to_string()),
            object_id: adapter_id.to_string(),
            version_id: None,
            file_name: format!("{}.aos", adapter_id),
            kind: StorageKind::AdapterArtifact,
        };
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
            quantization: Some("q15".to_string()),
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
            scan_roots: Vec::new(),
            session_id: None,
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
        metadata.insert(
            "scan_root_content_hash".to_string(),
            "abc123".to_string(),
        );

        let roots = parse_scan_roots_from_metadata(&metadata);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].path, "/project/src");
        assert_eq!(roots[0].label, Some("primary".to_string()));
        assert_eq!(roots[0].file_count, Some(42));
        assert_eq!(roots[0].content_hash, Some("abc123".to_string()));
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
        metadata.insert("scope_branch".to_string(), "main".to_string());
        metadata.insert("scope_commit".to_string(), "abc123".to_string());
        metadata.insert("scope_remote_url".to_string(), "https://github.com/org/repo".to_string());
        metadata.insert("session_id".to_string(), "session-001".to_string());

        let scope = extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_repo, Some("my-repo".to_string()));
        assert_eq!(scope.scope_branch, Some("main".to_string()));
        assert_eq!(scope.scope_commit, Some("abc123".to_string()));
        assert_eq!(scope.scope_remote_url, Some("https://github.com/org/repo".to_string()));
        assert_eq!(scope.session_id, Some("session-001".to_string()));
    }

    #[test]
    fn extract_scope_metadata_falls_back_to_repo_keys() {
        let mut metadata = HashMap::new();
        metadata.insert("repo_name".to_string(), "fallback-repo".to_string());
        metadata.insert("repo_branch".to_string(), "develop".to_string());
        metadata.insert("repo_commit".to_string(), "def456".to_string());
        metadata.insert("repo_path".to_string(), "/home/user/project".to_string());
        metadata.insert("repo_remote".to_string(), "git@github.com:org/repo.git".to_string());

        let scope = extract_scope_metadata(&metadata);
        assert_eq!(scope.scope_repo, Some("fallback-repo".to_string()));
        assert_eq!(scope.scope_branch, Some("develop".to_string()));
        assert_eq!(scope.scope_commit, Some("def456".to_string()));
        assert_eq!(scope.scope_scan_root, Some("/home/user/project".to_string()));
        assert_eq!(scope.scope_remote_url, Some("git@github.com:org/repo.git".to_string()));
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
}
