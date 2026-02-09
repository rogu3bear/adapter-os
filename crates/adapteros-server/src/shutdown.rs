//! Shutdown coordinator for graceful lifecycle management
//!
//! Canonical shutdown coordination lives in `adapteros-server-api` (`lifecycle.rs`).
//! This module re-exports the coordinator types and provides server-specific helpers
//! (boot-state degradation and Axum drain handling).

pub use adapteros_server_api::lifecycle::{
    ShutdownConfig, ShutdownCoordinator, ShutdownError, ShutdownProgress, ShutdownStatus,
};

use adapteros_deterministic_exec::select::select_3;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::state::BackgroundTaskTracker;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

/// Check for critical background task failures and degrade boot state if any are found.
///
/// Returns `true` if the boot state was degraded, `false` if all tasks are healthy.
pub async fn apply_background_task_degraded(
    boot_state: &BootStateManager,
    background_tasks: &BackgroundTaskTracker,
) -> bool {
    let critical_failures = background_tasks.critical_failures();
    if critical_failures.is_empty() {
        return false;
    }

    let failed_names: Vec<String> = critical_failures
        .iter()
        .map(|failure| failure.name.clone())
        .collect();
    let reason = format!(
        "critical background tasks failed to spawn: {}",
        failed_names.join(", ")
    );

    warn!(
        tasks = ?failed_names,
        "Critical background tasks failed to spawn; boot state degraded"
    );
    boot_state
        .degrade_component("background-tasks", &reason)
        .await;
    true
}

/// Graceful shutdown handler for Axum HTTP server.
///
/// Waits for Ctrl+C (SIGINT), SIGTERM, or an internal shutdown signal, then:
/// 1. Transitions boot state to draining
/// 2. Waits for in-flight requests to complete (with timeout)
/// 3. Transitions boot state to stopping
pub async fn shutdown_signal_with_drain(
    boot_state: BootStateManager,
    in_flight_requests: Arc<AtomicUsize>,
    drain_timeout: Duration,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    let ctrl_c = async {
        match signal::ctrl_c().await {
            Ok(()) => {}
            Err(e) => {
                error!(
                    error = %e,
                    "Failed to install Ctrl+C handler, shutdown may not work as expected"
                );
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => {
                warn!(
                    error = %e,
                    "Failed to install SIGTERM handler, will only respond to Ctrl+C"
                );
                // Return immediately so ctrl_c handler can still work
                // In this case, SIGTERM won't trigger shutdown, but Ctrl+C will
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    let internal = async {
        loop {
            match shutdown_rx.recv().await {
                Ok(()) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    // Use deterministic select instead of tokio::select!
    // Priority: ctrl_c -> terminate -> internal signal
    let _ = select_3(ctrl_c, terminate, internal).await;

    info!("Shutdown signal received");

    // Transition to draining state
    boot_state.drain().await;

    // Wait for in-flight requests to complete (with timeout)
    let start = tokio::time::Instant::now();
    let mut logged_waiting = false;
    let mut sample_count = 0u64;
    let mut total_in_flight = 0u64;
    let mut peak_in_flight = 0usize;

    loop {
        let count = in_flight_requests.load(Ordering::SeqCst);

        // Track statistics for drain analysis
        sample_count += 1;
        total_in_flight += count as u64;
        peak_in_flight = peak_in_flight.max(count);

        if count == 0 {
            info!("All in-flight requests completed");
            break;
        }

        if !logged_waiting {
            info!(
                in_flight = count,
                timeout_secs = drain_timeout.as_secs(),
                "Waiting for in-flight requests to complete"
            );
            logged_waiting = true;
        }

        let elapsed = start.elapsed();
        if elapsed >= drain_timeout {
            // Calculate average in-flight requests during drain
            let avg_in_flight = if sample_count > 0 {
                total_in_flight as f64 / sample_count as f64
            } else {
                0.0
            };

            error!(
                in_flight_current = count,
                in_flight_peak = peak_in_flight,
                in_flight_avg = format!("{:.2}", avg_in_flight),
                elapsed_secs = elapsed.as_secs(),
                timeout_secs = drain_timeout.as_secs(),
                sample_count,
                "Drain timeout exceeded - incomplete operations detected"
            );

            // Log detailed recovery instructions
            error!(
                "MANUAL RECOVERY REQUIRED: {} requests did not complete within {}s drain timeout. \
                 Check application logs for long-running operations. \
                 Peak in-flight: {}, Average: {:.2}. \
                 Consider investigating: database locks, slow network I/O, or stuck async tasks.",
                count,
                drain_timeout.as_secs(),
                peak_in_flight,
                avg_in_flight
            );

            break;
        }

        // Check every 100ms
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Transition to stopping state
    boot_state.stop().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_apply_background_task_degraded_no_failures() {
        let boot_state = BootStateManager::new();
        let tracker = BackgroundTaskTracker::default();

        let result = apply_background_task_degraded(&boot_state, &tracker).await;
        assert!(!result, "Expected false when no critical failures exist");
    }

    #[tokio::test]
    async fn test_apply_background_task_degraded_with_critical_failure() {
        let boot_state = BootStateManager::new();
        boot_state.ready().await;

        let tracker = BackgroundTaskTracker::default();
        tracker.record_failed("critical-task", "spawn failed", true);

        let result = apply_background_task_degraded(&boot_state, &tracker).await;
        assert!(result, "Expected true when critical failures exist");
    }

    #[tokio::test]
    async fn test_apply_background_task_degraded_ignores_non_critical() {
        let boot_state = BootStateManager::new();
        let tracker = BackgroundTaskTracker::default();
        tracker.record_failed("optional-task", "spawn failed", false);

        let result = apply_background_task_degraded(&boot_state, &tracker).await;
        assert!(
            !result,
            "Expected false when only non-critical failures exist"
        );
    }

    #[tokio::test]
    async fn test_drain_timeout_logic() {
        let in_flight_requests = Arc::new(AtomicUsize::new(5));
        let drain_timeout = Duration::from_millis(50);

        let start = tokio::time::Instant::now();
        let mut iterations = 0u32;

        // Simulate the drain loop (requests never complete)
        loop {
            let count = in_flight_requests.load(Ordering::SeqCst);
            iterations += 1;

            if count == 0 {
                break;
            }
            if start.elapsed() >= drain_timeout {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let elapsed = start.elapsed();

        // Verify timeout was respected (with some tolerance)
        assert!(
            elapsed >= drain_timeout,
            "Should have waited at least the drain timeout"
        );
        assert!(
            elapsed < drain_timeout + Duration::from_millis(50),
            "Should not have waited much longer than timeout"
        );
        assert!(iterations > 1, "Should have done multiple iterations");
    }
}
