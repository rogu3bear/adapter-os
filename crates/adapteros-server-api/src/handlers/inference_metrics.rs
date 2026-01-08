//! H8: Inference Metrics API Handler
//!
//! Provides endpoints for querying inference performance metrics:
//! - GET /v1/metrics/adapters - Adapter selection statistics
//! - GET /v1/metrics/inference - Overall inference metrics

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use sqlx::Row;

/// Inference metrics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InferenceMetricsResponse {
    pub schema_version: String,
    pub total_requests: u64,
    pub successful_requests: u64,
    pub failed_requests: u64,
    pub total_tokens: u64,
    pub tokens_per_second: f64,
    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
    pub latency_p99_ms: u64,
    pub latency_mean_ms: f64,
    pub last_updated: u64,
}

/// Adapter metrics response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterMetricsResponse {
    pub schema_version: String,
    pub adapters: Vec<AdapterMetricItem>,
}

/// Individual adapter metric
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AdapterMetricItem {
    pub adapter_id: String,
    pub selection_count: u64,
    pub selection_percentage: f64,
}

/// GET /v1/metrics/inference
///
/// Returns overall inference performance metrics including:
/// - Request counts (total, success, failed)
/// - Throughput (tokens/sec)
/// - Latency percentiles (p50, p95, p99)
#[utoipa::path(
    get,
    path = "/v1/metrics/inference",
    responses(
        (
            status = 200,
            description = "Inference metrics retrieved",
            body = InferenceMetricsResponse
        ),
        (
            status = 403,
            description = "Permission denied",
            body = ErrorResponse
        )
    ),
    tag = "metrics"
)]
pub async fn get_inference_metrics_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<InferenceMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::MetricsView)?;

    let pool = state.db.pool();

    // Query total requests from routing_decisions (most reliable inference counter)
    let total_requests: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_decisions WHERE tenant_id = ?",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Query failed requests (those with null or error status in request_log if available)
    let failed_requests: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM request_log WHERE tenant_id = ? AND status_code >= 500",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let successful_requests = (total_requests - failed_requests).max(0);

    // Query total tokens from telemetry events
    let total_tokens: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(json_extract(event_data, '$.total_tokens')), 0) \
         FROM telemetry_events \
         WHERE tenant_id = ? AND event_type = 'inference_complete'",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Calculate tokens per second from last 5 minutes
    let tokens_last_5min: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(json_extract(event_data, '$.total_tokens')), 0) \
         FROM telemetry_events \
         WHERE tenant_id = ? AND event_type = 'inference_complete' \
         AND timestamp > datetime('now', '-5 minutes')",
    )
    .bind(&claims.tenant_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let tokens_per_second = (tokens_last_5min as f64) / 300.0;

    // Query latency percentiles from routing_decisions
    let latencies: Vec<i64> = sqlx::query_scalar(
        "SELECT total_inference_latency_us FROM routing_decisions \
         WHERE tenant_id = ? AND total_inference_latency_us IS NOT NULL \
         ORDER BY total_inference_latency_us ASC",
    )
    .bind(&claims.tenant_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let (latency_p50_ms, latency_p95_ms, latency_p99_ms, latency_mean_ms) = if latencies.is_empty() {
        (0, 0, 0, 0.0)
    } else {
        let len = latencies.len();
        let p50_idx = (len as f64 * 0.50).floor() as usize;
        let p95_idx = (len as f64 * 0.95).floor() as usize;
        let p99_idx = (len as f64 * 0.99).floor() as usize;

        let p50 = (latencies.get(p50_idx).copied().unwrap_or(0) / 1000) as u64;
        let p95 = (latencies.get(p95_idx.min(len - 1)).copied().unwrap_or(0) / 1000) as u64;
        let p99 = (latencies.get(p99_idx.min(len - 1)).copied().unwrap_or(0) / 1000) as u64;
        let mean = latencies.iter().sum::<i64>() as f64 / len as f64 / 1000.0;

        (p50, p95, p99, mean)
    };

    let last_updated = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(Json(InferenceMetricsResponse {
        schema_version: "1.0".to_string(),
        total_requests: total_requests as u64,
        successful_requests: successful_requests as u64,
        failed_requests: failed_requests as u64,
        total_tokens: total_tokens as u64,
        tokens_per_second,
        latency_p50_ms,
        latency_p95_ms,
        latency_p99_ms,
        latency_mean_ms,
        last_updated,
    }))
}

/// GET /v1/metrics/adapters
///
/// Returns adapter selection statistics including:
/// - Selection counts per adapter
/// - Selection percentages
#[utoipa::path(
    get,
    path = "/v1/metrics/adapters",
    responses(
        (
            status = 200,
            description = "Adapter metrics retrieved",
            body = AdapterMetricsResponse
        ),
        (
            status = 403,
            description = "Permission denied",
            body = ErrorResponse
        )
    ),
    tag = "metrics"
)]
pub async fn get_adapter_metrics_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AdapterMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::MetricsView)?;

    // List all adapters for the tenant
    let adapters = state
        .db
        .list_adapters_for_tenant(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list adapters")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Get total inference count for percentage calculation
    let total_inferences: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_decisions WHERE tenant_id = ?",
    )
    .bind(&claims.tenant_id)
    .fetch_one(state.db.pool())
    .await
    .unwrap_or(0);

    let mut adapter_metrics = Vec::new();

    for adapter in adapters {
        let adapter_id = adapter
            .adapter_id
            .clone()
            .unwrap_or_else(|| adapter.id.clone());

        // Get adapter stats (total decisions, selected count, avg gate)
        let (_, selected, _) = state
            .db
            .get_adapter_stats(&claims.tenant_id, &adapter_id)
            .await
            .unwrap_or((0, 0, 0.0));

        let selection_percentage = if total_inferences > 0 {
            (selected as f64 / total_inferences as f64) * 100.0
        } else {
            0.0
        };

        adapter_metrics.push(AdapterMetricItem {
            adapter_id,
            selection_count: selected as u64,
            selection_percentage,
        });
    }

    // Sort by selection count descending
    adapter_metrics.sort_by(|a, b| b.selection_count.cmp(&a.selection_count));

    Ok(Json(AdapterMetricsResponse {
        schema_version: "1.0".to_string(),
        adapters: adapter_metrics,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_inference_metrics_response_structure() {
        let response = InferenceMetricsResponse {
            schema_version: "1.0".to_string(),
            total_requests: 1000,
            successful_requests: 950,
            failed_requests: 50,
            total_tokens: 50000,
            tokens_per_second: 500.0,
            latency_p50_ms: 45,
            latency_p95_ms: 95,
            latency_p99_ms: 150,
            latency_mean_ms: 52.3,
            last_updated: 1234567890,
        };

        assert_eq!(response.total_requests, 1000);
        assert_eq!(response.latency_p95_ms, 95);
        assert!(response.tokens_per_second > 0.0);
    }

    #[tokio::test]
    async fn test_adapter_metrics_response_structure() {
        let response = AdapterMetricsResponse {
            schema_version: "1.0".to_string(),
            adapters: vec![
                AdapterMetricItem {
                    adapter_id: "adapter1".to_string(),
                    selection_count: 500,
                    selection_percentage: 50.0,
                },
                AdapterMetricItem {
                    adapter_id: "adapter2".to_string(),
                    selection_count: 300,
                    selection_percentage: 30.0,
                },
            ],
        };

        assert_eq!(response.adapters.len(), 2);
        assert_eq!(response.adapters[0].selection_count, 500);
        assert!((response.adapters[0].selection_percentage - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_metrics_serialization() {
        let metrics = InferenceMetricsResponse {
            schema_version: "1.0".to_string(),
            total_requests: 100,
            successful_requests: 95,
            failed_requests: 5,
            total_tokens: 5000,
            tokens_per_second: 250.0,
            latency_p50_ms: 40,
            latency_p95_ms: 85,
            latency_p99_ms: 120,
            latency_mean_ms: 45.5,
            last_updated: 1234567890,
        };

        let json = serde_json::to_string(&metrics).expect("Should serialize");
        assert!(json.contains("total_requests"));
        assert!(json.contains("tokens_per_second"));
        assert!(json.contains("latency_p95_ms"));
    }
}
