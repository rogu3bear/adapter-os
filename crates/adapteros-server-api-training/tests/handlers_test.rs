//! Integration tests for training handlers
//!
//! Tests for the exported handler functions including backend capability mapping,
//! availability checks, fallback selection, and backend planning.

use adapteros_api_types::training::TrainingCoremlReadiness;
use adapteros_lora_worker::backend_factory::BackendCapabilities;
use adapteros_server_api_training::{
    backend_available, build_coreml_readiness, canonical_trust_state, choose_auto_backend,
    choose_fallback, coreml_unavailable_reason, map_capabilities, plan_backend_readiness,
};
use adapteros_types::training::{TrainingBackendKind, TrainingBackendPolicy};

// ============================================================================
// Test Helpers
// ============================================================================

/// Create BackendCapabilities with all fields set
fn caps_all() -> BackendCapabilities {
    BackendCapabilities {
        has_coreml: true,
        has_ane: true,
        has_metal: true,
        has_mlx: true,
        has_mlx_bridge: true,
        metal_device_name: Some("Apple M1 Pro".to_string()),
        gpu_memory_bytes: Some(16 * 1024 * 1024 * 1024),
    }
}

/// Create BackendCapabilities with only CoreML/ANE
fn caps_coreml_only() -> BackendCapabilities {
    BackendCapabilities {
        has_coreml: true,
        has_ane: true,
        has_metal: false,
        has_mlx: false,
        has_mlx_bridge: false,
        metal_device_name: None,
        gpu_memory_bytes: None,
    }
}

/// Create BackendCapabilities with only MLX
fn caps_mlx_only() -> BackendCapabilities {
    BackendCapabilities {
        has_coreml: false,
        has_ane: false,
        has_metal: true,
        has_mlx: true,
        has_mlx_bridge: false,
        metal_device_name: Some("Apple M1".to_string()),
        gpu_memory_bytes: Some(8 * 1024 * 1024 * 1024),
    }
}

/// Create BackendCapabilities with only Metal
fn caps_metal_only() -> BackendCapabilities {
    BackendCapabilities {
        has_coreml: false,
        has_ane: false,
        has_metal: true,
        has_mlx: false,
        has_mlx_bridge: false,
        metal_device_name: Some("Apple M1".to_string()),
        gpu_memory_bytes: Some(8 * 1024 * 1024 * 1024),
    }
}

/// Create BackendCapabilities with nothing available
fn caps_none() -> BackendCapabilities {
    BackendCapabilities::default()
}

/// Create TrainingCoremlReadiness for testing
fn coreml_ready(available: bool, ane_available: bool) -> TrainingCoremlReadiness {
    TrainingCoremlReadiness {
        available,
        gpu_available: available,
        ane_available,
        compute_units_preference: None,
        compute_units_effective: None,
        gpu_used: false,
        ane_used: false,
        production_mode: false,
    }
}

// ============================================================================
// map_capabilities Tests
// ============================================================================

#[test]
fn map_capabilities_transfers_all_fields() {
    let caps = caps_all();
    let api_caps = map_capabilities(&caps);

    assert!(api_caps.has_coreml);
    assert!(api_caps.has_ane);
    assert!(api_caps.has_metal);
    assert!(api_caps.has_mlx);
    assert_eq!(api_caps.has_mlx_bridge, Some(true));
    assert_eq!(
        api_caps.metal_device_name,
        Some("Apple M1 Pro".to_string())
    );
    assert_eq!(api_caps.gpu_memory_bytes, Some(16 * 1024 * 1024 * 1024));
}

#[test]
fn map_capabilities_handles_none_values() {
    let caps = caps_none();
    let api_caps = map_capabilities(&caps);

    assert!(!api_caps.has_coreml);
    assert!(!api_caps.has_ane);
    assert!(!api_caps.has_metal);
    assert!(!api_caps.has_mlx);
    assert_eq!(api_caps.has_mlx_bridge, Some(false));
    assert!(api_caps.metal_device_name.is_none());
    assert!(api_caps.gpu_memory_bytes.is_none());
}

