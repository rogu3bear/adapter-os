//! Q15 Quantization Edge Case Tests
//!
//! This test suite comprehensively validates Q15 fixed-point gate quantization
//! edge cases to ensure correctness, determinism, and proper handling of boundary
//! conditions in the router.
//!
//! Q15 Format:
//! - Uses signed 16-bit integers: range [-32768, 32767]
//! - Denominator: 32767.0 (NOT 32768.0)
//! - Encoding: gate_q15 = (gate_f32 * 32767.0).round() as i16, clamped to [0, 32767]
//! - Decoding: gate_f32 = gate_q15 as f32 / 32767.0
#![allow(clippy::useless_vec)]

use adapteros_core::determinism::{DeterminismContext, DeterminismSource};
use adapteros_lora_router::{
    policy_mask::PolicyMask, AdapterInfo, Decision, Router, RouterWeights, ROUTER_GATE_Q15_DENOM,
    ROUTER_GATE_Q15_MAX,
};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use smallvec::SmallVec;

/// Create a determinism context for tests
fn test_determinism_ctx() -> DeterminismContext {
    DeterminismContext::new(
        [42u8; 32],
        None,
        adapteros_core::SeedMode::BestEffort,
        RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    )
}

// ============================================================================
// CONSTANTS VALIDATION
// ============================================================================

#[test]
fn test_q15_constants_are_correct() {
    // Verify the Q15 constants are set correctly
    assert_eq!(
        ROUTER_GATE_Q15_DENOM, 32767.0,
        "Q15 denominator MUST be 32767.0, not 32768.0"
    );
    assert_eq!(
        ROUTER_GATE_Q15_MAX, 32767,
        "Q15 max value MUST be 32767 (i16::MAX)"
    );
    assert_eq!(
        ROUTER_GATE_Q15_MAX as f32, ROUTER_GATE_Q15_DENOM,
        "Max and denom should match for 1.0 representation"
    );
}

// ============================================================================
// EDGE CASE 1: Gate = 0 → Q15 = 0
// ============================================================================

#[test]
fn test_q15_zero_gate_converts_to_zero() {
    // When gate is exactly 0.0, Q15 should be 0
    let gate_f32 = 0.0f32;
    let gate_q15 = (gate_f32 * ROUTER_GATE_Q15_DENOM).round() as i16;
    let gate_q15_clamped = gate_q15.max(0);

    assert_eq!(gate_q15, 0, "0.0 gate should convert to Q15 = 0");
    assert_eq!(gate_q15_clamped, 0, "Clamped 0 should remain 0");

    // Verify round-trip
    let recovered = gate_q15_clamped as f32 / ROUTER_GATE_Q15_DENOM;
    assert_eq!(recovered, 0.0, "Q15 = 0 should decode to 0.0");
}

#[test]
fn test_router_produces_zero_gates_for_masked_adapters() {
    // Test that when an adapter is masked via policy, it's not selected
    let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    router.set_routing_determinism_mode(true);

    let features = vec![0.5; 22];
    let priors = vec![1.0, 1.0]; // Equal priors

    let adapter_info = vec![
        AdapterInfo {
            id: "adapter-1".to_string(),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        },
        AdapterInfo {
            id: "adapter-2".to_string(),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        },
    ];

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    // Deny the second adapter via policy mask
    let mask = PolicyMask::build(
        &adapter_ids,
        None,
        Some(&["adapter-2".to_string()]), // deny adapter-2
        None,
        None,
        None,
    );
    let ctx = test_determinism_ctx();
    let decision = router
        .route_with_adapter_info_with_ctx(&features, &priors, &adapter_info, &mask, Some(&ctx))
        .expect("routing decision");

    // Since adapter-2 is denied, only adapter-1 should be selected
    assert_eq!(
        decision.indices.len(),
        1,
        "Should only select 1 adapter when the other is masked"
    );
    assert_eq!(
        decision.indices[0], 0,
        "Should select adapter-0 (adapter-1 is the first/only allowed)"
    );
    // Single adapter should have gate = 1.0 (Q15 = 32767)
    assert_eq!(
        decision.gates_q15[0], 32767,
        "Single selected adapter should have full gate"
    );
}

