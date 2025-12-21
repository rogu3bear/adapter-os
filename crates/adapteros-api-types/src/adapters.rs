//! Adapter management types

use adapteros_types::{coreml::CoreMLMode, repository::RepoTier, training::LoraTier};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::schema_version;
use crate::training::DatasetVersionTrustSnapshot;

/// Register adapter request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RegisterAdapterRequest {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    /// Adapter tier: 'persistent', 'warm', or 'ephemeral'
    pub tier: String,
    pub languages: Vec<String>,
    pub framework: Option<String>,
    /// Adapter category: 'code', 'framework', 'codebase', or 'ephemeral'
    pub category: String,
    /// Adapter scope: 'global', 'tenant', 'repo', or 'commit'
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Expiration timestamp (ISO 8601 format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Adapter response
///
/// # State Fields
///
/// This type exposes two distinct state concepts:
///
/// - `lifecycle_state`: The adapter's lifecycle phase in the release workflow.
///   Values: "draft", "active", "deprecated", "retired"
///   This is the **canonical field** for adapter maturity/release status.
///
/// - `runtime_state`: The adapter's current memory/runtime status.
///   Values: "unloaded", "cold", "warm", "hot", "resident"
///   This reflects whether the adapter is loaded and at what priority tier.
///
/// Note: In the database, `current_state` maps to `runtime_state` in the API
/// to provide clearer semantics.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    /// Storage tier: 'persistent', 'warm', or 'ephemeral'
    pub tier: String,
    /// Assurance tier for drift/determinism (low|standard|high)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assurance_tier: Option<String>,
    /// Supported programming languages
    pub languages: Vec<String>,
    pub framework: Option<String>,
    /// Adapter category: 'code', 'framework', 'codebase', or 'ephemeral'
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Adapter scope: 'global', 'tenant', 'repo', or 'commit'
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Marketing/operational tier for routing (micro/standard/max)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(value_type = String)]
    pub lora_tier: Option<LoraTier>,
    /// Runtime strength multiplier (scales LoRA application without changing alpha)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_strength: Option<f32>,
    /// Logical scope for routing (may mirror scope)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_scope: Option<String>,
    /// Framework identifier for code intelligence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework_id: Option<String>,
    /// Framework version for code intelligence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,
    /// Repository identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_id: Option<String>,
    /// Git commit SHA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_sha: Option<String>,
    /// Adapter intent/purpose
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    pub stats: Option<AdapterStats>,
    /// Adapter version from migration 0068 (semantic or monotonic)
    pub version: String,
    /// Lifecycle state from migration 0068 (draft/active/deprecated/retired)
    /// This is the canonical field for adapter maturity/release status.
    pub lifecycle_state: String,
    /// Runtime state indicating memory/load status (unloaded/cold/warm/hot/resident)
    /// Maps from database `current_state` field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_state: Option<String>,
    /// Whether adapter is pinned (protected from eviction)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    /// Whether the adapter was deduplicated (found existing instead of creating new)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deduplicated: Option<bool>,
    /// Memory usage in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<i64>,
    /// Drift/determinism metadata from harness runs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_reference_backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_baseline_backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_test_backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_metric: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_loss_metric: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_slice_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_slice_offset: Option<u64>,
}

/// Adapter statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterStats {
    pub total_activations: i64,
    pub selected_count: i64,
    pub avg_gate_value: f64,
    pub selection_rate: f64,
}

/// Adapter activation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterActivationResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub adapter_id: String,
    pub request_id: Option<String>,
    pub gate_value: f64,
    pub selected: bool,
    pub created_at: String,
}

/// Adapter state transition response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterStateResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub adapter_id: String,
    pub old_state: String,
    pub new_state: String,
    pub timestamp: String,
}

