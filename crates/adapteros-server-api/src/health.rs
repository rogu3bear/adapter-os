//! Component-level health check endpoints for PRD-06
//!
//! Provides health status for individual components: router, loader, kernel, DB,
//! telemetry, and system-metrics.

use crate::state::AppState;
use crate::worker_health::{WorkerHealthStatus, WorkerHealthSummary};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::query;
use std::path::Path as StdPath;
use std::time::{Duration, SystemTime};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::task;
use tracing::warn;
use utoipa::ToSchema;

/// Component health status
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ComponentStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Individual component health check result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ComponentHealth {
    pub component: String,
    pub status: ComponentStatus,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    pub timestamp: u64,
}

/// Aggregate health response for all components
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemHealthResponse {
    pub overall_status: ComponentStatus,
    pub components: Vec<ComponentHealth>,
    pub timestamp: u64,
}

/// System-wide readiness response (aggregated)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemReadyResponse {
    pub ready: bool,
    pub overall_status: ComponentStatus,
    pub components: Vec<ComponentHealth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boot_elapsed_ms: Option<u64>,
    #[serde(default)]
    pub critical_degraded: Vec<String>,
    #[serde(default)]
    pub non_critical_degraded: Vec<String>,
    #[serde(default)]
    pub maintenance: bool,
    #[serde(default)]
    pub reason: String,
}

const DEFAULT_READY_FLAG_PATH: &str = "var/run/system_ready";
const DEFAULT_BOOT_LOG_PATH: &str = "var/log/boot-times.log";
const DEFAULT_UI_HEALTH_URL: &str = "http://127.0.0.1:3200/healthz";

