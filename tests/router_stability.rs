use adapteros_core::AosError;
use adapteros_lora_router::{
    policy_mask::PolicyMask, AdapterInfo, Router, RouterAbstainReason, RouterWeights,
    RoutingDecision,
};

fn allow_all_mask(adapters: &[AdapterInfo]) -> PolicyMask {
    let ids: Vec<String> = adapters.iter().map(|a| a.id.clone()).collect();
    PolicyMask::allow_all(&ids, None)
}

#[test]
fn tie_breaks_use_adapter_index_for_identical_scores() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 3, 1.0, 0.02);
    let features = vec![0.0f32; 22];
    let priors = vec![0.5f32, 0.5, 0.5];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{i}"),
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    let decision = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing decision");
    let decision_again = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect("routing decision");

    assert_eq!(
        decision.indices.as_slice(),
        &[0, 1, 2],
        "Tie-breaking should preserve adapter index order"
    );
    assert_eq!(
        decision.indices, decision_again.indices,
        "Tie-breaking should be deterministic across calls"
    );
}

#[test]
fn rejects_nan_embeddings() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    let mut features = vec![0.0f32; 22];
    features[0] = f32::NAN;
    let priors = vec![0.5f32, 0.6];
    let adapter_info: Vec<AdapterInfo> = (0..priors.len())
        .map(|i| AdapterInfo {
            id: format!("adapter_{i}"),
            ..Default::default()
        })
        .collect();
    let mask = allow_all_mask(&adapter_info);

    let err = router
        .route_with_adapter_info(&features, &priors, &adapter_info, &mask)
        .expect_err("NaN features should be rejected");
    match err {
        AosError::DeterminismViolation(msg) => assert!(
            msg.contains("Non-finite router feature"),
            "Expected numerics validation error, got {msg}"
        ),
        other => panic!("Unexpected error type: {other:?}"),
    }
}

#[test]
#[allow(deprecated)]
fn empty_router_config_abstains() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    let decision = router.route(&[], &[]);

    match decision {
        RoutingDecision::Abstain(RouterAbstainReason::EmptyRouterConfig) => {}
        other => panic!("Expected empty config abstain, got {:?}", other),
    }
}

#[test]
#[allow(deprecated)]
fn low_scores_trigger_abstention() {
    let mut router = Router::new_with_weights(RouterWeights::default(), 2, 1.0, 0.02);
    let features = vec![0.0f32; 22];
    let priors = vec![0.01f32, 0.02];

    let decision = router.route(&features, &priors);
    match decision {
        RoutingDecision::Abstain(RouterAbstainReason::ScoresBelowThreshold { .. }) => {}
        other => panic!("Expected abstain for low scores, got {:?}", other),
    }
}
