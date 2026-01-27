//! Tenant management types

use adapteros_types::tenants::{Tenant, TenantUsage};
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
    #[serde(flatten)]
    pub tenant: Tenant,
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
///
/// Returns real resource metrics calculated from database tables and filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantUsageResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    #[serde(flatten)]
    pub usage: TenantUsage,
    // Optional legacy fields
    pub avg_latency_ms: Option<f64>,
    pub estimated_cost_usd: Option<f64>,
}

/// Comprehensive tenant resource metrics
///
/// Returns detailed resource metrics with Prometheus-compatible labels.
/// This is the dedicated metrics endpoint for tenant resource monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantResourceMetricsResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub tenant_id: String,
    /// Timestamp of metrics collection (ISO 8601)
    pub collected_at: String,
    /// Storage metrics
    pub storage: TenantStorageMetricsData,
    /// Compute metrics
    pub compute: TenantComputeMetricsData,
    /// Memory metrics
    pub memory: TenantMemoryMetricsData,
}

/// Storage metrics breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantStorageMetricsData {
    /// Total storage used (GB)
    pub total_gb: f64,
    /// Configured storage quota (GB, if any)
    pub quota_gb: Option<f64>,
    /// Cache TTL for this metric (seconds)
    pub cache_ttl_secs: u64,
}

/// Compute metrics breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantComputeMetricsData {
    /// CPU usage percentage (0-100)
    pub cpu_usage_pct: f64,
    /// GPU usage percentage (0-100)
    pub gpu_usage_pct: f64,
    /// Window duration for rolling metrics (seconds)
    pub window_secs: u64,
}

/// Memory metrics breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantMemoryMetricsData {
    /// System memory used (GB)
    pub used_gb: f64,
    /// Total system memory (GB)
    pub total_gb: f64,
    /// Available system memory (GB)
    pub available_gb: f64,
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