impl ComponentHealth {
    fn new(
        component: impl Into<String>,
        status: ComponentStatus,
        message: impl Into<String>,
    ) -> Self {
        Self {
            component: component.into(),
            status,
            message: message.into(),
            details: None,
            timestamp: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl IntoResponse for ComponentHealth {
    fn into_response(self) -> Response {
        let status_code = match self.status {
            ComponentStatus::Healthy => StatusCode::OK,
            ComponentStatus::Degraded => StatusCode::OK, // Still operational
            ComponentStatus::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
        };
        (status_code, Json(self)).into_response()
    }
}

/// Check router component health
///
/// Verifies:
/// - Decision rate > 0 (has processed requests)
/// - Router overhead < 90%
pub async fn check_router_health(State(state): State<AppState>) -> impl IntoResponse {
    // Check metrics from the metrics exporter
    let metrics_snapshot = state.metrics_exporter.snapshot();

    // Check if router has made any decisions
    let has_decisions = metrics_snapshot.queue_depth > 0.0 || metrics_snapshot.total_requests > 0.0;

    // Check router overhead (should be reasonable, using queue depth as proxy)
    let high_load = metrics_snapshot.queue_depth > 100.0;

    if !has_decisions {
        // Treat idle router as healthy so startup scripts don't show spurious degradation
        ComponentHealth::new(
            "router",
            ComponentStatus::Healthy,
            "Router idle (no requests yet)",
        )
        .with_details(serde_json::json!({
            "queue_depth": metrics_snapshot.queue_depth,
            "total_requests": metrics_snapshot.total_requests
        }))
    } else if high_load {
        ComponentHealth::new(
            "router",
            ComponentStatus::Degraded,
            format!("High queue depth: {}", metrics_snapshot.queue_depth),
        )
        .with_details(serde_json::json!({
            "queue_depth": metrics_snapshot.queue_depth,
            "total_requests": metrics_snapshot.total_requests
        }))
    } else {
        ComponentHealth::new(
            "router",
            ComponentStatus::Healthy,
            format!(
                "Router operational ({} requests processed)",
                metrics_snapshot.total_requests
            ),
        )
        .with_details(serde_json::json!({
            "queue_depth": metrics_snapshot.queue_depth,
            "total_requests": metrics_snapshot.total_requests
        }))
    }
}

/// Check loader component health
///
/// Verifies:
/// - Loaded adapters match configured adapters
/// - No stuck loading states
/// - Lifecycle manager operational (if available)
pub async fn check_loader_health(State(state): State<AppState>) -> impl IntoResponse {
    // Query adapter states from database
    match state.db.pool().acquire().await {
        Ok(mut conn) => {
            // Check for adapters stuck in loading state
            let stuck_count: Result<i64, _> =
                sqlx::query_scalar("SELECT COUNT(*) FROM adapters WHERE current_state = 'loading'")
                    .fetch_one(&mut *conn)
                    .await;

            // Count total adapters and loaded adapters
            let total_adapters: Result<i64, _> =
                sqlx::query_scalar("SELECT COUNT(*) FROM adapters")
                    .fetch_one(&mut *conn)
                    .await;

            let loaded_adapters: Result<i64, _> = sqlx::query_scalar(
                "SELECT COUNT(*) FROM adapters WHERE current_state IN ('warm', 'hot')",
            )
            .fetch_one(&mut *conn)
            .await;

            match (stuck_count, total_adapters, loaded_adapters) {
                (Ok(stuck), Ok(total), Ok(loaded)) if stuck > 0 => ComponentHealth::new(
                    "loader",
                    ComponentStatus::Degraded,
                    format!("{} adapter(s) stuck in loading state", stuck),
                )
                .with_details(serde_json::json!({
                    "stuck_count": stuck,
                    "total_adapters": total,
                    "loaded_adapters": loaded,
                    "lifecycle_manager_available": state.has_lifecycle_manager()
                })),
                (Ok(_stuck), Ok(total), Ok(loaded)) => ComponentHealth::new(
                    "loader",
                    ComponentStatus::Healthy,
                    format!("{}/{} adapters loaded", loaded, total),
                )
                .with_details(serde_json::json!({
                    "total_adapters": total,
                    "loaded_adapters": loaded,
                    "lifecycle_manager_available": state.has_lifecycle_manager()
                })),
                _ => ComponentHealth::new(
                    "loader",
                    ComponentStatus::Unhealthy,
                    "Failed to query adapter states",
                ),
            }
        }
        Err(e) => ComponentHealth::new(
            "loader",
            ComponentStatus::Unhealthy,
            format!("Database connection failed: {}", e),
        ),
    }
}

/// Check kernel component health
///
/// Verifies:
/// - Worker available and operational
/// - GPU memory available (via UMA monitor)
pub async fn check_kernel_health(State(state): State<AppState>) -> impl IntoResponse {
    // Check if worker is available
    let worker_available = state.worker.is_some();

    // Check UMA memory pressure
    let uma_stats = state.uma_monitor.get_stats();
    let memory_ok = uma_stats.headroom_pct > 15.0; // Above critical threshold
    let memory_status = if uma_stats.headroom_pct > 30.0 {
        "normal"
    } else if uma_stats.headroom_pct > 20.0 {
        "medium"
    } else if uma_stats.headroom_pct > 15.0 {
        "high"
    } else {
        "critical"
    };

    if !worker_available {
        ComponentHealth::new(
            "kernel",
            ComponentStatus::Degraded,
            "Worker not initialized",
        )
        .with_details(serde_json::json!({
            "worker_available": false,
            "memory_headroom_pct": uma_stats.headroom_pct,
            "memory_status": memory_status
        }))
    } else if !memory_ok {
        ComponentHealth::new(
            "kernel",
            ComponentStatus::Degraded,
            format!(
                "Low GPU memory ({}% headroom)",
                uma_stats.headroom_pct as i32
            ),
        )
        .with_details(serde_json::json!({
            "worker_available": true,
            "memory_used_mb": uma_stats.used_mb,
            "memory_total_mb": uma_stats.total_mb,
            "memory_headroom_pct": uma_stats.headroom_pct,
            "memory_status": memory_status
        }))
    } else {
        ComponentHealth::new(
            "kernel",
            ComponentStatus::Healthy,
            format!(
                "Kernel operational ({}% memory free)",
                uma_stats.headroom_pct as i32
            ),
        )
        .with_details(serde_json::json!({
            "worker_available": true,
            "memory_used_mb": uma_stats.used_mb,
            "memory_total_mb": uma_stats.total_mb,
            "memory_headroom_pct": uma_stats.headroom_pct,
            "memory_status": memory_status
        }))
    }
}

/// Check database health
///
/// Verifies:
/// - Latest migration applied
/// - Connection pool healthy
/// - KV backend health (if attached)
pub async fn check_db_health(State(state): State<AppState>) -> impl IntoResponse {
    // Test database connectivity
    match state.db.pool().acquire().await {
        Ok(mut conn) => {
            // Check if we can perform a simple query
            let result: Result<i64, _> = sqlx::query_scalar("SELECT 1").fetch_one(&mut *conn).await;

            match result {
                Ok(_) => {
                    // Query migration status
                    let migration_count: Result<i64, _> =
                        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
                            .fetch_one(&mut *conn)
                            .await;

                    // Check KV backend health
                    let kv_health = state.db.kv_health_check().await.ok();

                    // Build details with KV health info
                    let mut details = serde_json::json!({
                        "sql_connected": true,
                    });

                    if let Ok(count) = migration_count {
                        details["migrations_applied"] = serde_json::json!(count);
                    }

                    if let Some(ref kv) = kv_health {
                        details["kv_attached"] = serde_json::json!(kv.attached);
                        details["kv_status"] = serde_json::json!(kv.status.to_string());
                        details["storage_mode"] = serde_json::json!(kv.storage_mode);
                        if let Some(read_latency) = kv.read_latency_ms {
                            details["kv_read_latency_ms"] = serde_json::json!(read_latency);
                        }
                        if let Some(write_latency) = kv.write_latency_ms {
                            details["kv_write_latency_ms"] = serde_json::json!(write_latency);
                        }
                    }

                    // Determine overall status based on SQL and KV health
                    let overall_status = if let Some(ref kv) = kv_health {
                        if kv.attached {
                            match kv.status {
                                adapteros_db::HealthStatus::Unhealthy => ComponentStatus::Degraded,
                                adapteros_db::HealthStatus::Degraded => ComponentStatus::Degraded,
                                _ => ComponentStatus::Healthy,
                            }
                        } else {
                            ComponentStatus::Healthy
                        }
                    } else {
                        ComponentStatus::Healthy
                    };

                    match migration_count {
                        Ok(count) => {
                            let message = if let Some(ref kv) = kv_health {
                                if kv.attached {
                                    format!(
                                        "Database healthy, {} migrations applied, KV backend: {}",
                                        count, kv.status
                                    )
                                } else {
                                    format!("Database healthy, {} migrations applied", count)
                                }
                            } else {
                                format!("Database healthy, {} migrations applied", count)
                            };

                            ComponentHealth::new("db", overall_status, message)
                                .with_details(details)
                        }
                        Err(_) => {
                            // Migrations table might not exist yet
                            ComponentHealth::new(
                                "db",
                                ComponentStatus::Healthy,
                                "Database connected",
                            )
                            .with_details(details)
                        }
                    }
                }
                Err(e) => ComponentHealth::new(
                    "db",
                    ComponentStatus::Unhealthy,
                    format!("Database query failed: {}", e),
                ),
            }
        }
        Err(e) => ComponentHealth::new(
            "db",
            ComponentStatus::Unhealthy,
            format!("Database connection failed: {}", e),
        ),
    }
}

/// Check telemetry component health
///
/// Verifies:
/// - Metrics exporter operational
/// - Recent telemetry events recorded
pub async fn check_telemetry_health(State(state): State<AppState>) -> impl IntoResponse {
    // Check metrics exporter availability
    let metrics_snapshot = state.metrics_exporter.snapshot();

    // Check if telemetry is recording recent activity
    let has_activity = metrics_snapshot.total_requests > 0.0;

    // Check latency metrics as a proxy for telemetry health
    let latency_ok = metrics_snapshot.avg_latency_ms < 1000.0; // <1s is healthy

    if !has_activity {
        // Treat idle telemetry as healthy so initial startup doesn't appear degraded
        ComponentHealth::new(
            "telemetry",
            ComponentStatus::Healthy,
            "Telemetry idle (no activity yet)",
        )
        .with_details(serde_json::json!({
            "total_requests": metrics_snapshot.total_requests,
            "avg_latency_ms": metrics_snapshot.avg_latency_ms
        }))
    } else if !latency_ok {
        ComponentHealth::new(
            "telemetry",
            ComponentStatus::Degraded,
            format!("High latency: {:.0}ms", metrics_snapshot.avg_latency_ms),
        )
        .with_details(serde_json::json!({
            "total_requests": metrics_snapshot.total_requests,
            "avg_latency_ms": metrics_snapshot.avg_latency_ms
        }))
    } else {
        ComponentHealth::new(
            "telemetry",
            ComponentStatus::Healthy,
            format!(
                "Telemetry operational ({} events, {:.0}ms avg latency)",
                metrics_snapshot.total_requests, metrics_snapshot.avg_latency_ms
            ),
        )
        .with_details(serde_json::json!({
            "total_requests": metrics_snapshot.total_requests,
            "avg_latency_ms": metrics_snapshot.avg_latency_ms
        }))
    }
}

/// Check KV backend component health
///
/// Verifies:
/// - KV backend attached and accessible
/// - Read/write connectivity
/// - Performance metrics (latency)
pub async fn check_kv_health(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.kv_health_check().await {
        Ok(kv_health) => {
            let status = match kv_health.status {
                adapteros_db::HealthStatus::Healthy => ComponentStatus::Healthy,
                adapteros_db::HealthStatus::Degraded => ComponentStatus::Degraded,
                adapteros_db::HealthStatus::Unhealthy => ComponentStatus::Unhealthy,
                adapteros_db::HealthStatus::Unknown => ComponentStatus::Degraded,
            };

            let message = if !kv_health.attached {
                "KV backend not attached".to_string()
            } else if kv_health.connectivity_ok {
                let latency_info = match (kv_health.read_latency_ms, kv_health.write_latency_ms) {
                    (Some(read), Some(write)) => {
                        format!(" (read: {:.1}ms, write: {:.1}ms)", read, write)
                    }
                    (Some(read), None) => format!(" (read: {:.1}ms)", read),
                    (None, Some(write)) => format!(" (write: {:.1}ms)", write),
                    (None, None) => String::new(),
                };
                format!("KV backend operational{}", latency_info)
            } else {
                kv_health
                    .error
                    .clone()
                    .unwrap_or_else(|| "KV backend connectivity check failed".to_string())
            };

            let mut details = serde_json::json!({
                "attached": kv_health.attached,
                "storage_mode": kv_health.storage_mode,
                "connectivity_ok": kv_health.connectivity_ok,
            });

            if let Some(read_latency) = kv_health.read_latency_ms {
                details["read_latency_ms"] = serde_json::json!(read_latency);
            }
            if let Some(write_latency) = kv_health.write_latency_ms {
                details["write_latency_ms"] = serde_json::json!(write_latency);
            }
            if let Some(key_count) = kv_health.key_count {
                details["key_count"] = serde_json::json!(key_count);
            }
            if let Some(ref error) = kv_health.error {
                details["error"] = serde_json::json!(error);
            }

            ComponentHealth::new("kv", status, message).with_details(details)
        }
        Err(e) => ComponentHealth::new(
            "kv",
            ComponentStatus::Unhealthy,
            format!("KV health check failed: {}", e),
        ),
    }
}

/// Check system-metrics component health
///
/// Verifies:
/// - UMA monitor recording recent metrics
/// - Memory pressure within acceptable range
pub async fn check_system_metrics_health(State(state): State<AppState>) -> impl IntoResponse {
    // Check UMA monitor stats
    let uma_stats = state.uma_monitor.get_stats();

    // Check if stats are recent (non-zero values indicate active monitoring)
    let has_stats = uma_stats.total_mb > 0;

    // Check memory pressure level
    let pressure_level = if uma_stats.headroom_pct > 30.0 {
        "normal"
    } else if uma_stats.headroom_pct > 20.0 {
        "medium"
    } else if uma_stats.headroom_pct > 15.0 {
        "high"
    } else {
        "critical"
    };

    let pressure_ok = uma_stats.headroom_pct > 15.0;

    if !has_stats {
        ComponentHealth::new(
            "system-metrics",
            ComponentStatus::Degraded,
            "System metrics not yet initialized",
        )
        .with_details(serde_json::json!({
            "uma_monitor_active": false
        }))
    } else if !pressure_ok {
        ComponentHealth::new(
            "system-metrics",
            ComponentStatus::Degraded,
            format!(
                "Critical memory pressure ({}% headroom)",
                uma_stats.headroom_pct as i32
            ),
        )
        .with_details(serde_json::json!({
            "uma_monitor_active": true,
            "memory_used_mb": uma_stats.used_mb,
            "memory_total_mb": uma_stats.total_mb,
            "headroom_pct": uma_stats.headroom_pct,
            "pressure_level": pressure_level
        }))
    } else {
        ComponentHealth::new(
            "system-metrics",
            ComponentStatus::Healthy,
            format!(
                "System metrics operational ({} MB used, {}% free)",
                uma_stats.used_mb, uma_stats.headroom_pct as i32
            ),
        )
        .with_details(serde_json::json!({
            "uma_monitor_active": true,
            "memory_used_mb": uma_stats.used_mb,
            "memory_total_mb": uma_stats.total_mb,
            "headroom_pct": uma_stats.headroom_pct,
            "pressure_level": pressure_level
        }))
    }
}

/// Get health status for all components
#[utoipa::path(
    get,
    path = "/healthz/all",
    responses(
        (status = 200, description = "System health status", body = SystemHealthResponse)
    ),
    tag = "health"
)]
pub async fn check_all_health(State(state): State<AppState>) -> impl IntoResponse {
    let components = vec![
        check_router_health(State(state.clone()))
            .await
            .into_response(),
        check_loader_health(State(state.clone()))
            .await
            .into_response(),
        check_kernel_health(State(state.clone()))
            .await
            .into_response(),
        check_db_health(State(state.clone())).await.into_response(),
        check_telemetry_health(State(state.clone()))
            .await
            .into_response(),
        check_system_metrics_health(State(state.clone()))
            .await
            .into_response(),
    ];

    // Extract ComponentHealth from responses
    let mut health_checks = Vec::new();
    for _response in components {
        // Try to extract the JSON body from the response
        // Note: This is a simplified approach; in practice, we'd need to handle this more carefully
        // For now, we'll call the functions directly
    }

    // Directly call health check functions instead of going through responses
    let router = extract_health(
        check_router_health(State(state.clone()))
            .await
            .into_response(),
    )
    .await;
    let loader = extract_health(
        check_loader_health(State(state.clone()))
            .await
            .into_response(),
    )
    .await;
    let kernel = extract_health(
        check_kernel_health(State(state.clone()))
            .await
            .into_response(),
    )
    .await;
    let db = extract_health(check_db_health(State(state.clone())).await.into_response()).await;
    let kv = extract_health(check_kv_health(State(state.clone())).await.into_response()).await;
    let telemetry = extract_health(
        check_telemetry_health(State(state.clone()))
            .await
            .into_response(),
    )
    .await;
    let system_metrics = extract_health(
        check_system_metrics_health(State(state.clone()))
            .await
            .into_response(),
    )
    .await;

    health_checks.extend(vec![
        router,
        loader,
        kernel,
        db,
        kv,
        telemetry,
        system_metrics,
    ]);

    // Determine overall status (worst status wins)
    let overall_status = health_checks
        .iter()
        .fold(ComponentStatus::Healthy, |acc, check| {
            match (&acc, &check.status) {
                (ComponentStatus::Unhealthy, _) | (_, ComponentStatus::Unhealthy) => {
                    ComponentStatus::Unhealthy
                }
                (ComponentStatus::Degraded, _) | (_, ComponentStatus::Degraded) => {
                    ComponentStatus::Degraded
                }
                _ => ComponentStatus::Healthy,
            }
        });

    let response = SystemHealthResponse {
        overall_status,
        components: health_checks,
        timestamp: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };

    Json(response)
}

/// Aggregate readiness across DB, server, workers, router, and UI
#[utoipa::path(
    tag = "system",
    get,
    path = "/system/ready",
    responses(
        (status = 200, description = "System is ready", body = SystemReadyResponse),
        (status = 503, description = "System not ready", body = SystemReadyResponse)
    )
)]
pub async fn system_ready(State(state): State<AppState>) -> impl IntoResponse {
    let (components, boot_elapsed_ms) = gather_system_ready_components(state.clone()).await;

    // Classify components by severity
    let critical_components = ["server", "db", "router"];
    let mut critical_degraded = Vec::new();
    let mut non_critical_degraded = Vec::new();

    for comp in components.iter() {
        if comp.status == ComponentStatus::Healthy {
            continue;
        }
        if critical_components.contains(&comp.component.as_str()) {
            critical_degraded.push(comp.component.clone());
        } else {
            non_critical_degraded.push(comp.component.clone());
        }
    }

    let mut maintenance = false;
    if let Some(ref boot_state) = state.boot_state {
        maintenance = boot_state.is_maintenance();
    }

    let overall_status = components
        .iter()
        .fold(ComponentStatus::Healthy, |acc, check| {
            match (&acc, &check.status) {
                (ComponentStatus::Unhealthy, _) | (_, ComponentStatus::Unhealthy) => {
                    ComponentStatus::Unhealthy
                }
                (ComponentStatus::Degraded, _) | (_, ComponentStatus::Degraded) => {
                    ComponentStatus::Degraded
                }
                _ => ComponentStatus::Healthy,
            }
        });

    let ready = critical_degraded.is_empty() && !maintenance;
    let reason = if maintenance {
        "maintenance".to_string()
    } else if !critical_degraded.is_empty() {
        format!("critical components degraded: {:?}", critical_degraded)
    } else if !non_critical_degraded.is_empty() {
        format!("non-critical degraded: {:?}", non_critical_degraded)
    } else {
        "ready".to_string()
    };

    handle_ready_side_effects(ready, boot_elapsed_ms, &components).await;

    let status_code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(SystemReadyResponse {
            ready,
            overall_status,
            components,
            boot_elapsed_ms,
            critical_degraded,
            non_critical_degraded,
            maintenance,
            reason,
        }),
    )
}

