//! Capacity Model API handlers for PRD G3
//!
//! Endpoints:
//! - GET /v1/system/capacity - Get system capacity model (RAM, VRAM, limits, usage, health)

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_lora_worker::memory::MemoryPressureLevel;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::debug;
use utoipa::ToSchema;

/// Node health indicator
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeHealth {
    Ok,
    Warning,
    Critical,
}

/// Capacity model response
#[derive(Debug, Serialize, ToSchema)]
pub struct CapacityResponse {
    /// Total system RAM in bytes
    pub total_ram_bytes: u64,
    /// Total VRAM in bytes (GPU memory)
    pub total_vram_bytes: u64,
    /// Configured limits
    pub limits: crate::state::CapacityLimits,
    /// Current usage
    pub usage: CapacityUsage,
    /// Node health indicator
    pub node_health: NodeHealth,
}

/// Current capacity usage
#[derive(Debug, Serialize, ToSchema)]
pub struct CapacityUsage {
    /// Number of models currently loaded
    pub models_loaded: usize,
    /// Number of adapters currently loaded
    pub adapters_loaded: usize,
    /// Number of active requests
    pub active_requests: usize,
    /// RAM used in bytes
    pub ram_used_bytes: u64,
    /// VRAM used in bytes
    pub vram_used_bytes: u64,
    /// RAM headroom percentage
    pub ram_headroom_pct: f32,
    /// VRAM headroom percentage
    pub vram_headroom_pct: f32,
}

/// GPU memory report response
#[derive(Debug, Serialize, ToSchema)]
pub struct MemoryReportResponse {
    /// Total GPU memory in bytes
    pub total_gpu_memory_bytes: u64,
    /// Used GPU memory in bytes
    pub used_gpu_memory_bytes: u64,
    /// Available GPU memory in bytes
    pub available_gpu_memory_bytes: u64,
    /// GPU memory headroom percentage
    pub gpu_headroom_pct: f32,
    /// Per-adapter memory usage
    pub per_adapter_usage: Vec<AdapterMemoryUsage>,
}

/// Per-adapter memory usage
#[derive(Debug, Serialize, ToSchema)]
pub struct AdapterMemoryUsage {
    /// Adapter ID
    pub adapter_id: String,
    /// Memory used in bytes
    pub memory_bytes: u64,
}

/// GET /v1/system/capacity - Get system capacity model
#[utoipa::path(
    get,
    path = "/v1/system/capacity",
    responses(
        (status = 200, description = "Capacity model", body = CapacityResponse)
    ),
    tag = "system",
    security(("bearer_token" = []))
)]
pub async fn get_capacity(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<CapacityResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check: MetricsView required for system capacity information
    require_permission(&claims, Permission::MetricsView)?;

    debug!("Querying system capacity");

    // Get system memory info from UMA monitor
    let uma_stats = state.uma_monitor.get_uma_stats().await;
    let total_ram_bytes = uma_stats.total_mb * 1024 * 1024;
    let ram_used_bytes = uma_stats.used_mb * 1024 * 1024;
    let ram_headroom_pct = uma_stats.headroom_pct;

    // Get VRAM info (GPU memory) - integrate with worker if available
    let (total_vram_bytes, vram_used_bytes) = get_vram_info(&state).await;
    let vram_headroom_pct = if total_vram_bytes > 0 {
        ((total_vram_bytes - vram_used_bytes) as f32 / total_vram_bytes as f32) * 100.0
    } else {
        100.0
    };

    // Get configured limits from config (PRD G3: Read from ApiConfig)
    let limits = {
        let config = state.config.read().map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Configuration lock poisoned").with_code("INTERNAL_ERROR")),
            )
        })?;
        config.capacity_limits.clone()
    };

    // Get current usage from database
    let models_loaded = state.db.count_loaded_models().await.unwrap_or(0) as usize;

    // Count adapters in various load states
    let adapters_loaded = state
        .db
        .count_adapters_by_load_state()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|(state, _)| matches!(state.as_str(), "loaded" | "warm" | "hot" | "resident"))
        .map(|(_, count)| count)
        .sum::<i64>() as usize;

    // Get active requests count - query from request_log table (PRD G3: Query in_progress status)
    // Note: This table may not exist yet, so we use a fallback approach
    let active_requests = if state.db.table_exists("request_log").await.unwrap_or(false) {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM request_log WHERE status = 'in_progress'",
        )
        .fetch_one(state.db.pool())
        .await
        .ok()
        .unwrap_or(0) as usize
    } else {
        0
    };

    // Determine node health based on headroom (matching MemoryPressureLevel thresholds)
    // Critical: < 15% (min_headroom), High: 15-20%, Medium: 20-30%, Low: >= 30%
    let node_health = {
        let pressure = state.uma_monitor.get_current_pressure();
        match pressure {
            MemoryPressureLevel::Critical => NodeHealth::Critical,
            MemoryPressureLevel::High => NodeHealth::Warning,
            MemoryPressureLevel::Medium => NodeHealth::Warning,
            MemoryPressureLevel::Low => NodeHealth::Ok,
        }
    };

    Ok(Json(CapacityResponse {
        total_ram_bytes,
        total_vram_bytes,
        limits,
        usage: CapacityUsage {
            models_loaded,
            adapters_loaded,
            active_requests,
            ram_used_bytes,
            vram_used_bytes,
            ram_headroom_pct,
            vram_headroom_pct,
        },
        node_health,
    }))
}

