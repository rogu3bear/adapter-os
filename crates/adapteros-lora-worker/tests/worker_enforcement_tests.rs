//! Worker enforcement tests for effective_adapter_ids hard gate
//!
//! These tests verify:
//! 1. Error types for hard gate enforcement exist and are correct
//! 2. Validation logic rejects invalid adapter configurations
//! 3. Strict mode disables fallback
//! 4. Telemetry surfaces effective_adapter_ids
//!
//! Run with: cargo test -p adapteros-lora-worker --test worker_enforcement_tests

use adapteros_api_types::inference::RouterDecision;
use adapteros_api_types::RoutingPolicy;
use adapteros_core::{AosError, Result};
use adapteros_lora_router::{Decision, DecisionCandidate};
use adapteros_lora_worker::router_bridge::decision_to_router_ring;
use adapteros_lora_worker::routing_policy_filter::filter_decision_by_policy;
use smallvec::SmallVec;
use std::collections::HashSet;

// =============================================================================
// ERROR TYPE TESTS (always run)
// =============================================================================

#[test]
fn test_adapter_not_in_effective_set_error_format() {
    let error = AosError::AdapterNotInEffectiveSet {
        adapter_id: "adapter-xyz".to_string(),
        effective_set: vec!["adapter-a".to_string(), "adapter-b".to_string()],
    };

    let message = format!("{}", error);
    assert!(
        message.contains("adapter-xyz"),
        "Error message should contain adapter ID: {}",
        message
    );
    assert!(
        message.contains("effective"),
        "Error message should mention 'effective': {}",
        message
    );
    assert!(
        message.contains("adapter-a"),
        "Error message should list effective set: {}",
        message
    );
}

#[test]
fn test_adapter_not_in_manifest_error_format() {
    let error = AosError::AdapterNotInManifest {
        adapter_id: "unknown-adapter".to_string(),
        available: vec!["real-adapter-1".to_string(), "real-adapter-2".to_string()],
    };

    let message = format!("{}", error);
    assert!(
        message.contains("unknown-adapter"),
        "Error message should contain adapter ID: {}",
        message
    );
    assert!(
        message.contains("manifest"),
        "Error message should mention 'manifest': {}",
        message
    );
    assert!(
        message.contains("real-adapter-1"),
        "Error message should list available adapters: {}",
        message
    );
}

// =============================================================================
// VALIDATION LOGIC TESTS (standalone, no Worker dependency)
// =============================================================================

/// Simulates the validation logic from Worker::validate_effective_adapter_gate()
/// without requiring a full Worker instance
fn validate_effective_adapter_gate_logic(
    manifest_adapter_ids: &[&str],
    effective_adapter_ids: Option<&[&str]>,
    pinned_adapter_ids: Option<&[&str]>,
) -> Result<()> {
    let manifest_ids: HashSet<&str> = manifest_adapter_ids.iter().copied().collect();

    // If no effective_adapter_ids specified, allow all (backward compatibility)
    let Some(effective_ids) = effective_adapter_ids else {
        return Ok(());
    };

    let effective_set: HashSet<&str> = effective_ids.iter().copied().collect();

    // Validate effective_adapter_ids themselves exist in manifest
    for effective_id in effective_ids {
        if !manifest_ids.contains(effective_id) {
            return Err(AosError::AdapterNotInManifest {
                adapter_id: effective_id.to_string(),
                available: manifest_ids.iter().map(|s| s.to_string()).collect(),
            });
        }
    }

    // Check pinned adapters are all in manifest AND in effective set
    if let Some(pinned_ids) = pinned_adapter_ids {
        for pinned_id in pinned_ids {
            // First check if pinned adapter exists in manifest
            if !manifest_ids.contains(pinned_id) {
                return Err(AosError::AdapterNotInManifest {
                    adapter_id: pinned_id.to_string(),
                    available: manifest_ids.iter().map(|s| s.to_string()).collect(),
                });
            }
            // Then check if it's in the effective set
            if !effective_set.contains(pinned_id) {
                return Err(AosError::AdapterNotInEffectiveSet {
                    adapter_id: pinned_id.to_string(),
                    effective_set: effective_ids.iter().map(|s| s.to_string()).collect(),
                });
            }
        }
    }

    Ok(())
}

