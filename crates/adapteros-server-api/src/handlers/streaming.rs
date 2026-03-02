//! Streaming endpoint handlers
//!
//! Provides real-time streaming APIs for system metrics, telemetry,
//! adapter states, and other continuous data feeds using Server-Sent Events (SSE).
//!
//! [2025-01-20 modularity streaming_handlers]

use crate::auth::Claims;
use crate::state::{AppState, StreamingConfig};
use adapteros_core::identity::IdentityEnvelope;
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tracing::{info, warn};
use utoipa::ToSchema;

// ============================================================================
// ID Obfuscation for User-Facing Streams
// ============================================================================

/// Per-stream salt for ID obfuscation. Generated once per SSE connection
/// to prevent correlation of internal IDs across different streaming sessions.
struct StreamSalt([u8; 32]);

impl StreamSalt {
    fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let mut salt = [0u8; 32];
        // Use a combination of timestamp and random-like data for the salt
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        salt[..16].copy_from_slice(&ts.to_le_bytes());
        // Fill remaining bytes with additional entropy from address space
        let ptr = &salt as *const _ as usize;
        salt[16..24].copy_from_slice(&ptr.to_le_bytes());
        Self(salt)
    }
}

/// Obfuscate an internal ID for user-facing output using BLAKE3 keyed hash.
/// Returns a 16-character hex string that cannot be reversed to the original ID.
/// The same internal ID will produce the same obfuscated ID within a single
/// streaming session (using the same salt), but different IDs across sessions.
fn obfuscate_id(internal_id: &str, salt: &StreamSalt) -> String {
    let hash = blake3::keyed_hash(&salt.0, internal_id.as_bytes());
    // Return first 8 bytes (16 hex chars) for a compact but collision-resistant ID
    hex::encode(&hash.as_bytes()[..8])
}

