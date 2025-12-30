//! Race condition tests for concurrency issues in the training pipeline.
//!
//! These tests cover cancel token races, concurrent job access,
//! and state transition timing issues.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use crate::training::job::{DataLineageMode, TrainingConfig, TrainingJobStatus};
use crate::training::service::TrainingService;

// ============================================================================
// Cancel Token Lifecycle Tests
// ============================================================================

/// Test that cancel works on a pending job.
#[tokio::test]
async fn test_cancel_pending_job() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "cancel-pending".to_string(),
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

    // Job is pending, cancel should succeed
    service.cancel_job(&job.id, None, None).await.unwrap();

    let cancelled = service.get_job(&job.id).await.unwrap();
    assert_eq!(cancelled.status, TrainingJobStatus::Cancelled);
}

/// Test that cancel after completion fails.
#[tokio::test]
async fn test_cancel_after_completion_fails() {
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

    // Complete the job
    service.complete_job(&job.id).await.unwrap();

    // Now cancel should fail
    let result = service.cancel_job(&job.id, None, None).await;
    assert!(result.is_err());
}

/// Test that cancel on non-existent job fails.
#[tokio::test]
async fn test_cancel_nonexistent_job() {
    let service = TrainingService::new();

    let result = service.cancel_job("fake-job-id", None, None).await;
    assert!(result.is_err());
}

// ============================================================================
// Concurrent Job Access Tests
// ============================================================================

/// Test concurrent reads of job list.
#[tokio::test]
async fn test_concurrent_list_jobs() {
    let service = Arc::new(TrainingService::new());
    let config = TrainingConfig::default();

    // Create some jobs
    for i in 0..5 {
        service
            .start_training(
                format!("concurrent-list-{}", i),
                config.clone(),
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
    }

    // Spawn multiple concurrent readers
    let mut handles = vec![];
    for _ in 0..10 {
        let svc = service.clone();
        handles.push(tokio::spawn(async move { svc.list_jobs().await.unwrap() }));
    }

    // All reads should succeed
    for handle in handles {
        let jobs = handle.await.unwrap();
        assert_eq!(jobs.len(), 5);
    }
}

/// Test concurrent progress updates.
#[tokio::test]
async fn test_concurrent_progress_updates() {
    let service = Arc::new(TrainingService::new());
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "concurrent-progress".to_string(),
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

    let job_id = job.id.clone();

    // Spawn concurrent progress updates
    let mut handles = vec![];
    for epoch in 1..=5 {
        let svc = service.clone();
        let id = job_id.clone();
        handles.push(tokio::spawn(async move {
            svc.update_progress(&id, epoch, 0.5 - (epoch as f32 * 0.1), 1000.0)
                .await
        }));
    }

    // All updates should succeed
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }

    // Final state should reflect some update
    let updated = service.get_job(&job_id).await.unwrap();
    assert!(updated.current_epoch >= 1);
}

