use adapteros_api_types::TenantExecutionPolicy;
use adapteros_core::backend::BackendKind;
use adapteros_core::{AosError, Result};

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
}
