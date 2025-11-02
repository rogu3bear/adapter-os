//! Telemetry endpoints for offline dashboard (logs, traces, metrics)

use crate::state::AppState;
use crate::types::{
    ErrorResponse, MetricDataPointResponse, MetricsSeriesResponse, MetricsSnapshotResponse,
};
use adapteros_telemetry::{LogLevel, TelemetryFilters, UnifiedTelemetryEvent};
use adapteros_trace::{Trace, TraceSearchQuery};
use axum::extract::{Path, Query, State};
use axum::response::{sse::Event, sse::KeepAlive, Sse};
use axum::{http::StatusCode, Json};
// use prometheus; // Temporarily disabled
use serde::Deserialize;
use std::convert::Infallible;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::Stream as TokioStream;
use tokio_stream::StreamExt;
use tracing::warn;

/// GET /api/metrics/snapshot - Get current metrics snapshot
pub async fn get_metrics_snapshot(
    State(state): State<AppState>,
) -> Result<Json<MetricsSnapshotResponse>, (StatusCode, Json<ErrorResponse>)> {
    let snapshot = state.metrics_collector.get_metrics_snapshot().await;
    Ok(Json(MetricsSnapshotResponse::from(snapshot)))
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
) -> Result<Json<Vec<MetricsSeriesResponse>>, (StatusCode, Json<ErrorResponse>)> {
    if let (Some(start), Some(end)) = (params.start_ms, params.end_ms) {
        if start > end {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("start_ms must be less than or equal to end_ms")
                        .with_code("BAD_REQUEST"),
                ),
            ));
        }
    }

    let registry = &state.metrics_registry;
    let mut responses = Vec::new();

    let make_series_response =
        |name: String, start: Option<u64>, end: Option<u64>| -> Option<MetricsSeriesResponse> {
            registry.get_series(&name).map(|series| {
                let points = series
                    .get_points(start, end)
                    .into_iter()
                    .map(MetricDataPointResponse::from)
                    .collect::<Vec<_>>();
                MetricsSeriesResponse {
                    series_name: name,
                    points,
                }
            })
        };

    match params.series_name {
        Some(name) => match make_series_response(name.clone(), params.start_ms, params.end_ms) {
            Some(series) => {
                responses.push(series);
                Ok(Json(responses))
            }
            None => Err((
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("metrics series not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(name),
                ),
            )),
        },
        None => {
            for name in registry.list_series() {
                if let Some(series) =
                    make_series_response(name.clone(), params.start_ms, params.end_ms)
                {
                    responses.push(series);
                }
            }
            Ok(Json(responses))
        }
    }
}

/// Query parameters for logs endpoint
#[derive(Debug, Deserialize, Clone)]
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
) -> Result<Json<Vec<UnifiedTelemetryEvent>>, (StatusCode, Json<crate::types::ErrorResponse>)> {
    let parsed_filters = match normalize_log_filters(&params) {
        Ok(filters) => filters,
        Err(err) => return Err((StatusCode::BAD_REQUEST, Json(err))),
    };

    let events = state.telemetry_buffer.query(&parsed_filters.telemetry);
    Ok(Json(events))
}

