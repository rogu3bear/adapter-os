//! Dataset upload progress SSE handler.
//!
//! Provides Server-Sent Events (SSE) streaming for dataset upload progress,
//! including session-based codebase ingestion tracking.

#![allow(dead_code)]

use super::types::ProgressStreamQuery;
use crate::api_error::ApiError;
use crate::sse::{SseEventManager, SseStreamType};
use crate::state::{AppState, DatasetProgressEvent, IngestionPhase, SessionProgressEvent};
use crate::types::ErrorResponse;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    Json,
};
use futures_util::stream::{self, Stream};
use futures_util::StreamExt as FuturesStreamExt;
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as TokioStreamExt;
use tracing::{debug, info, warn};
use utoipa::{IntoParams, ToSchema};

// Re-export types from state for external use
#[allow(unused_imports)]
pub use crate::state::{IngestionPhase as Phase, SessionProgressEvent as ProgressEvent};

// ============================================================================
// SessionProgressEvent Builder Methods (extension trait)
// ============================================================================

/// Extension trait to add builder methods to SessionProgressEvent
#[allow(clippy::new_ret_no_self)]
pub trait SessionProgressEventExt {
    /// Create a new session progress event
    fn new(session_id: &str, phase: IngestionPhase, message: &str) -> SessionProgressEvent;
    /// Set the dataset ID
    fn with_dataset_id(self, dataset_id: &str) -> Self;
    /// Set the overall progress percentage
    fn with_progress(self, percentage: f32) -> Self;
    /// Set the phase progress percentage
    fn with_phase_progress(self, percentage: f32) -> Self;
    /// Set file counts
    fn with_file_counts(self, processed: i32, total: i32) -> Self;
    /// Set byte counts
    fn with_byte_counts(self, processed: u64, total: u64) -> Self;
    /// Set the current file being processed
    fn with_current_file(self, file: &str) -> Self;
    /// Set the sub-phase
    fn with_sub_phase(self, sub_phase: &str) -> Self;
    /// Set an error message (automatically sets phase to Failed)
    fn with_error(self, error: &str) -> Self;
    /// Set additional metadata
    fn with_metadata(self, metadata: serde_json::Value) -> Self;
}

impl SessionProgressEventExt for SessionProgressEvent {
    fn new(session_id: &str, phase: IngestionPhase, message: &str) -> SessionProgressEvent {
        SessionProgressEvent {
            session_id: session_id.to_string(),
            dataset_id: None,
            phase,
            sub_phase: None,
            current_file: None,
            percentage_complete: 0.0,
            phase_percentage: None,
            total_files: None,
            files_processed: None,
            total_bytes: None,
            bytes_processed: None,
            message: message.to_string(),
            error: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            metadata: None,
        }
    }

    fn with_dataset_id(mut self, dataset_id: &str) -> Self {
        self.dataset_id = Some(dataset_id.to_string());
        self
    }

    fn with_progress(mut self, percentage: f32) -> Self {
        self.percentage_complete = percentage.clamp(0.0, 100.0);
        self
    }

    fn with_phase_progress(mut self, percentage: f32) -> Self {
        self.phase_percentage = Some(percentage.clamp(0.0, 100.0));
        self
    }

    fn with_file_counts(mut self, processed: i32, total: i32) -> Self {
        self.files_processed = Some(processed);
        self.total_files = Some(total);
        self
    }

    fn with_byte_counts(mut self, processed: u64, total: u64) -> Self {
        self.bytes_processed = Some(processed);
        self.total_bytes = Some(total);
        self
    }

    fn with_current_file(mut self, file: &str) -> Self {
        self.current_file = Some(file.to_string());
        self
    }

    fn with_sub_phase(mut self, sub_phase: &str) -> Self {
        self.sub_phase = Some(sub_phase.to_string());
        self
    }

    fn with_error(mut self, error: &str) -> Self {
        self.error = Some(error.to_string());
        self.phase = IngestionPhase::Failed;
        self
    }

    fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

// ============================================================================
// Query Parameters
// ============================================================================

/// Extended query parameters for progress stream with session support
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct SessionProgressStreamQuery {
    /// Filter by dataset ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dataset_id: Option<String>,
    /// Filter by session ID (for codebase ingestion sessions)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Filter by ingestion phase
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

// ============================================================================
// Session Progress Emitter
// ============================================================================

/// Manages emission of session-based progress events
#[derive(Clone)]
pub struct SessionProgressEmitter {
    tx: Arc<broadcast::Sender<SessionProgressEvent>>,
    session_id: String,
    dataset_id: Option<String>,
}

impl SessionProgressEmitter {
    /// Create a new emitter for a session
    pub fn new(tx: Arc<broadcast::Sender<SessionProgressEvent>>, session_id: &str) -> Self {
        Self {
            tx,
            session_id: session_id.to_string(),
            dataset_id: None,
        }
    }

    /// Set the dataset ID once it's known
    pub fn set_dataset_id(&mut self, dataset_id: &str) {
        self.dataset_id = Some(dataset_id.to_string());
    }

    /// Emit a progress event
    pub fn emit(&self, mut event: SessionProgressEvent) {
        // Ensure session_id matches
        event.session_id.clone_from(&self.session_id);
        if let Some(ref dataset_id) = self.dataset_id {
            event.dataset_id = Some(dataset_id.clone());
        }
        event.timestamp = chrono::Utc::now().to_rfc3339();

        if let Err(e) = self.tx.send(event) {
            debug!(
                session_id = %self.session_id,
                error = %e,
                "No active listeners for session progress events"
            );
        }
    }

    /// Emit a simple progress update
    pub fn emit_progress(&self, phase: IngestionPhase, percentage: f32, message: &str) {
        let mut event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            phase,
            message,
        );
        event.percentage_complete = percentage.clamp(0.0, 100.0);
        if let Some(ref dataset_id) = self.dataset_id {
            event.dataset_id = Some(dataset_id.clone());
        }
        self.emit(event);
    }

    /// Emit scanning phase progress
    pub fn emit_scanning(&self, files_found: i32, current_dir: Option<&str>) {
        let message = match current_dir {
            Some(dir) => format!("Scanning: {} files found, checking {}", files_found, dir),
            None => format!("Scanning: {} files found", files_found),
        };
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::Scanning,
            &message,
        )
        .with_file_counts(0, files_found);
        self.emit(event);
    }

    /// Emit parsing phase progress
    pub fn emit_parsing(&self, processed: i32, total: i32, current_file: Option<&str>) {
        let percentage = if total > 0 {
            (processed as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        let message = format!("Parsing source files: {}/{}", processed, total);
        let mut event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::Parsing,
            &message,
        )
        .with_file_counts(processed, total)
        .with_phase_progress(percentage);
        if let Some(file) = current_file {
            event = event.with_current_file(file);
        }
        // Scale to 10-30% of overall progress
        event.percentage_complete = 10.0 + (percentage * 0.2);
        self.emit(event);
    }

    /// Emit analyzing phase progress
    pub fn emit_analyzing(&self, percentage: f32, sub_phase: Option<&str>) {
        let message = match sub_phase {
            Some(s) => format!("Analyzing: {}", s),
            None => "Analyzing code structure...".to_string(),
        };
        let mut event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::Analyzing,
            &message,
        )
        .with_phase_progress(percentage);
        if let Some(s) = sub_phase {
            event = event.with_sub_phase(s);
        }
        // Scale to 30-50% of overall progress
        event.percentage_complete = 30.0 + (percentage * 0.2);
        self.emit(event);
    }

