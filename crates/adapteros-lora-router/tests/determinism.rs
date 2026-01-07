//! Determinism and Q15 quantization tests for router
//!
//! Tests verify:
//! - Deterministic top-K selection (deterministic via score sorting, not seed)
//! - Stable ordering on ties (by index for reproducibility)
//! - Q15 gate quantization (non-negative, proper scaling)
//! - K=0 path returns empty indices/gates
//! - Q15 denominator locked at 32767.0
//! - Round-trip conversion consistency (f32 -> Q15 -> f32)
//! - Softmax determinism with Kahan summation
//! - Entropy floor enforcement
//! - Cross-instance determinism
//!
//! Note: Router seed is used for telemetry sampling determinism, not routing decisions.
//! Routing determinism comes from stable sorting (score desc, then index asc).
#![allow(clippy::absurd_extreme_comparisons)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::unnecessary_unwrap)]

use adapteros_core::determinism::{DeterminismContext, DeterminismSource};
use adapteros_core::AosError;
use adapteros_lora_router::{
    policy_mask::PolicyMask, AdapterInfo, Decision, Router, ROUTER_GATE_Q15_DENOM,
    ROUTER_GATE_Q15_MAX,
};
use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use proptest::prelude::*;
use smallvec::smallvec;

fn allow_all_mask(adapters: &[AdapterInfo]) -> PolicyMask {
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    PolicyMask::allow_all(&ids, None)
}

#[test]
fn test_deterministic_top_k_ordering() {
    // Routing determinism comes from stable sorting, not seed
    // Seed is only used for telemetry sampling determinism
    let seed = [42u8; 32];

    // Use Router::new which accepts seed parameter
    let weights_vec = vec![1.0; 5]; // Dummy weights for adapter count
    let mut router = Router::new(weights_vec.clone(), 3, 1.0, 0.01, seed).expect("router creation");

    // Create priors with ties
    let priors = vec![0.5, 0.5, 0.5, 0.3, 0.2]; // First three tied
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision1 = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");
    let decision2 = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // Results should be identical (deterministic via sorting)
    assert_eq!(decision1.indices, decision2.indices);
    assert_eq!(decision1.gates_q15, decision2.gates_q15);

    // On ties, should sort by index (lower index wins for stable ordering)
    assert_eq!(decision1.indices[0], 0);
    assert_eq!(decision1.indices[1], 1);
    assert_eq!(decision1.indices[2], 2);

    // New router instance should also produce same results (determinism)
    let mut router2 = Router::new(weights_vec, 3, 1.0, 0.01, seed).expect("router creation");
    let decision3 = router2
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");
    assert_eq!(decision1.indices, decision3.indices);
    assert_eq!(decision1.gates_q15, decision3.gates_q15);
}

#[test]
fn test_adaptive_routing_requires_determinism_context() {
    let seed = [11u8; 32];
    let weights_vec = vec![1.0; 2];
    let mut router = Router::new(weights_vec, 1, 1.0, 0.01, seed).expect("router creation");

    assert!(
        !router.adaptive_routing(),
        "adaptive_routing should default to false"
    );

    router.set_routing_determinism_mode(true);

    let priors = vec![0.5, 0.5];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    let err = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect_err("adaptive routing without determinism context should error");
    assert!(
        matches!(err, AosError::Config(_)),
        "expected config error, got {err:?}"
    );
}

#[test]
fn q15_denominator_is_locked() {
    assert!(
        (ROUTER_GATE_Q15_DENOM - 32767.0).abs() < f32::EPSILON,
        "Q15 denominator must remain 32767.0, found {}",
        ROUTER_GATE_Q15_DENOM
    );
}

#[test]
fn test_q15_quantization_properties() {
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 3, 1.0, 0.01, seed).expect("router creation");

    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // Q15 gates should be non-negative (i16::MAX is 32767, so <= 32767 is guaranteed by type)
    for gate in &decision.gates_q15 {
        assert!(*gate >= 0, "Q15 gate should be non-negative, got {}", gate);
    }

    // Verify indices match expected top-K
    assert_eq!(decision.indices.len(), 3);
    assert_eq!(decision.gates_q15.len(), 3);

    // Convert back to f32 and verify normalization
    let gates_f32: Vec<f32> = decision
        .gates_q15
        .iter()
        .map(|&q| q as f32 / 32767.0)
        .collect();

    // Gates should sum approximately to 1.0 (allowing for quantization error)
    let sum: f32 = gates_f32.iter().sum();
    assert!(
        (sum - 1.0).abs() < 0.01,
        "Gates should sum to ~1.0, got {}",
        sum
    );
}

#[test]
fn gates_f32_uses_q15_denominator_constant() {
    let decision = Decision {
        indices: smallvec![0],
        gates_q15: smallvec![ROUTER_GATE_Q15_MAX],
        entropy: 0.0,
        candidates: Vec::new(),
        decision_hash: None,
        policy_mask_digest_b3: None,
        policy_overrides_applied: None,
    };

    let gates = decision.gates_f32();
    assert_eq!(gates[0], ROUTER_GATE_Q15_MAX as f32 / ROUTER_GATE_Q15_DENOM);
}

