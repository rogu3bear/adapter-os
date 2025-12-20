//! Integration tests for drain timeout behavior during graceful shutdown.
//!
//! ## Test Coverage
//!
//! This test suite validates the drain timeout logic used in `shutdown_signal_with_drain`
//! from `adapteros-server/src/main.rs`. It covers:
//!
//! 1. **Timeout Enforcement**: Verifies that the drain timeout is respected when in-flight
//!    requests don't complete within the configured timeout period.
//!
//! 2. **Statistics Tracking**: Validates that peak in-flight count, average in-flight count,
//!    and sample count are calculated correctly during the drain period.
//!
//! 3. **Graceful Completion**: Ensures that drain completes successfully when all in-flight
//!    requests finish before the timeout.
//!
//! 4. **Edge Cases**: Tests zero in-flight requests, fluctuating loads, and various boot
//!    state transitions (Ready → Draining, FullyReady → Draining, Maintenance → Draining).
//!
//! ## Implementation Notes
//!
//! The `simulate_drain_with_timeout` function replicates the core drain logic from main.rs:
//! - Polls in-flight request count every 10ms
//! - Tracks statistics (peak, average, sample count)
//! - Respects timeout boundary
//! - Returns diagnostic information for verification
//!
//! ## Recovery Behavior
//!
//! When the timeout is exceeded with incomplete requests, the production code logs:
//! - Current, peak, and average in-flight request counts
//! - Sample count and timeout duration
//! - Manual recovery instructions (check for database locks, slow I/O, stuck async tasks)
//!
//! These tests verify the statistics are calculated correctly so operators receive
//! accurate diagnostic information during incident recovery.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use adapteros_server_api::boot_state::{BootState, BootStateManager};

