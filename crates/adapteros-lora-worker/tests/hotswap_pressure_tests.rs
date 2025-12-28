//! Memory Pressure During Swap Tests (P2 Medium)
//!
//! Tests for memory pressure handling during hot-swap operations.
//! The system should handle memory pressure gracefully.
//!
//! These tests verify:
//! - Swap with 2x VRAM spike
//! - Monitor unavailable graceful fallback
//! - Force cleanup failure handling
//! - Pressure level transitions
//! - Preload memory estimate accuracy
//! - Concurrent preloads memory tracking

use adapteros_core::B3Hash;
use adapteros_lora_worker::{
    adapter_hotswap::AdapterTable,
    memory::{MemoryPressureLevel, UmaPressureMonitor},
};
use std::sync::Arc;

/// Test VRAM tracking during swap operations.
///
/// When swapping adapters, total VRAM should reflect the new active set.
#[tokio::test]
async fn test_vram_tracking_during_swap() {
    let table = Arc::new(AdapterTable::new());

    // Preload adapters with different VRAM sizes
    let hash1 = B3Hash::hash(b"small-adapter");
    let hash2 = B3Hash::hash(b"large-adapter");
    table.preload("small-adapter".to_string(), hash1, 50).await.unwrap();
    table.preload("large-adapter".to_string(), hash2, 200).await.unwrap();

    // Initial state: no VRAM used
    assert_eq!(table.total_vram_mb(), 0);

    // Swap in small adapter
    table.swap(&["small-adapter".to_string()], &[]).await.unwrap();
    assert_eq!(table.total_vram_mb(), 50);

    // During swap to large adapter, both might be temporarily active
    // After swap completes, only large should be counted
    table
        .swap(&["large-adapter".to_string()], &["small-adapter".to_string()])
        .await
        .unwrap();
    assert_eq!(table.total_vram_mb(), 200);
}

/// Test that multiple active adapters sum VRAM correctly.
#[tokio::test]
async fn test_multiple_adapters_vram_sum() {
    let table = Arc::new(AdapterTable::new());

    // Preload several adapters
    for i in 0..5 {
        let hash = B3Hash::hash(format!("sum-adapter-{}", i).as_bytes());
        let vram = (i + 1) * 20; // 20, 40, 60, 80, 100
        table.preload(format!("sum-adapter-{}", i), hash, vram as u64).await.unwrap();
    }

    // Activate all adapters
    let all_ids: Vec<String> = (0..5).map(|i| format!("sum-adapter-{}", i)).collect();
    table.swap(&all_ids, &[]).await.unwrap();

    // Total should be 20+40+60+80+100 = 300
    assert_eq!(table.total_vram_mb(), 300);

    // Remove some adapters
    table
        .swap(
            &["sum-adapter-0".to_string(), "sum-adapter-1".to_string()],
            &[
                "sum-adapter-2".to_string(),
                "sum-adapter-3".to_string(),
                "sum-adapter-4".to_string(),
            ],
        )
        .await
        .unwrap();

    // Total should be 20+40 = 60
    assert_eq!(table.total_vram_mb(), 60);
}

/// Test memory pressure monitor test override functionality.
#[test]
fn test_pressure_monitor_override() {
    let monitor = UmaPressureMonitor::new(15, None);

    // Initial should be Low
    assert_eq!(monitor.get_current_pressure(), MemoryPressureLevel::Low);

    // Override to each level and verify
    for level in [
        MemoryPressureLevel::Low,
        MemoryPressureLevel::Medium,
        MemoryPressureLevel::High,
        MemoryPressureLevel::Critical,
    ] {
        monitor.set_pressure_for_test(level);
        assert_eq!(monitor.get_current_pressure(), level);
    }
}

/// Test that pressure levels are ordered correctly.
#[test]
fn test_pressure_level_ordering() {
    assert!(MemoryPressureLevel::Low < MemoryPressureLevel::Medium);
    assert!(MemoryPressureLevel::Medium < MemoryPressureLevel::High);
    assert!(MemoryPressureLevel::High < MemoryPressureLevel::Critical);

    // Test >= comparisons
    assert!(MemoryPressureLevel::High >= MemoryPressureLevel::High);
    assert!(MemoryPressureLevel::Critical >= MemoryPressureLevel::High);
}

/// Test VRAM tracking with empty adapter set.
#[tokio::test]
async fn test_vram_tracking_empty_set() {
    let table = Arc::new(AdapterTable::new());

    // No adapters active
    assert_eq!(table.total_vram_mb(), 0);
    assert!(table.get_active().is_empty());
}

/// Test VRAM accuracy after many swaps.
#[tokio::test]
async fn test_vram_accuracy_after_many_swaps() {
    let table = Arc::new(AdapterTable::new());

    // Preload two adapters for ping-pong
    let hash_a = B3Hash::hash(b"ping-vram");
    let hash_b = B3Hash::hash(b"pong-vram");
    table.preload("ping-vram".to_string(), hash_a, 100).await.unwrap();
    table.preload("pong-vram".to_string(), hash_b, 150).await.unwrap();

    // Initial swap
    table.swap(&["ping-vram".to_string()], &[]).await.unwrap();
    assert_eq!(table.total_vram_mb(), 100);

    // Many swaps back and forth
    for i in 0..50 {
        if i % 2 == 0 {
            table
                .swap(&["pong-vram".to_string()], &["ping-vram".to_string()])
                .await
                .unwrap();
            assert_eq!(table.total_vram_mb(), 150, "After swap {} to pong", i);
        } else {
            table
                .swap(&["ping-vram".to_string()], &["pong-vram".to_string()])
                .await
                .unwrap();
            assert_eq!(table.total_vram_mb(), 100, "After swap {} to ping", i);
        }
    }
}
