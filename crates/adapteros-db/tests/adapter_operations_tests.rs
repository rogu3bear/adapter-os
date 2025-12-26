//! Comprehensive adapter database operations tests
//!
//! Tests cover:
//! - Adapter CRUD operations with tenant isolation
//! - Adapter state transitions
//! - Category/scope/state queries
//! - Edge cases and error conditions

use adapteros_core::{AosError, Result};
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;

/// Helper to create a tenant for testing
async fn create_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;
    Ok(())
}

/// Helper to register a basic adapter
async fn register_adapter(
    db: &Db,
    tenant_id: &str,
    adapter_id: &str,
    hash_b3: &str,
) -> Result<String> {
    let params = AdapterRegistrationBuilder::new()
        .tenant_id(tenant_id)
        .adapter_id(adapter_id)
        .name(adapter_id)
        .hash_b3(hash_b3)
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .map_err(|e| AosError::Validation(format!("Failed to build params: {}", e)))?;

    db.register_adapter(params).await
}

// ============================================================================
// Create Operations
// ============================================================================

#[tokio::test]
async fn register_adapter_creates_new_entry() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "test-adapter", "b3:hash-001").await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "test-adapter")
        .await?
        .expect("adapter should exist");

    assert_eq!(adapter.adapter_id.as_deref(), Some("test-adapter"));
    assert_eq!(adapter.tenant_id, "tenant-a");
    assert_eq!(adapter.hash_b3, "b3:hash-001");
    assert_eq!(adapter.tier, "warm");
    assert_eq!(adapter.category, "code");
    assert_eq!(adapter.scope, "global");
    assert_eq!(adapter.rank, 8);
    assert_eq!(adapter.active, 1);

    Ok(())
}

#[tokio::test]
async fn register_adapter_with_optional_fields() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-b").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-b")
        .adapter_id("full-adapter")
        .name("Full Adapter")
        .hash_b3("b3:hash-full")
        .rank(16)
        .tier("persistent")
        .category("codebase")
        .scope("tenant")
        .alpha(32.0)
        .lora_strength(Some(0.75))
        .framework(Some("pytorch"))
        .intent(Some("Specialized translation"))
        .build()?;

    db.register_adapter(params).await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-b", "full-adapter")
        .await?
        .expect("adapter should exist");

    assert_eq!(adapter.alpha, 32.0);
    assert_eq!(adapter.lora_strength, Some(0.75));
    assert_eq!(adapter.framework.as_deref(), Some("pytorch"));

    Ok(())
}

#[tokio::test]
async fn register_adapter_rejects_duplicate_hash() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-c").await?;

    register_adapter(&db, "tenant-c", "adapter-1", "b3:unique-hash").await?;

    let err = register_adapter(&db, "tenant-c", "adapter-2", "b3:unique-hash")
        .await
        .expect_err("duplicate hash should be rejected");

    assert!(matches!(err, AosError::Database(_)));

    Ok(())
}

// ============================================================================
// Read Operations with Tenant Isolation
// ============================================================================

#[tokio::test]
async fn get_adapter_for_tenant_enforces_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    register_adapter(&db, "tenant-a", "adapter-a", "b3:hash-a").await?;
    register_adapter(&db, "tenant-b", "adapter-b", "b3:hash-b").await?;

    let adapter_a = db
        .get_adapter_for_tenant("tenant-a", "adapter-a")
        .await?
        .expect("tenant-a should see adapter-a");
    assert_eq!(adapter_a.tenant_id, "tenant-a");

    let not_found = db.get_adapter_for_tenant("tenant-a", "adapter-b").await?;
    assert!(
        not_found.is_none(),
        "tenant-a should not see tenant-b's adapter"
    );

    Ok(())
}

#[tokio::test]
async fn list_adapters_for_tenant_returns_only_tenant_adapters() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    register_adapter(&db, "tenant-a", "a-1", "b3:hash-a1").await?;
    register_adapter(&db, "tenant-a", "a-2", "b3:hash-a2").await?;
    register_adapter(&db, "tenant-b", "b-1", "b3:hash-b1").await?;

    let tenant_a_adapters = db.list_adapters_for_tenant("tenant-a").await?;
    assert_eq!(tenant_a_adapters.len(), 2);
    assert!(tenant_a_adapters.iter().all(|a| a.tenant_id == "tenant-a"));

    Ok(())
}

