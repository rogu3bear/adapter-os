//! Integration tests for lifecycle database operations
#![allow(deprecated)]

use adapteros_db::Db;
use tempfile::TempDir;

async fn promote_adapter_to_active(db: &Db, adapter_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE adapters SET lifecycle_state = 'training' WHERE adapter_id = ?")
        .bind(adapter_id)
        .execute(db.pool())
        .await?;

    sqlx::query("UPDATE adapters SET lifecycle_state = 'ready' WHERE adapter_id = ?")
        .bind(adapter_id)
        .execute(db.pool())
        .await?;

    sqlx::query(
        "UPDATE adapters SET lifecycle_state = 'active', aos_file_path = 'path/to.aos', aos_file_hash = 'hash123', content_hash_b3 = 'content123' WHERE adapter_id = ?",
    )
    .bind(adapter_id)
    .execute(db.pool())
    .await?;

    Ok(())
}

async fn create_test_db_persistent() -> (Db, TempDir) {
    let temp_dir = TempDir::with_prefix("aos-test-")
        .expect("Failed to create temporary directory for lifecycle database test");
    let db_path = temp_dir.path().join("test.db");
    let db = Db::connect(
        db_path
            .to_str()
            .expect("Failed to convert database path to valid UTF-8 string for lifecycle test"),
    )
    .await
    .expect("Failed to connect to persistent test database for lifecycle operations");
    db.migrate()
        .await
        .expect("Failed to apply database migrations to lifecycle test database");
    (db, temp_dir)
}

#[tokio::test]
async fn test_adapter_lifecycle_transition() {
    let (db, _temp_dir) = create_test_db_persistent().await;

    // Create tenant first (required for FK constraint)
    let tenant_id = db
        .create_tenant("Test Tenant", false)
        .await
        .expect("Failed to create tenant for lifecycle transition test");

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
        .expect("Failed to register test adapter for lifecycle transition test");

    // Move through allowed transitions to reach active (triggers enforce lifecycle rules).
    promote_adapter_to_active(&db, "test-adapter-001")
        .await
        .expect("Failed to promote test adapter to active state for lifecycle transition test");

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
    let (db, _temp_dir) = create_test_db_persistent().await;

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

    // Move through allowed transitions to reach active (triggers enforce lifecycle rules).
    promote_adapter_to_active(&db, "test-adapter-002")
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

    // Query lifecycle history (records now stored in adapter_lifecycle_history via migration 0222)
    let mut history = db
        .get_adapter_lifecycle_history("test-adapter-002")
        .await
        .expect("Failed to query lifecycle history");

    assert_eq!(history.len(), 2);

    // Order by version to avoid timestamp tie-break sensitivity
    history.sort_by(|a, b| a.version.cmp(&b.version));

    assert_eq!(history[0].entity_id, "test-adapter-002");
    assert_eq!(history[0].lifecycle_state, "deprecated");
    assert_eq!(
        history[0].previous_lifecycle_state.as_deref(),
        Some("active")
    );
    assert_eq!(history[0].version, "1.0.1");
    assert_eq!(history[0].reason.as_deref(), Some("End of life"));
    assert_eq!(history[0].initiated_by, "test-user");

    assert_eq!(history[1].entity_id, "test-adapter-002");
    assert_eq!(history[1].lifecycle_state, "retired");
    assert_eq!(
        history[1].previous_lifecycle_state.as_deref(),
        Some("deprecated")
    );
    assert_eq!(history[1].version, "1.0.2");
    assert_eq!(history[1].reason.as_deref(), Some("Full retirement"));
    assert_eq!(history[1].initiated_by, "test-user");

    // Verify state was updated correctly by checking the adapter directly
    let adapter = db
        .get_adapter("test-adapter-002")
        .await
        .expect("Failed to get adapter")
        .expect("Adapter should exist");
    assert_eq!(adapter.lifecycle_state, "retired");
    assert_eq!(adapter.version, "1.0.2"); // Two transitions: 1.0.0 -> 1.0.1 -> 1.0.2
}
