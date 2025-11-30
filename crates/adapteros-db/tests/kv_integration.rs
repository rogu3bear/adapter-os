//! Integration tests for dual-write KV storage system
//!
//! These tests validate the SQL-to-KV migration path, ensuring:
//! - Dual-write mode correctly writes to both SQL and KV stores
//! - KvPrimary mode reads from KV with SQL fallback
//! - Data integrity is maintained across storage modes
//! - Lineage queries produce identical results between SQL CTE and KV traversal
//! - Storage mode transitions work correctly

use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use tempfile::TempDir;

// NOTE: StorageMode is assumed to be defined in adapteros_db or adapteros_core
// This is a placeholder definition for the test structure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum StorageMode {
    /// SQL-only mode (legacy, current implementation)
    SqlOnly,
    /// Dual-write mode: writes go to both SQL and KV, reads from SQL
    DualWrite,
    /// KV-primary mode: reads from KV, writes to both, SQL is fallback
    KvPrimary,
}

/// Helper function to create a test database with a specific storage mode
///
/// Creates an in-memory SQLite database with migrations applied and
/// a default tenant configured for testing.
///
/// NOTE: This assumes Db::connect_with_mode() or similar API exists
/// For now, we'll use the standard connect and simulate mode switching
async fn create_test_db(_mode: StorageMode) -> Db {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    // Create default tenant for testing
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    db
}

/// Helper function to create a test database with persistent storage
///
/// Uses a temporary directory that's automatically cleaned up after the test
async fn create_test_db_persistent(_mode: StorageMode) -> (Db, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db_path_str = db_path.to_str().unwrap();

    let db = Db::connect(db_path_str).await.unwrap();
    db.migrate().await.unwrap();

    // Create default tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    (db, temp_dir)
}

/// Cleanup helper - ensures database is properly closed
async fn cleanup_test_db(db: &Db) {
    db.close().await.unwrap();
}

