use adapteros_core::{AosError, Result};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;

async fn create_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to create tenant: {}", e)))?;
    Ok(())
}

async fn register_adapter(db: &Db, tenant_id: &str, adapter_id: &str, hash_b3: &str) -> Result<()> {
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(adapter_id)
        .name(adapter_id)
        .hash_b3(hash_b3)
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()?;

    db.register_adapter(params).await?;
    Ok(())
}

#[tokio::test]
async fn adapter_hash_update_repairs_existing_row_for_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    register_adapter(&db, "tenant-a", "adapter-a", "b3:hash-old").await?;

    let original_updated_at = "2000-01-01 00:00:00";
    sqlx::query("UPDATE adapters SET updated_at = ? WHERE tenant_id = ? AND adapter_id = ?")
        .bind(original_updated_at)
        .bind("tenant-a")
        .bind("adapter-a")
        .execute(db.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

    db.update_adapter_weight_hash_for_tenant("tenant-a", "adapter-a", "b3:hash-new")
        .await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "adapter-a")
        .await?
        .expect("adapter should exist");

    assert_eq!(adapter.hash_b3, "b3:hash-new");
    assert_eq!(adapter.content_hash_b3.as_deref(), Some("b3:hash-new"));

    let updated_at: String = sqlx::query_scalar(
        "SELECT updated_at FROM adapters WHERE tenant_id = ? AND adapter_id = ?",
    )
    .bind("tenant-a")
    .bind("adapter-a")
    .fetch_one(db.pool())
    .await
    .map_err(|e| AosError::database(e.to_string()))?;
    assert_ne!(updated_at, original_updated_at);

    Ok(())
}

#[tokio::test]
async fn adapter_hash_update_does_not_cross_tenant_boundaries() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    register_adapter(&db, "tenant-a", "adapter-a", "b3:hash-a-old").await?;
    register_adapter(&db, "tenant-b", "adapter-b", "b3:hash-b-old").await?;

    let err = db
        .update_adapter_weight_hash_for_tenant("tenant-b", "adapter-a", "b3:hash-cross")
        .await
        .expect_err("cross-tenant update must not succeed");
    assert!(
        matches!(err, AosError::NotFound(ref msg) if msg.contains("adapter-a") && msg.contains("tenant-b")),
        "expected tenant-scoped not-found, got: {err}"
    );

    let tenant_a_adapter = db
        .get_adapter_for_tenant("tenant-a", "adapter-a")
        .await?
        .expect("tenant-a adapter should exist");
    assert_eq!(tenant_a_adapter.hash_b3, "b3:hash-a-old");
    assert_eq!(
        tenant_a_adapter.content_hash_b3.as_deref(),
        Some("b3:hash-a-old")
    );

    let tenant_b_adapter = db
        .get_adapter_for_tenant("tenant-b", "adapter-b")
        .await?
        .expect("tenant-b adapter should exist");
    assert_eq!(tenant_b_adapter.hash_b3, "b3:hash-b-old");
    assert_eq!(
        tenant_b_adapter.content_hash_b3.as_deref(),
        Some("b3:hash-b-old")
    );

    Ok(())
}

#[tokio::test]
async fn adapter_hash_update_with_duplicate_adapter_id_is_tenant_scoped() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    register_adapter(&db, "tenant-a", "adapter-shared", "b3:hash-a-old").await?;
    register_adapter(&db, "tenant-b", "adapter-shared", "b3:hash-b-old").await?;

    let tenant_a_original_updated_at = "2000-01-01 00:00:00";
    let tenant_b_original_updated_at = "2000-01-02 00:00:00";
    sqlx::query("UPDATE adapters SET updated_at = ? WHERE tenant_id = ? AND adapter_id = ?")
        .bind(tenant_a_original_updated_at)
        .bind("tenant-a")
        .bind("adapter-shared")
        .execute(db.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;
    sqlx::query("UPDATE adapters SET updated_at = ? WHERE tenant_id = ? AND adapter_id = ?")
        .bind(tenant_b_original_updated_at)
        .bind("tenant-b")
        .bind("adapter-shared")
        .execute(db.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

    db.update_adapter_weight_hash_for_tenant("tenant-a", "adapter-shared", "b3:hash-a-new")
        .await?;

    let tenant_a_adapter = db
        .get_adapter_for_tenant("tenant-a", "adapter-shared")
        .await?
        .expect("tenant-a adapter should exist");
    assert_eq!(tenant_a_adapter.hash_b3, "b3:hash-a-new");
    assert_eq!(
        tenant_a_adapter.content_hash_b3.as_deref(),
        Some("b3:hash-a-new")
    );

    let tenant_b_adapter = db
        .get_adapter_for_tenant("tenant-b", "adapter-shared")
        .await?
        .expect("tenant-b adapter should exist");
    assert_eq!(tenant_b_adapter.hash_b3, "b3:hash-b-old");
    assert_eq!(
        tenant_b_adapter.content_hash_b3.as_deref(),
        Some("b3:hash-b-old")
    );

    let tenant_a_updated_at: String = sqlx::query_scalar(
        "SELECT updated_at FROM adapters WHERE tenant_id = ? AND adapter_id = ?",
    )
    .bind("tenant-a")
    .bind("adapter-shared")
    .fetch_one(db.pool())
    .await
    .map_err(|e| AosError::database(e.to_string()))?;
    assert_ne!(tenant_a_updated_at, tenant_a_original_updated_at);

    let tenant_b_updated_at: String = sqlx::query_scalar(
        "SELECT updated_at FROM adapters WHERE tenant_id = ? AND adapter_id = ?",
    )
    .bind("tenant-b")
    .bind("adapter-shared")
    .fetch_one(db.pool())
    .await
    .map_err(|e| AosError::database(e.to_string()))?;
    assert_eq!(tenant_b_updated_at, tenant_b_original_updated_at);

    Ok(())
}
