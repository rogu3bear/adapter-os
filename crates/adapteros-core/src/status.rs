//! Status types for adapterOS system monitoring
//!
//! Defines the data structures used for system status reporting
//! across menu bar, web UI, and other monitoring interfaces.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Canonical health status for components across adapterOS.
///
/// Use this type for health checks across all subsystems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum HealthStatus {
    /// Component is functioning normally
    Healthy,
    /// Component is functional but experiencing issues
    Degraded,
    /// Component is not functioning
    Unhealthy,
    /// Health status cannot be determined
    #[default]
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => write!(f, "healthy"),
            Self::Degraded => write!(f, "degraded"),
            Self::Unhealthy => write!(f, "unhealthy"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Detailed health check result with metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    /// Overall health status
    pub status: HealthStatus,
    /// Timestamp of the check
    pub timestamp: SystemTime,
    /// Response time for the check
    pub response_time: Duration,
    /// Error message if unhealthy
    pub error: Option<String>,
    /// Additional metrics
    pub metrics: HashMap<String, serde_json::Value>,
}

impl Default for HealthCheckResult {
    fn default() -> Self {
        Self {
            status: HealthStatus::Unknown,
            timestamp: SystemTime::now(),
            response_time: Duration::ZERO,
            error: None,
            metrics: HashMap::new(),
        }
    }
}

/// Status of a managed service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    /// Service identifier
    pub id: String,
    /// Human-readable service name
    pub name: String,
    /// Current state: "stopped" | "starting" | "running" | "stopping" | "failed" | "restarting"
    pub state: String,
    /// Process ID if running
    pub pid: Option<u32>,
    /// Port number if applicable
    pub port: Option<u16>,
    /// Health status: "unknown" | "healthy" | "unhealthy" | "checking"
    pub health_status: String,
    /// Number of restart attempts
    pub restart_count: u32,
    /// Last error message if any
    pub last_error: Option<String>,
}

/// Status reported to menu bar app
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(non_camel_case_types)] // Intentional: matches adapterOS branding
pub struct adapterOSStatus {
    /// Schema version for forward/backward compatibility
    pub schema_version: String,
    /// System status: "ok" | "degraded" | "error"
    pub status: String,
    /// Uptime in seconds since control plane started
    pub uptime_secs: u64,
    /// Number of adapters currently loaded
    pub adapters_loaded: usize,
    /// Whether deterministic mode is enabled
    pub deterministic: bool,
    /// Short kernel hash (first 8 chars)
    pub kernel_hash: String,
    /// Telemetry mode: "local" | "disabled"
    pub telemetry_mode: String,
    /// Number of active workers
    pub worker_count: usize,
    /// Whether base model is loaded
    pub base_model_loaded: bool,
    /// Base model identifier (optional)
    pub base_model_id: Option<String>,
    /// Base model display name (optional)
    pub base_model_name: Option<String>,
    /// Base model status: "ready" | "loading" | "error"
    pub base_model_status: String,
    /// Base model memory usage in MB (optional)
    pub base_model_memory_mb: Option<usize>,
    /// Service status information from supervisor (optional)
    pub services: Option<Vec<ServiceStatus>>,
}

impl adapterOSStatus {
    /// Get uptime formatted as a human-readable string
    pub fn uptime_formatted(&self) -> String {
        let hours = self.uptime_secs / 3600;
        let minutes = (self.uptime_secs % 3600) / 60;
        if hours > 0 {
            format!("{}h {}m", hours, minutes)
        } else if minutes > 0 {
            format!("{}m", minutes)
        } else {
            format!("{}s", self.uptime_secs)
        }
    }

    /// Get kernel hash as short form (first 8 chars)
    pub fn kernel_hash_short(&self) -> String {
        self.kernel_hash.chars().take(8).collect()
    }

    /// Get all failed services
    pub fn failed_services(&self) -> Vec<&ServiceStatus> {
        self.services
            .as_ref()
            .map(|services| services.iter().filter(|s| s.state == "failed").collect())
            .unwrap_or_default()
    }

    /// Get all non-running services (stopped or failed)
    pub fn non_running_services(&self) -> Vec<&ServiceStatus> {
        self.services
            .as_ref()
            .map(|services| {
                services
                    .iter()
                    .filter(|s| s.state == "stopped" || s.state == "failed")
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if any services have failures
    pub fn has_service_failures(&self) -> bool {
        !self.failed_services().is_empty()
    }
}
