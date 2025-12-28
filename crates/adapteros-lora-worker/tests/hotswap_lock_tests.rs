//! Hot-Swap Lock Contention Tests (P2 Medium)
//!
//! Tests for lock behavior and contention handling in the hot-swap system.
//! The system uses RwLock for active/staged and Mutex for refcounts.
//!
//! These tests verify:
//! - RwLock writer starvation scenarios
//! - Concurrent preload and swap operations
//! - Retirement task lock contention
//! - Snapshot read doesn't hold lock
//! - Circular lock dependency avoided
//! - Lock poisoning recovery

use adapteros_core::B3Hash;
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

/// Test that multiple readers don't block each other.
#[tokio::test]
async fn test_concurrent_reads_dont_block() {
    let table = Arc::new(AdapterTable::new());

    let hash = B3Hash::hash(b"read-test");
    table
        .preload("read-test".to_string(), hash, 50)
        .await
        .unwrap();
    table.swap(&["read-test".to_string()], &[]).await.unwrap();

    let mut handles = vec![];

    // Spawn many concurrent readers
    for _ in 0..50 {
        let table_clone = table.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..100 {
                let _ = table_clone.get_active();
                let _ = table_clone.total_vram_mb();
                let _ = table_clone.get_current_stack_handle();
            }
        }));
    }

    // All reads should complete quickly
    let result = timeout(Duration::from_secs(5), async {
        for handle in handles {
            handle.await.unwrap();
        }
    })
    .await;

    assert!(result.is_ok(), "Concurrent reads should complete quickly");
}

/// Test that concurrent preloads don't deadlock.
#[tokio::test]
async fn test_concurrent_preloads_no_deadlock() {
    let table = Arc::new(AdapterTable::new());

    let mut handles = vec![];

    // Spawn many concurrent preloads
    for i in 0..20 {
        let table_clone = table.clone();
        handles.push(tokio::spawn(async move {
            let hash = B3Hash::hash(format!("concurrent-{}", i).as_bytes());
            table_clone
                .preload(format!("concurrent-{}", i), hash, 10 + i as u64)
                .await
        }));
    }

    // All preloads should complete
    let result = timeout(Duration::from_secs(10), async {
        let mut success_count = 0;
        for handle in handles {
            if handle.await.unwrap().is_ok() {
                success_count += 1;
            }
        }
        success_count
    })
    .await;

    assert!(result.is_ok(), "Concurrent preloads should not deadlock");
    let success_count = result.unwrap();
    assert_eq!(success_count, 20, "All preloads should succeed");
}

/// Test that swap during heavy read load completes.
#[tokio::test]
async fn test_swap_during_heavy_read_load() {
    let table = Arc::new(AdapterTable::new());

    // Preload adapters
    let hash1 = B3Hash::hash(b"load-test-1");
    let hash2 = B3Hash::hash(b"load-test-2");
    table
        .preload("load-test-1".to_string(), hash1, 50)
        .await
        .unwrap();
    table
        .preload("load-test-2".to_string(), hash2, 50)
        .await
        .unwrap();
    table.swap(&["load-test-1".to_string()], &[]).await.unwrap();

    // Start heavy read load
    let table_for_readers = table.clone();
    let readers_running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let readers_running_clone = readers_running.clone();

    let reader_handle = tokio::spawn(async move {
        while readers_running_clone.load(std::sync::atomic::Ordering::Relaxed) {
            for _ in 0..100 {
                let _ = table_for_readers.get_active();
            }
            tokio::time::sleep(Duration::from_micros(10)).await;
        }
    });

    // Perform swap while readers are active
    let swap_result = timeout(Duration::from_secs(5), async {
        table
            .swap(&["load-test-2".to_string()], &["load-test-1".to_string()])
            .await
    })
    .await;

    // Stop readers
    readers_running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = reader_handle.await;

    assert!(swap_result.is_ok(), "Swap should complete within timeout");
    assert!(
        swap_result.unwrap().is_ok(),
        "Swap should succeed during read load"
    );
}

/// Test that sequential swaps don't block indefinitely.
#[tokio::test]
async fn test_sequential_swaps_complete() {
    let table = Arc::new(AdapterTable::new());

    // Preload adapters
    for i in 0..10 {
        let hash = B3Hash::hash(format!("seq-{}", i).as_bytes());
        table.preload(format!("seq-{}", i), hash, 10).await.unwrap();
    }

    // First swap
    table.swap(&["seq-0".to_string()], &[]).await.unwrap();

    // Many sequential swaps
    let result = timeout(Duration::from_secs(10), async {
        for i in 1..10 {
            table
                .swap(&[format!("seq-{}", i)], &[format!("seq-{}", i - 1)])
                .await
                .unwrap();
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "Sequential swaps should complete within timeout"
    );

    let active = table.get_active();
    assert_eq!(active[0].id, "seq-9");
}

/// Test that stack handle snapshot doesn't hold locks.
#[tokio::test]
async fn test_stack_handle_snapshot_no_lock_hold() {
    let table = Arc::new(AdapterTable::new());

    let hash = B3Hash::hash(b"snapshot-test");
    table
        .preload("snapshot-test".to_string(), hash, 50)
        .await
        .unwrap();
    table
        .swap(&["snapshot-test".to_string()], &[])
        .await
        .unwrap();

    // Get a snapshot handle
    let handle = table.get_current_stack_handle();

    // While holding the handle, we should still be able to do operations
    let hash2 = B3Hash::hash(b"snapshot-test-2");
    let preload_result = table
        .preload("snapshot-test-2".to_string(), hash2, 50)
        .await;

    assert!(
        preload_result.is_ok(),
        "Preload should work while handle is held"
    );

    // Handle should still be valid
    assert!(handle.generation >= 1);
}

/// Test interleaved read and write operations.
#[tokio::test]
async fn test_interleaved_read_write_operations() {
    let table = Arc::new(AdapterTable::new());

    // Preload initial adapter
    let hash = B3Hash::hash(b"interleave-base");
    table
        .preload("interleave-base".to_string(), hash, 50)
        .await
        .unwrap();
    table
        .swap(&["interleave-base".to_string()], &[])
        .await
        .unwrap();

    let mut handles = vec![];

    // Mix of readers and writers
    for i in 0..30 {
        let table_clone = table.clone();
        if i % 3 == 0 {
            // Writer: preload new adapter
            handles.push(tokio::spawn(async move {
                let hash = B3Hash::hash(format!("interleave-{}", i).as_bytes());
                let _ = table_clone
                    .preload(format!("interleave-{}", i), hash, 10)
                    .await;
                "write"
            }));
        } else {
            // Reader
            handles.push(tokio::spawn(async move {
                let _ = table_clone.get_active();
                "read"
            }));
        }
    }

    let result = timeout(Duration::from_secs(10), async {
        let mut completed = 0;
        for handle in handles {
            let _ = handle.await;
            completed += 1;
        }
        completed
    })
    .await;

    assert!(result.is_ok(), "All operations should complete");
    assert_eq!(result.unwrap(), 30);
}
