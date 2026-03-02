//! Adapter lifecycle tenant isolation tests
//!
//! These tests validate that tenant-scoped adapter queries enforce tenant boundaries
//! at the database layer (defense-in-depth), and do not rely on handler-level checks.

use adapteros_core::{AosError, Result};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;

async fn create_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool_result()?)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;
    Ok(())
}

async fn register_adapter(
    db: &Db,
    tenant_id: &str,
    adapter_id: &str,
    hash_b3: &str,
    expires_at: Option<String>,
) -> Result<()> {
    let mut builder = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(adapter_id)
        .name(adapter_id)
        .hash_b3(hash_b3)
        .rank(8)
        .tier("warm");

    if let Some(expires_at) = expires_at {
        builder = builder.expires_at(Some(expires_at));
    }

    let params = builder.build().map_err(|e| {
        AosError::Validation(format!(
            "Failed to build adapter registration params: {}",
            e
        ))
    })?;

    let _ = db.register_adapter(params).await?;
    Ok(())
}

#[tokio::test]
async fn get_adapter_scopes_by_tenant_even_with_duplicate_adapter_ids() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    // Same adapter_id across tenants (allowed by schema); hashes must differ (hash_b3 is UNIQUE).
    register_adapter(&db, "tenant-a", "shared-adapter", "b3:hash-a", None).await?;
    register_adapter(&db, "tenant-b", "shared-adapter", "b3:hash-b", None).await?;

    let adapter_a = db
        .get_adapter_for_tenant("tenant-a", "shared-adapter")
        .await?
        .expect("tenant-a should resolve its adapter");
    assert_eq!(adapter_a.tenant_id, "tenant-a");
    assert_eq!(adapter_a.adapter_id.as_deref(), Some("shared-adapter"));

    let adapter_b = db
        .get_adapter_for_tenant("tenant-b", "shared-adapter")
        .await?
        .expect("tenant-b should resolve its adapter");
    assert_eq!(adapter_b.tenant_id, "tenant-b");
    assert_eq!(adapter_b.adapter_id.as_deref(), Some("shared-adapter"));

    let not_found = db
        .get_adapter_for_tenant("tenant-c", "shared-adapter")
        .await?;
    assert!(
        not_found.is_none(),
        "other tenants must not observe adapters they do not own"
    );

    Ok(())
}

#[tokio::test]
async fn list_adapters_for_tenant_returns_only_that_tenants_adapters() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    register_adapter(&db, "tenant-a", "a-1", "b3:hash-a1", None).await?;
    register_adapter(&db, "tenant-a", "a-2", "b3:hash-a2", None).await?;
    register_adapter(&db, "tenant-b", "b-1", "b3:hash-b1", None).await?;

    let tenant_a = db.list_adapters_for_tenant("tenant-a").await?;
    assert!(tenant_a
        .iter()
        .any(|a| a.adapter_id.as_deref() == Some("a-1")));
    assert!(tenant_a
        .iter()
        .any(|a| a.adapter_id.as_deref() == Some("a-2")));
    assert!(!tenant_a
        .iter()
        .any(|a| a.adapter_id.as_deref() == Some("b-1")));

    let tenant_b = db.list_adapters_for_tenant("tenant-b").await?;
    assert!(tenant_b
        .iter()
        .any(|a| a.adapter_id.as_deref() == Some("b-1")));
    assert!(!tenant_b
        .iter()
        .any(|a| a.adapter_id.as_deref() == Some("a-1")));
    assert!(!tenant_b
        .iter()
        .any(|a| a.adapter_id.as_deref() == Some("a-2")));

    Ok(())
}
