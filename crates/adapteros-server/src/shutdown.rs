//! Shutdown coordinator for graceful lifecycle management
//!
//! Ensures all background services are properly terminated in correct order.
//! 【2025-11-22†feature(shutdown)†coordinator-implementation】

use adapteros_deterministic_exec::DeterministicJoinHandle;
use std::time::Duration;
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

/// Coordinator for graceful shutdown of all AdapterOS components
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
        let background_handles = std::mem::take(&mut self.background_handles);
        if !background_handles.is_empty() {
            info!(
                "Shutting down {} background tasks",
                background_handles.len()
            );
            for (i, handle) in background_handles.into_iter().enumerate() {
                self.report_progress(
                    &format!("background_task_{}", i),
                    ShutdownStatus::InProgress,
                    start_time.elapsed(),
                );
                handle.abort();
                self.report_progress(
                    &format!("background_task_{}", i),
                    ShutdownStatus::Completed,
                    start_time.elapsed(),
                );
                // Note: DeterministicJoinHandle doesn't support timeout waiting
            }
        }

        // Give remaining tasks a brief moment to respond to abort signals
        tokio::time::sleep(Duration::from_millis(100)).await;

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

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::B3Hash;
    use adapteros_deterministic_exec::{init_global_executor, ExecutorConfig};
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

        // Should report partial failure due to timeout
        let result = coordinator.shutdown().await;
        println!("Shutdown result: {:?}", result);
        assert!(result.is_err());
        match result.unwrap_err() {
            ShutdownError::PartialFailure { failed_count } => {
                assert_eq!(failed_count, 1);
            }
            other => panic!("Expected PartialFailure, got {:?}", other),
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
}
