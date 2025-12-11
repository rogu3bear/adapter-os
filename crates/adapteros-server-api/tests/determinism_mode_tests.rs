//! Determinism mode tests
//!
//! Tests for determinism mode resolution, strict mode validation, and replay guarantee computation.
//!
//! # Architecture Note
//!
//! Strict mode enforcement has two levels:
//!
//! 1. **Control Plane Level**:
//!    - `validate_strict_mode_constraints()` rejects requests without seed in strict mode
//!    - `resolve_determinism_mode()` resolves Stack > Tenant > Global hierarchy
//!    - `compute_replay_guarantee()` computes guarantee based on mode/fallback/truncation
//!    - `compute_strict_mode()` determines if fallback should be disabled
//!
//! 2. **Worker Level**:
//!    - `KernelWrapper::Coordinated` wraps primary + fallback backends
//!    - `strict_mode` flag on `InferenceRequest` controls fallback behavior
//!    - `Worker::infer_internal()` calls `kernels.set_strict_mode(request.strict_mode)`
//!    - `KernelWrapper::run_step()` returns error immediately if strict and primary fails
//!    - See `aos_worker.rs` lines 410-414 for KernelWrapper creation
//!    - See `lib.rs` lines 1155-1160 for strict_mode application
//!
//! Worker-level tests are in `crates/adapteros-lora-worker/tests/worker_enforcement_tests.rs`
//!
//! # Manual Testing
//!
//! ## Prerequisites
//!
//! **For Scenario 2 (strict mode validation error):**
//! ```bash
//! make dev  # Start control plane only (port 8080, NO_AUTH=1)
//! ```
//!
//! **For Scenarios 1 and 3 (full inference):**
//! Requires worker with loaded model. See `make full-stack` or equivalent.
//!
//! ## Scenario 1: Relaxed mode, no seed (requires worker)
//! ```bash
//! curl -X POST http://localhost:8080/v1/infer \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "prompt": "Hello",
//!     "max_tokens": 10,
//!     "determinism_mode": "relaxed"
//!   }'
//! # Expected: 200 OK, replay_guarantee: "none"
//! # Without worker: 503 "No healthy workers available"
//! ```
//!
//! ## Scenario 2: Strict mode, no seed (control plane only)
//! ```bash
//! curl -X POST http://localhost:8080/v1/infer \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "prompt": "Hello",
//!     "max_tokens": 10,
//!     "determinism_mode": "strict"
//!   }'
//! # Expected: 400, error: "Strict determinism mode requires a seed"
//! # This validation happens BEFORE worker dispatch, so no worker needed.
//! ```
//!
//! ## Scenario 3: Strict mode with seed (requires worker)
//! ```bash
//! curl -X POST http://localhost:8080/v1/infer \
//!   -H "Content-Type: application/json" \
//!   -d '{
//!     "prompt": "Hello",
//!     "max_tokens": 10,
//!     "determinism_mode": "strict",
//!     "seed": 12345
//!   }'
//! # Expected: 200 OK, replay_guarantee: "exact"
//! # Without worker: 503 "No healthy workers available"
//! ```
//!
//! ## Running Tests
//! ```bash
//! # Control plane tests
//! cargo test -p adapteros-server-api -- determinism_mode
//!
//! # Worker strict mode tests
//! cargo test -p adapteros-lora-worker --test worker_enforcement_tests -- strict_mode
//!
//! # All determinism tests
//! cargo test --workspace -- determinism
//! ```

use adapteros_api_types::inference::ReplayGuarantee;
use adapteros_server_api::inference_core::{
    compute_replay_guarantee, resolve_determinism_mode, validate_strict_mode_constraints,
    DeterminismMode,
};

// =============================================================================
// Determinism Mode Resolution Tests
// =============================================================================

#[test]
fn test_resolve_mode_stack_overrides_all() {
    // Stack setting takes highest priority
    let mode = resolve_determinism_mode(Some("relaxed"), Some("strict"), "besteffort");
    assert_eq!(mode, DeterminismMode::Relaxed);
}

