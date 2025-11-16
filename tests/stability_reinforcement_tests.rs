// Stability Reinforcement Tests
// Citation: Agent G Stability Reinforcement Plan
//
// These tests verify fixes for architectural drift issues including:
// - Race conditions in concurrent adapter state updates
// - Pinned adapter delete prevention
// - TTL automatic cleanup
// - Transaction rollback behavior

use adapteros_db::Db;
use anyhow::Result;
use std::sync::Arc;
use tokio::task::JoinSet;

#[tokio::test]
async fn test_concurrent_state_update_race_condition() -> Result<()> {
    // Setup: Create test database
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Create a test adapter
    let adapter_id = uuid::Uuid::now_v7().to_string();
    let name = "test-concurrent-adapter";
    let hash_b3 = "test-hash";

    let params = adapteros_db::AdapterRegistrationParams {
        adapter_id: adapter_id.clone(),
        name: name.to_string(),
        hash_b3: hash_b3.to_string(),
        rank: 16,
        tier: 1,
        languages_json: None,
        framework: None,
        category: "code".to_string(),
        scope: "global".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: None,
    };

    db.register_adapter_extended(params).await?;

    // Test: Concurrent state and memory updates using transactional methods
    let db_arc = Arc::new(db);
    let mut tasks = JoinSet::new();

    // Spawn 10 concurrent tasks that update state
    for i in 0..10 {
        let db_clone = Arc::clone(&db_arc);
        let adapter_id_clone = adapter_id.clone();
        tasks.spawn(async move {
            let state = if i % 2 == 0 { "hot" } else { "warm" };
            db_clone
                .update_adapter_state_tx(&adapter_id_clone, state, &format!("test-{}", i))
                .await
        });
    }

    // Spawn 10 concurrent tasks that update memory
    for i in 0..10 {
        let db_clone = Arc::clone(&db_arc);
        let adapter_id_clone = adapter_id.clone();
        tasks.spawn(async move {
            let memory = (i + 1) * 1024 * 1024; // Variable memory values
            db_clone
                .update_adapter_memory_tx(&adapter_id_clone, memory as i64)
                .await
        });
    }

    // Wait for all tasks to complete
    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        results.push(result.unwrap());
    }

    // Verify: All updates should succeed (no lost updates due to race conditions)
    let success_count = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        success_count, 20,
        "All concurrent updates should succeed without race conditions"
    );

    // Verify: Final adapter state should be consistent (one valid state, one valid memory)
    let adapter = db_arc
        .get_adapter(&adapter_id)
        .await?
        .expect("Adapter should exist");

    assert!(
        adapter.current_state == "hot" || adapter.current_state == "warm",
        "Final state should be one of the updated values"
    );
    assert!(
        adapter.memory_bytes > 0,
        "Final memory should be one of the updated values"
    );

    Ok(())
}

#[tokio::test]
async fn test_pinned_adapter_delete_prevention() -> Result<()> {
    // Setup: Create test database
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Create and register a test adapter
    let adapter_id = uuid::Uuid::now_v7().to_string();
    let params = adapteros_db::AdapterRegistrationParams {
        adapter_id: adapter_id.clone(),
        name: "test-pinned-adapter".to_string(),
        hash_b3: "test-hash".to_string(),
        rank: 16,
        tier: 1,
        languages_json: None,
        framework: None,
        category: "code".to_string(),
        scope: "global".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: None,
    };

    let id = db.register_adapter_extended(params).await?;

    // Get the adapter to retrieve its database ID
    let adapter = db
        .get_adapter(&adapter_id)
        .await?
        .expect("Adapter should exist");

    // Pin the adapter using the pinned column
    sqlx::query("UPDATE adapters SET pinned = 1 WHERE id = ?")
        .bind(&adapter.id)
        .execute(db.pool())
        .await?;

    // Test: Attempt to delete the pinned adapter
    let delete_result = db.delete_adapter(&adapter.id).await;

    // Verify: Deletion should fail with PolicyViolation error
    assert!(
        delete_result.is_err(),
        "Deleting a pinned adapter should fail"
    );

    let error_msg = delete_result.unwrap_err().to_string();
    assert!(
        error_msg.contains("pinned"),
        "Error should mention adapter is pinned: {}",
        error_msg
    );

    // Unpin the adapter
    sqlx::query("UPDATE adapters SET pinned = 0 WHERE id = ?")
        .bind(&adapter.id)
        .execute(db.pool())
        .await?;

    // Test: Deletion should succeed after unpinning
    let delete_result = db.delete_adapter(&adapter.id).await;
    assert!(
        delete_result.is_ok(),
        "Deleting an unpinned adapter should succeed"
    );

    // Verify: Adapter should no longer exist
    let adapter_check = db.get_adapter(&adapter_id).await?;
    assert!(adapter_check.is_none(), "Adapter should be deleted");

    Ok(())
}

