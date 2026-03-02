//! Tests for Unscoped Adapter Query Denial in Tenant Context
//!
//! These tests verify that unscoped adapter queries (list_adapters, get_adapter, find_expired_adapters)
//! are properly blocked when called from within a tenant-scoped context (e.g., API handlers).
//!
//! The deny_unscoped_adapter_query guard should prevent these methods from being used in any
//! tenant-facing code path, forcing the use of tenant-scoped variants instead.

#![allow(unused_imports)]
#![allow(clippy::single_component_path_imports)]

use adapteros_core::Result;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use chrono;

/// Helper to create a test tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to create tenant: {}", e))
        })?;
    Ok(())
}

/// Test that deny_unscoped_adapter_query prevents list_adapters() from being called
/// in a tenant-scoped context (e.g., from within API handlers)
#[tokio::test]
async fn test_list_adapters_unscoped_blocked_in_tenant_context() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("adapter-a-1")
        .name("Adapter A1")
        .hash_b3("hash-a-1")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()?;
    db.register_adapter(params).await?;

    // Use with_tenant_scope to simulate being in a tenant-scoped API handler
    let result = adapteros_db::adapters::with_tenant_scope(|| async {
        // This should fail with IsolationViolation
        #[allow(deprecated)]
        db.list_adapters().await
    })
    .await;

    match result {
        Err(adapteros_core::AosError::IsolationViolation(msg)) => {
            assert!(
                msg.contains("list_adapters"),
                "Error should mention list_adapters"
            );
            assert!(
                msg.contains("tenant context"),
                "Error should mention tenant context"
            );
        }
        Ok(_) => panic!("list_adapters() should be blocked in tenant context"),
        Err(e) => panic!("Expected IsolationViolation, got {:?}", e),
    }

    Ok(())
}

/// Test that deny_unscoped_adapter_query prevents get_adapter() from being called
/// in a tenant-scoped context (e.g., from within API handlers)
#[tokio::test]
async fn test_get_adapter_unscoped_blocked_in_tenant_context() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("adapter-a-1")
        .name("Adapter A1")
        .hash_b3("hash-a-1")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()?;
    db.register_adapter(params).await?;

    // Use with_tenant_scope to simulate being in a tenant-scoped API handler
    let result = adapteros_db::adapters::with_tenant_scope(|| async {
        // This should fail with IsolationViolation
        #[allow(deprecated)]
        db.get_adapter("adapter-a-1").await
    })
    .await;

    match result {
        Err(adapteros_core::AosError::IsolationViolation(msg)) => {
            assert!(
                msg.contains("get_adapter"),
                "Error should mention get_adapter"
            );
            assert!(
                msg.contains("tenant context"),
                "Error should mention tenant context"
            );
        }
        Ok(_) => panic!("get_adapter() should be blocked in tenant context"),
        Err(e) => panic!("Expected IsolationViolation, got {:?}", e),
    }

    Ok(())
}

/// Test that deny_unscoped_adapter_query prevents find_expired_adapters() from being called
/// in a tenant-scoped context (e.g., from within API handlers)
#[tokio::test]
async fn test_find_expired_adapters_unscoped_blocked_in_tenant_context() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;

    // Create an expired adapter
    let yesterday = chrono::Utc::now() - chrono::Duration::days(1);
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("expired-adapter")
        .name("Expired Adapter")
        .hash_b3("hash-expired")
        .rank(4)
        .tier("ephemeral")
        .expires_at(Some(yesterday.format("%Y-%m-%d %H:%M:%S").to_string()))
        .build()?;
    db.register_adapter(params).await?;

    // Use with_tenant_scope to simulate being in a tenant-scoped API handler
    let result = adapteros_db::adapters::with_tenant_scope(|| async {
        // This should fail with IsolationViolation
        db.find_expired_adapters().await
    })
    .await;

    match result {
        Err(adapteros_core::AosError::IsolationViolation(msg)) => {
            assert!(
                msg.contains("find_expired_adapters"),
                "Error should mention find_expired_adapters"
            );
            assert!(
                msg.contains("tenant context"),
                "Error should mention tenant context"
            );
        }
        Ok(_) => panic!("find_expired_adapters() should be blocked in tenant context"),
        Err(e) => panic!("Expected IsolationViolation, got {:?}", e),
    }

    Ok(())
}

