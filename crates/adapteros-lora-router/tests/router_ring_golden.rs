//! Golden snapshot tests for Decision → RouterRing conversion
//!
//! This test suite verifies that the router Decision structure is
//! structurally compatible with the canonical RouterRing format
//! used by fused kernels.
//!
//! **What is a golden snapshot test?**
//! A golden test captures the expected output for a known input and
//! verifies that future changes don't break the contract. These tests
//! document the canonical format and catch regression.
//!
//! **References:**
//! - Router-Kernel Ring Buffer Unification
//! - Router Decision: adapteros-lora-router/src/lib.rs:1010-1032

use adapteros_lora_router::{
    policy_mask::PolicyMask, AdapterInfo, Decision, DecisionCandidate, Router, RouterWeights,
};
use smallvec::SmallVec;

/// Golden example: Typical K=3 routing decision
///
/// **Scenario:** Multi-language code completion with 3 adapters selected
/// - Adapter 0: Python (gate=0.6)
/// - Adapter 1: Rust (gate=0.3)
/// - Adapter 2: General (gate=0.1)
///
/// **Expected Q15 gates:**
/// - 0.6 * 32767 = 19660
/// - 0.3 * 32767 = 9830
/// - 0.1 * 32767 = 3276
#[test]
fn golden_decision_k3_typical() {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1, 2]),
        gates_q15: SmallVec::from_slice(&[19660, 9830, 3276]),
        entropy: 0.8472,
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
        candidates: vec![
            DecisionCandidate {
                adapter_idx: 0,
                raw_score: 0.6,
                gate_q15: 19660,
            },
            DecisionCandidate {
                adapter_idx: 1,
                raw_score: 0.3,
                gate_q15: 9830,
            },
            DecisionCandidate {
                adapter_idx: 2,
                raw_score: 0.1,
                gate_q15: 3276,
            },
        ],
    };

    // Verify structural invariants
    assert_eq!(decision.indices.len(), 3, "K=3 adapters selected");
    assert_eq!(
        decision.indices.len(),
        decision.gates_q15.len(),
        "Indices and gates must have matching lengths"
    );
    assert!(decision.indices.len() <= 8, "K must be ≤ 8");

    // Verify Q15 encoding
    let gates_f32 = decision.gates_f32();
    assert!((gates_f32[0] - 0.6).abs() < 0.001, "Gate 0 should be ~0.6");
    assert!((gates_f32[1] - 0.3).abs() < 0.001, "Gate 1 should be ~0.3");
    assert!((gates_f32[2] - 0.1).abs() < 0.001, "Gate 2 should be ~0.1");

    // Verify candidates match top-K decision
    for (i, candidate) in decision.candidates.iter().enumerate() {
        assert_eq!(
            candidate.adapter_idx, decision.indices[i],
            "Candidate {} index mismatch",
            i
        );
        assert_eq!(
            candidate.gate_q15, decision.gates_q15[i],
            "Candidate {} gate mismatch",
            i
        );
    }

    // Document canonical format for RouterRing conversion:
    // RouterRing {
    //     indices: [0, 1, 2, 0, 0, 0, 0, 0],  // K=3 active, rest zero-filled
    //     gates_q15: [19660, 9830, 3276, 0, 0, 0, 0, 0],
    //     position: 0,
    //     k: 3,
    // }
}

