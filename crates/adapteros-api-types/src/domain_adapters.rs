//! Domain adapter types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::schema_version;

/// Create domain adapter request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CreateDomainAdapterRequest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain_type: String,
    pub model: String,
    pub hash: String,
    pub input_format: String,
    pub output_format: String,
    pub config: HashMap<String, serde_json::Value>,
}

/// Domain adapter response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DomainAdapterResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain_type: String,
    pub model: String,
    pub hash: String,
    pub input_format: String,
    pub output_format: String,
    pub config: HashMap<String, serde_json::Value>,
    pub status: String,
    pub epsilon_stats: Option<EpsilonStatsResponse>,
    pub last_execution: Option<String>,
    pub execution_count: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// Epsilon statistics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct EpsilonStatsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub mean_error: f64,
    pub max_error: f64,
    pub error_count: u64,
    pub last_updated: String,
}

/// Test domain adapter request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TestDomainAdapterRequest {
    pub adapter_id: String,
    pub input_data: String,
    pub expected_output: Option<String>,
    pub iterations: Option<u32>,
}

/// Test domain adapter response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TestDomainAdapterResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub test_id: String,
    pub adapter_id: String,
    pub input_data: String,
    pub actual_output: String,
    pub expected_output: Option<String>,
    pub epsilon: Option<f64>,
    pub passed: bool,
    pub iterations: u32,
    pub execution_time_ms: u64,
    pub executed_at: String,
}

/// Domain adapter test response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DomainAdapterTestResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub adapter_id: String,
    pub success: bool,
    pub output: Option<serde_json::Value>,
    pub error: Option<String>,
    pub execution_time_ms: u64,
}

/// Domain adapter manifest response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DomainAdapterManifestResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub adapter_id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub domain_type: String,
    pub model: String,
    pub hash: String,
    pub input_format: String,
    pub output_format: String,
    pub config: HashMap<String, serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// Load domain adapter request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct LoadDomainAdapterRequest {
    pub adapter_id: String,
    pub executor_config: Option<HashMap<String, serde_json::Value>>,
}

/// Domain adapter execution response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DomainAdapterExecutionResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub execution_id: String,
    pub adapter_id: String,
    pub input_hash: String,
    pub output_hash: String,
    pub epsilon: f64,
    pub execution_time_ms: u64,
    pub trace_events: Vec<String>,
    pub executed_at: String,
}