#[tokio::test]
async fn test_time_based_pinned_adapter_delete_prevention() -> Result<()> {
    // Setup: Create test database
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Create and register a test adapter
    let adapter_id = uuid::Uuid::now_v7().to_string();
    let params = adapteros_db::AdapterRegistrationParams {
        adapter_id: adapter_id.clone(),
        name: "test-time-pinned-adapter".to_string(),
        hash_b3: "test-hash".to_string(),
        rank: 16,
        tier: 1,
        languages_json: None,
        framework: None,
        category: "code".to_string(),
        scope: "global".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: None,
    };

    let id = db.register_adapter_extended(params).await?;

    // Get the adapter
    let adapter = db
        .get_adapter(&adapter_id)
        .await?
        .expect("Adapter should exist");

    // Add a time-based pin (pinned indefinitely)
    db.pin_adapter(
        "default",
        &adapter_id,
        None, // No expiration
        "test reason",
        "test-user",
    )
    .await?;

    // Test: Attempt to delete the adapter with active pin
    let delete_result = db.delete_adapter(&adapter.id).await;

    // Verify: Deletion should fail
    assert!(
        delete_result.is_err(),
        "Deleting an adapter with active pin should fail"
    );

    let error_msg = delete_result.unwrap_err().to_string();
    assert!(
        error_msg.contains("pin"),
        "Error should mention active pins: {}",
        error_msg
    );

    // Unpin the adapter
    db.unpin_adapter("default", &adapter_id).await?;

    // Test: Deletion should succeed after unpinning
    let delete_result = db.delete_adapter(&adapter.id).await;
    assert!(
        delete_result.is_ok(),
        "Deleting adapter after unpinning should succeed"
    );

    Ok(())
}

#[tokio::test]
async fn test_ttl_automatic_cleanup() -> Result<()> {
    // Setup: Create test database
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Create an adapter with TTL set to past (already expired)
    let adapter_id = uuid::Uuid::now_v7().to_string();
    let params = adapteros_db::AdapterRegistrationParams {
        adapter_id: adapter_id.clone(),
        name: "test-expired-adapter".to_string(),
        hash_b3: "test-hash".to_string(),
        rank: 16,
        tier: 1,
        languages_json: None,
        framework: None,
        category: "ephemeral".to_string(),
        scope: "global".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: Some("2020-01-01 00:00:00".to_string()), // Past date
    };

    let id = db.register_adapter_extended(params).await?;

    // Verify adapter exists
    let adapter = db.get_adapter(&adapter_id).await?;
    assert!(adapter.is_some(), "Adapter should exist initially");

    // Test: Find expired adapters
    let expired = db.find_expired_adapters().await?;
    assert_eq!(
        expired.len(),
        1,
        "Should find one expired adapter"
    );
    assert_eq!(
        expired[0].adapter_id, adapter_id,
        "Expired adapter should match our test adapter"
    );

    // Simulate cleanup (what the background task does)
    let adapter = adapter.unwrap();
    db.delete_adapter(&adapter.id).await?;

    // Verify: Adapter should be deleted
    let adapter_check = db.get_adapter(&adapter_id).await?;
    assert!(adapter_check.is_none(), "Expired adapter should be deleted");

    // Verify: find_expired_adapters should return empty
    let expired_after = db.find_expired_adapters().await?;
    assert_eq!(
        expired_after.len(),
        0,
        "Should find no expired adapters after cleanup"
    );

    Ok(())
}

#[tokio::test]
async fn test_transaction_rollback_on_cascade_delete_failure() -> Result<()> {
    // Setup: Create test database
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Create a pinned adapter
    let adapter_id = uuid::Uuid::now_v7().to_string();
    let params = adapteros_db::AdapterRegistrationParams {
        adapter_id: adapter_id.clone(),
        name: "test-rollback-adapter".to_string(),
        hash_b3: "test-hash".to_string(),
        rank: 16,
        tier: 1,
        languages_json: None,
        framework: None,
        category: "code".to_string(),
        scope: "global".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: None,
    };

    let id = db.register_adapter_extended(params).await?;

    // Get adapter
    let adapter = db
        .get_adapter(&adapter_id)
        .await?
        .expect("Adapter should exist");

    // Pin it
    sqlx::query("UPDATE adapters SET pinned = 1 WHERE id = ?")
        .bind(&adapter.id)
        .execute(db.pool())
        .await?;

    // Test: Attempt cascade delete on pinned adapter
    let delete_result = db.delete_adapter_cascade(&adapter.id).await;

    // Verify: Should fail due to pin
    assert!(
        delete_result.is_err(),
        "Cascade delete of pinned adapter should fail"
    );

    // Verify: Adapter should still exist (transaction rolled back)
    let adapter_check = db.get_adapter(&adapter_id).await?;
    assert!(
        adapter_check.is_some(),
        "Adapter should still exist after failed cascade delete"
    );

    Ok(())
}

#[tokio::test]
async fn test_atomic_state_and_memory_update() -> Result<()> {
    // Setup: Create test database
    let db = Db::connect(":memory:").await?;
    db.migrate().await?;

    // Create test adapter
    let adapter_id = uuid::Uuid::now_v7().to_string();
    let params = adapteros_db::AdapterRegistrationParams {
        adapter_id: adapter_id.clone(),
        name: "test-atomic-update".to_string(),
        hash_b3: "test-hash".to_string(),
        rank: 16,
        tier: 1,
        languages_json: None,
        framework: None,
        category: "code".to_string(),
        scope: "global".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: None,
    };

    db.register_adapter_extended(params).await?;

    // Test: Atomic update of both state and memory
    db.update_adapter_state_and_memory(&adapter_id, "hot", 5_000_000, "test atomic")
        .await?;

    // Verify: Both fields updated
    let adapter = db
        .get_adapter(&adapter_id)
        .await?
        .expect("Adapter should exist");

    assert_eq!(adapter.current_state, "hot", "State should be updated");
    assert_eq!(
        adapter.memory_bytes, 5_000_000,
        "Memory should be updated"
    );

    Ok(())
}
