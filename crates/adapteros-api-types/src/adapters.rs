//! Adapter management types

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
    pub tier: i32,
    pub languages: Vec<String>,
    pub framework: Option<String>,
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
    pub tier: i32,
    pub languages: Vec<String>,
    pub framework: Option<String>,
    pub created_at: String,
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
    pub tier: i32,
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
