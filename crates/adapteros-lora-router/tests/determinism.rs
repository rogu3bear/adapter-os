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

use adapteros_lora_router::Router;
use proptest::prelude::*;
use rand_chacha::ChaChaRng;
use std::collections::HashSet;

#[test]
fn test_deterministic_top_k_ordering() {
    // Routing determinism comes from stable sorting, not seed
    // Seed is only used for telemetry sampling determinism
    let seed = [42u8; 32];

    // Use Router::new which accepts seed parameter
    let weights_vec = vec![1.0; 5]; // Dummy weights for adapter count
    let mut router = Router::new(weights_vec.clone(), 3, 1.0, 0.01, seed);

    // Create priors with ties
    let priors = vec![0.5, 0.5, 0.5, 0.3, 0.2]; // First three tied

    // Multiple calls with same inputs should produce identical results
    let decision1 = router.route(&[], &priors);
    let decision2 = router.route(&[], &priors);

    // Results should be identical (deterministic via sorting)
    assert_eq!(decision1.indices, decision2.indices);
    assert_eq!(decision1.gates_q15, decision2.gates_q15);

    // On ties, should sort by index (lower index wins for stable ordering)
    assert_eq!(decision1.indices[0], 0);
    assert_eq!(decision1.indices[1], 1);
    assert_eq!(decision1.indices[2], 2);

    // New router instance should also produce same results (determinism)
    let mut router2 = Router::new(weights_vec, 3, 1.0, 0.01, seed);
    let decision3 = router2.route(&[], &priors);
    assert_eq!(decision1.indices, decision3.indices);
    assert_eq!(decision1.gates_q15, decision3.gates_q15);
}

#[test]
fn test_q15_quantization_properties() {
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 3, 1.0, 0.01, seed);

    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let decision = router.route(&[], &priors);

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
fn test_k0_detection_empty_result() {
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 3, 1.0, 0.01, seed);

    // Empty priors should result in empty decision
    let decision = router.route_with_k0_detection(&[], &[]);

    assert!(decision.indices.is_empty());
    assert!(decision.gates_q15.is_empty());
}

#[test]
fn test_gate_normalization_and_entropy_floor() {
    let seed = [42u8; 32];
    let eps = 0.01;
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 3, 1.0, eps, seed);

    let priors = vec![0.9, 0.8, 0.1, 0.05, 0.02];
    let decision = router.route(&[], &priors);

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
    let mut router1 = Router::new(weights_vec1, 2, 1.0, 0.01, seed);
    let decision1_1 = router1.route(&[], &[0.7, 0.6, 0.5, 0.4]);
    let decision1_2 = router1.route(&[], &[0.7, 0.6, 0.5, 0.4]);

    // Second router instance with same seed (seed doesn't affect routing, just telemetry)
    let weights_vec2 = vec![1.0; 4];
    let mut router2 = Router::new(weights_vec2, 2, 1.0, 0.01, seed);
    let decision2_1 = router2.route(&[], &[0.7, 0.6, 0.5, 0.4]);

    // All three should produce identical results (deterministic sorting)
    assert_eq!(decision1_1.indices, decision1_2.indices);
    assert_eq!(decision1_1.indices, decision2_1.indices);
    assert_eq!(decision1_1.gates_q15, decision1_2.gates_q15);
    assert_eq!(decision1_1.gates_q15, decision2_1.gates_q15);
}

#[test]
fn test_q15_range_properties() {
    let seed = [42u8; 32];
    let weights_vec = vec![1.0; 5];
    let mut router = Router::new(weights_vec, 5, 1.0, 0.001, seed);

    let priors = vec![1.0, 0.9, 0.8, 0.7, 0.6];
    let decision = router.route(&[], &priors);

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
    let mut router = Router::new(weights_vec, 4, 1.0, 0.01, seed);

    let priors = vec![0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2];
    let decision = router.route(&[], &priors);

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
    let mut router_k0 = Router::new(vec![1.0; 8], 0, 1.0, 0.01, seed);
    let decision_k0 = router_k0.route(&[], &priors);
    assert!(decision_k0.indices.is_empty());
    assert!(decision_k0.gates_q15.is_empty());
}

// Add more tests for varying K up to MAX_K
#[test]
fn test_varying_k_stability() {
    let seed = [42u8; 32];
    let priors = vec![1.0; 8];

    for k in 0..=super::MAX_K {
        let weights_vec = vec![1.0; 8];
        let mut router = Router::new(weights_vec, k, 1.0, 0.01, seed);
        let decision = router.route(&[], &priors);

        assert_eq!(decision.indices.len(), k);
        assert_eq!(decision.gates_q15.len(), k);
    }
}

#[test]
fn test_router_event_wire_format() {
    use adapteros_telemetry::RouterCandidate;
    use adapteros_telemetry::RouterDecisionEvent;
    use bincode;

    let event = RouterDecisionEvent {
        step: 5,
        input_token_id: Some(123),
        candidate_adapters: vec![
            RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.8,
                gate_q15: 32768,
            },
            RouterCandidate {
                adapter_idx: 2,
                raw_score: 0.6,
                gate_q15: 24576,
            },
        ],
        entropy: 0.9,
        tau: 1.0,
        entropy_floor: 1e-6,
        stack_hash: Some("b3:deadbeef".to_string()),
    };

    // Bincode roundtrip
    let encoded = bincode::serialize(&event).unwrap();
    let decoded: RouterDecisionEvent = bincode::deserialize(&encoded).unwrap();
    assert_eq!(event, decoded);

    // JSON roundtrip
    let json = serde_json::to_string(&event).unwrap();
    let decoded_json: RouterDecisionEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, decoded_json);
}

proptest! {
    #[test]
    fn prop_router_determinism priors in prop::collection::vec(0.0..1.0f32, 1..10), seed in any::<u64>() {
        let mut rng = ChaChaRng::seed_from_u64(seed);
        let router = Router::new(priors, 3, 1.0, 1e-6, [0u8;32]); // Fixed seed for prop
        let features = vec![1.0; priors.len()]; // Fixed features
        let decision1 = router.decide(&features, &mut rng).unwrap();
        let decision2 = router.decide(&features, &mut rng).unwrap(); // Same rng state? No, but since seeded same, but rng is mutated.
        // For determinism, seed per test
        let seed_bytes: [u8; 8] = seed.to_le_bytes(); // u64 to [u8;8]
        let mut rng1 = ChaChaRng::from_seed([seed_bytes; 4]); // Repeat to 32 bytes
        let mut rng2 = ChaChaRng::from_seed([seed_bytes; 4]);
        let decision1 = router.decide(&features, &mut rng1).unwrap();
        let decision2 = router.decide(&features, &mut rng2).unwrap();
        prop_assert_eq!(decision1, decision2);

        // Properties
        prop_assert_eq!(decision1.indices.len(), 3);
        let indices_set: HashSet<_> = decision1.indices.iter().cloned().collect();
        prop_assert_eq!(indices_set.len(), 3usize); // Unique
        let sum_gates: f32 = decision1.gates.iter().sum();
        prop_assert!((sum_gates - 1.0).abs() < 1e-5f32); // Sum ~1.0
        let entropy = -decision1.gates.iter().map(|&g| if g > 0.0 { g * g.log2() } else { 0.0 }).sum::<f32>();
        prop_assert!(entropy > 1e-6f32); // > floor
    }
}
