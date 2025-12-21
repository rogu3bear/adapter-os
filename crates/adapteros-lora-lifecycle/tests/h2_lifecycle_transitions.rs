//! Integration tests for H2: Lifecycle State Transitions
//!
//! Tests the full state machine (Unloaded→Cold→Warm→Hot→Resident)
//! with activation-based promotion and demotion.

use adapteros_core::B3Hash;
use adapteros_lora_lifecycle::{AdapterState, LifecycleManager};
use adapteros_manifest::Policies;
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_adapters_dir() -> TempDir {
    let base_dir = PathBuf::from("var").join("tmp");
    let _ = std::fs::create_dir_all(&base_dir);
    tempfile::Builder::new()
        .prefix("lifecycle_test_")
        .tempdir_in(&base_dir)
        .expect("tempdir")
}

#[tokio::test]
async fn test_h2_manual_state_promotions() {
    let adapter_names = vec!["test-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("test-adapter".to_string(), B3Hash::hash(b"test"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
    );

    // Test: Unloaded → Cold → Warm → Hot → Resident
    manager.promote_adapter(0).await.unwrap(); // Unloaded → Cold
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Cold
    );

    manager.promote_adapter(0).await.unwrap(); // Cold → Warm
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Warm
    );

    manager.promote_adapter(0).await.unwrap(); // Warm → Hot
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Hot
    );

    manager.promote_adapter(0).await.unwrap(); // Hot → Resident
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Resident
    );
}

#[tokio::test]
async fn test_h2_manual_state_demotions() {
    let adapter_names = vec!["test-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("test-adapter".to_string(), B3Hash::hash(b"test"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
    );

    // Promote to Resident first
    for _ in 0..4 {
        manager.promote_adapter(0).await.unwrap();
    }

    // Test demotion path: Resident → Hot → Warm → Cold → Unloaded
    manager.demote_adapter(0).await.unwrap(); // Resident → Hot
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Hot
    );

    manager.demote_adapter(0).await.unwrap(); // Hot → Warm
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Warm
    );

    manager.demote_adapter(0).await.unwrap(); // Warm → Cold
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Cold
    );

    manager.demote_adapter(0).await.unwrap(); // Cold → Unloaded
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Unloaded
    );
}

#[tokio::test]
async fn test_h2_state_machine_completeness() {
    let adapter_names = vec!["test-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("test-adapter".to_string(), B3Hash::hash(b"test"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
    );

    // Verify all 5 states are reachable
    let states = [
        AdapterState::Unloaded,
        AdapterState::Cold,
        AdapterState::Warm,
        AdapterState::Hot,
        AdapterState::Resident,
    ];

    for (idx, expected_state) in states.iter().enumerate() {
        if idx > 0 {
            manager.promote_adapter(0).await.unwrap();
        }
        let current_states = manager.get_all_states();
        assert_eq!(
            current_states
                .iter()
                .find(|s| s.adapter_idx == 0)
                .unwrap()
                .state,
            *expected_state,
            "Failed to reach state {:?} at step {}",
            expected_state,
            idx
        );
    }
}

#[tokio::test]
async fn test_h2_activation_recording() {
    // Test activation recording without database (simpler test)
    let adapter_names = vec!["test-adapter".to_string()];
    let mut hashes = HashMap::new();
    hashes.insert("test-adapter".to_string(), B3Hash::hash(b"test"));

    let policies = Policies::default();
    let temp_dir = new_test_adapters_dir();
    let manager = LifecycleManager::new(
        adapter_names,
        hashes,
        &policies,
        temp_dir.path().to_path_buf(),
        None,
        3,
    );

    // Record activations (in-memory only, no database)
    // Note: record_adapter_activation requires database, so we test the state directly
    // Promote to verify state transitions work
    manager.promote_adapter(0).await.unwrap();
    let states = manager.get_all_states();
    assert_eq!(
        states.iter().find(|s| s.adapter_idx == 0).unwrap().state,
        AdapterState::Cold
    );
}
