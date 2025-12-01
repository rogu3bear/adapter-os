//! Memory Detail Handler
//!
//! Provides detailed memory information including:
//! - UMA memory breakdown (system, GPU, ANE, free)
//! - Adapter memory usage
//! - Auto-eviction configuration and candidates

use crate::permissions::{require_permission, Permission};
use crate::{AppState, Claims, ErrorResponse};
use adapteros_api_types::API_SCHEMA_VERSION;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;

/// UMA memory breakdown response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct UmaMemoryBreakdownResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub total_mb: u64,
    pub system_memory: MemoryRegion,
    pub gpu_memory: MemoryRegion,
    pub ane_memory: MemoryRegion,
    pub free_memory: MemoryRegion,
    pub pressure_level: String,
    pub headroom_pct: f32,
    pub eviction_config: EvictionConfig,
    pub eviction_candidates: Vec<EvictionCandidate>,
    pub timestamp: u64,
    /// Origin node identifier for traceability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_node_id: Option<String>,
}

/// Memory region information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MemoryRegion {
    pub allocated_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub usage_percent: f32,
}

/// Eviction configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct EvictionConfig {
    pub enabled: bool,
    pub min_headroom_pct: f32,
    pub pressure_threshold: f32,
    pub eviction_strategy: String,
}

/// Eviction candidate
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct EvictionCandidate {
    pub adapter_id: String,
    pub size_mb: f32,
    pub state: String,
    pub last_access: String,
    pub activation_rate: f32,
    pub priority_score: f32,
}

/// Adapter memory usage response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterMemoryUsageResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub adapters: Vec<AdapterMemoryInfo>,
    pub total_memory_mb: f32,
    pub timestamp: u64,
    /// Origin node identifier for traceability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_node_id: Option<String>,
}

/// Adapter memory information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct AdapterMemoryInfo {
    pub adapter_id: String,
    pub name: String,
    pub size_mb: f32,
    pub state: String,
    pub location: MemoryLocation,
    pub last_access: String,
    pub access_count: i64,
    pub pinned: bool,
}

/// Memory location
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLocation {
    System,
    GPU,
    ANE,
    Disk,
}

fn schema_version() -> String {
    API_SCHEMA_VERSION.to_string()
}

/// Get UMA memory breakdown
#[utoipa::path(
    tag = "memory",
    get,
    path = "/v1/memory/uma-breakdown",
    responses(
        (status = 200, description = "UMA memory breakdown", body = UmaMemoryBreakdownResponse)
    )
)]
pub async fn get_uma_memory_breakdown(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<UmaMemoryBreakdownResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view memory metrics
    require_permission(&claims, Permission::MetricsView)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Get UMA memory statistics
    let uma_stats = state.uma_monitor.get_uma_stats().await;

    // Get pressure level
    let pressure_level = state.uma_monitor.get_current_pressure().to_string();

    // Get eviction candidates
    let eviction_candidates = get_eviction_candidates(&state).await;

    // Calculate memory regions
    let total_mb = uma_stats.total_mb;
    let used_mb = uma_stats.used_mb;
    let available_mb = uma_stats.available_mb;
    let headroom_pct = uma_stats.headroom_pct;

    // Calculate region breakdown using real ANE metrics where available
    // On macOS with Apple Silicon, ANE metrics are collected from system stats
    // ANE allocation is estimated as ~18% of unified memory architecture
    // ANE usage is estimated from memory compression activity (proxy for ML workload)
    let (ane_allocated, ane_used, ane_available) =
        if let (Some(allocated), Some(used), Some(available)) = (
            uma_stats.ane_allocated_mb,
            uma_stats.ane_used_mb,
            uma_stats.ane_available_mb,
        ) {
            (allocated, used, available)
        } else {
            // Fallback estimation for non-Apple Silicon or when ANE data unavailable
            let ane_allocated = (total_mb as f32 * 0.18) as u64;
            let ane_used = (used_mb as f32 * 0.15) as u64;
            let ane_available = ane_allocated.saturating_sub(ane_used);
            (ane_allocated, ane_used, ane_available)
        };

    // Estimate GPU and system breakdown (remaining after ANE)
    let remaining_mb = total_mb.saturating_sub(ane_allocated);
    let remaining_used = used_mb.saturating_sub(ane_used);

    let gpu_allocated = (remaining_mb as f32 * 0.45) as u64; // 45% of remaining for GPU
    let gpu_used = (remaining_used as f32 * 0.45) as u64;

    let system_allocated = remaining_mb.saturating_sub(gpu_allocated); // Rest for system
    let system_used = remaining_used.saturating_sub(gpu_used);

    Ok(Json(UmaMemoryBreakdownResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        total_mb,
        system_memory: MemoryRegion {
            allocated_mb: system_allocated,
            used_mb: system_used,
            available_mb: system_allocated.saturating_sub(system_used),
            usage_percent: if system_allocated > 0 {
                (system_used as f32 / system_allocated as f32) * 100.0
            } else {
                0.0
            },
        },
        gpu_memory: MemoryRegion {
            allocated_mb: gpu_allocated,
            used_mb: gpu_used,
            available_mb: gpu_allocated.saturating_sub(gpu_used),
            usage_percent: if gpu_allocated > 0 {
                (gpu_used as f32 / gpu_allocated as f32) * 100.0
            } else {
                0.0
            },
        },
        ane_memory: MemoryRegion {
            allocated_mb: ane_allocated,
            used_mb: ane_used,
            available_mb: ane_available,
            usage_percent: uma_stats.ane_usage_percent.unwrap_or_else(|| {
                if ane_allocated > 0 {
                    (ane_used as f32 / ane_allocated as f32) * 100.0
                } else {
                    0.0
                }
            }),
        },
        free_memory: MemoryRegion {
            allocated_mb: total_mb,
            used_mb,
            available_mb,
            usage_percent: (used_mb as f32 / total_mb as f32) * 100.0,
        },
        pressure_level,
        headroom_pct,
        eviction_config: EvictionConfig {
            enabled: true,
            min_headroom_pct: 15.0,
            pressure_threshold: 85.0,
            eviction_strategy: "lru_with_activation_rate".to_string(),
        },
        eviction_candidates,
        timestamp,
        origin_node_id: Some(get_local_node_id()),
    }))
}

