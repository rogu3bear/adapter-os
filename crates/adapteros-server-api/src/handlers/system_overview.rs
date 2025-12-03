//! System Overview Handler
//!
//! Provides comprehensive system overview including:
//! - Uptime, load averages, process count
//! - Service status for all critical components
//! - System resource usage
//! - Active sessions and workers

use crate::permissions::{require_permission, Permission};
use crate::{AppState, Claims, ErrorResponse};
use adapteros_api_types::API_SCHEMA_VERSION;
use adapteros_system_metrics::SystemMetricsCollector;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;

/// System overview response with complete system state
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct SystemOverviewResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub load_average: LoadAverageInfo,
    pub resource_usage: ResourceUsageInfo,
    pub services: Vec<ServiceStatus>,
    pub active_sessions: i32,
    pub active_workers: i32,
    pub adapter_count: i32,
    pub timestamp: u64,
    /// Origin node identifier for traceability
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_node_id: Option<String>,
}

/// Load average information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct LoadAverageInfo {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// Resource usage information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ResourceUsageInfo {
    pub cpu_usage_percent: f32,
    pub memory_usage_percent: f32,
    pub disk_usage_percent: f32,
    pub network_rx_mbps: f32,
    pub network_tx_mbps: f32,
    pub gpu_utilization_percent: Option<f32>,
    pub gpu_memory_used_gb: Option<f32>,
    pub gpu_memory_total_gb: Option<f32>,
}

/// Service status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ServiceStatus {
    pub name: String,
    pub status: ServiceHealthStatus,
    pub message: Option<String>,
    pub last_check: u64,
}

/// Service health status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceHealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

fn schema_version() -> String {
    API_SCHEMA_VERSION.to_string()
}

