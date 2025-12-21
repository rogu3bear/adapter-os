//! Retirement Queue Tests
//!
//! Tests for RCU-style adapter retirement and cleanup.
//!
//! Tests comprehensive coverage of:
//! - Refcount-based cleanup (adapters unloaded when refcount==0)
//! - Event-driven wake-up (retirement task wakes within 5ms)
//! - Bounded queue growth (retired queue doesn't grow unbounded)
//! - Concurrent retirement (multiple adapters retired simultaneously)
//! - Memory leak prevention (all retired adapters eventually cleaned up)
//!
//! Status: Complete implementation (2025-01-18)
//! See: crates/adapteros-lora-worker/src/adapter_hotswap.rs:612-704 for implementation

#![cfg(all(test, feature = "extended-tests"))]

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::MockKernels;
use adapteros_lora_worker::adapter_hotswap::AdapterTable;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

/// Test basic retirement flow: refcount reaches 0, adapter is eventually unloaded
#[tokio::test]
async fn test_retirement_on_refcount_zero() {
    let (retirement_tx, mut retirement_rx) = mpsc::channel::<()>(10);
    let table = Arc::new(create_table_with_retirement(retirement_tx));
    let table_clone = table.clone();

    let cleanup_done = Arc::new(AtomicBool::new(false));
    let cleanup_flag = cleanup_done.clone();

    // Spawn retirement task
    let retirement_handle = tokio::spawn(async move {
        while let Some(_) = retirement_rx.recv().await {
            let _ = table_clone
                .process_retired_stacks::<MockKernels>(None)
                .await;
            if table_clone.retired_queue_size() == 0 {
                cleanup_flag.store(true, Ordering::SeqCst);
            }
        }
    });

    // Preload adapter
    let hash = B3Hash::hash(b"test_adapter");
    table.preload("adapter1".to_string(), hash, 100).unwrap();

    // Swap in
    table.swap(&["adapter1".to_string()], &[]).unwrap();

    // Increment refcount (simulate inference using adapter)
    table.inc_ref("adapter1");
    assert_eq!(table.get_refcount("adapter1"), 1);

    // Swap out (should move to retired queue)
    table
        .preload("adapter2".to_string(), B3Hash::hash(b"other"), 50)
        .unwrap();
    table
        .swap(&["adapter2".to_string()], &["adapter1".to_string()])
        .unwrap();

    // Verify adapter is in retired queue
    assert_eq!(table.retired_queue_size(), 1);

    // Decrement refcount to 0 (triggers retirement signal)
    let old_rc = table.dec_ref("adapter1");
    assert_eq!(old_rc, 1); // Was 1 before decrement
    assert_eq!(table.get_refcount("adapter1"), 0);

    // Send explicit signal
    table.send_retirement_signal().unwrap();

    // Wait for cleanup
    for _ in 0..40 {
        // 2 seconds max
        if cleanup_done.load(Ordering::SeqCst) {
            break;
        }
        sleep(Duration::from_millis(50)).await;
    }

    // Verify adapter is eventually unloaded
    assert!(
        cleanup_done.load(Ordering::SeqCst),
        "Adapter should be unloaded after refcount reaches 0"
    );
    assert_eq!(table.retired_queue_size(), 0);

    retirement_handle.abort();
}

/// Test event-driven wake-up latency
///
/// Verifies documented claim: "retirement task wakes within 5ms of refcount==0"
#[tokio::test]
async fn test_retirement_wake_latency() {
    let (retirement_tx, mut retirement_rx) = mpsc::channel::<()>(10);
    let table = Arc::new(create_table_with_retirement(retirement_tx));
    let table_clone = table.clone();

    let wake_time = Arc::new(AtomicUsize::new(0));
    let wake_time_clone = wake_time.clone();

    // Spawn retirement task that records wake-up time
    let retirement_handle = tokio::spawn(async move {
        if let Some(_) = retirement_rx.recv().await {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as usize;
            wake_time_clone.store(now, Ordering::SeqCst);
            let _ = table_clone
                .process_retired_stacks::<MockKernels>(None)
                .await;
        }
    });

    // Setup: Load and swap adapter
    let hash = B3Hash::hash(b"test_adapter");
    table.preload("test".to_string(), hash, 100).unwrap();
    table.swap(&["test".to_string()], &[]).unwrap();
    table.inc_ref("test");

    // Swap out (moves to retired queue)
    table
        .preload("other".to_string(), B3Hash::hash(b"other"), 50)
        .unwrap();
    table
        .swap(&["other".to_string()], &["test".to_string()])
        .unwrap();

    // Measure time from refcount==0 to wake-up
    table.dec_ref("test");
    let signal_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as usize;

    table.send_retirement_signal().unwrap();

    // Wait up to 100ms for wake
    sleep(Duration::from_millis(100)).await;

    let wake = wake_time.load(Ordering::SeqCst);
    assert!(wake > 0, "Retirement task should have woken up");

    let latency_ms = wake.saturating_sub(signal_time);
    eprintln!("⏱️  Retirement wake latency: {} ms", latency_ms);

    // Verify documented claim: <5ms (relaxed to <10ms for CI)
    assert!(
        latency_ms < 10,
        "Wake latency should be <10ms, got {}ms",
        latency_ms
    );

    retirement_handle.abort();
}

