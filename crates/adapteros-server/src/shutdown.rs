//! Shutdown coordinator for graceful lifecycle management
//!
//! Canonical shutdown coordination lives in `adapteros-server-api` (`lifecycle.rs`).
//! This module re-exports the coordinator types and provides server-specific helpers
//! (boot-state degradation and Axum drain handling).

pub use adapteros_server_api::lifecycle::{
    ShutdownConfig, ShutdownCoordinator, ShutdownError, ShutdownProgress, ShutdownStatus,
};

use adapteros_core::drain::{
    phase_for_elapsed, should_emit_warning_sample, DrainPhase, DrainPhaseConfig, DrainStats,
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

// ---------------------------------------------------------------------------
// Shutdown handler
// ---------------------------------------------------------------------------

/// Graceful shutdown handler for Axum HTTP server.
///
/// Waits for Ctrl+C (SIGINT), SIGTERM, or an internal shutdown signal, then
/// proceeds through a graduated drain escalation:
///
/// | Phase    | Default Window | Behaviour                                      |
/// |----------|---------------|------------------------------------------------|
/// | Graceful | 0 – 15 s      | Stop accepting requests, drain in-flight       |
/// | Warning  | 15 – 25 s     | Log remaining request count and ages           |
/// | Notify   | 25 – 30 s     | Broadcast shutdown to SSE streams              |
/// | Force    | 30 s          | Force-cancel remaining requests, log each      |
pub async fn shutdown_signal_with_drain(
    boot_state: BootStateManager,
    in_flight_requests: Arc<AtomicUsize>,
    drain_timeout: Duration,
    shutdown_rx: broadcast::Receiver<()>,
) {
    shutdown_signal_with_graduated_drain(
        boot_state,
        in_flight_requests,
        drain_timeout,
        shutdown_rx,
        DrainPhaseConfig::default(),
        None,
    )
    .await;
}

/// Extended shutdown handler with configurable phase durations and an optional
/// SSE shutdown broadcaster.
pub async fn shutdown_signal_with_graduated_drain(
    boot_state: BootStateManager,
    in_flight_requests: Arc<AtomicUsize>,
    drain_timeout: Duration,
    mut shutdown_rx: broadcast::Receiver<()>,
    phases: DrainPhaseConfig,
    shutdown_broadcast: Option<Arc<broadcast::Sender<()>>>,
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
    let _ = select_3(ctrl_c, terminate, internal).await;

    info!("Shutdown signal received, beginning graduated drain");

    // If still booting, fail immediately — no requests to drain
    if boot_state.is_booting() {
        warn!("Shutdown signal received during boot — failing immediately");
        boot_state
            .fail(adapteros_server_api::boot_state::FailureReason::new(
                adapteros_server_api::boot_state::failure_codes::SHUTDOWN_DURING_BOOT,
                "shutdown signal received before server reached ready state",
            ))
            .await;
        return;
    }

    // Transition to draining state
    boot_state.drain().await;

    // --- Phase boundaries ---
    let (phase1_end, phase2_end) = phases.phase_boundaries();
    let total = drain_timeout;

    info!(
        graceful_secs = phases.graceful.as_secs(),
        warning_secs = phases.warning.as_secs(),
        notify_secs = phases.notify.as_secs(),
        total_secs = total.as_secs(),
        configured_drain_timeout_secs = drain_timeout.as_secs(),
        "Drain phase durations configured"
    );

    let start = tokio::time::Instant::now();
    let mut current_phase = DrainPhase::Graceful;
    let mut stats = DrainStats::default();
    let mut sse_notified = false;

    loop {
        let count = in_flight_requests.load(Ordering::SeqCst);
        let elapsed = start.elapsed();

        // Track statistics
        stats.record(count);

        // All requests drained — exit early
        if count == 0 {
            info!(
                phase = current_phase.as_str(),
                elapsed_ms = elapsed.as_millis() as u64,
                "All in-flight requests completed"
            );
            break;
        }

        // Determine current phase
        let new_phase = phase_for_elapsed(elapsed, phase1_end, phase2_end);

        // Phase transition logging
        if new_phase != current_phase {
            info!(
                from = current_phase.as_str(),
                to = new_phase.as_str(),
                in_flight = count,
                elapsed_secs = elapsed.as_secs(),
                "Drain phase transition"
            );
            current_phase = new_phase;
        }

        match current_phase {
            DrainPhase::Graceful => {
                // Phase 1: silent wait, log only on first iteration
                if stats.sample_count == 1 {
                    info!(
                        in_flight = count,
                        timeout_secs = total.as_secs(),
                        "Waiting for in-flight requests to complete"
                    );
                }
            }
            DrainPhase::Warning => {
                // Phase 2: periodic structured warnings
                // Log every ~2s (20 iterations at 100ms)
                if should_emit_warning_sample(stats.sample_count) {
                    warn!(
                        in_flight = count,
                        peak = stats.peak_in_flight,
                        elapsed_secs = elapsed.as_secs(),
                        remaining_secs = total.saturating_sub(elapsed).as_secs(),
                        "Drain warning: requests still in flight"
                    );
                }
            }
            DrainPhase::Notify => {
                // Phase 3: broadcast shutdown to SSE streams (once)
                if !sse_notified {
                    if let Some(ref tx) = shutdown_broadcast {
                        let _ = tx.send(());
                        info!("Broadcast shutdown notification to SSE streams");
                    }
                    sse_notified = true;

                    warn!(
                        in_flight = count,
                        elapsed_secs = elapsed.as_secs(),
                        "Drain escalation: SSE streams notified, final drain window"
                    );
                }
            }
            DrainPhase::Force => {
                // Should not reach here inside the loop (handled below)
            }
        }

        // Check if we've exceeded all phases
        if elapsed >= total {
            current_phase = DrainPhase::Force;
            break;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // --- Phase 4: Force ---
    if current_phase == DrainPhase::Force {
        let count = in_flight_requests.load(Ordering::SeqCst);
        let avg_in_flight = stats.average_in_flight();

        error!(
            phase = "force",
            in_flight_current = count,
            in_flight_peak = stats.peak_in_flight,
            in_flight_avg = format!("{:.2}", avg_in_flight),
            elapsed_secs = start.elapsed().as_secs(),
            total_timeout_secs = total.as_secs(),
            sample_count = stats.sample_count,
            "Drain timeout exceeded — force-cancelling remaining requests"
        );

        // Broadcast final shutdown signal for any remaining SSE streams
        if let Some(ref tx) = shutdown_broadcast {
            if !sse_notified {
                let _ = tx.send(());
            }
        }

        error!(
            "FORCE SHUTDOWN: {} request(s) did not complete within {}s graduated drain. \
             Peak in-flight: {}, Average: {:.2}. \
             Investigate: database locks, slow network I/O, or stuck async tasks.",
            count,
            total.as_secs(),
            stats.peak_in_flight,
            avg_in_flight
        );
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

    // ================================================================
    // Graduated drain tests
    // ================================================================

    #[test]
    fn drain_phase_config_default_totals_30s() {
        let cfg = DrainPhaseConfig::default();
        assert_eq!(cfg.graceful, Duration::from_secs(15));
        assert_eq!(cfg.warning, Duration::from_secs(10));
        assert_eq!(cfg.notify, Duration::from_secs(5));
        assert_eq!(cfg.total(), Duration::from_secs(30));
    }

    #[test]
    fn drain_phase_config_custom_total() {
        let cfg = DrainPhaseConfig {
            graceful: Duration::from_secs(5),
            warning: Duration::from_secs(3),
            notify: Duration::from_secs(2),
        };
        assert_eq!(cfg.total(), Duration::from_secs(10));
    }

    #[test]
    fn drain_phase_as_str() {
        assert_eq!(DrainPhase::Graceful.as_str(), "graceful");
        assert_eq!(DrainPhase::Warning.as_str(), "warning");
        assert_eq!(DrainPhase::Notify.as_str(), "notify");
        assert_eq!(DrainPhase::Force.as_str(), "force");
    }

    #[test]
    fn drain_phase_equality() {
        assert_eq!(DrainPhase::Graceful, DrainPhase::Graceful);
        assert_ne!(DrainPhase::Graceful, DrainPhase::Warning);
        assert_ne!(DrainPhase::Warning, DrainPhase::Notify);
        assert_ne!(DrainPhase::Notify, DrainPhase::Force);
    }

    #[tokio::test]
    async fn graduated_drain_completes_when_no_requests() {
        let boot_state = BootStateManager::new();
        boot_state.ready().await;

        let in_flight = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = broadcast::channel(4);

        // Send immediate shutdown signal
        let _ = tx.send(());

        let phases = DrainPhaseConfig {
            graceful: Duration::from_millis(50),
            warning: Duration::from_millis(50),
            notify: Duration::from_millis(50),
        };

        let start = tokio::time::Instant::now();
        shutdown_signal_with_graduated_drain(
            boot_state,
            in_flight,
            Duration::from_millis(150),
            rx,
            phases,
            None,
        )
        .await;
        let elapsed = start.elapsed();

        // Should complete almost immediately since no in-flight requests
        assert!(
            elapsed < Duration::from_millis(500),
            "Should complete quickly when no requests in flight"
        );
    }

    #[tokio::test]
    async fn graduated_drain_reaches_force_phase() {
        let boot_state = BootStateManager::new();
        boot_state.ready().await;

        let in_flight = Arc::new(AtomicUsize::new(3)); // Requests that never complete
        let (tx, rx) = broadcast::channel(4);

        // Send immediate shutdown signal
        let _ = tx.send(());

        // Use very short phase durations for testing
        let phases = DrainPhaseConfig {
            graceful: Duration::from_millis(20),
            warning: Duration::from_millis(20),
            notify: Duration::from_millis(20),
        };
        let total = phases.total();

        let start = tokio::time::Instant::now();
        shutdown_signal_with_graduated_drain(
            boot_state,
            in_flight.clone(),
            Duration::from_millis(60),
            rx,
            phases,
            None,
        )
        .await;
        let elapsed = start.elapsed();

        // Should have waited at least the total phase duration
        assert!(
            elapsed >= total,
            "Should wait at least the total phase duration ({:?} >= {:?})",
            elapsed,
            total
        );
        // But not much longer (within 200ms tolerance for test scheduling)
        assert!(
            elapsed < total + Duration::from_millis(200),
            "Should not wait much longer than total ({:?} < {:?})",
            elapsed,
            total + Duration::from_millis(200)
        );
    }

    #[tokio::test]
    async fn graduated_drain_exits_early_when_drained() {
        let boot_state = BootStateManager::new();
        boot_state.ready().await;

        let in_flight = Arc::new(AtomicUsize::new(2));
        let (tx, rx) = broadcast::channel(4);

        // Send immediate shutdown signal
        let _ = tx.send(());

        let phases = DrainPhaseConfig {
            graceful: Duration::from_millis(500),
            warning: Duration::from_millis(500),
            notify: Duration::from_millis(500),
        };

        // Drain requests after 100ms
        let in_flight_clone = in_flight.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            in_flight_clone.store(0, Ordering::SeqCst);
        });

        let start = tokio::time::Instant::now();
        shutdown_signal_with_graduated_drain(
            boot_state,
            in_flight,
            Duration::from_millis(1500),
            rx,
            phases,
            None,
        )
        .await;
        let elapsed = start.elapsed();

        // Should exit early — well before the 1500ms total
        assert!(
            elapsed < Duration::from_millis(500),
            "Should exit early when all requests complete ({:?})",
            elapsed
        );
    }

    #[tokio::test]
    async fn graduated_drain_broadcasts_shutdown() {
        let boot_state = BootStateManager::new();
        boot_state.ready().await;

        let in_flight = Arc::new(AtomicUsize::new(1));
        let (shutdown_tx, shutdown_rx) = broadcast::channel(4);
        let (sse_tx, mut sse_rx) = broadcast::channel::<()>(4);

        // Send immediate shutdown signal
        let _ = shutdown_tx.send(());

        let phases = DrainPhaseConfig {
            graceful: Duration::from_millis(10),
            warning: Duration::from_millis(10),
            notify: Duration::from_millis(10),
        };

        shutdown_signal_with_graduated_drain(
            boot_state,
            in_flight,
            Duration::from_millis(30),
            shutdown_rx,
            phases,
            Some(Arc::new(sse_tx)),
        )
        .await;

        // The SSE broadcast should have been sent during notify phase
        let received = sse_rx.try_recv();
        assert!(
            received.is_ok(),
            "SSE shutdown broadcast should have been sent"
        );
    }
}
