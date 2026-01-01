//! Migration utility tests for SQL-to-KV migration
//!
//! These tests validate the migration utilities in `kv_migration.rs`, including:
//! - Batch migration of all adapters
//! - Consistency verification between SQL and KV
//! - Handling of already-migrated adapters
//! - Migration statistics accuracy
//! - Error handling for failed migrations
//!
//! **NOTE**: One test (`test_migration_large_dataset`) is currently marked as `#[ignore]` due to
//! a KV index query bug where prefix scanning causes false matches for adapter IDs with common
//! prefixes (e.g., "test-adapter-1" incorrectly matches "test-adapter-10").
//! See `crates/adapteros-storage/src/kv/indexing.rs` line 56-57 for the prefix scan implementation.
//!
//! This issue doesn't affect production use cases with UUIDs or properly namespaced adapter IDs.

use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::kv_migration::{MigrationDiscrepancy, MigrationStats};
use adapteros_db::Db;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

/// Helper function to create a test database with persistent storage and KV backend
///
/// Creates a temporary SQLite database with migrations applied, a default tenant,
/// and an initialized KV backend for testing migration utilities.
///
/// Note: Storage mode defaults to SqlOnly, so initial adapter registrations go to SQL only.
async fn create_test_db_with_kv() -> (Db, TempDir, TempDir) {
    let temp_sql_dir = new_test_tempdir();
    let temp_kv_dir = new_test_tempdir();

    let db_path = temp_sql_dir.path().join("test.db");
    let kv_path = temp_kv_dir.path().join("test.redb");

    let mut db = Db::connect(db_path.to_str().unwrap()).await.unwrap();
    db.migrate().await.unwrap();

    // Initialize KV backend (but storage mode remains SqlOnly by default)
    db.init_kv_backend(&kv_path).unwrap();

    // Verify we're in SqlOnly mode
    assert_eq!(
        db.storage_mode(),
        adapteros_db::StorageMode::SqlOnly,
        "Storage mode should be SqlOnly after init_kv_backend"
    );

    // Create default tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    (db, temp_sql_dir, temp_kv_dir)
}

/// Helper to create test adapters with various configurations
async fn create_test_adapters(db: &Db, count: usize) -> Vec<String> {
    let mut adapter_ids = Vec::new();

    for i in 0..count {
        let params = AdapterRegistrationBuilder::new()
            .adapter_id(&format!("test-adapter-{}", i))
            .name(&format!("Test Adapter {}", i))
            .hash_b3(&format!("b3:hash_{}", i))
            .rank((i % 3 + 1) as i32 * 8) // Vary ranks: 8, 16, 24
            .tier(match i % 3 {
                0 => "persistent",
                1 => "warm",
                _ => "ephemeral",
            })
            .category("code")
            .scope("global")
            .framework(Some("rust".to_string()))
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();
        adapter_ids.push(format!("test-adapter-{}", i));
    }

    adapter_ids
}