#[test]
fn test_resolve_mode_tenant_overrides_global() {
    // Tenant setting overrides global when stack is not set
    let mode = resolve_determinism_mode(None, Some("strict"), "besteffort");
    assert_eq!(mode, DeterminismMode::Strict);
}

#[test]
fn test_resolve_mode_global_fallback() {
    // Global default when neither stack nor tenant is set
    let mode = resolve_determinism_mode(None, None, "relaxed");
    assert_eq!(mode, DeterminismMode::Relaxed);
}

#[test]
fn test_resolve_mode_besteffort_variations() {
    // Various spellings of besteffort
    assert_eq!(
        DeterminismMode::from("besteffort"),
        DeterminismMode::BestEffort
    );
    assert_eq!(
        DeterminismMode::from("best_effort"),
        DeterminismMode::BestEffort
    );
    assert_eq!(
        DeterminismMode::from("best-effort"),
        DeterminismMode::BestEffort
    );
    assert_eq!(
        DeterminismMode::from("BestEffort"),
        DeterminismMode::BestEffort
    );
}

#[test]
fn test_mode_as_str() {
    assert_eq!(DeterminismMode::Strict.as_str(), "strict");
    assert_eq!(DeterminismMode::BestEffort.as_str(), "besteffort");
    assert_eq!(DeterminismMode::Relaxed.as_str(), "relaxed");
}

// =============================================================================
// Strict Mode Validation Tests
// =============================================================================

#[test]
fn test_strict_mode_requires_seed() {
    // Strict mode without seed should fail
    let result = validate_strict_mode_constraints(DeterminismMode::Strict, None);
    assert!(result.is_err());

    // Verify exact error message for user-facing clarity
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("Strict determinism mode requires a seed"),
        "Expected clear error message, got: '{}'",
        err_msg
    );
}

#[test]
fn test_strict_mode_with_seed_succeeds() {
    // Strict mode with seed should succeed
    let result = validate_strict_mode_constraints(DeterminismMode::Strict, Some(12345));
    assert!(result.is_ok());
}

#[test]
fn test_besteffort_mode_no_seed_allowed() {
    // BestEffort mode doesn't require seed
    let result = validate_strict_mode_constraints(DeterminismMode::BestEffort, None);
    assert!(result.is_ok());
}

#[test]
fn test_relaxed_mode_no_seed_allowed() {
    // Relaxed mode doesn't require seed
    let result = validate_strict_mode_constraints(DeterminismMode::Relaxed, None);
    assert!(result.is_ok());
}

// =============================================================================
// Replay Guarantee Computation Tests
// =============================================================================

#[test]
fn test_guarantee_exact_strict_no_fallback_no_truncation() {
    // Strict mode + no fallback + no truncation = exact
    let guarantee = compute_replay_guarantee(
        DeterminismMode::Strict,
        false, // no fallback
        false, // no prompt truncation
        false, // no response truncation
        true,  // seed present
    );
    assert_eq!(guarantee, ReplayGuarantee::Exact);
}

#[test]
fn test_guarantee_approximate_strict_with_fallback() {
    // Strict mode + fallback triggered = approximate
    let guarantee = compute_replay_guarantee(
        DeterminismMode::Strict,
        true, // fallback triggered
        false,
        false,
        true,
    );
    assert_eq!(guarantee, ReplayGuarantee::Approximate);
}

#[test]
fn test_guarantee_approximate_strict_with_prompt_truncation() {
    // Strict mode + prompt truncation = approximate
    let guarantee = compute_replay_guarantee(
        DeterminismMode::Strict,
        false,
        true, // prompt truncated
        false,
        true,
    );
    assert_eq!(guarantee, ReplayGuarantee::Approximate);
}

#[test]
fn test_guarantee_approximate_strict_with_response_truncation() {
    // Strict mode + response truncation = approximate
    let guarantee = compute_replay_guarantee(
        DeterminismMode::Strict,
        false,
        false,
        true, // response truncated
        true,
    );
    assert_eq!(guarantee, ReplayGuarantee::Approximate);
}

#[test]
fn test_guarantee_approximate_besteffort() {
    // BestEffort mode = always approximate (even with no issues)
    let guarantee =
        compute_replay_guarantee(DeterminismMode::BestEffort, false, false, false, true);
    assert_eq!(guarantee, ReplayGuarantee::Approximate);
}

