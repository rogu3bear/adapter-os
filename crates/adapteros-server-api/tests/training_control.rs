//! Training control integration tests
use std::sync::Arc;
use std::time::Duration;

use adapteros_api_types::{StartTrainingRequest, TrainingConfigRequest, TrainingListParams};
use adapteros_orchestrator::TrainingJobStatus;
use adapteros_server_api::handlers::get_training_logs;
use adapteros_server_api::handlers::training::{
    cancel_training, get_training_job, list_training_jobs, start_training,
};
use adapteros_server_api::state::AppState;
use adapteros_types::training::TrainingConfig;
use axum::{extract::State, Extension, Json};
use tempfile::TempDir;
use tokio::time::sleep;

mod common;
use common::test_admin_claims;

fn make_request(name: &str) -> StartTrainingRequest {
    let cfg = TrainingConfig::quick_training();
    StartTrainingRequest {
        adapter_name: name.to_string(),
        config: TrainingConfigRequest {
            rank: cfg.rank,
            alpha: cfg.alpha,
            targets: cfg.targets,
            epochs: cfg.epochs,
            learning_rate: cfg.learning_rate,
            batch_size: cfg.batch_size,
            warmup_steps: cfg.warmup_steps,
            max_seq_length: cfg.max_seq_length,
            gradient_accumulation_steps: cfg.gradient_accumulation_steps,
        },
        template_id: None,
        repo_id: None,
        dataset_id: None,
        base_model_id: None,
        collection_id: None,
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
    state
        .training_service
        .get_job(job_id)
        .await
        .unwrap()
        .status
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

    let Json(job) = start_training(
        State(state.clone()),
        Extension(claims.clone()),
        Json(req),
    )
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
        matches!(terminal, TrainingJobStatus::Cancelled | TrainingJobStatus::Completed),
        "job should end after cancellation request"
    );
}
