//! Boot phase definitions and transition rules.
//!
//! This module defines the boot lifecycle phases that mirror `BootState` in
//! `adapteros-server-api/src/boot_state.rs` but without any Axum/HTTP dependencies.
//!
//! ## State Flow
//!
//! ```text
//! stopped -> starting -> security-init -> executor-init -> preflight -> boot-invariants ->
//! db-connecting -> migrating -> post-db-invariants -> startup-recovery -> seeding ->
//! loading-policies -> starting-backend -> loading-base-models -> loading-adapters ->
//! worker-discovery -> router-build -> finalize -> bind -> ready -> fully-ready ->
//! draining -> stopping
//!
//! Any state can transition to:
//!   - failed (critical failure)
//!   - degraded (non-critical dependency failure, only from ready states)
//!
//! Degraded can recover to ready, but failed is terminal.
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Instant;

/// Boot lifecycle phases.
///
/// These phases represent the stages of server boot, runtime, and shutdown.
/// The phases are designed to be monotonic during normal boot (forward-only),
/// with special handling for failure and degraded states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BootPhase {
    /// Server not running (initial state)
    Stopped,
    /// Initial process startup (PID lock, config load)
    Starting,
    /// Security subsystem initialization
    SecurityInit,
    /// Deterministic executor setup
    ExecutorInit,
    /// Security preflight checks
    Preflight,
    /// Pre-database invariant validation
    BootInvariants,
    /// Establishing database connection
    DbConnecting,
    /// Running database migrations
    Migrating,
    /// Post-database invariant validation
    PostDbInvariants,
    /// Orphaned resource recovery
    StartupRecovery,
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
    /// API router construction
    RouterBuild,
    /// Final boot preparation
    Finalize,
    /// Server socket binding
    Bind,
    /// Accepting requests (HTTP/UDS), models may still be loading
    Ready,
    /// All priority models loaded and health-checked
    FullyReady,
    /// Non-critical dependency failure (can recover to Ready)
    Degraded,
    /// Critical failure - terminal state
    Failed,
    /// Maintenance mode (no new work, in-flight continues)
    Maintenance,
    /// Shutdown initiated (reject new requests, track in-flight)
    Draining,
    /// Component shutdown (ordered termination)
    Stopping,
}

impl BootPhase {
    /// Returns true if this phase indicates the server is accepting requests.
    pub fn is_ready(&self) -> bool {
        matches!(self, BootPhase::Ready | BootPhase::FullyReady)
    }

    /// Returns true if all models are loaded and healthy.
    pub fn is_fully_ready(&self) -> bool {
        matches!(self, BootPhase::FullyReady)
    }

    /// Returns true if server is in maintenance.
    pub fn is_maintenance(&self) -> bool {
        matches!(self, BootPhase::Maintenance)
    }

    /// Returns true if server is in degraded state.
    pub fn is_degraded(&self) -> bool {
        matches!(self, BootPhase::Degraded)
    }

    /// Returns true if server has failed.
    pub fn is_failed(&self) -> bool {
        matches!(self, BootPhase::Failed)
    }

    /// Returns true if server is draining or stopping.
    pub fn is_draining(&self) -> bool {
        matches!(self, BootPhase::Draining | BootPhase::Stopping)
    }

    /// Returns true if this phase indicates the server is shutting down.
    ///
    /// This is an alias for `is_draining()` for backward compatibility
    /// with code that used the old `BootState::is_shutting_down()` method.
    pub fn is_shutting_down(&self) -> bool {
        self.is_draining()
    }

    /// Returns true if this phase is terminal (no further transitions allowed).
    pub fn is_terminal(&self) -> bool {
        matches!(self, BootPhase::Stopping | BootPhase::Failed)
    }

