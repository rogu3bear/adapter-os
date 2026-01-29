//! Inference state tracker for full lifecycle management.
//!
//! Tracks all inference requests through their lifecycle:
//! Running → Paused → Running → Complete/Failed/Cancelled
//!
//! This complements ServerPauseTracker by tracking all states, not just paused ones.
//!
//! ## Idempotency (ANCHOR, AUDIT, RECTIFY)
//!
//! - **ANCHOR**: `register_inference` rejects duplicates while request is in-flight
//! - **AUDIT**: Tracks `idempotency_accepts` and `idempotency_rejects` counters
//! - **RECTIFY**: Terminal states (Failed) allow retry - `is_terminal()` check passes
//!
//! ## In-Flight Guard (ANCHOR, AUDIT, RECTIFY)
//!
//! Prevents adapter modification during inference to avoid race conditions:
//!
//! - **ANCHOR**: `is_adapter_in_flight()` enforces invariant before lifecycle transitions
//! - **AUDIT**: Tracks `in_flight_guard_allows` and `in_flight_guard_blocks` counters
//! - **RECTIFY**: Returns false (blocked) with actionable logging for retry

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use tracing::{debug, info, warn};

use adapteros_api_types::review::{InferenceState, PauseKind, PauseReason, ReviewContext};
use adapteros_telemetry::diagnostics::{
    DiagEnvelope, DiagEvent, DiagRunId, DiagSeverity, DiagnosticsService,
};
use adapteros_telemetry::tracing::TraceContext;

/// Entry tracking an inference through its lifecycle
#[derive(Debug, Clone)]
pub struct InferenceEntry {
    /// Inference request ID (primary key)
    pub inference_id: String,
    /// Current state
    pub state: TrackedState,
    /// Tenant ID for isolation
    pub tenant_id: String,
    /// Adapter IDs being used by this inference
    pub adapter_ids: Vec<String>,
    /// When inference started (monotonic, for duration calculation)
    pub started_at: Instant,
    /// When inference started (wall clock, for API responses)
    pub created_at: DateTime<Utc>,
    /// When state last changed (monotonic)
    pub state_changed_at: Instant,
    /// Total token count (if available)
    pub token_count: Option<u32>,
    /// Error code (if Failed state)
    pub error_code: Option<String>,
    /// Whether this is a replay
    pub is_replay: bool,
}

/// Simplified tracked state (internal representation)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackedState {
    /// Actively generating tokens
    Running,
    /// Waiting for external review
    Paused {
        pause_id: String,
        pause_kind: String,
    },
    /// Successfully finished
    Complete,
    /// Failed with error
    Failed,
    /// User cancelled
    Cancelled,
}

impl TrackedState {
    /// Convert to API InferenceState
    pub fn to_api_state(&self) -> InferenceState {
        match self {
            TrackedState::Running => InferenceState::Running,
            TrackedState::Paused {
                pause_id,
                pause_kind,
            } => {
                // Create minimal pause reason for API
                InferenceState::Paused(PauseReason {
                    kind: parse_pause_kind(pause_kind),
                    pause_id: pause_id.clone(),
                    context: ReviewContext {
                        code: None,
                        question: None,
                        scope: vec![],
                        metadata: None,
                    },
                    created_at: None,
                })
            }
            TrackedState::Complete => InferenceState::Complete,
            TrackedState::Failed => InferenceState::Failed,
            TrackedState::Cancelled => InferenceState::Cancelled,
        }
    }

    /// Get state name for logging/diagnostics
    pub fn name(&self) -> &'static str {
        match self {
            TrackedState::Running => "Running",
            TrackedState::Paused { .. } => "Paused",
            TrackedState::Complete => "Complete",
            TrackedState::Failed => "Failed",
            TrackedState::Cancelled => "Cancelled",
        }
    }

    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TrackedState::Complete | TrackedState::Failed | TrackedState::Cancelled
        )
    }
}

/// Configuration for InferenceStateTracker
#[derive(Debug, Clone)]
pub struct StateTrackerConfig {
    /// TTL for terminal states (Complete/Failed/Cancelled) before cleanup
    pub terminal_ttl: Duration,
    /// Whether to enable diagnostics emission
    pub enable_diagnostics: bool,
}

impl Default for StateTrackerConfig {
    fn default() -> Self {
        Self {
            terminal_ttl: Duration::from_secs(3600), // 1 hour
            enable_diagnostics: true,
        }
    }
}

