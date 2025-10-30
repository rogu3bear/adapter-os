//! Telemetry endpoints for offline dashboard (logs, traces, metrics)

use crate::state::AppState;
use adapteros_telemetry::{LogBuffer, MetricsRegistry, TelemetryFilters, TelemetryLogger};
use adapteros_trace::{TraceBuffer, TraceSearchQuery};
use axum::extract::{Path, Query, State};
use axum::response::{sse::Event, IntoResponse, Response, Sse};
use axum::{http::StatusCode, Json};
// use prometheus; // Temporarily disabled
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
    State(_state): State<AppState>,
) -> Result<Json<MetricsSnapshotResponse>, (StatusCode, Json<crate::types::ErrorResponse>)> {
    // TODO: Re-enable prometheus metrics parsing when prometheus crate is added
    // For now, return empty snapshot
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(Json(MetricsSnapshotResponse {
        timestamp,
        counters: serde_json::Value::Object(serde_json::Map::new()),
        gauges: serde_json::Value::Object(serde_json::Map::new()),
        histograms: serde_json::Value::Object(serde_json::Map::new()),
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
) -> Result<Json<Vec<MetricsSeriesResponse>>, (StatusCode, Json<crate::types::ErrorResponse>)> {
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
) -> Result<Json<Vec<adapteros_telemetry::UnifiedTelemetryEvent>>, (StatusCode, Json<crate::types::ErrorResponse>)> {
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
) -> Result<Json<Vec<String>>, (StatusCode, Json<crate::types::ErrorResponse>)> {
    // If trace buffer is available in AppState, search it
    // For now, return empty
    Ok(Json(Vec::new()))
}

/// GET /api/traces/:traceId - Get a specific trace
pub async fn get_trace(
    State(state): State<AppState>,
    Path(trace_id): Path<String>,
) -> Result<Json<Option<adapteros_trace::Trace>>, (StatusCode, Json<crate::types::ErrorResponse>)> {
    // If trace buffer is available in AppState, get trace
    // For now, return None
    Ok(Json(None))
}
