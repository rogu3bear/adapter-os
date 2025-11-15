//! Telemetry metric data types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Snapshot of current metrics for JSON export
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsSnapshot {
    pub timestamp: u64,
    pub latency: LatencyMetrics,
    pub queue_depth: QueueDepthMetrics,
    pub throughput: ThroughputMetrics,
    pub system: SystemMetrics,
    pub policy: PolicyMetrics,
    pub adapters: AdapterMetrics,
    pub lifecycle: LifecycleMetrics,
    pub disk: DiskMetrics,
    pub network: NetworkMetrics,
    pub determinism: DeterminismMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LatencyMetrics {
    pub inference_p50_ms: f64,
    pub inference_p95_ms: f64,
    pub inference_p99_ms: f64,
    pub router_p50_ms: f64,
    pub router_p95_ms: f64,
    pub router_p99_ms: f64,
    pub kernel_p50_ms: f64,
    pub kernel_p95_ms: f64,
    pub kernel_p99_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueueDepthMetrics {
    pub request_queue: f64,
    pub adapter_queue: f64,
    pub kernel_queue: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThroughputMetrics {
    pub tokens_per_second: f64,
    pub tokens_generated_total: u64,
    pub sessions_per_minute: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemMetrics {
    pub active_sessions: f64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyMetrics {
    pub violations_total: u64,
    pub abstain_events_total: u64,
    pub violations_by_policy: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterMetrics {
    pub activations_total: u64,
    pub evictions_total: u64,
    pub active_adapters: f64,
    pub activations_by_adapter: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LifecycleMetrics {
    pub load_p50_ms: f64,
    pub load_p95_ms: f64,
    pub load_p99_ms: f64,
    pub unload_p50_ms: f64,
    pub unload_p95_ms: f64,
    pub unload_p99_ms: f64,
    pub load_operations_total: u64,
    pub unload_operations_total: u64,
    pub load_operations_failed: u64,
    pub unload_operations_failed: u64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct DiskMetrics {
    pub io_utilization: f64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub bandwidth_utilization: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeterminismMetrics {
    pub seed_collision_count: u64,
    pub seed_propagation_failure_count: u64,
    pub active_seed_threads: usize,
    pub thread_seed_generations: HashMap<String, u64>,
}

/// Time-series datapoint for metrics queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDataPoint {
    pub timestamp_ms: u64,
    pub value: f64,
}
