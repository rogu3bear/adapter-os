use adapteros_core::AosError;
use adapteros_db::kv_backend::KvDb;
use adapteros_db::{Db, StorageMode};

async fn insert_default_tenant(db: &Db) {
    sqlx::query(
        r#"
        INSERT INTO tenants (id, name, created_at)
        VALUES ('default-tenant', 'Default', datetime('now'))
        ON CONFLICT(id) DO NOTHING
        "#,
    )
    .execute(db.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn policy_audit_fails_when_kv_chain_missing() {
    let mut db = Db::new_in_memory().await.unwrap();
    insert_default_tenant(&db).await;

    let kv = KvDb::init_in_memory().unwrap();
    let backend = kv.backend().clone();
    db.attach_kv_backend(kv);
    db.set_storage_mode(StorageMode::DualWrite).unwrap();

    // First entry succeeds
    let first_id = db
        .log_policy_decision(
            "default-tenant",
            "pack1",
            "hook1",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    // Delete KV entry and seq to simulate drift
    let entry_key = format!("tenant/{}/policy_audit/{}", "default-tenant", first_id);
    let seq_key = format!(
        "tenant/{}/policy_audit/seq/{:020}:{}",
        "default-tenant", 1, first_id
    );
    let _ = backend.delete(&entry_key).await;
    let _ = backend.delete(&seq_key).await;

    // Second write should fail closed
    let err = db
        .log_policy_decision(
            "default-tenant",
            "pack1",
            "hook1",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap_err();
    match err {
        AosError::Validation(msg) => {
            assert!(
                msg.contains("missing prior entries") || msg.contains("out of sync"),
                "unexpected validation message: {}",
                msg
            );
        }
        other => panic!("expected validation error, got {:?}", other),
    }
}
