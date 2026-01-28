#![no_main]

//! Fuzz target for router input validation with special floating-point values.
//!
//! This target specifically tests security-critical numeric edge cases:
//! - NaN values (signaling and quiet)
//! - Positive and negative infinity
//! - Subnormal/denormal numbers
//! - Extreme large and small values
//! - Zero (positive and negative)
//! - Mixed normal and special values
//!
//! Goal: Ensure router never panics on malformed numeric inputs and
//! maintains determinism even with edge-case floating-point values.

use adapteros_lora_router::{
    policy_mask::PolicyMask, AdapterInfo, Decision, GateQuantFormat, Router, RouterWeights, MAX_K,
    ROUTER_GATE_Q15_DENOM,
};
use arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;

/// Special floating-point values for testing edge cases
const SPECIAL_FLOATS: &[f32] = &[
    f32::NAN,
    f32::INFINITY,
    f32::NEG_INFINITY,
    f32::MIN,
    f32::MAX,
    f32::MIN_POSITIVE, // Smallest positive normal
    f32::EPSILON,      // Smallest difference from 1.0
    0.0,
    -0.0,
    1.0,
    -1.0,
    // Subnormal numbers (between 0 and MIN_POSITIVE)
    1.0e-45,  // Smallest positive subnormal
    1.0e-40,  // Subnormal
    -1.0e-45, // Smallest negative subnormal
    // Large values near overflow
    1.0e38,
    -1.0e38,
    3.4e38, // Near MAX
    -3.4e38,
    // Common problematic values
    1.0 / 3.0, // Repeating decimal
    std::f32::consts::PI,
    std::f32::consts::E,
];

/// Generate a float that may be special or arbitrary
fn next_float(u: &mut Unstructured<'_>, allow_special: bool) -> Option<f32> {
    if allow_special && u.arbitrary::<bool>().ok()? {
        // 50% chance of special value when allowed
        let idx = u.int_in_range::<usize>(0..=SPECIAL_FLOATS.len() - 1).ok()?;
        Some(SPECIAL_FLOATS[idx])
    } else {
        // Arbitrary float (may still be special via bit pattern)
        u.arbitrary().ok()
    }
}

/// Generate a float clamped to valid range, with special value handling
fn next_safe_float(u: &mut Unstructured<'_>, min: f32, max: f32, fallback: f32) -> Option<f32> {
    let raw = next_float(u, true)?;
    if !raw.is_finite() {
        Some(fallback)
    } else {
        Some(raw.clamp(min, max))
    }
}

