//! Telemetry endpoints for offline dashboard (logs, traces, metrics)

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::permissions::{require_permission, Permission};
use crate::state::AppState;
use crate::telemetry::{SpanStatus, TraceSearchQuery};
use crate::types::{
    ErrorResponse, InferenceTraceDetailResponse, InferenceTraceResponse, MetricDataPointResponse,
    MetricsSeriesResponse, MetricsSnapshotResponse, TimingBreakdown, TokenDecision,
    TraceReceiptSummary, UiInferenceTraceDetailResponse, UiTraceReceiptSummary,
};
use adapteros_db::kv_metrics::{
    global_kv_metrics, KV_ALERT_METRIC_DEGRADATIONS, KV_ALERT_METRIC_DRIFT, KV_ALERT_METRIC_ERRORS,
    KV_ALERT_METRIC_FALLBACKS,
};
use adapteros_db::users::Role;
use adapteros_db::ActivityEvent;
use adapteros_db::{get_inference_trace_detail_for_tenant, list_inference_traces_for_tenant};
use adapteros_telemetry::{LogLevel, TelemetryFilters, UnifiedTelemetryEvent};
use axum::extract::{Extension, Path, Query, State};
use axum::response::{sse::Event, sse::KeepAlive, Sse};
use axum::{http::StatusCode, Json};
use chrono::TimeZone;
// use prometheus; // Temporarily disabled
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::Stream as TokioStream;
use tokio_stream::StreamExt;
use tracing::warn;

/// Local ActivityEventResponse for telemetry handlers.
/// This avoids utoipa ToSchema issues when ActivityEventResponse is used internally.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ActivityEventResponse {
    pub id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub event_type: String,
    pub workspace_id: Option<String>,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: String,
}

/// GET /api/metrics/snapshot - Get current metrics snapshot
#[utoipa::path(
    get,
    path = "/v1/metrics/snapshot",
    responses(
        (status = 200, description = "Current metrics snapshot", body = MetricsSnapshotResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "metrics"
)]
pub async fn get_metrics_snapshot(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MetricsSnapshotResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::MetricsView)?;

    // Use metrics_exporter snapshot and convert to response format
    let exporter_snapshot = state.metrics_exporter.snapshot();
    let kv_snapshot = global_kv_metrics().snapshot();

    let mut counters = std::collections::HashMap::from([(
        "total_requests".to_string(),
        exporter_snapshot.total_requests,
    )]);
    counters.insert(
        KV_ALERT_METRIC_FALLBACKS.to_string(),
        kv_snapshot.fallback_operations_total as f64,
    );
    counters.insert(
        KV_ALERT_METRIC_ERRORS.to_string(),
        kv_snapshot.errors_total as f64,
    );
    counters.insert(
        KV_ALERT_METRIC_DRIFT.to_string(),
        kv_snapshot.drift_detections_total as f64,
    );
    counters.insert(
        KV_ALERT_METRIC_DEGRADATIONS.to_string(),
        kv_snapshot.degraded_events_total as f64,
    );

    let gauges = std::collections::HashMap::from([
        ("queue_depth".to_string(), exporter_snapshot.queue_depth),
        (
            "avg_latency_ms".to_string(),
            exporter_snapshot.avg_latency_ms,
        ),
    ]);

    // Create flattened metrics map (union of counters and gauges) for frontend compatibility
    let mut metrics = counters.clone();
    metrics.extend(gauges.clone());

    // Create a MetricsSnapshotResponse from the exporter snapshot
    let response = MetricsSnapshotResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        counters,
        gauges,
        histograms: std::collections::HashMap::new(),
        metrics,
    };

    Ok(Json(response))
}

/// Query parameters for metrics series endpoint
#[derive(Debug, Deserialize)]
pub struct MetricsSeriesQuery {
    pub series_name: Option<String>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
}

