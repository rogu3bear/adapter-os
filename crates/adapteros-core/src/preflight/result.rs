//! Preflight result types
//!
//! Provides structured result types for preflight checks, including
//! individual check results and comprehensive preflight outcomes.

use super::error::{PreflightCheckFailure, PreflightErrorCode};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Status of an individual preflight check
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// Check passed
    Pass,
    /// Check failed (blocking)
    Fail,
    /// Check has warning (non-blocking)
    Warning,
    /// Check was skipped (by configuration)
    Skipped,
}

impl CheckStatus {
    /// Returns true if this status represents a failure
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Fail)
    }

    /// Returns true if this status represents a pass or skip
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Pass | Self::Skipped)
    }
}

/// Individual preflight check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightCheck {
    /// Name of the check (e.g., "Content Hash", "Lifecycle State")
    pub name: String,

    /// Check status
    pub status: CheckStatus,

    /// Human-readable message
    pub message: String,

    /// Error code (only for failures)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<PreflightErrorCode>,

    /// Suggested remediation (only for failures)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,

    /// Time taken for this check
    pub duration_ms: u64,
}

impl PreflightCheck {
    /// Create a passing check result
    pub fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            message: message.into(),
            code: None,
            remediation: None,
            duration_ms: 0,
        }
    }

    /// Create a failing check result
    pub fn fail(
        name: impl Into<String>,
        code: PreflightErrorCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            message: message.into(),
            code: Some(code),
            remediation: None,
            duration_ms: 0,
        }
    }

    /// Create a warning check result
    pub fn warning(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Warning,
            message: message.into(),
            code: None,
            remediation: None,
            duration_ms: 0,
        }
    }

    /// Create a skipped check result
    pub fn skipped(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Skipped,
            message: reason.into(),
            code: None,
            remediation: None,
            duration_ms: 0,
        }
    }

    /// Add remediation suggestion
    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    /// Set duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration_ms = duration.as_millis() as u64;
        self
    }

    /// Set duration in milliseconds
    pub fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = duration_ms;
        self
    }
}

/// Record of a bypass being used (for audit purposes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BypassEvent {
    /// Which bypass was used
    pub bypass_used: Option<String>,
    /// Reason for the bypass
    pub reason: Option<String>,
}

/// Comprehensive preflight result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightResult {
    /// Whether all checks passed (or force was used)
    pub passed: bool,

    /// Adapter ID that was checked
    pub adapter_id: String,

    /// All individual check results
    pub checks: Vec<PreflightCheck>,

    /// Structured failures (for programmatic handling)
    pub failures: Vec<PreflightCheckFailure>,

    /// Warning messages (non-blocking)
    pub warnings: Vec<String>,

    /// Total time taken for all checks
    pub total_duration_ms: u64,

    /// Bypass flags that were used
    pub bypasses_used: Vec<String>,

    /// Whether force mode was applied
    pub force_applied: bool,

    /// Audit events for tracking bypass reasons
    #[serde(default)]
    pub audit_events: Vec<BypassEvent>,
}

impl PreflightResult {
    /// Create a new empty result for an adapter
    pub fn new(adapter_id: impl Into<String>) -> Self {
        Self {
            passed: true,
            adapter_id: adapter_id.into(),
            checks: Vec::new(),
            failures: Vec::new(),
            warnings: Vec::new(),
            total_duration_ms: 0,
            bypasses_used: Vec::new(),
            force_applied: false,
            audit_events: Vec::new(),
        }
    }

