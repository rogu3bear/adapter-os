//! Server-side pause tracking for human-in-the-loop reviews
//!
//! This module tracks paused inferences received from workers and provides
//! mechanisms to forward reviews to the appropriate worker via UDS.
//!
//! # Architecture
//!
//! - Worker detects review trigger and pauses inference
//! - Worker emits Paused SSE event to server
//! - Server stores pause info + worker UDS path
//! - Human reviews via CLI: `aosctl review list` / `aosctl review submit`
//! - Server forwards review to worker via UDS `/inference/resume`
//! - Worker resumes inference

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::uds_client::{UdsClient, WorkerStreamPaused};
use adapteros_api_types::review::{InferenceState, PauseKind, ReviewContext, SubmitReviewRequest};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_telemetry::diagnostics::{
    DiagEnvelope, DiagEvent, DiagRunId, DiagSeverity, DiagnosticsService,
};
use adapteros_telemetry::tracing::TraceContext;

/// Entry tracking a paused inference
#[derive(Debug, Clone)]
struct PausedEntry {
    /// Tenant ID that owns this inference/pause
    tenant_id: String,
    /// Inference request ID
    inference_id: String,
    /// Pause ID
    pause_id: String,
    /// Type of pause trigger
    trigger_kind: String,
    /// Context for the reviewer
    context: Option<String>,
    /// Generated text so far
    text_so_far: Option<String>,
    /// Token count at pause
    token_count: usize,
    /// Worker UDS path (to forward resume)
    worker_uds_path: PathBuf,
    /// When paused (monotonic, for duration calculation)
    paused_at: Instant,
    /// When paused (wall clock, for API responses)
    created_at: DateTime<Utc>,
}

/// Info about a paused inference (for API responses)
#[derive(Debug, Clone)]
pub struct PausedInferenceInfo {
    pub tenant_id: String,
    pub inference_id: String,
    pub pause_id: String,
    pub kind: PauseKind,
    pub context: ReviewContext,
    pub duration_secs: u64,
    pub text_so_far: Option<String>,
    pub token_count: usize,
    /// When the pause was registered (wall clock, for API responses)
    pub created_at: DateTime<Utc>,
}

/// Convert a `PausedEntry` into the API-facing `PausedInferenceInfo`.
fn entry_to_info(entry: &PausedEntry) -> PausedInferenceInfo {
    let kind = parse_trigger_kind(&entry.trigger_kind);
    PausedInferenceInfo {
        tenant_id: entry.tenant_id.clone(),
        inference_id: entry.inference_id.clone(),
        pause_id: entry.pause_id.clone(),
        kind,
        context: ReviewContext {
            code: entry.text_so_far.clone(),
            question: entry.context.clone(),
            scope: vec![],
            metadata: Some(serde_json::json!({
                "token_count": entry.token_count,
            })),
        },
        duration_secs: entry.paused_at.elapsed().as_secs(),
        text_so_far: entry.text_so_far.clone(),
        token_count: entry.token_count,
        created_at: entry.created_at,
    }
}

/// Server-side pause tracker
pub struct ServerPauseTracker {
    /// Map of pause_id -> paused entry
    paused: RwLock<HashMap<String, PausedEntry>>,
    /// Monotonic revision used to notify pause list changes
    change_revision: AtomicU64,
    /// Change notification channel for pause list updates
    change_tx: watch::Sender<u64>,
    /// UDS client for forwarding reviews
    uds_client: Option<Arc<UdsClient>>,
    /// Optional diagnostics service for emitting pause/resume events
    diagnostics: Option<Arc<DiagnosticsService>>,
}

impl Default for ServerPauseTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerPauseTracker {
    /// Create a new tracker
    pub fn new() -> Self {
        let (change_tx, _) = watch::channel(0);
        Self {
            paused: RwLock::new(HashMap::new()),
            change_revision: AtomicU64::new(0),
            change_tx,
            uds_client: None,
            diagnostics: None,
        }
    }

