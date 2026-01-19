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

/// Tenant usage statistics (PRD-004)
///
/// Returns real resource metrics calculated from database tables and filesystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantUsageResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub tenant_id: String,
    /// Storage used by tenant artifacts, adapters, and datasets (in GB)
    pub storage_used_gb: f64,
    /// CPU usage percentage (rolling 5-minute window)
    pub cpu_usage_pct: f64,
    /// GPU usage percentage (rolling 5-minute window)
    pub gpu_usage_pct: f64,
    /// Memory used by system (in GB) - per-tenant attribution is approximate
    pub memory_used_gb: f64,
    /// Total system memory (in GB)
    pub memory_total_gb: f64,
    /// Number of inference operations in last 24 hours
    pub inference_count_24h: i64,
    /// Number of active adapters
    pub active_adapters_count: i32,
    // Optional legacy fields
    pub avg_latency_ms: Option<f64>,
    pub estimated_cost_usd: Option<f64>,
}

/// Comprehensive tenant resource metrics (PRD-004)
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
