//! Regression test for training capacity check against the correct table.

mod common;

use std::sync::Arc;

use adapteros_db::sqlx;
use adapteros_server_api::services::training_service::{DefaultTrainingService, TrainingService};
use common::{insert_training_job, setup_state};

#[tokio::test]
async fn check_training_capacity_counts_running_jobs() {
    let state = setup_state(None).await.expect("setup state");

    // Ensure FK to git_repositories passes for inserted training jobs.
    // Ensure repo_id satisfies FK expectations by adding a unique index for the test DB.
    sqlx::query("CREATE UNIQUE INDEX IF NOT EXISTS idx_git_repositories_repo_unique ON git_repositories(repo_id)")
        .execute(state.db.pool())
        .await
        .expect("create repo_id unique index");

    sqlx::query(
        "INSERT INTO git_repositories (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("repo-1-id")
    .bind("repo-1")
    .bind("/tmp/repo-1")
    .bind("main")
    .bind("{}")
    .bind("{}")
    .bind("{}")
    .bind("analyzed")
    .bind("tester")
    .execute(state.db.pool())
    .await
    .expect("insert repo");

    // Two running jobs and one completed to exercise the running count.
    insert_training_job(&state, "job-running-1", "running")
        .await
        .expect("insert running job 1");
    insert_training_job(&state, "job-running-2", "running")
        .await
        .expect("insert running job 2");
    insert_training_job(&state, "job-completed-1", "completed")
        .await
        .expect("insert completed job");

    let service = DefaultTrainingService::new(Arc::new(state));
    let capacity = service
        .check_training_capacity()
        .await
        .expect("capacity check should succeed");

    assert_eq!(capacity.running_jobs, 2);
    assert_eq!(capacity.max_concurrent_jobs, 5);
    assert_eq!(capacity.available_slots, 3);
    assert!(capacity.can_start_new_job);
    assert_eq!(capacity.memory_pressure, "low");
}
