//! Policy resolution for tenant execution policies.
//!
//! This module handles resolution of execution policies combining determinism,
//! routing, and golden-run policies into a unified source of truth for the
//! inference path.

use super::determinism::{compute_strict_mode, resolve_determinism_mode};
use crate::types::InferenceError;
use adapteros_core::determinism_mode::DeterminismMode;
use adapteros_types::coreml::CoreMLMode;

/// Resolved routing policy knobs
#[derive(Debug, Clone, Default)]
pub struct RoutingPolicyResolved {
    /// Whether to use session's stack_id when no explicit stack is provided
    /// Enforced in resolve_effective_adapters() at line ~848
    pub use_session_stack_for_routing: bool,
    /// Whether pins outside effective set are allowed (always false per Bundle A)
    pub allow_pins_outside_effective_set: bool,
}

/// Resolved golden-run policy knobs
#[derive(Debug, Clone)]
pub struct GoldenPolicyResolved {
    /// Whether to fail inference when golden drift is detected
    /// Enforced in check_golden_drift() after worker response
    pub fail_on_drift: bool,
    /// Golden baseline ID to compare against (if any)
    pub golden_baseline_id: Option<String>,
    /// Epsilon threshold for floating-point comparison of gate values
    /// Note: Current implementation only checks adapter selection/order
    /// Gate epsilon comparison requires worker to return detailed routing decisions
    pub epsilon_threshold: f64,
}

impl Default for GoldenPolicyResolved {
    fn default() -> Self {
        Self {
            fail_on_drift: false,
            golden_baseline_id: None,
            epsilon_threshold: 1e-6,
        }
    }
}

/// Resolved execution policy combining all policy dimensions
///
/// This struct unifies the policy resolution for a tenant's inference request,
/// combining determinism, routing, and golden-run policies into a single source
/// of truth for the inference path.
///
/// # Policy Enforcement
///
/// All policies are actively enforced during inference:
/// - **Determinism**: Mode and strict_mode enforced at worker call (line ~524)
/// - **Routing**: use_session_stack_for_routing enforced in resolve_effective_adapters() (line ~848)
/// - **Golden**: fail_on_drift enforced in check_golden_drift() after worker response (line ~552)
#[derive(Debug, Clone)]
pub struct ExecutionPolicyResolved {
    /// The underlying tenant execution policy
    pub policy: adapteros_api_types::TenantExecutionPolicy,
    /// The effective determinism mode after stack > tenant > global resolution
    pub effective_determinism_mode: DeterminismMode,
    /// Whether strict mode is active (for worker/coordinator behavior)
    pub strict_mode: bool,
    /// CoreML mode applied to backend selection for this request
    pub coreml_mode: CoreMLMode,
    /// Resolved routing policy knobs (enforced)
    pub routing: RoutingPolicyResolved,
    /// Resolved golden-run policy knobs (enforced)
    pub golden: GoldenPolicyResolved,
}

/// Resolve execution policy for a tenant's inference request
///
/// Combines:
/// - Database-stored execution policy (determinism, routing, golden)
/// - Config-level defaults (use_session_stack_for_routing, global determinism mode)
/// - Stack-level overrides (determinism_mode on the stack)
///
/// Returns a unified ExecutionPolicyResolved that can be used throughout the
/// inference path.
pub async fn resolve_tenant_execution_policy(
    db: &adapteros_db::Db,
    config: &crate::state::ApiConfig,
    tenant_id: &str,
    stack_determinism_mode: Option<&str>,
    coreml_mode: Option<CoreMLMode>,
) -> Result<ExecutionPolicyResolved, InferenceError> {
    // 1. Fetch tenant execution policy (or permissive default)
    let policy = db
        .get_execution_policy_or_default(tenant_id)
        .await
        .map_err(|e| {
            InferenceError::WorkerError(format!("Failed to load execution policy: {}", e))
        })?;

    // 2. Get global determinism mode from config
    let global_mode = config
        .general
        .as_ref()
        .and_then(|g| g.determinism_mode)
        // Default to strict to avoid relaxed/best-effort slipping in implicitly.
        .unwrap_or(DeterminismMode::Strict);

    // Use tenant policy's default_mode only for explicit policies.
    // Implicit/default policies should fall through to global mode.
    let tenant_mode = if policy.is_implicit {
        None
    } else {
        Some(policy.determinism.default_mode.as_str())
    };

    // 3. Resolve determinism mode (stack > tenant > global)
    let effective_determinism_mode =
        resolve_determinism_mode(stack_determinism_mode, tenant_mode, global_mode.as_str());

    // 4. Compute strict mode
    let coreml_mode = coreml_mode.unwrap_or(CoreMLMode::CoremlPreferred);
    let allow_backend_fallback =
        policy.determinism.allow_fallback && coreml_mode != CoreMLMode::CoremlStrict;

    let strict_mode = compute_strict_mode(effective_determinism_mode, allow_backend_fallback);

    // 5. Resolve routing policy knobs
    let routing = RoutingPolicyResolved {
        use_session_stack_for_routing: config.use_session_stack_for_routing,
        // Per Bundle A: pins outside effective set are never allowed
        allow_pins_outside_effective_set: false,
    };

    // 6. Resolve golden policy knobs from policy or defaults
    let golden = if let Some(ref golden_policy) = policy.golden {
        GoldenPolicyResolved {
            fail_on_drift: golden_policy.fail_on_drift,
            golden_baseline_id: golden_policy.golden_baseline_id.clone(),
            epsilon_threshold: golden_policy.epsilon_threshold,
        }
    } else {
        GoldenPolicyResolved::default()
    };

    Ok(ExecutionPolicyResolved {
        policy,
        effective_determinism_mode,
        strict_mode,
        routing,
        golden,
        coreml_mode,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_routing_policy_resolved_defaults() {
        let defaults = RoutingPolicyResolved::default();
        assert!(!defaults.use_session_stack_for_routing);
        assert!(!defaults.allow_pins_outside_effective_set);
    }

    #[test]
    fn test_golden_policy_resolved_defaults() {
        let defaults = GoldenPolicyResolved::default();
        assert!(!defaults.fail_on_drift);
        assert!(defaults.golden_baseline_id.is_none());
        assert!((defaults.epsilon_threshold - 1e-6).abs() < 1e-12);
    }
}
