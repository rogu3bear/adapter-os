//! Integration tests for per-tenant policy bindings and audit
//!
//! Tests verify:
//! 1. Policy enable/disable per tenant
//! 2. Audit record creation on toggle
//! 3. Default bindings for new tenants
//! 4. Tenant isolation (one tenant's toggles don't affect another)

use adapteros_core::{AosError, Result};
use adapteros_db::Db;

/// Test: New tenant gets default policy bindings
/// Core 4 policies should be enabled, all others disabled
#[tokio::test]
async fn test_new_tenant_default_bindings() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create a tenant (this should auto-initialize bindings)
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('test-tenant', 'Test', 0)")
        .execute(&*db.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    db.initialize_tenant_policy_bindings("test-tenant", "system")
        .await?;

    // Get active policies
    let active = db.get_active_policies_for_tenant("test-tenant").await?;

    // Should have exactly 4 core policies enabled
    assert_eq!(active.len(), 4, "Should have 4 core policies enabled");
    assert!(active.contains(&"egress".to_string()));
    assert!(active.contains(&"determinism".to_string()));
    assert!(active.contains(&"isolation".to_string()));
    assert!(active.contains(&"evidence".to_string()));

    // Should NOT have non-core policies enabled
    assert!(!active.contains(&"telemetry".to_string()));
    assert!(!active.contains(&"router".to_string()));
    assert!(!active.contains(&"refusal".to_string()));

    Ok(())
}

/// Test: Toggle creates audit record in policy_audit_decisions
#[tokio::test]
async fn test_toggle_creates_audit_record() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create tenant and initialize bindings
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('audit-test', 'Audit Test', 0)")
        .execute(&*db.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    db.initialize_tenant_policy_bindings("audit-test", "system")
        .await?;

    // Toggle telemetry on (was off by default)
    let previous = db
        .toggle_tenant_policy("audit-test", "telemetry", true, "admin-user")
        .await?;
    assert!(!previous, "telemetry should have been disabled");

    // Verify audit record exists
    let audit_count: i32 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) as cnt
        FROM policy_audit_decisions
        WHERE tenant_id = 'audit-test'
          AND policy_pack_id = 'telemetry'
          AND hook = 'toggle'
        "#,
    )
    .fetch_one(&*db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    assert!(audit_count > 0, "Should have audit record for toggle");

    // Toggle off
    let previous = db
        .toggle_tenant_policy("audit-test", "telemetry", false, "admin-user")
        .await?;
    assert!(previous, "telemetry should have been enabled");

    // Should have 2 audit records now
    let audit_count: i32 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) as cnt
        FROM policy_audit_decisions
        WHERE tenant_id = 'audit-test'
          AND policy_pack_id = 'telemetry'
          AND hook = 'toggle'
        "#,
    )
    .fetch_one(&*db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    assert_eq!(audit_count, 2, "Should have 2 audit records (on then off)");

    Ok(())
}

/// Test: Tenant A's toggles don't affect Tenant B
#[tokio::test]
async fn test_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('tenant-a', 'Tenant A', 0)")
        .execute(&*db.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('tenant-b', 'Tenant B', 0)")
        .execute(&*db.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    db.initialize_tenant_policy_bindings("tenant-a", "system")
        .await?;
    db.initialize_tenant_policy_bindings("tenant-b", "system")
        .await?;

    // Enable telemetry for tenant A only
    db.toggle_tenant_policy("tenant-a", "telemetry", true, "admin")
        .await?;

    // Tenant A should have telemetry enabled
    let active_a = db.get_active_policies_for_tenant("tenant-a").await?;
    assert!(
        active_a.contains(&"telemetry".to_string()),
        "Tenant A should have telemetry enabled"
    );

    // Tenant B should NOT have telemetry enabled
    let active_b = db.get_active_policies_for_tenant("tenant-b").await?;
    assert!(
        !active_b.contains(&"telemetry".to_string()),
        "Tenant B should NOT have telemetry enabled"
    );

    Ok(())
}

/// Test: Listing all bindings returns all 24 policies
#[tokio::test]
async fn test_list_all_bindings() -> Result<()> {
    let db = Db::new_in_memory().await?;

    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('list-test', 'List Test', 0)")
        .execute(&*db.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    db.initialize_tenant_policy_bindings("list-test", "system")
        .await?;

    let bindings = db.list_tenant_policy_bindings("list-test").await?;

    assert_eq!(bindings.len(), 24, "Should have 24 policy bindings");

    // Count enabled vs disabled
    let enabled_count = bindings.iter().filter(|b| b.enabled).count();
    let disabled_count = bindings.iter().filter(|b| !b.enabled).count();

    assert_eq!(enabled_count, 4, "Should have 4 enabled (core policies)");
    assert_eq!(disabled_count, 20, "Should have 20 disabled");

    Ok(())
}

/// Test: is_policy_enabled_for_tenant helper
#[tokio::test]
async fn test_is_policy_enabled() -> Result<()> {
    let db = Db::new_in_memory().await?;

    sqlx::query(
        "INSERT INTO tenants (id, name, itar_flag) VALUES ('enabled-test', 'Enabled Test', 0)",
    )
    .execute(&*db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    db.initialize_tenant_policy_bindings("enabled-test", "system")
        .await?;

    // Core policy should be enabled
    let egress_enabled = db
        .is_policy_enabled_for_tenant("enabled-test", "egress")
        .await?;
    assert!(egress_enabled, "egress should be enabled");

    // Non-core policy should be disabled
    let telemetry_enabled = db
        .is_policy_enabled_for_tenant("enabled-test", "telemetry")
        .await?;
    assert!(!telemetry_enabled, "telemetry should be disabled");

    // Toggle and check again
    db.toggle_tenant_policy("enabled-test", "telemetry", true, "admin")
        .await?;

    let telemetry_enabled = db
        .is_policy_enabled_for_tenant("enabled-test", "telemetry")
        .await?;
    assert!(telemetry_enabled, "telemetry should now be enabled");

    Ok(())
}

/// Test: Audit chain has sequential entries with proper hashes
#[tokio::test]
async fn test_audit_chain_integrity() -> Result<()> {
    let db = Db::new_in_memory().await?;

    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ('chain-test', 'Chain Test', 0)")
        .execute(&*db.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

    db.initialize_tenant_policy_bindings("chain-test", "system")
        .await?;

    // Perform multiple toggles
    for _ in 0..3 {
        db.toggle_tenant_policy("chain-test", "telemetry", true, "admin")
            .await?;
        db.toggle_tenant_policy("chain-test", "telemetry", false, "admin")
            .await?;
    }

    // Query audit entries
    let entries: Vec<(i64, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT chain_sequence, entry_hash, previous_hash
        FROM policy_audit_decisions
        WHERE tenant_id = 'chain-test'
        ORDER BY chain_sequence ASC
        "#,
    )
    .fetch_all(&*db.pool())
    .await
    .map_err(|e| AosError::Database(e.to_string()))?;

    assert!(!entries.is_empty(), "Should have audit entries");

    // Verify chain linkage
    let mut previous_hash: Option<String> = None;
    for (seq, entry_hash, prev_hash) in entries {
        if seq == 1 {
            // First entry should have no previous hash
            assert!(
                prev_hash.is_none(),
                "First entry should have no previous hash"
            );
        } else {
            // Subsequent entries should link to previous
            assert_eq!(
                prev_hash, previous_hash,
                "Chain sequence {} should link to previous hash",
                seq
            );
        }
        previous_hash = Some(entry_hash);
    }

    Ok(())
}