/// HEAD handler for SSE endpoints - returns 200 OK for preflight checks
/// This allows clients to verify endpoint availability without starting a stream
pub async fn sse_preflight_check() -> StatusCode {
    StatusCode::OK
}

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
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: MetricsView required for system metrics
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::MetricsView,
    )
    .is_ok();

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

    // Track whether error has been sent to terminate stream after one error
    let stream = stream::unfold(
        (state, has_permission, false, None::<(f64, u64)>),
        |(state, has_permission, error_sent, prev)| async move {
            if !has_permission {
                if error_sent {
                    // Already sent error, terminate stream
                    return None;
                }
                // Return error event once then terminate
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - MetricsView required\"}")),
                    (state, false, true, prev), // Mark error as sent
                ));
            }
            // Sleep for 5 seconds between updates
            tokio::time::sleep(Duration::from_secs(5)).await;

            // Build a snapshot from real sources (metrics exporter + system metrics)
            let exporter_snapshot = state.metrics_exporter.snapshot();
            let mut system_collector = adapteros_system_metrics::SystemMetricsCollector::new();
            let system_metrics = system_collector.collect_metrics();

            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;

            let inferences_per_second = prev
                .and_then(|(prev_total, prev_ts)| {
                    let elapsed_ms = now_ms.saturating_sub(prev_ts);
                    if elapsed_ms == 0 {
                        return None;
                    }
                    let delta = exporter_snapshot.total_requests - prev_total;
                    if delta <= 0.0 {
                        return None;
                    }
                    Some(delta / (elapsed_ms as f64 / 1000.0))
                })
                .unwrap_or(0.0);

            // Convert to our streaming event format
            let event = MetricsSnapshotEvent {
                timestamp_ms: now_ms,
                latency: StreamingLatencyMetrics {
                    p50_ms: exporter_snapshot.avg_latency_ms,
                    p95_ms: exporter_snapshot.avg_latency_ms,
                    p99_ms: exporter_snapshot.avg_latency_ms,
                },
                throughput: StreamingThroughputMetrics {
                    tokens_per_second: 0.0,
                    inferences_per_second,
                },
                system: StreamingSystemMetrics {
                    cpu_percent: system_metrics.cpu_usage,
                    memory_percent: system_metrics.memory_usage,
                    disk_percent: system_metrics.disk_io.usage_percent as f64,
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
                        (
                            state,
                            has_permission,
                            false,
                            Some((exporter_snapshot.total_requests, now_ms)),
                        ),
                    ));
                }
            };

            Some((
                Ok(Event::default().event("metrics").data(json)),
                (
                    state,
                    has_permission,
                    false,
                    Some((exporter_snapshot.total_requests, now_ms)),
                ),
            ))
        },
    );

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
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: TelemetryView required
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::TelemetryView,
    )
    .is_ok();

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

    // Generate per-stream salt for ID obfuscation to prevent correlation across sessions
    let salt = StreamSalt::new();

    // Track error_sent to terminate stream after one error
    let stream = stream::unfold(
        (
            state,
            initial_timestamp,
            tenant_id,
            has_permission,
            false,
            salt,
        ),
        |(state, last_timestamp, tenant_id, has_permission, error_sent, salt)| async move {
            if !has_permission {
                if error_sent {
                    // Already sent error, terminate stream
                    return None;
                }
                // Return error event once then terminate
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - TelemetryView required\"}")),
                    (state, last_timestamp, tenant_id, false, true, salt),
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
                tenant_id: Some(tenant_id.clone()),
                ..Default::default()
            };

            let events = match state.telemetry_buffer.query(&filters) {
                Ok(events) => events,
                Err(err) => {
                    warn!(error = %err, "Failed to query telemetry buffer for stream");
                    let error_json = format!("{{\"error\": \"telemetry query failed: {}\"}}", err);
                    return Some((
                        Ok(Event::default().event("error").data(error_json)),
                        (state, last_timestamp, tenant_id, has_permission, true, salt),
                    ));
                }
            };

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
                    (
                        state,
                        current_timestamp,
                        tenant_id,
                        has_permission,
                        false,
                        salt,
                    ),
                ));
            }

            // CRITICAL: Filter events by tenant_id for multi-tenant isolation
            // NOTE: Internal IDs (event_id, trace_id) are obfuscated to prevent
            // leaking internal system identifiers to user-facing streams
            let stream_events: Vec<TelemetryStreamEvent> = events
                .iter()
                .filter(|e| e.identity.tenant_id == tenant_id)
                .map(|e| TelemetryStreamEvent {
                    event_id: obfuscate_id(&e.id, &salt),
                    timestamp: e.timestamp.timestamp() as u64,
                    event_type: e.event_type.clone(),
                    tenant_id: e.identity.tenant_id.clone(),
                    level: format!("{:?}", e.level),
                    message: e.message.clone(),
                    component: e.component.clone(),
                    trace_id: e.trace_id.as_ref().map(|id| obfuscate_id(id, &salt)),
                })
                .collect();

            // If all events were filtered out, send keepalive
            if stream_events.is_empty() {
                return Some((
                    Ok(Event::default()
                        .event("telemetry")
                        .data("{\"events\": [], \"count\": 0}")),
                    (
                        state,
                        current_timestamp,
                        tenant_id,
                        has_permission,
                        false,
                        salt,
                    ),
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
                        (
                            state,
                            current_timestamp,
                            tenant_id,
                            has_permission,
                            false,
                            salt,
                        ),
                    ));
                }
            };

            Some((
                Ok(Event::default().event("telemetry").data(json)),
                (
                    state,
                    current_timestamp,
                    tenant_id,
                    has_permission,
                    false,
                    salt,
                ),
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
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: AdapterView required
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::AdapterView,
    )
    .is_ok();

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

    // Track error_sent to terminate stream after one error
    let stream = stream::unfold(
        (state, initial_states, tenant_id, has_permission, false),
        |(state, mut previous_states, tenant_id, has_permission, error_sent)| async move {
            if !has_permission {
                if error_sent {
                    // Already sent error, terminate stream
                    return None;
                }
                // Return error event once then terminate
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - AdapterView required\"}")),
                    (state, previous_states, tenant_id, false, true),
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
                let manager = lifecycle_manager;
                let all_states = manager.get_all_states();

                let mut events = Vec::new();

                for record in &all_states {
                    let state_str = format!("{:?}", record.state);
                    let previous_state = previous_states.get(&record.adapter_id).cloned();

                    // Check if state has changed
                    if previous_state.as_ref() != Some(&state_str) {
                        // PRD-RECT-001: Use tenant-scoped query to prevent cross-tenant data leakage.
                        // Only show adapter details for adapters belonging to the current tenant.
                        let (adapter_name, activation_pct, memory_mb) = match state
                            .db
                            .get_adapter_for_tenant(&tenant_id, &record.adapter_id)
                            .await
                        {
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
                            // Cross-tenant or not found: use fallback info (no details leaked)
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
                            (state, previous_states, tenant_id, has_permission, false),
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
                        (state, previous_states, tenant_id, has_permission, false),
                    ));
                }
            };

            Some((
                Ok(Event::default().event("adapter_state").data(json)),
                (state, previous_states, tenant_id, has_permission, false),
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
#[utoipa::path(
    get,
    path = "/v1/stream/boot-progress",
    responses(
        (status = 200, description = "Boot progress stream (SSE)")
    ),
    tag = "streams"
)]
pub async fn boot_progress_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: MetricsView required to view system boot progress
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::MetricsView,
    )
    .is_ok();

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

    // Track error_sent to terminate stream after one error
    let stream = stream::unfold(
        (state, initial_state, has_permission, false),
        |(state, mut previous_state, has_permission, error_sent)| async move {
            if !has_permission {
                if error_sent {
                    // Already sent error, terminate stream
                    return None;
                }
                // Return error event once then terminate
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - MetricsView required\"}")),
                    (state, previous_state, false, true),
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
                        (state, previous_state, has_permission, false),
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
                            (state, previous_state, has_permission, false),
                        ));
                    }
                };

                return Some((
                    Ok(Event::default().event("boot_progress").data(json)),
                    (state, previous_state, has_permission, false),
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
                            (state, previous_state, has_permission, false),
                        ));
                    }
                };

                return Some((
                    Ok(Event::default().event("boot_progress").data(json)),
                    (state, previous_state, has_permission, false),
                ));
            }

            // If fully ready, emit final event
            if current_state.is_fully_ready() {
                let elapsed_ms = boot_state_ref.elapsed().as_millis() as u64;
                let total_models = model_status.ready.len();
                let total_download_mb = boot_state_ref.total_download_mb();

                let event = BootProgressEvent::FullyReady {
                    total_models,
                    total_download_mb,
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
                            (state, previous_state, has_permission, false),
                        ));
                    }
                };

                return Some((
                    Ok(Event::default().event("boot_progress").data(json)),
                    (state, previous_state, has_permission, false),
                ));
            }

            // No changes, send keep-alive with timestamp
            Some((
                Ok(Event::default()
                    .event("keepalive")
                    .data(format!("{{\"timestamp\": {}}}", current_timestamp))),
                (state, previous_state, has_permission, false),
            ))
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}

