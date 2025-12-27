//! Harmony trace coverage test
//!
//! Boots a minimal replayable trace that exercises all major subsystems:
//! Ingest -> Router -> Worker (with RAG) -> DB. The test ensures that the
//! recorded trace contains spans for each subsystem under a single trace_id.

use adapteros_core::B3Hash;
use adapteros_replay::ReplaySession;
use adapteros_telemetry::events::{RouterCandidate, RouterDecisionEvent};
use adapteros_telemetry::tracing::{Span, SpanKind, SpanStatus, TraceContext};
use adapteros_trace::{
    schema::{Event, EventMetadata, TraceBundle},
    write_trace_bundle,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

fn base_metadata() -> EventMetadata {
    EventMetadata {
        global_seed: B3Hash::hash(b"harmony-global-seed"),
        plan_id: "harmony-plan".to_string(),
        cpid: "harmony-tenant".to_string(),
        tenant_id: "harmony-tenant".to_string(),
        session_id: "harmony-session".to_string(),
        adapter_ids: vec!["adapter-primary".to_string()],
        memory_usage_mb: 0,
        gpu_utilization_pct: 0.0,
        custom: HashMap::new(),
    }
}

fn span_event(tick_id: u64, op_id: &str, span: &Span, metadata: &EventMetadata) -> Event {
    let mut outputs = HashMap::new();
    outputs.insert(
        "span".to_string(),
        serde_json::to_value(span).expect("serialize span"),
    );
    Event::new(
        tick_id,
        op_id.to_string(),
        "telemetry.span".to_string(),
        HashMap::new(),
        outputs,
        metadata.clone(),
    )
}

#[tokio::test]
async fn harmony_trace_contains_all_subsystems() {
    let temp_dir = new_test_tempdir();
    let trace_path = temp_dir.path().join("harmony_trace.ndjson");

    let metadata = base_metadata();
    let mut bundle = TraceBundle::new(
        metadata.global_seed,
        metadata.plan_id.clone(),
        metadata.cpid.clone(),
        metadata.tenant_id.clone(),
        metadata.session_id.clone(),
    );

    let root_ctx = TraceContext::new_root();

    let mut ingest_span = Span::new(
        root_ctx.clone(),
        "ingest.document".to_string(),
        SpanKind::Server,
    );
    ingest_span.set_attribute("subsystem".into(), "ingest".into());
    ingest_span.set_attribute("doc_id".into(), "doc-123".into());
    ingest_span.end(SpanStatus::Ok);

    let mut router_span = Span::new(
        root_ctx.create_child_span(),
        "router.decision".to_string(),
        SpanKind::Internal,
    );
    router_span.set_attribute("subsystem".into(), "router".into());
    router_span.set_attribute("route_choice".into(), "adapter-primary".into());
    router_span.end(SpanStatus::Ok);

    let mut worker_span = Span::new(
        root_ctx.create_child_span(),
        "worker.inference".to_string(),
        SpanKind::Server,
    );
    worker_span.set_attribute("subsystem".into(), "worker".into());
    worker_span.set_attribute("rag".into(), "true".into());
    worker_span.end(SpanStatus::Ok);

    let mut rag_span = Span::new(
        root_ctx.create_child_span(),
        "rag.evidence".to_string(),
        SpanKind::Internal,
    );
    rag_span.set_attribute("subsystem".into(), "rag".into());
    rag_span.set_attribute("doc_id".into(), "doc-123".into());
    rag_span.end(SpanStatus::Ok);

    let mut db_span = Span::new(
        root_ctx.create_child_span(),
        "db.write".to_string(),
        SpanKind::Client,
    );
    db_span.set_attribute("subsystem".into(), "db".into());
    db_span.set_attribute("table".into(), "inference_traces".into());
    db_span.end(SpanStatus::Ok);

    let mut ingest_inputs = HashMap::new();
    ingest_inputs.insert("doc_id".to_string(), json!("doc-123"));
    ingest_inputs.insert("source".to_string(), json!("harmony-fixture"));
    let mut ingest_outputs = HashMap::new();
    ingest_outputs.insert("chunks_indexed".to_string(), json!(2));
    ingest_outputs.insert("rag_ready".to_string(), json!(true));
    bundle.add_event(Event::new(
        0,
        "ingest-doc".to_string(),
        "ingest.document".to_string(),
        ingest_inputs,
        ingest_outputs,
        metadata.clone(),
    ));

    let router_event = RouterDecisionEvent {
        step: 0,
        input_token_id: Some(11),
        candidate_adapters: vec![RouterCandidate {
            adapter_idx: 0,
            raw_score: 1.0,
            gate_q15: 16384,
        }],
        entropy: 0.0,
        tau: 0.7,
        entropy_floor: 0.01,
        stack_hash: Some("stack-harmony".to_string()),
        stack_id: None,
        stack_version: None,
        model_type: adapteros_types::routing::RouterModelType::Dense,
        active_experts: None,
    };
    let mut router_outputs = HashMap::new();
    router_outputs.insert(
        "decision".to_string(),
        serde_json::to_value(&router_event).expect("serialize router decision"),
    );
    bundle.add_event(Event::new(
        1,
        "router-decision".to_string(),
        "router.decision".to_string(),
        HashMap::new(),
        router_outputs,
        metadata.clone(),
    ));

    let mut worker_outputs = HashMap::new();
    worker_outputs.insert("rag_used".to_string(), Value::Bool(true));
    worker_outputs.insert("response_tokens".to_string(), json!(5));
    bundle.add_event(Event::new(
        2,
        "worker-step".to_string(),
        "worker.inference".to_string(),
        HashMap::new(),
        worker_outputs,
        metadata.clone(),
    ));

    let mut db_outputs = HashMap::new();
    db_outputs.insert("rows_written".to_string(), json!(1));
    db_outputs.insert("table".to_string(), json!("inference_traces"));
    bundle.add_event(Event::new(
        3,
        "db-write".to_string(),
        "db.write".to_string(),
        HashMap::new(),
        db_outputs,
        metadata.clone(),
    ));

    let span_events = vec![
        span_event(4, "ingest-span", &ingest_span, &metadata),
        span_event(5, "router-span", &router_span, &metadata),
        span_event(6, "worker-span", &worker_span, &metadata),
        span_event(7, "rag-span", &rag_span, &metadata),
        span_event(8, "db-span", &db_span, &metadata),
    ];

    for event in span_events {
        bundle.add_event(event);
    }

    write_trace_bundle(&trace_path, bundle).expect("write trace bundle");

    let mut session =
        ReplaySession::from_log(&trace_path).expect("Failed to create harmony replay session");
    session.run().await.expect("Harmony replay run failed");

    let trace_bundle = session.trace_bundle();
    let spans: Vec<Span> = trace_bundle
        .events
        .iter()
        .filter(|e| e.event_type == "telemetry.span")
        .filter_map(|e| e.outputs.get("span"))
        .filter_map(|value| serde_json::from_value::<Span>(value.clone()).ok())
        .collect();

    let span_names: HashSet<String> = spans.iter().map(|s| s.name.clone()).collect();
    for expected in [
        "ingest.document",
        "router.decision",
        "worker.inference",
        "db.write",
    ] {
        assert!(
            span_names.contains(expected),
            "Missing span for subsystem: {}",
            expected
        );
    }

    assert!(
        span_names.contains("rag.evidence"),
        "RAG evidence span should be recorded"
    );

    let trace_ids: HashSet<String> = spans.iter().map(|s| s.context.trace_id.clone()).collect();
    assert_eq!(
        trace_ids.len(),
        1,
        "All subsystem spans should share the same trace_id"
    );

    assert!(
        trace_bundle
            .events
            .iter()
            .any(|e| e.event_type == "ingest.document"),
        "Ingest document event should be present for RAG setup"
    );
}