/// Get local node identifier
fn get_local_node_id() -> String {
    std::env::var("AOS_NODE_ID")
        .or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .map_err(|_| std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Get adapter memory usage
#[utoipa::path(
    tag = "memory",
    get,
    path = "/v1/memory/adapters",
    responses(
        (status = 200, description = "Adapter memory usage", body = AdapterMemoryUsageResponse)
    )
)]
pub async fn get_adapter_memory_usage(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AdapterMemoryUsageResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view memory metrics
    require_permission(&claims, Permission::MetricsView)?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Get all adapters with memory info
    let adapters = state
        .db
        .get_adapter_memory_info()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch adapter memory info")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut total_memory_mb = 0.0;
    let adapter_infos: Vec<AdapterMemoryInfo> = adapters
        .into_iter()
        .map(|a| {
            // Estimate adapter size based on rank (simplified calculation)
            // Actual size would be: rank * hidden_dim * num_layers * 2 (bytes) / 1MB
            let size_mb = estimate_adapter_size_mb(a.rank);
            total_memory_mb += size_mb;

            let location = match a.current_state.as_str() {
                "hot" | "resident" => MemoryLocation::GPU,
                "warm" => MemoryLocation::System,
                "cold" => MemoryLocation::System,
                _ => MemoryLocation::Disk,
            };

            AdapterMemoryInfo {
                adapter_id: a.adapter_id.clone().unwrap_or_default(),
                name: a.name,
                size_mb,
                state: a.current_state,
                location,
                last_access: a.last_access,
                access_count: a.access_count,
                pinned: a.pinned != 0,
            }
        })
        .collect();

    Ok(Json(AdapterMemoryUsageResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        adapters: adapter_infos,
        total_memory_mb,
        timestamp,
        origin_node_id: Some(get_local_node_id()),
    }))
}

/// Get fallback UMA stats from system metrics
/// Reserved for graceful degradation when ANE metrics are unavailable
#[allow(dead_code)]
fn get_fallback_uma_stats() -> UmaStats {
    use sysinfo::System;

    let sys = System::new_all();
    let total_mb = (sys.total_memory() / 1_048_576) as u64;
    let used_mb = (sys.used_memory() / 1_048_576) as u64;
    let available_mb = total_mb - used_mb;
    let headroom_pct = (available_mb as f32 / total_mb as f32) * 100.0;

    UmaStats {
        total_mb,
        used_mb,
        available_mb,
        headroom_pct,
    }
}