#[tokio::test]
async fn get_adapter_by_hash_finds_correct_adapter() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let id = register_adapter(&db, "tenant-a", "hash-lookup", "b3:unique-lookup-hash").await?;

    let adapter = db
        .find_adapter_by_hash_for_tenant("tenant-a", "b3:unique-lookup-hash")
        .await?
        .expect("adapter should be found by hash");

    assert_eq!(adapter.id, id);
    assert_eq!(adapter.hash_b3, "b3:unique-lookup-hash");

    Ok(())
}

// ============================================================================
// Update Operations
// ============================================================================

#[tokio::test]
async fn update_adapter_state_changes_current_state() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "state-test", "b3:hash-state").await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "state-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.current_state, "unloaded");

    db.update_adapter_state("state-test", "warm", "test")
        .await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "state-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.current_state, "warm");

    Ok(())
}

#[tokio::test]
async fn update_adapter_tier_for_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "tier-test", "b3:hash-tier").await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "tier-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.tier, "warm");

    db.update_adapter_tier_for_tenant("tenant-a", "tier-test", "persistent")
        .await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "tier-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.tier, "persistent");

    Ok(())
}

#[tokio::test]
async fn update_adapter_memory_for_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "memory-test", "b3:hash-memory").await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "memory-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.memory_bytes, 0);

    db.update_adapter_memory_for_tenant("tenant-a", "memory-test", 1048576)
        .await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "memory-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.memory_bytes, 1048576);

    Ok(())
}

#[tokio::test]
async fn update_adapter_strength() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "strength-test", "b3:hash-strength").await?;

    db.update_adapter_strength("strength-test", 0.85).await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "strength-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.lora_strength, Some(0.85));

    Ok(())
}

#[tokio::test]
async fn update_adapter_state_and_memory_atomically() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "atomic-test", "b3:hash-atomic").await?;

    db.update_adapter_state_and_memory("atomic-test", "warm", 2097152, "test")
        .await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "atomic-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.current_state, "warm");
    assert_eq!(adapter.memory_bytes, 2097152);

    Ok(())
}

#[tokio::test]
async fn update_adapter_state_cas_succeeds_with_correct_expected_state() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "cas-test", "b3:hash-cas").await?;

    let success = db
        .update_adapter_state_cas("cas-test", "unloaded", "warm", "test")
        .await?;
    assert!(success);

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "cas-test")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.current_state, "warm");

    Ok(())
}

#[tokio::test]
async fn update_adapter_state_cas_fails_with_wrong_expected_state() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "cas-fail", "b3:hash-cas-fail").await?;

    // First transition to warm
    db.update_adapter_state_cas("cas-fail", "unloaded", "warm", "test")
        .await?;

    // Try to transition from unloaded (wrong state) to hot - should fail
    let failed = db
        .update_adapter_state_cas("cas-fail", "unloaded", "hot", "test")
        .await?;
    assert!(!failed, "CAS should fail with incorrect expected state");

    // State should remain warm
    let adapter = db
        .get_adapter_for_tenant("tenant-a", "cas-fail")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.current_state, "warm");

    Ok(())
}

// ============================================================================
// Delete Operations
// ============================================================================

#[tokio::test]
async fn delete_adapter_for_tenant_enforces_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;
    create_tenant(&db, "tenant-b").await?;

    register_adapter(&db, "tenant-a", "adapter-a", "b3:hash-del-a").await?;
    register_adapter(&db, "tenant-b", "adapter-b", "b3:hash-del-b").await?;

    // Tenant A cannot delete tenant B's adapter
    let err = db
        .delete_adapter_for_tenant("tenant-a", "adapter-b")
        .await
        .expect_err("cross-tenant delete should be denied");
    assert!(matches!(err, AosError::NotFound(_)));

    let adapter_b = db.get_adapter_for_tenant("tenant-b", "adapter-b").await?;
    assert!(adapter_b.is_some(), "tenant-b's adapter should still exist");

    // Tenant A can delete their own adapter
    db.delete_adapter_for_tenant("tenant-a", "adapter-a")
        .await?;

    let adapter_a = db.get_adapter_for_tenant("tenant-a", "adapter-a").await?;
    assert!(adapter_a.is_none(), "tenant-a's adapter should be deleted");

    Ok(())
}

// ============================================================================
// Category, Scope, and State Queries
// ============================================================================

#[tokio::test]
async fn list_adapters_by_category() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let params1 = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("code-1")
        .name("Code 1")
        .hash_b3("b3:hash-code-1")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()?;

    let params2 = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("translate-1")
        .name("Translate 1")
        .hash_b3("b3:hash-translate-1")
        .rank(8)
        .tier("warm")
        .category("framework")
        .scope("global")
        .build()?;

    db.register_adapter(params1).await?;
    db.register_adapter(params2).await?;

    let code_adapters = db.list_adapters_by_category("tenant-a", "code").await?;
    assert!(code_adapters.len() >= 1);
    assert!(code_adapters.iter().all(|a| a.category == "code"));

    Ok(())
}

