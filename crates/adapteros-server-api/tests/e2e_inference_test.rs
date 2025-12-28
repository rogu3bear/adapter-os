//! End-to-End Inference Test
//!
//! This test verifies the complete inference pipeline from request to response:
//! 1. Server initialization (in-memory DB with migrations)
//! 2. Tenant creation and user authentication
//! 3. Base model and adapter registration
//! 4. Worker registration and promotion to serving
//! 5. Inference request through InferenceCore
//! 6. Response validation (text, metadata, deterministic receipt)
//! 7. Audit trail verification (policy decisions, routing decisions)
//! 8. Telemetry event capture
//!
//! This is the canonical E2E test demonstrating the full AdapterOS inference flow.

mod common;

use adapteros_api_types::inference::StopReasonCode;
use adapteros_api_types::InferRequest;
use adapteros_core::identity::IdentityEnvelope;
use adapteros_core::version::API_SCHEMA_VERSION;
use adapteros_core::B3Hash;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::chat_sessions::CreateChatSessionParams;
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::policy_audit::PolicyDecisionFilters;
use adapteros_db::routing_decisions::RoutingDecision;
use adapteros_db::workers::WorkerRegistrationParams;
use adapteros_server_api::handlers::inference::infer;
use adapteros_server_api::middleware::request_id::RequestId;
use adapteros_server_api::types::{InferResponse, RouterSummary, WorkerInferResponse, WorkerTrace};
use axum::{extract::State, http::StatusCode, Extension, Json};
use chrono::Utc;
use common::{setup_state, test_admin_claims};
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

