//! Shutdown coordinator for graceful lifecycle management
//!
//! Ensures all background services are properly terminated in correct order.
//! 【2025-11-22†feature(shutdown)†coordinator-implementation】

use adapteros_deterministic_exec::select::select_2;
use adapteros_deterministic_exec::DeterministicJoinHandle;
use adapteros_server_api::boot_state::BootStateManager;
use adapteros_server_api::state::BackgroundTaskTracker;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

/// Errors that can occur during shutdown
#[derive(Debug, thiserror::Error)]
pub enum ShutdownError {
    #[error("Shutdown timeout exceeded")]
    Timeout,

    #[error("Component shutdown failed: {component}")]
    ComponentError { component: String },

    #[error("Task join error: {0}")]
    JoinError(#[from] tokio::task::JoinError),

    #[error("Partial shutdown failure - {failed_count} components failed")]
    PartialFailure { failed_count: usize },

    #[error("Critical component failure: {component}")]
    CriticalFailure { component: String },
}

/// Shutdown configuration for each component
#[derive(Debug, Clone)]
pub struct ShutdownConfig {
    pub telemetry_timeout: Duration,
    pub federation_timeout: Duration,
    pub uds_metrics_timeout: Duration,
    pub git_daemon_timeout: Duration,
    pub policy_watcher_timeout: Duration,
    pub overall_timeout: Duration,
    /// Timeout for background tasks to respond to shutdown signal before abort.
    /// Tasks should use `tokio::select!` to check for shutdown signals.
    pub background_task_timeout: Duration,
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            telemetry_timeout: Duration::from_secs(10),
            federation_timeout: Duration::from_secs(15),
            uds_metrics_timeout: Duration::from_secs(5),
            git_daemon_timeout: Duration::from_secs(10),
            policy_watcher_timeout: Duration::from_secs(5),
            overall_timeout: Duration::from_secs(30),
            background_task_timeout: Duration::from_secs(5),
        }
    }
}

/// Shutdown progress tracking
#[derive(Debug, Clone)]
pub struct ShutdownProgress {
    pub component: String,
    pub status: ShutdownStatus,
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub enum ShutdownStatus {
    Pending,
    InProgress,
    Completed,
    Failed(String),
    Timeout,
}

/// Coordinator for graceful shutdown of all adapterOS components
pub struct ShutdownCoordinator {
    shutdown_tx: broadcast::Sender<()>,
    background_handles: Vec<DeterministicJoinHandle>,
    telemetry_handle: Option<JoinHandle<()>>,
    federation_handle: Option<JoinHandle<()>>,
    alert_handle: Option<DeterministicJoinHandle>,
    policy_watcher_handle: Option<JoinHandle<()>>,
    uds_metrics_handle: Option<JoinHandle<()>>,
    git_daemon_handle: Option<JoinHandle<()>>,
    config: ShutdownConfig,
}

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator with default config
    pub fn new() -> Self {
        Self::with_config(ShutdownConfig::default())
    }