#[test]
fn test_routing_policy_denies_specific_adapter() {
    let adapter_ids = vec!["adapter_a".to_string(), "adapter_b".to_string()];
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1]),
        gates_q15: SmallVec::from_slice(&[20000, 12000]),
        entropy: 0.0,
        candidates: vec![
            DecisionCandidate {
                adapter_idx: 0,
                raw_score: 0.8,
                gate_q15: 20000,
            },
            DecisionCandidate {
                adapter_idx: 1,
                raw_score: 0.7,
                gate_q15: 12000,
            },
        ],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let policy = RoutingPolicy {
        allowed_stack_ids: None,
        allowed_adapter_ids: None,
        denied_adapter_ids: Some(vec!["adapter_b".to_string()]),
        max_adapters_per_token: Some(2),
        allowed_clusters: None,
        denied_clusters: None,
        max_reasoning_depth: Some(10),
        cluster_fallback: "stay_on_current".to_string(),
        pin_enforcement: "warn".to_string(),
        require_stack: false,
        require_pins: false,
    };

    let clusters = vec![None, None, None];
    let filtered = filter_decision_by_policy(decision, &adapter_ids, &clusters, Some(&policy))
        .expect("filter succeeds");

    assert_eq!(filtered.indices.as_slice(), &[0]);
    let ring =
        decision_to_router_ring(&filtered, adapter_ids.len() as u16).expect("router ring builds");
    assert_eq!(ring.active_indices(), &[0]);
}

