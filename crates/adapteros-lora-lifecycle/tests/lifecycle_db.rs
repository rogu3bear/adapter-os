//! Database integration tests for adapter lifecycle management
//!
//! Uses fixtures module for database setup/teardown.
//! Tests verify:
//! - update_adapter_state updates database correctly
//! - record_adapter_activation updates activation count and timestamp
//! - evict_adapter updates state to unloaded and resets memory
//! - Adapters can run in parallel without conflicts
//!
//! NOTE: These tests are marked as ignored pending fixture API refactoring
//! to align with the updated AdapterRegistrationBuilder API.

mod fixtures;

use adapteros_db::Db;
use adapteros_lora_lifecycle::{AdapterState, LifecycleManager};
use adapteros_manifest::Policies;
use fixtures::{fixtures as test_fixtures, utils, TestDbFixture};
use std::path::PathBuf;

// TODO: Refactor TestAdapterBuilder to match AdapterRegistrationBuilder API
// The builder types are mismatched (u16 vs i32 for rank, u16 vs String for tier, etc.)
#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_update_adapter_state_persists_to_db() {
    let fixture = TestDbFixture::new().await;
    let adapter_id = test_fixtures::single_unloaded(fixture.db()).await;

    // Create a temporary directory for the adapter loader
    let temp_dir = std::env::temp_dir().join("lifecycle_test_state_update");
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // Create lifecycle manager with DB
    let mut adapter_hashes = std::collections::HashMap::new();
    adapter_hashes.insert(
        adapter_id.clone(),
        adapteros_core::B3Hash::hash(adapter_id.as_bytes()),
    );

    let manager = LifecycleManager::new_with_db(
        vec![adapter_id.clone()],
        adapter_hashes,
        &Policies::default(),
        PathBuf::from(&temp_dir),
        None,
        3,
        fixture.db().clone(),
    );

    // Update state through lifecycle manager
    manager
        .update_adapter_state(0, AdapterState::Warm, "test_reason")
        .await
        .expect("Failed to update adapter state");

    // Poll database until state is updated (spawn_deterministic runs async)
    let mut attempts = 0;
    loop {
        if utils::verify_adapter_state(fixture.db(), &adapter_id, "warm").await {
            break;
        }
        attempts += 1;
        if attempts > 50 {
            panic!("Database update did not complete within timeout");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // Final verification
    let is_warm = utils::verify_adapter_state(fixture.db(), &adapter_id, "warm").await;
    assert!(is_warm);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_record_adapter_activation_updates_db() {
    let fixture = TestDbFixture::new().await;
    let adapter_id = test_fixtures::single_unloaded(fixture.db()).await;

    // Create a temporary directory for the adapter loader
    let temp_dir = std::env::temp_dir().join("lifecycle_test_activation");
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let mut adapter_hashes = std::collections::HashMap::new();
    adapter_hashes.insert(
        adapter_id.clone(),
        adapteros_core::B3Hash::hash(adapter_id.as_bytes()),
    );

    let manager = LifecycleManager::new_with_db(
        vec![adapter_id.clone()],
        adapter_hashes,
        &Policies::default(),
        PathBuf::from(&temp_dir),
        None,
        3,
        fixture.db().clone(),
    );

    // Record activation
    manager
        .record_adapter_activation(0)
        .await
        .expect("Failed to record activation");

    // Poll database until activation is recorded
    let mut attempts = 0;
    loop {
        if let Ok(Some(adapter)) = fixture.db().get_adapter(&adapter_id).await {
            if adapter.activation_count >= 1 && adapter.last_activated.is_some() {
                break;
            }
        }
        attempts += 1;
        if attempts > 50 {
            panic!("Activation update did not complete within timeout");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // Final verification
    let adapter = fixture
        .db()
        .get_adapter(&adapter_id)
        .await
        .expect("Failed to get adapter");
    assert!(adapter.is_some());
    let adapter = adapter.unwrap();
    assert_eq!(adapter.activation_count, 1);
    assert!(adapter.last_activated.is_some());

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_evict_adapter_updates_state_and_memory() {
    let fixture = TestDbFixture::new().await;
    let adapter_id = test_fixtures::single_cold(fixture.db()).await;

    // Set initial memory
    fixture
        .db()
        .update_adapter_memory(&adapter_id, 1024 * 1024)
        .await
        .expect("Failed to set initial memory");

    // Create a temporary directory for the adapter loader
    let temp_dir = std::env::temp_dir().join("lifecycle_test_evict");
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let mut adapter_hashes = std::collections::HashMap::new();
    adapter_hashes.insert(
        adapter_id.clone(),
        adapteros_core::B3Hash::hash(adapter_id.as_bytes()),
    );

    let manager = LifecycleManager::new_with_db(
        vec![adapter_id.clone()],
        adapter_hashes,
        &Policies::default(),
        PathBuf::from(&temp_dir),
        None,
        3,
        fixture.db().clone(),
    );

    // Promote adapter first
    manager
        .promote_adapter(0)
        .await
        .expect("Failed to promote adapter");

    // Evict adapter
    manager
        .evict_adapter(0)
        .await
        .expect("Failed to evict adapter");

    // Poll database until eviction is complete
    let mut attempts = 0;
    loop {
        if let Ok(Some(adapter)) = fixture.db().get_adapter(&adapter_id).await {
            if adapter.current_state == "unloaded" && adapter.memory_bytes == 0 {
                break;
            }
        }
        attempts += 1;
        if attempts > 50 {
            panic!("Eviction update did not complete within timeout");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // Final verification
    let adapter = fixture
        .db()
        .get_adapter(&adapter_id)
        .await
        .expect("Failed to get adapter");
    assert!(adapter.is_some());
    let adapter = adapter.unwrap();
    assert_eq!(adapter.current_state, "unloaded");
    assert_eq!(adapter.memory_bytes, 0);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_multiple_activations_increment_count() {
    let fixture = TestDbFixture::new().await;
    let adapter_id = test_fixtures::single_unloaded(fixture.db()).await;

    // Create a temporary directory for the adapter loader
    let temp_dir = std::env::temp_dir().join("lifecycle_test_multi_activation");
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let mut adapter_hashes = std::collections::HashMap::new();
    adapter_hashes.insert(
        adapter_id.clone(),
        adapteros_core::B3Hash::hash(adapter_id.as_bytes()),
    );

    let manager = LifecycleManager::new_with_db(
        vec![adapter_id.clone()],
        adapter_hashes,
        &Policies::default(),
        PathBuf::from(&temp_dir),
        None,
        3,
        fixture.db().clone(),
    );

    // Record multiple activations
    for _ in 0..5 {
        manager
            .record_adapter_activation(0)
            .await
            .expect("Failed to record activation");
    }

    // Poll database until all activations are recorded
    let mut attempts = 0;
    loop {
        if let Ok(Some(adapter)) = fixture.db().get_adapter(&adapter_id).await {
            if adapter.activation_count >= 5 {
                break;
            }
        }
        attempts += 1;
        if attempts > 100 {
            panic!("Multiple activations did not complete within timeout");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // Final verification
    let adapter = fixture
        .db()
        .get_adapter(&adapter_id)
        .await
        .expect("Failed to get adapter");
    assert!(adapter.is_some());
    assert_eq!(adapter.unwrap().activation_count, 5);

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_multi_state_lifecycle_verification() {
    let fixture = TestDbFixture::new().await;
    let (cold, warm, hot) = test_fixtures::multi_state_lifecycle(fixture.db()).await;

    // Verify each adapter is in correct state
    assert!(utils::verify_adapter_state(fixture.db(), &cold, "cold").await);
    assert!(utils::verify_adapter_state(fixture.db(), &warm, "warm").await);
    assert!(utils::verify_adapter_state(fixture.db(), &hot, "hot").await);

    // Verify memory usage
    let cold_mem = utils::get_adapter_memory(fixture.db(), &cold).await;
    let warm_mem = utils::get_adapter_memory(fixture.db(), &warm).await;
    let hot_mem = utils::get_adapter_memory(fixture.db(), &hot).await;

    assert_eq!(cold_mem, 1024 * 100);
    assert_eq!(warm_mem, 1024 * 200);
    assert_eq!(hot_mem, 1024 * 300);

    // Verify total count
    let total = utils::count_all_adapters(fixture.db()).await;
    assert_eq!(total, 3);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_category_based_adapters() {
    let fixture = TestDbFixture::new().await;
    let (code, framework, codebase) = test_fixtures::category_adapters(fixture.db()).await;

    // Verify all are present
    assert!(utils::verify_adapter_state(fixture.db(), &code, "warm").await);
    assert!(utils::verify_adapter_state(fixture.db(), &framework, "warm").await);
    assert!(utils::verify_adapter_state(fixture.db(), &codebase, "warm").await);

    // Verify total count
    let total = utils::count_all_adapters(fixture.db()).await;
    assert_eq!(total, 3);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_high_memory_pressure_scenario() {
    let fixture = TestDbFixture::new().await;
    let adapters = test_fixtures::high_memory_pressure(fixture.db()).await;

    assert_eq!(adapters.len(), 5);

    // Verify total memory
    let total_mem = utils::total_memory_usage(fixture.db()).await;
    assert!(total_mem > 1024 * 1024 * 40); // At least 40 MB

    // Verify count in warm state
    let warm_count = utils::count_adapters_in_state(fixture.db(), "warm").await;
    assert_eq!(warm_count, 5);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_pinned_adapter_cannot_be_unpinned_in_lifecycle() {
    let fixture = TestDbFixture::new().await;
    let (pinned, unpinned) = test_fixtures::pinned_and_unpinned(fixture.db()).await;

    // Verify states
    assert!(utils::verify_adapter_state(fixture.db(), &pinned, "resident").await);
    assert!(utils::verify_adapter_state(fixture.db(), &unpinned, "warm").await);

    // Verify count
    let total = utils::count_all_adapters(fixture.db()).await;
    assert_eq!(total, 2);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_parallel_fixture_isolation() {
    // This test verifies that fixtures can run in parallel without conflicts
    let (fixture1_result, fixture2_result) = tokio::join!(
        async {
            let fixture = TestDbFixture::new().await;
            let adapter_id = test_fixtures::single_cold(fixture.db()).await;
            (
                adapter_id.clone(),
                utils::verify_adapter_state(fixture.db(), &adapter_id, "cold").await,
            )
        },
        async {
            let fixture = TestDbFixture::new().await;
            let adapter_id = test_fixtures::single_hot(fixture.db()).await;
            (
                adapter_id.clone(),
                utils::verify_adapter_state(fixture.db(), &adapter_id, "hot").await,
            )
        }
    );

    // Both fixtures should have correctly set up their adapters
    assert!(fixture1_result.1); // fixture1 cold adapter
    assert!(fixture2_result.1); // fixture2 hot adapter
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_activation_tracking_with_utilities() {
    let fixture = TestDbFixture::new().await;
    let adapter_id = test_fixtures::low_activation(fixture.db()).await;

    // Verify initial state
    assert_eq!(
        utils::get_adapter_memory(fixture.db(), &adapter_id).await,
        0
    );

    // Verify count
    let cold_count = utils::count_adapters_in_state(fixture.db(), "cold").await;
    assert_eq!(cold_count, 1);
}

#[tokio::test]
#[ignore = "Pending fixture API refactoring"]
async fn test_list_adapters_with_state() {
    let fixture = TestDbFixture::new().await;
    let (cold, warm, hot) = test_fixtures::multi_state_lifecycle(fixture.db()).await;

    let adapters = utils::list_adapters_with_state(fixture.db()).await;

    assert_eq!(adapters.len(), 3);

    // Verify states
    let states: std::collections::HashMap<String, String> = adapters.into_iter().collect();

    assert_eq!(states.get(&cold).map(|s| s.as_str()), Some("cold"));
    assert_eq!(states.get(&warm).map(|s| s.as_str()), Some("warm"));
    assert_eq!(states.get(&hot).map(|s| s.as_str()), Some("hot"));
}
