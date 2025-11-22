//! Streaming endpoint handlers
//!
//! Provides real-time streaming APIs for system metrics, telemetry,
//! adapter states, and other continuous data feeds using Server-Sent Events (SSE).
//!
//! [2025-01-20 modularity streaming_handlers]

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::Extension,
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tracing::{info, warn};
use utoipa::ToSchema;

/// Metrics snapshot event for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsSnapshotEvent {
    /// Timestamp of the snapshot (milliseconds)
    pub timestamp_ms: u64,
    /// Latency metrics (percentiles)
    pub latency: StreamingLatencyMetrics,
    /// Throughput metrics
    pub throughput: StreamingThroughputMetrics,
    /// System resource metrics
    pub system: StreamingSystemMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingLatencyMetrics {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingThroughputMetrics {
    pub tokens_per_second: f64,
    pub inferences_per_second: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct StreamingSystemMetrics {
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub disk_percent: f64,
}

/// Telemetry event for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryStreamEvent {
    pub event_id: String,
    pub timestamp: u64,
    pub event_type: String,
    pub tenant_id: String,
    pub level: String,
    pub message: String,
    pub component: Option<String>,
    pub trace_id: Option<String>,
}

/// Adapter state change event for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterStateEvent {
    pub adapter_id: String,
    pub adapter_name: String,
    pub previous_state: Option<String>,
    pub current_state: String,
    pub timestamp: u64,
    pub activation_percentage: f64,
    pub memory_usage_mb: Option<f64>,
}

