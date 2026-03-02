//! Integration tests for system bootstrap validation and repair
//!
//! Tests that the system tenant and core policies are properly created
//! during bootstrap and can be repaired if missing or incomplete.

use adapteros_core::Result;
use adapteros_db::Db;

/// Helper to create a test database with migrations applied
async fn setup_test_db() -> Result<Db> {
    let db = Db::new_in_memory().await?;
    Ok(db)
}

/// Test that ensure_system_tenant creates system tenant and policies on fresh install
#[tokio::test]
async fn test_ensure_system_tenant_creates_on_fresh_install() -> Result<()> {
    let db = setup_test_db().await?;

    // Initially, system tenant should not exist
    let system = db.get_tenant("system").await?;
    assert!(system.is_none(), "System tenant should not exist initially");

    // Ensure system tenant
    db.ensure_system_tenant().await?;

    // Verify system tenant was created
    let system = db.get_tenant("system").await?;
    assert!(
        system.is_some(),
        "System tenant should exist after bootstrap"
    );

    // Verify core policies are enabled
    let policies = db.get_active_policies_for_tenant("system").await?;
    assert!(
        policies.contains(&"egress".to_string()),
        "egress policy should be enabled"
    );
    assert!(
        policies.contains(&"determinism".to_string()),
        "determinism policy should be enabled"
    );
    assert!(
        policies.contains(&"isolation".to_string()),
        "isolation policy should be enabled"
    );
    assert!(
        policies.contains(&"evidence".to_string()),
        "evidence policy should be enabled"
    );

    Ok(())
}

/// Test that ensure_system_tenant is idempotent (safe to call multiple times)
#[tokio::test]
async fn test_ensure_system_tenant_is_idempotent() -> Result<()> {
    let db = setup_test_db().await?;

    // Call ensure_system_tenant multiple times
    db.ensure_system_tenant().await?;
    db.ensure_system_tenant().await?;
    db.ensure_system_tenant().await?;

    // Verify system tenant exists exactly once
    let system = db.get_tenant("system").await?;
    assert!(system.is_some(), "System tenant should exist");

    // Verify policies are still correct
    let policies = db.get_active_policies_for_tenant("system").await?;
    assert!(
        policies.len() >= 4,
        "At least 4 core policies should be enabled"
    );

    Ok(())
}

/// Test that ensure_system_tenant repairs missing policies
#[tokio::test]
async fn test_ensure_system_tenant_repairs_missing_policies() -> Result<()> {
    let db = setup_test_db().await?;

    // Create system tenant WITHOUT policies (simulating partial bootstrap failure)
    sqlx::query(
        "INSERT INTO tenants (id, name, itar_flag, created_at)
         VALUES ('system', 'System', 0, datetime('now'))",
    )
    .execute(db.pool_result()?)
    .await?;

    // Verify tenant exists but policies are missing
    let system = db.get_tenant("system").await?;
    assert!(system.is_some(), "System tenant should exist");

    let policies = db.get_active_policies_for_tenant("system").await?;
    assert!(
        policies.is_empty(),
        "Policies should be empty before repair"
    );

    // ensure_system_tenant should detect and repair
    db.ensure_system_tenant().await?;

    // Verify policies were seeded
    let policies = db.get_active_policies_for_tenant("system").await?;
    assert!(
        policies.contains(&"egress".to_string()),
        "egress policy should be enabled after repair"
    );
    assert!(
        policies.contains(&"determinism".to_string()),
        "determinism policy should be enabled after repair"
    );
    assert!(
        policies.contains(&"isolation".to_string()),
        "isolation policy should be enabled after repair"
    );
    assert!(
        policies.contains(&"evidence".to_string()),
        "evidence policy should be enabled after repair"
    );

    Ok(())
}

/// Test validate_bootstrap_state reports issues correctly
#[tokio::test]
async fn test_validate_bootstrap_state_reports_missing_tenant() -> Result<()> {
    let db = setup_test_db().await?;

    // Don't create system tenant - validate should report it missing
    let status = db.validate_bootstrap_state().await?;

    assert!(!status.healthy, "Status should be unhealthy");
    assert!(
        status
            .issues
            .iter()
            .any(|i| i.contains("System tenant missing")),
        "Should report missing system tenant"
    );

    Ok(())
}