/// Build adapter info with deterministic fields
fn build_adapter(idx: usize) -> AdapterInfo {
    AdapterInfo {
        id: format!("adapter-{}", idx),
        framework: Some(if idx % 2 == 0 { "coreml" } else { "mlx" }.to_string()),
        languages: vec![idx % 8],
        tier: "prod".to_string(),
        scope_path: None,
        lora_tier: None,
        base_model: None,
        ..Default::default()
    }
}

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);

    // Test 1: Features with special floating-point values
    let feature_len = u.int_in_range::<usize>(22..=30).unwrap_or(25);
    let mut features: Vec<f32> = Vec::with_capacity(feature_len);

    for _ in 0..feature_len {
        let val = match next_float(&mut u, true) {
            Some(v) => v,
            None => return,
        };
        features.push(val);
    }

    // Test 2: Priors with special floating-point values
    let adapter_count = u.int_in_range::<usize>(1..=MAX_K).unwrap_or(4);
    let mut priors: Vec<f32> = Vec::with_capacity(adapter_count);

    for _ in 0..adapter_count {
        let val = match next_float(&mut u, true) {
            Some(v) => v,
            None => return,
        };
        priors.push(val);
    }

    let adapters: Vec<AdapterInfo> = (0..adapter_count).map(build_adapter).collect();
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&ids, None);

    // Test 3: Router parameters with edge cases
    let k = u
        .int_in_range::<usize>(1..=adapter_count.min(MAX_K))
        .unwrap_or(2);

    // Temperature: may be zero, negative, special
    let tau = match next_float(&mut u, true) {
        Some(v) => v,
        None => return,
    };

    // Epsilon: may be special
    let eps = match next_float(&mut u, true) {
        Some(v) => v,
        None => return,
    };

    // Test 4: Router weights with special values
    let weights = if u.arbitrary::<bool>().unwrap_or(false) {
        // Custom weights with potential special values
        let w1 = next_float(&mut u, true).unwrap_or(0.5);
        let w2 = next_float(&mut u, true).unwrap_or(0.3);
        let w3 = next_float(&mut u, true).unwrap_or(0.2);
        let w4 = next_float(&mut u, true).unwrap_or(0.1);
        let w5 = next_float(&mut u, true).unwrap_or(0.1);
        let w6 = next_float(&mut u, true).unwrap_or(0.1);
        let w7 = next_float(&mut u, true).unwrap_or(0.1);
        let w8 = next_float(&mut u, true).unwrap_or(0.1);
        RouterWeights::new_with_dir_weights(w1, w2, w3, w4, w5, w6, w7, w8)
    } else {
        RouterWeights::default()
    };

    // Create router - should handle invalid parameters gracefully
    let mut router = Router::new_with_weights(weights.clone(), k, tau, eps);

    // Test 5: Route with potentially invalid inputs
    // This should NOT panic - it should return an error or handle gracefully
    let result = router.route_with_adapter_info(&features, &priors, &adapters, &policy_mask);

    // Verify result consistency if successful
    if let Ok(ref decision) = result {
        verify_decision_invariants(decision);
    }

    // Test 6: All-NaN features
    let nan_features: Vec<f32> = vec![f32::NAN; feature_len];
    let safe_priors: Vec<f32> = vec![1.0; adapter_count];
    let mut router2 =
        Router::new_with_weights(RouterWeights::default(), k.min(adapter_count), 1.0, 0.05);
    let _ = router2.route_with_adapter_info(&nan_features, &safe_priors, &adapters, &policy_mask);
    // Should not panic

    // Test 7: All-infinity features
    let inf_features: Vec<f32> = vec![f32::INFINITY; feature_len];
    let _ = router2.route_with_adapter_info(&inf_features, &safe_priors, &adapters, &policy_mask);
    // Should not panic

    // Test 8: All-infinity priors
    let inf_priors: Vec<f32> = vec![f32::INFINITY; adapter_count];
    let safe_features: Vec<f32> = vec![1.0; 25];
    let _ = router2.route_with_adapter_info(&safe_features, &inf_priors, &adapters, &policy_mask);
    // Should not panic

    // Test 9: Mixed special values
    let mixed_features: Vec<f32> = (0..25)
        .map(|i| match i % 5 {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => 0.0,
            _ => 1.0,
        })
        .collect();
    let _ = router2.route_with_adapter_info(&mixed_features, &safe_priors, &adapters, &policy_mask);
    // Should not panic

    // Test 10: Subnormal values (denormals)
    let subnormal_features: Vec<f32> = vec![1.0e-45; 25];
    let _ =
        router2.route_with_adapter_info(&subnormal_features, &safe_priors, &adapters, &policy_mask);
    // Should not panic

    // Test 11: Extreme large values
    let extreme_features: Vec<f32> = vec![f32::MAX; 25];
    let _ =
        router2.route_with_adapter_info(&extreme_features, &safe_priors, &adapters, &policy_mask);
    // Should not panic

    // Test 12: Zero temperature (hard routing)
    let mut router_zero_temp = Router::new_with_weights(RouterWeights::default(), 2, 0.0, 0.05);
    let _ = router_zero_temp.route_with_adapter_info(
        &safe_features,
        &safe_priors,
        &adapters,
        &policy_mask,
    );
    // Should not panic

    // Test 13: Negative temperature (invalid)
    let mut router_neg_temp = Router::new_with_weights(RouterWeights::default(), 2, -1.0, 0.05);
    let _ = router_neg_temp.route_with_adapter_info(
        &safe_features,
        &safe_priors,
        &adapters,
        &policy_mask,
    );
    // Should not panic

    // Test 14: Empty inputs
    let empty_features: Vec<f32> = vec![];
    let empty_priors: Vec<f32> = vec![];
    let empty_adapters: Vec<AdapterInfo> = vec![];
    let empty_mask = PolicyMask::allow_all(&[], None);
    let _ = router2.route_with_adapter_info(
        &empty_features,
        &empty_priors,
        &empty_adapters,
        &empty_mask,
    );
    // Should not panic

    // Test 15: Mismatched lengths
    let short_priors: Vec<f32> = vec![1.0; adapter_count.saturating_sub(1)];
    let _ = router2.route_with_adapter_info(&safe_features, &short_priors, &adapters, &policy_mask);
    // Should not panic - should return error or handle gracefully

    // Test 16: Quantization edge cases
    let quant_format = GateQuantFormat::q15();
    for special in SPECIAL_FLOATS {
        let q = quant_format.encode(*special);
        // Quantized value should be within valid Q15 range (non-negative for gates)
        assert!(
            q >= 0 && q <= (ROUTER_GATE_Q15_DENOM as i16),
            "Quantized value {} from {} out of valid gate range",
            q,
            special
        );
    }

    // Test 17: Softmax with special values in scores
    // Router::deterministic_softmax is pub(crate), so we test it indirectly
    // through routing with edge-case scores

    // Test 18: Verify determinism with valid inputs
    if let Some(decision_a) = result.as_ref().ok() {
        if !decision_a.indices.is_empty() {
            // Re-route with same inputs should give identical results
            let mut router_b = Router::new_with_weights(weights, k, tau, eps);
            if let Ok(decision_b) =
                router_b.route_with_adapter_info(&features, &priors, &adapters, &policy_mask)
            {
                // Determinism: identical inputs should yield identical outputs
                // (only if inputs were valid and processed consistently)
                if decision_a.indices == decision_b.indices {
                    assert_eq!(
                        decision_a.gates_q15, decision_b.gates_q15,
                        "Determinism violation: same indices but different gates"
                    );
                }
            }
        }
    }
});

/// Verify invariants that should hold for any valid Decision
fn verify_decision_invariants(decision: &Decision) {
    // Indices and gates must have same length
    assert_eq!(
        decision.indices.len(),
        decision.gates_q15.len(),
        "Indices and gates length mismatch"
    );

    // All gates should be within Q15 range
    for gate in &decision.gates_q15 {
        assert!(
            *gate >= -(ROUTER_GATE_Q15_DENOM as i16) && *gate <= (ROUTER_GATE_Q15_DENOM as i16),
            "Gate {} out of Q15 range",
            gate
        );
    }

    // Entropy should be finite and non-negative
    assert!(
        decision.entropy.is_finite() && decision.entropy >= 0.0,
        "Invalid entropy: {}",
        decision.entropy
    );

    // Indices should be unique
    let unique_count = decision
        .indices
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(
        unique_count,
        decision.indices.len(),
        "Duplicate indices in decision"
    );

    // Candidates should match indices/gates length
    assert_eq!(
        decision.candidates.len(),
        decision.indices.len(),
        "Candidates length mismatch"
    );

    // Verify gates_f32() produces finite values
    for gate in decision.gates_f32() {
        assert!(gate.is_finite(), "gates_f32() produced non-finite value");
    }
}
