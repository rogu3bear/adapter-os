//! Metrics handlers
//!
//! Handlers for quality metrics, adapter metrics, system metrics, and Prometheus endpoint.

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};

// ========== Handlers ==========

/// Get quality metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/quality",
    responses(
        (status = 200, description = "Quality metrics", body = QualityMetricsResponse)
    )
)]
pub async fn get_quality_metrics(
    Extension(claims): Extension<Claims>,
) -> Result<Json<QualityMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    // Stub implementation - would compute from telemetry
    Ok(Json(QualityMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        arr: 0.95,
        ecs5: 0.82,
        hlr: 0.02,
        cr: 0.01,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Get adapter performance metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/adapters",
    responses(
        (status = 200, description = "Adapter metrics", body = AdapterMetricsResponse)
    )
)]
pub async fn get_adapter_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AdapterMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list adapters")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let mut performances = Vec::new();
    for adapter in adapters {
        let (total, selected, avg_gate) = state
            .db
            .get_adapter_stats(
                &claims.tenant_id,
                adapter.adapter_id.as_ref().unwrap_or(&adapter.id),
            )
            .await
            .unwrap_or((0, 0, 0.0));

        let activation_rate = if total > 0 {
            (selected as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        performances.push(AdapterPerformance {
            adapter_id: adapter
                .adapter_id
                .clone()
                .unwrap_or_else(|| adapter.id.clone()),
            name: adapter.name,
            activation_rate,
            avg_gate_value: avg_gate,
            total_requests: total,
        });
    }

    Ok(Json(AdapterMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        adapters: performances,
    }))
}

/// Get system metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/metrics/system",
    responses(
        (status = 200, description = "System metrics", body = SystemMetricsResponse)
    )
)]
pub async fn get_system_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<SystemMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    crate::permissions::require_permission(&claims, crate::permissions::Permission::MetricsView)?;

    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("System time before UNIX epoch")
        .as_secs();

    // Collect additional metrics for frontend compatibility
    let active_workers =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0) as i32;

    let requests_per_second = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-1 minute')",
    )
    .fetch_one(state.db.pool())
    .await
    .map(|count| count as f32 / 60.0)
    .unwrap_or(0.0);

    let avg_latency_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT AVG(latency_ms) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .unwrap_or(0.0) as f32;

    // Calculate active sessions count
    let active_sessions = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM chat_sessions WHERE updated_at > datetime('now', '-30 minutes')",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0) as i32;

    // Calculate error rate from recent requests
    let error_rate = {
        let total = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-5 minutes')",
        )
        .fetch_one(state.db.pool())
        .await
        .unwrap_or(0);

        if total > 0 {
            let errors = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM request_log WHERE timestamp > datetime('now', '-5 minutes') AND status_code >= 500",
            )
            .fetch_one(state.db.pool())
            .await
            .unwrap_or(0);
            Some((errors as f32) / (total as f32))
        } else {
            Some(0.0)
        }
    };

    // Tokens per second would come from inference telemetry - use 0.0 as default
    let tokens_per_second: f32 = 0.0;

    // Calculate p95 latency
    let latency_p95_ms = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT latency_ms FROM request_log WHERE timestamp > datetime('now', '-5 minutes') ORDER BY latency_ms DESC LIMIT 1 OFFSET (SELECT COUNT(*) * 5 / 100 FROM request_log WHERE timestamp > datetime('now', '-5 minutes'))",
    )
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(None)
    .map(|v| v as f32);

    Ok(Json(SystemMetricsResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        cpu_usage: metrics.cpu_usage as f32,
        memory_usage: metrics.memory_usage as f32,
        active_workers,
        requests_per_second,
        avg_latency_ms,
        disk_usage: metrics.disk_io.usage_percent,
        network_bandwidth: metrics.network_io.bandwidth_mbps,
        gpu_utilization: metrics.gpu_metrics.utilization.unwrap_or(0.0) as f32,
        uptime_seconds: collector.uptime_seconds(),
        process_count: collector.process_count(),
        load_average: LoadAverageResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            load_1min: load_avg.0,
            load_5min: load_avg.1,
            load_15min: load_avg.2,
        },
        timestamp,
        // Additional fields for frontend compatibility
        cpu_usage_percent: Some(metrics.cpu_usage as f32),
        memory_usage_percent: Some(metrics.memory_usage as f32),
        tokens_per_second: Some(tokens_per_second),
        error_rate,
        active_sessions: Some(active_sessions),
        latency_p95_ms,
    }))
}

/// Prometheus/OpenMetrics endpoint
/// Note: This endpoint requires bearer token authentication via Authorization header.
/// Authentication is checked in the route layer, not in the handler itself.
pub async fn metrics_handler(State(state): State<AppState>) -> impl IntoResponse {
    // Check if metrics are enabled
    let metrics_enabled = {
        let config = match state.config.read() {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::error!("Failed to acquire config read lock: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("internal error")
                            .with_code("INTERNAL_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
                    .into_response();
            }
        };
        config.metrics.enabled
    };

    if !metrics_enabled {
        return (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("metrics disabled").with_code("INTERNAL_ERROR")),
        )
            .into_response();
    }

    // Update worker metrics from database
    if let Err(e) = state
        .metrics_exporter
        .update_worker_metrics(&state.db)
        .await
    {
        tracing::warn!("Failed to update worker metrics: {}", e);
    }

    // Update alert metrics from database
    {
        use adapteros_db::process_monitoring::{AlertFilters, ProcessAlert};

        let filters = AlertFilters::default();
        match ProcessAlert::list(state.db.pool(), filters).await {
            Ok(alerts) => {
                let alert_tuples: Vec<(String, String, String, String, String)> = alerts
                    .iter()
                    .map(|a| {
                        (
                            a.title.clone(),
                            format!("{:?}", a.severity).to_lowercase(),
                            a.tenant_id.clone(),
                            a.worker_id.clone(),
                            format!("{:?}", a.status).to_lowercase(),
                        )
                    })
                    .collect();
                state.metrics_exporter.update_alert_metrics(&alert_tuples);
            }
            Err(e) => {
                tracing::warn!("Failed to fetch alerts for metrics: {}", e);
            }
        }
    }

    // Render metrics
    let metrics = match state.metrics_exporter.render() {
        Ok(m) => m,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to render metrics")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
                .into_response();
        }
    };

    (
        StatusCode::OK,
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        metrics,
    )
        .into_response()
}