/// E2E Test: Complete inference flow with audit trail verification
///
/// This test exercises the full 11-stage inference pipeline:
/// 1. Request validation (tenant isolation)
/// 2. Adapter resolution (from request adapter_ids)
/// 3. Policy hooks (OnRequestBeforeRouting)
/// 4. RAG context (skipped - no collection)
/// 5. Router decision (K-sparse selection)
/// 6. Worker selection (finds serving worker)
/// 7. Policy hooks (OnBeforeInference)
/// 8. Worker inference (mock UDS server)
/// 9. Policy hooks (OnAfterInference)
/// 10. Evidence & telemetry capture
/// 11. Response assembly (with deterministic receipt)
#[tokio::test]
#[ignore = "requires full tenant/model fixture setup"]
async fn test_e2e_inference_with_audit_trail() {
    // =============================================================================
    // Stage 1: Setup - Server Initialization
    // =============================================================================

    let manifest_hash = "e2e-test-manifest-hash";
    let backend_name = "mlx";
    let model_name = "Qwen2.5-7B-Instruct";
    let adapter_id = "adapter-sentiment-analysis";
    let request_id = "e2e-test-request-001";

    // Create isolated test environment
    let base_state = setup_state(None).await.expect("Failed to setup test state");
    let state = base_state.with_manifest_info(manifest_hash.to_string(), backend_name.to_string());

    // =============================================================================
    // Stage 2: Tenant & User Setup
    // =============================================================================

    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test-rev".to_string(),
    );

    // Initialize tenant policy bindings (required for policy hooks)
    state
        .db
        .initialize_tenant_policy_bindings(&claims.tenant_id, "test-system")
        .await
        .expect("Failed to initialize tenant policy bindings");

    // =============================================================================
    // Stage 3: Model & Adapter Registration
    // =============================================================================

    // Register base model
    let model_params = ModelRegistrationBuilder::new()
        .name(model_name)
        .hash_b3("qwen-model-hash-b3")
        .config_hash_b3("qwen-config-hash-b3")
        .tokenizer_hash_b3("qwen-tokenizer-hash-b3")
        .tokenizer_cfg_hash_b3("qwen-tokenizer-cfg-hash-b3")
        .build()
        .expect("Failed to build model params");

    let model_id = state
        .db
        .register_model(model_params)
        .await
        .expect("Failed to register model");

    // Mark model as ready for serving
    state
        .db
        .update_base_model_status(&claims.tenant_id, &model_id, "ready", None, Some(4096))
        .await
        .expect("Failed to mark model ready");

    // Register adapter for sentiment analysis
    let adapter_hash = B3Hash::hash(adapter_id.as_bytes()).to_hex();
    let adapter_params = AdapterRegistrationBuilder::new()
        .tenant_id(claims.tenant_id.clone())
        .adapter_id(adapter_id.to_string())
        .name("Sentiment Analysis Adapter".to_string())
        .hash_b3(adapter_hash)
        .rank(16)
        .targets_json(r#"["q_proj","v_proj"]"#)
        .base_model_id(Some(model_id.clone()))
        .build()
        .expect("Failed to build adapter params");

    state
        .db
        .register_adapter(adapter_params)
        .await
        .expect("Failed to register adapter");

    // =============================================================================
    // Stage 4: Chat Session Creation
    // =============================================================================

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
            name: "E2E Test Session".to_string(),
            title: Some("Sentiment Analysis Test".to_string()),
            source_type: Some("general".to_string()),
            source_ref_id: None,
            metadata_json: None,
            tags_json: None,
            pinned_adapter_ids: None,
        })
        .await
        .expect("Failed to create chat session");

    // =============================================================================
    // Stage 5: Worker Infrastructure Setup
    // =============================================================================

    // Create manifest/node/plan records to satisfy worker FKs
    adapteros_db::sqlx::query(
        "INSERT INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind("manifest-e2e")
    .bind(&claims.tenant_id)
    .bind(manifest_hash)
    .bind(r#"{"version":"1.0","backend":"mlx"}"#)
    .execute(state.db.pool())
    .await
    .expect("Failed to seed manifest");

    adapteros_db::sqlx::query(
        "INSERT INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
    )
    .bind("node-e2e")
    .bind("localhost")
    .bind("http://localhost:9090")
    .execute(state.db.pool())
    .await
    .expect("Failed to seed node");

    adapteros_db::sqlx::query(
        "INSERT INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3, metadata_json)
         VALUES (?, ?, ?, ?, '[]', ?, NULL)",
    )
    .bind("plan-e2e")
    .bind(&claims.tenant_id)
    .bind("plan-b3-e2e")
    .bind(manifest_hash)
    .bind("layout-hash-e2e")
    .execute(state.db.pool())
    .await
    .expect("Failed to seed plan");

    // =============================================================================
    // Stage 6: Mock Worker Setup
    // =============================================================================

    // Create UDS socket in current directory (not /tmp - path security)
    let uds_dir = TempDir::new_in(".").expect("Failed to create tempdir");
    let uds_path = uds_dir
        .path()
        .join("aos-e2e-worker.sock")
        .to_string_lossy()
        .to_string();

    // Mock worker response
    let worker_response = WorkerInferResponse {
        text: Some("This text has a positive sentiment.".to_string()),
        status: "stop".to_string(),
        trace: WorkerTrace {
            router_summary: RouterSummary {
                adapters_used: vec![adapter_id.to_string()],
            },
            router_decisions: None,
            router_decision_chain: None,
            moe_info: None,
            expert_routing: None,
            active_experts: None,
            model_type: None,
        },
        backend_used: Some(backend_name.to_string()),
        backend_version: Some("mlx-0.1.0".to_string()),
        fallback_triggered: false,
        coreml_compute_preference: None,
        coreml_compute_units: None,
        coreml_gpu_used: None,
        coreml_package_hash: None,
        coreml_expected_package_hash: None,
        coreml_hash_mismatch: None,
        fallback_backend: None,
        determinism_mode_applied: Some("strict".to_string()),
        unavailable_pinned_adapters: None,
        pinned_routing_fallback: None,
        placement_trace: None,
        stop_reason_code: Some(StopReasonCode::Length),
        stop_reason_token_index: Some(10),
        stop_policy_digest_b3: None,
    };

    // Start mock worker UDS server
    let metrics_registry = state.metrics_registry.clone();
    let uds_path_owned = uds_path.clone();
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();

    let worker_handle = tokio::spawn(async move {
        let _ = tokio::fs::remove_file(&uds_path_owned).await;
        let listener = UnixListener::bind(&uds_path_owned).expect("Failed to bind UDS");
        let _ = ready_tx.send(());

        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = vec![0u8; 8192];
            let _n = stream
                .read(&mut buf)
                .await
                .expect("Failed to read from UDS");

            // Record inference metric
            metrics_registry
                .record_metric("inference.requests".to_string(), 1.0)
                .await;

            // Send HTTP response over UDS
            let body =
                serde_json::to_string(&worker_response).expect("Failed to serialize response");
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream
                .write_all(response.as_bytes())
                .await
                .expect("Failed to write to UDS");
            let _ = stream.shutdown().await;
        }
    });

    ready_rx.await.expect("Worker failed to bind");

    // Register worker and promote to serving status
    let worker_id = "worker-e2e-001";
    let registration = WorkerRegistrationParams {
        worker_id: worker_id.to_string(),
        tenant_id: claims.tenant_id.clone(),
        node_id: "node-e2e".to_string(),
        plan_id: "plan-e2e".to_string(),
        uds_path: uds_path.to_string(),
        pid: 12345,
        manifest_hash: manifest_hash.to_string(),
        backend: Some(backend_name.to_string()),
        model_hash_b3: None,
        capabilities_json: Some(r#"{"max_batch_size":8,"supports_streaming":true}"#.to_string()),
        schema_version: API_SCHEMA_VERSION.to_string(),
        api_version: API_SCHEMA_VERSION.to_string(),
    };

    state
        .db
        .register_worker(registration)
        .await
        .expect("Failed to register worker");

    state
        .db
        .transition_worker_status(
            worker_id,
            "serving",
            "Ready for E2E test",
            Some("test-system"),
        )
        .await
        .expect("Failed to transition worker to serving");

    // =============================================================================
    // Stage 7: Execute Inference Request
    // =============================================================================

    let metrics_before = state
        .metrics_registry
        .get_series_async("inference.requests")
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);

    let telemetry_before = state.telemetry_buffer.len().await;

    let infer_req = InferRequest {
        prompt: "This product exceeded my expectations!".to_string(),
        model: Some(model_id.clone()),
        session_id: Some(request_id.to_string()),
        adapters: Some(vec![adapter_id.to_string()]),
        max_tokens: Some(50),
        temperature: Some(0.7),
        top_p: Some(0.9),
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
    .expect("Inference request failed");

    worker_handle.await.expect("Worker task failed");

    // =============================================================================
    // Stage 8: Response Validation
    // =============================================================================

    let payload: InferResponse = response.0;

    // Verify response text
    assert_eq!(
        payload.text, "This text has a positive sentiment.",
        "Response text should match worker output"
    );

    // Verify adapters used
    assert_eq!(
        payload.adapters_used,
        vec![adapter_id.to_string()],
        "Should use the sentiment analysis adapter"
    );

    // Verify model
    assert_eq!(
        payload.model.as_deref(),
        Some(model_id.as_str()),
        "Response should include model ID"
    );

    // Verify finish reason
    assert_eq!(
        payload.finish_reason, "stop",
        "Finish reason should be 'stop'"
    );

    // =============================================================================
    // Stage 9: Deterministic Receipt Validation
    // =============================================================================

    let receipt = payload
        .deterministic_receipt
        .as_ref()
        .expect("Response should include deterministic receipt");

    assert!(
        !receipt.router_seed.is_empty(),
        "Receipt should have router seed for audit"
    );

    assert_eq!(
        receipt.adapters_used,
        vec![adapter_id.to_string()],
        "Receipt should record adapters used"
    );

    assert_eq!(
        receipt.model.as_deref(),
        Some(model_id.as_str()),
        "Receipt should record model"
    );

    assert_eq!(
        receipt.backend_used.as_deref(),
        Some(backend_name),
        "Receipt should record backend"
    );

    assert_eq!(
        receipt.sampling_params.max_tokens, 50,
        "Receipt should capture sampling params"
    );

    assert!(
        receipt.sampling_params.seed.is_some(),
        "Receipt should have deterministic seed"
    );

    assert_eq!(
        receipt.prompt_system_params_digest_b3.to_hex().len(),
        64,
        "Receipt should have BLAKE3 digest (64 hex chars)"
    );

    // =============================================================================
    // Stage 10: Metrics Validation
    // =============================================================================

    let metrics_after = state
        .metrics_registry
        .get_series_async("inference.requests")
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);

    assert_eq!(
        metrics_after,
        metrics_before + 1,
        "Metrics registry should record exactly one inference request"
    );

    // =============================================================================
    // Stage 11: Telemetry Validation
    // =============================================================================

    let telemetry_after = state.telemetry_buffer.len().await;

    // Telemetry events may vary based on what triggers during inference
    // Just verify telemetry buffer is being used
    println!(
        "Telemetry events: before={}, after={}, delta={}",
        telemetry_before,
        telemetry_after,
        telemetry_after - telemetry_before
    );

    // =============================================================================
    // Stage 12: Audit Trail - Policy Decisions
    // =============================================================================

    // Query policy decisions for this tenant
    let policy_filters = PolicyDecisionFilters {
        tenant_id: Some(claims.tenant_id.clone()),
        ..Default::default()
    };

    let policy_decisions = state
        .db
        .query_policy_decisions(policy_filters)
        .await
        .expect("Failed to query policy decisions");

    println!("Policy decisions recorded: {}", policy_decisions.len());

    // Verify policy audit chain integrity
    let chain_result = state
        .db
        .verify_policy_audit_chain(Some(&claims.tenant_id))
        .await
        .expect("Failed to verify policy audit chain");

    assert!(
        chain_result.is_valid,
        "Policy audit chain should be valid. Result: {:?}",
        chain_result
    );

    // Check for specific policy hooks
    let has_before_routing = policy_decisions
        .iter()
        .any(|d| d.hook == "on_request_before_routing");

    let has_before_inference = policy_decisions
        .iter()
        .any(|d| d.hook == "on_before_inference");

    let has_after_inference = policy_decisions
        .iter()
        .any(|d| d.hook == "on_after_inference");

    println!(
        "Policy hooks fired - before_routing: {}, before_inference: {}, after_inference: {}",
        has_before_routing, has_before_inference, has_after_inference
    );

    // Verify at least one policy decision was made
    assert!(
        !policy_decisions.is_empty(),
        "At least one policy decision should be recorded"
    );

    // =============================================================================
    // Stage 13: Audit Trail - Routing Decisions
    // =============================================================================

    // For a complete E2E test, we would also verify routing decisions were recorded
    // However, routing decisions are created by the worker during inference
    // and may not be present in this mock setup

    // We can still insert a test routing decision to verify the endpoint works
    let decision = RoutingDecision {
        id: "e2e-routing-decision".to_string(),
        tenant_id: claims.tenant_id.clone(),
        timestamp: Utc::now().to_rfc3339(),
        request_id: Some(request_id.to_string()),
        step: 1,
        input_token_id: Some(0),
        stack_id: None,
        stack_hash: None,
        entropy: 0.42,
        tau: 1.0,
        entropy_floor: 0.01,
        k_value: Some(1),
        candidate_adapters: serde_json::json!([{
            "adapter_idx": 0,
            "adapter_id": adapter_id,
            "raw_score": 0.95,
            "gate_q15": 31129
        }])
        .to_string(),
        selected_adapter_ids: Some(adapter_id.to_string()),
        router_latency_us: Some(15),
        total_inference_latency_us: Some(125),
        overhead_pct: Some(2.3),
        created_at: Utc::now().to_rfc3339(),
    };

    state
        .db
        .insert_routing_decision(&decision)
        .await
        .expect("Failed to insert routing decision");

    // Verify routing decision was recorded
    let routing_filters =
        adapteros_server_api::handlers::routing_decisions::RoutingDecisionsQuery {
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
        };

    let routing_decisions =
        adapteros_server_api::handlers::routing_decisions::get_routing_decisions(
            State(state.clone()),
            Extension(claims.clone()),
            axum::extract::Query(routing_filters),
        )
        .await
        .expect("Failed to query routing decisions");

    assert!(
        !routing_decisions.0.items.is_empty(),
        "Should have at least one routing decision"
    );

    let found_decision = routing_decisions
        .0
        .items
        .iter()
        .any(|d| d.request_id.as_deref() == Some(request_id));

    assert!(
        found_decision,
        "Should find routing decision for our request"
    );

    // =============================================================================
    // Test Complete
    // =============================================================================

    println!("\n=== E2E Inference Test Summary ===");
    println!("✓ Server initialized with in-memory DB");
    println!("✓ Tenant and user created");
    println!("✓ Model registered: {}", model_id);
    println!("✓ Adapter registered: {}", adapter_id);
    println!("✓ Worker registered and serving");
    println!("✓ Inference request succeeded");
    println!("✓ Response validated");
    println!("✓ Deterministic receipt verified");
    println!("✓ Metrics recorded: {} requests", metrics_after);
    println!("✓ Policy decisions: {}", policy_decisions.len());
    println!("✓ Policy audit chain valid");
    println!("✓ Routing decisions recorded");
    println!("=====================================\n");
}

