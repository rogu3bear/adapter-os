//! Canonical `/v1/adapters/from-dataset/{dataset_id}` capability behavior.

use adapteros_api_types::workers::WorkerCapabilities;
use adapteros_db::training_datasets::{CreateDatasetParams, CreateTrainingDatasetRowParams};
use adapteros_server_api::handlers::training_datasets::{
    create_adapter_from_dataset, CreateAdapterFromDatasetRequest,
};
use adapteros_server_api::state::AppState;
use anyhow::Result;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

mod common;
use common::{register_test_model, register_test_worker, setup_state, test_admin_claims};

async fn create_ready_dataset_with_row(
    state: &AppState,
    tenant_id: &str,
) -> Result<(String, String)> {
    let dataset_seed = format!("from-dataset-{}", uuid::Uuid::new_v4().simple());
    let hash = adapteros_core::B3Hash::hash(dataset_seed.as_bytes()).to_hex();
    let storage_path = format!("var/test-datasets/{dataset_seed}");

    let params = CreateDatasetParams::builder()
        .name(&dataset_seed)
        .format("jsonl")
        .hash_b3(&hash)
        .storage_path(&storage_path)
        .status("ready")
        .dataset_type("training")
        .purpose("training")
        .collection_method("manual")
        .ownership("tenant")
        .category("codebase")
        .tenant_id(tenant_id)
        .created_by("tenant-1-user")
        .build()
        .expect("dataset params");

    let (dataset_id, dataset_version_id) = state
        .db
        .create_training_dataset_from_params_with_version(
            &params,
            Some("v1"),
            &storage_path,
            &hash,
            None,
            None,
        )
        .await
        .expect("dataset + version");

    state
        .db
        .update_dataset_validation(&dataset_id, "valid", None, None)
        .await
        .expect("dataset validation");

    state
        .db
        .update_dataset_version_safety_status(
            &dataset_version_id,
            Some("clean"),
            Some("clean"),
            Some("clean"),
            Some("clean"),
        )
        .await
        .expect("dataset safety");

    state
        .db
        .update_dataset_version_structural_validation(&dataset_version_id, "valid", None)
        .await
        .expect("dataset structural validation");

    let row = CreateTrainingDatasetRowParams::builder(
        &dataset_id,
        "What is AdapterOS?",
        "AdapterOS is a multi-LoRA platform.",
    )
    .dataset_version_id(dataset_version_id.clone())
    .tenant_id(tenant_id)
    .created_by("tenant-1-user")
    .build();

    state
        .db
        .insert_training_dataset_row(&row)
        .await
        .expect("dataset row");

    Ok((dataset_id, dataset_version_id))
}

async fn create_base_model(state: &AppState, tenant_id: &str) -> Result<String> {
    let model_path = std::env::temp_dir().join(format!(
        "aos-from-dataset-{}.bin",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&model_path, b"weights")?;
    let model_id = register_test_model(state, &model_path).await?;
    adapteros_db::sqlx::query("UPDATE models SET tenant_id = ? WHERE id = ?")
        .bind(tenant_id)
        .bind(&model_id)
        .execute(state.db.pool_result()?)
        .await?;
    Ok(model_id)
}

#[tokio::test]
async fn from_dataset_rejects_when_gpu_capability_missing() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();
    let tenant_id = claims.tenant_id.clone();

    let (dataset_id, dataset_version_id) =
        create_ready_dataset_with_row(&state, &tenant_id).await?;
    let base_model_id = create_base_model(&state, &tenant_id).await?;

    let request = CreateAdapterFromDatasetRequest {
        base_model_id,
        workspace_id: None,
        dataset_version_id: Some(dataset_version_id),
        adapter_name: Some("from-dataset-missing-gpu".to_string()),
        training_config: None,
        post_actions: None,
    };

    let err = create_adapter_from_dataset(
        State(state.clone()),
        Extension(claims),
        Path(dataset_id),
        Json(request),
    )
    .await
    .expect_err("request should fail without gpu-capable workers");

    assert_eq!(err.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(err.code.as_ref(), "WORKER_CAPABILITY_MISSING");

    Ok(())
}

#[tokio::test]
async fn from_dataset_accepts_when_gpu_capable_worker_exists() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();
    let tenant_id = claims.tenant_id.clone();

    let (dataset_id, dataset_version_id) =
        create_ready_dataset_with_row(&state, &tenant_id).await?;
    let base_model_id = create_base_model(&state, &tenant_id).await?;

    let caps = WorkerCapabilities {
        backend_kind: "mlx".to_string(),
        implementation: None,
        supports_step: true,
        supports_bulk: true,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: true,
        multi_backend: true,
    };
    register_test_worker(&state, &tenant_id, caps).await?;

    let request = CreateAdapterFromDatasetRequest {
        base_model_id: base_model_id.clone(),
        workspace_id: None,
        dataset_version_id: Some(dataset_version_id),
        adapter_name: Some("from-dataset-gpu-ok".to_string()),
        training_config: None,
        post_actions: None,
    };

    let (status, Json(response)) = create_adapter_from_dataset(
        State(state.clone()),
        Extension(claims),
        Path(dataset_id.clone()),
        Json(request),
    )
    .await
    .expect("request should be accepted with gpu-capable worker");

    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(!response.id.is_empty());
    assert_eq!(response.dataset_id.as_deref(), Some(dataset_id.as_str()));

    Ok(())
}
