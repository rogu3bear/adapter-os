//! Capacity Model API handlers for PRD G3
//!
//! Endpoints:
//! - GET /v1/system/capacity - Get system capacity model (RAM, VRAM, limits, usage, health)

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_core::error_codes;
use adapteros_lora_worker::memory::MemoryPressureLevel;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use tracing::debug;
use utoipa::ToSchema;

fn is_schema_contract_violation(message: &str) -> bool {
    message.contains("no such table")
        || message.contains("no such column")
        || message.contains("has no column named")
}

fn map_capacity_db_error<E: std::fmt::Display>(error: E) -> (StatusCode, Json<ErrorResponse>) {
    let message = error.to_string();
    if is_schema_contract_violation(&message) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                ErrorResponse::new("schema contract violation")
                    .with_code(error_codes::SCHEMA_CONTRACT_VIOLATION)
                    .with_string_details(message),
            ),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(
            ErrorResponse::new("database error")
                .with_code("DATABASE_ERROR")
                .with_string_details(message),
        ),
    )
}

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
    /// Whether GPU memory telemetry is currently available.
    pub availability: GpuMemoryAvailability,
    /// Total GPU memory in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_gpu_memory_bytes: Option<u64>,
    /// Used GPU memory in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_gpu_memory_bytes: Option<u64>,
    /// Available GPU memory in bytes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_gpu_memory_bytes: Option<u64>,
    /// GPU memory headroom percentage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_headroom_pct: Option<f32>,
    /// Per-adapter memory usage
    pub per_adapter_usage: Vec<AdapterMemoryUsage>,
}

/// Availability state for GPU memory telemetry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GpuMemoryAvailability {
    Available,
    Unavailable,
}

/// Per-adapter memory usage
#[derive(Debug, Clone, Serialize, ToSchema)]
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

    // Get VRAM info (GPU memory) from telemetry when available.
    let gpu_snapshot = get_gpu_memory_snapshot(&state).await;
    let total_vram_bytes = gpu_snapshot.total_gpu_bytes.unwrap_or(0);
    let vram_used_bytes = gpu_snapshot.used_gpu_bytes.unwrap_or(0);
    let vram_headroom_pct = match (gpu_snapshot.total_gpu_bytes, gpu_snapshot.used_gpu_bytes) {
        (Some(total), Some(used)) if total > 0 => {
            ((total.saturating_sub(used)) as f32 / total as f32) * 100.0
        }
        _ => 0.0,
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
    let models_loaded = state
        .db
        .count_loaded_models()
        .await
        .map_err(map_capacity_db_error)? as usize;

    // Count adapters in various load states
    let adapters_loaded = state
        .db
        .count_adapters_by_load_state()
        .await
        .map_err(map_capacity_db_error)?
        .into_iter()
        .filter(|(state, _)| matches!(state.as_str(), "loaded" | "warm" | "hot" | "resident"))
        .map(|(_, count)| count)
        .sum::<i64>() as usize;

    // Get active requests count - query from request_log table (PRD G3: Query in_progress status)
    let active_requests = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM request_log WHERE status = 'in_progress'",
    )
    .fetch_one(state.db.pool())
    .await
    .map_err(map_capacity_db_error)? as usize;

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

#[derive(Debug, Clone)]
struct GpuMemorySnapshot {
    total_gpu_bytes: Option<u64>,
    used_gpu_bytes: Option<u64>,
    per_adapter_usage: Vec<AdapterMemoryUsage>,
}

/// Get GPU memory information from worker telemetry when available.
///
/// Returns `None` values when worker telemetry is unavailable instead of
/// fabricating synthetic VRAM defaults.
async fn get_gpu_memory_snapshot(state: &AppState) -> GpuMemorySnapshot {
    if let Some(ref worker_handle) = state.worker {
        let worker = worker_handle.lock().await;
        if let Some(report) = worker.memory_report().await {
            debug!(
                total_gpu_bytes = report.total_gpu_bytes,
                used_gpu_bytes = report.used_gpu_bytes,
                adapter_count = report.adapter_count,
                "Retrieved GPU memory report from worker"
            );
            return GpuMemorySnapshot {
                total_gpu_bytes: Some(report.total_gpu_bytes),
                used_gpu_bytes: Some(report.used_gpu_bytes),
                per_adapter_usage: report
                    .adapter_allocations
                    .into_iter()
                    .map(|(adapter_id, memory_bytes)| AdapterMemoryUsage {
                        adapter_id: adapter_id.to_string(),
                        memory_bytes,
                    })
                    .collect(),
            };
        }
        debug!("Worker available but memory_report() returned None");
    } else {
        debug!("Worker not available for GPU memory telemetry");
    }

    GpuMemorySnapshot {
        total_gpu_bytes: None,
        used_gpu_bytes: None,
        per_adapter_usage: Vec::new(),
    }
}

fn compute_gpu_report(
    total_gpu_bytes: Option<u64>,
    used_gpu_bytes: Option<u64>,
) -> (GpuMemoryAvailability, Option<u64>, Option<f32>) {
    match (total_gpu_bytes, used_gpu_bytes) {
        (Some(total), Some(used)) if total > 0 => {
            let available = total.saturating_sub(used);
            let headroom_pct = (available as f32 / total as f32) * 100.0;
            (
                GpuMemoryAvailability::Available,
                Some(available),
                Some(headroom_pct),
            )
        }
        _ => (GpuMemoryAvailability::Unavailable, None, None),
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

    // Get GPU memory snapshot from worker telemetry.
    let gpu_snapshot = get_gpu_memory_snapshot(&state).await;
    let (availability, available_vram, gpu_headroom_pct) =
        compute_gpu_report(gpu_snapshot.total_gpu_bytes, gpu_snapshot.used_gpu_bytes);

    Ok(Json(MemoryReportResponse {
        availability,
        total_gpu_memory_bytes: gpu_snapshot.total_gpu_bytes,
        used_gpu_memory_bytes: gpu_snapshot.used_gpu_bytes,
        available_gpu_memory_bytes: available_vram,
        gpu_headroom_pct,
        per_adapter_usage: gpu_snapshot.per_adapter_usage,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_gpu_report_marks_unavailable_without_metrics() {
        let (availability, available, headroom_pct) = compute_gpu_report(None, None);
        assert_eq!(availability, GpuMemoryAvailability::Unavailable);
        assert_eq!(available, None);
        assert_eq!(headroom_pct, None);
    }

    #[test]
    fn compute_gpu_report_uses_actual_totals() {
        let (availability, available, headroom_pct) = compute_gpu_report(Some(10_000), Some(2_500));
        assert_eq!(availability, GpuMemoryAvailability::Available);
        assert_eq!(available, Some(7_500));
        let headroom_pct = headroom_pct.expect("headroom expected");
        assert!((headroom_pct - 75.0).abs() < 0.001);
    }
}
