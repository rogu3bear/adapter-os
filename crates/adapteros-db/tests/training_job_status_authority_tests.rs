#![allow(clippy::expect_fun_call)]

use adapteros_db::training_jobs_kv::TrainingJobKvRepository;
use adapteros_db::{Db, KvDb, StorageMode};
use std::sync::Arc;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-")
        .expect("Failed to create temporary directory for training status authority test")
}

async fn create_dual_write_db() -> (Db, TempDir) {
    let temp_dir = new_test_tempdir();
    let db_path = temp_dir.path().join("test.db");
    let kv_path = temp_dir.path().join("kv.redb");

    let db_sql = Db::connect(db_path.to_str().expect("db path should be UTF-8"))
        .await
        .expect("Failed to connect SQL DB");
    db_sql.migrate().await.expect("Failed to apply migrations");
    db_sql
        .seed_dev_data()
        .await
        .expect("Failed to seed dev data");

    let kv_db = KvDb::init_redb(&kv_path).expect("Failed to initialize KV backend");
    let pool = db_sql
        .pool_result()
        .expect("db pool should be available")
        .clone();
    let db = Db::new(pool, Some(Arc::new(kv_db)), StorageMode::DualWrite);

    (db, temp_dir)
}

#[tokio::test]
async fn get_training_job_prefers_terminal_sql_state_when_kv_is_stale() {
    let (mut db, temp_dir) = create_dual_write_db().await;
    let repo_id = "repo-status-authority";
    let repo_path = temp_dir.path().join("repo-status-authority");

    db.create_git_repository(
        "unused",
        repo_id,
        repo_path.to_str().expect("path UTF-8"),
        "main",
        "{}",
        "tester",
    )
    .await
    .expect("Failed to create git repository fixture");

    let job_id = db
        .create_training_job_with_provenance(
            None,
            repo_id,
            "{\"epochs\":1}",
            "tester",
            None,
            None,
            None,
            None,
            None,
            None,
            Some("default"),
            None,
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
        .await
        .expect("Failed to create training job fixture");

    db.set_storage_mode(StorageMode::KvPrimary)
        .expect("Failed to set KvPrimary mode");

    let kv = db.kv_backend().expect("KV backend should be attached");
    let kv_repo = TrainingJobKvRepository::new(kv.backend().clone());

    kv_repo
        .update_job(&job_id, |job| {
            job.status = "running".to_string();
            job.progress_json = serde_json::json!({
                "progress_pct": 50.0,
                "current_epoch": 1,
                "total_epochs": 3,
                "current_loss": 0.9,
                "learning_rate": 0.0001,
                "tokens_per_second": 12.0,
                "error_message": null
            })
            .to_string();
        })
        .await
        .expect("Failed to update KV job fixture");

    let sql_progress = serde_json::json!({
        "progress_pct": 100.0,
        "current_epoch": 1,
        "total_epochs": 1,
        "current_loss": 0.0,
        "learning_rate": 0.0001,
        "tokens_per_second": 0.0,
        "error_message": "sql terminal failure",
        "error_code": "TRAINING_EXECUTION_FAILED"
    })
    .to_string();

    sqlx::query(
        "UPDATE repository_training_jobs
         SET status = 'failed', progress_json = ?, completed_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&sql_progress)
    .bind(&job_id)
    .execute(db.pool_result().expect("db pool should be available"))
    .await
    .expect("Failed to force SQL terminal state");

    let resolved = db
        .get_training_job(&job_id)
        .await
        .expect("Failed to read training job")
        .expect("Training job should exist");

    assert_eq!(resolved.status, "failed");
    assert!(
        resolved.progress_json.contains("sql terminal failure"),
        "terminal SQL progress payload should win over stale KV state"
    );
}

#[tokio::test]
async fn get_training_job_keeps_terminal_kv_state_when_sql_is_non_terminal() {
    let (mut db, temp_dir) = create_dual_write_db().await;
    let repo_id = "repo-status-authority-kv-terminal";
    let repo_path = temp_dir.path().join("repo-status-authority-kv-terminal");

    db.create_git_repository(
        "unused",
        repo_id,
        repo_path.to_str().expect("path UTF-8"),
        "main",
        "{}",
        "tester",
    )
    .await
    .expect("Failed to create git repository fixture");

    let job_id = db
        .create_training_job_with_provenance(
            None,
            repo_id,
            "{\"epochs\":1}",
            "tester",
            None,
            None,
            None,
            None,
            None,
            None,
            Some("default"),
            None,
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
        .await
        .expect("Failed to create training job fixture");

    db.set_storage_mode(StorageMode::KvPrimary)
        .expect("Failed to set KvPrimary mode");

    let kv = db.kv_backend().expect("KV backend should be attached");
    let kv_repo = TrainingJobKvRepository::new(kv.backend().clone());

    kv_repo
        .update_job(&job_id, |job| {
            job.status = "failed".to_string();
            job.progress_json = serde_json::json!({
                "progress_pct": 100.0,
                "current_epoch": 1,
                "total_epochs": 1,
                "current_loss": 0.0,
                "learning_rate": 0.0001,
                "tokens_per_second": 0.0,
                "error_message": "kv terminal failure",
                "error_code": "TRAINING_EXECUTION_FAILED"
            })
            .to_string();
        })
        .await
        .expect("Failed to update KV terminal fixture");

    let sql_progress = serde_json::json!({
        "progress_pct": 25.0,
        "current_epoch": 1,
        "total_epochs": 4,
        "current_loss": 0.5,
        "learning_rate": 0.0001,
        "tokens_per_second": 10.0,
        "error_message": null
    })
    .to_string();

    sqlx::query(
        "UPDATE repository_training_jobs
         SET status = 'running', progress_json = ?, completed_at = NULL
         WHERE id = ?",
    )
    .bind(&sql_progress)
    .bind(&job_id)
    .execute(db.pool_result().expect("db pool should be available"))
    .await
    .expect("Failed to force SQL non-terminal state");

    let resolved = db
        .get_training_job(&job_id)
        .await
        .expect("Failed to read training job")
        .expect("Training job should exist");

    assert_eq!(resolved.status, "failed");
    assert!(
        resolved.progress_json.contains("kv terminal failure"),
        "terminal KV status should remain authoritative when SQL is non-terminal"
    );
}