#[tokio::test]
async fn test_dual_write_tenant() {
    // Create Db with DualWrite mode
    let db = create_test_db(StorageMode::DualWrite).await;

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();
    assert!(!tenant_id.is_empty());

    // Verify it exists in SQL
    let tenant_sql = db.get_tenant(&tenant_id).await.unwrap();
    assert!(tenant_sql.is_some());
    assert_eq!(tenant_sql.unwrap().name, "test-tenant");

    // TODO: Once KV API exists, verify it also exists in KV store
    // let tenant_kv = db.get_tenant_from_kv(&tenant_id).await.unwrap();
    // assert!(tenant_kv.is_some());
    // assert_eq!(tenant_kv.unwrap().name, "test-tenant");

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_dual_write_adapter() {
    // Create Db with DualWrite mode
    let db = create_test_db(StorageMode::DualWrite).await;

    // Create an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("dual-write-test-1")
        .name("Dual Write Test Adapter")
        .hash_b3("b3:dual_write_hash")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    let adapter_id = db.register_adapter(params).await.unwrap();
    assert!(!adapter_id.is_empty());

    // Verify it exists in SQL
    let adapter_sql = db
        .get_adapter("dual-write-test-1")
        .await
        .unwrap();
    assert!(adapter_sql.is_some());
    let adapter = adapter_sql.unwrap();
    assert_eq!(adapter.name, "Dual Write Test Adapter");
    assert_eq!(adapter.hash_b3, "b3:dual_write_hash");
    assert_eq!(adapter.rank, 16);

    // TODO: Verify it exists in KV store
    // let adapter_kv = db.get_adapter_from_kv("default-tenant", "dual-write-test-1").await.unwrap();
    // assert!(adapter_kv.is_some());
    // assert_eq!(adapter_kv.unwrap().name, "Dual Write Test Adapter");

    // Update adapter
    db.increment_adapter_activation("dual-write-test-1")
        .await
        .unwrap();

    // Verify both SQL and KV are updated
    let adapter_after = db
        .get_adapter("dual-write-test-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(adapter_after.activation_count, 1);

    // TODO: Verify KV is also updated
    // let adapter_kv_after = db.get_adapter_from_kv("default-tenant", "dual-write-test-1").await.unwrap().unwrap();
    // assert_eq!(adapter_kv_after.activation_count, 1);

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_kv_primary_read() {
    // Create Db with KvPrimary mode
    let db = create_test_db(StorageMode::KvPrimary).await;

    // First, write data in SqlOnly mode to populate SQL
    // (Simulating migration scenario)
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("kv-primary-test-1")
        .name("KV Primary Test Adapter")
        .hash_b3("b3:kv_primary_hash")
        .rank(24)
        .tier("persistent")
        .category("framework")
        .scope("tenant")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // TODO: Manually populate KV with test data
    // db.insert_adapter_to_kv("default-tenant", "kv-primary-test-1", adapter_data).await.unwrap();

    // Read should come from KV (once KV implementation exists)
    // For now, this tests the SQL path still works
    let adapter = db
        .get_adapter("kv-primary-test-1")
        .await
        .unwrap();
    assert!(adapter.is_some());
    assert_eq!(adapter.unwrap().name, "KV Primary Test Adapter");

    // TODO: Verify read came from KV, not SQL
    // assert!(db.get_last_read_source() == ReadSource::Kv);

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_storage_mode_switch() {
    // Start in SqlOnly
    let (db, _temp_dir) = create_test_db_persistent(StorageMode::SqlOnly).await;

    // Create adapter in SqlOnly mode
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("mode-switch-test-1")
        .name("Mode Switch Test Adapter")
        .hash_b3("b3:mode_switch_hash")
        .rank(12)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Verify it exists in SQL
    let adapter_sql_only = db
        .get_adapter("mode-switch-test-1")
        .await
        .unwrap();
    assert!(adapter_sql_only.is_some());

    // TODO: Switch to DualWrite mode
    // db.set_storage_mode(StorageMode::DualWrite).await.unwrap();

    // Create new adapter - should go to both SQL and KV
    let params2 = AdapterRegistrationBuilder::new()
        .adapter_id("mode-switch-test-2")
        .name("Second Adapter After Switch")
        .hash_b3("b3:mode_switch_hash_2")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params2).await.unwrap();

    // Verify writes go to both
    let adapter_dual = db
        .get_adapter("mode-switch-test-2")
        .await
        .unwrap();
    assert!(adapter_dual.is_some());

    // TODO: Verify it also exists in KV
    // let adapter_kv = db.get_adapter_from_kv("default-tenant", "mode-switch-test-2").await.unwrap();
    // assert!(adapter_kv.is_some());

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_lineage_kv_vs_sql() {
    // Create adapter hierarchy to test lineage queries
    let db = create_test_db(StorageMode::DualWrite).await;

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
    let mut child_uuids = Vec::new();
    for i in 1..=3 {
        let child_params = AdapterRegistrationBuilder::new()
            .adapter_id(&format!("lineage-child-{}", i))
            .name(&format!("Child Adapter {}", i))
            .hash_b3(&format!("b3:child_hash_{}", i))
            .rank(8)
            .tier("warm")
            .category("code")
            .scope("tenant")
            .parent_id(Some(parent_uuid.clone()))
            .fork_type(Some("extension".to_string()))
            .fork_reason(Some(format!("Test fork {}", i)))
            .build()
            .unwrap();

        let child_uuid = db.register_adapter(child_params).await.unwrap();
        child_uuids.push(child_uuid);
    }

    // Create grandchild adapter (child of first child)
    let grandchild_params = AdapterRegistrationBuilder::new()
        .adapter_id("lineage-grandchild-1")
        .name("Grandchild Adapter")
        .hash_b3("b3:grandchild_hash")
        .rank(4)
        .tier("ephemeral")
        .category("code")
        .scope("session")
        .parent_id(Some(child_uuids[0].clone()))
        .fork_type(Some("independent".to_string()))
        .fork_reason(Some("Testing lineage".to_string()))
        .build()
        .unwrap();

    db.register_adapter(grandchild_params).await.unwrap();

    // Query lineage using SQL CTE (existing implementation)
    let lineage_sql = db
        .get_adapter_lineage("lineage-parent")
        .await
        .unwrap();

    // Should find parent + 3 children + 1 grandchild = 5 adapters
    assert_eq!(lineage_sql.len(), 5, "Should find complete lineage tree");

    // Verify parent is included
    assert!(
        lineage_sql
            .iter()
            .any(|a| a.adapter_id.as_deref() == Some("lineage-parent")),
        "Parent should be in lineage"
    );

    // Verify all children are included
    for i in 1..=3 {
        assert!(
            lineage_sql
                .iter()
                .any(|a| a.adapter_id.as_deref() == Some(&format!("lineage-child-{}", i))),
            "Child {} should be in lineage",
            i
        );
    }

    // Verify grandchild is included
    assert!(
        lineage_sql
            .iter()
            .any(|a| a.adapter_id.as_deref() == Some("lineage-grandchild-1")),
        "Grandchild should be in lineage"
    );

    // TODO: Query lineage using KV traversal
    // let lineage_kv = db.get_adapter_lineage_from_kv("default-tenant", "lineage-parent").await.unwrap();

    // TODO: Compare results - they should match exactly
    // assert_eq!(lineage_sql.len(), lineage_kv.len(), "SQL and KV lineage should have same count");
    //
    // let sql_ids: HashSet<_> = lineage_sql.iter().filter_map(|a| a.adapter_id.as_ref()).collect();
    // let kv_ids: HashSet<_> = lineage_kv.iter().filter_map(|a| a.adapter_id.as_ref()).collect();
    // assert_eq!(sql_ids, kv_ids, "SQL and KV lineage should contain same adapter IDs");

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_migration_data_integrity() {
    // Create data in SQL-only mode
    let db = create_test_db(StorageMode::SqlOnly).await;

    // Create multiple adapters with various configurations
    let test_adapters = vec![
        ("migrate-1", "Migration Test 1", "b3:migrate_hash_1", 16, "persistent"),
        ("migrate-2", "Migration Test 2", "b3:migrate_hash_2", 8, "warm"),
        ("migrate-3", "Migration Test 3", "b3:migrate_hash_3", 24, "ephemeral"),
    ];

    for (adapter_id, name, hash, rank, tier) in &test_adapters {
        let params = AdapterRegistrationBuilder::new()
            .adapter_id(*adapter_id)
            .name(*name)
            .hash_b3(*hash)
            .rank(*rank)
            .tier(*tier)
            .category("code")
            .scope("global")
            .framework(Some("rust".to_string()))
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();
    }

    // Get all adapters from SQL before migration
    let adapters_before = db.list_adapters_by_tenant("default-tenant").await.unwrap();
    assert_eq!(adapters_before.len(), 3, "Should have 3 adapters in SQL");

    // TODO: Run migration to KV
    // db.migrate_sql_to_kv().await.unwrap();

    // TODO: Verify all data exists in KV
    // let adapters_kv = db.list_adapters_from_kv("default-tenant").await.unwrap();
    // assert_eq!(adapters_kv.len(), 3, "Should have 3 adapters in KV after migration");

    // Verify each adapter's data integrity
    for (adapter_id, name, hash, rank, tier) in &test_adapters {
        let adapter_sql = db
            .get_adapter(adapter_id)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(adapter_sql.name, *name);
        assert_eq!(adapter_sql.hash_b3, *hash);
        assert_eq!(adapter_sql.rank, *rank);
        assert_eq!(adapter_sql.tier, *tier);
        assert_eq!(adapter_sql.framework.as_deref(), Some("rust"));

        // TODO: Verify KV matches SQL exactly
        // let adapter_kv = db.get_adapter_from_kv("default-tenant", adapter_id).await.unwrap().unwrap();
        // assert_eq!(adapter_sql.name, adapter_kv.name);
        // assert_eq!(adapter_sql.hash_b3, adapter_kv.hash_b3);
        // assert_eq!(adapter_sql.rank, adapter_kv.rank);
        // assert_eq!(adapter_sql.tier, adapter_kv.tier);
        // assert_eq!(adapter_sql.framework, adapter_kv.framework);
    }

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_dual_write_atomicity() {
    // Ensure that dual writes are atomic - both succeed or both fail
    let db = create_test_db(StorageMode::DualWrite).await;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("atomic-test-1")
        .name("Atomic Test Adapter")
        .hash_b3("b3:atomic_hash")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    // This should succeed - both SQL and KV writes complete
    let result = db.register_adapter(params).await;
    assert!(result.is_ok(), "Dual write should succeed atomically");

    // Verify in SQL
    let adapter_sql = db
        .get_adapter("atomic-test-1")
        .await
        .unwrap();
    assert!(adapter_sql.is_some());

    // TODO: Verify in KV
    // let adapter_kv = db.get_adapter_from_kv("default-tenant", "atomic-test-1").await.unwrap();
    // assert!(adapter_kv.is_some());

    // TODO: Test failure scenario - if KV write fails, SQL write should rollback
    // This would require injecting a KV failure
    // db.inject_kv_failure_for_next_write();
    // let params2 = AdapterRegistrationBuilder::new()...;
    // let result2 = db.register_adapter(params2).await;
    // assert!(result2.is_err(), "Should fail if KV write fails");
    //
    // // Verify SQL was also rolled back
    // let adapter_should_not_exist = db.get_adapter_by_id("default-tenant", "atomic-test-2").await.unwrap();
    // assert!(adapter_should_not_exist.is_none(), "SQL should rollback if KV fails");

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_kv_fallback_to_sql() {
    // Test that KvPrimary mode falls back to SQL if KV doesn't have the data
    let db = create_test_db(StorageMode::KvPrimary).await;

    // Insert adapter directly into SQL (simulating incomplete migration)
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("fallback-test-1")
        .name("Fallback Test Adapter")
        .hash_b3("b3:fallback_hash")
        .rank(12)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // TODO: In KvPrimary mode, if data isn't in KV, should fall back to SQL
    // Clear KV to force fallback
    // db.clear_kv_for_adapter("default-tenant", "fallback-test-1").await.unwrap();

    // Read should still succeed by falling back to SQL
    let adapter = db
        .get_adapter("fallback-test-1")
        .await
        .unwrap();
    assert!(adapter.is_some());
    assert_eq!(adapter.unwrap().name, "Fallback Test Adapter");

    // TODO: Verify that read came from SQL fallback, not KV
    // assert_eq!(db.get_last_read_source(), ReadSource::SqlFallback);

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_concurrent_dual_writes() {
    // Test that concurrent dual writes maintain consistency
    let db = create_test_db(StorageMode::DualWrite).await;

    // Create base adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("concurrent-test-base")
        .name("Concurrent Base Adapter")
        .hash_b3("b3:concurrent_hash")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Spawn multiple concurrent activation increments
    let mut handles = vec![];
    for _ in 0..10 {
        let db_clone = db.clone();
        let handle = tokio::spawn(async move {
            db_clone
                .increment_adapter_activation("concurrent-test-base")
                .await
                .unwrap();
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify final count is correct in SQL
    let adapter = db
        .get_adapter("concurrent-test-base")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        adapter.activation_count, 10,
        "All 10 increments should be reflected in SQL"
    );

    // TODO: Verify KV also has correct count
    // let adapter_kv = db.get_adapter_from_kv("default-tenant", "concurrent-test-base").await.unwrap().unwrap();
    // assert_eq!(adapter_kv.activation_count, 10, "All 10 increments should be reflected in KV");

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_kv_bulk_migration() {
    // Test bulk migration of adapters from SQL to KV
    let db = create_test_db(StorageMode::SqlOnly).await;

    // Create many adapters (simulating production data)
    for i in 0..50 {
        let params = AdapterRegistrationBuilder::new()
            .adapter_id(&format!("bulk-migrate-{}", i))
            .name(&format!("Bulk Migration Test {}", i))
            .hash_b3(&format!("b3:bulk_hash_{}", i))
            .rank((i % 3 + 1) * 8) // Vary ranks: 8, 16, 24
            .tier(if i % 3 == 0 {
                "persistent"
            } else if i % 3 == 1 {
                "warm"
            } else {
                "ephemeral"
            })
            .category("code")
            .scope("global")
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();
    }

    // Get count before migration
    let adapters_before = db.list_adapters_by_tenant("default-tenant").await.unwrap();
    assert_eq!(adapters_before.len(), 50, "Should have 50 adapters");

    // TODO: Run bulk migration
    // let migration_result = db.bulk_migrate_to_kv("default-tenant").await.unwrap();
    // assert_eq!(migration_result.migrated_count, 50);
    // assert_eq!(migration_result.failed_count, 0);

    // TODO: Verify all migrated to KV
    // let adapters_kv = db.list_adapters_from_kv("default-tenant").await.unwrap();
    // assert_eq!(adapters_kv.len(), 50, "All 50 adapters should be in KV");

    // TODO: Spot-check a few adapters for data integrity
    // for i in [0, 25, 49] {
    //     let adapter_id = format!("bulk-migrate-{}", i);
    //     let sql = db.get_adapter_by_id("default-tenant", &adapter_id).await.unwrap().unwrap();
    //     let kv = db.get_adapter_from_kv("default-tenant", &adapter_id).await.unwrap().unwrap();
    //     assert_eq!(sql.name, kv.name);
    //     assert_eq!(sql.hash_b3, kv.hash_b3);
    //     assert_eq!(sql.rank, kv.rank);
    // }

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_storage_mode_read_performance() {
    // Performance comparison test (informational, not strict)
    let db = create_test_db(StorageMode::DualWrite).await;

    // Create test adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("perf-test-1")
        .name("Performance Test Adapter")
        .hash_b3("b3:perf_hash")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Warm up
    for _ in 0..10 {
        let _ = db
            .get_adapter("perf-test-1")
            .await
            .unwrap();
    }

    // Time SQL reads
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = db
            .get_adapter("perf-test-1")
            .await
            .unwrap();
    }
    let sql_duration = start.elapsed();

    println!("SQL read 100 times: {:?}", sql_duration);

    // TODO: Time KV reads
    // let start = std::time::Instant::now();
    // for _ in 0..100 {
    //     let _ = db.get_adapter_from_kv("default-tenant", "perf-test-1").await.unwrap();
    // }
    // let kv_duration = start.elapsed();
    //
    // println!("KV read 100 times: {:?}", kv_duration);
    // println!("Speedup: {:.2}x", sql_duration.as_secs_f64() / kv_duration.as_secs_f64());

    cleanup_test_db(&db).await;
}
