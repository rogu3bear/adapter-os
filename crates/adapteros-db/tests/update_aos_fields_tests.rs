// Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//
// UPDATE Query Tests for .aos File Fields (Agent 13 - PRD-2 Corner Fix)
//
// Purpose: Verify that UPDATE operations on adapters table preserve aos_file_path and aos_file_hash
// Rationale: Partial UPDATE queries intentionally exclude aos_file fields to prevent overwriting them.
//            This test suite confirms that this behavior is correct and consistent.

use adapteros_db::adapters::AdapterRegistrationParams;

/// Helper to initialize test database with schema
async fn init_test_db() -> anyhow::Result<adapteros_db::Db> {
    let db = adapteros_db::Db::connect("sqlite::memory:").await?;
    db.migrate().await?;

    // Create a default tenant for tests
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')")
        .execute(db.pool())
        .await?;

    Ok(db)
}

/// Helper to create AdapterRegistrationParams with .aos fields
fn make_aos_params(
    adapter_id: &str,
    name: &str,
    hash: &str,
    rank: i32,
    file_path: &str,
    file_hash: &str,
) -> AdapterRegistrationParams {
    AdapterRegistrationParams {
        adapter_id: adapter_id.to_string(),
        tenant_id: "tenant-1".to_string(),
        name: name.to_string(),
        hash_b3: hash.to_string(),
        rank,
        tier: "persistent".to_string(),
        alpha: (rank * 2) as f64,
        targets_json: "[]".to_string(),
        acl_json: None,
        languages_json: None,
        framework: None,
        category: "test".to_string(),
        scope: "general".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: None,
        aos_file_path: Some(file_path.to_string()),
        aos_file_hash: Some(file_hash.to_string()),
        adapter_name: None,
        tenant_namespace: None,
        domain: None,
        purpose: None,
        revision: None,
        parent_id: None,
        fork_type: None,
        fork_reason: None,
    }
}

// ============================================================================
// Test Case 1: UPDATE adapter state preserves aos_file fields
// ============================================================================
//
// Purpose: Verify that update_adapter_state() does not overwrite aos_file_path
// and aos_file_hash. These fields should remain unchanged after the update.
//
// Rationale: The UPDATE query intentionally uses partial updates to preserve
// existing aos_file data. This test confirms that behavior.

#[tokio::test]
async fn test_update_adapter_state_preserves_aos_fields() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter with .aos file metadata
    let params = make_aos_params(
        "test-adapter-001",
        "Test Adapter",
        "b3:abc123",
        16,
        "/path/to/adapter.aos",
        "b3:file_hash_123",
    );

    db.register_adapter_extended(params).await?;

    // Record initial aos_file values
    let initial_adapter = db
        .get_adapter("test-adapter-001")
        .await?
        .expect("Adapter should exist");

    assert_eq!(
        initial_adapter.aos_file_path,
        Some("/path/to/adapter.aos".to_string()),
        "Initial aos_file_path should be set"
    );
    assert_eq!(
        initial_adapter.aos_file_hash,
        Some("b3:file_hash_123".to_string()),
        "Initial aos_file_hash should be set"
    );

    // Update adapter state
    db.update_adapter_state("test-adapter-001", "hot", "Test state change")
        .await?;

    // Verify aos_file fields are preserved
    let updated_adapter = db
        .get_adapter("test-adapter-001")
        .await?
        .expect("Adapter should still exist");

    assert_eq!(
        updated_adapter.aos_file_path, initial_adapter.aos_file_path,
        "aos_file_path should be preserved after state update"
    );
    assert_eq!(
        updated_adapter.aos_file_hash, initial_adapter.aos_file_hash,
        "aos_file_hash should be preserved after state update"
    );
    assert_eq!(
        updated_adapter.current_state, "hot",
        "current_state should be updated"
    );

    Ok(())
}

// ============================================================================
// Test Case 2: UPDATE adapter memory preserves aos_file fields
// ============================================================================
//
// Purpose: Verify that update_adapter_memory() preserves aos_file fields.

#[tokio::test]
async fn test_update_adapter_memory_preserves_aos_fields() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter with .aos metadata
    let params = make_aos_params(
        "mem-test-001",
        "Memory Test Adapter",
        "b3:def456",
        8,
        "/path/to/mem_adapter.aos",
        "b3:mem_hash_456",
    );

    db.register_adapter_extended(params).await?;

    // Record initial values
    let initial_adapter = db
        .get_adapter("mem-test-001")
        .await?
        .expect("Adapter should exist");

    let initial_aos_path = initial_adapter.aos_file_path.clone();
    let initial_aos_hash = initial_adapter.aos_file_hash.clone();

    // Update memory
    db.update_adapter_memory("mem-test-001", 1024 * 1024 * 512) // 512 MB
        .await?;

    // Verify aos_file fields are preserved
    let updated_adapter = db
        .get_adapter("mem-test-001")
        .await?
        .expect("Adapter should exist");

    assert_eq!(
        updated_adapter.aos_file_path, initial_aos_path,
        "aos_file_path should be preserved after memory update"
    );
    assert_eq!(
        updated_adapter.aos_file_hash, initial_aos_hash,
        "aos_file_hash should be preserved after memory update"
    );
    assert_eq!(updated_adapter.memory_bytes, 1024 * 1024 * 512);

    Ok(())
}

