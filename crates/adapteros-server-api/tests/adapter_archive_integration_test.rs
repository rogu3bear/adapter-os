//! Adapter Archive Integration Tests
//!
//! These tests verify that archived adapters are properly rejected for inference
//! and that the archive/unarchive lifecycle works correctly.

use adapteros_core::Result;
use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;

/// Test helper to create a tenant
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

/// Test helper to create an adapter
async fn create_test_adapter(db: &Db, adapter_id: &str, tenant_id: &str, name: &str) -> Result<()> {
    // Generate unique hash based on adapter_id to avoid UNIQUE constraint violation
    let hash = format!("hash_{}_12345678901234567890123456", adapter_id);
    let adapter_dir = std::path::PathBuf::from("var").join("adapters");
    std::fs::create_dir_all(&adapter_dir).map_err(|e| {
        adapteros_core::AosError::Io(format!("Failed to create adapter dir: {}", e))
    })?;
    let adapter_path = adapter_dir.join(format!("{}.aos", adapter_id));
    std::fs::write(&adapter_path, b"test").map_err(|e| {
        adapteros_core::AosError::Io(format!("Failed to create adapter file: {}", e))
    })?;

    let params = AdapterRegistrationBuilder::new()
        .name(name)
        .tenant_id(tenant_id)
        .adapter_id(adapter_id)
        .hash_b3(&hash)
        .rank(16)
        .alpha(32.0)
        .targets_json(r#"["q_proj","v_proj"]"#)
        .tier("warm")
        .category("code")
        .scope("global")
        .aos_file_path(Some(adapter_path.to_string_lossy().to_string()))
        .build();

    db.register_adapter(params?).await?;
    Ok(())
}

// =============================================================================
// TEST: Archived adapter is not loadable
// =============================================================================

#[tokio::test]
async fn test_archived_adapter_not_loadable() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create tenant and adapter
    create_test_tenant(&db, "tenant-archive-test").await?;
    create_test_adapter(
        &db,
        "adapter-to-archive",
        "tenant-archive-test",
        "Test Adapter",
    )
    .await?;

    // Verify adapter is loadable initially
    let loadable = db.is_adapter_loadable("adapter-to-archive").await?;
    assert!(loadable, "Adapter should be loadable before archiving");

    // Archive the adapter
    db.archive_adapter(
        "tenant-archive-test",
        "adapter-to-archive",
        "test_user",
        "Test archival",
    )
    .await?;

    // Verify adapter is no longer loadable
    let loadable_after = db.is_adapter_loadable("adapter-to-archive").await?;
    assert!(
        !loadable_after,
        "Adapter should NOT be loadable after archiving"
    );

    // Verify adapter is marked as archived
    let adapter = db
        .get_adapter_for_tenant("tenant-archive-test", "adapter-to-archive")
        .await?
        .expect("Adapter should exist");
    assert!(
        adapter.archived_at.is_some(),
        "Adapter should have archived_at timestamp"
    );
    assert_eq!(
        adapter.archived_by.as_deref(),
        Some("test_user"),
        "Archived by should be recorded"
    );
    assert_eq!(
        adapter.archive_reason.as_deref(),
        Some("Test archival"),
        "Archive reason should be recorded"
    );

    Ok(())
}

// =============================================================================
// TEST: Unarchived adapter becomes loadable again
// =============================================================================

#[tokio::test]
async fn test_unarchived_adapter_becomes_loadable() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create tenant and adapter
    create_test_tenant(&db, "tenant-unarchive-test").await?;
    create_test_adapter(
        &db,
        "adapter-to-unarchive",
        "tenant-unarchive-test",
        "Test Adapter",
    )
    .await?;

    // Archive the adapter
    db.archive_adapter(
        "tenant-unarchive-test",
        "adapter-to-unarchive",
        "test_user",
        "Temporary archival",
    )
    .await?;

    // Verify adapter is not loadable
    let loadable = db.is_adapter_loadable("adapter-to-unarchive").await?;
    assert!(!loadable, "Adapter should NOT be loadable after archiving");

    // Unarchive the adapter
    db.unarchive_adapter("tenant-unarchive-test", "adapter-to-unarchive")
        .await?;

    // Verify adapter is loadable again
    let loadable_after = db.is_adapter_loadable("adapter-to-unarchive").await?;
    assert!(
        loadable_after,
        "Adapter should be loadable after unarchiving"
    );

    // Verify archive fields are cleared
    let adapter = db
        .get_adapter_for_tenant("tenant-unarchive-test", "adapter-to-unarchive")
        .await?
        .expect("Adapter should exist");
    assert!(
        adapter.archived_at.is_none(),
        "Archived_at should be cleared after unarchive"
    );
    assert!(
        adapter.archived_by.is_none(),
        "Archived_by should be cleared after unarchive"
    );
    assert!(
        adapter.archive_reason.is_none(),
        "Archive_reason should be cleared after unarchive"
    );

    Ok(())
}

