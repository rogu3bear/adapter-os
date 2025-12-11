//! Training control integration tests
use std::sync::Arc;
use std::time::Duration;

use adapteros_api_types::{
    DatasetVersionSelection, StartTrainingRequest, TrainingConfigRequest, TrainingListParams,
};
use adapteros_core::B3Hash;
use adapteros_orchestrator::TrainingJobStatus;
use adapteros_server_api::handlers::get_training_logs;
use adapteros_server_api::handlers::training::{
    cancel_training, get_training_job, list_training_jobs, start_training,
};
use adapteros_server_api::state::AppState;
use adapteros_types::training::{BranchClassification, TrainingConfig};
use axum::http::StatusCode;
use axum::{extract::State, Extension, Json};
use tempfile::TempDir;
use tokio::time::sleep;

mod common;
use common::{create_test_dataset, test_admin_claims};

fn make_request(name: &str) -> StartTrainingRequest {
    let cfg = TrainingConfig::quick_training();
    StartTrainingRequest {
        adapter_name: name.to_string(),
        config: TrainingConfigRequest {
            rank: cfg.rank,
            alpha: cfg.alpha,
            targets: cfg.targets,
            coreml_training_fallback: None,
            coreml_placement: None,
            epochs: cfg.epochs,
            learning_rate: cfg.learning_rate,
            batch_size: cfg.batch_size,
            warmup_steps: cfg.warmup_steps,
            max_seq_length: cfg.max_seq_length,
            gradient_accumulation_steps: cfg.gradient_accumulation_steps,
            preferred_backend: None,
            backend_policy: None,
            enable_coreml_export: None,
            require_gpu: None,
            max_gpu_memory_mb: None,
        },
        template_id: None,
        repo_id: None,
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
        base_model_id: None,
        collection_id: None,
        lora_tier: None,
        scope: None,
        category: None,
        description: None,
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

async fn setup_training_state() -> (AppState, TempDir) {
    std::env::set_var("AOS_ALLOW_NONDET_TRAINING", "1");
    let mut state = common::setup_state(None).await.expect("state");
    let temp_dir = tempfile::tempdir().expect("tempdir");

    if let Some(service) = Arc::get_mut(&mut state.training_service) {
        service.set_db(state.db.clone());
        service.set_storage_root(temp_dir.path().to_path_buf());
    } else {
        state.training_service = Arc::new(adapteros_orchestrator::TrainingService::with_db(
            state.db.clone(),
            temp_dir.path().to_path_buf(),
        ));
    }

    (state, temp_dir)
}

async fn wait_for_terminal(state: &AppState, job_id: &str) -> TrainingJobStatus {
    for _ in 0..120 {
        let job = state.training_service.get_job(job_id).await.unwrap();
        match job.status {
            TrainingJobStatus::Completed
            | TrainingJobStatus::Cancelled
            | TrainingJobStatus::Failed => return job.status,
            _ => sleep(Duration::from_millis(50)).await,
        }
    }
    state.training_service.get_job(job_id).await.unwrap().status
}

#[tokio::test]
async fn test_training_start() {
    let (state, _temp_dir) = setup_training_state().await;
    let claims = test_admin_claims();

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims),
        Json(make_request("adapter-start")),
    )
    .await
    .expect("start training");

    assert!(!job.id.is_empty(), "job id should be returned");
    assert_eq!(job.adapter_name, "adapter-start");
}

#[tokio::test]
async fn test_training_rejects_missing_dataset_versions_when_non_synthetic() {
    let (state, _temp_dir) = setup_training_state().await;
    let claims = test_admin_claims();

    let mut req = make_request("adapter-no-dataset");
    req.synthetic_mode = false;

    let result = start_training(State(state.clone()), Extension(claims), Json(req)).await;
    let (status, _body) = result.expect_err("missing dataset_version_ids should be rejected");
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_training_status_completes() {
    let (state, _temp_dir) = setup_training_state().await;
    let claims = test_admin_claims();

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims),
        Json(make_request("adapter-status")),
    )
    .await
    .expect("start training");

    let status = wait_for_terminal(&state, &job.id).await;
    assert_eq!(status, TrainingJobStatus::Completed);
}

#[tokio::test]
async fn test_training_list_includes_started_job() {
    let (state, _temp_dir) = setup_training_state().await;
    let claims = test_admin_claims();

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims.clone()),
        Json(make_request("adapter-list")),
    )
    .await
    .expect("start training");

    let Json(list) = list_training_jobs(
        State(state.clone()),
        Extension(claims),
        axum::extract::Query(TrainingListParams::default()),
    )
    .await
    .expect("list jobs");

    assert!(
        list.jobs.iter().any(|j| j.id == job.id),
        "started job should appear in list"
    );
}