/// GET /api/metrics/series - Get time series data for metrics
#[utoipa::path(
    get,
    path = "/v1/metrics/series",
    params(
        ("series_name" = Option<String>, Query, description = "Series name filter"),
        ("start_ms" = Option<u64>, Query, description = "Start time (ms since epoch)"),
        ("end_ms" = Option<u64>, Query, description = "End time (ms since epoch)")
    ),
    responses(
        (status = 200, description = "Metrics series", body = Vec<MetricsSeriesResponse>),
        (status = 400, description = "Invalid query", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "metrics"
)]
pub async fn get_metrics_series(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<MetricsSeriesQuery>,
) -> Result<Json<Vec<MetricsSeriesResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::MetricsView)?;

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

    match params.series_name {
        Some(name) => match registry.get_series_async(&name).await {
            Some(series) => {
                let points = series
                    .get_points(params.start_ms, params.end_ms)
                    .into_iter()
                    .map(MetricDataPointResponse::from)
                    .collect::<Vec<_>>();
                responses.push(MetricsSeriesResponse {
                    series_name: name,
                    points,
                });
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
            for name in registry.list_series_async().await {
                if let Some(series) = registry.get_series_async(&name).await {
                    let points = series
                        .get_points(params.start_ms, params.end_ms)
                        .into_iter()
                        .map(MetricDataPointResponse::from)
                        .collect::<Vec<_>>();
                    responses.push(MetricsSeriesResponse {
                        series_name: name,
                        points,
                    });
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

/// Query parameters for inference trace list endpoint
#[derive(Debug, Deserialize)]
pub struct InferenceTracesQueryParams {
    pub request_id: Option<String>,
    pub limit: Option<usize>,
}

/// Query parameters for inference trace detail endpoint
#[derive(Debug, Deserialize)]
pub struct InferenceTraceDetailQueryParams {
    /// Return tokens with index > tokens_after
    pub tokens_after: Option<u32>,
    /// Maximum number of token decisions to return (0 = no cap)
    pub tokens_limit: Option<u32>,
}

const UI_DEFAULT_TRACE_TOKENS_LIMIT: u32 = 200;

fn normalize_ui_tokens_limit(tokens_limit: Option<u32>) -> Option<u32> {
    match tokens_limit {
        Some(0) => None,
        Some(value) => Some(value),
        None => Some(UI_DEFAULT_TRACE_TOKENS_LIMIT),
    }
}

/// GET /api/logs/query - Query logs with filters
#[utoipa::path(
    get,
    path = "/v1/logs/query",
    params(
        ("limit" = Option<usize>, Query, description = "Max results"),
        ("tenant_id" = Option<String>, Query, description = "Tenant ID"),
        ("event_type" = Option<String>, Query, description = "Event type"),
        ("level" = Option<String>, Query, description = "Log level"),
        ("component" = Option<String>, Query, description = "Component"),
        ("trace_id" = Option<String>, Query, description = "Trace ID")
    ),
    responses(
        (status = 200, description = "Log events"),
        (status = 400, description = "Invalid query"),
        (status = 403, description = "Forbidden")
    ),
    tag = "telemetry"
)]
pub async fn query_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<LogsQueryParams>,
) -> Result<Json<Vec<UnifiedTelemetryEvent>>, (StatusCode, Json<crate::types::ErrorResponse>)> {
    require_permission(&claims, Permission::TelemetryView).map_err(|_| {
        (
            axum::http::StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        )
    })?;

    let mut parsed_filters = match normalize_log_filters(&params) {
        Ok(filters) => filters,
        Err(err) => return Err((StatusCode::BAD_REQUEST, Json(err))),
    };

    if parsed_filters.telemetry.tenant_id.is_none() {
        parsed_filters.telemetry.tenant_id = Some(claims.tenant_id.clone());
    }
    if parsed_filters.realtime.tenant_id.is_none() {
        parsed_filters.realtime.tenant_id = Some(claims.tenant_id.clone());
    }

    let events = state
        .telemetry_buffer
        .query(&parsed_filters.telemetry)
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid telemetry filters")
                        .with_code("BAD_REQUEST")
                        .with_string_details(err.to_string()),
                ),
            )
        })?;
    Ok(Json(events))
}