/// GET /api/logs/stream - SSE stream of logs
pub async fn stream_logs(
    State(state): State<AppState>,
    Query(params): Query<LogsQueryParams>,
) -> Sse<impl TokioStream<Item = Result<Event, Infallible>>> {
    let filters_for_stream = match normalize_log_filters(&params) {
        Ok(filters) => filters.realtime,
        Err(err) => {
            warn!(
                error = %err.error,
                "invalid log stream filters provided; defaulting to unfiltered stream"
            );
            NormalizedLogFilters::default()
        }
    };

    let rx = state.telemetry_tx.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(move |res| {
        let filters = filters_for_stream.clone();
        match res {
            Ok(event) if event_matches_filters(&event, &filters) => {
                match serde_json::to_string(&event) {
                    Ok(json) => Some(Ok(Event::default().data(json))),
                    Err(e) => {
                        warn!("failed to serialize log event for stream: {}", e);
                        None
                    }
                }
            }
            Ok(_) => None,
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
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
    // Parse status parameter
    let status = params.status.as_ref().and_then(|s| match s.as_str() {
        "ok" | "OK" => Some(adapteros_trace::SpanStatus::Ok),
        "error" | "ERROR" => Some(adapteros_trace::SpanStatus::Error),
        "unset" | "UNSET" => Some(adapteros_trace::SpanStatus::Unset),
        _ => None,
    });

    // Create search query
    let query = TraceSearchQuery {
        span_name: params.span_name.clone(),
        status,
        start_time_ns: params.start_time_ns,
        end_time_ns: params.end_time_ns,
    };

    // Search traces in the trace buffer
    let trace_ids = state.trace_buffer.search(&query);
    Ok(Json(trace_ids))
}

/// GET /api/traces/:traceId - Get a specific trace
pub async fn get_trace(
    State(state): State<AppState>,
    Path(trace_id): Path<String>,
) -> Result<Json<Option<Trace>>, (StatusCode, Json<crate::types::ErrorResponse>)> {
    // Get trace from the trace buffer
    let trace = state.trace_buffer.get_trace(&trace_id);
    Ok(Json(trace))
}

#[derive(Clone, Debug, Default)]
pub struct NormalizedLogFilters {
    pub tenant_id: Option<String>,
    pub event_type: Option<String>,
    pub level: Option<LogLevel>,
    pub component: Option<String>,
    pub trace_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ParsedLogFilters {
    pub telemetry: TelemetryFilters,
    pub realtime: NormalizedLogFilters,
}

impl Default for ParsedLogFilters {
    fn default() -> Self {
        Self {
            telemetry: TelemetryFilters::default(),
            realtime: NormalizedLogFilters::default(),
        }
    }
}

pub fn normalize_log_filters(params: &LogsQueryParams) -> Result<ParsedLogFilters, ErrorResponse> {
    let mut telemetry_filters = TelemetryFilters::default();
    let mut realtime_filters = NormalizedLogFilters::default();

    if let Some(limit) = params.limit {
        if limit == 0 {
            return Err(
                ErrorResponse::new("limit must be greater than zero").with_code("BAD_REQUEST")
            );
        }
        telemetry_filters.limit = Some(limit.min(1024));
    }

    if let Some(ref tenant) = params.tenant_id {
        let trimmed = tenant.trim();
        if !trimmed.is_empty() {
            telemetry_filters.tenant_id = Some(trimmed.to_string());
            realtime_filters.tenant_id = Some(trimmed.to_string());
        }
    }

    if let Some(ref event_type) = params.event_type {
        let trimmed = event_type.trim();
        if !trimmed.is_empty() {
            telemetry_filters.event_type = Some(trimmed.to_string());
            realtime_filters.event_type = Some(trimmed.to_string());
        }
    }

    if let Some(ref level) = params.level {
        let trimmed = level.trim();
        if !trimmed.is_empty() {
            let parsed = parse_log_level(trimmed).ok_or_else(|| {
                ErrorResponse::new("invalid log level")
                    .with_code("BAD_REQUEST")
                    .with_string_details(trimmed.to_string())
            })?;
            telemetry_filters.level = Some(parsed.clone());
            realtime_filters.level = Some(parsed);
        }
    }

    if let Some(ref component) = params.component {
        let trimmed = component.trim();
        if !trimmed.is_empty() {
            telemetry_filters.component = Some(trimmed.to_string());
            realtime_filters.component = Some(trimmed.to_string());
        }
    }

    if let Some(ref trace_id) = params.trace_id {
        let trimmed = trace_id.trim();
        if !trimmed.is_empty() {
            telemetry_filters.trace_id = Some(trimmed.to_string());
            realtime_filters.trace_id = Some(trimmed.to_string());
        }
    }

    Ok(ParsedLogFilters {
        telemetry: telemetry_filters,
        realtime: realtime_filters,
    })
}

fn parse_log_level(level: &str) -> Option<LogLevel> {
    match level.to_ascii_lowercase().as_str() {
        "debug" => Some(LogLevel::Debug),
        "info" => Some(LogLevel::Info),
        "warn" | "warning" => Some(LogLevel::Warn),
        "error" => Some(LogLevel::Error),
        "critical" => Some(LogLevel::Critical),
        _ => None,
    }
}

pub fn event_matches_filters(
    event: &UnifiedTelemetryEvent,
    filters: &NormalizedLogFilters,
) -> bool {
    if let Some(ref tenant) = filters.tenant_id {
        if event.tenant_id.as_ref() != Some(tenant) {
            return false;
        }
    }

    if let Some(ref event_type) = filters.event_type {
        if &event.event_type != event_type {
            return false;
        }
    }

    if let Some(ref level) = filters.level {
        if &event.level != level {
            return false;
        }
    }

    if let Some(ref component) = filters.component {
        if event.component.as_ref() != Some(component) {
            return false;
        }
    }

    if let Some(ref trace_id) = filters.trace_id {
        if event.trace_id.as_ref() != Some(trace_id) {
            return false;
        }
    }

    true
}