#[test]
fn map_capabilities_preserves_device_name() {
    let mut caps = caps_metal_only();
    caps.metal_device_name = Some("Apple M3 Max".to_string());

    let api_caps = map_capabilities(&caps);
    assert_eq!(api_caps.metal_device_name, Some("Apple M3 Max".to_string()));
}

// ============================================================================
// backend_available Tests
// ============================================================================

#[test]
fn backend_available_coreml_when_coreml_available() {
    let caps = caps_coreml_only();
    assert!(backend_available(
        TrainingBackendKind::CoreML,
        true,
        &caps,
        false
    ));
}

#[test]
fn backend_available_coreml_when_coreml_not_available() {
    let caps = caps_mlx_only();
    assert!(!backend_available(
        TrainingBackendKind::CoreML,
        false,
        &caps,
        false
    ));
}

#[test]
fn backend_available_mlx_when_mlx_available() {
    let caps = caps_mlx_only();
    assert!(backend_available(
        TrainingBackendKind::Mlx,
        false,
        &caps,
        false
    ));
}

#[test]
fn backend_available_mlx_when_mlx_not_available() {
    let caps = caps_coreml_only();
    assert!(!backend_available(
        TrainingBackendKind::Mlx,
        true,
        &caps,
        false
    ));
}

#[test]
fn backend_available_metal_when_metal_available() {
    let caps = caps_metal_only();
    assert!(backend_available(
        TrainingBackendKind::Metal,
        false,
        &caps,
        false
    ));
}

#[test]
fn backend_available_metal_when_metal_not_available() {
    let caps = caps_none();
    assert!(!backend_available(
        TrainingBackendKind::Metal,
        false,
        &caps,
        false
    ));
}

#[test]
fn backend_available_cpu_when_gpu_not_required() {
    let caps = caps_none();
    assert!(backend_available(
        TrainingBackendKind::Cpu,
        false,
        &caps,
        false
    ));
}

#[test]
fn backend_available_cpu_when_gpu_required() {
    let caps = caps_none();
    assert!(!backend_available(
        TrainingBackendKind::Cpu,
        false,
        &caps,
        true
    ));
}

#[test]
fn backend_available_auto_when_any_available() {
    let caps = caps_mlx_only();
    assert!(backend_available(
        TrainingBackendKind::Auto,
        false,
        &caps,
        false
    ));
}

#[test]
fn backend_available_auto_when_nothing_available_but_cpu_allowed() {
    let caps = caps_none();
    assert!(backend_available(
        TrainingBackendKind::Auto,
        false,
        &caps,
        false
    ));
}

#[test]
fn backend_available_auto_when_nothing_available_and_gpu_required() {
    let caps = caps_none();
    assert!(!backend_available(
        TrainingBackendKind::Auto,
        false,
        &caps,
        true
    ));
}

// ============================================================================
// choose_fallback Tests
// ============================================================================

#[test]
fn choose_fallback_respects_preferred_order() {
    let caps = caps_all();
    let fallback = choose_fallback(Some(TrainingBackendKind::Metal), true, &caps, false);
    // Metal is preferred and available
    assert_eq!(fallback, Some(TrainingBackendKind::Metal));
}

#[test]
fn choose_fallback_uses_mlx_first_when_no_preference() {
    let caps = caps_mlx_only();
    let fallback = choose_fallback(None, false, &caps, false);
    assert_eq!(fallback, Some(TrainingBackendKind::Mlx));
}

#[test]
fn choose_fallback_uses_metal_when_mlx_unavailable() {
    let caps = caps_metal_only();
    let fallback = choose_fallback(None, false, &caps, false);
    assert_eq!(fallback, Some(TrainingBackendKind::Metal));
}