/// GET /api/logs/stream - SSE stream of logs
#[utoipa::path(
    get,
    path = "/v1/logs/stream",
    params(
        ("limit" = Option<usize>, Query, description = "Max buffered results"),
        ("tenant_id" = Option<String>, Query, description = "Tenant ID"),
        ("event_type" = Option<String>, Query, description = "Event type"),
        ("level" = Option<String>, Query, description = "Log level"),
        ("component" = Option<String>, Query, description = "Component"),
        ("trace_id" = Option<String>, Query, description = "Trace ID")
    ),
    responses(
        (status = 200, description = "Log stream (SSE)")
    ),
    tag = "streams"
)]
pub async fn stream_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<LogsQueryParams>,
) -> Sse<impl TokioStream<Item = Result<Event, Infallible>>> {
    // Permission check (note: can't use ? operator in SSE handler, so logging here)
    if require_permission(&claims, Permission::TelemetryView).is_err() {
        warn!("Unauthorized access to log stream");
    }

    let mut filters_for_stream = match normalize_log_filters(&params) {
        Ok(filters) => filters.realtime,
        Err(err) => {
            warn!(
                error = %err.message,
                "invalid log stream filters provided; defaulting to unfiltered stream"
            );
            NormalizedLogFilters::default()
        }
    };

    if filters_for_stream.tenant_id.is_none() {
        filters_for_stream.tenant_id = Some(claims.tenant_id.clone());
    }

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

#[utoipa::path(
    get,
    path = "/v1/traces/search",
    params(
        ("span_name" = Option<String>, Query, description = "Filter by span operation name"),
        ("status" = Option<String>, Query, description = "Filter by span status (ok, error, unset)"),
        ("start_time_ns" = Option<u64>, Query, description = "Start time in nanoseconds"),
        ("end_time_ns" = Option<u64>, Query, description = "End time in nanoseconds"),
    ),
    responses(
        (status = 200, description = "List of matching trace IDs", body = Vec<String>),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "traces",
    security(("bearer_token" = []))
)]
pub async fn search_traces(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<TracesSearchQuery>,
) -> Result<Json<Vec<String>>, (StatusCode, Json<crate::types::ErrorResponse>)> {
    require_permission(&claims, Permission::TelemetryView).map_err(|_| {
        (
            axum::http::StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        )
    })?;

    // Parse status parameter
    let status = params.status.as_ref().and_then(|s| match s.as_str() {
        "ok" | "OK" => Some(SpanStatus::Ok),
        "error" | "ERROR" => Some(SpanStatus::Error),
        "unset" | "UNSET" => Some(SpanStatus::Unset),
        _ => None,
    });

    // Create search query with tenant isolation
    let query = TraceSearchQuery {
        tenant_id: Some(claims.tenant_id.clone()),
        span_name: params.span_name.clone(),
        status,
        start_time_ns: params.start_time_ns,
        end_time_ns: params.end_time_ns,
    };

    // Search traces in the trace buffer
    let trace_ids = state.trace_buffer.search(&query);
    Ok(Json(trace_ids))
}

#[utoipa::path(
    get,
    path = "/v1/traces/{trace_id}",
    params(
        ("trace_id" = String, Path, description = "Trace ID to retrieve"),
    ),
    responses(
        (status = 200, description = "Trace event details or null if not found"),
        (status = 500, description = "Internal server error"),
    ),
    tag = "traces",
    security(("bearer_token" = []))
)]
pub async fn get_trace(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(trace_id): Path<String>,
) -> Result<
    Json<Option<crate::telemetry::TraceEvent>>,
    (StatusCode, Json<crate::types::ErrorResponse>),
> {
    require_permission(&claims, Permission::TelemetryView).map_err(|_| {
        (
            axum::http::StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        )
    })?;

    let trace_id = crate::id_resolver::resolve_any_id(&state.db, &trace_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    // Get trace from the trace buffer with tenant isolation
    let trace = state
        .trace_buffer
        .get_trace_for_tenant(&trace_id, &claims.tenant_id);
    Ok(Json(trace))
}

fn parse_trace_timestamp(timestamp: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S") {
        return Some(chrono::Utc.from_utc_datetime(&naive));
    }
    None
}

