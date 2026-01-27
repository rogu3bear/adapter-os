//! Tenant domain types
//!
//! This module provides the single source of truth for Tenant-related data structures.

use serde::{Deserialize, Serialize};

/// Core tenant record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct Tenant {
    /// Unique identifier for the tenant
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// ITAR compliance flag
    pub itar_flag: bool,
    /// Creation timestamp (ISO 8601)
    pub created_at: String,
    /// Current status (active, paused, archived)
    pub status: Option<String>,
    /// Last update timestamp (ISO 8601)
    pub updated_at: Option<String>,
    /// Default stack identifier for this tenant
    pub default_stack_id: Option<String>,
    /// Maximum number of adapters this tenant can have
    pub max_adapters: Option<i32>,
    /// Maximum number of concurrent training jobs
    pub max_training_jobs: Option<i32>,
    /// Maximum storage quota in GB
    pub max_storage_gb: Option<f64>,
    /// Rate limit in requests per minute
    pub rate_limit_rpm: Option<i32>,
    /// Default pinned adapter IDs for new sessions (JSON array)
    pub default_pinned_adapter_ids: Option<String>,
    /// Maximum KV cache size in bytes
    pub max_kv_cache_bytes: Option<i64>,
    /// Identifier for the KV residency policy
    pub kv_residency_policy_id: Option<String>,
}

/// Tenant usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub struct TenantUsage {
    /// Associated tenant identifier
    pub tenant_id: String,
    /// Number of active adapters
    pub active_adapters_count: i32,
    /// Number of currently running training jobs
    pub running_training_jobs: i32,
    /// Inference request count in the last 24 hours
    pub inference_count_24h: i64,
    /// Storage used in GB
    pub storage_used_gb: f64,
    /// CPU usage percentage
    pub cpu_usage_pct: f64,
    /// GPU usage percentage
    pub gpu_usage_pct: f64,
    /// Memory used in GB
    pub memory_used_gb: f64,
    /// Total memory available in GB
    pub memory_total_gb: f64,
}
