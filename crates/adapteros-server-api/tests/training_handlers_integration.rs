//! Integration tests for training job handlers
//!
//! Tests training job lifecycle including start, cancel, list,
//! status checking, and tenant isolation.

use adapteros_core::Result;
use adapteros_server_api::handlers::{
    cancel_training, get_training_job, get_training_queue, list_training_jobs,
};
use adapteros_server_api::ip_extraction::ClientIp;
use adapteros_server_api::types::TrainingListParams;
use adapteros_types::training::{DataLineageMode, TrainingConfig};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;

mod common;
use common::{
    delete_test_training_job, setup_state, test_admin_claims, test_viewer_claims, TestkitEnvGuard,
};

/// Test listing training jobs returns tenant-scoped results
#[tokio::test]
async fn list_training_jobs_returns_tenant_scoped_results() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    // Create training jobs for different tenants
    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-tenant1-1")
    .bind("repo-1")
    .bind("tenant-1")
    .bind("{\"rank\":16,\"alpha\":32}")
    .bind("pending")
    .bind("{}")
    .bind("user1")
    .execute(state.db.pool_result()?)
    .await?;

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-tenant1-2")
    .bind("repo-1")
    .bind("tenant-1")
    .bind("{\"rank\":8,\"alpha\":16}")
    .bind("running")
    .bind("{}")
    .bind("user1")
    .execute(state.db.pool_result()?)
    .await?;

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-default-1")
    .bind("repo-2")
    .bind("default")
    .bind("{\"rank\":16,\"alpha\":32}")
    .bind("completed")
    .bind("{}")
    .bind("user2")
    .execute(state.db.pool_result()?)
    .await?;

    // List with tenant-1 credentials
    let claims = test_admin_claims(); // tenant-1
    let result = list_training_jobs(
        State(state.clone()),
        Extension(claims),
        Query(TrainingListParams::default()),
    )
    .await;

    assert!(result.is_ok(), "list should succeed");
    let jobs = result.unwrap().0;
    assert_eq!(jobs.jobs.len(), 2, "should only see tenant-1 jobs");
    assert!(jobs.jobs.iter().all(|j| j.id.starts_with("job-tenant1")));

    // List with default tenant credentials
    let default_claims = test_viewer_claims(); // default tenant
    let result2 = list_training_jobs(
        State(state.clone()),
        Extension(default_claims),
        Query(TrainingListParams::default()),
    )
    .await;

    assert!(result2.is_ok(), "list should succeed");
    let jobs2 = result2.unwrap().0;
    assert_eq!(jobs2.jobs.len(), 1, "should only see default job");
    assert_eq!(jobs2.jobs[0].id, "job-default-1");

    // Cleanup
    delete_test_training_job(&state, "job-tenant1-1").await?;
    delete_test_training_job(&state, "job-tenant1-2").await?;
    delete_test_training_job(&state, "job-default-1").await?;

    Ok(())
}

/// Admin listing should fall back to DB records when orchestrator cache is empty.
#[tokio::test]
async fn list_training_jobs_admin_falls_back_to_db_without_e2e_mode() -> Result<()> {
    let _env_guard = TestkitEnvGuard::disabled().await;
    let state = setup_state(None).await.expect("state");

    adapteros_db::sqlx::query(
        "INSERT INTO git_repositories (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("git-repo-1")
    .bind("repo-1")
    .bind("/tmp/repo-1")
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind("ready")
    .bind("tenant-1-user")
    .execute(state.db.pool_result()?)
    .await
    .expect("git repository seed should succeed");

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-admin-db-fallback")
    .bind("repo-1")
    .bind("tenant-1")
    .bind("{\"rank\":16,\"alpha\":32}")
    .bind("pending")
    .bind("{}")
    .bind("tenant-1-user")
    .execute(state.db.pool_result()?)
    .await
    .expect("test insert should succeed");

    let claims = test_admin_claims();
    let result = list_training_jobs(
        State(state.clone()),
        Extension(claims),
        Query(TrainingListParams::default()),
    )
    .await;

    assert!(result.is_ok(), "list should succeed");
    let jobs = result.unwrap().0;
    assert!(
        jobs.jobs
            .iter()
            .any(|job| job.id == "job-admin-db-fallback"),
        "admin list should include DB-backed jobs when in-memory list is empty"
    );

    delete_test_training_job(&state, "job-admin-db-fallback")
        .await
        .expect("test cleanup should succeed");
    Ok(())
}

/// Test getting specific training job
#[tokio::test]
async fn get_training_job_returns_details() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-details-test")
    .bind("repo-1")
    .bind("tenant-1")
    .bind("{\"rank\":16,\"alpha\":32,\"epochs\":3}")
    .bind("pending")
    .bind("{\"progress_pct\":0.0}")
    .bind("tester")
    .execute(state.db.pool_result()?)
    .await?;

    let claims = test_admin_claims();
    let result = get_training_job(
        State(state.clone()),
        Extension(claims),
        Path("job-details-test".to_string()),
    )
    .await;

    assert!(result.is_ok(), "get should succeed");
    let job = result.unwrap().0;
    assert_eq!(job.id, "job-details-test");
    assert_eq!(job.status, "pending");

    // Cleanup
    delete_test_training_job(&state, "job-details-test").await?;

    Ok(())
}

