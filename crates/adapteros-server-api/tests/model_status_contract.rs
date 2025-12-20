use std::path::Path;

use adapteros_api_types::ModelLoadStatus;
use adapteros_core::{BackendKind, SeedMode};
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::sqlx;
use adapteros_server_api::handlers::models::{load_model, unload_model};
use adapteros_server_api::inference_core::InferenceCore;
use adapteros_server_api::state::AppState;
use adapteros_server_api::types::{InferenceError, InferenceRequestInternal};
use axum::extract::{Path as AxumPath, State};
use axum::Extension;
use axum::Json;
use tempfile::tempdir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;

mod common;

use common::{setup_state, test_admin_claims};

fn render_metrics(state: &AppState) -> String {
    String::from_utf8(state.metrics_exporter.render().unwrap()).unwrap()
}

async fn register_model_with_path(state: &AppState, model_path: &Path) -> anyhow::Result<String> {
    let params = ModelRegistrationBuilder::new()
        .name("test-model")
        .hash_b3("hash")
        .config_hash_b3("config-hash")
        .tokenizer_hash_b3("tok-hash")
        .tokenizer_cfg_hash_b3("tok-cfg-hash")
        .build()?;
    let model_id = state.db.register_model(params).await?;
    state
        .db
        .update_model_path(&model_id, model_path.to_str().unwrap())
        .await?;
    Ok(model_id)
}

async fn insert_worker(state: &AppState, socket_path: &Path) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status) VALUES (?, ?, ?, 'active')",
    )
    .bind("node-1")
    .bind("node-1.local")
    .bind("http://localhost:0")
    .execute(state.db.pool())
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json) VALUES (?, ?, ?, ?)",
    )
    .bind("manifest-ms")
    .bind("tenant-1")
    .bind("manifest-ms")
    .bind("{}")
    .execute(state.db.pool())
    .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO plans (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3, metadata_json) VALUES (?, ?, ?, ?, '[]', ?, NULL)",
    )
    .bind("plan-1")
    .bind("tenant-1")
    .bind("plan-ms-b3")
    .bind("manifest-ms")
    .bind("layout-ms")
    .execute(state.db.pool())
    .await?;
    sqlx::query(
        "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("worker-1")
    .bind("tenant-1")
    .bind("node-1")
    .bind("plan-1")
    .bind(socket_path.to_str().unwrap())
    .bind(1234i32)
    .bind("serving")
    .bind(&now)
    .bind(&now)
    .execute(state.db.pool())
    .await?;
    Ok(())
}

async fn spawn_fake_worker(
    socket_path: &Path,
    body: serde_json::Value,
) -> anyhow::Result<tokio::task::JoinHandle<()>> {
    if socket_path.exists() {
        let _ = tokio::fs::remove_file(socket_path).await;
    }
    if let Some(parent) = socket_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let listener = UnixListener::bind(socket_path)?;
    let payload = serde_json::to_string(&body)?;
    let handle = tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{}",
                payload.len(),
                payload
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });
    Ok(handle)
}

fn minimal_request(tenant_id: &str, model_id: &str) -> InferenceRequestInternal {
    InferenceRequestInternal {
        request_id: "req-1".to_string(),
        cpid: tenant_id.to_string(),
        prompt: "hello".to_string(),
        stream: false,
        batch_item_id: None,
        rag_enabled: false,
        rag_collection_id: None,
        dataset_version_id: None,
        adapter_stack: None,
        adapters: None,
        stack_id: None,
        stack_version: None,
        stack_determinism_mode: None,
        stack_routing_determinism_mode: None,
        domain_hint: None,
        effective_adapter_ids: None,
        determinism_mode: None,
        routing_determinism_mode: None,
        adapter_strength_overrides: None,
        seed_mode: Some(SeedMode::BestEffort),
        request_seed: None,
        backend_profile: Some(BackendKind::Auto),
        coreml_mode: None,
        max_tokens: 16,
        temperature: 0.0,
        top_k: None,
        top_p: None,
        seed: None,
        router_seed: None,
        require_evidence: false,
        stop_policy: None,
        session_id: None,
        pinned_adapter_ids: None,
        chat_context_hash: None,
        model: Some(model_id.to_string()),
        created_at: std::time::Instant::now(),
        worker_auth_token: None,
    }
}

