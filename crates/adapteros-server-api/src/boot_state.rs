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
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Boot lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BootState {
    /// Server not running (initial state)
    Stopped,
    /// Initial process startup (PID lock, config load)
    Starting,
    /// Establishing database connection
    DbConnecting,
    /// Running database migrations
    Migrating,
    /// Seeding initial data (dev fixtures, model cache)
    Seeding,
    /// Policy verification (hash watcher, baseline load)
    LoadingPolicies,
    /// Backend initialization (MLX/CoreML/Metal)
    StartingBackend,
    /// Base model loading (manifest validation, executor seeding)
    LoadingBaseModels,
    /// Adapter warmup (lifecycle manager, heartbeat recovery)
    LoadingAdapters,
    /// Discovering and registering worker processes
    WorkerDiscovery,
    /// Accepting requests (HTTP/UDS), models may still be loading
    Ready,
    /// All priority models loaded and health-checked
    FullyReady,
    /// Non-critical dependency failure (metrics, telemetry, etc.)
    /// Can recover to Ready. Only allowed from Ready/FullyReady states.
    Degraded,
    /// Critical failure - terminal state with reason_code
    Failed,
    /// Maintenance mode (no new work, in-flight continues)
    Maintenance,
    /// Shutdown initiated (reject new requests, track in-flight)
    Draining,
    /// Component shutdown (ordered termination)
    Stopping,

    // Legacy aliases for backwards compatibility during migration
    #[doc(hidden)]
    Booting,
    #[doc(hidden)]
    InitializingDb,
}

impl BootState {
    /// Returns true if this state indicates the server is accepting requests
    pub fn is_ready(&self) -> bool {
        matches!(self, BootState::Ready | BootState::FullyReady)
    }

    /// Returns true if all models are loaded and healthy
    pub fn is_fully_ready(&self) -> bool {
        matches!(self, BootState::FullyReady)
    }

    /// Returns true if server is in maintenance
    pub fn is_maintenance(&self) -> bool {
        matches!(self, BootState::Maintenance)
    }

    /// Returns true if server is in degraded state
    pub fn is_degraded(&self) -> bool {
        matches!(self, BootState::Degraded)
    }

    /// Returns true if server has failed
    pub fn is_failed(&self) -> bool {
        matches!(self, BootState::Failed)
    }

    /// Returns true if server is draining or stopping
    pub fn is_draining(&self) -> bool {
        matches!(self, BootState::Draining | BootState::Stopping)
    }

    /// Returns true if this state indicates the server is shutting down
    pub fn is_shutting_down(&self) -> bool {
        matches!(self, BootState::Draining | BootState::Stopping)
    }

    /// Returns true if this state indicates the server is booting
    pub fn is_booting(&self) -> bool {
        matches!(
            self,
            BootState::Starting
                | BootState::DbConnecting
                | BootState::Migrating
                | BootState::Seeding
                | BootState::LoadingPolicies
                | BootState::StartingBackend
                | BootState::LoadingBaseModels
                | BootState::LoadingAdapters
                | BootState::WorkerDiscovery
                // Legacy aliases
                | BootState::Booting
                | BootState::InitializingDb
        )
    }

    /// Returns true if this state is terminal (no transitions allowed from it)
    ///
    /// Terminal states represent final states in the lifecycle that cannot
    /// transition to any other state. `Stopping` and `Failed` are terminal.
    /// `Stopped` is not terminal as it allows rebooting (Stopped → Starting).
    pub fn is_terminal(&self) -> bool {
        matches!(self, BootState::Stopping | BootState::Failed)
    }

    /// Convert state to string for logging/telemetry
    pub fn as_str(&self) -> &'static str {
        match self {
            BootState::Stopped => "stopped",
            BootState::Starting => "starting",
            BootState::DbConnecting => "db-connecting",
            BootState::Migrating => "migrating",
            BootState::Seeding => "seeding",
            BootState::LoadingPolicies => "loading-policies",
            BootState::StartingBackend => "starting-backend",
            BootState::LoadingBaseModels => "loading-base-models",
            BootState::LoadingAdapters => "loading-adapters",
            BootState::WorkerDiscovery => "worker-discovery",
            BootState::Ready => "ready",
            BootState::FullyReady => "fully-ready",
            BootState::Degraded => "degraded",
            BootState::Failed => "failed",
            BootState::Maintenance => "maintenance",
            BootState::Draining => "draining",
            BootState::Stopping => "stopping",
            // Legacy aliases map to new names
            BootState::Booting => "starting",
            BootState::InitializingDb => "db-connecting",
        }
    }
}

