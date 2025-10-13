//! Policy integration for system metrics
//!
//! Provides policy enforcement integration for system metrics following
//! AdapterOS policy patterns and memory ruleset enforcement.

use crate::{SystemMetrics, ThresholdsConfig};
use adapteros_core::{AosError, Result};
use adapteros_policy::PolicyEngine;
use tracing::warn;

/// System metrics policy enforcer
pub struct SystemMetricsPolicy {
    thresholds: ThresholdsConfig,
    policy_engine: Option<PolicyEngine>,
}

impl SystemMetricsPolicy {
    /// Create a new system metrics policy enforcer
    pub fn new(thresholds: ThresholdsConfig) -> Self {
        Self {
            thresholds,
            policy_engine: None,
        }
    }

    /// Create with policy engine integration
    pub fn with_policy_engine(thresholds: ThresholdsConfig, policy_engine: PolicyEngine) -> Self {
        Self {
            thresholds,
            policy_engine: Some(policy_engine),
        }
    }

    /// Check if system metrics meet policy thresholds
    pub fn check_thresholds(&self, metrics: &SystemMetrics) -> Result<()> {
        // Check CPU usage threshold
        if metrics.cpu_usage > self.thresholds.cpu_critical as f64 {
            return Err(AosError::PerformanceViolation(format!(
                "CPU usage {:.1}% exceeds critical threshold {:.1}%",
                metrics.cpu_usage, self.thresholds.cpu_critical
            )));
        }

        // Check memory usage threshold
        if metrics.memory_usage > self.thresholds.memory_critical as f64 {
            return Err(AosError::MemoryPressure(format!(
                "Memory usage {:.1}% exceeds critical threshold {:.1}%",
                metrics.memory_usage, self.thresholds.memory_critical
            )));
        }

        // Check disk usage threshold
        if metrics.disk_io.usage_percent > self.thresholds.disk_critical {
            return Err(AosError::ResourceExhaustion(format!(
                "Disk usage {:.1}% exceeds critical threshold {:.1}%",
                metrics.disk_io.usage_percent, self.thresholds.disk_critical
            )));
        }

        // Check GPU utilization threshold
        if let Some(gpu_util) = metrics.gpu_metrics.utilization {
            if gpu_util > self.thresholds.gpu_critical as f64 {
                return Err(AosError::PerformanceViolation(format!(
                    "GPU utilization {:.1}% exceeds critical threshold {:.1}%",
                    gpu_util, self.thresholds.gpu_critical
                )));
            }
        }

        Ok(())
    }

    /// Check memory headroom policy (Memory Ruleset #12)
    pub fn check_memory_headroom(&self, headroom_pct: f32) -> Result<()> {
        if headroom_pct < self.thresholds.min_memory_headroom {
            return Err(AosError::MemoryPressure(format!(
                "Insufficient memory headroom: {:.1}% < {:.1}% (Memory Ruleset #12)",
                headroom_pct, self.thresholds.min_memory_headroom
            )));
        }
        Ok(())
    }

    /// Check performance budgets (Performance Ruleset #11)
    pub fn check_performance_budgets(&self, metrics: &SystemMetrics) -> Result<()> {
        // Check latency budget (would need additional metrics)
        // Check throughput budget (would need additional metrics)
        // Check router overhead budget (would need additional metrics)

        // For now, just check basic thresholds
        self.check_thresholds(metrics)
    }

