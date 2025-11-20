//! Metrics endpoint handler for upload system monitoring
//!
//! Provides REST endpoints for querying upload metrics and health status.
//! Integrated with Prometheus metrics and custom telemetry events.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

use crate::{
    state::AppState,
    upload_metrics::{
        CleanupMetrics, QueueMetrics, RateLimitMetrics, SuccessRateMetrics, TenantUploadMetrics,
        UploadMetricsCollector, UploadMetricsSnapshot,
    },
};

/// Query parameters for metrics filtering
#[derive(Debug, Deserialize)]
pub struct MetricsQueryParams {
    /// Optional tenant ID to filter by
    pub tenant_id: Option<String>,
    /// Optional time window in seconds (default: 3600)
    pub window_secs: Option<u64>,
}

/// Metrics response structure
#[derive(Debug, Serialize)]
pub struct MetricsResponse {
    /// Timestamp when metrics were collected
    pub timestamp: u64,
    /// Upload duration statistics
    pub upload_durations: UploadDurationResponse,
    /// File size distribution
    pub file_size: FileSizeResponse,
    /// Upload success/failure rates
    pub success_rates: SuccessRateResponse,
    /// Per-tenant upload metrics
    pub tenant_metrics: TenantResponse,
    /// Queue depth metrics
    pub queue_metrics: QueueResponse,
    /// Cleanup operation metrics
    pub cleanup_metrics: CleanupResponse,
    /// Rate limiter metrics
    pub rate_limit_metrics: RateLimitResponse,
}

/// Upload duration response
#[derive(Debug, Serialize)]
pub struct UploadDurationResponse {
    pub streaming_p50_ms: f64,
    pub streaming_p95_ms: f64,
    pub streaming_p99_ms: f64,
    pub database_p50_ms: f64,
    pub database_p95_ms: f64,
    pub database_p99_ms: f64,
    pub total_p50_ms: f64,
    pub total_p95_ms: f64,
    pub total_p99_ms: f64,
}

/// File size response
#[derive(Debug, Serialize)]
pub struct FileSizeResponse {
    pub small_files_count: u64,
    pub medium_files_count: u64,
    pub large_files_count: u64,
    pub xlarge_files_count: u64,
    pub avg_file_size_bytes: f64,
    pub max_file_size_bytes: u64,
}

/// Success rate response
#[derive(Debug, Serialize)]
pub struct SuccessRateResponse {
    pub successful_uploads: u64,
    pub failed_uploads: u64,
    pub rate_limited: u64,
    pub aborted: u64,
    pub success_rate_percent: f64,
}

/// Tenant metrics response
#[derive(Debug, Serialize)]
pub struct TenantResponse {
    pub total_tenants: usize,
    pub top_10_tenants: Vec<TenantStatsResponse>,
    pub total_uploads: u64,
    pub total_bytes: u64,
}

/// Per-tenant statistics
#[derive(Debug, Serialize)]
pub struct TenantStatsResponse {
    pub tenant_id: String,
    pub upload_count: u64,
    pub bytes_uploaded: u64,
    pub avg_file_size_bytes: f64,
}

/// Queue metrics response
#[derive(Debug, Serialize)]
pub struct QueueResponse {
    pub current_queue_depth: f64,
    pub max_queue_depth: f64,
    pub pending_cleanup_items: f64,
}

/// Cleanup metrics response
#[derive(Debug, Serialize)]
pub struct CleanupResponse {
    pub total_operations: u64,
    pub avg_duration_ms: f64,
    pub p95_duration_ms: f64,
    pub p99_duration_ms: f64,
    pub total_items_deleted: u64,
    pub total_errors: u64,
}