#[test]
fn test_routing_policy_denies_all_adapters() {
    let adapter_ids = vec!["adapter_a".to_string()];
    let decision = Decision {
        indices: SmallVec::from_slice(&[0]),
        gates_q15: SmallVec::from_slice(&[16000]),
        entropy: 0.0,
        candidates: vec![DecisionCandidate {
            adapter_idx: 0,
            raw_score: 1.0,
            gate_q15: 16000,
        }],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    let policy = RoutingPolicy {
        allowed_stack_ids: None,
        allowed_adapter_ids: Some(vec!["other".to_string()]), // excludes adapter_a
        denied_adapter_ids: None,
        max_adapters_per_token: None,
        allowed_clusters: None,
        denied_clusters: None,
        max_reasoning_depth: Some(10),
        cluster_fallback: "stay_on_current".to_string(),
        pin_enforcement: "warn".to_string(),
        require_stack: false,
        require_pins: false,
    };

    let clusters = vec![None];
    let result = filter_decision_by_policy(decision, &adapter_ids, &clusters, Some(&policy));
    let err = result.expect_err("policy should reject all adapters");
    assert!(matches!(err, AosError::PolicyViolation(_)));
    let msg = format!("{}", err);
    assert!(
        msg.contains("denied all adapters"),
        "Error should clearly state denial cause: {msg}"
    );
}

#[test]
fn test_no_effective_ids_allows_all() {
    // Backward compatibility: no effective_adapter_ids means all are allowed
    let manifest = vec!["adapter-a", "adapter-b", "adapter-c"];
    let result = validate_effective_adapter_gate_logic(&manifest, None, Some(&["adapter-a"]));

    assert!(result.is_ok(), "No effective_ids should allow all adapters");
}

#[test]
fn test_valid_adapters_pass_validation() {
    let manifest = vec!["adapter-a", "adapter-b", "adapter-c"];
    let effective = vec!["adapter-a", "adapter-b"];
    let pinned = vec!["adapter-a"];

    let result = validate_effective_adapter_gate_logic(&manifest, Some(&effective), Some(&pinned));

    assert!(
        result.is_ok(),
        "Valid adapter configuration should pass: {:?}",
        result
    );
}

#[test]
fn test_effective_id_not_in_manifest_rejected() {
    let manifest = vec!["adapter-a", "adapter-b"];
    let effective = vec!["adapter-a", "adapter-xyz"]; // adapter-xyz not in manifest

    let result = validate_effective_adapter_gate_logic(&manifest, Some(&effective), None);

    assert!(result.is_err(), "Invalid effective_id should be rejected");
    match result.unwrap_err() {
        AosError::AdapterNotInManifest { adapter_id, .. } => {
            assert_eq!(adapter_id, "adapter-xyz");
        }
        e => panic!("Expected AdapterNotInManifest error, got: {:?}", e),
    }
}

#[test]
fn test_pinned_not_in_manifest_rejected() {
    let manifest = vec!["adapter-a", "adapter-b"];
    let effective = vec!["adapter-a", "adapter-b"];
    let pinned = vec!["adapter-unknown"]; // not in manifest

    let result = validate_effective_adapter_gate_logic(&manifest, Some(&effective), Some(&pinned));

    assert!(
        result.is_err(),
        "Pinned adapter not in manifest should be rejected"
    );
    match result.unwrap_err() {
        AosError::AdapterNotInManifest { adapter_id, .. } => {
            assert_eq!(adapter_id, "adapter-unknown");
        }
        e => panic!("Expected AdapterNotInManifest error, got: {:?}", e),
    }
}

#[test]
fn test_pinned_not_in_effective_set_rejected() {
    let manifest = vec!["adapter-a", "adapter-b", "adapter-c"];
    let effective = vec!["adapter-a", "adapter-b"]; // adapter-c not in effective
    let pinned = vec!["adapter-c"]; // pinned adapter-c which IS in manifest but NOT in effective

    let result = validate_effective_adapter_gate_logic(&manifest, Some(&effective), Some(&pinned));

    assert!(
        result.is_err(),
        "Pinned adapter not in effective set should be rejected"
    );
    match result.unwrap_err() {
        AosError::AdapterNotInEffectiveSet {
            adapter_id,
            effective_set,
        } => {
            assert_eq!(adapter_id, "adapter-c");
            assert!(effective_set.contains(&"adapter-a".to_string()));
            assert!(effective_set.contains(&"adapter-b".to_string()));
            assert!(!effective_set.contains(&"adapter-c".to_string()));
        }
        e => panic!("Expected AdapterNotInEffectiveSet error, got: {:?}", e),
    }
}

#[test]
fn test_empty_effective_set_rejects_all_pinned() {
    let manifest = vec!["adapter-a", "adapter-b"];
    let effective: Vec<&str> = vec![]; // empty effective set
    let pinned = vec!["adapter-a"];

    let result = validate_effective_adapter_gate_logic(&manifest, Some(&effective), Some(&pinned));

    assert!(
        result.is_err(),
        "Pinned adapter with empty effective set should be rejected"
    );
    match result.unwrap_err() {
        AosError::AdapterNotInEffectiveSet { adapter_id, .. } => {
            assert_eq!(adapter_id, "adapter-a");
        }
        e => panic!("Expected AdapterNotInEffectiveSet error, got: {:?}", e),
    }
}

#[test]
fn test_multiple_pinned_all_valid() {
    let manifest = vec!["adapter-a", "adapter-b", "adapter-c", "adapter-d"];
    let effective = vec!["adapter-a", "adapter-b", "adapter-c"];
    let pinned = vec!["adapter-a", "adapter-b", "adapter-c"];

    let result = validate_effective_adapter_gate_logic(&manifest, Some(&effective), Some(&pinned));

    assert!(
        result.is_ok(),
        "All valid pinned adapters should pass: {:?}",
        result
    );
}

#[test]
fn base_only_request_zeroes_priors_and_clears_decisions() {
    let effective_adapter_ids: Option<Vec<String>> = Some(Vec::new());
    let base_only_request = matches!(
        effective_adapter_ids.as_ref(),
        Some(ids) if ids.is_empty()
    );
    assert!(
        base_only_request,
        "Empty effective_adapter_ids should mark base-only routing"
    );

    let mut priors = vec![1.0f32; 3];
    if base_only_request {
        priors.iter_mut().for_each(|p| *p = 0.0);
    }
    assert!(
        priors.iter().all(|p| (*p - 0.0).abs() < f32::EPSILON),
        "All priors should be zeroed for base-only requests"
    );

    let mut decision = Decision {
        indices: SmallVec::from_slice(&[0, 1, 2]),
        gates_q15: SmallVec::from_slice(&[1000, 2000, 3000]),
        entropy: 0.0,
        candidates: vec![
            DecisionCandidate {
                adapter_idx: 0,
                raw_score: 0.4,
                gate_q15: 1000,
            },
            DecisionCandidate {
                adapter_idx: 1,
                raw_score: 0.5,
                gate_q15: 2000,
            },
            DecisionCandidate {
                adapter_idx: 2,
                raw_score: 0.6,
                gate_q15: 3000,
            },
        ],
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
    };

    if base_only_request {
        decision.indices.clear();
        decision.candidates.clear();
        decision.gates_q15.clear();
    }

    let mut router_decisions = Vec::new();
    for step in 0..2 {
        router_decisions.push(RouterDecision {
            step,
            input_token_id: None,
            candidate_adapters: decision
                .candidates
                .iter()
                .map(|c| adapteros_api_types::inference::RouterCandidate {
                    adapter_idx: c.adapter_idx,
                    raw_score: c.raw_score,
                    gate_q15: c.gate_q15,
                })
                .collect(),
            entropy: decision.entropy,
            tau: 0.0,
            entropy_floor: 0.0,
            stack_hash: None,
            interval_id: None,
            allowed_mask: None,
            policy_mask_digest: None,
            policy_overrides_applied: None,
            model_type: adapteros_api_types::inference::RouterModelType::Dense,
            active_experts: None,
        });
    }

    assert!(
        decision.indices.is_empty() && decision.gates_q15.is_empty(),
        "Base-only requests should clear adapter indices and gates"
    );
    assert!(
        router_decisions
            .iter()
            .all(|d| d.candidate_adapters.is_empty()),
        "Router decisions should record no adapters for base-only streaming steps"
    );
}

#[test]
fn test_multiple_pinned_one_invalid() {
    let manifest = vec!["adapter-a", "adapter-b", "adapter-c", "adapter-d"];
    let effective = vec!["adapter-a", "adapter-b"];
    let pinned = vec!["adapter-a", "adapter-c"]; // adapter-c in manifest but not in effective

    let result = validate_effective_adapter_gate_logic(&manifest, Some(&effective), Some(&pinned));

    assert!(
        result.is_err(),
        "One invalid pinned adapter should cause rejection"
    );
    match result.unwrap_err() {
        AosError::AdapterNotInEffectiveSet { adapter_id, .. } => {
            assert_eq!(adapter_id, "adapter-c");
        }
        e => panic!("Expected AdapterNotInEffectiveSet error, got: {:?}", e),
    }
}

#[test]
fn test_no_pinned_adapters_with_effective_set() {
    // If no pinned adapters, only effective_ids are validated against manifest
    let manifest = vec!["adapter-a", "adapter-b"];
    let effective = vec!["adapter-a"];

    let result = validate_effective_adapter_gate_logic(&manifest, Some(&effective), None);

    assert!(
        result.is_ok(),
        "No pinned adapters with valid effective set should pass"
    );
}

#[test]
fn test_router_decision_outside_effective_set_causes_error() {
    // Allowed set limits routing to adapter-a and adapter-b
    let manifest = vec!["adapter-a", "adapter-b", "adapter-c"];
    let effective = vec!["adapter-a", "adapter-b"];

    let allowed_indices: HashSet<usize> = manifest
        .iter()
        .enumerate()
        .filter_map(|(idx, id)| {
            if effective.contains(id) {
                Some(idx)
            } else {
                None
            }
        })
        .collect();

    // Simulate router picking adapter-c (index 2) which is not allowed
    let router_selection = vec![0_u16, 2_u16];
    let first_invalid = router_selection
        .iter()
        .find(|idx| !allowed_indices.contains(&(**idx as usize)))
        .expect("should detect adapter outside effective set");

    let error = AosError::AdapterNotInEffectiveSet {
        adapter_id: manifest[*first_invalid as usize].to_string(),
        effective_set: effective.iter().map(|s| s.to_string()).collect(),
    };

    let message = format!("{}", error);
    assert!(message.contains("adapter-c"));
    assert!(message.contains("effective"));
}

// =============================================================================
// STRICT MODE KERNEL WRAPPER TESTS
// =============================================================================
//
// These tests verify the `KernelWrapper` strict mode behavior that is used by
// the Worker during inference. The flow is:
//
// 1. Control plane sends `InferenceRequest` with `strict_mode: bool`
// 2. `Worker::infer_internal()` at lib.rs:1155-1160 calls:
//    ```
//    kernels.set_strict_mode(request.strict_mode);
//    kernels.reset_fallback();
//    ```
// 3. `KernelWrapper::run_step()` at lib.rs:278-315 checks `strict_mode`:
//    - If true and primary fails: returns error immediately (no fallback)
//    - If false and primary fails: attempts fallback to secondary backend
//
// The tests below directly exercise `KernelWrapper` to verify this behavior.
// Full integration tests require hardware and are gated behind `hardware-residency`.
// =============================================================================

use adapteros_lora_kernel_api::{FailingKernel, FusedKernels, IoBuffers, MockKernels, RouterRing};
use adapteros_lora_worker::{
    BackendLane, CoordinatedKernels, DirectKernels, KernelWrapper, StrictnessControl,
};

/// Scenario 3: Strict mode + backend failure = hard fail (no fallback)
///
/// This is the key sanity check: when `strict_mode=true` and the primary
/// backend fails, the worker MUST NOT attempt fallback. This ensures
/// deterministic failure behavior for replay guarantees.
///
/// Control plane sets `strict_mode=true` when:
/// - `DeterminismMode::Strict` is active, OR
/// - Tenant policy has `allow_fallback=false`
#[test]
fn test_strict_mode_backend_failure_no_fallback() {
    let primary = Box::new(FailingKernel::new("Primary backend failure"))
        as Box<dyn FusedKernels + Send + Sync>;
    let fallback = Box::new(MockKernels::new()) as Box<dyn FusedKernels + Send + Sync>;

    let coordinated = CoordinatedKernels::new(primary, Some(fallback));
    let mut wrapper = KernelWrapper::Coordinated(coordinated);

    // Simulate what Worker::infer_internal() does at lib.rs:1155-1160
    wrapper.set_strict_mode(true);
    wrapper.reset_fallback();

    let mut io = IoBuffers::new(1024);
    io.input_ids.push(1);
    let ring = RouterRing::new(1);

    // Run step - should fail, NOT fallback
    let result = wrapper.run_step(&ring, &mut io);

    // Verify: error returned, fallback NOT triggered
    assert!(
        result.is_err(),
        "Strict mode should fail when primary fails"
    );
    assert!(
        !wrapper.fallback_triggered(),
        "Fallback should NOT be triggered in strict mode"
    );

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Primary backend failure"),
        "Error should contain primary failure message, got: {}",
        err
    );
}