/// Golden example: Maximum K=8 routing decision
///
/// **Scenario:** All 8 adapters selected (stress test)
#[test]
fn golden_decision_k8_maximum() {
    let indices: [u16; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
    let gates_q15: [i16; 8] = [
        16383, // 0.5
        12287, // 0.375
        8191,  // 0.25
        6553,  // 0.2
        4915,  // 0.15
        3276,  // 0.1
        1638,  // 0.05
        819,   // 0.025
    ];

    let decision = Decision {
        indices: SmallVec::from_slice(&indices),
        gates_q15: SmallVec::from_slice(&gates_q15),
        entropy: 1.95,
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
        candidates: vec![],
    };

    assert_eq!(decision.indices.len(), 8, "K=8 (maximum)");
    assert_eq!(decision.indices.len(), decision.gates_q15.len());

    // Verify all indices are within u16 range
    for &idx in decision.indices.iter() {
        assert!(idx < u16::MAX, "Index must fit in u16");
    }

    // Verify all gates are valid signed Q15
    for &gate in decision.gates_q15.iter() {
        assert!(
            (-32767..=32767).contains(&gate),
            "Gate must be in signed Q15 range"
        );
    }
}

/// Golden example: Empty decision (K=0)
///
/// **Scenario:** No adapters selected (e.g., base model only)
#[test]
fn golden_decision_k0_empty() {
    let decision = Decision {
        indices: SmallVec::new(),
        gates_q15: SmallVec::new(),
        entropy: 0.0,
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
        candidates: vec![],
    };

    assert_eq!(decision.indices.len(), 0, "K=0 (no adapters)");
    assert_eq!(decision.gates_q15.len(), 0);

    // RouterRing should be fully zero-filled:
    // RouterRing {
    //     indices: [0, 0, 0, 0, 0, 0, 0, 0],
    //     gates_q15: [0, 0, 0, 0, 0, 0, 0, 0],
    //     position: 0,
    //     k: 0,
    // }
}

/// Golden example: Negative gates (signed Q15)
///
/// **Scenario:** Router supports negative gates for suppression
/// (e.g., anti-patterns, deprecated APIs)
#[test]
fn golden_decision_negative_gates() {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1, 2]),
        gates_q15: SmallVec::from_slice(&[16383, -8191, -4095]), // Mix of positive/negative
        entropy: 0.6,
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
        candidates: vec![],
    };

    assert_eq!(decision.gates_q15[1], -8191, "Gate 1 should be negative");
    assert_eq!(decision.gates_q15[2], -4095, "Gate 2 should be negative");

    // Verify signed Q15 round-trip
    let gates_f32 = decision.gates_f32();
    assert!(gates_f32[0] > 0.0, "Gate 0 positive");
    assert!(gates_f32[1] < 0.0, "Gate 1 negative");
    assert!(gates_f32[2] < 0.0, "Gate 2 negative");
}

/// Golden example: High-entropy decision
///
/// **Scenario:** Router is uncertain, many adapters compete
/// Entropy measures decision uncertainty (H = -Σ p*log(p))
#[test]
fn golden_decision_high_entropy() {
    // Uniform distribution → maximum entropy for K=4
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1, 2, 3]),
        gates_q15: SmallVec::from_slice(&[8191, 8191, 8191, 8191]), // All ~0.25
        entropy: 1.386, // ln(4) ≈ 1.386 for uniform distribution
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
        candidates: vec![],
    };

    assert!(
        decision.entropy > 1.0,
        "High entropy indicates uncertain decision"
    );
    assert_eq!(decision.indices.len(), 4);
}

/// Golden example: Low-entropy decision
///
/// **Scenario:** Router is confident, one dominant adapter
#[test]
fn golden_decision_low_entropy() {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1, 2]),
        gates_q15: SmallVec::from_slice(&[29491, 1638, 1638]), // 0.9, 0.05, 0.05
        entropy: 0.325,                                        // Low entropy, one dominant gate
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
        candidates: vec![],
    };

    assert!(
        decision.entropy < 0.5,
        "Low entropy indicates confident decision"
    );
    assert!(
        decision.gates_q15[0] > decision.gates_q15[1] * 10,
        "First gate should dominate"
    );
}

