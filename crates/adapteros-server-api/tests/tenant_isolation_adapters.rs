//! Regression tests for PRD-RECT-001: Tenant Isolation — Adapter Lifecycle Queries

use adapteros_core::Result;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::sqlx;
use adapteros_db::workers::WorkerRegistrationParams;
use adapteros_server_api::handlers::{delete_adapter, get_adapter};
use adapteros_server_api::inference_core::InferenceCore;
use adapteros_server_api::types::{InferenceError, InferenceRequestInternal};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Extension;

mod common;
use common::{setup_state, test_admin_claims, test_viewer_claims};

async fn register_test_adapter(
    state: &adapteros_server_api::state::AppState,
    tenant_id: &str,
    adapter_id: &str,
) -> Result<String> {
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(adapter_id)
        .name("Test Adapter")
        .hash_b3("b3:tenant-isolation-test")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()
        .expect("adapter params");

    let id = state.db.register_adapter(params).await?;
    Ok(id)
}

#[tokio::test]
async fn cross_tenant_get_adapter_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    register_test_adapter(&state, "tenant-1", "tenant-1-adapter").await?;

    let claims_other_tenant = test_viewer_claims(); // tenant_id = "default"
    let result = get_adapter(
        State(state),
        Extension(claims_other_tenant),
        Path("tenant-1-adapter".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant get_adapter should not succeed"),
    }

    Ok(())
}

#[tokio::test]
async fn cross_tenant_delete_adapter_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    register_test_adapter(&state, "tenant-1", "tenant-1-adapter").await?;

    let mut claims_other_tenant = test_admin_claims();
    claims_other_tenant.tenant_id = "default".to_string();

    let result = delete_adapter(
        State(state),
        Extension(claims_other_tenant),
        Path("tenant-1-adapter".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant delete_adapter should not succeed"),
    }

    Ok(())
}

#[tokio::test]
async fn pinned_adapter_cross_tenant_is_indistinguishable_from_not_found() -> Result<()> {
    let state = setup_state(None)
        .await
        .expect("state")
        .with_manifest_info("test-manifest-hash".to_string(), "mlx".to_string());
    let foreign_adapter_id = register_test_adapter(&state, "default", "default-adapter").await?;

    // Satisfy base-model gating and worker selection so we can reach pinned validation.
    sqlx::query(
        "INSERT OR IGNORE INTO models
         (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("base-model")
    .bind("Base Model")
    .bind("hash-base-model")
    .bind("hash-config")
    .bind("hash-tokenizer")
    .bind("hash-tokenizer-cfg")
    .execute(state.db.pool())
    .await?;

    state
        .db
        .update_base_model_status("tenant-1", "base-model", "ready", None, None)
        .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO nodes (id, hostname, agent_endpoint, status)
         VALUES (?, ?, ?, 'active')",
    )
    .bind("test-node-1")
    .bind("test-node-1")
    .bind("http://localhost")
    .execute(state.db.pool())
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO manifests (id, tenant_id, hash_b3, body_json)
         VALUES (?, ?, ?, ?)",
    )
    .bind("test-manifest-1")
    .bind("tenant-1")
    .bind("test-manifest-hash")
    .bind("{}")
    .execute(state.db.pool())
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO plans
         (id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, layout_hash_b3)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("test-plan-1")
    .bind("tenant-1")
    .bind("plan-b3:test-plan-1")
    .bind("test-manifest-hash")
    .bind("[]")
    .bind("layout-b3:test-plan-1")
    .execute(state.db.pool())
    .await?;

    let worker_id = "test-worker-1".to_string();
    state
        .db
        .register_worker(WorkerRegistrationParams {
            worker_id: worker_id.clone(),
            tenant_id: "tenant-1".to_string(),
            node_id: "test-node-1".to_string(),
            plan_id: "test-plan-1".to_string(),
            uds_path: "var/run/test-worker.sock".to_string(),
            pid: 1234,
            manifest_hash: "test-manifest-hash".to_string(),
            backend: Some("mlx".to_string()),
            model_hash_b3: None,
            capabilities_json: None,
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            api_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        })
        .await?;
    state
        .db
        .transition_worker_status(&worker_id, "healthy", "test", None)
        .await?;

    let mut request = InferenceRequestInternal::new("tenant-1".to_string(), "prompt".to_string());
    request.session_id = Some("session-1".to_string());
    request.pinned_adapter_ids = Some(vec![foreign_adapter_id.clone()]);

    let core = InferenceCore::new(&state);
    let err = core.route_and_infer(request, None).await.unwrap_err();

    match err {
        InferenceError::AdapterNotFound(msg) => {
            assert!(msg.contains(&foreign_adapter_id));
            assert!(!msg.contains("tenant-1"));
        }
        other => panic!("expected AdapterNotFound, got {:?}", other),
    }

    Ok(())
}

/// Regression test for handlers.rs tenant isolation fix.
/// Verifies that get_adapter handler returns 404 (not 403) for cross-tenant access,
/// ensuring the tenant-scoped query pattern is indistinguishable from adapter-not-found.
#[tokio::test]
async fn handlers_rs_get_adapter_cross_tenant_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    // Register adapter in tenant-1
    register_test_adapter(&state, "tenant-1", "handlers-rs-isolation-test").await?;

    // Attempt cross-tenant access from "default" tenant
    let claims_other_tenant = test_viewer_claims(); // tenant_id = "default"
    let result = get_adapter(
        State(state),
        Extension(claims_other_tenant),
        Path("handlers-rs-isolation-test".to_string()),
    )
    .await;

    // Must return 404 NOT_FOUND (not 403 FORBIDDEN) to prevent adapter enumeration
    match result {
        Err((status, body)) => {
            assert_eq!(
                status,
                StatusCode::NOT_FOUND,
                "cross-tenant access must return 404, not 403"
            );
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant get_adapter should return 404, not succeed"),
    }

    Ok(())
}