    /// Returns true if this phase indicates the server is booting.
    pub fn is_booting(&self) -> bool {
        matches!(
            self,
            BootPhase::Starting
                | BootPhase::SecurityInit
                | BootPhase::ExecutorInit
                | BootPhase::Preflight
                | BootPhase::BootInvariants
                | BootPhase::DbConnecting
                | BootPhase::Migrating
                | BootPhase::PostDbInvariants
                | BootPhase::StartupRecovery
                | BootPhase::Seeding
                | BootPhase::LoadingPolicies
                | BootPhase::StartingBackend
                | BootPhase::LoadingBaseModels
                | BootPhase::LoadingAdapters
                | BootPhase::WorkerDiscovery
                | BootPhase::RouterBuild
                | BootPhase::Finalize
                | BootPhase::Bind
        )
    }

    /// Convert phase to string for logging/telemetry.
    pub fn as_str(&self) -> &'static str {
        match self {
            BootPhase::Stopped => "stopped",
            BootPhase::Starting => "starting",
            BootPhase::SecurityInit => "security-init",
            BootPhase::ExecutorInit => "executor-init",
            BootPhase::Preflight => "preflight",
            BootPhase::BootInvariants => "boot-invariants",
            BootPhase::DbConnecting => "db-connecting",
            BootPhase::Migrating => "migrating",
            BootPhase::PostDbInvariants => "post-db-invariants",
            BootPhase::StartupRecovery => "startup-recovery",
            BootPhase::Seeding => "seeding",
            BootPhase::LoadingPolicies => "loading-policies",
            BootPhase::StartingBackend => "starting-backend",
            BootPhase::LoadingBaseModels => "loading-base-models",
            BootPhase::LoadingAdapters => "loading-adapters",
            BootPhase::WorkerDiscovery => "worker-discovery",
            BootPhase::RouterBuild => "router-build",
            BootPhase::Finalize => "finalize",
            BootPhase::Bind => "bind",
            BootPhase::Ready => "ready",
            BootPhase::FullyReady => "fully-ready",
            BootPhase::Degraded => "degraded",
            BootPhase::Failed => "failed",
            BootPhase::Maintenance => "maintenance",
            BootPhase::Draining => "draining",
            BootPhase::Stopping => "stopping",
        }
    }
}

impl fmt::Display for BootPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Phase timing information for boot report generation.
#[derive(Debug, Clone)]
pub struct PhaseTiming {
    /// The phase this timing refers to.
    pub phase: BootPhase,
    /// When this phase started.
    pub started_at: Instant,
    /// When this phase completed (None if still in progress).
    pub completed_at: Option<Instant>,
    /// Duration in milliseconds (computed when completed).
    pub duration_ms: Option<u64>,
}

impl PhaseTiming {
    /// Create a new phase timing starting now.
    pub fn start(phase: BootPhase) -> Self {
        Self {
            phase,
            started_at: Instant::now(),
            completed_at: None,
            duration_ms: None,
        }
    }

    /// Mark this phase as completed.
    pub fn complete(&mut self) {
        let now = Instant::now();
        self.completed_at = Some(now);
        self.duration_ms = Some(now.duration_since(self.started_at).as_millis() as u64);
    }
}

/// Phase transition validator.
///
/// Enforces the boot lifecycle state machine rules:
/// - Boot phases progress forward monotonically
/// - Failed state can be reached from any non-terminal state
/// - Degraded can only be reached from Ready/FullyReady
/// - Recovery from Degraded to Ready is allowed
/// - Terminal states (Failed, Stopping) prevent further transitions
pub struct PhaseTransitions;