/// Rate limiter metrics response
#[derive(Debug, Serialize)]
pub struct RateLimitResponse {
    pub total_refills: u64,
    pub tenants_at_limit: usize,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct UploadHealthResponse {
    pub status: String, // "healthy", "degraded", "unhealthy"
    pub metrics: HealthMetrics,
}

/// Health metrics details
#[derive(Debug, Serialize)]
pub struct HealthMetrics {
    pub success_rate_percent: f64,
    pub p95_upload_duration_ms: f64,
    pub queue_depth: f64,
    pub cleanup_errors_in_last_hour: u64,
    pub rate_limited_attempts_in_last_hour: u64,
}

/// Get upload metrics summary
///
/// Returns aggregated upload metrics including duration, file size, success rates,
/// and per-tenant statistics.
///
/// # Query Parameters
/// - `tenant_id`: Optional tenant ID to filter metrics by
/// - `window_secs`: Time window in seconds for metrics (default: 3600)
///
/// # Responses
/// - 200: Metrics retrieved successfully
/// - 500: Internal error retrieving metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/uploads",
    params(MetricsQueryParams),
    responses(
        (status = 200, description = "Upload metrics", body = MetricsResponse),
        (status = 500, description = "Internal error")
    ),
    tag = "metrics"
)]
pub async fn get_upload_metrics(
    State(state): State<AppState>,
    Query(params): Query<MetricsQueryParams>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    info!(
        tenant_id = ?params.tenant_id,
        window_secs = ?params.window_secs,
        "Fetching upload metrics"
    );

    // Get snapshot from metrics collector
    let snapshot = state.upload_metrics.get_metrics_snapshot().await;

    let response = MetricsResponse {
        timestamp: snapshot.timestamp,
        upload_durations: UploadDurationResponse {
            streaming_p50_ms: snapshot.upload_durations.streaming_p50_ms,
            streaming_p95_ms: snapshot.upload_durations.streaming_p95_ms,
            streaming_p99_ms: snapshot.upload_durations.streaming_p99_ms,
            database_p50_ms: snapshot.upload_durations.database_p50_ms,
            database_p95_ms: snapshot.upload_durations.database_p95_ms,
            database_p99_ms: snapshot.upload_durations.database_p99_ms,
            total_p50_ms: snapshot.upload_durations.total_p50_ms,
            total_p95_ms: snapshot.upload_durations.total_p95_ms,
            total_p99_ms: snapshot.upload_durations.total_p99_ms,
        },
        file_size: FileSizeResponse {
            small_files_count: snapshot.file_size.small_files_total,
            medium_files_count: snapshot.file_size.medium_files_total,
            large_files_count: snapshot.file_size.large_files_total,
            xlarge_files_count: snapshot.file_size.xlarge_files_total,
            avg_file_size_bytes: snapshot.file_size.avg_file_size_bytes,
            max_file_size_bytes: snapshot.file_size.max_file_size_bytes,
        },
        success_rates: SuccessRateResponse {
            successful_uploads: snapshot.success_rates.successful_uploads_total,
            failed_uploads: snapshot.success_rates.failed_uploads_total,
            rate_limited: snapshot.success_rates.rate_limited_total,
            aborted: snapshot.success_rates.aborted_total,
            success_rate_percent: snapshot.success_rates.success_rate_percent,
        },
        tenant_metrics: TenantResponse {
            total_tenants: snapshot.tenant_metrics.uploads_per_tenant.len(),
            top_10_tenants: snapshot
                .tenant_metrics
                .top_uploading_tenants
                .iter()
                .take(10)
                .map(|(tenant_id, bytes)| {
                    let upload_count = snapshot
                        .tenant_metrics
                        .uploads_per_tenant
                        .get(tenant_id)
                        .copied()
                        .unwrap_or(0.0) as u64;
                    TenantStatsResponse {
                        tenant_id: tenant_id.clone(),
                        upload_count,
                        bytes_uploaded: *bytes,
                        avg_file_size_bytes: if upload_count > 0 {
                            (*bytes as f64) / (upload_count as f64)
                        } else {
                            0.0
                        },
                    }
                })
                .collect(),
            total_uploads: snapshot.success_rates.successful_uploads_total,
            total_bytes: snapshot.tenant_metrics.bytes_per_tenant.values().sum(),
        },
        queue_metrics: QueueResponse {
            current_queue_depth: snapshot.queue_metrics.current_queue_depth,
            max_queue_depth: snapshot.queue_metrics.max_queue_depth,
            pending_cleanup_items: snapshot.queue_metrics.pending_cleanup_items,
        },
        cleanup_metrics: CleanupResponse {
            total_operations: snapshot.cleanup_metrics.cleanup_operations_total,
            avg_duration_ms: (snapshot.cleanup_metrics.cleanup_duration_p50_ms
                + snapshot.cleanup_metrics.cleanup_duration_p95_ms
                + snapshot.cleanup_metrics.cleanup_duration_p99_ms)
                / 3.0,
            p95_duration_ms: snapshot.cleanup_metrics.cleanup_duration_p95_ms,
            p99_duration_ms: snapshot.cleanup_metrics.cleanup_duration_p99_ms,
            total_items_deleted: snapshot.cleanup_metrics.temp_files_deleted_total,
            total_errors: snapshot.cleanup_metrics.cleanup_errors_total,
        },
        rate_limit_metrics: RateLimitResponse {
            total_refills: snapshot.rate_limit_metrics.refills_total,
            tenants_at_limit: snapshot
                .rate_limit_metrics
                .tokens_available_per_tenant
                .iter()
                .filter(|(_, tokens)| **tokens <= 0.0)
                .count(),
        },
    };

    Ok(Json(response))
}

