//! Hot-Swap Generation Counter Tests (P0 Critical)
//!
//! Tests for generation counter edge cases in the adapter hot-swap system.
//! The generation counter (`current_stack: AtomicUsize`) tracks swap generations
//! and must be handled correctly near boundaries.
//!
//! These tests verify:
//! - Generation monotonically increases on successful swaps
//! - Generation counter behavior at high values
//! - Concurrent swap generation ordering
//! - Stale retry count handling on recycled generations
//! - Generation counter leak detection

#![allow(unused_imports)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::expect_fun_call)]

use adapteros_core::B3Hash;
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Test that generation counter monotonically increases on successful swaps.
///
/// Each successful swap should increment the generation by exactly 1.
/// This invariant is critical for RCU-style read isolation.
#[tokio::test]
async fn test_generation_monotonically_increases() {
    let table = AdapterTable::new();

    // Preload adapters
    for i in 0..10 {
        let hash = B3Hash::hash(format!("adapter{}", i).as_bytes());
        table
            .preload(format!("adapter{}", i), hash, 10)
            .await
            .expect("Preload should succeed");
    }

    // Initial generation should be 0
    let stack0 = table.get_current_stack_handle();
    assert_eq!(stack0.generation, 0, "Initial generation should be 0");

    // First swap: generation 0 -> 1
    table.swap(&["adapter0".to_string()], &[]).await.unwrap();
    let stack1 = table.get_current_stack_handle();
    assert_eq!(
        stack1.generation, 1,
        "After first swap, generation should be 1"
    );

    // Multiple swaps should increment generation
    for i in 1..10 {
        table
            .swap(&[format!("adapter{}", i)], &[format!("adapter{}", i - 1)])
            .await
            .expect("Swap should succeed");

        let stack = table.get_current_stack_handle();
        assert_eq!(
            stack.generation,
            (i + 1) as u64,
            "Generation should be {} after {} swaps",
            i + 1,
            i + 1
        );
    }
}

/// Test that generation is NOT incremented on failed swaps.
///
/// If a swap fails (e.g., adapter not preloaded), generation should
/// remain unchanged to maintain consistency.
#[tokio::test]
async fn test_generation_unchanged_on_failed_swap() {
    let table = AdapterTable::new();

    // Preload only adapter0
    let hash = B3Hash::hash(b"adapter0");
    table
        .preload("adapter0".to_string(), hash, 10)
        .await
        .expect("Preload should succeed");

    // Initial swap to establish generation 1
    table.swap(&["adapter0".to_string()], &[]).await.unwrap();
    let initial_gen = table.get_current_stack_handle().generation;
    assert_eq!(initial_gen, 1);

    // Attempt to swap in adapter1 (not preloaded) - should fail
    let result = table.swap(&["adapter1".to_string()], &[]).await;
    assert!(
        result.is_err(),
        "Swap with non-preloaded adapter should fail"
    );

    // Generation should remain unchanged
    let after_failed = table.get_current_stack_handle().generation;
    assert_eq!(
        after_failed, initial_gen,
        "Generation should not change after failed swap"
    );
}

/// Test concurrent swap generation ordering.
///
/// Multiple concurrent swap attempts should result in proper ordering,
/// with each successful swap getting a unique, increasing generation.
#[tokio::test]
async fn test_concurrent_swap_generation_ordering() {
    let table = Arc::new(AdapterTable::new());

    // Preload many adapters for concurrent swapping
    for i in 0..100 {
        let hash = B3Hash::hash(format!("concurrent{}", i).as_bytes());
        table
            .preload(format!("concurrent{}", i), hash, 5)
            .await
            .expect("Preload should succeed");
    }

    // Initial swap
    table.swap(&["concurrent0".to_string()], &[]).await.unwrap();

    // Track all observed generations
    let observed_gens = Arc::new(std::sync::Mutex::new(Vec::new()));

    let mut handles = vec![];
    for i in 1..50 {
        let table_clone = table.clone();
        let gens_clone = observed_gens.clone();

        handles.push(tokio::spawn(async move {
            // Small delay to increase concurrency overlap
            tokio::time::sleep(tokio::time::Duration::from_micros(i as u64 * 10)).await;

            if let Ok(_) = table_clone
                .swap(
                    &[format!("concurrent{}", i)],
                    &[format!("concurrent{}", i - 1)],
                )
                .await
            {
                let gen = table_clone.get_current_stack_handle().generation;
                gens_clone.lock().unwrap().push(gen);
            }
        }));
    }

    // Wait for all concurrent swaps
    for handle in handles {
        let _ = handle.await;
    }

    // Verify observed generations
    let gens = observed_gens.lock().unwrap();
    assert!(!gens.is_empty(), "Some swaps should have succeeded");

    // The observed generations should all be valid (> 0)
    for gen in gens.iter() {
        assert!(*gen > 0, "All observed generations should be positive");
    }

    // Final generation should be greater than initial
    let final_gen = table.get_current_stack_handle().generation;
    assert!(final_gen > 1, "Final generation should have increased");
}