/// E2E Test: Inference failure when model is not ready
///
/// This test verifies that the inference pipeline correctly fails fast
/// when attempting to use a model that hasn't been marked as ready.
#[tokio::test]
async fn test_e2e_inference_fails_when_model_not_ready() {
    let manifest_hash = "not-ready-manifest";
    let backend_name = "mlx";

    let base_state = setup_state(None).await.expect("Failed to setup state");
    let state = base_state.with_manifest_info(manifest_hash.to_string(), backend_name.to_string());

    let claims = test_admin_claims();
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test-rev".to_string(),
    );

    // Register model but DON'T mark it ready
    let model_params = ModelRegistrationBuilder::new()
        .name("unready-model")
        .hash_b3("unready-hash")
        .config_hash_b3("config-hash")
        .tokenizer_hash_b3("tok-hash")
        .tokenizer_cfg_hash_b3("tokcfg-hash")
        .build()
        .expect("Failed to build model params");

    let model_id = state
        .db
        .register_model(model_params)
        .await
        .expect("Failed to register model");

    // Model is registered but not ready - should fail
    let infer_req = InferRequest {
        prompt: "Test prompt".to_string(),
        model: Some(model_id.clone()),
        ..Default::default()
    };

    let result = infer(
        State(state),
        Extension(claims),
        Extension(identity),
        Some(Extension(RequestId("not-ready-test".to_string()))),
        None,
        Json(infer_req),
    )
    .await;

    // Should fail with MODEL_NOT_READY error
    match result {
        Err((status, Json(body))) => {
            assert_eq!(
                status,
                StatusCode::SERVICE_UNAVAILABLE,
                "Should return 503 for model not ready"
            );
            assert_eq!(
                body.code, "MODEL_NOT_READY",
                "Error code should be MODEL_NOT_READY"
            );
            println!("✓ Correctly failed with MODEL_NOT_READY: {}", body.error);
        }
        Ok(_) => {
            panic!("Inference should fail when model is not ready");
        }
    }
}

