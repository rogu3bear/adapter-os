//! Integration tests for lifecycle database operations

use adapteros_core::lifecycle::{LifecycleState, TransitionReason};
use adapteros_db::lifecycle::{LifecycleMetadata, transition_adapter_lifecycle};
use adapteros_db::Db;
use chrono::Utc;

#[tokio::test]
async fn test_adapter_lifecycle_transition() {
    let db = Db::new_in_memory().await.expect("Failed to create test database");

    // Create test adapter
    let adapter_id = "test-adapter-001";
    let tenant_id = "test-tenant";

    // Insert test adapter
    sqlx::query(
        r#"
        INSERT INTO adapters (adapter_id, tenant_id, created_at, current_state, activation_count, memory_bytes)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(adapter_id)
    .bind(tenant_id)
    .bind(Utc::now().to_rfc3339())
    .bind("unloaded")
    .bind(0)
    .bind(100_000_000)
    .execute(db.pool())
    .await
    .expect("Failed to insert test adapter");

    // Test Draft → Active transition
    let metadata = LifecycleMetadata {
        adapter_id: adapter_id.to_string(),
        tenant_id: tenant_id.to_string(),
        from_state: LifecycleState::Draft,
        to_state: LifecycleState::Active,
        reason: TransitionReason::Manual("Initial activation".to_string()),
        performed_by: "test-user".to_string(),
    };

    let result = transition_adapter_lifecycle(&db, &metadata).await;
    assert!(result.is_ok(), "Failed to transition Draft → Active: {:?}", result);

    // Verify transition in database
    let row: (String,) = sqlx::query_as(
        "SELECT lifecycle_state FROM adapter_lifecycle_history WHERE adapter_id = ? ORDER BY transitioned_at DESC LIMIT 1",
    )
    .bind(adapter_id)
    .fetch_one(db.pool())
    .await
    .expect("Failed to query lifecycle history");

    assert_eq!(row.0, "Active");

    // Test Active → Deprecated transition
    let metadata = LifecycleMetadata {
        adapter_id: adapter_id.to_string(),
        tenant_id: tenant_id.to_string(),
        from_state: LifecycleState::Active,
        to_state: LifecycleState::Deprecated,
        reason: TransitionReason::Automatic("End of life".to_string()),
        performed_by: "system".to_string(),
    };

    let result = transition_adapter_lifecycle(&db, &metadata).await;
    assert!(result.is_ok(), "Failed to transition Active → Deprecated: {:?}", result);

    // Test invalid Retired → Active transition (should fail)
    let metadata = LifecycleMetadata {
        adapter_id: adapter_id.to_string(),
        tenant_id: tenant_id.to_string(),
        from_state: LifecycleState::Retired,
        to_state: LifecycleState::Active,
        reason: TransitionReason::Manual("Invalid transition".to_string()),
        performed_by: "test-user".to_string(),
    };

    let result = transition_adapter_lifecycle(&db, &metadata).await;
    assert!(result.is_err(), "Invalid Retired → Active transition should fail");

    // Test Deprecated → Retired transition
    let metadata = LifecycleMetadata {
        adapter_id: adapter_id.to_string(),
        tenant_id: tenant_id.to_string(),
        from_state: LifecycleState::Deprecated,
        to_state: LifecycleState::Retired,
        reason: TransitionReason::Manual("Cleanup".to_string()),
        performed_by: "admin".to_string(),
    };

    let result = transition_adapter_lifecycle(&db, &metadata).await;
    assert!(result.is_ok(), "Failed to transition Deprecated → Retired: {:?}", result);
}

#[tokio::test]
async fn test_lifecycle_history_query() {
    let db = Db::new_in_memory().await.expect("Failed to create test database");

    // Create test adapter
    let adapter_id = "test-adapter-002";
    let tenant_id = "test-tenant";

    // Insert test adapter
    sqlx::query(
        r#"
        INSERT INTO adapters (adapter_id, tenant_id, created_at, current_state, activation_count, memory_bytes)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(adapter_id)
    .bind(tenant_id)
    .bind(Utc::now().to_rfc3339())
    .bind("unloaded")
    .bind(0)
    .bind(100_000_000)
    .execute(db.pool())
    .await
    .expect("Failed to insert test adapter");

    // Perform multiple transitions
    let transitions = vec![
        (LifecycleState::Draft, LifecycleState::Active),
        (LifecycleState::Active, LifecycleState::Deprecated),
    ];

    for (from_state, to_state) in transitions {
        let metadata = LifecycleMetadata {
            adapter_id: adapter_id.to_string(),
            tenant_id: tenant_id.to_string(),
            from_state,
            to_state,
            reason: TransitionReason::Manual("Test transition".to_string()),
            performed_by: "test-user".to_string(),
        };

        transition_adapter_lifecycle(&db, &metadata)
            .await
            .expect("Transition should succeed");
    }

    // Query lifecycle history
    let history = sqlx::query_as::<_, (String, String)>(
        "SELECT lifecycle_state, transitioned_by FROM adapter_lifecycle_history WHERE adapter_id = ? ORDER BY transitioned_at ASC",
    )
    .bind(adapter_id)
    .fetch_all(db.pool())
    .await
    .expect("Failed to query lifecycle history");

    // Verify history
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].0, "Active");
    assert_eq!(history[0].1, "test-user");
    assert_eq!(history[1].0, "Deprecated");
    assert_eq!(history[1].1, "test-user");
}
