//! Boot lifecycle state management for AdapterOS server
//!
//! Implements the lifecycle state machine for server boot, runtime, and shutdown.
//!
//! ## State Flow
//!
//! ```text
//! stopped → starting → db-connecting → migrating → seeding → loading-policies →
//! starting-backend → loading-base-models → loading-adapters → worker-discovery →
//! ready → fully-ready → draining → stopping
//!
//! Any state can transition to:
//!   - failed (critical failure with reason_code)
//!   - degraded (non-critical dependency failure, only from ready states)
//!
//! Degraded can recover to ready, but failed is terminal.
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_server::boot_state::{BootState, BootStateManager, FailureReason};
//!
//! let manager = BootStateManager::new();
//!
//! // Transition through states
//! manager.start().await;
//! manager.db_connecting().await;
//! manager.migrating().await;
//! manager.seeding().await;
//! manager.worker_discovery().await;
//! manager.ready().await;
//!
//! // Check current state
//! if manager.is_ready() {
//!     // Accept requests
//! }
//!
//! // Handle failures
//! if critical_error {
//!     manager.fail(FailureReason::new("DB_CONN_FAILED", "Database connection timeout")).await;
//! }
//!
//! // Handle degraded state (non-critical)
//! if non_critical_dependency_down {
//!     manager.degrade("metrics-unavailable").await;
//! }
//! ```
//!
//! ## Integration Points
//!
//! - **Health Endpoints**: `/readyz` returns 503 unless state is `Ready`
//! - **Audit Logs**: Each transition emits an audit event
//! - **Metrics**: State transitions are recorded as telemetry events
//!
//! 【2025-11-25†feat(boot)†lifecycle-state-machine】

use adapteros_db::Db;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

// Re-export BootPhase as BootState for backward compatibility.
// This consolidates the duplicate boot state enums into a single source of truth
// defined in adapteros-boot, eliminating enum drift across crates.
pub use adapteros_boot::BootPhase as BootState;

/// State transition event
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// Previous state
    pub from: BootState,
    /// New state
    pub to: BootState,
    /// Reason for transition
    pub reason: String,
    /// Time elapsed since process start
    pub elapsed: Duration,
    /// Timestamp of transition
    pub timestamp: Instant,
}

/// Failure reason with structured code for programmatic handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailureReason {
    /// Machine-readable failure code (e.g., "DB_CONN_TIMEOUT", "MIGRATION_FAILED")
    pub code: String,
    /// Human-readable failure message
    pub message: String,
    /// Optional component that failed
    pub component: Option<String>,
    /// Whether this failure is recoverable (for future use)
    pub recoverable: bool,
}

impl FailureReason {
    /// Create a new failure reason
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            component: None,
            recoverable: false,
        }
    }

    /// Create a failure reason with component information
    pub fn with_component(
        code: impl Into<String>,
        message: impl Into<String>,
        component: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            component: Some(component.into()),
            recoverable: false,
        }
    }

    /// Mark this failure as potentially recoverable
    pub fn recoverable(mut self) -> Self {
        self.recoverable = true;
        self
    }
}

