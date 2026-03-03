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
use crate::handlers::workers::is_terminal_worker_status;
use crate::pause_tracker::{PausedInferenceInfo as ServerPausedInfo, ServerPauseTracker};
use crate::permissions::{require_permission, Permission};
use crate::security::check_tenant_access;
use crate::sse::{EventGapRecoveryHint, SseErrorEvent, SseEvent, SseEventManager, SseStreamType};
use crate::state::AppState;
use crate::types::*;
use adapteros_api_types::review::{ListPausedResponse, PausedInferenceInfo as ApiPausedInfo};
use adapteros_api_types::schema_version;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{
        sse::{Event, KeepAlive, KeepAliveStream, Sse},
        IntoResponse,
    },
    Extension,
};
use chrono::{DateTime, NaiveDateTime, Utc};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt as FuturesStreamExt;
use serde::{Deserialize, Serialize};
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

const DETERMINISM_STATUS_STALE_AFTER_SECS: i64 = 60 * 60;
const REPLAY_GUARD_STALE_AFTER_SECS: i64 = 15 * 60;

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

fn parse_determinism_last_run(last_run: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(last_run) {
        return Some(parsed.with_timezone(&Utc));
    }

    NaiveDateTime::parse_from_str(last_run, "%Y-%m-%d %H:%M:%S%.f")
        .or_else(|_| NaiveDateTime::parse_from_str(last_run, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .map(|parsed| DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc))
}

async fn determinism_guard_stream_status(state: &AppState) -> serde_json::Value {
    let query_result = sqlx::query(
        "SELECT last_run, result, runs, divergences
         FROM determinism_checks
         ORDER BY last_run DESC
         LIMIT 1",
    )
    .fetch_optional(state.db.pool())
    .await;

    match query_result {
        Ok(Some(row)) => {
            let last_run: Option<String> = row.try_get("last_run").ok();
            let raw_result: Option<String> = row.try_get("result").ok();
            let runs: Option<i64> = row.try_get("runs").ok();
            let divergences: Option<i64> = row.try_get("divergences").ok();
            let normalized_result = raw_result.map(|value| value.trim().to_ascii_lowercase());

            let now = Utc::now();
            let (freshness_status, freshness_reason, freshness_age_seconds) = match last_run
                .as_deref()
            {
                None => ("unknown", "missing_last_run", None),
                Some(last_run_raw) => match parse_determinism_last_run(last_run_raw) {
                    None => ("unknown", "invalid_last_run_format", None),
                    Some(parsed_last_run) => {
                        let age_seconds = now.signed_duration_since(parsed_last_run).num_seconds();
                        if age_seconds < 0 {
                            ("unknown", "future_last_run", None)
                        } else if age_seconds <= DETERMINISM_STATUS_STALE_AFTER_SECS {
                            ("fresh", "recent_run", Some(age_seconds))
                        } else {
                            ("stale", "stale_last_run", Some(age_seconds))
                        }
                    }
                },
            };

            let (replay_guard_outcome, replay_guard_reason) = match normalized_result.as_deref() {
                Some("pass")
                    if freshness_status == "fresh"
                        && freshness_age_seconds
                            .map(|age| age <= REPLAY_GUARD_STALE_AFTER_SECS)
                            .unwrap_or(false) =>
                {
                    ("pass", "latest_replay_guard_passed")
                }
                Some("pass") => ("unknown", "latest_pass_not_fresh_enough"),
                Some("fail") => ("fail", "latest_replay_guard_failed"),
                Some(_) => ("unknown", "unknown_replay_guard_result"),
                None => ("unknown", "missing_replay_guard_result"),
            };

            json!({
                "freshness_status": freshness_status,
                "freshness_reason": freshness_reason,
                "freshness_age_seconds": freshness_age_seconds,
                "freshness_stale_after_seconds": DETERMINISM_STATUS_STALE_AFTER_SECS,
                "replay_guard_outcome": replay_guard_outcome,
                "replay_guard_reason": replay_guard_reason,
                "replay_guard_stale_after_seconds": REPLAY_GUARD_STALE_AFTER_SECS,
                "last_run": last_run,
                "result": normalized_result,
                "runs": runs,
                "divergences": divergences,
            })
        }
        Ok(None) => json!({
            "freshness_status": "unknown",
            "freshness_reason": "no_determinism_checks",
            "freshness_age_seconds": serde_json::Value::Null,
            "freshness_stale_after_seconds": DETERMINISM_STATUS_STALE_AFTER_SECS,
            "replay_guard_outcome": "unknown",
            "replay_guard_reason": "no_determinism_checks",
            "replay_guard_stale_after_seconds": REPLAY_GUARD_STALE_AFTER_SECS,
            "last_run": serde_json::Value::Null,
            "result": serde_json::Value::Null,
            "runs": serde_json::Value::Null,
            "divergences": serde_json::Value::Null,
        }),
        Err(error) => {
            tracing::warn!(
                error = %error,
                "Failed to query determinism status for SSE payload context"
            );
            json!({
                "freshness_status": "unknown",
                "freshness_reason": "query_error",
                "freshness_age_seconds": serde_json::Value::Null,
                "freshness_stale_after_seconds": DETERMINISM_STATUS_STALE_AFTER_SECS,
                "replay_guard_outcome": "unknown",
                "replay_guard_reason": "query_error",
                "replay_guard_stale_after_seconds": REPLAY_GUARD_STALE_AFTER_SECS,
                "last_run": serde_json::Value::Null,
                "result": serde_json::Value::Null,
                "runs": serde_json::Value::Null,
                "divergences": serde_json::Value::Null,
            })
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TenantScopedStreamEnvelope {
    tenant_id: String,
    payload_json: String,
}

fn encode_tenant_scoped_envelope(
    tenant_id: &str,
    payload_json: String,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&TenantScopedStreamEnvelope {
        tenant_id: tenant_id.to_string(),
        payload_json,
    })
}

fn decode_tenant_scoped_payload_for_tenant(event: &SseEvent, tenant_id: &str) -> Option<String> {
    let envelope: TenantScopedStreamEnvelope = serde_json::from_str(&event.data).ok()?;
    if envelope.tenant_id != tenant_id {
        return None;
    }
    Some(envelope.payload_json)
}

fn decode_tenant_scoped_replay_events_for_tenant(
    events: Vec<SseEvent>,
    tenant_id: &str,
) -> Vec<Result<Event, Infallible>> {
    events
        .into_iter()
        .filter_map(|event| {
            decode_tenant_scoped_payload_for_tenant(&event, tenant_id)
                .map(|payload| Ok(to_axum_event_with_payload(&event, payload)))
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReviewsStreamEnvelope {
    tenant_id: String,
    payload_json: String,
}

fn paused_review_to_api(info: ServerPausedInfo) -> ApiPausedInfo {
    ApiPausedInfo {
        inference_id: info.inference_id,
        pause_id: info.pause_id,
        kind: info.kind,
        paused_at: info.created_at.to_rfc3339(),
        duration_secs: info.duration_secs,
        context_preview: info.context.question.map(|q| {
            if q.len() > 100 {
                format!("{}...", &q[..97])
            } else {
                q
            }
        }),
    }
}

fn list_paused_payload(tracker: &ServerPauseTracker, tenant_id: &str) -> ListPausedResponse {
    let paused_list = tracker.list_paused_for_tenant(tenant_id);
    let total = paused_list.len();
    let paused = paused_list.into_iter().map(paused_review_to_api).collect();

    ListPausedResponse {
        schema_version: schema_version(),
        paused,
        total,
    }
}

fn reviews_payload_signature(payload: &ListPausedResponse) -> Result<String, serde_json::Error> {
    let mut entries: Vec<serde_json::Value> = payload
        .paused
        .iter()
        .map(|info| {
            json!({
                "pause_id": info.pause_id,
                "inference_id": info.inference_id,
                "kind": info.kind,
                "paused_at": info.paused_at,
                "context_preview": info.context_preview,
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        let left = a
            .get("pause_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let right = b
            .get("pause_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        left.cmp(right)
    });
    serde_json::to_string(&entries)
}

fn encode_reviews_envelope(
    tenant_id: &str,
    payload_json: String,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&ReviewsStreamEnvelope {
        tenant_id: tenant_id.to_string(),
        payload_json,
    })
}

fn decode_reviews_payload_for_tenant(event: &SseEvent, tenant_id: &str) -> Option<String> {
    let envelope: ReviewsStreamEnvelope = serde_json::from_str(&event.data).ok()?;
    if envelope.tenant_id != tenant_id {
        return None;
    }
    Some(envelope.payload_json)
}

fn to_axum_event_with_payload(event: &SseEvent, payload_json: String) -> Event {
    let mut sse_event = Event::default()
        .id(event.id.to_string())
        .event(&event.event_type)
        .data(payload_json);

    if let Some(retry_ms) = event.retry_ms {
        sse_event = sse_event.retry(Duration::from_millis(retry_ms as u64));
    }

    sse_event
}

#[cfg(test)]
mod reviews_stream_tests {
    use super::*;

    #[test]
    fn reviews_signature_ignores_duration_churn() {
        let base = ListPausedResponse {
            schema_version: schema_version(),
            paused: vec![ApiPausedInfo {
                inference_id: "inf-1".to_string(),
                pause_id: "pause-1".to_string(),
                kind: adapteros_api_types::review::PauseKind::ReviewNeeded,
                paused_at: "2026-01-01T00:00:00Z".to_string(),
                duration_secs: 5,
                context_preview: Some("needs review".to_string()),
            }],
            total: 1,
        };
        let mut updated = base.clone();
        updated.paused[0].duration_secs = 65;

        let base_sig = reviews_payload_signature(&base).expect("base signature");
        let updated_sig = reviews_payload_signature(&updated).expect("updated signature");

        assert_eq!(base_sig, updated_sig);
    }

    #[test]
    fn decode_reviews_payload_filters_by_tenant() {
        let payload = r#"{"paused":[],"total":0,"schema_version":"1.0.0"}"#.to_string();
        let envelope = encode_reviews_envelope("tenant-a", payload.clone()).expect("envelope");
        let event = SseEvent {
            id: 7,
            event_type: "reviews".to_string(),
            data: envelope,
            timestamp_ms: 0,
            retry_ms: Some(3000),
        };

        assert_eq!(
            decode_reviews_payload_for_tenant(&event, "tenant-a"),
            Some(payload)
        );
        assert_eq!(decode_reviews_payload_for_tenant(&event, "tenant-b"), None);
    }

    #[test]
    fn reviews_signature_ignores_order_churn() {
        let first = ListPausedResponse {
            schema_version: schema_version(),
            paused: vec![
                ApiPausedInfo {
                    inference_id: "inf-2".to_string(),
                    pause_id: "pause-2".to_string(),
                    kind: adapteros_api_types::review::PauseKind::ReviewNeeded,
                    paused_at: "2026-01-01T00:00:01Z".to_string(),
                    duration_secs: 4,
                    context_preview: Some("two".to_string()),
                },
                ApiPausedInfo {
                    inference_id: "inf-1".to_string(),
                    pause_id: "pause-1".to_string(),
                    kind: adapteros_api_types::review::PauseKind::ReviewNeeded,
                    paused_at: "2026-01-01T00:00:00Z".to_string(),
                    duration_secs: 5,
                    context_preview: Some("one".to_string()),
                },
            ],
            total: 2,
        };
        let second = ListPausedResponse {
            schema_version: schema_version(),
            paused: vec![first.paused[1].clone(), first.paused[0].clone()],
            total: 2,
        };

        let first_sig = reviews_payload_signature(&first).expect("first signature");
        let second_sig = reviews_payload_signature(&second).expect("second signature");
        assert_eq!(first_sig, second_sig);
    }
}

#[cfg(test)]
mod tenant_scoped_stream_envelope_tests {
    use super::*;

    #[test]
    fn decode_tenant_scoped_payload_matches_tenant() {
        let payload = r#"{"workers":[]}"#.to_string();
        let envelope =
            encode_tenant_scoped_envelope("tenant-a", payload.clone()).expect("envelope");
        let event = SseEvent {
            id: 11,
            event_type: "workers".to_string(),
            data: envelope,
            timestamp_ms: 0,
            retry_ms: Some(3000),
        };

        assert_eq!(
            decode_tenant_scoped_payload_for_tenant(&event, "tenant-a"),
            Some(payload)
        );
    }

    #[test]
    fn decode_tenant_scoped_payload_rejects_invalid_or_mismatched() {
        let mismatched = SseEvent {
            id: 12,
            event_type: "training".to_string(),
            data: encode_tenant_scoped_envelope("tenant-b", "{}".to_string()).expect("envelope"),
            timestamp_ms: 0,
            retry_ms: Some(3000),
        };
        let invalid = SseEvent {
            id: 13,
            event_type: "training".to_string(),
            data: "{\"tenant_id\":\"tenant-a\"}".to_string(),
            timestamp_ms: 0,
            retry_ms: Some(3000),
        };

        assert_eq!(
            decode_tenant_scoped_payload_for_tenant(&mismatched, "tenant-a"),
            None
        );
        assert_eq!(
            decode_tenant_scoped_payload_for_tenant(&invalid, "tenant-a"),
            None
        );
    }

    #[test]
    fn replay_filter_drops_invalid_or_mismatched_envelopes() {
        let matching_payload = r#"{"adapters":[]}"#.to_string();
        let matching = SseEvent {
            id: 14,
            event_type: "adapters".to_string(),
            data: encode_tenant_scoped_envelope("tenant-a", matching_payload).expect("envelope"),
            timestamp_ms: 0,
            retry_ms: Some(3000),
        };
        let mismatched = SseEvent {
            id: 15,
            event_type: "adapters".to_string(),
            data: encode_tenant_scoped_envelope("tenant-b", "{}".to_string()).expect("envelope"),
            timestamp_ms: 0,
            retry_ms: Some(3000),
        };
        let invalid = SseEvent {
            id: 16,
            event_type: "adapters".to_string(),
            data: "not-json".to_string(),
            timestamp_ms: 0,
            retry_ms: Some(3000),
        };

        let filtered = decode_tenant_scoped_replay_events_for_tenant(
            vec![matching, mismatched, invalid],
            "tenant-a",
        );

        assert_eq!(filtered.len(), 1);
    }
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
    let replay_events: Vec<Result<Event, Infallible>> = if let Some(last_id) = last_event_id {
        decode_tenant_scoped_replay_events_for_tenant(
            sse_manager
                .get_replay_events(SseStreamType::AdapterState, last_id)
                .await,
            &tenant_id,
        )
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(replay_events);

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

            let envelope_json = match encode_tenant_scoped_envelope(&tenant_id, json.clone()) {
                Ok(value) => value,
                Err(err) => {
                    tracing::warn!(
                        tenant_id = %tenant_id,
                        error = %err,
                        "Failed to serialize adapter state stream envelope"
                    );
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
                .create_event(SseStreamType::AdapterState, "adapters", envelope_json)
                .await;

            Some((
                Ok(to_axum_event_with_payload(&event, json)),
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
    let replay_events: Vec<Result<Event, Infallible>> = if let Some(last_id) = last_event_id {
        decode_tenant_scoped_replay_events_for_tenant(
            sse_manager
                .get_replay_events(SseStreamType::Workers, last_id)
                .await,
            &tenant_id,
        )
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(replay_events);

    let live_stream = stream::unfold(
        (state.clone(), tenant_id),
        move |(state, tenant_id)| async move {
            let mgr = state.sse_manager.clone();

            tokio::time::sleep(Duration::from_secs(10)).await;

            let workers: Vec<_> = match state.db.list_workers_by_tenant(&tenant_id).await {
                Ok(w) => w
                    .into_iter()
                    .filter(|worker| !is_terminal_worker_status(&worker.status))
                    .collect(),
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
                .map(|w| {
                    let display_name = adapteros_id::display_name_for(&w.id);
                    WorkerResponse {
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
                        display_name,
                        active_model_id: None,
                        active_model_hash: None,
                        model_generation: None,
                        model_load_state: None,
                        model_error: None,
                    }
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

            let envelope_json = match encode_tenant_scoped_envelope(&tenant_id, json.clone()) {
                Ok(value) => value,
                Err(err) => {
                    tracing::warn!(
                        tenant_id = %tenant_id,
                        error = %err,
                        "Failed to serialize workers stream envelope"
                    );
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
                .create_event(SseStreamType::Workers, "workers", envelope_json)
                .await;

            Some((
                Ok(to_axum_event_with_payload(&event, json)),
                (state, tenant_id),
            ))
        },
    );

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// SSE stream for paused review queue updates
///
/// Streams tenant-scoped snapshots of paused reviews with replay support.
#[utoipa::path(
    tag = "reviews",
    get,
    path = "/v1/stream/reviews",
    params(
        ("tenant" = Option<String>, Query, description = "Tenant ID for filtering events (defaults to caller tenant)")
    ),
    responses(
        (status = 200, description = "SSE stream of paused review queue updates")
    )
)]
pub async fn reviews_stream(
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
            "Reviews stream tenant access denied"
        );
        let event = Event::default()
            .event("error")
            .data("{\"error\": \"Access denied for tenant reviews stream\"}");
        return sse_response(stream::iter(vec![Ok(event)]));
    }

    let pause_tracker = match state.pause_tracker.clone() {
        Some(tracker) => tracker,
        None => {
            let event = Event::default()
                .event("error")
                .data("{\"error\": \"Server pause tracker not initialized\"}");
            return sse_response(stream::iter(vec![Ok(event)]));
        }
    };

    let sse_manager = state.sse_manager.clone();
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    let replay_events: Vec<Result<Event, Infallible>> = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Reviews, last_id)
            .await
            .into_iter()
            .filter_map(|event| {
                decode_reviews_payload_for_tenant(&event, &tenant_id)
                    .map(|payload| Ok(to_axum_event_with_payload(&event, payload)))
            })
            .collect()
    } else {
        Vec::new()
    };
    let replay_stream = stream::iter(replay_events);

    let pause_change_rx = pause_tracker.subscribe_changes();
    let live_stream = stream::unfold(
        (
            state.sse_manager.clone(),
            pause_tracker,
            tenant_id,
            None::<String>,
            pause_change_rx,
            true,
        ),
        move |(
            sse_manager,
            pause_tracker,
            tenant_id,
            mut last_signature,
            mut pause_change_rx,
            mut first_poll,
        )| async move {
            loop {
                if !first_poll {
                    tokio::select! {
                        change_result = pause_change_rx.changed() => {
                            if change_result.is_err() {
                                tracing::debug!("Pause tracker change channel closed");
                                return None;
                            }
                        }
                        _ = tokio::time::sleep(Duration::from_secs(60)) => {}
                    }
                }
                first_poll = false;

                let payload = list_paused_payload(&pause_tracker, &tenant_id);
                let payload_json = match serde_json::to_string(&payload) {
                    Ok(json) => json,
                    Err(err) => {
                        tracing::warn!(
                            tenant_id = %tenant_id,
                            error = %err,
                            "Failed to serialize reviews stream payload"
                        );
                        let event = Event::default()
                            .event("error")
                            .data("{\"error\":\"serialization failed\"}");
                        return Some((
                            Ok(event),
                            (
                                sse_manager,
                                pause_tracker,
                                tenant_id,
                                last_signature,
                                pause_change_rx,
                                first_poll,
                            ),
                        ));
                    }
                };

                let payload_signature = match reviews_payload_signature(&payload) {
                    Ok(signature) => signature,
                    Err(err) => {
                        tracing::warn!(
                            tenant_id = %tenant_id,
                            error = %err,
                            "Failed to serialize reviews stream signature"
                        );
                        let event = Event::default()
                            .event("error")
                            .data("{\"error\":\"serialization failed\"}");
                        return Some((
                            Ok(event),
                            (
                                sse_manager,
                                pause_tracker,
                                tenant_id,
                                last_signature,
                                pause_change_rx,
                                first_poll,
                            ),
                        ));
                    }
                };

                if last_signature.as_deref() == Some(payload_signature.as_str()) {
                    continue;
                }

                let envelope_json = match encode_reviews_envelope(&tenant_id, payload_json.clone())
                {
                    Ok(json) => json,
                    Err(err) => {
                        tracing::warn!(
                            tenant_id = %tenant_id,
                            error = %err,
                            "Failed to serialize reviews stream envelope"
                        );
                        let event = Event::default()
                            .event("error")
                            .data("{\"error\":\"serialization failed\"}");
                        return Some((
                            Ok(event),
                            (
                                sse_manager,
                                pause_tracker,
                                tenant_id,
                                last_signature,
                                pause_change_rx,
                                first_poll,
                            ),
                        ));
                    }
                };

                let event = sse_manager
                    .create_event(SseStreamType::Reviews, "reviews", envelope_json)
                    .await;

                last_signature = Some(payload_signature);
                let outbound = to_axum_event_with_payload(&event, payload_json);
                return Some((
                    Ok(outbound),
                    (
                        sse_manager,
                        pause_tracker,
                        tenant_id,
                        last_signature,
                        pause_change_rx,
                        first_poll,
                    ),
                ));
            }
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
    path = "/v1/stream/training",
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
    let replay_events: Vec<Result<Event, Infallible>> = if let Some(last_id) = last_event_id {
        decode_tenant_scoped_replay_events_for_tenant(
            sse_manager
                .get_replay_events(SseStreamType::Training, last_id)
                .await,
            &tenant_id,
        )
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(replay_events);

    // Subscribe to the training signal broadcast channel
    let rx = state.training_signal_tx.subscribe();

    // Convert the broadcast receiver into a stream that filters by tenant
    // Use FuturesStreamExt::filter_map explicitly for async closure support
    let mgr_for_signals = Arc::new(state.sse_manager.clone());
    let tenant_id_for_signals = tenant_id.clone();
    let signal_stream = FuturesStreamExt::filter_map(BroadcastStream::new(rx), move |result| {
        let tenant_filter = tenant_id_for_signals.clone();
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

                    // Fail closed: only pass through when signal tenant matches subscriber tenant.
                    if signal_tenant == tenant_filter.as_str() {
                        let event_data = serde_json::json!({
                            "type": signal.signal_type.to_string(),
                            "timestamp": signal.timestamp,
                            "priority": format!("{:?}", signal.priority),
                            "payload": signal.payload,
                            "trace_id": signal.trace_id,
                        });

                        let payload_json = event_data.to_string();
                        let envelope_json = match encode_tenant_scoped_envelope(
                            &tenant_filter,
                            payload_json.clone(),
                        ) {
                            Ok(value) => value,
                            Err(err) => {
                                tracing::warn!(
                                    tenant_id = %tenant_filter,
                                    error = %err,
                                    "Failed to serialize training signal stream envelope"
                                );
                                let event = mgr
                                    .create_error_event(
                                        SseStreamType::Training,
                                        "serialization failed",
                                    )
                                    .await;
                                return Some(Ok(SseEventManager::to_axum_event(&event)));
                            }
                        };

                        let event = mgr
                            .create_event(SseStreamType::Training, "training", envelope_json)
                            .await;

                        Some(Ok(to_axum_event_with_payload(&event, payload_json)))
                    } else {
                        tracing::debug!(
                            tenant_filter = %tenant_filter,
                            signal_tenant = %signal_tenant,
                            "Dropping training signal for tenant-mismatched or unlabeled payload"
                        );
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
    let tenant_id_for_heartbeat = tenant_id.clone();
    let heartbeat_stream = stream::unfold(0u64, move |counter| {
        let mgr = mgr_for_heartbeat.clone();
        let tenant_id = tenant_id_for_heartbeat.clone();
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

            let payload_json = event_data.to_string();
            let envelope_json =
                match encode_tenant_scoped_envelope(&tenant_id, payload_json.clone()) {
                    Ok(value) => value,
                    Err(err) => {
                        tracing::warn!(
                            tenant_id = %tenant_id,
                            error = %err,
                            "Failed to serialize training heartbeat stream envelope"
                        );
                        let event = mgr
                            .create_error_event(SseStreamType::Training, "serialization failed")
                            .await;
                        return Some((Ok(SseEventManager::to_axum_event(&event)), counter + 1));
                    }
                };

            let event = mgr
                .create_event(SseStreamType::Training, "training", envelope_json)
                .await;

            Some((
                Ok(to_axum_event_with_payload(&event, payload_json)),
                counter + 1,
            ))
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
    Extension(claims): Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    let sse_manager = state.sse_manager.clone();
    let tenant_id = claims.tenant_id;

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events: Vec<Result<Event, Infallible>> = if let Some(last_id) = last_event_id {
        decode_tenant_scoped_replay_events_for_tenant(
            sse_manager
                .get_replay_events(SseStreamType::Alerts, last_id)
                .await,
            &tenant_id,
        )
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(replay_events);

    let live_stream = stream::unfold(
        (state.clone(), tenant_id),
        move |(state, tenant_id)| async move {
            let mgr = state.sse_manager.clone();

            tokio::time::sleep(Duration::from_secs(2)).await;

            // Fetch recent alerts
            let filters = adapteros_system_metrics::AlertFilters {
                tenant_id: Some(tenant_id.clone()),
                worker_id: None,
                status: None,
                severity: None,
                start_time: Some(chrono::Utc::now() - chrono::Duration::minutes(5)),
                end_time: None,
                limit: Some(50),
                offset: None,
            };

            let alerts = match adapteros_system_metrics::ProcessAlert::list(
                state.db.pool(),
                filters,
            )
            .await
            {
                Ok(alerts) => alerts,
                Err(e) => {
                    tracing::warn!("Failed to fetch alerts for SSE: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::Alerts, &e.to_string())
                        .await;
                    return Some((
                        Ok(SseEventManager::to_axum_event(&event)),
                        (state, tenant_id),
                    ));
                }
            };
            let determinism_guard = determinism_guard_stream_status(&state).await;

            let alert_data = serde_json::json!({
                "tenant_id": tenant_id.clone(),
                "alerts": alerts.iter().map(|a| adapteros_system_metrics::AlertResponse::from(a.clone())).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "count": alerts.len(),
                "determinism_guard": determinism_guard
            });
            let payload_json = alert_data.to_string();
            let envelope_json =
                match encode_tenant_scoped_envelope(&tenant_id, payload_json.clone()) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(
                            tenant_id = %tenant_id,
                            error = %error,
                            "Failed to serialize alerts stream envelope"
                        );
                        let event = mgr
                            .create_error_event(SseStreamType::Alerts, "serialization failed")
                            .await;
                        return Some((
                            Ok(SseEventManager::to_axum_event(&event)),
                            (state, tenant_id),
                        ));
                    }
                };

            let event = mgr
                .create_event(SseStreamType::Alerts, "alerts", envelope_json)
                .await;

            Some((
                Ok(to_axum_event_with_payload(&event, payload_json)),
                (state, tenant_id),
            ))
        },
    );

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// SSE stream for anomalies
/// Pushes real-time anomaly detections with replay support
pub async fn anomalies_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    let sse_manager = state.sse_manager.clone();
    let tenant_id = claims.tenant_id;

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events: Vec<Result<Event, Infallible>> = if let Some(last_id) = last_event_id {
        decode_tenant_scoped_replay_events_for_tenant(
            sse_manager
                .get_replay_events(SseStreamType::Anomalies, last_id)
                .await,
            &tenant_id,
        )
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(replay_events);

    let live_stream = stream::unfold(
        (state.clone(), tenant_id),
        move |(state, tenant_id)| async move {
            let mgr = state.sse_manager.clone();

            tokio::time::sleep(Duration::from_secs(10)).await;

            // Fetch recent anomalies
            let filters = adapteros_system_metrics::AnomalyFilters {
                tenant_id: Some(tenant_id.clone()),
                worker_id: None,
                status: None,
                anomaly_type: None,
                start_time: Some(chrono::Utc::now() - chrono::Duration::minutes(10)),
                end_time: None,
                limit: Some(20),
                offset: None,
            };

            let anomalies = match adapteros_system_metrics::ProcessAnomaly::list(
                state.db.pool(),
                filters,
            )
            .await
            {
                Ok(anomalies) => anomalies,
                Err(e) => {
                    tracing::warn!("Failed to fetch anomalies for SSE: {}", e);
                    let event = mgr
                        .create_error_event(SseStreamType::Anomalies, &e.to_string())
                        .await;
                    return Some((
                        Ok(SseEventManager::to_axum_event(&event)),
                        (state, tenant_id),
                    ));
                }
            };
            let determinism_guard = determinism_guard_stream_status(&state).await;

            let anomaly_data = serde_json::json!({
                "tenant_id": tenant_id.clone(),
                "anomalies": anomalies.iter().map(|a| adapteros_system_metrics::AnomalyResponse::from(a.clone())).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "count": anomalies.len(),
                "determinism_guard": determinism_guard
            });
            let payload_json = anomaly_data.to_string();
            let envelope_json =
                match encode_tenant_scoped_envelope(&tenant_id, payload_json.clone()) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(
                            tenant_id = %tenant_id,
                            error = %error,
                            "Failed to serialize anomalies stream envelope"
                        );
                        let event = mgr
                            .create_error_event(SseStreamType::Anomalies, "serialization failed")
                            .await;
                        return Some((
                            Ok(SseEventManager::to_axum_event(&event)),
                            (state, tenant_id),
                        ));
                    }
                };

            let event = mgr
                .create_event(SseStreamType::Anomalies, "anomalies", envelope_json)
                .await;

            Some((
                Ok(to_axum_event_with_payload(&event, payload_json)),
                (state, tenant_id),
            ))
        },
    );

    sse_response(FuturesStreamExt::chain(replay_stream, live_stream))
}

/// SSE stream for dashboard-specific metrics
/// Pushes metrics tailored for dashboard widgets with replay support
pub async fn dashboard_stream(
    state: State<AppState>,
    claims: Extension<Claims>,
    headers: HeaderMap,
) -> SseResponse {
    dashboard_metrics_stream(state, claims, Path("default".to_string()), headers).await
}

/// SSE stream for dashboard-specific metrics by dashboard id
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
    let replay_events: Vec<Result<Event, Infallible>> = if let Some(last_id) = last_event_id {
        decode_tenant_scoped_replay_events_for_tenant(
            sse_manager
                .get_replay_events(SseStreamType::Dashboard, last_id)
                .await,
            &claims.tenant_id,
        )
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(replay_events);

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
                    tenant_id: Some(tenant_id.clone()),
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
                            tenant_id: Some(tenant_id.clone()),
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
            let determinism_guard = determinism_guard_stream_status(&state).await;

            let dashboard_data = serde_json::json!({
                "dashboard_id": dashboard_id.clone(),
                "tenant_id": tenant_id.clone(),
                "widgets": widget_data,
                "widget_config": widget_config,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "refresh_interval": refresh_interval,
                "determinism_guard": determinism_guard
            });
            let payload_json = dashboard_data.to_string();
            let envelope_json =
                match encode_tenant_scoped_envelope(&tenant_id, payload_json.clone()) {
                    Ok(value) => value,
                    Err(error) => {
                        tracing::warn!(
                            tenant_id = %tenant_id,
                            error = %error,
                            "Failed to serialize dashboard stream envelope"
                        );
                        let event = mgr
                            .create_error_event(SseStreamType::Dashboard, "serialization failed")
                            .await;
                        return Some((
                            Ok(SseEventManager::to_axum_event(&event)),
                            (state, dashboard_id, tenant_id, user_id),
                        ));
                    }
                };

            let event = mgr
                .create_event(SseStreamType::Dashboard, "dashboard_metrics", envelope_json)
                .await;

            Some((
                Ok(to_axum_event_with_payload(&event, payload_json)),
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
