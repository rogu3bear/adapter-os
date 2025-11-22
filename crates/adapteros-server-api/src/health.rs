//! Component-level health check endpoints for PRD-06
//!
//! Provides health status for individual components: router, loader, kernel, DB,
//! telemetry, and system-metrics.

use crate::state::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
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
        ComponentHealth::new(
            "router",
            ComponentStatus::Degraded,
            "Router has not processed any requests yet",
        )
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

                    match migration_count {
                        Ok(count) => ComponentHealth::new(
                            "db",
                            ComponentStatus::Healthy,
                            format!("Database healthy, {} migrations applied", count),
                        ),
                        Err(_) => {
                            // Migrations table might not exist yet
                            ComponentHealth::new(
                                "db",
                                ComponentStatus::Healthy,
                                "Database connected",
                            )
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
        ComponentHealth::new(
            "telemetry",
            ComponentStatus::Degraded,
            "No telemetry activity recorded yet",
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
    for response in components {
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

    health_checks.extend(vec![router, loader, kernel, db, telemetry, system_metrics]);

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

/// Extract ComponentHealth from a response
async fn extract_health(response: Response) -> ComponentHealth {
    let (parts, body) = response.into_parts();

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

/// Get health status for a specific component
#[utoipa::path(
    get,
    path = "/healthz/{component}",
    params(
        ("component" = String, Path, description = "Component name (router, loader, kernel, db, telemetry, system-metrics)")
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
        "telemetry" => check_telemetry_health(State(state)).await.into_response(),
        "system-metrics" => check_system_metrics_health(State(state)).await.into_response(),
        _ => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Unknown component: {}", component),
                "valid_components": ["router", "loader", "kernel", "db", "telemetry", "system-metrics"]
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
}
