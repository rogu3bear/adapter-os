//! Adapter management types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Register adapter request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterResponse {
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
}

/// Adapter statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterStats {
    pub total_activations: i64,
    pub selected_count: i64,
    pub avg_gate_value: f64,
    pub selection_rate: f64,
}

/// Adapter activation response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterActivationResponse {
    pub id: String,
    pub adapter_id: String,
    pub request_id: Option<String>,
    pub gate_value: f64,
    pub selected: bool,
    pub created_at: String,
}

/// Adapter state transition response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterStateResponse {
    pub adapter_id: String,
    pub old_state: String,
    pub new_state: String,
    pub timestamp: String,
}

/// Adapter manifest for download
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
pub struct AdapterHealthResponse {
    pub adapter_id: String,
    pub total_activations: i32,
    pub selected_count: i32,
    pub avg_gate_value: f64,
    pub memory_usage_mb: f64,
    pub policy_violations: Vec<String>,
    pub recent_activations: Vec<AdapterActivationResponse>,
}

/// Hot-swap request for updating an adapter to a new .aos path
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HotSwapRequest {
    /// Path to new .aos file
    pub new_path: String,
}

/// Hot-swap response containing timing and previous adapter
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HotSwapResponse {
    pub adapter_id: String,
    /// Swap time in milliseconds
    pub swap_time_ms: u64,
    /// Previous adapter ID (if any)
    pub old_adapter: Option<String>,
}

// (duplication removed)