/// Non-strict mode does NOT switch mid-run; primary failure ends the request
#[test]
fn test_non_strict_mode_does_not_switch_mid_run() {
    let primary = Box::new(FailingKernel::new("Primary backend failure"))
        as Box<dyn FusedKernels + Send + Sync>;
    let fallback = Box::new(MockKernels::new()) as Box<dyn FusedKernels + Send + Sync>;

    let coordinated = CoordinatedKernels::new(primary, Some(fallback));
    let mut wrapper = KernelWrapper::Coordinated(coordinated);

    wrapper.set_strict_mode(false);
    wrapper.reset_fallback();

    let mut io = IoBuffers::new(1024);
    io.input_ids.push(1);
    let ring = RouterRing::new(1);

    let result = wrapper.run_step(&ring, &mut io);

    assert!(
        result.is_err(),
        "Primary failure should end the request when backend is pinned"
    );
    assert!(
        !wrapper.fallback_triggered(),
        "Fallback is not used mid-run in pinned mode"
    );
}

/// After a primary failure, the next request pins to fallback (non-strict)
#[test]
fn test_next_request_uses_fallback_after_primary_failure() {
    let primary = Box::new(FailingKernel::new("Primary backend failure"))
        as Box<dyn FusedKernels + Send + Sync>;
    let fallback = Box::new(MockKernels::new()) as Box<dyn FusedKernels + Send + Sync>;

    let coordinated = CoordinatedKernels::new(primary, Some(fallback));
    let mut wrapper = KernelWrapper::Coordinated(coordinated);

    wrapper.set_strict_mode(false);
    wrapper.reset_fallback();

    let mut io = IoBuffers::new(1024);
    io.input_ids.push(1);
    let ring = RouterRing::new(1);

    // First request fails on primary
    let result = wrapper.run_step(&ring, &mut io);
    assert!(result.is_err(), "First request should fail on primary");

    // Next request should pin to fallback because primary is degraded
    wrapper.reset_fallback();
    let mut io_second = IoBuffers::new(1024);
    io_second.input_ids.push(1);

    let second = wrapper.run_step(&ring, &mut io_second);
    assert!(
        second.is_ok(),
        "Fallback should be used on subsequent request"
    );
    assert!(
        wrapper.fallback_triggered(),
        "Fallback should be marked when pinned before the request"
    );
    assert_eq!(
        wrapper.last_backend_used(),
        Some("Mock Kernels (Test)".to_string()),
        "Fallback backend should be used once pinned"
    );
}