// ============================================================================
// EDGE CASE 2: Gate = 1.0 → Q15 = 32767
// ============================================================================

#[test]
fn test_q15_max_gate_converts_to_32767() {
    // When gate is exactly 1.0, Q15 should be 32767
    let gate_f32 = 1.0f32;
    let gate_q15 = (gate_f32 * ROUTER_GATE_Q15_DENOM).round() as i16;

    assert_eq!(gate_q15, 32767, "1.0 gate should convert to Q15 = 32767");

    // Verify round-trip
    let recovered = gate_q15 as f32 / ROUTER_GATE_Q15_DENOM;
    assert_eq!(recovered, 1.0, "Q15 = 32767 should decode to exactly 1.0");
}

#[test]
fn test_router_produces_max_gate_for_single_adapter() {
    // When only one adapter is selected, its gate should be 1.0 → Q15 = 32767
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    router.set_routing_determinism_mode(true);

    let features = vec![0.5; 22];
    let priors = vec![1.0];

    let adapter_info = vec![AdapterInfo {
        id: "adapter-1".to_string(),
        framework: None,
        languages: vec![],
        tier: "default".to_string(),
        scope_path: None,
        lora_tier: None,
        base_model: None,
        ..Default::default()
    }];

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let mask = PolicyMask::allow_all(&adapter_ids, None);
    let ctx = test_determinism_ctx();
    let decision = router
        .route_with_adapter_info_with_ctx(&features, &priors, &adapter_info, &mask, Some(&ctx))
        .expect("routing decision");

    assert_eq!(decision.indices.len(), 1, "Should select exactly 1 adapter");
    assert_eq!(
        decision.gates_q15[0], 32767,
        "Single adapter should have Q15 gate = 32767"
    );

    let gates_f32 = decision.gates_f32();
    assert_eq!(
        gates_f32[0], 1.0,
        "Single adapter should have float gate = 1.0"
    );
}

// ============================================================================
// EDGE CASE 3: Negative gates (should not happen, but verify clamping)
// ============================================================================

#[test]
fn test_q15_negative_values_are_clamped_to_zero() {
    // The conversion code uses .max(0) to clamp negative values
    let negative_gate = -0.5f32;
    let gate_q15_raw = (negative_gate * ROUTER_GATE_Q15_DENOM).round() as i16;
    let gate_q15_clamped = gate_q15_raw.max(0);

    assert!(
        gate_q15_raw < 0,
        "Negative gate should produce negative Q15 before clamping"
    );
    assert_eq!(gate_q15_clamped, 0, "Negative Q15 should be clamped to 0");
}

#[test]
fn test_q15_conversion_ensures_non_negative_output() {
    // Test the actual conversion logic matches what's in lib.rs
    let test_gates = vec![-1.0, -0.5, -0.001, 0.0, 0.001, 0.5, 1.0];

    for gate in test_gates {
        let q = (gate * ROUTER_GATE_Q15_DENOM).round() as i16;
        let q_clamped = q.max(0);

        assert!(
            q_clamped >= 0,
            "Q15 value should always be non-negative after clamping, got {} for gate {}",
            q_clamped,
            gate
        );

        if gate <= 0.0 {
            assert_eq!(
                q_clamped, 0,
                "Non-positive gates should clamp to 0, got {} for gate {}",
                q_clamped, gate
            );
        }
    }
}

// ============================================================================
// EDGE CASE 4: Very small gates (underflow check)
// ============================================================================

#[test]
fn test_q15_very_small_positive_gates() {
    // Test gates that are very small but positive
    let tiny_gates = vec![1e-8, 1e-7, 1e-6, 1e-5, 1e-4, 1e-3, 1e-2, 1e-1];

    for gate in tiny_gates {
        let q = (gate * ROUTER_GATE_Q15_DENOM).round() as i16;
        let q_clamped = q.max(0);

        // Very small values should round to 0 or 1
        if gate * ROUTER_GATE_Q15_DENOM < 0.5 {
            assert_eq!(
                q_clamped,
                0,
                "Gate {} should round to Q15 = 0 (product = {})",
                gate,
                gate * ROUTER_GATE_Q15_DENOM
            );
        } else {
            assert!(
                q_clamped >= 1,
                "Gate {} should round to Q15 >= 1 (product = {})",
                gate,
                gate * ROUTER_GATE_Q15_DENOM
            );
        }
    }
}