    /// Generate a human-readable summary of all failures
    pub fn failure_summary(&self) -> String {
        if self.failures.is_empty() {
            return String::new();
        }
        self.failures
            .iter()
            .map(|f| format!("{}: {}", f.code.as_str(), f.message))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Add a check result
    pub fn add_check(&mut self, check: PreflightCheck) {
        if check.status == CheckStatus::Fail {
            self.passed = false;
            if let Some(code) = check.code {
                self.failures.push(
                    PreflightCheckFailure::new(code, &check.name, &check.message)
                        .with_remediation(check.remediation.clone().unwrap_or_default()),
                );
            }
        } else if check.status == CheckStatus::Warning {
            self.warnings.push(check.message.clone());
        }
        self.checks.push(check);
    }

    /// Add a passing check
    pub fn add_pass(&mut self, name: impl Into<String>, message: impl Into<String>) {
        self.add_check(PreflightCheck::pass(name, message));
    }

    /// Add a failing check
    pub fn add_fail(
        &mut self,
        name: impl Into<String>,
        code: PreflightErrorCode,
        message: impl Into<String>,
        remediation: Option<String>,
    ) {
        let mut check = PreflightCheck::fail(name, code, message);
        if let Some(rem) = remediation {
            check = check.with_remediation(rem);
        }
        self.add_check(check);
    }

    /// Add a warning
    pub fn add_warning(&mut self, name: impl Into<String>, message: impl Into<String>) {
        self.add_check(PreflightCheck::warning(name, message));
    }

    /// Add a skipped check
    pub fn add_skipped(&mut self, name: impl Into<String>, reason: impl Into<String>) {
        self.add_check(PreflightCheck::skipped(name, reason));
    }

    /// Record bypass usage
    pub fn record_bypass(&mut self, bypass_name: impl Into<String>) {
        self.bypasses_used.push(bypass_name.into());
    }

    /// Apply force mode (converts failures to warnings)
    pub fn apply_force(&mut self) {
        self.force_applied = true;
        self.passed = true;

        // Convert failures to warnings
        for check in &mut self.checks {
            if check.status == CheckStatus::Fail {
                check.status = CheckStatus::Warning;
                self.warnings
                    .push(format!("[FORCED] {}: {}", check.name, check.message));
            }
        }
    }

    /// Set total duration
    pub fn set_duration(&mut self, duration: Duration) {
        self.total_duration_ms = duration.as_millis() as u64;
    }

    /// Get all error codes from failures
    pub fn error_codes(&self) -> Vec<PreflightErrorCode> {
        self.failures.iter().map(|f| f.code).collect()
    }

    /// Get the primary (first) error code
    pub fn primary_error_code(&self) -> Option<PreflightErrorCode> {
        self.failures.first().map(|f| f.code)
    }

    /// Get all failed check names
    pub fn failed_checks(&self) -> Vec<&str> {
        self.checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .map(|c| c.name.as_str())
            .collect()
    }

    /// Get count of passed checks
    pub fn passed_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == CheckStatus::Pass)
            .count()
    }

    /// Get count of failed checks
    pub fn failed_count(&self) -> usize {
        self.checks
            .iter()
            .filter(|c| c.status == CheckStatus::Fail)
            .count()
    }

    /// Generate a summary string
    pub fn summary(&self) -> String {
        let passed = self.passed_count();
        let failed = self.failed_count();
        let warnings = self.warnings.len();
        let skipped = self
            .checks
            .iter()
            .filter(|c| c.status == CheckStatus::Skipped)
            .count();

        if self.passed {
            if warnings > 0 {
                format!(
                    "Preflight passed with {} warnings ({}/{} checks, {} skipped)",
                    warnings,
                    passed,
                    self.checks.len(),
                    skipped
                )
            } else {
                format!(
                    "Preflight passed ({}/{} checks, {} skipped)",
                    passed,
                    self.checks.len(),
                    skipped
                )
            }
        } else {
            format!(
                "Preflight failed: {} failures, {} warnings ({}/{} checks passed)",
                failed,
                warnings,
                passed,
                self.checks.len()
            )
        }
    }
}

/// Event emitted when preflight completes (for audit logging)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreflightAuditEvent {
    /// Adapter that was checked
    pub adapter_id: String,

    /// Tenant context
    pub tenant_id: String,

    /// Actor who initiated the check
    pub actor: String,

    /// Whether preflight passed
    pub passed: bool,

    /// Bypass flags that were used
    pub bypasses_used: Vec<String>,

    /// Reason for bypasses (if any)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bypass_reason: Option<String>,

    /// Error codes from failures
    pub failure_codes: Vec<String>,

    /// Total check duration in ms
    pub duration_ms: u64,

    /// Timestamp of the event
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl PreflightResult {
    /// Generate an audit event from this result
    pub fn to_audit_event(
        &self,
        tenant_id: &str,
        actor: &str,
        bypass_reason: Option<&str>,
    ) -> PreflightAuditEvent {
        PreflightAuditEvent {
            adapter_id: self.adapter_id.clone(),
            tenant_id: tenant_id.to_string(),
            actor: actor.to_string(),
            passed: self.passed,
            bypasses_used: self.bypasses_used.clone(),
            bypass_reason: bypass_reason.map(String::from),
            failure_codes: self
                .failures
                .iter()
                .map(|f| f.code.as_str().to_string())
                .collect(),
            duration_ms: self.total_duration_ms,
            timestamp: chrono::Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_status() {
        assert!(CheckStatus::Fail.is_failure());
        assert!(!CheckStatus::Pass.is_failure());
        assert!(CheckStatus::Pass.is_ok());
        assert!(CheckStatus::Skipped.is_ok());
    }

    #[test]
    fn test_result_building() {
        let mut result = PreflightResult::new("test-adapter");
        result.add_pass("Check 1", "Passed");
        result.add_fail(
            "Check 2",
            PreflightErrorCode::MissingContentHash,
            "Missing hash",
            Some("repair-hashes".to_string()),
        );

        assert!(!result.passed);
        assert_eq!(result.passed_count(), 1);
        assert_eq!(result.failed_count(), 1);
        assert_eq!(
            result.primary_error_code(),
            Some(PreflightErrorCode::MissingContentHash)
        );
    }

    #[test]
    fn test_force_mode() {
        let mut result = PreflightResult::new("test-adapter");
        result.add_fail(
            "Check 1",
            PreflightErrorCode::MissingContentHash,
            "Missing hash",
            None,
        );

        assert!(!result.passed);
        result.apply_force();
        assert!(result.passed);
        assert!(result.force_applied);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_summary() {
        let mut result = PreflightResult::new("test-adapter");
        result.add_pass("Check 1", "OK");
        result.add_pass("Check 2", "OK");

        let summary = result.summary();
        assert!(summary.contains("passed"));
        assert!(summary.contains("2/2"));
    }
}
