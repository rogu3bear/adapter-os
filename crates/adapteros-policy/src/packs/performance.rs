//! Performance Policy Pack
//!
//! Enforces performance budgets and latency requirements
//! for the AdapterOS inference pipeline.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Performance policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// P95 latency budget in milliseconds
    pub latency_p95_ms: u64,
    /// Maximum router overhead percentage
    pub router_overhead_pct_max: f64,
    /// Minimum throughput in tokens per second
    pub throughput_tokens_per_s_min: u64,
    /// Maximum memory usage percentage
    pub memory_usage_pct_max: f64,
    /// Maximum GPU utilization percentage
    pub gpu_utilization_pct_max: f64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            latency_p95_ms: 24,
            router_overhead_pct_max: 8.0,
            throughput_tokens_per_s_min: 40,
            memory_usage_pct_max: 85.0,
            gpu_utilization_pct_max: 95.0,
        }
    }
}

/// Performance metrics for a single inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMetrics {
    pub inference_id: String,
    pub latency_ms: u64,
    pub tokens_generated: u32,
    pub router_overhead_ms: u64,
    pub memory_usage_mb: u64,
    pub gpu_utilization_pct: f64,
    pub timestamp: u64,
}

/// Performance statistics over a time window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceStats {
    pub window_start: u64,
    pub window_end: u64,
    pub total_inferences: u64,
    pub p95_latency_ms: u64,
    pub p99_latency_ms: u64,
    pub avg_throughput_tokens_per_s: f64,
    pub avg_router_overhead_pct: f64,
    pub max_memory_usage_mb: u64,
    pub max_gpu_utilization_pct: f64,
    pub violations: Vec<PerformanceViolation>,
}

/// Performance violation details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceViolation {
    pub violation_type: PerformanceViolationType,
    pub severity: PerformanceSeverity,
    pub value: f64,
    pub threshold: f64,
    pub timestamp: u64,
    pub details: String,
}

/// Types of performance violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceViolationType {
    /// Latency exceeded budget
    LatencyExceeded,
    /// Router overhead too high
    RouterOverheadExceeded,
    /// Throughput too low
    ThroughputTooLow,
    /// Memory usage too high
    MemoryUsageExceeded,
    /// GPU utilization too high
    GpuUtilizationExceeded,
}

/// Severity levels for performance violations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceSeverity {
    /// Warning level
    Warning,
    /// Error level
    Error,
    /// Critical level
    Critical,
}

/// Performance policy implementation
pub struct PerformancePolicy {
    config: PerformanceConfig,
}

impl PerformancePolicy {
    /// Create new performance policy
    pub fn new(config: PerformanceConfig) -> Self {
        Self { config }
    }

