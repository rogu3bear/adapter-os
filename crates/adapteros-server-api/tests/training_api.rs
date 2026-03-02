use adapteros_core::B3Hash;
use adapteros_db::adapter_repositories::CreateRepositoryParams;
use adapteros_db::AdapterRegistrationBuilder;
use adapteros_server_api::handlers::adapters::{activate_adapter, AdapterActivateRequest};
use adapteros_server_api::handlers::workspaces::WorkspaceActiveStateResponse;
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::state::AppState;
use axum::extract::Path;
use axum::extract::State;
use axum::Extension;

mod common;
use common::{test_admin_claims, TestkitEnvGuard};

async fn seed_dataset_version(
    state: &AppState,
    dataset_id: &str,
    version_id: &str,
    tenant_id: &str,
    hash: &str,
) -> anyhow::Result<()> {
    adapteros_db::sqlx::query(
        "INSERT INTO training_datasets (id, name, hash_b3, validation_status, format, storage_path, tenant_id)
         VALUES (?, ?, ?, 'valid', 'jsonl', 'var/test-datasets', ?)",
    )
    .bind(dataset_id)
    .bind(dataset_id)
    .bind(hash)
    .bind(tenant_id)
    .execute(state.db.pool_result()?)
    .await?;

    state
        .db
        .create_training_dataset_version_with_id(
            version_id,
            dataset_id,
            Some(tenant_id),
            Some("v1"),
            "var/test-datasets",
            hash,
            None,
            None,
            Some("tester"),
        )
        .await?;

    state
        .db
        .update_dataset_version_structural_validation(version_id, "valid", None)
        .await?;
    state
        .db
        .update_dataset_version_safety_status(
            version_id,
            Some("clean"),
            Some("clean"),
            Some("clean"),
            Some("clean"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn training_job_completion_and_activation_updates_workspace_state() -> anyhow::Result<()> {
    let _guard = TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await?;
    let claims = test_admin_claims();
    let workspace_id = claims.tenant_id.clone();

    // Seed dataset + version with trusted/valid status
    let dataset_id = "ds-activate";
    let version_id = "ds-activate-v1";
    let dataset_hash = B3Hash::hash(version_id.as_bytes()).to_hex();
    seed_dataset_version(
        &state,
        dataset_id,
        version_id,
        &claims.tenant_id,
        &dataset_hash,
    )
    .await?;

    // Minimal repo + training job
    let repo_id = state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id: &claims.tenant_id,
            name: "repo-activate",
            base_model_id: None,
            default_branch: Some("main"),
            created_by: Some(&claims.sub),
            description: None,
        })
        .await?;

    // Avoid actual training by stubbing job insert and artifact persistence
    let training_config_json = serde_json::json!({
        "rank": 4,
        "alpha": 8,
        "targets": ["q_proj"],
        "epochs": 1,
        "learning_rate": 0.001,
        "batch_size": 1,
        "warmup_steps": null,
        "max_seq_length": null,
        "gradient_accumulation_steps": null
    })
    .to_string();
    let data_spec_json =
        serde_json::json!({ "dataset_version_ids": [ { "dataset_version_id": version_id, "weight": 1.0 }] })
            .to_string();
    let training_job_id = state
        .db
        .create_training_job_with_provenance(
            Some("job-activate"),
            &repo_id,
            &training_config_json,
            &claims.sub,
            Some(dataset_id),
            None,
            Some(version_id),
            None,
            Some("base-model-1"),
            None,
            Some(&claims.tenant_id),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&data_spec_json),
            false,
            Some("dataset_only"),
        )
        .await?;

    // Register adapter and link to training job
    let adapter_hash = B3Hash::hash(b"adapter-activate").to_hex();
    let adapter_id = "adapter-activate";
    let adapter_params = AdapterRegistrationBuilder::new()
        .tenant_id(&claims.tenant_id)
        .adapter_id(adapter_id)
        .name("adapter-activate")
        .hash_b3(&adapter_hash)
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("global")
        .base_model_id(Some("base-model-1"))
        .aos_file_path(Some("var/adapters/adapter-activate.aos"))
        .aos_file_hash(Some(adapter_hash.clone()))
        .content_hash_b3(Some(adapter_hash.clone()))
        .build()
        .expect("adapter params");
    state.db.register_adapter(adapter_params).await?;
    state
        .db
        .update_adapter_training_job_id(adapter_id, &training_job_id)
        .await?;

    // Simulate completed training job with determinism metadata
    state
        .db
        .update_training_status(&training_job_id, "completed")
        .await?;
    state
        .db
        .update_training_job_adapter_name(&training_job_id, "ws-det-adapter")
        .await?;

    let metadata = serde_json::json!({
        "dataset_hash_b3": dataset_hash,
        "manifest_hash_b3": adapter_hash,
        "adapter_hash_b3": adapter_hash,
        "seed_inputs": {
            "dataset_version_ids": [version_id],
            "base_model_id": "base-model-1"
        }
    });
    state
        .db
        .update_training_job_artifact(
            &training_job_id,
            "var/artifacts/test.aos",
            adapter_id,
            &adapter_hash,
            Some(metadata),
        )
        .await?;

    // Activate adapter and verify workspace active state was updated
    let response = activate_adapter(
        State(state.clone()),
        Extension(claims.clone()),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path(adapter_id.to_string()),
        axum::Json(AdapterActivateRequest {
            workspace_id: Some(workspace_id.clone()),
        }),
    )
    .await
    .expect("activation succeeds");

    let active_state: WorkspaceActiveStateResponse = response.0;
    assert_eq!(active_state.workspace_id, workspace_id);
    assert!(active_state
        .active_adapter_ids
        .contains(&adapter_id.to_string()));
    assert_eq!(
        active_state.manifest_hash_b3.as_deref(),
        Some(adapter_hash.as_str())
    );

    // Training job metadata contains determinism hashes
    let stored_job = state
        .db
        .get_training_job(&training_job_id)
        .await?
        .expect("job exists");
    let metadata_json = stored_job.metadata_json.expect("metadata_json");
    assert!(
        metadata_json.contains("dataset_hash_b3") && metadata_json.contains("manifest_hash_b3")
    );

    Ok(())
}
