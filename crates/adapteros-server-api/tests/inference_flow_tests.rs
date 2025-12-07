use adapteros_api_types::ErrorResponse;
use adapteros_api_types::InferRequest;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::version::API_SCHEMA_VERSION;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::chat_sessions::CreateChatSessionParams;
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::routing_decisions::RoutingDecision;
use adapteros_db::workers::WorkerRegistrationParams;
use adapteros_server_api::handlers::inference::infer;
use adapteros_server_api::handlers::routing_decisions::{
    get_routing_decisions, get_session_router_view, RoutingDecisionsQuery,
};
use adapteros_server_api::middleware::request_id::RequestId;
use adapteros_server_api::types::{InferResponse, RouterSummary, WorkerInferResponse, WorkerTrace};
use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::Utc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn ready_model_happy_path_inference_and_routing() {
    let manifest_hash = "test-manifest-hash";
    let backend_name = "mlx";
    let uds_path = "/tmp/aos-happy-worker.sock";
    let request_id = "session-general-request";
    let model_name = "test-model-id";
    let adapter_id = "adapter-a";

    // Build state with manifest/backend info and mark model ready
    let base_state = setup_state(None).await.expect("state");
    let state = base_state.with_manifest_info(manifest_hash.to_string(), backend_name.to_string());
    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test-rev".to_string(),
    );

    // Register a model and mark it ready for the tenant
    let model_params = ModelRegistrationBuilder::new()
        .name(model_name)
        .hash_b3("hash-b3")
        .config_hash_b3("config-hash-b3")
        .tokenizer_hash_b3("tok-hash-b3")
        .tokenizer_cfg_hash_b3("tokcfg-hash-b3")
        .build()
        .expect("model params");
    let model_id = state
        .db
        .register_model(model_params)
        .await
        .expect("register model");
    state
        .db
        .update_base_model_status(&claims.tenant_id, &model_id, "ready", None, Some(1024))
        .await
        .expect("mark model ready");

    // Session for source_type filtering
    state
        .db
        .create_chat_session(CreateChatSessionParams {
            id: request_id.to_string(),
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
        })
        .await
        .expect("create chat session");

    // Seed manifest/node/plan records to satisfy worker FKs
    adapteros_db::sqlx::query(
        "INSERT INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind("manifest-test")
    .bind(&claims.tenant_id)
    .bind(manifest_hash)
    .bind("{}")
    .execute(state.db.pool())
    .await
    .expect("seed manifest");

    adapteros_db::sqlx::query(
        "INSERT INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
    )
    .bind("node-1")
    .bind("node-1.local")
    .bind("http://localhost:0")
    .execute(state.db.pool())
    .await
    .expect("seed node");

    adapteros_db::sqlx::query(
        "INSERT INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3, metadata_json) VALUES (?, ?, ?, ?, '[]', ?, NULL)",
    )
    .bind("plan-1")
    .bind(&claims.tenant_id)
    .bind("plan-b3")
    .bind(manifest_hash)
    .bind("layout-hash")
    .execute(state.db.pool())
    .await
    .expect("seed plan");

    // Register adapter required by inference request
    let adapter_params = AdapterRegistrationBuilder::new()
        .tenant_id(claims.tenant_id.clone())
        .adapter_id(adapter_id.to_string())
        .name(adapter_id.to_string())
        .hash_b3("adapter-hash-a")
        .rank(4)
        .targets_json(r#"["q_proj"]"#)
        .base_model_id(Some(model_id.clone()))
        .build()
        .expect("adapter params");
    state
        .db
        .register_adapter(adapter_params)
        .await
        .expect("register adapter");

    // Fake worker that returns a minimal inference response and records a metric
    let metrics_registry = state.metrics_registry.clone();
    let uds_path_owned = uds_path.to_string();
    let worker_response = WorkerInferResponse {
        text: Some("worker response".to_string()),
        status: "stop".to_string(),
        trace: WorkerTrace {
            router_summary: RouterSummary {
                adapters_used: vec![adapter_id.to_string()],
            },
            router_decisions: None,
            router_decision_chain: None,
        },
        backend_used: Some(backend_name.to_string()),
        fallback_triggered: false,
        determinism_mode_applied: Some("strict".to_string()),
        unavailable_pinned_adapters: Some(vec!["missing-pin".to_string()]),
        pinned_routing_fallback: Some("stack_only".to_string()),
        placement_trace: None,
    };

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let worker_handle = tokio::spawn(async move {
        let _ = tokio::fs::remove_file(&uds_path_owned).await;
        let listener = UnixListener::bind(&uds_path_owned).expect("bind uds");
        let _ = ready_tx.send(());

        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;

            metrics_registry
                .record_metric("inference.requests".to_string(), 1.0)
                .await;

            let body = serde_json::to_string(&worker_response).expect("serialize response");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    ready_rx.await.expect("worker bound");

    // Register and promote worker to serving so routing can find it
    let worker_id = "worker-happy";
    let registration = WorkerRegistrationParams {
        worker_id: worker_id.to_string(),
        tenant_id: claims.tenant_id.clone(),
        node_id: "node-1".to_string(),
        plan_id: "plan-1".to_string(),
        uds_path: uds_path.to_string(),
        pid: 4242,
        manifest_hash: manifest_hash.to_string(),
        schema_version: API_SCHEMA_VERSION.to_string(),
        api_version: API_SCHEMA_VERSION.to_string(),
    };
    state
        .db
        .register_worker(registration)
        .await
        .expect("register worker");
    state
        .db
        .transition_worker_status(worker_id, "serving", "ready for tests", Some("tester"))
        .await
        .expect("transition worker");

    let metrics_before = state
        .metrics_registry
        .get_series_async("inference.requests")
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    let telemetry_before = state.telemetry_buffer.len().await;

    // Invoke inference handler through the router path
    let infer_req = InferRequest {
        prompt: "Hello world".to_string(),
        model: Some(model_id.clone()),
        session_id: Some(request_id.to_string()),
        adapters: Some(vec![adapter_id.to_string()]),
        ..Default::default()
    };

    let response = infer(
        State(state.clone()),
        Extension(claims.clone()),
        Extension(identity),
        Some(Extension(RequestId(request_id.to_string()))),
        None,
        Json(infer_req),
    )
    .await
    .expect("inference should succeed");

    worker_handle.await.expect("worker finished");

    let payload: InferResponse = response.0;
    assert_eq!(payload.text, "worker response");
    assert_eq!(payload.adapters_used, vec![adapter_id.to_string()]);
    assert!(payload.model.is_none());
    assert_eq!(payload.finish_reason, "stop");

    // Metrics and telemetry should observe the inference
    let metrics_after = state
        .metrics_registry
        .get_series_async("inference.requests")
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    assert_eq!(
        metrics_after,
        metrics_before + 1,
        "metrics registry should record an inference data point for inference.requests"
    );
    let telemetry_after = state.telemetry_buffer.len().await;
    assert_eq!(
        telemetry_after,
        telemetry_before + 1,
        "telemetry buffer should capture pinned-adapter fallback event"
    );

    // Insert a routing decision tied to this request/session for endpoint verification
    let decision = RoutingDecision {
        id: "decision-happy".to_string(),
        tenant_id: claims.tenant_id.clone(),
        timestamp: Utc::now().to_rfc3339(),
        request_id: Some(request_id.to_string()),
        step: 1,
        input_token_id: Some(0),
        stack_id: None,
        stack_hash: None,
        entropy: 0.25,
        tau: 1.0,
        entropy_floor: 0.01,
        k_value: Some(1),
        candidate_adapters: serde_json::json!([{
            "adapter_idx": 0,
            "raw_score": 0.9,
            "gate_q15": 100
        }])
        .to_string(),
        selected_adapter_ids: Some(model_id.to_string()),
        router_latency_us: Some(10),
        total_inference_latency_us: Some(50),
        overhead_pct: Some(1.5),
        created_at: Utc::now().to_rfc3339(),
    };
    state
        .db
        .insert_routing_decision(&decision)
        .await
        .expect("insert routing decision");

    // Query routing decisions filtered by tenant + source_type
    let routing = get_routing_decisions(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Query(RoutingDecisionsQuery {
            tenant: claims.tenant_id.clone(),
            limit: Some(10),
            offset: None,
            since: None,
            until: None,
            stack_id: None,
            adapter_id: None,
            source_type: Some("general".to_string()),
            min_entropy: None,
            max_overhead_pct: None,
            anomalies_only: Some(false),
        }),
    )
    .await
    .expect("routing decisions");

    assert_eq!(routing.0.items.len(), 1);
    assert_eq!(routing.0.items[0].request_id.as_deref(), Some(request_id));

    // Session-specific routing view should also surface the decision
    let session_view = get_session_router_view(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(request_id.to_string()),
    )
    .await
    .expect("session router view");
    assert_eq!(session_view.0.request_id, request_id);
    assert_eq!(session_view.0.total_steps, 1);
}

#[tokio::test]
async fn not_ready_model_fails_fast_with_model_not_ready_code() {
    let manifest_hash = "test-manifest-not-ready";
    let backend_name = "mlx";
    let base_state = setup_state(None).await.expect("state");
    let state = base_state.with_manifest_info(manifest_hash.to_string(), backend_name.to_string());
    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test-rev".to_string(),
    );

    // No base model status records -> aggregates to NoModel
    let infer_req = InferRequest {
        prompt: "Hello world".to_string(),
        model: Some("missing-model".to_string()),
        ..Default::default()
    };

    let err = infer(
        State(state),
        Extension(claims),
        Extension(identity),
        Some(Extension(RequestId("not-ready-request".to_string()))),
        None,
        Json(infer_req),
    )
    .await
    .expect_err("inference should fail fast when model is not ready");

    let (status, Json(body)): (StatusCode, Json<ErrorResponse>) = err;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body.code, "MODEL_NOT_READY");
}
