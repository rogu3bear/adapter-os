//! Router policy integration tests
//!
//! Tests for PRD-ROUTER-01: Policy hooks and validation

use adapteros_lora_router::{AdapterInfo, Router, RouterWeights};
use adapteros_policy::packs::router::{
    AdapterMetadata, RouterConfig, RouterPolicy, StackConfiguration,
};

#[test]
fn test_router_respects_k_sparse_policy() {
    let mut policy_config = RouterConfig::default();
    policy_config.k_sparse = 2; // Limit to 2 adapters

    let policy = RouterPolicy::new(policy_config.clone());
    let mut router =
        Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

    // Router should have clamped K to policy maximum
    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
        })
        .collect();

    let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);

    // Should only select 2 adapters (clamped to policy)
    assert_eq!(
        decision.indices.len(),
        2,
        "Should clamp to policy K-sparse maximum"
    );
}

#[test]
fn test_policy_validates_stack_configuration() {
    let policy_config = RouterConfig::default();
    let policy = RouterPolicy::new(policy_config);

    // Valid stack
    let valid_stack = StackConfiguration {
        id: "valid-stack".to_string(),
        adapter_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
        adapters: vec![
            AdapterMetadata {
                id: "a1".to_string(),
                tier: "tier_0".to_string(),
                tags: vec![],
                forbidden_peers: vec![],
            },
            AdapterMetadata {
                id: "a2".to_string(),
                tier: "tier_1".to_string(),
                tags: vec![],
                forbidden_peers: vec![],
            },
            AdapterMetadata {
                id: "a3".to_string(),
                tier: "tier_2".to_string(),
                tags: vec![],
                forbidden_peers: vec![],
            },
        ],
    };

    assert!(policy.validate_stack_configuration(&valid_stack).is_ok());

    // Invalid stack (forbidden peer violation)
    let invalid_stack = StackConfiguration {
        id: "invalid-stack".to_string(),
        adapter_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
        adapters: vec![
            AdapterMetadata {
                id: "a1".to_string(),
                tier: "tier_1".to_string(),
                tags: vec![],
                forbidden_peers: vec!["a2".to_string()],
            },
            AdapterMetadata {
                id: "a2".to_string(),
                tier: "tier_0".to_string(),
                tags: vec![],
                forbidden_peers: vec![],
            },
            AdapterMetadata {
                id: "a3".to_string(),
                tier: "tier_2".to_string(),
                tags: vec![],
                forbidden_peers: vec![],
            },
        ],
    };

    assert!(policy.validate_stack_configuration(&invalid_stack).is_err());
}

#[test]
fn test_policy_detects_conflicting_tags() {
    let policy_config = RouterConfig::default();
    let policy = RouterPolicy::new(policy_config);

    // Stack with conflicting tags (security vs. performance)
    let conflicting_stack = StackConfiguration {
        id: "conflicting-stack".to_string(),
        adapter_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
        adapters: vec![
            AdapterMetadata {
                id: "a1".to_string(),
                tier: "tier_0".to_string(),
                tags: vec!["security".to_string()],
                forbidden_peers: vec![],
            },
            AdapterMetadata {
                id: "a2".to_string(),
                tier: "tier_1".to_string(),
                tags: vec!["performance".to_string()],
                forbidden_peers: vec![],
            },
            AdapterMetadata {
                id: "a3".to_string(),
                tier: "tier_2".to_string(),
                tags: vec![],
                forbidden_peers: vec![],
            },
        ],
    };

    let result = policy.validate_stack_configuration(&conflicting_stack);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("conflicting tags"));
}

#[test]
fn test_policy_validates_decision_entropy() {
    let policy_config = RouterConfig::default();
    let policy = RouterPolicy::new(policy_config);

    // Decision with low entropy (should fail)
    let selected_indices = vec![0, 1, 2];
    let low_entropy_gates = vec![0.95, 0.03, 0.02]; // Single adapter dominates

    let result = policy.validate_decision(&selected_indices, &low_entropy_gates, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("entropy"));

    // Decision with good entropy (should pass)
    let good_entropy_gates = vec![0.4, 0.35, 0.25];
    assert!(policy
        .validate_decision(&selected_indices, &good_entropy_gates, None)
        .is_ok());
}

#[test]
fn test_policy_validates_decision_k_limit() {
    let policy_config = RouterConfig::default();
    let policy = RouterPolicy::new(policy_config);

    // Decision exceeding K limit (should fail)
    let selected_indices = vec![0, 1, 2, 3]; // 4 adapters, but K=3
    let gates = vec![0.3, 0.3, 0.2, 0.2];

    let result = policy.validate_decision(&selected_indices, &gates, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("K-sparse limit"));
}

