use super::{reasoning_router::ReasoningRouterConfig, ReasoningSwapGuard, MAX_REASONING_SWAPS};
use adapteros_core::AosError;
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};

#[test]
fn swap_loop_triggers_reasoning_guard() {
    let (mut router, adapter_info, priors, policy_mask) = reasoning_router_fixture();
    let mut guard = ReasoningSwapGuard::new(MAX_REASONING_SWAPS);

    let mut last_adapter = None;
    for idx in 0..MAX_REASONING_SWAPS {
        let rationale = if idx % 2 == 0 {
            "<thinking>Switch to python for this step</thinking>"
        } else {
            "<thinking>Go back to creative writing now</thinking>"
        };
        let decision = router
            .route_on_reasoning(rationale, &priors, &adapter_info, &policy_mask, None)
            .expect("router decision");
        let adapter_id = adapter_info[decision.indices[0] as usize].id.clone();

        if let Some(prev) = last_adapter.as_ref() {
            assert_ne!(
                prev, &adapter_id,
                "swap loop should alternate adapters at step {idx}"
            );
        }
        last_adapter = Some(adapter_id);

        let result = guard.record_swap();
        if idx + 1 < MAX_REASONING_SWAPS {
            assert!(result.is_ok(), "swap {idx} should be permitted");
        } else {
            let err = result.expect_err("50th swap must trip guard");
            assert!(
                matches!(err, AosError::ReasoningLoop(_)),
                "guard must return AosError::ReasoningLoop"
            );
        }
    }
}

fn reasoning_router_fixture() -> (Router, Vec<AdapterInfo>, Vec<f32>, PolicyMask) {
    let mut router = Router::new_with_weights(RouterWeights::default(), 1, 1.0, 0.02);
    let adapter_info = vec![
        AdapterInfo {
            id: "creative-writer".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["creative".to_string()],
            ..Default::default()
        },
        AdapterInfo {
            id: "python-coder".to_string(),
            framework: None,
            languages: vec![0],
            tier: "default".to_string(),
            reasoning_specialties: vec!["python".to_string()],
            ..Default::default()
        },
    ];
    let priors = vec![0.55, 0.55];
    let adapter_ids: Vec<String> = adapter_info.iter().map(|a| a.id.clone()).collect();
    let policy_mask = PolicyMask::allow_all(&adapter_ids, None);

    (router, adapter_info, priors, policy_mask)
}

#[test]
fn reasoning_span_selects_adapter_consistent_with_router() {
    let (mut router, adapter_info, priors, policy_mask) = reasoning_router_fixture();
    let config = ReasoningRouterConfig {
        thinking_token: "<thinking>".to_string(),
        ..ReasoningRouterConfig::default()
    };

    let rationale = format!("{}Switch to python for this step.", config.thinking_token);
    let decision = router
        .route_on_reasoning(&rationale, &priors, &adapter_info, &policy_mask, None)
        .expect("router decision");

    let adapter_id = adapter_info[decision.indices[0] as usize].id.clone();
    assert_eq!(adapter_id, "python-coder");
}