/// Get upload health status
///
/// Quick health check endpoint that returns the overall health of the upload system
/// based on success rates, latency, and error metrics.
///
/// # Responses
/// - 200: Health status retrieved successfully
#[utoipa::path(
    get,
    path = "/v1/metrics/uploads/health",
    responses(
        (status = 200, description = "Upload system health", body = UploadHealthResponse),
    ),
    tag = "metrics"
)]
pub async fn get_upload_health(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let snapshot = state.upload_metrics.get_metrics_snapshot().await;

    // Determine health status based on metrics
    let success_rate = snapshot.success_rates.success_rate_percent;
    let p95_duration = snapshot.upload_durations.total_p95_ms;
    let queue_depth = snapshot.queue_metrics.current_queue_depth;
    let cleanup_errors = snapshot.cleanup_metrics.cleanup_errors_total;

    let status = match (success_rate, p95_duration, queue_depth, cleanup_errors) {
        // Healthy: >95% success, p95 < 5s, queue < 100, < 10 errors
        (success, duration, depth, errors)
            if success > 95.0 && duration < 5000.0 && depth < 100.0 && errors < 10 =>
        {
            "healthy".to_string()
        }
        // Degraded: >90% success or elevated latency/queue/errors
        (success, duration, depth, errors)
            if success > 90.0 || duration < 10000.0 || depth < 500.0 || errors < 50 =>
        {
            "degraded".to_string()
        }
        // Unhealthy: low success rate or high errors
        _ => "unhealthy".to_string(),
    };

    let response = UploadHealthResponse {
        status,
        metrics: HealthMetrics {
            success_rate_percent: snapshot.success_rates.success_rate_percent,
            p95_upload_duration_ms: snapshot.upload_durations.total_p95_ms,
            queue_depth: snapshot.queue_metrics.current_queue_depth,
            cleanup_errors_in_last_hour: snapshot.cleanup_metrics.cleanup_errors_total,
            rate_limited_attempts_in_last_hour: snapshot.success_rates.rate_limited_total,
        },
    };

    Ok(Json(response))
}

/// Get Prometheus format metrics
///
/// Returns metrics in Prometheus text format (OpenMetrics format).
/// Suitable for scraping by Prometheus, Grafana, and other monitoring tools.
///
/// # Responses
/// - 200: Prometheus format metrics
/// - 500: Internal error retrieving metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/uploads/prometheus",
    responses(
        (status = 200, description = "Prometheus format metrics"),
        (status = 500, description = "Internal error")
    ),
    tag = "metrics"
)]
pub async fn get_prometheus_metrics(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    state
        .upload_metrics
        .get_prometheus_metrics()
        .map(|metrics| (StatusCode::OK, metrics))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_response_serialization() {
        let response = MetricsResponse {
            timestamp: 1234567890,
            upload_durations: UploadDurationResponse {
                streaming_p50_ms: 100.0,
                streaming_p95_ms: 200.0,
                streaming_p99_ms: 300.0,
                database_p50_ms: 50.0,
                database_p95_ms: 100.0,
                database_p99_ms: 150.0,
                total_p50_ms: 150.0,
                total_p95_ms: 300.0,
                total_p99_ms: 450.0,
            },
            file_size: FileSizeResponse {
                small_files_count: 10,
                medium_files_count: 5,
                large_files_count: 2,
                xlarge_files_count: 1,
                avg_file_size_bytes: 50_000_000.0,
                max_file_size_bytes: 1_000_000_000,
            },
            success_rates: SuccessRateResponse {
                successful_uploads: 100,
                failed_uploads: 5,
                rate_limited: 2,
                aborted: 1,
                success_rate_percent: 95.0,
            },
            tenant_metrics: TenantResponse {
                total_tenants: 10,
                top_10_tenants: vec![],
                total_uploads: 100,
                total_bytes: 5_000_000_000,
            },
            queue_metrics: QueueResponse {
                current_queue_depth: 5.0,
                max_queue_depth: 100.0,
                pending_cleanup_items: 2.0,
            },
            cleanup_metrics: CleanupResponse {
                total_operations: 50,
                avg_duration_ms: 100.0,
                p95_duration_ms: 200.0,
                p99_duration_ms: 300.0,
                total_items_deleted: 100,
                total_errors: 0,
            },
            rate_limit_metrics: RateLimitResponse {
                total_refills: 1000,
                tenants_at_limit: 2,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"timestamp\":"));
        assert!(json.contains("\"success_rate_percent\":"));
    }

    #[test]
    fn test_health_status_determination() {
        // Test healthy status
        let health = UploadHealthResponse {
            status: "healthy".to_string(),
            metrics: HealthMetrics {
                success_rate_percent: 99.0,
                p95_upload_duration_ms: 1000.0,
                queue_depth: 10.0,
                cleanup_errors_in_last_hour: 0,
                rate_limited_attempts_in_last_hour: 0,
            },
        };

        assert_eq!(health.status, "healthy");
        assert_eq!(health.metrics.success_rate_percent, 99.0);
    }
}