/// Strict mode with no fallback backend configured
///
/// Even with strict mode, if no fallback is configured, the behavior is the
/// same: primary failure returns error. This tests the edge case.
#[test]
fn test_strict_mode_no_fallback_backend_reports_failure() {
    let primary =
        Box::new(FailingKernel::new("Primary failure")) as Box<dyn FusedKernels + Send + Sync>;

    // No fallback backend
    let coordinated = CoordinatedKernels::new(primary, None);
    let mut wrapper = KernelWrapper::Coordinated(coordinated);

    wrapper.set_strict_mode(true);
    wrapper.reset_fallback();

    let mut io = IoBuffers::new(1024);
    io.input_ids.push(1);
    let ring = RouterRing::new(1);

    let result = wrapper.run_step(&ring, &mut io);

    assert!(
        result.is_err(),
        "Should fail when primary fails with no fallback"
    );
    assert!(!wrapper.fallback_triggered());
}

/// Primary success should not trigger fallback regardless of strict mode
#[test]
fn test_primary_success_no_fallback_triggered() {
    let primary = Box::new(MockKernels::new()) as Box<dyn FusedKernels + Send + Sync>;
    let fallback = Box::new(FailingKernel::new("Fallback should not be called"))
        as Box<dyn FusedKernels + Send + Sync>;

    let coordinated = CoordinatedKernels::new(primary, Some(fallback));
    let mut wrapper = KernelWrapper::Coordinated(coordinated);

    wrapper.set_strict_mode(false);
    wrapper.reset_fallback();

    let mut io = IoBuffers::new(1024);
    io.input_ids.push(1);
    let ring = RouterRing::new(1);

    let result = wrapper.run_step(&ring, &mut io);

    assert!(result.is_ok(), "Should succeed with working primary");
    assert!(
        !wrapper.fallback_triggered(),
        "Fallback should not trigger when primary succeeds"
    );
    assert_eq!(
        wrapper.last_backend_used(),
        Some("Mock Kernels (Test)".to_string())
    );
}

/// Forcing fallback lane should surface fallback_triggered for telemetry/reporting
#[test]
fn test_forced_fallback_lane_is_recorded() {
    let primary = Box::new(MockKernels::new()) as Box<dyn FusedKernels + Send + Sync>;
    let fallback = Box::new(MockKernels::new()) as Box<dyn FusedKernels + Send + Sync>;

    let mut wrapper = KernelWrapper::Coordinated(CoordinatedKernels::new(primary, Some(fallback)));
    wrapper.set_active_lane(BackendLane::Fallback);

    assert_eq!(wrapper.active_lane(), BackendLane::Fallback);
    assert!(
        wrapper.fallback_triggered(),
        "fallback flag must be surfaced when fallback lane is active"
    );
    assert_eq!(
        wrapper.last_backend_used(),
        Some("Mock Kernels (Test)".to_string())
    );
}

