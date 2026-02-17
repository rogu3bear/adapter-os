//! Quarantine Manager
//!
//! Enforces strict quarantine when policy hash violations are detected.
//! Per Determinism Ruleset #2: refuse to serve if policy hashes don't match.
//!
//! Quarantine semantics:
//! - **DENY**: All inference and adapter operations
//! - **ALLOW**: Read-only audit operations (status, metrics)
//! - **REQUIRE**: Operator intervention via `aosctl` to resolve

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Operation types for quarantine checks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineOperation {
    /// Inference request
    Inference,

    /// Load adapter into memory
    AdapterLoad,

    /// Swap adapters (hot-swap)
    AdapterSwap,

    /// Memory allocation or reallocation
    MemoryOperation,

    /// Training operation
    Training,

    /// Policy update
    PolicyUpdate,

    /// Audit operation (read-only)
    Audit,

    /// Status check (read-only)
    Status,

    /// Metrics retrieval (read-only)
    Metrics,
}

impl QuarantineOperation {
    /// Check if this operation is allowed during quarantine
    pub fn allowed_in_quarantine(&self) -> bool {
        matches!(
            self,
            QuarantineOperation::Audit | QuarantineOperation::Status | QuarantineOperation::Metrics
        )
    }

    /// Get human-readable operation name
    pub fn name(&self) -> &'static str {
        match self {
            QuarantineOperation::Inference => "inference",
            QuarantineOperation::AdapterLoad => "adapter_load",
            QuarantineOperation::AdapterSwap => "adapter_swap",
            QuarantineOperation::MemoryOperation => "memory_operation",
            QuarantineOperation::Training => "training",
            QuarantineOperation::PolicyUpdate => "policy_update",
            QuarantineOperation::Audit => "audit",
            QuarantineOperation::Status => "status",
            QuarantineOperation::Metrics => "metrics",
        }
    }
}

/// Quarantine manager for policy violation enforcement.
///
/// # Persistence Model
///
/// `QuarantineManager` is intentionally in-memory. Quarantine *records* are persisted
/// to the `policy_quarantine` DB table by `PolicyHashWatcher::trigger_quarantine()` and
/// `FederationDaemon::trigger_policy_quarantine()`. On server boot,
/// `FederationDaemon::restore_quarantine_from_db()` checks for unreleased DB records
/// and restores the in-memory flag, ensuring quarantine survives restarts.
///
/// The background `PolicyHashWatcher` (60s) and `FederationDaemon` (5min) sweeps
/// provide continuous re-validation — if violations exist at runtime, quarantine
/// will be re-triggered regardless of the boot-time state.
pub struct QuarantineManager {
    /// Whether the system is currently quarantined
    quarantined: bool,

    /// Violation details for reporting
    violation_summary: String,
}

impl QuarantineManager {
    /// Create a new quarantine manager
    pub fn new() -> Self {
        Self {
            quarantined: false,
            violation_summary: String::new(),
        }
    }

    /// Set quarantine status
    pub fn set_quarantined(&mut self, quarantined: bool, violation_summary: String) {
        self.quarantined = quarantined;
        self.violation_summary = violation_summary;
        if quarantined {
            warn!(
                target: "security.quarantine",
                violation_summary = %self.violation_summary,
                "quarantine entered"
            );
        }
    }

    /// Check if system is quarantined
    pub fn is_quarantined(&self) -> bool {
        self.quarantined
    }

    /// Get violation summary
    pub fn violation_summary(&self) -> &str {
        &self.violation_summary
    }

    /// Release quarantine (clear quarantine status).
    ///
    /// This sets `quarantined` to false and clears the violation summary.
    /// Should be called after violations have been resolved.
    pub fn release_quarantine(&mut self) {
        self.quarantined = false;
        self.violation_summary = String::new();
        info!(
            target: "security.quarantine",
            "quarantine released"
        );
    }

    /// Release quarantine if the violation matches a specific policy pack.
    ///
    /// Returns true if quarantine was released, false if the violation was for a different pack.
    pub fn release_quarantine_for_pack(&mut self, pack_id: &str) -> bool {
        if !self.quarantined {
            return false;
        }

        // Check if the violation summary mentions this pack
        if self.violation_summary.contains(pack_id) {
            info!(
                target: "security.quarantine",
                pack = %pack_id,
                "quarantine released for policy pack"
            );
            self.release_quarantine();
            true
        } else {
            false
        }
    }