    /// Calculate performance statistics from metrics
    pub fn calculate_stats(&self, metrics: &[InferenceMetrics]) -> Result<PerformanceStats> {
        if metrics.is_empty() {
            return Err(AosError::PolicyViolation("No metrics provided".to_string()));
        }

        let mut latencies: Vec<u64> = metrics.iter().map(|m| m.latency_ms).collect();
        latencies.sort();

        let p95_index = (latencies.len() as f64 * 0.95) as usize;
        let p99_index = (latencies.len() as f64 * 0.99) as usize;

        let p95_latency = latencies[p95_index.min(latencies.len() - 1)];
        let p99_latency = latencies[p99_index.min(latencies.len() - 1)];

        let total_tokens: u64 = metrics.iter().map(|m| m.tokens_generated as u64).sum();
        let total_time_ms: u64 = metrics.iter().map(|m| m.latency_ms).sum();
        let avg_throughput = if total_time_ms > 0 {
            (total_tokens as f64 * 1000.0) / total_time_ms as f64
        } else {
            0.0
        };

        let avg_router_overhead = if total_time_ms > 0 {
            let total_router_time: u64 = metrics.iter().map(|m| m.router_overhead_ms).sum();
            (total_router_time as f64 / total_time_ms as f64) * 100.0
        } else {
            0.0
        };

        let max_memory = metrics.iter().map(|m| m.memory_usage_mb).max().unwrap_or(0);
        let max_gpu = metrics
            .iter()
            .map(|m| m.gpu_utilization_pct)
            .fold(0.0, f64::max);

        let mut violations = Vec::new();

        // Check for violations
        if p95_latency > self.config.latency_p95_ms {
            violations.push(PerformanceViolation {
                violation_type: PerformanceViolationType::LatencyExceeded,
                severity: PerformanceSeverity::Error,
                value: p95_latency as f64,
                threshold: self.config.latency_p95_ms as f64,
                timestamp: metrics.last().unwrap().timestamp,
                details: format!(
                    "P95 latency {}ms exceeds budget {}ms",
                    p95_latency, self.config.latency_p95_ms
                ),
            });
        }

        if avg_router_overhead > self.config.router_overhead_pct_max {
            violations.push(PerformanceViolation {
                violation_type: PerformanceViolationType::RouterOverheadExceeded,
                severity: PerformanceSeverity::Warning,
                value: avg_router_overhead,
                threshold: self.config.router_overhead_pct_max,
                timestamp: metrics.last().unwrap().timestamp,
                details: format!(
                    "Router overhead {:.2}% exceeds limit {:.2}%",
                    avg_router_overhead, self.config.router_overhead_pct_max
                ),
            });
        }

        if avg_throughput < self.config.throughput_tokens_per_s_min as f64 {
            violations.push(PerformanceViolation {
                violation_type: PerformanceViolationType::ThroughputTooLow,
                severity: PerformanceSeverity::Error,
                value: avg_throughput,
                threshold: self.config.throughput_tokens_per_s_min as f64,
                timestamp: metrics.last().unwrap().timestamp,
                details: format!(
                    "Throughput {:.2} tokens/s below minimum {} tokens/s",
                    avg_throughput, self.config.throughput_tokens_per_s_min
                ),
            });
        }

        if max_memory as f64 > self.config.memory_usage_pct_max {
            violations.push(PerformanceViolation {
                violation_type: PerformanceViolationType::MemoryUsageExceeded,
                severity: PerformanceSeverity::Critical,
                value: max_memory as f64,
                threshold: self.config.memory_usage_pct_max,
                timestamp: metrics.last().unwrap().timestamp,
                details: format!(
                    "Memory usage {}MB exceeds limit {:.2}%",
                    max_memory, self.config.memory_usage_pct_max
                ),
            });
        }

        if max_gpu > self.config.gpu_utilization_pct_max {
            violations.push(PerformanceViolation {
                violation_type: PerformanceViolationType::GpuUtilizationExceeded,
                severity: PerformanceSeverity::Warning,
                value: max_gpu,
                threshold: self.config.gpu_utilization_pct_max,
                timestamp: metrics.last().unwrap().timestamp,
                details: format!(
                    "GPU utilization {:.2}% exceeds limit {:.2}%",
                    max_gpu, self.config.gpu_utilization_pct_max
                ),
            });
        }

        Ok(PerformanceStats {
            window_start: metrics.first().unwrap().timestamp,
            window_end: metrics.last().unwrap().timestamp,
            total_inferences: metrics.len() as u64,
            p95_latency_ms: p95_latency,
            p99_latency_ms: p99_latency,
            avg_throughput_tokens_per_s: avg_throughput,
            avg_router_overhead_pct: avg_router_overhead,
            max_memory_usage_mb: max_memory,
            max_gpu_utilization_pct: max_gpu,
            violations,
        })
    }

