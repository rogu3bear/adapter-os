use adapteros_server_api::auth::Claims;
use adapteros_server_api::handlers;
use adapteros_server_api::types::{StartTrainingRequest, TrainingConfigRequest};
use axum::extract::State;
use axum::{Extension, Json};

mod common;
use common::{setup_state, test_admin_claims};

#[tokio::test]
async fn list_and_get_training_jobs() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims: Claims = test_admin_claims();

    // Seed orchestrator with jobs
    let j1 = state
        .training_service
        .start_training(
            "adapter-a".to_string(),
            adapteros_orchestrator::TrainingConfig::default(),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            None,
        )
        .await?;
    let _j2 = state
        .training_service
        .start_training(
            "adapter-b".to_string(),
            adapteros_orchestrator::TrainingConfig::default(),
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

    // List
    let Json(list) = handlers::list_training_jobs(State(state.clone()), Extension(claims.clone()))
        .await
        .map_err(|(status, err_json)| anyhow::anyhow!(format!("handler error {}: {}", status, serde_json::to_string(&err_json.0).unwrap_or_default())))?;
    assert!(list.len() >= 2);

    // Get one
    let Json(one) = handlers::get_training_job(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(j1.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| anyhow::anyhow!(format!("handler error {}: {}", status, serde_json::to_string(&err_json.0).unwrap_or_default())))?;
    assert_eq!(one.id, j1.id);

    // Get unknown should 404
    let err = handlers::get_training_job(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path("missing-id".to_string()),
    )
    .await
    .err()
    .expect("expected 404 for missing job");
    assert_eq!(err.0, axum::http::StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
async fn start_cancel_logs_metrics_artifacts_roundtrip() -> anyhow::Result<()> {
    let state = setup_state(None).await?;
    let claims: Claims = test_admin_claims();

    // Start training
    let req = StartTrainingRequest {
        adapter_name: "adapter-c".to_string(),
        config: TrainingConfigRequest {
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
        template_id: None,
        repo_id: None,
        dataset_path: None,
        directory_root: None,
        directory_path: None,
        tenant_id: Some("tenant-1".to_string()),
        adapters_root: None,
        package: Some(false),
        register: Some(false),
        adapter_id: None,
        tier: None,
    };

    let Json(job) = handlers::start_training(
        State(state.clone()),
        Extension(claims.clone()),
        Json(req),
    )
    .await
    .map_err(|(status, err_json)| anyhow::anyhow!(format!("handler error {}: {}", status, serde_json::to_string(&err_json.0).unwrap_or_default())))?;

    // Cancel
    let status = handlers::cancel_training(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| anyhow::anyhow!(format!("handler error {}: {}", status, serde_json::to_string(&err_json.0).unwrap_or_default())))?;
    assert_eq!(status, axum::http::StatusCode::OK);

    // Logs (may be empty)
    let _ = handlers::get_training_logs(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await;

    // Metrics presence
    let Json(metrics) = handlers::get_training_metrics(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| anyhow::anyhow!(format!("handler error {}: {}", status, serde_json::to_string(&err_json.0).unwrap_or_default())))?;
    assert!(metrics.total_epochs >= 1);

    // Artifacts default response without real packaging
    let Json(art) = handlers::get_training_artifacts(
        State(state.clone()),
        Extension(claims.clone()),
        axum::extract::Path(job.id.clone()),
    )
    .await
    .map_err(|(status, err_json)| anyhow::anyhow!(format!("handler error {}: {}", status, serde_json::to_string(&err_json.0).unwrap_or_default())))?;
    assert!(!art.ready);
    Ok(())
}
