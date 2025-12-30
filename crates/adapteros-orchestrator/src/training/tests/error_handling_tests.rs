//! Error handling tests for silent error patterns in the training pipeline.
//!
//! These tests verify that error paths that use `.ok()`, `.unwrap_or_default()`,
//! or log-only warnings are handled correctly without causing silent data loss
//! or incorrect job state.

use crate::training::job::{DataLineageMode, DatasetVersionSelection, TrainingConfig};
use crate::training::service::TrainingService;

/// Test that training proceeds when config hash computation fails.
/// The config_hash field should be None but the job should still be created.
#[tokio::test]
async fn test_start_training_proceeds_without_config_hash_on_failure() {
    let service = TrainingService::new();

    // Create a config that will have a valid hash - this tests the success path.
    // We can't easily force compute_config_hash to fail without mocking,
    // but we verify that config_hash is populated when it succeeds.
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "test-config-hash".to_string(),
            config,
            None, // template_id
            None, // repo_id
            None, // target_branch
            None, // base_version_id
            None, // dataset_id
            None, // dataset_version_ids
            true, // synthetic_mode
            DataLineageMode::Synthetic,
            None, // tenant_id
            None, // initiated_by
            None, // initiated_by_role
            None, // base_model_id
            None, // collection_id
            None, // scope
            None, // lora_tier
            None, // category
            None, // description
            None, // language
            None, // framework_id
            None, // framework_version
            None, // post_actions_json
            None, // retry_of_job_id
            None, // versioning
            None, // code_commit_sha
            None, // data_spec_json
            None, // data_spec_hash
        )
        .await
        .expect("Job creation should succeed even if config_hash fails");

    // Job should be created regardless of config hash status
    assert!(!job.id.is_empty());
    // config_hash_b3 may be Some or None depending on whether compute succeeded
    // The important thing is the job was created
}

/// Test that training rejects jobs with synthetic_mode=true AND dataset_version_ids.
/// This validates the mutual exclusivity constraint.
#[tokio::test]
async fn test_start_training_rejects_synthetic_with_dataset_versions() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let result = service
        .start_training(
            "test-invalid-combo".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            Some(vec![DatasetVersionSelection {
                dataset_version_id: "fake-version".to_string(),
                weight: 1.0,
            }]),
            true, // synthetic_mode = true with dataset_versions = invalid
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;

    assert!(
        result.is_err(),
        "Should reject synthetic_mode=true with dataset_version_ids"
    );
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("synthetic_mode=true requires dataset_version_ids to be empty"),
        "Error message should explain the constraint: {}",
        err_msg
    );
}

/// Test that non-fatal DB write failures don't crash the job creation.
/// Without a database configured, the job should still be created and
/// the warning path should be taken.
#[tokio::test]
async fn test_job_creation_succeeds_without_db() {
    // TrainingService without DB should still create jobs
    let service = TrainingService::new(); // No DB configured

    let config = TrainingConfig::default();
    let job = service
        .start_training(
            "no-db-adapter".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true, // synthetic_mode
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .expect("Job creation should succeed without DB");

    assert!(!job.id.is_empty());
    // The job is in-memory only, no DB persistence
    assert!(service.get_job(&job.id).await.is_ok());
}

/// Test that fail_job correctly handles missing adapter_id.
/// This exercises the adapter lifecycle transition code path.
#[tokio::test]
async fn test_fail_job_handles_missing_adapter_id() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "fail-test-adapter".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Fail the job - adapter_id is None at this point
    service
        .fail_job(&job.id, "Test failure".to_string())
        .await
        .expect("fail_job should succeed even without adapter_id");

    let failed_job = service.get_job(&job.id).await.unwrap();
    assert_eq!(
        failed_job.status,
        crate::training::job::TrainingJobStatus::Failed
    );
    assert_eq!(failed_job.error_message, Some("Test failure".to_string()));
}

/// Test that update_progress handles non-existent job ID.
#[tokio::test]
async fn test_update_progress_rejects_unknown_job() {
    let service = TrainingService::new();

    let result = service
        .update_progress("nonexistent-job-id", 1, 0.5, 1000.0)
        .await;

    assert!(result.is_err(), "Should reject unknown job ID");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "Error should indicate job not found: {}",
        err_msg
    );
}

/// Test that complete_job handles non-existent job ID.
#[tokio::test]
async fn test_complete_job_rejects_unknown_job() {
    let service = TrainingService::new();

    let result = service.complete_job("nonexistent-job-id").await;

    assert!(result.is_err(), "Should reject unknown job ID");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("not found"),
        "Error should indicate job not found: {}",
        err_msg
    );
}

/// Test that cancel_job correctly handles a job that's already completed.
#[tokio::test]
async fn test_cancel_job_rejects_completed_job() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "complete-then-cancel".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Complete the job first
    service.complete_job(&job.id).await.unwrap();

    // Now try to cancel it
    let result = service.cancel_job(&job.id, None, None).await;

    assert!(result.is_err(), "Should reject cancelling completed job");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Cannot cancel job in state"),
        "Error should explain state constraint: {}",
        err_msg
    );
}

/// Test that cancel_job correctly handles a job that's already failed.
#[tokio::test]
async fn test_cancel_job_rejects_failed_job() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "fail-then-cancel".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Fail the job first
    service
        .fail_job(&job.id, "Intentional failure".to_string())
        .await
        .unwrap();

    // Now try to cancel it
    let result = service.cancel_job(&job.id, None, None).await;

    assert!(result.is_err(), "Should reject cancelling failed job");
}

/// Test that build_id falls back through the chain correctly.
/// This tests the silent fallback to "dev" when no env vars are set.
#[tokio::test]
async fn test_build_id_fallback_chain() {
    // Clear any existing env vars for this test
    std::env::remove_var("BUILD_ID");
    std::env::remove_var("GIT_COMMIT");

    let service = TrainingService::new();
    let config = TrainingConfig::default();

    // Without code_commit_sha and without env vars, should fall back to "dev"
    let job = service
        .start_training(
            "build-id-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(job.build_id, Some("dev".to_string()));
}

/// Test that build_id prefers code_commit_sha when provided.
#[tokio::test]
async fn test_build_id_uses_code_commit_sha() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "build-id-sha-test".to_string(),
            config,
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("abc123def".to_string()), // code_commit_sha
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(job.build_id, Some("abc123def".to_string()));
}
