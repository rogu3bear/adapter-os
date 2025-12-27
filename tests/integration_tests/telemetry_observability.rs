use adapteros_core::identity::IdentityEnvelope;
use adapteros_server_api::telemetry::TelemetryBuffer;
use adapteros_telemetry::{
    build_health_event, build_inference_metrics_event, build_routing_event, make_health_payload,
    HealthEventKind, InferenceMetricsEvent, RoutingTelemetryEvent,
};

#[tokio::test]
async fn telemetry_observability_flow_records_events() {
    let buffer = TelemetryBuffer::new(50);
    let identity = IdentityEnvelope::new(
        "tenant-int".to_string(),
        "api".to_string(),
        "inference".to_string(),
        "test".to_string(),
    );

    let health_payload = make_health_payload(
        "worker-int",
        "tenant-int",
        HealthEventKind::HealthStateChange,
        None,
        Some("serving".to_string()),
        None,
        None,
        None,
    );
    let health_event = build_health_event(identity.clone(), health_payload).unwrap();
    buffer.push(health_event).await.unwrap();

    let metrics_payload = InferenceMetricsEvent {
        tenant_id: "tenant-int".into(),
        request_id: "req-int".into(),
        model_id: "model-int".into(),
        adapter_set: vec!["a1".into(), "a2".into()],
        seed_present: true,
        latency_ms: Some(12),
        input_tokens: Some(3),
        output_tokens: Some(7),
        success: true,
        error: None,
    };
    let metrics_event =
        build_inference_metrics_event(identity.clone(), metrics_payload).unwrap();
    buffer.push(metrics_event).await.unwrap();

    let routing_payload = RoutingTelemetryEvent {
        tenant_id: "tenant-int".into(),
        request_id: "req-int".into(),
        model_id: Some("model-int".into()),
        worker_id: Some("worker-int".into()),
        adapter_ids: vec!["a1".into(), "a2".into()],
        determinism_mode: Some("strict".into()),
        seed_hash: Some("router-seed-int".into()),
        router_decisions: vec![adapteros_api_types::inference::RouterDecision {
            step: 0,
            input_token_id: Some(1),
            candidate_adapters: vec![adapteros_api_types::inference::RouterCandidate {
                adapter_idx: 0,
                raw_score: 1.0,
                gate_q15: 123,
            }],
            entropy: 0.1,
            tau: 1.0,
            entropy_floor: 0.01,
            stack_hash: None,
            interval_id: None,
            allowed_mask: Some(vec![true]),
            policy_mask_digest: Some(adapteros_core::B3Hash::hash(b"mask")),
            policy_overrides_applied: Some(adapteros_api_types::inference::PolicyOverrideFlags {
                allow_list: true,
                deny_list: false,
                trust_state: false,
            }),
            model_type: adapteros_api_types::inference::RouterModelType::Dense,
            active_experts: None,
        }],
        router_decision_chain: None,
        is_replay: false,
    };
    let routing_event = build_routing_event(identity, routing_payload).unwrap();
    buffer.push(routing_event).await.unwrap();

    assert_eq!(buffer.len().await, 3);

    let filters =
        adapteros_telemetry::unified_events::TelemetryFilters::with_tenant("tenant-int");
    let events = buffer
        .query(&filters)
        .expect("telemetry query should succeed");
    assert!(events.iter().any(|e| e.event_type == "health.lifecycle"));
    assert!(events.iter().any(|e| e.event_type == "inference.metrics"));

    let routing_meta = events
        .iter()
        .find(|e| e.event_type == "routing.decision_chain")
        .and_then(|e| e.metadata.as_ref())
        .cloned()
        .expect("routing telemetry metadata present");

    assert_eq!(routing_meta["router_decisions"][0]["step"], 0);
    assert_eq!(
        routing_meta["router_decisions"][0]["candidate_adapters"][0]["gate_q15"],
        123
    );
    assert_eq!(routing_meta["tenant_id"], "tenant-int");
    assert_eq!(routing_meta["is_replay"], false);

    let routing_event = events
        .iter()
        .find(|e| e.event_type == "routing.decision_chain")
        .expect("routing telemetry event present");
    assert_eq!(routing_event.identity.tenant_id, "tenant-int");
}

#[tokio::test]
async fn routing_replay_event_carries_tenant() {
    let buffer = TelemetryBuffer::new(10);
    let identity = IdentityEnvelope::new(
        "tenant-replay".to_string(),
        "api".to_string(),
        "inference-replay".to_string(),
        "test".to_string(),
    );

    let routing_payload = RoutingTelemetryEvent {
        tenant_id: "tenant-replay".into(),
        request_id: "req-replay".into(),
        model_id: Some("model-r".into()),
        worker_id: Some("worker-r".into()),
        adapter_ids: vec!["a3".into()],
        determinism_mode: Some("strict".into()),
        seed_hash: Some("router-seed-replay".into()),
        router_decisions: vec![adapteros_api_types::inference::RouterDecision {
            step: 1,
            input_token_id: Some(2),
            candidate_adapters: vec![adapteros_api_types::inference::RouterCandidate {
                adapter_idx: 1,
                raw_score: 0.5,
                gate_q15: 234,
            }],
            entropy: 0.2,
            tau: 1.1,
            entropy_floor: 0.02,
            stack_hash: Some("stack-hash".into()),
            interval_id: None,
            allowed_mask: Some(vec![false, true]),
            policy_mask_digest: Some(adapteros_core::B3Hash::hash(b"mask2")),
            policy_overrides_applied: Some(adapteros_api_types::inference::PolicyOverrideFlags {
                allow_list: false,
                deny_list: true,
                trust_state: false,
            }),
            model_type: adapteros_api_types::inference::RouterModelType::Dense,
            active_experts: None,
        }],
        router_decision_chain: None,
        is_replay: true,
    };

    let routing_event = build_routing_event(identity, routing_payload).unwrap();
    buffer.push(routing_event).await.unwrap();

    let filters =
        adapteros_telemetry::unified_events::TelemetryFilters::with_tenant("tenant-replay");
    let events = buffer.query(&filters).expect("telemetry query");
    let routing_event = events
        .iter()
        .find(|e| e.event_type == "routing.decision_chain")
        .expect("routing telemetry event present");

    assert_eq!(routing_event.identity.tenant_id, "tenant-replay");
    let meta = routing_event.metadata.as_ref().expect("metadata present");
    assert_eq!(meta["tenant_id"], "tenant-replay");
    assert_eq!(meta["is_replay"], true);
}