    /// Get policy violation details
    pub fn get_violations(&self, metrics: &SystemMetrics) -> Vec<PolicyViolation> {
        let mut violations = Vec::new();

        // CPU usage violations
        if metrics.cpu_usage > self.thresholds.cpu_critical as f64 {
            violations.push(PolicyViolation {
                metric: "cpu_usage".to_string(),
                current_value: metrics.cpu_usage,
                threshold_value: self.thresholds.cpu_critical as f64,
                severity: ViolationSeverity::Critical,
                message: format!(
                    "CPU usage {:.1}% exceeds critical threshold {:.1}%",
                    metrics.cpu_usage, self.thresholds.cpu_critical
                ),
            });
        } else if metrics.cpu_usage > self.thresholds.cpu_warning as f64 {
            violations.push(PolicyViolation {
                metric: "cpu_usage".to_string(),
                current_value: metrics.cpu_usage,
                threshold_value: self.thresholds.cpu_warning as f64,
                severity: ViolationSeverity::Warning,
                message: format!(
                    "CPU usage {:.1}% exceeds warning threshold {:.1}%",
                    metrics.cpu_usage, self.thresholds.cpu_warning
                ),
            });
        }

        // Memory usage violations
        if metrics.memory_usage > self.thresholds.memory_critical as f64 {
            violations.push(PolicyViolation {
                metric: "memory_usage".to_string(),
                current_value: metrics.memory_usage,
                threshold_value: self.thresholds.memory_critical as f64,
                severity: ViolationSeverity::Critical,
                message: format!(
                    "Memory usage {:.1}% exceeds critical threshold {:.1}%",
                    metrics.memory_usage, self.thresholds.memory_critical
                ),
            });
        } else if metrics.memory_usage > self.thresholds.memory_warning as f64 {
            violations.push(PolicyViolation {
                metric: "memory_usage".to_string(),
                current_value: metrics.memory_usage,
                threshold_value: self.thresholds.memory_warning as f64,
                severity: ViolationSeverity::Warning,
                message: format!(
                    "Memory usage {:.1}% exceeds warning threshold {:.1}%",
                    metrics.memory_usage, self.thresholds.memory_warning
                ),
            });
        }

        // Disk usage violations
        if metrics.disk_io.usage_percent > self.thresholds.disk_critical {
            violations.push(PolicyViolation {
                metric: "disk_usage".to_string(),
                current_value: metrics.disk_io.usage_percent as f64,
                threshold_value: self.thresholds.disk_critical as f64,
                severity: ViolationSeverity::Critical,
                message: format!(
                    "Disk usage {:.1}% exceeds critical threshold {:.1}%",
                    metrics.disk_io.usage_percent, self.thresholds.disk_critical
                ),
            });
        } else if metrics.disk_io.usage_percent > self.thresholds.disk_warning {
            violations.push(PolicyViolation {
                metric: "disk_usage".to_string(),
                current_value: metrics.disk_io.usage_percent as f64,
                threshold_value: self.thresholds.disk_warning as f64,
                severity: ViolationSeverity::Warning,
                message: format!(
                    "Disk usage {:.1}% exceeds warning threshold {:.1}%",
                    metrics.disk_io.usage_percent, self.thresholds.disk_warning
                ),
            });
        }

        // GPU utilization violations
        if let Some(gpu_util) = metrics.gpu_metrics.utilization {
            if gpu_util > self.thresholds.gpu_critical as f64 {
                violations.push(PolicyViolation {
                    metric: "gpu_utilization".to_string(),
                    current_value: gpu_util,
                    threshold_value: self.thresholds.gpu_critical as f64,
                    severity: ViolationSeverity::Critical,
                    message: format!(
                        "GPU utilization {:.1}% exceeds critical threshold {:.1}%",
                        gpu_util, self.thresholds.gpu_critical
                    ),
                });
            } else if gpu_util > self.thresholds.gpu_warning as f64 {
                violations.push(PolicyViolation {
                    metric: "gpu_utilization".to_string(),
                    current_value: gpu_util,
                    threshold_value: self.thresholds.gpu_warning as f64,
                    severity: ViolationSeverity::Warning,
                    message: format!(
                        "GPU utilization {:.1}% exceeds warning threshold {:.1}%",
                        gpu_util, self.thresholds.gpu_warning
                    ),
                });
            }
        }

        violations
    }

    /// Check if system is healthy according to policy
    pub fn is_healthy(&self, metrics: &SystemMetrics) -> bool {
        self.check_thresholds(metrics).is_ok()
    }

