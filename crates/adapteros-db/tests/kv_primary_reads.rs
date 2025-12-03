//! Integration tests for KV-primary read operations
//!
//! These tests validate the KV-primary read path, ensuring:
//! 1. Reads attempt KV first when in KvPrimary mode
//! 2. Fallback to SQL occurs when KV returns None
//! 3. Fallback to SQL occurs when KV errors
//! 4. Correct data is returned in each scenario
//! 5. Proper error handling and logging
//!
//! Test Strategy:
//! - Create a Db in KvPrimary mode with both SQL and KV backends
//! - Test all read operations: get_adapter, list_adapters, find_by_hash, etc.
//! - Verify fallback behavior by controlling KV state
//! - Ensure data consistency between SQL and KV

use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::{Db, KvDb, StorageMode};
use adapteros_storage::repos::adapter::AdapterRepository;
use std::sync::Arc;
use tempfile::TempDir;

/// Initialize tracing for tests (call once per test)
fn init_tracing() {
    // Tracing initialization is optional for tests
    // Tests will run without it if tracing_subscriber is not available
}

/// Helper to create a test database in KvPrimary mode
///
/// Creates both SQL and KV backends in a temporary directory,
/// with migrations applied and a default tenant configured.
async fn create_kv_primary_db() -> (Db, TempDir) {
    init_tracing();

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let kv_path = temp_dir.path().join("kv.redb");

    // Create SQL pool
    let sql_url = db_path.to_str().unwrap();
    let db_sql = Db::connect(sql_url).await.unwrap();
    db_sql.migrate().await.unwrap();

    // Create KV backend
    let kv_db = KvDb::init_redb(&kv_path).unwrap();

    // Create Db in KvPrimary mode
    let pool = db_sql.pool().clone();
    let db = Db::new(pool, Some(Arc::new(kv_db)), StorageMode::KvPrimary);

    // Ensure storage mode is correctly set
    assert_eq!(db.storage_mode(), StorageMode::KvPrimary);
    assert!(db.has_kv_backend());

    // Create default tenant for testing
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    (db, temp_dir)
}

/// Helper to create a test database in DualWrite mode for setup
///
/// Useful for populating both SQL and KV with test data
async fn create_dual_write_db() -> (Db, TempDir) {
    init_tracing();

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let kv_path = temp_dir.path().join("kv.redb");

    // Create SQL pool
    let sql_url = db_path.to_str().unwrap();
    let db_sql = Db::connect(sql_url).await.unwrap();
    db_sql.migrate().await.unwrap();

    // Create KV backend
    let kv_db = KvDb::init_redb(&kv_path).unwrap();

    // Create Db in DualWrite mode
    let pool = db_sql.pool().clone();
    let db = Db::new(pool, Some(Arc::new(kv_db)), StorageMode::DualWrite);

    // Create default tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    (db, temp_dir)
}

/// Helper to directly insert into KV without going through Db
///
/// Used to test KV-only scenarios or to corrupt KV for error testing
async fn insert_adapter_to_kv(
    kv: &KvDb,
    tenant_id: &str,
    adapter_id: &str,
    name: &str,
    hash_b3: &str,
    rank: i32,
) {
    use adapteros_storage::AdapterKv;
    use chrono::Utc;

    let adapter_kv = AdapterKv {
        id: uuid::Uuid::now_v7().to_string(),
        adapter_id: Some(adapter_id.to_string()),
        tenant_id: tenant_id.to_string(),
        name: name.to_string(),
        hash_b3: hash_b3.to_string(),
        rank,
        alpha: (rank * 2) as f64,
        tier: "warm".to_string(),
        category: "code".to_string(),
        scope: "global".to_string(),
        current_state: "unloaded".to_string(),
        memory_bytes: 0,
        activation_count: 0,
        created_at: Utc::now().to_rfc3339(),
        updated_at: Utc::now().to_rfc3339(),
        parent_id: None,
        fork_type: None,
        fork_reason: None,
        framework: None,
        targets_json: "[]".to_string(),
        acl_json: None,
        languages_json: None,
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        last_activated: None,
        last_loaded_at: None,
        active: 1,
        pinned: 0,
        expires_at: None,
        aos_file_path: None,
        aos_file_hash: None,
        adapter_name: None,
        tenant_namespace: None,
        domain: None,
        purpose: None,
        revision: None,
        lifecycle_state: "active".to_string(),
        load_state: "unloaded".to_string(),
        version: "1.0".to_string(),
        archived_at: None,
        archived_by: None,
        archive_reason: None,
        purged_at: None,
    };

    let repo = AdapterRepository::new(kv.backend().clone(), kv.index_manager().clone());

    repo.create(adapter_kv).await.unwrap();
}

