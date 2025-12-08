//! Adapter management types

use adapteros_types::training::LoraTier;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

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

/// Adapter health status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterHealthResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub adapter_id: String,
    pub total_activations: i32,
    pub selected_count: i32,
    pub avg_gate_value: f64,
    pub memory_usage_mb: f64,
    pub policy_violations: Vec<String>,
    pub recent_activations: Vec<AdapterActivationResponse>,
}
