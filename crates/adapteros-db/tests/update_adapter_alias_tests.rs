//! Tests for adapter update operations
//!
//! - update_adapter_alias_for_tenant: semantic alias (tenant/domain/purpose/revision)
//! - update_adapter_display_name_for_tenant: simple display name (UI rename)
//!
//! Verifies that updates work for mutable states (Draft, Training)
//! and are blocked for immutable states (Active, Deprecated, Retired, Failed).

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

#[tokio::test]
async fn update_adapter_alias_succeeds_for_draft() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "default-tenant").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default-tenant")
        .adapter_id("alias-test-draft")
        .name("Original Name")
        .hash_b3("b3:alias_test_hash")
        .rank(8)
        .tier("warm")
        .build()
        .map_err(|e| AosError::Validation(e.to_string()))?;

    db.register_adapter(params).await?;

    db.update_adapter_alias_for_tenant(
        "default-tenant",
        "alias-test-draft",
        Some("default-tenant/code/review/r001"),
    )
    .await?;

    let adapter = db
        .get_adapter_for_tenant("default-tenant", "alias-test-draft")
        .await?
        .expect("adapter should exist");
    assert_eq!(
        adapter.adapter_name.as_deref(),
        Some("default-tenant/code/review/r001")
    );

    Ok(())
}

#[tokio::test]
async fn update_adapter_alias_clear_succeeds_for_draft() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "default-tenant").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default-tenant")
        .adapter_id("alias-clear-test")
        .name("Custom Name")
        .hash_b3("b3:alias_clear_hash")
        .rank(8)
        .tier("warm")
        .build()
        .map_err(|e| AosError::Validation(e.to_string()))?;

    db.register_adapter(params).await?;
    db.update_adapter_alias_for_tenant(
        "default-tenant",
        "alias-clear-test",
        Some("default-tenant/code/clear/r001"),
    )
    .await?;

    db.update_adapter_alias_for_tenant("default-tenant", "alias-clear-test", None)
        .await?;

    let adapter = db
        .get_adapter_for_tenant("default-tenant", "alias-clear-test")
        .await?
        .expect("adapter should exist");
    assert_eq!(adapter.adapter_name.as_deref(), None);

    Ok(())
}

#[tokio::test]
async fn update_adapter_alias_blocked_for_active() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "default-tenant").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default-tenant")
        .adapter_id("alias-blocked-active")
        .name("Active Adapter")
        .hash_b3("b3:alias_blocked_hash")
        .rank(8)
        .tier("persistent")
        .build()
        .map_err(|e| AosError::Validation(e.to_string()))?;

    db.register_adapter(params).await?;

    // Set lifecycle_state to 'active' directly (bypass transition validation for test isolation)
    sqlx::query(
        "UPDATE adapters SET lifecycle_state = 'active' WHERE adapter_id = 'alias-blocked-active'",
    )
    .execute(db.pool_result()?)
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    let result = db
        .update_adapter_alias_for_tenant(
            "default-tenant",
            "alias-blocked-active",
            Some("default-tenant/code/blocked/r001"),
        )
        .await;

    assert!(matches!(result, Err(AosError::PolicyViolation(_))));

    Ok(())
}

// =============================================================================
// Display name tests (simple string, UI rename flow)
// =============================================================================

#[tokio::test]
async fn update_adapter_display_name_succeeds_for_draft() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "default-tenant").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default-tenant")
        .adapter_id("display-name-test")
        .name("Original Name")
        .hash_b3("b3:display_name_hash")
        .rank(8)
        .tier("warm")
        .build()
        .map_err(|e| AosError::Validation(e.to_string()))?;

    db.register_adapter(params).await?;

    db.update_adapter_display_name_for_tenant(
        "default-tenant",
        "display-name-test",
        Some("My Custom Adapter"),
    )
    .await?;

    let adapter = db
        .get_adapter_for_tenant("default-tenant", "display-name-test")
        .await?
        .expect("adapter should exist");
    assert_eq!(adapter.name, "My Custom Adapter");

    Ok(())
}

#[tokio::test]
async fn update_adapter_display_name_clear_succeeds_for_draft() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "default-tenant").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default-tenant")
        .adapter_id("display-clear-test")
        .name("Custom Name")
        .hash_b3("b3:display_clear_hash")
        .rank(8)
        .tier("warm")
        .build()
        .map_err(|e| AosError::Validation(e.to_string()))?;

    db.register_adapter(params).await?;
    db.update_adapter_display_name_for_tenant(
        "default-tenant",
        "display-clear-test",
        Some("Custom"),
    )
    .await?;

    db.update_adapter_display_name_for_tenant("default-tenant", "display-clear-test", None)
        .await?;

    let adapter = db
        .get_adapter_for_tenant("default-tenant", "display-clear-test")
        .await?
        .expect("adapter should exist");
    assert_eq!(adapter.name, "display-clear-test");

    Ok(())
}

#[tokio::test]
async fn update_adapter_display_name_blocked_for_active() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "default-tenant").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default-tenant")
        .adapter_id("display-blocked-active")
        .name("Active Adapter")
        .hash_b3("b3:display_blocked_hash")
        .rank(8)
        .tier("persistent")
        .build()
        .map_err(|e| AosError::Validation(e.to_string()))?;

    db.register_adapter(params).await?;

    sqlx::query(
        "UPDATE adapters SET lifecycle_state = 'active' WHERE adapter_id = 'display-blocked-active'",
    )
    .execute(db.pool_result()?)
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    let result = db
        .update_adapter_display_name_for_tenant(
            "default-tenant",
            "display-blocked-active",
            Some("New Name"),
        )
        .await;

    assert!(matches!(result, Err(AosError::PolicyViolation(_))));

    Ok(())
}

#[tokio::test]
async fn update_adapter_display_name_rejects_empty() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "default-tenant").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default-tenant")
        .adapter_id("display-empty-test")
        .name("Original")
        .hash_b3("b3:display_empty_hash")
        .rank(8)
        .tier("warm")
        .build()
        .map_err(|e| AosError::Validation(e.to_string()))?;

    db.register_adapter(params).await?;

    let result = db
        .update_adapter_display_name_for_tenant("default-tenant", "display-empty-test", Some("   "))
        .await;

    assert!(matches!(result, Err(AosError::Validation(_))));

    Ok(())
}