    /// Create with a UDS client for forwarding reviews
    pub fn with_uds_client(client: Arc<UdsClient>) -> Self {
        let (change_tx, _) = watch::channel(0);
        Self {
            paused: RwLock::new(HashMap::new()),
            change_revision: AtomicU64::new(0),
            change_tx,
            uds_client: Some(client),
            diagnostics: None,
        }
    }

    /// Attach a diagnostics service for emitting pause/resume events
    pub fn with_diagnostics(mut self, service: Arc<DiagnosticsService>) -> Self {
        self.diagnostics = Some(service);
        self
    }

    fn notify_changed(&self) {
        let revision = self.change_revision.fetch_add(1, Ordering::Relaxed) + 1;
        let _ = self.change_tx.send(revision);
    }

    /// Subscribe to pause tracker change revisions.
    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.change_tx.subscribe()
    }

    /// Register a paused inference received from a worker
    pub fn register_pause(
        &self,
        tenant_id: String,
        event: WorkerStreamPaused,
        worker_uds_path: PathBuf,
    ) {
        let now = Utc::now();

        info!(
            tenant_id = %tenant_id,
            pause_id = %event.pause_id,
            inference_id = %event.inference_id,
            "Registered paused inference from worker"
        );

        // Emit diagnostic event for pause (before moving values into entry)
        if let Some(ref diag) = self.diagnostics {
            let context_hash = B3Hash::hash(event.context.as_deref().unwrap_or("").as_bytes());
            let trace_ctx = TraceContext::new_root();
            let run_id = DiagRunId::from_trace_context(&trace_ctx);
            let envelope = DiagEnvelope::new(
                &trace_ctx,
                &tenant_id,
                run_id,
                DiagSeverity::Info,
                0, // mono_us - relative to run start
                DiagEvent::InferencePaused {
                    pause_id: event.pause_id.clone(),
                    inference_id: event.inference_id.clone(),
                    pause_kind: format!("{:?}", parse_trigger_kind(&event.trigger_kind)),
                    trigger_kind: Some(event.trigger_kind.clone()),
                    context_hash,
                    token_count: event.token_count as u32,
                },
            );
            if let Err(e) = diag.emit(envelope) {
                warn!(error = %e, "Failed to emit InferencePaused diagnostic");
            }
        }

        let entry = PausedEntry {
            tenant_id,
            inference_id: event.inference_id.clone(),
            pause_id: event.pause_id.clone(),
            trigger_kind: event.trigger_kind.clone(),
            context: event.context.clone(),
            text_so_far: event.text_so_far,
            token_count: event.token_count,
            worker_uds_path,
            paused_at: Instant::now(),
            created_at: now,
        };

        self.paused.write().insert(event.pause_id, entry);
        self.notify_changed();
    }

    /// List all paused inferences
    pub fn list_paused(&self) -> Vec<PausedInferenceInfo> {
        self.paused.read().values().map(entry_to_info).collect()
    }

    /// List paused inferences for a specific tenant.
    pub fn list_paused_for_tenant(&self, tenant_id: &str) -> Vec<PausedInferenceInfo> {
        self.paused
            .read()
            .values()
            .filter(|entry| entry.tenant_id == tenant_id)
            .map(entry_to_info)
            .collect()
    }

    /// Get state for a specific inference
    pub fn get_state_by_inference(&self, inference_id: &str) -> Option<PausedInferenceInfo> {
        self.paused.read().values().find_map(|entry| {
            if entry.inference_id == inference_id {
                Some(entry_to_info(entry))
            } else {
                None
            }
        })
    }

    /// Get state for a specific inference scoped to a tenant.
    pub fn get_state_by_inference_for_tenant(
        &self,
        tenant_id: &str,
        inference_id: &str,
    ) -> Option<PausedInferenceInfo> {
        self.paused.read().values().find_map(|entry| {
            if entry.tenant_id == tenant_id && entry.inference_id == inference_id {
                Some(entry_to_info(entry))
            } else {
                None
            }
        })
    }

    /// Get state by pause ID
    pub fn get_state_by_pause_id(&self, pause_id: &str) -> Option<PausedInferenceInfo> {
        self.paused.read().get(pause_id).map(entry_to_info)
    }

    /// Get state by pause ID scoped to a tenant.
    pub fn get_state_by_pause_id_for_tenant(
        &self,
        tenant_id: &str,
        pause_id: &str,
    ) -> Option<PausedInferenceInfo> {
        self.paused.read().get(pause_id).and_then(|entry| {
            if entry.tenant_id != tenant_id {
                return None;
            }
            Some(entry_to_info(entry))
        })
    }

    /// Submit a review and forward to worker
    ///
    /// Returns the new inference state on success
    pub async fn submit_review(&self, request: SubmitReviewRequest) -> Result<InferenceState> {
        // Look up the paused entry
        let entry = {
            let guard = self.paused.read();
            guard.get(&request.pause_id).cloned()
        };

        let entry = entry.ok_or_else(|| {
            AosError::not_found(format!(
                "No paused inference found with pause_id: {}",
                request.pause_id
            ))
        })?;

        // Create UDS client for forwarding (30s timeout for review operations)
        let uds_client = self
            .uds_client
            .clone()
            .unwrap_or_else(|| Arc::new(UdsClient::new(std::time::Duration::from_secs(30))));

        let resume_path = format!("/inference/resume/{}", request.pause_id);
        let body = serde_json::to_string(&request).map_err(|e| {
            AosError::internal(format!("Failed to serialize review request: {}", e))
        })?;

        info!(
            pause_id = %request.pause_id,
            worker_path = %entry.worker_uds_path.display(),
            "Forwarding review to worker"
        );

        // Send resume request to worker via HTTP POST over UDS
        let body_json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            AosError::internal(format!("Failed to parse review request as JSON: {}", e))
        })?;

        match uds_client
            .send_http_request(
                &entry.worker_uds_path,
                "POST",
                &resume_path,
                Some(body_json),
            )
            .await
        {
            Ok(response) => {
                // Parse worker response to verify it actually resumed
                // Only remove from tracking if worker confirms success
                let status = response
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                if status == "resumed" {
                    // Worker confirmed resume - safe to remove from tracking
                    let pause_duration_us = entry.paused_at.elapsed().as_micros() as u64;
                    self.paused.write().remove(&request.pause_id);
                    self.notify_changed();
                    info!(pause_id = %request.pause_id, "Review forwarded, inference resumed");

                    // Emit diagnostic event for resume
                    if let Some(ref diag) = self.diagnostics {
                        let review_hash = B3Hash::hash(
                            serde_json::to_string(&request.review)
                                .unwrap_or_default()
                                .as_bytes(),
                        );
                        let trace_ctx = TraceContext::new_root();
                        let run_id = DiagRunId::from_trace_context(&trace_ctx);
                        let envelope = DiagEnvelope::new(
                            &trace_ctx,
                            &entry.tenant_id,
                            run_id,
                            DiagSeverity::Info,
                            pause_duration_us,
                            DiagEvent::InferenceResumed {
                                pause_id: request.pause_id.clone(),
                                inference_id: entry.inference_id.clone(),
                                reviewer: request.reviewer.clone(),
                                assessment: format!("{:?}", request.review.assessment),
                                review_hash,
                                pause_duration_us,
                                issue_count: request.review.issues.len() as u32,
                                success: true,
                            },
                        );
                        if let Err(e) = diag.emit(envelope) {
                            warn!(error = %e, "Failed to emit InferenceResumed diagnostic");
                        }
                    }

                    Ok(InferenceState::Running)
                } else {
                    // Worker returned error status - keep pause in tracking for retry
                    let error_msg = response
                        .get("error")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown worker error");
                    warn!(
                        pause_id = %request.pause_id,
                        status = %status,
                        error = %error_msg,
                        "Worker rejected review submission"
                    );
                    Err(AosError::internal(format!(
                        "Worker rejected review: {}",
                        error_msg
                    )))
                }
            }
            Err(e) => {
                warn!(
                    pause_id = %request.pause_id,
                    error = %e,
                    "Failed to forward review to worker"
                );
                Err(AosError::internal(format!(
                    "Failed to forward review to worker: {}",
                    e
                )))
            }
        }
    }

    /// Register a server-side pause (not from a worker).
    ///
    /// Use this for server-originated pauses such as:
    /// - Dataset safety check failures requiring human review
    /// - Policy approval gates
    /// - Administrative holds
    ///
    /// Unlike `register_pause`, this does not require a worker UDS path
    /// since the pause originates from the control plane itself.
    pub fn register_server_pause(
        &self,
        tenant_id: String,
        pause_id: String,
        resource_id: String,
        trigger_kind: &str,
        context: Option<String>,
        metadata: Option<serde_json::Value>,
    ) {
        let now = Utc::now();

        info!(
            tenant_id = %tenant_id,
            pause_id = %pause_id,
            resource_id = %resource_id,
            trigger_kind = %trigger_kind,
            "Registered server-side pause for review"
        );

        // Emit diagnostic event for server-side pause
        if let Some(ref diag) = self.diagnostics {
            let context_hash = B3Hash::hash(context.as_deref().unwrap_or("").as_bytes());
            let trace_ctx = TraceContext::new_root();
            let run_id = DiagRunId::from_trace_context(&trace_ctx);
            let envelope = DiagEnvelope::new(
                &trace_ctx,
                &tenant_id,
                run_id,
                DiagSeverity::Info,
                0, // mono_us - relative to run start
                DiagEvent::InferencePaused {
                    pause_id: pause_id.clone(),
                    inference_id: resource_id.clone(),
                    pause_kind: format!("{:?}", parse_trigger_kind(trigger_kind)),
                    trigger_kind: Some(trigger_kind.to_string()),
                    context_hash,
                    token_count: 0,
                },
            );
            if let Err(e) = diag.emit(envelope) {
                warn!(error = %e, "Failed to emit server-side pause diagnostic");
            }
        }

        let entry = PausedEntry {
            tenant_id,
            inference_id: resource_id,
            pause_id: pause_id.clone(),
            trigger_kind: trigger_kind.to_string(),
            context,
            text_so_far: metadata.map(|m| serde_json::to_string(&m).unwrap_or_default()),
            token_count: 0,
            // Server-side pauses don't have a worker UDS path
            worker_uds_path: PathBuf::new(),
            paused_at: Instant::now(),
            created_at: now,
        };

        self.paused.write().insert(pause_id, entry);
        self.notify_changed();
    }

    /// Remove a pause entry (e.g., if inference completes or errors)
    pub fn remove(&self, pause_id: &str) {
        self.paused.write().remove(pause_id);
        self.notify_changed();
    }

    /// Get count of paused inferences
    pub fn count(&self) -> usize {
        self.paused.read().len()
    }
}

