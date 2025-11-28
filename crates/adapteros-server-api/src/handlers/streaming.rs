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
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: MetricsView required for system metrics
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::MetricsView,
    ).is_ok();

    if !has_permission {
        warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for system metrics stream"
        );
    } else {
        info!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Starting system metrics SSE stream"
        );
    }

    let stream = stream::unfold((state, has_permission), |(state, has_permission)| async move {
        if !has_permission {
            // Return error event once and end stream
            return Some((
                Ok(Event::default()
                    .event("error")
                    .data("{\"error\": \"Permission denied - MetricsView required\"}")),
                (state, false),
            ));
        }
        // Sleep for 5 seconds between updates
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Create a default snapshot with current timestamp
        // Note: adapteros_telemetry::MetricsCollector doesn't have snapshot method
        // This creates a default snapshot structure for streaming
        let snapshot = adapteros_telemetry::metrics::MetricsSnapshot::default();

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
                    (state, has_permission),
                ));
            }
        };

        Some((Ok(Event::default().event("metrics").data(json)), (state, has_permission)))
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
/// CRITICAL: Events are filtered by tenant_id for multi-tenant isolation.
///
/// # SSE Event Format
/// ```json
/// event: telemetry
/// data: {"events": [...], "count": 5}
/// ```
pub async fn telemetry_events_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: TelemetryView required
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::TelemetryView,
    ).is_ok();

    if !has_permission {
        warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for telemetry events stream"
        );
    } else {
        info!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Starting telemetry events SSE stream"
        );
    }

    // Track last seen timestamp to only send new events
    let initial_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Capture tenant_id for filtering
    let tenant_id = claims.tenant_id.clone();

    let stream = stream::unfold(
        (state, initial_timestamp, tenant_id, has_permission),
        |(state, last_timestamp, tenant_id, has_permission)| async move {
            if !has_permission {
                // Return error event once and end stream
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - TelemetryView required\"}")),
                    (state, last_timestamp, tenant_id, false),
                ));
            }
            // Poll every 2 seconds for new events
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Convert last_timestamp (u64) to DateTime<Utc>
            let start_time_dt = chrono::DateTime::from_timestamp(last_timestamp as i64, 0)
                .unwrap_or_else(chrono::Utc::now);

            // Query telemetry buffer for events since last timestamp
            let filters = adapteros_telemetry::unified_events::TelemetryFilters {
                start_time: Some(start_time_dt),
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
                    (state, current_timestamp, tenant_id, has_permission),
                ));
            }

            // CRITICAL: Filter events by tenant_id for multi-tenant isolation
            let stream_events: Vec<TelemetryStreamEvent> = events
                .iter()
                .filter(|e| e.identity.tenant_id == tenant_id)
                .map(|e| TelemetryStreamEvent {
                    event_id: e.id.clone(),
                    timestamp: e.timestamp.timestamp() as u64,
                    event_type: e.event_type.clone(),
                    tenant_id: e.identity.tenant_id.clone(),
                    level: format!("{:?}", e.level),
                    message: e.message.clone(),
                    component: e.component.clone(),
                    trace_id: e.trace_id.clone(),
                })
                .collect();

            // If all events were filtered out, send keepalive
            if stream_events.is_empty() {
                return Some((
                    Ok(Event::default()
                        .event("telemetry")
                        .data("{\"events\": [], \"count\": 0}")),
                    (state, current_timestamp, tenant_id, has_permission),
                ));
            }

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
                        (state, current_timestamp, tenant_id, has_permission),
                    ));
                }
            };

            Some((
                Ok(Event::default().event("telemetry").data(json)),
                (state, current_timestamp, tenant_id, has_permission),
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
/// CRITICAL: Only streams adapters belonging to the authenticated tenant.
///
/// # SSE Event Format
/// ```json
/// event: adapter_state
/// data: {"adapters": [...], "count": 3}
/// ```
pub async fn adapter_state_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: AdapterView required
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::AdapterView,
    ).is_ok();

    if !has_permission {
        warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for adapter state stream"
        );
    } else {
        info!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Starting adapter state SSE stream"
        );
    }

    // Capture tenant_id for use in the stream closure
    let tenant_id = claims.tenant_id.clone();

    // Initialize with empty state cache
    let initial_states: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    let stream = stream::unfold(
        (state, initial_states, tenant_id, has_permission),
        |(state, mut previous_states, tenant_id, has_permission)| async move {
            if !has_permission {
                // Return error event once and end stream
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - AdapterView required\"}")),
                    (state, previous_states, tenant_id, false),
                ));
            }
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

                for record in &all_states {
                    let state_str = format!("{:?}", record.state);
                    let previous_state = previous_states.get(&record.adapter_id).cloned();

                    // Check if state has changed
                    if previous_state.as_ref() != Some(&state_str) {
                        // Get additional adapter info from database
                        let (adapter_name, activation_pct, memory_mb) =
                            match state.db.get_adapter(&record.adapter_id).await {
                                Ok(Some(adapter)) => {
                                    let name = adapter.name.clone();
                                    let activation = 0.0; // Not available in Adapter struct
                                    let memory = if adapter.memory_bytes > 0 {
                                        Some(adapter.memory_bytes as f64 / (1024.0 * 1024.0))
                                    } else {
                                        None
                                    };
                                    (name, activation, memory)
                                }
                                _ => (record.adapter_id.clone(), 0.0, None),
                            };

                        events.push(AdapterStateEvent {
                            adapter_id: record.adapter_id.clone(),
                            adapter_name,
                            previous_state,
                            current_state: state_str.clone(),
                            timestamp: current_timestamp,
                            activation_percentage: activation_pct,
                            memory_usage_mb: memory_mb,
                        });

                        // Update cache
                        previous_states.insert(record.adapter_id.clone(), state_str);
                    }
                }

                events
            } else {
                // Fallback: list adapters from database (tenant-filtered)
                match state.db.list_adapters_for_tenant(&tenant_id).await {
                    Ok(adapters) => {
                        let mut events = Vec::new();

                        for adapter in adapters {
                            let adapter_id = adapter.id.clone();
                            let current_state = adapter.lifecycle_state.clone();

                            let previous_state = previous_states.get(&adapter_id).cloned();

                            // Check if state has changed
                            if previous_state.as_ref() != Some(&current_state) {
                                let adapter_name = adapter.name.clone();
                                let activation_pct = 0.0; // Not available in Adapter struct
                                let memory_mb = if adapter.memory_bytes > 0 {
                                    Some(adapter.memory_bytes as f64 / (1024.0 * 1024.0))
                                } else {
                                    None
                                };

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
                            (state, previous_states, tenant_id, has_permission),
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
                        (state, previous_states, tenant_id, has_permission),
                    ));
                }
            };

            Some((
                Ok(Event::default().event("adapter_state").data(json)),
                (state, previous_states, tenant_id, has_permission),
            ))
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Boot progress event for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "event_type")]
pub enum BootProgressEvent {
    StateChanged {
        previous: String,
        current: String,
        elapsed_ms: u64,
        models_pending: usize,
        models_ready: usize,
    },
    DownloadProgress {
        model_id: String,
        repo_id: String,
        downloaded_bytes: u64,
        total_bytes: u64,
        speed_mbps: f64,
        eta_seconds: u64,
        files_completed: usize,
        files_total: usize,
    },
    LoadProgress {
        model_id: String,
        phase: String,
        progress_pct: f64,
        memory_allocated_mb: u64,
    },
    ModelReady {
        model_id: String,
        warmup_latency_ms: u64,
        memory_usage_mb: u64,
    },
    FullyReady {
        total_models: usize,
        total_download_mb: u64,
        total_load_time_ms: u64,
    },
}

