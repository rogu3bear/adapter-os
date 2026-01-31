//! Model management types
//!
//! Types for model listing, status, import, and lifecycle operations.

use serde::{Deserialize, Serialize};

use crate::{schema_version, ModelLoadStatus};

/// ANE memory status for CoreML models
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AneMemoryStatus {
    pub allocated_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub usage_pct: f32,
}

/// Base model status response (from /v1/models/status/all)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct BaseModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    pub status: ModelLoadStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unloaded_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
    pub updated_at: String,
}

/// Model status response (from /v1/models/{id}/status endpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<String>,
    pub status: ModelLoadStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loaded_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane_memory: Option<AneMemoryStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uma_pressure_level: Option<String>,
}

/// All models status response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct AllModelsStatusResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub models: Vec<BaseModelStatusResponse>,
    pub total_memory_mb: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_memory_mb: Option<i64>,
    pub active_model_count: i64,
}

/// Model list response (from /internal/models endpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ModelListResponse<T> {
    pub models: Vec<T>,
    pub total: usize,
}

/// Import model request (for POST /v1/models/import)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SeedModelRequest {
    pub model_name: String,
    pub model_path: String,
    /// Format: "mlx", "safetensors", "pytorch", "gguf"
    pub format: String,
    /// Backend: "mlx", "metal", "coreml"
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Import model response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SeedModelResponse {
    pub import_id: String,
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<i32>,
}
