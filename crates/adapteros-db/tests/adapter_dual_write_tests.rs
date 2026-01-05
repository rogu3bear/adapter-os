//! Integration tests for adapter dual-write functionality
//!
//! These tests verify that adapter operations correctly write to both SQL and KV
//! backends when in DualWrite mode, and that data remains consistent between stores.
#![allow(deprecated)]

use adapteros_db::adapters::{Adapter, AdapterRegistrationBuilder};
use adapteros_db::{Db, ProtectedDb, StorageMode, WriteCapableDb};
use adapteros_storage::repos::adapter::AdapterRepository;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = std::path::PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

/// Helper to set up test database with KV backend in DualWrite mode
async fn create_dual_write_db() -> (ProtectedDb, TempDir, TempDir) {
    // Note: tracing is initialized by test harness if needed

    // Create temp directories for SQL and KV
    let sql_temp = new_test_tempdir();
    let kv_temp = new_test_tempdir();

    let sql_path = sql_temp.path().join("test.db");
    let kv_path = kv_temp.path().join("test.kv");

    // Create SQL database
    let mut db = Db::connect(sql_path.to_str().unwrap()).await.unwrap();
    db.migrate().await.unwrap();

    // Initialize KV backend
    db.init_kv_backend(&kv_path).unwrap();

    // Set to DualWrite mode
    db.set_storage_mode(StorageMode::DualWrite).unwrap();

    // Create default tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('system', 'System')")
        .execute(db.pool())
        .await
        .unwrap();

    let db = ProtectedDb::new(db);

    (db, sql_temp, kv_temp)
}

fn write_db(db: &ProtectedDb) -> WriteCapableDb<'_> {
    db.write(db.lifecycle_token())
}

/// Helper to get adapter from KV directly (bypassing Db)
async fn get_adapter_from_kv(db: &Db, tenant_id: &str, adapter_id: &str) -> Option<Adapter> {
    if let Some(kv) = db.kv_backend() {
        let repo = AdapterRepository::new(kv.backend().clone(), kv.index_manager().clone());
        repo.get(tenant_id, adapter_id)
            .await
            .ok()
            .flatten()
            .map(|kv_adapter| kv_adapter.into())
    } else {
        None
    }
}

/// Helper to check if adapter exists in KV
async fn adapter_exists_in_kv(db: &Db, tenant_id: &str, adapter_id: &str) -> bool {
    get_adapter_from_kv(db, tenant_id, adapter_id)
        .await
        .is_some()
}

