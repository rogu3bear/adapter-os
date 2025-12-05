use adapteros_db::{global_kv_metrics, users::Role, Db, StorageMode};
use anyhow::Result;
use tempfile::tempdir;

#[tokio::test]
async fn kv_primary_smoke_load() -> Result<()> {
    let tmp = tempdir()?;
    let kv_path = tmp.path().join("kv.redb");

    let mut db = Db::new_in_memory().await?;
    db.init_kv_backend(&kv_path)?;
    db.set_storage_mode(StorageMode::KvPrimary)?;
    db.clear_degraded();

    let mut tenant_ids = Vec::new();
    for i in 0..5 {
        let id = db.create_tenant(&format!("tenant-{i}"), false).await?;
        tenant_ids.push(id);
    }

    for (idx, tenant_id) in tenant_ids.iter().enumerate() {
        let email = format!("user{idx}@example.com");
        db.create_user(
            &email,
            &format!("User {idx}"),
            "hash",
            Role::Admin,
            tenant_id,
        )
        .await?;
    }

    let tenants = db.list_tenants().await?;
    assert_eq!(tenants.len(), 5);

    let (users, total) = db.list_users(1, 50, None, None).await?;
    assert_eq!(users.len(), 5);
    assert_eq!(total, 5);

    let metrics = global_kv_metrics().snapshot();
    assert_eq!(metrics.fallback_operations_total, 0);

    Ok(())
}