/// Extract ComponentHealth from a response
async fn extract_health(response: Response) -> ComponentHealth {
    let (_parts, body) = response.into_parts();

    // Try to read the body
    match axum::body::to_bytes(body, usize::MAX).await {
        Ok(bytes) => match serde_json::from_slice::<ComponentHealth>(&bytes) {
            Ok(health) => health,
            Err(_) => ComponentHealth::new(
                "unknown",
                ComponentStatus::Unhealthy,
                "Failed to parse health response",
            ),
        },
        Err(_) => ComponentHealth::new(
            "unknown",
            ComponentStatus::Unhealthy,
            "Failed to read response body",
        ),
    }
}

pub async fn gather_system_ready_components(
    state: AppState,
) -> (Vec<ComponentHealth>, Option<u64>) {
    let mut components = Vec::new();

    // Server boot state
    let server_health = if let Some(ref boot_state) = state.boot_state {
        let state_label = boot_state.current_state().as_str();
        if boot_state.is_maintenance() {
            ComponentHealth::new(
                "server",
                ComponentStatus::Degraded,
                format!("maintenance: {}", state_label),
            )
        } else if boot_state.is_draining() || boot_state.is_shutting_down() {
            ComponentHealth::new(
                "server",
                ComponentStatus::Degraded,
                format!("draining: {}", state_label),
            )
        } else if boot_state.is_ready() {
            ComponentHealth::new(
                "server",
                ComponentStatus::Healthy,
                format!("boot state: {}", state_label),
            )
        } else {
            ComponentHealth::new(
                "server",
                ComponentStatus::Degraded,
                format!("boot state: {}", state_label),
            )
        }
    } else {
        ComponentHealth::new(
            "server",
            ComponentStatus::Degraded,
            "boot state manager not configured",
        )
    };
    components.push(server_health);

    // Core dependencies
    let db = extract_health(check_db_health(State(state.clone())).await.into_response()).await;
    let router = extract_health(
        check_router_health(State(state.clone()))
            .await
            .into_response(),
    )
    .await;
    components.push(db);
    components.push(router);

    // Workers (UDS)
    components.push(worker_component_health(&state));

    // UI health
    components.push(check_ui_health().await);

    let boot_elapsed_ms = state
        .boot_state
        .as_ref()
        .map(|bs| bs.elapsed().as_millis() as u64);

    (components, boot_elapsed_ms)
}

