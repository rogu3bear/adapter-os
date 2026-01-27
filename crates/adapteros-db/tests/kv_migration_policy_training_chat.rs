#![allow(clippy::too_many_arguments)]

use adapteros_core::{B3Hash, Result};
use adapteros_db::kv_migration::{MigrationDomain, MigrationOptions};
use adapteros_db::{Db, StorageMode};
use chrono::Utc;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-").expect("tempdir")
}

fn audit_hash(
    id: &str,
    timestamp: &str,
    tenant_id: &str,
    policy_pack_id: &str,
    hook: &str,
    decision: &str,
    reason: Option<&str>,
    request_id: Option<&str>,
    user_id: Option<&str>,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    metadata_json: Option<&str>,
    previous_hash: Option<&str>,
) -> String {
    let entry_data = format!(
        "{id}|{timestamp}|{tenant_id}|{policy_pack_id}|{hook}|{decision}|{}|{}|{}|{}|{}|{}|{}",
        reason.unwrap_or(""),
        request_id.unwrap_or(""),
        user_id.unwrap_or(""),
        resource_type.unwrap_or(""),
        resource_id.unwrap_or(""),
        metadata_json.unwrap_or(""),
        previous_hash.unwrap_or("")
    );
    B3Hash::hash(entry_data.as_bytes()).to_string()
}

async fn seed_policy_audit(db: &Db, tenant_id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let id1 = "audit-1";
    let hash1 = audit_hash(
        id1,
        &now,
        tenant_id,
        "pack-1",
        "hook-a",
        "allow",
        Some("ok"),
        Some("req-1"),
        None,
        None,
        None,
        None,
        None,
    );
    sqlx::query(
        r#"INSERT INTO policy_audit_decisions
           (id, tenant_id, policy_pack_id, hook, decision, reason, request_id, user_id,
            resource_type, resource_id, metadata_json, timestamp, entry_hash, previous_hash, chain_sequence)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(id1)
    .bind(tenant_id)
    .bind("pack-1")
    .bind("hook-a")
    .bind("allow")
    .bind("ok")
    .bind("req-1")
    .bind::<Option<String>>(None)
    .bind::<Option<String>>(None)
    .bind::<Option<String>>(None)
    .bind::<Option<String>>(None)
    .bind(&now)
    .bind(&hash1)
    .bind::<Option<String>>(None)
    .bind(1_i64)
    .execute(db.pool())
    .await?;

    let id2 = "audit-2";
    let hash2 = audit_hash(
        id2,
        &now,
        tenant_id,
        "pack-1",
        "hook-b",
        "deny",
        Some("blocked"),
        Some("req-2"),
        Some("user-1"),
        Some("resource"),
        Some("res-1"),
        Some(r#"{"k":"v"}"#),
        Some(&hash1),
    );
    sqlx::query(
        r#"INSERT INTO policy_audit_decisions
           (id, tenant_id, policy_pack_id, hook, decision, reason, request_id, user_id,
            resource_type, resource_id, metadata_json, timestamp, entry_hash, previous_hash, chain_sequence)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(id2)
    .bind(tenant_id)
    .bind("pack-1")
    .bind("hook-b")
    .bind("deny")
    .bind("blocked")
    .bind("req-2")
    .bind("user-1")
    .bind("resource")
    .bind("res-1")
    .bind(r#"{"k":"v"}"#)
    .bind(&now)
    .bind(&hash2)
    .bind(&hash1)
    .bind(2_i64)
    .execute(db.pool())
    .await?;

    Ok(())
}

async fn seed_training_job(db: &Db, tenant_id: &str) -> Result<()> {
    // git_repositories is referenced by repository_training_jobs.repo_id
    sqlx::query(
        r#"INSERT INTO git_repositories
           (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
           VALUES ('git-1', 'repo-1', 'var/repo', 'main', '{}', '{}', '{}', 'ready', 'admin-user')"#,
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        r#"INSERT INTO repository_training_jobs
           (id, repo_id, training_config_json, status, progress_json, started_at, completed_at,
            created_by, adapter_name, template_id, created_at, metadata_json, config_hash_b3,
            dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
            retryable, retry_of_job_id, stack_id, adapter_id)
           VALUES
           ('job-1', 'repo-1', '{"lr":1}', 'running', '{"p":0.1}', datetime('now'), NULL,
            'admin-user', 'adapter-a', NULL, datetime('now'), NULL, NULL,
            NULL, NULL, NULL, ?, NULL, NULL,
            0, NULL, NULL, NULL)"#,
    )
    .bind(tenant_id)
    .execute(db.pool())
    .await?;

    sqlx::query(
        r#"INSERT INTO repository_training_metrics
           (id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp)
           VALUES ('metric-1', 'job-1', 1, 0, 'loss', 0.5, datetime('now'))"#,
    )
    .execute(db.pool())
    .await?;

    Ok(())
}

async fn seed_chat(db: &Db, tenant_id: &str) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"INSERT INTO chat_sessions
           (id, tenant_id, name, title, source_type, status, created_at, updated_at, last_activity_at, is_shared)
           VALUES ('chat-1', ?, 'Chat One', 'Chat One', 'general', 'active', ?, ?, ?, 0)"#,
    )
    .bind(tenant_id)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await?;

    sqlx::query(
        r#"INSERT INTO chat_messages
           (id, session_id, tenant_id, role, content, timestamp, created_at, sequence, metadata_json)
           VALUES ('msg-1', 'chat-1', ?, 'user', 'hello', ?, ?, 0, NULL)"#,
    )
    .bind(tenant_id)
    .bind(&now)
    .bind(&now)
    .execute(db.pool())
    .await?;

    Ok(())
}

#[tokio::test]
async fn migrate_policy_training_chat_to_kv_and_diff_clean() -> Result<()> {
    let tmp = new_test_tempdir();
    let db_path = tmp.path().join("aos-cp.sqlite3");
    let kv_path = tmp.path().join("aos-kv.redb");

    let mut db = Db::connect(db_path.to_string_lossy().as_ref()).await?;
    db.migrate().await?;
    db.seed_dev_data().await?;
    db.init_kv_backend(&kv_path)?;
    db.set_storage_mode(StorageMode::SqlOnly)?;

    // Relax FK checks for synthetic fixture inserts
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(db.pool())
        .await?;

    let tenant_id = "default";
    seed_policy_audit(&db, tenant_id).await?;
    seed_training_job(&db, tenant_id).await?;
    seed_chat(&db, tenant_id).await?;

    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(db.pool())
        .await?;

    let domains = vec![
        MigrationDomain::PolicyAudit,
        MigrationDomain::TrainingJobs,
        MigrationDomain::ChatSessions,
    ];
    let opts = MigrationOptions {
        batch_size: 200,
        dry_run: false,
        tenant_filter: None,
        checkpoint: None,
    };
    let (results, _) = db.migrate_domains(&domains, &opts).await?;
    for (domain, stats) in &results {
        assert_eq!(stats.failed, 0, "domain {} failed", domain.label());
    }

    let mut issues = Vec::new();
    issues.extend(db.diff_policy_audit().await?);
    issues.extend(db.diff_training_jobs().await?);
    issues.extend(db.diff_chat_sessions().await?);
    assert!(
        issues.is_empty(),
        "Expected no SQL↔KV drift, got: {:?}",
        issues
    );

    Ok(())
}
