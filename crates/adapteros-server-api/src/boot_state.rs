//! Boot lifecycle state management for AdapterOS server
//!
//! Implements the lifecycle state machine for server boot, runtime, and shutdown.
//!
//! ## State Flow
//!
//! ```text
//! stopped → booting → initializing-db → loading-policies → starting-backend →
//! loading-base-models → loading-adapters → ready → draining → stopping
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
    /// Accepting requests (HTTP/UDS)
    Ready,
    /// Shutdown initiated (reject new requests, track in-flight)
    Draining,
    /// Component shutdown (ordered termination)
    Stopping,
}

impl BootState {
    /// Returns true if this state indicates the server is accepting requests
    pub fn is_ready(&self) -> bool {
        matches!(self, BootState::Ready)
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

/// Manager for boot lifecycle state
pub struct BootStateManager {
    /// Current state
    current: Arc<RwLock<BootState>>,
    /// Process start time
    start_time: Instant,
    /// Database for audit logging (optional)
    db: Option<Arc<Db>>,
}

impl BootStateManager {
    /// Create a new boot state manager
    pub fn new() -> Self {
        Self {
            current: Arc::new(RwLock::new(BootState::Stopped)),
            start_time: Instant::now(),
            db: None,
        }
    }

    /// Create a new boot state manager with database for audit logging
    pub fn with_db(db: Arc<Db>) -> Self {
        Self {
            current: Arc::new(RwLock::new(BootState::Stopped)),
            start_time: Instant::now(),
            db: Some(db),
        }
    }

    /// Get the current state
    pub fn current_state(&self) -> BootState {
        *self.current.read()
    }

    /// Check if server is ready to accept requests
    pub fn is_ready(&self) -> bool {
        self.current_state().is_ready()
    }

    /// Check if server is shutting down
    pub fn is_shutting_down(&self) -> bool {
        self.current_state().is_shutting_down()
    }

    /// Check if server is booting
    pub fn is_booting(&self) -> bool {
        self.current_state().is_booting()
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(BootState::Draining.as_str(), "draining");
    }

    #[tokio::test]
    async fn test_elapsed_time() {
        let manager = BootStateManager::new();
        tokio::time::sleep(Duration::from_millis(10)).await;

        let elapsed = manager.elapsed();
        assert!(elapsed >= Duration::from_millis(10));
    }
}
