//! Resource management errors
//!
//! Covers memory pressure, quota limits, and resource exhaustion.
//!
//! This module defines structured error types for 5 resource exhaustion scenarios:
//! - CPU throttling
//! - Out of memory (OOM)
//! - File descriptor exhaustion
//! - Thread pool saturation
//! - GPU unavailability

use thiserror::Error;

/// Resource management errors
#[derive(Error, Debug)]
pub enum AosResourceError {
    /// Generic resource exhaustion
    #[error("Resource exhaustion: {0}")]
    Exhaustion(String),

    /// Memory pressure detected
    #[error("Memory pressure: {0}")]
    MemoryPressure(String),

    /// Memory allocation or management error
    #[error("Memory error: {0}")]
    Memory(String),

    /// Quota exceeded for a specific resource
    #[error("Quota exceeded for resource '{resource}'")]
    QuotaExceeded {
        resource: String,
        /// Failure code string (e.g., "KV_QUOTA_EXCEEDED")
        failure_code: Option<String>,
    },

    /// Resource temporarily unavailable
    #[error("Resource unavailable: {0}")]
    Unavailable(String),

    /// CPU usage exceeds limits and throttles the process
    #[error("CPU throttled: {reason} (usage: {usage_percent:.1}%, limit: {limit_percent:.1}%)")]
    CpuThrottled {
        /// Human-readable reason for throttling
        reason: String,
        /// Current CPU usage percentage
        usage_percent: f32,
        /// Configured CPU limit percentage
        limit_percent: f32,
        /// Recommended backoff duration in milliseconds
        backoff_ms: u64,
    },

    /// Memory usage hits OOM and the service may restart
    #[error("Out of memory: {reason} (used: {used_mb} MB, limit: {limit_mb} MB)")]
    OutOfMemory {
        /// Human-readable reason for OOM
        reason: String,
        /// Current memory usage in MB
        used_mb: u64,
        /// Memory limit in MB
        limit_mb: u64,
        /// Whether service restart is imminent
        restart_imminent: bool,
    },

    /// File descriptor limit is reached
    #[error("File descriptor limit reached: {current}/{limit} descriptors in use")]
    FileDescriptorExhausted {
        /// Current number of open file descriptors
        current: u64,
        /// Maximum allowed file descriptors
        limit: u64,
        /// Suggested action to resolve
        suggestion: String,
    },

    /// Thread pool is saturated
    #[error("Thread pool saturated: {active}/{max} threads busy, {queued} tasks queued")]
    ThreadPoolSaturated {
        /// Number of currently active threads
        active: usize,
        /// Maximum thread pool size
        max: usize,
        /// Number of tasks waiting in queue
        queued: usize,
        /// Estimated wait time in milliseconds
        estimated_wait_ms: u64,
    },

    /// GPU device is unavailable
    #[error("GPU unavailable: {reason}")]
    GpuUnavailable {
        /// Human-readable reason for unavailability
        reason: String,
        /// Device identifier if known
        device_id: Option<String>,
        /// Whether fallback to CPU is possible
        cpu_fallback_available: bool,
        /// Whether this is a transient condition that may recover
        is_transient: bool,
    },
}

impl AosResourceError {
    /// Check if this error indicates the system should back off
    pub fn should_backoff(&self) -> bool {
        matches!(
            self,
            Self::MemoryPressure(_)
                | Self::QuotaExceeded { .. }
                | Self::Exhaustion(_)
                | Self::CpuThrottled { .. }
                | Self::OutOfMemory { .. }
                | Self::FileDescriptorExhausted { .. }
                | Self::ThreadPoolSaturated { .. }
                | Self::GpuUnavailable {
                    is_transient: true,
                    ..
                }
        )
    }

    /// Get recommended backoff duration in milliseconds for this error
    pub fn recommended_backoff_ms(&self) -> u64 {
        match self {
            Self::CpuThrottled { backoff_ms, .. } => *backoff_ms,
            Self::ThreadPoolSaturated {
                estimated_wait_ms, ..
            } => *estimated_wait_ms,
            Self::GpuUnavailable {
                is_transient: true, ..
            } => 5000, // 5 seconds
            Self::FileDescriptorExhausted { .. } => 1000, // 1 second
            Self::OutOfMemory { .. } => 10000,            // 10 seconds
            Self::MemoryPressure(_) => 500,               // 500ms
            Self::QuotaExceeded { .. } => 1000,           // 1 second
            Self::Exhaustion(_) => 100,                   // 100ms
            _ => 100,                                     // default
        }
    }

    /// Check if this error indicates the service should restart
    pub fn requires_restart(&self) -> bool {
        matches!(
            self,
            Self::OutOfMemory {
                restart_imminent: true,
                ..
            }
        )
    }

    /// Check if this error is a transient condition that may recover
    pub fn is_transient(&self) -> bool {
        match self {
            Self::GpuUnavailable { is_transient, .. } => *is_transient,
            Self::CpuThrottled { .. }
            | Self::ThreadPoolSaturated { .. }
            | Self::FileDescriptorExhausted { .. } => true,
            Self::OutOfMemory {
                restart_imminent, ..
            } => !*restart_imminent,
            _ => true,
        }
    }
}
