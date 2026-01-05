use adapteros_api_types::TenantExecutionPolicy;
use adapteros_core::backend::BackendKind;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

// =============================================================================
// BackendDowngradePolicy - Policy for handling backend fallback scenarios
// =============================================================================

/// Policy controlling backend downgrade/fallback behavior.
///
/// When a requested backend (e.g., Metal GPU) is unavailable, this policy
/// determines whether fallback to a slower backend (e.g., CPU) is permitted
/// and what audit/telemetry requirements apply.
///
/// # Invariants
///
/// 1. **Explicit consent**: Downgrades only occur when policy permits
/// 2. **Audit trail**: All downgrades emit telemetry events
/// 3. **Latency awareness**: Policy can enforce latency impact thresholds
///
/// # Example
///
/// ```ignore
/// use adapteros_policy::BackendDowngradePolicy;
///
/// let policy = BackendDowngradePolicy::strict();
/// assert!(!policy.allow_silent_downgrade);
///
/// let policy = BackendDowngradePolicy::permissive();
/// assert!(policy.allow_gpu_to_cpu_fallback);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendDowngradePolicy {
    /// Allow automatic fallback from GPU to CPU when GPU is unavailable
    pub allow_gpu_to_cpu_fallback: bool,

    /// Allow fallback from ANE/CoreML to Metal
    pub allow_ane_to_metal_fallback: bool,

    /// Allow fallback from Metal to MLX (CPU path)
    pub allow_metal_to_mlx_fallback: bool,

    /// If true, downgrades happen silently without telemetry.
    /// WARNING: Setting this to true violates audit requirements.
    pub allow_silent_downgrade: bool,

    /// Maximum acceptable latency multiplier for fallback.
    /// E.g., 2.0 means fallback is rejected if it would be >2x slower.
    /// None means no latency threshold (always allow if otherwise permitted).
    pub max_latency_multiplier: Option<f32>,

    /// Require user/operator acknowledgment before downgrade
    pub require_acknowledgment: bool,

    /// Log level for downgrade events ("error", "warn", "info", "debug")
    pub downgrade_log_level: String,
}

impl BackendDowngradePolicy {
    /// Create a strict policy: no silent downgrades, all require acknowledgment.
    pub fn strict() -> Self {
        Self {
            allow_gpu_to_cpu_fallback: false,
            allow_ane_to_metal_fallback: false,
            allow_metal_to_mlx_fallback: false,
            allow_silent_downgrade: false,
            max_latency_multiplier: Some(1.5),
            require_acknowledgment: true,
            downgrade_log_level: "error".to_string(),
        }
    }

    /// Create a permissive policy: allow fallbacks with logging.
    pub fn permissive() -> Self {
        Self {
            allow_gpu_to_cpu_fallback: true,
            allow_ane_to_metal_fallback: true,
            allow_metal_to_mlx_fallback: true,
            allow_silent_downgrade: false,
            max_latency_multiplier: None,
            require_acknowledgment: false,
            downgrade_log_level: "warn".to_string(),
        }
    }

    /// Create a development policy: allow everything for local testing.
    pub fn development() -> Self {
        Self {
            allow_gpu_to_cpu_fallback: true,
            allow_ane_to_metal_fallback: true,
            allow_metal_to_mlx_fallback: true,
            allow_silent_downgrade: false, // Still audit even in dev
            max_latency_multiplier: None,
            require_acknowledgment: false,
            downgrade_log_level: "info".to_string(),
        }
    }

    /// Check if a specific downgrade path is permitted.
    ///
    /// # Arguments
    /// * `from` - The requested backend
    /// * `to` - The fallback backend
    ///
    /// # Returns
    /// `Ok(())` if downgrade is permitted, `Err` with reason otherwise.
    pub fn check_downgrade(&self, from: BackendKind, to: BackendKind) -> Result<()> {
        // Same backend is not a downgrade
        if from == to {
            return Ok(());
        }

        // Check specific downgrade paths
        let permitted = match (from, to) {
            // GPU to CPU fallback
            (BackendKind::Metal, BackendKind::CPU) | (BackendKind::Mlx, BackendKind::CPU) => {
                self.allow_gpu_to_cpu_fallback
            }
            // ANE to Metal fallback
            (BackendKind::CoreML, BackendKind::Metal) => self.allow_ane_to_metal_fallback,
            // Metal to MLX (which may use CPU path)
            (BackendKind::Metal, BackendKind::Mlx) => self.allow_metal_to_mlx_fallback,
            // Auto can go anywhere
            (BackendKind::Auto, _) => true,
            // Anything else is a potential downgrade - be conservative
            _ => false,
        };

        if !permitted {
            return Err(AosError::PolicyViolation(format!(
                "Backend downgrade from {} to {} is not permitted by policy",
                from.as_str(),
                to.as_str()
            )));
        }

        Ok(())
    }

