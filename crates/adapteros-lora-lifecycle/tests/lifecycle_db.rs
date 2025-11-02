//! Database integration tests for adapter lifecycle management
//!
//! Tests verify:
//! - update_adapter_state updates database correctly
//! - record_adapter_activation updates activation count and timestamp
//! - evict_adapter updates state to unloaded and resets memory

use adapteros_db::Db;
use adapteros_lora_lifecycle::{AdapterState, LifecycleManager};
use adapteros_manifest::Policies;
use std::path::PathBuf;
use tempfile::tempdir;

async fn setup_test_db() -> Db {
    let db = Db::connect(":memory:")
        .await
        .expect("Failed to create test DB");
    db.migrate().await.expect("Failed to run migrations");
    db
}

fn test_policies() -> Policies {
    Policies::default()
}

#[tokio::test]
async fn test_update_adapter_state_persists_to_db() {
    let db = setup_test_db().await;

    // Create test adapter in database
    let adapter_id = "test-adapter-state";
    db.register_adapter(
        adapteros_db::AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id)
            .name("Test Adapter")
            .hash_b3("test_hash")
            .rank(16)
            .tier(2)
            .build()
            .expect("Failed to build adapter params"),
    )
    .await
    .expect("Failed to register adapter");

    // Create lifecycle manager with DB
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let adapter_names = vec![adapter_id.to_string()];
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        &test_policies(),
        PathBuf::from(temp_dir.path()),
        None,
        3,
        db.clone(),
    );

    // Update state through lifecycle manager
    manager
        .update_adapter_state(0, AdapterState::Warm, "test_reason")
        .await
        .expect("Failed to update adapter state");

    // Poll database until state is updated (spawn_deterministic runs async)
    let mut attempts = 0;
    loop {
        let adapter = db
            .get_adapter(adapter_id)
            .await
            .expect("Failed to get adapter");
        if let Some(adapter) = adapter {
            if adapter.current_state == "warm" {
                break;
            }
        }
        attempts += 1;
        if attempts > 50 {
            panic!("Database update did not complete within timeout");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // Final verification
    let adapter = db
        .get_adapter(adapter_id)
        .await
        .expect("Failed to get adapter");
    assert!(adapter.is_some());
    assert_eq!(adapter.unwrap().current_state, "warm");
}

#[tokio::test]
async fn test_record_adapter_activation_updates_db() {
    let db = setup_test_db().await;

    let adapter_id = "test-adapter-activation";
    db.register_adapter(
        adapteros_db::AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id)
            .name("Test Adapter")
            .hash_b3("test_hash")
            .rank(16)
            .tier(2)
            .build()
            .expect("Failed to build adapter params"),
    )
    .await
    .expect("Failed to register adapter");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let adapter_names = vec![adapter_id.to_string()];
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        &test_policies(),
        PathBuf::from(temp_dir.path()),
        None,
        3,
        db.clone(),
    );

    // Record activation
    manager
        .record_adapter_activation(0)
        .await
        .expect("Failed to record activation");

    // Poll database until activation is recorded
    let mut attempts = 0;
    loop {
        let adapter = db
            .get_adapter(adapter_id)
            .await
            .expect("Failed to get adapter");
        if let Some(adapter) = &adapter {
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
    let adapter = db
        .get_adapter(adapter_id)
        .await
        .expect("Failed to get adapter");
    assert!(adapter.is_some());
    let adapter = adapter.unwrap();
    assert_eq!(adapter.activation_count, 1);
    assert!(adapter.last_activated.is_some());
}

#[tokio::test]
async fn test_evict_adapter_updates_state_and_memory() {
    let db = setup_test_db().await;

    let adapter_id = "test-adapter-evict";
    db.register_adapter(
        adapteros_db::AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id)
            .name("Test Adapter")
            .hash_b3("test_hash")
            .rank(16)
            .tier(2)
            .build()
            .expect("Failed to build adapter params"),
    )
    .await
    .expect("Failed to register adapter");

    // Set initial memory
    db.update_adapter_memory(adapter_id, 1024 * 1024)
        .await
        .expect("Failed to set initial memory");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let adapter_names = vec![adapter_id.to_string()];
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        &test_policies(),
        PathBuf::from(temp_dir.path()),
        None,
        3,
        db.clone(),
    );

    // Promote adapter first
    manager
        .promote_adapter(0)
        .expect("Failed to promote adapter");

    // Evict adapter
    manager
        .evict_adapter(0)
        .await
        .expect("Failed to evict adapter");

    // Poll database until eviction is complete
    let mut attempts = 0;
    loop {
        let adapter = db
            .get_adapter(adapter_id)
            .await
            .expect("Failed to get adapter");
        if let Some(adapter) = &adapter {
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
    let adapter = db
        .get_adapter(adapter_id)
        .await
        .expect("Failed to get adapter");
    assert!(adapter.is_some());
    let adapter = adapter.unwrap();
    assert_eq!(adapter.current_state, "unloaded");
    assert_eq!(adapter.memory_bytes, 0);
}

#[tokio::test]
async fn test_multiple_activations_increment_count() {
    let db = setup_test_db().await;

    let adapter_id = "test-adapter-multi-activation";
    db.register_adapter(
        adapteros_db::AdapterRegistrationBuilder::new()
            .adapter_id(adapter_id)
            .name("Test Adapter")
            .hash_b3("test_hash")
            .rank(16)
            .tier(2)
            .build()
            .expect("Failed to build adapter params"),
    )
    .await
    .expect("Failed to register adapter");

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let adapter_names = vec![adapter_id.to_string()];
    let manager = LifecycleManager::new_with_db(
        adapter_names,
        &test_policies(),
        PathBuf::from(temp_dir.path()),
        None,
        3,
        db.clone(),
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
        let adapter = db
            .get_adapter(adapter_id)
            .await
            .expect("Failed to get adapter");
        if let Some(adapter) = &adapter {
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
    let adapter = db
        .get_adapter(adapter_id)
        .await
        .expect("Failed to get adapter");
    assert!(adapter.is_some());
    assert_eq!(adapter.unwrap().activation_count, 5);
}