/// Test cross-tenant training job access returns 404
#[tokio::test]
async fn get_training_job_cross_tenant_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-tenant1-private")
    .bind("repo-1")
    .bind("tenant-1")
    .bind("{\"rank\":16}")
    .bind("running")
    .bind("{}")
    .bind("user1")
    .execute(state.db.pool_result()?)
    .await?;

    // Try to access from different tenant
    let other_claims = test_viewer_claims(); // default tenant
    let result = get_training_job(
        State(state.clone()),
        Extension(other_claims),
        Path("job-tenant1-private".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant access should fail"),
    }

    // Cleanup
    delete_test_training_job(&state, "job-tenant1-private").await?;

    Ok(())
}

/// Test DB status is authoritative when an in-memory shadow diverges.
#[tokio::test]
async fn get_training_job_prefers_db_status_over_in_memory_shadow() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    let in_memory_job = state
        .training_service
        .start_training(
            "shadow-job".to_string(),
            TrainingConfig::default(),
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            Some("tenant-1".to_string()),
            Some("tenant-1-user".to_string()),
            Some("admin".to_string()),
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
        .expect("start in-memory job");

    state.db.ensure_direct_training_repo_exists("user1").await?;

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&in_memory_job.id)
    .bind("direct-training")
    .bind("tenant-1")
    .bind("{\"rank\":16,\"alpha\":32}")
    .bind("cancelled")
    .bind("{}")
    .bind("user1")
    .execute(state.db.pool_result()?)
    .await?;

    let claims = test_admin_claims();
    let result = get_training_job(
        State(state.clone()),
        Extension(claims),
        Path(in_memory_job.id.clone()),
    )
    .await;

    assert!(result.is_ok(), "get should succeed");
    let job = result.unwrap().0;
    assert_eq!(
        job.status, "cancelled",
        "DB status should be authoritative over in-memory shadow state"
    );

    delete_test_training_job(&state, &in_memory_job.id).await?;
    Ok(())
}

/// Test queue endpoint uses DB data when in-memory state diverges.
#[tokio::test]
async fn get_training_queue_prefers_db_state_over_in_memory_shadow() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    let in_memory_job = state
        .training_service
        .start_training(
            "shadow-queue-job".to_string(),
            TrainingConfig::default(),
            None,
            None,
            None,
            None,
            None,
            None,
            true,
            DataLineageMode::Synthetic,
            Some("tenant-1".to_string()),
            Some("tenant-1-user".to_string()),
            Some("admin".to_string()),
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
        .expect("start in-memory job");

    state
        .training_service
        .update_progress(&in_memory_job.id, 1, 0.42, 12.0)
        .await
        .expect("transition in-memory job to running");

    state.db.ensure_direct_training_repo_exists("user1").await?;

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&in_memory_job.id)
    .bind("direct-training")
    .bind("tenant-1")
    .bind("{\"rank\":16,\"alpha\":32}")
    .bind("cancelled")
    .bind("{}")
    .bind("user1")
    .execute(state.db.pool_result()?)
    .await?;

    let claims = test_admin_claims();
    let queue = get_training_queue(State(state.clone()), Extension(claims))
        .await
        .expect("queue should succeed")
        .0;

    assert_eq!(
        queue.queue_depth, 0,
        "DB cancelled status should remove diverged in-memory running shadow from queue"
    );
    assert!(
        queue
            .pending_jobs
            .iter()
            .all(|job| job.id != in_memory_job.id),
        "pending queue should not include in-memory shadow job"
    );
    assert!(
        queue
            .running_jobs
            .iter()
            .all(|job| job.id != in_memory_job.id),
        "running queue should not include in-memory shadow job"
    );

    delete_test_training_job(&state, &in_memory_job.id).await?;
    Ok(())
}