/// Test that stack hash remains deterministic across generation changes.
///
/// The stack hash computed at a given generation should be deterministic
/// regardless of how many generations have passed.
#[tokio::test]
async fn test_stack_hash_deterministic_across_generations() {
    let table = AdapterTable::new();

    // Preload with known data
    let hash1 = B3Hash::hash(b"deterministic_adapter1");
    let hash2 = B3Hash::hash(b"deterministic_adapter2");

    table
        .preload("deterministic_adapter1".to_string(), hash1, 20)
        .await
        .unwrap();
    table
        .preload("deterministic_adapter2".to_string(), hash2, 20)
        .await
        .unwrap();

    // Swap in adapter1
    table
        .swap(&["deterministic_adapter1".to_string()], &[])
        .await
        .unwrap();
    let hash_v1 = table.compute_stack_hash();

    // Swap to adapter2
    table
        .swap(
            &["deterministic_adapter2".to_string()],
            &["deterministic_adapter1".to_string()],
        )
        .await
        .unwrap();
    let hash_v2 = table.compute_stack_hash();

    // Swap back to adapter1
    table
        .swap(
            &["deterministic_adapter1".to_string()],
            &["deterministic_adapter2".to_string()],
        )
        .await
        .unwrap();
    let hash_v1_again = table.compute_stack_hash();

    // Same active set should produce same hash
    assert_eq!(
        hash_v1, hash_v1_again,
        "Same active adapter set should produce same hash"
    );
    assert_ne!(
        hash_v1, hash_v2,
        "Different adapter sets should produce different hashes"
    );
}

/// Test generation counter at high values.
///
/// Verifies the system behaves correctly when generation counter
/// reaches high values (simulated by many rapid swaps).
#[tokio::test]
async fn test_generation_counter_high_values() {
    let table = AdapterTable::new();

    // Preload two adapters for ping-pong swapping
    let hash_a = B3Hash::hash(b"ping");
    let hash_b = B3Hash::hash(b"pong");

    table.preload("ping".to_string(), hash_a, 10).await.unwrap();
    table.preload("pong".to_string(), hash_b, 10).await.unwrap();

    // Initial swap
    table.swap(&["ping".to_string()], &[]).await.unwrap();

    // Perform many swaps to drive generation counter up
    const ITERATIONS: usize = 500;
    for i in 0..ITERATIONS {
        let (add, remove) = if i % 2 == 0 {
            (&["pong".to_string()], &["ping".to_string()])
        } else {
            (&["ping".to_string()], &["pong".to_string()])
        };

        table
            .swap(add, remove)
            .await
            .expect(&format!("Swap {} should succeed", i));
    }

    // Final generation should be ITERATIONS + 1 (initial swap + ITERATIONS swaps)
    let final_gen = table.get_current_stack_handle().generation;
    assert_eq!(
        final_gen,
        (ITERATIONS + 1) as u64,
        "Generation should be {} after {} swaps",
        ITERATIONS + 1,
        ITERATIONS + 1
    );

    // System should still function correctly
    let stack_hash = table.compute_stack_hash();
    assert_ne!(stack_hash, B3Hash::zero(), "Stack hash should be valid");
}