    /// Validate performance configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.latency_p95_ms == 0 {
            return Err(AosError::PolicyViolation(
                "latency_p95_ms must be greater than 0".to_string(),
            ));
        }

        if self.config.router_overhead_pct_max < 0.0 || self.config.router_overhead_pct_max > 100.0
        {
            return Err(AosError::PolicyViolation(
                "router_overhead_pct_max must be between 0 and 100".to_string(),
            ));
        }

        if self.config.throughput_tokens_per_s_min == 0 {
            return Err(AosError::PolicyViolation(
                "throughput_tokens_per_s_min must be greater than 0".to_string(),
            ));
        }

        if self.config.memory_usage_pct_max < 0.0 || self.config.memory_usage_pct_max > 100.0 {
            return Err(AosError::PolicyViolation(
                "memory_usage_pct_max must be between 0 and 100".to_string(),
            ));
        }

        if self.config.gpu_utilization_pct_max < 0.0 || self.config.gpu_utilization_pct_max > 100.0
        {
            return Err(AosError::PolicyViolation(
                "gpu_utilization_pct_max must be between 0 and 100".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for performance policy enforcement
#[derive(Debug)]
pub struct PerformanceContext {
    pub metrics: Vec<InferenceMetrics>,
    pub tenant_id: String,
    pub session_id: String,
}

impl PolicyContext for PerformanceContext {
    fn context_type(&self) -> &str {
        "performance"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for PerformancePolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Performance
    }

    fn name(&self) -> &'static str {
        "Performance"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let perf_ctx = ctx
            .as_any()
            .downcast_ref::<PerformanceContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid performance context".to_string()))?;

        // Validate configuration
        self.validate_config()?;

        // Calculate performance statistics
        let stats = self.calculate_stats(&perf_ctx.metrics)?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Convert performance violations to policy violations
        for perf_violation in &stats.violations {
            let severity = match perf_violation.severity {
                PerformanceSeverity::Warning => Severity::Medium,
                PerformanceSeverity::Error => Severity::High,
                PerformanceSeverity::Critical => Severity::Critical,
            };

            violations.push(Violation {
                severity,
                message: perf_violation.details.clone(),
                details: Some(format!(
                    "Value: {:.2}, Threshold: {:.2}, Type: {:?}",
                    perf_violation.value, perf_violation.threshold, perf_violation.violation_type
                )),
            });
        }

        // Add warnings for high percentiles
        if stats.p99_latency_ms > self.config.latency_p95_ms * 2 {
            warnings.push(format!(
                "P99 latency {}ms is significantly higher than P95 budget {}ms",
                stats.p99_latency_ms, self.config.latency_p95_ms
            ));
        }

        // Add warnings for low throughput
        if stats.avg_throughput_tokens_per_s < self.config.throughput_tokens_per_s_min as f64 * 0.8
        {
            warnings.push(format!(
                "Average throughput {:.2} tokens/s is below 80% of minimum {} tokens/s",
                stats.avg_throughput_tokens_per_s, self.config.throughput_tokens_per_s_min
            ));
        }

        Ok(Audit {
            policy_id: PolicyId::Performance,
            passed: violations.is_empty(),
            violations,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_performance_config_default() {
        let config = PerformanceConfig::default();
        assert_eq!(config.latency_p95_ms, 24);
        assert_eq!(config.router_overhead_pct_max, 8.0);
        assert_eq!(config.throughput_tokens_per_s_min, 40);
    }

    #[test]
    fn test_performance_policy_creation() {
        let config = PerformanceConfig::default();
        let policy = PerformancePolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Performance);
    }

    #[test]
    fn test_performance_stats_calculation() {
        let config = PerformanceConfig::default();
        let policy = PerformancePolicy::new(config);

        let metrics = vec![
            InferenceMetrics {
                inference_id: "inf1".to_string(),
                latency_ms: 20,
                tokens_generated: 10,
                router_overhead_ms: 1,
                memory_usage_mb: 100,
                gpu_utilization_pct: 50.0,
                timestamp: 1000,
            },
            InferenceMetrics {
                inference_id: "inf2".to_string(),
                latency_ms: 30,
                tokens_generated: 15,
                router_overhead_ms: 2,
                memory_usage_mb: 120,
                gpu_utilization_pct: 60.0,
                timestamp: 2000,
            },
        ];

        let stats = policy.calculate_stats(&metrics).unwrap();
        assert_eq!(stats.total_inferences, 2);
        assert_eq!(stats.p95_latency_ms, 30); // Should be the higher value
        assert!(stats.avg_throughput_tokens_per_s > 0.0);
        assert!(stats.avg_router_overhead_pct > 0.0);
    }

    #[test]
    fn test_performance_violations() {
        let mut config = PerformanceConfig::default();
        config.latency_p95_ms = 10; // Very strict budget
        let policy = PerformancePolicy::new(config);

        let metrics = vec![InferenceMetrics {
            inference_id: "inf1".to_string(),
            latency_ms: 25, // Exceeds budget
            tokens_generated: 10,
            router_overhead_ms: 1,
            memory_usage_mb: 100,
            gpu_utilization_pct: 50.0,
            timestamp: 1000,
        }];

        let stats = policy.calculate_stats(&metrics).unwrap();
        assert!(!stats.violations.is_empty());
        assert!(stats
            .violations
            .iter()
            .any(|v| matches!(v.violation_type, PerformanceViolationType::LatencyExceeded)));
    }

    #[test]
    fn test_performance_config_validation() {
        let mut config = PerformanceConfig::default();
        config.latency_p95_ms = 0; // Invalid
        let policy = PerformancePolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
