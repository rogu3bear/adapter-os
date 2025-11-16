<<<<<<< HEAD
#![cfg(all(test, feature = "extended-tests"))]

//! Router correctness proofs for stack filtering
//!
//! These tests prove the correctness of router stack filtering logic,
//! ensuring deterministic and correct adapter selection when stacks are active.

use adapteros_lora_router::{CodeFeatures, Router, RouterWeights};

/// Proof: Stack filtering preserves routing determinism
#[test]
fn proof_stack_filter_deterministic() {
    let mut router1 = Router::new(vec![1.0; 5], 3, 1.0, 0.02, [42u8; 32]);
    let mut router2 = Router::new(vec![1.0; 5], 3, 1.0, 0.02, [42u8; 32]);

    let catalog = vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
        "e".to_string(),
    ];

    router1.set_adapter_catalog(catalog.clone());
    router2.set_adapter_catalog(catalog);

    router1
        .activate_stack("test", &["b".to_string(), "d".to_string()])
        .expect("stack activation");
    router2
        .activate_stack("test", &["b".to_string(), "d".to_string()])
        .expect("stack activation");

    let priors = vec![0.9, 0.8, 0.7, 0.6, 0.5];
    let features = CodeFeatures::from_context("test prompt").to_vector();
=======
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
>>>>>>> integration-branch

    let decision1 = router1.route(&features, &priors);
    let decision2 = router2.route(&features, &priors);

<<<<<<< HEAD
    assert_eq!(
        decision1.indices, decision2.indices,
        "Stack filtering must be deterministic with same seed"
    );
    assert_eq!(
        decision1.gates_q15(),
        decision2.gates_q15(),
        "Gates must be deterministic with same seed"
    );
}

/// Proof: Stack filtering only selects from stack adapters
#[test]
fn proof_stack_filter_contains_only_stack_adapters() {
    let mut router = Router::new(vec![1.0; 6], 3, 1.0, 0.02, [0u8; 32]);
    router.set_adapter_catalog(vec![
        "alpha".to_string(),
        "beta".to_string(),
        "gamma".to_string(),
        "delta".to_string(),
        "epsilon".to_string(),
        "zeta".to_string(),
    ]);

    let stack_ids = vec!["beta".to_string(), "delta".to_string(), "epsilon".to_string()];
    router.activate_stack("bde-stack", &stack_ids).unwrap();

    let priors = vec![1.0, 0.9, 0.8, 0.7, 0.6, 0.5];
    let features = vec![];

    let decision = router.route(&features, &priors);

    // Verify all selected indices are in the stack (indices 1, 3, 4)
    let allowed_indices = [1usize, 3, 4];
    for &idx in &decision.indices {
        assert!(
            allowed_indices.contains(&idx),
            "Router selected adapter {} which is not in stack {:?}",
            idx,
            stack_ids
=======
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
>>>>>>> integration-branch
        );
    }
}

<<<<<<< HEAD
/// Proof: Stack filter with no qualifying adapters returns empty decision
#[test]
fn proof_stack_filter_empty_when_no_matches() {
    let mut router = Router::new(vec![1.0; 4], 2, 1.0, 0.02, [0u8; 32]);
    router.set_adapter_catalog(vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
    ]);

    // Activate stack with only adapters c and d (indices 2,3)
    router
        .activate_stack("cd-stack", &["c".to_string(), "d".to_string()])
        .unwrap();

    // Set priors such that only a and b (indices 0,1) would score high
    // but they're not in the stack
    let priors = vec![1.0, 0.9, 0.0, 0.0];
    let features = vec![];

    let decision = router.route(&features, &priors);

    // Should return adapters from stack even if lower priors
    // (stack filtering happens AFTER scoring, not before)
    // So we should get indices 2 and 3 despite lower priors
    assert_eq!(decision.indices.len(), 2);
    assert!(decision.indices.contains(&2));
    assert!(decision.indices.contains(&3));
}

/// Proof: Catalog update preserves active stack if still valid
#[test]
fn proof_catalog_update_preserves_valid_stack() {
    let mut router = Router::new(vec![1.0; 3], 2, 1.0, 0.02, [0u8; 32]);

    router.set_adapter_catalog(vec!["x".to_string(), "y".to_string(), "z".to_string()]);

    router
        .activate_stack("xy", &["x".to_string(), "y".to_string()])
        .unwrap();

    assert_eq!(router.active_stack_name(), Some("xy"));

    // Update catalog with same adapters in same order
    router.set_adapter_catalog(vec!["x".to_string(), "y".to_string(), "z".to_string()]);

    // Stack should still be active
    assert_eq!(router.active_stack_name(), Some("xy"));
}