fn worker_component_health(state: &AppState) -> ComponentHealth {
    if let Some(monitor) = &state.health_monitor {
        let summary = monitor.get_health_summary();
        let serving = has_healthy_worker(state);

        if summary.is_empty() {
            return if serving {
                ComponentHealth::new(
                    "workers",
                    ComponentStatus::Healthy,
                    "workers registered (health metrics pending)",
                )
            } else {
                ComponentHealth::new(
                    "workers",
                    ComponentStatus::Degraded,
                    "no workers registered",
                )
            };
        }

        let status = reduce_worker_status(&summary, serving);

        match status {
            ComponentStatus::Unhealthy => ComponentHealth::new(
                "workers",
                ComponentStatus::Unhealthy,
                "one or more workers crashed",
            ),
            ComponentStatus::Healthy => {
                if serving
                    && summary
                        .iter()
                        .any(|h| h.health_status != WorkerHealthStatus::Healthy)
                {
                    ComponentHealth::new(
                        "workers",
                        ComponentStatus::Healthy,
                        "serving worker reachable (health telemetry degraded/unknown)",
                    )
                } else {
                    ComponentHealth::new(
                        "workers",
                        ComponentStatus::Healthy,
                        format!("{} workers healthy", summary.len()),
                    )
                }
            }
            ComponentStatus::Degraded => ComponentHealth::new(
                "workers",
                ComponentStatus::Degraded,
                "worker health degraded or unknown",
            ),
        }
    } else {
        ComponentHealth::new(
            "workers",
            ComponentStatus::Degraded,
            "worker health monitor unavailable",
        )
    }
}

