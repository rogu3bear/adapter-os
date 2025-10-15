//! Tests for K-reduction policy under memory pressure

use adapteros_lora_lifecycle::LifecycleManager;
use adapteros_manifest::Policies;
use adapteros_profiler::AdapterProfiler;

#[test]
fn test_k_reduction_before_hot_eviction() {
    let temp_dir = std::env::temp_dir().join("mplora_test_k_before_evict");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec![
        "adapter_0".to_string(),
        "adapter_1".to_string(),
        "adapter_2".to_string(),
    ];

    let policies = Policies::default();
    let manager = LifecycleManager::new(
        adapter_names.clone(),
        &policies,
        temp_dir.clone(),
        None,
        3, // K=3
    );

    let profiler = AdapterProfiler::new(adapter_names, None);

    // Promote all adapters to hot
    manager.promote_adapter(0).unwrap(); // cold
    manager.promote_adapter(0).unwrap(); // warm
    manager.promote_adapter(0).unwrap(); // hot
    
    manager.promote_adapter(1).unwrap();
    manager.promote_adapter(1).unwrap();
    manager.promote_adapter(1).unwrap();
    
    manager.promote_adapter(2).unwrap();
    manager.promote_adapter(2).unwrap();
    manager.promote_adapter(2).unwrap();

    // High activation for all
    for _ in 0..100 {
        profiler.record_routing_decision(&[0, 1, 2]);
    }

    // All should be hot
    use adapteros_lora_lifecycle::AdapterState;
    assert_eq!(manager.get_state(0), Some(AdapterState::Hot));
    assert_eq!(manager.get_state(1), Some(AdapterState::Hot));
    assert_eq!(manager.get_state(2), Some(AdapterState::Hot));

    // Trigger memory pressure - should reduce K rather than evict hot adapters
    let result = manager.handle_memory_pressure(&profiler);
    assert!(result.is_ok());

    // K should be reduced
    assert!(manager.current_k() < 3);

    // All adapters should still be hot (not evicted)
    assert_eq!(manager.get_state(0), Some(AdapterState::Hot));
    assert_eq!(manager.get_state(1), Some(AdapterState::Hot));
    assert_eq!(manager.get_state(2), Some(AdapterState::Hot));

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn test_k_gradual_reduction() {
    let temp_dir = std::env::temp_dir().join("mplora_test_k_gradual");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec!["adapter_0".to_string()];
    let policies = Policies::default();
    
    let manager = LifecycleManager::new(
        adapter_names.clone(),
        &policies,
        temp_dir.clone(),
        None,
        5, // Start with K=5
    );

    let profiler = AdapterProfiler::new(adapter_names, None);

    // Pin adapter to prevent eviction
    manager.pin_adapter(0).unwrap();

    // Gradually reduce K
    assert_eq!(manager.current_k(), 5);

    manager.handle_memory_pressure(&profiler).unwrap();
    assert_eq!(manager.current_k(), 4);

    manager.handle_memory_pressure(&profiler).unwrap();
    assert_eq!(manager.current_k(), 3);

    manager.handle_memory_pressure(&profiler).unwrap();
    assert_eq!(manager.current_k(), 2);

    manager.handle_memory_pressure(&profiler).unwrap();
    assert_eq!(manager.current_k(), 1);

    // Cannot go below 1
    let result = manager.handle_memory_pressure(&profiler);
    assert!(result.is_err());

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn test_eviction_order_respects_policy() {
    let temp_dir = std::env::temp_dir().join("mplora_test_eviction_order");
    std::fs::create_dir_all(&temp_dir).unwrap();

    let adapter_names = vec![
        "cold_adapter".to_string(),
        "warm_adapter".to_string(),
        "hot_adapter".to_string(),
    ];

    let mut policies = Policies::default();
    policies.memory.evict_order = vec![
        "cold_lru".to_string(),
        "warm_lru".to_string(),
    ];

    let manager = LifecycleManager::new(
        adapter_names.clone(),
        &policies,
        temp_dir.clone(),
        None,
        3,
    );

    let profiler = AdapterProfiler::new(adapter_names, None);

    // Set up different states
    manager.promote_adapter(0).unwrap(); // cold
    
    manager.promote_adapter(1).unwrap(); // cold
    manager.promote_adapter(1).unwrap(); // warm
    
    manager.promote_adapter(2).unwrap(); // cold
    manager.promote_adapter(2).unwrap(); // warm
    manager.promote_adapter(2).unwrap(); // hot

    // Very low activation for cold adapter
    for _ in 0..100 {
        profiler.record_routing_decision(&[1, 2]); // Skip adapter 0
    }

    // Trigger memory pressure - cold adapter should be evicted first
    let result = manager.handle_memory_pressure(&profiler);
    assert!(result.is_ok());

    // Cold adapter should be evicted (back to unloaded)
    use adapteros_lora_lifecycle::AdapterState;
    assert_eq!(manager.get_state(0), Some(AdapterState::Unloaded));
    
    // Other adapters should remain
    assert!(manager.get_state(1).unwrap().is_loaded());
    assert!(manager.get_state(2).unwrap().is_loaded());

    // Cleanup
    std::fs::remove_dir_all(temp_dir).unwrap();
}

