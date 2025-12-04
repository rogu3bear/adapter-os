// Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//
// Metadata Integration Tests (Agent 27 - GROUP F)
//
// Purpose: Verify database schema_version, version, and lifecycle_state fields
// are correctly stored and retrieved from database to API responses

use adapteros_core::LifecycleState;
use adapteros_db::{adapters::AdapterRegistrationBuilder, metadata::AdapterMeta, Db};

/// Helper to initialize database with schema
async fn init_test_db() -> anyhow::Result<Db> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;

    // Create a default tenant for tests
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')")
        .execute(db.pool())
        .await?;

    Ok(db)
}

// ============================================================================
// Test Case 1: Verify version and lifecycle_state Columns Exist in DB
// ============================================================================

#[tokio::test]
async fn test_database_schema_has_metadata_columns() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Query schema to verify columns exist
    let schema: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, type FROM pragma_table_info('adapters') WHERE name IN ('version', 'lifecycle_state')"
    )
    .fetch_all(db.pool())
    .await?;

    assert_eq!(
        schema.len(),
        2,
        "Should have both version and lifecycle_state columns"
    );

    let column_names: Vec<String> = schema.iter().map(|(name, _)| name.clone()).collect();
    assert!(
        column_names.contains(&"version".to_string()),
        "Should have version column"
    );
    assert!(
        column_names.contains(&"lifecycle_state".to_string()),
        "Should have lifecycle_state column"
    );

    Ok(())
}

// ============================================================================
// Test Case 2: Verify Default Values
// ============================================================================

#[tokio::test]
async fn test_adapter_default_version_and_lifecycle() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter using builder (no version/lifecycle specified)
    let adapter_id = "test-defaults-001";

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .tenant_id("tenant-1")
        .name("test-adapter")
        .hash_b3("b3:test_hash")
        .rank(16)
        .tier("persistent")
        .category("code")
        .scope("global")
        .build()?;

    db.register_adapter(params).await?;

    // Query adapter (system-level for tests)
    let adapters = db.list_all_adapters_system().await?;
    let adapter = adapters
        .into_iter()
        .find(|a| a.adapter_id.as_deref() == Some(adapter_id))
        .expect("Adapter should exist");

    // Verify default values (from migration 0068)
    assert_eq!(adapter.version, "1.0.0", "Default version should be 1.0.0");
    assert_eq!(
        adapter.lifecycle_state, "active",
        "Default lifecycle_state should be active"
    );

    Ok(())
}

// ============================================================================
// Test Case 3: Adapter to AdapterMeta Conversion
// ============================================================================

#[tokio::test]
async fn test_adapter_to_adaptermeta_conversion() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    let adapter_id = "test-conversion-001";

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .tenant_id("tenant-1")
        .name("test-adapter")
        .hash_b3("b3:test_hash_conversion")
        .rank(16)
        .tier("persistent")
        .category("code")
        .scope("global")
        .build()?;

    db.register_adapter(params).await?;

    // Query adapter (system-level for tests)
    let adapters = db.list_all_adapters_system().await?;
    let adapter = adapters
        .into_iter()
        .find(|a| a.adapter_id.as_deref() == Some(adapter_id))
        .expect("Adapter should exist");

    // Convert to AdapterMeta (canonical metadata struct)
    let adapter_meta: AdapterMeta = adapter.into();

    // Verify all required fields are present
    assert!(!adapter_meta.id.is_empty());
    assert!(!adapter_meta.tenant_id.is_empty());
    assert!(!adapter_meta.name.is_empty());
    assert!(!adapter_meta.version.is_empty());
    assert!(!adapter_meta.hash_b3.is_empty());
    assert!(!adapter_meta.category.is_empty());
    assert!(!adapter_meta.scope.is_empty());
    assert!(!adapter_meta.tier.is_empty());

    // Verify lifecycle_state enum conversion
    assert_eq!(adapter_meta.lifecycle_state, LifecycleState::Active);

    Ok(())
}

// ============================================================================
// Test Case 4: Multiple Adapters - Consistency Check
// ============================================================================

