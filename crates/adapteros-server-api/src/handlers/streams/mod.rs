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
use crate::sse::{SseEventManager, SseStreamType};
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
    Extension,
};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt as FuturesStreamExt;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

/// Query parameters for stream endpoints
#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub tenant: String,
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
pub async fn system_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        let result = sse_manager
            .get_replay_with_analysis(SseStreamType::SystemMetrics, last_id)
            .await;

        // Log gap warning if events were lost
        if result.has_gap {
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
    let replay_stream = create_replay_stream(replay_events);

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
    Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(KeepAlive::default())
}

/// SSE stream for telemetry events
/// Streams telemetry events in real-time via broadcast channel with replay support
pub async fn telemetry_events_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::Telemetry, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = create_replay_stream(replay_events);

    // Subscribe to the telemetry broadcast channel for real-time events
    let receiver = state.telemetry_tx.subscribe();

    let live_stream = stream::unfold(
        (receiver, state.clone()),
        move |(mut rx, state)| async move {
            let mgr = state.sse_manager.clone();

            // Use select to handle both real-time events and keepalive timeout
            tokio::select! {
                // Try to receive a real-time telemetry event
                result = rx.recv() => {
                    match result {
                        Ok(telemetry_event) => {
                            // Serialize the telemetry event
                            let json = match serde_json::to_string(&telemetry_event) {
                                Ok(j) => j,
                                Err(e) => {
                                    tracing::warn!("Failed to serialize telemetry event: {}", e);
                                    let event = mgr
                                        .create_error_event(SseStreamType::Telemetry, &format!("serialization failed: {}", e))
                                        .await;
                                    return Some((Ok(SseEventManager::to_axum_event(&event)), (rx, state)));
                                }
                            };

                            // Create event with monotonic ID
                            let event = mgr
                                .create_event(SseStreamType::Telemetry, "telemetry", json)
                                .await;

                            Some((Ok(SseEventManager::to_axum_event(&event)), (rx, state)))
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                            // Client is lagging behind, notify and continue
                            tracing::warn!(lagged_count = count, "Telemetry SSE client lagged behind");
                            let data = serde_json::json!({ "lagged_events": count }).to_string();
                            let event = mgr
                                .create_event(SseStreamType::Telemetry, "warning", data)
                                .await;
                            Some((Ok(SseEventManager::to_axum_event(&event)), (rx, state)))
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            // Channel closed, end the stream gracefully
                            tracing::info!("Telemetry broadcast channel closed");
                            None
                        }
                    }
                }
                // Send keepalive if no events for 30 seconds
                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    // Check buffer health and send status
                    let buffer_len = state.telemetry_buffer.len().await;
                    let health_json = serde_json::json!({
                        "status": "keepalive",
                        "buffer_size": buffer_len
                    }).to_string();

                    let event = mgr
                        .create_event(SseStreamType::Telemetry, "keepalive", health_json)
                        .await;

                    Some((Ok(SseEventManager::to_axum_event(&event)), (rx, state)))
                }
            }
        },
    );

    Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(KeepAlive::default())
}

/// SSE stream for adapter state transitions
/// Streams adapter lifecycle events with replay support
pub async fn adapter_state_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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

    Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(KeepAlive::default())
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
        ("tenant" = String, Query, description = "Tenant ID for filtering events")
    ),
    responses(
        (status = 200, description = "SSE stream of training events")
    )
)]
pub async fn training_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Query(params): Query<StreamQuery>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let sse_manager = state.sse_manager.clone();
    let tenant_id = params.tenant.clone();

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
                    .expect("System time before UNIX epoch")
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
    Sse::new(FuturesStreamExt::chain(replay_stream, merged_stream)).keep_alive(KeepAlive::default())
}

/// SSE stream for alerts
/// Pushes real-time alerts as they are created or updated with replay support
pub async fn alerts_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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

    Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(KeepAlive::default())
}

/// SSE stream for anomalies
/// Pushes real-time anomaly detections with replay support
pub async fn anomalies_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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

    Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(KeepAlive::default())
}

/// SSE stream for dashboard-specific metrics
/// Pushes metrics tailored for dashboard widgets with replay support
pub async fn dashboard_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(dashboard_id): Path<String>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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

    let live_stream = stream::unfold(
        (state.clone(), dashboard_id),
        move |(state, dashboard_id)| async move {
            let mgr = state.sse_manager.clone();

            tokio::time::sleep(Duration::from_secs(5)).await;

            // Get dashboard configuration (placeholder for now)
            let dashboard_config = serde_json::json!({
                "widgets": [
                    {
                        "type": "time_series",
                        "metric": "cpu_usage",
                        "aggregation": "avg",
                        "window": "1h"
                    },
                    {
                        "type": "gauge",
                        "metric": "gpu_utilization",
                        "threshold_warning": 80,
                        "threshold_critical": 95
                    },
                    {
                        "type": "alert_list",
                        "severities": ["critical", "error"],
                        "limit": 10
                    }
                ],
                "refresh_interval": 30,
                "time_range": "24h"
            });

            // Fetch metrics for each widget
            let mut widget_data = Vec::new();

            for widget in dashboard_config["widgets"].as_array().unwrap_or(&vec![]) {
                let widget_type = widget["type"].as_str().unwrap_or("unknown");
                let metric_name = widget["metric"].as_str().unwrap_or("");

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

                let widget_result = match widget_type {
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
                            "widget_id": "time_series_1",
                            "widget_type": "time_series",
                            "data": {
                                "metric": metric_name,
                                "points": points,
                                "aggregation": widget["aggregation"],
                                "window": widget["window"]
                            }
                        })
                    }
                    "gauge" => {
                        let current_value = metrics.last().map(|m| m.metric_value).unwrap_or(0.0);
                        let status = if current_value
                            >= widget["threshold_critical"].as_f64().unwrap_or(95.0)
                        {
                            "critical"
                        } else if current_value
                            >= widget["threshold_warning"].as_f64().unwrap_or(80.0)
                        {
                            "warning"
                        } else {
                            "healthy"
                        };

                        serde_json::json!({
                            "widget_id": "gauge_1",
                            "widget_type": "gauge",
                            "data": {
                                "metric": metric_name,
                                "current_value": current_value,
                                "threshold_warning": widget["threshold_warning"],
                                "threshold_critical": widget["threshold_critical"],
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
                            limit: Some(widget["limit"].as_i64().unwrap_or(10)),
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
                            "widget_id": "alert_list_1",
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
                            "widget_id": "unknown_1",
                            "widget_type": widget_type,
                            "data": {},
                            "error": "Unknown widget type"
                        })
                    }
                };

                widget_data.push(widget_result);
            }

            let dashboard_data = serde_json::json!({
                "dashboard_id": dashboard_id,
                "widgets": widget_data,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "refresh_interval": dashboard_config["refresh_interval"]
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
                (state, dashboard_id),
            ))
        },
    );

    Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(KeepAlive::default())
}

/// Enhanced system metrics stream with monitoring data
/// Includes GPU metrics, inference latency, active alerts count, and recent anomalies
pub async fn enhanced_system_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
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
        };

        let recent_anomalies_count =
            match adapteros_system_metrics::ProcessAnomaly::list(state.db.pool(), anomaly_filters)
                .await
            {
                Ok(anomalies) => anomalies.len(),
                Err(_) => 0,
            };

        // Fetch worker health status
        let workers = match sqlx::query("SELECT id, status FROM workers WHERE status = 'active'")
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

    Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(KeepAlive::default())
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

    let active_workers =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM workers WHERE status = 'active'")
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