#[test]
fn choose_fallback_uses_cpu_when_nothing_else_available() {
    let caps = caps_none();
    let fallback = choose_fallback(None, false, &caps, false);
    assert_eq!(fallback, Some(TrainingBackendKind::Cpu));
}

#[test]
fn choose_fallback_returns_none_when_gpu_required_and_nothing_available() {
    let caps = caps_none();
    let fallback = choose_fallback(None, false, &caps, true);
    assert!(fallback.is_none());
}

#[test]
fn choose_fallback_skips_unavailable_preferred() {
    let caps = caps_metal_only();
    // Prefer CoreML but it's not available
    let fallback = choose_fallback(Some(TrainingBackendKind::CoreML), false, &caps, false);
    // Should fall back to Metal (next available in order after preferred)
    assert_eq!(fallback, Some(TrainingBackendKind::Metal));
}

// ============================================================================
// choose_auto_backend Tests
// ============================================================================

#[test]
fn choose_auto_backend_prefers_coreml_when_available() {
    let caps = caps_all();
    let backend = choose_auto_backend(true, &caps, false);
    assert_eq!(backend, Some(TrainingBackendKind::CoreML));
}

#[test]
fn choose_auto_backend_uses_mlx_when_coreml_unavailable() {
    let caps = caps_mlx_only();
    let backend = choose_auto_backend(false, &caps, false);
    assert_eq!(backend, Some(TrainingBackendKind::Mlx));
}

#[test]
fn choose_auto_backend_uses_metal_when_mlx_unavailable() {
    let caps = caps_metal_only();
    let backend = choose_auto_backend(false, &caps, false);
    assert_eq!(backend, Some(TrainingBackendKind::Metal));
}

#[test]
fn choose_auto_backend_uses_cpu_as_last_resort() {
    let caps = caps_none();
    let backend = choose_auto_backend(false, &caps, false);
    assert_eq!(backend, Some(TrainingBackendKind::Cpu));
}

#[test]
fn choose_auto_backend_returns_none_when_gpu_required_and_nothing_available() {
    let caps = caps_none();
    let backend = choose_auto_backend(false, &caps, true);
    assert!(backend.is_none());
}

// ============================================================================
// coreml_unavailable_reason Tests
// ============================================================================

#[test]
fn coreml_unavailable_reason_ane_unavailable() {
    let caps = BackendCapabilities {
        has_coreml: true,
        has_ane: false,
        ..Default::default()
    };
    let coreml = coreml_ready(true, false);
    let reason = coreml_unavailable_reason(&caps, &coreml);
    assert_eq!(reason, "ane_unavailable");
}

#[test]
fn coreml_unavailable_reason_coreml_unavailable() {
    let caps = BackendCapabilities {
        has_coreml: false,
        has_ane: false,
        ..Default::default()
    };
    let coreml = coreml_ready(false, false);
    let reason = coreml_unavailable_reason(&caps, &coreml);
    assert_eq!(reason, "coreml_unavailable");
}

// ============================================================================
// plan_backend_readiness Tests
// ============================================================================

#[test]
fn plan_backend_readiness_coreml_only_policy_ready() {
    let caps = caps_all();
    let coreml = coreml_ready(true, true);
    let plan = plan_backend_readiness(
        TrainingBackendKind::CoreML,
        TrainingBackendPolicy::CoremlOnly,
        None,
        false,
        &caps,
        &coreml,
    );

    assert!(plan.ready);
    assert_eq!(plan.resolved_backend, TrainingBackendKind::CoreML);
    assert!(plan.fallback_backend.is_none());
    assert!(plan.fallback_reason.is_none());
}

#[test]
fn plan_backend_readiness_coreml_only_policy_not_ready() {
    let caps = caps_mlx_only();
    let coreml = coreml_ready(false, false);
    let plan = plan_backend_readiness(
        TrainingBackendKind::CoreML,
        TrainingBackendPolicy::CoremlOnly,
        None,
        false,
        &caps,
        &coreml,
    );

    assert!(!plan.ready);
    assert_eq!(
        plan.fallback_reason.as_deref(),
        Some("coreml_required_unavailable")
    );
    assert!(!plan.warnings.is_empty());
}