/// System metrics streaming endpoint
///
/// Streams real-time system metrics snapshots every 5 seconds.
/// Includes latency percentiles, throughput, system resources, and queue depths.
///
/// # SSE Event Format
/// ```json
/// event: metrics
/// data: {"timestamp":..., "latency":{...}, "throughput":{...}, ...}
/// ```
pub async fn system_metrics_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("Starting system metrics SSE stream");

    let stream = stream::unfold(state, |state| async move {
        // Sleep for 5 seconds between updates
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Collect metrics from the MetricsCollector
        let snapshot = state.metrics_collector.snapshot();

        // Convert to our streaming event format
        let event = MetricsSnapshotEvent {
            timestamp_ms: snapshot.timestamp_ms,
            latency: StreamingLatencyMetrics {
                p50_ms: snapshot.latency.p50_ms,
                p95_ms: snapshot.latency.p95_ms,
                p99_ms: snapshot.latency.p99_ms,
            },
            throughput: StreamingThroughputMetrics {
                tokens_per_second: snapshot.throughput.tokens_per_second,
                inferences_per_second: snapshot.throughput.inferences_per_second,
            },
            system: StreamingSystemMetrics {
                cpu_percent: snapshot.system.cpu_usage_percent,
                memory_percent: snapshot.system.memory_usage_percent,
                disk_percent: snapshot.system.disk_usage_percent,
            },
        };

        // Serialize to JSON
        let json = match serde_json::to_string(&event) {
            Ok(j) => j,
            Err(e) => {
                warn!(error = %e, "Failed to serialize metrics snapshot");
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data(format!("{{\"error\": \"serialization failed: {}\"}}", e))),
                    state,
                ));
            }
        };

        Some((Ok(Event::default().event("metrics").data(json)), state))
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Telemetry events streaming endpoint
///
/// Streams new telemetry events from the buffer as they arrive.
/// Polls the telemetry buffer every 2 seconds for new events.
///
/// # SSE Event Format
/// ```json
/// event: telemetry
/// data: {"events": [...], "count": 5}
/// ```
pub async fn telemetry_events_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("Starting telemetry events SSE stream");

    // Track last seen timestamp to only send new events
    let initial_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let stream = stream::unfold(
        (state, initial_timestamp),
        |(state, last_timestamp)| async move {
            // Poll every 2 seconds for new events
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Query telemetry buffer for events since last timestamp
            let filters = adapteros_telemetry::unified_events::TelemetryFilters {
                start_time: Some(last_timestamp),
                limit: Some(100), // Limit batch size
                ..Default::default()
            };

            let events = state.telemetry_buffer.query(&filters);

            // Get current timestamp for next iteration
            let current_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(last_timestamp);

            if events.is_empty() {
                // Send keepalive with event count
                return Some((
                    Ok(Event::default()
                        .event("telemetry")
                        .data("{\"events\": [], \"count\": 0}")),
                    (state, current_timestamp),
                ));
            }

            // Convert events to stream format
            let stream_events: Vec<TelemetryStreamEvent> = events
                .iter()
                .map(|e| TelemetryStreamEvent {
                    event_id: e.event_id.clone(),
                    timestamp: e.timestamp,
                    event_type: e.event_type.clone(),
                    tenant_id: e.identity.tenant_id.clone(),
                    level: e.level.clone(),
                    message: e.message.clone(),
                    component: e.component.clone(),
                    trace_id: e.trace_id.clone(),
                })
                .collect();

            let payload = serde_json::json!({
                "events": stream_events,
                "count": stream_events.len(),
            });

            let json = match serde_json::to_string(&payload) {
                Ok(j) => j,
                Err(e) => {
                    warn!(error = %e, "Failed to serialize telemetry events");
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"serialization failed: {}\"}}", e))),
                        (state, current_timestamp),
                    ));
                }
            };

            Some((
                Ok(Event::default().event("telemetry").data(json)),
                (state, current_timestamp),
            ))
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Adapter state streaming endpoint
///
/// Streams adapter lifecycle state changes. Monitors adapters every 3 seconds
/// and sends updates when states change.
///
/// # SSE Event Format
/// ```json
/// event: adapter_state
/// data: {"adapters": [...], "count": 3}
/// ```
pub async fn adapter_state_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("Starting adapter state SSE stream");

    // Initialize with empty state cache
    let initial_states: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    let stream = stream::unfold(
        (state, initial_states),
        |(state, mut previous_states)| async move {
            // Poll every 3 seconds for state changes
            tokio::time::sleep(Duration::from_secs(3)).await;

            let current_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            // Get adapter states from lifecycle manager if available
            let adapter_events = if let Some(ref lifecycle_manager) = state.lifecycle_manager {
                let manager = lifecycle_manager.lock().await;
                let all_states = manager.get_all_states();

                let mut events = Vec::new();

                for (adapter_id, current_state) in &all_states {
                    let state_str = format!("{:?}", current_state);
                    let previous_state = previous_states.get(adapter_id).cloned();

                    // Check if state has changed
                    if previous_state.as_ref() != Some(&state_str) {
                        // Get additional adapter info from database
                        let (adapter_name, activation_pct, memory_mb) =
                            match state.db.get_adapter(adapter_id).await {
                                Ok(Some(adapter)) => {
                                    let name = adapter
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or(adapter_id)
                                        .to_string();
                                    let activation = adapter
                                        .get("activation_percentage")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    let memory = adapter
                                        .get("memory_usage_mb")
                                        .and_then(|v| v.as_f64());
                                    (name, activation, memory)
                                }
                                _ => (adapter_id.clone(), 0.0, None),
                            };

                        events.push(AdapterStateEvent {
                            adapter_id: adapter_id.clone(),
                            adapter_name,
                            previous_state,
                            current_state: state_str.clone(),
                            timestamp: current_timestamp,
                            activation_percentage: activation_pct,
                            memory_usage_mb: memory_mb,
                        });

                        // Update cache
                        previous_states.insert(adapter_id.clone(), state_str);
                    }
                }

                events
            } else {
                // Fallback: list adapters from database
                match state.db.list_adapters().await {
                    Ok(adapters) => {
                        let mut events = Vec::new();

                        for adapter in adapters {
                            let adapter_id = adapter
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown")
                                .to_string();
                            let current_state = adapter
                                .get("lifecycle_state")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Unknown")
                                .to_string();

                            let previous_state = previous_states.get(&adapter_id).cloned();

                            // Check if state has changed
                            if previous_state.as_ref() != Some(&current_state) {
                                let adapter_name = adapter
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or(&adapter_id)
                                    .to_string();
                                let activation_pct = adapter
                                    .get("activation_percentage")
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(0.0);
                                let memory_mb =
                                    adapter.get("memory_usage_mb").and_then(|v| v.as_f64());

                                events.push(AdapterStateEvent {
                                    adapter_id: adapter_id.clone(),
                                    adapter_name,
                                    previous_state,
                                    current_state: current_state.clone(),
                                    timestamp: current_timestamp,
                                    activation_percentage: activation_pct,
                                    memory_usage_mb: memory_mb,
                                });

                                // Update cache
                                previous_states.insert(adapter_id, current_state);
                            }
                        }

                        events
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to list adapters for state stream");
                        return Some((
                            Ok(Event::default()
                                .event("error")
                                .data(format!("{{\"error\": \"{}\"}}", e))),
                            (state, previous_states),
                        ));
                    }
                }
            };

            // Build response payload
            let payload = serde_json::json!({
                "adapters": adapter_events,
                "count": adapter_events.len(),
                "timestamp": current_timestamp,
            });

            let json = match serde_json::to_string(&payload) {
                Ok(j) => j,
                Err(e) => {
                    warn!(error = %e, "Failed to serialize adapter states");
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"serialization failed: {}\"}}", e))),
                        (state, previous_states),
                    ));
                }
            };

            Some((
                Ok(Event::default().event("adapter_state").data(json)),
                (state, previous_states),
            ))
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}
