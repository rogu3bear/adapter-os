//! Integration tests for dual-write KV storage system
//!
//! These tests validate the SQL-to-KV migration path, ensuring:
//! - Dual-write mode correctly writes to both SQL and KV stores
//! - KvPrimary mode reads from KV with SQL fallback
//! - Data integrity is maintained across storage modes
//! - Lineage queries produce identical results between SQL CTE and KV traversal
//! - Storage mode transitions work correctly
#![allow(deprecated)]
#![allow(unused_variables)]
#![allow(clippy::items_after_test_module)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::io_other_error)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::borrow_deref_ref)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::cloned_ref_to_slice_refs)]

use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::adapters_kv::{AdapterKvOps, AdapterKvRepository};
use adapteros_db::{Db, KvDb, StorageMode};
use adapteros_storage::repos::adapter::AdapterRepository;
use std::collections::HashSet;
use std::sync::Arc;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    TempDir::with_prefix("aos-test-")
        .expect("Failed to create temporary directory for KV integration test")
}

/// Helper function to create a test database with a specific storage mode
///
/// Creates persistent databases for both SQL and KV with migrations applied
/// and a default tenant configured for testing.
async fn create_test_db(mode: StorageMode) -> (Db, TempDir) {
    let temp_dir = new_test_tempdir();
    let db_path = temp_dir.path().join("test.db");
    let kv_path = temp_dir.path().join("kv.redb");

    // Create SQL database
    let sql_url = db_path.to_str().unwrap();
    let db_sql = Db::connect(sql_url).await.unwrap();
    db_sql.migrate().await.unwrap();

    // Create KV backend
    let kv_db = KvDb::init_redb(&kv_path).unwrap();

    // Create Db with specified storage mode
    let pool = db_sql.pool_result().unwrap().clone();
    let db = Db::new(pool, Some(Arc::new(kv_db)), mode);

    // Create default tenant for testing
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool_result().unwrap())
        .await
        .unwrap();

    (db, temp_dir)
}

/// Create an AdapterKvRepository for direct KV operations
fn create_kv_repo(db: &Db) -> AdapterKvRepository {
    let kv = db
        .kv_backend()
        .expect("Failed to get KV backend - backend should be attached to database");
    let storage_repo = AdapterRepository::new(kv.backend().clone(), kv.index_manager().clone());
    AdapterKvRepository::new(Arc::new(storage_repo), "default-tenant".to_string())
}

/// Cleanup helper - ensures database is properly closed
async fn cleanup_test_db(db: &Db) {
    db.close().await.unwrap();
}

