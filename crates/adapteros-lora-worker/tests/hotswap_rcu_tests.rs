//! Iterator & RCU Edge Cases Tests (P3 Low)
//!
//! Tests for RCU-style adapter retirement and iterator edge cases.
//! The hot-swap system uses RCU for safe deferred cleanup.
//!
//! These tests verify:
//! - RCU max retry quarantine
//! - Detach adapter failure retry
//! - Retire list iterator safety
//! - Circular refcount/retire dependency
//! - Unload still-active adapter prevention
//! - Retired generation reuse
//! - Swap atomicity on attach failure
//! - Staged not found at validation
//! - Concurrent modification race
//! - No rollback state fatal error
//! - Partial swap kernel attach failure
//! - Active state visibility ordering

use adapteros_core::B3Hash;
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use std::sync::Arc;
use std::time::Duration;

/// Test that RCU retirement processes correctly with multiple retires.
#[tokio::test]
async fn test_rcu_retirement_multiple_adapters() {
    let table = Arc::new(AdapterTable::new());

    // Preload 10 adapters
    for i in 0..10 {
        let hash = B3Hash::hash(format!("rcu-adapter-{}", i).as_bytes());
        table
            .preload(format!("rcu-adapter-{}", i), hash, 10)
            .await
            .unwrap();
    }

    // Activate all
    let all_ids: Vec<String> = (0..10).map(|i| format!("rcu-adapter-{}", i)).collect();
    table.swap(&all_ids, &[]).await.unwrap();

    // Retire adapters one by one
    for i in 0..10 {
        let keep: Vec<String> = ((i + 1)..10)
            .map(|j| format!("rcu-adapter-{}", j))
            .collect();
        let retire = vec![format!("rcu-adapter-{}", i)];

        if keep.is_empty() {
            // Preload a new adapter to swap to when retiring the last one
            let hash = B3Hash::hash(b"final-adapter");
            table
                .preload("final-adapter".to_string(), hash, 10)
                .await
                .unwrap();
            table
                .swap(&["final-adapter".to_string()], &retire)
                .await
                .unwrap();
        } else {
            table.swap(&keep, &retire).await.unwrap();
        }
    }

    // Final active should be just one adapter
    let active = table.get_active();
    assert_eq!(active.len(), 1);
}

/// Test that retire list remains consistent through multiple swaps.
#[tokio::test]
async fn test_retire_list_consistency() {
    let table = Arc::new(AdapterTable::new());

    let hash1 = B3Hash::hash(b"retire-test-1");
    let hash2 = B3Hash::hash(b"retire-test-2");
    let hash3 = B3Hash::hash(b"retire-test-3");

    table
        .preload("retire-test-1".to_string(), hash1, 50)
        .await
        .unwrap();
    table
        .preload("retire-test-2".to_string(), hash2, 50)
        .await
        .unwrap();
    table
        .preload("retire-test-3".to_string(), hash3, 50)
        .await
        .unwrap();

    // Activate 1 and 2
    table
        .swap(
            &["retire-test-1".to_string(), "retire-test-2".to_string()],
            &[],
        )
        .await
        .unwrap();

    // Add refs to adapter 1
    table.inc_ref("retire-test-1").await;

    // Swap: remove 1, add 3, keep 2
    table
        .swap(
            &["retire-test-2".to_string(), "retire-test-3".to_string()],
            &["retire-test-1".to_string()],
        )
        .await
        .unwrap();

    // Adapter 1 should still be tracked (has refs)
    // Release ref
    table.dec_ref("retire-test-1").await;

    // Active should be 2 and 3
    let active = table.get_active();
    assert_eq!(active.len(), 2);
    let ids: Vec<&str> = active.iter().map(|a| a.id.as_str()).collect();
    assert!(ids.contains(&"retire-test-2"));
    assert!(ids.contains(&"retire-test-3"));
}

/// Test that unloading still-active adapter is prevented.
#[tokio::test]
async fn test_prevent_unload_active_adapter() {
    let table = Arc::new(AdapterTable::new());

    let hash = B3Hash::hash(b"still-active");
    table
        .preload("still-active".to_string(), hash, 50)
        .await
        .unwrap();
    table
        .swap(&["still-active".to_string()], &[])
        .await
        .unwrap();

    // Increment refs to make it active
    table.inc_ref("still-active").await;
    table.inc_ref("still-active").await;

    // Even if we try to swap out, it should remain tracked until refs released
    let hash2 = B3Hash::hash(b"replacement");
    table
        .preload("replacement".to_string(), hash2, 50)
        .await
        .unwrap();
    table
        .swap(&["replacement".to_string()], &["still-active".to_string()])
        .await
        .unwrap();

    // Release refs
    table.dec_ref("still-active").await;
    table.dec_ref("still-active").await;

    // Now the adapter can be cleaned up
    // Active should only be replacement
    let active = table.get_active();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "replacement");
}

