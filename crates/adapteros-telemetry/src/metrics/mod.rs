//! Metrics collection and export for adapterOS telemetry
//!
//! This module provides comprehensive Prometheus-compatible metrics for monitoring
//! critical adapterOS components including:
//!
//! - Metal kernel execution timing
//! - Hot-swap operation latencies
//! - Determinism violation tracking
//! - Hash operations (BLAKE3, HKDF)
//! - Memory pressure indicators
//! - Adapter lifecycle state transitions
//! - Checkpoint operations
//! - GPU fingerprint verification
//!
//! ## Prometheus Metrics
//!
//! Use `CriticalComponentMetrics` from the `critical_components` submodule for
//! production-grade Prometheus metrics:
//!
//! ```rust,ignore
//! use adapteros_telemetry::metrics::critical_components::CriticalComponentMetrics;
//! use std::sync::Arc;
//!
//! let metrics = Arc::new(CriticalComponentMetrics::new()?);
//!
//! // Record kernel execution
//! metrics.record_metal_kernel_execution_seconds("fused_mlp", "4096", 0.0015);
//!
//! // Export for Prometheus
//! let prometheus_text = metrics.export()?;
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Submodules with Prometheus-based metrics
pub mod critical_components;
pub mod system;
pub mod system_provider;

// Re-export critical component types
pub use critical_components::{
    CriticalComponentMetrics as PrometheusCriticalMetrics, HotSwapTimer as PrometheusHotSwapTimer,
    KernelExecutionTimer as PrometheusKernelTimer,
};

/// Metrics aggregation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics export interval in seconds
    pub export_interval_secs: u64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            export_interval_secs: 60,
        }
    }
}

/// Throughput metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    /// Tokens processed per second
    pub tokens_per_second: f64,
    /// Inferences performed per second
    pub inferences_per_second: f64,
}

/// Latency metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LatencyMetrics {
    /// 50th percentile latency in milliseconds
    pub p50_ms: f64,
    /// 95th percentile latency in milliseconds
    pub p95_ms: f64,
    /// 99th percentile latency in milliseconds
    pub p99_ms: f64,
}

/// System metrics (simple serializable struct)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemMetrics {
    /// CPU usage percentage
    pub cpu_usage_percent: f64,
    /// Memory usage percentage
    pub memory_usage_percent: f64,
    /// Disk usage percentage
    pub disk_usage_percent: f64,
}

/// Adapter metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterMetrics {
    /// Number of activations
    pub activations: u64,
    /// Total inferences
    pub total_inferences: u64,
    /// Number of errors
    pub errors: u64,
}

/// Policy metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyMetrics {
    /// Number of violations
    pub violations: u64,
    /// Checks performed
    pub checks_performed: u64,
}

/// Queue depth metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueueDepthMetrics {
    /// Current depth
    pub current_depth: usize,
    /// Maximum depth
    pub max_depth: usize,
    /// Average depth
    pub avg_depth: f64,
}

/// Simple critical component metrics (serializable struct)
///
/// For Prometheus-based metrics, use `PrometheusCriticalMetrics` instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimpleCriticalMetrics {
    /// Component name
    pub component_name: String,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Number of restarts
    pub restarts: u64,
}

/// Simple kernel execution timer (serializable struct)
///
/// For Prometheus-based timer, use `PrometheusKernelTimer` instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimpleKernelTimer {
    /// Total time in milliseconds
    pub total_time_ms: u64,
    /// Call count
    pub call_count: u64,
}

/// Simple hot swap timer (serializable struct)
///
/// For Prometheus-based timer, use `PrometheusHotSwapTimer` instead.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimpleHotSwapTimer {
    /// Swap duration in milliseconds
    pub swap_duration_ms: u64,
}

/// Metrics snapshot
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
    /// Throughput metrics
    pub throughput: ThroughputMetrics,
    /// Latency metrics
    pub latency: LatencyMetrics,
    /// System metrics
    pub system: SystemMetrics,
}

/// Metrics server
pub struct MetricsServer {
    config: MetricsConfig,
    snapshots: Vec<MetricsSnapshot>,
}

impl MetricsServer {
    /// Create new metrics server
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            config,
            snapshots: Vec::new(),
        }
    }

    /// Record metrics snapshot
    pub fn record_snapshot(&mut self, snapshot: MetricsSnapshot) {
        self.snapshots.push(snapshot);
    }

    /// Get config reference
    pub fn config(&self) -> &MetricsConfig {
        &self.config
    }

    /// Get snapshots
    pub fn snapshots(&self) -> &[MetricsSnapshot] {
        &self.snapshots
    }
}

/// Metrics collector
pub struct MetricsCollector {
    config: MetricsConfig,
    counters: HashMap<String, u64>,
}

impl MetricsCollector {
    /// Create new metrics collector
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            config,
            counters: HashMap::new(),
        }
    }

    /// Increment counter
    pub fn increment(&mut self, name: &str, value: u64) {
        *self.counters.entry(name.to_string()).or_insert(0) += value;
    }

    /// Get counter value
    pub fn get(&self, name: &str) -> Option<u64> {
        self.counters.get(name).copied()
    }

    /// Get config reference
    pub fn config(&self) -> &MetricsConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_config_default() {
        let config = MetricsConfig::default();
        assert!(config.enabled);
        assert_eq!(config.export_interval_secs, 60);
    }

    #[test]
    fn test_metrics_collector() {
        let config = MetricsConfig::default();
        let mut collector = MetricsCollector::new(config);

        collector.increment("test_counter", 5);
        collector.increment("test_counter", 3);

        assert_eq!(collector.get("test_counter"), Some(8));
        assert_eq!(collector.get("nonexistent"), None);
    }

    #[test]
    fn test_metrics_server() {
        let config = MetricsConfig::default();
        let mut server = MetricsServer::new(config);

        let snapshot = MetricsSnapshot {
            timestamp_ms: 1234567890,
            throughput: ThroughputMetrics {
                tokens_per_second: 100.0,
                inferences_per_second: 10.0,
            },
            latency: LatencyMetrics {
                p50_ms: 5.0,
                p95_ms: 15.0,
                p99_ms: 25.0,
            },
            system: SystemMetrics {
                cpu_usage_percent: 50.0,
                memory_usage_percent: 60.0,
                disk_usage_percent: 70.0,
            },
        };

        server.record_snapshot(snapshot);
        assert_eq!(server.snapshots().len(), 1);
    }
}