    /// Emit generating phase progress
    pub fn emit_generating(&self, processed: i32, total: i32) {
        let percentage = if total > 0 {
            (processed as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        let message = format!("Generating training data: {}/{} examples", processed, total);
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::Generating,
            &message,
        )
        .with_file_counts(processed, total)
        .with_phase_progress(percentage)
        .with_progress(50.0 + (percentage * 0.2)); // 50-70%
        self.emit(event);
    }

    /// Emit uploading phase progress
    pub fn emit_uploading(
        &self,
        bytes_uploaded: u64,
        total_bytes: u64,
        current_file: Option<&str>,
    ) {
        let percentage = if total_bytes > 0 {
            (bytes_uploaded as f32 / total_bytes as f32) * 100.0
        } else {
            0.0
        };
        let message = format!(
            "Uploading: {:.1} MB / {:.1} MB",
            bytes_uploaded as f64 / 1_048_576.0,
            total_bytes as f64 / 1_048_576.0
        );
        let mut event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::Uploading,
            &message,
        )
        .with_byte_counts(bytes_uploaded, total_bytes)
        .with_phase_progress(percentage)
        .with_progress(70.0 + (percentage * 0.15)); // 70-85%
        if let Some(file) = current_file {
            event = event.with_current_file(file);
        }
        self.emit(event);
    }

    /// Emit validating phase progress
    pub fn emit_validating(&self, percentage: f32) {
        let message = format!("Validating dataset: {:.0}%", percentage);
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::Validating,
            &message,
        )
        .with_phase_progress(percentage)
        .with_progress(85.0 + (percentage * 0.1)); // 85-95%
        self.emit(event);
    }

    /// Emit statistics computation progress
    pub fn emit_computing_statistics(&self, percentage: f32) {
        let message = format!("Computing statistics: {:.0}%", percentage);
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::ComputingStatistics,
            &message,
        )
        .with_phase_progress(percentage)
        .with_progress(95.0 + (percentage * 0.05)); // 95-100%
        self.emit(event);
    }

    /// Emit completion event
    pub fn emit_completed(&self, dataset_id: &str, total_files: i32, total_bytes: u64) {
        let message = format!(
            "Ingestion complete: {} files, {:.1} MB",
            total_files,
            total_bytes as f64 / 1_048_576.0
        );
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::Completed,
            &message,
        )
        .with_dataset_id(dataset_id)
        .with_file_counts(total_files, total_files)
        .with_byte_counts(total_bytes, total_bytes)
        .with_progress(100.0);
        self.emit(event);
    }

    /// Emit failure event
    pub fn emit_failed(&self, error: &str) {
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            &self.session_id,
            IngestionPhase::Failed,
            "Ingestion failed",
        )
        .with_error(error);
        self.emit(event);
    }
}

// ============================================================================
// SSE Handlers
// ============================================================================

