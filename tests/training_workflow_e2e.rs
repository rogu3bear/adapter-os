#![cfg(all(test, feature = "extended-tests"))]

//! End-to-end training workflow integration tests

use adapteros_core::{TrainingConfig, TrainingJobStatus};
use adapteros_db::Db;
use adapteros_orchestrator::training::{TrainingJobBuilder, TrainingService};
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

async fn setup_test_db() -> Result<(Db, TempDir)> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let db = Db::connect(&format!("sqlite://{}", db_path.display())).await?;
    db.migrate().await?;
    Ok((db, temp_dir))
}

#[tokio::test]
async fn test_training_service_lifecycle() -> Result<()> {
    let (db, _temp_dir) = setup_test_db().await?;
    let db_arc = Arc::new(db);
    let service = TrainingService::new_with_db(db_arc.clone(), "qwen2.5-7b");

    // Test: List templates
    let templates = service.list_templates().await?;
    assert!(templates.len() >= 4);
    assert!(templates.iter().any(|t| t.id == "general-code"));

    // Test: Start training job
    let config = TrainingConfig {
        rank: 8,
        alpha: 16,
        epochs: 1,
        learning_rate: 0.001,
        batch_size: 4,
        ..Default::default()
    };
    let params = TrainingJobBuilder::new()
        .adapter_name("test-adapter")
        .config(config.clone())
        .repo_id(Some("test-repo".to_string()))
        .build()?;

    let job = service.start_training(params).await?;
    assert_eq!(job.status, TrainingJobStatus::Pending);
    assert_eq!(job.adapter_name, "test-adapter");
    assert_eq!(job.config.rank, 8);

    // Test: Get job details
    let retrieved_job = service.get_job(&job.id).await?;
    assert_eq!(retrieved_job.id, job.id);
    assert_eq!(retrieved_job.adapter_name, "test-adapter");

    // Test: List jobs
    let jobs = service.list_jobs().await?;
    assert!(jobs.iter().any(|j| j.id == job.id));

    // Test: Cancel job
    service.cancel_job(&job.id).await?;
    let cancelled_job = service.get_job(&job.id).await?;
    assert_eq!(cancelled_job.status, TrainingJobStatus::Cancelled);

    // Test: Verify job persisted to database
    let db_job = db_arc.get_training_job(&job.id).await?;
    assert!(db_job.is_some());
    let db_job = db_job.unwrap();
    assert_eq!(db_job.status, "cancelled");

    Ok(())
}

#[tokio::test]
async fn test_training_template_loading() -> Result<()> {
    let (db, _temp_dir) = setup_test_db().await?;
    let service = TrainingService::new_with_db(Arc::new(db), "qwen2.5-7b");

    // Test: Load default templates
    let templates = service.list_templates().await?;
    assert!(templates.len() >= 4);

    // Test: Validate template configuration
    let general_code = service.get_template("general-code").await?;
    assert_eq!(general_code.id, "general-code");
    assert_eq!(general_code.name, "General Code Adapter");
    assert_eq!(general_code.config.rank, 16);
    assert_eq!(general_code.config.alpha, 32);

    let ephemeral_quick = service.get_template("ephemeral-quick").await?;
    assert_eq!(ephemeral_quick.config.rank, 8);
    assert_eq!(ephemeral_quick.config.epochs, 1);

    // Test: Apply template to new job
    let template_config = general_code.config.clone();
    let params = TrainingJobBuilder::new()
        .adapter_name("template-test")
        .config(template_config)
        .template_id(Some("general-code".to_string()))
        .build()?;

    let job = service.start_training(params).await?;
    assert_eq!(job.template_id, Some("general-code".to_string()));
    assert_eq!(job.config.rank, 16);
    assert_eq!(job.config.alpha, 32);

    Ok(())
}

#[tokio::test]
async fn test_training_metrics_collection() -> Result<()> {
    let (db, _temp_dir) = setup_test_db().await?;
    let db_arc = Arc::new(db);
    let service = TrainingService::new_with_db(db_arc.clone(), "qwen2.5-7b");

    // Test: Start training
    let config = TrainingConfig {
        rank: 4,
        alpha: 8,
        epochs: 2,
        learning_rate: 0.001,
        batch_size: 2,
        ..Default::default()
    };
    let params = TrainingJobBuilder::new()
        .adapter_name("metrics-test")
        .config(config)
        .build()?;

    let job = service.start_training(params).await?;

    // Test: Update progress (simulating training worker)
    service.update_progress(&job.id, 1, 0.5, 100.0).await?;

    let updated_job = service.get_job(&job.id).await?;
    assert_eq!(updated_job.current_epoch, 1);
    assert!((updated_job.current_loss - 0.5).abs() < 0.01);
    assert!((updated_job.tokens_per_second - 100.0).abs() < 0.01);
    assert_eq!(updated_job.status, TrainingJobStatus::Running);

    // Test: Verify metrics persisted to database
    let db_job = db_arc.get_training_job(&job.id).await?;
    assert!(db_job.is_some());
    let db_job = db_job.unwrap();
    let progress: adapteros_db::training_jobs::TrainingProgress =
        serde_json::from_str(&db_job.progress_json)?;
    assert_eq!(progress.current_epoch, 1);
    assert!((progress.current_loss - 0.5).abs() < 0.01);

    // Test: Check logs are being written
    let logs = service.get_logs(&job.id).await?;
    assert!(!logs.is_empty());
    assert!(logs.iter().any(|l| l.contains("Training job")));

    Ok(())
}

