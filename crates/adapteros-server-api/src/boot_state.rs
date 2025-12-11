//! Boot lifecycle state management for AdapterOS server
//!
//! Implements the lifecycle state machine for server boot, runtime, and shutdown.
//!
//! ## State Flow
//!
//! ```text
//! stopped → booting → initializing-db → loading-policies → starting-backend →
//! loading-base-models → loading-adapters → ready → fully-ready → draining → stopping
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use adapteros_server::boot_state::{BootState, BootStateManager};
//!
//! let manager = BootStateManager::new();
//!
//! // Transition through states
//! manager.transition(BootState::Booting, "process-start").await;
//! manager.transition(BootState::InitializingDb, "config-loaded").await;
//! manager.transition(BootState::Ready, "network-bound").await;
//!
//! // Check current state
//! if manager.is_ready() {
//!     // Accept requests
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
    Booting,
    /// Database initialization (migrations, recovery)
    InitializingDb,
    /// Policy verification (hash watcher, baseline load)
    LoadingPolicies,
    /// Backend initialization (MLX/CoreML/Metal)
    StartingBackend,
    /// Base model loading (manifest validation, executor seeding)
    LoadingBaseModels,
    /// Adapter warmup (lifecycle manager, heartbeat recovery)
    LoadingAdapters,
    /// Accepting requests (HTTP/UDS), models may still be loading
    Ready,
    /// All priority models loaded and health-checked
    FullyReady,
    /// Maintenance mode (no new work, in-flight continues)
    Maintenance,
    /// Shutdown initiated (reject new requests, track in-flight)
    Draining,
    /// Component shutdown (ordered termination)
    Stopping,
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
            BootState::Booting
                | BootState::InitializingDb
                | BootState::LoadingPolicies
                | BootState::StartingBackend
                | BootState::LoadingBaseModels
                | BootState::LoadingAdapters
        )
    }

    /// Convert state to string for logging/telemetry
    pub fn as_str(&self) -> &'static str {
        match self {
            BootState::Stopped => "stopped",
            BootState::Booting => "booting",
            BootState::InitializingDb => "initializing-db",
            BootState::LoadingPolicies => "loading-policies",
            BootState::StartingBackend => "starting-backend",
            BootState::LoadingBaseModels => "loading-base-models",
            BootState::LoadingAdapters => "loading-adapters",
            BootState::Ready => "ready",
            BootState::FullyReady => "fully-ready",
            BootState::Maintenance => "maintenance",
            BootState::Draining => "draining",
            BootState::Stopping => "stopping",
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
}

impl BootStateManager {
    /// Create a new boot state manager
    pub fn new() -> Self {
        Self {
            current: Arc::new(RwLock::new(BootState::Stopped)),
            start_time: Instant::now(),
            db: None,
            model_status: Arc::new(RwLock::new(ModelLoadingStatus::default())),
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
        // Allow both the canonical DB-first boot path and the backend-first path
        // currently executed by adapteros-server during startup.
        matches!(
            (from, to),
            (BootState::Stopped, BootState::Booting)
                | (BootState::Booting, BootState::StartingBackend)
                | (BootState::Booting, BootState::InitializingDb)
                | (BootState::InitializingDb, BootState::LoadingPolicies)
                | (BootState::LoadingPolicies, BootState::StartingBackend)
                | (BootState::LoadingPolicies, BootState::LoadingAdapters)
                | (BootState::StartingBackend, BootState::LoadingBaseModels)
                | (BootState::LoadingBaseModels, BootState::InitializingDb)
                | (BootState::LoadingBaseModels, BootState::LoadingAdapters)
                | (BootState::LoadingAdapters, BootState::Ready)
                | (BootState::Ready, BootState::FullyReady)
                | (BootState::Ready, BootState::Maintenance)
                | (BootState::FullyReady, BootState::Maintenance)
                | (BootState::Maintenance, BootState::Ready)
                | (BootState::Ready, BootState::Draining)
                | (BootState::FullyReady, BootState::Draining)
                | (BootState::Maintenance, BootState::Draining)
                | (BootState::Draining, BootState::Stopping)
        )
    }

    /// Transition to Booting state
    pub async fn boot(&self) {
        self.transition(BootState::Booting, "process-start").await;
    }

    /// Transition to InitializingDb state
    pub async fn init_db(&self) {
        self.transition(BootState::InitializingDb, "config-loaded")
            .await;
    }

    /// Transition to LoadingPolicies state
    pub async fn load_policies(&self) {
        self.transition(BootState::LoadingPolicies, "migrations-complete")
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
        assert_eq!(BootState::Stopped.as_str(), "stopped");
        assert_eq!(BootState::Booting.as_str(), "booting");
        assert_eq!(BootState::Ready.as_str(), "ready");
        assert_eq!(BootState::Maintenance.as_str(), "maintenance");
        assert_eq!(BootState::Draining.as_str(), "draining");
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
}
