//! Router abstain edge case tests
//!
//! This test suite validates the router's abstain mechanism under edge conditions:
//! - All adapters denied by policy (should produce empty decision)
//! - Empty adapter list (should produce empty decision)
//! - Partial policy denial (some adapters allowed)
//! - Mismatched input lengths (should fail gracefully)
//!
//! The abstain mechanism emits telemetry events when:
//! 1. Entropy exceeds the configured entropy threshold (high uncertainty)
//! 2. Max gate value falls below the confidence threshold (low confidence)
//!
//! Note: Threshold boundary tests (entropy/confidence exactly at threshold) are
//! covered by the policy integration tests and telemetry tests.
#![allow(clippy::useless_vec)]

use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};

fn allow_all_mask(adapters: &[AdapterInfo]) -> PolicyMask {
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    PolicyMask::allow_all(&ids, None)
}

fn deny_all_mask(adapters: &[AdapterInfo]) -> PolicyMask {
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    PolicyMask::deny_all(&ids, None)
}

/// Test that when ALL adapters are denied by policy, router returns empty decision
/// This is critical for fail-safe behavior: if policy blocks everything, router abstains.
#[test]
fn test_all_adapters_denied_by_policy() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    // Deny ALL adapters via policy mask
    let deny_mask = deny_all_mask(&adapter_info);

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &deny_mask)
        .expect("router should handle deny-all gracefully");

    // Should produce empty decision (k=0)
    assert_eq!(
        decision.indices.len(),
        0,
        "When all adapters denied by policy, should return empty decision"
    );
    assert_eq!(
        decision.gates_q15.len(),
        0,
        "When all adapters denied by policy, should have no gates"
    );
    assert_eq!(
        decision.entropy, 0.0,
        "Empty decision should have zero entropy"
    );
    assert_eq!(
        decision.candidates.len(),
        0,
        "Empty decision should have no candidates"
    );

    // Verify policy mask digest is preserved
    assert_eq!(
        decision.policy_mask_digest_b3,
        Some(deny_mask.digest),
        "Policy mask digest should be preserved in decision"
    );
}

/// Test that empty adapter list produces empty decision
/// This validates graceful degradation when no adapters are available.
#[test]
fn test_empty_adapter_list() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    let features = vec![0.5; 22];
    let priors: Vec<f32> = vec![];
    let adapter_info: Vec<AdapterInfo> = vec![];

    let empty_mask = allow_all_mask(&adapter_info);

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &empty_mask)
        .expect("router should handle empty adapter list");

    assert_eq!(
        decision.indices.len(),
        0,
        "Empty adapter list should produce empty decision"
    );
    assert_eq!(
        decision.gates_q15.len(),
        0,
        "Empty adapter list should have no gates"
    );
    assert_eq!(
        decision.entropy, 0.0,
        "Empty decision should have zero entropy"
    );
}

/// Test entropy computation on various distributions
/// This validates that entropy is computed correctly and deterministically.
#[test]
fn test_entropy_computation() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    let features = vec![0.0; 22]; // Zero features so priors dominate
    let adapter_info: Vec<AdapterInfo> = (0..3)
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    // Test 1: Uniform distribution should have high entropy
    let uniform_priors = vec![0.33, 0.33, 0.34];
    let decision1 = router
        .route_with_adapter_info(&features, &uniform_priors, &adapter_info, &mask)
        .expect("routing decision");

    // Test 2: Skewed distribution should have lower entropy
    let skewed_priors = vec![2.0, 0.1, 0.1];
    let decision2 = router
        .route_with_adapter_info(&features, &skewed_priors, &adapter_info, &mask)
        .expect("routing decision");

    // Verify entropy relationship
    assert!(
        decision1.entropy > decision2.entropy,
        "Uniform distribution should have higher entropy: {} > {}",
        decision1.entropy,
        decision2.entropy
    );

    // Verify decisions are still made correctly
    assert_eq!(decision1.indices.len(), 3, "Should route 3 adapters");
    assert_eq!(decision2.indices.len(), 3, "Should route 3 adapters");
}

/// Test that empty adapter list produces zero entropy
#[test]
fn test_empty_adapter_list_zero_entropy() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    let features = vec![0.5; 22];
    let priors: Vec<f32> = vec![];
    let adapter_info: Vec<AdapterInfo> = vec![];
    let mask = allow_all_mask(&adapter_info);

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("should handle empty adapters");

    // Should not panic, should produce empty decision with zero entropy
    assert_eq!(decision.indices.len(), 0);
    assert_eq!(decision.gates_q15.len(), 0);
    assert_eq!(
        decision.entropy, 0.0,
        "Empty decision should have zero entropy"
    );
}

