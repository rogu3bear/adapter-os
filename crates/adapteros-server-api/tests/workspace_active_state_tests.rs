use adapteros_api_types::ModelLoadStatus;
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_server_api::handlers::models::unload_model;
use adapteros_server_api::handlers::workspaces::{
    get_workspace_active_state, reconcile_active_models, set_workspace_active_state,
    WorkspaceActiveStateRequest,
};
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::state::AppState;
use axum::extract::State;
use axum::{Extension, Json};

mod common;
use common::{setup_state, test_admin_claims};

async fn register_test_model(state: &AppState, name: &str) -> String {
    let params = ModelRegistrationBuilder::new()
        .name(name)
        .hash_b3(format!("{name}-hash"))
        .config_hash_b3(format!("{name}-config"))
        .tokenizer_hash_b3(format!("{name}-tok"))
        .tokenizer_cfg_hash_b3(format!("{name}-tokcfg"))
        .build()
        .expect("builder produces params");

    let model_id = state
        .db
        .register_model(params)
        .await
        .expect("model registration");

    let claims = test_admin_claims();
    adapteros_db::sqlx::query("UPDATE models SET tenant_id = ? WHERE id = ?")
        .bind(&claims.tenant_id)
        .bind(&model_id)
        .execute(state.db.pool_result().expect("db pool"))
        .await
        .expect("set model tenant");

    model_id
}

#[tokio::test]
async fn active_state_round_trip() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();
    let model_id = register_test_model(&state, "round-trip-model").await;

    state
        .db
        .update_base_model_status(
            &claims.tenant_id,
            &model_id,
            ModelLoadStatus::Ready.as_str(),
            None,
            Some(256),
        )
        .await
        .expect("status update");

    let Json(set_response) = set_workspace_active_state(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(claims.tenant_id.clone()),
        Json(WorkspaceActiveStateRequest {
            active_base_model_id: Some(model_id.clone()),
            active_plan_id: None,
            active_adapter_ids: Vec::new(),
            manifest_hash_b3: Some("manifest-hash".to_string()),
        }),
    )
    .await
    .expect("set active state");

    assert_eq!(
        set_response.active_base_model_id.as_deref(),
        Some(model_id.as_str())
    );
    assert_eq!(
        set_response.manifest_hash_b3.as_deref(),
        Some("manifest-hash")
    );
    assert_eq!(set_response.model_loaded, Some(true));
    assert!(!set_response.model_mismatch);

    let Json(get_response) = get_workspace_active_state(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(claims.tenant_id.clone()),
    )
    .await
    .expect("fetch active state");

    assert_eq!(
        get_response.active_base_model_id.as_deref(),
        Some(model_id.as_str())
    );
    assert_eq!(get_response.active_adapter_ids.len(), 0);
    assert_eq!(get_response.model_loaded, Some(true));
    assert!(!get_response.model_mismatch);
}

#[tokio::test]
async fn unload_clears_active_state() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();
    let model_id = register_test_model(&state, "to-unload").await;

    state
        .db
        .update_base_model_status(
            &claims.tenant_id,
            &model_id,
            ModelLoadStatus::Ready.as_str(),
            None,
            Some(128),
        )
        .await
        .expect("status update");

    state
        .db
        .upsert_workspace_active_state(&claims.tenant_id, Some(&model_id), None, None, None)
        .await
        .expect("active state upsert");

    let _ = unload_model(
        State(state.clone()),
        Extension(claims.clone()),
        Extension(ClientIp("127.0.0.1".to_string())),
        axum::extract::Path(model_id.clone()),
    )
    .await
    .expect("unload model");

    let active = state
        .db
        .get_workspace_active_state(&claims.tenant_id)
        .await
        .expect("lookup active state");
    assert!(active.is_some());
    assert!(active.unwrap().active_base_model_id.as_deref().is_none());
}

#[tokio::test]
async fn reconcile_marks_mismatch_when_worker_empty() {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();
    let model_id = register_test_model(&state, "needs-reconcile").await;

    state
        .db
        .update_base_model_status(
            &claims.tenant_id,
            &model_id,
            ModelLoadStatus::Ready.as_str(),
            None,
            Some(64),
        )
        .await
        .expect("status update");

    state
        .db
        .upsert_workspace_active_state(&claims.tenant_id, Some(&model_id), None, None, None)
        .await
        .expect("active state upsert");

    // No workers are registered in the test harness, so reconciliation should
    // treat the active model as a mismatch and downgrade the status.
    reconcile_active_models(&state).await;

    let Json(response) = get_workspace_active_state(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(claims.tenant_id.clone()),
    )
    .await
    .expect("fetch reconciled state");

    assert_eq!(
        response.active_base_model_id.as_deref(),
        Some(model_id.as_str())
    );
    assert_eq!(response.model_loaded, Some(false));
    assert!(response.model_mismatch);
}
