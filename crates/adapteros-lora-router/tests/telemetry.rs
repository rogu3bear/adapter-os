//! Router telemetry emission tests
//!
//! Tests for 2-PRD[01]: Telemetry RouterDecision v1

use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use adapteros_telemetry::writer::RouterDecisionWriter;

fn allow_all_mask(adapters: &[AdapterInfo]) -> PolicyMask {
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    PolicyMask::allow_all(&ids, None)
}

#[test]
fn test_router_emits_telemetry_on_decision() {
    // Create router with telemetry writer
    let (writer, mut receiver) = RouterDecisionWriter::new();
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_telemetry_writer(writer);

    // Make a routing decision
    let features = vec![0.5; 22];
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
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .expect("router decision");

    // Verify telemetry event was emitted
    let event = receiver
        .try_recv()
        .expect("Should have emitted a telemetry event");

    // Verify event contents
    assert_eq!(event.step, 0, "First decision should be step 0");
    assert_eq!(event.tau, 1.0, "Temperature should match router config");
    assert_eq!(event.entropy_floor, 0.02, "Entropy floor should match");
    assert_eq!(
        event.candidate_adapters.len(),
        decision.candidates.len(),
        "Should have same number of candidates"
    );

    // Verify candidate data matches
    for (event_candidate, decision_candidate) in event
        .candidate_adapters
        .iter()
        .zip(decision.candidates.iter())
    {
        assert_eq!(
            event_candidate.adapter_idx, decision_candidate.adapter_idx,
            "Adapter indices should match"
        );
        assert_eq!(
            event_candidate.gate_q15, decision_candidate.gate_q15,
            "Gate values should match"
        );
        assert_eq!(
            event_candidate.raw_score, decision_candidate.raw_score,
            "Raw scores should match"
        );
    }
}

#[test]
fn test_router_increments_step_counter() {
    let (writer, mut receiver) = RouterDecisionWriter::new();
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_telemetry_writer(writer);

    // Make multiple routing decisions
    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];

    for expected_step in 0..5 {
        let adapter_info: Vec<AdapterInfo> = (0..priors.len())
            .map(|i| AdapterInfo {
                id: format!("test_adapter_{}", i),
                framework: None,
                languages: vec![],
                tier: "warm".to_string(),
                ..Default::default()
            })
            .collect();
        let policy_mask = allow_all_mask(&adapter_info);
        let _ = router
            .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
            .expect("router decision");

        let event = receiver
            .try_recv()
            .expect("Should have emitted telemetry event");
        assert_eq!(event.step, expected_step, "Step counter should increment");
    }
}

#[test]
fn test_router_propagates_stack_hash() {
    let (writer, mut receiver) = RouterDecisionWriter::new();
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_telemetry_writer(writer);

    // Set active stack
    let stack_hash = adapteros_core::B3Hash::hash(b"test-stack");
    router.set_active_stack(
        Some("test-stack".to_string()),
        Some(vec!["adapter1".to_string(), "adapter2".to_string()]),
        Some(stack_hash),
    );

    // Make routing decision
    let features = vec![0.5; 22];
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
    let policy_mask = allow_all_mask(&adapter_info);
    let _ = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .expect("router decision");

    // Verify stack hash was propagated
    let event = receiver
        .try_recv()
        .expect("Should have emitted telemetry event");
    assert!(event.stack_hash.is_some(), "Stack hash should be present");
    assert_eq!(
        event.stack_hash.unwrap(),
        stack_hash.to_short_hex(),
        "Stack hash should match"
    );
    assert_eq!(
        event.stack_id,
        Some("test-stack".to_string()),
        "Stack ID should match"
    );
}

#[test]
fn test_router_without_telemetry_writer_works() {
    // Router should work fine without telemetry writer
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);

    let features = vec![0.5; 22];
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
    let policy_mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .expect("router decision");

    assert_eq!(decision.indices.len(), 3, "Should still route correctly");
}

#[test]
fn test_telemetry_writer_bounded_channel_drops_on_overflow() {
    // Create writer with small capacity
    let (writer, mut receiver) = RouterDecisionWriter::with_capacity(2);
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_telemetry_writer(writer.clone());

    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];

    // Fill the channel - use same adapter list format as other tests
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
    let policy_mask = allow_all_mask(&adapter_info);
    let _ = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .expect("router decision"); // Event 0

    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            ..Default::default()
        })
        .collect();
    let policy_mask = allow_all_mask(&adapter_info);
    let _ = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .expect("router decision"); // Event 1

    // These should be dropped (channel full)
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            ..Default::default()
        })
        .collect();
    let policy_mask = allow_all_mask(&adapter_info);
    let _ = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .expect("router decision"); // Event 2 (should be dropped)

    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("test_adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "warm".to_string(),
            ..Default::default()
        })
        .collect();
    let policy_mask = allow_all_mask(&adapter_info);
    let _ = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .expect("router decision"); // Event 3 (should be dropped)

    // Verify that we received at least some events (exact count may vary due to timing)
    let mut received_count = 0;
    while receiver.try_recv().is_ok() {
        received_count += 1;
    }

    // We should have received at most 2 events (capacity of channel)
    assert!(
        received_count <= 2,
        "Should receive at most 2 events due to channel capacity, got {}",
        received_count
    );

    // We should have received at least 1 event
    assert!(
        received_count >= 1,
        "Should receive at least 1 event, got {}",
        received_count
    );

    // Verify that some events were processed
    assert!(
        writer.total_count() >= 2,
        "Should have attempted at least 2 events, got {}",
        writer.total_count()
    );
}

#[test]
fn test_entropy_values_match() {
    let (writer, mut receiver) = RouterDecisionWriter::new();
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    router.set_telemetry_writer(writer);

    let features = vec![0.5; 22];
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
    let policy_mask = allow_all_mask(&adapter_info);
    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &policy_mask)
        .expect("router decision");

    let event = receiver
        .try_recv()
        .expect("Should have emitted telemetry event");

    assert_eq!(
        event.entropy, decision.entropy,
        "Entropy values should match"
    );
}