#[utoipa::path(
    get,
    path = "/v1/traces/inference/{trace_id}",
    params(
        ("trace_id" = String, Path, description = "Inference trace ID"),
        ("tokens_after" = Option<u32>, Query, description = "Return tokens with index > tokens_after"),
        ("tokens_limit" = Option<u32>, Query, description = "Max token decisions to return (0 = no cap)"),
    ),
    responses(
        (status = 200, description = "Inference trace detail", body = InferenceTraceDetailResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Trace not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "traces",
    security(("bearer_token" = []))
)]
pub async fn get_inference_trace_detail(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(trace_id): Path<String>,
    Query(params): Query<InferenceTraceDetailQueryParams>,
) -> Result<Json<InferenceTraceDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TelemetryView).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        )
    })?;

    let trace_id = crate::id_resolver::resolve_any_id(&state.db, &trace_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let record = get_inference_trace_detail_for_tenant(
        &state.db,
        &claims.tenant_id,
        &trace_id,
        params.tokens_after,
        params.tokens_limit,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Database error loading trace: {e}"
            ))),
        )
    })?;

    let Some(record) = record else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("trace not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(trace_id),
            ),
        ));
    };

    let mut adapters_used = Vec::new();
    let mut seen = HashSet::new();
    let mut backend_id: Option<String> = None;

    let token_decisions = record
        .tokens
        .into_iter()
        .map(|token| {
            for adapter in &token.adapter_ids {
                if seen.insert(adapter.clone()) {
                    adapters_used.push(adapter.clone());
                }
            }
            if backend_id.is_none() {
                backend_id = token.backend_id.clone();
            }

            TokenDecision {
                token_index: token.token_index,
                token_id: None,
                adapter_ids: token.adapter_ids,
                gates_q15: token.gates_q15,
                entropy: 0.0,
                decision_hash: Some(hex::encode(token.decision_hash)),
                backend_id: token.backend_id,
                kernel_version_id: token.kernel_version_id,
            }
        })
        .collect::<Vec<_>>();

    let latency_ms = record
        .receipt
        .as_ref()
        .and_then(|receipt| {
            let start = parse_trace_timestamp(&record.created_at)?;
            let end = receipt
                .created_at
                .as_deref()
                .and_then(parse_trace_timestamp)?;
            let delta = end.signed_duration_since(start).num_milliseconds();
            if delta < 0 {
                None
            } else {
                Some(delta as u64)
            }
        })
        .unwrap_or(0);

    let timing_breakdown = TimingBreakdown {
        total_ms: latency_ms,
        routing_ms: 0,
        inference_ms: latency_ms,
        policy_ms: 0,
        prefill_ms: None,
        decode_ms: None,
    };

    let receipt = record.receipt.map(|receipt| TraceReceiptSummary {
        receipt_digest: hex::encode(receipt.receipt_digest),
        run_head_hash: hex::encode(receipt.run_head_hash),
        output_digest: hex::encode(receipt.output_digest),
        logical_prompt_tokens: receipt.logical_prompt_tokens,
        logical_output_tokens: receipt.logical_output_tokens,
        stop_reason_code: receipt.stop_reason_code,
        stop_reason_token_index: receipt.stop_reason_token_index,
        verified: receipt.receipt_parity_verified.unwrap_or(false),
        processor_id: receipt.processor_id,
        engine_version: receipt.engine_version,
        ane_version: receipt.ane_version,
        prefix_cache_hit: receipt.prefix_cache_hit,
        prefix_kv_bytes: receipt.prefix_kv_bytes,
    });

    Ok(Json(InferenceTraceDetailResponse {
        trace_id: record.trace_id,
        request_id: record.request_id,
        created_at: record.created_at,
        latency_ms,
        adapters_used,
        stack_id: record.stack_id,
        model_id: record.model_id,
        policy_id: record.policy_id,
        token_decisions,
        token_decisions_next_cursor: record.token_decisions_next_cursor,
        token_decisions_has_more: record.token_decisions_has_more,
        timing_breakdown,
        receipt,
        backend_id,
    }))
}

