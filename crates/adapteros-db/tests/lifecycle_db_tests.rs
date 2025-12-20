//! Integration tests for lifecycle database operations

use adapteros_db::Db;

#[tokio::test]
async fn test_adapter_lifecycle_transition() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create tenant first (required for FK constraint)
    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    // Register a test adapter
    // NOTE: Adapters default to 'draft' lifecycle_state
    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .adapter_id("test-adapter-001")
        .tenant_id(&tenant_id)
        .name("Test Adapter")
        .hash_b3("abc123")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();

    db.register_adapter(params)
        .await
        .expect("Failed to register adapter");

    // Set adapter to 'active' state so we can test valid transitions
    // Also seed required artifacts for ready/active states
    sqlx::query(
        "UPDATE adapters SET lifecycle_state = 'active', aos_file_path = 'path/to.aos', aos_file_hash = 'hash123', content_hash_b3 = 'content123' WHERE adapter_id = ?",
    )
    .bind("test-adapter-001")
    .execute(db.pool())
    .await
    .expect("Failed to update adapter state");

    // Test Active → Deprecated transition
    let result = db
        .transition_adapter_lifecycle("test-adapter-001", "deprecated", "End of life", "test-user")
        .await;

    assert!(
        result.is_ok(),
        "Failed to transition Active → Deprecated: {:?}",
        result
    );
    let new_version = result.unwrap();
    assert_eq!(new_version, "1.0.1");

    // Test Deprecated → Retired transition
    let result = db
        .transition_adapter_lifecycle("test-adapter-001", "retired", "Cleanup", "system")
        .await;

    assert!(
        result.is_ok(),
        "Failed to transition Deprecated → Retired: {:?}",
        result
    );
    let new_version = result.unwrap();
    assert_eq!(new_version, "1.0.2");

    // Test no-op transition (retired → retired should not bump version)
    let result = db
        .transition_adapter_lifecycle("test-adapter-001", "retired", "Same state", "admin")
        .await;

    assert!(
        result.is_ok(),
        "Failed no-op transition Retired → Retired: {:?}",
        result
    );
    let new_version = result.unwrap();
    assert_eq!(new_version, "1.0.2"); // Version unchanged for no-op
}

#[tokio::test]
async fn test_lifecycle_history_query() {
    let db = Db::new_in_memory()
        .await
        .expect("Failed to create test database");

    // Create tenant first (required for FK constraint)
    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant");

    // Register test adapter
    // NOTE: Adapters default to 'draft' lifecycle_state
    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .adapter_id("test-adapter-002")
        .tenant_id(&tenant_id)
        .name("Test Adapter 2")
        .hash_b3("def456")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();

    db.register_adapter(params)
        .await
        .expect("Failed to register adapter");

    // Set adapter to 'active' state so we can test valid transitions
    // Also seed required artifacts for ready/active states
    sqlx::query(
        "UPDATE adapters SET lifecycle_state = 'active', aos_file_path = 'path/to.aos', aos_file_hash = 'hash123', content_hash_b3 = 'content123' WHERE adapter_id = ?",
    )
    .bind("test-adapter-002")
    .execute(db.pool())
    .await
    .expect("Failed to update adapter state");

    // Perform multiple transitions (starting from 'active')
    let transitions = vec![
        ("deprecated", "End of life"),
        ("retired", "Full retirement"),
    ];

    for (new_state, reason) in transitions {
        db.transition_adapter_lifecycle("test-adapter-002", new_state, reason, "test-user")
            .await
            .expect("Transition should succeed");
    }

    // Query lifecycle history
    // NOTE: After migration 0186, adapter_version_history was redesigned for the
    // adapter_versions system (requires version_id FK). The legacy adapters table
    // system no longer writes to this table, so history will be empty.
    // Lifecycle state is tracked directly in adapters.lifecycle_state column.
    let history = db
        .get_adapter_lifecycle_history("test-adapter-002")
        .await
        .expect("Failed to query lifecycle history");

    // History is empty for legacy adapters (schema changed in migration 0186)
    assert!(
        history.is_empty(),
        "Legacy adapters no longer write to adapter_version_history"
    );

    // Verify state was updated correctly by checking the adapter directly
    let adapter = db
        .get_adapter("test-adapter-002")
        .await
        .expect("Failed to get adapter")
        .expect("Adapter should exist");
    assert_eq!(adapter.lifecycle_state, "retired");
    assert_eq!(adapter.version, "1.0.2"); // Two transitions: 1.0.0 -> 1.0.1 -> 1.0.2
}
