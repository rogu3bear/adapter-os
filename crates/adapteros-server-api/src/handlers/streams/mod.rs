//! SSE streaming handlers with reliable replay support
//!
//! This module provides Server-Sent Events (SSE) endpoints for real-time
//! data streaming including system metrics, telemetry, adapters, training,
//! alerts, anomalies, and dashboard metrics.
//!
//! All streams support:
//! - Monotonic event IDs for ordering
//! - Last-Event-ID header for reconnection replay
//! - Ring buffer storage for missed event recovery

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::check_tenant_access;
use crate::sse::{EventGapRecoveryHint, SseErrorEvent, SseEventManager, SseStreamType};
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{
        sse::{Event, KeepAlive, KeepAliveStream, Sse},
        IntoResponse,
    },
    Extension,
};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt as FuturesStreamExt;
use serde::Deserialize;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

/// Boxed SSE stream type for unified returns with keep-alive
type BoxedSseStream = std::pin::Pin<Box<dyn Stream<Item = Result<Event, Infallible>> + Send>>;
type SseResponse = Sse<KeepAliveStream<BoxedSseStream>>;

const DEFAULT_DASHBOARD_CONFIG_JSON: &str = r#"{
    "widgets": [
        {
            "id": "cpu_usage",
            "type": "time_series",
            "metric": "cpu_usage",
            "aggregation": "avg",
            "window": "1h"
        },
        {
            "id": "gpu_utilization",
            "type": "gauge",
            "metric": "gpu_utilization",
            "threshold_warning": 80,
            "threshold_critical": 95
        },
        {
            "id": "active_alerts",
            "type": "alert_list",
            "severities": ["critical", "error"],
            "limit": 10
        }
    ],
    "refresh_interval": 30,
    "time_range": "24h"
}"#;

fn default_dashboard_config() -> serde_json::Value {
    serde_json::from_str(DEFAULT_DASHBOARD_CONFIG_JSON).unwrap_or_else(|_| {
        json!({
            "widgets": [],
            "refresh_interval": 30,
            "time_range": "24h"
        })
    })
}

fn extract_widgets(config: &serde_json::Value) -> Vec<serde_json::Value> {
    config
        .get("widgets")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
}

fn parse_refresh_interval(config: &serde_json::Value, fallback: u64) -> u64 {
    config
        .get("refresh_interval")
        .and_then(|v| v.as_u64())
        .unwrap_or(fallback)
}

fn widget_type_label(value: &str) -> String {
    let normalized = value.trim().to_lowercase().replace([' ', '-'], "_");

    match normalized.as_str() {
        "timeseries" => "time_series".to_string(),
        "alertlist" => "alert_list".to_string(),
        "anomalyheatmap" => "anomaly_heatmap".to_string(),
        "metriccard" => "metric_card".to_string(),
        "statusindicator" => "status_indicator".to_string(),
        other => other.to_string(),
    }
}

fn widget_type_from_value(widget: &serde_json::Value) -> String {
    widget
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| widget.get("widget_type").and_then(|v| v.as_str()))
        .map(widget_type_label)
        .unwrap_or_else(|| "unknown".to_string())
}