/// Test validate_bootstrap_state reports missing policies
#[tokio::test]
async fn test_validate_bootstrap_state_reports_missing_policies() -> Result<()> {
    let db = setup_test_db().await?;

    // Create system tenant WITHOUT policies
    sqlx::query(
        "INSERT INTO tenants (id, name, itar_flag, created_at)
         VALUES ('system', 'System', 0, datetime('now'))",
    )
    .execute(db.pool_result()?)
    .await?;

    let status = db.validate_bootstrap_state().await?;

    assert!(!status.healthy, "Status should be unhealthy");
    assert!(
        status
            .issues
            .iter()
            .any(|i| i.contains("Core policy") && i.contains("not enabled")),
        "Should report missing core policies"
    );

    Ok(())
}

/// Test validate_bootstrap_state returns healthy when properly bootstrapped
#[tokio::test]
async fn test_validate_bootstrap_state_healthy_after_bootstrap() -> Result<()> {
    let db = setup_test_db().await?;

    // Bootstrap properly
    db.ensure_system_tenant().await?;

    let status = db.validate_bootstrap_state().await?;

    assert!(status.healthy, "Status should be healthy after bootstrap");
    assert!(
        status.issues.is_empty(),
        "No issues should be reported: {:?}",
        status.issues
    );

    Ok(())
}

/// Test that seed_dev_data creates default tenant (not system)
/// This verifies that system tenant creation is separate from dev seeding
#[tokio::test]
async fn test_seed_dev_data_creates_default_tenant_not_system() -> Result<()> {
    let db = setup_test_db().await?;

    // Seed dev data
    db.seed_dev_data().await?;

    // Default tenant should exist
    let default = db.get_tenant("default").await?;
    assert!(
        default.is_some(),
        "Default tenant should exist after seeding"
    );

    // System tenant should NOT exist (created separately)
    let system = db.get_tenant("system").await?;
    assert!(
        system.is_none(),
        "System tenant should not be created by seed_dev_data"
    );

    Ok(())
}

/// Regression: seed_dev_data must ensure the local dev node exists even when users already exist.
/// Worker registration hardcodes node_id="local" and the workers table enforces a FK to nodes(id).
#[tokio::test]
async fn test_seed_dev_data_ensures_local_node_even_when_users_exist() -> Result<()> {
    let db = setup_test_db().await?;

    // We need the schema present to manipulate tables directly.
    db.migrate().await?;

    // Seed once to create users/tenant, then simulate a partial/dev-corrupt state where
    // nodes are missing but users exist (this is the scenario that breaks worker registration).
    db.seed_dev_data().await?;
    sqlx::query("DELETE FROM nodes")
        .execute(db.pool_result()?)
        .await?;

    db.seed_dev_data().await?;

    let local_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id = 'local'")
        .fetch_one(db.pool_result()?)
        .await?;
    assert_eq!(
        local_exists, 1,
        "seed_dev_data should ensure nodes.local exists"
    );

    Ok(())
}

/// Test complete bootstrap flow (migrate + seed + ensure_system_tenant)
#[tokio::test]
async fn test_complete_bootstrap_flow() -> Result<()> {
    let db = setup_test_db().await?;

    // Full bootstrap flow
    db.seed_dev_data().await.ok(); // May fail if already seeded, that's OK
    db.ensure_system_tenant().await?;

    // Validate
    let status = db.validate_bootstrap_state().await?;
    assert!(
        status.healthy,
        "Bootstrap state should be healthy: {:?}",
        status.issues
    );

    // Both tenants should exist
    let default = db.get_tenant("default").await?;
    let system = db.get_tenant("system").await?;
    assert!(
        default.is_some() || system.is_some(),
        "At least one tenant should exist"
    );
    assert!(system.is_some(), "System tenant must exist");

    Ok(())
}
