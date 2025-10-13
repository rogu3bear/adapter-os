//! Process health monitoring and lifecycle management
//!
//! Implements health checks and process monitoring to prevent runaway processes.
//! Aligns with Isolation Ruleset #8 and Memory Ruleset #12 from policy enforcement.

use adapteros_core::{AosError, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
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

/// Health status
#[derive(Debug, Clone)]
pub enum HealthStatus {
    Healthy,
    Warning(String),
    Critical(String),
    Failing,
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
        })
    }

    pub async fn start_monitoring(&self) -> Result<()> {
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
                    if !matches!(status, HealthStatus::Healthy) {
                        warn!("Health check warning: {:?}", status);
                    }
                }
                Err(e) => {
                    let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
                    error!("Health check failed ({}): {}", failures, e);

                    if failures >= self.config.max_consecutive_failures {
                        error!("Too many consecutive health check failures, triggering shutdown");
                        self.trigger_shutdown().await?;
                        break;
                    }
                }
            }
        }

        Ok(())
    }

    async fn check_health(&self) -> Result<HealthStatus> {
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
                return Ok(HealthStatus::Warning(format!(
                    "No requests processed for {} seconds",
                    time_since_request
                )));
            }
        }

        Ok(HealthStatus::Healthy)
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
    pub fn from_monitor(monitor: &HealthMonitor) -> Result<Self> {
        let memory_usage = monitor.get_memory_usage()?;
        let cpu_time = monitor.get_cpu_time()?;
        let memory_growth = memory_usage.saturating_sub(monitor.baseline_memory);

        Ok(Self {
            status: "healthy".to_string(), // Would be determined by health check
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

    let output = Command::new("ps")
        .args(&["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .map_err(|e| AosError::Worker(format!("Failed to get memory info: {}", e)))?;

    let rss_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let rss_kb: u64 = rss_str
        .parse()
        .map_err(|e| AosError::Worker(format!("Failed to parse memory: {}", e)))?;

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
                return Ok(parts[1].parse::<u64>()? * 1024); // Convert KB to bytes
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

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let config = HealthConfig::default();
        let monitor =
            HealthMonitor::new(config).expect("Test health monitor creation should succeed");

        assert!(!monitor.is_shutdown_requested());
        assert!(monitor.get_uptime().as_secs() < 1);
    }

    #[tokio::test]
    async fn test_health_monitor_request_recording() {
        let config = HealthConfig::default();
        let monitor =
            HealthMonitor::new(config).expect("Test health monitor creation should succeed");

        monitor.record_request();

        // Should not panic and should record the request
        assert!(!monitor.is_shutdown_requested());
    }

    #[test]
    fn test_health_event_creation() {
        let config = HealthConfig::default();
        let monitor =
            HealthMonitor::new(config).expect("Test health monitor creation should succeed");

        let event =
            HealthEvent::from_monitor(&monitor).expect("Test health event creation should succeed");

        assert_eq!(event.status, "healthy");
        assert_eq!(event.consecutive_failures, 0);
        assert!(event.timestamp > 0);
    }
}