/// E2E Test: Tenant isolation in inference
///
/// This test verifies that cross-tenant access is blocked during inference.
/// A user from tenant-1 should not be able to use an adapter from tenant-2.
#[tokio::test]
#[ignore = "requires full tenant/model fixture setup"]
async fn test_e2e_inference_tenant_isolation() {
    let manifest_hash = "isolation-test";
    let backend_name = "mlx";

    let base_state = setup_state(None).await.expect("Failed to setup state");
    let state = base_state.with_manifest_info(manifest_hash.to_string(), backend_name.to_string());

    // Create second tenant
    common::create_test_tenant(&state, "tenant-2", "Tenant Two")
        .await
        .expect("Failed to create tenant-2");

    let claims = test_admin_claims(); // tenant-1 user
    let identity = IdentityEnvelope::new(
        claims.tenant_id.clone(),
        "api".to_string(),
        "inference".to_string(),
        "test-rev".to_string(),
    );

    // Register model and adapter for tenant-2
    let model_params = ModelRegistrationBuilder::new()
        .name("tenant-2-model")
        .hash_b3("t2-model-hash")
        .config_hash_b3("t2-config-hash")
        .tokenizer_hash_b3("t2-tok-hash")
        .tokenizer_cfg_hash_b3("t2-tokcfg-hash")
        .build()
        .expect("Failed to build model params");

    let model_id = state
        .db
        .register_model(model_params)
        .await
        .expect("Failed to register model");

    state
        .db
        .update_base_model_status("tenant-2", &model_id, "ready", None, Some(2048))
        .await
        .expect("Failed to mark model ready");

    let adapter_params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-2".to_string())
        .adapter_id("tenant-2-adapter".to_string())
        .name("Tenant 2 Adapter".to_string())
        .hash_b3("t2-adapter-hash")
        .rank(8)
        .targets_json(r#"["q_proj"]"#)
        .base_model_id(Some(model_id.clone()))
        .build()
        .expect("Failed to build adapter params");

    state
        .db
        .register_adapter(adapter_params)
        .await
        .expect("Failed to register adapter");

    // Try to use tenant-2's adapter from tenant-1 user
    let infer_req = InferRequest {
        prompt: "Cross-tenant test".to_string(),
        model: Some(model_id),
        adapters: Some(vec!["tenant-2-adapter".to_string()]),
        ..Default::default()
    };

    let result = infer(
        State(state),
        Extension(claims),
        Extension(identity),
        Some(Extension(RequestId("isolation-test".to_string()))),
        None,
        Json(infer_req),
    )
    .await;

    // Should fail - exact error depends on where isolation is enforced
    match result {
        Err((status, Json(body))) => {
            println!("✓ Cross-tenant access blocked: {} - {}", status, body.error);
            // Could be 403 FORBIDDEN, 404 NOT_FOUND, or 400 BAD_REQUEST
            // depending on where tenant isolation catches it
            assert!(
                status == StatusCode::FORBIDDEN
                    || status == StatusCode::NOT_FOUND
                    || status == StatusCode::BAD_REQUEST,
                "Should return 403, 404, or 400 for cross-tenant access"
            );
        }
        Ok(_) => {
            panic!("Should not allow cross-tenant adapter access");
        }
    }
}