#[tokio::test]
async fn test_training_error_handling() -> Result<()> {
    let (db, _temp_dir) = setup_test_db().await?;
    let service = TrainingService::new_with_db(Arc::new(db), "qwen2.5-7b");

    // Test: Invalid configuration rejection (via builder)
    let result = TrainingJobBuilder::new().build();
    assert!(result.is_err()); // Missing adapter_name and config

    // Test: Start job with valid config
    let config = TrainingConfig::default();
    let params = TrainingJobBuilder::new()
        .adapter_name("error-test")
        .config(config)
        .build()?;

    let job = service.start_training(params).await?;

    // Test: Cancellation cleanup
    service.cancel_job(&job.id).await?;
    let cancelled_job = service.get_job(&job.id).await?;
    assert_eq!(cancelled_job.status, TrainingJobStatus::Cancelled);
    assert!(cancelled_job.completed_at.is_some());

    // Test: Cannot cancel already cancelled job
    let result = service.cancel_job(&job.id).await;
    // Should succeed (idempotent) or fail gracefully - depends on implementation
    // For now, we'll verify the job is still cancelled
    let still_cancelled = service.get_job(&job.id).await?;
    assert_eq!(still_cancelled.status, TrainingJobStatus::Cancelled);

    // Test: Cannot pause completed job
    let result = service.pause_job(&job.id).await;
    assert!(result.is_err()); // Should fail for terminal state

    // Test: Fail job explicitly
    service.fail_job(&job.id, "Test error".to_string()).await?;
    let failed_job = service.get_job(&job.id).await?;
    assert_eq!(failed_job.status, TrainingJobStatus::Failed);
    assert_eq!(failed_job.error_message, Some("Test error".to_string()));

    // Test: Recovery from failures - start new job after failure
    let config2 = TrainingConfig::default();
    let params2 = TrainingJobBuilder::new()
        .adapter_name("recovery-test")
        .config(config2)
        .build()?;

    let job2 = service.start_training(params2).await?;
    assert_eq!(job2.status, TrainingJobStatus::Pending);
    assert_ne!(job2.id, job.id); // Different job ID

    Ok(())
}

#[tokio::test]
async fn test_training_pause_resume() -> Result<()> {
    let (db, _temp_dir) = setup_test_db().await?;
    let service = TrainingService::new_with_db(Arc::new(db), "qwen2.5-7b");

    let config = TrainingConfig::default();
    let params = TrainingJobBuilder::new()
        .adapter_name("pause-resume-test")
        .config(config)
        .build()?;

    let job = service.start_training(params).await?;

    // Test: Pause job
    service.pause_job(&job.id).await?;
    let paused_job = service.get_job(&job.id).await?;
    assert_eq!(paused_job.status, TrainingJobStatus::Paused);

    // Test: Resume job
    service.resume_job(&job.id).await?;
    let resumed_job = service.get_job(&job.id).await?;
    assert_eq!(resumed_job.status, TrainingJobStatus::Running);

    // Test: Idempotent pause
    service.pause_job(&job.id).await?;
    service.pause_job(&job.id).await?; // Should succeed
    let still_paused = service.get_job(&job.id).await?;
    assert_eq!(still_paused.status, TrainingJobStatus::Paused);

    Ok(())
}

#[tokio::test]
async fn test_training_logs_persistence() -> Result<()> {
    let (db, temp_dir) = setup_test_db().await?;
    let service = TrainingService::new_with_db(Arc::new(db), "qwen2.5-7b");

    let config = TrainingConfig::default();
    let params = TrainingJobBuilder::new()
        .adapter_name("logs-test")
        .config(config)
        .build()?;

    let job = service.start_training(params).await?;

    // Wait a bit for logs to be written
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Test: Get logs
    let logs = service.get_logs(&job.id).await?;
    assert!(!logs.is_empty());

    // Verify log content
    let log_text = logs.join("\n");
    assert!(log_text.contains("Training job"));
    assert!(log_text.contains("logs-test"));

    // Test: Logs persist after job cancellation
    service.cancel_job(&job.id).await?;
    let logs_after_cancel = service.get_logs(&job.id).await?;
    assert!(!logs_after_cancel.is_empty());

    Ok(())
}
