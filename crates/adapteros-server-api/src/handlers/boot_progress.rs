//! Boot progress streaming endpoint
//!
//! Provides SSE streaming of boot state changes and model loading progress.

use crate::auth::Claims;
use crate::sse::{SseEventManager, SseStreamType};
use crate::state::AppState;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Extension;
use futures_util::stream::{self, Stream};
use futures_util::StreamExt as FuturesStreamExt;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use tracing::{info, warn};
use utoipa::ToSchema;

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
pub async fn boot_progress_stream(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("Starting boot progress SSE stream");

    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::BootProgress, last_id)
            .await
    } else {
        Vec::new()
    };

    // Create replay stream
    let replay_stream = stream::iter(
        replay_events
            .into_iter()
            .map(|e| Ok(SseEventManager::to_axum_event(&e))),
    );

    // Track previous state for change detection
    let initial_state = state
        .boot_state
        .as_ref()
        .map(|bs| bs.current_state().as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let live_stream = stream::unfold(
        (state, initial_state),
        |(state, mut previous_state)| async move {
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
                    let event = state
                        .sse_manager
                        .create_error_event(
                            SseStreamType::BootProgress,
                            "boot state manager not available",
                        )
                        .await;
                    return Some((
                        Ok(SseEventManager::to_axum_event(&event)),
                        (state, previous_state),
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

                let boot_event = BootProgressEvent::StateChanged {
                    previous: previous_state.clone(),
                    current: current_state_str.clone(),
                    elapsed_ms,
                    models_pending,
                    models_ready,
                };

                previous_state = current_state_str;

                let json = match serde_json::to_string(&boot_event) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize boot progress event");
                        let event = state
                            .sse_manager
                            .create_error_event(
                                SseStreamType::BootProgress,
                                &format!("serialization failed: {}", e),
                            )
                            .await;
                        return Some((
                            Ok(SseEventManager::to_axum_event(&event)),
                            (state, previous_state),
                        ));
                    }
                };

                let event = state
                    .sse_manager
                    .create_event(SseStreamType::BootProgress, "boot_progress", json)
                    .await;

                return Some((
                    Ok(SseEventManager::to_axum_event(&event)),
                    (state, previous_state),
                ));
            }

            // Check for model loading status changes
            let model_status = boot_state_ref.get_model_status();

            // If in loading state and models are pending, emit periodic progress
            if current_state.is_booting() && !model_status.pending.is_empty() {
                let elapsed_ms = boot_state_ref.elapsed().as_millis() as u64;

                let boot_event = BootProgressEvent::StateChanged {
                    previous: current_state_str.clone(),
                    current: current_state_str.clone(),
                    elapsed_ms,
                    models_pending: model_status.pending.len(),
                    models_ready: model_status.ready.len(),
                };

                let json = match serde_json::to_string(&boot_event) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize boot progress event");
                        let event = state
                            .sse_manager
                            .create_error_event(
                                SseStreamType::BootProgress,
                                &format!("serialization failed: {}", e),
                            )
                            .await;
                        return Some((
                            Ok(SseEventManager::to_axum_event(&event)),
                            (state, previous_state),
                        ));
                    }
                };

                let event = state
                    .sse_manager
                    .create_event(SseStreamType::BootProgress, "boot_progress", json)
                    .await;

                return Some((
                    Ok(SseEventManager::to_axum_event(&event)),
                    (state, previous_state),
                ));
            }

            // If fully ready, emit final event
            if current_state.is_fully_ready() {
                let elapsed_ms = boot_state_ref.elapsed().as_millis() as u64;
                let total_models = model_status.ready.len();
                let total_download_mb = boot_state_ref.total_download_mb();

                let boot_event = BootProgressEvent::FullyReady {
                    total_models,
                    total_download_mb,
                    total_load_time_ms: elapsed_ms,
                };

                let json = match serde_json::to_string(&boot_event) {
                    Ok(j) => j,
                    Err(e) => {
                        warn!(error = %e, "Failed to serialize boot progress event");
                        let event = state
                            .sse_manager
                            .create_error_event(
                                SseStreamType::BootProgress,
                                &format!("serialization failed: {}", e),
                            )
                            .await;
                        return Some((
                            Ok(SseEventManager::to_axum_event(&event)),
                            (state, previous_state),
                        ));
                    }
                };

                let event = state
                    .sse_manager
                    .create_event(SseStreamType::BootProgress, "boot_progress", json)
                    .await;

                return Some((
                    Ok(SseEventManager::to_axum_event(&event)),
                    (state, previous_state),
                ));
            }

            // No changes, send keep-alive with timestamp
            let event = state
                .sse_manager
                .create_event(
                    SseStreamType::BootProgress,
                    "keepalive",
                    format!("{{\"timestamp\": {}}}", current_timestamp),
                )
                .await;

            Some((
                Ok(SseEventManager::to_axum_event(&event)),
                (state, previous_state),
            ))
        },
    );

    // Chain replay with live stream
    Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(10))
            .text("keep-alive"),
    )
}