/// Test bounded queue growth: retired queue doesn't grow unbounded
#[tokio::test]
async fn test_retirement_queue_bounded() {
    let (retirement_tx, mut retirement_rx) = mpsc::channel::<()>(100);
    let table = Arc::new(create_table_with_retirement(retirement_tx));
    let table_clone = table.clone();

    // Spawn retirement task
    let retirement_handle = tokio::spawn(async move {
        while let Some(_) = retirement_rx.recv().await {
            let _ = table_clone
                .process_retired_stacks::<MockKernels>(None)
                .await;
        }
    });

    // Retire many adapters
    for i in 0..50 {
        let id = format!("adapter_{}", i);
        let hash = B3Hash::hash(format!("adapter_{}", i).as_bytes());
        table.preload(id.clone(), hash, 10).unwrap();
        table.swap(&[id.clone()], &[]).unwrap();

        // Swap out immediately (no refcount, should be cleaned up fast)
        table.swap(&[], &[id.clone()]).expect("Failed to swap out");

        // Send retirement signal
        table.send_retirement_signal().unwrap();

        // Give retirement task time to process
        sleep(Duration::from_millis(10)).await;
    }

    // Wait for final cleanup
    sleep(Duration::from_millis(200)).await;

    // Verify queue doesn't grow unbounded (should be 0 or very small)
    let final_size = table.retired_queue_size();
    eprintln!(
        "📊 Final retired queue size after 50 adapters: {}",
        final_size
    );
    assert!(
        final_size < 10,
        "Retired queue should be small (<10), got {}",
        final_size
    );

    retirement_handle.abort();
}

/// Test concurrent retirement: multiple adapters retired simultaneously
#[tokio::test]
async fn test_concurrent_retirement() {
    let (retirement_tx, mut retirement_rx) = mpsc::channel::<()>(100);
    let table = Arc::new(create_table_with_retirement(retirement_tx));
    let table_clone = table.clone();

    // Spawn retirement task
    let retirement_handle = tokio::spawn(async move {
        while let Some(_) = retirement_rx.recv().await {
            let _ = table_clone
                .process_retired_stacks::<MockKernels>(None)
                .await;
        }
    });

    // Preload 10 adapters
    for i in 0..10 {
        let id = format!("adapter_{}", i);
        let hash = B3Hash::hash(format!("adapter_{}", i).as_bytes());
        table.preload(id.clone(), hash, 10).unwrap();
    }

    // Swap all in
    let adapter_ids: Vec<String> = (0..10).map(|i| format!("adapter_{}", i)).collect();
    table.swap(&adapter_ids, &[]).unwrap();

    // Increment refcounts
    for id in &adapter_ids {
        table.inc_ref(id);
    }

    // Swap all out concurrently (moves to retired queue)
    table.swap(&[], &adapter_ids).unwrap();
    assert_eq!(table.retired_queue_size(), 1); // All in one stack

    // Decrement refcounts concurrently
    let mut handles = vec![];
    for id in &adapter_ids {
        let table_clone = table.clone();
        let id_clone = id.clone();
        let handle = tokio::spawn(async move {
            table_clone.dec_ref(&id_clone);
        });
        handles.push(handle);
    }

    // Wait for all decrements
    for handle in handles {
        handle.await.unwrap();
    }

    // Send retirement signal
    table.send_retirement_signal().unwrap();

    // Wait for cleanup
    sleep(Duration::from_millis(200)).await;

    // Verify no race conditions and all adapters eventually cleaned up
    let final_size = table.retired_queue_size();
    eprintln!(
        "📊 Retired queue size after concurrent retirement: {}",
        final_size
    );
    assert_eq!(final_size, 0, "All adapters should be cleaned up");

    retirement_handle.abort();
}

/// Test memory leak prevention: cycle through many adapters
#[tokio::test]
async fn test_no_retirement_memory_leaks() {
    let (retirement_tx, mut retirement_rx) = mpsc::channel::<()>(1000);
    let table = Arc::new(create_table_with_retirement(retirement_tx));
    let table_clone = table.clone();

    // Spawn retirement task
    let retirement_handle = tokio::spawn(async move {
        while let Some(_) = retirement_rx.recv().await {
            let _ = table_clone
                .process_retired_stacks::<MockKernels>(None)
                .await;
        }
    });

    // Cycle through many adapters (simulates long-running system)
    for i in 0..100 {
        let id = format!("adapter_{}", i);
        let hash = B3Hash::hash(format!("adapter_{}", i).as_bytes());
        table.preload(id.clone(), hash, 10).unwrap();
        table.swap(&[id.clone()], &[]).unwrap();

        // Use adapter (inc/dec refcount)
        table.inc_ref(&id);
        table.dec_ref(&id);

        // Swap out
        table.swap(&[], &[id]).unwrap();

        // Signal retirement
        table.send_retirement_signal().unwrap();

        // Periodic check every 10 iterations
        if i % 10 == 0 {
            sleep(Duration::from_millis(50)).await;
            let queue_size = table.retired_queue_size();
            eprintln!("   Iteration {}: retired queue size = {}", i, queue_size);
        }
    }

    // Final cleanup wait
    sleep(Duration::from_millis(500)).await;

    // Verify retired queue is empty (no memory leaks)
    let final_size = table.retired_queue_size();
    eprintln!(
        "📊 Final retired queue size after 100 cycles: {}",
        final_size
    );
    assert_eq!(
        final_size, 0,
        "Retired queue should be empty, no lingering Arc<Stack> references"
    );

    // Verify no lingering refcounts
    for i in 0..100 {
        let id = format!("adapter_{}", i);
        let refcount = table.get_refcount(&id);
        assert_eq!(refcount, 0, "Adapter {} should have refcount 0", id);
    }

    eprintln!("✅ No memory leaks detected after 100 adapter cycles");

    retirement_handle.abort();
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Create AdapterTable with retirement channel configured
fn create_table_with_retirement(tx: mpsc::Sender<()>) -> AdapterTable {
    let mut table = AdapterTable::new();
    table.set_retirement_sender(Some(tx));
    table
}