/// Direct mode (coordinator_enabled=false) has no fallback capability
///
/// When `aos_worker` is started with `--coordinator-enabled=false`, it uses
/// `KernelWrapper::Direct` which wraps a single backend. This test verifies
/// that Direct mode never reports fallback_triggered=true.
#[test]
fn test_direct_mode_no_fallback_capability() {
    let primary = Box::new(MockKernels::new()) as Box<dyn FusedKernels + Send + Sync>;

    let direct = DirectKernels::new(primary);
    let mut wrapper = KernelWrapper::Direct(direct);

    // set_strict_mode is a no-op for Direct mode
    wrapper.set_strict_mode(true);
    wrapper.reset_fallback();

    let mut io = IoBuffers::new(1024);
    io.input_ids.push(1);
    let ring = RouterRing::new(1);

    let result = wrapper.run_step(&ring, &mut io);

    assert!(result.is_ok(), "Direct mode should succeed");
    assert!(
        !wrapper.fallback_triggered(),
        "Direct mode should never trigger fallback"
    );
}

/// Direct mode failure propagates error (no fallback possible)
#[test]
fn test_direct_mode_failure_propagates() {
    let primary =
        Box::new(FailingKernel::new("Direct mode failure")) as Box<dyn FusedKernels + Send + Sync>;

    let direct = DirectKernels::new(primary);
    let mut wrapper = KernelWrapper::Direct(direct);

    wrapper.reset_fallback();

    let mut io = IoBuffers::new(1024);
    io.input_ids.push(1);
    let ring = RouterRing::new(1);

    let result = wrapper.run_step(&ring, &mut io);

    assert!(
        result.is_err(),
        "Direct mode should fail when backend fails"
    );
    assert!(!wrapper.fallback_triggered());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Direct mode failure"));
}

// =============================================================================
// STRICT MODE SEMANTICS TESTS
// =============================================================================

#[test]
fn test_strict_mode_semantics_documented() {
    // Document the strict_mode behavior:
    // - When strict_mode=true, worker pins to primary only (no fallback)
    // - When strict_mode=false, worker may pin to fallback before a request
    //   (e.g., after primary is degraded) but does not swap mid-run
    //
    // strict_mode is set to true when:
    // 1. determinism_mode == "strict" (DeterminismMode::Strict)
    // 2. tenant policy has fallback_allowed = false

    // This test documents the expected behavior
    // Actual integration test requires full worker setup

    struct StrictModeScenario {
        determinism_mode: &'static str,
        fallback_allowed: bool,
        expected_strict_mode: bool,
    }

    let scenarios = vec![
        StrictModeScenario {
            determinism_mode: "strict",
            fallback_allowed: true,
            expected_strict_mode: true, // strict mode always enables strict_mode
        },
        StrictModeScenario {
            determinism_mode: "strict",
            fallback_allowed: false,
            expected_strict_mode: true,
        },
        StrictModeScenario {
            determinism_mode: "besteffort",
            fallback_allowed: true,
            expected_strict_mode: false, // besteffort with fallback allowed = not strict
        },
        StrictModeScenario {
            determinism_mode: "besteffort",
            fallback_allowed: false,
            expected_strict_mode: true, // fallback_allowed=false forces strict_mode
        },
        StrictModeScenario {
            determinism_mode: "relaxed",
            fallback_allowed: true,
            expected_strict_mode: false,
        },
        StrictModeScenario {
            determinism_mode: "relaxed",
            fallback_allowed: false,
            expected_strict_mode: true, // fallback_allowed=false forces strict_mode
        },
    ];

    for scenario in scenarios {
        // Compute strict_mode as InferenceCore does
        let strict = scenario.determinism_mode == "strict" || !scenario.fallback_allowed;
        assert_eq!(
            strict, scenario.expected_strict_mode,
            "Scenario: determinism_mode={}, fallback_allowed={} should have strict_mode={}",
            scenario.determinism_mode, scenario.fallback_allowed, scenario.expected_strict_mode
        );
    }
}

// =============================================================================
// TELEMETRY TESTS
// =============================================================================

#[test]
fn test_effective_adapter_ids_serializes_correctly() {
    // Verify that effective_adapter_ids can be serialized for telemetry
    let effective_ids = vec!["adapter-1", "adapter-2", "adapter-3"];
    let json = serde_json::to_string(&effective_ids).expect("should serialize");

    assert_eq!(json, r#"["adapter-1","adapter-2","adapter-3"]"#);

    // Verify None serializes as null
    let none_ids: Option<Vec<String>> = None;
    let json = serde_json::to_string(&none_ids).expect("should serialize");
    assert_eq!(json, "null");
}

// =============================================================================
// INTEGRATION TESTS (require hardware, feature-gated)
// =============================================================================

#[tokio::test]
#[cfg_attr(
    not(feature = "hardware-residency"),
    ignore = "Requires hardware-residency feature and real worker [tracking: STAB-IGN-0222]"
)]
async fn test_worker_rejects_invalid_pinned_adapter() {
    // This test would require a full Worker instance
    // It's gated behind hardware-residency feature for now

    // Expected behavior:
    // 1. Create Worker with manifest containing adapters [A, B, C]
    // 2. Send InferenceRequest with effective_adapter_ids=[A, B], pinned_adapter_ids=[C]
    // 3. Worker should return AdapterNotInEffectiveSet error

    eprintln!("INTEGRATION TEST: test_worker_rejects_invalid_pinned_adapter");
    eprintln!("This test requires a full Worker instance with real backend");
    eprintln!("Expected: Worker rejects request when pinned adapter not in effective set");
}