    /// Get health status with details
    pub fn get_health_status(&self, metrics: &SystemMetrics) -> SystemHealthStatus {
        let violations = self.get_violations(metrics);

        if violations.is_empty() {
            SystemHealthStatus::Healthy
        } else if violations
            .iter()
            .any(|v| v.severity == ViolationSeverity::Critical)
        {
            SystemHealthStatus::Critical
        } else {
            SystemHealthStatus::Warning
        }
    }
}

/// Policy violation details
#[derive(Debug, Clone)]
pub struct PolicyViolation {
    pub metric: String,
    pub current_value: f64,   // Align with metric types
    pub threshold_value: f64, // Align with threshold types
    pub severity: ViolationSeverity,
    pub message: String,
}

/// Violation severity levels
#[derive(Debug, Clone, PartialEq)]
pub enum ViolationSeverity {
    Warning,
    Critical,
}

/// System health status
#[derive(Debug, Clone, PartialEq)]
pub enum SystemHealthStatus {
    Healthy,
    Warning,
    Critical,
}

impl SystemHealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SystemHealthStatus::Healthy => "healthy",
            SystemHealthStatus::Warning => "warning",
            SystemHealthStatus::Critical => "critical",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DiskMetrics, GpuMetrics, NetworkMetrics};
    use std::time::SystemTime;

    fn create_test_metrics(cpu_usage: f64, memory_usage: f64, disk_usage: f32) -> SystemMetrics {
        SystemMetrics {
            cpu_usage,
            memory_usage,
            disk_io: DiskMetrics {
                read_bytes: 1000,
                write_bytes: 1000,
                read_ops: 10,
                write_ops: 10,
                usage_percent: disk_usage,
                available_bytes: 1000,
                total_bytes: 1000,
            },
            network_io: NetworkMetrics {
                rx_bytes: 1000,
                tx_bytes: 1000,
                rx_packets: 10,
                tx_packets: 10,
                bandwidth_mbps: 1.0,
            },
            gpu_metrics: GpuMetrics::default(),
            timestamp: SystemTime::now(),
        }
    }

    #[test]
    fn test_healthy_metrics() {
        let thresholds = crate::ThresholdsConfig::default();
        let policy = SystemMetricsPolicy::new(thresholds);
        let metrics = create_test_metrics(50.0, 60.0, 70.0);

        assert!(policy.check_thresholds(&metrics).is_ok());
        assert!(policy.is_healthy(&metrics));
        assert_eq!(
            policy.get_health_status(&metrics),
            SystemHealthStatus::Healthy
        );
    }

    #[test]
    fn test_cpu_violation() {
        let thresholds = crate::ThresholdsConfig::default();
        let policy = SystemMetricsPolicy::new(thresholds);
        let metrics = create_test_metrics(95.0, 60.0, 70.0);

        assert!(policy.check_thresholds(&metrics).is_err());
        assert!(!policy.is_healthy(&metrics));
        assert_eq!(
            policy.get_health_status(&metrics),
            SystemHealthStatus::Critical
        );

        let violations = policy.get_violations(&metrics);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].metric, "cpu_usage");
        assert_eq!(violations[0].severity, ViolationSeverity::Critical);
    }

    #[test]
    fn test_memory_violation() {
        let thresholds = crate::ThresholdsConfig::default();
        let policy = SystemMetricsPolicy::new(thresholds);
        let metrics = create_test_metrics(50.0, 96.0, 70.0);

        assert!(policy.check_thresholds(&metrics).is_err());
        assert!(!policy.is_healthy(&metrics));
        assert_eq!(
            policy.get_health_status(&metrics),
            SystemHealthStatus::Critical
        );

        let violations = policy.get_violations(&metrics);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].metric, "memory_usage");
        assert_eq!(violations[0].severity, ViolationSeverity::Critical);
    }

    #[test]
    fn test_memory_headroom() {
        let thresholds = crate::ThresholdsConfig::default();
        let policy = SystemMetricsPolicy::new(thresholds);

        // Test sufficient headroom
        assert!(policy.check_memory_headroom(20.0).is_ok());

        // Test insufficient headroom
        assert!(policy.check_memory_headroom(10.0).is_err());
    }
}