// ============================================================================
// Test Case 3: UPDATE state AND memory (atomic) preserves aos_file fields
// ============================================================================
//
// Purpose: Verify that atomic state+memory updates preserve aos_file fields.

#[tokio::test]
async fn test_update_adapter_state_and_memory_preserves_aos_fields() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter with .aos metadata
    let params = make_aos_params(
        "atomic-test-001",
        "Atomic Test Adapter",
        "b3:ghi789",
        4,
        "/path/to/atomic.aos",
        "b3:atomic_hash_789",
    );

    db.register_adapter_extended(params).await?;

    // Record initial values
    let initial_adapter = db
        .get_adapter("atomic-test-001")
        .await?
        .expect("Adapter should exist");

    let initial_aos_path = initial_adapter.aos_file_path.clone();
    let initial_aos_hash = initial_adapter.aos_file_hash.clone();

    // Update both state and memory atomically
    db.update_adapter_state_and_memory(
        "atomic-test-001",
        "resident",
        2048 * 1024 * 1024, // 2 GB
        "Promoted to resident tier",
    )
    .await?;

    // Verify aos_file fields are preserved
    let updated_adapter = db
        .get_adapter("atomic-test-001")
        .await?
        .expect("Adapter should exist");

    assert_eq!(
        updated_adapter.aos_file_path, initial_aos_path,
        "aos_file_path should be preserved after atomic update"
    );
    assert_eq!(
        updated_adapter.aos_file_hash, initial_aos_hash,
        "aos_file_hash should be preserved after atomic update"
    );
    assert_eq!(updated_adapter.current_state, "resident");
    assert_eq!(updated_adapter.memory_bytes, 2048 * 1024 * 1024);

    Ok(())
}

// ============================================================================
// Test Case 4: UPDATE version preserves aos_file fields
// ============================================================================
//
// Purpose: Verify that update_adapter_version() preserves aos_file fields.

#[tokio::test]
async fn test_update_adapter_version_preserves_aos_fields() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter with .aos metadata
    let params = make_aos_params(
        "version-test-001",
        "Version Test Adapter",
        "b3:mno345",
        6,
        "/path/to/version.aos",
        "b3:version_hash_345",
    );

    db.register_adapter_extended(params).await?;

    // Record initial values
    let initial_adapter = db
        .get_adapter("version-test-001")
        .await?
        .expect("Adapter should exist");

    let initial_aos_path = initial_adapter.aos_file_path.clone();
    let initial_aos_hash = initial_adapter.aos_file_hash.clone();

    // Update version
    db.update_adapter_version("version-test-001", "2.0.0")
        .await?;

    // Verify aos_file fields are preserved
    let updated_adapter = db
        .get_adapter("version-test-001")
        .await?
        .expect("Adapter should exist");

    assert_eq!(
        updated_adapter.aos_file_path, initial_aos_path,
        "aos_file_path should be preserved after version update"
    );
    assert_eq!(
        updated_adapter.aos_file_hash, initial_aos_hash,
        "aos_file_hash should be preserved after version update"
    );
    assert_eq!(updated_adapter.version, "2.0.0");

    Ok(())
}

// ============================================================================
// Test Case 5: Transactional UPDATE preserves aos_file fields
// ============================================================================
//
// Purpose: Verify that transactional updates (update_adapter_state_tx,
// update_adapter_memory_tx) preserve aos_file fields.

#[tokio::test]
async fn test_transactional_update_preserves_aos_fields() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter with .aos metadata
    let params = make_aos_params(
        "tx-test-001",
        "TX Test Adapter",
        "b3:pqr678",
        3,
        "/path/to/tx.aos",
        "b3:tx_hash_678",
    );

    db.register_adapter_extended(params).await?;

    // Record initial values
    let initial_adapter = db
        .get_adapter("tx-test-001")
        .await?
        .expect("Adapter should exist");

    let initial_aos_path = initial_adapter.aos_file_path.clone();
    let initial_aos_hash = initial_adapter.aos_file_hash.clone();

    // Update with transaction
    db.update_adapter_state_tx("tx-test-001", "cold", "Transactional state change")
        .await?;

    // Verify aos_file fields are preserved
    let updated_adapter = db
        .get_adapter("tx-test-001")
        .await?
        .expect("Adapter should exist");

    assert_eq!(
        updated_adapter.aos_file_path, initial_aos_path,
        "aos_file_path should be preserved after transactional state update"
    );
    assert_eq!(
        updated_adapter.aos_file_hash, initial_aos_hash,
        "aos_file_hash should be preserved after transactional state update"
    );
    assert_eq!(updated_adapter.current_state, "cold");

    Ok(())
}