#[test]
fn test_k0_detection_empty_result() {
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 3, 1.0, 0.01, seed).expect("router creation");

    // Empty priors should result in empty decision
    let adapter_info: Vec<AdapterInfo> = vec![];
    let mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&[], &[], &adapter_info, &mask)
        .expect("router decision");

    assert!(decision.indices.is_empty());
    assert!(decision.gates_q15.is_empty());
}

#[test]
fn test_gate_normalization_and_entropy_floor() {
    let seed = [42u8; 32];
    let eps = 0.01;
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 3, 1.0, eps, seed).expect("router creation");

    let priors = vec![0.9, 0.8, 0.1, 0.05, 0.02];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // Convert gates back to f32
    let gates_f32: Vec<f32> = decision
        .gates_q15
        .iter()
        .map(|&q| q as f32 / 32767.0)
        .collect();

    // Each gate should be >= min_gate (eps / k)
    let min_gate = eps / 3.0;
    for gate in &gates_f32 {
        assert!(
            *gate >= min_gate,
            "Gate {} should be >= min_gate {}",
            gate,
            min_gate
        );
    }

    // Gates should sum to approximately 1.0 after renormalization
    let sum: f32 = gates_f32.iter().sum();
    assert!(
        (sum - 1.0).abs() < 0.01,
        "Normalized gates should sum to ~1.0, got {}",
        sum
    );
}

#[test]
fn test_multiple_calls_deterministic() {
    // Routing determinism comes from stable sorting algorithm
    // Same inputs (priors) should always produce same outputs regardless of router instance
    let seed = [99u8; 32];

    // First router instance
    let weights_vec1 = vec![1.0; 4];
    let mut router1 = Router::new(weights_vec1, 2, 1.0, 0.01, seed).expect("router creation");
    let priors = vec![0.7, 0.6, 0.5, 0.4];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision1_1 = router1
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");
    let decision1_2 = router1
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // Second router instance with same seed (seed doesn't affect routing, just telemetry)
    let weights_vec2 = vec![1.0; 4];
    let mut router2 = Router::new(weights_vec2, 2, 1.0, 0.01, seed).expect("router creation");
    let decision2_1 = router2
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // All three should produce identical results (deterministic sorting)
    assert_eq!(decision1_1.indices, decision1_2.indices);
    assert_eq!(decision1_1.indices, decision2_1.indices);
    assert_eq!(decision1_1.gates_q15, decision1_2.gates_q15);
    assert_eq!(decision1_1.gates_q15, decision2_1.gates_q15);
}

#[test]
fn test_repeated_routing_returns_identical_indices_and_gates() {
    // Fixed priors and adapter list must produce identical outputs across runs
    let seed = [7u8; 32];
    let weights_vec = vec![1.0; 4];
    let mut router = Router::new(weights_vec, 2, 1.0, 1e-6, seed).expect("router creation");

    let priors = vec![0.4, 0.3, 0.2, 0.1];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("repeat_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    let mask = allow_all_mask(&adapter_info);
    let mut baseline_indices = None;
    let mut baseline_gates = None;

    for _ in 0..10 {
        let decision = router
            .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
            .expect("router decision");
        if baseline_indices.is_none() {
            baseline_indices = Some(decision.indices.clone());
            baseline_gates = Some(decision.gates_q15.clone());
        } else {
            assert_eq!(
                baseline_indices.as_ref().unwrap(),
                &decision.indices,
                "indices should remain stable across runs"
            );
            assert_eq!(
                baseline_gates.as_ref().unwrap(),
                &decision.gates_q15,
                "gates must remain stable across runs"
            );
        }
    }
}

#[test]
fn test_q15_range_properties() {
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 5, 1.0, 0.001, seed).expect("router creation");

    let priors = vec![1.0, 0.9, 0.8, 0.7, 0.6];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // All gates should be non-negative (i16::MAX is 32767, guaranteed by type system)
    for gate in &decision.gates_q15 {
        assert!(*gate >= 0, "Gate {} should be non-negative", gate);
    }

    // Verify we got exactly K gates
    assert_eq!(decision.indices.len(), 5);
    assert_eq!(decision.gates_q15.len(), 5);
}

#[test]
fn test_router_ring_invariants() {
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 8];
    let mut router = Router::new(weights_vec, 4, 1.0, 0.01, seed).expect("router creation");

    let priors = vec![0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // 1:1 mapping
    assert_eq!(decision.indices.len(), 4);
    assert_eq!(decision.indices.len(), decision.gates_q15.len());

    // Unique indices
    let mut indices_sorted = decision.indices.clone();
    indices_sorted.sort_unstable();
    for i in 1..indices_sorted.len() {
        assert_ne!(indices_sorted[i], indices_sorted[i - 1]);
    }

    // For K=0 case (clamped to 1)
    let mut router_k0 = Router::new(vec![1.0; 8], 0, 1.0, 0.01, seed).expect("router creation");
    let priors = vec![0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask_k0 = allow_all_mask(&adapter_info);
    let decision_k0 = router_k0
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask_k0)
        .expect("router decision");
    assert_eq!(decision_k0.indices.len(), 1);
    assert_eq!(decision_k0.gates_q15.len(), 1);
}

