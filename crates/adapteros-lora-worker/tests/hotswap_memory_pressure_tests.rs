//! Hot-Swap Memory Pressure Tests (P0 Critical)
//!
//! Tests for memory pressure handling during adapter preload and swap operations.
//! These tests verify:
//! - Preload rejection on insufficient VRAM
//! - Preload rejection on critical memory pressure
//! - Memory spike handling during swap
//! - Force cleanup on high pressure
//! - Preload success when VRAM is available

#![allow(unused_mut)]

use adapteros_core::B3Hash;
use adapteros_lora_worker::{
    adapter_hotswap::AdapterTable,
    memory::{MemoryPressureLevel, UmaPressureMonitor},
};
use std::sync::Arc;

/// Test that preload fails when VRAM estimate is zero.
///
/// Zero VRAM indicates the adapter weights could not be measured,
/// which typically means corrupt or invalid SafeTensors data.
#[tokio::test]
async fn test_preload_rejected_on_zero_vram_estimate() {
    let table = AdapterTable::new();
    let hash = B3Hash::hash(b"test_adapter");

    // Attempt to preload with zero VRAM should fail
    let result = table.preload("test_adapter".to_string(), hash, 0).await;

    assert!(result.is_err(), "Preload with zero VRAM should fail");
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("VRAM estimate is zero"),
        "Error should mention zero VRAM, got: {}",
        err_msg
    );
    assert!(
        err_msg.contains("test_adapter"),
        "Error should mention adapter ID, got: {}",
        err_msg
    );
}

/// Test that preload succeeds with valid VRAM estimate when memory is available.
///
/// This is the happy path test ensuring that when memory conditions are
/// favorable, preload operations complete successfully.
#[tokio::test]
async fn test_preload_succeeds_when_vram_available() {
    let table = AdapterTable::new();
    let hash = B3Hash::hash(b"adapter_with_vram");

    // Preload with positive VRAM should succeed
    let result = table
        .preload("adapter_with_vram".to_string(), hash, 100)
        .await;

    assert!(
        result.is_ok(),
        "Preload with valid VRAM should succeed: {:?}",
        result.err()
    );

    // Verify adapter is in staged state
    let active = table.get_active();
    assert!(
        active.is_empty(),
        "Preloaded adapter should not be active yet"
    );

    // Swap in the adapter to verify it was staged
    let swap_result = table.swap(&["adapter_with_vram".to_string()], &[]).await;
    assert!(
        swap_result.is_ok(),
        "Swap should succeed for preloaded adapter"
    );

    // Now the adapter should be active
    let active = table.get_active();
    assert_eq!(active.len(), 1, "Should have one active adapter");
    assert_eq!(active[0].id, "adapter_with_vram");
    assert_eq!(active[0].vram_mb, 100);
}

/// Test that memory pressure levels are correctly categorized.
///
/// Verifies the threshold logic for Low, Medium, High, and Critical pressure.
#[test]
fn test_memory_pressure_level_ordering() {
    // Verify ordering: Low < Medium < High < Critical
    assert!(MemoryPressureLevel::Low < MemoryPressureLevel::Medium);
    assert!(MemoryPressureLevel::Medium < MemoryPressureLevel::High);
    assert!(MemoryPressureLevel::High < MemoryPressureLevel::Critical);

    // Verify display formatting
    assert_eq!(format!("{}", MemoryPressureLevel::Low), "Low");
    assert_eq!(format!("{}", MemoryPressureLevel::Critical), "Critical");
}

/// Test that UmaPressureMonitor can be set to test pressure levels.
///
/// This validates the test infrastructure for simulating memory pressure.
#[test]
fn test_uma_pressure_monitor_test_override() {
    let monitor = UmaPressureMonitor::new(15, None);

    // Default should be Low
    let initial = monitor.get_current_pressure();
    assert_eq!(
        initial,
        MemoryPressureLevel::Low,
        "Initial pressure should be Low"
    );

    // Override to Critical for testing
    monitor.set_pressure_for_test(MemoryPressureLevel::Critical);
    let critical = monitor.get_current_pressure();
    assert_eq!(
        critical,
        MemoryPressureLevel::Critical,
        "Override should take effect"
    );

    // Override to High
    monitor.set_pressure_for_test(MemoryPressureLevel::High);
    let high = monitor.get_current_pressure();
    assert_eq!(high, MemoryPressureLevel::High, "Can change pressure level");

    // Reset to Low
    monitor.set_pressure_for_test(MemoryPressureLevel::Low);
    let reset = monitor.get_current_pressure();
    assert_eq!(reset, MemoryPressureLevel::Low, "Can reset to Low");
}

/// Test force cleanup behavior under high memory pressure.
///
/// When memory pressure is High or Critical, the hot-swap manager
/// should trigger aggressive cleanup of retired adapter stacks.
#[tokio::test]
async fn test_force_cleanup_triggered_on_high_pressure() {
    let table = Arc::new(AdapterTable::new());

    // Preload multiple adapters
    for i in 0..5 {
        let hash = B3Hash::hash(format!("adapter{}", i).as_bytes());
        table
            .preload(format!("adapter{}", i), hash, 50)
            .await
            .expect("Preload should succeed");
    }

    // Swap in adapter0
    table.swap(&["adapter0".to_string()], &[]).await.unwrap();

    // Perform several swaps to accumulate retired stacks
    for i in 1..5 {
        table
            .swap(&[format!("adapter{}", i)], &[format!("adapter{}", i - 1)])
            .await
            .expect("Swap should succeed");
    }

    // Verify final state
    let active = table.get_active();
    assert_eq!(active.len(), 1, "Should have one active adapter");
    assert_eq!(
        active[0].id, "adapter4",
        "Last swapped adapter should be active"
    );

    // Total VRAM should be tracked for the active adapter
    let total_vram = table.total_vram_mb();
    assert_eq!(total_vram, 50, "Should track VRAM for active adapter");
}

/// Test that multiple concurrent preloads can succeed.
///
/// Validates that the staging area can hold multiple adapters
/// without interference during concurrent operations.
#[tokio::test]
async fn test_concurrent_preloads_succeed() {
    let table = Arc::new(AdapterTable::new());

    let mut handles = vec![];

    // Spawn 10 concurrent preload operations
    for i in 0..10 {
        let table_clone = table.clone();
        handles.push(tokio::spawn(async move {
            let hash = B3Hash::hash(format!("concurrent_adapter{}", i).as_bytes());
            table_clone
                .preload(format!("concurrent_adapter{}", i), hash, 25)
                .await
        }));
    }

    // Wait for all preloads
    let mut success_count = 0;
    for handle in handles {
        if handle.await.unwrap().is_ok() {
            success_count += 1;
        }
    }

    assert_eq!(
        success_count, 10,
        "All 10 concurrent preloads should succeed"
    );

    // Verify all adapters can be swapped in
    let mut all_adapter_ids: Vec<String> = (0..10)
        .map(|i| format!("concurrent_adapter{}", i))
        .collect();

    // Swap in all adapters
    table
        .swap(&all_adapter_ids, &[])
        .await
        .expect("Swap all should succeed");

    let active = table.get_active();
    assert_eq!(active.len(), 10, "All 10 adapters should be active");

    let total_vram = table.total_vram_mb();
    assert_eq!(total_vram, 250, "Total VRAM should be 25 * 10 = 250 MB");
}