// =============================================================================
// TEST: Purged adapter is not loadable and cannot be unarchived
// =============================================================================

#[tokio::test]
async fn test_purged_adapter_not_loadable() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create tenant and adapter
    create_test_tenant(&db, "tenant-purge-test").await?;
    create_test_adapter(&db, "adapter-to-purge", "tenant-purge-test", "Test Adapter").await?;

    // Archive the adapter first (required before purging)
    db.archive_adapter(
        "tenant-purge-test",
        "adapter-to-purge",
        "test_user",
        "Pre-purge archival",
    )
    .await?;

    // Mark adapter as purged (simulates GC)
    db.mark_adapter_purged("tenant-purge-test", "adapter-to-purge")
        .await?;

    // Verify adapter is not loadable
    let loadable = db.is_adapter_loadable("adapter-to-purge").await?;
    assert!(!loadable, "Purged adapter should NOT be loadable");

    // Verify purged_at is set
    let adapter = db
        .get_adapter_for_tenant("tenant-purge-test", "adapter-to-purge")
        .await?
        .expect("Adapter should exist");
    assert!(
        adapter.purged_at.is_some(),
        "Adapter should have purged_at timestamp"
    );
    assert!(
        adapter.aos_file_path.is_none(),
        "File path should be cleared after purge"
    );

    // Attempting to unarchive a purged adapter should fail
    let unarchive_result = db
        .unarchive_adapter("tenant-purge-test", "adapter-to-purge")
        .await;
    assert!(
        unarchive_result.is_err(),
        "Cannot unarchive a purged adapter"
    );

    Ok(())
}

// =============================================================================
// TEST: Tenant archival cascades to all adapters
// =============================================================================

#[tokio::test]
async fn test_tenant_archival_cascades_to_adapters() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create tenant with multiple adapters
    create_test_tenant(&db, "tenant-cascade-test").await?;
    create_test_adapter(&db, "cascade-adapter-1", "tenant-cascade-test", "Adapter 1").await?;
    create_test_adapter(&db, "cascade-adapter-2", "tenant-cascade-test", "Adapter 2").await?;
    create_test_adapter(&db, "cascade-adapter-3", "tenant-cascade-test", "Adapter 3").await?;

    // All adapters should be loadable initially
    assert!(db.is_adapter_loadable("cascade-adapter-1").await?);
    assert!(db.is_adapter_loadable("cascade-adapter-2").await?);
    assert!(db.is_adapter_loadable("cascade-adapter-3").await?);

    // Archive all adapters for the tenant
    let archived_count = db
        .archive_adapters_for_tenant("tenant-cascade-test", "system", "tenant_archived")
        .await?;

    assert_eq!(archived_count, 3, "Should archive 3 adapters");

    // All adapters should be not loadable now
    assert!(
        !db.is_adapter_loadable("cascade-adapter-1").await?,
        "Adapter 1 should not be loadable"
    );
    assert!(
        !db.is_adapter_loadable("cascade-adapter-2").await?,
        "Adapter 2 should not be loadable"
    );
    assert!(
        !db.is_adapter_loadable("cascade-adapter-3").await?,
        "Adapter 3 should not be loadable"
    );

    Ok(())
}

// =============================================================================
// TEST: Archived adapter count tracking
// =============================================================================

#[tokio::test]
async fn test_archived_adapter_count() -> Result<()> {
    // Setup: Create in-memory database
    let db = Db::new_in_memory().await?;

    // Create tenant with adapters
    create_test_tenant(&db, "tenant-count-test").await?;
    create_test_adapter(&db, "count-adapter-1", "tenant-count-test", "Adapter 1").await?;
    create_test_adapter(&db, "count-adapter-2", "tenant-count-test", "Adapter 2").await?;

    // Initial count should be zero
    let archived = db.count_archived_adapters("tenant-count-test").await?;
    assert_eq!(archived, 0, "No adapters archived initially");

    // Archive one adapter
    db.archive_adapter("tenant-count-test", "count-adapter-1", "test_user", "Test")
        .await?;

    let archived = db.count_archived_adapters("tenant-count-test").await?;
    assert_eq!(archived, 1, "One adapter archived");

    // Purge the archived adapter (purged adapters are not counted by count_archived_adapters)
    db.mark_adapter_purged("tenant-count-test", "count-adapter-1")
        .await?;

    let archived = db.count_archived_adapters("tenant-count-test").await?;
    assert_eq!(archived, 0, "Purged adapters are not counted as archived");

    Ok(())
}
