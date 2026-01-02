//! Tenant management types

use serde::{Deserialize, Serialize};

use crate::schema_version;

/// Create tenant request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct CreateTenantRequest {
    pub name: String,
    pub itar_flag: bool,
}

/// Tenant response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub itar_flag: bool,
    pub created_at: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_adapters: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_training_jobs: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_storage_gb: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_rpm: Option<i32>,
}

/// Update tenant request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub itar_flag: Option<bool>,
    pub max_adapters: Option<i32>,
    pub max_training_jobs: Option<i32>,
    pub max_storage_gb: Option<f64>,
    pub rate_limit_rpm: Option<i32>,
}

/// Tenant usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantUsageResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub tenant_id: String,
    pub cpu_usage_pct: f64,
    pub gpu_usage_pct: f64,
    pub memory_used_gb: f64,
    pub memory_total_gb: f64,
    pub inference_count_24h: i64,
    pub active_adapters_count: i32,
    // Optional legacy fields
    pub avg_latency_ms: Option<f64>,
    pub estimated_cost_usd: Option<f64>,
}

/// Set default stack request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct SetDefaultStackRequest {
    pub stack_id: String,
}

/// Default stack response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct DefaultStackResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub tenant_id: String,
    pub stack_id: String,
}

/// Assign policies to tenant request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AssignPoliciesRequest {
    pub policy_ids: Vec<String>,
}

/// Assign adapters to tenant request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct AssignAdaptersRequest {
    pub adapter_ids: Vec<String>,
}