/// Notifications SSE stream endpoint
///
/// Streams real-time notifications for the authenticated user.
/// Polls for new notifications every 5 seconds and emits changes.
///
/// # SSE Event Format
/// ```json
/// event: notification
/// data: {"notifications": [...], "unread_count": 3, "timestamp": "..."}
/// ```
#[utoipa::path(
    tag = "notifications",
    get,
    path = "/v1/stream/notifications",
    responses(
        (status = 200, description = "SSE stream of notifications")
    )
)]
pub async fn notifications_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    use crate::permissions::require_permission;
    use crate::permissions::Permission;

    // Permission check
    let has_permission = require_permission(&claims, Permission::NotificationView).is_ok();

    if !has_permission {
        warn!(
            user_id = %claims.sub,
            "Permission denied for notifications stream"
        );
    } else {
        info!(
            user_id = %claims.sub,
            "Starting notifications SSE stream"
        );
    }

    // Capture user_id for use in the stream closure
    let user_id = claims.sub.clone();

    // Initialize with empty notification cache
    let previous_notification_ids: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // Track error_sent to terminate stream after one error
    let stream = stream::unfold(
        (
            state,
            previous_notification_ids,
            user_id,
            has_permission,
            false,
        ),
        |(state, mut previous_ids, user_id, has_permission, error_sent)| async move {
            if !has_permission {
                if error_sent {
                    // Already sent error, terminate stream
                    return None;
                }
                // Return error event once then terminate
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - NotificationView required\"}")),
                    (state, previous_ids, user_id, false, true),
                ));
            }

            // Poll every 5 seconds for new notifications
            tokio::time::sleep(Duration::from_secs(5)).await;

            // Fetch recent unread notifications
            let notifications = match state
                .db
                .list_user_notifications(&user_id, None, None, true, Some(50), Some(0))
                .await
            {
                Ok(notifications) => notifications,
                Err(e) => {
                    warn!("Failed to fetch notifications for SSE: {}", e);
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        (state, previous_ids, user_id, has_permission, false),
                    ));
                }
            };

            // Check if there are new notifications
            let current_ids: std::collections::HashSet<String> =
                notifications.iter().map(|n| n.id.clone()).collect();

            // Only emit if there are changes
            if current_ids != previous_ids {
                let notification_data = serde_json::json!({
                    "notifications": notifications.iter().map(|n| serde_json::json!({
                        "id": n.id,
                        "user_id": n.user_id,
                        "workspace_id": n.workspace_id,
                        "type": n.type_,
                        "target_type": n.target_type,
                        "target_id": n.target_id,
                        "title": n.title,
                        "content": n.content,
                        "read_at": n.read_at,
                        "created_at": n.created_at,
                    })).collect::<Vec<_>>(),
                    "unread_count": notifications.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });

                let json = match serde_json::to_string(&notification_data) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!("Failed to serialize notifications: {}", e);
                        return Some((
                            Ok(Event::default()
                                .event("error")
                                .data("{\"error\": \"serialization failed\"}")),
                            (state, previous_ids, user_id, has_permission, false),
                        ));
                    }
                };

                previous_ids = current_ids;

                Some((
                    Ok(Event::default().event("notification").data(json)),
                    (state, previous_ids, user_id, has_permission, false),
                ))
            } else {
                // No changes, but keep connection alive with heartbeat
                let heartbeat = serde_json::json!({
                    "type": "heartbeat",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "unread_count": notifications.len(),
                });

                let heartbeat_json = match serde_json::to_string(&heartbeat) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize notification heartbeat");
                        "{}".to_string()
                    }
                };
                Some((
                    Ok(Event::default().event("heartbeat").data(heartbeat_json)),
                    (state, previous_ids, user_id, has_permission, false),
                ))
            }
        },
    );

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ============================================================================
// Circuit Breaker for SSE Streams
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamBreakerState {
    Closed,
    Open { opened_at: std::time::Instant },
    HalfOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamBreakerTransition {
    Opened,
    HalfOpen,
    Recovered,
}

impl StreamBreakerTransition {
    fn as_str(self) -> &'static str {
        match self {
            Self::Opened => "open",
            Self::HalfOpen => "half_open",
            Self::Recovered => "recover",
        }
    }
}

/// Circuit breaker state for SSE stream error handling
#[derive(Debug, Clone)]
struct StreamCircuitBreaker {
    /// Number of consecutive errors before opening circuit
    threshold: u32,
    /// Current consecutive error count
    error_count: u32,
    /// Current breaker state
    state: StreamBreakerState,
    /// Recovery timeout before trying half-open state
    recovery_timeout: Duration,
}

impl Default for StreamCircuitBreaker {
    fn default() -> Self {
        let (threshold, recovery_timeout) = StreamingConfig::default().sse_breaker_settings();
        Self {
            threshold,
            error_count: 0,
            state: StreamBreakerState::Closed,
            recovery_timeout,
        }
    }
}

impl StreamCircuitBreaker {
    fn new(threshold: u32, recovery_timeout: Duration) -> Self {
        Self {
            threshold: threshold.max(1),
            recovery_timeout,
            ..Default::default()
        }
    }

    fn record_success(&mut self) -> Option<StreamBreakerTransition> {
        let was_half_open = matches!(self.state, StreamBreakerState::HalfOpen);
        self.error_count = 0;
        self.state = StreamBreakerState::Closed;
        if was_half_open {
            Some(StreamBreakerTransition::Recovered)
        } else {
            None
        }
    }