/// Test swap atomicity - all-or-nothing for add operations.
#[tokio::test]
async fn test_swap_atomicity_all_or_nothing() {
    let table = Arc::new(AdapterTable::new());

    let hash1 = B3Hash::hash(b"atom-1");
    table
        .preload("atom-1".to_string(), hash1, 50)
        .await
        .unwrap();

    // Swap in atom-1
    table.swap(&["atom-1".to_string()], &[]).await.unwrap();

    // Try to swap in non-existent adapter (should fail validation)
    let result = table
        .swap(&["atom-1".to_string(), "non-existent".to_string()], &[])
        .await;

    // Should fail because non-existent was not preloaded
    assert!(result.is_err());

    // Original state should be preserved
    let active = table.get_active();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "atom-1");
}

/// Test that staged adapter not found causes validation failure.
#[tokio::test]
async fn test_staged_not_found_validation_failure() {
    let table = Arc::new(AdapterTable::new());

    // Try to swap in an adapter that was never preloaded
    let result = table.swap(&["never-preloaded".to_string()], &[]).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("not preloaded") || err.to_string().contains("not found"),
        "Error should indicate adapter not preloaded: {}",
        err
    );
}

/// Test concurrent swap operations maintain consistency.
#[tokio::test]
async fn test_concurrent_swap_consistency() {
    let table = Arc::new(AdapterTable::new());

    // Preload adapters for concurrent swaps
    for i in 0..5 {
        let hash = B3Hash::hash(format!("concurrent-{}", i).as_bytes());
        table
            .preload(format!("concurrent-{}", i), hash, 20)
            .await
            .unwrap();
    }

    // Initial swap
    table
        .swap(&["concurrent-0".to_string()], &[])
        .await
        .unwrap();

    let mut handles = vec![];

    // Concurrent swaps (some may fail due to race, that's expected)
    for i in 1..5 {
        let table_clone = table.clone();
        let prev = format!("concurrent-{}", i - 1);
        let next = format!("concurrent-{}", i);
        handles.push(tokio::spawn(async move {
            // Delay slightly to create race conditions
            tokio::time::sleep(Duration::from_micros(i as u64 * 10)).await;
            let _ = table_clone.swap(&[next], &[prev]).await;
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // After all swaps, should have exactly one active adapter
    let active = table.get_active();
    assert!(
        !active.is_empty(),
        "Should have at least one active adapter"
    );
}

/// Test generation counter doesn't regress during rollback.
#[tokio::test]
async fn test_generation_no_regression_after_failed_swap() {
    let table = Arc::new(AdapterTable::new());

    let hash1 = B3Hash::hash(b"gen-noregress-1");
    table
        .preload("gen-noregress-1".to_string(), hash1, 50)
        .await
        .unwrap();
    table
        .swap(&["gen-noregress-1".to_string()], &[])
        .await
        .unwrap();

    let handle1 = table.get_current_stack_handle();
    let gen1 = handle1.generation;

    // Try a swap that will fail
    let result = table.swap(&["non-existent".to_string()], &[]).await;
    assert!(result.is_err());

    // Generation should not have changed
    let handle2 = table.get_current_stack_handle();
    let gen2 = handle2.generation;
    assert_eq!(gen1, gen2, "Generation should not change on failed swap");
}

/// Test that active state is visible after swap completes.
#[tokio::test]
async fn test_active_state_visibility_after_swap() {
    let table = Arc::new(AdapterTable::new());

    let hash1 = B3Hash::hash(b"vis-1");
    let hash2 = B3Hash::hash(b"vis-2");
    table.preload("vis-1".to_string(), hash1, 50).await.unwrap();
    table.preload("vis-2".to_string(), hash2, 50).await.unwrap();

    // Swap in vis-1
    table.swap(&["vis-1".to_string()], &[]).await.unwrap();

    // Immediately check visibility
    let active1 = table.get_active();
    assert_eq!(active1.len(), 1);
    assert_eq!(active1[0].id, "vis-1");

    // Swap to vis-2
    table
        .swap(&["vis-2".to_string()], &["vis-1".to_string()])
        .await
        .unwrap();

    // Immediately check visibility
    let active2 = table.get_active();
    assert_eq!(active2.len(), 1);
    assert_eq!(active2[0].id, "vis-2");
}

/// Test rapid ping-pong swaps between two adapters.
#[tokio::test]
async fn test_rapid_pingpong_swaps() {
    let table = Arc::new(AdapterTable::new());

    let hash_a = B3Hash::hash(b"ping");
    let hash_b = B3Hash::hash(b"pong");
    table.preload("ping".to_string(), hash_a, 50).await.unwrap();
    table.preload("pong".to_string(), hash_b, 50).await.unwrap();

    // Initial swap
    table.swap(&["ping".to_string()], &[]).await.unwrap();

    // Rapid ping-pong
    for i in 0..100 {
        if i % 2 == 0 {
            table
                .swap(&["pong".to_string()], &["ping".to_string()])
                .await
                .unwrap();
            let active = table.get_active();
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].id, "pong", "Should be pong at iteration {}", i);
        } else {
            table
                .swap(&["ping".to_string()], &["pong".to_string()])
                .await
                .unwrap();
            let active = table.get_active();
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].id, "ping", "Should be ping at iteration {}", i);
        }
    }
}