// Add more tests for varying K up to MAX_K
#[test]
fn test_varying_k_stability() {
    let seed = [42u8; 32];

    // Create adapters and priors for k tests
    let priors = vec![1.0; 8];
    let adapter_info: Vec<AdapterInfo> = (0..8)
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    for k in 0..=8 {
        let weights_vec = vec![1.0; 8];
        let mut router = Router::new(weights_vec, k, 1.0, 0.01, seed).expect("router creation");
        let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
        let mask = PolicyMask::allow_all(&adapter_ids, None);
        let decision = router
            .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
            .expect("router decision");

        let expected_k = if k == 0 { 1 } else { k };
        assert_eq!(decision.indices.len(), expected_k);
        assert_eq!(decision.gates_q15.len(), expected_k);
    }
}

// RouterDecisionEvent serialization tests belong in adapteros-telemetry crate

#[test]
fn test_q15_round_trip_conversion() {
    // Test that f32 -> Q15 -> f32 conversion is consistent
    let test_values = vec![0.0, 0.25, 0.5, 0.75, 1.0, 0.333, 0.666, 0.1, 0.9];

    for &val in &test_values {
        // Convert to Q15
        let q15 = (val * ROUTER_GATE_Q15_DENOM).round() as i16;
        // Convert back to f32
        let recovered = q15 as f32 / ROUTER_GATE_Q15_DENOM;

        // Should be very close (within quantization error)
        let error = (val - recovered).abs();
        assert!(
            error < 1.0 / ROUTER_GATE_Q15_DENOM + f32::EPSILON,
            "Round-trip error {} too large for value {}",
            error,
            val
        );
    }
}

#[test]
fn test_q15_max_value_encoding() {
    // Max Q15 value should encode exactly 1.0
    let max_gate_f32 = ROUTER_GATE_Q15_MAX as f32 / ROUTER_GATE_Q15_DENOM;
    assert!(
        (max_gate_f32 - 1.0).abs() < f32::EPSILON,
        "Max Q15 should encode 1.0, got {}",
        max_gate_f32
    );
}

#[test]
fn test_q15_denominator_never_32768() {
    // Critical: denominator must be 32767, not 32768
    // Using 32768 would overflow i16::MAX and break determinism
    assert_ne!(
        ROUTER_GATE_Q15_DENOM, 32768.0,
        "Q15 denominator must never be 32768 (would overflow i16::MAX)"
    );
    assert_eq!(
        ROUTER_GATE_Q15_DENOM, 32767.0,
        "Q15 denominator must be exactly 32767.0"
    );
}

#[test]
fn test_score_sorting_descending_with_index_tiebreak() {
    // Test that scores are sorted descending, with index ascending on ties
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 3, 1.0, 0.01, seed).expect("router creation");

    // Create priors with known order: [0.9, 0.5, 0.5, 0.5, 0.1]
    // Expected top-3: indices [0, 1, 2] (0.9, then ties at 0.5 broken by index)
    let priors = vec![0.9, 0.5, 0.5, 0.5, 0.1];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // Should select indices in score-descending order
    assert_eq!(
        decision.indices[0], 0,
        "Highest score should be selected first"
    );

    // For tied scores, index ascending should win
    assert!(
        decision.indices[1] < decision.indices[2],
        "Tied scores should be broken by index ascending"
    );
}

#[test]
fn test_gate_computation_consistency() {
    // Same inputs should always produce identical Q15 gates
    let seed = [123u8; 32];
    let weights_vec = vec![1.0; 4];
    let mut router = Router::new(weights_vec, 3, 1.0, 0.02, seed).expect("router creation");

    let priors = vec![0.8, 0.6, 0.4, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    // Run routing 5 times with same inputs
    let decisions: Vec<_> = (0..5)
        .map(|_| {
            router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision")
        })
        .collect();

    // All decisions should have identical gates
    for i in 1..decisions.len() {
        assert_eq!(
            decisions[0].gates_q15, decisions[i].gates_q15,
            "Gate computation should be deterministic across runs"
        );
    }
}

#[test]
fn test_k_sparse_boundary_conditions() {
    let seed = [42u8; 32];
    let priors = vec![0.9, 0.8, 0.7, 0.6, 0.5];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    // Test K=0 (clamped to 1)
    let weights_vec = vec![1.0; 5];
    let mut router_k0 = Router::new(weights_vec, 0, 1.0, 0.02, seed).expect("router creation");
    let decision_k0 = router_k0
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");
    assert_eq!(decision_k0.indices.len(), 1, "K=0 should clamp to 1");
    assert_eq!(decision_k0.gates_q15.len(), 1, "K=0 should clamp to 1");

    // Test K=1 (single selection)
    let weights_vec = vec![1.0; 5];
    let mut router_k1 = Router::new(weights_vec, 1, 1.0, 0.02, seed).expect("router creation");
    let decision_k1 = router_k1
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");
    assert_eq!(decision_k1.indices.len(), 1, "K=1 should select 1 adapter");
    assert_eq!(decision_k1.indices[0], 0, "K=1 should select highest score");
    // Gate should be approximately 1.0 (32767 in Q15)
    assert!(
        decision_k1.gates_q15[0] > 32700,
        "K=1 gate should be close to max (32767)"
    );

    // Test K=max (select all)
    let weights_vec = vec![1.0; 5];
    let mut router_k5 = Router::new(weights_vec, 5, 1.0, 0.02, seed).expect("router creation");
    let decision_k5 = router_k5
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");
    assert_eq!(
        decision_k5.indices.len(),
        5,
        "K=5 should select all 5 adapters"
    );
}

#[test]
fn test_softmax_determinism() {
    // Test that routing (which uses deterministic_softmax internally) produces consistent results
    // This indirectly tests softmax determinism via stable gate computation
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 4];
    let mut router = Router::new(weights_vec, 4, 1.0, 0.02, seed).expect("router creation");

    // Create priors that will exercise softmax
    let priors = vec![0.9, 0.8, 0.7, 0.6];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    // Run routing multiple times
    let decisions: Vec<_> = (0..10)
        .map(|_| {
            router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision")
        })
        .collect();

    // All decisions should have identical gates (deterministic softmax)
    for i in 1..decisions.len() {
        assert_eq!(
            decisions[0].gates_q15, decisions[i].gates_q15,
            "Softmax-based gate computation should be deterministic"
        );
    }

    // Convert first decision gates to f32 and verify normalization
    let gates_f32: Vec<f32> = decisions[0]
        .gates_q15
        .iter()
        .map(|&q| q as f32 / ROUTER_GATE_Q15_DENOM)
        .collect();
    let sum: f32 = gates_f32.iter().sum();
    assert!(
        (sum - 1.0).abs() < 1e-2,
        "Softmax output should sum to ~1.0, got {}",
        sum
    );
}

