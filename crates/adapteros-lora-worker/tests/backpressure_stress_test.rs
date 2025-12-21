//! Stress test for worker backpressure fast-fail behavior
//!
//! Verifies that under overload:
//! - Some requests succeed (up to max_concurrent)
//! - Excess requests fail fast with HTTP 503
//! - No long-tail timeouts
//! - Stats are accurate

use adapteros_lora_worker::backpressure::{BackpressureGate, DEFAULT_MAX_CONCURRENT};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Barrier;

/// Test that the backpressure gate correctly rejects excess requests immediately
#[tokio::test]
async fn test_backpressure_gate_fast_fail() {
    let gate = Arc::new(BackpressureGate::new(8));
    let concurrent_requests = 100;
    let barrier = Arc::new(Barrier::new(concurrent_requests));

    let admitted = Arc::new(AtomicUsize::new(0));
    let rejected = Arc::new(AtomicUsize::new(0));
    let max_latency_us = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();

    for _ in 0..concurrent_requests {
        let gate = Arc::clone(&gate);
        let barrier = Arc::clone(&barrier);
        let admitted = Arc::clone(&admitted);
        let rejected = Arc::clone(&rejected);
        let max_latency_us = Arc::clone(&max_latency_us);

        handles.push(tokio::spawn(async move {
            // Synchronize all tasks to start together for maximum contention
            barrier.wait().await;

            let start = Instant::now();

            if let Some(_permit) = gate.try_acquire() {
                admitted.fetch_add(1, Ordering::Relaxed);
                // Simulate work - hold the permit for 50ms
                tokio::time::sleep(Duration::from_millis(50)).await;
            } else {
                rejected.fetch_add(1, Ordering::Relaxed);
            }

            let latency_us = start.elapsed().as_micros() as usize;
            // Update max latency atomically
            let mut current = max_latency_us.load(Ordering::Relaxed);
            while latency_us > current {
                match max_latency_us.compare_exchange_weak(
                    current,
                    latency_us,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(c) => current = c,
                }
            }
        }));
    }

    // Wait for all tasks to complete
    for handle in handles {
        handle.await.expect("Task should complete successfully");
    }

    let stats = gate.stats();
    let admitted_count = admitted.load(Ordering::Relaxed);
    let rejected_count = rejected.load(Ordering::Relaxed);
    let max_latency = Duration::from_micros(max_latency_us.load(Ordering::Relaxed) as u64);

    // Verify some requests were admitted
    assert!(admitted_count > 0, "Some requests should be admitted");
    assert!(
        admitted_count <= 8,
        "Should not exceed max_concurrent (8), got {}",
        admitted_count
    );

    // Verify excess requests were rejected
    assert!(rejected_count > 0, "Excess requests should be rejected");
    assert_eq!(
        admitted_count + rejected_count,
        concurrent_requests,
        "Total should equal concurrent_requests"
    );

    // Verify fast-fail - rejected requests should return immediately
    // Max latency should be around the work duration (50ms) + small overhead
    // Not 500ms+ which would indicate queuing/waiting
    assert!(
        max_latency < Duration::from_millis(200),
        "Fast-fail should prevent long tail latencies: {:?}",
        max_latency
    );

    // Verify stats match observed behavior
    assert_eq!(
        stats.admitted_count as usize, admitted_count,
        "Stats admitted count should match"
    );
    assert_eq!(
        stats.rejected_count as usize, rejected_count,
        "Stats rejected count should match"
    );

    println!(
        "Results: Admitted={}, Rejected={}, MaxLatency={:?}",
        admitted_count, rejected_count, max_latency
    );
}

/// Test that permits are correctly released and reused
#[tokio::test]
async fn test_backpressure_gate_permit_release_and_reuse() {
    let gate = Arc::new(BackpressureGate::new(2));

    // First wave - acquire all permits
    let permit1 = gate.try_acquire().expect("Should acquire first permit");
    let permit2 = gate.try_acquire().expect("Should acquire second permit");
    assert_eq!(gate.in_flight(), 2);

    // Third should be rejected
    assert!(
        gate.try_acquire().is_none(),
        "Third permit should be rejected when full"
    );
    assert_eq!(gate.stats().rejected_count, 1);

    // Release one permit
    drop(permit1);
    assert_eq!(gate.in_flight(), 1);

    // Now should be able to acquire again
    let permit3 = gate.try_acquire().expect("Should acquire after release");
    assert_eq!(gate.in_flight(), 2);

    // Second wave - release all and verify clean state
    drop(permit2);
    drop(permit3);
    assert_eq!(gate.in_flight(), 0);

    // Should be able to acquire max again
    let _p1 = gate
        .try_acquire()
        .expect("Should acquire after full release");
    let _p2 = gate
        .try_acquire()
        .expect("Should acquire after full release");
    assert_eq!(gate.in_flight(), 2);
}