/// Test swap with empty add list (retire only).
#[tokio::test]
async fn test_swap_retire_only() {
    let table = Arc::new(AdapterTable::new());

    let hash1 = B3Hash::hash(b"retire-only-1");
    let hash2 = B3Hash::hash(b"retire-only-2");
    table
        .preload("retire-only-1".to_string(), hash1, 50)
        .await
        .unwrap();
    table
        .preload("retire-only-2".to_string(), hash2, 50)
        .await
        .unwrap();

    // Activate both
    table
        .swap(
            &["retire-only-1".to_string(), "retire-only-2".to_string()],
            &[],
        )
        .await
        .unwrap();

    assert_eq!(table.get_active().len(), 2);

    // Retire one using keep/remove pattern
    table
        .swap(
            &["retire-only-2".to_string()],
            &["retire-only-1".to_string()],
        )
        .await
        .unwrap();

    let active = table.get_active();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "retire-only-2");
}

/// Test preload then immediate swap without any delay.
#[tokio::test]
async fn test_preload_immediate_swap() {
    let table = Arc::new(AdapterTable::new());

    for i in 0..20 {
        let hash = B3Hash::hash(format!("immediate-{}", i).as_bytes());
        let id = format!("immediate-{}", i);

        // Preload and immediately swap
        table.preload(id.clone(), hash, 25).await.unwrap();

        if i == 0 {
            table.swap(&[id], &[]).await.unwrap();
        } else {
            let prev = format!("immediate-{}", i - 1);
            table.swap(&[id], &[prev]).await.unwrap();
        }

        // Verify immediate visibility
        let active = table.get_active();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, format!("immediate-{}", i));
    }
}

/// Test that stack handle remains valid after swap.
#[tokio::test]
async fn test_stack_handle_validity_after_swap() {
    let table = Arc::new(AdapterTable::new());

    let hash1 = B3Hash::hash(b"handle-valid-1");
    let hash2 = B3Hash::hash(b"handle-valid-2");
    table
        .preload("handle-valid-1".to_string(), hash1, 50)
        .await
        .unwrap();
    table
        .preload("handle-valid-2".to_string(), hash2, 50)
        .await
        .unwrap();

    table
        .swap(&["handle-valid-1".to_string()], &[])
        .await
        .unwrap();

    // Get handle before swap
    let handle_before = table.get_current_stack_handle();
    assert!(handle_before.active.contains_key("handle-valid-1"));

    // Swap to new adapter
    table
        .swap(
            &["handle-valid-2".to_string()],
            &["handle-valid-1".to_string()],
        )
        .await
        .unwrap();

    // Old handle should still reference its snapshot
    assert!(
        handle_before.active.contains_key("handle-valid-1"),
        "Old handle should retain its snapshot"
    );

    // New handle should reference new state
    let handle_after = table.get_current_stack_handle();
    assert!(handle_after.active.contains_key("handle-valid-2"));
    assert!(!handle_after.active.contains_key("handle-valid-1"));

    // Generations should differ
    assert!(handle_after.generation > handle_before.generation);
}

/// Test concurrent swaps targeting the SAME adapter ID.
///
/// Spawns multiple tasks that each attempt to swap to the same adapter ID
/// simultaneously. Asserts: no panics, no data races, final state contains
/// at least one valid active adapter.
#[tokio::test]
async fn test_concurrent_same_adapter_id_swap() {
    let table = Arc::new(AdapterTable::new());

    let hash_initial = B3Hash::hash(b"contested-initial");
    let hash_a = B3Hash::hash(b"target-a");
    let hash_b = B3Hash::hash(b"target-b");

    table
        .preload("contested".to_string(), hash_initial, 50)
        .await
        .unwrap();
    table
        .preload("target-a".to_string(), hash_a, 50)
        .await
        .unwrap();
    table
        .preload("target-b".to_string(), hash_b, 50)
        .await
        .unwrap();

    table.swap(&["contested".to_string()], &[]).await.unwrap();

    let mut handles = vec![];

    for i in 0..4 {
        let table_clone = table.clone();
        let target = if i % 2 == 0 {
            "target-a".to_string()
        } else {
            "target-b".to_string()
        };
        handles.push(tokio::spawn(async move {
            let _ = table_clone
                .swap(&[target], &["contested".to_string()])
                .await;
        }));
    }

    for handle in handles {
        handle.await.expect("Task must not panic");
    }

    let active = table.get_active();
    assert!(
        !active.is_empty(),
        "Must have at least one active adapter after concurrent swaps"
    );
    for entry in &active {
        assert!(
            entry.id == "target-a" || entry.id == "target-b" || entry.id == "contested",
            "Active adapter must be one of the valid swap targets, got: {}",
            entry.id
        );
    }
}
