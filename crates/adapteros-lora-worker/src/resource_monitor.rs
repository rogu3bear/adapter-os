//! System resource monitoring for exhaustion detection
//!
//! Monitors CPU, memory, file descriptors, thread pools, and GPU availability.
//! This module provides structured detection and reporting of resource exhaustion
//! conditions to enable graceful degradation and proper error handling.

use adapteros_core::{AosError, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Resource exhaustion thresholds
#[derive(Debug, Clone)]
pub struct ResourceThresholds {
    /// CPU usage percent that triggers throttling warning
    pub cpu_warning_percent: f32,
    /// CPU usage percent that triggers throttling error
    pub cpu_critical_percent: f32,
    /// Memory usage percent that triggers warning
    pub memory_warning_percent: f32,
    /// Memory usage percent that triggers OOM handling
    pub memory_critical_percent: f32,
    /// File descriptor usage percent that triggers warning
    pub fd_warning_percent: f32,
    /// Thread pool queue depth that triggers warning
    pub thread_queue_warning: usize,
    /// GPU health check interval
    pub gpu_check_interval: Duration,
}

impl Default for ResourceThresholds {
    fn default() -> Self {
        Self {
            cpu_warning_percent: 80.0,
            cpu_critical_percent: 95.0,
            memory_warning_percent: 75.0,
            memory_critical_percent: 90.0,
            fd_warning_percent: 80.0,
            thread_queue_warning: 100,
            gpu_check_interval: Duration::from_secs(30),
        }
    }
}

/// Current resource state
#[derive(Debug, Clone)]
pub struct ResourceState {
    /// Current CPU usage percentage
    pub cpu_usage_percent: f32,
    /// Memory currently used in MB
    pub memory_used_mb: u64,
    /// Memory limit in MB
    pub memory_limit_mb: u64,
    /// Current open file descriptors
    pub fd_current: u64,
    /// File descriptor limit
    pub fd_limit: u64,
    /// Number of active threads
    pub thread_active: usize,
    /// Maximum thread pool size
    pub thread_max: usize,
    /// Number of queued tasks
    pub thread_queued: usize,
    /// Whether GPU is currently available
    pub gpu_available: bool,
    /// Last time GPU was checked
    pub last_gpu_check: Instant,
}

impl Default for ResourceState {
    fn default() -> Self {
        let thread_max = std::thread::available_parallelism()
            .map(|n| n.get() * 4)
            .unwrap_or(16);

        Self {
            cpu_usage_percent: 0.0,
            memory_used_mb: 0,
            memory_limit_mb: 8192, // Default 8GB
            fd_current: 0,
            fd_limit: 1024,
            thread_active: 0,
            thread_max,
            thread_queued: 0,
            gpu_available: true,
            last_gpu_check: Instant::now(),
        }
    }
}

/// Resource monitor for exhaustion detection
pub struct ResourceMonitor {
    thresholds: ResourceThresholds,
    state: Arc<RwLock<ResourceState>>,
    gpu_healthy: AtomicBool,
    /// Track queued tasks for thread pool saturation detection
    queued_tasks: AtomicUsize,
    /// Track active tasks
    active_tasks: AtomicUsize,
    /// Last FD check timestamp (seconds since epoch)
    #[allow(dead_code)]
    last_fd_check: AtomicU64,
}

impl ResourceMonitor {
    /// Create a new resource monitor with the given thresholds
    pub fn new(thresholds: ResourceThresholds) -> Result<Self> {
        let initial_state = Self::collect_initial_state()?;
        Ok(Self {
            thresholds,
            state: Arc::new(RwLock::new(initial_state)),
            gpu_healthy: AtomicBool::new(true),
            queued_tasks: AtomicUsize::new(0),
            active_tasks: AtomicUsize::new(0),
            last_fd_check: AtomicU64::new(0),
        })
    }

    /// Create a new resource monitor with default thresholds
    pub fn with_defaults() -> Result<Self> {
        Self::new(ResourceThresholds::default())
    }

    /// Check all resources and return first critical error if any
    pub async fn check_resources(&self) -> Result<()> {
        let mut state = self.state.write().await;

        // Update state from system
        self.update_state(&mut state)?;

        // Check each resource type
        self.check_cpu(&state)?;
        self.check_memory(&state)?;
        self.check_file_descriptors(&state)?;
        self.check_thread_pool(&state)?;
        self.check_gpu(&mut state).await?;

        Ok(())
    }

    /// Check CPU usage and return error if critical
    fn check_cpu(&self, state: &ResourceState) -> Result<()> {
        if state.cpu_usage_percent >= self.thresholds.cpu_critical_percent {
            return Err(AosError::CpuThrottled {
                reason: "CPU usage critical".to_string(),
                usage_percent: state.cpu_usage_percent,
                limit_percent: self.thresholds.cpu_critical_percent,
                backoff_ms: 1000,
            });
        }
        if state.cpu_usage_percent >= self.thresholds.cpu_warning_percent {
            warn!(
                cpu_percent = state.cpu_usage_percent,
                "CPU usage approaching limit"
            );
        }
        Ok(())
    }

    /// Check memory usage and return error if critical
    fn check_memory(&self, state: &ResourceState) -> Result<()> {
        if state.memory_limit_mb == 0 {
            return Ok(()); // Skip if limit unknown
        }

        let usage_percent = (state.memory_used_mb as f32 / state.memory_limit_mb as f32) * 100.0;

        if usage_percent >= self.thresholds.memory_critical_percent {
            return Err(AosError::OutOfMemory {
                reason: "Memory usage critical".to_string(),
                used_mb: state.memory_used_mb,
                limit_mb: state.memory_limit_mb,
                restart_imminent: usage_percent >= 95.0,
            });
        }
        if usage_percent >= self.thresholds.memory_warning_percent {
            warn!(
                used_mb = state.memory_used_mb,
                limit_mb = state.memory_limit_mb,
                "Memory usage approaching limit"
            );
        }
        Ok(())
    }

    /// Check file descriptor usage and return error if exhausted
    fn check_file_descriptors(&self, state: &ResourceState) -> Result<()> {
        if state.fd_limit == 0 {
            return Ok(()); // Skip if limit unknown
        }

        let usage_percent = (state.fd_current as f32 / state.fd_limit as f32) * 100.0;

        if usage_percent >= 95.0 {
            return Err(AosError::FileDescriptorExhausted {
                current: state.fd_current,
                limit: state.fd_limit,
                suggestion: "Close idle connections or increase ulimit -n".to_string(),
            });
        }
        if usage_percent >= self.thresholds.fd_warning_percent {
            warn!(
                current = state.fd_current,
                limit = state.fd_limit,
                "File descriptor usage high"
            );
        }
        Ok(())
    }

    /// Check thread pool and return error if saturated
    fn check_thread_pool(&self, state: &ResourceState) -> Result<()> {
        if state.thread_active >= state.thread_max
            && state.thread_queued >= self.thresholds.thread_queue_warning
        {
            return Err(AosError::ThreadPoolSaturated {
                active: state.thread_active,
                max: state.thread_max,
                queued: state.thread_queued,
                estimated_wait_ms: (state.thread_queued as u64) * 50,
            });
        }
        Ok(())
    }

    /// Check GPU availability
    async fn check_gpu(&self, state: &mut ResourceState) -> Result<()> {
        // Only check GPU at configured interval
        if state.last_gpu_check.elapsed() < self.thresholds.gpu_check_interval {
            return Ok(());
        }

        state.last_gpu_check = Instant::now();

        // Platform-specific GPU check
        let gpu_available = self.probe_gpu_availability().await;

        if !gpu_available && state.gpu_available {
            // GPU became unavailable
            error!("GPU device became unavailable");
            self.gpu_healthy.store(false, Ordering::Relaxed);
            state.gpu_available = false;

            return Err(AosError::GpuUnavailable {
                reason: "Metal device not responding".to_string(),
                device_id: None,
                cpu_fallback_available: cfg!(feature = "coreml-backend"),
                is_transient: true,
            });
        } else if gpu_available && !state.gpu_available {
            // GPU recovered
            info!("GPU device recovered");
            self.gpu_healthy.store(true, Ordering::Relaxed);
            state.gpu_available = true;
        }

        Ok(())
    }

    /// Probe GPU availability (platform-specific)
    #[cfg(target_os = "macos")]
    async fn probe_gpu_availability(&self) -> bool {
        // Use system_profiler to check Metal GPU availability
        // This is a lightweight check that doesn't require Metal framework
        tokio::task::spawn_blocking(|| {
            std::process::Command::new("system_profiler")
                .args(["SPDisplaysDataType", "-detailLevel", "mini"])
                .output()
                .map(|o| o.status.success() && !o.stdout.is_empty())
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    async fn probe_gpu_availability(&self) -> bool {
        // No GPU checking on non-macOS platforms
        true
    }

    /// Collect initial resource state
    fn collect_initial_state() -> Result<ResourceState> {
        let memory_limit_mb = get_memory_limit_mb().unwrap_or(8192);
        let fd_limit = get_fd_limit().unwrap_or(1024);
        let thread_max = std::thread::available_parallelism()
            .map(|n| n.get() * 4)
            .unwrap_or(16);

        Ok(ResourceState {
            cpu_usage_percent: 0.0,
            memory_used_mb: 0,
            memory_limit_mb,
            fd_current: 0,
            fd_limit,
            thread_active: 0,
            thread_max,
            thread_queued: 0,
            gpu_available: true,
            last_gpu_check: Instant::now(),
        })
    }

    /// Update current resource state from system
    fn update_state(&self, state: &mut ResourceState) -> Result<()> {
        if let Ok(mem) = get_process_memory_mb() {
            state.memory_used_mb = mem;
        }
        if let Ok(fds) = get_open_fd_count() {
            state.fd_current = fds;
        }

        // Update thread pool stats from atomic counters
        state.thread_queued = self.queued_tasks.load(Ordering::Relaxed);
        state.thread_active = self.active_tasks.load(Ordering::Relaxed);

        Ok(())
    }

    /// Record that a task has been queued
    pub fn record_task_queued(&self) {
        self.queued_tasks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that a task has started executing
    pub fn record_task_started(&self) {
        self.queued_tasks
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            })
            .ok();
        self.active_tasks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record that a task has completed
    pub fn record_task_completed(&self) {
        self.active_tasks
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            })
            .ok();
    }

    /// Get the current GPU health status
    pub fn is_gpu_healthy(&self) -> bool {
        self.gpu_healthy.load(Ordering::Relaxed)
    }

    /// Mark GPU as unavailable (called when GPU errors are detected elsewhere)
    pub fn mark_gpu_unavailable(&self) {
        self.gpu_healthy.store(false, Ordering::Relaxed);
    }

    /// Mark GPU as available (called when GPU operations succeed)
    pub fn mark_gpu_available(&self) {
        self.gpu_healthy.store(true, Ordering::Relaxed);
    }

    /// Get a snapshot of current resource state
    pub async fn get_state(&self) -> ResourceState {
        let mut state = self.state.read().await.clone();
        state.thread_queued = self.queued_tasks.load(Ordering::Relaxed);
        state.thread_active = self.active_tasks.load(Ordering::Relaxed);
        state
    }
}

// Platform-specific helper functions

/// Get system memory limit in MB
#[cfg(target_os = "macos")]
fn get_memory_limit_mb() -> Result<u64> {
    use std::process::Command;
    let output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .map_err(|e| AosError::Unavailable(format!("Failed to get memory limit: {}", e)))?;

    let bytes: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(8 * 1024 * 1024 * 1024); // Default 8GB

    Ok(bytes / (1024 * 1024))
}

#[cfg(target_os = "linux")]
fn get_memory_limit_mb() -> Result<u64> {
    // Check cgroup limit first, then fall back to total memory
    if let Ok(limit) = std::fs::read_to_string("/sys/fs/cgroup/memory/memory.limit_in_bytes") {
        if let Ok(bytes) = limit.trim().parse::<u64>() {
            if bytes < u64::MAX / 2 {
                return Ok(bytes / (1024 * 1024));
            }
        }
    }

    // Fall back to total system memory
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if line.starts_with("MemTotal:") {
                let kb: u64 = line
                    .split_whitespace()
                    .nth(1)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(8 * 1024 * 1024);
                return Ok(kb / 1024);
            }
        }
    }
    Ok(8 * 1024) // Default 8GB
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn get_memory_limit_mb() -> Result<u64> {
    Ok(8 * 1024) // Default 8GB
}

/// Get current process memory usage in MB
#[cfg(target_os = "macos")]
fn get_process_memory_mb() -> Result<u64> {
    use std::process::Command;
    let output = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .map_err(|e| AosError::Unavailable(format!("Failed to get memory info: {}", e)))?;

    let rss_kb: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .unwrap_or(0);

    Ok(rss_kb / 1024)
}

#[cfg(target_os = "linux")]
fn get_process_memory_mb() -> Result<u64> {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if line.starts_with("VmRSS:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<u64>() {
                        return Ok(kb / 1024);
                    }
                }
            }
        }
    }
    Ok(0)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn get_process_memory_mb() -> Result<u64> {
    Ok(0)
}

