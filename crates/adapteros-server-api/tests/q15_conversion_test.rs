use adapteros_db::routing_decisions::RoutingDecision;
use adapteros_server_api::handlers::routing_decisions::{
    get_routing_decisions, RoutingDecisionsQuery,
};
use axum::{extract::State, Extension};
use chrono::Utc;
use serde_json::json;

mod common;
use common::{setup_state, test_admin_claims};

/// Verify that Q15 to float conversion uses the canonical denominator 32767.0
/// This test ensures compliance with the router's Q15 fixed-point standard.
#[tokio::test]
async fn test_q15_conversion_uses_32767_denominator() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    // Test case 1: Maximum positive value (32767 → 1.0)
    let decision_max = RoutingDecision {
        id: "decision-q15-max".to_string(),
        tenant_id: claims.tenant_id.clone(),
        timestamp: Utc::now().to_rfc3339(),
        request_id: None,
        step: 1,
        input_token_id: Some(0),
        stack_id: None,
        stack_hash: None,
        entropy: 0.1,
        tau: 1.0,
        entropy_floor: 0.01,
        k_value: Some(3),
        candidate_adapters: json!([
            {
                "adapter_idx": 0,
                "raw_score": 1.0,
                "gate_q15": 32767  // Max Q15 value
            },
            {
                "adapter_idx": 1,
                "raw_score": 0.5,
                "gate_q15": 16383  // ~0.5 in Q15
            },
            {
                "adapter_idx": 2,
                "raw_score": 0.0,
                "gate_q15": 0      // Zero
            }
        ])
        .to_string(),
        selected_adapter_ids: Some("adapter-0,adapter-1,adapter-2".to_string()),
        router_latency_us: Some(10),
        total_inference_latency_us: Some(1000),
        overhead_pct: Some(1.0),
        created_at: Utc::now().to_rfc3339(),
    };

    state
        .db
        .insert_routing_decision(&decision_max)
        .await
        .expect("insert decision");

    let query = RoutingDecisionsQuery {
        tenant: Some(claims.tenant_id.clone()),
        limit: Some(1),
        offset: None,
        since: None,
        until: None,
        stack_id: None,
        adapter_id: None,
        source_type: None,
        min_entropy: None,
        max_overhead_pct: None,
        anomalies_only: None,
    };

    let result =
        get_routing_decisions(State(state), Extension(claims), axum::extract::Query(query))
            .await
            .expect("routing decisions");

    assert_eq!(result.0.items.len(), 1);
    let candidates = &result.0.items[0].candidates;
    assert_eq!(candidates.len(), 3);

    // Verify Q15 to float conversion uses 32767.0 denominator
    // gate_float = gate_q15 / 32767.0

    // Test max value: 32767 / 32767.0 = 1.0
    let gate_float_max = candidates[0].gate_float;
    let expected_max = 32767_f32 / 32767.0;
    assert!(
        (gate_float_max - expected_max).abs() < 1e-6,
        "Q15 max (32767) should convert to exactly 1.0, got {}",
        gate_float_max
    );

    // Test mid value: 16383 / 32767.0 ≈ 0.5
    let gate_float_mid = candidates[1].gate_float;
    let expected_mid = 16383_f32 / 32767.0;
    assert!(
        (gate_float_mid - expected_mid).abs() < 1e-6,
        "Q15 mid (16383) should convert to ~0.5, got {}",
        gate_float_mid
    );

    // Test zero: 0 / 32767.0 = 0.0
    let gate_float_zero = candidates[2].gate_float;
    let expected_zero = 0_f32 / 32767.0;
    assert!(
        (gate_float_zero - expected_zero).abs() < 1e-6,
        "Q15 zero (0) should convert to 0.0, got {}",
        gate_float_zero
    );

    // Critical: Verify NOT using incorrect 32768.0 denominator
    // If using 32768.0, max would be 32767/32768.0 = 0.99996948...
    let incorrect_max = 32767_f32 / 32768.0;
    assert!(
        (gate_float_max - incorrect_max).abs() > 1e-6,
        "MUST NOT use 32768.0 denominator (would give max=0.99997 instead of 1.0)"
    );
}
