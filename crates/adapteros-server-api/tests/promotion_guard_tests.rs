use adapteros_api_types::adapters::PromoteVersionRequest;
use adapteros_db::adapter_repositories::{CreateRepositoryParams, CreateVersionParams};
use adapteros_server_api::handlers::promote_adapter_version_handler;
use adapteros_server_api::state::AppState;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::Extension;
use axum::Json;

mod common;
use common::setup_state;
use common::test_admin_claims;

#[tokio::test]
#[ignore = "requires full promotion flow setup"]
async fn promote_rejects_not_serveable_version() {
    let state: AppState = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let repo_id = state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id: &claims.tenant_id,
            name: "repo-guard",
            base_model_id: Some("base-model"),
            default_branch: Some("main"),
            created_by: Some("tester"),
            description: Some("serveable guard"),
        })
        .await
        .expect("create repo");

    let version_id = state
        .db
        .create_adapter_version(CreateVersionParams {
            repo_id: &repo_id,
            tenant_id: &claims.tenant_id,
            version: "1.0.0",
            branch: "main",
            branch_classification: "protected",
            aos_path: None,
            aos_hash: None,
            manifest_schema_version: None,
            parent_version_id: None,
            code_commit_sha: None,
            data_spec_hash: None,
            training_backend: None,
            coreml_used: None,
            coreml_device_type: None,
            dataset_version_ids: None,
            release_state: "ready",
            metrics_snapshot_id: None,
            evaluation_summary: None,
            allow_archived: false,
            actor: Some("tester"),
            reason: None,
            train_job_id: None,
        })
        .await
        .expect("create version");

    // adapter_trust_state defaults to unknown → not serveable
    let result = promote_adapter_version_handler(
        axum::extract::State(state.clone()),
        Extension(claims),
        Path(version_id.clone()),
        Json(PromoteVersionRequest {
            repo_id: repo_id.clone(),
        }),
    )
    .await;

    match result {
        Err((status, Json(err))) => {
            assert_eq!(status, StatusCode::BAD_REQUEST);
            assert_eq!(err.code, "NOT_SERVEABLE");
        }
        Ok(_) => panic!("Promotion should fail for non-serveable version"),
    }
}
