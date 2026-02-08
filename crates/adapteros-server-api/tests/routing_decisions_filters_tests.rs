use adapteros_db::routing_decisions::RoutingDecision;
use adapteros_server_api::handlers::get_routing_history;
use adapteros_server_api::handlers::routing_decisions::{
    get_routing_decisions, RoutingDecisionsQuery,
};
use adapteros_server_api::types::RoutingHistoryQuery;
use axum::{
    extract::{Query, State},
    Extension,
};
use chrono::Utc;
use serde_json::json;

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn routing_decisions_filter_by_source_type() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    // Create chat sessions with different source_types
    let doc_session = "session-doc";
    let general_session = "session-general";

    state
        .db
        .create_chat_session(adapteros_db::chat_sessions::CreateChatSessionParams {
            id: doc_session.to_string(),
            tenant_id: claims.tenant_id.clone(),
            user_id: Some(claims.sub.clone()),
            created_by: Some(claims.sub.clone()),
            stack_id: None,
            collection_id: None,
            document_id: None,
            name: "Doc chat".to_string(),
            title: None,
            source_type: Some("document".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None,
            codebase_adapter_id: None,
        })
        .await
        .expect("doc session");

    state
        .db
        .create_chat_session(adapteros_db::chat_sessions::CreateChatSessionParams {
            id: general_session.to_string(),
            tenant_id: claims.tenant_id.clone(),
            user_id: Some(claims.sub.clone()),
            created_by: Some(claims.sub.clone()),
            stack_id: None,
            collection_id: None,
            document_id: None,
            name: "General chat".to_string(),
            title: None,
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None,
            codebase_adapter_id: None,
        })
        .await
        .expect("general session");

    // Insert routing decisions tied to the sessions
    for (idx, request_id) in [doc_session, general_session].iter().enumerate() {
        let decision = RoutingDecision {
            id: format!("decision-{}", idx),
            tenant_id: claims.tenant_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            request_id: Some(request_id.to_string()),
            step: 1,
            input_token_id: Some(0),
            stack_id: None,
            stack_hash: None,
            entropy: 0.1,
            tau: 1.0,
            entropy_floor: 0.01,
            k_value: Some(1),
            candidate_adapters: json!([{
                "adapter_idx": 0,
                "raw_score": 0.5,
                "gate_q15": 100
            }])
            .to_string(),
            selected_adapter_ids: Some("adapter-0".to_string()),
            router_latency_us: Some(10),
            total_inference_latency_us: Some(1000),
            overhead_pct: Some(1.0),
            created_at: Utc::now().to_rfc3339(),
        };
        state
            .db
            .insert_routing_decision(&decision)
            .await
            .expect("insert decision");
    }

    // Filter by source_type=document
    let query = RoutingDecisionsQuery {
        tenant: Some(claims.tenant_id.clone()),
        limit: None,
        offset: None,
        since: None,
        until: None,
        stack_id: None,
        adapter_id: None,
        source_type: Some("document".to_string()),
        min_entropy: None,
        max_overhead_pct: None,
        anomalies_only: None,
    };

    let result =
        get_routing_decisions(State(state), Extension(claims), axum::extract::Query(query))
            .await
            .expect("routing decisions");

    assert_eq!(
        result.0.items.len(),
        1,
        "only document session should match"
    );
    assert_eq!(result.0.items[0].request_id.as_deref(), Some(doc_session));
}

#[tokio::test]
async fn routing_history_uses_q15_denominator_32767() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let decision = RoutingDecision {
        id: "decision-q15".to_string(),
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
        k_value: Some(1),
        candidate_adapters: json!([{
            "adapter_idx": 0,
            "raw_score": 0.5,
            "gate_q15": 32767
        }])
        .to_string(),
        selected_adapter_ids: Some("adapter-0".to_string()),
        router_latency_us: Some(10),
        total_inference_latency_us: Some(1000),
        overhead_pct: Some(1.0),
        created_at: Utc::now().to_rfc3339(),
    };

    state
        .db
        .insert_routing_decision(&decision)
        .await
        .expect("insert decision");

    let query = RoutingHistoryQuery { limit: Some(1) };
    let result = get_routing_history(State(state), Extension(claims), Query(query))
        .await
        .expect("routing history");

    assert_eq!(result.0.len(), 1);
    assert_eq!(result.0[0].adapter_scores.len(), 1);
    let gate_value = result.0[0].adapter_scores[0].gate_value;
    assert!(
        (gate_value - 1.0).abs() < 1e-6,
        "expected gate_value to use 32767.0 denominator"
    );
}