/// Test that job creation gives unique IDs.
#[tokio::test]
async fn test_concurrent_job_creation_unique_ids() {
    let service = Arc::new(TrainingService::new());

    // Create multiple jobs concurrently
    let mut handles = vec![];
    for i in 0..10 {
        let svc = service.clone();
        handles.push(tokio::spawn(async move {
            let config = TrainingConfig::default();
            svc.start_training(
                format!("parallel-{}", i),
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
            .unwrap()
            .id
        }));
    }

    // Collect all job IDs
    let mut job_ids = vec![];
    for handle in handles {
        job_ids.push(handle.await.unwrap());
    }

    // All IDs should be unique
    job_ids.sort();
    job_ids.dedup();
    assert_eq!(job_ids.len(), 10, "All job IDs should be unique");
}

// ============================================================================
// State Transition Tests
// ============================================================================

/// Test state transitions are atomic.
#[tokio::test]
async fn test_state_transition_atomicity() {
    let service = Arc::new(TrainingService::new());
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "atomic-state".to_string(),
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

    let job_id = job.id.clone();

    // Start progress update which transitions to Running
    service
        .update_progress(&job_id, 1, 0.5, 1000.0)
        .await
        .unwrap();

    let running = service.get_job(&job_id).await.unwrap();
    assert_eq!(running.status, TrainingJobStatus::Running);

    // Complete the job
    service.complete_job(&job_id).await.unwrap();

    let completed = service.get_job(&job_id).await.unwrap();
    assert_eq!(completed.status, TrainingJobStatus::Completed);
}

/// Test that fail_job properly sets error state.
#[tokio::test]
async fn test_fail_job_sets_error() {
    let service = TrainingService::new();
    let config = TrainingConfig::default();

    let job = service
        .start_training(
            "fail-test".to_string(),
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

    let error_msg = "Simulated training failure".to_string();
    service.fail_job(&job.id, error_msg.clone()).await.unwrap();

    let failed = service.get_job(&job.id).await.unwrap();
    assert_eq!(failed.status, TrainingJobStatus::Failed);
    assert_eq!(failed.error_message, Some(error_msg));
    assert!(
        failed.completed_at.is_some(),
        "completed_at should be set on failure"
    );
}

// ============================================================================
// Cancel Token Pattern Tests
// ============================================================================

/// Test AtomicBool cancel pattern works correctly.
#[test]
fn test_cancel_token_pattern() {
    let cancel_token = Arc::new(AtomicBool::new(false));

    // Initially not cancelled
    assert!(!cancel_token.load(Ordering::SeqCst));

    // Set cancel flag
    cancel_token.store(true, Ordering::SeqCst);

    // Should be cancelled now
    assert!(cancel_token.load(Ordering::SeqCst));
}

/// Test cancel token visibility across threads.
#[tokio::test]
async fn test_cancel_token_cross_thread_visibility() {
    let cancel_token = Arc::new(AtomicBool::new(false));
    let counter = Arc::new(AtomicU32::new(0));

    let token_clone = cancel_token.clone();
    let counter_clone = counter.clone();

    // Spawn a task that checks the token
    let handle = tokio::spawn(async move {
        loop {
            if token_clone.load(Ordering::SeqCst) {
                break;
            }
            counter_clone.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
        }
    });

    // Let the task run for a bit
    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

    // Set cancel
    cancel_token.store(true, Ordering::SeqCst);

    // Wait for task to finish
    tokio::time::timeout(tokio::time::Duration::from_millis(100), handle)
        .await
        .expect("task should observe cancellation")
        .expect("task join");

    let count = counter.load(Ordering::SeqCst);
    assert!(count > 0, "task should have made progress before cancel");
}

// ============================================================================
// Multiple Operations on Same Job Tests
// ============================================================================

/// Test multiple progress updates don't corrupt state.
#[tokio::test]
async fn test_multiple_progress_updates() {
    let service = TrainingService::new();
    let mut config = TrainingConfig::default();
    config.epochs = 10;

    let job = service
        .start_training(
            "multi-progress".to_string(),
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

    // Update progress multiple times
    for epoch in 1..=10 {
        let loss = 1.0 - (epoch as f32 * 0.1);
        service
            .update_progress(&job.id, epoch, loss, 1000.0 + (epoch as f32 * 100.0))
            .await
            .unwrap();
    }

    let updated = service.get_job(&job.id).await.unwrap();
    assert_eq!(updated.current_epoch, 10);
    assert!((updated.progress_pct - 100.0).abs() < 0.1);
}

/// Test rapid create-cancel cycle.
#[tokio::test]
async fn test_rapid_create_cancel_cycle() {
    let service = TrainingService::new();

    for i in 0..5 {
        let config = TrainingConfig::default();
        let job = service
            .start_training(
                format!("rapid-cancel-{}", i),
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

        // Immediately cancel
        service.cancel_job(&job.id, None, None).await.unwrap();

        let cancelled = service.get_job(&job.id).await.unwrap();
        assert_eq!(cancelled.status, TrainingJobStatus::Cancelled);
    }

    // All jobs should be in the list
    let all_jobs = service.list_jobs().await.unwrap();
    assert_eq!(all_jobs.len(), 5);
}

// ============================================================================
// Template Access Tests
// ============================================================================

/// Test concurrent template access.
#[tokio::test]
async fn test_concurrent_template_access() {
    let service = Arc::new(TrainingService::new());

    let mut handles = vec![];
    for _ in 0..10 {
        let svc = service.clone();
        handles.push(tokio::spawn(async move { svc.list_templates().await }));
    }

    for handle in handles {
        let templates = handle.await.unwrap().unwrap();
        assert!(templates.len() >= 4, "Should have default templates");
    }
}

/// Test get_template returns consistent results.
#[tokio::test]
async fn test_get_template_consistency() {
    let service = TrainingService::new();

    // Get same template multiple times
    let t1 = service.get_template("general-code").await.unwrap();
    let t2 = service.get_template("general-code").await.unwrap();

    assert_eq!(t1.id, t2.id);
    assert_eq!(t1.name, t2.name);
    assert_eq!(t1.config.rank, t2.config.rank);
}