impl std::fmt::Display for FailureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(ref component) = self.component {
            write!(f, "[{}] {}: {}", self.code, component, self.message)
        } else {
            write!(f, "[{}] {}", self.code, self.message)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum PhaseOutcome {
    Pending,
    InProgress,
    Success,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PhaseStatus {
    pub name: String,
    pub status: PhaseOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Standard failure codes for boot failures
pub mod failure_codes {
    /// Database connection failed
    pub const DB_CONN_FAILED: &str = "DB_CONN_FAILED";
    /// Database connection timeout
    pub const DB_CONN_TIMEOUT: &str = "DB_CONN_TIMEOUT";
    /// Migration failed
    pub const MIGRATION_FAILED: &str = "MIGRATION_FAILED";
    /// Migration signature verification failed
    pub const MIGRATION_SIG_FAILED: &str = "MIGRATION_SIG_FAILED";
    /// Policy verification failed
    pub const POLICY_VERIFY_FAILED: &str = "POLICY_VERIFY_FAILED";
    /// Backend initialization failed
    pub const BACKEND_INIT_FAILED: &str = "BACKEND_INIT_FAILED";
    /// Model loading failed
    pub const MODEL_LOAD_FAILED: &str = "MODEL_LOAD_FAILED";
    /// Worker discovery failed
    pub const WORKER_DISCOVERY_FAILED: &str = "WORKER_DISCOVERY_FAILED";
    /// Socket bind failed
    pub const SOCKET_BIND_FAILED: &str = "SOCKET_BIND_FAILED";
    /// Configuration error
    pub const CONFIG_ERROR: &str = "CONFIG_ERROR";
    /// Security check failed
    pub const SECURITY_CHECK_FAILED: &str = "SECURITY_CHECK_FAILED";
    /// Boot timeout exceeded
    pub const BOOT_TIMEOUT: &str = "BOOT_TIMEOUT";
    /// Security initialization failed
    pub const SECURITY_INIT_FAILED: &str = "SECURITY_INIT_FAILED";
    /// Executor initialization failed
    pub const EXECUTOR_INIT_FAILED: &str = "EXECUTOR_INIT_FAILED";
    /// Preflight checks failed
    pub const PREFLIGHT_FAILED: &str = "PREFLIGHT_FAILED";
    /// Router build failed
    pub const ROUTER_BUILD_FAILED: &str = "ROUTER_BUILD_FAILED";
    /// Bind failed
    pub const BIND_FAILED: &str = "BIND_FAILED";
    /// Worker attach failed
    pub const WORKER_ATTACH_FAILED: &str = "WORKER_ATTACH_FAILED";
    /// OpenTelemetry initialization failed
    pub const OTEL_INIT_FAILED: &str = "OTEL_INIT_FAILED";
}

/// Degraded state reason for non-critical dependency failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DegradedReason {
    /// Component that is degraded
    pub component: String,
    /// Reason for degradation
    pub reason: String,
    /// When the degradation was detected
    pub detected_at: Instant,
}

/// Model loading status tracking
///
/// Uses `BTreeSet` for deterministic ordering of model IDs. This ensures
/// consistent iteration order regardless of concurrent insertion order,
/// aligning with the project's determinism requirements.
#[derive(Debug, Clone, Default)]
pub struct ModelLoadingStatus {
    /// Models still being loaded (sorted by model ID)
    pub pending: BTreeSet<String>,
    /// Models successfully loaded (sorted by model ID)
    pub ready: BTreeSet<String>,
    /// Models that failed to load (sorted by model ID)
    pub failed: BTreeSet<String>,
}

/// A warning recorded during boot that doesn't prevent startup but indicates
/// reduced functionality. Exposed via /readyz for operator visibility.
///
/// Boot warnings are distinct from the Degraded state - they capture issues
/// that occurred during boot (before Ready state) when Degraded transitions
/// aren't possible. This provides honest observability without lying about
/// the system state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BootWarning {
    /// Component or task that had the issue
    pub component: String,
    /// Human-readable description of what failed
    pub message: String,
    /// Milliseconds since boot when the warning was recorded
    pub recorded_at_ms: u64,
}

/// Manager for boot lifecycle state
pub struct BootStateManager {
    /// Current state
    current: Arc<RwLock<BootState>>,
    /// Process start time
    start_time: Instant,
    /// Boot trace identifier for correlating logs/readyz
    boot_trace_id: String,
    /// Database for audit logging (optional)
    db: Option<Arc<Db>>,
    /// Model loading status
    model_status: Arc<RwLock<ModelLoadingStatus>>,
    /// Failure reason (set when transitioning to Failed state)
    failure_reason: Arc<RwLock<Option<FailureReason>>>,
    /// Degraded reasons (components that have failed non-critically)
    degraded_reasons: Arc<RwLock<Vec<DegradedReason>>>,
    /// Transition history (for metrics and diagnostics)
    transitions: Arc<RwLock<Vec<StateTransition>>>,
    /// Phase timing/status tracking
    phases: Arc<RwLock<HashMap<String, PhaseStatus>>>,
    /// Warnings recorded during boot (non-fatal issues exposed via /readyz)
    boot_warnings: Arc<RwLock<Vec<BootWarning>>>,
}

impl BootStateManager {
    /// Create a new boot state manager
    pub fn new() -> Self {
        Self {
            current: Arc::new(RwLock::new(BootState::Stopped)),
            start_time: Instant::now(),
            boot_trace_id: Uuid::new_v4().to_string(),
            db: None,
            model_status: Arc::new(RwLock::new(ModelLoadingStatus::default())),
            failure_reason: Arc::new(RwLock::new(None)),
            degraded_reasons: Arc::new(RwLock::new(Vec::new())),
            transitions: Arc::new(RwLock::new(Vec::new())),
            phases: Arc::new(RwLock::new(std::collections::HashMap::new())),
            boot_warnings: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Attach a database handle without resetting state or counters.
    /// Returns a new manager sharing the same state/time/model status.
    pub fn with_db(&self, db: Arc<Db>) -> Self {
        Self {
            current: Arc::clone(&self.current),
            start_time: self.start_time,
            boot_trace_id: self.boot_trace_id.clone(),
            db: Some(db),
            model_status: Arc::clone(&self.model_status),
            failure_reason: Arc::clone(&self.failure_reason),
            degraded_reasons: Arc::clone(&self.degraded_reasons),
            transitions: Arc::clone(&self.transitions),
            phases: Arc::clone(&self.phases),
            boot_warnings: Arc::clone(&self.boot_warnings),
        }
    }

    /// Alias for `with_db` to keep call sites clear when upgrading after startup.
    pub fn attach_db(&self, db: Arc<Db>) -> Self {
        self.with_db(db)
    }

    /// Get the current state
    pub fn current_state(&self) -> BootState {
        *self.current.read()
    }

    /// Check if server is ready to accept requests
    pub fn is_ready(&self) -> bool {
        self.current_state().is_ready()
    }

    /// Boot trace identifier for correlating logs/readyz
    pub fn boot_trace_id(&self) -> String {
        self.boot_trace_id.clone()
    }

    /// Begin tracking a boot phase
    pub fn start_phase(&self, name: &str) {
        let mut phases = self.phases.write();
        let status = PhaseStatus {
            name: name.to_string(),
            status: PhaseOutcome::InProgress,
            started_at_ms: Some(self.start_time.elapsed().as_millis() as u64),
            finished_at_ms: None,
            duration_ms: None,
            error_code: None,
            hint: None,
        };
        phases.insert(name.to_string(), status);
    }

    /// Mark a boot phase as successful
    pub fn finish_phase_ok(&self, name: &str) {
        let mut phases = self.phases.write();
        let entry = phases
            .entry(name.to_string())
            .or_insert_with(|| PhaseStatus {
                name: name.to_string(),
                status: PhaseOutcome::Pending,
                started_at_ms: Some(self.start_time.elapsed().as_millis() as u64),
                finished_at_ms: None,
                duration_ms: None,
                error_code: None,
                hint: None,
            });
        entry.status = PhaseOutcome::Success;
        let now_ms = self.start_time.elapsed().as_millis() as u64;
        entry.finished_at_ms = Some(now_ms);
        entry.duration_ms = entry.started_at_ms.map(|s| now_ms.saturating_sub(s));
    }

    /// Mark a boot phase as failed with an error code/hint
    pub fn finish_phase_err(&self, name: &str, code: &str, hint: Option<String>) {
        let mut phases = self.phases.write();
        let entry = phases
            .entry(name.to_string())
            .or_insert_with(|| PhaseStatus {
                name: name.to_string(),
                status: PhaseOutcome::Pending,
                started_at_ms: Some(self.start_time.elapsed().as_millis() as u64),
                finished_at_ms: None,
                duration_ms: None,
                error_code: None,
                hint: None,
            });
        entry.status = PhaseOutcome::Failed;
        let now_ms = self.start_time.elapsed().as_millis() as u64;
        entry.finished_at_ms = Some(now_ms);
        entry.duration_ms = entry.started_at_ms.map(|s| now_ms.saturating_sub(s));
        entry.error_code = Some(code.to_string());
        entry.hint = hint;
    }

    /// Snapshot of all boot phases, ordered by execution time (earliest first).
    ///
    /// Phases are sorted by their `started_at_ms` timestamp to reflect the actual
    /// execution timeline. Phases without a start time appear last, sorted by name.
    pub fn phase_statuses(&self) -> Vec<PhaseStatus> {
        let phases = self.phases.read();
        let mut vals: Vec<_> = phases.values().cloned().collect();
        vals.sort_by(|a, b| match (a.started_at_ms, b.started_at_ms) {
            (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.name.cmp(&b.name),
        });
        vals
    }

    /// Check if server is accepting requests (Ready or FullyReady)
    pub fn is_accepting_requests(&self) -> bool {
        self.current_state().is_ready()
    }

    /// Get last boot error code if any
    pub fn last_error_code(&self) -> Option<String> {
        if let Some(reason) = &*self.failure_reason.read() {
            return Some(reason.code.clone());
        }
        let phases = self.phases.read();
        phases
            .values()
            .filter(|p| matches!(p.status, PhaseOutcome::Failed))
            .max_by(|a, b| {
                let a_time = a.finished_at_ms.or(a.started_at_ms).unwrap_or_default();
                let b_time = b.finished_at_ms.or(b.started_at_ms).unwrap_or_default();
                a_time.cmp(&b_time).then_with(|| a.name.cmp(&b.name))
            })
            .and_then(|p| p.error_code.clone())
    }

    /// Check if all models are loaded and healthy
    pub fn is_fully_ready(&self) -> bool {
        self.current_state().is_fully_ready()
    }

    /// Check if server is shutting down
    pub fn is_shutting_down(&self) -> bool {
        self.current_state().is_shutting_down()
    }

    /// Check if server is in maintenance
    pub fn is_maintenance(&self) -> bool {
        self.current_state().is_maintenance()
    }

    /// Check if server is draining or stopping
    pub fn is_draining(&self) -> bool {
        self.current_state().is_draining()
    }

    /// Check if server is booting
    pub fn is_booting(&self) -> bool {
        self.current_state().is_booting()
    }

    /// Get count of models still loading
    pub fn pending_model_count(&self) -> usize {
        self.model_status.read().pending.len()
    }

    /// Get recorded transition history (monotonic since process start)
    pub fn transition_history(&self) -> Vec<StateTransition> {
        self.transitions.read().clone()
    }

    /// Get count of ready models
    pub fn ready_model_count(&self) -> usize {
        self.model_status.read().ready.len()
    }

    /// Get current model loading status
    pub fn get_model_status(&self) -> ModelLoadingStatus {
        self.model_status.read().clone()
    }

    /// Mark a model as pending
    pub fn add_pending_model(&self, model_id: String) {
        let mut status = self.model_status.write();
        status.pending.insert(model_id);
    }

    /// Mark a model as ready
    pub fn mark_model_ready(&self, model_id: String) {
        let mut status = self.model_status.write();
        status.pending.remove(&model_id);
        status.ready.insert(model_id);
    }

    /// Mark a model as failed
    pub fn mark_model_failed(&self, model_id: String) {
        let mut status = self.model_status.write();
        status.pending.remove(&model_id);
        status.failed.insert(model_id);
    }

    /// Get time elapsed since process start
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Transition to a new state
    ///
    /// This emits structured logs and audit events for the transition.
    pub async fn transition(&self, new_state: BootState, reason: &str) {
        let old_state = {
            let mut current = self.current.write();

            if !Self::is_allowed_transition(*current, new_state) {
                warn!(
                    attempted = %new_state,
                    current = %*current,
                    reason = reason,
                    "Rejected invalid boot state transition"
                );
                return;
            }

            let old = *current;
            *current = new_state;
            old
        };

        let elapsed = self.elapsed();

        // Emit structured log
        info!(
            state = %new_state,
            previous_state = %old_state,
            reason = reason,
            elapsed_ms = elapsed.as_millis() as u64,
            "Boot state transition"
        );

        // Create transition event
        let transition = StateTransition {
            from: old_state,
            to: new_state,
            reason: reason.to_string(),
            elapsed,
            timestamp: Instant::now(),
        };

        self.transitions.write().push(transition.clone());

        // Emit audit log if database is available
        if let Some(ref db) = self.db {
            let metadata = serde_json::json!({
                "from_state": old_state.as_str(),
                "to_state": new_state.as_str(),
                "reason": reason,
                "elapsed_ms": elapsed.as_millis(),
            });

            let result = db
                .log_audit(
                    "system",
                    "system",
                    "system",
                    "server.state_transition",
                    "boot_state",
                    Some(new_state.as_str()),
                    "success",
                    None,
                    None,
                    Some(&serde_json::to_string(&metadata).unwrap_or_default()),
                )
                .await;

            if let Err(e) = result {
                warn!(error = %e, "Failed to log state transition to audit trail");
            }
        }

        debug!(
            "State transition: {} → {} ({})",
            transition.from, transition.to, transition.reason
        );
    }

    /// Validate ordered transition to prevent skipping boot/drain steps.
    fn is_allowed_transition(from: BootState, to: BootState) -> bool {
        // Explicitly prevent transitions from terminal states (Failed, Stopping)
        if from.is_terminal() {
            return false;
        }

        // Failed state can be reached from any non-terminal state (critical failure)
        if to == BootState::Failed {
            return true;
        }

        // Degraded can only be reached from Ready states (non-critical dependency failure)
        // and can recover back to Ready
        if to == BootState::Degraded {
            return matches!(from, BootState::Ready | BootState::FullyReady);
        }

        // Recovery from Degraded back to Ready
        if from == BootState::Degraded && to == BootState::Ready {
            return true;
        }

        // Boot sequence transitions (strictly ordered)
        matches!(
            (from, to),
            // Boot flow: stopped → starting → db-connecting → migrating → seeding
            (BootState::Stopped, BootState::Starting)
                | (BootState::Starting, BootState::DbConnecting)
                | (BootState::DbConnecting, BootState::Migrating)
                | (BootState::Migrating, BootState::Seeding)
                | (BootState::Seeding, BootState::LoadingPolicies)
                // Backend initialization
                | (BootState::LoadingPolicies, BootState::StartingBackend)
                | (BootState::StartingBackend, BootState::LoadingBaseModels)
                | (BootState::LoadingBaseModels, BootState::LoadingAdapters)
                | (BootState::LoadingAdapters, BootState::WorkerDiscovery)
                | (BootState::WorkerDiscovery, BootState::Ready)
                // Ready state transitions
                | (BootState::Ready, BootState::FullyReady)
                | (BootState::Ready, BootState::Maintenance)
                | (BootState::FullyReady, BootState::Maintenance)
                | (BootState::Maintenance, BootState::Ready)
                // Shutdown transitions
                | (BootState::Ready, BootState::Draining)
                | (BootState::FullyReady, BootState::Draining)
                | (BootState::Degraded, BootState::Draining)
                | (BootState::Maintenance, BootState::Draining)
                | (BootState::Draining, BootState::Stopping)
        )
    }

    /// Transition to Starting state
    pub async fn start(&self) {
        self.transition(BootState::Starting, "process-start").await;
    }

    /// Transition to DbConnecting state
    pub async fn db_connecting(&self) {
        self.transition(BootState::DbConnecting, "config-loaded")
            .await;
    }

    /// Transition to Migrating state
    pub async fn migrating(&self) {
        self.transition(BootState::Migrating, "db-connected").await;
    }

    /// Transition to Seeding state
    pub async fn seeding(&self) {
        self.transition(BootState::Seeding, "migrations-complete")
            .await;
    }

    /// Transition to WorkerDiscovery state
    pub async fn worker_discovery(&self) {
        self.transition(BootState::WorkerDiscovery, "adapters-loaded")
            .await;
    }

    /// Transition to LoadingPolicies state
    pub async fn load_policies(&self) {
        self.transition(BootState::LoadingPolicies, "seeding-complete")
            .await;
    }

    /// Transition to StartingBackend state
    pub async fn start_backend(&self) {
        self.transition(BootState::StartingBackend, "policies-validated")
            .await;
    }

    /// Transition to LoadingBaseModels state
    pub async fn load_base_models(&self) {
        self.transition(BootState::LoadingBaseModels, "backend-initialized")
            .await;
    }

    /// Transition to LoadingAdapters state
    pub async fn load_adapters(&self) {
        self.transition(BootState::LoadingAdapters, "base-model-loaded")
            .await;
    }

    /// Transition to Ready state
    pub async fn ready(&self) {
        // Clear degraded reasons when transitioning to Ready
        self.degraded_reasons.write().clear();
        self.transition(BootState::Ready, "network-bound").await;
    }

    /// Transition to FullyReady state
    pub async fn fully_ready(&self) {
        self.transition(BootState::FullyReady, "all-models-loaded")
            .await;
    }

    /// Transition to Maintenance state
    pub async fn maintenance(&self, reason: &str) {
        self.transition(BootState::Maintenance, reason).await;
    }

    /// Transition to Draining state
    pub async fn drain(&self) {
        self.transition(BootState::Draining, "shutdown-signal")
            .await;
    }

    /// Transition to Stopping state
    pub async fn stop(&self) {
        self.transition(BootState::Stopping, "drain-complete").await;
    }

    // ============ Failure and Degraded state handling ============

    /// Transition to Failed state with a structured failure reason.
    ///
    /// This is a terminal state - no further transitions are allowed.
    /// Every failure MUST have a reason_code for programmatic handling.
    pub async fn fail(&self, reason: FailureReason) {
        // Store the failure reason
        *self.failure_reason.write() = Some(reason.clone());

        let reason_str = format!("[{}] {}", reason.code, reason.message);

        // Log at error level for failures
        tracing::error!(
            failure_code = %reason.code,
            failure_message = %reason.message,
            component = ?reason.component,
            recoverable = reason.recoverable,
            "Boot failure - transitioning to Failed state"
        );

        self.transition(BootState::Failed, &reason_str).await;
    }

    /// Transition to Degraded state for non-critical dependency failures.
    ///
    /// Only allowed from Ready or FullyReady states. The system remains
    /// operational but with reduced functionality.
    pub async fn degrade(&self, reason: &str) {
        let current = self.current_state();
        if !matches!(current, BootState::Ready | BootState::FullyReady) {
            warn!(
                current_state = %current,
                reason = reason,
                "Cannot transition to Degraded from non-ready state"
            );
            return;
        }

        // Track the degraded reason
        self.degraded_reasons.write().push(DegradedReason {
            component: "unknown".to_string(),
            reason: reason.to_string(),
            detected_at: Instant::now(),
        });

        self.transition(BootState::Degraded, reason).await;
    }

    /// Transition to Degraded state with component information.
    pub async fn degrade_component(&self, component: &str, reason: &str) {
        let current = self.current_state();
        if !matches!(
            current,
            BootState::Ready | BootState::FullyReady | BootState::Degraded
        ) {
            warn!(
                current_state = %current,
                component = component,
                reason = reason,
                "Cannot transition to Degraded from non-ready state"
            );
            return;
        }

        // Track the degraded reason
        self.degraded_reasons.write().push(DegradedReason {
            component: component.to_string(),
            reason: reason.to_string(),
            detected_at: Instant::now(),
        });

        // Only transition if not already degraded
        if !current.is_degraded() {
            let full_reason = format!("{}: {}", component, reason);
            self.transition(BootState::Degraded, &full_reason).await;
        }
    }

    /// Recover from Degraded state back to Ready.
    ///
    /// Clears all degraded reasons.
    pub async fn recover(&self) {
        if self.current_state() != BootState::Degraded {
            return;
        }

        self.degraded_reasons.write().clear();
        self.transition(BootState::Ready, "recovered").await;
    }

    // ============ Failure and Degraded state accessors ============

    /// Get the failure reason if in Failed state
    pub fn get_failure_reason(&self) -> Option<FailureReason> {
        self.failure_reason.read().clone()
    }

    /// Get all degraded reasons
    pub fn get_degraded_reasons(&self) -> Vec<DegradedReason> {
        self.degraded_reasons.read().clone()
    }

    /// Check if server is in degraded state
    pub fn is_degraded(&self) -> bool {
        self.current_state().is_degraded()
    }

    /// Check if server has failed
    pub fn is_failed(&self) -> bool {
        self.current_state().is_failed()
    }
    // ============ Boot warnings ============

    /// Record a warning during boot for a component that failed non-fatally.
    ///
    /// Unlike `degrade()`, this can be called from any state (including boot states).
    /// Warnings are exposed via `/readyz` to give operators visibility into
    /// components that failed to start without lying about the system state.
    pub fn record_boot_warning(&self, component: impl Into<String>, message: impl Into<String>) {
        let warning = BootWarning {
            component: component.into(),
            message: message.into(),
            recorded_at_ms: self.start_time.elapsed().as_millis() as u64,
        };
        self.boot_warnings.write().push(warning);
    }

    /// Get all boot warnings recorded during startup.
    pub fn get_boot_warnings(&self) -> Vec<BootWarning> {
        self.boot_warnings.read().clone()
    }

    /// Check if any boot warnings were recorded.
    pub fn has_boot_warnings(&self) -> bool {
        !self.boot_warnings.read().is_empty()
    }
}

impl Default for BootStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for BootStateManager {
    fn clone(&self) -> Self {
        Self {
            current: Arc::clone(&self.current),
            start_time: self.start_time,
            boot_trace_id: self.boot_trace_id.clone(),
            db: self.db.clone(),
            model_status: Arc::clone(&self.model_status),
            failure_reason: Arc::clone(&self.failure_reason),
            degraded_reasons: Arc::clone(&self.degraded_reasons),
            transitions: Arc::clone(&self.transitions),
            phases: Arc::clone(&self.phases),
            boot_warnings: Arc::clone(&self.boot_warnings),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Helper to boot manager to Ready state using the full new boot sequence
    async fn boot_to_ready(manager: &BootStateManager) {
        manager.start().await;
        manager.db_connecting().await;
        manager.migrating().await;
        manager.seeding().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.worker_discovery().await;
        manager.ready().await;
    }

    #[tokio::test]
    async fn test_state_transitions() {
        let manager = BootStateManager::new();

        // Initial state invariants
        assert_eq!(manager.current_state(), BootState::Stopped);
        assert!(!manager.is_ready());
        assert!(!manager.is_shutting_down());
        assert!(!manager.is_booting());

        // Boot sequence - test invariants, not exact states
        manager.start().await;
        assert!(manager.is_booting(), "After start(), should be booting");

        // Complete full boot sequence
        manager.db_connecting().await;
        manager.migrating().await;
        manager.seeding().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.worker_discovery().await;
        manager.ready().await;

        // Verify ready state invariants
        assert!(manager.is_ready(), "After ready(), should be ready");
        assert!(
            !manager.is_booting(),
            "After ready(), should not be booting"
        );

        // Shutdown sequence - verify monotonic progression
        let before_drain = manager.current_state();
        manager.drain().await;
        assert!(
            manager.is_shutting_down(),
            "After drain(), should be shutting down"
        );
        assert_ne!(
            manager.current_state(),
            before_drain,
            "State should change on drain"
        );

        manager.stop().await;
        assert_eq!(manager.current_state(), BootState::Stopping);
        assert!(
            manager.current_state().is_terminal(),
            "Stopping should be terminal"
        );
    }

    #[tokio::test]
    async fn test_state_string_conversion() {
        // New states
        assert_eq!(BootState::Stopped.as_str(), "stopped");
        assert_eq!(BootState::Starting.as_str(), "starting");
        assert_eq!(BootState::DbConnecting.as_str(), "db-connecting");
        assert_eq!(BootState::Migrating.as_str(), "migrating");
        assert_eq!(BootState::Seeding.as_str(), "seeding");
        assert_eq!(BootState::WorkerDiscovery.as_str(), "worker-discovery");
        assert_eq!(BootState::Ready.as_str(), "ready");
        assert_eq!(BootState::FullyReady.as_str(), "fully-ready");
        assert_eq!(BootState::Degraded.as_str(), "degraded");
        assert_eq!(BootState::Failed.as_str(), "failed");
        assert_eq!(BootState::Maintenance.as_str(), "maintenance");
        assert_eq!(BootState::Draining.as_str(), "draining");
        assert_eq!(BootState::Stopping.as_str(), "stopping");
    }

    #[tokio::test]
    async fn test_elapsed_time() {
        let manager = BootStateManager::new();
        tokio::time::sleep(Duration::from_millis(10)).await;

        let elapsed = manager.elapsed();
        assert!(elapsed >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_progressive_startup() {
        let manager = BootStateManager::new();

        // Boot to Ready state using standard sequence
        boot_to_ready(&manager).await;

        // Verify Ready state invariants
        assert!(manager.is_ready(), "Should be ready after boot");
        assert!(
            manager.is_accepting_requests(),
            "Ready should accept requests"
        );
        assert!(!manager.is_fully_ready(), "Ready is not FullyReady");

        // Transition to FullyReady
        manager.fully_ready().await;
        assert!(manager.is_fully_ready(), "Should be fully ready");
        assert!(
            manager.is_accepting_requests(),
            "FullyReady should accept requests"
        );
        // FullyReady implies is_ready() returns true
        assert!(manager.is_ready(), "FullyReady implies ready");
    }

    #[tokio::test]
    async fn test_final_state_can_be_ready_or_fully_ready() {
        // Test that final boot state can be Ready or FullyReady
        let manager = BootStateManager::new();
        boot_to_ready(&manager).await;

        // Ready is a valid final state
        assert!(
            manager.current_state().is_ready(),
            "Ready should be valid final state"
        );

        // Can optionally progress to FullyReady
        manager.fully_ready().await;
        assert!(
            manager.current_state().is_ready(),
            "FullyReady is also a ready state"
        );
        assert!(
            manager.is_fully_ready(),
            "FullyReady is the fully ready state"
        );
    }

    #[tokio::test]
    async fn test_model_loading_tracking() {
        let manager = BootStateManager::new();

        // Add pending models
        manager.add_pending_model("model-1".to_string());
        manager.add_pending_model("model-2".to_string());
        manager.add_pending_model("model-3".to_string());

        assert_eq!(manager.pending_model_count(), 3);
        assert_eq!(manager.ready_model_count(), 0);

        // Mark model as ready
        manager.mark_model_ready("model-1".to_string());
        assert_eq!(manager.pending_model_count(), 2);
        assert_eq!(manager.ready_model_count(), 1);

        // Mark model as failed
        manager.mark_model_failed("model-2".to_string());
        assert_eq!(manager.pending_model_count(), 1);
        assert_eq!(manager.ready_model_count(), 1);

        // Get status
        let status = manager.get_model_status();
        assert_eq!(status.pending.len(), 1);
        assert_eq!(status.ready.len(), 1);
        assert_eq!(status.failed.len(), 1);
        assert!(status.pending.contains(&"model-3".to_string()));
        assert!(status.ready.contains(&"model-1".to_string()));
        assert!(status.failed.contains(&"model-2".to_string()));
    }

    #[tokio::test]
    async fn test_fully_ready_state_string() {
        assert_eq!(BootState::FullyReady.as_str(), "fully-ready");
        assert_eq!(BootState::FullyReady.to_string(), "fully-ready");
    }

    #[tokio::test]
    async fn test_maintenance_transition() {
        let manager = BootStateManager::new();
        boot_to_ready(&manager).await;
        assert!(manager.is_ready(), "Should be ready after boot");

        // Maintenance flag defaults to false
        assert!(
            !manager.is_maintenance(),
            "Maintenance should default to false"
        );

        manager.maintenance("admin-maintenance").await;
        assert_eq!(manager.current_state(), BootState::Maintenance);
        assert!(manager.is_maintenance(), "Should be in maintenance");
        assert!(!manager.is_ready(), "Maintenance is not ready");

        manager.drain().await;
        assert!(manager.is_draining(), "Should be draining after drain()");
    }

    #[tokio::test]
    async fn test_invalid_transition_is_rejected() {
        let manager = BootStateManager::new();

        // Invariant: Cannot skip ahead from Stopped to Ready
        manager.transition(BootState::Ready, "attempt-skip").await;
        assert_eq!(
            manager.current_state(),
            BootState::Stopped,
            "Skipping ahead should be rejected"
        );

        // Progress to Ready
        boot_to_ready(&manager).await;
        assert!(manager.is_ready(), "Should be ready after boot");

        // Invariant: Cannot go backward to a boot state
        let state_before = manager.current_state();
        manager
            .transition(BootState::LoadingAdapters, "invalid-backward")
            .await;
        assert_eq!(
            manager.current_state(),
            state_before,
            "Backward transition should be rejected"
        );
    }

    #[tokio::test]
    async fn attach_db_preserves_state_and_transitions_ordered() {
        let manager = BootStateManager::new();

        manager.start().await;
        manager.db_connecting().await;

        let elapsed_before = manager.elapsed();

        std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
        let db = Arc::new(adapteros_db::Db::new_in_memory().await.unwrap());
        let attached = manager.attach_db(Arc::clone(&db));

        // Invariant: elapsed time is monotonic
        let elapsed_after = attached.elapsed();
        assert!(
            elapsed_after >= elapsed_before,
            "Elapsed time should be monotonic after attach_db"
        );

        // Continue boot sequence on attached manager
        attached.migrating().await;
        attached.seeding().await;
        attached.load_policies().await;
        attached.start_backend().await;
        attached.load_base_models().await;
        attached.load_adapters().await;
        attached.worker_discovery().await;
        attached.ready().await;

        // Invariant: Both managers share state
        assert!(attached.is_ready(), "Attached manager should be ready");
        assert!(
            manager.is_ready(),
            "Original manager should also be ready (shared state)"
        );
        assert_eq!(
            attached.current_state(),
            manager.current_state(),
            "Both managers should share same state"
        );
    }

    #[tokio::test]
    async fn test_maintenance_mode_transitions() {
        let manager = BootStateManager::new();

        // Boot to FullyReady state
        boot_to_ready(&manager).await;
        manager.fully_ready().await;

        // Invariants for FullyReady
        assert!(manager.is_fully_ready(), "Should be fully ready");
        assert!(manager.is_ready(), "FullyReady implies ready");
        assert!(!manager.is_maintenance(), "Maintenance defaults to false");

        // Transition from FullyReady to Maintenance
        manager.maintenance("scheduled-upgrade").await;
        assert!(manager.is_maintenance(), "Should be in maintenance");
        assert!(!manager.is_ready(), "Maintenance is not ready");
        assert!(!manager.is_fully_ready(), "Maintenance is not fully ready");

        // Transition from Maintenance back to Ready
        manager
            .transition(BootState::Ready, "maintenance-complete")
            .await;
        assert!(manager.is_ready(), "Should be ready after maintenance exit");
        assert!(!manager.is_maintenance(), "Should not be in maintenance");
        assert!(!manager.is_fully_ready(), "Ready is not FullyReady");

        // Invariant: Maintenance -> FullyReady is NOT allowed (must go through Ready)
        manager.maintenance("second-maintenance-window").await;
        assert!(manager.is_maintenance(), "Should be in maintenance again");

        // Attempt direct transition to FullyReady (should be rejected)
        let state_before = manager.current_state();
        manager
            .transition(BootState::FullyReady, "invalid-direct-fully-ready")
            .await;
        assert_eq!(
            manager.current_state(),
            state_before,
            "Maintenance -> FullyReady should be rejected"
        );

        // Verify proper path: Maintenance -> Ready -> FullyReady
        manager
            .transition(BootState::Ready, "exit-maintenance")
            .await;
        assert!(manager.is_ready(), "Should be ready");

        manager.fully_ready().await;
        assert!(manager.is_fully_ready(), "Should be fully ready");
    }

    #[tokio::test]
    async fn test_is_maintenance_helper() {
        // Invariant: Only Maintenance state returns true for is_maintenance()
        assert!(BootState::Maintenance.is_maintenance());

        // All other states should return false
        let non_maintenance_states = [
            BootState::Stopped,
            BootState::Starting,
            BootState::DbConnecting,
            BootState::Migrating,
            BootState::Seeding,
            BootState::LoadingPolicies,
            BootState::StartingBackend,
            BootState::LoadingBaseModels,
            BootState::LoadingAdapters,
            BootState::WorkerDiscovery,
            BootState::Ready,
            BootState::FullyReady,
            BootState::Degraded,
            BootState::Failed,
            BootState::Draining,
            BootState::Stopping,
        ];
        for state in non_maintenance_states {
            assert!(
                !state.is_maintenance(),
                "{:?} should not be maintenance",
                state
            );
        }

        // Test manager method defaults correctly
        let manager = BootStateManager::new();
        assert!(
            !manager.is_maintenance(),
            "Maintenance should default to false"
        );

        // Boot to Ready and enter maintenance
        boot_to_ready(&manager).await;
        assert!(!manager.is_maintenance(), "Ready should not be maintenance");

        manager.maintenance("testing-is-maintenance").await;
        assert!(manager.is_maintenance(), "Should be in maintenance");

        // Exit maintenance
        manager
            .transition(BootState::Ready, "maintenance-done")
            .await;
        assert!(!manager.is_maintenance(), "Should exit maintenance");
    }

    #[tokio::test]
    async fn test_terminal_state_behavior() {
        // Stopping is terminal
        assert!(BootState::Stopping.is_terminal());

        // Stopped is NOT terminal (allows rebooting)
        assert!(!BootState::Stopped.is_terminal());

        // Other states are not terminal
        assert!(!BootState::Starting.is_terminal());
        assert!(!BootState::Ready.is_terminal());
        assert!(!BootState::Draining.is_terminal());
        assert!(!BootState::Maintenance.is_terminal());
    }

    #[tokio::test]
    async fn test_terminal_state_prevents_all_transitions() {
        let manager = BootStateManager::new();

        // Progress to Stopping (terminal) state
        boot_to_ready(&manager).await;
        manager.drain().await;
        manager.stop().await;

        assert_eq!(manager.current_state(), BootState::Stopping);
        assert!(
            manager.current_state().is_terminal(),
            "Stopping should be terminal"
        );

        // Invariant: No transitions allowed from terminal state
        let target_states = [
            BootState::Stopped,
            BootState::Starting,
            BootState::DbConnecting,
            BootState::Migrating,
            BootState::Seeding,
            BootState::LoadingPolicies,
            BootState::StartingBackend,
            BootState::LoadingBaseModels,
            BootState::LoadingAdapters,
            BootState::WorkerDiscovery,
            BootState::Ready,
            BootState::FullyReady,
            BootState::Degraded,
            BootState::Maintenance,
            BootState::Draining,
        ];

        for target in target_states {
            manager.transition(target, "invalid").await;
            assert_eq!(
                manager.current_state(),
                BootState::Stopping,
                "Transition to {:?} should be rejected from terminal state",
                target
            );
        }
    }

    #[tokio::test]
    async fn test_stopped_allows_reboot() {
        let manager = BootStateManager::new();

        // Initial state is Stopped
        assert_eq!(manager.current_state(), BootState::Stopped);
        assert!(
            !manager.current_state().is_terminal(),
            "Stopped is not terminal"
        );

        // Stopped should allow transition to a booting state
        manager.start().await;
        assert!(
            manager.is_booting(),
            "After start(), should be in a booting state"
        );
        assert_ne!(
            manager.current_state(),
            BootState::Stopped,
            "Should have left Stopped state"
        );
    }

    #[tokio::test]
    async fn test_is_allowed_transition_respects_terminal_states() {
        // Verify that is_allowed_transition explicitly prevents transitions from terminal states

        // Stopping is terminal - no transitions allowed
        assert!(!BootStateManager::is_allowed_transition(
            BootState::Stopping,
            BootState::Stopped
        ));
        assert!(!BootStateManager::is_allowed_transition(
            BootState::Stopping,
            BootState::Starting
        ));
        assert!(!BootStateManager::is_allowed_transition(
            BootState::Stopping,
            BootState::Ready
        ));
        assert!(!BootStateManager::is_allowed_transition(
            BootState::Stopping,
            BootState::Draining
        ));

        // Stopped is NOT terminal - should allow Starting
        assert!(BootStateManager::is_allowed_transition(
            BootState::Stopped,
            BootState::Starting
        ));

        // Other transitions should work as expected
        assert!(BootStateManager::is_allowed_transition(
            BootState::Draining,
            BootState::Stopping
        ));
        assert!(BootStateManager::is_allowed_transition(
            BootState::Ready,
            BootState::Draining
        ));
    }

    #[tokio::test]
    async fn test_concurrent_state_transitions() {
        // Create a manager and progress to LoadingAdapters state
        let manager = Arc::new(BootStateManager::new());
        manager.start().await;
        manager.db_connecting().await;
        manager.migrating().await;
        manager.seeding().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;

        // At this point we should be in LoadingAdapters
        assert_eq!(manager.current_state(), BootState::LoadingAdapters);

        // Spawn concurrent tasks to transition through WorkerDiscovery to Ready
        let mut handles = vec![];
        for _ in 0..5 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.worker_discovery().await;
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Now transition to Ready
        manager.ready().await;

        // Invariant: Should be in a ready state
        assert!(
            manager.is_ready(),
            "Should be ready after concurrent transitions"
        );

        // Invariant: Concurrent invalid transitions should all be rejected
        let state_before = manager.current_state();
        let mut invalid_handles = vec![];
        for i in 0..10 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                // These should all be rejected (can't go back to Starting from Ready)
                m.transition(BootState::Starting, &format!("invalid-{}", i))
                    .await;
            });
            invalid_handles.push(handle);
        }

        for handle in invalid_handles {
            handle.await.unwrap();
        }

        // Invariant: State should remain unchanged after invalid concurrent transitions
        assert_eq!(
            manager.current_state(),
            state_before,
            "Invalid concurrent transitions should be rejected"
        );

        // Test concurrent transitions to valid next states
        let mut mixed_handles = vec![];

        // Spawn concurrent tasks attempting to transition to FullyReady, Maintenance, and Draining
        for i in 0..5 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::FullyReady, &format!("fully-ready-{}", i))
                    .await;
            });
            mixed_handles.push(handle);
        }

        for i in 0..5 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::Maintenance, &format!("maintenance-{}", i))
                    .await;
            });
            mixed_handles.push(handle);
        }

        for i in 0..5 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::Draining, &format!("draining-{}", i))
                    .await;
            });
            mixed_handles.push(handle);
        }

        for handle in mixed_handles {
            handle.await.unwrap();
        }

        // Invariant: Final state must be one of the valid target states
        let final_state = manager.current_state();
        assert!(
            matches!(
                final_state,
                BootState::FullyReady
                    | BootState::Maintenance
                    | BootState::Draining
                    | BootState::Stopping
            ),
            "Expected valid final state after concurrent transitions, got {:?}",
            final_state
        );
    }

    #[tokio::test]
    async fn test_concurrent_model_tracking() {
        let manager = Arc::new(BootStateManager::new());

        // Spawn 20 tasks concurrently adding models
        let mut add_handles = vec![];
        for i in 0..20 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.add_pending_model(format!("model-{}", i));
            });
            add_handles.push(handle);
        }

        for handle in add_handles {
            handle.await.unwrap();
        }

        // Verify all 20 models were added
        assert_eq!(manager.pending_model_count(), 20);

        // Concurrently mark half as ready, half as failed
        let mut update_handles = vec![];
        for i in 0..10 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.mark_model_ready(format!("model-{}", i));
            });
            update_handles.push(handle);
        }

        for i in 10..20 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.mark_model_failed(format!("model-{}", i));
            });
            update_handles.push(handle);
        }

        for handle in update_handles {
            handle.await.unwrap();
        }

        // Verify final counts
        assert_eq!(manager.pending_model_count(), 0);
        assert_eq!(manager.ready_model_count(), 10);

        let status = manager.get_model_status();
        assert_eq!(status.ready.len(), 10);
        assert_eq!(status.failed.len(), 10);
        assert_eq!(status.pending.len(), 0);
    }

    #[tokio::test]
    async fn test_concurrent_shutdown_sequence() {
        let manager = Arc::new(BootStateManager::new());

        // Boot to Ready state
        boot_to_ready(&manager).await;
        assert!(manager.is_ready(), "Should be ready after boot");

        // Spawn multiple tasks attempting to drain concurrently
        let mut drain_handles = vec![];
        for i in 0..10 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::Draining, &format!("shutdown-{}", i))
                    .await;
            });
            drain_handles.push(handle);
        }

        for handle in drain_handles {
            handle.await.unwrap();
        }

        // Invariant: Should have transitioned to Draining
        assert!(
            manager.is_draining(),
            "Should be draining after concurrent drain calls"
        );
        assert!(manager.is_shutting_down(), "Should be shutting down");

        // Now spawn concurrent tasks attempting to stop
        let mut stop_handles = vec![];
        for i in 0..10 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::Stopping, &format!("stop-{}", i))
                    .await;
            });
            stop_handles.push(handle);
        }

        for handle in stop_handles {
            handle.await.unwrap();
        }

        // Invariant: Should have transitioned to Stopping (terminal)
        assert_eq!(manager.current_state(), BootState::Stopping);
        assert!(manager.is_shutting_down(), "Should still be shutting down");
        assert!(
            manager.current_state().is_terminal(),
            "Stopping is terminal"
        );
    }

    // ============ New state tests ============

    #[tokio::test]
    async fn test_new_boot_sequence() {
        let manager = BootStateManager::new();

        // New boot sequence with granular states
        manager.start().await;
        assert_eq!(manager.current_state(), BootState::Starting);
        assert!(manager.is_booting());

        manager.db_connecting().await;
        assert_eq!(manager.current_state(), BootState::DbConnecting);

        manager.migrating().await;
        assert_eq!(manager.current_state(), BootState::Migrating);

        manager.seeding().await;
        assert_eq!(manager.current_state(), BootState::Seeding);

        manager.load_policies().await;
        assert_eq!(manager.current_state(), BootState::LoadingPolicies);

        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;

        manager.worker_discovery().await;
        assert_eq!(manager.current_state(), BootState::WorkerDiscovery);

        manager.ready().await;
        assert_eq!(manager.current_state(), BootState::Ready);
        assert!(manager.is_ready());
        assert!(!manager.is_booting());
    }

    #[tokio::test]
    async fn test_failed_state_transition() {
        let manager = BootStateManager::new();

        // Boot to a mid-boot state
        manager.start().await;
        manager.db_connecting().await;

        // Transition to Failed with reason
        let failure = FailureReason::new(
            failure_codes::DB_CONN_TIMEOUT,
            "Connection to database timed out after 30s",
        );
        manager.fail(failure).await;

        assert_eq!(manager.current_state(), BootState::Failed);
        assert!(manager.is_failed());
        assert!(manager.current_state().is_terminal());

        // Verify failure reason was stored
        let stored_reason = manager.get_failure_reason();
        assert!(stored_reason.is_some());
        let reason = stored_reason.unwrap();
        assert_eq!(reason.code, failure_codes::DB_CONN_TIMEOUT);
        assert!(reason.message.contains("timed out"));
    }

    #[tokio::test]
    async fn test_failed_state_is_terminal() {
        let manager = BootStateManager::new();

        manager.start().await;
        manager
            .fail(FailureReason::new(
                failure_codes::CONFIG_ERROR,
                "Invalid configuration",
            ))
            .await;

        assert_eq!(manager.current_state(), BootState::Failed);

        // No transitions allowed from Failed state
        manager.transition(BootState::Ready, "attempt").await;
        assert_eq!(manager.current_state(), BootState::Failed);

        manager.transition(BootState::Stopped, "attempt").await;
        assert_eq!(manager.current_state(), BootState::Failed);

        manager.transition(BootState::Starting, "attempt").await;
        assert_eq!(manager.current_state(), BootState::Failed);
    }

    #[tokio::test]
    async fn test_failed_from_any_non_terminal_state() {
        // Test that Failed can be reached from various states
        for initial_state in [
            BootState::Starting,
            BootState::DbConnecting,
            BootState::Migrating,
            BootState::LoadingPolicies,
            BootState::Ready,
            BootState::FullyReady,
            BootState::Draining,
        ] {
            let manager = BootStateManager::new();

            // Set the state directly via transition (using Stopped → Starting path first)
            manager.start().await;

            // Allow transition to Failed from any non-terminal state
            assert!(
                BootStateManager::is_allowed_transition(initial_state, BootState::Failed),
                "Should allow transition from {:?} to Failed",
                initial_state
            );
        }
    }

    #[tokio::test]
    async fn test_degraded_state_from_ready() {
        let manager = BootStateManager::new();

        // Boot to Ready state
        manager.start().await;
        manager.db_connecting().await;
        manager.migrating().await;
        manager.seeding().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.worker_discovery().await;
        manager.ready().await;

        assert_eq!(manager.current_state(), BootState::Ready);

        // Transition to Degraded
        manager.degrade("metrics-unavailable").await;
        assert_eq!(manager.current_state(), BootState::Degraded);
        assert!(manager.is_degraded());

        // Verify degraded reasons were tracked
        let reasons = manager.get_degraded_reasons();
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].reason.contains("metrics"));
    }

    #[tokio::test]
    async fn test_degraded_only_from_ready_states() {
        let manager = BootStateManager::new();

        // Start booting
        manager.start().await;
        manager.db_connecting().await;

        // Degraded should NOT be allowed from booting states
        manager.degrade("test").await;
        assert_eq!(
            manager.current_state(),
            BootState::DbConnecting,
            "Degraded transition should be rejected from booting state"
        );

        // Degraded only allowed from Ready/FullyReady
        assert!(BootStateManager::is_allowed_transition(
            BootState::Ready,
            BootState::Degraded
        ));
        assert!(BootStateManager::is_allowed_transition(
            BootState::FullyReady,
            BootState::Degraded
        ));
        assert!(!BootStateManager::is_allowed_transition(
            BootState::Starting,
            BootState::Degraded
        ));
        assert!(!BootStateManager::is_allowed_transition(
            BootState::Migrating,
            BootState::Degraded
        ));
    }

    #[tokio::test]
    async fn test_degraded_recovery() {
        let manager = BootStateManager::new();

        // Boot to Ready then Degraded
        manager.start().await;
        manager.db_connecting().await;
        manager.migrating().await;
        manager.seeding().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.worker_discovery().await;
        manager.ready().await;
        manager.degrade("telemetry-down").await;

        assert_eq!(manager.current_state(), BootState::Degraded);
        assert_eq!(manager.get_degraded_reasons().len(), 1);

        // Recover
        manager.recover().await;
        assert_eq!(manager.current_state(), BootState::Ready);
        assert!(manager.is_ready());
        assert!(!manager.is_degraded());

        // Degraded reasons should be cleared
        assert!(manager.get_degraded_reasons().is_empty());
    }

    #[tokio::test]
    async fn test_degraded_component_tracking() {
        let manager = BootStateManager::new();

        // Boot to Ready
        manager.start().await;
        manager.db_connecting().await;
        manager.migrating().await;
        manager.seeding().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.worker_discovery().await;
        manager.ready().await;

        // Add multiple degraded components
        manager
            .degrade_component("telemetry", "exporter unavailable")
            .await;
        manager
            .degrade_component("metrics", "prometheus unreachable")
            .await;

        // Should still be in Degraded state (not transition again)
        assert_eq!(manager.current_state(), BootState::Degraded);

        // Should have tracked both degraded reasons
        let reasons = manager.get_degraded_reasons();
        assert_eq!(reasons.len(), 2);
        assert!(reasons.iter().any(|r| r.component == "telemetry"));
        assert!(reasons.iter().any(|r| r.component == "metrics"));
    }

    #[tokio::test]
    async fn test_degraded_allows_draining() {
        let manager = BootStateManager::new();

        // Boot to Degraded
        manager.start().await;
        manager.db_connecting().await;
        manager.migrating().await;
        manager.seeding().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.worker_discovery().await;
        manager.ready().await;
        manager.degrade("test").await;

        assert_eq!(manager.current_state(), BootState::Degraded);

        // Draining should be allowed from Degraded
        manager.drain().await;
        assert_eq!(manager.current_state(), BootState::Draining);
    }

    #[tokio::test]
    async fn test_failure_reason_with_component() {
        let reason = FailureReason::with_component(
            failure_codes::MIGRATION_FAILED,
            "Column 'tenant_id' already exists",
            "database",
        );

        assert_eq!(reason.code, failure_codes::MIGRATION_FAILED);
        assert_eq!(reason.component, Some("database".to_string()));
        assert!(!reason.recoverable);

        // Test display
        let display = format!("{}", reason);
        assert!(display.contains("MIGRATION_FAILED"));
        assert!(display.contains("database"));
    }

    #[tokio::test]
    async fn test_failure_reason_recoverable() {
        let reason = FailureReason::new(failure_codes::DB_CONN_TIMEOUT, "Timeout").recoverable();

        assert!(reason.recoverable);
    }
}