/// Test stats accuracy under concurrent access
#[tokio::test]
async fn test_backpressure_stats_accuracy() {
    let gate = Arc::new(BackpressureGate::new(4));

    // Initial state
    let stats = gate.stats();
    assert_eq!(stats.in_flight, 0);
    assert_eq!(stats.max_concurrent, 4);
    assert_eq!(stats.admitted_count, 0);
    assert_eq!(stats.rejected_count, 0);
    assert_eq!(stats.utilization_percent(), 0.0);

    // Acquire some permits
    let _p1 = gate.try_acquire();
    let _p2 = gate.try_acquire();

    let stats = gate.stats();
    assert_eq!(stats.in_flight, 2);
    assert_eq!(stats.admitted_count, 2);
    assert_eq!(stats.utilization_percent(), 50.0);

    // Fill up and trigger rejection
    let _p3 = gate.try_acquire();
    let _p4 = gate.try_acquire();
    let _ = gate.try_acquire(); // Should be rejected

    let stats = gate.stats();
    assert_eq!(stats.in_flight, 4);
    assert_eq!(stats.admitted_count, 4);
    assert_eq!(stats.rejected_count, 1);
    assert_eq!(stats.utilization_percent(), 100.0);
}

/// Test suggested retry delay scaling with load
#[test]
fn test_suggested_retry_ms_scaling() {
    let gate = Arc::new(BackpressureGate::new(10));

    // At zero load - base delay
    let retry_low = gate.suggested_retry_ms();
    assert!(retry_low >= 100, "Base delay should be at least 100ms");
    assert!(retry_low < 150, "At zero load, delay should be near base");

    // Simulate high load by acquiring permits
    let mut permits = Vec::new();
    for _ in 0..10 {
        if let Some(p) = gate.try_acquire() {
            permits.push(p);
        }
    }

    let retry_high = gate.suggested_retry_ms();
    assert!(
        retry_high > retry_low,
        "Retry delay should increase with load: {} vs {}",
        retry_high,
        retry_low
    );
    assert!(
        retry_high <= 300,
        "Max retry should be around 300ms, got {}",
        retry_high
    );
}

/// Test default configuration matches expected values
#[test]
fn test_default_configuration() {
    assert_eq!(
        DEFAULT_MAX_CONCURRENT, 8,
        "Default max concurrent should be 8"
    );

    let gate = BackpressureGate::default();
    assert_eq!(gate.max_concurrent(), 8);
}

/// Test rejection rate calculation
#[test]
fn test_rejection_rate_calculation() {
    let gate = Arc::new(BackpressureGate::new(1));

    // Acquire the only permit
    let _permit = gate.try_acquire();

    // Generate some rejections
    for _ in 0..9 {
        let _ = gate.try_acquire();
    }

    let stats = gate.stats();
    assert_eq!(stats.admitted_count, 1);
    assert_eq!(stats.rejected_count, 9);
    assert_eq!(stats.rejection_rate_percent(), 90.0);
}

/// Test concurrent access to stats doesn't cause data races
#[tokio::test]
async fn test_concurrent_stats_access() {
    let gate = Arc::new(BackpressureGate::new(4));
    let barrier = Arc::new(Barrier::new(20));

    let mut handles = Vec::new();

    for _ in 0..20 {
        let gate = Arc::clone(&gate);
        let barrier = Arc::clone(&barrier);

        handles.push(tokio::spawn(async move {
            barrier.wait().await;

            // Mix of operations
            if let Some(permit) = gate.try_acquire() {
                let _ = gate.stats();
                tokio::time::sleep(Duration::from_millis(10)).await;
                let _ = gate.stats();
                drop(permit);
            } else {
                let _ = gate.stats();
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Task should complete");
    }

    // Just verify no panics occurred and stats are consistent
    let stats = gate.stats();
    assert!(
        stats.admitted_count + stats.rejected_count >= 20,
        "Should have processed at least 20 requests"
    );
}