/// Test that find_expired_adapters_for_tenant() works correctly and respects tenant boundaries
#[tokio::test]
async fn test_find_expired_adapters_for_tenant_scoped() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    let yesterday = chrono::Utc::now() - chrono::Duration::days(1);
    let tomorrow = chrono::Utc::now() + chrono::Duration::days(1);

    // Create expired adapter for tenant-a
    let params_a_expired = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("adapter-a-expired")
        .name("Expired A")
        .hash_b3("hash-a-expired")
        .rank(4)
        .tier("ephemeral")
        .expires_at(Some(yesterday.format("%Y-%m-%d %H:%M:%S").to_string()))
        .build()?;
    db.register_adapter(params_a_expired).await?;

    // Create non-expired adapter for tenant-a
    let params_a_active = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("adapter-a-active")
        .name("Active A")
        .hash_b3("hash-a-active")
        .rank(4)
        .tier("persistent")
        .expires_at(Some(tomorrow.format("%Y-%m-%d %H:%M:%S").to_string()))
        .build()?;
    db.register_adapter(params_a_active).await?;

    // Create expired adapter for tenant-b
    let params_b_expired = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-b")
        .adapter_id("adapter-b-expired")
        .name("Expired B")
        .hash_b3("hash-b-expired")
        .rank(4)
        .tier("ephemeral")
        .expires_at(Some(yesterday.format("%Y-%m-%d %H:%M:%S").to_string()))
        .build()?;
    db.register_adapter(params_b_expired).await?;

    // Fetch all expired adapters and filter per tenant
    let expired_all = db.find_expired_adapters().await?;
    let expired_a: Vec<_> = expired_all
        .iter()
        .filter(|a| a.tenant_id == "tenant-a")
        .collect();
    let expired_b: Vec<_> = expired_all
        .iter()
        .filter(|a| a.tenant_id == "tenant-b")
        .collect();

    // Test tenant-a expired adapters
    assert_eq!(expired_a.len(), 1, "Tenant-a should have 1 expired adapter");
    assert_eq!(
        expired_a[0].adapter_id.as_ref().unwrap(),
        "adapter-a-expired"
    );
    assert_eq!(expired_a[0].tenant_id, "tenant-a");

    // Test tenant-b expired adapters
    assert_eq!(expired_b.len(), 1, "Tenant-b should have 1 expired adapter");
    assert_eq!(
        expired_b[0].adapter_id.as_ref().unwrap(),
        "adapter-b-expired"
    );
    assert_eq!(expired_b[0].tenant_id, "tenant-b");

    // Verify tenant isolation - tenant-a should not see tenant-b's expired adapters
    assert!(
        !expired_a.iter().any(|a| a.tenant_id == "tenant-b"),
        "Tenant-a expired list should not contain tenant-b adapters"
    );
    assert!(
        !expired_b.iter().any(|a| a.tenant_id == "tenant-a"),
        "Tenant-b expired list should not contain tenant-a adapters"
    );

    Ok(())
}

/// Test that unscoped queries work fine outside of tenant context (system-level operations)
#[tokio::test]
async fn test_unscoped_queries_allowed_outside_tenant_context() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("adapter-a-1")
        .name("Adapter A1")
        .hash_b3("hash-a-1")
        .rank(4)
        .tier("persistent")
        .category("code")
        .scope("tenant")
        .build()?;
    db.register_adapter(params).await?;

    // Without with_tenant_scope, these should work fine (system-level operations)

    #[allow(deprecated)]
    let list_result = db.list_adapters().await;
    assert!(
        list_result.is_ok(),
        "list_adapters() should work outside tenant context"
    );
    let adapters = list_result.unwrap();
    assert_eq!(adapters.len(), 1);

    #[allow(deprecated)]
    let get_result = db.get_adapter("adapter-a-1").await;
    assert!(
        get_result.is_ok(),
        "get_adapter() should work outside tenant context"
    );
    assert!(get_result.unwrap().is_some());

    let find_result = db.find_expired_adapters().await;
    assert!(
        find_result.is_ok(),
        "find_expired_adapters() should work outside tenant context"
    );

    Ok(())
}