/// Inference state tracker for full lifecycle management
pub struct InferenceStateTracker {
    /// Active and recently completed inferences
    inferences: RwLock<HashMap<String, InferenceEntry>>,
    /// Configuration
    config: StateTrackerConfig,
    /// Optional diagnostics service for event emission
    diagnostics: Option<Arc<DiagnosticsService>>,
    /// AUDIT: Count of successful idempotency checks (request accepted)
    idempotency_accepts: AtomicU64,
    /// AUDIT: Count of rejected requests (duplicate in-flight)
    idempotency_rejects: AtomicU64,
    /// AUDIT: Count of in-flight guard checks that allowed modification
    in_flight_guard_allows: AtomicU64,
    /// AUDIT: Count of in-flight guard checks that blocked modification
    in_flight_guard_blocks: AtomicU64,
}

impl Default for InferenceStateTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl InferenceStateTracker {
    /// Create a new tracker with default config
    pub fn new() -> Self {
        Self {
            inferences: RwLock::new(HashMap::new()),
            config: StateTrackerConfig::default(),
            diagnostics: None,
            idempotency_accepts: AtomicU64::new(0),
            idempotency_rejects: AtomicU64::new(0),
            in_flight_guard_allows: AtomicU64::new(0),
            in_flight_guard_blocks: AtomicU64::new(0),
        }
    }

    /// Create with custom config
    pub fn with_config(config: StateTrackerConfig) -> Self {
        Self {
            inferences: RwLock::new(HashMap::new()),
            config,
            diagnostics: None,
            idempotency_accepts: AtomicU64::new(0),
            idempotency_rejects: AtomicU64::new(0),
            in_flight_guard_allows: AtomicU64::new(0),
            in_flight_guard_blocks: AtomicU64::new(0),
        }
    }

    /// Attach diagnostics service for event emission
    pub fn with_diagnostics(mut self, service: Arc<DiagnosticsService>) -> Self {
        self.diagnostics = Some(service);
        self
    }

    /// Register a new inference at Running state
    ///
    /// Returns false if a request with this ID is already in-flight (idempotency check)
    pub fn register_inference(
        &self,
        inference_id: String,
        tenant_id: String,
        is_replay: bool,
    ) -> bool {
        self.register_inference_with_adapters(inference_id, tenant_id, is_replay, Vec::new())
    }

    /// Register a new inference at Running state with adapter IDs
    ///
    /// Returns false if a request with this ID is already in-flight (idempotency check)
    pub fn register_inference_with_adapters(
        &self,
        inference_id: String,
        tenant_id: String,
        is_replay: bool,
        adapter_ids: Vec<String>,
    ) -> bool {
        let now = Utc::now();
        let instant = Instant::now();

        // Idempotency check: reject if already in-flight
        {
            let guard = self.inferences.read();
            if let Some(existing) = guard.get(&inference_id) {
                if !existing.state.is_terminal() {
                    // AUDIT: Track idempotency rejection
                    self.idempotency_rejects.fetch_add(1, Ordering::Relaxed);
                    warn!(
                        inference_id = %inference_id,
                        state = %existing.state.name(),
                        idempotency_rejects = self.idempotency_rejects.load(Ordering::Relaxed),
                        "Duplicate request rejected: inference already in-flight"
                    );
                    return false;
                }
            }
        }

        let entry = InferenceEntry {
            inference_id: inference_id.clone(),
            state: TrackedState::Running,
            tenant_id: tenant_id.clone(),
            adapter_ids,
            started_at: instant,
            created_at: now,
            state_changed_at: instant,
            token_count: None,
            error_code: None,
            is_replay,
        };

        // AUDIT: Track idempotency acceptance
        self.idempotency_accepts.fetch_add(1, Ordering::Relaxed);

        debug!(
            inference_id = %inference_id,
            tenant_id = %tenant_id,
            is_replay = is_replay,
            idempotency_accepts = self.idempotency_accepts.load(Ordering::Relaxed),
            "Registered inference as Running"
        );

        // Emit diagnostic event
        self.emit_state_changed(&entry, "Started", "Running", 0);

        self.inferences.write().insert(inference_id, entry);
        true
    }

    /// Check if a request with this ID is already in-flight
    pub fn is_request_in_flight(&self, inference_id: &str) -> bool {
        self.inferences
            .read()
            .get(inference_id)
            .map(|e| !e.state.is_terminal())
            .unwrap_or(false)
    }

    /// Get all adapter IDs currently in use by active (non-terminal) inferences
    ///
    /// Used to prevent modification of adapters during inference
    pub fn adapters_in_flight(&self) -> std::collections::HashSet<String> {
        self.inferences
            .read()
            .values()
            .filter(|e| !e.state.is_terminal())
            .flat_map(|e| e.adapter_ids.iter().cloned())
            .collect()
    }