#[tokio::test]
async fn load_handler_idempotent_and_metrics() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims = test_admin_claims();
    let tmp = tempdir()?;
    let model_path = tmp.path().join("model.bin");
    std::fs::write(&model_path, b"bin")?;
    let model_id = register_model_with_path(&state, &model_path).await?;

    let socket_path = tmp.path().join("worker.sock");
    insert_worker(&state, &socket_path).await?;
    let worker_handle = spawn_fake_worker(
        &socket_path,
        serde_json::json!({
            "status": "loaded",
            "model_id": model_id,
            "memory_usage_mb": 1024,
            "error": null,
            "loaded_at": chrono::Utc::now().to_rfc3339(),
        }),
    )
    .await?;

    let axum_state = State(state.clone());
    let Json(first) = load_model(
        axum_state.clone(),
        Extension(claims.clone()),
        AxumPath(model_id.clone()),
    )
    .await
    .expect("initial load should succeed");
    assert_eq!(ModelLoadStatus::Ready, first.status);
    assert!(first.is_loaded);

    let statuses = state.db.list_base_model_statuses().await?;
    let ready_status = statuses
        .iter()
        .find(|s| s.model_id == model_id && s.tenant_id == claims.tenant_id)
        .expect("status record exists");
    assert_eq!("loaded", ready_status.status);

    let Json(second) = load_model(
        axum_state.clone(),
        Extension(claims.clone()),
        AxumPath(model_id.clone()),
    )
    .await
    .expect("second load should be idempotent");
    assert_eq!(ModelLoadStatus::Ready, second.status);

    state
        .db
        .update_base_model_status(
            &claims.tenant_id,
            &model_id,
            ModelLoadStatus::Ready.as_str(),
            None,
            Some(2048),
        )
        .await?;

    let Json(third) = load_model(
        axum_state,
        Extension(claims.clone()),
        AxumPath(model_id.clone()),
    )
    .await
    .expect("third load should reflect ready state");
    assert_eq!(ModelLoadStatus::Ready, third.status);

    let metrics = render_metrics(&state);
    let success_line = format!(
        "adapteros_model_load_success_total{{model_id=\"{}\",tenant_id=\"{}\"}} 1",
        model_id, claims.tenant_id
    );
    let gauge_line = format!(
        "adapteros_model_loaded{{model_id=\"{}\",tenant_id=\"{}\"}} 1",
        model_id, claims.tenant_id
    );
    assert!(metrics.contains(&success_line));
    assert!(metrics.contains(&gauge_line));

    worker_handle.abort();
    Ok(())
}

#[tokio::test]
async fn unload_is_idempotent_and_updates_gauge() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims = test_admin_claims();
    let tmp = tempdir()?;
    let model_path = tmp.path().join("model.bin");
    std::fs::write(&model_path, b"bin")?;
    let model_id = register_model_with_path(&state, &model_path).await?;

    state
        .db
        .update_base_model_status(
            &claims.tenant_id,
            &model_id,
            ModelLoadStatus::Ready.as_str(),
            None,
            Some(1024),
        )
        .await?;

    let axum_state = State(state.clone());
    let Json(first) = unload_model(
        axum_state.clone(),
        Extension(claims.clone()),
        AxumPath(model_id.clone()),
    )
    .await
    .expect("unload should succeed");
    assert_eq!(ModelLoadStatus::NoModel, first.status);
    assert!(!first.is_loaded);

    let statuses = state.db.list_base_model_statuses().await?;
    let current = statuses
        .iter()
        .find(|s| s.model_id == model_id && s.tenant_id == claims.tenant_id)
        .expect("status record exists");
    assert_eq!("unloaded", current.status);

    let Json(second) = unload_model(
        axum_state,
        Extension(claims.clone()),
        AxumPath(model_id.clone()),
    )
    .await
    .expect("second unload should be idempotent");
    assert_eq!(ModelLoadStatus::NoModel, second.status);
    assert!(!second.is_loaded);

    let metrics = render_metrics(&state);
    let gauge_line = format!(
        "adapteros_model_loaded{{model_id=\"{}\",tenant_id=\"{}\"}} 0",
        model_id, claims.tenant_id
    );
    assert!(metrics.contains(&gauge_line));
    Ok(())
}