/// Parse trigger kind string to PauseKind enum
fn parse_trigger_kind(kind: &str) -> PauseKind {
    match kind.to_lowercase().as_str() {
        // Map review trigger kinds to API PauseKind variants
        "explicittag" | "explicit_tag" | "review" => PauseKind::ReviewNeeded,
        "uncertaintysignal" | "uncertainty_signal" | "uncertainty" => PauseKind::ReviewNeeded,
        "complexitythreshold" | "complexity_threshold" => PauseKind::ReviewNeeded,
        "policy" | "policy_approval" | "policy_violation" => PauseKind::PolicyApproval,
        "safety_gate" | "threat" | "threat_escalation" => PauseKind::ThreatEscalation,
        "resource" | "resource_wait" => PauseKind::ResourceWait,
        "manual" | "user_requested" => PauseKind::UserRequested,
        _ => PauseKind::ReviewNeeded, // Default fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_register_and_list() {
        let tracker = ServerPauseTracker::new();

        let event = WorkerStreamPaused {
            pause_id: "pause-1".to_string(),
            inference_id: "infer-1".to_string(),
            trigger_kind: "ExplicitTag".to_string(),
            context: Some("Review this code".to_string()),
            text_so_far: Some("I'll analyze...".to_string()),
            token_count: 10,
        };

        tracker.register_pause(
            "tenant-1".to_string(),
            event,
            PathBuf::from("/var/run/worker.sock"),
        );

        let paused = tracker.list_paused();
        assert_eq!(paused.len(), 1);
        assert_eq!(paused[0].pause_id, "pause-1");
        assert_eq!(paused[0].inference_id, "infer-1");
    }

    #[test]
    fn test_get_by_inference() {
        let tracker = ServerPauseTracker::new();

        let event = WorkerStreamPaused {
            pause_id: "pause-2".to_string(),
            inference_id: "infer-2".to_string(),
            trigger_kind: "UncertaintySignal".to_string(),
            context: None,
            text_so_far: None,
            token_count: 5,
        };

        tracker.register_pause(
            "tenant-1".to_string(),
            event,
            PathBuf::from("/var/run/worker.sock"),
        );

        let info = tracker.get_state_by_inference("infer-2");
        assert!(info.is_some());
        assert_eq!(info.unwrap().kind, PauseKind::ReviewNeeded);
    }

    #[test]
    fn test_parse_trigger_kind() {
        assert_eq!(parse_trigger_kind("ExplicitTag"), PauseKind::ReviewNeeded);
        assert_eq!(
            parse_trigger_kind("uncertainty_signal"),
            PauseKind::ReviewNeeded
        );
        assert_eq!(
            parse_trigger_kind("ComplexityThreshold"),
            PauseKind::ReviewNeeded
        );
        assert_eq!(parse_trigger_kind("policy"), PauseKind::PolicyApproval);
        assert_eq!(parse_trigger_kind("resource"), PauseKind::ResourceWait);
        assert_eq!(parse_trigger_kind("manual"), PauseKind::UserRequested);
        assert_eq!(parse_trigger_kind("unknown"), PauseKind::ReviewNeeded);
    }

    #[tokio::test]
    async fn test_register_pause_notifies_subscribers() {
        let tracker = ServerPauseTracker::new();
        let mut rx = tracker.subscribe_changes();
        assert_eq!(*rx.borrow(), 0);

        tracker.register_pause(
            "tenant-1".to_string(),
            WorkerStreamPaused {
                pause_id: "pause-notify-1".to_string(),
                inference_id: "infer-notify-1".to_string(),
                trigger_kind: "review".to_string(),
                context: Some("check".to_string()),
                text_so_far: None,
                token_count: 1,
            },
            PathBuf::from("/var/run/worker.sock"),
        );

        tokio::time::timeout(Duration::from_secs(1), rx.changed())
            .await
            .expect("change notification timeout")
            .expect("change channel closed");
        assert_eq!(*rx.borrow(), 1);
    }

    #[tokio::test]
    async fn test_server_pause_and_remove_notify_subscribers() {
        let tracker = ServerPauseTracker::new();
        let mut rx = tracker.subscribe_changes();
        assert_eq!(*rx.borrow(), 0);

        tracker.register_server_pause(
            "tenant-1".to_string(),
            "pause-notify-2".to_string(),
            "resource-1".to_string(),
            "policy",
            Some("requires approval".to_string()),
            None,
        );
        tokio::time::timeout(Duration::from_secs(1), rx.changed())
            .await
            .expect("server pause change notification timeout")
            .expect("change channel closed");
        let after_server_pause = *rx.borrow();
        assert!(after_server_pause >= 1);

        tracker.remove("pause-notify-2");
        tokio::time::timeout(Duration::from_secs(1), rx.changed())
            .await
            .expect("remove change notification timeout")
            .expect("change channel closed");
        let after_remove = *rx.borrow();
        assert!(after_remove > after_server_pause);

        tracker.remove("unknown-pause");
        tokio::time::timeout(Duration::from_secs(1), rx.changed())
            .await
            .expect("unknown remove change notification timeout")
            .expect("change channel closed");
        assert!(*rx.borrow() > after_remove);
    }
}