fn widget_id_from_value(widget: &serde_json::Value) -> String {
    widget
        .get("id")
        .and_then(|v| v.as_str())
        .or_else(|| widget.get("widget_id").and_then(|v| v.as_str()))
        .map(|id| id.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn widget_config_from_value(widget: &serde_json::Value) -> &serde_json::Value {
    widget.get("config").unwrap_or(widget)
}

async fn resolve_dashboard_config(
    state: &AppState,
    dashboard_id: &str,
    tenant_id: &str,
    user_id: &str,
) -> (serde_json::Value, u64) {
    let default_config = default_dashboard_config();
    let default_refresh = parse_refresh_interval(&default_config, 30);

    // Try process_custom_dashboards first (tenant-scoped)
    let config_row = sqlx::query(
        "SELECT dashboard_config_json, dashboard_refresh_interval_seconds \
         FROM process_custom_dashboards WHERE id = ? AND tenant_id = ?",
    )
    .bind(dashboard_id)
    .bind(tenant_id)
    .fetch_optional(state.db.pool())
    .await;

    match config_row {
        Ok(Some(row)) => {
            let config_raw: String = row.get("dashboard_config_json");
            let refresh_db: i64 = row.get("dashboard_refresh_interval_seconds");
            let mut config =
                serde_json::from_str(&config_raw).unwrap_or_else(|_| default_config.clone());
            if !config.is_object() {
                config = default_config.clone();
            }
            if extract_widgets(&config).is_empty() {
                config["widgets"] = default_config["widgets"].clone();
            }
            let refresh = if refresh_db > 0 {
                refresh_db as u64
            } else {
                parse_refresh_interval(&config, default_refresh)
            };
            config["refresh_interval"] = json!(refresh);
            return (config, refresh);
        }
        Ok(None) => {}
        Err(e) => {
            if !e.to_string().contains("no such table") {
                tracing::warn!(error = %e, "Failed to load process dashboard config");
            }
        }
    }

    // Fall back to per-user dashboard configuration
    let user_widgets = match state.db.get_dashboard_config(user_id).await {
        Ok(configs) => configs,
        Err(e) => {
            if !e.to_string().contains("no such table") {
                tracing::warn!(error = %e, "Failed to load user dashboard config");
            }
            Vec::new()
        }
    };

    if !user_widgets.is_empty() {
        let catalog = extract_widgets(&default_config);
        let mut catalog_map: HashMap<String, serde_json::Value> = HashMap::new();
        for widget in catalog {
            let widget_id = widget_id_from_value(&widget);
            if widget_id != "unknown" {
                catalog_map.insert(widget_id, widget);
            }
        }

        let mut selected = Vec::new();
        for widget in user_widgets {
            if !widget.enabled {
                continue;
            }
            if let Some(definition) = catalog_map.get(&widget.widget_id) {
                selected.push(definition.clone());
            } else {
                tracing::warn!(
                    widget_id = %widget.widget_id,
                    "Dashboard widget not found in default catalog"
                );
            }
        }

        if !selected.is_empty() {
            let mut config = default_config.clone();
            config["widgets"] = serde_json::Value::Array(selected);
            let refresh = parse_refresh_interval(&config, default_refresh);
            config["refresh_interval"] = json!(refresh);
            return (config, refresh);
        }
    }

    let mut config = default_config.clone();
    config["refresh_interval"] = json!(default_refresh);
    (config, default_refresh)
}

/// Helper to create SSE response from any stream with keep-alive
fn sse_response<S>(stream: S) -> SseResponse
where
    S: Stream<Item = Result<Event, Infallible>> + Send + 'static,
{
    Sse::new(Box::pin(stream) as BoxedSseStream).keep_alive(KeepAlive::default())
}

/// Query parameters for stream endpoints
#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub tenant: Option<String>,
}

/// Helper to create replay stream from Last-Event-ID
fn create_replay_stream(
    events: Vec<crate::sse::SseEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
    stream::iter(
        events
            .into_iter()
            .map(|e| Ok(SseEventManager::to_axum_event(&e))),
    )
}

/// SSE stream for system metrics
/// Pushes SystemMetrics every 5 seconds with monotonic IDs and replay support
#[utoipa::path(
    get,
    path = "/v1/stream/metrics",
    responses(
        (status = 200, description = "System metrics stream (SSE)")
    ),
    tag = "streams"
)]
pub async fn system_metrics_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    let has_permission = require_permission(&claims, Permission::MetricsView).is_ok();

    if !has_permission {
        tracing::warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for system metrics stream"
        );
        let event = Event::default()
            .event("error")
            .data("{\"error\": \"Permission denied - MetricsView required\"}");
        return sse_response(stream::iter(vec![Ok(event)]));
    }

    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    let mut gap_events: Vec<Result<Event, Infallible>> = Vec::new();
    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        let result = sse_manager
            .get_replay_with_analysis(SseStreamType::SystemMetrics, last_id)
            .await;

        // Log gap warning if events were lost
        if result.has_gap {
            let stats = sse_manager.get_stats(SseStreamType::SystemMetrics);
            let oldest_available_id = stats.map(|s| s.lowest_id).unwrap_or(0);
            let gap_event = SseErrorEvent::gap_detected(
                last_id,
                oldest_available_id,
                result.dropped_count,
                EventGapRecoveryHint::RefetchFullState,
            );
            let gap_json = serde_json::to_string(&gap_event).unwrap_or_else(|_| "{}".to_string());
            gap_events.push(Ok(Event::default().event("error").data(gap_json)));
            tracing::warn!(
                last_id = last_id,
                dropped = result.dropped_count,
                "SSE client reconnected with gap in SystemMetrics stream"
            );
        }
        result.events
    } else {
        Vec::new()
    };

    // Create replay stream
    let gap_stream = stream::iter(gap_events);
    let replay_stream = FuturesStreamExt::chain(gap_stream, create_replay_stream(replay_events));

    // Create live stream
    let live_stream = stream::unfold(state.clone(), move |state| {
        let mgr = state.sse_manager.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(5)).await;

            // Fetch metrics
            let metrics = match get_system_metrics_internal(&state).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Failed to fetch metrics for SSE: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::SystemMetrics, &e)
                        .await;
                    return Some((Ok(SseEventManager::to_axum_event(&event)), state));
                }
            };

            let json = match serde_json::to_string(&metrics) {
                Ok(j) => j,
                Err(e) => {
                    tracing::warn!("Failed to serialize metrics: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::SystemMetrics, "serialization failed")
                        .await;
                    return Some((Ok(SseEventManager::to_axum_event(&event)), state));
                }
            };

            // Create event with monotonic ID
            let event = mgr
                .create_event(SseStreamType::SystemMetrics, "metrics", json)
                .await;

            Some((Ok(SseEventManager::to_axum_event(&event)), state))
        }
    });

    // Chain replay with live stream
    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// SSE stream for telemetry events