    /// Check if an adapter is in-flight and update AUDIT metrics
    ///
    /// Returns true if the adapter is in-flight (blocked), false if safe to modify.
    /// This method updates `in_flight_guard_allows` and `in_flight_guard_blocks` counters.
    pub fn is_adapter_in_flight(&self, adapter_id: &str) -> bool {
        let in_flight = self.adapters_in_flight();
        let is_blocked = in_flight.contains(adapter_id);

        if is_blocked {
            self.in_flight_guard_blocks.fetch_add(1, Ordering::Relaxed);
            debug!(
                adapter_id = %adapter_id,
                in_flight_count = in_flight.len(),
                total_blocks = self.in_flight_guard_blocks.load(Ordering::Relaxed),
                "In-flight guard: blocked adapter modification"
            );
        } else {
            self.in_flight_guard_allows.fetch_add(1, Ordering::Relaxed);
        }

        is_blocked
    }

    /// Update adapter IDs for an inference (called when adapters are resolved)
    pub fn update_adapter_ids(&self, inference_id: &str, adapter_ids: Vec<String>) -> bool {
        let mut guard = self.inferences.write();
        if let Some(entry) = guard.get_mut(inference_id) {
            entry.adapter_ids = adapter_ids;
            true
        } else {
            false
        }
    }

    /// Transition inference to Paused state
    pub fn mark_paused(
        &self,
        inference_id: &str,
        pause_id: String,
        pause_kind: String,
        token_count: Option<u32>,
    ) -> bool {
        let mut guard = self.inferences.write();
        if let Some(entry) = guard.get_mut(inference_id) {
            let from_state = entry.state.name().to_string();
            let state_duration_us = entry.state_changed_at.elapsed().as_micros() as u64;

            entry.state = TrackedState::Paused {
                pause_id: pause_id.clone(),
                pause_kind: pause_kind.clone(),
            };
            entry.state_changed_at = Instant::now();
            if let Some(tc) = token_count {
                entry.token_count = Some(tc);
            }

            debug!(
                inference_id = %inference_id,
                pause_id = %pause_id,
                "Transitioned inference to Paused"
            );

            // Clone for diagnostic emission after releasing lock
            let entry_clone = entry.clone();
            drop(guard);

            self.emit_state_changed(&entry_clone, &from_state, "Paused", state_duration_us);
            true
        } else {
            warn!(inference_id = %inference_id, "Cannot mark paused: inference not found");
            false
        }
    }

    /// Transition inference from Paused back to Running
    pub fn mark_resumed(&self, inference_id: &str) -> bool {
        let mut guard = self.inferences.write();
        if let Some(entry) = guard.get_mut(inference_id) {
            let from_state = entry.state.name().to_string();
            let state_duration_us = entry.state_changed_at.elapsed().as_micros() as u64;

            entry.state = TrackedState::Running;
            entry.state_changed_at = Instant::now();

            debug!(inference_id = %inference_id, "Transitioned inference to Running (resumed)");

            let entry_clone = entry.clone();
            drop(guard);

            self.emit_state_changed(&entry_clone, &from_state, "Running", state_duration_us);
            true
        } else {
            warn!(inference_id = %inference_id, "Cannot mark resumed: inference not found");
            false
        }
    }

    /// Mark inference as complete
    pub fn mark_complete(&self, inference_id: &str, token_count: Option<u32>) -> bool {
        let mut guard = self.inferences.write();
        if let Some(entry) = guard.get_mut(inference_id) {
            let from_state = entry.state.name().to_string();
            let state_duration_us = entry.state_changed_at.elapsed().as_micros() as u64;

            entry.state = TrackedState::Complete;
            entry.state_changed_at = Instant::now();
            if let Some(tc) = token_count {
                entry.token_count = Some(tc);
            }

            info!(
                inference_id = %inference_id,
                total_duration_ms = entry.started_at.elapsed().as_millis() as u64,
                "Inference completed"
            );

            let entry_clone = entry.clone();
            drop(guard);

            self.emit_state_changed(&entry_clone, &from_state, "Complete", state_duration_us);
            true
        } else {
            warn!(inference_id = %inference_id, "Cannot mark complete: inference not found");
            false
        }
    }

