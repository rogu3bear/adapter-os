//! Conformance tests for lineage/trust/health contract on HTTP training entrypoint.
use adapteros_api_types::training::{
    DatasetVersionSelection, StartTrainingRequest, TrainingConfigRequest,
};
use adapteros_api_types::workers::WorkerCapabilities;
use adapteros_db::adapter_repositories::CreateRepositoryParams;
use adapteros_orchestrator::training::compute_combined_data_spec_hash;
use adapteros_server_api::handlers::training::start_training;
use adapteros_server_api::types::ErrorResponse;
use adapteros_types::training::TRAINING_DATA_CONTRACT_VERSION;
use axum::{extract::State, http::StatusCode, Extension, Json};
use tempfile::TempDir;
use tokio::time::Duration;

mod common;

use common::{register_test_model, register_test_worker, setup_state, test_admin_claims};

const METRIC_LINEAGE_REQUIRED: &str = "training_jobs_rejected_lineage_required";
const METRIC_TRUST_BLOCKED: &str = "training_jobs_rejected_trust_blocked";
const METRIC_TRUST_NEEDS_APPROVAL: &str = "training_jobs_rejected_trust_needs_approval";

async fn create_test_repo(
    state: &adapteros_server_api::state::AppState,
    claims: &adapteros_server_api::auth::Claims,
    base_model_id: &str,
) -> String {
    state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id: &claims.tenant_id,
            name: "test-repo",
            base_model_id: Some(base_model_id),
            default_branch: Some("main"),
            created_by: Some(&claims.sub),
            description: None,
        })
        .await
        .expect("create adapter repository")
}

fn base_config() -> TrainingConfigRequest {
    TrainingConfigRequest {
        rank: 4,
        alpha: 8,
        targets: vec!["q_proj".to_string()],
        training_contract_version: TRAINING_DATA_CONTRACT_VERSION.to_string(),
        pad_token_id: 0,
        ignore_index: 0,
        epochs: 1,
        learning_rate: 0.01,
        batch_size: 1,
        warmup_steps: None,
        max_seq_length: None,
        gradient_accumulation_steps: None,
        validation_split: None,
        preferred_backend: None,
        backend_policy: None,
        coreml_training_fallback: None,
        coreml_placement: None,
        enable_coreml_export: None,
        require_gpu: None,
        max_gpu_memory_mb: None,
        base_model_path: None,
        preprocessing: None,
        force_resume: None,
        multi_module_training: None,
        lora_layer_indices: None,
    }
}

fn base_request(repo_id: String, base_model_id: &str) -> StartTrainingRequest {
    StartTrainingRequest {
        adapter_name: "adapter-test".to_string(),
        config: base_config(),
        template_id: None,
        repo_id: Some(repo_id),
        target_branch: Some("main".to_string()),
        branch_classification: None,
        base_version_id: None,
        code_commit_sha: Some("commit-sha".to_string()),
        data_spec: None,
        data_spec_hash: None,
        hyperparameters: None,
        dataset_id: None,
        dataset_version_ids: None,
        synthetic_mode: false,
        data_lineage_mode: None,
        base_model_id: base_model_id.to_string(),
        collection_id: None,
        lora_tier: None,
        scope: None,
        category: None,
        description: None,
        adapter_type: None,
        language: None,
        symbol_targets: None,
        framework_id: None,
        framework_version: None,
        api_patterns: None,
        repo_scope: None,
        file_patterns: None,
        exclude_patterns: None,
        post_actions: None,
    }
}

async fn seed_base_model(state: &adapteros_server_api::state::AppState) -> (String, TempDir) {
    let temp_dir = tempfile::TempDir::with_prefix("aos-test-").expect("tempdir");
    let model_path = temp_dir.path().join("model.safetensors");
    std::fs::write(&model_path, b"stub").expect("write model stub");
    let model_id = register_test_model(state, &model_path)
        .await
        .expect("register model");
    (model_id, temp_dir)
}

