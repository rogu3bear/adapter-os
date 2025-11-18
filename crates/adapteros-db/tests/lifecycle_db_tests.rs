//! Integration tests for lifecycle database operations

use adapteros_db::Db;

#[tokio::test]
async fn test_adapter_lifecycle_transition() {
    let db = Db::new_in_memory().await.expect("Failed to create test database");

    // Register a test adapter
    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .adapter_id("test-adapter-001")
        .name("Test Adapter")
        .hash_b3("abc123")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();

    db.register_adapter(params).await.expect("Failed to register adapter");

    // Test Draft → Active transition
    let result = db.transition_adapter_lifecycle(
        "test-adapter-001",
        "active",
        "Initial activation",
        "test-user"
    ).await;

    assert!(result.is_ok(), "Failed to transition Draft → Active: {:?}", result);
    let new_version = result.unwrap();
    assert_eq!(new_version, "1.0.1");

    // Test Active → Deprecated transition
    let result = db.transition_adapter_lifecycle(
        "test-adapter-001",
        "deprecated",
        "End of life",
        "system"
    ).await;

    assert!(result.is_ok(), "Failed to transition Active → Deprecated: {:?}", result);
    let new_version = result.unwrap();
    assert_eq!(new_version, "1.0.2");

    // Test Deprecated → Retired transition
    let result = db.transition_adapter_lifecycle(
        "test-adapter-001",
        "retired",
        "Cleanup",
        "admin"
    ).await;

    assert!(result.is_ok(), "Failed to transition Deprecated → Retired: {:?}", result);
    let new_version = result.unwrap();
    assert_eq!(new_version, "1.0.3");
}

#[tokio::test]
async fn test_lifecycle_history_query() {
    let db = Db::new_in_memory().await.expect("Failed to create test database");

    // Register test adapter
    let params = adapteros_db::adapters::AdapterRegistrationBuilder::new()
        .adapter_id("test-adapter-002")
        .name("Test Adapter 2")
        .hash_b3("def456")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();

    db.register_adapter(params).await.expect("Failed to register adapter");

    // Perform multiple transitions
    let transitions = vec![
        ("active", "Initial activation"),
        ("deprecated", "End of life"),
    ];

    for (new_state, reason) in transitions {
        db.transition_adapter_lifecycle(
            "test-adapter-002",
            new_state,
            reason,
            "test-user"
        )
        .await
        .expect("Transition should succeed");
    }

    // Query lifecycle history
    let history = db.get_adapter_lifecycle_history("test-adapter-002")
        .await
        .expect("Failed to query lifecycle history");

    // Verify history
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].lifecycle_state, "deprecated");
    assert_eq!(history[0].initiated_by, "test-user");
    assert_eq!(history[1].lifecycle_state, "active");
    assert_eq!(history[1].initiated_by, "test-user");
}