    fn record_error(&mut self) -> Option<StreamBreakerTransition> {
        match self.state {
            StreamBreakerState::HalfOpen => {
                self.error_count = self.threshold;
                self.state = StreamBreakerState::Open {
                    opened_at: std::time::Instant::now(),
                };
                warn!(
                    error_count = self.error_count,
                    threshold = self.threshold,
                    "SSE stream circuit breaker reopened from half-open"
                );
                Some(StreamBreakerTransition::Opened)
            }
            _ => {
                self.error_count += 1;
                if self.error_count < self.threshold {
                    return None;
                }

                self.state = StreamBreakerState::Open {
                    opened_at: std::time::Instant::now(),
                };
                warn!(
                    error_count = self.error_count,
                    threshold = self.threshold,
                    "SSE stream circuit breaker opened"
                );
                Some(StreamBreakerTransition::Opened)
            }
        }
    }

    fn should_allow(&mut self) -> (bool, Option<StreamBreakerTransition>) {
        match self.state {
            StreamBreakerState::Closed | StreamBreakerState::HalfOpen => (true, None),
            StreamBreakerState::Open { opened_at } => {
                if opened_at.elapsed() < self.recovery_timeout {
                    return (false, None);
                }

                self.state = StreamBreakerState::HalfOpen;
                warn!(
                    threshold = self.threshold,
                    recovery_timeout_secs = self.recovery_timeout_secs(),
                    "SSE stream circuit breaker entering half-open"
                );
                (true, Some(StreamBreakerTransition::HalfOpen))
            }
        }
    }

    fn recovery_timeout_secs(&self) -> u64 {
        self.recovery_timeout.as_secs().max(1)
    }

    fn state_metric_value(&self) -> f64 {
        match self.state {
            StreamBreakerState::Closed => 0.0,
            StreamBreakerState::HalfOpen => 1.0,
            StreamBreakerState::Open { .. } => 2.0,
        }
    }
}

fn stream_breaker_settings(state: &AppState) -> (u32, Duration) {
    match state.config.read() {
        Ok(config) => config.streaming.sse_breaker_settings(),
        Err(_) => {
            warn!("Configuration lock poisoned; using default SSE circuit breaker settings");
            StreamingConfig::default().sse_breaker_settings()
        }
    }
}

fn stream_breaker_transition_metadata(
    stream_name: &str,
    workspace_id: &str,
    cb: &StreamCircuitBreaker,
    transition: StreamBreakerTransition,
) -> serde_json::Value {
    serde_json::json!({
        "stream_name": stream_name,
        "workspace_id": workspace_id,
        "transition": transition.as_str(),
        "threshold": cb.threshold,
        "error_count": cb.error_count,
        "recovery_timeout_secs": cb.recovery_timeout_secs(),
    })
}

async fn emit_stream_breaker_transition(
    state: &AppState,
    tenant_id: &str,
    stream_name: &str,
    workspace_id: &str,
    cb: &StreamCircuitBreaker,
    transition: StreamBreakerTransition,
) {
    let transition_label = transition.as_str();
    state
        .metrics_registry
        .record_metric(
            format!(
                "streaming.circuit_breaker.{}.{}.total",
                stream_name, transition_label
            ),
            1.0,
        )
        .await;
    state
        .metrics_registry
        .set_gauge(
            format!("streaming.circuit_breaker.{}.state", stream_name),
            cb.state_metric_value(),
        )
        .await;

    let identity = IdentityEnvelope::new(
        tenant_id.to_string(),
        "server_api".to_string(),
        "streaming_sse".to_string(),
        env!("CARGO_PKG_VERSION").to_string(),
    );
    let level = if transition == StreamBreakerTransition::Opened {
        LogLevel::Warn
    } else {
        LogLevel::Info
    };
    let event = match TelemetryEventBuilder::new(
        EventType::Custom("streaming.circuit_breaker.transition".to_string()),
        level,
        format!(
            "SSE circuit breaker {} for {} stream",
            transition_label, stream_name
        ),
        identity,
    )
    .component("streaming.sse".to_string())
    .metadata(stream_breaker_transition_metadata(
        stream_name,
        workspace_id,
        cb,
        transition,
    ))
    .build()
    {
        Ok(event) => event,
        Err(err) => {
            warn!(
                error = %err,
                stream_name = stream_name,
                "Failed to build circuit breaker telemetry event"
            );
            return;
        }
    };

    if let Err(err) = state.telemetry_buffer.push(event.clone()).await {
        warn!(
            error = %err,
            stream_name = stream_name,
            "Failed to buffer circuit breaker telemetry event"
        );
    }
    if let Err(err) = state.telemetry_tx.send(event) {
        warn!(
            error = %err,
            stream_name = stream_name,
            "Failed to broadcast circuit breaker telemetry event"
        );
    }
}

// ============================================================================
// Workspace Messages Stream
// ============================================================================

/// Message event for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MessageStreamEvent {
    pub id: String,
    pub workspace_id: String,
    pub from_user_id: String,
    pub content: String,
    pub thread_id: Option<String>,
    pub created_at: String,
    pub edited_at: Option<String>,
}