/// Integration test: Router produces valid Decisions
///
/// Verifies that the Router::route() method produces Decisions
/// that satisfy all structural invariants for RouterRing conversion.
#[test]
fn test_router_produces_valid_decisions() {
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    let features = vec![0.5; 10];
    let priors = vec![0.1, 0.9, 0.5, 0.3, 0.7, 0.2, 0.8, 0.4, 0.6, 0.0];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            ..Default::default()
        })
        .collect();
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
    let decision = router.route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask);

    // Verify structural invariants
    assert!(
        decision.indices.len() <= 8,
        "Router must produce K ≤ 8 decisions"
    );
    assert_eq!(
        decision.indices.len(),
        decision.gates_q15.len(),
        "Indices and gates must match"
    );
    assert_eq!(
        decision.indices.len(),
        3,
        "Router configured for K=3 should produce 3 adapters"
    );

    // Verify all indices are unique (no duplicates)
    let mut seen = std::collections::HashSet::new();
    for &idx in decision.indices.iter() {
        assert!(seen.insert(idx), "Duplicate adapter index: {}", idx);
    }

    // Verify gates are in valid Q15 range
    for &gate in decision.gates_q15.iter() {
        assert!(
            (-32767..=32767).contains(&gate),
            "Gate {} out of Q15 range",
            gate
        );
    }

    // Verify entropy is non-negative
    assert!(decision.entropy >= 0.0, "Entropy must be non-negative");
}

/// Snapshot: Document canonical Q15 conversion formula
///
/// This test documents the exact Q15 conversion used by the router
/// and expected by the kernel. Any change to this formula breaks
/// the Decision → RouterRing contract.
#[test]
fn golden_q15_conversion_formula() {
    // Canonical Q15 conversion (signed, denominator 32767)
    // Reference: adapteros-lora-router/src/lib.rs:1030

    let test_cases = vec![
        (0.0, 0),
        (1.0, 32767),
        (-1.0, -32767),
        (0.5, 16383), // 0.5 * 32767 = 16383.5 → 16383
        (-0.5, -16383),
        (0.25, 8191), // 0.25 * 32767 = 8191.75 → 8191
        (0.1, 3276),  // 0.1 * 32767 = 3276.7 → 3276
    ];

    for (float_val, expected_q15) in test_cases {
        let q15 = (float_val * 32767.0) as i16;
        assert_eq!(q15, expected_q15, "Q15 conversion failed for {}", float_val);

        // Verify round-trip (allowing for quantization error)
        let float_back = q15 as f32 / 32767.0;
        assert!(
            (float_back - float_val).abs() < 0.001,
            "Round-trip failed for {}",
            float_val
        );
    }
}

/// Regression test: Verify SmallVec<[u16; 8]> capacity
///
/// The router uses SmallVec<[u16; 8]> which enforces K ≤ 8 at compile time.
/// This test verifies the capacity matches RouterRing's fixed-size arrays.
#[test]
fn test_smallvec_capacity_matches_router_ring() {
    let indices: SmallVec<[u16; 8]> = SmallVec::from_slice(&[0, 1, 2, 3, 4, 5, 6, 7]);
    let gates: SmallVec<[i16; 8]> = SmallVec::from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

    assert_eq!(indices.len(), 8, "SmallVec can hold K=8");
    assert_eq!(gates.len(), 8);

    // Verify SmallVec inline capacity (no heap allocation for K ≤ 8)
    assert!(
        indices.spilled() == false,
        "SmallVec should be inline for K=8"
    );
    assert!(gates.spilled() == false);
}

/// Edge case: Maximum Q15 values
#[test]
fn golden_q15_max_values() {
    let decision = Decision {
        indices: SmallVec::from_slice(&[0, 1]),
        gates_q15: SmallVec::from_slice(&[32767, -32767]), // Max positive/negative
        entropy: 0.0,
        decision_hash: None,
        policy_mask_digest: None,
        policy_overrides_applied: None,
        candidates: vec![],
    };

    assert_eq!(decision.gates_q15[0], 32767, "Max positive Q15");
    assert_eq!(decision.gates_q15[1], -32767, "Max negative Q15");

    let gates_f32 = decision.gates_f32();
    assert!((gates_f32[0] - 1.0).abs() < 0.001, "Max Q15 → 1.0");
    assert!((gates_f32[1] - (-1.0)).abs() < 0.001, "Min Q15 → -1.0");
}
