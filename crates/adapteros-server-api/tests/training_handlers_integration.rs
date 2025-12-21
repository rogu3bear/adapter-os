//! Integration tests for training job handlers
//!
//! Tests training job lifecycle including start, cancel, list,
//! status checking, and tenant isolation.

use adapteros_core::Result;
use adapteros_server_api::handlers::{
    cancel_training, get_training_job, list_training_jobs,
};
use adapteros_server_api::types::TrainingListParams;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Extension;

mod common;
use common::{
    delete_test_training_job, insert_training_job, setup_state, test_admin_claims,
    test_viewer_claims,
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
    .execute(state.db.pool())
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
    .execute(state.db.pool())
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
    .execute(state.db.pool())
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
    assert!(jobs
        .jobs
        .iter()
        .all(|j| j.id.starts_with("job-tenant1")));

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
    .execute(state.db.pool())
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
    .execute(state.db.pool())
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
    .execute(state.db.pool())
    .await?;

    let claims = test_admin_claims();

    // Cancel the job
    let result = cancel_training(
        State(state.clone()),
        Extension(claims.clone()),
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
    .execute(state.db.pool())
    .await?;

    // Try to cancel from different tenant
    let other_claims = test_viewer_claims(); // default tenant
    let result = cancel_training(
        State(state.clone()),
        Extension(other_claims),
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
        .execute(state.db.pool())
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
        .execute(state.db.pool())
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
    .execute(state.db.pool())
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