    /// Mark inference as failed
    pub fn mark_failed(&self, inference_id: &str, error_code: String) -> bool {
        let mut guard = self.inferences.write();
        if let Some(entry) = guard.get_mut(inference_id) {
            let from_state = entry.state.name().to_string();
            let state_duration_us = entry.state_changed_at.elapsed().as_micros() as u64;

            entry.state = TrackedState::Failed;
            entry.state_changed_at = Instant::now();
            entry.error_code = Some(error_code.clone());

            warn!(
                inference_id = %inference_id,
                error_code = %error_code,
                total_duration_ms = entry.started_at.elapsed().as_millis() as u64,
                "Inference failed"
            );

            let entry_clone = entry.clone();
            drop(guard);

            self.emit_state_changed(&entry_clone, &from_state, "Failed", state_duration_us);
            true
        } else {
            warn!(inference_id = %inference_id, "Cannot mark failed: inference not found");
            false
        }
    }

    /// Mark inference as cancelled
    pub fn mark_cancelled(&self, inference_id: &str) -> bool {
        let mut guard = self.inferences.write();
        if let Some(entry) = guard.get_mut(inference_id) {
            let from_state = entry.state.name().to_string();
            let state_duration_us = entry.state_changed_at.elapsed().as_micros() as u64;

            entry.state = TrackedState::Cancelled;
            entry.state_changed_at = Instant::now();

            info!(
                inference_id = %inference_id,
                total_duration_ms = entry.started_at.elapsed().as_millis() as u64,
                "Inference cancelled"
            );

            let entry_clone = entry.clone();
            drop(guard);

            self.emit_state_changed(&entry_clone, &from_state, "Cancelled", state_duration_us);
            true
        } else {
            warn!(inference_id = %inference_id, "Cannot mark cancelled: inference not found");
            false
        }
    }

    /// Get current state for an inference
    pub fn get_state(&self, inference_id: &str) -> Option<InferenceState> {
        self.inferences
            .read()
            .get(inference_id)
            .map(|e| e.state.to_api_state())
    }

    /// Get full entry for an inference
    pub fn get_entry(&self, inference_id: &str) -> Option<InferenceEntry> {
        self.inferences.read().get(inference_id).cloned()
    }

    /// Get all active (non-terminal) inferences
    pub fn list_active(&self) -> Vec<InferenceEntry> {
        self.inferences
            .read()
            .values()
            .filter(|e| !e.state.is_terminal())
            .cloned()
            .collect()
    }

    /// Get count of tracked inferences
    pub fn count(&self) -> usize {
        self.inferences.read().len()
    }

    /// Get count of active (non-terminal) inferences
    pub fn count_active(&self) -> usize {
        self.inferences
            .read()
            .values()
            .filter(|e| !e.state.is_terminal())
            .count()
    }

    /// AUDIT: Get count of accepted inference requests (idempotency passed)
    pub fn idempotency_accepts(&self) -> u64 {
        self.idempotency_accepts.load(Ordering::Relaxed)
    }

    /// AUDIT: Get count of rejected inference requests (duplicate in-flight)
    pub fn idempotency_rejects(&self) -> u64 {
        self.idempotency_rejects.load(Ordering::Relaxed)
    }

    /// AUDIT: Get count of in-flight guard checks that allowed modification
    pub fn in_flight_guard_allows(&self) -> u64 {
        self.in_flight_guard_allows.load(Ordering::Relaxed)
    }

    /// AUDIT: Get count of in-flight guard checks that blocked modification
    pub fn in_flight_guard_blocks(&self) -> u64 {
        self.in_flight_guard_blocks.load(Ordering::Relaxed)
    }

    /// Remove an inference entry
    pub fn remove(&self, inference_id: &str) -> bool {
        self.inferences.write().remove(inference_id).is_some()
    }

    /// Cleanup terminal states older than TTL
    pub fn cleanup_expired(&self) -> usize {
        let ttl = self.config.terminal_ttl;

        let mut guard = self.inferences.write();
        let before = guard.len();

        guard.retain(|_, entry| {
            if entry.state.is_terminal() {
                // Keep if state change is within TTL
                entry.state_changed_at.elapsed() < ttl
            } else {
                // Always keep non-terminal entries
                true
            }
        });

        let removed = before - guard.len();
        if removed > 0 {
            debug!(removed = removed, "Cleaned up expired inference entries");
        }
        removed
    }