/// Workspace messages streaming endpoint
///
/// Streams real-time messages for a workspace. Polls for new messages
/// every 2 seconds and emits changes. Uses circuit breaker for error handling.
///
/// # SSE Event Format
/// ```json
/// event: message
/// data: {"messages": [...], "count": 3, "timestamp": "..."}
/// ```
#[utoipa::path(
    tag = "streaming",
    get,
    path = "/v1/stream/messages/{workspace_id}",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID to stream messages for")
    ),
    responses(
        (status = 200, description = "SSE stream of workspace messages")
    )
)]
pub async fn messages_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    use crate::permissions::require_permission;
    use crate::permissions::Permission;

    let workspace_id = match crate::id_resolver::resolve_any_id(&state.db, &workspace_id).await {
        Ok(id) => id,
        Err(err) => {
            warn!(error = %err, workspace_id = %workspace_id, "Failed to resolve workspace ID");
            workspace_id
        }
    };

    // Permission check: WorkspaceView required
    let has_permission = require_permission(&claims, Permission::WorkspaceView).is_ok();

    if !has_permission {
        warn!(
            user_id = %claims.sub,
            workspace_id = %workspace_id,
            "Permission denied for messages stream"
        );
    } else {
        info!(
            user_id = %claims.sub,
            workspace_id = %workspace_id,
            "Starting messages SSE stream"
        );
    }

    let tenant_id = claims.tenant_id.clone();

    // Initialize circuit breaker using config defaults/overrides.
    let (breaker_threshold, breaker_recovery_timeout) = stream_breaker_settings(&state);
    let retry_after_secs = breaker_recovery_timeout.as_secs().max(1);
    let circuit_breaker = StreamCircuitBreaker::new(breaker_threshold, breaker_recovery_timeout);

    // Track last seen message ID
    let last_message_id: Option<String> = None;

    // Track error_sent to terminate stream after one error
    let stream = stream::unfold(
        (
            state,
            workspace_id,
            tenant_id,
            has_permission,
            false,
            last_message_id,
            circuit_breaker,
            retry_after_secs,
        ),
        |(
            state,
            workspace_id,
            tenant_id,
            has_permission,
            error_sent,
            mut last_id,
            mut cb,
            retry_after_secs,
        )| async move {
            if !has_permission {
                if error_sent {
                    return None;
                }
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - WorkspaceView required\"}")),
                    (
                        state,
                        workspace_id,
                        tenant_id,
                        false,
                        true,
                        last_id,
                        cb,
                        retry_after_secs,
                    ),
                ));
            }

            // Check circuit breaker
            let (allow_breaker, transition) = cb.should_allow();
            if let Some(transition) = transition {
                emit_stream_breaker_transition(
                    &state,
                    &tenant_id,
                    "messages",
                    &workspace_id,
                    &cb,
                    transition,
                )
                .await;
            }
            if !allow_breaker {
                // Circuit is open, wait and emit status
                tokio::time::sleep(Duration::from_secs(5)).await;
                return Some((
                    Ok(Event::default().event("circuit_open").data(format!(
                        "{{\"status\": \"circuit_breaker_open\", \"retry_after_secs\": {}}}",
                        retry_after_secs
                    ))),
                    (
                        state,
                        workspace_id,
                        tenant_id,
                        has_permission,
                        error_sent,
                        last_id,
                        cb,
                        retry_after_secs,
                    ),
                ));
            }

            // Poll every 2 seconds for new messages
            tokio::time::sleep(Duration::from_secs(2)).await;

            // Fetch recent messages (passing None for `since` to get all recent messages)
            let messages = match state
                .db
                .get_recent_workspace_messages(&workspace_id, None)
                .await
            {
                Ok(msgs) => {
                    if let Some(transition) = cb.record_success() {
                        emit_stream_breaker_transition(
                            &state,
                            &tenant_id,
                            "messages",
                            &workspace_id,
                            &cb,
                            transition,
                        )
                        .await;
                    }
                    msgs
                }
                Err(e) => {
                    if let Some(transition) = cb.record_error() {
                        emit_stream_breaker_transition(
                            &state,
                            &tenant_id,
                            "messages",
                            &workspace_id,
                            &cb,
                            transition,
                        )
                        .await;
                    }
                    warn!(error = %e, workspace_id = %workspace_id, "Failed to fetch messages for SSE");
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        (
                            state,
                            workspace_id,
                            tenant_id,
                            has_permission,
                            error_sent,
                            last_id,
                            cb,
                            retry_after_secs,
                        ),
                    ));
                }
            };

            // Check if there are new messages
            let has_new = messages
                .first()
                .map(|m| Some(&m.id) != last_id.as_ref())
                .unwrap_or(false);

            if has_new || last_id.is_none() {
                // Update last seen ID
                if let Some(first) = messages.first() {
                    last_id = Some(first.id.clone());
                }

                let message_events: Vec<MessageStreamEvent> = messages
                    .iter()
                    .map(|m| MessageStreamEvent {
                        id: m.id.clone(),
                        workspace_id: m.workspace_id.clone(),
                        from_user_id: m.from_user_id.clone(),
                        content: m.content.clone(),
                        thread_id: m.thread_id.clone(),
                        created_at: m.created_at.clone(),
                        edited_at: m.edited_at.clone(),
                    })
                    .collect();

                let payload = serde_json::json!({
                    "messages": message_events,
                    "count": message_events.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });

                let json = match serde_json::to_string(&payload) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize messages");
                        return Some((
                            Ok(Event::default()
                                .event("error")
                                .data("{\"error\": \"serialization failed\"}")),
                            (
                                state,
                                workspace_id,
                                tenant_id,
                                has_permission,
                                error_sent,
                                last_id,
                                cb,
                                retry_after_secs,
                            ),
                        ));
                    }
                };

                Some((
                    Ok(Event::default().event("message").data(json)),
                    (
                        state,
                        workspace_id,
                        tenant_id,
                        has_permission,
                        error_sent,
                        last_id,
                        cb,
                        retry_after_secs,
                    ),
                ))
            } else {
                // No changes, emit heartbeat
                let heartbeat = serde_json::json!({
                    "type": "heartbeat",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });

                let heartbeat_json = match serde_json::to_string(&heartbeat) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize message heartbeat");
                        "{}".to_string()
                    }
                };
                Some((
                    Ok(Event::default().event("heartbeat").data(heartbeat_json)),
                    (
                        state,
                        workspace_id,
                        tenant_id,
                        has_permission,
                        error_sent,
                        last_id,
                        cb,
                        retry_after_secs,
                    ),
                ))
            }
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