#[test]
fn test_guarantee_none_relaxed() {
    // Relaxed mode = always none
    let guarantee = compute_replay_guarantee(DeterminismMode::Relaxed, false, false, false, false);
    assert_eq!(guarantee, ReplayGuarantee::None);
}

#[test]
fn test_guarantee_none_relaxed_even_with_issues() {
    // Relaxed mode = none regardless of other factors
    let guarantee = compute_replay_guarantee(
        DeterminismMode::Relaxed,
        true, // fallback
        true, // truncated
        true, // truncated
        false,
    );
    assert_eq!(guarantee, ReplayGuarantee::None);
}

// =============================================================================
// InferenceResult to InferResponse Conversion Tests
// =============================================================================

#[test]
fn test_inference_result_determinism_fields_flow_through() {
    use adapteros_api_types::inference::InferResponse;
    use adapteros_server_api::types::InferenceResult;

    // Create InferenceResult with determinism tracking fields
    let result = InferenceResult {
        text: "test".to_string(),
        tokens_generated: 10,
        finish_reason: "stop".to_string(),
        adapters_used: vec!["adapter1".to_string()],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 100,
        request_id: "req-123".to_string(),
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        effective_adapter_ids: None,
        backend_used: Some("Metal".to_string()),
        fallback_triggered: true,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: Some("besteffort".to_string()),
        replay_guarantee: Some(ReplayGuarantee::Approximate),
        placement_trace: None,
    };

    // Convert to InferResponse
    let response: InferResponse = result.into();

    // Verify all determinism fields flow through
    assert_eq!(response.backend_used, Some("Metal".to_string()));
    assert!(response.fallback_triggered);
    assert_eq!(
        response.determinism_mode_applied,
        Some("besteffort".to_string())
    );
    assert_eq!(
        response.replay_guarantee,
        Some(ReplayGuarantee::Approximate)
    );
}

#[test]
fn test_inference_result_strict_mode_exact_guarantee() {
    use adapteros_api_types::inference::InferResponse;
    use adapteros_server_api::types::InferenceResult;

    // Strict mode + no fallback + no truncation = exact guarantee
    let result = InferenceResult {
        text: "test".to_string(),
        tokens_generated: 10,
        finish_reason: "stop".to_string(),
        adapters_used: vec![],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 100,
        request_id: "req-456".to_string(),
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        effective_adapter_ids: None,
        backend_used: Some("CoreML".to_string()),
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: Some("strict".to_string()),
        replay_guarantee: Some(ReplayGuarantee::Exact),
        placement_trace: None,
    };

    let response: InferResponse = result.into();

    assert_eq!(response.backend_used, Some("CoreML".to_string()));
    assert!(!response.fallback_triggered);
    assert_eq!(
        response.determinism_mode_applied,
        Some("strict".to_string())
    );
    assert_eq!(response.replay_guarantee, Some(ReplayGuarantee::Exact));
}

#[test]
fn test_inference_result_direct_mode_no_fallback() {
    use adapteros_api_types::inference::InferResponse;
    use adapteros_server_api::types::InferenceResult;

    // Direct mode (coordinator_enabled=false) always has fallback_triggered=false
    let result = InferenceResult {
        text: "direct mode test".to_string(),
        tokens_generated: 5,
        finish_reason: "stop".to_string(),
        adapters_used: vec![],
        router_decisions: vec![],
        router_decision_chain: None,
        rag_evidence: None,
        citations: vec![],
        latency_ms: 50,
        request_id: "req-789".to_string(),
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        effective_adapter_ids: None,
        backend_used: Some("MLX".to_string()),
        fallback_triggered: false, // Always false in direct mode
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        fallback_backend: None,
        determinism_mode_applied: Some("strict".to_string()),
        replay_guarantee: Some(ReplayGuarantee::Exact),
        placement_trace: None,
    };

    let response: InferResponse = result.into();

    // Direct mode should always have fallback_triggered=false
    assert!(!response.fallback_triggered);
    assert_eq!(response.backend_used, Some("MLX".to_string()));
}