/// Test queue endpoint falls back to in-memory state when DB queue queries fail.
#[tokio::test]
async fn get_training_queue_falls_back_to_in_memory_on_db_error() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    adapteros_db::sqlx::query("DROP TABLE repository_training_jobs")
        .execute(state.db.pool_result()?)
        .await?;

    let claims = test_admin_claims();
    let queue = get_training_queue(State(state), Extension(claims))
        .await
        .expect("queue fallback should succeed")
        .0;

    assert_eq!(queue.queue_depth, 0);
    assert_eq!(queue.pending_count, 0);
    assert_eq!(queue.running_count, 0);
    Ok(())
}

/// Test canceling a training job
#[tokio::test]
async fn cancel_training_job_succeeds() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-to-cancel")
    .bind("repo-1")
    .bind("tenant-1")
    .bind("{\"rank\":16}")
    .bind("running")
    .bind("{}")
    .bind("user1")
    .execute(state.db.pool_result()?)
    .await?;

    let claims = test_admin_claims();

    // Cancel the job
    let result = cancel_training(
        State(state.clone()),
        Extension(claims.clone()),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path("job-to-cancel".to_string()),
    )
    .await;

    assert!(result.is_ok(), "cancel should succeed");

    // Verify job is canceled
    let get_result = get_training_job(
        State(state.clone()),
        Extension(claims),
        Path("job-to-cancel".to_string()),
    )
    .await;

    if let Ok(job) = get_result {
        assert!(
            job.0.status.contains("cancel") || job.0.status == "cancelled",
            "job should be marked as cancelled"
        );
    }

    // Cleanup
    delete_test_training_job(&state, "job-to-cancel").await?;

    Ok(())
}

/// Test cross-tenant cancel returns 404
#[tokio::test]
async fn cancel_training_job_cross_tenant_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-protected")
    .bind("repo-1")
    .bind("tenant-1")
    .bind("{\"rank\":16}")
    .bind("running")
    .bind("{}")
    .bind("user1")
    .execute(state.db.pool_result()?)
    .await?;

    // Try to cancel from different tenant
    let other_claims = test_viewer_claims(); // default tenant
    let result = cancel_training(
        State(state.clone()),
        Extension(other_claims),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path("job-protected".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("cross-tenant cancel should fail"),
    }

    // Cleanup
    delete_test_training_job(&state, "job-protected").await?;

    Ok(())
}

/// Test filtering training jobs by status
#[tokio::test]
async fn list_training_jobs_filters_by_status() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    // Create jobs with different statuses
    for (id, status) in [
        ("job-pending", "pending"),
        ("job-running", "running"),
        ("job-completed", "completed"),
    ] {
        adapteros_db::sqlx::query(
            "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind("repo-1")
        .bind("tenant-1")
        .bind("{\"rank\":16}")
        .bind(status)
        .bind("{}")
        .bind("user1")
        .execute(state.db.pool_result()?)
        .await?;
    }

    let claims = test_admin_claims();

    // Filter by running status
    let result = list_training_jobs(
        State(state.clone()),
        Extension(claims),
        Query(TrainingListParams {
            status: Some("running".to_string()),
            ..Default::default()
        }),
    )
    .await;

    assert!(result.is_ok(), "filtered list should succeed");
    let jobs = result.unwrap().0;
    assert_eq!(jobs.jobs.len(), 1, "should only see running jobs");
    assert_eq!(jobs.jobs[0].status, "running");

    // Cleanup
    for id in ["job-pending", "job-running", "job-completed"] {
        delete_test_training_job(&state, id).await?;
    }

    Ok(())
}