// ============================================================================
// Workspace Activity Stream
// ============================================================================

/// Activity event for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ActivityStreamEvent {
    pub id: String,
    pub workspace_id: Option<String>,
    pub user_id: String,
    pub event_type: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

/// Workspace activity streaming endpoint
///
/// Streams real-time activity events for a workspace. Polls for new events
/// every 3 seconds and emits changes. Uses circuit breaker for error handling.
///
/// # SSE Event Format
/// ```json
/// event: activity
/// data: {"events": [...], "count": 5, "timestamp": "..."}
/// ```
#[utoipa::path(
    tag = "streaming",
    get,
    path = "/v1/stream/activity/{workspace_id}",
    params(
        ("workspace_id" = String, Path, description = "Workspace ID to stream activity for")
    ),
    responses(
        (status = 200, description = "SSE stream of workspace activity")
    )
)]
pub async fn activity_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    axum::extract::Path(workspace_id): axum::extract::Path<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    use crate::permissions::require_permission;
    use crate::permissions::Permission;

    let workspace_id = match crate::id_resolver::resolve_any_id(&state.db, &workspace_id).await {
        Ok(id) => id,
        Err(err) => {
            warn!(error = %err, workspace_id = %workspace_id, "Failed to resolve workspace ID");
            workspace_id
        }
    };

    // Permission check: ActivityView required
    let has_permission = require_permission(&claims, Permission::ActivityView).is_ok();

    if !has_permission {
        warn!(
            user_id = %claims.sub,
            workspace_id = %workspace_id,
            "Permission denied for activity stream"
        );
    } else {
        info!(
            user_id = %claims.sub,
            workspace_id = %workspace_id,
            "Starting activity SSE stream"
        );
    }

    let tenant_id = claims.tenant_id.clone();
    let user_id = claims.sub.clone();

    // Initialize circuit breaker using config defaults/overrides.
    let (breaker_threshold, breaker_recovery_timeout) = stream_breaker_settings(&state);
    let retry_after_secs = breaker_recovery_timeout.as_secs().max(1);
    let circuit_breaker = StreamCircuitBreaker::new(breaker_threshold, breaker_recovery_timeout);

    // Track last seen timestamp
    let last_timestamp = chrono::Utc::now();

    // Track error_sent to terminate stream after one error
    let stream = stream::unfold(
        (
            state,
            workspace_id,
            tenant_id,
            user_id,
            has_permission,
            false,
            last_timestamp,
            circuit_breaker,
            retry_after_secs,
        ),
        |(
            state,
            workspace_id,
            tenant_id,
            user_id,
            has_permission,
            error_sent,
            mut last_ts,
            mut cb,
            retry_after_secs,
        )| async move {
            if !has_permission {
                if error_sent {
                    return None;
                }
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - ActivityView required\"}")),
                    (
                        state,
                        workspace_id,
                        tenant_id,
                        user_id,
                        false,
                        true,
                        last_ts,
                        cb,
                        retry_after_secs,
                    ),
                ));
            }

            // Check circuit breaker
            let (allow_breaker, transition) = cb.should_allow();
            if let Some(transition) = transition {
                emit_stream_breaker_transition(
                    &state,
                    &tenant_id,
                    "activity",
                    &workspace_id,
                    &cb,
                    transition,
                )
                .await;
            }
            if !allow_breaker {
                tokio::time::sleep(Duration::from_secs(5)).await;
                return Some((
                    Ok(Event::default().event("circuit_open").data(format!(
                        "{{\"status\": \"circuit_breaker_open\", \"retry_after_secs\": {}}}",
                        retry_after_secs
                    ))),
                    (
                        state,
                        workspace_id,
                        tenant_id,
                        user_id,
                        has_permission,
                        error_sent,
                        last_ts,
                        cb,
                        retry_after_secs,
                    ),
                ));
            }

            // Poll every 3 seconds for new activity
            tokio::time::sleep(Duration::from_secs(3)).await;

            // Fetch recent activity events for this workspace
            let events = match state
                .db
                .list_activity_events_since(&workspace_id, Some(&last_ts.to_rfc3339()), Some(50))
                .await
            {
                Ok(evts) => {
                    if let Some(transition) = cb.record_success() {
                        emit_stream_breaker_transition(
                            &state,
                            &tenant_id,
                            "activity",
                            &workspace_id,
                            &cb,
                            transition,
                        )
                        .await;
                    }
                    evts
                }
                Err(e) => {
                    if let Some(transition) = cb.record_error() {
                        emit_stream_breaker_transition(
                            &state,
                            &tenant_id,
                            "activity",
                            &workspace_id,
                            &cb,
                            transition,
                        )
                        .await;
                    }
                    warn!(error = %e, workspace_id = %workspace_id, "Failed to fetch activity for SSE");
                    return Some((
                        Ok(Event::default()
                            .event("error")
                            .data(format!("{{\"error\": \"{}\"}}", e))),
                        (
                            state,
                            workspace_id,
                            tenant_id,
                            user_id,
                            has_permission,
                            error_sent,
                            last_ts,
                            cb,
                            retry_after_secs,
                        ),
                    ));
                }
            };

            // Filter to workspace-specific events
            let workspace_events: Vec<_> = events
                .iter()
                .filter(|e| e.workspace_id.as_ref() == Some(&workspace_id))
                .collect();

            // Update timestamp
            last_ts = chrono::Utc::now();

            if !workspace_events.is_empty() {
                let activity_events: Vec<ActivityStreamEvent> = workspace_events
                    .iter()
                    .map(|e| ActivityStreamEvent {
                        id: e.id.clone(),
                        workspace_id: e.workspace_id.clone(),
                        user_id: e.user_id.clone(),
                        event_type: e.event_type.clone(),
                        target_type: e.target_type.clone(),
                        target_id: e.target_id.clone(),
                        metadata: e
                            .metadata_json
                            .as_ref()
                            .and_then(|j| serde_json::from_str(j).ok()),
                        created_at: e.created_at.clone(),
                    })
                    .collect();

                let payload = serde_json::json!({
                    "events": activity_events,
                    "count": activity_events.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });

                let json = match serde_json::to_string(&payload) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize activity events");
                        return Some((
                            Ok(Event::default()
                                .event("error")
                                .data("{\"error\": \"serialization failed\"}")),
                            (
                                state,
                                workspace_id,
                                tenant_id,
                                user_id,
                                has_permission,
                                error_sent,
                                last_ts,
                                cb,
                                retry_after_secs,
                            ),
                        ));
                    }
                };

                Some((
                    Ok(Event::default().event("activity").data(json)),
                    (
                        state,
                        workspace_id,
                        tenant_id,
                        user_id,
                        has_permission,
                        error_sent,
                        last_ts,
                        cb,
                        retry_after_secs,
                    ),
                ))
            } else {
                // No new events, emit heartbeat
                let heartbeat = serde_json::json!({
                    "type": "heartbeat",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                });

                let heartbeat_json = match serde_json::to_string(&heartbeat) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize activity heartbeat");
                        "{}".to_string()
                    }
                };
                Some((
                    Ok(Event::default().event("heartbeat").data(heartbeat_json)),
                    (
                        state,
                        workspace_id,
                        tenant_id,
                        user_id,
                        has_permission,
                        error_sent,
                        last_ts,
                        cb,
                        retry_after_secs,
                    ),
                ))
            }
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