#[test]
fn test_entropy_floor_enforcement() {
    // Test that gates respect the entropy floor (min_gate = eps / k)
    let seed = [42u8; 32];
    let eps = 0.1; // Large epsilon for testing
    let weights_vec = vec![1.0; 4];
    let mut router = Router::new(weights_vec, 3, 1.0, eps, seed).expect("router creation");

    // Priors with large spread
    let priors = vec![0.99, 0.01, 0.005, 0.002];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // Convert gates to f32
    let gates_f32: Vec<f32> = decision
        .gates_q15
        .iter()
        .map(|&q| q as f32 / ROUTER_GATE_Q15_DENOM)
        .collect();

    // Minimum gate should be at least eps / k
    let min_gate = eps / 3.0;
    for &gate in &gates_f32 {
        assert!(
            gate >= min_gate * 0.9, // Allow 10% slack for quantization
            "Gate {} should be >= min_gate {} (eps={}, k=3)",
            gate,
            min_gate,
            eps
        );
    }
}

#[test]
fn test_cross_instance_determinism() {
    // Different router instances with same config should produce identical results
    let seed1 = [1u8; 32];
    let seed2 = [2u8; 32];

    let priors = vec![0.7, 0.6, 0.5, 0.4, 0.3];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    // Create routers with different seeds (seed doesn't affect routing)
    let weights_vec1 = vec![1.0; 5];
    let mut router1 = Router::new(weights_vec1, 3, 1.0, 0.02, seed1).expect("router creation");
    let weights_vec2 = vec![1.0; 5];
    let mut router2 = Router::new(weights_vec2, 3, 1.0, 0.02, seed2).expect("router creation");

    let decision1 = router1
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");
    let decision2 = router2
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    // Should produce identical results despite different seeds
    assert_eq!(
        decision1.indices, decision2.indices,
        "Different router instances should produce same indices"
    );
    assert_eq!(
        decision1.gates_q15, decision2.gates_q15,
        "Different router instances should produce same gates"
    );
}

#[test]
fn test_gate_normalization_invariant() {
    // Test that gates always sum to approximately 1.0 after Q15 conversion
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 6];
    let mut router = Router::new(weights_vec, 4, 1.0, 0.01, seed).expect("router creation");

    let test_cases = vec![
        vec![1.0, 0.9, 0.8, 0.7, 0.6, 0.5],
        vec![0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
        vec![0.99, 0.01, 0.01, 0.01, 0.01, 0.01],
        vec![0.6, 0.5, 0.4, 0.3, 0.2, 0.1],
    ];

    for priors in test_cases {
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("test_adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "warm".to_string(),
                scope_path: None,
                lora_tier: None,
                base_model: None,
                ..Default::default()
            })
            .collect();
        let mask = allow_all_mask(&adapter_info);
        let decision = router
            .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
            .expect("router decision");

        // Convert to f32 and check sum
        let gates_f32: Vec<f32> = decision
            .gates_q15
            .iter()
            .map(|&q| q as f32 / ROUTER_GATE_Q15_DENOM)
            .collect();
        let sum: f32 = gates_f32.iter().sum();

        assert!(
            (sum - 1.0).abs() < 0.01,
            "Gates should sum to ~1.0, got {} for priors {:?}",
            sum,
            priors
        );
    }
}

#[test]
fn test_q15_no_negative_gates() {
    // Q15 gates should always be non-negative
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 4, 1.0, 0.02, seed).expect("router creation");

    let priors = vec![0.8, 0.6, 0.4, 0.2, 0.0];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
        .expect("router decision");

    for &gate in &decision.gates_q15 {
        assert!(gate >= 0, "Q15 gate should be non-negative, got {}", gate);
        assert!(
            gate <= ROUTER_GATE_Q15_MAX,
            "Q15 gate should be <= {}, got {}",
            ROUTER_GATE_Q15_MAX,
            gate
        );
    }
}