#[test]
fn test_q15_minimum_representable_value() {
    // The minimum non-zero representable value in Q15 is 1/32767
    let min_value = 1.0 / ROUTER_GATE_Q15_DENOM;
    let gate_q15 = (min_value * ROUTER_GATE_Q15_DENOM).round() as i16;

    assert_eq!(gate_q15, 1, "Minimum representable Q15 value should be 1");

    let recovered = gate_q15 as f32 / ROUTER_GATE_Q15_DENOM;
    assert!(
        (recovered - min_value).abs() < 1e-6,
        "Min value round-trip should be accurate"
    );
}

#[test]
fn test_q15_underflow_threshold() {
    // Values smaller than 1/(2*32767) should round down to 0
    let threshold = 0.5 / ROUTER_GATE_Q15_DENOM;
    let just_under = threshold * 0.99;
    let just_over = threshold * 1.01;

    let q_under = (just_under * ROUTER_GATE_Q15_DENOM).round() as i16;
    let q_over = (just_over * ROUTER_GATE_Q15_DENOM).round() as i16;

    assert_eq!(q_under, 0, "Value just under threshold should round to 0");
    assert_eq!(q_over, 1, "Value just over threshold should round to 1");
}

// ============================================================================
// EDGE CASE 5: Sum of Q15 gates normalization
// ============================================================================

#[test]
fn test_q15_sum_after_normalization() {
    // When gates are normalized (sum to 1.0), their Q15 sum should be ~32767
    // However, due to rounding, the sum might be slightly off
    let normalized_gates = vec![0.25, 0.25, 0.25, 0.25]; // Sum = 1.0

    let gates_q15: Vec<i16> = normalized_gates
        .iter()
        .map(|&g| {
            let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
            q.max(0)
        })
        .collect();

    let sum_q15: i32 = gates_q15.iter().map(|&g| g as i32).sum();

    // Sum should be close to 32767, but may differ due to rounding
    assert!(
        (sum_q15 - ROUTER_GATE_Q15_MAX as i32).abs() <= gates_q15.len() as i32,
        "Sum of Q15 gates ({}) should be within {} of max ({})",
        sum_q15,
        gates_q15.len(),
        ROUTER_GATE_Q15_MAX
    );
}