/// Trace receipt event for SSE streaming
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TraceReceiptEvent {
    /// Trace ID
    pub trace_id: String,
    /// Tenant ID
    pub tenant_id: String,
    /// Receipt digest (hex)
    pub receipt_digest: String,
    /// Output digest (hex)
    pub output_digest: String,
    /// Timestamp of creation
    pub created_at: String,
    /// Token counts
    pub logical_prompt_tokens: i64,
    pub logical_output_tokens: i64,
    pub billed_input_tokens: i64,
    pub billed_output_tokens: i64,
}

/// Trace receipts streaming endpoint
///
/// Streams inference trace receipts for deterministic proof and audit.
/// Polls the database every 5 seconds for new receipts.
/// CRITICAL: Only streams receipts for the authenticated tenant.
///
/// # SSE Event Format
/// ```json
/// event: trace_receipts
/// data: {"receipts": [...], "count": 2}
/// ```
#[utoipa::path(
    get,
    path = "/v1/stream/trace-receipts",
    responses(
        (status = 200, description = "Trace receipts stream (SSE)")
    ),
    tag = "streams"
)]
pub async fn trace_receipts_stream(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Permission check: TelemetryView required for trace receipts
    let has_permission = crate::permissions::require_permission(
        &claims,
        crate::permissions::Permission::TelemetryView,
    )
    .is_ok();

    if !has_permission {
        warn!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Permission denied for trace receipts stream"
        );
    } else {
        info!(
            user_id = %claims.sub,
            tenant_id = %claims.tenant_id,
            "Starting trace receipts SSE stream"
        );
    }

    // Capture tenant_id for filtering
    let tenant_id = claims.tenant_id.clone();

    // Track last seen timestamp to only send new receipts
    let initial_timestamp = chrono::Utc::now().to_rfc3339();

    // Track error_sent to terminate stream after one error
    let stream = stream::unfold(
        (state, initial_timestamp, tenant_id, has_permission, false),
        |(state, last_timestamp, tenant_id, has_permission, error_sent)| async move {
            if !has_permission {
                if error_sent {
                    // Already sent error, terminate stream
                    return None;
                }
                // Return error event once then terminate
                return Some((
                    Ok(Event::default()
                        .event("error")
                        .data("{\"error\": \"Permission denied - TelemetryView required\"}")),
                    (state, last_timestamp, tenant_id, false, true),
                ));
            }

            // Poll every 5 seconds for new receipts
            tokio::time::sleep(Duration::from_secs(5)).await;

            let current_timestamp = chrono::Utc::now().to_rfc3339();

            // Query for new trace receipts since last timestamp
            let receipts: Vec<TraceReceiptEvent> = match sqlx::query_as::<
                _,
                (String, String, Vec<u8>, Vec<u8>, String, i64, i64, i64, i64),
            >(
                r#"
                SELECT
                    r.trace_id,
                    t.tenant_id,
                    r.receipt_digest,
                    r.output_digest,
                    r.created_at,
                    r.logical_prompt_tokens,
                    r.logical_output_tokens,
                    r.billed_input_tokens,
                    r.billed_output_tokens
                FROM inference_trace_receipts r
                JOIN inference_traces t ON r.trace_id = t.trace_id
                WHERE t.tenant_id = ? AND r.created_at > ?
                ORDER BY r.created_at DESC
                LIMIT 100
                "#,
            )
            .bind(&tenant_id)
            .bind(&last_timestamp)
            .fetch_all(state.db.pool())
            .await
            {
                Ok(rows) => rows
                    .into_iter()
                    .map(
                        |(
                            trace_id,
                            tenant_id,
                            receipt_digest,
                            output_digest,
                            created_at,
                            logical_prompt,
                            logical_output,
                            billed_input,
                            billed_output,
                        )| {
                            TraceReceiptEvent {
                                trace_id,
                                tenant_id,
                                receipt_digest: hex::encode(&receipt_digest),
                                output_digest: hex::encode(&output_digest),
                                created_at,
                                logical_prompt_tokens: logical_prompt,
                                logical_output_tokens: logical_output,
                                billed_input_tokens: billed_input,
                                billed_output_tokens: billed_output,
                            }
                        },
                    )
                    .collect(),
                Err(e) => {
                    warn!(error = %e, "Failed to query trace receipts for stream");
                    vec![]
                }
            };

            // Create the response JSON
            let response = serde_json::json!({
                "receipts": receipts,
                "count": receipts.len(),
                "timestamp": current_timestamp,
            });

            let json = serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string());

            Some((
                Ok(Event::default().event("trace_receipts").data(json)),
                (state, current_timestamp, tenant_id, has_permission, false),
            ))
        },
    );

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_circuit_breaker_opens_after_threshold() {
        let mut cb = StreamCircuitBreaker::new(2, Duration::from_millis(10));
        assert_eq!(cb.record_error(), None);
        assert_eq!(
            cb.record_error(),
            Some(StreamBreakerTransition::Opened),
            "breaker should open on threshold"
        );

        let (allow, transition) = cb.should_allow();
        assert!(!allow, "open breaker should block before timeout");
        assert_eq!(transition, None);
    }

    #[test]
    fn stream_circuit_breaker_transitions_to_half_open_after_timeout() {
        let mut cb = StreamCircuitBreaker::new(1, Duration::from_millis(5));
        assert_eq!(cb.record_error(), Some(StreamBreakerTransition::Opened));
        std::thread::sleep(Duration::from_millis(8));

        let (allow, transition) = cb.should_allow();
        assert!(allow, "breaker should allow test request after timeout");
        assert_eq!(transition, Some(StreamBreakerTransition::HalfOpen));
    }

    #[test]
    fn stream_circuit_breaker_recovers_after_half_open_success() {
        let mut cb = StreamCircuitBreaker::new(1, Duration::from_millis(5));
        assert_eq!(cb.record_error(), Some(StreamBreakerTransition::Opened));
        std::thread::sleep(Duration::from_millis(8));
        let _ = cb.should_allow();

        assert_eq!(
            cb.record_success(),
            Some(StreamBreakerTransition::Recovered),
            "successful half-open probe should recover breaker"
        );

        let (allow, transition) = cb.should_allow();
        assert!(allow, "recovered breaker should allow traffic");
        assert_eq!(transition, None);
    }

    #[test]
    fn stream_circuit_breaker_reopens_on_half_open_failure() {
        let mut cb = StreamCircuitBreaker::new(1, Duration::from_millis(5));
        assert_eq!(cb.record_error(), Some(StreamBreakerTransition::Opened));
        std::thread::sleep(Duration::from_millis(8));
        let _ = cb.should_allow();

        assert_eq!(
            cb.record_error(),
            Some(StreamBreakerTransition::Opened),
            "failed half-open probe should reopen breaker"
        );

        let (allow, transition) = cb.should_allow();
        assert!(!allow, "reopened breaker should block again");
        assert_eq!(transition, None);
    }

    #[test]
    fn stream_circuit_breaker_open_transition_metadata_includes_threshold_and_counts() {
        let mut cb = StreamCircuitBreaker::new(2, Duration::from_secs(17));
        assert_eq!(cb.record_error(), None);
        let transition = cb
            .record_error()
            .expect("second error should open breaker at threshold");

        let metadata =
            stream_breaker_transition_metadata("messages", "workspace-1", &cb, transition);
        assert_eq!(metadata["transition"], serde_json::json!("open"));
        assert_eq!(metadata["threshold"], serde_json::json!(2));
        assert_eq!(metadata["error_count"], serde_json::json!(2));
        assert_eq!(metadata["recovery_timeout_secs"], serde_json::json!(17));
    }

    #[test]
    fn stream_circuit_breaker_half_open_transition_metadata_includes_threshold_and_counts() {
        let mut cb = StreamCircuitBreaker::new(3, Duration::from_secs(11));
        cb.error_count = 3;
        cb.state = StreamBreakerState::Open {
            opened_at: std::time::Instant::now() - Duration::from_secs(12),
        };

        let (allow, transition) = cb.should_allow();
        assert!(allow, "half-open transition should allow a probe request");
        let transition = transition.expect("open breaker should transition to half-open");

        let metadata =
            stream_breaker_transition_metadata("activity", "workspace-2", &cb, transition);
        assert_eq!(metadata["transition"], serde_json::json!("half_open"));
        assert_eq!(metadata["threshold"], serde_json::json!(3));
        assert_eq!(metadata["error_count"], serde_json::json!(3));
        assert_eq!(metadata["recovery_timeout_secs"], serde_json::json!(11));
    }

    #[test]
    fn stream_circuit_breaker_recover_transition_metadata_includes_threshold_and_counts() {
        let mut cb = StreamCircuitBreaker::new(4, Duration::from_secs(9));
        cb.error_count = 4;
        cb.state = StreamBreakerState::HalfOpen;

        let transition = cb
            .record_success()
            .expect("successful half-open probe should recover breaker");
        let metadata =
            stream_breaker_transition_metadata("messages", "workspace-3", &cb, transition);
        assert_eq!(metadata["transition"], serde_json::json!("recover"));
        assert_eq!(metadata["threshold"], serde_json::json!(4));
        assert_eq!(metadata["error_count"], serde_json::json!(0));
        assert_eq!(metadata["recovery_timeout_secs"], serde_json::json!(9));
    }
}
