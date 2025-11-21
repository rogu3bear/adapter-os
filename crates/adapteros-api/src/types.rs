//! API types (OpenAPI compatible)
//!
//! This module provides serializable API types for the AdapterOS REST API.
//! All types derive `Serialize` and `Deserialize` for JSON serialization.

use serde::{Deserialize, Serialize};

// Re-export worker types for API compatibility
pub use adapteros_lora_worker::{InferenceRequest, InferenceResponse};

/// API health status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    /// Health status (e.g., "healthy", "degraded", "unhealthy")
    pub status: String,
    /// ISO 8601 timestamp
    pub timestamp: String,
    /// Optional version information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Adapter information response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    /// Adapter identifier
    pub id: String,
    /// Adapter name
    pub name: String,
    /// Current lifecycle state
    pub lifecycle_state: String,
    /// Activation percentage (0-100)
    pub activation_pct: f32,
    /// Whether the adapter is pinned
    pub is_pinned: bool,
    /// Optional expiration timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// List adapters response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListAdaptersResponse {
    /// List of adapters
    pub adapters: Vec<AdapterInfo>,
    /// Total count
    pub total: usize,
}

/// Load adapter request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadAdapterRequest {
    /// Adapter identifier to load
    pub adapter_id: String,
    /// Optional tenant context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

/// Load adapter response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadAdapterResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// New lifecycle state after loading
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lifecycle_state: Option<String>,
}

/// Swap adapters request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapAdaptersRequest {
    /// Current adapter to unload
    pub from_adapter_id: String,
    /// New adapter to load
    pub to_adapter_id: String,
    /// Optional tenant context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

/// Swap adapters response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapAdaptersResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Router configuration response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouterConfigResponse {
    /// Number of top adapters to select (K in K-sparse)
    pub k: usize,
    /// Whether router is enabled
    pub enabled: bool,
    /// Current routing strategy
    pub strategy: String,
}

/// Training job status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingJobStatus {
    /// Job identifier
    pub job_id: String,
    /// Current status (pending, running, completed, failed, cancelled)
    pub status: String,
    /// Progress percentage (0-100)
    pub progress_pct: f32,
    /// Current loss value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loss: Option<f32>,
    /// Tokens processed per second
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_per_sec: Option<f32>,
    /// Optional error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Generic API success response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiSuccessResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
