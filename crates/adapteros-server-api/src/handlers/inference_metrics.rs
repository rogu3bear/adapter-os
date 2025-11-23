//! H8: Inference Metrics API Handler
//!
//! Provides endpoints for querying inference performance metrics:
//! - GET /v1/metrics/adapters - Adapter selection statistics
//! - GET /v1/metrics/inference - Overall inference metrics

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::types::ErrorResponse;
use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

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

/// Mock state for demonstration (replace with actual state in production)
#[derive(Clone)]
pub struct MetricsState {
    // In production, this would hold InferenceMetricsCollector
}

impl MetricsState {
    pub fn new() -> Self {
        Self {}
    }

    /// Get inference metrics (mock implementation)
    pub async fn get_inference_metrics(&self) -> InferenceMetricsResponse {
        // In production, this would query InferenceMetricsCollector
        InferenceMetricsResponse {
            schema_version: "1.0".to_string(),
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            total_tokens: 0,
            tokens_per_second: 0.0,
            latency_p50_ms: 0,
            latency_p95_ms: 0,
            latency_p99_ms: 0,
            latency_mean_ms: 0.0,
            last_updated: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }

    /// Get adapter metrics (mock implementation)
    pub async fn get_adapter_metrics(&self) -> AdapterMetricsResponse {
        // In production, this would query InferenceMetricsCollector
        AdapterMetricsResponse {
            schema_version: "1.0".to_string(),
            adapters: vec![],
        }
    }
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
    State(state): State<Arc<MetricsState>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<InferenceMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::MetricsView)?;

    let metrics = state.get_inference_metrics().await;

    Ok(Json(metrics))
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
    State(state): State<Arc<MetricsState>>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<AdapterMetricsResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::MetricsView)?;

    let metrics = state.get_adapter_metrics().await;

    Ok(Json(metrics))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_metrics_state_creation() {
        let state = MetricsState::new();
        let metrics = state.get_inference_metrics().await;

        assert_eq!(metrics.schema_version, "1.0");
        assert!(metrics.last_updated > 0);
    }

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
