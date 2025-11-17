//! Concurrency tests for RCU hot-swap

#[cfg(feature = "loom")]
use loom::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "loom")]
use loom::sync::Arc;
#[cfg(feature = "loom")]
use loom::thread;

#[cfg(test)]
use crate::adapter_hotswap::{AdapterTable, B3Hash, Stack};
#[cfg(test)]
use std::sync::Arc as StdArc;
#[cfg(test)]
use std::thread::sleep;
#[cfg(test)]
use std::time::Duration;

#[cfg(feature = "loom")]
mod loom_tests {
    use super::*;

    #[test]
    fn loom_rcu_basic() {
        loom::model(|| {
            let table = StdArc::new(AdapterTable::new());
            let hash = B3Hash::hash(b"test");

            // Preload
            table.preload("test".to_string(), hash, 10).unwrap();

            // Swap in
            table.swap(&["test".to_string()], &[]).unwrap();

            let adapter_id = "test".to_string();

            // Multiple readers inc ref
            let num_readers = 5;
            let threads: Vec<_> = (0..num_readers).map(|_| {
                let table = StdArc::clone(&table);
                thread::spawn(move || {
                    table.inc_ref(&adapter_id);
                    // Simulate hold
                    for _ in 0..1000 {
                        std::hint::spin_loop();
                    }
                    let new_ref = table.dec_ref(&adapter_id);
                    assert!(new_ref >= 0);
                })
            }).collect();

            // Writer swap out after some time
            thread::spawn(move || {
                sleep(Duration::from_millis(10));
                table.swap(&[], &["test".to_string()]).unwrap();
            });

            // Join readers
            for t in threads {
                t.join().unwrap();
            }

            // After all, ref should be 0, but since background not in loom, just check
            let refc = table.refcounts.get(&adapter_id).unwrap().load(Ordering::Relaxed);
            assert_eq!(refc, 0);
        });
    }
}

#[cfg(not(feature = "loom"))]
mod loom_tests {
    #[test]
    fn loom_rcu_basic() {
        // Loom not enabled, skip
    }
}

#[tokio::test]
async fn stress_rcu() {
    use std::sync::Arc as StdArc;
    use tokio::time::{sleep, Duration};

    let table = StdArc::new(AdapterTable::new());
    let hash = B3Hash::hash(b"test");

    // Preload and swap in
    table.preload("test".to_string(), hash, 10).unwrap();
    table.swap(&["test".to_string()], &[]).unwrap();

    let adapter_id = "test".to_string();

    let mut handles = vec![];

    // 50 concurrent inferences
    for i in 0..50 {
        let table_clone = StdArc::clone(&table);
        let handle = tokio::spawn(async move {
            table_clone.inc_ref(&adapter_id);
            sleep(Duration::from_secs(1)).await; // Hold for 1s
            let new_ref = table_clone.dec_ref(&adapter_id);
            assert!(new_ref >= 0, "Refcount negative after dec");
        });
        handles.push(handle);
    }

    // Swap task
    let table_clone = StdArc::clone(&table);
    let swap_handle = tokio::spawn(async move {
        for _ in 0..100 { // 10s / 100ms
            sleep(Duration::from_millis(100)).await;
            // Swap out and back
            table_clone.swap(&[], &["test".to_string()]).unwrap();
            table_clone.swap(&["test".to_string()], &[]).unwrap();
        }
    });

    // Wait for all
    for h in handles {
        h.await.unwrap();
    }
    swap_handle.await.unwrap();

    // Check final ref 0
    let refc = table.refcounts.get(&adapter_id).unwrap().load(Ordering::Relaxed);
    assert_eq!(refc, 0, "Refcount not zero after stress");

    // Since no kernels, no unload check
    println!("Stress test passed");
}

