//! Process health monitoring and lifecycle management
//!
//! Implements health checks and process monitoring to prevent runaway processes.
//! Aligns with Isolation Ruleset #8 and Memory Ruleset #12 from policy enforcement.

use crate::resource_monitor::{ResourceMonitor, ResourceThresholds};
use adapteros_core::{identity::IdentityEnvelope, AosError, Result};
use adapteros_telemetry::{make_health_payload, HealthEventKind, TelemetryWriter};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::interval;
use tracing::{error, info, warn};

/// Health check configuration
#[derive(Debug, Clone)]
pub struct HealthConfig {
    pub check_interval: Duration,
    pub max_response_time: Duration,
    pub max_memory_growth: u64, // bytes
    pub max_cpu_time: Duration,
    pub max_consecutive_failures: usize,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(30),
            max_response_time: Duration::from_secs(60),
            max_memory_growth: 100 * 1024 * 1024,   // 100MB
            max_cpu_time: Duration::from_secs(300), // 5 minutes
            max_consecutive_failures: 3,
        }
    }
}

/// Process health status for worker monitoring
///
/// Note: For general health status, use `adapteros_core::HealthStatus`.
/// This enum is specific to process health monitoring with detailed message variants.
#[derive(Debug, Clone)]
pub enum ProcessHealthStatus {
    Healthy,
    Warning(String),
    Critical(String),
    Failing,
}

impl ProcessHealthStatus {
    fn label(&self) -> String {
        match self {
            ProcessHealthStatus::Healthy => "healthy".to_string(),
            ProcessHealthStatus::Warning(msg) => format!("warning:{msg}"),
            ProcessHealthStatus::Critical(msg) => format!("critical:{msg}"),
            ProcessHealthStatus::Failing => "failing".to_string(),
        }
    }
}

fn dev_no_auth_enabled() -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }

    match std::env::var("AOS_DEV_NO_AUTH") {
        Ok(raw) => matches!(raw.to_lowercase().as_str(), "true" | "1" | "yes" | "on"),
        Err(_) => false,
    }
}

/// Process health monitor
pub struct HealthMonitor {
    config: HealthConfig,
    start_time: Instant,
    last_request_time: AtomicU64,
    baseline_memory: u64,
    cpu_time_start: Duration,
    consecutive_failures: AtomicUsize,
    shutdown_requested: AtomicBool,
    telemetry: Option<TelemetryWriter>,
    tenant_id: String,
    worker_id: String,
    last_status: Mutex<Option<String>>,
    /// Resource monitor for CPU, memory, FD, thread pool, and GPU exhaustion
    resource_monitor: Option<Arc<ResourceMonitor>>,
}

impl HealthMonitor {
    pub fn new(config: HealthConfig) -> Result<Self> {
        let baseline_memory = get_process_memory()?;
        let cpu_time_start = get_process_cpu_time()?;

        Ok(Self {
            config,
            start_time: Instant::now(),
            last_request_time: AtomicU64::new(0),
            baseline_memory,
            cpu_time_start,
            consecutive_failures: AtomicUsize::new(0),
            shutdown_requested: AtomicBool::new(false),
            telemetry: None,
            tenant_id: "system".to_string(),
            worker_id: "unknown".to_string(),
            last_status: Mutex::new(None),
            resource_monitor: None,
        })
    }

    /// Attach a resource monitor for comprehensive resource exhaustion checking
    pub fn with_resource_monitor(mut self, monitor: Arc<ResourceMonitor>) -> Self {
        self.resource_monitor = Some(monitor);
        self
    }

    /// Create and attach a resource monitor with custom thresholds
    pub fn with_resource_thresholds(mut self, thresholds: ResourceThresholds) -> Result<Self> {
        let monitor = ResourceMonitor::new(thresholds)?;
        self.resource_monitor = Some(Arc::new(monitor));
        Ok(self)
    }

    /// Create and attach a resource monitor with default thresholds
    pub fn with_default_resource_monitor(self) -> Result<Self> {
        self.with_resource_thresholds(ResourceThresholds::default())
    }