/// Stream dataset upload and processing progress via SSE
///
/// This endpoint establishes a Server-Sent Events (SSE) connection that streams
/// progress events for dataset operations. Clients can connect to receive real-time
/// updates about:
/// - File upload progress (percentage, current file)
/// - Dataset validation progress (files processed, validation results)
/// - Statistics computation progress
///
/// Events are JSON objects with the following fields:
/// - `dataset_id`: The ID of the dataset being processed
/// - `event_type`: One of "upload", "validation", or "statistics"
/// - `current_file`: The file currently being processed (optional)
/// - `percentage_complete`: Overall progress as a percentage (0-100)
/// - `total_files`: Total number of files in the dataset (optional)
/// - `files_processed`: Number of files processed so far (optional)
/// - `message`: Human-readable status message
/// - `timestamp`: RFC3339 formatted timestamp
///
/// Example client usage (JavaScript):
/// ```javascript
/// const eventSource = new EventSource('/v1/datasets/upload/progress?dataset_id=abc123');
/// eventSource.onmessage = (event) => {
///   const progress = JSON.parse(event.data);
///   console.log(`${progress.message}: ${progress.percentage_complete}%`);
/// };
/// ```
#[utoipa::path(
    get,
    path = "/v1/datasets/upload/progress",
    params(
        ("dataset_id" = Option<String>, Query, description = "Optional filter by dataset ID")
    ),
    responses(
        (status = 200, description = "Server-Sent Events stream of dataset progress"),
        (status = 503, description = "Progress streaming not available")
    ),
    tag = "datasets"
)]
pub async fn dataset_upload_progress(
    State(state): State<AppState>,
    Query(query): Query<ProgressStreamQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::DatasetProgress, last_id)
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

    // Get progress broadcast channel from state
    let rx = state
        .dataset_progress_tx
        .as_ref()
        .ok_or_else(|| {
            let err: (StatusCode, Json<ErrorResponse>) =
                ApiError::internal("Dataset progress streaming not available").into();
            err
        })?
        .subscribe();

    info!(
        dataset_id = ?query.dataset_id,
        "Starting dataset upload progress SSE stream"
    );

    // Track consecutive lag events for slow client detection
    const MAX_CONSECUTIVE_LAGS: u32 = 3;
    let consecutive_lags = Arc::new(AtomicU32::new(0));
    let consecutive_lags_clone = consecutive_lags.clone();
    let mgr_for_stream = Arc::new(state.sse_manager.clone());

    let live_stream = FuturesStreamExt::filter_map(BroadcastStream::new(rx), move |result| {
        let consecutive_lags_inner = consecutive_lags_clone.clone();
        let query_dataset_id = query.dataset_id.clone();
        let mgr = Arc::clone(&mgr_for_stream);
        async move {
            match result {
                Ok(event) => {
                    // Reset lag counter on successful receive
                    consecutive_lags_inner.store(0, Ordering::Relaxed);

                    // Filter by dataset_id if specified
                    if let Some(ref dataset_id) = query_dataset_id {
                        if event.dataset_id != *dataset_id {
                            return None;
                        }
                    }

                    // Convert to SSE event
                    match serde_json::to_string(&event) {
                        Ok(json) => {
                            let sse_event = mgr
                                .create_event(
                                    SseStreamType::DatasetProgress,
                                    &event.event_type,
                                    json,
                                )
                                .await;
                            Some(Ok(SseEventManager::to_axum_event(&sse_event)))
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to serialize dataset progress event");
                            None
                        }
                    }
                }
                Err(e) => {
                    let lags = consecutive_lags_inner.fetch_add(1, Ordering::Relaxed) + 1;
                    if lags >= MAX_CONSECUTIVE_LAGS {
                        warn!(
                            consecutive_lags = lags,
                            "Slow client detected in dataset progress stream, forcing disconnect"
                        );
                        // Return a terminal event to signal stream end
                        return Some(Ok(Event::default()
                            .event("error")
                            .data(r#"{"error":"slow_client_disconnected"}"#)));
                    }
                    debug!(error = %e, consecutive_lags = lags, "Broadcast lag in dataset progress stream");
                    None
                }
            }
        }
    });

    // Apply timeout and take_while
    let live_stream = TokioStreamExt::map(
        TokioStreamExt::timeout(live_stream, Duration::from_secs(300)),
        |result| match result {
            Ok(event) => event,
            Err(_timeout) => {
                warn!("Dataset progress SSE stream timed out after 5 minutes");
                Ok(Event::default()
                    .event("timeout")
                    .data(r#"{"error":"stream_timeout"}"#))
            }
        },
    );

    // Chain replay with live stream
    Ok(
        Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        ),
    )
}

/// Stream session-based codebase ingestion progress via SSE
///
/// This endpoint provides real-time progress updates for codebase ingestion
/// sessions. It supports filtering by session ID and/or dataset ID.
///
/// Events include detailed phase information:
/// - `scanning`: Finding source files in the codebase
/// - `parsing`: Parsing source files and extracting structure
/// - `analyzing`: Analyzing code graph and dependencies
/// - `generating`: Generating training data
/// - `uploading`: Uploading generated files
/// - `validating`: Validating the dataset
/// - `computing_statistics`: Computing dataset statistics
/// - `completed`: Ingestion finished successfully
/// - `failed`: Ingestion failed with error
///
/// Example client usage (JavaScript):
/// ```javascript
/// const eventSource = new EventSource('/v1/datasets/session/progress?session_id=abc123');
/// eventSource.addEventListener('session_progress', (event) => {
///   const progress = JSON.parse(event.data);
///   console.log(`[${progress.phase}] ${progress.message}: ${progress.percentage_complete}%`);
/// });
/// ```
#[utoipa::path(
    get,
    path = "/v1/datasets/session/progress",
    params(
        ("session_id" = Option<String>, Query, description = "Filter by session ID"),
        ("dataset_id" = Option<String>, Query, description = "Filter by dataset ID"),
        ("phase" = Option<String>, Query, description = "Filter by ingestion phase")
    ),
    responses(
        (status = 200, description = "Server-Sent Events stream of session progress"),
        (status = 503, description = "Session progress streaming not available")
    ),
    tag = "datasets"
)]
pub async fn session_progress_stream(
    State(state): State<AppState>,
    Query(query): Query<SessionProgressStreamQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ErrorResponse>)> {
    let sse_manager = state.sse_manager.clone();

    // Parse Last-Event-ID for replay
    let last_event_id = SseEventManager::parse_last_event_id(&headers);

    // Get replay events if reconnecting
    let replay_events = if let Some(last_id) = last_event_id {
        sse_manager
            .get_replay_events(SseStreamType::DatasetProgress, last_id)
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

    // Get session progress broadcast channel from state
    let rx = state
        .session_progress_tx
        .as_ref()
        .ok_or_else(|| {
            let err: (StatusCode, Json<ErrorResponse>) =
                ApiError::internal("Session progress streaming not available").into();
            err
        })?
        .subscribe();

    info!(
        session_id = ?query.session_id,
        dataset_id = ?query.dataset_id,
        phase = ?query.phase,
        "Starting session progress SSE stream"
    );

    // Track consecutive lag events for slow client detection
    const MAX_CONSECUTIVE_LAGS: u32 = 3;
    let consecutive_lags = Arc::new(AtomicU32::new(0));
    let consecutive_lags_clone = consecutive_lags.clone();
    let mgr_for_stream = Arc::new(state.sse_manager.clone());

    let live_stream = FuturesStreamExt::filter_map(BroadcastStream::new(rx), move |result| {
        let consecutive_lags_inner = consecutive_lags_clone.clone();
        let query_session_id = query.session_id.clone();
        let query_dataset_id = query.dataset_id.clone();
        let query_phase = query.phase.clone();
        let mgr = Arc::clone(&mgr_for_stream);
        async move {
            match result {
                Ok(event) => {
                    // Reset lag counter on successful receive
                    consecutive_lags_inner.store(0, Ordering::Relaxed);

                    // Filter by session_id if specified
                    if let Some(ref session_id) = query_session_id {
                        if event.session_id != *session_id {
                            return None;
                        }
                    }

                    // Filter by dataset_id if specified
                    if let Some(ref dataset_id) = query_dataset_id {
                        if event.dataset_id.as_ref() != Some(dataset_id) {
                            return None;
                        }
                    }

                    // Filter by phase if specified
                    if let Some(ref phase_filter) = query_phase {
                        let event_phase = event.phase.to_string();
                        if event_phase != *phase_filter {
                            return None;
                        }
                    }

                    // Convert to SSE event
                    match serde_json::to_string(&event) {
                        Ok(json) => {
                            let sse_event = mgr
                                .create_event(
                                    SseStreamType::DatasetProgress,
                                    "session_progress",
                                    json,
                                )
                                .await;
                            Some(Ok(SseEventManager::to_axum_event(&sse_event)))
                        }
                        Err(e) => {
                            warn!(error = %e, "Failed to serialize session progress event");
                            None
                        }
                    }
                }
                Err(e) => {
                    let lags = consecutive_lags_inner.fetch_add(1, Ordering::Relaxed) + 1;
                    if lags >= MAX_CONSECUTIVE_LAGS {
                        warn!(
                            consecutive_lags = lags,
                            "Slow client detected in session progress stream, forcing disconnect"
                        );
                        // Return a terminal event to signal stream end
                        return Some(Ok(Event::default()
                            .event("error")
                            .data(r#"{"error":"slow_client_disconnected"}"#)));
                    }
                    debug!(error = %e, consecutive_lags = lags, "Broadcast lag in session progress stream");
                    None
                }
            }
        }
    });

    // Apply timeout
    let live_stream = TokioStreamExt::map(
        TokioStreamExt::timeout(live_stream, Duration::from_secs(300)),
        |result| match result {
            Ok(event) => event,
            Err(_timeout) => {
                warn!("Session progress SSE stream timed out after 5 minutes");
                Ok(Event::default()
                    .event("timeout")
                    .data(r#"{"error":"stream_timeout"}"#))
            }
        },
    );

    // Chain replay with live stream
    Ok(
        Sse::new(FuturesStreamExt::chain(replay_stream, live_stream)).keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(10))
                .text("keep-alive"),
        ),
    )
}

// ============================================================================
// Helper Functions for Emitting Progress
// ============================================================================

/// Emit a dataset progress event (legacy API)
#[allow(clippy::too_many_arguments)]
pub fn emit_dataset_progress(
    tx: Option<&Arc<broadcast::Sender<DatasetProgressEvent>>>,
    dataset_id: &str,
    event_type: &str,
    current_file: Option<String>,
    percentage_complete: f32,
    message: String,
    total_files: Option<i32>,
    files_processed: Option<i32>,
) {
    if let Some(sender) = tx {
        let event = DatasetProgressEvent {
            dataset_id: dataset_id.to_string(),
            event_type: event_type.to_string(),
            current_file,
            percentage_complete,
            total_files,
            files_processed,
            message,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(e) = sender.send(event) {
            debug!(
                dataset_id = %dataset_id,
                error = %e,
                "No active listeners for dataset progress events"
            );
        }
    }
}

/// Create a new session progress emitter
pub fn create_session_emitter(
    tx: Arc<broadcast::Sender<SessionProgressEvent>>,
    session_id: &str,
) -> SessionProgressEmitter {
    SessionProgressEmitter::new(tx, session_id)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_progress_event_builder() {
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            "session-123",
            IngestionPhase::Parsing,
            "Test message",
        )
        .with_dataset_id("dataset-456")
        .with_progress(50.0)
        .with_file_counts(5, 10)
        .with_current_file("test.rs");

        assert_eq!(event.session_id, "session-123");
        assert_eq!(event.dataset_id, Some("dataset-456".to_string()));
        assert_eq!(event.phase, IngestionPhase::Parsing);
        assert_eq!(event.percentage_complete, 50.0);
        assert_eq!(event.files_processed, Some(5));
        assert_eq!(event.total_files, Some(10));
        assert_eq!(event.current_file, Some("test.rs".to_string()));
    }

    #[test]
    fn test_session_progress_event_error() {
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            "session-123",
            IngestionPhase::Parsing,
            "Test message",
        )
        .with_error("Something went wrong");

        assert_eq!(event.phase, IngestionPhase::Failed);
        assert_eq!(event.error, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_ingestion_phase_display() {
        assert_eq!(IngestionPhase::Scanning.to_string(), "scanning");
        assert_eq!(IngestionPhase::Parsing.to_string(), "parsing");
        assert_eq!(
            IngestionPhase::ComputingStatistics.to_string(),
            "computing_statistics"
        );
    }

    #[test]
    fn test_progress_clamping() {
        let event = <SessionProgressEvent as SessionProgressEventExt>::new(
            "session-123",
            IngestionPhase::Uploading,
            "Test",
        )
        .with_progress(150.0);
        assert_eq!(event.percentage_complete, 100.0);

        let event2 = <SessionProgressEvent as SessionProgressEventExt>::new(
            "session-123",
            IngestionPhase::Uploading,
            "Test",
        )
        .with_progress(-10.0);
        assert_eq!(event2.percentage_complete, 0.0);
    }
}