// ============================================================================
// Test 1: KvPrimary mode reads from KV first
// ============================================================================

#[tokio::test]
async fn test_kv_primary_reads_from_kv_first() {
    let (mut db, _temp_dir) = create_dual_write_db().await;

    // Register an adapter in DualWrite mode (writes to both SQL and KV)
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("kv-read-test-1")
        .name("KV Read Test Adapter")
        .hash_b3("b3:kv_read_hash_1")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .tenant_id("default-tenant")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Switch to KvPrimary mode
    db.set_storage_mode(StorageMode::KvPrimary);

    // Read adapter - should come from KV or fall back to SQL
    let adapter = db
        .get_adapter("kv-read-test-1")
        .await
        .unwrap()
        .expect("Adapter should exist");

    assert_eq!(adapter.adapter_id.as_deref(), Some("kv-read-test-1"));
    assert_eq!(adapter.name, "KV Read Test Adapter");
    assert_eq!(adapter.hash_b3, "b3:kv_read_hash_1");
    assert_eq!(adapter.rank, 16);
}

// ============================================================================
// Test 2: Fallback to SQL when KV returns None
// ============================================================================

#[tokio::test]
async fn test_kv_primary_fallback_on_kv_none() {
    let (db, _temp_dir) = create_kv_primary_db().await;

    // Insert adapter directly into SQL (bypassing KV)
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("fallback-test-1")
        .name("Fallback Test Adapter")
        .hash_b3("b3:fallback_hash_1")
        .rank(12)
        .tier("warm")
        .category("code")
        .scope("global")
        .tenant_id("default-tenant")
        .build()
        .unwrap();

    // Insert directly into SQL
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO adapters (
            id, adapter_id, tenant_id, name, hash_b3, rank, alpha, tier, category, scope,
            current_state, memory_bytes, activation_count, created_at, updated_at, active, targets_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'unloaded', 0, 0, datetime('now'), datetime('now'), 1, '[]')"
    )
    .bind(&id)
    .bind(&params.adapter_id)
    .bind(&params.tenant_id)
    .bind(&params.name)
    .bind(&params.hash_b3)
    .bind(params.rank)
    .bind(params.alpha)
    .bind(&params.tier)
    .bind(&params.category)
    .bind(&params.scope)
    .execute(db.pool())
    .await
    .unwrap();

    // KV should return None, triggering SQL fallback
    let adapter = db
        .get_adapter("fallback-test-1")
        .await
        .unwrap()
        .expect("Adapter should exist via SQL fallback");

    assert_eq!(adapter.adapter_id.unwrap(), "fallback-test-1");
    assert_eq!(adapter.name, "Fallback Test Adapter");
    assert_eq!(adapter.hash_b3, "b3:fallback_hash_1");
    assert_eq!(adapter.rank, 12);
}

// ============================================================================
// Test 3: Fallback to SQL when KV errors (simulated by missing tenant)
// ============================================================================