async fn register_training_worker(state: &adapteros_server_api::state::AppState, tenant_id: &str) {
    let caps = WorkerCapabilities {
        backend_kind: "mlx".to_string(),
        implementation: None,
        supports_step: true,
        supports_bulk: false,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: true,
        multi_backend: true,
    };
    register_test_worker(state, tenant_id, caps)
        .await
        .expect("register worker");
}

async fn seed_dataset_version(
    state: &adapteros_server_api::state::AppState,
    dataset_id: &str,
    version_id: &str,
    tenant_id: &str,
    trust_state: &str,
    hash_b3: &str,
) {
    // Create dataset + version
    let _ = state
        .db
        .create_training_dataset_with_id(
            dataset_id,
            "test dataset",
            Some("desc"),
            "jsonl",
            hash_b3,
            "var/test-dataset",
            Some("tester"),
            None,
            Some("ready"),
            Some(hash_b3),
            None,
        )
        .await
        .unwrap();

    let _ = state
        .db
        .create_training_dataset_version_with_id(
            version_id,
            dataset_id,
            Some(tenant_id),
            Some("v1"),
            "var/test-dataset/version",
            hash_b3,
            None,
            None,
            Some("tester"),
        )
        .await
        .unwrap();

    adapteros_db::sqlx::query(
        "UPDATE training_dataset_versions SET trust_state = ?, overall_trust_status = ? WHERE id = ?",
    )
    .bind(trust_state)
    .bind(trust_state)
    .bind(version_id)
    .execute(state.db.pool())
    .await
    .unwrap();
}

async fn extract_error(
    result: Result<
        Json<adapteros_server_api::types::TrainingJobResponse>,
        (StatusCode, Json<ErrorResponse>),
    >,
) -> (StatusCode, ErrorResponse) {
    match result {
        Ok(_) => panic!("expected error"),
        Err((status, Json(err))) => (status, err),
    }
}

#[tokio::test]
async fn synthetic_with_datasets_is_rejected_and_counts_metric() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();
    register_training_worker(&state, &claims.tenant_id).await;
    let (base_model_id, _model_dir) = seed_base_model(&state).await;
    let repo_id = create_test_repo(&state, &claims, &base_model_id).await;

    let mut req = base_request(repo_id, &base_model_id);
    req.synthetic_mode = true;
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: "dsv-synth".to_string(),
        weight: 1.0,
    }]);

    let (status, body) =
        extract_error(start_training(State(state.clone()), Extension(claims), Json(req)).await)
            .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.code, "LINEAGE_REQUIRED");

    // Metric emitted
    tokio::time::sleep(Duration::from_millis(10)).await;
    let count = state
        .metrics_registry
        .get_series_async(METRIC_LINEAGE_REQUIRED)
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn non_synthetic_without_datasets_is_rejected_and_counts_metric() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();
    register_training_worker(&state, &claims.tenant_id).await;
    let (base_model_id, _model_dir) = seed_base_model(&state).await;
    let repo_id = create_test_repo(&state, &claims, &base_model_id).await;

    let req = base_request(repo_id, &base_model_id);
    let (status, body) =
        extract_error(start_training(State(state.clone()), Extension(claims), Json(req)).await)
            .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.code, "LINEAGE_REQUIRED");
    assert!(body.message.contains("dataset_version_ids"));

    tokio::time::sleep(Duration::from_millis(10)).await;
    let count = state
        .metrics_registry
        .get_series_async(METRIC_LINEAGE_REQUIRED)
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn trust_blocked_dataset_rejected_with_metric() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();
    register_training_worker(&state, &claims.tenant_id).await;
    let (base_model_id, _model_dir) = seed_base_model(&state).await;
    let repo_id = create_test_repo(&state, &claims, &base_model_id).await;

    seed_dataset_version(
        &state,
        "ds-blocked",
        "dsv-blocked",
        &claims.tenant_id,
        "blocked",
        "hash-blocked",
    )
    .await;

    let mut req = base_request(repo_id, &base_model_id);
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: "dsv-blocked".to_string(),
        weight: 1.0,
    }]);

    let (status, body) =
        extract_error(start_training(State(state.clone()), Extension(claims), Json(req)).await)
            .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.code, "DATASET_TRUST_BLOCKED");
    assert!(body.message.contains("dsv-blocked"));
    assert!(body.message.contains("blocked"));

    tokio::time::sleep(Duration::from_millis(10)).await;
    let count = state
        .metrics_registry
        .get_series_async(METRIC_TRUST_BLOCKED)
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn trust_unknown_is_rejected_as_needs_approval_and_counts_metric() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();
    register_training_worker(&state, &claims.tenant_id).await;
    let (base_model_id, _model_dir) = seed_base_model(&state).await;
    let repo_id = create_test_repo(&state, &claims, &base_model_id).await;

    seed_dataset_version(
        &state,
        "ds-unknown",
        "dsv-unknown",
        &claims.tenant_id,
        "unknown",
        "hash-unknown",
    )
    .await;

    let mut req = base_request(repo_id, &base_model_id);
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: "dsv-unknown".to_string(),
        weight: 1.0,
    }]);

    let (status, body) =
        extract_error(start_training(State(state.clone()), Extension(claims), Json(req)).await)
            .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.code, "DATASET_TRUST_NEEDS_APPROVAL");
    assert!(body.message.contains("dsv-unknown"));
    assert!(body.message.contains("unknown"));

    tokio::time::sleep(Duration::from_millis(10)).await;
    let count = state
        .metrics_registry
        .get_series_async(METRIC_TRUST_NEEDS_APPROVAL)
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    assert_eq!(count, 1);
}