    /// Check if an operation is allowed
    ///
    /// Returns `Ok(())` if allowed, `Err(AosError::Quarantined)` if denied.
    pub fn check_operation(&self, operation: QuarantineOperation) -> Result<()> {
        if !self.quarantined {
            // Not quarantined, all operations allowed
            return Ok(());
        }

        if operation.allowed_in_quarantine() {
            // Audit operations allowed during quarantine
            return Ok(());
        }

        // Operation denied due to quarantine
        warn!(
            operation = %operation.name(),
            "Operation denied due to policy hash quarantine"
        );

        Err(AosError::Quarantined(format!(
            "Operation '{}' denied: {}",
            operation.name(),
            self.violation_summary
        )))
    }

    /// Format quarantine status for display
    pub fn status_message(&self) -> String {
        if self.quarantined {
            format!(
                "QUARANTINED: {}\nOnly audit operations (status, metrics, audit) are allowed.",
                self.violation_summary
            )
        } else {
            "OPERATIONAL: All operations allowed.".to_string()
        }
    }
}

impl Default for QuarantineManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operations_allowed_when_not_quarantined() {
        let manager = QuarantineManager::new();

        assert!(manager
            .check_operation(QuarantineOperation::Inference)
            .is_ok());
        assert!(manager
            .check_operation(QuarantineOperation::AdapterLoad)
            .is_ok());
        assert!(manager.check_operation(QuarantineOperation::Audit).is_ok());
        assert!(manager.check_operation(QuarantineOperation::Status).is_ok());
    }

    #[test]
    fn test_operations_denied_when_quarantined() {
        let mut manager = QuarantineManager::new();
        manager.set_quarantined(true, "Test violation".to_string());

        assert!(manager
            .check_operation(QuarantineOperation::Inference)
            .is_err());
        assert!(manager
            .check_operation(QuarantineOperation::AdapterLoad)
            .is_err());
        assert!(manager
            .check_operation(QuarantineOperation::AdapterSwap)
            .is_err());
        assert!(manager
            .check_operation(QuarantineOperation::MemoryOperation)
            .is_err());
        assert!(manager
            .check_operation(QuarantineOperation::Training)
            .is_err());
        assert!(manager
            .check_operation(QuarantineOperation::PolicyUpdate)
            .is_err());
    }

    #[test]
    fn test_audit_operations_allowed_when_quarantined() {
        let mut manager = QuarantineManager::new();
        manager.set_quarantined(true, "Test violation".to_string());

        assert!(manager.check_operation(QuarantineOperation::Audit).is_ok());
        assert!(manager.check_operation(QuarantineOperation::Status).is_ok());
        assert!(manager
            .check_operation(QuarantineOperation::Metrics)
            .is_ok());
    }

    #[test]
    fn test_status_message() {
        let mut manager = QuarantineManager::new();

        let msg = manager.status_message();
        assert!(msg.contains("OPERATIONAL"));

        manager.set_quarantined(true, "Policy hash mismatch".to_string());
        let msg = manager.status_message();
        assert!(msg.contains("QUARANTINED"));
        assert!(msg.contains("Policy hash mismatch"));
    }

    #[test]
    fn test_operation_type_allowed_in_quarantine() {
        assert!(!QuarantineOperation::Inference.allowed_in_quarantine());
        assert!(!QuarantineOperation::AdapterLoad.allowed_in_quarantine());
        assert!(!QuarantineOperation::AdapterSwap.allowed_in_quarantine());
        assert!(!QuarantineOperation::MemoryOperation.allowed_in_quarantine());
        assert!(!QuarantineOperation::Training.allowed_in_quarantine());
        assert!(!QuarantineOperation::PolicyUpdate.allowed_in_quarantine());

        assert!(QuarantineOperation::Audit.allowed_in_quarantine());
        assert!(QuarantineOperation::Status.allowed_in_quarantine());
        assert!(QuarantineOperation::Metrics.allowed_in_quarantine());
    }
}
