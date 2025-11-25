//! Capacity Model API handlers for PRD G3
//!
//! Endpoints:
//! - GET /v1/system/capacity - Get system capacity model (RAM, VRAM, limits, usage, health)

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::users::Role;
use adapteros_lora_worker::memory::MemoryPressureLevel;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::{debug, warn};
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
    require_any_role(
        &claims,
        &[Role::Admin, Role::Operator, Role::Viewer, Role::SRE],
    )?;

    debug!("Querying system capacity");

    // Get system memory info from UMA monitor
    let uma_stats = state.uma_monitor.get_uma_stats().await;
    let total_ram_bytes = (uma_stats.total_mb as u64) * 1024 * 1024;
    let ram_used_bytes = (uma_stats.used_mb as u64) * 1024 * 1024;
    let ram_headroom_pct = uma_stats.headroom_pct;
    
    // Get VRAM info (GPU memory) - integrate with worker if available
    let (total_vram_bytes, vram_used_bytes) = get_vram_info(&state).await;
    let vram_headroom_pct = if total_vram_bytes > 0 {
        ((total_vram_bytes - vram_used_bytes) as f32 / total_vram_bytes as f32) * 100.0
    } else {
        100.0
    };

    // Get configured limits from config (PRD G3: Read from ApiConfig)
    let config = state.config.read().unwrap();
    let limits = config.capacity_limits.clone();

    // Get current usage from database
    let models_loaded = sqlx::query("SELECT COUNT(*) FROM adapters WHERE load_state = 'loaded'")
        .fetch_one(state.db.pool())
        .await
        .ok()
        .and_then(|row| row.try_get::<i64, _>(0).ok())
        .unwrap_or(0) as usize;

    let adapters_loaded = sqlx::query("SELECT COUNT(*) FROM adapters WHERE load_state IN ('loaded', 'warm', 'hot', 'resident')")
        .fetch_one(state.db.pool())
        .await
        .ok()
        .and_then(|row| row.try_get::<i64, _>(0).ok())
        .unwrap_or(0) as usize;

    // Get active requests count - query from request_log table (PRD G3: Query in_progress status)
    let active_requests = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM request_log WHERE status = 'in_progress'"
    )
    .fetch_one(state.db.pool())
    .await
    .ok()
    .unwrap_or(0) as usize;

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
/// **LIMITATION (PRD G3):** Worker doesn't expose a public `memory_report()` method.
/// The kernels (Metal/MLX/CoreML) have `memory_report()` but it's wrapped in private `Arc<Mutex<K>>`.
/// TODO: Add `pub fn memory_report(&self) -> GpuMemoryReport` to Worker struct.
async fn get_vram_info(state: &AppState) -> (u64, u64) {
    // Try to get VRAM info from worker's kernel backend
    if let Some(ref worker_mutex) = state.worker {
        let worker = worker_mutex.lock().await;
        // NOTE: Worker.kernels is private, so we can't access memory_report() directly.
        // The kernel backends (MetalKernels, MLXKernels, CoreMLKernels) all implement
        // FusedKernels trait with memory_report() -> GpuMemoryReport, but Worker doesn't
        // expose a public method to access it.
        //
        // For now, return defaults. When Worker exposes memory_report(), update this to:
        // let report = worker.memory_report();
        // (report.adapter_vram_total, report.pool_stats.allocated_bytes)
        
        debug!("Worker available but VRAM tracking requires Worker.memory_report() method - using defaults");
        (8 * 1024 * 1024 * 1024, 0) // 8GB total, 0 used (placeholder until Worker exposes memory_report)
    } else {
        debug!("Worker not available - using VRAM defaults");
        (8 * 1024 * 1024 * 1024, 0) // 8GB total, 0 used
    }
}

