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
    // NOTE: Adapters default to 'active' lifecycle_state per migration 0068
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

    // Test Active → Deprecated transition (adapters start in 'active' state)
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
    // NOTE: Adapters default to 'active' lifecycle_state per migration 0068
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
    let history = db
        .get_adapter_lifecycle_history("test-adapter-002")
        .await
        .expect("Failed to query lifecycle history");

    // Verify history contains both transitions
    assert_eq!(history.len(), 2);

    // Find each transition by lifecycle_state (order may vary due to timestamp precision)
    let deprecated_entry = history
        .iter()
        .find(|e| e.lifecycle_state == "deprecated")
        .expect("Should have deprecated transition");
    let retired_entry = history
        .iter()
        .find(|e| e.lifecycle_state == "retired")
        .expect("Should have retired transition");

    assert_eq!(
        deprecated_entry.previous_lifecycle_state,
        Some("active".to_string())
    );
    assert_eq!(deprecated_entry.initiated_by, "test-user");
    assert_eq!(
        retired_entry.previous_lifecycle_state,
        Some("deprecated".to_string())
    );
    assert_eq!(retired_entry.initiated_by, "test-user");
}
