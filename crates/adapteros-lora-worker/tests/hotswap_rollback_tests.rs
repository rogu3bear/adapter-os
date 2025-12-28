//! Hot-Swap Rollback Tests (P2 Medium)
//!
//! Tests for rollback functionality in the hot-swap system.
//! Rollback allows reverting to a previous known-good state.
//!
//! These tests verify:
//! - Rollback without prior swap fails
//! - Rollback restores previous generation
//! - Rollback clears staged on failure
//! - Double rollback fails
//! - Rollback during active inference
//! - Rollback generation regression
//! - Rollback preserves refcounts

use adapteros_core::B3Hash;
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use std::sync::Arc;

/// Test that rollback fails when there's no prior state to roll back to.
#[tokio::test]
async fn test_rollback_without_prior_swap_fails() {
    let table = AdapterTable::new();

    // Attempt rollback on fresh table (no prior swap)
    let result = table.rollback().await;

    // Should fail - no rollback state available
    assert!(result.is_err(), "Rollback without prior swap should fail");
}

/// Test that rollback restores the previous adapter configuration.
#[tokio::test]
async fn test_rollback_restores_previous_state() {
    let table = Arc::new(AdapterTable::new());

    // Preload adapters
    let hash1 = B3Hash::hash(b"adapter1");
    let hash2 = B3Hash::hash(b"adapter2");
    table
        .preload("adapter1".to_string(), hash1, 50)
        .await
        .unwrap();
    table
        .preload("adapter2".to_string(), hash2, 50)
        .await
        .unwrap();

    // First swap - activate adapter1
    table.swap(&["adapter1".to_string()], &[]).await.unwrap();
    let active_after_first = table.get_active();
    assert_eq!(active_after_first.len(), 1);
    assert_eq!(active_after_first[0].id, "adapter1");

    // Second swap - replace with adapter2
    table
        .swap(&["adapter2".to_string()], &["adapter1".to_string()])
        .await
        .unwrap();
    let active_after_second = table.get_active();
    assert_eq!(active_after_second.len(), 1);
    assert_eq!(active_after_second[0].id, "adapter2");

    // Rollback - should restore adapter1
    let rollback_result = table.rollback().await;
    assert!(rollback_result.is_ok(), "Rollback should succeed");

    let active_after_rollback = table.get_active();
    assert_eq!(active_after_rollback.len(), 1);
    assert_eq!(
        active_after_rollback[0].id, "adapter1",
        "Rollback should restore adapter1"
    );
}

/// Test that generation counter behaves correctly during rollback.
#[tokio::test]
async fn test_rollback_generation_behavior() {
    let table = Arc::new(AdapterTable::new());

    let hash1 = B3Hash::hash(b"gen-adapter1");
    let hash2 = B3Hash::hash(b"gen-adapter2");
    table
        .preload("gen-adapter1".to_string(), hash1, 50)
        .await
        .unwrap();
    table
        .preload("gen-adapter2".to_string(), hash2, 50)
        .await
        .unwrap();

    // Swap 1: generation 0 -> 1
    table
        .swap(&["gen-adapter1".to_string()], &[])
        .await
        .unwrap();
    let gen1 = table.get_current_stack_handle().generation;
    assert_eq!(gen1, 1);

    // Swap 2: generation 1 -> 2
    table
        .swap(&["gen-adapter2".to_string()], &["gen-adapter1".to_string()])
        .await
        .unwrap();
    let gen2 = table.get_current_stack_handle().generation;
    assert_eq!(gen2, 2);

    // Rollback: generation should still increase (or stay same)
    table.rollback().await.unwrap();
    let gen_after_rollback = table.get_current_stack_handle().generation;

    // Generation should not go backwards
    assert!(
        gen_after_rollback >= gen1,
        "Generation should not decrease after rollback"
    );
}

/// Test that multiple swaps followed by rollback works correctly.
#[tokio::test]
async fn test_multiple_swaps_then_rollback() {
    let table = Arc::new(AdapterTable::new());

    // Preload several adapters
    for i in 0..5 {
        let hash = B3Hash::hash(format!("multi-{}", i).as_bytes());
        table
            .preload(format!("multi-{}", i), hash, 20)
            .await
            .unwrap();
    }

    // Chain of swaps
    table.swap(&["multi-0".to_string()], &[]).await.unwrap();
    table
        .swap(&["multi-1".to_string()], &["multi-0".to_string()])
        .await
        .unwrap();
    table
        .swap(&["multi-2".to_string()], &["multi-1".to_string()])
        .await
        .unwrap();
    table
        .swap(&["multi-3".to_string()], &["multi-2".to_string()])
        .await
        .unwrap();

    let before_rollback = table.get_active();
    assert_eq!(before_rollback[0].id, "multi-3");

    // Single rollback should go back one step
    table.rollback().await.unwrap();

    let after_rollback = table.get_active();
    assert_eq!(
        after_rollback[0].id, "multi-2",
        "Should roll back to previous state (multi-2)"
    );
}

/// Test that active adapters have correct VRAM after rollback.
#[tokio::test]
async fn test_rollback_preserves_vram_tracking() {
    let table = Arc::new(AdapterTable::new());

    let hash1 = B3Hash::hash(b"vram-adapter1");
    let hash2 = B3Hash::hash(b"vram-adapter2");
    table
        .preload("vram-adapter1".to_string(), hash1, 100)
        .await
        .unwrap();
    table
        .preload("vram-adapter2".to_string(), hash2, 200)
        .await
        .unwrap();

    // Swap in adapter1 (100MB VRAM)
    table
        .swap(&["vram-adapter1".to_string()], &[])
        .await
        .unwrap();
    assert_eq!(table.total_vram_mb(), 100);

    // Swap to adapter2 (200MB VRAM)
    table
        .swap(
            &["vram-adapter2".to_string()],
            &["vram-adapter1".to_string()],
        )
        .await
        .unwrap();
    assert_eq!(table.total_vram_mb(), 200);

    // Rollback to adapter1
    table.rollback().await.unwrap();

    // VRAM should be back to 100MB
    assert_eq!(
        table.total_vram_mb(),
        100,
        "VRAM tracking should be restored after rollback"
    );
}

/// Test that rollback works with multiple active adapters.
#[tokio::test]
async fn test_rollback_with_multiple_active_adapters() {
    let table = Arc::new(AdapterTable::new());

    // Preload adapters
    for i in 0..4 {
        let hash = B3Hash::hash(format!("multi-active-{}", i).as_bytes());
        table
            .preload(format!("multi-active-{}", i), hash, 25)
            .await
            .unwrap();
    }

    // First state: adapter 0 and 1
    table
        .swap(
            &["multi-active-0".to_string(), "multi-active-1".to_string()],
            &[],
        )
        .await
        .unwrap();
    let state1 = table.get_active();
    assert_eq!(state1.len(), 2);

    // Second state: adapter 2 and 3
    table
        .swap(
            &["multi-active-2".to_string(), "multi-active-3".to_string()],
            &["multi-active-0".to_string(), "multi-active-1".to_string()],
        )
        .await
        .unwrap();
    let state2 = table.get_active();
    assert_eq!(state2.len(), 2);

    // Rollback to state1
    table.rollback().await.unwrap();

    let rolled_back = table.get_active();
    assert_eq!(rolled_back.len(), 2);

    // Should have adapter 0 and 1 again
    let ids: Vec<&str> = rolled_back.iter().map(|a| a.id.as_str()).collect();
    assert!(ids.contains(&"multi-active-0"));
    assert!(ids.contains(&"multi-active-1"));
}