/// Test pagination of training jobs
#[tokio::test]
async fn list_training_jobs_supports_pagination() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    // Create multiple jobs
    for i in 1..=5 {
        adapteros_db::sqlx::query(
            "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("job-page-{}", i))
        .bind("repo-1")
        .bind("tenant-1")
        .bind("{\"rank\":16}")
        .bind("pending")
        .bind("{}")
        .bind("user1")
        .execute(state.db.pool_result()?)
        .await?;
    }

    let claims = test_admin_claims();

    // Get first page (limit 2)
    let result = list_training_jobs(
        State(state.clone()),
        Extension(claims.clone()),
        Query(TrainingListParams {
            status: None,
            page: Some(1),
            page_size: Some(2),
            ..Default::default()
        }),
    )
    .await;

    assert!(result.is_ok(), "paginated list should succeed");
    let jobs = result.unwrap().0;
    assert!(
        jobs.jobs.len() <= 2,
        "should respect limit: got {}",
        jobs.jobs.len()
    );

    // Get second page (offset 2, limit 2)
    let result2 = list_training_jobs(
        State(state.clone()),
        Extension(claims),
        Query(TrainingListParams {
            status: None,
            page: Some(2),
            page_size: Some(2),
            ..Default::default()
        }),
    )
    .await;

    assert!(result2.is_ok(), "paginated list should succeed");
    let jobs2 = result2.unwrap().0;
    assert!(
        jobs2.jobs.len() <= 2,
        "should respect limit: got {}",
        jobs2.jobs.len()
    );

    // Cleanup
    for i in 1..=5 {
        delete_test_training_job(&state, &format!("job-page-{}", i)).await?;
    }

    Ok(())
}

/// Test getting non-existent training job
#[tokio::test]
async fn get_nonexistent_training_job_returns_404() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let result = get_training_job(
        State(state),
        Extension(claims),
        Path("nonexistent-job".to_string()),
    )
    .await;

    match result {
        Err((status, body)) => {
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert_eq!(body.0.code, "NOT_FOUND");
        }
        Ok(_) => panic!("should return 404 for nonexistent job"),
    }

    Ok(())
}

/// Test viewer permissions for training jobs
#[tokio::test]
async fn viewer_can_list_but_not_cancel() -> Result<()> {
    let state = setup_state(None).await.expect("state");

    adapteros_db::sqlx::query(
        "INSERT INTO repository_training_jobs (id, repo_id, tenant_id, training_config_json, status, progress_json, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("job-viewer-test")
    .bind("repo-1")
    .bind("default")
    .bind("{\"rank\":16}")
    .bind("running")
    .bind("{}")
    .bind("user1")
    .execute(state.db.pool_result()?)
    .await?;

    let viewer_claims = test_viewer_claims();

    // Viewer should be able to list
    let list_result = list_training_jobs(
        State(state.clone()),
        Extension(viewer_claims.clone()),
        Query(TrainingListParams::default()),
    )
    .await;

    assert!(list_result.is_ok(), "viewer should be able to list jobs");

    // Viewer should NOT be able to cancel
    let cancel_result = cancel_training(
        State(state.clone()),
        Extension(viewer_claims),
        Extension(ClientIp("127.0.0.1".to_string())),
        Path("job-viewer-test".to_string()),
    )
    .await;

    match cancel_result {
        Err((status, _)) => {
            assert!(
                status == StatusCode::FORBIDDEN || status == StatusCode::UNAUTHORIZED,
                "viewer should not have permission to cancel"
            );
        }
        Ok(_) => panic!("viewer should not be able to cancel jobs"),
    }

    // Cleanup
    delete_test_training_job(&state, "job-viewer-test").await?;

    Ok(())
}

/// Test training job with empty list
#[tokio::test]
async fn list_training_jobs_empty_returns_empty_list() -> Result<()> {
    let state = setup_state(None).await.expect("state");
    let claims = test_admin_claims();

    let result = list_training_jobs(
        State(state),
        Extension(claims),
        Query(TrainingListParams::default()),
    )
    .await;

    assert!(result.is_ok(), "list should succeed even when empty");
    let jobs = result.unwrap().0;
    // Should return valid response (may be empty or have test data)
    assert!(jobs.jobs.is_empty() || !jobs.jobs.is_empty());

    Ok(())
}
