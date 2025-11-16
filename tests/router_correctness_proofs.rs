//! Router correctness proofs for entropy floor and stack filtering
//!
//! This test suite validates that router changes maintain critical invariants:
//! - Entropy floor prevents gate collapse
//! - Stack filtering maintains determinism
//! - K-sparse selection is preserved

use adapteros_lora_router::{Router, RouterWeights};

#[test]
fn test_entropy_floor_maintained_with_stack_filtering() {
    // Create router with entropy floor
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.1);

    // Set active stack (filtering enabled)
    router.set_active_stack(
        Some("test_stack".to_string()),
        Some(vec![
            "adapter_1".to_string(),
            "adapter_2".to_string(),
            "adapter_3".to_string(),
        ]),
    );

    // Create priors with one dominant prior
    let features = vec![0.0; 22];
    let priors = vec![10.0, 0.0, 0.0]; // One dominant

    let decision = router.route(&features, &priors);
    let gates = decision.gates_f32();

    // Verify entropy floor is maintained
    let min_gate = 0.1 / 3.0; // eps / k
    for &g in &gates {
        assert!(
            g >= min_gate - 0.001,
            "Gate {} violates entropy floor {}",
            g,
            min_gate
        );
    }

    // Gates should sum to approximately 1.0
    let sum: f32 = gates.iter().sum();
    assert!(
        (sum - 1.0).abs() < 0.01,
        "Gates sum to {}, expected 1.0",
        sum
    );
}

#[test]
fn test_stack_filtering_determinism() {
    // Create two routers with identical configuration
    let weights = RouterWeights::default();
    let mut router1 = Router::new_with_weights(weights.clone(), 3, 1.0, 0.02);
    let mut router2 = Router::new_with_weights(weights, 3, 1.0, 0.02);

    // Set identical stack on both
    let stack_adapters = vec!["a1".to_string(), "a2".to_string(), "a3".to_string()];
    router1.set_active_stack(Some("stack1".to_string()), Some(stack_adapters.clone()));
    router2.set_active_stack(Some("stack1".to_string()), Some(stack_adapters));

    // Route with identical inputs
    let features = vec![0.5; 22];
    let priors = vec![0.3, 0.5, 0.7];

    let decision1 = router1.route(&features, &priors);
    let decision2 = router2.route(&features, &priors);

    // Results must be deterministic
    assert_eq!(
        decision1.indices.as_slice(),
        decision2.indices.as_slice(),
        "Stack filtering produced non-deterministic indices"
    );

    // Gates must match
    let gates1 = decision1.gates_f32();
    let gates2 = decision2.gates_f32();
    for (g1, g2) in gates1.iter().zip(gates2.iter()) {
        assert!(
            (g1 - g2).abs() < 0.0001,
            "Gate values differ: {} vs {}",
            g1,
            g2
        );
    }
}

#[test]
fn test_k_sparse_selection_preserved() {
    let weights = RouterWeights::default();
    let k = 3;
    let mut router = Router::new_with_weights(weights, k, 1.0, 0.02);

    // Set stack with 5 adapters
    router.set_active_stack(
        Some("stack".to_string()),
        Some((0..5).map(|i| format!("adapter_{}", i)).collect()),
    );

    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.2, 0.3, 0.4, 0.5];

    let decision = router.route(&features, &priors);

    // Must select exactly K adapters
    assert_eq!(
        decision.indices.len(),
        k,
        "Router selected {} adapters, expected K={}",
        decision.indices.len(),
        k
    );
    assert_eq!(
        decision.gates_q15.len(),
        k,
        "Router produced {} gates, expected K={}",
        decision.gates_q15.len(),
        k
    );
}