impl PhaseTransitions {
    /// Check if a transition from one phase to another is valid.
    ///
    /// # Rules
    ///
    /// 1. Terminal states (Failed, Stopping) cannot transition to anything
    /// 2. Any non-terminal state can transition to Failed
    /// 3. Degraded can only be reached from Ready or FullyReady
    /// 4. Degraded can recover to Ready
    /// 5. Normal boot progression is forward-only
    pub fn is_valid(from: BootPhase, to: BootPhase) -> bool {
        // Terminal states cannot transition
        if from.is_terminal() {
            return false;
        }

        // Failed can be reached from any non-terminal state (critical failure)
        if to == BootPhase::Failed {
            return true;
        }

        // Degraded can only be reached from Ready states
        if to == BootPhase::Degraded {
            return matches!(from, BootPhase::Ready | BootPhase::FullyReady);
        }

        // Recovery from Degraded back to Ready
        if from == BootPhase::Degraded && to == BootPhase::Ready {
            return true;
        }

        // Standard boot sequence transitions
        matches!(
            (from, to),
            // Boot sequence: stopped -> starting -> security -> executor -> preflight -> invariants
            (BootPhase::Stopped, BootPhase::Starting)
                | (BootPhase::Starting, BootPhase::SecurityInit)
                | (BootPhase::SecurityInit, BootPhase::ExecutorInit)
                | (BootPhase::ExecutorInit, BootPhase::Preflight)
                | (BootPhase::Preflight, BootPhase::BootInvariants)
                // Database initialization
                | (BootPhase::BootInvariants, BootPhase::DbConnecting)
                | (BootPhase::DbConnecting, BootPhase::Migrating)
                | (BootPhase::Migrating, BootPhase::PostDbInvariants)
                | (BootPhase::PostDbInvariants, BootPhase::StartupRecovery)
                // Data and policy loading
                | (BootPhase::StartupRecovery, BootPhase::Seeding)
                | (BootPhase::Seeding, BootPhase::LoadingPolicies)
                // Backend initialization
                | (BootPhase::LoadingPolicies, BootPhase::StartingBackend)
                | (BootPhase::StartingBackend, BootPhase::LoadingBaseModels)
                | (BootPhase::LoadingBaseModels, BootPhase::LoadingAdapters)
                | (BootPhase::LoadingAdapters, BootPhase::WorkerDiscovery)
                // Server finalization
                | (BootPhase::WorkerDiscovery, BootPhase::RouterBuild)
                | (BootPhase::RouterBuild, BootPhase::Finalize)
                | (BootPhase::Finalize, BootPhase::Bind)
                | (BootPhase::Bind, BootPhase::Ready)
                // Ready state transitions
                | (BootPhase::Ready, BootPhase::FullyReady)
                | (BootPhase::Ready, BootPhase::Maintenance)
                | (BootPhase::FullyReady, BootPhase::Maintenance)
                | (BootPhase::Maintenance, BootPhase::Ready)
                // Shutdown transitions
                | (BootPhase::Ready, BootPhase::Draining)
                | (BootPhase::FullyReady, BootPhase::Draining)
                | (BootPhase::Degraded, BootPhase::Draining)
                | (BootPhase::Maintenance, BootPhase::Draining)
                | (BootPhase::Draining, BootPhase::Stopping)
                // Backwards compatibility: allow skipping infrastructure phases
                | (BootPhase::Starting, BootPhase::DbConnecting)
                | (BootPhase::Migrating, BootPhase::Seeding)
                | (BootPhase::LoadingAdapters, BootPhase::Ready)
                | (BootPhase::WorkerDiscovery, BootPhase::Ready)
        )
    }