#[tokio::test]
async fn test_multiple_adapters_metadata_consistency() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create multiple adapters
    for i in 1..=5 {
        let adapter_id = format!("test-adapter-{:03}", i);
        let params = AdapterRegistrationBuilder::new()
            .adapter_id(&adapter_id)
            .tenant_id("tenant-1")
            .name(format!("adapter-{}", i))
            .hash_b3(format!("b3:hash_{}", i))
            .rank(16)
            .tier("persistent")
            .category("code")
            .scope("global")
            .build()?;

        db.register_adapter(params).await?;
    }

    // Query all adapters (system-level for tests)
    let adapters = db.list_all_adapters_system().await?;
    assert_eq!(adapters.len(), 5, "Should have 5 adapters");

    // Verify all have version and lifecycle_state
    for adapter in adapters {
        assert!(
            !adapter.version.is_empty(),
            "Adapter {} should have version",
            adapter.adapter_id.as_deref().unwrap_or("unknown")
        );
        assert!(
            !adapter.lifecycle_state.is_empty(),
            "Adapter {} should have lifecycle_state",
            adapter.adapter_id.as_deref().unwrap_or("unknown")
        );

        // Convert to AdapterMeta
        let meta: AdapterMeta = adapter.into();
        assert!(!meta.version.is_empty());
        // Lifecycle state should parse to enum
        assert!(matches!(
            meta.lifecycle_state,
            LifecycleState::Draft
                | LifecycleState::Active
                | LifecycleState::Deprecated
                | LifecycleState::Retired
        ));
    }

    Ok(())
}

// ============================================================================
// Test Case 5: AdapterMeta Field Presence (Metadata Normalization)
// ============================================================================

#[tokio::test]
async fn test_adapter_meta_has_all_required_fields() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    let adapter_id = "test-meta-fields-001";

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .tenant_id("tenant-1")
        .name("test-adapter-meta")
        .hash_b3("b3:test_hash_meta")
        .rank(16)
        .tier("persistent")
        .category("code")
        .scope("global")
        .build()?;

    db.register_adapter(params).await?;

    let adapters = db.list_all_adapters_system().await?;
    let adapter = adapters
        .into_iter()
        .find(|a| a.adapter_id.as_deref() == Some(adapter_id))
        .expect("Adapter should exist");

    // Convert to canonical AdapterMeta
    let meta: AdapterMeta = adapter.into();

    // Verify required fields for metadata normalization
    assert_eq!(meta.id, adapter_id);
    assert_eq!(meta.tenant_id, "tenant-1");
    assert!(!meta.name.is_empty());
    assert!(!meta.version.is_empty()); // Version field
    assert!(!meta.hash_b3.is_empty());
    assert_eq!(meta.rank, 16);
    assert!(!meta.tier.is_empty());
    assert_eq!(meta.lifecycle_state, LifecycleState::Active); // Lifecycle state
    assert!(!meta.category.is_empty());
    assert!(!meta.scope.is_empty());
    assert!(!meta.created_at.is_empty());
    assert!(!meta.updated_at.is_empty());

    Ok(())
}

// ============================================================================
// Test Case 6: Database Query with Explicit Field Selection
// ============================================================================

#[tokio::test]
async fn test_explicit_field_query() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    let adapter_id = "test-explicit-001";

    let params = AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .tenant_id("tenant-1")
        .name("test-explicit")
        .hash_b3("b3:test_hash_explicit")
        .rank(16)
        .tier("persistent")
        .category("code")
        .scope("global")
        .build()?;

    db.register_adapter(params).await?;

    // Explicit query for version and lifecycle_state
    let result: (String, String) =
        sqlx::query_as("SELECT version, lifecycle_state FROM adapters WHERE adapter_id = ?")
            .bind(adapter_id)
            .fetch_one(db.pool())
            .await?;

    let (version, lifecycle_state) = result;
    assert_eq!(version, "1.0.0");
    assert_eq!(lifecycle_state, "active");

    Ok(())
}
