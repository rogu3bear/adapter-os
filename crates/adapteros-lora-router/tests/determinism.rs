//! Determinism and Q15 quantization tests for router
//!
//! Tests verify:
//! - Deterministic top-K selection (deterministic via score sorting, not seed)
//! - Stable ordering on ties (by index for reproducibility)
//! - Q15 gate quantization (non-negative, proper scaling)
//! - K=0 path returns empty indices/gates
//!
//! Note: Router seed is used for telemetry sampling determinism, not routing decisions.
//! Routing determinism comes from stable sorting (score desc, then index asc).

use adapteros_lora_router::{
    policy_mask::PolicyMask, AdapterInfo, Decision, Router, ROUTER_GATE_Q15_DENOM,
    ROUTER_GATE_Q15_MAX,
};
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
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision1 = router.route_with_adapter_info(&[], &priors, &adapter_info, &mask);
    let decision2 = router.route_with_adapter_info(&[], &priors, &adapter_info, &mask);

    // Results should be identical (deterministic via sorting)
    assert_eq!(decision1.indices, decision2.indices);
    assert_eq!(decision1.gates_q15, decision2.gates_q15);

    // On ties, should sort by index (lower index wins for stable ordering)
    assert_eq!(decision1.indices[0], 0);
    assert_eq!(decision1.indices[1], 1);
    assert_eq!(decision1.indices[2], 2);

    // New router instance should also produce same results (determinism)
    let mut router2 = Router::new(weights_vec, 3, 1.0, 0.01, seed).expect("router creation");
    let decision3 = router2.route_with_adapter_info(&[], &priors, &adapter_info, &mask);
    assert_eq!(decision1.indices, decision3.indices);
    assert_eq!(decision1.gates_q15, decision3.gates_q15);
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
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router.route_with_adapter_info(&[], &priors, &adapter_info, &mask);

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
        policy_mask_digest: None,
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
    let decision = router.route_with_adapter_info(&[], &[], &adapter_info, &mask);

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
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router.route_with_adapter_info(&[], &priors, &adapter_info, &mask);

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
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision1_1 = router1.route_with_adapter_info(&[], &priors, &adapter_info, &mask);
    let decision1_2 = router1.route_with_adapter_info(&[], &priors, &adapter_info, &mask);

    // Second router instance with same seed (seed doesn't affect routing, just telemetry)
    let weights_vec2 = vec![1.0; 4];
    let mut router2 = Router::new(weights_vec2, 2, 1.0, 0.01, seed).expect("router creation");
    let decision2_1 = router2.route_with_adapter_info(&[], &priors, &adapter_info, &mask);

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
        })
        .collect();

    let mask = allow_all_mask(&adapter_info);
    let mut baseline_indices = None;
    let mut baseline_gates = None;

    for _ in 0..10 {
        let decision = router.route_with_adapter_info(&[], &priors, &adapter_info, &mask);
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
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router.route_with_adapter_info(&[], &priors, &adapter_info, &mask);

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
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);
    let decision = router.route_with_adapter_info(&[], &priors, &adapter_info, &mask);

    // 1:1 mapping
    assert_eq!(decision.indices.len(), 4);
    assert_eq!(decision.indices.len(), decision.gates_q15.len());

    // Unique indices
    let mut indices_sorted = decision.indices.clone();
    indices_sorted.sort_unstable();
    for i in 1..indices_sorted.len() {
        assert_ne!(indices_sorted[i], indices_sorted[i - 1]);
    }

    // For K=0 case
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
        })
        .collect();
    let mask_k0 = allow_all_mask(&adapter_info);
    let decision_k0 = router_k0.route_with_adapter_info(&[], &priors, &adapter_info, &mask_k0);
    assert!(decision_k0.indices.is_empty());
    assert!(decision_k0.gates_q15.is_empty());
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
        })
        .collect();

    for k in 0..=8 {
        let weights_vec = vec![1.0; 8];
        let mut router = Router::new(weights_vec, k, 1.0, 0.01, seed).expect("router creation");
        let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
        let mask = PolicyMask::allow_all(&adapter_ids, None);
        let decision = router.route_with_adapter_info(&[], &priors, &adapter_info, &mask);

        assert_eq!(decision.indices.len(), k);
        assert_eq!(decision.gates_q15.len(), k);
    }
}

// RouterDecisionEvent serialization tests belong in adapteros-telemetry crate

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
            })
            .collect();
        let decision1 =
            router1.route_with_adapter_info(&[], &priors, &adapter_info, &allow_all_mask(&adapter_info));
        let decision2 =
            router2.route_with_adapter_info(&[], &priors, &adapter_info, &allow_all_mask(&adapter_info));

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
}