    /// Validate that the policy does not violate audit requirements.
    ///
    /// This function checks for dangerous configuration that should never
    /// be used in production:
    /// - `allow_silent_downgrade=true` bypasses mandatory audit logging
    ///
    /// # Arguments
    /// * `production_mode` - Whether the server is running in production mode
    ///
    /// # Returns
    /// `Ok(())` if policy is safe, `Err` if it violates audit requirements.
    ///
    /// # Security
    /// This validation is critical for compliance. Silent downgrades would
    /// allow backend changes without audit trail, violating determinism
    /// guarantees.
    pub fn validate_audit_compliance(&self, production_mode: bool) -> Result<()> {
        if self.allow_silent_downgrade {
            if production_mode {
                return Err(AosError::PolicyViolation(
                    "AUDIT VIOLATION: allow_silent_downgrade=true is forbidden in production. \
                     Silent downgrades bypass mandatory audit logging and violate determinism \
                     guarantees. Remove this setting from your configuration."
                        .to_string(),
                ));
            } else {
                // In development mode, log a warning but allow
                tracing::warn!(
                    "allow_silent_downgrade=true detected - this would be blocked in production"
                );
            }
        }
        Ok(())
    }

    /// Check if latency impact is acceptable.
    ///
    /// # Arguments
    /// * `original_latency_ms` - Expected latency with original backend
    /// * `fallback_latency_ms` - Expected latency with fallback backend
    ///
    /// # Returns
    /// `Ok(())` if latency is acceptable, `Err` if it exceeds threshold.
    pub fn check_latency_impact(
        &self,
        original_latency_ms: f32,
        fallback_latency_ms: f32,
    ) -> Result<()> {
        if let Some(max_multiplier) = self.max_latency_multiplier {
            if original_latency_ms > 0.0 {
                let actual_multiplier = fallback_latency_ms / original_latency_ms;
                if actual_multiplier > max_multiplier {
                    return Err(AosError::PolicyViolation(format!(
                        "Backend fallback latency impact {:.1}x exceeds max allowed {:.1}x",
                        actual_multiplier, max_multiplier
                    )));
                }
            }
        }
        Ok(())
    }
}

impl Default for BackendDowngradePolicy {
    fn default() -> Self {
        Self::permissive()
    }
}

impl std::fmt::Display for BackendDowngradePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BackendDowngradePolicy(gpu_to_cpu={}, silent={}, ack_required={})",
            self.allow_gpu_to_cpu_fallback,
            self.allow_silent_downgrade,
            self.require_acknowledgment
        )
    }
}