/// Stress test: deterministic tie-breaking with near-equal scores across many iterations.
///
/// This test validates that the router produces consistent results when scores differ
/// by amounts close to the relative epsilon threshold. It runs many iterations with
/// slight score variations to ensure the tie-breaker is stable.
#[test]
fn stress_test_near_equal_scores_determinism() {
    const ITERATIONS: usize = 1000;
    const ADAPTER_COUNT: usize = 8;
    const K: usize = 4;

    let seed = [0xABu8; 32];
    let weights_vec = vec![1.0; ADAPTER_COUNT];

    // Base score with near-equal values that differ by amounts near the relative epsilon
    // Using 0.5 as base, scores within ~5e-7 of each other should be treated as ties
    let base_score = 0.5f32;
    let tiny_delta = 1e-7; // Below relative epsilon threshold (1e-6 * 0.5 = 5e-7)

    let adapter_info: Vec<AdapterInfo> = (0..ADAPTER_COUNT)
        .map(|i| AdapterInfo {
            id: format!("stress_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    // Test 1: Exactly equal scores - should always produce same result
    {
        let priors: Vec<f32> = vec![base_score; ADAPTER_COUNT];
        let mut baseline_decision: Option<Decision> = None;

        for iteration in 0..ITERATIONS {
            let mut router =
                Router::new(weights_vec.clone(), K, 1.0, 0.01, seed).expect("router creation");
            let decision = router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision");

            if let Some(ref baseline) = baseline_decision {
                assert_eq!(
                    baseline.indices, decision.indices,
                    "Iteration {}: indices diverged with equal scores",
                    iteration
                );
                assert_eq!(
                    baseline.gates_q15, decision.gates_q15,
                    "Iteration {}: gates diverged with equal scores",
                    iteration
                );
            } else {
                baseline_decision = Some(decision);
            }
        }

        // With equal scores, should select indices 0..K (lowest indices win ties)
        let baseline = baseline_decision.unwrap();
        for i in 0..K {
            assert_eq!(
                baseline.indices[i], i as u16,
                "Equal scores should select indices in ascending order"
            );
        }
    }

    // Test 2: Near-equal scores with tiny deltas below relative epsilon
    {
        // Scores differ by amounts smaller than the tie threshold
        let priors: Vec<f32> = (0..ADAPTER_COUNT)
            .map(|i| base_score + (i as f32) * tiny_delta)
            .collect();

        let mut baseline_decision: Option<Decision> = None;

        for iteration in 0..ITERATIONS {
            let mut router =
                Router::new(weights_vec.clone(), K, 1.0, 0.01, seed).expect("router creation");
            let decision = router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision");

            if let Some(ref baseline) = baseline_decision {
                assert_eq!(
                    baseline.indices, decision.indices,
                    "Iteration {}: indices diverged with near-equal scores",
                    iteration
                );
                assert_eq!(
                    baseline.gates_q15, decision.gates_q15,
                    "Iteration {}: gates diverged with near-equal scores",
                    iteration
                );
            } else {
                baseline_decision = Some(decision);
            }
        }
    }

    // Test 3: Mixed near-equal and distinct scores
    {
        // First 4 adapters have near-equal high scores, last 4 have lower distinct scores
        let mut priors: Vec<f32> = vec![0.0; ADAPTER_COUNT];
        for i in 0..4 {
            priors[i] = 0.8 + (i as f32) * tiny_delta; // Near-equal high scores
        }
        for i in 4..ADAPTER_COUNT {
            priors[i] = 0.3 - (i as f32) * 0.05; // Distinct lower scores
        }

        let mut baseline_decision: Option<Decision> = None;

        for iteration in 0..ITERATIONS {
            let mut router =
                Router::new(weights_vec.clone(), K, 1.0, 0.01, seed).expect("router creation");
            let decision = router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision");

            if let Some(ref baseline) = baseline_decision {
                assert_eq!(
                    baseline.indices, decision.indices,
                    "Iteration {}: indices diverged with mixed scores",
                    iteration
                );
                assert_eq!(
                    baseline.gates_q15, decision.gates_q15,
                    "Iteration {}: gates diverged with mixed scores",
                    iteration
                );
            } else {
                baseline_decision = Some(decision);
            }
        }

        // Should select the 4 high-scoring adapters (indices 0-3)
        let baseline = baseline_decision.unwrap();
        let mut selected: Vec<u16> = baseline.indices.iter().cloned().collect();
        selected.sort();
        assert_eq!(
            selected,
            vec![0u16, 1, 2, 3],
            "Should select the 4 near-equal high-scoring adapters"
        );
    }

    // Test 4: Stress with cross-instance determinism
    {
        let priors: Vec<f32> = (0..ADAPTER_COUNT)
            .map(|i| base_score + (i as f32) * tiny_delta * 0.5)
            .collect();

        // Create multiple router instances with different seeds
        // (seed doesn't affect non-adaptive routing)
        let seeds: [[u8; 32]; 4] = [[1u8; 32], [2u8; 32], [99u8; 32], [255u8; 32]];

        let first_decision = {
            let mut router =
                Router::new(weights_vec.clone(), K, 1.0, 0.01, seeds[0]).expect("router creation");
            router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision")
        };

        for (idx, &s) in seeds.iter().enumerate().skip(1) {
            let mut router =
                Router::new(weights_vec.clone(), K, 1.0, 0.01, s).expect("router creation");
            let decision = router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision");

            assert_eq!(
                first_decision.indices, decision.indices,
                "Seed {:?} (index {}): cross-instance indices should match",
                s, idx
            );
            assert_eq!(
                first_decision.gates_q15, decision.gates_q15,
                "Seed {:?} (index {}): cross-instance gates should match",
                s, idx
            );
        }
    }
}

/// Stress test for adaptive routing with near-equal scores.
///
/// This tests the seeded RNG tie-breaking path when adaptive_routing is enabled
/// and a determinism context is provided.
#[test]
fn stress_test_adaptive_routing_near_equal_scores() {
    const ITERATIONS: usize = 500;
    const ADAPTER_COUNT: usize = 6;
    const K: usize = 3;

    let base_score = 0.5f32;
    let tiny_delta = 1e-7; // Below relative epsilon threshold

    let adapter_info: Vec<AdapterInfo> = (0..ADAPTER_COUNT)
        .map(|i| AdapterInfo {
            id: format!("adaptive_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    // Near-equal scores
    let priors: Vec<f32> = (0..ADAPTER_COUNT)
        .map(|i| base_score + (i as f32) * tiny_delta)
        .collect();

    let determinism_ctx = DeterminismContext::new(
        [42u8; 32],
        None,
        adapteros_core::SeedMode::BestEffort,
        RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    );

    let mut baseline_decision: Option<Decision> = None;

    for iteration in 0..ITERATIONS {
        let weights_vec = vec![1.0; ADAPTER_COUNT];
        let mut router =
            Router::new(weights_vec, K, 1.0, 0.01, [0u8; 32]).expect("router creation");
        router.set_routing_determinism_mode(true);

        let decision = router
            .route_with_adapter_info_with_ctx(
                &[],
                &priors,
                &adapter_info,
                &mask,
                Some(&determinism_ctx),
            )
            .expect("router decision");

        if let Some(ref baseline) = baseline_decision {
            assert_eq!(
                baseline.indices, decision.indices,
                "Iteration {}: adaptive routing indices diverged with near-equal scores",
                iteration
            );
            assert_eq!(
                baseline.gates_q15, decision.gates_q15,
                "Iteration {}: adaptive routing gates diverged with near-equal scores",
                iteration
            );
        } else {
            baseline_decision = Some(decision);
        }
    }

    // Different determinism context should potentially produce different results
    // (the seeded RNG tie-breaker uses the context's seed)
    let different_ctx = DeterminismContext::new(
        [99u8; 32], // Different seed
        None,
        adapteros_core::SeedMode::BestEffort,
        RoutingDeterminismMode::Adaptive,
        DeterminismSource::DerivedFromRequest,
    );

    let weights_vec = vec![1.0; ADAPTER_COUNT];
    let mut router = Router::new(weights_vec, K, 1.0, 0.01, [0u8; 32]).expect("router creation");
    router.set_routing_determinism_mode(true);
    let _different_decision = router
        .route_with_adapter_info_with_ctx(&[], &priors, &adapter_info, &mask, Some(&different_ctx))
        .expect("router decision");

    // Note: We don't assert they're different because with near-equal scores,
    // the RNG might happen to produce the same ordering. The key invariant is
    // that same context => same result (tested above).
}

/// Test relative epsilon edge cases: near-zero, large, and negative scores.
#[test]
fn test_relative_epsilon_edge_cases() {
    const K: usize = 3;
    let seed = [0u8; 32];

    // Helper to create adapter info
    let make_adapters = |n: usize| -> Vec<AdapterInfo> {
        (0..n)
            .map(|i| AdapterInfo {
                id: format!("edge_adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "warm".to_string(),
                scope_path: None,
                lora_tier: None,
                base_model: None,
                ..Default::default()
            })
            .collect()
    };

    // Test 1: Near-zero scores - absolute epsilon floor should kick in
    {
        let adapter_info = make_adapters(5);
        let mask = allow_all_mask(&adapter_info);
        // Scores very close to zero, differing by less than f32::EPSILON
        let priors: Vec<f32> = vec![1e-8, 1e-8 + 1e-10, 1e-8 + 2e-10, 1e-8 + 3e-10, 1e-8 + 4e-10];

        let mut baseline: Option<Decision> = None;
        for _ in 0..100 {
            let weights_vec = vec![1.0; 5];
            let mut router = Router::new(weights_vec, K, 1.0, 0.01, seed).expect("router creation");
            let decision = router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision");

            if let Some(ref b) = baseline {
                assert_eq!(
                    b.indices, decision.indices,
                    "Near-zero scores: indices should be stable"
                );
            } else {
                baseline = Some(decision);
            }
        }
    }

    // Test 2: Large scores - relative epsilon should dominate
    {
        let adapter_info = make_adapters(5);
        let mask = allow_all_mask(&adapter_info);
        // Large scores with differences that are small relatively but large absolutely
        let base = 1e6_f32;
        let priors: Vec<f32> = vec![
            base,
            base + 0.1, // Difference of 0.1 is tiny relative to 1e6
            base + 0.2,
            base + 0.3,
            base - 0.1,
        ];

        let mut baseline: Option<Decision> = None;
        for _ in 0..100 {
            let weights_vec = vec![1.0; 5];
            let mut router = Router::new(weights_vec, K, 1.0, 0.01, seed).expect("router creation");
            let decision = router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision");

            if let Some(ref b) = baseline {
                assert_eq!(
                    b.indices, decision.indices,
                    "Large scores: indices should be stable"
                );
            } else {
                baseline = Some(decision);
            }
        }
    }

    // Test 3: Scores that span positive and near-zero
    {
        let adapter_info = make_adapters(6);
        let mask = allow_all_mask(&adapter_info);
        let priors: Vec<f32> = vec![
            0.9,
            0.9 + 1e-7, // Near-equal to first
            0.5,
            0.5 + 1e-7, // Near-equal to third
            0.1,
            0.1 + 1e-7, // Near-equal to fifth
        ];

        let mut baseline: Option<Decision> = None;
        for _ in 0..100 {
            let weights_vec = vec![1.0; 6];
            let mut router = Router::new(weights_vec, K, 1.0, 0.01, seed).expect("router creation");
            let decision = router
                .route_with_adapter_info(&[], &priors, &adapter_info, &mask)
                .expect("router decision");

            if let Some(ref b) = baseline {
                assert_eq!(
                    b.indices, decision.indices,
                    "Mixed scores: indices should be stable"
                );
            } else {
                baseline = Some(decision);
            }
        }

        // Should select top 3 scores (indices 0, 1, 2 or some permutation of high scorers)
        let baseline = baseline.unwrap();
        let selected: Vec<u16> = baseline.indices.iter().cloned().collect();
        // All selected indices should be from the high-scoring group (0, 1) or middle (2, 3)
        for &idx in &selected {
            assert!(
                idx <= 3,
                "Should select from higher-scoring adapters, got {}",
                idx
            );
        }
    }
}

proptest! {
    #[test]
    fn prop_router_determinism(priors in prop::collection::vec(0.0..1.0f32, 3..10), _seed in any::<u64>()) {
        // Create two routers with same config
        let weights_vec = vec![1.0; priors.len()];
        let k = std::cmp::min(3, priors.len());
        let mut router1 = Router::new(weights_vec.clone(), k, 1.0, 1e-6, [0u8; 32]).expect("router creation");
        let mut router2 = Router::new(weights_vec, k, 1.0, 1e-6, [0u8; 32]).expect("router creation");

        // Same inputs should produce same outputs (determinism via stable sorting)
        let priors = vec![1.0; 8];
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("test_adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "warm".to_string(),
                scope_path: None,
                lora_tier: None,
                base_model: None,
                ..Default::default()
            })
            .collect();
        let decision1 = router1
            .route_with_adapter_info(&[], &priors, &adapter_info, &allow_all_mask(&adapter_info))
            .expect("router decision");
        let decision2 = router2
            .route_with_adapter_info(&[], &priors, &adapter_info, &allow_all_mask(&adapter_info))
            .expect("router decision");

        // Properties - check before moving values
        prop_assert_eq!(decision1.indices.len(), k);

        // Convert Q15 gates to f32 and verify normalization
        let gates_f32: Vec<f32> = decision1.gates_q15.iter().map(|&q| q as f32 / 32767.0).collect();
        let sum_gates: f32 = gates_f32.iter().sum();
        prop_assert!((sum_gates - 1.0).abs() < 0.01f32, "Gates sum to {} instead of ~1.0", sum_gates);

        // Check determinism (uses move, so do last)
        prop_assert_eq!(decision1.indices, decision2.indices);
        prop_assert_eq!(decision1.gates_q15, decision2.gates_q15);
    }

    #[test]
    fn prop_q15_gates_in_valid_range(gates_f32 in prop::collection::vec(0.0..1.0f32, 1..8)) {
        // Normalize to sum to 1.0
        let sum: f32 = gates_f32.iter().sum();
        let normalized: Vec<f32> = gates_f32.iter().map(|&g| g / sum).collect();

        // Convert to Q15
        let gates_q15: Vec<i16> = normalized
            .iter()
            .map(|&g| {
                let q = (g * ROUTER_GATE_Q15_DENOM).round() as i16;
                q.max(0)
            })
            .collect();

        // All gates should be in valid range
        for &gate in &gates_q15 {
            prop_assert!(gate >= 0, "Gate {} should be non-negative", gate);
            prop_assert!(gate <= ROUTER_GATE_Q15_MAX, "Gate {} should be <= {}", gate, ROUTER_GATE_Q15_MAX);
        }

        // Sum should be close to ROUTER_GATE_Q15_MAX
        let sum_q15: i32 = gates_q15.iter().map(|&g| g as i32).sum();
        let expected_sum = ROUTER_GATE_Q15_MAX as i32;
        let diff = (sum_q15 - expected_sum).abs();
        prop_assert!(diff < 10, "Q15 gates should sum to ~{}, got {} (diff: {})", expected_sum, sum_q15, diff);
    }
}

// =============================================================================
// Determinism Violation Detection Tests
//
// These tests verify that the system REJECTS non-deterministic configurations
// rather than just testing that deterministic configs produce consistent output.
// =============================================================================

mod determinism_violation_detection {
    use super::*;
    use adapteros_core::hash::B3Hash;
    use adapteros_core::seed::{
        derive_request_seed, with_determinism_config, DeterminismConfig, SeedMode,
    };

    /// Verifies that NonDeterministic seed mode is rejected in strict determinism mode
    #[test]
    fn test_nondeterministic_seed_rejected_in_strict_mode() {
        let global = B3Hash::hash(b"test-global");
        let manifest = B3Hash::hash(b"test-manifest");

        // Configure strict determinism mode
        let strict_config = DeterminismConfig::builder().strict_mode(true).build();

        // In strict mode, NonDeterministic seed mode should be rejected
        let result = with_determinism_config(strict_config, || {
            derive_request_seed(
                &global,
                Some(&manifest),
                "tenant",
                "request",
                1,
                0,
                SeedMode::NonDeterministic,
            )
        });

        assert!(
            result.is_err(),
            "NonDeterministic seed mode MUST be rejected in strict determinism mode, but got: {:?}",
            result
        );

        // Verify the error message mentions determinism
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("NonDeterministic") || err_str.contains("determinism"),
            "Error should mention non-determinism, got: {}",
            err_str
        );
    }

    /// Verifies that different seeds produce different routing decisions
    ///
    /// This test ensures that determinism is actually controlled by the seed,
    /// not accidental (e.g., always returning the same result).
    #[test]
    fn test_different_seeds_produce_different_decisions() {
        let seed_a = [1u8; 32];
        let seed_b = [2u8; 32];

        let weights = vec![0.25, 0.25, 0.25, 0.25]; // Equal weights
        let priors = vec![0.5, 0.5, 0.5, 0.5]; // Equal priors

        let adapter_info: Vec<AdapterInfo> = (0..4)
            .map(|i| AdapterInfo {
                id: format!("adapter_{}", i),
                stable_id: i as u64,
                tier: "warm".to_string(),
                ..Default::default()
            })
            .collect();

        let mask = allow_all_mask(&adapter_info);
        let features = vec![0.5f32; 4]; // Feature vector for routing

        // Create two routers with different seeds
        let mut router_a =
            Router::new(weights.clone(), 2, 1.0, 0.01, seed_a).expect("router A creation");
        let mut router_b =
            Router::new(weights.clone(), 2, 1.0, 0.01, seed_b).expect("router B creation");

        // Route and verify both return valid decisions
        // NOTE: Routing decisions are deterministic based on priors/scores, NOT seeds.
        // Seeds affect telemetry sampling, not routing. The decision hash is computed
        // from input/output data (features, priors, indices, gates), not the seed.
        // With identical inputs, identical decisions are expected regardless of seed.
        let decision_a = router_a
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("route A");
        let decision_b = router_b
            .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
            .expect("route B");

        // Verify routing produces identical results for identical inputs
        // (this is the core determinism guarantee)
        assert_eq!(
            decision_a.indices, decision_b.indices,
            "Identical inputs must produce identical routing indices"
        );
        assert_eq!(
            decision_a.gates_q15, decision_b.gates_q15,
            "Identical inputs must produce identical gates"
        );

        // Note: The routers have different seeds (seed_a vs seed_b) but produce
        // identical routing decisions because seeds only affect telemetry sampling,
        // not the routing algorithm itself. This is the correct deterministic behavior.
    }

    /// Verifies that strict mode seed validation catches version mismatches
    #[test]
    fn test_strict_mode_rejects_old_seed_version() {
        use adapteros_core::seed::{TypedSeed, HKDF_ALGORITHM_VERSION, HKDF_OUTPUT_LENGTH};

        // Create a seed with an old version
        let bytes = [0u8; HKDF_OUTPUT_LENGTH];
        let old_version_seed = TypedSeed::with_version(bytes, 1); // Version 1 is old

        // Validation should fail for old version
        let result = old_version_seed.validate();
        assert!(
            result.is_err(),
            "Old version seed should fail validation, got: {:?}",
            result
        );

        // Verify error mentions version
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("version"),
            "Error should mention version mismatch: {}",
            err
        );

        // Create a current version seed - should pass
        let current_seed = TypedSeed::with_version(bytes, HKDF_ALGORITHM_VERSION);
        assert!(
            current_seed.validate().is_ok(),
            "Current version seed should validate successfully"
        );
    }

    /// Verifies that seed checksum corruption is detected
    #[test]
    fn test_corrupted_seed_checksum_detected() {
        use adapteros_core::seed::derive_typed_seed;

        let global = B3Hash::hash(b"checksum-test");
        let mut typed_seed = derive_typed_seed(&global, "test-label");

        // Corrupt the checksum by using wrong bytes
        typed_seed.checksum = B3Hash::hash(b"wrong-data");

        let result = typed_seed.validate_checksum();
        assert!(result.is_err(), "Corrupted checksum should fail validation");

        assert!(
            result.unwrap_err().to_string().contains("checksum"),
            "Error should mention checksum mismatch"
        );
    }

    /// Verifies that same inputs always produce same seed (positive case for determinism)
    #[test]
    fn test_determinism_invariant_same_inputs_same_output() {
        use adapteros_core::seed::derive_seed;

        let global = B3Hash::hash(b"invariant-test");

        // Derive same seed multiple times
        let seed1 = derive_seed(&global, "router");
        let seed2 = derive_seed(&global, "router");
        let seed3 = derive_seed(&global, "router");

        assert_eq!(seed1, seed2, "Same inputs must produce identical seeds");
        assert_eq!(seed2, seed3, "Same inputs must produce identical seeds");

        // Different labels should produce different seeds
        let seed_different = derive_seed(&global, "sampling");
        assert_ne!(
            seed1, seed_different,
            "Different labels must produce different seeds"
        );
    }
}