/// Get system overview
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/system/overview",
    responses(
        (status = 200, description = "System overview", body = SystemOverviewResponse)
    )
)]
pub async fn get_system_overview(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SystemOverviewResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view system overview
    require_permission(&claims, Permission::MetricsView)?;

    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Count active sessions, workers, and adapters using Db trait methods
    let active_sessions = state.db.count_active_chat_sessions().await.unwrap_or(0) as i32;

    let active_workers = state.db.count_active_workers().await.unwrap_or(0) as i32;

    let adapter_count = state.db.count_active_adapters().await.unwrap_or(0) as i32;

    // Check service health
    let services = check_service_health(&state).await;

    Ok(Json(SystemOverviewResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        uptime_seconds: collector.uptime_seconds(),
        process_count: collector.process_count(),
        load_average: LoadAverageInfo {
            load_1min: load_avg.0,
            load_5min: load_avg.1,
            load_15min: load_avg.2,
        },
        resource_usage: ResourceUsageInfo {
            cpu_usage_percent: metrics.cpu_usage as f32,
            memory_usage_percent: metrics.memory_usage as f32,
            disk_usage_percent: metrics.disk_io.usage_percent,
            network_rx_mbps: (metrics.network_io.rx_bytes as f32 * 8.0) / 1_000_000.0,
            network_tx_mbps: (metrics.network_io.tx_bytes as f32 * 8.0) / 1_000_000.0,
            gpu_utilization_percent: metrics.gpu_metrics.utilization.map(|v| v as f32),
            gpu_memory_used_gb: metrics
                .gpu_metrics
                .memory_used
                .map(|v| v as f32 / 1_073_741_824.0),
            gpu_memory_total_gb: metrics
                .gpu_metrics
                .memory_total
                .map(|v| v as f32 / 1_073_741_824.0),
        },
        services,
        active_sessions,
        active_workers,
        adapter_count,
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

/// Check health of all critical services
///
/// Runs health checks in parallel with per-check timeouts to reduce latency.
/// Sequential checks took 300-600ms; parallel execution reduces this to ~100ms.
pub async fn check_service_health(state: &AppState) -> Vec<ServiceStatus> {
    use std::time::Duration;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let timeout_duration = Duration::from_secs(5);

    // Helper to create a timeout status
    let timeout_status = || {
        (
            ServiceHealthStatus::Unknown,
            Some("Health check timed out".to_string()),
        )
    };

    // Run all health checks in parallel with 5-second timeout each
    let (db_result, lifecycle_result, telemetry_result, mlx_result, router_result) = tokio::join!(
        tokio::time::timeout(timeout_duration, check_database_health(state)),
        tokio::time::timeout(timeout_duration, check_lifecycle_manager_health(state)),
        tokio::time::timeout(timeout_duration, check_telemetry_health(state)),
        tokio::time::timeout(timeout_duration, check_mlx_backend_health()),
        tokio::time::timeout(timeout_duration, check_router_health(state)),
    );

    // Unwrap results with timeout fallback
    let db_status = db_result.unwrap_or_else(|_| timeout_status());
    let lifecycle_status = lifecycle_result.unwrap_or_else(|_| timeout_status());
    let telemetry_status = telemetry_result.unwrap_or_else(|_| timeout_status());
    let mlx_status = mlx_result.unwrap_or_else(|_| timeout_status());
    let router_status = router_result.unwrap_or_else(|_| timeout_status());

    vec![
        ServiceStatus {
            name: "database".to_string(),
            status: db_status.0,
            message: db_status.1,
            last_check: timestamp,
        },
        ServiceStatus {
            name: "api_server".to_string(),
            status: ServiceHealthStatus::Healthy,
            message: Some("API server is responding".to_string()),
            last_check: timestamp,
        },
        ServiceStatus {
            name: "lifecycle_manager".to_string(),
            status: lifecycle_status.0,
            message: lifecycle_status.1,
            last_check: timestamp,
        },
        ServiceStatus {
            name: "telemetry".to_string(),
            status: telemetry_status.0,
            message: telemetry_status.1,
            last_check: timestamp,
        },
        ServiceStatus {
            name: "mlx_backend".to_string(),
            status: mlx_status.0,
            message: mlx_status.1,
            last_check: timestamp,
        },
        ServiceStatus {
            name: "router".to_string(),
            status: router_status.0,
            message: router_status.1,
            last_check: timestamp,
        },
    ]
}

/// Check database health
async fn check_database_health(state: &AppState) -> (ServiceHealthStatus, Option<String>) {
    match state.db.check_database_health().await {
        Ok(_) => (
            ServiceHealthStatus::Healthy,
            Some("Database is responding".to_string()),
        ),
        Err(e) => (
            ServiceHealthStatus::Unhealthy,
            Some(format!("Database error: {}", e)),
        ),
    }
}

/// Check lifecycle manager health
async fn check_lifecycle_manager_health(state: &AppState) -> (ServiceHealthStatus, Option<String>) {
    if let Some(ref lifecycle) = state.lifecycle_manager {
        // Check if lifecycle manager is operational by checking lock
        match lifecycle.try_lock() {
            Ok(_) => (
                ServiceHealthStatus::Healthy,
                Some("Lifecycle manager is operational".to_string()),
            ),
            Err(_) => (
                ServiceHealthStatus::Degraded,
                Some("Lifecycle manager is busy".to_string()),
            ),
        }
    } else {
        (
            ServiceHealthStatus::Unknown,
            Some("Lifecycle manager not configured".to_string()),
        )
    }
}

/// Check telemetry health
///
/// First verifies the telemetry_events table exists before querying.
/// This prevents silent failures when the table hasn't been created yet.
async fn check_telemetry_health(state: &AppState) -> (ServiceHealthStatus, Option<String>) {
    // First check if the telemetry_events table exists using Db method
    let table_exists = state
        .db
        .table_exists("telemetry_events")
        .await
        .unwrap_or(false);

    if !table_exists {
        return (
            ServiceHealthStatus::Unknown,
            Some("Telemetry not configured (table missing)".to_string()),
        );
    }

    // Check if telemetry events are being written
    match state.db.count_table_rows("telemetry_events").await {
        Ok(count) if count > 0 => (
            ServiceHealthStatus::Healthy,
            Some(format!("Telemetry active ({} events)", count)),
        ),
        Ok(_) => (
            ServiceHealthStatus::Degraded,
            Some("No telemetry events".to_string()),
        ),
        Err(_) => (
            ServiceHealthStatus::Unknown,
            Some("Telemetry status unknown".to_string()),
        ),
    }
}

/// Check MLX backend health
async fn check_mlx_backend_health() -> (ServiceHealthStatus, Option<String>) {
    // Check if MLX model is configured
    if std::env::var("AOS_MLX_FFI_MODEL").is_ok() {
        (
            ServiceHealthStatus::Healthy,
            Some("MLX backend configured".to_string()),
        )
    } else {
        (
            ServiceHealthStatus::Unknown,
            Some("MLX backend not configured".to_string()),
        )
    }
}

/// Check router health
///
/// First verifies the routing_decisions table exists before querying.
/// This prevents silent failures when the table hasn't been created yet.
async fn check_router_health(state: &AppState) -> (ServiceHealthStatus, Option<String>) {
    // First check if the routing_decisions table exists using Db method
    let table_exists = state
        .db
        .table_exists("routing_decisions")
        .await
        .unwrap_or(false);

    if !table_exists {
        return (
            ServiceHealthStatus::Unknown,
            Some("Router not configured (table missing)".to_string()),
        );
    }

    // Check if router has made recent decisions
    match state.db.count_table_rows("routing_decisions").await {
        Ok(count) if count > 0 => (
            ServiceHealthStatus::Healthy,
            Some(format!("Router active ({} decisions)", count)),
        ),
        Ok(_) => (
            ServiceHealthStatus::Degraded,
            Some("No recent routing decisions".to_string()),
        ),
        Err(_) => (
            ServiceHealthStatus::Unknown,
            Some("Router status unknown".to_string()),
        ),
    }
}