#[test]
fn test_policy_validates_decision_forbidden_peers() {
    let policy_config = RouterConfig::default();
    let policy = RouterPolicy::new(policy_config);

    let stack = StackConfiguration {
        id: "test-stack".to_string(),
        adapter_ids: vec!["a1".to_string(), "a2".to_string(), "a3".to_string()],
        adapters: vec![
            AdapterMetadata {
                id: "a1".to_string(),
                tier: "tier_1".to_string(),
                tags: vec![],
                forbidden_peers: vec!["a2".to_string()],
            },
            AdapterMetadata {
                id: "a2".to_string(),
                tier: "tier_0".to_string(),
                tags: vec![],
                forbidden_peers: vec![],
            },
            AdapterMetadata {
                id: "a3".to_string(),
                tier: "tier_2".to_string(),
                tags: vec![],
                forbidden_peers: vec![],
            },
        ],
    };

    // Decision selecting both a1 and a2 (forbidden peers)
    let selected_indices = vec![0, 1, 2];
    let gates = vec![0.4, 0.35, 0.25];

    let result = policy.validate_decision(&selected_indices, &gates, Some(&stack));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("forbidden peer"));

    // Decision selecting only a1 and a3 (no violation)
    let safe_indices = vec![0, 2];
    let safe_gates = vec![0.6, 0.4];

    assert!(policy
        .validate_decision(&safe_indices, &safe_gates, Some(&stack))
        .is_ok());
}

#[test]
fn test_entropy_floor_enforcement() {
    let mut policy_config = RouterConfig::default();
    policy_config.entropy_floor = 0.1; // Higher entropy floor

    let mut router =
        Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

    let features = vec![0.0; 5];
    let priors = vec![1.0, 0.0, 0.0, 0.0, 0.0]; // One dominant prior
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
        })
        .collect();

    let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);
    let gates = decision.gates_f32();

    // All gates should be >= entropy floor / k
    let min_gate = policy_config.entropy_floor / 3.0;
    for &g in &gates {
        assert!(
            g >= min_gate - 0.001,
            "Gate {} should be >= minimum {} required by policy",
            g,
            min_gate
        );
    }
}

#[test]
fn test_router_uses_policy_config_sample_tokens() {
    let mut policy_config = RouterConfig::default();
    policy_config.sample_tokens_full = 256;

    let router = Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);

    // Verify router uses policy config value
    assert_eq!(
        router.full_log_tokens, 256,
        "Router should use policy config sample_tokens_full"
    );
}

#[test]
fn test_gate_quantization_q15_validation() {
    let policy_config = RouterConfig::default();
    let policy = RouterPolicy::new(policy_config);

    // Valid Q15 gates (in range [-1.0, 1.0])
    assert!(policy.validate_gate_quantization(&[0.5, -0.3, 0.8]).is_ok());

    // Invalid Q15 gates (out of range)
    assert!(policy
        .validate_gate_quantization(&[1.5, -0.3, 0.8])
        .is_err());
    assert!(policy
        .validate_gate_quantization(&[0.5, -1.5, 0.8])
        .is_err());
}

#[test]
fn test_router_overhead_validation() {
    let policy_config = RouterConfig::default();
    let policy = RouterPolicy::new(policy_config);

    // Valid overhead (within budget)
    assert!(policy.validate_router_overhead(5.0).is_ok());

    // Invalid overhead (exceeds budget)
    assert!(policy.validate_router_overhead(10.0).is_err());
}

#[test]
fn test_feature_dimensions_validation() {
    let policy_config = RouterConfig::default();
    let policy = RouterPolicy::new(policy_config);

    // Expected dimensions: 8 + 3 + 1 + 1 + 8 + 1 = 22
    let valid_vector = vec![0.0; 22];
    let invalid_vector = vec![0.0; 20];

    assert!(policy.validate_feature_dimensions(&valid_vector).is_ok());
    assert!(policy.validate_feature_dimensions(&invalid_vector).is_err());
}

#[test]
fn test_policy_integration_with_telemetry() {
    use adapteros_telemetry::writer::RouterDecisionWriter;

    let policy_config = RouterConfig::default();
    let (writer, mut receiver) = RouterDecisionWriter::new();

    let mut router =
        Router::new_with_policy_config(RouterWeights::default(), 3, 1.0, &policy_config);
    router.set_telemetry_writer(writer);

    let features = vec![0.5; 22];
    let priors = vec![0.8, 0.6, 0.4, 0.3, 0.2];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{}", i),
            framework: None,
            languages: vec![],
            tier: "default".to_string(),
        })
        .collect();

    let decision = router.route_with_adapter_info(&features, &priors, &adapter_info);

    // Verify telemetry event was emitted
    let event = receiver
        .try_recv()
        .expect("Should have emitted telemetry event");

    // Verify event reflects policy config
    assert_eq!(event.entropy_floor, policy_config.entropy_floor);
    assert_eq!(event.tau, 1.0);
    assert_eq!(event.candidate_adapters.len(), decision.candidates.len());

    // Verify decision respects policy
    assert!(decision.indices.len() <= policy_config.k_sparse);
    assert!(decision.entropy >= policy_config.entropy_floor);
}