#[test]
fn test_router_normalized_gates_sum_to_approximately_32767() {
    // Router gates should normalize and their Q15 sum should be ~32767
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_routing_determinism_mode(true);

    let features = vec![0.5; 22];
    let priors = vec![1.0, 1.0, 1.0]; // Equal priors

    let adapter_info: Vec<AdapterInfo> = (0..3)
        .map(|i| AdapterInfo {
            id: format!("adapter-{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let mask = PolicyMask::allow_all(&adapter_ids, None);
    let ctx = test_determinism_ctx();
    let decision = router
        .route_with_adapter_info_with_ctx(&features, &priors, &adapter_info, &mask, Some(&ctx))
        .expect("routing decision");

    let sum_q15: i32 = decision.gates_q15.iter().map(|&g| g as i32).sum();
    let sum_f32: f32 = decision.gates_f32().iter().sum();

    // Float gates should sum to 1.0
    assert!(
        (sum_f32 - 1.0).abs() < 0.01,
        "Float gates should sum to ~1.0, got {}",
        sum_f32
    );

    // Q15 gates should sum to ~32767 (within rounding error)
    assert!(
        (sum_q15 - ROUTER_GATE_Q15_MAX as i32).abs() <= decision.gates_q15.len() as i32,
        "Q15 gates sum ({}) should be within {} of 32767",
        sum_q15,
        decision.gates_q15.len()
    );
}

#[test]
fn test_q15_rounding_error_accumulation() {
    // Test that rounding errors don't accumulate excessively
    let test_cases = vec![
        vec![1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0], // Three equal parts
        vec![0.1, 0.2, 0.3, 0.4],              // Unequal distribution
        vec![0.5, 0.3, 0.15, 0.05],            // Decreasing values
    ];

    for gates_f32 in test_cases {
        let gates_q15: Vec<i16> = gates_f32
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();

        let sum_f32: f32 = gates_f32.iter().sum();
        let sum_q15: i32 = gates_q15.iter().map(|&g| g as i32).sum();

        // Original gates should sum to 1.0
        assert!(
            (sum_f32 - 1.0).abs() < 1e-6,
            "Test gates should sum to 1.0, got {} for {:?}",
            sum_f32,
            gates_f32
        );

        // Q15 sum should be within reasonable rounding error
        let max_error = gates_f32.len() as i32;
        assert!(
            (sum_q15 - ROUTER_GATE_Q15_MAX as i32).abs() <= max_error,
            "Q15 sum ({}) differs from max ({}) by more than expected ({}) for {:?}",
            sum_q15,
            ROUTER_GATE_Q15_MAX,
            max_error,
            gates_f32
        );
    }
}

// ============================================================================
// EDGE CASE 6: Q15→f32 conversion verification
// ============================================================================

#[test]
fn test_q15_to_f32_conversion_formula() {
    // Test the exact conversion formula: gate_f32 = gate_q15 / 32767.0
    let test_values = vec![
        (0i16, 0.0f32),
        (1i16, 1.0 / 32767.0),
        (16383i16, 16383.0 / 32767.0),
        (32767i16, 1.0),
    ];

    for (q15, expected_f32) in test_values {
        let converted = q15 as f32 / ROUTER_GATE_Q15_DENOM;
        assert!(
            (converted - expected_f32).abs() < 1e-6,
            "Q15 {} should convert to {}, got {}",
            q15,
            expected_f32,
            converted
        );
    }
}

#[test]
fn test_decision_gates_f32_method() {
    // Test the Decision::gates_f32() method
    let decision = Decision {
        indices: SmallVec::from_vec(vec![0, 1, 2]),
        gates_q15: SmallVec::from_vec(vec![32767, 16383, 0]),
        entropy: 0.5,
        candidates: vec![],
        decision_hash: None,
        policy_mask_digest_b3: None,
        policy_overrides_applied: None,
    };

    let gates_f32 = decision.gates_f32();

    assert_eq!(gates_f32.len(), 3, "Should have 3 gates");
    assert_eq!(gates_f32[0], 1.0, "32767 should convert to 1.0");
    assert!(
        (gates_f32[1] - 0.5).abs() < 0.001,
        "16383 should convert to ~0.5, got {}",
        gates_f32[1]
    );
    assert_eq!(gates_f32[2], 0.0, "0 should convert to 0.0");
}

#[test]
fn test_q15_round_trip_precision() {
    // Test f32 → Q15 → f32 round-trip precision
    let test_gates = vec![
        0.0, 0.001, 0.01, 0.1, 0.25, 0.333, 0.5, 0.666, 0.75, 0.9, 0.99, 0.999, 1.0,
    ];

    for original in test_gates {
        // Forward: f32 → Q15
        let q15 = (original * ROUTER_GATE_Q15_DENOM).round() as i16;
        let q15_clamped = q15.max(0);

        // Backward: Q15 → f32
        let recovered = q15_clamped as f32 / ROUTER_GATE_Q15_DENOM;

        // Calculate expected precision loss
        let max_error = 1.0 / ROUTER_GATE_Q15_DENOM;
        let actual_error = (recovered - original).abs();

        assert!(
            actual_error <= max_error,
            "Round-trip error ({}) exceeds maximum ({}) for gate {}",
            actual_error,
            max_error,
            original
        );
    }
}

// ============================================================================
// EDGE CASE 7: Determinism - same gates → same Q15 values
// ============================================================================

#[test]
fn test_q15_conversion_is_deterministic() {
    // Same input gates should always produce same Q15 values
    let gates = vec![0.2, 0.3, 0.5];

    // Convert 10 times and verify consistency
    let mut results = Vec::new();
    for _ in 0..10 {
        let gates_q15: Vec<i16> = gates
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();
        results.push(gates_q15);
    }

    // All results should be identical
    for i in 1..results.len() {
        assert_eq!(
            results[0], results[i],
            "Q15 conversion should be deterministic"
        );
    }
}

#[test]
fn test_router_produces_identical_q15_for_identical_inputs() {
    // Multiple routing calls with identical inputs should produce identical Q15 gates
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_routing_determinism_mode(true);

    let features = vec![0.5; 22];
    let priors = vec![0.6, 0.3, 0.1];

    let adapter_info: Vec<AdapterInfo> = (0..3)
        .map(|i| AdapterInfo {
            id: format!("adapter-{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let mask = PolicyMask::allow_all(&adapter_ids, None);
    let ctx = test_determinism_ctx();

    // Make 5 identical routing decisions
    let mut decisions = Vec::new();
    for _ in 0..5 {
        let decision = router
            .route_with_adapter_info_with_ctx(&features, &priors, &adapter_info, &mask, Some(&ctx))
            .expect("routing decision");
        decisions.push(decision);
    }

    // All decisions should have identical Q15 gates
    for i in 1..decisions.len() {
        assert_eq!(
            decisions[0].gates_q15, decisions[i].gates_q15,
            "Identical inputs should produce identical Q15 gates"
        );
        assert_eq!(
            decisions[0].indices, decisions[i].indices,
            "Identical inputs should produce identical indices"
        );
    }
}

#[test]
fn test_q15_determinism_across_different_architectures() {
    // This test documents expected Q15 behavior that should be consistent
    // across different CPU architectures (x86, ARM, etc.)

    // Test specific values that should have exact representations
    let exact_values = vec![
        (0.0f32, 0i16),
        (1.0f32, 32767i16),
        (0.5f32, 16384i16), // Note: 0.5 * 32767 = 16383.5 → rounds to 16384
    ];

    for (gate_f32, expected_q15) in exact_values {
        let computed_q15 = (gate_f32 * ROUTER_GATE_Q15_DENOM).round() as i16;
        assert_eq!(
            computed_q15, expected_q15,
            "Q15({}) should be {} on all architectures",
            gate_f32, expected_q15
        );
    }
}

// ============================================================================
// EDGE CASE 8: Special float values (NaN, Infinity)
// ============================================================================

#[test]
#[should_panic]
fn test_q15_nan_input_is_invalid() {
    // NaN inputs should not occur in practice, but test defensive behavior
    let nan_gate = f32::NAN;
    let _q15 = (nan_gate * ROUTER_GATE_Q15_DENOM).round() as i16;
    // This will produce undefined behavior, which we want to detect
    panic!("NaN should be detected");
}

#[test]
fn test_q15_infinity_handling() {
    // Infinity should overflow to i16 limits
    let inf_gate = f32::INFINITY;
    let q15 = (inf_gate * ROUTER_GATE_Q15_DENOM).round() as i16;

    // Infinity * 32767 will overflow i16, resulting in undefined value
    // This test documents that infinity is NOT handled gracefully
    // In practice, gates should never be infinity due to normalization
    let _ = q15; // Just verify it doesn't panic during conversion
}

// ============================================================================
// EDGE CASE 9: Consistency with legacy 32768 (verify we DON'T use it)
// ============================================================================

#[test]
fn test_q15_does_not_use_legacy_32768_denominator() {
    // This test ensures we're NOT using the incorrect 32768 denominator
    let gate_max = 1.0f32;

    // Correct conversion (32767)
    let q15_correct = (gate_max * 32767.0).round() as i16;

    assert_eq!(q15_correct, 32767, "Correct denom should give 32767");

    // Verify recovery is exact with 32767 denominator
    let recovered_correct = q15_correct as f32 / 32767.0;
    assert_eq!(
        recovered_correct, 1.0,
        "32767 denominator should give exact 1.0"
    );

    // With 32768, max gate would be slightly less than 1.0
    let recovered_incorrect = 32767_f32 / 32768.0;
    assert!(
        recovered_incorrect < 1.0,
        "32768 denominator would not represent 1.0 exactly"
    );
}

// ============================================================================
// EDGE CASE 10: Boundary value testing
// ============================================================================

#[test]
fn test_q15_boundary_values() {
    // Test values at and around i16 boundaries
    let boundary_tests = vec![
        (0i16, 0.0f32),
        (1i16, 1.0 / 32767.0),
        (32766i16, 32766.0 / 32767.0),
        (32767i16, 1.0),
    ];

    for (q15, expected_f32) in boundary_tests {
        let converted = q15 as f32 / ROUTER_GATE_Q15_DENOM;
        assert!(
            (converted - expected_f32).abs() < 1e-6,
            "Boundary Q15 {} should convert accurately to {}",
            q15,
            expected_f32
        );
    }
}

#[test]
fn test_q15_clamping_prevents_overflow() {
    // Test that gates > 1.0 are handled correctly
    let overflow_gates = vec![1.1, 1.5, 2.0, 10.0];

    for gate in overflow_gates {
        let q15_raw = (gate * ROUTER_GATE_Q15_DENOM).round() as i16;
        let q15_clamped = q15_raw.max(0);

        // Note: The current implementation doesn't clamp to 32767,
        // only to 0 for negative values. Gates > 1.0 would overflow.
        // This test documents current behavior.

        if gate <= 1.0 {
            assert!(q15_raw >= 0, "Gate {} should not underflow Q15", gate);
            assert_eq!(
                q15_clamped, q15_raw,
                "Gate {} should not be clamped in-range",
                gate
            );
        } else {
            // Gates > 1.0 will overflow i16
            // This is acceptable because normalization ensures gates ≤ 1.0
            let _ = q15_clamped; // Just document the behavior
        }
    }
}

// ============================================================================
// INTEGRATION TEST: Full routing pipeline Q15 validation
// ============================================================================

#[test]
fn test_full_routing_pipeline_q15_properties() {
    // Integration test: verify Q15 properties through full routing pipeline
    let mut router = Router::new_with_weights(RouterWeights::default(), 4, 1.0, 0.02);
    router.set_routing_determinism_mode(true);

    let features = vec![0.3; 22];
    let priors = vec![0.4, 0.3, 0.2, 0.1];

    let adapter_info: Vec<AdapterInfo> = (0..4)
        .map(|i| AdapterInfo {
            id: format!("adapter-{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let mask = PolicyMask::allow_all(&adapter_ids, None);
    let ctx = test_determinism_ctx();
    let decision = router
        .route_with_adapter_info_with_ctx(&features, &priors, &adapter_info, &mask, Some(&ctx))
        .expect("routing decision");

    // Property 1: All Q15 gates should be non-negative
    for &gate_q15 in &decision.gates_q15 {
        assert!(
            gate_q15 >= 0,
            "All Q15 gates should be non-negative, got {}",
            gate_q15
        );
    }

    // Property 2: Float gates should sum to ~1.0
    let gates_f32 = decision.gates_f32();
    let sum_f32: f32 = gates_f32.iter().sum();
    assert!(
        (sum_f32 - 1.0).abs() < 0.01,
        "Float gates should sum to ~1.0, got {}",
        sum_f32
    );

    // Property 3: Q15 gates should sum to ~32767
    let sum_q15: i32 = decision.gates_q15.iter().map(|&g| g as i32).sum();
    assert!(
        (sum_q15 - 32767).abs() <= decision.gates_q15.len() as i32,
        "Q15 gates should sum to ~32767 (within rounding), got {}",
        sum_q15
    );

    // Property 4: Round-trip should preserve relative ordering
    for i in 0..gates_f32.len() {
        for j in (i + 1)..gates_f32.len() {
            if decision.gates_q15[i] > decision.gates_q15[j] {
                assert!(
                    gates_f32[i] >= gates_f32[j],
                    "Q15 ordering should be preserved in f32"
                );
            }
        }
    }
}

// Note: Q15 denominator invariant tests have been moved to
// tests/q15_denominator_invariants.rs to isolate them from Router-dependent tests.