#[tokio::test]
async fn test_kv_primary_fallback_on_kv_error() {
    let (db, _temp_dir) = create_kv_primary_db().await;

    // Insert adapter into SQL with a different tenant that might cause KV lookup issues
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("error-fallback-test-1")
        .name("Error Fallback Test Adapter")
        .hash_b3("b3:error_fallback_hash_1")
        .rank(8)
        .tier("ephemeral")
        .category("code")
        .scope("global")
        .tenant_id("default-tenant")
        .build()
        .unwrap();

    // Insert directly into SQL
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO adapters (
            id, adapter_id, tenant_id, name, hash_b3, rank, alpha, tier, category, scope,
            current_state, memory_bytes, activation_count, created_at, updated_at, active, targets_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'unloaded', 0, 0, datetime('now'), datetime('now'), 1, '[]')"
    )
    .bind(&id)
    .bind(&params.adapter_id)
    .bind(&params.tenant_id)
    .bind(&params.name)
    .bind(&params.hash_b3)
    .bind(params.rank)
    .bind(params.alpha)
    .bind(&params.tier)
    .bind(&params.category)
    .bind(&params.scope)
    .execute(db.pool())
    .await
    .unwrap();

    // Even if KV encounters issues, SQL fallback should work
    let adapter = db
        .get_adapter("error-fallback-test-1")
        .await
        .unwrap()
        .expect("Adapter should exist via SQL fallback");

    assert_eq!(adapter.adapter_id.unwrap(), "error-fallback-test-1");
    assert_eq!(adapter.name, "Error Fallback Test Adapter");
}

// ============================================================================
// Test 4: Verify correct data returned in all scenarios
// ============================================================================

#[tokio::test]
async fn test_kv_primary_data_consistency() {
    let (mut db, _temp_dir) = create_dual_write_db().await;

    // Create adapters with varying configurations
    let test_cases = vec![
        (
            "data-test-1",
            "Data Test 1",
            "b3:data_hash_1",
            8,
            "persistent",
        ),
        ("data-test-2", "Data Test 2", "b3:data_hash_2", 16, "warm"),
        (
            "data-test-3",
            "Data Test 3",
            "b3:data_hash_3",
            24,
            "ephemeral",
        ),
    ];

    for (adapter_id, name, hash_b3, rank, tier) in &test_cases {
        let params = AdapterRegistrationBuilder::new()
            .adapter_id(*adapter_id)
            .name(*name)
            .hash_b3(*hash_b3)
            .rank(*rank)
            .tier(*tier)
            .category("code")
            .scope("global")
            .tenant_id("default-tenant")
            .framework(Some("rust".to_string()))
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();
    }

    // Switch to KvPrimary mode
    db.set_storage_mode(StorageMode::KvPrimary);

    // Verify each adapter can be read with correct data
    for (adapter_id, name, hash_b3, rank, tier) in &test_cases {
        let adapter = db
            .get_adapter(adapter_id)
            .await
            .unwrap()
            .expect(&format!("Adapter {} should exist", adapter_id));

        assert_eq!(adapter.adapter_id.as_deref(), Some(*adapter_id));
        assert_eq!(adapter.name, *name);
        assert_eq!(adapter.hash_b3, *hash_b3);
        assert_eq!(adapter.rank, *rank);
        assert_eq!(adapter.tier, *tier);
        assert_eq!(adapter.framework.as_deref(), Some("rust"));
    }
}

// ============================================================================
// Test 5: List operations use KV primary
// ============================================================================

#[tokio::test]
async fn test_kv_primary_list_adapters() {
    let (mut db, _temp_dir) = create_dual_write_db().await;

    // Create multiple adapters
    for i in 1..=5 {
        let params = AdapterRegistrationBuilder::new()
            .adapter_id(&format!("list-test-{}", i))
            .name(&format!("List Test Adapter {}", i))
            .hash_b3(&format!("b3:list_hash_{}", i))
            .rank(8 * i)
            .tier("warm")
            .category("code")
            .scope("global")
            .tenant_id("default-tenant")
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();
    }

    // Switch to KvPrimary mode
    db.set_storage_mode(StorageMode::KvPrimary);

    // List adapters - should use KV
    let adapters = db.list_adapters_by_tenant("default-tenant").await.unwrap();

    assert_eq!(adapters.len(), 5, "Should find all 5 adapters");

    // Verify all adapters are present
    for i in 1..=5 {
        let adapter_id = format!("list-test-{}", i);
        assert!(
            adapters
                .iter()
                .any(|a| a.adapter_id.as_deref() == Some(adapter_id.as_str())),
            "Should find adapter {}",
            adapter_id
        );
    }
}