impl std::fmt::Display for BootState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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
#[derive(Debug, Clone, Default)]
pub struct ModelLoadingStatus {
    /// Models still being loaded
    pub pending: Vec<String>,
    /// Models successfully loaded
    pub ready: Vec<String>,
    /// Models that failed to load
    pub failed: Vec<String>,
}

/// Manager for boot lifecycle state
pub struct BootStateManager {
    /// Current state
    current: Arc<RwLock<BootState>>,
    /// Process start time
    start_time: Instant,
    /// Database for audit logging (optional)
    db: Option<Arc<Db>>,
    /// Model loading status
    model_status: Arc<RwLock<ModelLoadingStatus>>,
    /// Failure reason (set when transitioning to Failed state)
    failure_reason: Arc<RwLock<Option<FailureReason>>>,
    /// Degraded reasons (components that have failed non-critically)
    degraded_reasons: Arc<RwLock<Vec<DegradedReason>>>,
}

impl BootStateManager {
    /// Create a new boot state manager
    pub fn new() -> Self {
        Self {
            current: Arc::new(RwLock::new(BootState::Stopped)),
            start_time: Instant::now(),
            db: None,
            model_status: Arc::new(RwLock::new(ModelLoadingStatus::default())),
            failure_reason: Arc::new(RwLock::new(None)),
            degraded_reasons: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Attach a database handle without resetting state or counters.
    /// Returns a new manager sharing the same state/time/model status.
    pub fn with_db(&self, db: Arc<Db>) -> Self {
        Self {
            current: Arc::clone(&self.current),
            start_time: self.start_time,
            db: Some(db),
            model_status: Arc::clone(&self.model_status),
            failure_reason: Arc::clone(&self.failure_reason),
            degraded_reasons: Arc::clone(&self.degraded_reasons),
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

    /// Check if server is accepting requests (Ready or FullyReady)
    pub fn is_accepting_requests(&self) -> bool {
        self.current_state().is_ready()
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
        if !status.pending.contains(&model_id) {
            status.pending.push(model_id);
        }
    }

    /// Mark a model as ready
    pub fn mark_model_ready(&self, model_id: String) {
        let mut status = self.model_status.write();
        status.pending.retain(|id| id != &model_id);
        if !status.ready.contains(&model_id) {
            status.ready.push(model_id);
        }
    }

    /// Mark a model as failed
    pub fn mark_model_failed(&self, model_id: String) {
        let mut status = self.model_status.write();
        status.pending.retain(|id| id != &model_id);
        if !status.failed.contains(&model_id) {
            status.failed.push(model_id);
        }
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

        // Standard boot sequence transitions (new granular states)
        // Also support legacy aliases for backwards compatibility
        matches!(
            (from, to),
            // New state flow: stopped → starting → db-connecting → migrating → seeding
            (BootState::Stopped, BootState::Starting)
                | (BootState::Starting, BootState::DbConnecting)
                | (BootState::DbConnecting, BootState::Migrating)
                | (BootState::Migrating, BootState::Seeding)
                | (BootState::Seeding, BootState::LoadingPolicies)
                // Continue from seeding through the rest of boot
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
                // Legacy aliases for backwards compatibility
                | (BootState::Stopped, BootState::Booting)
                | (BootState::Booting, BootState::StartingBackend)
                | (BootState::Booting, BootState::InitializingDb)
                | (BootState::Booting, BootState::DbConnecting)
                | (BootState::InitializingDb, BootState::LoadingPolicies)
                | (BootState::InitializingDb, BootState::Migrating)
                | (BootState::LoadingPolicies, BootState::LoadingAdapters)
                | (BootState::LoadingBaseModels, BootState::InitializingDb)
                // Allow skipping WorkerDiscovery for backwards compatibility
                | (BootState::LoadingAdapters, BootState::Ready)
        )
    }

    // ============ New granular state transitions ============

    /// Transition to Starting state (new primary method)
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

    // ============ Legacy aliases for backwards compatibility ============

    /// Transition to Booting state (legacy alias for start())
    #[deprecated(since = "0.1.0", note = "Use start() instead")]
    pub async fn boot(&self) {
        self.transition(BootState::Booting, "process-start").await;
    }

    /// Transition to InitializingDb state (legacy alias for db_connecting())
    #[deprecated(since = "0.1.0", note = "Use db_connecting() instead")]
    pub async fn init_db(&self) {
        self.transition(BootState::InitializingDb, "config-loaded")
            .await;
    }

    // ============ Standard state transitions ============

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
            db: self.db.clone(),
            model_status: Arc::clone(&self.model_status),
            failure_reason: Arc::clone(&self.failure_reason),
            degraded_reasons: Arc::clone(&self.degraded_reasons),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_state_transitions() {
        let manager = BootStateManager::new();

        // Initial state
        assert_eq!(manager.current_state(), BootState::Stopped);
        assert!(!manager.is_ready());
        assert!(!manager.is_shutting_down());
        assert!(!manager.is_booting());

        // Boot sequence
        manager.boot().await;
        assert_eq!(manager.current_state(), BootState::Booting);
        assert!(manager.is_booting());

        manager.init_db().await;
        assert_eq!(manager.current_state(), BootState::InitializingDb);

        manager.load_policies().await;
        assert_eq!(manager.current_state(), BootState::LoadingPolicies);

        manager.start_backend().await;
        assert_eq!(manager.current_state(), BootState::StartingBackend);

        manager.load_base_models().await;
        assert_eq!(manager.current_state(), BootState::LoadingBaseModels);

        manager.load_adapters().await;
        assert_eq!(manager.current_state(), BootState::LoadingAdapters);

        manager.ready().await;
        assert_eq!(manager.current_state(), BootState::Ready);
        assert!(manager.is_ready());
        assert!(!manager.is_booting());

        // Shutdown sequence
        manager.drain().await;
        assert_eq!(manager.current_state(), BootState::Draining);
        assert!(manager.is_shutting_down());

        manager.stop().await;
        assert_eq!(manager.current_state(), BootState::Stopping);
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

        // Legacy aliases map to new names
        #[allow(deprecated)]
        {
            assert_eq!(BootState::Booting.as_str(), "starting");
            assert_eq!(BootState::InitializingDb.as_str(), "db-connecting");
        }
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

        // Boot to Ready state
        manager.boot().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.ready().await;

        assert_eq!(manager.current_state(), BootState::Ready);
        assert!(manager.is_ready());
        assert!(manager.is_accepting_requests());
        assert!(!manager.is_fully_ready());

        // Transition to FullyReady
        manager.fully_ready().await;
        assert_eq!(manager.current_state(), BootState::FullyReady);
        assert!(manager.is_fully_ready());
        assert!(manager.is_accepting_requests());
    }

    #[tokio::test]
    async fn test_runtime_boot_order() {
        let manager = BootStateManager::new();

        // This mirrors the startup order used by adapteros-server today.
        manager.boot().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.load_adapters().await;
        manager.ready().await;

        assert_eq!(manager.current_state(), BootState::Ready);
        assert!(manager.is_ready());
        assert!(!manager.is_booting());
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
        manager.boot().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.ready().await;
        assert!(manager.is_ready());

        manager.maintenance("admin-maintenance").await;
        assert_eq!(manager.current_state(), BootState::Maintenance);
        assert!(manager.is_maintenance());
        assert!(!manager.is_ready());

        manager.drain().await;
        assert_eq!(manager.current_state(), BootState::Draining);
        assert!(manager.is_draining());
    }

    #[tokio::test]
    async fn test_invalid_transition_is_rejected() {
        let manager = BootStateManager::new();

        // Attempt to skip ahead should be ignored
        manager.transition(BootState::Ready, "attempt-skip").await;
        assert_eq!(manager.current_state(), BootState::Stopped);

        // Progress to Ready, then attempt backward invalid transition
        manager.boot().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.ready().await;
        assert_eq!(manager.current_state(), BootState::Ready);

        // Draining is allowed from Ready, but LoadingAdapters is not
        manager
            .transition(BootState::LoadingAdapters, "invalid-backward")
            .await;
        assert_eq!(manager.current_state(), BootState::Ready);
    }

    #[tokio::test]
    async fn attach_db_preserves_state_and_transitions_ordered() {
        let manager = BootStateManager::new();

        manager.boot().await;
        manager.init_db().await;

        let elapsed_before = manager.elapsed();

        std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
        let db = Arc::new(adapteros_db::Db::new_in_memory().await.unwrap());
        let attached = manager.attach_db(Arc::clone(&db));

        let elapsed_after = attached.elapsed();
        assert!(elapsed_after >= elapsed_before);

        attached.load_policies().await;
        attached.start_backend().await;
        attached.load_base_models().await;
        attached.load_adapters().await;
        attached.ready().await;

        assert_eq!(attached.current_state(), BootState::Ready);
        assert_eq!(manager.current_state(), BootState::Ready);
    }

    #[tokio::test]
    async fn test_maintenance_mode_transitions() {
        let manager = BootStateManager::new();

        // Boot to FullyReady state
        manager.boot().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.ready().await;
        manager.fully_ready().await;

        assert_eq!(manager.current_state(), BootState::FullyReady);
        assert!(manager.is_fully_ready());
        assert!(manager.is_ready());
        assert!(!manager.is_maintenance());

        // Transition from FullyReady to Maintenance
        manager.maintenance("scheduled-upgrade").await;
        assert_eq!(manager.current_state(), BootState::Maintenance);
        assert!(manager.is_maintenance());
        assert!(!manager.is_ready());
        assert!(!manager.is_fully_ready());

        // Transition from Maintenance back to Ready
        manager
            .transition(BootState::Ready, "maintenance-complete")
            .await;
        assert_eq!(manager.current_state(), BootState::Ready);
        assert!(manager.is_ready());
        assert!(!manager.is_maintenance());
        assert!(!manager.is_fully_ready());

        // Verify Maintenance → FullyReady is NOT allowed
        manager.maintenance("second-maintenance-window").await;
        assert_eq!(manager.current_state(), BootState::Maintenance);

        // Attempt direct transition to FullyReady (should be rejected)
        manager
            .transition(BootState::FullyReady, "invalid-direct-fully-ready")
            .await;
        assert_eq!(
            manager.current_state(),
            BootState::Maintenance,
            "Maintenance → FullyReady should be rejected"
        );

        // Verify proper path: Maintenance → Ready → FullyReady
        manager
            .transition(BootState::Ready, "exit-maintenance")
            .await;
        assert_eq!(manager.current_state(), BootState::Ready);

        manager.fully_ready().await;
        assert_eq!(manager.current_state(), BootState::FullyReady);
        assert!(manager.is_fully_ready());
    }

    #[tokio::test]
    async fn test_is_maintenance_helper() {
        // Test helper returns correct values across different states
        assert!(!BootState::Stopped.is_maintenance());
        assert!(!BootState::Booting.is_maintenance());
        assert!(!BootState::InitializingDb.is_maintenance());
        assert!(!BootState::LoadingPolicies.is_maintenance());
        assert!(!BootState::StartingBackend.is_maintenance());
        assert!(!BootState::LoadingBaseModels.is_maintenance());
        assert!(!BootState::LoadingAdapters.is_maintenance());
        assert!(!BootState::Ready.is_maintenance());
        assert!(!BootState::FullyReady.is_maintenance());
        assert!(BootState::Maintenance.is_maintenance());
        assert!(!BootState::Draining.is_maintenance());
        assert!(!BootState::Stopping.is_maintenance());

        // Test manager method
        let manager = BootStateManager::new();
        assert!(!manager.is_maintenance());

        // Boot to Ready and enter maintenance
        manager.boot().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.load_adapters().await;
        manager.ready().await;
        assert!(!manager.is_maintenance());

        manager.maintenance("testing-is-maintenance").await;
        assert!(manager.is_maintenance());

        // Exit maintenance
        manager
            .transition(BootState::Ready, "maintenance-done")
            .await;
        assert!(!manager.is_maintenance());
    }

    #[tokio::test]
    async fn test_terminal_state_behavior() {
        // Stopping is terminal
        assert!(BootState::Stopping.is_terminal());

        // Stopped is NOT terminal (allows rebooting)
        assert!(!BootState::Stopped.is_terminal());

        // Other states are not terminal
        assert!(!BootState::Booting.is_terminal());
        assert!(!BootState::Ready.is_terminal());
        assert!(!BootState::Draining.is_terminal());
        assert!(!BootState::Maintenance.is_terminal());
    }

    #[tokio::test]
    async fn test_terminal_state_prevents_all_transitions() {
        let manager = BootStateManager::new();

        // Progress to Stopping state
        manager.boot().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.ready().await;
        manager.drain().await;
        manager.stop().await;

        assert_eq!(manager.current_state(), BootState::Stopping);
        assert!(manager.current_state().is_terminal());

        // Attempt transition to Stopped should be rejected
        manager.transition(BootState::Stopped, "invalid").await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to Booting should be rejected
        manager.transition(BootState::Booting, "invalid").await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to Draining should be rejected
        manager.transition(BootState::Draining, "invalid").await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to Ready should be rejected
        manager.transition(BootState::Ready, "invalid").await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to Maintenance should be rejected
        manager.transition(BootState::Maintenance, "invalid").await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to InitializingDb should be rejected
        manager
            .transition(BootState::InitializingDb, "invalid")
            .await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to LoadingPolicies should be rejected
        manager
            .transition(BootState::LoadingPolicies, "invalid")
            .await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to StartingBackend should be rejected
        manager
            .transition(BootState::StartingBackend, "invalid")
            .await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to LoadingBaseModels should be rejected
        manager
            .transition(BootState::LoadingBaseModels, "invalid")
            .await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to LoadingAdapters should be rejected
        manager
            .transition(BootState::LoadingAdapters, "invalid")
            .await;
        assert_eq!(manager.current_state(), BootState::Stopping);

        // Attempt transition to FullyReady should be rejected
        manager.transition(BootState::FullyReady, "invalid").await;
        assert_eq!(manager.current_state(), BootState::Stopping);
    }

    #[tokio::test]
    async fn test_stopped_allows_reboot() {
        let manager = BootStateManager::new();

        // Initial state is Stopped
        assert_eq!(manager.current_state(), BootState::Stopped);
        assert!(!manager.current_state().is_terminal());

        // Stopped should allow transition to Booting (reboot scenario)
        manager.boot().await;
        assert_eq!(manager.current_state(), BootState::Booting);
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
            BootState::Booting
        ));
        assert!(!BootStateManager::is_allowed_transition(
            BootState::Stopping,
            BootState::Ready
        ));
        assert!(!BootStateManager::is_allowed_transition(
            BootState::Stopping,
            BootState::Draining
        ));

        // Stopped is NOT terminal - should allow Booting
        assert!(BootStateManager::is_allowed_transition(
            BootState::Stopped,
            BootState::Booting
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
        // Create a manager and progress to a state where we can test concurrent transitions
        let manager = Arc::new(BootStateManager::new());
        manager.boot().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;

        // At this point we're at LoadingAdapters state
        assert_eq!(manager.current_state(), BootState::LoadingAdapters);

        // Spawn 15 concurrent tasks attempting to transition to Ready state
        let mut handles = vec![];
        for i in 0..15 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::Ready, &format!("concurrent-{}", i))
                    .await;
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify we successfully transitioned to Ready (exactly once)
        assert_eq!(manager.current_state(), BootState::Ready);
        assert!(manager.is_ready());

        // Now test concurrent invalid transitions from Ready
        let mut invalid_handles = vec![];
        for i in 0..10 {
            let m = Arc::clone(&manager);
            let handle = tokio::spawn(async move {
                // These should all be rejected (can't go back to Booting from Ready)
                m.transition(BootState::Booting, &format!("invalid-{}", i))
                    .await;
            });
            invalid_handles.push(handle);
        }

        // Wait for all invalid attempts to complete
        for handle in invalid_handles {
            handle.await.unwrap();
        }

        // State should remain Ready (all invalid transitions rejected)
        assert_eq!(manager.current_state(), BootState::Ready);

        // Test concurrent transitions to valid next states
        let valid_manager = Arc::clone(&manager);
        let mut mixed_handles = vec![];

        // Spawn concurrent tasks attempting to transition to FullyReady, Maintenance, and Draining
        // Only one should succeed based on timing
        for i in 0..5 {
            let m = Arc::clone(&valid_manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::FullyReady, &format!("fully-ready-{}", i))
                    .await;
            });
            mixed_handles.push(handle);
        }

        for i in 0..5 {
            let m = Arc::clone(&valid_manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::Maintenance, &format!("maintenance-{}", i))
                    .await;
            });
            mixed_handles.push(handle);
        }