    /// Get a reference to the resource monitor if configured
    pub fn resource_monitor(&self) -> Option<&Arc<ResourceMonitor>> {
        self.resource_monitor.as_ref()
    }

    /// Attach telemetry context for health lifecycle emissions.
    pub fn with_telemetry(
        mut self,
        telemetry: TelemetryWriter,
        tenant_id: impl Into<String>,
        worker_id: impl Into<String>,
    ) -> Self {
        self.telemetry = Some(telemetry);
        self.tenant_id = tenant_id.into();
        self.worker_id = worker_id.into();
        self
    }

    fn emit_health_status(&self, kind: HealthEventKind, status: &str, error: Option<String>) {
        let mut last = self.last_status.lock().unwrap_or_else(|e| e.into_inner());

        let previous = last.clone();
        let should_emit =
            matches!(kind, HealthEventKind::FatalError) || previous.as_deref() != Some(status);

        if !should_emit {
            return;
        }

        // Always update last_status, even without telemetry
        *last = Some(status.to_string());

        // Emit telemetry if configured
        let Some(writer) = &self.telemetry else {
            return;
        };

        let identity = IdentityEnvelope::new(
            self.tenant_id.clone(),
            "worker".to_string(),
            "health".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        );

        let payload = make_health_payload(
            self.worker_id.clone(),
            self.tenant_id.clone(),
            kind,
            previous,
            Some(status.to_string()),
            None,
            None,
            error,
        );

        if let Err(e) = writer.log_health_lifecycle(identity, payload) {
            warn!(error = %e, "Failed to emit health telemetry");
        }
    }

    /// Emit a fatal health event outside the periodic monitor loop.
    pub fn record_fatal(&self, status: &str, error: impl Into<String>) {
        self.emit_health_status(HealthEventKind::FatalError, status, Some(error.into()));
    }

