//! Integration tests for training guardrails (fail-fast validation).

use std::path::{Path, PathBuf};

use adapteros_api_types::training::{
    DatasetVersionSelection, StartTrainingRequest, TrainingConfigRequest,
};
use adapteros_core::B3Hash;
use adapteros_db::adapter_repositories::CreateRepositoryParams;
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::training_datasets::CreateDatasetParams;
use adapteros_server_api::handlers::training::start_training;
use adapteros_server_api::state::AppState;
use adapteros_types::training::BranchClassification;
use axum::extract::State;
use axum::Extension;
use axum::Json;

mod common;
use common::{register_test_worker, test_admin_claims};

async fn create_model(state: &AppState, model_id: &str, model_path: &Path) -> String {
    let params = ModelRegistrationBuilder::new()
        .name(model_id.to_string())
        .hash_b3(B3Hash::hash(model_id.as_bytes()).to_hex())
        .config_hash_b3("cfg-hash")
        .tokenizer_hash_b3("tok-hash")
        .tokenizer_cfg_hash_b3("tok-cfg-hash")
        .build()
        .expect("model params");
    let id = state
        .db
        .register_model(params)
        .await
        .expect("register model");
    state
        .db
        .update_model_path(&id, model_path.to_str().unwrap_or_default())
        .await
        .expect("set model path");
    id
}

#[tokio::test]
async fn start_training_rejects_base_model_mismatch() {
    let state = common::setup_state(None).await.expect("state");
    let tmp = tempfile::TempDir::with_prefix("aos-test-").expect("create temp dir");
    let model_path = tmp.path().join("model.bin");
    std::fs::write(&model_path, b"weights").expect("write dummy model");

    // Register a model whose id will be sent in the request
    let wrong_model_id = create_model(&state, "wrong-model", &model_path).await;

    // Register a worker so worker capability checks pass
    let caps = adapteros_api_types::workers::WorkerCapabilities {
        backend_kind: "mlx".to_string(),
        implementation: None,
        supports_step: true,
        supports_bulk: true,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: true,
        multi_backend: false,
    };
    register_test_worker(&state, "tenant-1", caps)
        .await
        .expect("worker");

    // Create repository with a different base_model_id to force mismatch
    let repo_id = state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id: "tenant-1",
            name: "repo",
            base_model_id: Some("expected-model"),
            default_branch: Some("main"),
            created_by: Some("tester"),
            description: None,
        })
        .await
        .expect("repo");

    let cfg = adapteros_types::training::TrainingConfig::quick_training();
    let req = StartTrainingRequest {
        adapter_name: "guardrail-adapter".to_string(),
        config: TrainingConfigRequest {
            rank: cfg.rank,
            alpha: cfg.alpha,
            targets: cfg.targets,
            training_contract_version: cfg.training_contract_version,
            pad_token_id: cfg.pad_token_id,
            ignore_index: cfg.ignore_index,
            epochs: cfg.epochs,
            learning_rate: cfg.learning_rate,
            batch_size: cfg.batch_size,
            warmup_steps: cfg.warmup_steps,
            max_seq_length: cfg.max_seq_length,
            gradient_accumulation_steps: cfg.gradient_accumulation_steps,
            validation_split: cfg.validation_split,
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
        },
        template_id: None,
        repo_id: Some(repo_id.clone()),
        target_branch: None,
        branch_classification: Some(BranchClassification::Protected),
        base_version_id: None,
        code_commit_sha: None,
        data_spec: None,
        data_spec_hash: None,
        hyperparameters: None,
        dataset_id: None,
        dataset_version_ids: None,
        synthetic_mode: true,
        data_lineage_mode: None,
        base_model_id: wrong_model_id,
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
    };

    let claims = test_admin_claims();
    let result = start_training(State(state.clone()), Extension(claims), Json(req)).await;
    let err = result.expect_err("should reject base model mismatch");
    assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    let body = err.1 .0;
    assert_eq!(body.code, "BASE_MODEL_MISMATCH");
    assert!(body.message.to_ascii_lowercase().contains("base_model"));
}

#[tokio::test]
async fn start_training_rejects_empty_dataset() {
    let state = common::setup_state(None).await.expect("state");
    let tmp = tempfile::TempDir::with_prefix("aos-test-").expect("create temp dir");
    let model_path = tmp.path().join("model.bin");
    std::fs::write(&model_path, b"weights").expect("write dummy model");

    let empty_model_id = create_model(&state, "empty-model", &model_path).await;

    let caps = adapteros_api_types::workers::WorkerCapabilities {
        backend_kind: "mlx".to_string(),
        implementation: None,
        supports_step: true,
        supports_bulk: true,
        supports_logits: true,
        supports_streaming: true,
        gpu_backward: true,
        multi_backend: false,
    };
    register_test_worker(&state, "tenant-1", caps)
        .await
        .expect("worker");

    let repo_id = state
        .db
        .create_adapter_repository(CreateRepositoryParams {
            tenant_id: "tenant-1",
            name: "repo-empty",
            base_model_id: Some(empty_model_id.as_str()),
            default_branch: Some("main"),
            created_by: Some("tester"),
            description: None,
        })
        .await
        .expect("repo");

    let hash = "a".repeat(64);
    let params = CreateDatasetParams::builder()
        .name("empty-dataset")
        .format("jsonl")
        .hash_b3(&hash)
        .storage_path("var/test-datasets/empty")
        .status("ready")
        .tenant_id("tenant-1")
        .created_by("tester")
        .build()
        .expect("dataset params");
    let (dataset_id, dataset_version_id) = state
        .db
        .create_training_dataset_from_params_with_version(
            &params,
            None,
            "var/test-datasets/empty",
            &hash,
            None,
            None,
        )
        .await
        .expect("dataset");
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
        .expect("dataset validation version");

    let cfg = adapteros_types::training::TrainingConfig::quick_training();
    let req = StartTrainingRequest {
        adapter_name: "guardrail-empty-dataset".to_string(),
        config: TrainingConfigRequest {
            rank: cfg.rank,
            alpha: cfg.alpha,
            targets: cfg.targets,
            training_contract_version: cfg.training_contract_version,
            pad_token_id: cfg.pad_token_id,
            ignore_index: cfg.ignore_index,
            epochs: cfg.epochs,
            learning_rate: cfg.learning_rate,
            batch_size: cfg.batch_size,
            warmup_steps: cfg.warmup_steps,
            max_seq_length: cfg.max_seq_length,
            gradient_accumulation_steps: cfg.gradient_accumulation_steps,
            validation_split: cfg.validation_split,
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
        },
        template_id: None,
        repo_id: Some(repo_id),
        target_branch: None,
        branch_classification: Some(BranchClassification::Protected),
        base_version_id: None,
        code_commit_sha: Some("commit-sha".to_string()),
        data_spec: None,
        data_spec_hash: None,
        hyperparameters: None,
        dataset_id: None,
        dataset_version_ids: Some(vec![DatasetVersionSelection {
            dataset_version_id: dataset_version_id.clone(),
            weight: 1.0,
        }]),
        synthetic_mode: false,
        data_lineage_mode: None,
        base_model_id: empty_model_id,
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
    };

    let claims = test_admin_claims();
    let result = start_training(State(state.clone()), Extension(claims), Json(req)).await;
    let err = result.expect_err("should reject empty dataset");
    assert_eq!(err.0, axum::http::StatusCode::BAD_REQUEST);
    let body = err.1 .0;
    assert_eq!(body.code, "DATASET_EMPTY");
    assert!(body.message.to_ascii_lowercase().contains("dataset"));
}