#[test]
fn plan_backend_readiness_coreml_else_fallback_uses_coreml() {
    let caps = caps_all();
    let coreml = coreml_ready(true, true);
    let plan = plan_backend_readiness(
        TrainingBackendKind::CoreML,
        TrainingBackendPolicy::CoremlElseFallback,
        Some(TrainingBackendKind::Mlx),
        false,
        &caps,
        &coreml,
    );

    assert!(plan.ready);
    assert_eq!(plan.resolved_backend, TrainingBackendKind::CoreML);
    assert!(plan.fallback_backend.is_none());
}

#[test]
fn plan_backend_readiness_coreml_else_fallback_falls_back() {
    let caps = caps_mlx_only();
    let coreml = coreml_ready(false, false);
    let plan = plan_backend_readiness(
        TrainingBackendKind::CoreML,
        TrainingBackendPolicy::CoremlElseFallback,
        Some(TrainingBackendKind::Mlx),
        false,
        &caps,
        &coreml,
    );

    assert!(plan.ready);
    assert_eq!(plan.resolved_backend, TrainingBackendKind::Mlx);
    assert_eq!(plan.fallback_backend, Some(TrainingBackendKind::Mlx));
    assert_eq!(
        plan.fallback_reason.as_deref(),
        Some("coreml_policy_fallback")
    );
}

#[test]
fn plan_backend_readiness_coreml_else_fallback_no_backend() {
    let caps = caps_none();
    let coreml = coreml_ready(false, false);
    let plan = plan_backend_readiness(
        TrainingBackendKind::CoreML,
        TrainingBackendPolicy::CoremlElseFallback,
        None,
        true, // require GPU, so CPU fallback not allowed
        &caps,
        &coreml,
    );

    assert!(!plan.ready);
    assert_eq!(
        plan.fallback_reason.as_deref(),
        Some("coreml_policy_no_backend")
    );
}

#[test]
fn plan_backend_readiness_auto_policy_coreml_request_ready() {
    let caps = caps_all();
    let coreml = coreml_ready(true, true);
    let plan = plan_backend_readiness(
        TrainingBackendKind::CoreML,
        TrainingBackendPolicy::Auto,
        None,
        false,
        &caps,
        &coreml,
    );

    assert!(plan.ready);
    assert_eq!(plan.resolved_backend, TrainingBackendKind::CoreML);
}

#[test]
fn plan_backend_readiness_auto_policy_coreml_request_falls_back() {
    let caps = caps_mlx_only();
    let coreml = coreml_ready(false, false);
    let plan = plan_backend_readiness(
        TrainingBackendKind::CoreML,
        TrainingBackendPolicy::Auto,
        Some(TrainingBackendKind::Mlx),
        false,
        &caps,
        &coreml,
    );

    assert!(plan.ready);
    assert_eq!(plan.resolved_backend, TrainingBackendKind::Mlx);
    assert_eq!(plan.fallback_backend, Some(TrainingBackendKind::Mlx));
    assert_eq!(plan.fallback_reason.as_deref(), Some("coreml_unavailable"));
}

#[test]
fn plan_backend_readiness_auto_policy_auto_request() {
    let caps = caps_all();
    let coreml = coreml_ready(true, true);
    let plan = plan_backend_readiness(
        TrainingBackendKind::Auto,
        TrainingBackendPolicy::Auto,
        None,
        false,
        &caps,
        &coreml,
    );

    assert!(plan.ready);
    // Auto should select CoreML when available
    assert_eq!(plan.resolved_backend, TrainingBackendKind::CoreML);
}

#[test]
fn plan_backend_readiness_auto_policy_mlx_request() {
    let caps = caps_mlx_only();
    let coreml = coreml_ready(false, false);
    let plan = plan_backend_readiness(
        TrainingBackendKind::Mlx,
        TrainingBackendPolicy::Auto,
        None,
        false,
        &caps,
        &coreml,
    );

    assert!(plan.ready);
    assert_eq!(plan.resolved_backend, TrainingBackendKind::Mlx);
}