// ============================================================================
// Test Case 6: DELETE cascade properly handles aos_adapter_metadata
// ============================================================================
//
// Purpose: Verify that when an adapter is deleted, the associated
// aos_adapter_metadata record is properly cascade-deleted.

#[tokio::test]
async fn test_delete_adapter_cascade_deletes_aos_metadata() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter with .aos metadata
    let params = make_aos_params(
        "delete-test-001",
        "Delete Test Adapter",
        "b3:stu901",
        2,
        "/path/to/delete.aos",
        "b3:delete_hash_901",
    );

    let adapter_id = db.register_adapter_with_aos(params).await?;

    // Verify adapter and metadata exist
    let adapter = db
        .get_adapter("delete-test-001")
        .await?
        .expect("Adapter should exist");
    assert!(adapter.aos_file_path.is_some());
    assert!(adapter.aos_file_hash.is_some());

    let metadata_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM aos_adapter_metadata WHERE adapter_id = ?")
            .bind(&adapter_id)
            .fetch_one(db.pool())
            .await?;
    assert_eq!(metadata_before, 1);

    // Delete adapter
    db.delete_adapter(&adapter_id).await?;

    // Verify adapter is deleted
    let deleted_adapter = db.get_adapter("delete-test-001").await?;
    assert!(deleted_adapter.is_none());

    // Verify metadata is cascade-deleted
    let metadata_after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM aos_adapter_metadata WHERE adapter_id = ?")
            .bind(&adapter_id)
            .fetch_one(db.pool())
            .await?;
    assert_eq!(
        metadata_after, 0,
        "aos_adapter_metadata should be cascade-deleted"
    );

    Ok(())
}

// ============================================================================
// Test Case 7: Query adapters by state preserves aos_file fields in results
// ============================================================================
//
// Purpose: Verify that SELECT queries used by state/category/scope filters
// include aos_file_path and aos_file_hash in results.

#[tokio::test]
async fn test_query_adapters_by_state_includes_aos_fields() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter in "hot" state with .aos metadata
    let params = make_aos_params(
        "query-state-001",
        "Query State Test",
        "b3:vwx234",
        7,
        "/path/to/query.aos",
        "b3:query_hash_234",
    );

    db.register_adapter_extended(params).await?;

    // Update to "hot" state
    db.update_adapter_state("query-state-001", "hot", "For querying")
        .await?;

    // Query by state
    let hot_adapters = db.list_adapters_by_state("hot").await?;

    // Find our adapter
    let found = hot_adapters
        .iter()
        .find(|a| a.adapter_id == Some("query-state-001".to_string()))
        .expect("Adapter should be found in hot state query");

    // Verify aos_file fields are in the result
    assert!(found.aos_file_path.is_some());
    assert_eq!(found.aos_file_path.as_deref(), Some("/path/to/query.aos"));
    assert!(found.aos_file_hash.is_some());
    assert_eq!(found.aos_file_hash.as_deref(), Some("b3:query_hash_234"));

    Ok(())
}

// ============================================================================
// Test Case 8: Multiple sequential updates preserve aos_file fields
// ============================================================================
//
// Purpose: Verify that aos_file fields remain preserved through multiple
// sequential UPDATE operations (state -> memory).

#[tokio::test]
async fn test_multiple_sequential_updates_preserve_aos_fields() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Register adapter with .aos metadata
    let params = make_aos_params(
        "multi-update-001",
        "Multi Update Test",
        "b3:xyz567",
        5,
        "/path/to/multi.aos",
        "b3:multi_hash_567",
    );

    db.register_adapter_extended(params).await?;

    // Record initial aos_file values
    let initial_adapter = db
        .get_adapter("multi-update-001")
        .await?
        .expect("Adapter should exist");

    let expected_aos_path = initial_adapter.aos_file_path.clone();
    let expected_aos_hash = initial_adapter.aos_file_hash.clone();

    // Perform sequential updates
    db.update_adapter_state("multi-update-001", "hot", "Update 1")
        .await?;

    db.update_adapter_memory("multi-update-001", 256 * 1024 * 1024)
        .await?;

    db.update_adapter_version("multi-update-001", "3.0.0")
        .await?;

    // Verify aos_file fields remained constant through all updates
    let final_adapter = db
        .get_adapter("multi-update-001")
        .await?
        .expect("Adapter should exist");

    assert_eq!(
        final_adapter.aos_file_path, expected_aos_path,
        "aos_file_path should be preserved through all updates"
    );
    assert_eq!(
        final_adapter.aos_file_hash, expected_aos_hash,
        "aos_file_hash should be preserved through all updates"
    );

    // Verify other fields were actually updated
    assert_eq!(final_adapter.current_state, "hot");
    assert_eq!(final_adapter.memory_bytes, 256 * 1024 * 1024);
    assert_eq!(final_adapter.version, "3.0.0");

    Ok(())
}