    /// Emit state change diagnostic event
    fn emit_state_changed(
        &self,
        entry: &InferenceEntry,
        from_state: &str,
        to_state: &str,
        state_duration_us: u64,
    ) {
        if !self.config.enable_diagnostics {
            return;
        }

        if let Some(ref diag) = self.diagnostics {
            let trace_ctx = TraceContext::new_root();
            let run_id = DiagRunId::from_trace_context(&trace_ctx);
            let total_duration_us = entry.started_at.elapsed().as_micros() as u64;

            let envelope = DiagEnvelope::new(
                &trace_ctx,
                &entry.tenant_id,
                run_id,
                DiagSeverity::Info,
                total_duration_us,
                DiagEvent::InferenceStateChanged {
                    inference_id: entry.inference_id.clone(),
                    from_state: from_state.to_string(),
                    to_state: to_state.to_string(),
                    state_duration_us,
                    total_duration_us,
                    error_code: entry.error_code.clone(),
                },
            );

            if let Err(e) = diag.emit(envelope) {
                warn!(error = %e, "Failed to emit InferenceStateChanged diagnostic");
            }
        }
    }
}

/// Parse pause kind string to PauseKind enum
fn parse_pause_kind(kind: &str) -> PauseKind {
    match kind.to_lowercase().as_str() {
        "reviewneeded" | "review_needed" | "review" => PauseKind::ReviewNeeded,
        "policyapproval" | "policy_approval" | "policy" => PauseKind::PolicyApproval,
        "resourcewait" | "resource_wait" | "resource" => PauseKind::ResourceWait,
        "userrequested" | "user_requested" | "manual" => PauseKind::UserRequested,
        _ => PauseKind::ReviewNeeded,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get_state() {
        let tracker = InferenceStateTracker::new();

        tracker.register_inference("infer-001".to_string(), "tenant-001".to_string(), false);

        let state = tracker.get_state("infer-001");
        assert!(state.is_some());
        assert!(matches!(state.unwrap(), InferenceState::Running));
    }

    #[test]
    fn test_state_transitions() {
        let tracker = InferenceStateTracker::new();

        tracker.register_inference("infer-002".to_string(), "tenant-001".to_string(), false);

        // Running -> Paused
        assert!(tracker.mark_paused(
            "infer-002",
            "pause-001".to_string(),
            "ExplicitTag".to_string(),
            Some(10)
        ));

        let state = tracker.get_state("infer-002");
        assert!(matches!(state, Some(InferenceState::Paused(_))));

        // Paused -> Running
        assert!(tracker.mark_resumed("infer-002"));
        let state = tracker.get_state("infer-002");
        assert!(matches!(state, Some(InferenceState::Running)));

        // Running -> Complete
        assert!(tracker.mark_complete("infer-002", Some(50)));
        let state = tracker.get_state("infer-002");
        assert!(matches!(state, Some(InferenceState::Complete)));
    }

    #[test]
    fn test_mark_failed() {
        let tracker = InferenceStateTracker::new();

        tracker.register_inference("infer-003".to_string(), "tenant-001".to_string(), false);

        assert!(tracker.mark_failed("infer-003", "E1001".to_string()));

        let entry = tracker.get_entry("infer-003");
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert!(matches!(entry.state, TrackedState::Failed));
        assert_eq!(entry.error_code, Some("E1001".to_string()));
    }

    #[test]
    fn test_cleanup_expired() {
        let config = StateTrackerConfig {
            terminal_ttl: Duration::from_millis(10),
            enable_diagnostics: false,
        };
        let tracker = InferenceStateTracker::with_config(config);

        tracker.register_inference("infer-004".to_string(), "tenant-001".to_string(), false);

        // Mark complete (terminal state)
        tracker.mark_complete("infer-004", None);

        // Still present
        assert_eq!(tracker.count(), 1);

        // Wait for TTL
        std::thread::sleep(Duration::from_millis(20));

        // Cleanup
        let removed = tracker.cleanup_expired();
        assert_eq!(removed, 1);
        assert_eq!(tracker.count(), 0);
    }

    #[test]
    fn test_list_active() {
        let tracker = InferenceStateTracker::new();

        tracker.register_inference("active-1".to_string(), "tenant".to_string(), false);
        tracker.register_inference("active-2".to_string(), "tenant".to_string(), false);
        tracker.register_inference("done".to_string(), "tenant".to_string(), false);

        tracker.mark_complete("done", None);

        let active = tracker.list_active();
        assert_eq!(active.len(), 2);
        assert_eq!(tracker.count_active(), 2);
        assert_eq!(tracker.count(), 3);
    }

    #[test]
    fn test_not_found_handling() {
        let tracker = InferenceStateTracker::new();

        // Operations on non-existent inference should return false
        assert!(!tracker.mark_paused("nonexistent", "pause".to_string(), "kind".to_string(), None));
        assert!(!tracker.mark_resumed("nonexistent"));
        assert!(!tracker.mark_complete("nonexistent", None));
        assert!(!tracker.mark_failed("nonexistent", "error".to_string()));
        assert!(!tracker.mark_cancelled("nonexistent"));
    }
}
