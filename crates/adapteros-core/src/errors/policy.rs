//! Policy and security violation errors
//!
//! Covers policy violations, egress restrictions, isolation breaches, and determinism failures.

use thiserror::Error;

/// Policy and security errors
#[derive(Error, Debug)]
pub enum AosPolicyError {
    /// General policy violation
    #[error("Policy violation: {0}")]
    Violation(String),

    /// Policy configuration or loading error
    #[error("Policy error: {0}")]
    Policy(String),

    /// Egress (outbound network) violation
    #[error("Egress violation: {0}")]
    EgressViolation(String),

    /// Tenant or process isolation violation
    #[error("Isolation violation: {0}")]
    IsolationViolation(String),

    /// System quarantined due to violations
    #[error("System quarantined due to policy hash violations: {0}")]
    Quarantined(String),

    /// Policy pack hash mismatch
    #[error("Policy hash mismatch for {pack_id}: expected {expected}, got {actual}")]
    PolicyHashMismatch {
        pack_id: String,
        expected: String,
        actual: String,
    },

    /// Determinism violation (non-reproducible behavior)
    #[error("Determinism violation: {0}")]
    DeterminismViolation(String),

    /// Performance SLA violation
    #[error("Performance violation: {0}")]
    PerformanceViolation(String),
}

impl AosPolicyError {
    /// Check if this is a security-critical error that should be audited
    pub fn is_security_critical(&self) -> bool {
        matches!(
            self,
            Self::Violation(_)
                | Self::EgressViolation(_)
                | Self::IsolationViolation(_)
                | Self::Quarantined(_)
                | Self::PolicyHashMismatch { .. }
        )
    }
}