#[tokio::test]
async fn test_training_logs_return_entries() {
    let (state, _temp_dir) = setup_training_state().await;
    let claims = test_admin_claims();

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims.clone()),
        Json(make_request("adapter-logs")),
    )
    .await
    .expect("start training");

    let _ = wait_for_terminal(&state, &job.id).await;

    let Json(logs) = get_training_logs(
        State(state.clone()),
        Extension(claims),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .expect("logs");

    assert!(
        !logs.is_empty(),
        "training logs should include at least one entry"
    );
    assert!(
        logs.iter().any(|l| l.contains("Training job")),
        "logs should contain creation message"
    );
}

#[tokio::test]
async fn test_training_cancel_transitions_job() {
    let (state, _temp_dir) = setup_training_state().await;
    let claims = test_admin_claims();

    let mut req = make_request("adapter-cancel");
    req.config.epochs = 25;
    req.config.gradient_accumulation_steps = Some(16);

    let Json(job) = start_training(State(state.clone()), Extension(claims.clone()), Json(req))
        .await
        .expect("start training");

    let status = cancel_training(
        State(state.clone()),
        Extension(claims),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .expect("cancel training");
    assert_eq!(status, axum::http::StatusCode::NO_CONTENT);

    let terminal = wait_for_terminal(&state, &job.id).await;
    assert!(
        matches!(
            terminal,
            TrainingJobStatus::Cancelled | TrainingJobStatus::Completed
        ),
        "job should end after cancellation request"
    );
}

async fn seed_dataset_version(
    state: &AppState,
    dataset_id: &str,
    version_id: &str,
    tenant_id: &str,
    hash: &str,
) -> anyhow::Result<()> {
    create_test_dataset(state, dataset_id).await?;
    state
        .db
        .create_training_dataset_version_with_id(
            version_id,
            dataset_id,
            Some(tenant_id),
            Some("v1"),
            "/tmp/test-dataset",
            hash,
            None,
            None,
            Some("tester"),
        )
        .await?;
    Ok(())
}

#[tokio::test]
async fn ui_path_computes_data_spec_hash_when_missing() {
    let (state, _temp_dir) = setup_training_state().await;
    let claims = test_admin_claims();
    let dataset_id = "ds-ui";
    let version_id = "ds-ui-ver-1";
    let manifest_hash = B3Hash::hash(b"dataset-ui-manifest").to_hex();

    seed_dataset_version(
        &state,
        dataset_id,
        version_id,
        &claims.tenant_id,
        &manifest_hash,
    )
    .await
    .expect("seed dataset version");

    let mut req = make_request("adapter-versioned");
    req.dataset_id = Some(dataset_id.to_string());
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: version_id.to_string(),
        weight: 1.0,
    }]);
    req.data_spec_hash = None;

    let Json(job) = start_training(State(state.clone()), Extension(claims), Json(req))
        .await
        .expect("start training with dataset_version_ids");

    let versions = job.dataset_version_ids.expect("dataset_version_ids");
    assert_eq!(versions[0].dataset_version_id, version_id);
    assert_eq!(job.data_spec_hash, Some(manifest_hash));
}

#[tokio::test]
async fn cli_path_rejects_data_spec_hash_mismatch() {
    let (state, _temp_dir) = setup_training_state().await;
    let claims = test_admin_claims();
    let dataset_id = "ds-cli";
    let version_id = "ds-cli-ver-1";
    let manifest_hash = B3Hash::hash(b"dataset-cli-manifest").to_hex();

    seed_dataset_version(
        &state,
        dataset_id,
        version_id,
        &claims.tenant_id,
        &manifest_hash,
    )
    .await
    .expect("seed dataset version");

    let mut req = make_request("adapter-cli");
    req.dataset_id = Some(dataset_id.to_string());
    req.dataset_version_ids = Some(vec![DatasetVersionSelection {
        dataset_version_id: version_id.to_string(),
        weight: 1.0,
    }]);
    req.data_spec_hash = Some("mismatch-hash".to_string());

    let Err((status, Json(err))) =
        start_training(State(state.clone()), Extension(claims), Json(req)).await
    else {
        panic!("expected start_training to reject hash mismatch");
    };

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err.code.as_str(), "VALIDATION_ERROR");
}
