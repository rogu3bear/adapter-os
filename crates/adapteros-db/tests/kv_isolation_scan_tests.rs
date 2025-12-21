use adapteros_db::{
    kv_isolation_scan::{KvIsolationIssue, KvIsolationScanConfig},
    messages_kv::MessageKv,
    policy_audit_kv::PolicyAuditKvRepository,
    tenants_kv::TenantKvRepository,
    Db, KvDb, StorageMode,
};
use adapteros_storage::entities::tenant::TenantKv;
use chrono::Utc;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

async fn setup_db() -> adapteros_core::Result<(Db, SqlitePool)> {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePool::connect_with(options).await?;

    // Minimal schema fragments used by the scan
    sqlx::query(
        r#"
        CREATE TABLE tenants (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            itar_flag INTEGER NOT NULL DEFAULT 0,
            status TEXT DEFAULT 'active',
            default_stack_id TEXT
        );
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE messages (
            id TEXT PRIMARY KEY,
            workspace_id TEXT NOT NULL,
            from_user_id TEXT NOT NULL,
            from_tenant_id TEXT NOT NULL,
            content TEXT NOT NULL,
            thread_id TEXT
        );
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE policy_audit_decisions (
            id TEXT PRIMARY KEY,
            tenant_id TEXT NOT NULL,
            policy_pack_id TEXT,
            hook TEXT,
            decision TEXT,
            chain_sequence INTEGER NOT NULL
        );
        "#,
    )
    .execute(&pool)
    .await?;

    let kv = KvDb::init_in_memory()?;
    let mut db = Db::new(pool.clone(), None, StorageMode::SqlOnly);
    db.attach_kv_backend(kv);
    db.set_storage_mode(StorageMode::DualWrite)?;

    Ok((db, pool))
}

#[tokio::test]
async fn detects_cross_tenant_message_mismatch() -> adapteros_core::Result<()> {
    let (db, pool) = setup_db().await?;

    // SQL ground truth
    sqlx::query(
        r#"
        INSERT INTO messages (id, workspace_id, from_user_id, from_tenant_id, content)
        VALUES (?1, ?2, ?3, ?4, 'hi')
        "#,
    )
    .bind("msg-1")
    .bind("ws-1")
    .bind("user-1")
    .bind("tenant-sql")
    .execute(&pool)
    .await?;

    // KV entry with wrong tenant id
    let kv_backend = db.kv_backend().unwrap().backend().clone();
    let msg_repo = adapteros_db::messages_kv::MessageKvRepository::new(kv_backend.clone());
    msg_repo
        .put(&MessageKv {
            id: "msg-1".to_string(),
            workspace_id: "ws-1".to_string(),
            from_user_id: "user-1".to_string(),
            from_tenant_id: "tenant-kv".to_string(),
            content: "hi".to_string(),
            thread_id: None,
            created_at: "now".to_string(),
            edited_at: None,
        })
        .await?;

    let report = db
        .run_kv_isolation_scan(KvIsolationScanConfig::default())
        .await?;

    assert_eq!(report.findings.len(), 1);
    let finding = &report.findings[0];
    assert_eq!(finding.domain, "messages_kv");
    assert!(matches!(
        finding.issue,
        KvIsolationIssue::CrossTenantMismatch { .. }
    ));

    Ok(())
}

#[tokio::test]
async fn healthy_records_produce_no_findings() -> adapteros_core::Result<()> {
    let (db, pool) = setup_db().await?;

    // SQL ground truth
    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, itar_flag, status) VALUES (?1, 'Tenant', 0, 'active')
        "#,
    )
    .bind("tenant-1")
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO messages (id, workspace_id, from_user_id, from_tenant_id, content)
        VALUES (?1, ?2, ?3, ?4, 'ok')
        "#,
    )
    .bind("msg-ok")
    .bind("ws-1")
    .bind("user-1")
    .bind("tenant-1")
    .execute(&pool)
    .await?;

    let kv_backend = db.kv_backend().unwrap().backend().clone();
    let tenant_repo = TenantKvRepository::new(kv_backend.clone());
    tenant_repo
        .put_tenant(&TenantKv {
            id: "tenant-1".to_string(),
            name: "Tenant".to_string(),
            itar_flag: false,
            status: "active".to_string(),
            default_stack_id: None,
            default_pinned_adapter_ids: None,
            max_adapters: None,
            max_training_jobs: None,
            max_storage_gb: None,
            rate_limit_rpm: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await?;

    let msg_repo = adapteros_db::messages_kv::MessageKvRepository::new(kv_backend.clone());
    msg_repo
        .put(&MessageKv {
            id: "msg-ok".to_string(),
            workspace_id: "ws-1".to_string(),
            from_user_id: "user-1".to_string(),
            from_tenant_id: "tenant-1".to_string(),
            content: "ok".to_string(),
            thread_id: None,
            created_at: "now".to_string(),
            edited_at: None,
        })
        .await?;

    let audit_repo = PolicyAuditKvRepository::new(kv_backend);
    let kv_audit_id = audit_repo
        .log_policy_decision(
            "tenant-1",
            "isolation",
            "kv.scan",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await?;

    // Mirror KV entry into SQL for parity
    sqlx::query(
        r#"
        INSERT INTO policy_audit_decisions (id, tenant_id, policy_pack_id, hook, decision, chain_sequence)
        VALUES (?1, ?2, 'isolation', 'kv.scan', 'allow', 1)
        "#,
    )
    .bind(&kv_audit_id)
    .bind("tenant-1")
    .execute(&pool)
    .await?;

    let report = db
        .run_kv_isolation_scan(KvIsolationScanConfig::default())
        .await?;

    assert!(report.findings.is_empty());

    Ok(())
}