#[tokio::test]
async fn test_migrate_adapters_to_kv_success() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create test adapters in SQL only
    let adapter_ids = create_test_adapters(&db, 10).await;

    // Verify adapters exist in SQL
    for adapter_id in &adapter_ids {
        let adapter = db.get_adapter(adapter_id).await.unwrap();
        assert!(
            adapter.is_some(),
            "Adapter {} should exist in SQL",
            adapter_id
        );
    }

    // Run migration
    let stats = db.migrate_adapters_to_kv().await.unwrap();

    // Verify migration stats
    assert_eq!(stats.total, 10, "Should report 10 total adapters");
    assert_eq!(stats.migrated, 10, "Should migrate all 10 adapters");
    assert_eq!(stats.failed, 0, "Should have no failures");
    assert_eq!(stats.skipped, 0, "Should have no skips on first migration");
    assert!(stats.is_success(), "Migration should be successful");
    assert_eq!(stats.success_rate(), 100.0, "Success rate should be 100%");

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migrate_adapter_to_kv_single() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create a single adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("single-test")
        .name("Single Test Adapter")
        .hash_b3("b3:single_hash")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Migrate single adapter
    let migrated = db.migrate_adapter_to_kv("single-test").await.unwrap();

    // Should return true for successful migration
    assert!(migrated, "Single adapter migration should return true");

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migrate_adapter_to_kv_already_migrated() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("already-migrated")
        .name("Already Migrated Adapter")
        .hash_b3("b3:already_hash")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // First migration - should succeed
    let first_result = db.migrate_adapter_to_kv("already-migrated").await.unwrap();
    assert!(first_result, "First migration should return true");

    // Second migration - should detect already migrated and skip
    let second_result = db.migrate_adapter_to_kv("already-migrated").await.unwrap();
    assert!(
        !second_result,
        "Second migration should return false (already migrated)"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migrate_adapters_skip_already_migrated() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create test adapters
    create_test_adapters(&db, 5).await;

    // First migration
    let stats1 = db.migrate_adapters_to_kv().await.unwrap();
    assert_eq!(stats1.migrated, 5, "First migration should migrate all 5");
    assert_eq!(stats1.skipped, 0, "First migration should skip none");

    // Second migration - all should be skipped
    let stats2 = db.migrate_adapters_to_kv().await.unwrap();
    assert_eq!(stats2.total, 5, "Should still report 5 total");
    assert_eq!(stats2.migrated, 0, "Second migration should migrate none");
    assert_eq!(stats2.skipped, 5, "Second migration should skip all 5");
    assert_eq!(stats2.failed, 0, "Should have no failures");

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_stats_accuracy() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create mix of adapters
    create_test_adapters(&db, 20).await;

    // Run migration
    let stats = db.migrate_adapters_to_kv().await.unwrap();

    // Verify stats structure
    assert_eq!(stats.total, 20, "Total should be 20");
    assert_eq!(
        stats.migrated + stats.failed + stats.skipped,
        20,
        "Migrated + failed + skipped should equal total"
    );

    // Verify success metrics
    assert!(
        stats.is_success(),
        "Migration with no failures should be success"
    );
    assert_eq!(stats.success_rate(), 100.0, "Success rate should be 100%");
    assert!(
        stats.failed_ids.is_empty(),
        "Failed IDs list should be empty"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_stats_partial_success() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create adapters
    create_test_adapters(&db, 10).await;

    // Migrate first 5
    for i in 0..5 {
        db.migrate_adapter_to_kv(&format!("test-adapter-{}", i))
            .await
            .unwrap();
    }

    // Run full migration - should skip first 5, migrate last 5
    let stats = db.migrate_adapters_to_kv().await.unwrap();

    assert_eq!(stats.total, 10, "Total should be 10");
    assert_eq!(stats.migrated, 5, "Should migrate 5 new adapters");
    assert_eq!(stats.skipped, 5, "Should skip 5 already migrated");
    assert_eq!(stats.failed, 0, "Should have no failures");

    // Success rate = (migrated + skipped) / total - skipped count as success
    assert_eq!(
        stats.success_rate(),
        100.0,
        "Success rate should be 100% (all processed without failures)"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_verify_migration_consistency_success() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create and migrate adapters
    let adapter_ids = create_test_adapters(&db, 5).await;

    // Debug: Check what's in SQL
    println!("Created adapters:");
    for adapter_id in &adapter_ids {
        let sql_adapter = db.get_adapter(adapter_id).await.unwrap().unwrap();
        println!(
            "  - SQL adapter: id={}, adapter_id={:?}, tenant_id={}",
            sql_adapter.id, sql_adapter.adapter_id, sql_adapter.tenant_id
        );
    }

    let stats = db.migrate_adapters_to_kv().await.unwrap();

    println!(
        "\nMigration stats: migrated={}, failed={}, skipped={}",
        stats.migrated, stats.failed, stats.skipped
    );

    // Verify consistency
    let discrepancies = db.verify_migration_consistency().await.unwrap();

    // Debug: print discrepancies if any
    if !discrepancies.is_empty() {
        println!("\nFound {} discrepancies:", discrepancies.len());
        for d in &discrepancies {
            println!(
                "  - adapter_id={}, field={}, sql={}, kv={}",
                d.adapter_id, d.field, d.sql_value, d.kv_value
            );
        }
    }

    // Should have no discrepancies
    assert!(
        discrepancies.is_empty(),
        "Should have no discrepancies after clean migration"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_verify_migration_consistency_detects_missing() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create adapters but don't migrate them all
    create_test_adapters(&db, 5).await;

    // Only migrate first 3
    for i in 0..3 {
        db.migrate_adapter_to_kv(&format!("test-adapter-{}", i))
            .await
            .unwrap();
    }

    // Verify consistency - should detect 2 missing
    let discrepancies = db.verify_migration_consistency().await.unwrap();

    assert_eq!(discrepancies.len(), 2, "Should detect 2 missing adapters");

    // Verify discrepancies are for the right adapters
    for discrepancy in &discrepancies {
        assert_eq!(
            discrepancy.field, "_existence",
            "Should be existence discrepancy"
        );
        assert_eq!(
            discrepancy.sql_value, "exists",
            "SQL value should be 'exists'"
        );
        assert_eq!(
            discrepancy.kv_value, "missing",
            "KV value should be 'missing'"
        );
        assert!(
            discrepancy.adapter_id == "test-adapter-3"
                || discrepancy.adapter_id == "test-adapter-4",
            "Discrepancy should be for adapter 3 or 4"
        );
    }

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migrate_adapters_batch() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create larger dataset
    create_test_adapters(&db, 50).await;

    // Migrate in batches of 10
    let stats = db.migrate_adapters_batch(10).await.unwrap();

    assert_eq!(stats.total, 50, "Should report 50 total adapters");
    assert_eq!(
        stats.migrated + stats.skipped,
        50,
        "Should process all 50 adapters (migrated or skipped)"
    );
    assert_eq!(stats.failed, 0, "Should have no failures");
    assert_eq!(stats.success_rate(), 100.0, "Success rate should be 100%");

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migrate_tenant_adapters() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create second tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-2', 'Second Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    // Create adapters for default tenant
    create_test_adapters(&db, 5).await;

    // Create adapters for second tenant
    for i in 0..3 {
        let params = AdapterRegistrationBuilder::new()
            .tenant_id("tenant-2")
            .adapter_id(&format!("tenant2-adapter-{}", i))
            .name(&format!("Tenant 2 Adapter {}", i))
            .hash_b3(&format!("b3:t2_hash_{}", i))
            .rank(16)
            .tier("warm")
            .category("code")
            .scope("global")
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();
    }

    // Migrate only default tenant
    let stats = db.migrate_tenant_adapters("default-tenant").await.unwrap();

    assert_eq!(
        stats.total, 5,
        "Should report 5 adapters for default tenant"
    );
    assert_eq!(
        stats.migrated, 5,
        "Should migrate all 5 default tenant adapters"
    );

    // Verify tenant-2 adapters were NOT migrated by checking overall stats
    let all_stats = db.migrate_adapters_to_kv().await.unwrap();
    assert_eq!(all_stats.total, 8, "Should report 8 total adapters (5+3)");
    assert_eq!(
        all_stats.migrated, 3,
        "Should migrate 3 new adapters (tenant-2)"
    );
    assert_eq!(
        all_stats.skipped, 5,
        "Should skip 5 already migrated (default-tenant)"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migrate_with_progress_callback() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create adapters
    create_test_adapters(&db, 10).await;

    // Track progress calls
    let progress_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let progress_count_clone = progress_count.clone();

    // Migrate with progress callback
    let stats = db
        .migrate_with_progress(|progress| {
            progress_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            // Verify progress structure
            assert!(
                progress.processed <= progress.total,
                "Processed should not exceed total"
            );
            assert!(
                progress.percentage() <= 100.0,
                "Percentage should not exceed 100%"
            );
            assert!(progress.batch >= 1, "Batch number should be at least 1");
        })
        .await
        .unwrap();

    // Verify stats
    assert_eq!(stats.total, 10, "Should report 10 total");
    assert_eq!(stats.migrated, 10, "Should migrate all 10");

    // Verify progress callback was called (once per migrated adapter, not for skipped)
    let final_count = progress_count.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        final_count, 10,
        "Progress callback should be called 10 times"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_preserves_adapter_fields() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create adapter with all fields populated
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("full-fields-test")
        .name("Full Fields Test")
        .hash_b3("b3:full_hash")
        .rank(24)
        .alpha(48.0)
        .tier("persistent")
        .category("framework")
        .scope("tenant")
        .framework(Some("pytorch".to_string()))
        .framework_id(Some("pytorch-1.0".to_string()))
        .framework_version(Some("1.0.0".to_string()))
        .intent(Some("Test adapter with all fields".to_string()))
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Get original adapter from SQL
    let sql_adapter = db
        .get_adapter_for_tenant("default-tenant", "full-fields-test")
        .await
        .unwrap()
        .unwrap();

    // Migrate
    db.migrate_adapter_to_kv("full-fields-test").await.unwrap();

    // Verify no discrepancies
    let discrepancies = db.verify_migration_consistency().await.unwrap();
    assert!(
        discrepancies.is_empty(),
        "Should have no field discrepancies"
    );

    // Additional verification: check critical fields
    assert_eq!(sql_adapter.name, "Full Fields Test");
    assert_eq!(sql_adapter.rank, 24);
    assert_eq!(sql_adapter.alpha, 48.0);
    assert_eq!(sql_adapter.tier, "persistent");
    assert_eq!(sql_adapter.category, "framework");
    assert_eq!(sql_adapter.scope, "tenant");
    assert_eq!(sql_adapter.framework, Some("pytorch".to_string()));

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_adapter_not_found() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Try to migrate non-existent adapter
    let result = db.migrate_adapter_to_kv("non-existent").await;

    // Should return NotFound error
    assert!(
        result.is_err(),
        "Should return error for non-existent adapter"
    );

    let err = result.unwrap_err();
    assert!(
        matches!(err, adapteros_core::AosError::NotFound(_)),
        "Should be NotFound error, got: {:?}",
        err
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_rollback_kv_data() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create and migrate adapters
    create_test_adapters(&db, 5).await;
    db.migrate_adapters_to_kv().await.unwrap();

    // Verify migration succeeded
    let discrepancies_before = db.verify_migration_consistency().await.unwrap();
    assert!(
        discrepancies_before.is_empty(),
        "Migration should be consistent before rollback"
    );

    // Rollback KV data
    db.rollback_kv_data().await.unwrap();

    // Verify KV data is gone but SQL data remains
    let discrepancies_after = db.verify_migration_consistency().await.unwrap();
    assert_eq!(
        discrepancies_after.len(),
        5,
        "All 5 adapters should be missing from KV after rollback"
    );

    // Verify all discrepancies are existence issues
    for discrepancy in &discrepancies_after {
        assert_eq!(discrepancy.field, "_existence");
        assert_eq!(discrepancy.sql_value, "exists");
        assert_eq!(discrepancy.kv_value, "missing");
    }

    // Verify SQL adapters still exist
    let sql_adapters = db.list_adapters_by_tenant("default-tenant").await.unwrap();
    assert_eq!(
        sql_adapters.len(),
        5,
        "SQL adapters should remain after KV rollback"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_rollback_kv_data_when_empty() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Rollback when KV is empty (no-op)
    let result = db.rollback_kv_data().await;

    // Should succeed without error
    assert!(result.is_ok(), "Rollback of empty KV should succeed");

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_discrepancy_structure() {
    // Test the MigrationDiscrepancy structure
    let discrepancy = MigrationDiscrepancy {
        adapter_id: "test-123".to_string(),
        field: "name".to_string(),
        sql_value: "Old Name".to_string(),
        kv_value: "New Name".to_string(),
    };

    assert_eq!(discrepancy.adapter_id, "test-123");
    assert_eq!(discrepancy.field, "name");
    assert_eq!(discrepancy.sql_value, "Old Name");
    assert_eq!(discrepancy.kv_value, "New Name");
}

#[tokio::test]
async fn test_migration_stats_zero_total() {
    let stats = MigrationStats {
        total: 0,
        migrated: 0,
        failed: 0,
        skipped: 0,
        failed_ids: vec![],
    };

    // Zero total should not be success (nothing to migrate)
    assert!(!stats.is_success(), "Zero total should not be success");
    assert_eq!(
        stats.success_rate(),
        0.0,
        "Success rate should be 0% for zero total"
    );
}

#[tokio::test]
async fn test_migration_stats_with_failures() {
    let stats = MigrationStats {
        total: 100,
        migrated: 95,
        failed: 5,
        skipped: 0,
        failed_ids: vec![
            "adapter-1".to_string(),
            "adapter-2".to_string(),
            "adapter-3".to_string(),
            "adapter-4".to_string(),
            "adapter-5".to_string(),
        ],
    };

    assert!(!stats.is_success(), "Should not be success with failures");
    assert_eq!(stats.success_rate(), 95.0, "Success rate should be 95%");
    assert_eq!(stats.failed_ids.len(), 5, "Should track 5 failed IDs");
}

#[tokio::test]
async fn test_migration_large_dataset() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create larger dataset (100 adapters)
    create_test_adapters(&db, 100).await;

    // Migrate with batch size
    let stats = db.migrate_adapters_batch(25).await.unwrap();

    assert_eq!(stats.total, 100, "Should report 100 total");
    assert_eq!(stats.migrated, 100, "Should migrate all 100");
    assert_eq!(stats.failed, 0, "Should have no failures");
    assert_eq!(stats.success_rate(), 100.0, "Success rate should be 100%");

    // Verify consistency
    let discrepancies = db.verify_migration_consistency().await.unwrap();
    assert!(
        discrepancies.is_empty(),
        "Large dataset migration should be consistent"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_with_lineage() {
    let (db, _sql_dir, _kv_dir) = create_test_db_with_kv().await;

    // Create parent adapter
    let parent_params = AdapterRegistrationBuilder::new()
        .adapter_id("lineage-parent")
        .name("Parent Adapter")
        .hash_b3("b3:parent_hash")
        .rank(16)
        .tier("persistent")
        .category("framework")
        .scope("global")
        .build()
        .unwrap();

    let parent_uuid = db.register_adapter(parent_params).await.unwrap();

    // Create child adapters
    for i in 0..3 {
        let child_params = AdapterRegistrationBuilder::new()
            .adapter_id(&format!("lineage-child-{}", i))
            .name(&format!("Child Adapter {}", i))
            .hash_b3(&format!("b3:child_{}", i))
            .rank(8)
            .tier("warm")
            .category("code")
            .scope("tenant")
            .parent_id(Some(parent_uuid.clone()))
            .fork_type(Some("extension".to_string()))
            .fork_reason(Some(format!("Test fork {}", i)))
            .build()
            .unwrap();

        db.register_adapter(child_params).await.unwrap();
    }

    // Migrate all adapters
    let stats = db.migrate_adapters_to_kv().await.unwrap();

    assert_eq!(stats.total, 4, "Should have parent + 3 children");
    assert_eq!(stats.migrated, 4, "Should migrate all 4 adapters");

    // Verify consistency (including lineage relationships)
    let discrepancies = db.verify_migration_consistency().await.unwrap();
    assert!(
        discrepancies.is_empty(),
        "Lineage migration should be consistent"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_migration_error_without_kv_backend() {
    // Create DB without KV backend
    let temp_dir = new_test_tempdir();
    let db_path = temp_dir.path().join("test.db");

    let db = Db::connect(db_path.to_str().unwrap()).await.unwrap();
    db.migrate().await.unwrap();

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    // Create adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("no-kv-test")
        .name("No KV Backend Test")
        .hash_b3("b3:no_kv_hash")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Try to migrate without KV backend
    let result = db.migrate_adapters_to_kv().await;

    // Should return Config error
    assert!(result.is_err(), "Should fail without KV backend");
    let err = result.unwrap_err();
    assert!(
        matches!(err, adapteros_core::AosError::Config(_)),
        "Should be Config error, got: {:?}",
        err
    );

    db.close().await.unwrap();
}