/// Get file descriptor limit
fn get_fd_limit() -> Result<u64> {
    #[cfg(unix)]
    {
        use libc::{getrlimit, rlimit, RLIMIT_NOFILE};
        let mut rlim = rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        unsafe {
            if getrlimit(RLIMIT_NOFILE, &mut rlim) == 0 {
                return Ok(rlim.rlim_cur);
            }
        }
    }
    Ok(1024) // Default
}

/// Get count of open file descriptors
fn get_open_fd_count() -> Result<u64> {
    #[cfg(target_os = "linux")]
    {
        let count = std::fs::read_dir("/proc/self/fd")
            .map(|d| d.count() as u64)
            .unwrap_or(0);
        return Ok(count);
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, use lsof but cache the result to avoid overhead
        use std::process::Command;
        let output = Command::new("lsof")
            .args(["-p", &std::process::id().to_string()])
            .output();

        if let Ok(out) = output {
            let count = String::from_utf8_lossy(&out.stdout).lines().count() as u64;
            return Ok(count.saturating_sub(1)); // Subtract header
        }
        Ok(0)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_thresholds() {
        let thresholds = ResourceThresholds::default();
        assert_eq!(thresholds.cpu_warning_percent, 80.0);
        assert_eq!(thresholds.cpu_critical_percent, 95.0);
        assert_eq!(thresholds.memory_warning_percent, 75.0);
        assert_eq!(thresholds.memory_critical_percent, 90.0);
    }

    #[test]
    fn test_resource_state_default() {
        let state = ResourceState::default();
        assert!(state.gpu_available);
        assert_eq!(state.thread_queued, 0);
    }

    #[tokio::test]
    async fn test_resource_monitor_creation() {
        let monitor = ResourceMonitor::with_defaults();
        assert!(monitor.is_ok());

        let monitor = monitor.unwrap();
        assert!(monitor.is_gpu_healthy());
    }

    #[tokio::test]
    async fn test_task_tracking() {
        let monitor = ResourceMonitor::with_defaults().unwrap();

        monitor.record_task_queued();
        monitor.record_task_queued();

        monitor.record_task_started();
        monitor.record_task_completed();

        // Should have 1 queued, 0 active
        let state = monitor.get_state().await;
        assert_eq!(state.thread_queued, 1);
        assert_eq!(state.thread_active, 0);
    }

    #[test]
    fn test_cpu_throttled_error() {
        let state = ResourceState {
            cpu_usage_percent: 96.0,
            ..Default::default()
        };

        let thresholds = ResourceThresholds::default();
        let monitor = ResourceMonitor {
            thresholds,
            state: Arc::new(RwLock::new(ResourceState::default())),
            gpu_healthy: AtomicBool::new(true),
            queued_tasks: AtomicUsize::new(0),
            active_tasks: AtomicUsize::new(0),
            last_fd_check: AtomicU64::new(0),
        };

        let result = monitor.check_cpu(&state);
        assert!(result.is_err());

        if let Err(AosError::CpuThrottled { usage_percent, .. }) = result {
            assert_eq!(usage_percent, 96.0);
        } else {
            panic!("Expected CpuThrottled error");
        }
    }

    #[test]
    fn test_memory_critical_error() {
        let state = ResourceState {
            memory_used_mb: 9500,
            memory_limit_mb: 10000,
            ..Default::default()
        };

        let thresholds = ResourceThresholds::default();
        let monitor = ResourceMonitor {
            thresholds,
            state: Arc::new(RwLock::new(ResourceState::default())),
            gpu_healthy: AtomicBool::new(true),
            queued_tasks: AtomicUsize::new(0),
            active_tasks: AtomicUsize::new(0),
            last_fd_check: AtomicU64::new(0),
        };

        let result = monitor.check_memory(&state);
        assert!(result.is_err());

        if let Err(AosError::OutOfMemory {
            restart_imminent, ..
        }) = result
        {
            assert!(restart_imminent);
        } else {
            panic!("Expected OutOfMemory error");
        }
    }

    #[test]
    fn test_fd_exhausted_error() {
        let state = ResourceState {
            fd_current: 980,
            fd_limit: 1000,
            ..Default::default()
        };

        let thresholds = ResourceThresholds::default();
        let monitor = ResourceMonitor {
            thresholds,
            state: Arc::new(RwLock::new(ResourceState::default())),
            gpu_healthy: AtomicBool::new(true),
            queued_tasks: AtomicUsize::new(0),
            active_tasks: AtomicUsize::new(0),
            last_fd_check: AtomicU64::new(0),
        };

        let result = monitor.check_file_descriptors(&state);
        assert!(result.is_err());
    }

    #[test]
    fn test_thread_pool_saturated() {
        let state = ResourceState {
            thread_active: 16,
            thread_max: 16,
            thread_queued: 150,
            ..Default::default()
        };

        let thresholds = ResourceThresholds::default();
        let monitor = ResourceMonitor {
            thresholds,
            state: Arc::new(RwLock::new(ResourceState::default())),
            gpu_healthy: AtomicBool::new(true),
            queued_tasks: AtomicUsize::new(0),
            active_tasks: AtomicUsize::new(0),
            last_fd_check: AtomicU64::new(0),
        };

        let result = monitor.check_thread_pool(&state);
        assert!(result.is_err());
    }
}