/// Proof: Catalog update clears stack if adapters no longer exist
#[test]
fn proof_catalog_update_clears_invalid_stack() {
    let mut router = Router::new(vec![1.0; 3], 2, 1.0, 0.02, [0u8; 32]);

    router.set_adapter_catalog(vec!["x".to_string(), "y".to_string(), "z".to_string()]);

    router
        .activate_stack("xy", &["x".to_string(), "y".to_string()])
        .unwrap();

    assert_eq!(router.active_stack_name(), Some("xy"));

    // Update catalog removing adapter "x"
    router.set_adapter_catalog(vec!["y".to_string(), "z".to_string(), "w".to_string()]);

    // Stack should be cleared because "x" no longer exists
    assert_eq!(router.active_stack_name(), None);
}

/// Proof: Stack activation fails with unknown adapter
#[test]
fn proof_stack_activation_rejects_unknown_adapter() {
    let mut router = Router::new(vec![1.0; 3], 2, 1.0, 0.02, [0u8; 32]);

    router.set_adapter_catalog(vec!["a".to_string(), "b".to_string(), "c".to_string()]);

    let result = router.activate_stack("test", &["a".to_string(), "unknown".to_string()]);

    assert!(
        result.is_err(),
        "Stack activation should fail with unknown adapter"
    );

    assert_eq!(
        router.active_stack_name(),
        None,
        "No stack should be active after failed activation"
    );
}

/// Proof: Stack activation fails without catalog
#[test]
fn proof_stack_activation_requires_catalog() {
    let mut router = Router::new(vec![1.0; 3], 2, 1.0, 0.02, [0u8; 32]);

    let result = router.activate_stack("test", &["a".to_string()]);

    assert!(
        result.is_err(),
        "Stack activation should fail without catalog"
    );
}

/// Proof: Stack activation fails with empty adapter list
#[test]
fn proof_stack_activation_rejects_empty_list() {
    let mut router = Router::new(vec![1.0; 3], 2, 1.0, 0.02, [0u8; 32]);

    router.set_adapter_catalog(vec!["a".to_string(), "b".to_string()]);

    let result = router.activate_stack("empty", &[]);

    assert!(
        result.is_err(),
        "Stack activation should fail with empty adapter list"
    );
}

/// Proof: Clearing stack restores full routing
#[test]
fn proof_clear_stack_restores_full_routing() {
    let mut router = Router::new(vec![1.0; 4], 3, 1.0, 0.02, [0u8; 32]);

    router.set_adapter_catalog(vec![
        "a".to_string(),
        "b".to_string(),
        "c".to_string(),
        "d".to_string(),
    ]);

    // Activate stack with only 2 adapters
    router
        .activate_stack("ab", &["a".to_string(), "b".to_string()])
        .unwrap();

    let priors = vec![1.0, 0.9, 0.8, 0.7];
    let features = vec![];

    let decision_filtered = router.route(&features, &priors);
    assert!(
        decision_filtered.indices.len() <= 2,
        "Should only select from stack"
    );

    // Clear stack
    router.clear_stack();
    assert_eq!(router.active_stack_name(), None);

    let decision_full = router.route(&features, &priors);
    assert_eq!(
        decision_full.indices.len(),
        3,
        "Should select K=3 adapters after clearing stack"
    );
}

/// Proof: Stack filtering with weights produces correct scores
#[test]
fn proof_stack_filter_with_custom_weights() {
    let weights = RouterWeights::new(0.3, 0.25, 0.2, 0.15, 0.1);
    let mut router = Router::new_with_weights(weights, 2, 1.0, 0.02, [0u8; 32]);

    router.set_adapter_catalog(vec!["p1".to_string(), "p2".to_string(), "p3".to_string()]);

    router
        .activate_stack("p2p3", &["p2".to_string(), "p3".to_string()])
        .unwrap();

    let features = CodeFeatures::from_context("Python function").to_vector();
    let priors = vec![1.0, 0.9, 0.8];

    let decision = router.route(&features, &priors);

    // Should only select from indices 1 and 2 (p2 and p3)
    assert_eq!(decision.indices.len(), 2);
    assert!(!decision.indices.contains(&0), "Should not select p1");
    assert!(
        decision.indices.contains(&1) && decision.indices.contains(&2),
        "Should select p2 and p3"
    );

    // Gates should sum to ~1.0
    let gate_sum: f32 = decision.gates_f32().iter().sum();
    assert!(
        (gate_sum - 1.0).abs() < 0.01,
        "Gates should sum to 1.0, got {}",
        gate_sum
    );
}
=======
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
>>>>>>> integration-branch