#[test]
fn plan_backend_readiness_auto_policy_unavailable_request() {
    let caps = caps_metal_only();
    let coreml = coreml_ready(false, false);
    let plan = plan_backend_readiness(
        TrainingBackendKind::Mlx, // Request MLX but it's unavailable
        TrainingBackendPolicy::Auto,
        None,
        false,
        &caps,
        &coreml,
    );

    assert!(!plan.ready);
    assert_eq!(
        plan.fallback_reason.as_deref(),
        Some("requested_backend_unavailable")
    );
}

#[test]
fn plan_backend_readiness_resolved_backend_unavailable() {
    // Edge case: resolved backend becomes unavailable after initial selection
    let caps = caps_none();
    let coreml = coreml_ready(false, false);
    let plan = plan_backend_readiness(
        TrainingBackendKind::Auto,
        TrainingBackendPolicy::Auto,
        None,
        true, // require GPU
        &caps,
        &coreml,
    );

    assert!(!plan.ready);
    assert_eq!(
        plan.fallback_reason.as_deref(),
        Some("no_backend_available")
    );
}

// ============================================================================
// build_coreml_readiness Tests
// ============================================================================

#[test]
fn build_coreml_readiness_from_capabilities() {
    let caps = caps_all();
    let readiness = build_coreml_readiness(&caps);

    assert!(readiness.available);
    assert!(readiness.gpu_available);
    assert!(readiness.ane_available);
}

#[test]
fn build_coreml_readiness_from_minimal_capabilities() {
    let caps = caps_none();
    let readiness = build_coreml_readiness(&caps);

    assert!(!readiness.available);
    // GPU availability depends on capabilities
    assert!(!readiness.ane_available);
}

// ============================================================================
// canonical_trust_state Tests
// ============================================================================

#[test]
fn canonical_trust_state_normalizes_allowed() {
    assert_eq!(canonical_trust_state("allowed"), "allowed");
    assert_eq!(canonical_trust_state("ALLOWED"), "allowed");
    assert_eq!(canonical_trust_state("Allowed"), "allowed");
}

#[test]
fn canonical_trust_state_normalizes_warn() {
    assert_eq!(canonical_trust_state("warn"), "allowed_with_warning");
    assert_eq!(canonical_trust_state("WARN"), "allowed_with_warning");
    assert_eq!(
        canonical_trust_state("allowed_with_warning"),
        "allowed_with_warning"
    );
}

#[test]
fn canonical_trust_state_normalizes_blocked() {
    assert_eq!(canonical_trust_state("blocked"), "blocked");
    assert_eq!(canonical_trust_state("blocked_regressed"), "blocked");
    assert_eq!(canonical_trust_state("BLOCKED_REGRESSED"), "blocked");
}

#[test]
fn canonical_trust_state_normalizes_needs_approval() {
    assert_eq!(canonical_trust_state("needs_approval"), "needs_approval");
    assert_eq!(canonical_trust_state("NEEDS_APPROVAL"), "needs_approval");
}

#[test]
fn canonical_trust_state_normalizes_unknown() {
    assert_eq!(canonical_trust_state("unknown"), "unknown");
    assert_eq!(canonical_trust_state("Unknown"), "unknown");
}

#[test]
fn canonical_trust_state_rejects_invalid() {
    assert_eq!(canonical_trust_state("invalid_state"), "unknown");
    assert_eq!(canonical_trust_state("random"), "unknown");
    assert_eq!(canonical_trust_state(""), "unknown");
}

#[test]
fn canonical_trust_state_handles_whitespace() {
    assert_eq!(canonical_trust_state("  allowed  "), "allowed");
    assert_eq!(canonical_trust_state("\tblocked\n"), "blocked");
}