/// Test determinism of routing decisions
/// The same input should always produce the same decision (including entropy).
#[test]
fn test_routing_determinism() {
    let mut router1 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);
    let mut router2 = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    // Set same thresholds (though they don't affect routing, only telemetry)
    router1.set_abstain_thresholds(Some(0.6), Some(0.4));
    router2.set_abstain_thresholds(Some(0.6), Some(0.4));

    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    let decision1 = router1
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing decision 1");

    let decision2 = router2
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing decision 2");

    // Entropy should be identical
    assert_eq!(
        decision1.entropy, decision2.entropy,
        "Entropy should be deterministic"
    );

    // Gates should be identical
    assert_eq!(
        decision1.gates_q15, decision2.gates_q15,
        "Gates should be deterministic"
    );

    // Selected indices should be identical
    assert_eq!(
        decision1.indices, decision2.indices,
        "Selected indices should be deterministic"
    );
}

/// Test that partial policy denial (some but not all denied) works correctly
/// This validates the filtering logic in route_with_adapter_info.
#[test]
fn test_partial_adapter_denial() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
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

    // Deny first 3 adapters, allow last 2
    let partial_mask = PolicyMask::build(
        &adapter_ids,
        Some(&vec!["adapter_3".to_string(), "adapter_4".to_string()]),
        None,
        None,
        None,
        None,
    );

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &partial_mask)
        .expect("routing decision");

    // Should only route from allowed adapters (indices 3 and 4)
    assert_eq!(
        decision.indices.len(),
        2,
        "Should only route allowed adapters"
    );
    for &idx in decision.indices.iter() {
        assert!(
            idx == 3 || idx == 4,
            "Should only select from allowed adapters, got {}",
            idx
        );
    }
}

/// Test that mismatched policy mask length is handled gracefully
/// This validates the length check in route_with_adapter_info_and_scope_with_ctx.
#[test]
fn test_mismatched_policy_mask_length() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    // Create mask with wrong length (3 instead of 5)
    let wrong_ids: Vec<String> = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];
    let wrong_mask = PolicyMask::allow_all(&wrong_ids, None);

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &wrong_mask)
        .expect("should handle mismatched mask");

    // Should return empty decision (fail-safe)
    assert_eq!(
        decision.indices.len(),
        0,
        "Mismatched policy mask should produce empty decision"
    );
    assert_eq!(
        decision.gates_q15.len(),
        0,
        "Mismatched policy mask should have no gates"
    );
}

/// Test that mismatched priors length is handled gracefully
/// This validates the length check in route_with_adapter_info_and_scope_with_ctx.
#[test]
fn test_mismatched_priors_length() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4]; // Only 3 priors
    let adapter_info: Vec<AdapterInfo> = (0..5)
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("should handle mismatched priors");

    // Should return empty decision (fail-safe)
    assert_eq!(
        decision.indices.len(),
        0,
        "Mismatched priors length should produce empty decision"
    );
    assert_eq!(
        decision.gates_q15.len(),
        0,
        "Mismatched priors length should have no gates"
    );
}

/// Test that abstain thresholds can be updated dynamically
#[test]
fn test_dynamic_abstain_threshold_updates() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    // Initially no thresholds
    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    let decision1 = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing decision 1");

    // Set thresholds
    router.set_abstain_thresholds(Some(0.5), Some(0.3));

    let decision2 = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing decision 2");

    // Clear thresholds
    router.set_abstain_thresholds(None, None);

    let decision3 = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing decision 3");

    // All decisions should still be made correctly
    assert_eq!(decision1.indices.len(), 3);
    assert_eq!(decision2.indices.len(), 3);
    assert_eq!(decision3.indices.len(), 3);

    // Decisions should be deterministic (same inputs = same outputs)
    assert_eq!(decision1.indices, decision2.indices);
    assert_eq!(decision2.indices, decision3.indices);
}

/// Test that empty decision (all adapters denied) doesn't trigger abstain events
/// This validates the fix for the edge case where empty gates shouldn't emit
/// "low confidence" abstain events.
#[test]
fn test_empty_decision_no_abstain_events() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.01);

    // Set very low thresholds that would trigger on any normal decision
    router.set_abstain_thresholds(Some(0.0), Some(1.0));

    // Note: We can't test telemetry emission without the actual TelemetryWriter
    // but we can verify that the router doesn't panic and produces correct empty decision

    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            ..Default::default()
        })
        .collect();

    // Deny ALL adapters
    let deny_mask = deny_all_mask(&adapter_info);

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &deny_mask)
        .expect("should handle deny-all gracefully");

    // Empty decision should be produced without panic
    assert_eq!(decision.indices.len(), 0);
    assert_eq!(decision.gates_q15.len(), 0);
    assert_eq!(decision.entropy, 0.0);

    // The key fix: check_abstain_conditions should skip empty decisions
    // and not emit spurious "low confidence" events for policy-based abstention
}