/// Get VRAM information from worker if available
///
/// Queries the Worker's kernel backend for real GPU memory metrics.
/// Falls back to defaults if worker is unavailable or doesn't support memory reporting.
async fn get_vram_info(state: &AppState) -> (u64, u64) {
    // Check if worker is available
    if let Some(ref worker_handle) = state.worker {
        let worker = worker_handle.lock().await;
        if let Some(report) = worker.memory_report().await {
            debug!(
                total_gpu_bytes = report.total_gpu_bytes,
                used_gpu_bytes = report.used_gpu_bytes,
                adapter_count = report.adapter_count,
                "Retrieved GPU memory report from worker"
            );
            return (report.total_gpu_bytes, report.used_gpu_bytes);
        }
        debug!("Worker available but memory_report() returned None - using defaults");
        (8 * 1024 * 1024 * 1024, 0) // 8GB total, 0 used (fallback)
    } else {
        debug!("Worker not available - using VRAM defaults");
        (8 * 1024 * 1024 * 1024, 0) // 8GB total, 0 used
    }
}

/// GET /v1/system/memory/gpu - Get GPU memory report
#[utoipa::path(
    get,
    path = "/v1/system/memory/gpu",
    responses(
        (status = 200, description = "GPU memory report with per-adapter usage", body = MemoryReportResponse)
    ),
    tag = "system",
    security(("bearer_token" = []))
)]
pub async fn get_memory_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MemoryReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check: MetricsView required for system memory information
    require_permission(&claims, Permission::MetricsView)?;

    debug!("Querying GPU memory report");

    // Get VRAM info from worker
    let (total_vram, used_vram) = get_vram_info(&state).await;
    let available_vram = total_vram.saturating_sub(used_vram);
    let gpu_headroom_pct = if total_vram > 0 {
        (available_vram as f32 / total_vram as f32) * 100.0
    } else {
        100.0
    };

    // Get per-adapter memory usage from worker's memory report
    let per_adapter_usage: Vec<AdapterMemoryUsage> = if let Some(ref worker_handle) = state.worker {
        let worker = worker_handle.lock().await;
        if let Some(report) = worker.memory_report().await {
            report
                .adapter_allocations
                .into_iter()
                .map(|(adapter_id, memory_bytes)| AdapterMemoryUsage {
                    adapter_id: adapter_id.to_string(),
                    memory_bytes,
                })
                .collect()
        } else {
            debug!("Worker available but memory_report() returned None - no per-adapter usage");
            vec![]
        }
    } else {
        debug!("Worker not available - no per-adapter usage available");
        vec![]
    };

    Ok(Json(MemoryReportResponse {
        total_gpu_memory_bytes: total_vram,
        used_gpu_memory_bytes: used_vram,
        available_gpu_memory_bytes: available_vram,
        gpu_headroom_pct,
        per_adapter_usage,
    }))
}