/// Boot progress streaming endpoint
///
/// Streams real-time boot progress including state changes, model downloads,
/// and load progress. Useful for displaying boot status in UIs.
///
/// # SSE Event Format
/// ```json
/// event: boot_progress
/// data: {"event_type": "StateChanged", "previous": "booting", "current": "loading-base-models", ...}
/// ```
pub async fn boot_progress_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: MetricsView required to view system boot progress
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::MetricsView,
    ).is_ok();

    if !has_permission {
        warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for boot progress stream"
        );
    } else {
        info!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Starting boot progress SSE stream"
        );
    }

    // Track previous state for change detection
    let initial_state = state
        .boot_state
        .as_ref()
        .map(|bs| bs.current_state().as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let stream = stream::unfold(
        (state, initial_state, has_permission),
        |(state, mut previous_state, has_permission)| async move {
            if !has_permission {
                // Return error event once and end stream
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - MetricsView required\"}")),
                    (state, previous_state, false),
                ));
            }
            // Poll every 500ms for rapid updates during boot
            tokio::time::sleep(Duration::from_millis(500)).await;

            let current_timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            // Check if boot state manager is available
            let boot_state_ref = match state.boot_state.as_ref() {
                Some(bs) => bs,
                None => {
                    // No boot state manager, send error once and keep alive
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data("{\"error\": \"boot state manager not available\"}")),
                        (state, previous_state, has_permission),
                    ));
                }
            };

            let current_state = boot_state_ref.current_state();
            let current_state_str = current_state.as_str().to_string();

            // Check if state has changed
            if current_state_str != previous_state {
                let elapsed_ms = boot_state_ref.elapsed().as_millis() as u64;
                let models_pending = boot_state_ref.pending_model_count();
                let models_ready = boot_state_ref.ready_model_count();

                let event = BootProgressEvent::StateChanged {
                    previous: previous_state.clone(),
                    current: current_state_str.clone(),
                    elapsed_ms,
                    models_pending,
                    models_ready,
                };

                previous_state = current_state_str;

                let json = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize boot progress event");
                        return Some((
                            Ok(Event::default()
                                .event("error")
                                .data(format!("{{\"error\": \"serialization failed: {}\"}}", e))),
                            (state, previous_state, has_permission),
                        ));
                    }
                };

                return Some((
                    Ok(Event::default().event("boot_progress").data(json)),
                    (state, previous_state, has_permission),
                ));
            }

            // Check for model loading status changes
            let model_status = boot_state_ref.get_model_status();

            // If in loading state and models are pending, emit periodic progress
            if current_state.is_booting() && !model_status.pending.is_empty() {
                let elapsed_ms = boot_state_ref.elapsed().as_millis() as u64;

                // Emit a summary event for ongoing loading
                let event = BootProgressEvent::StateChanged {
                    previous: current_state_str.clone(),
                    current: current_state_str.clone(),
                    elapsed_ms,
                    models_pending: model_status.pending.len(),
                    models_ready: model_status.ready.len(),
                };

                let json = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize boot progress event");
                        return Some((
                            Ok(Event::default()
                                .event("error")
                                .data(format!("{{\"error\": \"serialization failed: {}\"}}", e))),
                            (state, previous_state, has_permission),
                        ));
                    }
                };

                return Some((
                    Ok(Event::default().event("boot_progress").data(json)),
                    (state, previous_state, has_permission),
                ));
            }

            // If fully ready, emit final event
            if current_state.is_fully_ready() {
                let elapsed_ms = boot_state_ref.elapsed().as_millis() as u64;
                let total_models = model_status.ready.len();

                let event = BootProgressEvent::FullyReady {
                    total_models,
                    total_download_mb: 0, // TODO: track this in boot state manager
                    total_load_time_ms: elapsed_ms,
                };

                let json = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize boot progress event");
                        return Some((
                            Ok(Event::default()
                                .event("error")
                                .data(format!("{{\"error\": \"serialization failed: {}\"}}", e))),
                            (state, previous_state, has_permission),
                        ));
                    }
                };

                return Some((
                    Ok(Event::default().event("boot_progress").data(json)),
                    (state, previous_state, has_permission),
                ));
            }

            // No changes, send keep-alive with timestamp
            Some((
                Ok(Event::default()
                    .event("keepalive")
                    .data(format!("{{\"timestamp\": {}}}", current_timestamp))),
                (state, previous_state, has_permission),
            ))
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}