#[tokio::test]
async fn router_gates_on_model_status() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let model_name = "router-model";
    let model_params = ModelRegistrationBuilder::new()
        .name(model_name)
        .hash_b3("router-hash")
        .config_hash_b3("router-config")
        .tokenizer_hash_b3("router-tok")
        .tokenizer_cfg_hash_b3("router-cfg")
        .build()?;
    let model_id = state.db.register_model(model_params).await?;
    let core = InferenceCore::new(&state);
    let mut request = minimal_request("tenant-1", &model_id);

    for status in [
        ModelLoadStatus::NoModel,
        ModelLoadStatus::Loading,
        ModelLoadStatus::Unloading,
        ModelLoadStatus::Error,
    ] {
        state
            .db
            .update_base_model_status("tenant-1", &model_id, status.as_str(), None, None)
            .await?;
        let err = core
            .route_and_infer(request.clone(), None)
            .await
            .expect_err("non-ready should fail");
        match err {
            InferenceError::ModelNotReady(msg) => {
                assert!(msg.contains(status.as_str()));
            }
            other => panic!("expected ModelNotReady, got {:?}", other),
        }
    }

    state
        .db
        .update_base_model_status(
            "tenant-1",
            &model_id,
            ModelLoadStatus::Ready.as_str(),
            None,
            None,
        )
        .await?;
    request.model = Some(model_id.to_string());
    let ready_result = core.route_and_infer(request, None).await;
    assert!(
        !matches!(ready_result, Err(InferenceError::ModelNotReady(_))),
        "ready status should pass model gate"
    );
    Ok(())
}

#[tokio::test]
async fn model_load_failure_updates_metrics() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims = test_admin_claims();
    let tmp = tempdir()?;
    let model_path = tmp.path().join("model.bin");
    std::fs::write(&model_path, b"bin")?;
    let model_id = register_model_with_path(&state, &model_path).await?;

    let socket_path = tmp.path().join("worker.sock");
    insert_worker(&state, &socket_path).await?;
    let worker_handle = spawn_fake_worker(
        &socket_path,
        serde_json::json!({
            "status": "error",
            "model_id": model_id,
            "memory_usage_mb": null,
            "error": "disk error",
            "loaded_at": null,
        }),
    )
    .await?;

    let Json(response) = load_model(
        State(state.clone()),
        Extension(claims.clone()),
        AxumPath(model_id.clone()),
    )
    .await
    .expect("load failure should still return response payload");
    assert_eq!(ModelLoadStatus::Error, response.status);
    assert!(!response.is_loaded);

    let metrics = render_metrics(&state);
    let failure_line = format!(
        "adapteros_model_load_failure_total{{model_id=\"{}\",tenant_id=\"{}\"}} 1",
        model_id, claims.tenant_id
    );
    let gauge_line = format!(
        "adapteros_model_loaded{{model_id=\"{}\",tenant_id=\"{}\"}} 0",
        model_id, claims.tenant_id
    );
    assert!(metrics.contains(&failure_line));
    assert!(metrics.contains(&gauge_line));

    worker_handle.abort();
    Ok(())
}
