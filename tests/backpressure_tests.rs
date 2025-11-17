//! Backpressure and Memory Pressure Tests (PRD 5)
//!
//! Tests for comprehensive memory tracking across host, GPU, and KV cache dimensions
//! with tiered eviction and backpressure signaling.
//!
//! # Test Coverage
//! - Simulated memory pressure scenarios
//! - KV cache OOM simulation
//! - Tiered eviction policy
//! - Backpressure signaling
//! - GPU memory collection (best-effort)
//!
//! # Citations
//! - PRD 5: Memory & Backpressure (Host, GPU, KV)

use adapteros_memory::{
    BackpressureMonitor, EvictionAction, EvictionPolicy, KvCacheOom, MemorySnapshot, MemoryTier,
};
use std::time::Duration;

#[tokio::test]
async fn test_memory_snapshot_pressure_levels() {
    // Low pressure
    let snap_low = MemorySnapshot::new(
        1 * 1024 * 1024 * 1024,  // 1 GB used
        16 * 1024 * 1024 * 1024, // 16 GB total
        0,
        0,
        0,
    );
    assert!(matches!(
        snap_low.pressure_level(),
        adapteros_memory::backpressure::MemoryPressureLevel::Low
    ));
    assert_eq!(snap_low.host_usage_pct(), 6.25);

    // Medium pressure (70-85%)
    let snap_med = MemorySnapshot::new(
        12 * 1024 * 1024 * 1024, // 12 GB used
        16 * 1024 * 1024 * 1024, // 16 GB total
        0,
        0,
        0,
    );
    assert!(matches!(
        snap_med.pressure_level(),
        adapteros_memory::backpressure::MemoryPressureLevel::Medium
    ));
    assert_eq!(snap_med.host_usage_pct(), 75.0);

    // High pressure (85-95%)
    let snap_high = MemorySnapshot::new(
        14 * 1024 * 1024 * 1024, // 14 GB used
        16 * 1024 * 1024 * 1024, // 16 GB total
        0,
        0,
        0,
    );
    assert!(matches!(
        snap_high.pressure_level(),
        adapteros_memory::backpressure::MemoryPressureLevel::High
    ));
    assert_eq!(snap_high.host_usage_pct(), 87.5);

    // Critical pressure (>= 95%)
    let snap_crit = MemorySnapshot::new(
        15 * 1024 * 1024 * 1024 + 512 * 1024 * 1024, // 15.5 GB used
        16 * 1024 * 1024 * 1024,                     // 16 GB total
        0,
        0,
        0,
    );
    assert!(matches!(
        snap_crit.pressure_level(),
        adapteros_memory::backpressure::MemoryPressureLevel::Critical
    ));
    assert!(snap_crit.host_usage_pct() >= 95.0);
}

#[tokio::test]
async fn test_kv_cache_oom_telemetry() {
    // Simulate KV cache OOM
    let oom = KvCacheOom::new(
        512 * 1024 * 1024,  // Requested 512 MB
        100 * 1024 * 1024,  // Only 100 MB available
        1024 * 1024 * 1024, // 1 GB total capacity
    );

    assert_eq!(oom.requested_bytes, 512 * 1024 * 1024);
    assert_eq!(oom.available_bytes, 100 * 1024 * 1024);
    assert_eq!(oom.total_capacity_bytes, 1024 * 1024 * 1024);
    assert!(oom.timestamp > 0);

    // Emit telemetry (logs to tracing)
    oom.emit_telemetry().await;

    // Convert to error
    let err = oom.to_error();
    assert!(matches!(
        err,
        adapteros_core::AosError::MemoryPressure(_)
    ));
}

#[tokio::test]
async fn test_backpressure_monitor_lifecycle() {
    let policy = EvictionPolicy::default();
    let mut monitor = BackpressureMonitor::new(1, policy);

    // Initially no snapshot
    assert!(monitor.get_snapshot().await.is_none());
    assert!(!monitor.is_backpressure_active().await);

    // Start monitoring with mock snapshot function
    let snapshot_count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let snapshot_count_clone = snapshot_count.clone();

    monitor
        .start(move || {
            snapshot_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            MemorySnapshot::new(
                8 * 1024 * 1024 * 1024,  // 8 GB used
                16 * 1024 * 1024 * 1024, // 16 GB total (50% usage - low pressure)
                0,
                0,
                0,
            )
        })
        .await;

    // Wait for first snapshot
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // Should have snapshot now
    assert!(monitor.get_snapshot().await.is_some());
    assert!(!monitor.is_backpressure_active().await);

    // Verify snapshot was collected
    assert!(snapshot_count.load(std::sync::atomic::Ordering::SeqCst) > 0);

    // Stop monitor
    monitor.stop().await;
}

