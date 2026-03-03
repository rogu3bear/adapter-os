use super::{
    apply_reasoning_hotswap_with_retry, reasoning_router::ReasoningRouterConfig,
    ReasoningSwapGuard, MAX_REASONING_SWAPS,
};
use adapteros_core::AosError;
use adapteros_lora_router::{policy_mask::PolicyMask, AdapterInfo, Router, RouterWeights};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

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

#[tokio::test]
async fn reasoning_hotswap_retries_once_then_succeeds() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_closure = Arc::clone(&attempts);

    let result = apply_reasoning_hotswap_with_retry(7, move |_adapter_id| {
        let attempts = Arc::clone(&attempts_for_closure);
        async move {
            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
            if attempt == 0 {
                Err(AosError::Worker("first attempt failed".to_string()))
            } else {
                Ok(())
            }
        }
    })
    .await;

    assert_eq!(
        attempts.load(Ordering::SeqCst),
        2,
        "hot-swap should retry once before succeeding"
    );
    assert_eq!(result.expect("retry should recover"), 2);
}

#[tokio::test]
async fn reasoning_hotswap_fails_after_retry_budget() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let attempts_for_closure = Arc::clone(&attempts);

    let err = apply_reasoning_hotswap_with_retry(9, move |_adapter_id| {
        let attempts = Arc::clone(&attempts_for_closure);
        async move {
            attempts.fetch_add(1, Ordering::SeqCst);
            Err(AosError::Worker("still failing".to_string()))
        }
    })
    .await
    .expect_err("hotswap should fail after retry budget is exhausted");

    assert_eq!(
        attempts.load(Ordering::SeqCst),
        2,
        "hot-swap should stop after configured retry budget"
    );
    assert!(
        matches!(err, AosError::Worker(msg) if msg.contains("after 2 attempts")),
        "failure should be terminal so caller cannot continue with stale reasoning decision"
    );
}

#[tokio::test]
async fn reasoning_hotswap_failure_prevents_reasoning_decision_consumption() {
    let mut pending_reasoning_decision = Some("decision");
    let mut pending_hotswap = Some(11u16);
    let mut consumed_reasoning_decision = false;

    let result = async {
        if let Some(adapter_id) = pending_hotswap.take() {
            apply_reasoning_hotswap_with_retry(adapter_id, |_adapter_id| async {
                Err(AosError::Worker("synthetic hotswap failure".to_string()))
            })
            .await?;
        }

        let _ = pending_reasoning_decision.take();
        consumed_reasoning_decision = true;
        Ok::<(), AosError>(())
    }
    .await;

    assert!(
        result.is_err(),
        "failed hotswap must terminate the request path"
    );
    assert!(
        !consumed_reasoning_decision,
        "reasoning decision should not be consumed after failed hotswap"
    );
    assert!(
        pending_reasoning_decision.is_some(),
        "pending reasoning decision should remain untouched on failed hotswap"
    );
}
