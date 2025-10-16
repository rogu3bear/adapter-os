//! Tests for adapter lifecycle state transitions

use adapteros_lora_lifecycle::{AdapterState, AdapterStateRecord, LifecycleManager};
use adapteros_manifest::Policies;
use std::path::PathBuf;

#[test]
fn test_state_machine_transitions() {
    let mut record = AdapterStateRecord::new("test_adapter".to_string(), 0);

    // Start at unloaded
    assert_eq!(record.state, AdapterState::Unloaded);

    // Promote through states
    assert!(record.promote());
    assert_eq!(record.state, AdapterState::Cold);

    assert!(record.promote());
    assert_eq!(record.state, AdapterState::Warm);

    assert!(record.promote());
    assert_eq!(record.state, AdapterState::Hot);

    assert!(record.promote());
    assert_eq!(record.state, AdapterState::Resident);

    // Can't promote beyond resident
    assert!(!record.promote());
    assert_eq!(record.state, AdapterState::Resident);

    // Demote back down
    assert!(record.demote());
    assert_eq!(record.state, AdapterState::Hot);

    assert!(record.demote());
    assert_eq!(record.state, AdapterState::Warm);

    assert!(record.demote());
    assert_eq!(record.state, AdapterState::Cold);

    assert!(record.demote());
    assert_eq!(record.state, AdapterState::Unloaded);

    // Can't demote beyond unloaded
    assert!(!record.demote());
    assert_eq!(record.state, AdapterState::Unloaded);
}

#[test]
fn test_pinned_adapter_cannot_demote() {
    let mut record = AdapterStateRecord::new("test_adapter".to_string(), 0);

    // Pin to resident
    record.pin();
    assert_eq!(record.state, AdapterState::Resident);
    assert!(record.pinned);

    // Cannot demote pinned adapter
    assert!(!record.demote());
    assert_eq!(record.state, AdapterState::Resident);

    // Unpin allows demotion
    record.unpin();
    assert!(!record.pinned);

    assert!(record.demote());
    assert_eq!(record.state, AdapterState::Hot);
}

#[test]
fn test_lifecycle_manager_basic() {
    let temp_dir = std::env::temp_dir().join("mplora_test_lifecycle_basic");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];

    let policies = Policies::default();
    let manager = LifecycleManager::new(adapter_names, &policies, temp_dir.clone(), None, 3);

    // Check initial state
    assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));
    assert_eq!(manager.get_state(1), Some(AdapterState::Unloaded));
    assert_eq!(manager.get_state(2), Some(AdapterState::Unloaded));

    // Promote adapter 0
    manager.promote_adapter(0).unwrap();
    assert_eq!(manager.get_state(0), Some(AdapterState::Cold));

    // Demote adapter 0
    manager.demote_adapter(0).unwrap();
    assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));

    // Pin adapter 1
    manager.pin_adapter(1).unwrap();
    assert_eq!(manager.get_state(1), Some(AdapterState::Resident));

    // Cannot demote pinned adapter
    assert!(manager.demote_adapter(1).is_err());
    assert_eq!(manager.get_state(1), Some(AdapterState::Resident));

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn test_available_adapters_filtering() {
    let temp_dir = std::env::temp_dir().join("mplora_test_available");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];

    let policies = Policies::default();
    let manager = LifecycleManager::new(adapter_names, &policies, temp_dir.clone(), None, 3);

    // Initially, no adapters are available (all unloaded)
    let available = manager.get_available_adapters();
    assert_eq!(available.len(), 0);

    // Promote adapter 0 to warm (available)
    manager.promote_adapter(0).unwrap(); // -> cold
    manager.promote_adapter(0).unwrap(); // -> warm

    let available = manager.get_available_adapters();
    assert_eq!(available.len(), 1);
    assert!(available.contains(&0));

    // Promote adapter 1 to hot (available)
    manager.promote_adapter(1).unwrap(); // -> cold
    manager.promote_adapter(1).unwrap(); // -> warm
    manager.promote_adapter(1).unwrap(); // -> hot

    let available = manager.get_available_adapters();
    assert_eq!(available.len(), 2);
    assert!(available.contains(&0));
    assert!(available.contains(&1));

    // Adapter 2 stays cold (not available)
    manager.promote_adapter(2).unwrap(); // -> cold
    let available = manager.get_available_adapters();
    assert_eq!(available.len(), 2); // Still only 0 and 1

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn test_state_priority_boosts() {
    let temp_dir = std::env::temp_dir().join("mplora_test_boosts");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];

    let policies = Policies::default();
    let manager = LifecycleManager::new(adapter_names, &policies, temp_dir.clone(), None, 3);

    // Set different states
    manager.promote_adapter(0).unwrap(); // cold
    manager.promote_adapter(0).unwrap(); // warm

    manager.promote_adapter(1).unwrap(); // cold
    manager.promote_adapter(1).unwrap(); // warm
    manager.promote_adapter(1).unwrap(); // hot

    manager.pin_adapter(2).unwrap(); // resident

    // Get priority boosts
    let boosts = manager.get_priority_boosts();

    // Resident should have highest boost
    assert!(boosts[&2] > boosts[&1]);
    assert!(boosts[&1] > boosts[&0]);

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}