#[tokio::test]
async fn data_spec_hash_mismatch_rejected() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();
    register_training_worker(&state, &claims.tenant_id).await;
    let (base_model_id, _model_dir) = seed_base_model(&state).await;
    let repo_id = create_test_repo(&state, &claims, &base_model_id).await;

    seed_dataset_version(
        &state,
        "ds-mismatch",
        "dsv-mismatch",
        &claims.tenant_id,
        "allowed",
        "hash-match",
    )
    .await;

    let mut req = base_request(repo_id, &base_model_id);
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: "dsv-mismatch".to_string(),
        weight: 1.0,
    }]);
    req.data_spec_hash = Some("expected-other".to_string());

    let (status, body) =
        extract_error(start_training(State(state.clone()), Extension(claims), Json(req)).await)
            .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body.code, "DATA_SPEC_HASH_MISMATCH");
}

#[tokio::test]
async fn allowed_with_warning_trust_passes_and_preserves_canonical_tokens() {
    let state = setup_state(None).await.unwrap();
    let claims = test_admin_claims();
    register_training_worker(&state, &claims.tenant_id).await;
    let (base_model_id, _model_dir) = seed_base_model(&state).await;
    let repo_id = create_test_repo(&state, &claims, &base_model_id).await;

    seed_dataset_version(
        &state,
        "ds-warn",
        "dsv-warn",
        &claims.tenant_id,
        "warn",
        "hash-warn",
    )
    .await;

    let combined_hash =
        compute_combined_data_spec_hash(&[("dsv-warn".to_string(), "hash-warn".to_string(), 1.0)]);

    let mut req = base_request(repo_id, &base_model_id);
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: "dsv-warn".to_string(),
        weight: 1.0,
    }]);
    req.data_spec_hash = Some(combined_hash);

    let response = start_training(State(state.clone()), Extension(claims), Json(req))
        .await
        .unwrap();

    assert_eq!(response.0.dataset_version_ids.unwrap().len(), 1);
    // No rejection metrics should have been emitted for trust.
    let blocked = state
        .metrics_registry
        .get_series_async(METRIC_TRUST_BLOCKED)
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    let needs_approval = state
        .metrics_registry
        .get_series_async(METRIC_TRUST_NEEDS_APPROVAL)
        .await
        .map(|s| s.get_points(None, None).len())
        .unwrap_or(0);
    assert_eq!(blocked + needs_approval, 0);
}
