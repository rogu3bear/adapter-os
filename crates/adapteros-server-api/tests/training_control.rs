use adapteros_server_api::auth::Claims;
use adapteros_server_api::handlers;
use adapteros_server_api::types::StreamQuery;
use axum::extract::State;
use axum::{Extension, Json};

mod common;
use common::{insert_training_job, setup_state, test_admin_claims};

#[tokio::test]
async fn pause_resume_happy_path_and_idempotent() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims: Claims = test_admin_claims();

    // Create orchestrator job and mirror in DB as running
    let config = adapteros_orchestrator::TrainingConfig {
        rank: 8,
        alpha: 16,
        targets: vec!["q_proj".to_string()],
        epochs: 1,
        learning_rate: 0.001,
        batch_size: 8,
        warmup_steps: None,
        max_seq_length: None,
        gradient_accumulation_steps: None,
    };
    let params = adapteros_orchestrator::training::TrainingJobBuilder::new()
        .adapter_name("adapter-x")
        .config(config)
        .build()
        .unwrap();
    let job = state.training_service.start_training(params).await?;

    // Pre-pause orchestrator to avoid race with fast training loop
    state.training_service.pause_job(&job.id).await.unwrap();
    insert_training_job(&state, &job.id, "running").await?;

    // Pause once
    let Json(resp) = handlers::pause_training_session(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| {
        anyhow::anyhow!(format!(
            "handler error {}: {}",
            status,
            serde_json::to_string(&err_json.0).unwrap_or_default()
        ))
    })?;
    assert_eq!(resp.status, "paused");

    // Pause again should be idempotent (still paused)
    let Json(resp2) = handlers::pause_training_session(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| {
        anyhow::anyhow!(format!(
            "handler error {}: {}",
            status,
            serde_json::to_string(&err_json.0).unwrap_or_default()
        ))
    })?;
    assert_eq!(resp2.status, "paused");

    // Mark DB as paused and resume
    state.db.update_training_status(&job.id, "paused").await?;
    let Json(resp3) = handlers::resume_training_session(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| {
        anyhow::anyhow!(format!(
            "handler error {}: {}",
            status,
            serde_json::to_string(&err_json.0).unwrap_or_default()
        ))
    })?;
    assert_eq!(resp3.status, "running");

    // Resume again idempotent (already effectively running)
    let Json(resp4) = handlers::resume_training_session(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| {
        anyhow::anyhow!(format!(
            "handler error {}: {}",
            status,
            serde_json::to_string(&err_json.0).unwrap_or_default()
        ))
    })?;
    assert_eq!(resp4.status, "running");

    Ok(())
}

#[tokio::test]
async fn pause_conflicts_on_terminal_state() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims: Claims = test_admin_claims();

    let job_id = "train-terminal-1".to_string();
    insert_training_job(&state, &job_id, "completed").await?;

    let err = handlers::pause_training_session(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job_id.clone()),
    )
    .await
    .err()
    .expect("should be conflict");
    assert_eq!(err.0, axum::http::StatusCode::CONFLICT);
    Ok(())
}

#[tokio::test]
async fn pause_returns_404_when_not_found_in_orchestrator() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims: Claims = test_admin_claims();

    // DB has a job, orchestrator does not
    let job_id = "train-orch-missing".to_string();
    insert_training_job(&state, &job_id, "running").await?;

    let err = handlers::pause_training_session(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job_id.clone()),
    )
    .await
    .err()
    .expect("should map to 404");
    assert_eq!(err.0, axum::http::StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn resume_validations_and_transitions() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims: Claims = test_admin_claims();

    // Orchestrator job exists
    let job = state
        .training_service
        .start_training(
            "adapter-y".to_string(),
            adapteros_orchestrator::TrainingConfig {
                rank: 8,
                alpha: 16,
                targets: vec!["q_proj".to_string()],
                epochs: 1,
                learning_rate: 0.001,
                batch_size: 8,
                warmup_steps: None,
                max_seq_length: None,
                gradient_accumulation_steps: None,
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            false,
            None,
        )
        .await?;

    // DB state paused -> resume -> running
    insert_training_job(&state, &job.id, "paused").await?;
    let Json(resp) = handlers::resume_training_session(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| {
        anyhow::anyhow!(format!(
            "handler error {}: {}",
            status,
            serde_json::to_string(&err_json.0).unwrap_or_default()
        ))
    })?;
    assert_eq!(resp.status, "running");
    Ok(())
}

#[tokio::test]
async fn training_stream_constructs_sse() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    // Supply a token via query param path and minimal StreamQuery
    let _sse = handlers::training_stream(
        State(state.clone()),
        Extension(test_admin_claims()),
        axum::extract::Query(StreamQuery {
            tenant: "default".to_string(),
        }),
    )
    .await;
    Ok(())
}