/// Adapter manifest for download
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterManifest {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    /// Storage tier: 'persistent', 'warm', or 'ephemeral'
    pub tier: String,
    /// LoRA strength multiplier [0.0, 1.0]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lora_strength: Option<f32>,
    pub framework: Option<String>,
    pub languages_json: Option<String>,
    pub category: Option<String>,
    pub scope: Option<String>,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Adapter repository response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterRepositoryResponse {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub base_model_id: Option<String>,
    pub default_branch: String,
    pub archived: bool,
    pub created_by: Option<String>,
    pub created_at: String,
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub training_policy: Option<AdapterRepositoryPolicyResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterRepositoryPolicyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_backends: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_allowed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autopromote_coreml: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_mode: Option<CoreMLMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_tier: Option<RepoTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_rollback_on_trust_regress: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterRepositoryPolicyResponse {
    pub repo_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_backends: Option<Vec<String>>,
    pub coreml_allowed: bool,
    pub coreml_required: bool,
    pub autopromote_coreml: bool,
    #[serde(default)]
    pub coreml_mode: CoreMLMode,
    #[serde(default)]
    pub repo_tier: RepoTier,
    pub auto_rollback_on_trust_regress: bool,
    pub created_at: String,
}

/// Adapter version response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterVersionResponse {
    pub id: String,
    pub repo_id: String,
    pub tenant_id: String,
    pub version: String,
    pub branch: String,
    pub aos_path: Option<String>,
    pub aos_hash: Option<String>,
    pub manifest_schema_version: Option<String>,
    pub parent_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_hash: Option<String>,
    pub training_backend: Option<String>,
    pub coreml_used: Option<bool>,
    pub coreml_device_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_version_trust: Option<Vec<DatasetVersionTrustSnapshot>>,
    pub adapter_trust_state: String,
    pub release_state: String,
    pub metrics_snapshot_id: Option<String>,
    pub evaluation_summary: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_state: Option<String>,
    #[serde(default)]
    pub serveable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serveable_reason: Option<String>,
}

/// Create repository request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateRepositoryRequest {
    pub tenant_id: String,
    pub name: String,
    pub base_model_id: Option<String>,
    pub description: Option<String>,
    pub default_branch: Option<String>,
}

/// Create repository response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateRepositoryResponse {
    pub repo_id: String,
}

/// Create draft version request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateDraftVersionRequest {
    pub repo_id: String,
    pub branch: String,
    pub parent_version_id: Option<String>,
    pub code_commit_sha: Option<String>,
    pub data_spec_hash: Option<String>,
}

/// Create draft version response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateDraftVersionResponse {
    pub version_id: String,
}

/// Promote version request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PromoteVersionRequest {
    pub repo_id: String,
}

/// Rollback version request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct RollbackVersionRequest {
    pub branch: String,
    pub target_version_id: String,
}

/// Tag version request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TagVersionRequest {
    pub tag_name: String,
}

/// Resolve version request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ResolveVersionRequest {
    pub selector: String,
}

/// Resolve version response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ResolveVersionResponse {
    pub version_id: Option<String>,
}

/// Adapter health status
///
/// # Health flags (precedence: corrupt > unsafe > degraded > healthy)
/// | Flag     | Trigger examples                                                   |
/// |----------|-------------------------------------------------------------------|
/// | healthy  | Trust allowed & no drift/storage issues                           |
/// | degraded | High drift for tier, trust warning, or minor warnings             |
/// | unsafe   | Trust blocked or regressed datasets                               |
/// | corrupt  | Storage hash mismatch or missing artifacts                        |
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterHealthResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub adapter_id: String,
    /// Rolled-up adapter health
    pub health: AdapterHealthFlag,
    /// Primary contributing subcode for surfacing in UIs/CLIs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_subcode: Option<AdapterHealthSubcode>,
    /// Detailed health signals grouped by domain
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub subcodes: Vec<AdapterHealthSubcode>,
    /// Aggregate drift summary (per-tier thresholds apply)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drift_summary: Option<AdapterDriftSummary>,
    /// Dataset linkage and trust status
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datasets: Vec<AdapterDatasetHealth>,
    /// Storage/reconciler status for artifacts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<AdapterStorageHealth>,
    /// Backend/CoreML info surfaced for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<AdapterBackendHealth>,
    /// Recent activations and telemetry
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_activations: Vec<AdapterActivationResponse>,
    #[serde(default)]
    pub total_activations: i32,
    #[serde(default)]
    pub selected_count: i32,
    #[serde(default)]
    pub avg_gate_value: f64,
    #[serde(default)]
    pub memory_usage_mb: f64,
    #[serde(default)]
    pub policy_violations: Vec<String>,
}

/// Canonical adapter health states (roll-up)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterHealthFlag {
    Healthy,
    Degraded,
    Unsafe,
    Corrupt,
}

/// Domains/categories for health sub-codes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterHealthDomain {
    Drift,
    Trust,
    Storage,
    Other,
}

/// A single health sub-code with domain context.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterHealthSubcode {
    /// Category of the signal (drift/trust/storage/etc)
    pub domain: AdapterHealthDomain,
    /// Machine-readable code (e.g., "drift_high", "trust_blocked")
    pub code: String,
    /// Human-readable detail for debugging
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Optional structured payload for UI (thresholds, values, links)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Drift summary, including thresholds and current score.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterDriftSummary {
    pub current: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hard_threshold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
}

/// Dataset linkage + trust state used for health rollup.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterDatasetHealth {
    pub dataset_version_id: String,
    pub trust_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overall_trust_status: Option<String>,
}

/// Storage/reconciler status for adapter artifacts.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterStorageHealth {
    pub reconciler_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issues: Option<Vec<AdapterHealthSubcode>>,
}

/// Backend/CoreML info for debugging surfaced on the adapter detail view.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterBackendHealth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_device_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coreml_used: Option<bool>,
}