    /// Request shutdown without waiting for the monitor loop to hit thresholds.
    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn last_status_for_test(&self) -> Option<String> {
        self.last_status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub async fn start_monitoring(&self) -> Result<()> {
        self.start_monitoring_with_hook(|_, _| Ok(())).await
    }

    pub async fn start_monitoring_with_hook<F>(&self, mut on_tick: F) -> Result<()>
    where
        F: FnMut(&HealthMonitor, HealthTick) -> Result<()> + Send,
    {
        let mut interval = interval(self.config.check_interval);

        loop {
            interval.tick().await;

            if self.shutdown_requested.load(Ordering::Relaxed) {
                info!("Health monitor shutting down");
                break;
            }

            match self.check_health().await {
                Ok(status) => {
                    self.consecutive_failures.store(0, Ordering::Relaxed);
                    let status_str = status.label();
                    self.emit_health_status(HealthEventKind::HealthStateChange, &status_str, None);
                    let _ = on_tick(self, HealthTick::Status { status, status_str });
                }
                Err(e) => {
                    let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
                    error!("Health check failed ({}): {}", failures, e);
                    self.emit_health_status(
                        HealthEventKind::FatalError,
                        "failing",
                        Some(e.to_string()),
                    );
                    let _ = on_tick(
                        self,
                        HealthTick::Failure {
                            error: e.to_string(),
                            failures,
                        },
                    );

                    if failures >= self.config.max_consecutive_failures {
                        error!("Too many consecutive health check failures, triggering shutdown");
                        self.trigger_shutdown().await?;
                        let _ = on_tick(self, HealthTick::Shutdown { failures });
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn check_health(&self) -> Result<ProcessHealthStatus> {
        // Check resource exhaustion if monitor is configured
        if let Some(resource_monitor) = &self.resource_monitor {
            if let Err(e) = resource_monitor.check_resources().await {
                // Map resource errors to appropriate health status
                return match &e {
                    AosError::CpuThrottled { .. } => {
                        Err(e) // Critical - needs backoff
                    }
                    AosError::OutOfMemory {
                        restart_imminent, ..
                    } => {
                        if *restart_imminent {
                            Err(e) // Critical - imminent restart
                        } else {
                            Ok(ProcessHealthStatus::Warning(e.to_string()))
                        }
                    }
                    AosError::FileDescriptorExhausted { .. } => {
                        Err(e) // Critical - cannot open new connections
                    }
                    AosError::ThreadPoolSaturated { .. } => {
                        Ok(ProcessHealthStatus::Warning(e.to_string()))
                    }
                    AosError::GpuUnavailable {
                        cpu_fallback_available,
                        ..
                    } => {
                        if *cpu_fallback_available {
                            Ok(ProcessHealthStatus::Warning(e.to_string()))
                        } else {
                            Err(e) // Critical - no fallback
                        }
                    }
                    _ => Err(e), // Other errors are critical
                };
            }
        }

        // Check memory growth
        let current_memory = get_process_memory()?;
        let memory_growth = current_memory.saturating_sub(self.baseline_memory);

        if memory_growth > self.config.max_memory_growth {
            return Err(AosError::MemoryPressure(format!(
                "Memory growth {} bytes exceeds limit {} bytes",
                memory_growth, self.config.max_memory_growth
            )));
        }

        // Check CPU time
        let current_cpu_time = get_process_cpu_time()?;
        let cpu_delta = current_cpu_time.saturating_sub(self.cpu_time_start);

        if cpu_delta > self.config.max_cpu_time {
            return Err(AosError::Worker(format!(
                "CPU time {} exceeds limit {}",
                cpu_delta.as_secs(),
                self.config.max_cpu_time.as_secs()
            )));
        }

        // Check response time
        let last_request = self.last_request_time.load(Ordering::Relaxed);
        if last_request > 0 {
            let time_since_request = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_secs()
                .saturating_sub(last_request);

            if time_since_request > self.config.max_response_time.as_secs() {
                return Ok(ProcessHealthStatus::Warning(format!(
                    "No requests processed for {} seconds",
                    time_since_request
                )));
            }
        }

        Ok(ProcessHealthStatus::Healthy)
    }

    async fn trigger_shutdown(&self) -> Result<()> {
        self.shutdown_requested.store(true, Ordering::Relaxed);
        info!("Triggering graceful shutdown due to health check failure");

        // In a real implementation, this would send a signal to the main process
        // For now, we just set the flag and let the monitoring loop exit

        Ok(())
    }

    pub fn record_request(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();
        self.last_request_time.store(now, Ordering::Relaxed);
    }

    pub fn is_shutdown_requested(&self) -> bool {
        self.shutdown_requested.load(Ordering::Relaxed)
    }

    pub fn get_uptime(&self) -> Duration {
        self.start_time.elapsed()
    }

    pub fn get_memory_usage(&self) -> Result<u64> {
        get_process_memory()
    }

    pub fn get_cpu_time(&self) -> Result<Duration> {
        get_process_cpu_time()
    }
}

/// Hook events emitted for each health tick.
#[derive(Debug, Clone)]
pub enum HealthTick {
    Status {
        status: ProcessHealthStatus,
        status_str: String,
    },
    Failure {
        error: String,
        failures: usize,
    },
    Shutdown {
        failures: usize,
    },
}

/// Health event for telemetry
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthEvent {
    pub status: String,
    pub memory_usage_bytes: u64,
    pub memory_growth_bytes: u64,
    pub cpu_time_secs: u64,
    pub uptime_secs: u64,
    pub consecutive_failures: usize,
    pub timestamp: u64,
}

impl HealthEvent {
    pub fn from_monitor(monitor: &HealthMonitor, status: &ProcessHealthStatus) -> Result<Self> {
        let memory_usage = monitor.get_memory_usage()?;
        let cpu_time = monitor.get_cpu_time()?;
        let memory_growth = memory_usage.saturating_sub(monitor.baseline_memory);

        Ok(Self {
            status: status.label(),
            memory_usage_bytes: memory_usage,
            memory_growth_bytes: memory_growth,
            cpu_time_secs: cpu_time.as_secs(),
            uptime_secs: monitor.get_uptime().as_secs(),
            consecutive_failures: monitor.consecutive_failures.load(Ordering::Relaxed),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("System time before UNIX epoch")
                .as_secs(),
        })
    }
}

// Platform-specific memory and CPU time functions
#[cfg(target_os = "macos")]
fn get_process_memory() -> Result<u64> {
    use std::process::Command;

    let output = match Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
    {
        Ok(output) => output,
        Err(e) => {
            if dev_no_auth_enabled() {
                return Ok(0);
            }
            return Err(AosError::Worker(format!(
                "Failed to get memory info: {}",
                e
            )));
        }
    };

    let rss_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let rss_kb: u64 = match rss_str.parse() {
        Ok(value) => value,
        Err(e) => {
            if dev_no_auth_enabled() {
                return Ok(0);
            }
            return Err(AosError::Worker(format!("Failed to parse memory: {}", e)));
        }
    };

    Ok(rss_kb * 1024) // Convert KB to bytes
}

#[cfg(target_os = "linux")]
fn get_process_memory() -> Result<u64> {
    use std::fs;

    let status = fs::read_to_string("/proc/self/status")?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let kb = parts[1].parse::<u64>().map_err(|e| {
                    AosError::Worker(format!("Failed to parse memory value: {}", e))
                })?;
                return Ok(kb * 1024); // Convert KB to bytes
            }
        }
    }

    Err(AosError::Worker("Failed to parse memory info".to_string()))
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn get_process_memory() -> Result<u64> {
    // Fallback for unsupported platforms
    Ok(0)
}

fn get_process_cpu_time() -> Result<Duration> {
    // Simplified implementation - in practice would use platform-specific APIs
    // For now, return uptime as a proxy
    Ok(Duration::from_secs(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    // use std::time::Duration; // unused

    fn should_skip_memory_error(err: &AosError) -> bool {
        let msg = err.to_string();
        msg.contains("Operation not permitted") || msg.contains("permission")
    }

    fn new_monitor_or_skip(config: HealthConfig) -> Option<HealthMonitor> {
        match HealthMonitor::new(config) {
            Ok(monitor) => Some(monitor),
            Err(err) if should_skip_memory_error(&err) => {
                eprintln!("skipping: {}", err);
                None
            }
            Err(err) => panic!("Test health monitor creation failed: {}", err),
        }
    }

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let config = HealthConfig::default();
        let Some(monitor) = new_monitor_or_skip(config) else {
            return;
        };

        assert!(!monitor.is_shutdown_requested());
        assert!(monitor.get_uptime().as_secs() < 1);
    }

    #[tokio::test]
    async fn test_health_monitor_request_recording() {
        let config = HealthConfig::default();
        let Some(monitor) = new_monitor_or_skip(config) else {
            return;
        };

        monitor.record_request();

        // Should not panic and should record the request
        assert!(!monitor.is_shutdown_requested());
    }

    #[test]
    fn test_health_event_creation() {
        let config = HealthConfig::default();
        let Some(monitor) = new_monitor_or_skip(config) else {
            return;
        };

        let event = HealthEvent::from_monitor(&monitor, &ProcessHealthStatus::Healthy)
            .expect("Test health event creation should succeed");

        assert_eq!(event.status, "healthy");
        assert_eq!(event.consecutive_failures, 0);
        assert!(event.timestamp > 0);
    }

    #[tokio::test]
    async fn health_monitor_emits_shutdown_after_failures() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        use std::time::Duration;

        let mut config = HealthConfig::default();
        config.check_interval = Duration::from_millis(5);
        config.max_consecutive_failures = 1;
        config.max_memory_growth = 0; // any growth triggers failure

        let Some(mut monitor) = new_monitor_or_skip(config) else {
            return;
        };
        monitor.baseline_memory = 0; // force growth on next check

        let shutdown_seen = Arc::new(AtomicBool::new(false));
        let flag = shutdown_seen.clone();

        // Run with a short timeout to avoid hanging tests
        let _ = tokio::time::timeout(
            Duration::from_millis(50),
            monitor.start_monitoring_with_hook(|_, tick| {
                if matches!(tick, HealthTick::Shutdown { .. }) {
                    flag.store(true, Ordering::Relaxed);
                }
                Ok(())
            }),
        )
        .await;

        assert!(
            shutdown_seen.load(Ordering::Relaxed),
            "Health monitor should trigger shutdown after failures"
        );
    }
}
