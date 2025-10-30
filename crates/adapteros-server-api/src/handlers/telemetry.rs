//! Telemetry endpoints for offline dashboard (logs, traces, metrics)

use crate::state::AppState;
use adapteros_telemetry::{LogBuffer, MetricsRegistry, TelemetryFilters, TelemetryLogger};
use adapteros_trace::{TraceBuffer, TraceSearchQuery};
use axum::extract::{Path, Query, State};
use axum::response::{sse::Event, IntoResponse, Response, Sse};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::{wrappers::BroadcastStream, Stream as TokioStream};
use tokio_stream::StreamExt;

/// Response for metrics snapshot endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshotResponse {
    pub timestamp: u64,
    pub counters: serde_json::Value,
    pub gauges: serde_json::Value,
    pub histograms: serde_json::Value,
}

/// Response for metrics series endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSeriesResponse {
    pub series_name: String,
    pub points: Vec<adapteros_telemetry::MetricDataPoint>,
}

/// GET /api/metrics/snapshot - Get current metrics snapshot
pub async fn get_metrics_snapshot(
    State(state): State<AppState>,
) -> Result<Json<MetricsSnapshotResponse>, crate::errors::ApiError> {
    // Get metrics from exporter or telemetry registry
    let snapshot = state
        .metrics_exporter
        .registry()
        .gather();
    
    // Convert Prometheus metrics to JSON structure
    let mut counters = serde_json::Map::new();
    let mut gauges = serde_json::Map::new();
    let mut histograms = serde_json::Map::new();
    
    for family in snapshot {
        for metric in family.get_metric() {
            let metric_name = family.get_name();
            let value = match metric.get_counter() {
                Some(c) => {
                    counters.insert(metric_name.to_string(), serde_json::json!(c.get_value()));
                    continue;
                }
                None => (),
            };
            
            if let Some(g) = metric.get_gauge() {
                gauges.insert(metric_name.to_string(), serde_json::json!(g.get_value()));
                continue;
            }
            
            if let Some(h) = metric.get_histogram() {
                let buckets: Vec<_> = h.get_bucket().iter()
                    .map(|b| serde_json::json!({
                        "upper_bound": b.get_upper_bound(),
                        "count": b.get_cumulative_count()
                    }))
                    .collect();
                histograms.insert(metric_name.to_string(), serde_json::json!({
                    "sample_count": h.get_sample_count(),
                    "sample_sum": h.get_sample_sum(),
                    "buckets": buckets
                }));
            }
        }
    }
    
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    Ok(Json(MetricsSnapshotResponse {
        timestamp,
        counters: serde_json::Value::Object(counters),
        gauges: serde_json::Value::Object(gauges),
        histograms: serde_json::Value::Object(histograms),
    }))
}

/// Query parameters for metrics series endpoint
#[derive(Debug, Deserialize)]
pub struct MetricsSeriesQuery {
    pub series_name: Option<String>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

/// GET /api/metrics/series - Get time series data for metrics
pub async fn get_metrics_series(
    State(state): State<AppState>,
    Query(params): Query<MetricsSeriesQuery>,
) -> Result<Json<Vec<MetricsSeriesResponse>>, crate::errors::ApiError> {
    // If telemetry registry is available, use it; otherwise return empty
    // Note: We'd need to add MetricsRegistry to AppState - for now return empty
    Ok(Json(Vec::new()))
}

/// Query parameters for logs endpoint
#[derive(Debug, Deserialize)]
pub struct LogsQueryParams {
    pub limit: Option<usize>,
    pub tenant_id: Option<String>,
    pub event_type: Option<String>,
    pub level: Option<String>,
    pub component: Option<String>,
    pub trace_id: Option<String>,
}

/// GET /api/logs/query - Query logs with filters
pub async fn query_logs(
    State(state): State<AppState>,
    Query(params): Query<LogsQueryParams>,
) -> Result<Json<Vec<adapteros_telemetry::UnifiedTelemetryEvent>>, crate::errors::ApiError> {
    // If log buffer is available in AppState, query it
    // For now, return empty (we'll wire this up after adding to AppState)
    Ok(Json(Vec::new()))
}

/// GET /api/logs/stream - SSE stream of logs
pub async fn stream_logs(
    State(state): State<AppState>,
) -> Sse<impl TokioStream<Item = Result<Event, Infallible>>> {
    // Create a periodic stream that sends log updates
    let stream = tokio_stream::wrappers::IntervalStream::new(tokio::time::interval(Duration::from_secs(1)))
        .map(|_| {
            // In a real implementation, we'd read from LogBuffer and emit events
            // For now, send a keepalive
            Ok(Event::default().data("{\"type\":\"keepalive\"}"))
        });
    
    Sse::new(stream)
}

/// Query parameters for traces search endpoint
#[derive(Debug, Deserialize)]
pub struct TracesSearchQuery {
    pub span_name: Option<String>,
    pub status: Option<String>,
    pub start_time_ns: Option<u64>,
    pub end_time_ns: Option<u64>,
}

/// GET /api/traces/search - Search traces
pub async fn search_traces(
    State(state): State<AppState>,
    Query(params): Query<TracesSearchQuery>,
) -> Result<Json<Vec<String>>, crate::errors::ApiError> {
    // If trace buffer is available in AppState, search it
    // For now, return empty
    Ok(Json(Vec::new()))
}

/// GET /api/traces/:traceId - Get a specific trace
pub async fn get_trace(
    State(state): State<AppState>,
    Path(trace_id): Path<String>,
) -> Result<Json<Option<adapteros_trace::Trace>>, crate::errors::ApiError> {
    // If trace buffer is available in AppState, get trace
    // For now, return None
    Ok(Json(None))
}