/// UI-only inference trace detail endpoint with extended receipt data.
pub async fn get_ui_inference_trace_detail(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(trace_id): Path<String>,
    Query(params): Query<InferenceTraceDetailQueryParams>,
) -> Result<Json<UiInferenceTraceDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TelemetryView).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        )
    })?;

    let trace_id = crate::id_resolver::resolve_any_id(&state.db, &trace_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let record = get_inference_trace_detail_for_tenant(
        &state.db,
        &claims.tenant_id,
        &trace_id,
        params.tokens_after,
        normalize_ui_tokens_limit(params.tokens_limit),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Database error loading trace: {e}"
            ))),
        )
    })?;

    let Some(record) = record else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("trace not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(trace_id),
            ),
        ));
    };

    let mut adapters_used = Vec::new();
    let mut seen = HashSet::new();
    let mut backend_id: Option<String> = None;

    let token_decisions = record
        .tokens
        .into_iter()
        .map(|token| {
            for adapter in &token.adapter_ids {
                if seen.insert(adapter.clone()) {
                    adapters_used.push(adapter.clone());
                }
            }
            if backend_id.is_none() {
                backend_id = token.backend_id.clone();
            }

            TokenDecision {
                token_index: token.token_index,
                token_id: None,
                adapter_ids: token.adapter_ids,
                gates_q15: token.gates_q15,
                entropy: 0.0,
                decision_hash: Some(hex::encode(token.decision_hash)),
                backend_id: token.backend_id,
                kernel_version_id: token.kernel_version_id,
            }
        })
        .collect::<Vec<_>>();

    let latency_ms = record
        .receipt
        .as_ref()
        .and_then(|receipt| {
            let start = parse_trace_timestamp(&record.created_at)?;
            let end = receipt
                .created_at
                .as_deref()
                .and_then(parse_trace_timestamp)?;
            let delta = end.signed_duration_since(start).num_milliseconds();
            if delta < 0 {
                None
            } else {
                Some(delta as u64)
            }
        })
        .unwrap_or(0);

    let timing_breakdown = TimingBreakdown {
        total_ms: latency_ms,
        routing_ms: 0,
        inference_ms: latency_ms,
        policy_ms: 0,
        prefill_ms: None,
        decode_ms: None,
    };

    let receipt = record.receipt.map(|receipt| UiTraceReceiptSummary {
        receipt_digest: hex::encode(receipt.receipt_digest),
        run_head_hash: hex::encode(receipt.run_head_hash),
        output_digest: hex::encode(receipt.output_digest),
        input_digest_b3: receipt.input_digest_b3.map(hex::encode),
        seed_lineage_hash: receipt.seed_lineage_hash.map(hex::encode),
        backend_attestation_b3: receipt.backend_attestation_b3.map(hex::encode),
        logical_prompt_tokens: receipt.logical_prompt_tokens,
        logical_output_tokens: receipt.logical_output_tokens,
        stop_reason_code: receipt.stop_reason_code,
        stop_reason_token_index: receipt.stop_reason_token_index,
        verified: receipt.receipt_parity_verified,
        processor_id: receipt.processor_id,
        engine_version: receipt.engine_version,
        ane_version: receipt.ane_version,
        prefix_cache_hit: Some(receipt.prefix_cache_hit),
        prefix_kv_bytes: Some(receipt.prefix_kv_bytes),
        // adapter_training_digests is not yet available in the DB record
        adapter_training_digests: None,
    });

    Ok(Json(UiInferenceTraceDetailResponse {
        trace_id: record.trace_id,
        request_id: record.request_id,
        created_at: record.created_at,
        latency_ms,
        adapters_used,
        stack_id: record.stack_id,
        model_id: record.model_id,
        policy_id: record.policy_id,
        token_decisions,
        token_decisions_next_cursor: record.token_decisions_next_cursor,
        token_decisions_has_more: record.token_decisions_has_more,
        timing_breakdown,
        receipt,
        backend_id,
    }))
}