#[tokio::test]
async fn list_adapters_by_scope() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let params1 = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("global-1")
        .name("Global 1")
        .hash_b3("b3:hash-global-1")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()?;

    let params2 = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("tenant-1")
        .name("Tenant 1")
        .hash_b3("b3:hash-tenant-1")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("tenant")
        .build()?;

    db.register_adapter(params1).await?;
    db.register_adapter(params2).await?;

    let global_adapters = db.list_adapters_by_scope("tenant-a", "global").await?;
    assert!(global_adapters.iter().any(|a| a.scope == "global"));

    let tenant_adapters = db.list_adapters_by_scope("tenant-a", "tenant").await?;
    assert!(tenant_adapters.iter().any(|a| a.scope == "tenant"));

    Ok(())
}

#[tokio::test]
async fn list_adapters_by_state() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "idle-1", "b3:hash-idle-1").await?;
    register_adapter(&db, "tenant-a", "loading-1", "b3:hash-loading-1").await?;

    db.update_adapter_state("loading-1", "warm", "test").await?;

    let unloaded_adapters = db.list_adapters_by_state("tenant-a", "unloaded").await?;
    assert!(unloaded_adapters
        .iter()
        .any(|a| a.current_state == "unloaded"));

    let warm_adapters = db.list_adapters_by_state("tenant-a", "warm").await?;
    assert!(warm_adapters.iter().any(|a| a.current_state == "warm"));

    Ok(())
}

#[tokio::test]
async fn get_adapter_state_summary_returns_counts() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    register_adapter(&db, "tenant-a", "idle-1", "b3:hash-idle-1").await?;
    register_adapter(&db, "tenant-a", "idle-2", "b3:hash-idle-2").await?;

    let summary = db.get_adapter_state_summary("tenant-a").await?;

    // Summary is a Vec<(category, scope, state, count, total_memory, avg_activations, most_recent)>
    let unloaded_count = summary
        .iter()
        .find(|(_, _, state, _, _, _, _)| state == "unloaded")
        .map(|(_, _, _, count, _, _, _)| *count);

    assert_eq!(unloaded_count, Some(2));

    Ok(())
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn register_adapter_requires_tenant_exists() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let err = register_adapter(&db, "nonexistent-tenant", "test", "b3:hash-001")
        .await
        .expect_err("registration should fail for nonexistent tenant");

    assert!(matches!(err, AosError::Database(_)));

    Ok(())
}

#[tokio::test]
async fn get_adapter_returns_none_for_nonexistent() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let adapter = db.get_adapter_for_tenant("tenant-a", "nonexistent").await?;
    assert!(adapter.is_none());

    Ok(())
}

#[tokio::test]
async fn adapter_expires_at_sets_expiration() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let expiration = "2025-12-31T23:59:59Z";

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("expiring-adapter")
        .name("Expiring Adapter")
        .hash_b3("b3:hash-expiring")
        .rank(8)
        .tier("ephemeral")
        .category("code")
        .scope("tenant")
        .expires_at(Some(expiration))
        .build()?;

    db.register_adapter(params).await?;

    let adapter = db
        .get_adapter_for_tenant("tenant-a", "expiring-adapter")
        .await?
        .expect("adapter exists");
    assert_eq!(adapter.expires_at.as_deref(), Some(expiration));

    Ok(())
}

// ============================================================================
// Lineage and Versioning
// ============================================================================

#[tokio::test]
async fn adapter_parent_child_relationship() -> Result<()> {
    let db = Db::new_in_memory().await?;
    create_tenant(&db, "tenant-a").await?;

    let parent_id = register_adapter(&db, "tenant-a", "parent", "b3:hash-parent").await?;

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("tenant-a")
        .adapter_id("child")
        .name("Child Adapter")
        .hash_b3("b3:hash-child")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .parent_id(Some(&parent_id))
        .fork_type(Some("extension"))
        .build()?;

    let child_id = db.register_adapter(params).await?;

    let child = db
        .get_adapter_for_tenant("tenant-a", "child")
        .await?
        .expect("child exists");
    assert_eq!(child.parent_id.as_deref(), Some(parent_id.as_str()));
    assert_eq!(child.fork_type.as_deref(), Some("extension"));

    let children = db.get_adapter_children(&parent_id).await?;
    assert!(children.iter().any(|a| a.id == child_id));

    Ok(())
}