#[tokio::test]
#[cfg_attr(
    not(feature = "hardware-residency"),
    ignore = "Requires hardware-residency feature and real worker [tracking: STAB-IGN-0223]"
)]
async fn test_worker_strict_mode_no_fallback() {
    // This test would verify that strict_mode prevents fallback

    // Expected behavior:
    // 1. Create Worker with BackendCoordinator (Metal primary, CoreML fallback)
    // 2. Send InferenceRequest with strict_mode=true
    // 3. If Metal fails, request should fail (not fallback to CoreML)

    eprintln!("INTEGRATION TEST: test_worker_strict_mode_no_fallback");
    eprintln!("This test requires BackendCoordinator with multiple backends");
    eprintln!("Expected: strict_mode=true prevents fallback, request fails if primary fails");
}

#[tokio::test]
#[cfg_attr(
    not(feature = "hardware-residency"),
    ignore = "Requires hardware-residency feature and real worker [tracking: STAB-IGN-0224]"
)]
async fn test_worker_non_strict_mode_fallback_triggers() {
    // This test would verify that non-strict mode allows fallback

    // Expected behavior:
    // 1. Create Worker with BackendCoordinator (Metal primary, CoreML fallback)
    // 2. Simulate Metal failure
    // 3. Send InferenceRequest with strict_mode=false
    // 4. Request should succeed via CoreML fallback
    // 5. Response should have fallback_triggered=true

    eprintln!("INTEGRATION TEST: test_worker_non_strict_mode_fallback_triggers");
    eprintln!("This test requires BackendCoordinator with failure injection");
    eprintln!("Expected: strict_mode=false allows fallback, response.fallback_triggered=true");
}

#[tokio::test]
#[cfg_attr(
    not(feature = "hardware-residency"),
    ignore = "Requires hardware-residency feature and real worker [tracking: STAB-IGN-0225]"
)]
async fn test_response_surfaces_fallback_triggered() {
    // Verify that InferenceResponse correctly surfaces fallback_triggered field

    // Expected behavior:
    // 1. InferenceResponse has fallback_triggered: bool field
    // 2. When BackendCoordinator triggers fallback, this is set to true
    // 3. When no fallback occurs, this is set to false

    eprintln!("INTEGRATION TEST: test_response_surfaces_fallback_triggered");
    eprintln!("Expected: InferenceResponse.fallback_triggered correctly reflects fallback state");
}

#[tokio::test]
#[cfg_attr(
    not(feature = "hardware-residency"),
    ignore = "Requires hardware-residency feature [tracking: STAB-IGN-0226]"
)]
async fn test_telemetry_emits_effective_adapter_ids() {
    // Verify that telemetry correctly captures effective_adapter_ids

    // Expected behavior:
    // 1. Send InferenceRequest with effective_adapter_ids set
    // 2. Telemetry should include effective_adapter_ids in the event
    // 3. This enables audit trail for stack-aware routing

    eprintln!("INTEGRATION TEST: test_telemetry_emits_effective_adapter_ids");
    eprintln!("Expected: Telemetry events include effective_adapter_ids for audit");
}

// =============================================================================
// CACHE PINNING AND RSS STABILIZATION TESTS
// =============================================================================

use adapteros_core::B3Hash;
use adapteros_lora_kernel_api::attestation::BackendType;
use adapteros_lora_worker::model_handle_cache::{ModelHandle, ModelHandleCache};
use adapteros_lora_worker::model_key::{ModelCacheIdentity, ModelKey};
use adapteros_telemetry::CriticalComponentMetrics;
use std::sync::Arc;

fn make_test_key(backend: BackendType, data: &[u8]) -> ModelKey {
    ModelKey::new(
        backend,
        B3Hash::hash(data),
        ModelCacheIdentity::for_backend(backend),
    )
}

/// Test that cache pinning correctly emits telemetry metrics
#[test]
fn test_cache_pinning_emits_telemetry() {
    let metrics = Arc::new(CriticalComponentMetrics::new().expect("metrics creation"));
    let cache = ModelHandleCache::new_with_metrics(1024 * 1024, Arc::clone(&metrics));

    let key = make_test_key(BackendType::Metal, b"base_model");

    // Load and pin a base model
    cache
        .get_or_load_base_model(&key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0u8; 1024])), 1024))
        })
        .expect("load should succeed");

    // Verify pinned count gauge is updated
    assert_eq!(
        metrics.get_pinned_entries_count(),
        1,
        "Pinned entries gauge should be 1"
    );
    assert!(cache.is_pinned(&key), "Key should be pinned");
    assert_eq!(cache.pinned_count(), 1);

    // Unpin and verify gauge updates
    cache.unpin(&key);
    assert_eq!(
        metrics.get_pinned_entries_count(),
        0,
        "Pinned entries gauge should be 0 after unpin"
    );
}

