//! Tenant management types

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::schema_version;

/// Create tenant request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CreateTenantRequest {
    pub name: String,
    pub itar_flag: bool,
}

/// Tenant response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TenantResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub itar_flag: bool,
    pub created_at: String,
    pub status: String,
}

/// Update tenant request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UpdateTenantRequest {
    pub name: Option<String>,
    pub itar_flag: Option<bool>,
}

/// Tenant usage statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
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
