use adapteros_db::kv_backend::KvDb;
use adapteros_db::{adapters::AdapterRegistrationBuilder, Db, StorageMode};
use adapteros_storage::AdapterRepository as StorageAdapterRepo;

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
async fn detects_missing_kv_entry() {
    let mut db = Db::new_in_memory().await.unwrap();
    insert_default_tenant(&db).await;

    let kv = KvDb::init_in_memory().unwrap();
    db.attach_kv_backend(kv.clone());
    db.set_storage_mode(StorageMode::DualWrite).unwrap();

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("kv-missing")
        .name("KV Missing")
        .hash_b3("b3:missing")
        .rank(4)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    // Delete KV entry to simulate drift
    let repo = StorageAdapterRepo::new(kv.backend().clone(), kv.index_manager().clone());
    let _ = repo.delete("default-tenant", "kv-missing").await.ok();

    let status = db.check_adapter_consistency("kv-missing").await.unwrap();
    assert!(!status.kv_present);
    assert!(!status.is_ready());
    assert_eq!(status.message.as_deref(), Some("KV adapter missing"));
}

#[tokio::test]
async fn detects_hash_mismatch() {
    let mut db = Db::new_in_memory().await.unwrap();
    insert_default_tenant(&db).await;

    let kv = KvDb::init_in_memory().unwrap();
    db.attach_kv_backend(kv.clone());
    db.set_storage_mode(StorageMode::DualWrite).unwrap();

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("kv-mismatch")
        .name("KV Mismatch")
        .hash_b3("b3:original")
        .rank(4)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();
    db.register_adapter(params).await.unwrap();

    // Tamper KV hash to force mismatch
    let repo = StorageAdapterRepo::new(kv.backend().clone(), kv.index_manager().clone());
    let mut kv_adapter = repo
        .get("default-tenant", "kv-mismatch")
        .await
        .unwrap()
        .unwrap();
    kv_adapter.hash_b3 = "b3:tampered".to_string();
    repo.update(kv_adapter).await.unwrap();

    let status = db.check_adapter_consistency("kv-mismatch").await.unwrap();
    assert!(status.kv_present);
    assert!(!status.hash_match);
    assert!(!status.is_ready());
    assert_eq!(
        status.message.as_deref(),
        Some("hash_b3 mismatch between SQL and KV")
    );
}