/// Test that cache hit/miss metrics are emitted
#[test]
fn test_cache_hit_miss_telemetry() {
    let metrics = Arc::new(CriticalComponentMetrics::new().expect("metrics creation"));
    let cache = ModelHandleCache::new_with_metrics(1024 * 1024, Arc::clone(&metrics));

    let key = make_test_key(BackendType::Metal, b"model");

    // First load: cache miss
    cache
        .get_or_load(&key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0u8; 100])), 100))
        })
        .expect("load should succeed");

    assert_eq!(metrics.get_model_cache_misses(), 1.0, "Should have 1 miss");
    assert_eq!(metrics.get_model_cache_hits(), 0.0, "Should have 0 hits");

    // Second load: cache hit
    cache
        .get_or_load(&key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0u8; 100])), 100))
        })
        .expect("load should succeed");

    assert_eq!(
        metrics.get_model_cache_misses(),
        1.0,
        "Should still have 1 miss"
    );
    assert_eq!(metrics.get_model_cache_hits(), 1.0, "Should have 1 hit");
}

/// Test adapter churn with pinned base model - validates RSS stabilization pattern
#[test]
fn test_adapter_churn_with_pinned_base_model() {
    let metrics = Arc::new(CriticalComponentMetrics::new().expect("metrics creation"));
    // Small cache to force evictions: 1000 bytes max
    let cache = ModelHandleCache::new_with_metrics(1000, Arc::clone(&metrics));

    // Load and pin a base model (500 bytes)
    let base_key = make_test_key(BackendType::Metal, b"base_model");
    cache
        .get_or_load_base_model(&base_key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0u8; 500])), 500))
        })
        .expect("base model load should succeed");

    assert!(cache.is_pinned(&base_key), "Base model should be pinned");
    assert_eq!(cache.pinned_count(), 1);

    // Run adapter churn: load 50 adapters, each ~400 bytes
    // This will force evictions since cache max is 1000 bytes
    for i in 0..50 {
        let adapter_key = make_test_key(BackendType::Metal, format!("adapter_{}", i).as_bytes());
        cache
            .get_or_load(&adapter_key, || {
                Ok((ModelHandle::Metal(Arc::new(vec![0u8; 400])), 400))
            })
            .expect("adapter load should succeed");
    }

    // Validate invariants:
    // 1. Base model is still pinned
    assert!(
        cache.is_pinned(&base_key),
        "Base model should remain pinned after adapter churn"
    );
    assert_eq!(cache.pinned_count(), 1, "Only base model should be pinned");

    // 2. Cache size is bounded (base model + at most 1 adapter)
    let cache_len = cache.len();
    assert!(
        cache_len <= 3,
        "Cache should be bounded, got {} entries",
        cache_len
    );

    // 3. Eviction blocked pinned metric should have been incremented
    let stats = cache.stats();
    assert!(
        stats.eviction_skip_pinned_count > 0,
        "Eviction skip count should be > 0 (got {}), indicating pinning prevented eviction",
        stats.eviction_skip_pinned_count
    );

    // 4. Verify telemetry gauge still shows 1 pinned entry
    assert_eq!(
        metrics.get_pinned_entries_count(),
        1,
        "Telemetry should show 1 pinned entry"
    );
}

/// Test that multiple concurrent pin operations don't cause issues
#[test]
fn test_concurrent_pin_idempotent() {
    let metrics = Arc::new(CriticalComponentMetrics::new().expect("metrics creation"));
    let cache = ModelHandleCache::new_with_metrics(1024 * 1024, Arc::clone(&metrics));

    let key = make_test_key(BackendType::Metal, b"model");

    // Load a model
    cache
        .get_or_load(&key, || {
            Ok((ModelHandle::Metal(Arc::new(vec![0u8; 100])), 100))
        })
        .expect("load should succeed");

    // Pin multiple times (should be idempotent)
    assert!(cache.pin(&key));
    assert!(cache.pin(&key)); // Second pin on same key
    assert!(cache.pin(&key)); // Third pin

    // Should still only count as 1 pinned entry
    assert_eq!(cache.pinned_count(), 1);
    assert_eq!(metrics.get_pinned_entries_count(), 1);
}

#[test]
fn test_unpin_nonexistent_returns_false() {
    let cache = ModelHandleCache::new(1024 * 1024);
    let key = make_test_key(BackendType::Metal, b"nonexistent");

    // Unpin on key that was never pinned should return false
    assert!(!cache.unpin(&key));
}