/// Simulates the drain timeout logic from `shutdown_signal_with_drain` in main.rs
/// Returns (timeout_exceeded, peak_in_flight, avg_in_flight, sample_count)
async fn simulate_drain_with_timeout(
    in_flight_requests: Arc<AtomicUsize>,
    drain_timeout: Duration,
) -> (bool, usize, f64, u64) {
    let start = tokio::time::Instant::now();
    let mut sample_count = 0u64;
    let mut total_in_flight = 0u64;
    let mut peak_in_flight = 0usize;
    let mut timeout_exceeded = false;

    loop {
        let count = in_flight_requests.load(Ordering::SeqCst);

        // Track statistics
        sample_count += 1;
        total_in_flight += count as u64;
        peak_in_flight = peak_in_flight.max(count);

        // Check if all requests completed
        if count == 0 {
            break;
        }

        // Check timeout
        let elapsed = start.elapsed();
        if elapsed >= drain_timeout {
            timeout_exceeded = true;
            break;
        }

        // Poll interval
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let avg_in_flight = if sample_count > 0 {
        total_in_flight as f64 / sample_count as f64
    } else {
        0.0
    };

    (
        timeout_exceeded,
        peak_in_flight,
        avg_in_flight,
        sample_count,
    )
}

#[tokio::test]
async fn drain_timeout_is_respected_when_requests_dont_complete() {
    let boot_state = BootStateManager::new();
    let in_flight_requests = Arc::new(AtomicUsize::new(0));

    // Transition to Ready state first
    boot_state.boot().await;
    boot_state.init_db().await;
    boot_state.load_policies().await;
    boot_state.start_backend().await;
    boot_state.load_base_models().await;
    boot_state.load_adapters().await;
    boot_state.ready().await;
    assert_eq!(boot_state.current_state(), BootState::Ready);

    // Simulate shutdown signal
    boot_state.drain().await;
    assert_eq!(boot_state.current_state(), BootState::Draining);

    // Simulate stuck requests - set to 3 requests that never complete
    in_flight_requests.store(3, Ordering::SeqCst);

    // Use a short timeout for testing (100ms)
    let drain_timeout = Duration::from_millis(100);

    let (timeout_exceeded, peak_in_flight, avg_in_flight, sample_count) =
        simulate_drain_with_timeout(Arc::clone(&in_flight_requests), drain_timeout).await;

    // Verify timeout was exceeded
    assert!(
        timeout_exceeded,
        "Drain timeout should be exceeded when requests don't complete"
    );

    // Verify statistics were calculated
    assert_eq!(
        peak_in_flight, 3,
        "Peak in-flight should match the stuck request count"
    );
    assert!(
        avg_in_flight > 2.5,
        "Average in-flight should be close to 3 (got {:.2})",
        avg_in_flight
    );
    assert!(
        sample_count > 5,
        "Should have multiple samples during drain (got {})",
        sample_count
    );

    // Verify requests are still in-flight after timeout
    assert_eq!(
        in_flight_requests.load(Ordering::SeqCst),
        3,
        "Stuck requests should remain after timeout"
    );
}

#[tokio::test]
async fn drain_completes_successfully_when_all_requests_finish() {
    let boot_state = BootStateManager::new();
    let in_flight_requests = Arc::new(AtomicUsize::new(0));

    // Transition to Ready state
    boot_state.boot().await;
    boot_state.init_db().await;
    boot_state.load_policies().await;
    boot_state.start_backend().await;
    boot_state.load_base_models().await;
    boot_state.load_adapters().await;
    boot_state.ready().await;

    // Simulate shutdown signal
    boot_state.drain().await;
    assert_eq!(boot_state.current_state(), BootState::Draining);

    // Start with some in-flight requests
    in_flight_requests.store(5, Ordering::SeqCst);

    // Spawn task to gradually complete requests
    let in_flight_clone = Arc::clone(&in_flight_requests);
    tokio::spawn(async move {
        for i in (0..5).rev() {
            tokio::time::sleep(Duration::from_millis(10)).await;
            in_flight_clone.store(i, Ordering::SeqCst);
        }
    });

    // Use a generous timeout (should complete before timeout)
    let drain_timeout = Duration::from_secs(2);

    let (timeout_exceeded, peak_in_flight, avg_in_flight, sample_count) =
        simulate_drain_with_timeout(Arc::clone(&in_flight_requests), drain_timeout).await;

    // Verify drain completed without timeout
    assert!(
        !timeout_exceeded,
        "Drain should complete successfully without timeout"
    );

    // Verify all requests completed
    assert_eq!(
        in_flight_requests.load(Ordering::SeqCst),
        0,
        "All requests should be completed"
    );

    // Verify statistics
    assert_eq!(
        peak_in_flight, 5,
        "Peak should capture the initial request count"
    );
    assert!(
        avg_in_flight < 5.0,
        "Average should be less than peak as requests complete"
    );
    assert!(sample_count > 0, "Should have sampled during drain");
}

#[tokio::test]
async fn drain_statistics_track_request_count_accurately() {
    let in_flight_requests = Arc::new(AtomicUsize::new(0));

    // Simulate varying request counts
    in_flight_requests.store(10, Ordering::SeqCst);

    // Spawn task to vary request count over time
    let in_flight_clone = Arc::clone(&in_flight_requests);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        in_flight_clone.store(8, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        in_flight_clone.store(5, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(20)).await;
        in_flight_clone.store(3, Ordering::SeqCst);
        // Never complete - let timeout trigger
    });

    let drain_timeout = Duration::from_millis(150);

    let (timeout_exceeded, peak_in_flight, avg_in_flight, sample_count) =
        simulate_drain_with_timeout(Arc::clone(&in_flight_requests), drain_timeout).await;

    // Verify timeout occurred
    assert!(timeout_exceeded, "Should timeout with incomplete requests");

    // Verify peak captured maximum
    assert_eq!(
        peak_in_flight, 10,
        "Peak should capture initial maximum value"
    );

    // Verify average is reasonable (between min and max)
    assert!(avg_in_flight > 3.0, "Average should be above minimum value");
    assert!(
        avg_in_flight < 10.0,
        "Average should be below maximum value"
    );

    // Verify we collected enough samples
    assert!(
        sample_count >= 10,
        "Should have collected multiple samples (got {})",
        sample_count
    );
}

#[tokio::test]
async fn drain_handles_zero_in_flight_requests_immediately() {
    let boot_state = BootStateManager::new();
    let in_flight_requests = Arc::new(AtomicUsize::new(0));

    // Transition to Ready then Draining
    boot_state.boot().await;
    boot_state.init_db().await;
    boot_state.load_policies().await;
    boot_state.start_backend().await;
    boot_state.load_base_models().await;
    boot_state.load_adapters().await;
    boot_state.ready().await;
    boot_state.drain().await;

    // No in-flight requests
    in_flight_requests.store(0, Ordering::SeqCst);

    let start = tokio::time::Instant::now();
    let drain_timeout = Duration::from_secs(30);

    let (timeout_exceeded, peak_in_flight, avg_in_flight, sample_count) =
        simulate_drain_with_timeout(Arc::clone(&in_flight_requests), drain_timeout).await;

    let elapsed = start.elapsed();

    // Verify drain completed immediately
    assert!(!timeout_exceeded, "Should not timeout with zero requests");
    assert!(
        elapsed < Duration::from_millis(50),
        "Should complete almost immediately (took {:?})",
        elapsed
    );

    // Verify statistics
    assert_eq!(peak_in_flight, 0, "Peak should be 0");
    assert_eq!(avg_in_flight, 0.0, "Average should be 0.0");
    assert_eq!(sample_count, 1, "Should have exactly one sample");
}