#[tokio::test]
async fn test_tiered_eviction_actions() {
    let policy = EvictionPolicy::default();
    let mut monitor = BackpressureMonitor::new(1, policy);

    // Test Critical pressure → BlockNewRequests
    let snap_critical = MemorySnapshot::new(
        15 * 1024 * 1024 * 1024 + 512 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
        0,
        0,
        0,
    );
    *monitor.snapshot.clone().write().await = Some(snap_critical);

    let action = monitor.get_eviction_action().await;
    assert!(matches!(action, Some(EvictionAction::BlockNewRequests)));
    assert!(monitor.is_backpressure_active().await);

    // Test High pressure → UnloadIdleAdapters
    let snap_high = MemorySnapshot::new(
        14 * 1024 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
        0,
        0,
        0,
    );
    *monitor.snapshot.clone().write().await = Some(snap_high);

    let action = monitor.get_eviction_action().await;
    assert!(matches!(
        action,
        Some(EvictionAction::UnloadIdleAdapters { .. })
    ));
    assert!(monitor.is_backpressure_active().await);

    // Test Medium pressure → DropCache
    let snap_medium = MemorySnapshot::new(
        12 * 1024 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
        0,
        0,
        0,
    );
    *monitor.snapshot.clone().write().await = Some(snap_medium);

    let action = monitor.get_eviction_action().await;
    assert!(matches!(action, Some(EvictionAction::DropCache { .. })));
    assert!(!monitor.is_backpressure_active().await); // Medium is not backpressure

    // Test Low pressure → No action
    let snap_low = MemorySnapshot::new(
        2 * 1024 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
        0,
        0,
        0,
    );
    *monitor.snapshot.clone().write().await = Some(snap_low);

    let action = monitor.get_eviction_action().await;
    assert!(action.is_none());
    assert!(!monitor.is_backpressure_active().await);
}

#[tokio::test]
async fn test_gpu_memory_unavailable_graceful_degradation() {
    // Simulate GPU metrics unavailable (gpu_total_bytes = 0)
    let snap = MemorySnapshot::new(
        8 * 1024 * 1024 * 1024,
        16 * 1024 * 1024 * 1024,
        0, // GPU unavailable
        0,
        512 * 1024 * 1024, // KV cache still tracked
    );

    // Should still determine pressure based on host memory
    assert!(matches!(
        snap.pressure_level(),
        adapteros_memory::backpressure::MemoryPressureLevel::Medium
    ));

    // GPU usage should return 0.0
    assert_eq!(snap.gpu_usage_pct(), 0.0);
}

#[tokio::test]
async fn test_memory_tier_eviction_priority() {
    // Test that memory tiers have correct eviction priority
    use MemoryTier::*;

    // Cache should be evicted first
    assert_eq!(Cache as u8, 2);

    // Extra should be evicted next
    assert_eq!(Extra as u8, 1);

    // Critical should be evicted last
    assert_eq!(Critical as u8, 0);
}

#[tokio::test]
async fn test_snapshot_collection_interval() {
    let policy = EvictionPolicy::default();
    let mut monitor = BackpressureMonitor::new(1, policy); // 1 second interval

    let snapshot_count = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let snapshot_count_clone = snapshot_count.clone();

    monitor
        .start(move || {
            snapshot_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            MemorySnapshot::new(
                8 * 1024 * 1024 * 1024,
                16 * 1024 * 1024 * 1024,
                0,
                0,
                0,
            )
        })
        .await;

    // Wait for ~3 snapshots (3.5 seconds to account for timing jitter)
    tokio::time::sleep(Duration::from_millis(3500)).await;

    let count = snapshot_count.load(std::sync::atomic::Ordering::SeqCst);
    assert!(
        count >= 3 && count <= 4,
        "Expected 3-4 snapshots, got {}",
        count
    );

    monitor.stop().await;
}

#[tokio::test]
async fn test_eviction_policy_thresholds() {
    let policy = EvictionPolicy {
        warning_threshold: 80.0,
        critical_threshold: 90.0,
    };

    assert_eq!(policy.warning_threshold, 80.0);
    assert_eq!(policy.critical_threshold, 90.0);

    // Test default policy
    let default_policy = EvictionPolicy::default();
    assert_eq!(default_policy.warning_threshold, 85.0);
    assert_eq!(default_policy.critical_threshold, 95.0);
}

#[tokio::test]
async fn test_kv_cache_oom_in_worker() {
    // Integration test: simulate KV cache allocation failure
    use adapteros_lora_worker::kvcache::KvCache;

    let mut cache = KvCache::new(1024 * 1024); // 1 MB capacity (very small)

    // This should succeed
    let seq1 = cache.allocate(10).expect("First allocation should succeed");
    assert!(cache.is_allocated(seq1));

    // This should fail with OOM (requesting too much)
    let result = cache.allocate(10000); // Requesting huge sequence
    assert!(result.is_err());

    // Verify error message contains "OOM"
    if let Err(e) = result {
        let err_msg = e.to_string();
        assert!(err_msg.contains("OOM") || err_msg.contains("cache full"));
    }
}

#[test]
fn test_memory_snapshot_with_all_dimensions() {
    // Test snapshot with all memory dimensions tracked
    let snap = MemorySnapshot::new(
        8 * 1024 * 1024 * 1024,  // 8 GB host used
        16 * 1024 * 1024 * 1024, // 16 GB host total
        4 * 1024 * 1024 * 1024,  // 4 GB GPU used
        8 * 1024 * 1024 * 1024,  // 8 GB GPU total
        512 * 1024 * 1024,       // 512 MB KV cache
    );

    assert_eq!(snap.host_used_bytes, 8 * 1024 * 1024 * 1024);
    assert_eq!(snap.host_total_bytes, 16 * 1024 * 1024 * 1024);
    assert_eq!(snap.gpu_used_bytes, 4 * 1024 * 1024 * 1024);
    assert_eq!(snap.gpu_total_bytes, 8 * 1024 * 1024 * 1024);
    assert_eq!(snap.kv_used_bytes, 512 * 1024 * 1024);

    assert_eq!(snap.host_usage_pct(), 50.0);
    assert_eq!(snap.gpu_usage_pct(), 50.0);
    assert!(snap.ts_us > 0);
}