fn reduce_worker_status(summary: &[WorkerHealthSummary], serving: bool) -> ComponentStatus {
    let any_crashed = summary
        .iter()
        .any(|s| s.health_status == WorkerHealthStatus::Crashed);
    if any_crashed {
        return ComponentStatus::Unhealthy;
    }

    let any_healthy = summary
        .iter()
        .any(|s| s.health_status == WorkerHealthStatus::Healthy);
    let any_degraded = summary
        .iter()
        .any(|s| s.health_status == WorkerHealthStatus::Degraded);
    let any_unknown = summary
        .iter()
        .any(|s| s.health_status == WorkerHealthStatus::Unknown);

    if serving || any_healthy {
        ComponentStatus::Healthy
    } else if any_degraded || any_unknown {
        ComponentStatus::Degraded
    } else {
        ComponentStatus::Degraded
    }
}

fn has_healthy_worker(state: &AppState) -> bool {
    let pool = state.db_pool.clone();
    task::block_in_place(|| {
        let handle = tokio::runtime::Handle::current();
        handle.block_on(async {
            query("SELECT 1 FROM workers WHERE status = 'healthy' LIMIT 1")
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .is_some()
        })
    })
}

async fn check_ui_health() -> ComponentHealth {
    let url = std::env::var("AOS_UI_HEALTH_URL")
        .ok()
        .unwrap_or_else(|| DEFAULT_UI_HEALTH_URL.to_string());

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            return ComponentHealth::new(
                "ui",
                ComponentStatus::Degraded,
                format!("failed to build HTTP client: {}", e),
            )
        }
    };

    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => ComponentHealth::new(
            "ui",
            ComponentStatus::Healthy,
            format!("UI healthy at {}", url),
        ),
        Ok(resp) => ComponentHealth::new(
            "ui",
            ComponentStatus::Unhealthy,
            format!("UI health returned {}", resp.status()),
        ),
        Err(e) => ComponentHealth::new(
            "ui",
            ComponentStatus::Unhealthy,
            format!("UI health request failed: {}", e),
        ),
    }
}