        for i in 0..5 {
            let m = Arc::clone(&valid_manager);
            let handle = tokio::spawn(async move {
                m.transition(BootState::Draining, &format!("draining-{}", i))
                    .await;
            });
            mixed_handles.push(handle);
        }

        // Wait for all attempts
        for handle in mixed_handles {
            handle.await.unwrap();
        }

        // Verify final state is valid (one of FullyReady, Maintenance, or Draining)
        let final_state = valid_manager.current_state();
        assert!(
            matches!(
                final_state,
                BootState::FullyReady | BootState::Maintenance | BootState::Draining
            ),
            "Expected valid state, got {:?}",
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

        // Transition to Ready state
        manager.boot().await;
        manager.init_db().await;
        manager.load_policies().await;
        manager.start_backend().await;
        manager.load_base_models().await;
        manager.load_adapters().await;
        manager.ready().await;

        assert_eq!(manager.current_state(), BootState::Ready);

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

        // Should have transitioned to Draining (exactly once)
        assert_eq!(manager.current_state(), BootState::Draining);
        assert!(manager.is_draining());
        assert!(manager.is_shutting_down());

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

        // Should have transitioned to Stopping
        assert_eq!(manager.current_state(), BootState::Stopping);
        assert!(manager.is_shutting_down());
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