#[test]
fn test_entropy_floor_across_temperature_values() {
    // Test that entropy floor is maintained across different temperatures
    let weights = RouterWeights::default();
    let temperatures = vec![0.5, 1.0, 2.0, 5.0];
    let eps = 0.1;
    let k = 3;

    for tau in temperatures {
        let mut router = Router::new_with_weights(weights.clone(), k, tau, eps);

        let features = vec![0.0; 22];
        let priors = vec![10.0, 0.1, 0.01]; // Heavily skewed

        let decision = router.route(&features, &priors);
        let gates = decision.gates_f32();

        let min_gate = eps / k as f32;
        for &g in &gates {
            assert!(
                g >= min_gate - 0.001,
                "Temperature {} violated entropy floor: gate={}, min={}",
                tau,
                g,
                min_gate
            );
        }
    }
}

#[test]
fn test_stack_deactivation_restores_full_routing() {
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    // Activate stack with 3 adapters
    router.set_active_stack(
        Some("limited_stack".to_string()),
        Some(vec!["a1".to_string(), "a2".to_string(), "a3".to_string()]),
    );

    assert!(router.active_stack().is_some());

    // Deactivate stack
    router.set_active_stack(None, None);

    assert!(router.active_stack().is_none());

    // Should now be able to route with larger prior set
    let features = vec![0.5; 22];
    let priors = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6];

    let decision = router.route(&features, &priors);

    // Should still select K=3 from the full set
    assert_eq!(decision.indices.len(), 3);
}

/// Proof: Entropy floor prevents collapse to single adapter
///
/// Mathematical property:
/// For all gates g_i in top-K selection:
///   g_i >= eps / K
///
/// This ensures that even with one dominant adapter, all K adapters
/// receive at least minimum weight, preventing complete collapse.
#[test]
fn proof_entropy_floor_prevents_collapse() {
    let eps = 0.1;
    let k = 3;
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, k, 1.0, eps);

    // Extreme case: one adapter has massive advantage
    let features = vec![0.0; 22];
    let priors = vec![1000.0, 0.0, 0.0, 0.0, 0.0];

    let decision = router.route(&features, &priors);
    let gates = decision.gates_f32();

    // Even in extreme case, minimum gate is maintained
    let min_observed = gates.iter().fold(f32::INFINITY, |a, &b| a.min(b));
    let min_required = eps / k as f32;

    assert!(
        min_observed >= min_required - 0.001,
        "Entropy floor failed: min_observed={}, min_required={}",
        min_observed,
        min_required
    );

    // Distribution entropy is bounded below
    let entropy: f32 = gates
        .iter()
        .filter(|&&g| g > 0.0)
        .map(|&g| -g * g.log2())
        .sum();

    let min_entropy = -(eps / k as f32) * (eps / k as f32).log2() * k as f32;

    assert!(
        entropy >= min_entropy - 0.01,
        "Entropy {} is below theoretical minimum {}",
        entropy,
        min_entropy
    );
}

/// Proof: Stack filtering is a restriction of the routing space
///
/// Mathematical property:
/// Let S be the set of all adapters
/// Let T ⊆ S be the active stack
/// For any routing decision D:
///   ∀ adapter a ∈ D: a ∈ T
///
/// This proves stack filtering correctly restricts the search space
/// without violating routing invariants.
#[test]
fn proof_stack_filtering_is_restriction() {
    let weights = RouterWeights::default();
    let mut router = Router::new_with_weights(weights, 3, 1.0, 0.02);

    // Define stack as strict subset
    let all_adapters: Vec<String> = (0..10).map(|i| format!("adapter_{}", i)).collect();
    let stack_adapters: Vec<String> = vec![
        "adapter_2".to_string(),
        "adapter_5".to_string(),
        "adapter_7".to_string(),
    ];

    router.set_active_stack(
        Some("subset_stack".to_string()),
        Some(stack_adapters.clone()),
    );

    // Route with full prior set
    let features = vec![0.5; 22];
    let priors = vec![0.1; 10];

    let decision = router.route(&features, &priors);

    // All selected indices must be within stack
    let stack_indices = vec![2, 5, 7]; // Corresponding to adapter_2, adapter_5, adapter_7

    for &idx in decision.indices.iter() {
        assert!(
            stack_indices.contains(&(idx as usize)),
            "Selected index {} is not in stack subset {:?}",
            idx,
            stack_indices
        );
    }
}
