//! MLX Memory Management Integration Tests

use adapteros_lora_mlx_ffi::memory_management::{self, MLXMemoryManager};

#[test]
fn test_memory_management_allocates_and_tracks_usage() {
    let manager = MLXMemoryManager::new();

    let usage = manager
        .memory_usage()
        .expect("memory_usage should return a valid usage value");

    let allocation_count = manager
        .allocation_count()
        .expect("allocation_count should return a valid allocation count");
    let total_mb = manager.memory_stats().expect("memory_stats should succeed");

    assert_eq!(total_mb.allocation_count, allocation_count);
    assert_eq!(total_mb.total_bytes as u64, usage as u64);
}

#[test]
fn test_memory_management_gc_and_reset_are_tracking_safe() {
    let manager = MLXMemoryManager::new();

    assert_eq!(manager.tracker().peak_memory(), 0);
    assert_eq!(manager.tracker().collection_count(), 0);

    manager
        .gc_collect()
        .expect("gc_collect should not fail in deterministic path");
    assert_eq!(manager.tracker().collection_count(), 1);

    manager
        .reset()
        .expect("reset should clear tracker counters");
    assert_eq!(manager.tracker().peak_memory(), 0);
    assert_eq!(manager.tracker().collection_count(), 0);
}

#[test]
fn test_memory_management_thresholding_and_sync() {
    let manager = MLXMemoryManager::new();
    manager
        .reset()
        .expect("reset should initialize memory tracking");

    let _stats = manager
        .memory_stats()
        .expect("memory_stats should be queryable after reset");

    let gc_triggered = manager
        .check_and_gc(0.0)
        .expect("check_and_gc should remain infallible for deterministic threshold checks");
    if gc_triggered {
        assert_eq!(manager.tracker().collection_count(), 1);
    }

    manager
        .synchronize()
        .expect("synchronize should return successfully");
}

#[test]
fn test_memory_management_integration_recommendation_logic() {
    let low_pressure = memory_management::MemoryManagementStats {
        total_bytes: 2 * 1024 * 1024,
        allocation_count: 3,
        peak_bytes: 2 * 1024 * 1024,
    };
    let moderate_recommendation =
        memory_management::integration::analyze_memory_pressure(&low_pressure, 10);
    assert!(!moderate_recommendation.requires_immediate_action());

    let high_pressure = memory_management::MemoryManagementStats {
        total_bytes: 9 * 1024 * 1024,
        allocation_count: 3,
        peak_bytes: 9 * 1024 * 1024,
    };
    let critical_recommendation =
        memory_management::integration::analyze_memory_pressure(&high_pressure, 10);
    assert!(critical_recommendation.requires_immediate_action());
}