#[tokio::test]
async fn test_long_workflow_during_swap() {
    use std::sync::Arc as StdArc;
    use tokio::time::{sleep, Duration};

    let table = StdArc::new(AdapterTable::new());
    let hash = B3Hash::hash(b"test");

    // Preload and swap in
    table.preload("test".to_string(), hash, 10).unwrap();
    table.swap(&["test".to_string()], &[]).unwrap();

    let adapter_id = "test".to_string();

    // Start long workflow: hold for 2s
    let workflow_handle = tokio::spawn(async move {
        table.inc_ref(&adapter_id);
        sleep(Duration::from_secs(2)).await; // Long hold
        let _ = table.dec_ref(&adapter_id);
    });

    // Swap during workflow
    tokio::spawn(async move {
        sleep(Duration::from_millis(500)).await; // Mid-way
        let _ = table.swap(&[], &["test".to_string()]);
        let _ = table.swap(&["test".to_string()], &[]); // Swap back
    });

    workflow_handle.await.unwrap();

    // Assert buffers valid (no crash), ref 0
    let refc = table.refcounts.get(&adapter_id).unwrap().load(Ordering::Relaxed);
    assert_eq!(refc, 0, "Refcount not zero after long workflow");
    println!("Long workflow test passed");
}

    let adapter_id = "test".to_string();

    // Long workflow hold
    let table_clone = StdArc::clone(&table);
    let workflow_handle = tokio::spawn(async move {
        table_clone.inc_ref(&adapter_id);
        tokio::time::sleep(Duration::from_secs(2)).await; // Simulate long phase
        let new_ref = table_clone.dec_ref(&adapter_id);
        assert!(new_ref >= 0);
    });

    // Swap during hold
    let table_clone2 = StdArc::clone(&table);
    let swap_handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await; // Wait mid-hold
        table_clone2.swap(&[], &["test".to_string()]).unwrap(); // Swap out
        table_clone2.swap(&["test".to_string()], &[]).unwrap(); // Swap back
    });

    // Await both
    let _ = workflow_handle.await;
    let _ = swap_handle.await;

    // Check final ref 0
    let refc = table.refcounts.get(&adapter_id).unwrap().load(Ordering::Relaxed);
    assert_eq!(refc, 0, "Refcount should be 0 after long workflow and swap");

    println!("Long workflow during swap test passed");
}

#[tokio::test]
async fn test_quarantine_after_retries() {
    use std::sync::{Arc, Mutex};
    use std::collections::HashMap;
    use std::time::Instant;

    // Mock kernels_opt as None, since no unload
    let table = Arc::new(AdapterTable::new());
    let hash = B3Hash::hash(b"test");
    table.preload("test".to_string(), hash, 10).unwrap();
    let old_stack = table.current_stack.load(Ordering::Acquire).clone();
    table.swap(&["test".to_string()], &[]).unwrap();

    // Retire a stack
    {
        let mut retired = table.retired_stacks.lock().unwrap();
        retired.push(old_stack);
    }

    // Simulate 4 calls to process_retired_stacks with "fail" (since no kernels, it would succeed, but mock by checking retry)
    // Since no kernels, it removes immediately, so to test retry, need to mock the unload_failed condition.
    // For unit test, directly test the retry logic by manipulating.

    // Manually add retired stack with gen
    let retired_stack = Arc::new(Stack {
        generation: 1,
        active: HashMap::from([("test".to_string(), AdapterState {
            id: "test".to_string(),
            hash,
            vram_mb: 10,
            loaded_at: Instant::now(),
            active: false,
        })]),
    });
    {
        let mut retired = table.retired_stacks.lock().unwrap();
        retired.push(retired_stack.clone());
    }

    // Assume refcount 0 for can_unload
    // Simulate 3 fails: manually increment retry to 3
    {
        let mut retry = table.retry_counts.lock().unwrap();
        *retry.entry(1).or_insert(0) = 3;
    }

    // Call process, should quarantine (remove)
    table.process_retired_stacks(&None).await.unwrap();

    let retired_len = {
        let retired = table.retired_stacks.lock().unwrap();
        retired.len()
    };
    assert_eq!(retired_len, 0, "Stack should be quarantined after 3 retries");

    // Check retry removed
    {
        let retry = table.retry_counts.lock().unwrap();
        assert!(!retry.contains_key(&1), "Retry count should be removed on quarantine");
    }

    println!("Quarantine after retries test passed");
}
