//! Preflight configuration with bypass options
//!
//! Provides configuration for preflight behavior, including bypass flags
//! that can skip certain checks in controlled circumstances.

use serde::{Deserialize, Serialize};

/// Configuration for preflight behavior with bypass options
///
/// Bypass flags should be used sparingly and require justification.
/// When any bypass flag is set, the `bypass_reason` field should be populated
/// for audit logging purposes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PreflightConfig {
    /// Tenant ID for isolation checks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,

    /// Skip maintenance mode check
    ///
    /// Use only in emergencies when swap must proceed during maintenance.
    #[serde(default)]
    pub skip_maintenance_check: bool,

    /// Skip conflict detection for repo/branch uniqueness
    ///
    /// Use only when intentionally replacing an active adapter
    /// or when repo/branch metadata is known to be incomplete.
    #[serde(default)]
    pub skip_conflict_check: bool,

    /// Allow adapters in "training" state
    ///
    /// Normally only "ready" and "active" states allow swap/activation.
    /// This flag permits training adapters in controlled workflows.
    #[serde(default)]
    pub allow_training_state: bool,

    /// Force preflight to pass even with failures
    ///
    /// DANGEROUS: Use only in emergency recovery scenarios.
    /// All failures will be recorded as warnings instead.
    #[serde(default)]
    pub force: bool,

    /// Actor performing the operation (for audit logging)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,

    /// Reason for bypass (required if any skip_* flag is true)
    ///
    /// This field is logged for audit purposes when bypasses are used.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bypass_reason: Option<String>,
}

impl PreflightConfig {
    /// Create a new config with default (strict) settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a config with tenant context
    pub fn for_tenant(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: Some(tenant_id.into()),
            ..Default::default()
        }
    }

    /// Create a config with actor context
    pub fn with_actor(tenant_id: impl Into<String>, actor: impl Into<String>) -> Self {
        Self {
            tenant_id: Some(tenant_id.into()),
            actor: Some(actor.into()),
            ..Default::default()
        }
    }

    /// Check if any bypass flag is set
    pub fn has_any_bypass(&self) -> bool {
        self.skip_maintenance_check
            || self.skip_conflict_check
            || self.allow_training_state
            || self.force
    }

    /// Get list of active bypass flags (for audit logging)
    pub fn active_bypasses(&self) -> Vec<&'static str> {
        let mut bypasses = Vec::new();
        if self.skip_maintenance_check {
            bypasses.push("skip_maintenance_check");
        }
        if self.skip_conflict_check {
            bypasses.push("skip_conflict_check");
        }
        if self.allow_training_state {
            bypasses.push("allow_training_state");
        }
        if self.force {
            bypasses.push("force");
        }
        bypasses
    }

    /// Validate bypass configuration
    ///
    /// Returns an error if bypass flags are used without a reason.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.has_any_bypass() && self.bypass_reason.is_none() {
            return Err("bypass_reason required when using bypass flags");
        }
        Ok(())
    }

    /// Set maintenance check bypass
    pub fn skip_maintenance(mut self, reason: impl Into<String>) -> Self {
        self.skip_maintenance_check = true;
        self.bypass_reason = Some(reason.into());
        self
    }

    /// Set conflict check bypass
    pub fn skip_conflicts(mut self, reason: impl Into<String>) -> Self {
        self.skip_conflict_check = true;
        self.bypass_reason = Some(reason.into());
        self
    }

    /// Set force mode
    pub fn force_pass(mut self, reason: impl Into<String>) -> Self {
        self.force = true;
        self.bypass_reason = Some(reason.into());
        self
    }

    /// Set training state allowance
    pub fn allow_training(mut self) -> Self {
        self.allow_training_state = true;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_no_bypasses() {
        let config = PreflightConfig::new();
        assert!(!config.has_any_bypass());
        assert!(config.active_bypasses().is_empty());
    }

    #[test]
    fn test_bypass_detection() {
        let config = PreflightConfig::new().skip_maintenance("Emergency deployment");

        assert!(config.has_any_bypass());
        assert!(config.active_bypasses().contains(&"skip_maintenance_check"));
    }

    #[test]
    fn test_validation_requires_reason() {
        let config = PreflightConfig {
            skip_maintenance_check: true,
            bypass_reason: None,
            ..Default::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_passes_with_reason() {
        let config = PreflightConfig::new().skip_maintenance("Emergency deployment");

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_for_tenant() {
        let config = PreflightConfig::for_tenant("tenant-123");
        assert_eq!(config.tenant_id, Some("tenant-123".to_string()));
    }
}