    /// Get the expected next phase in the boot sequence.
    ///
    /// Returns None for terminal states or states where multiple transitions are valid.
    pub fn next_boot_phase(current: BootPhase) -> Option<BootPhase> {
        match current {
            BootPhase::Stopped => Some(BootPhase::Starting),
            BootPhase::Starting => Some(BootPhase::SecurityInit),
            BootPhase::SecurityInit => Some(BootPhase::ExecutorInit),
            BootPhase::ExecutorInit => Some(BootPhase::Preflight),
            BootPhase::Preflight => Some(BootPhase::BootInvariants),
            BootPhase::BootInvariants => Some(BootPhase::DbConnecting),
            BootPhase::DbConnecting => Some(BootPhase::Migrating),
            BootPhase::Migrating => Some(BootPhase::PostDbInvariants),
            BootPhase::PostDbInvariants => Some(BootPhase::StartupRecovery),
            BootPhase::StartupRecovery => Some(BootPhase::Seeding),
            BootPhase::Seeding => Some(BootPhase::LoadingPolicies),
            BootPhase::LoadingPolicies => Some(BootPhase::StartingBackend),
            BootPhase::StartingBackend => Some(BootPhase::LoadingBaseModels),
            BootPhase::LoadingBaseModels => Some(BootPhase::LoadingAdapters),
            BootPhase::LoadingAdapters => Some(BootPhase::WorkerDiscovery),
            BootPhase::WorkerDiscovery => Some(BootPhase::RouterBuild),
            BootPhase::RouterBuild => Some(BootPhase::Finalize),
            BootPhase::Finalize => Some(BootPhase::Bind),
            BootPhase::Bind => Some(BootPhase::Ready),
            BootPhase::Ready => Some(BootPhase::FullyReady),
            // FullyReady has no single next phase (could be Draining or Maintenance)
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normal_boot_sequence() {
        let sequence = [
            BootPhase::Stopped,
            BootPhase::Starting,
            BootPhase::DbConnecting,
            BootPhase::Migrating,
            BootPhase::Seeding,
            BootPhase::LoadingPolicies,
            BootPhase::StartingBackend,
            BootPhase::LoadingBaseModels,
            BootPhase::LoadingAdapters,
            BootPhase::WorkerDiscovery,
            BootPhase::Ready,
            BootPhase::FullyReady,
        ];

        for i in 0..sequence.len() - 1 {
            assert!(
                PhaseTransitions::is_valid(sequence[i], sequence[i + 1]),
                "Transition from {:?} to {:?} should be valid",
                sequence[i],
                sequence[i + 1]
            );
        }
    }

    #[test]
    fn test_failed_from_any_state() {
        let bootable_states = [
            BootPhase::Starting,
            BootPhase::DbConnecting,
            BootPhase::Migrating,
            BootPhase::Ready,
            BootPhase::FullyReady,
        ];

        for state in bootable_states {
            assert!(
                PhaseTransitions::is_valid(state, BootPhase::Failed),
                "Should be able to fail from {:?}",
                state
            );
        }
    }

    #[test]
    fn test_terminal_states_cannot_transition() {
        assert!(!PhaseTransitions::is_valid(
            BootPhase::Failed,
            BootPhase::Ready
        ));
        assert!(!PhaseTransitions::is_valid(
            BootPhase::Stopping,
            BootPhase::Ready
        ));
    }

    #[test]
    fn test_degraded_only_from_ready_states() {
        assert!(PhaseTransitions::is_valid(
            BootPhase::Ready,
            BootPhase::Degraded
        ));
        assert!(PhaseTransitions::is_valid(
            BootPhase::FullyReady,
            BootPhase::Degraded
        ));
        assert!(!PhaseTransitions::is_valid(
            BootPhase::Starting,
            BootPhase::Degraded
        ));
    }

    #[test]
    fn test_degraded_recovery() {
        assert!(PhaseTransitions::is_valid(
            BootPhase::Degraded,
            BootPhase::Ready
        ));
    }

    #[test]
    fn test_shutdown_sequence() {
        assert!(PhaseTransitions::is_valid(
            BootPhase::FullyReady,
            BootPhase::Draining
        ));
        assert!(PhaseTransitions::is_valid(
            BootPhase::Draining,
            BootPhase::Stopping
        ));
    }

    #[test]
    fn test_phase_display() {
        assert_eq!(BootPhase::Ready.to_string(), "ready");
        assert_eq!(
            BootPhase::LoadingBaseModels.to_string(),
            "loading-base-models"
        );
    }
}
