//! Tests for memory pressure handling and eviction

use mplora_lifecycle::LifecycleManager;
use mplora_manifest::Policies;
use mplora_profiler::AdapterProfiler;

#[test]
fn test_memory_pressure_eviction() {
    let temp_dir = std::env::temp_dir().join("mplora_test_memory_pressure");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];

    let mut policies = Policies::default();
    policies.memory.min_headroom_pct = 15;
    policies.adapters.min_activation_pct = 2.0;

    let manager = LifecycleManager::new(
        adapter_names.clone(),
        &policies,
        temp_dir.clone(),
        None,
        3,
    );

    let profiler = AdapterProfiler::new(adapter_names, None);

    // Promote all adapters to cold (loaded but not active)
    manager.promote_adapter(0).unwrap();
    manager.promote_adapter(1).unwrap();
    manager.promote_adapter(2).unwrap();

    // Simulate low activation for adapter 0 (candidate for eviction)
    profiler.record_routing_decision(&[1, 2]); // Skip adapter 0
    profiler.record_routing_decision(&[1, 2]);
    profiler.record_routing_decision(&[1, 2]);
    
    // High activation for adapter 1 and 2
    for _ in 0..50 {
        profiler.record_routing_decision(&[1, 2]);
    }

    // Trigger memory pressure handling
    let result = manager.handle_memory_pressure(&profiler);
    
    // Should succeed (evict one adapter)
    assert!(result.is_ok());

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn test_pinned_adapter_never_evicted() {
    let temp_dir = std::env::temp_dir().join("mplora_test_pinned_no_evict");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
    ];

    let policies = Policies::default();
    let manager = LifecycleManager::new(
        adapter_names.clone(),
        &policies,
        temp_dir.clone(),
        None,
        3,
    );

    let profiler = AdapterProfiler::new(adapter_names, None);

    // Pin adapter 0 (should never be evicted)
    manager.pin_adapter(0).unwrap();
    
    // Promote adapter 1 to cold
    manager.promote_adapter(1).unwrap();

    // Low activation for both (only adapter 1 should be evictable)
    profiler.record_routing_decision(&[]);
    profiler.record_routing_decision(&[]);

    // Trigger memory pressure
    let result = manager.handle_memory_pressure(&profiler);
    assert!(result.is_ok());

    // Adapter 0 should still be resident (pinned)
    use mplora_lifecycle::AdapterState;
    assert_eq!(manager.get_state(0), Some(AdapterState::Resident));

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn test_k_reduction_under_extreme_pressure() {
    let temp_dir = std::env::temp_dir().join("mplora_test_k_reduction");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec!["adapter_0".to_string()];
    let policies = Policies::default();
    
    let manager = LifecycleManager::new(
        adapter_names.clone(),
        &policies,
        temp_dir.clone(),
        None,
        3, // Start with K=3
    );

    let profiler = AdapterProfiler::new(adapter_names, None);

    // Initial K should be 3
    assert_eq!(manager.current_k(), 3);

    // Pin the only adapter (so it can't be evicted)
    manager.pin_adapter(0).unwrap();

    // Trigger memory pressure - should reduce K since adapter can't be evicted
    let result = manager.handle_memory_pressure(&profiler);
    assert!(result.is_ok());

    // K should be reduced to 2
    assert_eq!(manager.current_k(), 2);

    // Trigger again
    let result = manager.handle_memory_pressure(&profiler);
    assert!(result.is_ok());
    assert_eq!(manager.current_k(), 1);

    // Cannot reduce below 1
    let result = manager.handle_memory_pressure(&profiler);
    assert!(result.is_err()); // Should fail

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