#[tokio::test]
async fn drain_timeout_tracks_peak_correctly_with_fluctuating_load() {
    let in_flight_requests = Arc::new(AtomicUsize::new(0));

    // Start at medium load
    in_flight_requests.store(5, Ordering::SeqCst);

    let in_flight_clone = Arc::clone(&in_flight_requests);
    tokio::spawn(async move {
        // Fluctuate up and down
        tokio::time::sleep(Duration::from_millis(10)).await;
        in_flight_clone.store(12, Ordering::SeqCst); // Peak
        tokio::time::sleep(Duration::from_millis(10)).await;
        in_flight_clone.store(7, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
        in_flight_clone.store(10, Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(10)).await;
        in_flight_clone.store(2, Ordering::SeqCst);
        // Never reach zero
    });

    let drain_timeout = Duration::from_millis(100);

    let (timeout_exceeded, peak_in_flight, avg_in_flight, sample_count) =
        simulate_drain_with_timeout(Arc::clone(&in_flight_requests), drain_timeout).await;

    // Verify timeout occurred
    assert!(timeout_exceeded, "Should timeout");

    // Verify peak was captured
    assert_eq!(
        peak_in_flight, 12,
        "Peak should capture highest value during drain"
    );

    // Verify average is in reasonable range
    assert!(
        avg_in_flight >= 2.0,
        "Average should be at least minimum value"
    );
    assert!(
        avg_in_flight <= 12.0,
        "Average should be at most peak value"
    );

    // Multiple samples should have been taken
    assert!(
        sample_count >= 5,
        "Should have multiple samples during fluctuation"
    );
}

#[tokio::test]
async fn boot_state_transitions_to_stopping_after_drain() {
    let boot_state = BootStateManager::new();

    // Complete boot sequence
    boot_state.boot().await;
    boot_state.init_db().await;
    boot_state.load_policies().await;
    boot_state.start_backend().await;
    boot_state.load_base_models().await;
    boot_state.load_adapters().await;
    boot_state.ready().await;
    assert_eq!(boot_state.current_state(), BootState::Ready);

    // Drain
    boot_state.drain().await;
    assert_eq!(boot_state.current_state(), BootState::Draining);
    assert!(boot_state.is_draining());
    assert!(boot_state.is_shutting_down());

    // Stop
    boot_state.stop().await;
    assert_eq!(boot_state.current_state(), BootState::Stopping);
    assert!(boot_state.is_shutting_down());
}

#[tokio::test]
async fn drain_from_fully_ready_state_works() {
    let boot_state = BootStateManager::new();

    // Complete full boot sequence including FullyReady
    boot_state.boot().await;
    boot_state.init_db().await;
    boot_state.load_policies().await;
    boot_state.start_backend().await;
    boot_state.load_base_models().await;
    boot_state.load_adapters().await;
    boot_state.ready().await;
    boot_state.fully_ready().await;
    assert_eq!(boot_state.current_state(), BootState::FullyReady);

    // Can drain from FullyReady
    boot_state.drain().await;
    assert_eq!(boot_state.current_state(), BootState::Draining);
}

#[tokio::test]
async fn drain_from_maintenance_state_works() {
    let boot_state = BootStateManager::new();

    // Boot to Ready then enter Maintenance
    boot_state.boot().await;
    boot_state.init_db().await;
    boot_state.load_policies().await;
    boot_state.start_backend().await;
    boot_state.load_base_models().await;
    boot_state.load_adapters().await;
    boot_state.ready().await;
    boot_state.maintenance("testing-maintenance").await;
    assert_eq!(boot_state.current_state(), BootState::Maintenance);

    // Can drain from Maintenance
    boot_state.drain().await;
    assert_eq!(boot_state.current_state(), BootState::Draining);
}