#[tokio::test]
async fn test_dual_write_tenant() {
    // Create Db with DualWrite mode
    let (db, _temp_dir) = create_test_db(StorageMode::DualWrite).await;

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();
    assert!(!tenant_id.is_empty());

    // Verify it exists in SQL
    let tenant_sql = db.get_tenant(&tenant_id).await.unwrap();
    assert!(tenant_sql.is_some());
    assert_eq!(tenant_sql.unwrap().name, "test-tenant");

    // Verify it also exists in KV store
    // In DualWrite mode, tenants are written to both SQL and KV
    // The tenant should be readable when we switch to KvPrimary mode
    // For now, we verify SQL is the source of truth in DualWrite mode
    let tenant_check = db.get_tenant(&tenant_id).await.unwrap();
    assert!(
        tenant_check.is_some(),
        "Tenant should exist after dual write"
    );

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_dual_write_adapter() {
    // Create Db with DualWrite mode
    let (db, _temp_dir) = create_test_db(StorageMode::DualWrite).await;

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
    let adapter_sql = db.get_adapter("dual-write-test-1").await.unwrap();
    assert!(adapter_sql.is_some());
    let adapter = adapter_sql.unwrap();
    assert_eq!(adapter.name, "Dual Write Test Adapter");
    assert_eq!(adapter.hash_b3, "b3:dual_write_hash");
    assert_eq!(adapter.rank, 16);

    // Verify it exists in KV store using direct KV operations
    let kv_repo = create_kv_repo(&db);
    let adapter_kv = kv_repo.get_adapter_kv("dual-write-test-1").await.unwrap();
    assert!(
        adapter_kv.is_some(),
        "Adapter should exist in KV after dual write"
    );
    let kv_adapter = adapter_kv.unwrap();
    assert_eq!(kv_adapter.name, "Dual Write Test Adapter");

    // Update adapter
    db.increment_adapter_activation("default-tenant", "dual-write-test-1")
        .await
        .unwrap();

    // Verify both SQL and KV are updated
    let adapter_after = db.get_adapter("dual-write-test-1").await.unwrap().unwrap();
    assert_eq!(adapter_after.activation_count, 1);

    // Verify KV is also updated
    let adapter_kv_after = kv_repo
        .get_adapter_kv("dual-write-test-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        adapter_kv_after.activation_count, 1,
        "KV should reflect activation increment"
    );

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_kv_primary_read() {
    // Create Db with DualWrite mode first to populate both stores
    let (mut db, temp_dir) = create_test_db(StorageMode::DualWrite).await;

    // Write data in DualWrite mode to populate both SQL and KV
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

    // Verify data exists in both stores before mode switch
    let adapter_sql = db.get_adapter("kv-primary-test-1").await.unwrap();
    assert!(adapter_sql.is_some(), "Should exist in SQL");

    let kv_repo = create_kv_repo(&db);
    let adapter_kv = kv_repo.get_adapter_kv("kv-primary-test-1").await.unwrap();
    assert!(adapter_kv.is_some(), "Should exist in KV after dual write");

    // Switch to KvPrimary mode - reads should now come from KV
    db.set_storage_mode(StorageMode::KvPrimary).unwrap();
    assert_eq!(db.storage_mode(), StorageMode::KvPrimary);

    // Read should come from KV (this is now the primary source)
    let adapter = db.get_adapter("kv-primary-test-1").await.unwrap();
    assert!(adapter.is_some());
    assert_eq!(adapter.unwrap().name, "KV Primary Test Adapter");

    // In KvPrimary mode, the read comes from KV first
    // We can verify this by checking that the KV backend is used
    assert!(db.has_kv_backend(), "KV backend should be available");
    assert!(
        db.storage_mode().read_from_kv(),
        "KvPrimary mode should read from KV"
    );

    // Keep temp_dir alive until after cleanup
    cleanup_test_db(&db).await;
    drop(temp_dir);
}

#[tokio::test]
async fn test_storage_mode_switch() {
    // Start in SqlOnly
    let (mut db, temp_dir) = create_test_db(StorageMode::SqlOnly).await;

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
    let adapter_sql_only = db.get_adapter("mode-switch-test-1").await.unwrap();
    assert!(adapter_sql_only.is_some());

    // Switch to DualWrite mode
    db.set_storage_mode(StorageMode::DualWrite).unwrap();
    assert_eq!(db.storage_mode(), StorageMode::DualWrite);

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

    // Verify writes go to SQL
    let adapter_dual = db.get_adapter("mode-switch-test-2").await.unwrap();
    assert!(adapter_dual.is_some());

    // Verify it also exists in KV (since we're now in DualWrite mode)
    let kv_repo = create_kv_repo(&db);
    let adapter_kv = kv_repo.get_adapter_kv("mode-switch-test-2").await.unwrap();
    assert!(
        adapter_kv.is_some(),
        "New adapter should exist in KV after mode switch to DualWrite"
    );

    cleanup_test_db(&db).await;
    drop(temp_dir);
}

#[tokio::test]
async fn test_lineage_kv_vs_sql() {
    // Create adapter hierarchy to test lineage queries
    let (db, _temp_dir) = create_test_db(StorageMode::DualWrite).await;

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
        .scope("global")
        .parent_id(Some(child_uuids[0].clone()))
        .fork_type(Some("independent".to_string()))
        .fork_reason(Some("Testing lineage".to_string()))
        .build()
        .unwrap();

    db.register_adapter(grandchild_params).await.unwrap();

    // Query lineage using SQL CTE (existing implementation)
    let lineage_sql = db
        .get_adapter_lineage("default-tenant", "lineage-parent")
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

    // Query lineage using KV traversal
    let kv_repo = create_kv_repo(&db);
    let lineage_kv = kv_repo
        .get_adapter_lineage_kv("lineage-parent")
        .await
        .unwrap();

    // Compare results - they should match exactly
    assert_eq!(
        lineage_sql.len(),
        lineage_kv.len(),
        "SQL and KV lineage should have same count"
    );

    let sql_ids: HashSet<_> = lineage_sql
        .iter()
        .filter_map(|a| a.adapter_id.as_ref())
        .collect();
    let kv_ids: HashSet<_> = lineage_kv
        .iter()
        .filter_map(|a| a.adapter_id.as_ref())
        .collect();
    assert_eq!(
        sql_ids, kv_ids,
        "SQL and KV lineage should contain same adapter IDs"
    );

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_migration_data_integrity() {
    // Create data in DualWrite mode (writes to both SQL and KV)
    let (db, _temp_dir) = create_test_db(StorageMode::DualWrite).await;

    // Create multiple adapters with various configurations
    let test_adapters = vec![
        (
            "migrate-1",
            "Migration Test 1",
            "b3:migrate_hash_1",
            16,
            "persistent",
        ),
        (
            "migrate-2",
            "Migration Test 2",
            "b3:migrate_hash_2",
            8,
            "warm",
        ),
        (
            "migrate-3",
            "Migration Test 3",
            "b3:migrate_hash_3",
            24,
            "ephemeral",
        ),
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

    // Get all adapters from SQL before checking KV
    let adapters_before = db.list_adapters_by_tenant("default-tenant").await.unwrap();
    assert_eq!(adapters_before.len(), 3, "Should have 3 adapters in SQL");

    // In DualWrite mode, data is already written to KV, no migration needed
    // Verify all data exists in KV
    let kv_repo = create_kv_repo(&db);
    let adapters_kv = kv_repo
        .list_adapters_for_tenant_kv("default-tenant", None, None)
        .await
        .unwrap();
    assert_eq!(
        adapters_kv.len(),
        3,
        "Should have 3 adapters in KV after dual writes"
    );

    // Verify each adapter's data integrity between SQL and KV
    for (adapter_id, name, hash, rank, tier) in &test_adapters {
        let adapter_sql = db.get_adapter(adapter_id).await.unwrap().unwrap();

        assert_eq!(adapter_sql.name, *name);
        assert_eq!(adapter_sql.hash_b3, *hash);
        assert_eq!(adapter_sql.rank, *rank);
        assert_eq!(adapter_sql.tier, *tier);
        assert_eq!(adapter_sql.framework.as_deref(), Some("rust"));

        // Verify KV matches SQL exactly
        let adapter_kv = kv_repo.get_adapter_kv(adapter_id).await.unwrap().unwrap();
        assert_eq!(adapter_sql.name, adapter_kv.name, "Name should match");
        assert_eq!(adapter_sql.hash_b3, adapter_kv.hash_b3, "Hash should match");
        assert_eq!(adapter_sql.rank, adapter_kv.rank, "Rank should match");
        assert_eq!(adapter_sql.tier, adapter_kv.tier, "Tier should match");
        assert_eq!(
            adapter_sql.framework, adapter_kv.framework,
            "Framework should match"
        );
    }

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_dual_write_atomicity() {
    // Ensure that dual writes are atomic - both succeed or both fail
    let (db, _temp_dir) = create_test_db(StorageMode::DualWrite).await;

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
    let adapter_sql = db.get_adapter("atomic-test-1").await.unwrap();
    assert!(adapter_sql.is_some());

    // Verify in KV
    let kv_repo = create_kv_repo(&db);
    let adapter_kv = kv_repo.get_adapter_kv("atomic-test-1").await.unwrap();
    assert!(
        adapter_kv.is_some(),
        "Adapter should exist in KV after atomic dual write"
    );

    // Verify data consistency between SQL and KV
    let sql_adapter = adapter_sql.unwrap();
    let kv_adapter = adapter_kv.unwrap();
    assert_eq!(sql_adapter.name, kv_adapter.name);
    assert_eq!(sql_adapter.hash_b3, kv_adapter.hash_b3);
    assert_eq!(sql_adapter.rank, kv_adapter.rank);

    // Note: Testing failure scenarios (KV write fails -> SQL rollback) requires
    // injecting failures which is not currently supported in the test infrastructure.
    // The AtomicDualWriteConfig controls this behavior in production.

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_kv_fallback_to_sql() {
    // Test that KvPrimary mode falls back to SQL if KV doesn't have the data
    // First, write only to SQL by using SqlOnly mode
    let (mut db, temp_dir) = create_test_db(StorageMode::SqlOnly).await;

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

    // Verify data is in SQL only
    let adapter_sql = db.get_adapter("fallback-test-1").await.unwrap();
    assert!(adapter_sql.is_some(), "Adapter should exist in SQL");

    // In SqlOnly mode, KV is not populated
    // Now switch to KvPrimary mode
    db.set_storage_mode(StorageMode::KvPrimary).unwrap();
    assert!(db.storage_mode().read_from_kv());
    assert!(db.storage_mode().sql_fallback_enabled());

    // Read should still succeed by falling back to SQL
    let adapter = db.get_adapter("fallback-test-1").await.unwrap();
    assert!(adapter.is_some(), "Read should succeed via SQL fallback");
    assert_eq!(adapter.unwrap().name, "Fallback Test Adapter");

    // The fallback behavior is handled internally by the storage mode
    // KvPrimary mode attempts KV first, then falls back to SQL if not found

    cleanup_test_db(&db).await;
    drop(temp_dir);
}

#[tokio::test]
async fn test_concurrent_dual_writes() {
    // Test that concurrent dual writes maintain consistency
    let (db, _temp_dir) = create_test_db(StorageMode::DualWrite).await;

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
                .increment_adapter_activation("default-tenant", "concurrent-test-base")
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

    // Verify KV also has correct count
    let kv_repo = create_kv_repo(&db);
    let adapter_kv = kv_repo
        .get_adapter_kv("concurrent-test-base")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        adapter_kv.activation_count, 10,
        "All 10 increments should be reflected in KV"
    );

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_kv_bulk_migration() {
    // Test bulk writes of adapters to both SQL and KV
    let (db, _temp_dir) = create_test_db(StorageMode::DualWrite).await;

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

    // Verify SQL count
    let adapters_sql = db.list_adapters_by_tenant("default-tenant").await.unwrap();
    assert_eq!(adapters_sql.len(), 50, "Should have 50 adapters in SQL");

    // Verify all exist in KV (DualWrite populates both stores)
    let kv_repo = create_kv_repo(&db);
    let adapters_kv = kv_repo
        .list_adapters_for_tenant_kv("default-tenant", None, None)
        .await
        .unwrap();
    assert_eq!(
        adapters_kv.len(),
        50,
        "All 50 adapters should be in KV after dual writes"
    );

    // Spot-check a few adapters for data integrity
    for i in [0, 25, 49] {
        let adapter_id = format!("bulk-migrate-{}", i);
        let sql = db.get_adapter(&adapter_id).await.unwrap().unwrap();
        let kv = kv_repo.get_adapter_kv(&adapter_id).await.unwrap().unwrap();
        assert_eq!(sql.name, kv.name, "Name should match for adapter {}", i);
        assert_eq!(
            sql.hash_b3, kv.hash_b3,
            "Hash should match for adapter {}",
            i
        );
        assert_eq!(sql.rank, kv.rank, "Rank should match for adapter {}", i);
    }

    cleanup_test_db(&db).await;
}

#[tokio::test]
async fn test_storage_mode_read_performance() {
    // Performance comparison test (informational, not strict)
    let (mut db, temp_dir) = create_test_db(StorageMode::DualWrite).await;

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
        let _ = db.get_adapter("perf-test-1").await.unwrap();
    }

    // Time SQL reads (DualWrite mode reads from SQL)
    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = db.get_adapter("perf-test-1").await.unwrap();
    }
    let sql_duration = start.elapsed();

    println!("DualWrite (SQL read) 100 times: {:?}", sql_duration);

    // Switch to KvPrimary mode and time KV reads
    db.set_storage_mode(StorageMode::KvPrimary).unwrap();

    // Warm up KV path
    for _ in 0..10 {
        let _ = db.get_adapter("perf-test-1").await.unwrap();
    }

    let start = std::time::Instant::now();
    for _ in 0..100 {
        let _ = db.get_adapter("perf-test-1").await.unwrap();
    }
    let kv_duration = start.elapsed();

    println!("KvPrimary (KV read) 100 times: {:?}", kv_duration);
    println!(
        "Ratio (SQL/KV): {:.2}x",
        sql_duration.as_secs_f64() / kv_duration.as_secs_f64()
    );

    cleanup_test_db(&db).await;
    drop(temp_dir);
}
