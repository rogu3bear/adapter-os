use adapteros_api_types::TenantExecutionPolicy;
use adapteros_core::backend::BackendKind;
use adapteros_core::ExecutionProfile;
use adapteros_policy::enforce_backend_policy;

use crate::state::ApiConfig;
use crate::types::{InferenceError, InferenceRequestInternal};

/// Resolution result for a request-scoped execution profile.
pub struct ExecutionProfileResolution {
    pub profile: ExecutionProfile,
    /// Backend requested by API/config before policy enforcement.
    pub requested_backend: BackendKind,
    /// Backend configured at the control-plane level (used for logging overrides).
    pub default_backend: BackendKind,
}

/// Resolve the canonical ExecutionProfile (seed_mode + backend_profile) for a request.
///
/// - Pulls overrides from the request when provided.
/// - Applies tenant execution policy allow/deny rules for backends.
/// - Returns an ExecutionProfile that should be used as the single source of truth.
pub fn resolve_execution_profile(
    request: &InferenceRequestInternal,
    config: &ApiConfig,
    policy: &TenantExecutionPolicy,
) -> Result<ExecutionProfileResolution, InferenceError> {
    let seed_mode = request.seed_mode.unwrap_or(config.seed_mode);
    let requested_backend = request.backend_profile.unwrap_or(config.backend_profile);

    enforce_backend_policy(policy, requested_backend).map_err(|e| {
        InferenceError::PermissionDenied(format!(
            "backend {} not permitted: {}",
            requested_backend.as_str(),
            e
        ))
    })?;

    let profile = ExecutionProfile {
        seed_mode,
        backend_profile: requested_backend,
        require_explicit_fallback_opt_out: false,
    };

    Ok(ExecutionProfileResolution {
        profile,
        requested_backend,
        default_backend: config.backend_profile,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::SeedMode;

    #[test]
    fn backend_override_resolves_when_policy_allows() {
        let mut config = ApiConfig::default();
        config.backend_profile = BackendKind::Metal;
        config.seed_mode = SeedMode::BestEffort;

        let mut request =
            InferenceRequestInternal::new("tenant-a".to_string(), "prompt".to_string());
        request.backend_profile = Some(BackendKind::CoreML);

        let policy = TenantExecutionPolicy::permissive_default("tenant-a");

        let resolved = resolve_execution_profile(&request, &config, &policy).unwrap();
        assert_eq!(resolved.profile.backend_profile, BackendKind::CoreML);
        assert_eq!(resolved.requested_backend, BackendKind::CoreML);
        assert_eq!(resolved.profile.seed_mode, SeedMode::BestEffort);
    }
}