/// Streams telemetry events in real-time via broadcast channel with replay support
#[utoipa::path(
    get,
    path = "/v1/stream/telemetry",
    responses(
        (status = 200, description = "Telemetry events stream (SSE)")
    ),
    tag = "streams"
)]
pub async fn telemetry_events_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    let has_permission = require_permission(&claims, Permission::TelemetryView).is_ok();

    if !has_permission {
        tracing::warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for telemetry events stream"
        );
        let event = Event::default()
            .event("error")
            .data("{\"error\": \"Permission denied - TelemetryView required\"}");
        return sse_response(stream::iter(vec![Ok(event)]));
    }

    let sse_manager = state.sse_manager.clone();
    let tenant_id = claims.tenant_id.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Telemetry, last_id)
            .await
            .into_iter()
            .filter(|event| {
                if event.event_type == "telemetry" {
                    match serde_json::from_str::<
                        adapteros_telemetry::unified_events::TelemetryEvent,
                    >(&event.data)
                    {
                        Ok(parsed) => parsed.identity.tenant_id == tenant_id,
                        Err(err) => {
                            tracing::warn!(
                                event_id = event.id,
                                error = %err,
                                "Failed to parse telemetry replay event for tenant filtering"
                            );
                            false
                        }
                    }
                } else {
                    true
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    // Subscribe to the telemetry broadcast channel for real-time events
    let receiver = state.telemetry_tx.subscribe();

    let next_keepalive = tokio::time::Instant::now() + Duration::from_secs(30);
    let live_stream = stream::unfold(
        (receiver, state.clone(), tenant_id, next_keepalive),
        move |(mut rx, state, tenant_id, mut next_keepalive)| async move {
            let mgr = state.sse_manager.clone();

            loop {
                tokio::select! {
                    biased;
                    _ = tokio::time::sleep_until(next_keepalive) => {
                        let buffer_len = state.telemetry_buffer.len().await;
                        let health_json = serde_json::json!({
                            "status": "keepalive",
                            "buffer_size": buffer_len
                        }).to_string();

                        let event = mgr
                            .create_event(SseStreamType::Telemetry, "keepalive", health_json)
                            .await;
                        next_keepalive = tokio::time::Instant::now() + Duration::from_secs(30);

                        return Some((Ok(SseEventManager::to_axum_event(&event)), (rx, state, tenant_id, next_keepalive)));
                    }
                    result = rx.recv() => {
                        match result {
                            Ok(telemetry_event) => {
                                if telemetry_event.identity.tenant_id != tenant_id {
                                    continue;
                                }

                                let json = match serde_json::to_string(&telemetry_event) {
                                    Ok(j) => j,
                                    Err(e) => {
                                        tracing::warn!("Failed to serialize telemetry event: {}", e);
                                        let event = mgr
                                            .create_error_event(SseStreamType::Telemetry, &format!("serialization failed: {}", e))
                                            .await;
                                        next_keepalive = tokio::time::Instant::now() + Duration::from_secs(30);
                                        return Some((Ok(SseEventManager::to_axum_event(&event)), (rx, state, tenant_id, next_keepalive)));
                                    }
                                };

                                let event = mgr
                                    .create_event(SseStreamType::Telemetry, "telemetry", json)
                                    .await;
                                next_keepalive = tokio::time::Instant::now() + Duration::from_secs(30);

                                return Some((Ok(SseEventManager::to_axum_event(&event)), (rx, state, tenant_id, next_keepalive)));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                                tracing::warn!(lagged_count = count, "Telemetry SSE client lagged behind");
                                let data = serde_json::json!({ "lagged_events": count }).to_string();
                                let event = mgr
                                    .create_event(SseStreamType::Telemetry, "warning", data)
                                    .await;
                                next_keepalive = tokio::time::Instant::now() + Duration::from_secs(30);
                                return Some((Ok(SseEventManager::to_axum_event(&event)), (rx, state, tenant_id, next_keepalive)));
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                                tracing::info!("Telemetry broadcast channel closed");
                                return None;
                            }
                        }
                    }
                }
            }
        },
    );

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// SSE stream for adapter state transitions
/// Streams adapter lifecycle events with replay support
#[utoipa::path(
    get,
    path = "/v1/stream/adapters",
    responses(
        (status = 200, description = "Adapter state stream (SSE)")
    ),
    tag = "streams"
)]
pub async fn adapter_state_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    let has_permission = require_permission(&claims, Permission::AdapterView).is_ok();

    if !has_permission {
        tracing::warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for adapter state stream"
        );
        let event = Event::default()
            .event("error")
            .data("{\"error\": \"Permission denied - AdapterView required\"}");
        return sse_response(stream::iter(vec![Ok(event)]));
    }

    let sse_manager = state.sse_manager.clone();
    let tenant_id = claims.tenant_id.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::AdapterState, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    let live_stream = stream::unfold(
        (state.clone(), tenant_id),
        move |(state, tenant_id)| async move {
            let mgr = state.sse_manager.clone();

            tokio::time::sleep(Duration::from_secs(3)).await;

            // Fetch all adapters
            let adapters = match state.db.list_adapters_for_tenant(&tenant_id).await {
                Ok(a) => a,
                Err(e) => {
                    tracing::warn!("Failed to fetch adapters for SSE: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::AdapterState, &e.to_string())
                        .await;
                    return Some((
                        Ok(SseEventManager::to_axum_event(&event)),
                        (state, tenant_id),
                    ));
                }
            };

            let json = match serde_json::to_string(&adapters) {
                Ok(j) => j,
                Err(e) => {
                    tracing::warn!("Failed to serialize adapters: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::AdapterState, "serialization failed")
                        .await;
                    return Some((
                        Ok(SseEventManager::to_axum_event(&event)),
                        (state, tenant_id),
                    ));
                }
            };

            let event = mgr
                .create_event(SseStreamType::AdapterState, "adapters", json)
                .await;

            Some((
                Ok(SseEventManager::to_axum_event(&event)),
                (state, tenant_id),
            ))
        },
    );

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// SSE stream for worker status updates
///
/// Streams worker snapshots with replay support.
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/stream/workers",
    params(
        ("tenant" = Option<String>, Query, description = "Tenant ID for filtering events (defaults to caller tenant)")
    ),
    responses(
        (status = 200, description = "SSE stream of worker status updates")
    )
)]
pub async fn workers_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<StreamQuery>,
    headers: HeaderMap,
) -> SseResponse {
    let has_permission = require_permission(&claims, Permission::WorkerView).is_ok();

    if !has_permission {
        tracing::warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for worker stream"
        );
        let event = Event::default()
            .event("error")
            .data("{\"error\": \"Permission denied - WorkerView required\"}");
        return sse_response(stream::iter(vec![Ok(event)]));
    }

    let tenant_id = params
        .tenant
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());
    if !check_tenant_access(&claims, &tenant_id) {
        tracing::warn!(
            user_id = %claims.sub,
            user_tenant = %claims.tenant_id,
            requested_tenant = %tenant_id,
            "Worker stream tenant access denied"
        );
        let event = Event::default()
            .event("error")
            .data("{\"error\": \"Access denied for tenant worker stream\"}");
        return sse_response(stream::iter(vec![Ok(event)]));
    }

    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Workers, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    let live_stream = stream::unfold(
        (state.clone(), tenant_id),
        move |(state, tenant_id)| async move {
            let mgr = state.sse_manager.clone();

            tokio::time::sleep(Duration::from_secs(10)).await;

            let workers = match state.db.list_workers_by_tenant(&tenant_id).await {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("Failed to fetch workers for SSE: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::Workers, &e.to_string())
                        .await;
                    return Some((
                        Ok(SseEventManager::to_axum_event(&event)),
                        (state, tenant_id),
                    ));
                }
            };

            let response: Vec<WorkerResponse> = workers
                .into_iter()
                .map(|w| WorkerResponse {
                    schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                    id: w.id,
                    tenant_id: w.tenant_id,
                    node_id: w.node_id,
                    plan_id: w.plan_id,
                    uds_path: w.uds_path,
                    pid: w.pid,
                    status: w.status.clone(),
                    started_at: w.started_at,
                    last_seen_at: w.last_seen_at,
                    capabilities: w
                        .capabilities_json
                        .as_ref()
                        .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
                        .unwrap_or_default(),
                    capabilities_detail: w
                        .capabilities_json
                        .as_ref()
                        .and_then(|json| serde_json::from_str(json).ok()),
                    backend: w.backend.clone(),
                    model_id: None,
                    model_hash: w.model_hash_b3.clone(),
                    tokenizer_hash_b3: w.tokenizer_hash_b3.clone(),
                    tokenizer_vocab_size: w.tokenizer_vocab_size.map(|v| v as u32),
                    coreml_failure_stage: None,
                    coreml_failure_reason: None,
                    model_loaded: w.model_hash_b3.is_some(),
                    cache_used_mb: None,
                    cache_max_mb: None,
                    cache_pinned_entries: None,
                    cache_active_entries: None,
                    display_name: None,
                })
                .collect();

            let json = match serde_json::to_string(&serde_json::json!({ "workers": response })) {
                Ok(j) => j,
                Err(e) => {
                    tracing::warn!("Failed to serialize workers: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::Workers, "serialization failed")
                        .await;
                    return Some((
                        Ok(SseEventManager::to_axum_event(&event)),
                        (state, tenant_id),
                    ));
                }
            };

            let event = mgr
                .create_event(SseStreamType::Workers, "workers", json)
                .await;

            Some((
                Ok(SseEventManager::to_axum_event(&event)),
                (state, tenant_id),
            ))
        },
    );

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// Training stream SSE endpoint
///
/// Streams real-time training events including adapter lifecycle transitions,
/// promotion/demotion events, profiler metrics, and K reduction events.
///
/// Events are sent as Server-Sent Events (SSE) with the following format:
/// ```text
/// id: 42
/// event: training
/// retry: 3000
/// data: {"type":"adapter_promoted","timestamp":...,"payload":{...}}
/// ```
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/streams/training",
    params(
        ("tenant" = Option<String>, Query, description = "Tenant ID for filtering events (defaults to caller tenant)")
    ),
    responses(
        (status = 200, description = "SSE stream of training events")
    )
)]
pub async fn training_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<StreamQuery>,
    headers: HeaderMap,
) -> SseResponse {
    let tenant_id = params
        .tenant
        .clone()
        .unwrap_or_else(|| claims.tenant_id.clone());
    if !check_tenant_access(&claims, &tenant_id) {
        tracing::warn!(
            user_id = %claims.sub,
            user_tenant = %claims.tenant_id,
            requested_tenant = %tenant_id,
            "Training stream tenant access denied"
        );
        let event = Event::default()
            .event("error")
            .data("{\"error\": \"Access denied for tenant training stream\"}");
        return sse_response(stream::iter(vec![Ok(event)]));
    }

    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Training, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    // Subscribe to the training signal broadcast channel
    let rx = state.training_signal_tx.subscribe();

    // Convert the broadcast receiver into a stream that filters by tenant
    // Use FuturesStreamExt::filter_map explicitly for async closure support
    let mgr_for_signals = Arc::new(state.sse_manager.clone());
    let signal_stream = FuturesStreamExt::filter_map(BroadcastStream::new(rx), move |result| {
        let tenant_filter = tenant_id.clone();
        let mgr = Arc::clone(&mgr_for_signals);
        async move {
            match result {
                Ok(signal) => {
                    // Filter signals by tenant_id if present in payload
                    let signal_tenant = signal
                        .payload
                        .get("tenant_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    // Pass through if tenant matches or if no tenant filter in signal
                    if signal_tenant.is_empty() || signal_tenant == tenant_filter {
                        let event_data = serde_json::json!({
                            "type": signal.signal_type.to_string(),
                            "timestamp": signal.timestamp,
                            "priority": format!("{:?}", signal.priority),
                            "payload": signal.payload,
                            "trace_id": signal.trace_id,
                        });

                        let event = mgr
                            .create_event(
                                SseStreamType::Training,
                                "training",
                                event_data.to_string(),
                            )
                            .await;

                        Some(Ok(SseEventManager::to_axum_event(&event)))
                    } else {
                        None
                    }
                }
                Err(e) => {
                    tracing::debug!("Broadcast stream error (likely lag): {}", e);
                    None
                }
            }
        }
    });

    // Also include a periodic heartbeat to keep connection alive
    let mgr_for_heartbeat = state.sse_manager.clone();
    let heartbeat_stream = stream::unfold(0u64, move |counter| {
        let mgr = mgr_for_heartbeat.clone();
        async move {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let event_data = serde_json::json!({
                "type": "heartbeat",
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
                "sequence": counter,
            });

            let event = mgr
                .create_event(SseStreamType::Training, "training", event_data.to_string())
                .await;

            Some((Ok(SseEventManager::to_axum_event(&event)), counter + 1))
        }
    });

    // Merge the signal stream with heartbeat stream
    let merged_stream = futures_util::stream::select(signal_stream, heartbeat_stream);

    // Chain replay with merged stream
    sse_response(FuturesStreamExt::chain(replay_stream, merged_stream))
}

/// SSE stream for alerts
/// Pushes real-time alerts as they are created or updated with replay support
pub async fn alerts_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Alerts, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    let live_stream = stream::unfold(state.clone(), move |state| async move {
        let mgr = state.sse_manager.clone();

        tokio::time::sleep(Duration::from_secs(2)).await;

        // Fetch recent alerts
        let filters = adapteros_system_metrics::AlertFilters {
            tenant_id: None,
            worker_id: None,
            status: None,
            severity: None,
            start_time: Some(chrono::Utc::now() - chrono::Duration::minutes(5)),
            end_time: None,
            limit: Some(50),
            offset: None,
        };

        let alerts =
            match adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters).await {
                Ok(alerts) => alerts,
                Err(e) => {
                    tracing::warn!("Failed to fetch alerts for SSE: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::Alerts, &e.to_string())
                        .await;
                    return Some((Ok(SseEventManager::to_axum_event(&event)), state));
                }
            };

        let alert_data = serde_json::json!({
            "alerts": alerts.iter().map(|a| adapteros_system_metrics::AlertResponse::from(a.clone())).collect::<Vec<_>>(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "count": alerts.len()
        });

        let event = mgr
            .create_event(
                SseStreamType::Alerts,
                "alerts",
                serde_json::to_string(&alert_data).unwrap_or_else(|_| "{}".to_string()),
            )
            .await;

        Some((Ok(SseEventManager::to_axum_event(&event)), state))
    });

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// SSE stream for anomalies
/// Pushes real-time anomaly detections with replay support
pub async fn anomalies_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Anomalies, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    let live_stream = stream::unfold(state.clone(), move |state| async move {
        let mgr = state.sse_manager.clone();

        tokio::time::sleep(Duration::from_secs(10)).await;

        // Fetch recent anomalies
        let filters = adapteros_system_metrics::AnomalyFilters {
            tenant_id: None,
            worker_id: None,
            status: None,
            anomaly_type: None,
            start_time: Some(chrono::Utc::now() - chrono::Duration::minutes(10)),
            end_time: None,
            limit: Some(20),
            offset: None,
        };

        let anomalies =
            match adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), filters).await {
                Ok(anomalies) => anomalies,
                Err(e) => {
                    tracing::warn!("Failed to fetch anomalies for SSE: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::Anomalies, &e.to_string())
                        .await;
                    return Some((Ok(SseEventManager::to_axum_event(&event)), state));
                }
            };

        let anomaly_data = serde_json::json!({
            "anomalies": anomalies.iter().map(|a| adapteros_system_metrics::AnomalyResponse::from(a.clone())).collect::<Vec<_>>(),
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "count": anomalies.len()
        });

        let event = mgr
            .create_event(
                SseStreamType::Anomalies,
                "anomalies",
                serde_json::to_string(&anomaly_data).unwrap_or_else(|_| "{}".to_string()),
            )
            .await;

        Some((Ok(SseEventManager::to_axum_event(&event)), state))
    });

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// SSE stream for dashboard-specific metrics
/// Pushes metrics tailored for dashboard widgets with replay support
pub async fn dashboard_metrics_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(dashboard_id): Path<String>,
    headers: HeaderMap,
) -> SseResponse {
    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Dashboard, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    let tenant_id = claims.tenant_id.clone();
    let user_id = claims.sub.clone();

    let live_stream = stream::unfold(
        (state.clone(), dashboard_id, tenant_id, user_id),
        move |(state, dashboard_id, tenant_id, user_id)| async move {
            let mgr = state.sse_manager.clone();

            tokio::time::sleep(Duration::from_secs(5)).await;

            let (dashboard_config, refresh_interval) =
                resolve_dashboard_config(&state, &dashboard_id, &tenant_id, &user_id).await;

            // Fetch metrics for each widget
            let mut widget_data = Vec::new();
            let widget_config = extract_widgets(&dashboard_config);

            for widget in &widget_config {
                let widget_type = widget_type_from_value(widget);
                let widget_id = widget_id_from_value(widget);
                let config = widget_config_from_value(widget);
                let metric_name = config.get("metric").and_then(|v| v.as_str()).unwrap_or("");

                let filters = adapteros_system_metrics::MetricFilters {
                    worker_id: None,
                    tenant_id: None,
                    metric_name: Some(metric_name.to_string()),
                    start_time: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
                    end_time: None,
                    limit: Some(100),
                };

                let metrics = match adapteros_system_metrics::ProcessHealthMetric::query(
                    state.db.pool(),
                    filters,
                )
                .await
                {
                    Ok(metrics) => metrics,
                    Err(e) => {
                        tracing::warn!("Failed to fetch metrics for widget: {}", e);
                        continue;
                    }
                };

                let widget_result = match widget_type.as_str() {
                    "time_series" => {
                        let points: Vec<serde_json::Value> = metrics
                            .iter()
                            .map(|m| {
                                serde_json::json!({
                                    "timestamp": m.collected_at.to_rfc3339(),
                                    "value": m.metric_value,
                                    "worker_id": m.worker_id
                                })
                            })
                            .collect();

                        serde_json::json!({
                            "widget_id": widget_id,
                            "widget_type": "time_series",
                            "data": {
                                "metric": metric_name,
                                "points": points,
                                "aggregation": config.get("aggregation").cloned().unwrap_or_else(|| json!("avg")),
                                "window": config.get("window").cloned().unwrap_or_else(|| json!("1h"))
                            }
                        })
                    }
                    "gauge" => {
                        let current_value = metrics.last().map(|m| m.metric_value).unwrap_or(0.0);
                        let status = if current_value
                            >= config
                                .get("threshold_critical")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(95.0)
                        {
                            "critical"
                        } else if current_value
                            >= config
                                .get("threshold_warning")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(80.0)
                        {
                            "warning"
                        } else {
                            "healthy"
                        };

                        serde_json::json!({
                            "widget_id": widget_id,
                            "widget_type": "gauge",
                            "data": {
                                "metric": metric_name,
                                "current_value": current_value,
                                "threshold_warning": config.get("threshold_warning").cloned().unwrap_or_else(|| json!(80)),
                                "threshold_critical": config.get("threshold_critical").cloned().unwrap_or_else(|| json!(95)),
                                "status": status
                            }
                        })
                    }
                    "alert_list" => {
                        let alert_filters = adapteros_system_metrics::AlertFilters {
                            tenant_id: None,
                            worker_id: None,
                            status: Some(adapteros_system_metrics::AlertStatus::Active),
                            severity: None,
                            start_time: None,
                            end_time: None,
                            limit: Some(config.get("limit").and_then(|v| v.as_i64()).unwrap_or(10)),
                            offset: None,
                        };

                        let alerts = match adapteros_system_metrics::ProcessAlert::list(
                            state.db.pool(),
                            alert_filters,
                        )
                        .await
                        {
                            Ok(alerts) => alerts,
                            Err(e) => {
                                tracing::warn!("Failed to fetch alerts for widget: {}", e);
                                vec![]
                            }
                        };

                        let alert_summaries: Vec<serde_json::Value> = alerts
                            .iter()
                            .map(|a| {
                                serde_json::json!({
                                    "id": a.id,
                                    "title": a.title,
                                    "severity": a.severity.to_string(),
                                    "status": a.status.to_string(),
                                    "worker_id": a.worker_id,
                                    "created_at": a.created_at.to_rfc3339(),
                                    "acknowledged_by": a.acknowledged_by
                                })
                            })
                            .collect();

                        serde_json::json!({
                            "widget_id": widget_id,
                            "widget_type": "alert_list",
                            "data": {
                                "alerts": alert_summaries,
                                "total_count": alerts.len(),
                                "unacknowledged_count": alerts.iter().filter(|a| a.status.to_string() == "active").count()
                            }
                        })
                    }
                    _ => {
                        serde_json::json!({
                            "widget_id": widget_id,
                            "widget_type": widget_type,
                            "data": {},
                            "error": "Unknown widget type"
                        })
                    }
                };

                widget_data.push(widget_result);
            }

            let dashboard_data = serde_json::json!({
                "dashboard_id": dashboard_id.clone(),
                "widgets": widget_data,
                "widget_config": widget_config,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "refresh_interval": refresh_interval
            });

            let event = mgr
                .create_event(
                    SseStreamType::Dashboard,
                    "dashboard_metrics",
                    serde_json::to_string(&dashboard_data).unwrap_or_else(|_| "{}".to_string()),
                )
                .await;

            Some((
                Ok(SseEventManager::to_axum_event(&event)),
                (state, dashboard_id, tenant_id, user_id),
            ))
        },
    );

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// Enhanced system metrics stream with monitoring data
/// Includes GPU metrics, inference latency, active alerts count, and recent anomalies
pub async fn enhanced_system_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting (using SystemMetrics type for enhanced too)
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::SystemMetrics, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    let live_stream = stream::unfold(state.clone(), move |state| async move {
        let mgr = state.sse_manager.clone();

        tokio::time::sleep(Duration::from_secs(5)).await;

        // Fetch basic system metrics
        let metrics = match get_system_metrics_internal(&state).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Failed to fetch metrics for SSE: {}", e);
                let event = mgr
                    .create_error_event(SseStreamType::SystemMetrics, &e)
                    .await;
                return Some((Ok(SseEventManager::to_axum_event(&event)), state));
            }
        };

        // Fetch active alerts count
        let alert_filters = adapteros_system_metrics::AlertFilters {
            tenant_id: None,
            worker_id: None,
            status: Some(adapteros_system_metrics::AlertStatus::Active),
            severity: None,
            start_time: None,
            end_time: None,
            limit: Some(1),
            offset: None,
        };

        let active_alerts_count = match adapteros_system_metrics::ProcessAlert::list(
            state.db.pool(),
            alert_filters,
        )
        .await
        {
            Ok(alerts) => alerts.len(),
            Err(_) => 0,
        };

        // Fetch recent anomalies count
        let anomaly_filters = adapteros_system_metrics::AnomalyFilters {
            tenant_id: None,
            worker_id: None,
            status: Some(adapteros_system_metrics::AnomalyStatus::Detected),
            anomaly_type: None,
            start_time: Some(chrono::Utc::now() - chrono::Duration::hours(1)),
            end_time: None,
            limit: Some(1),
            offset: None,
        };

        let recent_anomalies_count =
            match adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), anomaly_filters)
                .await
            {
                Ok(anomalies) => anomalies.len(),
                Err(_) => 0,
            };

        // Fetch worker health status (workers in 'healthy' status are actively serving)
        let workers = match sqlx::query("SELECT id, status FROM workers WHERE status = 'healthy'")
            .fetch_all(state.db.pool())
            .await
        {
            Ok(workers) => workers.len(),
            Err(_) => 0,
        };

        let enhanced_metrics = serde_json::json!({
            "system_metrics": {
                "cpu_usage": metrics.cpu_usage,
                "memory_usage": metrics.memory_usage,
                "gpu_utilization": metrics.gpu_utilization,
                "gpu_memory_used": 0.0,
                "gpu_temperature": 0.0,
                "disk_usage": metrics.disk_usage,
                "network_rx": 0.0,
                "network_tx": 0.0
            },
            "monitoring_metrics": {
                "active_alerts_count": active_alerts_count,
                "recent_anomalies_count": recent_anomalies_count,
                "active_workers_count": workers,
                "inference_latency_p95": 0.0,
                "active_inference_sessions": 0,
                "adapter_swap_latency": 0.0,
            },
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        let event = mgr
            .create_event(
                SseStreamType::SystemMetrics,
                "enhanced_metrics",
                serde_json::to_string(&enhanced_metrics).unwrap_or_else(|_| "{}".to_string()),
            )
            .await;

        Some((Ok(SseEventManager::to_axum_event(&event)), state))
    });

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

// Helper to extract system metrics logic
async fn get_system_metrics_internal(state: &AppState) -> Result<SystemMetricsResponse, String> {
    use adapteros_system_metrics::SystemMetricsCollector;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Collect system metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("time error: {}", e))?
        .as_secs();

    // Workers in 'healthy' status are actively serving inference requests
    let active_workers =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'healthy'")
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

    Ok(SystemMetricsResponse {
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
        cpu_usage_percent: Some(metrics.cpu_usage as f32),
        memory_usage_percent: Some(metrics.memory_usage as f32),
        tokens_per_second: None,
        error_rate: None,
        active_sessions: None,
        latency_p95_ms: None,
    })
}
