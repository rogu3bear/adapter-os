//! Integration tests for adapter archive and garbage collection operations
//!
//! Tests the archive/GC lifecycle (migration 0138):
//! - Single adapter archival
//! - Bulk tenant adapter archival
//! - Tenant archive cascade
//! - GC candidate selection
//! - Adapter purging
//! - Loadability checks
//! - Unarchive operations
//! - Invariant enforcement (cannot purge non-archived, cannot unarchive purged)

use adapteros_db::Db;
use uuid::Uuid;

/// Helper to create a test adapter with given ID
async fn create_test_adapter(db: &Db, tenant_id: &str, adapter_id: &str) -> String {
    let file_path =
        std::env::temp_dir().join(format!("adapteros-{}-{}.aos", adapter_id, Uuid::new_v4()));
    std::fs::write(&file_path, b"test").expect("Failed to create dummy .aos file");

    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .adapter_id(adapter_id)
        .tenant_id(tenant_id)
        .name(&format!("Test Adapter {}", adapter_id))
        .hash_b3(&format!("hash_{}", adapter_id))
        .rank(8)
        .tier("persistent")
        .aos_file_path(Some(file_path.to_string_lossy().into_owned()))
        .build()
        .unwrap();

    db.register_adapter(params)
        .await
        .expect("Failed to register adapter")
}

#[tokio::test]
async fn test_archive_single_adapter() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    let _adapter_pk = create_test_adapter(&db, &tenant_id, "adapter-001").await;

    // Verify adapter is loadable before archiving
    let loadable = db
        .is_adapter_loadable("adapter-001")
        .await
        .expect("Failed to check loadability");
    assert!(loadable, "Adapter should be loadable before archiving");

    // Archive the adapter
    db.archive_adapter(&tenant_id, "adapter-001", "test-user", "manual retirement")
        .await
        .expect("Failed to archive adapter");

    // Verify adapter is no longer loadable
    let loadable = db
        .is_adapter_loadable("adapter-001")
        .await
        .expect("Failed to check loadability");
    assert!(!loadable, "Adapter should not be loadable after archiving");

    // Verify archived adapter can be found
    let adapter = db
        .get_adapter("adapter-001")
        .await
        .expect("Failed to get adapter")
        .expect("Adapter should exist");
    assert!(adapter.archived_at.is_some(), "archived_at should be set");
    assert_eq!(adapter.archived_by.as_deref(), Some("test-user"));
    assert_eq!(adapter.archive_reason.as_deref(), Some("manual retirement"));
}

#[tokio::test]
async fn test_archive_adapters_for_tenant() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    // Create multiple adapters for the tenant
    create_test_adapter(&db, &tenant_id, "adapter-001").await;
    create_test_adapter(&db, &tenant_id, "adapter-002").await;
    create_test_adapter(&db, &tenant_id, "adapter-003").await;

    // Archive all adapters for the tenant
    let archived_count = db
        .archive_adapters_for_tenant(&tenant_id, "system", "bulk_archive")
        .await
        .expect("Failed to archive adapters for tenant");

    assert_eq!(archived_count, 3, "Should archive 3 adapters");

    // Verify all adapters are archived
    for adapter_id in ["adapter-001", "adapter-002", "adapter-003"] {
        let loadable = db
            .is_adapter_loadable(adapter_id)
            .await
            .expect("Failed to check loadability");
        assert!(!loadable, "Adapter {} should not be loadable", adapter_id);
    }
}

#[tokio::test]
async fn test_tenant_archive_cascades_to_adapters() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    // Create adapters for the tenant
    create_test_adapter(&db, &tenant_id, "cascade-001").await;
    create_test_adapter(&db, &tenant_id, "cascade-002").await;

    // Verify adapters are loadable before tenant archival
    assert!(
        db.is_adapter_loadable("cascade-001").await.unwrap(),
        "Adapter should be loadable before tenant archival"
    );

    // Archive the tenant (should cascade to adapters)
    db.archive_tenant(&tenant_id)
        .await
        .expect("Failed to archive tenant");

    // Verify adapters are now archived
    let adapter1 = db
        .get_adapter("cascade-001")
        .await
        .expect("Failed to get adapter")
        .expect("Adapter should exist");
    assert!(
        adapter1.archived_at.is_some(),
        "Adapter should be archived after tenant archival"
    );
    assert_eq!(
        adapter1.archive_reason.as_deref(),
        Some("tenant_archived"),
        "Archive reason should indicate tenant cascade"
    );

    // Verify both adapters are not loadable
    assert!(!db.is_adapter_loadable("cascade-001").await.unwrap());
    assert!(!db.is_adapter_loadable("cascade-002").await.unwrap());
}

#[tokio::test]
async fn test_find_archived_adapters_for_gc() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    // Create and archive an adapter
    create_test_adapter(&db, &tenant_id, "gc-candidate").await;
    db.archive_adapter(&tenant_id, "gc-candidate", "system", "test")
        .await
        .expect("Failed to archive adapter");

    // GC with 0 days should find the adapter immediately
    let candidates = db
        .find_archived_adapters_for_gc(0, 100)
        .await
        .expect("Failed to find GC candidates");

    assert_eq!(candidates.len(), 1, "Should find 1 GC candidate");
    assert_eq!(candidates[0].adapter_id.as_deref(), Some("gc-candidate"));

    // GC with 30 days should NOT find the adapter (just archived)
    let candidates = db
        .find_archived_adapters_for_gc(30, 100)
        .await
        .expect("Failed to find GC candidates");

    assert!(
        candidates.is_empty(),
        "Should not find recently archived adapter with 30 day threshold"
    );
}