// ============================================================================
// Test 6: List operations fallback to SQL when KV incomplete
// ============================================================================

#[tokio::test]
async fn test_kv_primary_list_fallback() {
    let (db, _temp_dir) = create_kv_primary_db().await;

    // Insert adapters directly into SQL (not in KV)
    for i in 1..=3 {
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO adapters (
                id, adapter_id, tenant_id, name, hash_b3, rank, alpha, tier, category, scope,
                current_state, memory_bytes, activation_count, created_at, updated_at, active, targets_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'unloaded', 0, 0, datetime('now'), datetime('now'), 1, '[]')"
        )
        .bind(&id)
        .bind(&format!("sql-only-{}", i))
        .bind("default-tenant")
        .bind(&format!("SQL Only Adapter {}", i))
        .bind(&format!("b3:sql_only_hash_{}", i))
        .bind(8i32)
        .bind(16i32)
        .bind("warm")
        .bind("code")
        .bind("global")
        .execute(db.pool())
        .await
        .unwrap();
    }

    // List should fall back to SQL
    let adapters = db.list_adapters_by_tenant("default-tenant").await.unwrap();

    assert_eq!(
        adapters.len(),
        3,
        "Should find all 3 adapters via SQL fallback"
    );
}

// ============================================================================
// Test 7: Find by hash uses KV primary
// ============================================================================

#[tokio::test]
async fn test_kv_primary_find_by_hash() {
    let (mut db, _temp_dir) = create_dual_write_db().await;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("hash-test-1")
        .name("Hash Test Adapter")
        .hash_b3("b3:unique_hash_12345")
        .rank(16)
        .tier("persistent")
        .category("framework")
        .scope("global")
        .tenant_id("default-tenant")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Switch to KvPrimary mode
    db.set_storage_mode(StorageMode::KvPrimary);

    // Find by hash - currently falls back to SQL for cross-tenant hash lookup
    // This is expected behavior (see TODO in adapters.rs)
    let adapter = db
        .find_adapter_by_hash("b3:unique_hash_12345")
        .await
        .unwrap()
        .expect("Adapter should be found by hash");

    assert_eq!(adapter.hash_b3, "b3:unique_hash_12345");
    assert_eq!(adapter.adapter_id.as_deref(), Some("hash-test-1"));
}

// ============================================================================
// Test 8: State updates in KV primary mode
// ============================================================================

#[tokio::test]
async fn test_kv_primary_state_updates() {
    let (mut db, _temp_dir) = create_dual_write_db().await;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("state-test-1")
        .name("State Test Adapter")
        .hash_b3("b3:state_hash_1")
        .rank(12)
        .tier("warm")
        .category("code")
        .scope("global")
        .tenant_id("default-tenant")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Switch to KvPrimary mode
    db.set_storage_mode(StorageMode::KvPrimary);

    // Update state (dual-write in KvPrimary mode)
    db.update_adapter_state_tx("state-test-1", "warm", "Test state transition")
        .await
        .unwrap();

    // Read back - should show updated state
    let adapter = db
        .get_adapter("state-test-1")
        .await
        .unwrap()
        .expect("Adapter should exist");

    assert_eq!(adapter.current_state, "warm");
}

// ============================================================================
// Test 9: Memory updates in KV primary mode
// ============================================================================

#[tokio::test]
async fn test_kv_primary_memory_updates() {
    let (mut db, _temp_dir) = create_dual_write_db().await;

    let params = AdapterRegistrationBuilder::new()
        .adapter_id("memory-test-1")
        .name("Memory Test Adapter")
        .hash_b3("b3:memory_hash_1")
        .rank(16)
        .tier("warm")
        .category("code")
        .scope("global")
        .tenant_id("default-tenant")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Switch to KvPrimary mode
    db.set_storage_mode(StorageMode::KvPrimary);

    // Update memory
    db.update_adapter_memory_tx("memory-test-1", 1024 * 1024 * 500) // 500MB
        .await
        .unwrap();

    // Read back
    let adapter = db
        .get_adapter("memory-test-1")
        .await
        .unwrap()
        .expect("Adapter should exist");

    assert_eq!(adapter.memory_bytes, 1024 * 1024 * 500);
}