/// UMA statistics
/// Reserved for fallback memory monitoring when ANE metrics are unavailable
#[allow(dead_code)]
struct UmaStats {
    total_mb: u64,
    used_mb: u64,
    available_mb: u64,
    headroom_pct: f32,
}

/// Get eviction candidates
async fn get_eviction_candidates(state: &AppState) -> Vec<EvictionCandidate> {
    let candidates = state
        .db
        .get_eviction_candidates(10)
        .await
        .unwrap_or_default();

    candidates
        .into_iter()
        .map(|c| {
            let size_mb = estimate_adapter_size_mb(c.rank);
            let priority_score = calculate_eviction_priority(c.activation_rate, &c.last_access);

            EvictionCandidate {
                adapter_id: c.adapter_id,
                size_mb,
                state: c.current_state,
                last_access: c.last_access,
                activation_rate: c.activation_rate,
                priority_score,
            }
        })
        .collect()
}

/// Estimate adapter size in MB based on rank
fn estimate_adapter_size_mb(rank: i32) -> f32 {
    // Simplified calculation: rank * hidden_dim * num_layers * 2 bytes
    // Assuming hidden_dim=4096, num_layers=32
    let hidden_dim = 4096.0;
    let num_layers = 32.0;
    let bytes_per_param = 2.0; // FP16

    let size_bytes = rank as f32 * hidden_dim * num_layers * bytes_per_param;
    size_bytes / 1_048_576.0 // Convert to MB
}

/// Calculate eviction priority score (higher = more likely to evict)
fn calculate_eviction_priority(activation_rate: f32, last_access: &str) -> f32 {
    use chrono::{DateTime, Utc};

    // Parse last access time
    let days_since_access = if let Ok(last) = DateTime::parse_from_rfc3339(last_access) {
        let now = Utc::now();
        let duration = now.signed_duration_since(last);
        duration.num_days() as f32
    } else {
        0.0
    };

    // Calculate priority: lower activation rate + longer time since access = higher priority
    (100.0 - activation_rate) + (days_since_access * 10.0)
}

/// Combined memory usage response for frontend dashboard
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CombinedMemoryUsageResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub adapters: Vec<CombinedAdapterMemoryInfo>,
    pub total_memory_mb: f64,
    pub available_memory_mb: f64,
    pub memory_pressure_level: String,
}

/// Adapter memory info for combined response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct CombinedAdapterMemoryInfo {
    pub id: String,
    pub name: String,
    pub memory_usage_mb: f64,
    pub state: String,
    pub pinned: bool,
    pub category: String,
}

/// Get combined memory usage for frontend dashboard
#[utoipa::path(
    get,
    path = "/v1/memory/usage",
    responses(
        (status = 200, description = "Memory usage retrieved successfully", body = CombinedMemoryUsageResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "memory"
)]
pub async fn get_combined_memory_usage(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<CombinedMemoryUsageResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::MetricsView)?;

    // Get system memory info
    use sysinfo::System;
    let sys = System::new_all();
    let total_memory_mb = sys.total_memory() as f64 / 1_048_576.0;
    let available_memory_mb = sys.available_memory() as f64 / 1_048_576.0;
    let used_pct = ((total_memory_mb - available_memory_mb) / total_memory_mb) * 100.0;

    let memory_pressure_level = if used_pct > 90.0 {
        "critical"
    } else if used_pct > 80.0 {
        "high"
    } else if used_pct > 60.0 {
        "medium"
    } else {
        "low"
    };

    // Get adapter memory info
    let adapters = state
        .db
        .get_adapter_memory_info()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to fetch adapter memory info")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let adapter_infos: Vec<CombinedAdapterMemoryInfo> = adapters
        .into_iter()
        .map(|a| {
            let size_mb = estimate_adapter_size_mb(a.rank) as f64;
            CombinedAdapterMemoryInfo {
                id: a.adapter_id.clone().unwrap_or_default(),
                name: a.name,
                memory_usage_mb: size_mb,
                state: a.current_state,
                pinned: a.pinned != 0,
                category: a.category.unwrap_or_else(|| "code".to_string()),
            }
        })
        .collect();

    Ok(Json(CombinedMemoryUsageResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        adapters: adapter_infos,
        total_memory_mb,
        available_memory_mb,
        memory_pressure_level: memory_pressure_level.to_string(),
    }))
}