#[tokio::test]
async fn test_mark_adapter_purged() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    create_test_adapter(&db, &tenant_id, "purge-test").await;

    // Archive the adapter first
    db.archive_adapter(&tenant_id, "purge-test", "system", "test")
        .await
        .expect("Failed to archive adapter");

    // Mark as purged
    db.mark_adapter_purged(&tenant_id, "purge-test")
        .await
        .expect("Failed to mark adapter purged");

    // Verify purge state
    let adapter = db
        .get_adapter("purge-test")
        .await
        .expect("Failed to get adapter")
        .expect("Adapter should exist");

    assert!(adapter.purged_at.is_some(), "purged_at should be set");
    assert!(
        adapter.aos_file_path.is_none(),
        "aos_file_path should be cleared"
    );

    // Verify adapter is still not loadable
    assert!(!db.is_adapter_loadable("purge-test").await.unwrap());

    // Verify purged adapter is not in GC candidates
    let candidates = db
        .find_archived_adapters_for_gc(0, 100)
        .await
        .expect("Failed to find GC candidates");
    assert!(
        candidates.is_empty(),
        "Purged adapter should not appear in GC candidates"
    );
}

#[tokio::test]
async fn test_cannot_purge_non_archived_adapter() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    create_test_adapter(&db, &tenant_id, "no-purge").await;

    // Attempt to purge without archiving first
    let result = db.mark_adapter_purged(&tenant_id, "no-purge").await;

    assert!(
        result.is_err(),
        "Should not be able to purge non-archived adapter"
    );
}

#[tokio::test]
async fn test_unarchive_adapter() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    create_test_adapter(&db, &tenant_id, "unarchive-test").await;

    // Archive the adapter
    db.archive_adapter(&tenant_id, "unarchive-test", "system", "test")
        .await
        .expect("Failed to archive adapter");

    assert!(!db.is_adapter_loadable("unarchive-test").await.unwrap());

    // Unarchive the adapter
    db.unarchive_adapter(&tenant_id, "unarchive-test")
        .await
        .expect("Failed to unarchive adapter");

    // Verify adapter is loadable again
    assert!(
        db.is_adapter_loadable("unarchive-test").await.unwrap(),
        "Adapter should be loadable after unarchiving"
    );

    // Verify archive fields are cleared
    let adapter = db
        .get_adapter("unarchive-test")
        .await
        .expect("Failed to get adapter")
        .expect("Adapter should exist");

    assert!(
        adapter.archived_at.is_none(),
        "archived_at should be cleared"
    );
    assert!(
        adapter.archived_by.is_none(),
        "archived_by should be cleared"
    );
    assert!(
        adapter.archive_reason.is_none(),
        "archive_reason should be cleared"
    );
}

#[tokio::test]
async fn test_cannot_unarchive_purged_adapter() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    create_test_adapter(&db, &tenant_id, "purged-no-restore").await;

    // Archive then purge
    db.archive_adapter(&tenant_id, "purged-no-restore", "system", "test")
        .await
        .expect("Failed to archive adapter");
    db.mark_adapter_purged(&tenant_id, "purged-no-restore")
        .await
        .expect("Failed to purge adapter");

    // Attempt to unarchive purged adapter
    let result = db.unarchive_adapter(&tenant_id, "purged-no-restore").await;

    assert!(
        result.is_err(),
        "Should not be able to unarchive purged adapter"
    );
}

#[tokio::test]
async fn test_count_archived_and_purged_adapters() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    // Create 3 adapters
    create_test_adapter(&db, &tenant_id, "count-001").await;
    create_test_adapter(&db, &tenant_id, "count-002").await;
    create_test_adapter(&db, &tenant_id, "count-003").await;

    // Initially no archived or purged
    assert_eq!(db.count_archived_adapters(&tenant_id).await.unwrap(), 0);
    assert_eq!(db.count_purged_adapters(&tenant_id).await.unwrap(), 0);

    // Archive 2 adapters
    db.archive_adapter(&tenant_id, "count-001", "system", "test")
        .await
        .unwrap();
    db.archive_adapter(&tenant_id, "count-002", "system", "test")
        .await
        .unwrap();

    assert_eq!(db.count_archived_adapters(&tenant_id).await.unwrap(), 2);
    assert_eq!(db.count_purged_adapters(&tenant_id).await.unwrap(), 0);

    // Purge 1 of the archived adapters
    db.mark_adapter_purged(&tenant_id, "count-001")
        .await
        .unwrap();

    // count_archived_adapters only counts archived but not purged
    assert_eq!(db.count_archived_adapters(&tenant_id).await.unwrap(), 1);
    assert_eq!(db.count_purged_adapters(&tenant_id).await.unwrap(), 1);
}

#[tokio::test]
async fn test_archive_idempotent() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    create_test_adapter(&db, &tenant_id, "idempotent-test").await;

    // Archive the adapter
    db.archive_adapter(&tenant_id, "idempotent-test", "user1", "first archive")
        .await
        .expect("Failed to archive adapter");

    let adapter = db.get_adapter("idempotent-test").await.unwrap().unwrap();
    let first_archived_at = adapter.archived_at.clone();

    // Attempt to archive again should fail (already archived)
    let result = db
        .archive_adapter(&tenant_id, "idempotent-test", "user2", "second archive")
        .await;

    assert!(
        result.is_err(),
        "Should not be able to archive already archived adapter"
    );

    // Verify original archive data is preserved
    let adapter = db.get_adapter("idempotent-test").await.unwrap().unwrap();
    assert_eq!(adapter.archived_at, first_archived_at);
    assert_eq!(adapter.archived_by.as_deref(), Some("user1"));
    assert_eq!(adapter.archive_reason.as_deref(), Some("first archive"));
}