#[utoipa::path(
    get,
    path = "/v1/traces/inference",
    params(
        ("request_id" = Option<String>, Query, description = "Filter by request ID"),
        ("limit" = Option<usize>, Query, description = "Max results (default 50, max 500)"),
    ),
    responses(
        (status = 200, description = "Inference trace summaries", body = Vec<InferenceTraceResponse>),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    ),
    tag = "traces",
    security(("bearer_token" = []))
)]
pub async fn list_inference_traces(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<InferenceTracesQueryParams>,
) -> Result<Json<Vec<InferenceTraceResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::TelemetryView).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        )
    })?;

    let records = list_inference_traces_for_tenant(
        &state.db,
        &claims.tenant_id,
        params.request_id.as_deref(),
        params.limit,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(format!(
                "Database error loading inference traces: {e}"
            ))),
        )
    })?;

    let responses = records
        .into_iter()
        .map(|record| {
            let latency_ms = record
                .receipt_created_at
                .as_deref()
                .and_then(|end| {
                    let start = parse_trace_timestamp(&record.created_at)?;
                    let end = parse_trace_timestamp(end)?;
                    let delta = end.signed_duration_since(start).num_milliseconds();
                    if delta < 0 {
                        None
                    } else {
                        Some(delta as u64)
                    }
                })
                .unwrap_or(0);

            InferenceTraceResponse {
                trace_id: record.trace_id,
                request_id: record.request_id,
                created_at: record.created_at,
                latency_ms,
                token_count: record.token_count,
                adapters_used: record.adapters_used,
                finish_reason: record.stop_reason_code,
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(responses))
}

#[derive(Clone, Debug, Default)]
pub struct NormalizedLogFilters {
    pub tenant_id: Option<String>,
    pub event_type: Option<String>,
    pub level: Option<LogLevel>,
    pub component: Option<String>,
    pub trace_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ParsedLogFilters {
    pub telemetry: TelemetryFilters,
    pub realtime: NormalizedLogFilters,
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
        if &event.identity.tenant_id != tenant {
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
        (status = 200, description = "Recent activity events")
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn get_recent_activity(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<RecentActivityQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
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

    Ok(Json(serde_json::json!(events)))
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

    let telemetry_filters = TelemetryFilters {
        tenant_id: Some(tenant_id.to_string()),
        event_type: raw_event_types.first().cloned(),
        limit: Some((limit * 2).clamp(1, 200)),
        ..Default::default()
    };

    let telemetry_events = state
        .telemetry_buffer
        .query(&telemetry_filters)
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid telemetry filters")
                        .with_code("BAD_REQUEST")
                        .with_string_details(err.to_string()),
                ),
            )
        })?;
    for event in telemetry_events {
        if !event_type_matches(&event.event_type, event_type_filter) {
            continue;
        }
        let response = convert_unified_event(&event);
        if dedupe.insert(response.target_id.clone()) {
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
        if dedupe.insert(response.target_id.clone()) {
            events.push(response);
        }
    }

    events.sort_by(|a, b| parse_timestamp(&b.created_at).cmp(&parse_timestamp(&a.created_at)));

    Ok(events)
}

fn event_type_matches(event_type: &str, filter: Option<&HashSet<String>>) -> bool {
    match filter {
        Some(allowed) => allowed.contains(&event_type.to_ascii_lowercase()),
        None => true,
    }
}

fn convert_unified_event(event: &UnifiedTelemetryEvent) -> ActivityEventResponse {
    // Construct metadata
    let mut meta_obj = serde_json::Map::new();
    meta_obj.insert(
        "level".to_string(),
        Value::String(format!("{:?}", event.level).to_ascii_lowercase()),
    );
    meta_obj.insert("message".to_string(), Value::String(event.message.clone()));

    if let Some(comp) = &event.component {
        meta_obj.insert("component".to_string(), Value::String(comp.clone()));
    }

    if let Some(md) = &event.metadata {
        meta_obj.insert("details".to_string(), md.clone());
    }

    // Attempt to extract workspace_id from metadata if present
    let workspace_id = event
        .metadata
        .as_ref()
        .and_then(|m| m.get("workspace_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    ActivityEventResponse {
        id: event.id.clone(),
        tenant_id: event.identity.tenant_id.clone(),
        user_id: event
            .user_id
            .clone()
            .unwrap_or_else(|| "system".to_string()),
        event_type: event.event_type.clone(),
        workspace_id,
        target_type: event.component.clone(),
        target_id: Some(event.id.clone()), // Use event ID as proxy fallback
        metadata_json: serde_json::to_string(&Value::Object(meta_obj)).ok(),
        created_at: event.timestamp.to_rfc3339(),
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

    // Construct metadata
    let mut meta_obj = serde_json::Map::new();
    meta_obj.insert("message".to_string(), Value::String(message));
    meta_obj.insert("level".to_string(), Value::String("info".to_string()));
    if let Some(md) = metadata {
        meta_obj.insert("details".to_string(), md);
    }

    ActivityEventResponse {
        id: event.id,
        tenant_id: event.tenant_id,
        user_id: event.user_id,
        event_type: event.event_type,
        workspace_id: event.workspace_id,
        target_type: event.target_type,
        target_id: event.target_id,
        metadata_json: serde_json::to_string(&Value::Object(meta_obj)).ok(),
        created_at: timestamp.to_rfc3339(),
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
