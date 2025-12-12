//! Tests for SingleFlight and SingleFlightSync.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{SingleFlight, SingleFlightMetrics, SingleFlightSync};

/// Test metrics implementation that records calls.
#[derive(Default)]
struct TestMetrics {
    leaders: AtomicUsize,
    waiters: AtomicUsize,
    errors: AtomicUsize,
}

impl SingleFlightMetrics for TestMetrics {
    fn record_leader(&self, _operation: &str) {
        self.leaders.fetch_add(1, Ordering::SeqCst);
    }

    fn record_waiter(&self, _operation: &str) {
        self.waiters.fetch_add(1, Ordering::SeqCst);
    }

    fn set_waiter_gauge(&self, _operation: &str, _count: usize) {}

    fn record_error(&self, _operation: &str, _error_type: &str) {
        self.errors.fetch_add(1, Ordering::SeqCst);
    }
}

// ============================================================================
// Async SingleFlight Tests
// ============================================================================

/// PRD-mandated test: Concurrent model loads run loader only once.
#[tokio::test]
async fn test_singleflight_model_load_concurrent_miss_runs_loader_once() {
    let sf = Arc::new(SingleFlight::<String, u64, String>::new("model_load"));
    let load_count = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    // Spawn 10 concurrent requests for the same key
    for _ in 0..10 {
        let sf = sf.clone();
        let count = load_count.clone();
        handles.push(tokio::spawn(async move {
            sf.get_or_load("model-123".to_string(), || async move {
                count.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(50)).await;
                Ok(42u64)
            })
            .await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should succeed with the same value
    for result in &results {
        let value = result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(*value, 42);
    }

    // Loader should have been called exactly once
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        1,
        "loader should run exactly once"
    );
}

/// PRD-mandated test: Concurrent prefix KV builds run builder only once.
#[tokio::test]
async fn test_singleflight_prefix_kv_build_concurrent_miss_runs_builder_once() {
    // Use a B3Hash-like type (represented as [u8; 32])
    let sf = Arc::new(SingleFlight::<[u8; 32], Vec<f32>, String>::new("prefix_kv_build"));
    let build_count = Arc::new(AtomicUsize::new(0));

    let key = [0u8; 32]; // Simulated B3Hash

    let mut handles = vec![];

    // Spawn 8 concurrent requests for the same key
    for _ in 0..8 {
        let sf = sf.clone();
        let count = build_count.clone();
        let key = key;
        handles.push(tokio::spawn(async move {
            sf.get_or_load(key, || async move {
                count.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(vec![1.0, 2.0, 3.0])
            })
            .await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should succeed
    for result in &results {
        let value = result.as_ref().unwrap().as_ref().unwrap();
        assert_eq!(*value, vec![1.0, 2.0, 3.0]);
    }

    // Builder should have been called exactly once
    assert_eq!(
        build_count.load(Ordering::SeqCst),
        1,
        "builder should run exactly once"
    );
}

/// PRD-mandated test: Error propagation without cache poisoning.
#[tokio::test]
async fn test_singleflight_error_propagation_no_poisoned_cache() {
    let sf = Arc::new(SingleFlight::<String, u64, String>::new("error_test"));
    let load_count = Arc::new(AtomicUsize::new(0));

    // First batch: all should fail together
    let mut handles = vec![];
    for _ in 0..5 {
        let sf = sf.clone();
        let count = load_count.clone();
        handles.push(tokio::spawn(async move {
            sf.get_or_load("failing-key".to_string(), || async move {
                count.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                Err("test error".to_string())
            })
            .await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should receive the error
    for result in &results {
        let err = result.as_ref().unwrap().as_ref().unwrap_err();
        assert_eq!(err, "test error");
    }

    // Loader should have been called exactly once
    assert_eq!(load_count.load(Ordering::SeqCst), 1);

    // Second attempt: should succeed (no poisoned cache)
    let load_count2 = Arc::new(AtomicUsize::new(0));
    let count2 = load_count2.clone();
    let result = sf
        .get_or_load("failing-key".to_string(), || async move {
            count2.fetch_add(1, Ordering::SeqCst);
            Ok(42u64)
        })
        .await;

    assert_eq!(result.unwrap(), 42);
    assert_eq!(
        load_count2.load(Ordering::SeqCst),
        1,
        "retry should run fresh"
    );
}

/// Test that different keys run in parallel.
#[tokio::test]
async fn test_singleflight_different_keys_run_parallel() {
    let sf = Arc::new(SingleFlight::<String, u64, String>::new("parallel_test"));
    let load_count = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    // Spawn requests for different keys
    for i in 0..5 {
        let sf = sf.clone();
        let count = load_count.clone();
        let key = format!("key-{}", i);
        handles.push(tokio::spawn(async move {
            sf.get_or_load(key, || async move {
                count.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(10)).await;
                Ok(i as u64)
            })
            .await
        }));
    }

    let results: Vec<_> = futures::future::join_all(handles).await;

    // All should succeed
    for result in &results {
        assert!(result.as_ref().unwrap().is_ok());
    }

    // Each key should trigger its own load
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        5,
        "each unique key should trigger a load"
    );
}

/// Test metrics recording.
#[tokio::test]
async fn test_singleflight_metrics_recording() {
    let metrics = Arc::new(TestMetrics::default());
    let sf = Arc::new(SingleFlight::<String, u64, String>::with_metrics(
        "metrics_test",
        metrics.clone(),
    ));

    let mut handles = vec![];

    // Spawn 5 concurrent requests
    for _ in 0..5 {
        let sf = sf.clone();
        handles.push(tokio::spawn(async move {
            sf.get_or_load("key".to_string(), || async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(42u64)
            })
            .await
        }));
    }

    let _ = futures::future::join_all(handles).await;

    // Should have 1 leader and 4 waiters
    assert_eq!(metrics.leaders.load(Ordering::SeqCst), 1);
    assert_eq!(metrics.waiters.load(Ordering::SeqCst), 4);
    assert_eq!(metrics.errors.load(Ordering::SeqCst), 0);
}

/// Test is_loading and waiter_count.
#[tokio::test]
async fn test_singleflight_is_loading() {
    let sf = Arc::new(SingleFlight::<String, u64, String>::new("loading_test"));

    assert!(!sf.is_loading(&"key".to_string()));
    assert_eq!(sf.waiter_count(&"key".to_string()), 0);

    let sf_clone = sf.clone();
    let handle = tokio::spawn(async move {
        sf_clone
            .get_or_load("key".to_string(), || async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(42u64)
            })
            .await
    });

    // Wait a bit for the load to start
    tokio::time::sleep(Duration::from_millis(10)).await;

    assert!(sf.is_loading(&"key".to_string()));
    assert_eq!(sf.waiter_count(&"key".to_string()), 1);

    let _ = handle.await;

    assert!(!sf.is_loading(&"key".to_string()));
    assert_eq!(sf.waiter_count(&"key".to_string()), 0);
}

/// Test stats snapshot.
#[tokio::test]
async fn test_singleflight_stats() {
    let sf = Arc::new(SingleFlight::<String, u64, String>::new("stats_test"));

    let stats = sf.stats();
    assert_eq!(stats.pending_loads, 0);
    assert_eq!(stats.total_waiters, 0);

    let sf_clone = sf.clone();
    let _handle = tokio::spawn(async move {
        sf_clone
            .get_or_load("key".to_string(), || async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(42u64)
            })
            .await
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    let stats = sf.stats();
    assert_eq!(stats.pending_loads, 1);
    assert!(stats.total_waiters >= 1);
}

// ============================================================================
// Sync SingleFlightSync Tests
// ============================================================================

/// Test sync variant: concurrent loads run loader only once.
#[test]
fn test_singleflight_sync_concurrent_loads() {
    use std::thread;

    let sf = Arc::new(SingleFlightSync::<String, u64, String>::new("sync_test"));
    let load_count = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    // Spawn 8 concurrent threads
    for _ in 0..8 {
        let sf = sf.clone();
        let count = load_count.clone();
        handles.push(thread::spawn(move || {
            sf.get_or_load("key".to_string(), || {
                count.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(50));
                Ok(42u64)
            })
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All should succeed with the same value
    for result in &results {
        assert_eq!(*result.as_ref().unwrap(), 42);
    }

    // Loader should have been called exactly once
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        1,
        "loader should run exactly once"
    );
}

/// Test sync variant: error propagation.
#[test]
fn test_singleflight_sync_error_propagation() {
    use std::thread;

    let sf = Arc::new(SingleFlightSync::<String, u64, String>::new("sync_error_test"));

    let mut handles = vec![];

    // First batch: all fail
    for _ in 0..5 {
        let sf = sf.clone();
        handles.push(thread::spawn(move || {
            sf.get_or_load("key".to_string(), || {
                std::thread::sleep(Duration::from_millis(20));
                Err("sync error".to_string())
            })
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All should receive the error
    for result in &results {
        assert_eq!(result.as_ref().unwrap_err(), "sync error");
    }

    // Second attempt should succeed (no poisoning)
    let result = sf.get_or_load("key".to_string(), || Ok(42u64));
    assert_eq!(result.unwrap(), 42);
}

/// Test sync variant: different keys run in parallel.
#[test]
fn test_singleflight_sync_different_keys() {
    use std::thread;

    let sf = Arc::new(SingleFlightSync::<String, u64, String>::new("sync_parallel"));
    let load_count = Arc::new(AtomicUsize::new(0));

    let mut handles = vec![];

    for i in 0..5 {
        let sf = sf.clone();
        let count = load_count.clone();
        let key = format!("key-{}", i);
        handles.push(thread::spawn(move || {
            sf.get_or_load(key, || {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(i as u64)
            })
        }));
    }

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    for result in &results {
        assert!(result.is_ok());
    }

    assert_eq!(load_count.load(Ordering::SeqCst), 5);
}

/// Test bounded memory: entries are removed after completion.
#[tokio::test]
async fn test_singleflight_bounded_memory() {
    let sf = Arc::new(SingleFlight::<String, u64, String>::new("bounded_test"));

    // Run several loads
    for i in 0..10 {
        let key = format!("key-{}", i);
        let _ = sf
            .get_or_load(key, || async move { Ok(i as u64) })
            .await;
    }

    // All entries should be cleaned up
    let stats = sf.stats();
    assert_eq!(stats.pending_loads, 0, "all entries should be cleaned up");
}

/// Test sequential loads for the same key.
#[tokio::test]
async fn test_singleflight_sequential_loads() {
    let sf = SingleFlight::<String, u64, String>::new("sequential_test");
    let load_count = Arc::new(AtomicUsize::new(0));

    // First load
    let count1 = load_count.clone();
    let result1 = sf
        .get_or_load("key".to_string(), || async move {
            count1.fetch_add(1, Ordering::SeqCst);
            Ok(1u64)
        })
        .await;
    assert_eq!(result1.unwrap(), 1);

    // Second load (sequential, not concurrent - should run again)
    let count2 = load_count.clone();
    let result2 = sf
        .get_or_load("key".to_string(), || async move {
            count2.fetch_add(1, Ordering::SeqCst);
            Ok(2u64)
        })
        .await;
    assert_eq!(result2.unwrap(), 2);

    // Both loads should have executed
    assert_eq!(
        load_count.load(Ordering::SeqCst),
        2,
        "sequential loads should each execute"
    );
}

// ============================================================================
// Panic Safety Tests
// ============================================================================

/// Test sync variant: leader panic propagates to waiters.
#[test]
fn test_singleflight_sync_leader_panic_propagates() {
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::thread;

    let sf = Arc::new(SingleFlightSync::<String, u64, String>::new("panic_test"));
    let started = Arc::new(std::sync::Barrier::new(2));

    // Spawn a waiter that will block
    let sf_waiter = sf.clone();
    let started_waiter = started.clone();
    let waiter_handle = thread::spawn(move || {
        // Wait for leader to start
        started_waiter.wait();
        // Small delay to ensure leader is running
        thread::sleep(Duration::from_millis(10));
        // This will wait for leader, which will panic
        catch_unwind(AssertUnwindSafe(|| {
            sf_waiter.get_or_load("key".to_string(), || {
                // This shouldn't run - we're a waiter
                Ok(999u64)
            })
        }))
    });

    // Leader thread that panics
    let sf_leader = sf.clone();
    let started_leader = started.clone();
    let leader_handle = thread::spawn(move || {
        started_leader.wait();
        catch_unwind(AssertUnwindSafe(|| {
            sf_leader.get_or_load("key".to_string(), || {
                // Simulate long operation that panics
                thread::sleep(Duration::from_millis(50));
                panic!("intentional leader panic");
            })
        }))
    });

    // Wait for both threads
    let leader_result = leader_handle.join().expect("leader thread should not abort");
    let waiter_result = waiter_handle.join().expect("waiter thread should not abort");

    // Leader should have panicked
    assert!(leader_result.is_err(), "leader should have panicked");

    // Waiter should also have panicked (propagated)
    assert!(
        waiter_result.is_err(),
        "waiter should have received propagated panic"
    );

    // Entry should be cleaned up
    assert!(
        !sf.is_loading(&"key".to_string()),
        "entry should be cleaned up after panic"
    );
}

/// Test sync variant: panic doesn't poison the cache for future loads.
#[test]
fn test_singleflight_sync_panic_no_poison() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let sf = SingleFlightSync::<String, u64, String>::new("panic_no_poison");

    // First load: panic
    let result1 = catch_unwind(AssertUnwindSafe(|| {
        sf.get_or_load("key".to_string(), || {
            panic!("intentional panic");
        })
    }));
    assert!(result1.is_err(), "first load should panic");

    // Second load: should succeed (no poisoning)
    let result2 = sf.get_or_load("key".to_string(), || Ok(42u64));
    assert_eq!(result2.unwrap(), 42, "second load should succeed");
}

/// Test bounded memory after panic: entry is cleaned up.
#[test]
fn test_singleflight_sync_panic_cleanup() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let sf = SingleFlightSync::<String, u64, String>::new("panic_cleanup");

    // Cause a panic
    let _ = catch_unwind(AssertUnwindSafe(|| {
        sf.get_or_load("key".to_string(), || {
            panic!("intentional panic");
        })
    }));

    // Entry should be cleaned up
    let stats = sf.stats();
    assert_eq!(stats.pending_loads, 0, "no pending loads after panic");
    assert_eq!(stats.total_waiters, 0, "no waiters after panic");
}