/// Enforce backend allow/deny rules from the tenant execution policy.
///
/// - Deny list takes precedence over allow list.
/// - When no lists are provided, all backends are permitted.
pub fn enforce_backend_policy(
    policy: &TenantExecutionPolicy,
    requested: BackendKind,
) -> Result<()> {
    if let Some(denied) = &policy.determinism.denied_backends {
        if denied.contains(&requested) {
            return Err(AosError::PolicyViolation(format!(
                "Backend {} is denied by tenant policy",
                requested.as_str()
            )));
        }
    }

    if let Some(allowed) = &policy.determinism.allowed_backends {
        // Auto is treated as "any allowed backend" so only check when explicit.
        if requested != BackendKind::Auto && !allowed.contains(&requested) {
            return Err(AosError::PolicyViolation(format!(
                "Backend {} is not in the allowed set: {}",
                requested.as_str(),
                allowed
                    .iter()
                    .map(BackendKind::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_api_types::{CreateExecutionPolicyRequest, DeterminismPolicy};

    fn permissive_policy() -> TenantExecutionPolicy {
        TenantExecutionPolicy::permissive_default("tenant-1")
    }

    #[test]
    fn allows_when_no_lists() {
        let policy = permissive_policy();
        assert!(enforce_backend_policy(&policy, BackendKind::CoreML).is_ok());
    }

    #[test]
    fn denies_when_denied_list_matches() {
        let mut determinism = DeterminismPolicy::default();
        determinism.denied_backends = Some(vec![BackendKind::CoreML]);
        let request = CreateExecutionPolicyRequest {
            determinism,
            routing: None,
            golden: None,
            require_signed_adapters: false,
        };
        let mut policy = permissive_policy();
        policy.determinism = request.determinism;

        let err = enforce_backend_policy(&policy, BackendKind::CoreML).unwrap_err();
        assert!(err.to_string().contains("denied"));
    }

    #[test]
    fn allows_only_allowed_set() {
        let mut determinism = DeterminismPolicy::default();
        determinism.allowed_backends = Some(vec![BackendKind::Metal]);
        let request = CreateExecutionPolicyRequest {
            determinism,
            routing: None,
            golden: None,
            require_signed_adapters: false,
        };
        let mut policy = permissive_policy();
        policy.determinism = request.determinism;

        assert!(enforce_backend_policy(&policy, BackendKind::Metal).is_ok());
        let err = enforce_backend_policy(&policy, BackendKind::CoreML).unwrap_err();
        assert!(err.to_string().contains("allowed set"));
    }

    // ==========================================================================
    // BackendDowngradePolicy tests
    // ==========================================================================

    #[test]
    fn downgrade_policy_strict_defaults() {
        let policy = BackendDowngradePolicy::strict();
        assert!(!policy.allow_gpu_to_cpu_fallback);
        assert!(!policy.allow_silent_downgrade);
        assert!(policy.require_acknowledgment);
        assert_eq!(policy.max_latency_multiplier, Some(1.5));
    }

    #[test]
    fn downgrade_policy_permissive_defaults() {
        let policy = BackendDowngradePolicy::permissive();
        assert!(policy.allow_gpu_to_cpu_fallback);
        assert!(!policy.allow_silent_downgrade);
        assert!(!policy.require_acknowledgment);
        assert_eq!(policy.max_latency_multiplier, None);
    }

    #[test]
    fn downgrade_policy_check_same_backend() {
        let policy = BackendDowngradePolicy::strict();
        // Same backend is not a downgrade
        assert!(policy
            .check_downgrade(BackendKind::Metal, BackendKind::Metal)
            .is_ok());
    }

    #[test]
    fn downgrade_policy_strict_blocks_gpu_to_cpu() {
        let policy = BackendDowngradePolicy::strict();
        let err = policy
            .check_downgrade(BackendKind::Metal, BackendKind::CPU)
            .unwrap_err();
        assert!(err.to_string().contains("not permitted"));
    }

    #[test]
    fn downgrade_policy_permissive_allows_gpu_to_cpu() {
        let policy = BackendDowngradePolicy::permissive();
        assert!(policy
            .check_downgrade(BackendKind::Metal, BackendKind::CPU)
            .is_ok());
    }

    #[test]
    fn downgrade_policy_latency_check_passes() {
        let policy = BackendDowngradePolicy::strict();
        // 1.2x is under 1.5x threshold
        assert!(policy.check_latency_impact(100.0, 120.0).is_ok());
    }

    #[test]
    fn downgrade_policy_latency_check_fails() {
        let policy = BackendDowngradePolicy::strict();
        // 2.0x exceeds 1.5x threshold
        let err = policy.check_latency_impact(100.0, 200.0).unwrap_err();
        assert!(err.to_string().contains("exceeds max allowed"));
    }

    #[test]
    fn downgrade_policy_no_latency_threshold() {
        let policy = BackendDowngradePolicy::permissive();
        // No threshold set, any latency is acceptable
        assert!(policy.check_latency_impact(100.0, 1000.0).is_ok());
    }

    #[test]
    fn downgrade_policy_serialization_roundtrip() {
        let policy = BackendDowngradePolicy::strict();
        let json = serde_json::to_string(&policy).expect("serialize");
        let deserialized: BackendDowngradePolicy =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(policy, deserialized);
    }

    // ==========================================================================
    // Audit compliance tests (#160)
    // ==========================================================================

    #[test]
    fn audit_compliance_passes_when_silent_downgrade_false() {
        let policy = BackendDowngradePolicy::strict();
        assert!(!policy.allow_silent_downgrade);
        assert!(policy.validate_audit_compliance(true).is_ok());
        assert!(policy.validate_audit_compliance(false).is_ok());
    }

    #[test]
    fn audit_compliance_fails_in_production_when_silent_downgrade_true() {
        let mut policy = BackendDowngradePolicy::strict();
        policy.allow_silent_downgrade = true;

        // Should fail in production mode
        let err = policy.validate_audit_compliance(true).unwrap_err();
        assert!(err.to_string().contains("AUDIT VIOLATION"));
        assert!(err.to_string().contains("allow_silent_downgrade"));
    }

    #[test]
    fn audit_compliance_warns_in_dev_when_silent_downgrade_true() {
        let mut policy = BackendDowngradePolicy::strict();
        policy.allow_silent_downgrade = true;

        // Should pass in development mode (just warns)
        assert!(policy.validate_audit_compliance(false).is_ok());
    }

    #[test]
    fn all_factory_methods_have_silent_downgrade_false() {
        // Verify all factory methods set allow_silent_downgrade=false
        assert!(!BackendDowngradePolicy::strict().allow_silent_downgrade);
        assert!(!BackendDowngradePolicy::permissive().allow_silent_downgrade);
        assert!(!BackendDowngradePolicy::development().allow_silent_downgrade);
        assert!(!BackendDowngradePolicy::default().allow_silent_downgrade);
    }
}