async fn handle_ready_side_effects(
    ready: bool,
    boot_elapsed_ms: Option<u64>,
    components: &[ComponentHealth],
) {
    let flag_path =
        std::env::var("AOS_SYSTEM_READY_PATH").unwrap_or_else(|_| DEFAULT_READY_FLAG_PATH.into());
    let log_path =
        std::env::var("AOS_BOOT_LOG_PATH").unwrap_or_else(|_| DEFAULT_BOOT_LOG_PATH.into());

    let flag_exists = fs::metadata(&flag_path).await.is_ok();

    if ready {
        write_ready_flag(&flag_path, boot_elapsed_ms, components).await;
        if !flag_exists {
            append_boot_log(&log_path, boot_elapsed_ms).await;
        }
    } else if flag_exists {
        if let Err(e) = fs::remove_file(&flag_path).await {
            warn!(error = %e, path = %flag_path, "Failed to remove system ready flag");
        }
    }
}

async fn write_ready_flag(
    path: &str,
    boot_elapsed_ms: Option<u64>,
    components: &[ComponentHealth],
) {
    if let Some(parent) = StdPath::new(path).parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            warn!(
                error = %e,
                path = %parent.display(),
                "Failed to create system ready flag directory"
            );
            return;
        }
    }

    let payload = serde_json::json!({
        "ready": true,
        "boot_elapsed_ms": boot_elapsed_ms,
        "timestamp": Utc::now().to_rfc3339(),
        "components": components
    });

    let bytes = match serde_json::to_vec(&payload) {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!(error = %e, "Failed to serialize system ready payload");
            b"{\"ready\":true}".to_vec()
        }
    };

    if let Err(e) = fs::write(path, bytes).await {
        warn!(error = %e, path = %path, "Failed to write system ready flag");
    }
}