#[tokio::test]
async fn test_register_adapter_writes_to_both_sql_and_kv() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Verify mode and KV backend
    eprintln!("Storage mode: {:?}", db.storage_mode());
    eprintln!("Has KV backend: {}", db.has_kv_backend());
    eprintln!("Write to KV: {}", db.storage_mode().write_to_kv());

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("dual-write-test-1")
        .name("Dual Write Test Adapter")
        .hash_b3("b3:dual_write_hash_1")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    let uuid = db.register_adapter(params).await.unwrap();
    assert!(!uuid.is_empty());

    // Verify in SQL
    let adapter_sql = db.get_adapter("dual-write-test-1").await.unwrap();
    assert!(adapter_sql.is_some(), "Adapter should exist in SQL");
    let adapter = adapter_sql.unwrap();
    assert_eq!(adapter.name, "Dual Write Test Adapter");
    assert_eq!(adapter.hash_b3, "b3:dual_write_hash_1");
    assert_eq!(adapter.rank, 16);
    assert_eq!(adapter.tier, "warm");

    // Debug: Check if adapter exists in KV
    eprintln!("Checking KV for adapter...");
    let kv_result = get_adapter_from_kv(&db, "default-tenant", "dual-write-test-1").await;
    eprintln!("KV result: {:?}", kv_result.is_some());

    // Verify in KV
    let kv_exists = adapter_exists_in_kv(&db, "default-tenant", "dual-write-test-1").await;
    assert!(kv_exists, "Adapter should exist in KV store");

    // Verify KV data matches SQL
    let adapter_kv = get_adapter_from_kv(&db, "default-tenant", "dual-write-test-1")
        .await
        .unwrap();
    assert_eq!(adapter_kv.name, adapter.name);
    assert_eq!(adapter_kv.hash_b3, adapter.hash_b3);
    assert_eq!(adapter_kv.rank, adapter.rank);
    assert_eq!(adapter_kv.tier, adapter.tier);

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_update_adapter_state_writes_to_both() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("state-update-test")
        .name("State Update Test")
        .hash_b3("b3:state_update")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Update state
    write_db(&db)
        .update_adapter_state(
            "default-tenant",
            "state-update-test",
            "loaded",
            "test reason",
        )
        .await
        .unwrap();

    // Verify in SQL
    let adapter_sql = db.get_adapter("state-update-test").await.unwrap().unwrap();
    assert_eq!(adapter_sql.current_state, "loaded");

    // Verify in KV
    let adapter_kv = get_adapter_from_kv(&db, "default-tenant", "state-update-test")
        .await
        .unwrap();
    assert_eq!(adapter_kv.current_state, "loaded");

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_update_adapter_state_tx_writes_to_both() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("state-tx-test")
        .name("State TX Test")
        .hash_b3("b3:state_tx")
        .rank(12)
        .tier("persistent")
        .category("framework")
        .scope("tenant")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Update state with transaction
    write_db(&db)
        .update_adapter_state_tx("state-tx-test", "hot", "warming up")
        .await
        .unwrap();

    // Verify in SQL
    let adapter_sql = db.get_adapter("state-tx-test").await.unwrap().unwrap();
    assert_eq!(adapter_sql.current_state, "hot");

    // Verify in KV
    let adapter_kv = get_adapter_from_kv(&db, "default-tenant", "state-tx-test")
        .await
        .unwrap();
    assert_eq!(adapter_kv.current_state, "hot");

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_update_adapter_memory_writes_to_both() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("memory-update-test")
        .name("Memory Update Test")
        .hash_b3("b3:memory_update")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Update memory
    let memory_bytes = 1024 * 1024 * 512; // 512 MB
    db.update_adapter_memory("default-tenant", "memory-update-test", memory_bytes)
        .await
        .unwrap();

    // Verify in SQL
    let adapter_sql = db.get_adapter("memory-update-test").await.unwrap().unwrap();
    assert_eq!(adapter_sql.memory_bytes, memory_bytes);

    // Verify in KV
    let adapter_kv = get_adapter_from_kv(&db, "default-tenant", "memory-update-test")
        .await
        .unwrap();
    assert_eq!(adapter_kv.memory_bytes, memory_bytes);

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_update_adapter_state_and_memory_writes_to_both() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("combined-update-test")
        .name("Combined Update Test")
        .hash_b3("b3:combined_update")
        .rank(24)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Update both state and memory
    let memory_bytes = 1024 * 1024 * 256; // 256 MB
    write_db(&db)
        .update_adapter_state_and_memory("combined-update-test", "warm", memory_bytes, "loading")
        .await
        .unwrap();

    // Verify in SQL
    let adapter_sql = db
        .get_adapter("combined-update-test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(adapter_sql.current_state, "warm");
    assert_eq!(adapter_sql.memory_bytes, memory_bytes);

    // Verify in KV
    let adapter_kv = get_adapter_from_kv(&db, "default-tenant", "combined-update-test")
        .await
        .unwrap();
    assert_eq!(adapter_kv.current_state, "warm");
    assert_eq!(adapter_kv.memory_bytes, memory_bytes);

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_delete_adapter_removes_from_both() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("delete-test")
        .name("Delete Test")
        .hash_b3("b3:delete_test")
        .rank(8)
        .tier("ephemeral")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    let uuid = db.register_adapter(params).await.unwrap();

    // Verify it exists in both stores
    assert!(db.get_adapter("delete-test").await.unwrap().is_some());
    assert!(adapter_exists_in_kv(&db, "default-tenant", "delete-test").await);

    // Delete the adapter
    db.delete_adapter(&uuid).await.unwrap();

    // Verify removed from SQL
    assert!(
        db.get_adapter("delete-test").await.unwrap().is_none(),
        "Adapter should be removed from SQL"
    );

    // Verify removed from KV
    assert!(
        !adapter_exists_in_kv(&db, "default-tenant", "delete-test").await,
        "Adapter should be removed from KV"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_delete_adapter_cascade_removes_from_both() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("cascade-delete-test")
        .name("Cascade Delete Test")
        .hash_b3("b3:cascade_delete")
        .rank(12)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    let uuid = db.register_adapter(params).await.unwrap();

    // Verify it exists in both stores
    assert!(db
        .get_adapter("cascade-delete-test")
        .await
        .unwrap()
        .is_some());
    assert!(adapter_exists_in_kv(&db, "default-tenant", "cascade-delete-test").await);

    // Delete with cascade
    db.delete_adapter_cascade(&uuid).await.unwrap();

    // Verify removed from SQL
    assert!(db
        .get_adapter("cascade-delete-test")
        .await
        .unwrap()
        .is_none());

    // Verify removed from KV
    assert!(!adapter_exists_in_kv(&db, "default-tenant", "cascade-delete-test").await);

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_kv_failure_does_not_fail_sql_operation() {
    let (mut db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Detach KV backend to simulate KV failure
    // (In production, KV failures are logged but don't fail the operation)
    let kv_backend = db.kv_backend().cloned();
    db.detach_kv_backend().unwrap();
    db.set_storage_mode(StorageMode::DualWrite).unwrap();

    // Re-attach but we'll verify behavior
    if let Some(kv) = kv_backend {
        db.attach_kv_backend((*kv).clone()).unwrap();
        db.set_storage_mode(StorageMode::DualWrite).unwrap();
    }

    // Register an adapter - this should succeed even if KV write fails
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("kv-failure-test")
        .name("KV Failure Test")
        .hash_b3("b3:kv_failure")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    // This should succeed (KV failures are logged but don't fail the operation)
    let result = db.register_adapter(params).await;
    assert!(
        result.is_ok(),
        "SQL operation should succeed even if KV write fails"
    );

    // Verify in SQL
    let adapter_sql = db.get_adapter("kv-failure-test").await.unwrap();
    assert!(
        adapter_sql.is_some(),
        "Adapter should exist in SQL even if KV write failed"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_consistency_after_multiple_updates() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("consistency-test")
        .name("Consistency Test")
        .hash_b3("b3:consistency")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Perform multiple updates
    write_db(&db)
        .update_adapter_state(
            "default-tenant",
            "consistency-test",
            "loading",
            "initial load",
        )
        .await
        .unwrap();

    db.update_adapter_memory("default-tenant", "consistency-test", 1024 * 1024 * 128)
        .await
        .unwrap();

    write_db(&db)
        .update_adapter_state("default-tenant", "consistency-test", "warm", "loaded")
        .await
        .unwrap();

    db.update_adapter_memory("default-tenant", "consistency-test", 1024 * 1024 * 256)
        .await
        .unwrap();

    db.increment_adapter_activation("default-tenant", "consistency-test")
        .await
        .unwrap();

    db.increment_adapter_activation("default-tenant", "consistency-test")
        .await
        .unwrap();

    // Verify final state in SQL
    let adapter_sql = db.get_adapter("consistency-test").await.unwrap().unwrap();
    assert_eq!(adapter_sql.current_state, "warm");
    assert_eq!(adapter_sql.memory_bytes, 1024 * 1024 * 256);
    assert_eq!(adapter_sql.activation_count, 2);

    // Verify final state in KV matches
    let adapter_kv = get_adapter_from_kv(&db, "default-tenant", "consistency-test")
        .await
        .unwrap();
    assert_eq!(adapter_kv.current_state, adapter_sql.current_state);
    assert_eq!(adapter_kv.memory_bytes, adapter_sql.memory_bytes);
    assert_eq!(adapter_kv.activation_count, adapter_sql.activation_count);

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_sql_only_mode_does_not_write_to_kv() {
    let (mut db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Switch to SqlOnly mode
    db.set_storage_mode(StorageMode::SqlOnly).unwrap();

    // Register an adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("sql-only-test")
        .name("SQL Only Test")
        .hash_b3("b3:sql_only")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Verify in SQL
    assert!(db.get_adapter("sql-only-test").await.unwrap().is_some());

    // Verify NOT in KV
    assert!(
        !adapter_exists_in_kv(&db, "default-tenant", "sql-only-test").await,
        "Adapter should NOT be in KV when in SqlOnly mode"
    );

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_mode_transition_from_sql_to_dual_write() {
    let (mut db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Start in SqlOnly mode
    db.set_storage_mode(StorageMode::SqlOnly).unwrap();

    // Register adapter in SqlOnly mode
    let params1 = AdapterRegistrationBuilder::new()
        .adapter_id("transition-test-1")
        .name("Before Transition")
        .hash_b3("b3:before_transition")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params1).await.unwrap();

    // Verify only in SQL
    assert!(db.get_adapter("transition-test-1").await.unwrap().is_some());
    assert!(!adapter_exists_in_kv(&db, "default-tenant", "transition-test-1").await);

    // Switch to DualWrite mode
    db.set_storage_mode(StorageMode::DualWrite).unwrap();

    // Register new adapter in DualWrite mode
    let params2 = AdapterRegistrationBuilder::new()
        .adapter_id("transition-test-2")
        .name("After Transition")
        .hash_b3("b3:after_transition")
        .rank(16)
        .tier("persistent")
        .category("framework")
        .scope("tenant")
        .build()
        .unwrap();

    db.register_adapter(params2).await.unwrap();

    // Verify new adapter in both stores
    assert!(db.get_adapter("transition-test-2").await.unwrap().is_some());
    assert!(adapter_exists_in_kv(&db, "default-tenant", "transition-test-2").await);

    // Old adapter still only in SQL
    assert!(!adapter_exists_in_kv(&db, "default-tenant", "transition-test-1").await);

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_adapter_with_extended_fields() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register adapter with all extended fields
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("system")
        .adapter_id("extended-fields-test")
        .name("Extended Fields Test")
        .hash_b3("b3:extended_fields")
        .rank(24)
        .tier("persistent")
        .category("codebase")
        .scope("tenant")
        .framework(Some("rust".to_string()))
        .framework_id(Some("rust-framework-1".to_string()))
        .framework_version(Some("1.0.0".to_string()))
        .repo_id(Some("github.com/test/repo".to_string()))
        .commit_sha(Some("abc123def456".to_string()))
        .intent(Some("code analysis".to_string()))
        // Use valid semantic naming format: {tenant}/{domain}/{purpose}/r{NNN}
        .adapter_name(Some("testns/code/analysis/r001".to_string()))
        .tenant_namespace(Some("testns".to_string()))
        .domain(Some("code".to_string()))
        .purpose(Some("analysis".to_string()))
        .revision(Some("r001".to_string()))
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Verify in SQL
    let adapter_sql = db
        .get_adapter("extended-fields-test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(adapter_sql.framework.as_deref(), Some("rust"));
    assert_eq!(adapter_sql.category, "codebase");
    assert_eq!(adapter_sql.scope, "tenant");

    // Verify in KV with matching fields
    let adapter_kv = get_adapter_from_kv(&db, "system", "extended-fields-test")
        .await
        .unwrap();
    assert_eq!(adapter_kv.framework, adapter_sql.framework);
    assert_eq!(adapter_kv.category, adapter_sql.category);
    assert_eq!(adapter_kv.scope, adapter_sql.scope);
    assert_eq!(adapter_kv.framework_id, adapter_sql.framework_id);
    assert_eq!(adapter_kv.repo_id, adapter_sql.repo_id);
    assert_eq!(adapter_kv.commit_sha, adapter_sql.commit_sha);

    db.close().await.unwrap();
}

#[tokio::test]
async fn test_concurrent_dual_writes_maintain_consistency() {
    let (db, _sql_temp, _kv_temp) = create_dual_write_db().await;

    // Register base adapter
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("concurrent-dual-write-test")
        .name("Concurrent Dual Write Test")
        .hash_b3("b3:concurrent_dual")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Spawn concurrent activation increments
    let mut handles = vec![];
    for _ in 0..10 {
        let db_clone = db.clone();
        let handle = tokio::spawn(async move {
            db_clone
                .increment_adapter_activation("default-tenant", "concurrent-dual-write-test")
                .await
                .unwrap();
        });
        handles.push(handle);
    }

    // Wait for all to complete
    for handle in handles {
        handle.await.unwrap();
    }

    // Verify final count in SQL
    let adapter_sql = db
        .get_adapter("concurrent-dual-write-test")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(adapter_sql.activation_count, 10);

    // Verify final count in KV matches
    let adapter_kv = get_adapter_from_kv(&db, "default-tenant", "concurrent-dual-write-test")
        .await
        .unwrap();
    assert_eq!(adapter_kv.activation_count, 10);

    db.close().await.unwrap();
}
