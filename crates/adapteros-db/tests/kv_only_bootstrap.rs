use adapteros_db::users::Role;
use adapteros_db::{Db, KvDb, StorageMode};
use tempfile::TempDir;

#[tokio::test]
async fn kv_only_bootstrap_creates_system_tenant_and_policies() -> adapteros_core::Result<()> {
    let tmp = TempDir::new().unwrap();
    let kv_path = tmp.path().join("kv.redb");
    let kv = KvDb::init_redb(kv_path.as_path())?;
    let mut db = Db::new_kv_only(Some(std::sync::Arc::new(kv)), StorageMode::KvOnly);

    // Guard should allow kv-only when coverage present
    db.enforce_kv_only_guard()?;

    // Ensure system tenant and core policies in KV-only mode
    db.ensure_system_tenant().await?;
    let system = db.get_tenant("system").await?;
    assert!(
        system.is_some(),
        "system tenant should be created in KV-only"
    );

    let active = db.get_active_policies_for_tenant("system").await?;
    assert_eq!(active.len(), 4, "core policies should be enabled");
    assert!(active.contains(&"egress".to_string()));
    assert!(active.contains(&"determinism".to_string()));
    assert!(active.contains(&"isolation".to_string()));
    assert!(active.contains(&"evidence".to_string()));

    // User creation should work without SQL pool
    let user_id = db
        .create_user(
            "kv-only-admin@example.com",
            "KV Admin",
            "pw_hash",
            Role::Admin,
            "system",
        )
        .await?;
    assert!(!user_id.is_empty());

    Ok(())
}
