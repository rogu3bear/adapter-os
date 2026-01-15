//! System information and metadata handlers

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::*;
use adapteros_lora_worker::memory::UmaStats;
use axum::extract::{Extension, State};
use axum::Json;
use serde::Serialize;

/// Get system metadata
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/meta",
    responses(
        (status = 200, description = "System metadata", body = MetaResponse)
    )
)]
pub async fn meta(State(state): State<AppState>) -> Json<MetaResponse> {
    // Read actual runtime config values
    let (production_mode, dev_bypass, dev_login_enabled) = {
        let cfg = state.config.read().unwrap_or_else(|e| e.into_inner());
        (
            cfg.server.production_mode,
            cfg.security.dev_bypass,
            cfg.security.dev_login_enabled,
        )
    };

    // Derive environment from production_mode
    let environment = if production_mode {
        "production".to_string()
    } else if dev_bypass {
        "dev-bypass".to_string()
    } else {
        "dev".to_string()
    };

    Json(MetaResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_hash: option_env!("BUILD_HASH").unwrap_or("dev").to_string(),
        build_date: option_env!("BUILD_DATE").unwrap_or("unknown").to_string(),
        environment,
        production_mode,
        dev_login_enabled,
    })
}

// ============================================================================
// Resource Usage Endpoint
// ============================================================================

/// Memory usage breakdown by component
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MemoryUsage {
    pub total_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub headroom_pct: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ane: Option<AneUsage>,
}

/// Apple Neural Engine usage (when available)
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AneUsage {
    pub allocated_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub usage_pct: f32,
}

/// Compute usage metrics
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ComputeUsage {
    pub pressure_level: String,
    pub active_workers: usize,
    pub total_workers: usize,
}

/// Worker resource summary
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct WorkerResourceSummary {
    pub worker_id: String,
    pub status: String,
    pub adapters_loaded: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<i64>,
}

/// Response for GET /v1/system/resource-usage
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ResourceUsageResponse {
    pub memory: MemoryUsage,
    pub compute: ComputeUsage,
    pub workers: Vec<WorkerResourceSummary>,
    pub total_adapters_loaded: usize,
    pub timestamp: String,
}

/// Get system resource usage
///
/// Returns a comprehensive view of system resource usage including memory,
/// compute, and worker status.
#[utoipa::path(
    get,
    path = "/v1/system/resource-usage",
    responses(
        (status = 200, description = "Resource usage metrics", body = ResourceUsageResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "system"
)]
pub async fn get_resource_usage(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<ResourceUsageResponse> {
    require_permission(&claims, Permission::MetricsView)?;

    // Get UMA memory stats
    let uma_stats = state.uma_monitor.get_uma_stats().await;
    let pressure = state.uma_monitor.get_current_pressure();
    let ane_usage = ane_usage_from_stats(&uma_stats);

    // Get workers
    let workers = state.db.list_all_workers().await.map_err(|e| {
        tracing::error!(error = %e, "Failed to list workers");
        ApiError::db_error(e)
    })?;

    let total_workers = workers.len();
    // Workers in 'healthy' status are actively serving inference requests
    let active_workers = workers
        .iter()
        .filter(|w| w.status == "healthy")
        .count();

    // Get loaded adapters count
    let loaded_adapters = match state
        .db
        .list_adapters_by_state(&claims.tenant_id, "hot")
        .await
    {
        Ok(adapters) => adapters.len(),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to count loaded adapters - defaulting to 0");
            0
        }
    };

    // Build worker summaries
    let worker_summaries: Vec<WorkerResourceSummary> = workers
        .iter()
        .map(|w| WorkerResourceSummary {
            worker_id: w.id.clone(),
            status: w.status.clone(),
            adapters_loaded: 0, // Would need per-worker adapter count
            memory_bytes: None, // Worker struct doesn't track per-worker memory
        })
        .collect();

    Ok(Json(ResourceUsageResponse {
        memory: MemoryUsage {
            total_mb: uma_stats.total_mb,
            used_mb: uma_stats.used_mb,
            available_mb: uma_stats.available_mb,
            headroom_pct: uma_stats.headroom_pct,
            ane: ane_usage,
        },
        compute: ComputeUsage {
            pressure_level: pressure.to_string(),
            active_workers,
            total_workers,
        },
        workers: worker_summaries,
        total_adapters_loaded: loaded_adapters,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

fn ane_usage_from_stats(stats: &UmaStats) -> Option<AneUsage> {
    let allocated_mb = stats.ane_allocated_mb?;
    let used_mb = stats.ane_used_mb?;
    let available_mb = stats.ane_available_mb?;
    let usage_pct = stats.ane_usage_percent?;

    Some(AneUsage {
        allocated_mb,
        used_mb,
        available_mb,
        usage_pct,
    })
}

// Re-export system info handlers from parent module for routes.rs
pub use super::{__path_get_uma_memory, get_uma_memory};
