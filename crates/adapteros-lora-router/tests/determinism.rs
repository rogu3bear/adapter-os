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
