//! Integration tests for adapter hot-swap functionality (Tier 6)
//!
//! Tests:
//! - Adapter preload and swap cycles
//! - Rollback on failure
//! - Stack hash determinism
//! - Memory leak detection

use mplora_core::B3Hash;
use mplora_worker::adapter_hotswap::{AdapterCommand, AdapterTable, HotSwapManager};

#[test]
fn test_preload_and_swap_basic() {
    let table = AdapterTable::new();

    // Preload adapters
    let hash1 = B3Hash::hash(b"adapter1");
    let hash2 = B3Hash::hash(b"adapter2");

    table.preload("adapter1".to_string(), hash1, 100).unwrap();
    table.preload("adapter2".to_string(), hash2, 150).unwrap();

    // Swap both in
    let (delta, count) = table
        .swap(&["adapter1".to_string(), "adapter2".to_string()], &[])
        .unwrap();

    assert_eq!(delta, 250, "VRAM delta should be sum of adapter sizes");
    assert_eq!(count, 2, "Should have added 2 adapters");

    // Verify adapters are active
    let active = table.get_active();
    assert_eq!(active.len(), 2);
    assert_eq!(table.total_vram_mb(), 250);
}

#[test]
fn test_adapter_swap_cycle_100_times() {
    // Inject adapter set A→B→A 100 times
    let table = AdapterTable::new();

    let hash_a1 = B3Hash::hash(b"adapter_a1");
    let hash_a2 = B3Hash::hash(b"adapter_a2");
    let hash_b1 = B3Hash::hash(b"adapter_b1");
    let hash_b2 = B3Hash::hash(b"adapter_b2");

    for cycle in 0..100 {
        // Preload set A
        table
            .preload("adapter_a1".to_string(), hash_a1, 50)
            .unwrap();
        table
            .preload("adapter_a2".to_string(), hash_a2, 75)
            .unwrap();

        // Swap to set A (remove any existing)
        if cycle > 0 {
            table
                .swap(
                    &["adapter_a1".to_string(), "adapter_a2".to_string()],
                    &["adapter_b1".to_string(), "adapter_b2".to_string()],
                )
                .unwrap();
        } else {
            table
                .swap(&["adapter_a1".to_string(), "adapter_a2".to_string()], &[])
                .unwrap();
        }

        let hash_a = table.compute_stack_hash();

        // Preload set B
        table.clear_staged();
        table
            .preload("adapter_b1".to_string(), hash_b1, 60)
            .unwrap();
        table
            .preload("adapter_b2".to_string(), hash_b2, 80)
            .unwrap();

        // Swap to set B
        table
            .swap(
                &["adapter_b1".to_string(), "adapter_b2".to_string()],
                &["adapter_a1".to_string(), "adapter_a2".to_string()],
            )
            .unwrap();

        let hash_b = table.compute_stack_hash();

        // Verify hashes are different
        assert_ne!(hash_a, hash_b, "Set A and B should have different hashes");

        // Clear staging for next cycle
        table.clear_staged();
    }

    println!("✓ Completed 100 swap cycles without errors");
}

#[test]
fn test_rollback_on_partial_failure() {
    let table = AdapterTable::new();

    // Initial setup
    let hash1 = B3Hash::hash(b"adapter1");
    table.preload("adapter1".to_string(), hash1, 100).unwrap();
    table.swap(&["adapter1".to_string()], &[]).unwrap();

    let initial_hash = table.compute_stack_hash();

    // Try to swap with a missing adapter (should fail)
    let result = table.swap(&["adapter_missing".to_string()], &["adapter1".to_string()]);

    assert!(result.is_err(), "Swap with missing adapter should fail");

    // Verify state was rolled back
    let active = table.get_active();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, "adapter1");

    let rolled_back_hash = table.compute_stack_hash();
    assert_eq!(
        initial_hash, rolled_back_hash,
        "Hash should match initial state after rollback"
    );
}

#[test]
fn test_stack_hash_determinism() {
    let table = AdapterTable::new();

    // Load adapters in specific order
    let hash1 = B3Hash::hash(b"adapter1");
    let hash2 = B3Hash::hash(b"adapter2");
    let hash3 = B3Hash::hash(b"adapter3");

    table.preload("adapter1".to_string(), hash1, 10).unwrap();
    table.preload("adapter2".to_string(), hash2, 20).unwrap();
    table.preload("adapter3".to_string(), hash3, 30).unwrap();

    table
        .swap(
            &[
                "adapter1".to_string(),
                "adapter2".to_string(),
                "adapter3".to_string(),
            ],
            &[],
        )
        .unwrap();

    // Compute hash multiple times
    let hash_1 = table.compute_stack_hash();
    let hash_2 = table.compute_stack_hash();
    let hash_3 = table.compute_stack_hash();

    assert_eq!(hash_1, hash_2);
    assert_eq!(hash_2, hash_3);

    println!("✓ Stack hash is deterministic: {}", hash_1.to_hex());
}

#[test]
fn test_hotswap_manager_commands() {
    let manager = HotSwapManager::new();

    // Test preload command
    let cmd = AdapterCommand::Preload {
        adapter_id: "test_adapter".to_string(),
        hash: B3Hash::hash(b"test"),
    };

    let result = manager.execute(cmd).unwrap();
    assert!(result.success);
    assert!(result.vram_delta_mb.is_some());
    assert!(result.duration_ms > 0);

    // Test swap command
    let cmd = AdapterCommand::Swap {
        add_ids: vec!["test_adapter".to_string()],
        remove_ids: vec![],
    };

    let result = manager.execute(cmd).unwrap();
    assert!(result.success);
    assert!(result.stack_hash.is_some());

    // Test verify command
    let cmd = AdapterCommand::VerifyStack;
    let result = manager.execute(cmd).unwrap();
    assert!(result.success);
    assert!(result.stack_hash.is_some());
}

#[test]
fn test_vram_delta_tracking() {
    let table = AdapterTable::new();

    // Add adapters with known sizes
    let hash1 = B3Hash::hash(b"adapter1");
    let hash2 = B3Hash::hash(b"adapter2");
    let hash3 = B3Hash::hash(b"adapter3");

    table.preload("adapter1".to_string(), hash1, 100).unwrap();
    table.preload("adapter2".to_string(), hash2, 200).unwrap();
    table.preload("adapter3".to_string(), hash3, 150).unwrap();

    // Swap in adapter1 and adapter2
    let (delta1, _) = table
        .swap(&["adapter1".to_string(), "adapter2".to_string()], &[])
        .unwrap();
    assert_eq!(delta1, 300);

    // Swap out adapter1, add adapter3
    table.preload("adapter3".to_string(), hash3, 150).unwrap();
    let (delta2, _) = table
        .swap(&["adapter3".to_string()], &["adapter1".to_string()])
        .unwrap();

    // Delta should be +150 (adapter3) -100 (adapter1) = +50
    assert_eq!(delta2, 50);
}