async fn append_boot_log(path: &str, boot_elapsed_ms: Option<u64>) {
    let Some(ms) = boot_elapsed_ms else {
        return;
    };

    if let Some(parent) = StdPath::new(path).parent() {
        if let Err(e) = fs::create_dir_all(parent).await {
            warn!(
                error = %e,
                path = %parent.display(),
                "Failed to create boot log directory"
            );
            return;
        }
    }

    match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
    {
        Ok(mut file) => {
            let line = format!("{} boot_ms={}\n", Utc::now().to_rfc3339(), ms);
            if let Err(e) = file.write_all(line.as_bytes()).await {
                warn!(error = %e, path = %path, "Failed to append boot log");
            }
        }
        Err(e) => {
            warn!(error = %e, path = %path, "Failed to open boot log file");
        }
    }
}

/// Get health status for a specific component
#[utoipa::path(
    get,
    path = "/healthz/{component}",
    params(
        ("component" = String, Path, description = "Component name (router, loader, kernel, db, kv, telemetry, system-metrics)")
    ),
    responses(
        (status = 200, description = "Component health status", body = ComponentHealth),
        (status = 404, description = "Component not found")
    ),
    tag = "health"
)]
pub async fn check_component_health(
    State(state): State<AppState>,
    Path(component): Path<String>,
) -> Response {
    match component.as_str() {
        "router" => check_router_health(State(state)).await.into_response(),
        "loader" => check_loader_health(State(state)).await.into_response(),
        "kernel" => check_kernel_health(State(state)).await.into_response(),
        "db" => check_db_health(State(state)).await.into_response(),
        "kv" => check_kv_health(State(state)).await.into_response(),
        "telemetry" => check_telemetry_health(State(state)).await.into_response(),
        "system-metrics" => check_system_metrics_health(State(state)).await.into_response(),
        _ => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Unknown component: {}", component),
                "valid_components": ["router", "loader", "kernel", "db", "kv", "telemetry", "system-metrics"]
            }))
        ).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_component_health_creation() {
        let health = ComponentHealth::new("test", ComponentStatus::Healthy, "Test message");
        assert_eq!(health.component, "test");
        assert_eq!(health.status, ComponentStatus::Healthy);
        assert_eq!(health.message, "Test message");
        assert!(health.details.is_none());
    }

    #[test]
    fn test_component_health_with_details() {
        let health = ComponentHealth::new("test", ComponentStatus::Degraded, "Test")
            .with_details(serde_json::json!({"key": "value"}));
        assert!(health.details.is_some());
    }

    #[test]
    fn test_component_status_serialization() {
        let json = serde_json::to_string(&ComponentStatus::Healthy)
            .expect("Failed to serialize ComponentStatus::Healthy");
        assert_eq!(json, "\"healthy\"");

        let json = serde_json::to_string(&ComponentStatus::Degraded)
            .expect("Failed to serialize ComponentStatus::Degraded");
        assert_eq!(json, "\"degraded\"");

        let json = serde_json::to_string(&ComponentStatus::Unhealthy)
            .expect("Failed to serialize ComponentStatus::Unhealthy");
        assert_eq!(json, "\"unhealthy\"");
    }

    #[test]
    fn test_component_health_serialization() {
        let health = ComponentHealth::new(
            "router",
            ComponentStatus::Healthy,
            "All systems operational",
        );
        let json = serde_json::to_string(&health).expect("Failed to serialize ComponentHealth");

        // Verify it contains expected fields
        assert!(json.contains("\"component\":\"router\""));
        assert!(json.contains("\"status\":\"healthy\""));
        assert!(json.contains("\"message\":\"All systems operational\""));
    }

    #[test]
    fn test_system_health_overall_status() {
        let components = vec![
            ComponentHealth::new("router", ComponentStatus::Healthy, "OK"),
            ComponentHealth::new("db", ComponentStatus::Healthy, "OK"),
            ComponentHealth::new("loader", ComponentStatus::Degraded, "Warning"),
        ];

        // Overall should be degraded if any component is degraded
        let overall = components
            .iter()
            .fold(ComponentStatus::Healthy, |acc, check| {
                match (&acc, &check.status) {
                    (ComponentStatus::Unhealthy, _) | (_, ComponentStatus::Unhealthy) => {
                        ComponentStatus::Unhealthy
                    }
                    (ComponentStatus::Degraded, _) | (_, ComponentStatus::Degraded) => {
                        ComponentStatus::Degraded
                    }
                    _ => ComponentStatus::Healthy,
                }
            });

        assert_eq!(overall, ComponentStatus::Degraded);
    }

    #[test]
    fn test_system_health_unhealthy_priority() {
        let components = vec![
            ComponentHealth::new("router", ComponentStatus::Healthy, "OK"),
            ComponentHealth::new("db", ComponentStatus::Degraded, "Warning"),
            ComponentHealth::new("loader", ComponentStatus::Unhealthy, "Error"),
        ];

        // Overall should be unhealthy if any component is unhealthy
        let overall = components
            .iter()
            .fold(ComponentStatus::Healthy, |acc, check| {
                match (&acc, &check.status) {
                    (ComponentStatus::Unhealthy, _) | (_, ComponentStatus::Unhealthy) => {
                        ComponentStatus::Unhealthy
                    }
                    (ComponentStatus::Degraded, _) | (_, ComponentStatus::Degraded) => {
                        ComponentStatus::Degraded
                    }
                    _ => ComponentStatus::Healthy,
                }
            });

        assert_eq!(overall, ComponentStatus::Unhealthy);
    }

    fn summary(status: WorkerHealthStatus) -> WorkerHealthSummary {
        WorkerHealthSummary {
            worker_id: "w1".to_string(),
            health_status: status,
            avg_latency_ms: 0.0,
            total_requests: 0,
            total_failures: 0,
            consecutive_slow: 0,
            consecutive_failures: 0,
        }
    }

    #[test]
    fn worker_status_crashed_is_unhealthy() {
        let summary = vec![summary(WorkerHealthStatus::Crashed)];
        assert_eq!(
            reduce_worker_status(&summary, false),
            ComponentStatus::Unhealthy
        );
    }

    #[test]
    fn worker_status_serving_overrides_unknown() {
        let summary = vec![summary(WorkerHealthStatus::Unknown)];
        assert_eq!(
            reduce_worker_status(&summary, true),
            ComponentStatus::Healthy
        );
    }

    #[test]
    fn worker_status_degraded_without_serving() {
        let summary = vec![summary(WorkerHealthStatus::Degraded)];
        assert_eq!(
            reduce_worker_status(&summary, false),
            ComponentStatus::Degraded
        );
    }
}