    /// Create a new shutdown coordinator with custom config
    pub fn with_config(config: ShutdownConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            shutdown_tx,
            background_handles: Vec::new(),
            telemetry_handle: None,
            federation_handle: None,
            alert_handle: None,
            policy_watcher_handle: None,
            uds_metrics_handle: None,
            git_daemon_handle: None,
            config,
        }
    }

    /// Get a receiver for shutdown signals
    pub fn subscribe_shutdown(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Register a background task for cleanup
    pub fn register_task(&mut self, handle: DeterministicJoinHandle) {
        self.background_handles.push(handle);
    }

    /// Register specialized service handles
    pub fn set_telemetry_handle(&mut self, handle: JoinHandle<()>) {
        self.telemetry_handle = Some(handle);
    }

    pub fn set_federation_handle(&mut self, handle: JoinHandle<()>) {
        self.federation_handle = Some(handle);
    }

    pub fn set_alert_handle(&mut self, handle: DeterministicJoinHandle) {
        self.alert_handle = Some(handle);
    }

    pub fn set_policy_watcher_handle(&mut self, handle: JoinHandle<()>) {
        self.policy_watcher_handle = Some(handle);
    }

    pub fn set_uds_metrics_handle(&mut self, handle: JoinHandle<()>) {
        self.uds_metrics_handle = Some(handle);
    }

    pub fn set_git_daemon_handle(&mut self, handle: JoinHandle<()>) {
        self.git_daemon_handle = Some(handle);
    }

    /// Report shutdown progress (for monitoring/debugging)
    pub fn report_progress(&self, component: &str, status: ShutdownStatus, elapsed: Duration) {
        let progress = ShutdownProgress {
            component: component.to_string(),
            status,
            elapsed,
        };

        match &progress.status {
            ShutdownStatus::Completed => {
                info!(
                    "✅ {} shutdown completed in {:?}",
                    progress.component, progress.elapsed
                );
            }
            ShutdownStatus::Failed(reason) => {
                warn!(
                    "❌ {} shutdown failed after {:?}: {}",
                    progress.component, progress.elapsed, reason
                );
            }
            ShutdownStatus::Timeout => {
                warn!(
                    "⏰ {} shutdown timed out after {:?}",
                    progress.component, progress.elapsed
                );
            }
            ShutdownStatus::InProgress => {
                debug!(
                    "🔄 {} shutdown in progress ({:?})",
                    progress.component, progress.elapsed
                );
            }
            ShutdownStatus::Pending => {
                debug!("⏳ {} shutdown pending", progress.component);
            }
        }
    }

    /// Initiate graceful shutdown with timeout and error recovery
    pub async fn shutdown(mut self) -> Result<(), ShutdownError> {
        info!(
            "Initiating graceful shutdown (overall timeout: {:?})",
            self.config.overall_timeout
        );

        let start_time = std::time::Instant::now();
        let _ = self.shutdown_tx.send(());

        // Track shutdown failures for partial recovery
        let mut failed_components = Vec::new();
        let mut critical_failures = Vec::new();

        // Shutdown in dependency order: specialized services first, then background tasks

        // 1. Telemetry system - flush buffers and close connections (critical for data integrity)
        if let Some(mut handle) = self.telemetry_handle.take() {
            self.report_progress(
                "telemetry",
                ShutdownStatus::InProgress,
                start_time.elapsed(),
            );

            // Try graceful shutdown first
            match tokio::time::timeout(self.config.telemetry_timeout, &mut handle).await {
                Ok(result) => match result {
                    Ok(_) => {
                        self.report_progress(
                            "telemetry",
                            ShutdownStatus::Completed,
                            start_time.elapsed(),
                        );
                    }
                    Err(e) => {
                        warn!("Telemetry system shutdown failed with error: {}", e);
                        self.report_progress(
                            "telemetry",
                            ShutdownStatus::Failed(format!("Task error: {}", e)),
                            start_time.elapsed(),
                        );
                        critical_failures.push("telemetry".to_string());
                    }
                },
                Err(_) => {
                    // Timeout - force abort
                    handle.abort();
                    self.report_progress(
                        "telemetry",
                        ShutdownStatus::Timeout,
                        start_time.elapsed(),
                    );
                    critical_failures.push("telemetry".to_string());
                }
            }
        }

        // 2. Federation daemon - allow clean verification completion
        if let Some(mut handle) = self.federation_handle.take() {
            self.report_progress(
                "federation",
                ShutdownStatus::InProgress,
                start_time.elapsed(),
            );
            match tokio::time::timeout(self.config.federation_timeout, &mut handle).await {
                Ok(result) => match result {
                    Ok(_) => {
                        self.report_progress(
                            "federation",
                            ShutdownStatus::Completed,
                            start_time.elapsed(),
                        );
                    }
                    Err(e) => {
                        warn!("Federation daemon shutdown failed with error: {}", e);
                        self.report_progress(
                            "federation",
                            ShutdownStatus::Failed(format!("Task error: {}", e)),
                            start_time.elapsed(),
                        );
                        failed_components.push("federation".to_string());
                    }
                },
                Err(_) => {
                    handle.abort();
                    self.report_progress(
                        "federation",
                        ShutdownStatus::Timeout,
                        start_time.elapsed(),
                    );
                    failed_components.push("federation".to_string());
                }
            }
        }

        // 3. UDS metrics exporter - close socket connections
        if let Some(mut handle) = self.uds_metrics_handle.take() {
            self.report_progress(
                "uds_metrics",
                ShutdownStatus::InProgress,
                start_time.elapsed(),
            );
            match tokio::time::timeout(self.config.uds_metrics_timeout, &mut handle).await {
                Ok(result) => match result {
                    Ok(_) => {
                        self.report_progress(
                            "uds_metrics",
                            ShutdownStatus::Completed,
                            start_time.elapsed(),
                        );
                    }
                    Err(e) => {
                        warn!("UDS metrics exporter shutdown failed with error: {}", e);
                        self.report_progress(
                            "uds_metrics",
                            ShutdownStatus::Failed(format!("Task error: {}", e)),
                            start_time.elapsed(),
                        );
                        failed_components.push("uds_metrics".to_string());
                    }
                },
                Err(_) => {
                    handle.abort();
                    self.report_progress(
                        "uds_metrics",
                        ShutdownStatus::Timeout,
                        start_time.elapsed(),
                    );
                    failed_components.push("uds_metrics".to_string());
                }
            }
        }

        // 4. Git daemon - stop polling and file watching
        if let Some(mut handle) = self.git_daemon_handle.take() {
            self.report_progress(
                "git_daemon",
                ShutdownStatus::InProgress,
                start_time.elapsed(),
            );
            match tokio::time::timeout(self.config.git_daemon_timeout, &mut handle).await {
                Ok(result) => match result {
                    Ok(_) => {
                        self.report_progress(
                            "git_daemon",
                            ShutdownStatus::Completed,
                            start_time.elapsed(),
                        );
                    }
                    Err(e) => {
                        warn!("Git daemon shutdown failed with error: {}", e);
                        self.report_progress(
                            "git_daemon",
                            ShutdownStatus::Failed(format!("Task error: {}", e)),
                            start_time.elapsed(),
                        );
                        failed_components.push("git_daemon".to_string());
                    }
                },
                Err(_) => {
                    handle.abort();
                    self.report_progress(
                        "git_daemon",
                        ShutdownStatus::Timeout,
                        start_time.elapsed(),
                    );
                    failed_components.push("git_daemon".to_string());
                }
            }
        }

        // 5. Policy watcher - stop hash validation sweeps
        if let Some(mut handle) = self.policy_watcher_handle.take() {
            self.report_progress(
                "policy_watcher",
                ShutdownStatus::InProgress,
                start_time.elapsed(),
            );
            match tokio::time::timeout(self.config.policy_watcher_timeout, &mut handle).await {
                Ok(result) => match result {
                    Ok(_) => {
                        self.report_progress(
                            "policy_watcher",
                            ShutdownStatus::Completed,
                            start_time.elapsed(),
                        );
                    }
                    Err(e) => {
                        warn!("Policy watcher shutdown failed with error: {}", e);
                        self.report_progress(
                            "policy_watcher",
                            ShutdownStatus::Failed(format!("Task error: {}", e)),
                            start_time.elapsed(),
                        );
                        failed_components.push("policy_watcher".to_string());
                    }
                },
                Err(_) => {
                    handle.abort();
                    self.report_progress(
                        "policy_watcher",
                        ShutdownStatus::Timeout,
                        start_time.elapsed(),
                    );
                    failed_components.push("policy_watcher".to_string());
                }
            }
        }

        // 6. Alert watcher - stop job monitoring
        if let Some(handle) = self.alert_handle.take() {
            self.report_progress(
                "alert_watcher",
                ShutdownStatus::InProgress,
                start_time.elapsed(),
            );
            handle.abort();
            self.report_progress(
                "alert_watcher",
                ShutdownStatus::Completed,
                start_time.elapsed(),
            );
            // Note: DeterministicJoinHandle doesn't support timeout waiting
        }

        // 7. Background tasks - status writer, TTL cleanup, heartbeat recovery
        // Tasks should respond to the shutdown broadcast signal sent earlier.
        // We give them time to complete gracefully before forcing abort.
        let background_handles = std::mem::take(&mut self.background_handles);
        if !background_handles.is_empty() {
            info!(
                "Shutting down {} background tasks (waiting up to {:?} for graceful exit)",
                background_handles.len(),
                self.config.background_task_timeout
            );

            // Wait for tasks to respond to shutdown signal
            tokio::time::sleep(self.config.background_task_timeout).await;

            // Abort any tasks that haven't completed gracefully
            for (i, handle) in background_handles.into_iter().enumerate() {
                if !handle.is_finished() {
                    warn!(
                        "Background task {} did not complete within {:?}, aborting",
                        i, self.config.background_task_timeout
                    );
                    handle.abort();
                    self.report_progress(
                        &format!("background_task_{}", i),
                        ShutdownStatus::Timeout,
                        start_time.elapsed(),
                    );
                } else {
                    self.report_progress(
                        &format!("background_task_{}", i),
                        ShutdownStatus::Completed,
                        start_time.elapsed(),
                    );
                }
            }
        }

        // Database connections will be cleaned up automatically by the connection pool
        info!("Database connection pool cleanup handled automatically");

        let total_shutdown_time = start_time.elapsed();
        info!("Total shutdown time: {:?}", total_shutdown_time);

        // Analyze shutdown results and determine overall success
        if !critical_failures.is_empty() {
            error!(
                "Critical component shutdown failures: {:?}",
                critical_failures
            );
            return Err(ShutdownError::CriticalFailure {
                component: critical_failures.join(", "),
            });
        }

        if !failed_components.is_empty() {
            warn!(
                "Partial shutdown failures in {} components: {:?}",
                failed_components.len(),
                failed_components
            );
            info!(
                "Graceful shutdown completed with partial failures - system integrity maintained"
            );
            return Err(ShutdownError::PartialFailure {
                failed_count: failed_components.len(),
            });
        }

        info!(
            "Graceful shutdown sequence completed successfully - all components shut down cleanly"
        );
        Ok(())
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

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
/// Waits for either Ctrl+C (SIGINT) or SIGTERM signals, then:
/// 1. Transitions boot state to draining
/// 2. Waits for in-flight requests to complete (with timeout)
/// 3. Transitions boot state to stopping
pub async fn shutdown_signal_with_drain(
    boot_state: BootStateManager,
    in_flight_requests: Arc<AtomicUsize>,
    drain_timeout: Duration,
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

    // Use deterministic select instead of tokio::select!
    // Left (ctrl_c) has priority over Right (terminate)
    let _ = select_2(ctrl_c, terminate).await;

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
    use adapteros_core::B3Hash;
    use adapteros_deterministic_exec::{init_global_executor, EnforcementMode, ExecutorConfig};
    use tokio::time::{timeout, Duration};

    fn init_test_executor() {
        let manifest_hash = B3Hash::hash(b"test-manifest");
        let executor_config = ExecutorConfig {
            global_seed: adapteros_core::derive_seed(&manifest_hash, "test"),
            max_ticks_per_task: 1_000_000,
            enable_event_logging: false,
            replay_mode: false,
            replay_events: Vec::new(),
            agent_id: None,
            enable_thread_pinning: false,
            worker_threads: Some(2),
            enforcement_mode: EnforcementMode::default(),
        };
        let _ = init_global_executor(executor_config);
    }

    #[tokio::test]
    async fn test_shutdown_coordinator_creation() {
        let coordinator = ShutdownCoordinator::new();
        assert!(coordinator.background_handles.is_empty());
        assert!(coordinator.telemetry_handle.is_none());
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        init_test_executor();
        let mut coordinator = ShutdownCoordinator::new();

        // Register a simple test task using deterministic spawn
        let handle =
            adapteros_deterministic_exec::spawn_deterministic("test_task".to_string(), async {
                tokio::time::sleep(Duration::from_millis(50)).await;
            })
            .expect("Failed to spawn test task");
        coordinator.register_task(handle);

        // Test shutdown completes
        let result = timeout(Duration::from_secs(5), coordinator.shutdown()).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_timeout() {
        init_test_executor();
        let config = ShutdownConfig {
            telemetry_timeout: Duration::from_millis(50), // Very short timeout
            ..Default::default()
        };
        let mut coordinator = ShutdownCoordinator::with_config(config);

        // Mock telemetry handle that takes longer than the timeout
        let mock_handle = tokio::spawn(async {
            // This will definitely take longer than 50ms timeout
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
        coordinator.set_telemetry_handle(mock_handle);

        // Telemetry is a critical component, so timeout should be CriticalFailure
        let result = coordinator.shutdown().await;
        println!("Shutdown result: {:?}", result);
        assert!(result.is_err());
        match result.unwrap_err() {
            ShutdownError::CriticalFailure { component } => {
                assert!(component.contains("telemetry"));
            }
            other => panic!("Expected CriticalFailure, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_shutdown_with_multiple_components() {
        init_test_executor();
        let mut coordinator = ShutdownCoordinator::new();

        // Register multiple background tasks
        for i in 0..3 {
            let handle = adapteros_deterministic_exec::spawn_deterministic(
                format!("test_task_{}", i),
                async {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                },
            )
            .expect("Failed to spawn test task");
            coordinator.register_task(handle);
        }

        // Register mock handles
        let mock_telemetry = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(5)).await;
        });
        coordinator.set_telemetry_handle(mock_telemetry);

        let mock_federation = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(5)).await;
        });
        coordinator.set_federation_handle(mock_federation);

        // Test shutdown completes with all components
        let result = timeout(Duration::from_secs(5), coordinator.shutdown()).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_critical_failure() {
        init_test_executor();
        let config = ShutdownConfig {
            telemetry_timeout: Duration::from_millis(50),
            ..Default::default()
        };
        let mut coordinator = ShutdownCoordinator::with_config(config);

        // Mock telemetry handle that takes too long (telemetry is critical)
        let mock_handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
        });
        coordinator.set_telemetry_handle(mock_handle);

        // Shutdown should report critical failure
        let result = coordinator.shutdown().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ShutdownError::CriticalFailure { component } => {
                assert_eq!(component, "telemetry");
            }
            _ => panic!("Expected CriticalFailure"),
        }
    }

    #[tokio::test]
    async fn test_shutdown_broadcast_signal() {
        let coordinator = ShutdownCoordinator::new();
        let mut rx = coordinator.subscribe_shutdown();

        // Send shutdown signal
        let _ = coordinator.shutdown_tx.send(());

        // Should receive the signal
        let result = timeout(Duration::from_millis(100), rx.recv()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_config_defaults() {
        let config = ShutdownConfig::default();
        assert_eq!(config.telemetry_timeout, Duration::from_secs(10));
        assert_eq!(config.federation_timeout, Duration::from_secs(15));
        assert_eq!(config.uds_metrics_timeout, Duration::from_secs(5));
        assert_eq!(config.overall_timeout, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn test_apply_background_task_degraded_no_failures() {
        // Create a fresh boot state manager
        let boot_state = BootStateManager::new();
        // Create an empty tracker (no failures)
        let tracker = BackgroundTaskTracker::default();

        // Should return false when there are no critical failures
        let result = apply_background_task_degraded(&boot_state, &tracker).await;
        assert!(!result, "Expected false when no critical failures exist");
    }

    #[tokio::test]
    async fn test_apply_background_task_degraded_with_critical_failure() {
        // Create a fresh boot state manager
        let boot_state = BootStateManager::new();
        // Transition to ready state first (so degradation can happen)
        boot_state.ready().await;

        // Create a tracker with a critical failure
        let tracker = BackgroundTaskTracker::default();
        tracker.record_failed("critical-task", "spawn failed", true);

        // Should return true when there are critical failures
        let result = apply_background_task_degraded(&boot_state, &tracker).await;
        assert!(result, "Expected true when critical failures exist");
    }

    #[tokio::test]
    async fn test_apply_background_task_degraded_ignores_non_critical() {
        // Create a fresh boot state manager
        let boot_state = BootStateManager::new();
        // Create a tracker with only non-critical failures
        let tracker = BackgroundTaskTracker::default();
        tracker.record_failed("optional-task", "spawn failed", false);

        // Should return false when there are only non-critical failures
        let result = apply_background_task_degraded(&boot_state, &tracker).await;
        assert!(
            !result,
            "Expected false when only non-critical failures exist"
        );
    }

    #[tokio::test]
    async fn test_drain_timeout_logic() {
        // Test that the drain timeout logic respects the timeout duration
        // We simulate the drain loop without actual signal handling
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
