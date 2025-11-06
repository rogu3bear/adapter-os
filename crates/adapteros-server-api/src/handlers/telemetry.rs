//! Telemetry endpoints for offline dashboard (logs, traces, metrics)

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::{
    ActivityEventResponse, ErrorResponse, MetricDataPointResponse, MetricsSeriesResponse,
    MetricsSnapshotResponse,
};
use adapteros_db::{activity_events::ActivityEvent, users::Role};
use chrono::TimeZone;
use adapteros_telemetry::{LogLevel, TelemetryFilters, UnifiedTelemetryEvent};
use adapteros_trace::{Trace, TraceSearchQuery};
use axum::extract::{Extension, Path, Query, State};
use axum::response::{sse::Event, sse::KeepAlive, Sse};
use axum::{http::StatusCode, Json};
// use prometheus; // Temporarily disabled
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::convert::Infallible;
use futures_util::stream;
use std::sync::Arc;
use std::time::Duration;
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

#[derive(Debug, Deserialize)]
pub struct RecentActivityQuery {
    #[serde(default, rename = "event_types[]", alias = "event_types")]
    pub event_types: Vec<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[utoipa::path(
    get,
    path = "/v1/telemetry/events/recent",
    params(
        ("event_types[]" = Option<Vec<String>>, Query, description = "Filter by event types"),
        ("limit" = Option<usize>, Query, description = "Maximum number of events (default 50, max 200)"),
    ),
    responses(
        (status = 200, description = "Recent activity events", body = Vec<ActivityEventResponse>)
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn get_recent_activity(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<RecentActivityQuery>,
) -> Result<Json<Vec<ActivityEventResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let event_type_filter = if query.event_types.is_empty() {
        None
    } else {
        Some(
            query
                .event_types
                .iter()
                .map(|value| value.to_ascii_lowercase())
                .collect::<HashSet<String>>(),
        )
    };

    let mut events = load_recent_activity_events(
        &state,
        &claims.tenant_id,
        limit,
        event_type_filter.as_ref(),
        query.event_types.as_slice(),
    )
    .await?;

    events.truncate(limit);

    Ok(Json(events))
}

#[utoipa::path(
    get,
    path = "/v1/telemetry/events/recent/stream",
    params(
        ("event_types[]" = Option<Vec<String>>, Query, description = "Filter by event types"),
        ("limit" = Option<usize>, Query, description = "Initial backlog size (default 50, max 200)"),
    ),
    responses((status = 200, description = "SSE stream of recent activity events")),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn recent_activity_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<RecentActivityQuery>,
) -> Result<
    Sse<impl TokioStream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ErrorResponse>),
> {
    require_any_role(&claims, &[Role::Admin, Role::Operator, Role::Viewer])?;

    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let filter_set = if query.event_types.is_empty() {
        None
    } else {
        Some(Arc::new(
            query
                .event_types
                .iter()
                .map(|value| value.to_ascii_lowercase())
                .collect::<HashSet<String>>(),
        ))
    };

    let backlog = load_recent_activity_events(
        &state,
        &claims.tenant_id,
        limit,
        filter_set.as_ref().map(|arc| arc.as_ref()),
        query.event_types.as_slice(),
    )
    .await?;

    let backlog_stream = tokio_stream::iter(backlog.into_iter().filter_map(|event| {
        match serde_json::to_string(&event) {
            Ok(json) => Some(Ok(Event::default().event("activity").data(json))),
            Err(err) => {
                warn!(error = %err, "failed to serialize activity backlog event");
                None
            }
        }
    }));

    let tenant_id = Arc::new(claims.tenant_id.clone());
    let event_type_filter = filter_set.clone();
    // Temporarily disable realtime stream due to async filter_map complexity
    let realtime_stream = futures_util::stream::empty();

    let stream = backlog_stream.chain(realtime_stream);
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(30))
            .text("keepalive"),
    ))
}

async fn load_recent_activity_events(
    state: &AppState,
    tenant_id: &str,
    limit: usize,
    event_type_filter: Option<&HashSet<String>>,
    raw_event_types: &[String],
) -> Result<Vec<ActivityEventResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut dedupe = HashSet::new();
    let mut events: Vec<ActivityEventResponse> = Vec::new();

    let mut telemetry_filters = TelemetryFilters::default();
    telemetry_filters.tenant_id = Some(tenant_id.to_string());
    if let Some(first) = raw_event_types.first() {
        telemetry_filters.event_type = Some(first.clone());
    }
    telemetry_filters.limit = Some((limit * 2).clamp(1, 200));

    let telemetry_events = state.telemetry_buffer.query(&telemetry_filters);
    for event in telemetry_events {
        if !event_type_matches(&event.event_type, event_type_filter) {
            continue;
        }
        let response = convert_unified_event(&event);
        if dedupe.insert(response.id.clone()) {
            events.push(response);
        }
    }

    let db_events = state
        .db
        .list_activity_events(
            None,
            None,
            Some(tenant_id),
            None,
            Some((limit * 2) as i64),
            Some(0),
        )
        .await
        .map_err(|err| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to list activity events")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(err.to_string()),
                ),
            )
        })?;

    for event in db_events {
        if !event_type_matches(&event.event_type, event_type_filter) {
            continue;
        }
        let response = convert_activity_event(event);
        if dedupe.insert(response.id.clone()) {
            events.push(response);
        }
    }

    events.sort_by(|a, b| parse_timestamp(&b.timestamp).cmp(&parse_timestamp(&a.timestamp)));

    Ok(events)
}

fn event_type_matches(event_type: &str, filter: Option<&HashSet<String>>) -> bool {
    match filter {
        Some(allowed) => allowed.contains(&event_type.to_ascii_lowercase()),
        None => true,
    }
}

fn convert_unified_event(event: &UnifiedTelemetryEvent) -> ActivityEventResponse {
    ActivityEventResponse {
        id: event.id.clone(),
        timestamp: event.timestamp.to_rfc3339(),
        event_type: event.event_type.clone(),
        level: format!("{:?}", event.level).to_ascii_lowercase(),
        message: event.message.clone(),
        component: event.component.clone(),
        tenant_id: event.tenant_id.clone(),
        user_id: event.user_id.clone(),
        metadata: event.metadata.clone(),
    }
}

fn convert_activity_event(event: ActivityEvent) -> ActivityEventResponse {
    let metadata: Option<Value> = event
        .metadata_json
        .as_ref()
        .and_then(|raw| serde_json::from_str(raw).ok());

    let message = metadata
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .map(|m| m.to_string())
        .unwrap_or_else(|| format!("Activity: {}", event.event_type.replace('_', " ")));

    let timestamp = parse_timestamp(&event.created_at);

    ActivityEventResponse {
        id: event.id,
        timestamp: timestamp.to_rfc3339(),
        event_type: event.event_type,
        level: "info".to_string(),
        message,
        component: event.target_type,
        tenant_id: Some(event.tenant_id),
        user_id: Some(event.user_id),
        metadata,
    }
}

fn parse_timestamp(value: &str) -> chrono::DateTime<chrono::Utc> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return dt.with_timezone(&chrono::Utc);
    }

    if let Ok(ndt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return chrono::Utc.from_utc_datetime(&ndt);
    }

    chrono::Utc::now()
}