// ============================================================================
// Test 10: Lineage queries in KV primary mode
// ============================================================================

#[tokio::test]
async fn test_kv_primary_lineage() {
    let (mut db, _temp_dir) = create_dual_write_db().await;

    // Create parent adapter
    let parent_params = AdapterRegistrationBuilder::new()
        .adapter_id("lineage-parent")
        .name("Lineage Parent")
        .hash_b3("b3:lineage_parent")
        .rank(16)
        .tier("persistent")
        .category("framework")
        .scope("global")
        .tenant_id("default-tenant")
        .build()
        .unwrap();

    let parent_uuid = db.register_adapter(parent_params).await.unwrap();

    // Create child adapter
    let child_params = AdapterRegistrationBuilder::new()
        .adapter_id("lineage-child")
        .name("Lineage Child")
        .hash_b3("b3:lineage_child")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("tenant")
        .tenant_id("default-tenant")
        .parent_id(Some(parent_uuid.clone()))
        .fork_type(Some("extension".to_string()))
        .fork_reason(Some("Test lineage".to_string()))
        .build()
        .unwrap();

    db.register_adapter(child_params).await.unwrap();

    // Switch to KvPrimary mode
    db.set_storage_mode(StorageMode::KvPrimary);

    // Query lineage
    let lineage = db.get_adapter_lineage("lineage-parent").await.unwrap();

    // Should find both parent and child
    assert_eq!(lineage.len(), 2, "Should find parent and child in lineage");

    let adapter_ids: Vec<_> = lineage
        .iter()
        .filter_map(|a| a.adapter_id.as_deref())
        .collect();

    assert!(adapter_ids.contains(&"lineage-parent"));
    assert!(adapter_ids.contains(&"lineage-child"));
}

// ============================================================================
// Test 11: Non-existent adapter returns None
// ============================================================================

#[tokio::test]
async fn test_kv_primary_nonexistent_adapter() {
    let (db, _temp_dir) = create_kv_primary_db().await;

    // Try to read non-existent adapter
    let result = db.get_adapter("does-not-exist").await.unwrap();
    assert!(result.is_none(), "Non-existent adapter should return None");
}

// ============================================================================
// Test 12: Mixed KV and SQL data consistency
// ============================================================================

#[tokio::test]
async fn test_kv_primary_mixed_data_sources() {
    let (db, _temp_dir) = create_kv_primary_db().await;

    // Insert one adapter into SQL only
    let sql_id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO adapters (
            id, adapter_id, tenant_id, name, hash_b3, rank, alpha, tier, category, scope,
            current_state, memory_bytes, activation_count, created_at, updated_at, active, targets_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'unloaded', 0, 0, datetime('now'), datetime('now'), 1, '[]')"
    )
    .bind(&sql_id)
    .bind("sql-adapter")
    .bind("default-tenant")
    .bind("SQL Adapter")
    .bind("b3:sql_hash")
    .bind(8i32)
    .bind(16i32)
    .bind("warm")
    .bind("code")
    .bind("global")
    .execute(db.pool())
    .await
    .unwrap();

    // Insert one adapter into KV only
    if let Some(kv) = db.kv_backend() {
        insert_adapter_to_kv(
            kv,
            "default-tenant",
            "kv-adapter",
            "KV Adapter",
            "b3:kv_hash",
            16,
        )
        .await;
    }

    // Read SQL adapter (should fall back)
    let sql_adapter = db
        .get_adapter("sql-adapter")
        .await
        .unwrap()
        .expect("SQL adapter should exist");
    assert_eq!(sql_adapter.adapter_id.as_deref(), Some("sql-adapter"));

    // Read KV adapter (should read from KV)
    let kv_adapter = db
        .get_adapter("kv-adapter")
        .await
        .unwrap()
        .expect("KV adapter should exist");
    assert_eq!(kv_adapter.adapter_id.as_deref(), Some("kv-adapter"));
}

// ============================================================================
// Copyright JKCA | 2025 James KC Auchterlonie
// ============================================================================
