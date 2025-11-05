//! Telemetry types

use adapteros_telemetry_types::{
    AdapterMetrics as TelemetryAdapterMetrics,
    DeterminismMetrics as TelemetryDeterminismMetrics,
    DiskMetrics as TelemetryDiskMetrics,
    LatencyMetrics as TelemetryLatencyMetrics,
    MetricDataPoint as TelemetryMetricDataPoint,
    MetricsSnapshot as TelemetryMetricsSnapshot,
    NetworkMetrics as TelemetryNetworkMetrics,
    PolicyMetrics as TelemetryPolicyMetrics,
    QueueDepthMetrics as TelemetryQueueDepthMetrics,
    SystemMetrics as TelemetrySystemMetrics,
    ThroughputMetrics as TelemetryThroughputMetrics,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Telemetry event
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryEvent {
    pub event_type: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    pub data: serde_json::Value,
    pub tenant_id: Option<String>,
    pub user_id: Option<String>,
}

/// Telemetry bundle response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TelemetryBundleResponse {
    pub bundle_id: String,
    pub created_at: String,
    pub event_count: u64,
    pub size_bytes: u64,
    pub signature: String,
}

/// Export telemetry bundle request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExportTelemetryBundleRequest {
    pub bundle_id: String,
    pub format: String, // "json", "ndjson", "csv"
}

/// Verify bundle signature request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VerifyBundleSignatureRequest {
    pub bundle_id: String,
    pub expected_signature: String,
}

/// Bundle verification response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BundleVerificationResponse {
    pub bundle_id: String,
    pub verified: bool,
    pub signature_match: bool,
    pub timestamp: String,
}

/// Envelope returned by metrics snapshot endpoint
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsSnapshotResponse {
    pub timestamp: u64,
    pub latency: LatencyMetricsResponse,
    pub queue_depth: QueueDepthMetricsResponse,
    pub throughput: ThroughputMetricsResponse,
    pub system: SystemMetricsResponse,
    pub policy: PolicyMetricsResponse,
    pub adapters: AdapterMetricsResponse,
    pub disk: DiskMetricsResponse,
    pub network: NetworkMetricsResponse,
    pub determinism: DeterminismMetricsResponse,
}

/// Latency percentile metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LatencyMetricsResponse {
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

/// Queue depth metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct QueueDepthMetricsResponse {
    pub request_queue: f64,
    pub adapter_queue: f64,
    pub kernel_queue: f64,
}

/// Throughput metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ThroughputMetricsResponse {
    pub tokens_per_second: f64,
    pub tokens_generated_total: u64,
    pub sessions_per_minute: f64,
}

/// System metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemMetricsResponse {
    pub active_sessions: f64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
}

/// Policy metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PolicyMetricsResponse {
    pub violations_total: u64,
    pub abstain_events_total: u64,
    pub violations_by_policy: HashMap<String, u64>,
}

/// Adapter metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AdapterMetricsResponse {
    pub activations_total: u64,
    pub evictions_total: u64,
    pub active_adapters: f64,
    pub activations_by_adapter: HashMap<String, u64>,
}

/// Disk metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DiskMetricsResponse {
    pub io_utilization: f64,
}

/// Network metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NetworkMetricsResponse {
    pub bandwidth_utilization: f64,
}

/// Determinism metrics snapshot
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DeterminismMetricsResponse {
    pub seed_collision_count: u64,
    pub seed_propagation_failure_count: u64,
    pub active_seed_threads: usize,
    pub thread_seed_generations: HashMap<String, u64>,
}

/// Time-series datapoint for metrics queries
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricDataPointResponse {
    pub timestamp_ms: u64,
    pub value: f64,
}

/// Response container for time-series metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsSeriesResponse {
    pub series_name: String,
    pub points: Vec<MetricDataPointResponse>,
}

impl From<TelemetryMetricsSnapshot> for MetricsSnapshotResponse {
    fn from(snapshot: TelemetryMetricsSnapshot) -> Self {
        Self {
            timestamp: snapshot.timestamp,
            latency: snapshot.latency.into(),
            queue_depth: snapshot.queue_depth.into(),
            throughput: snapshot.throughput.into(),
            system: snapshot.system.into(),
            policy: snapshot.policy.into(),
            adapters: snapshot.adapters.into(),
            disk: snapshot.disk.into(),
            network: snapshot.network.into(),
            determinism: snapshot.determinism.into(),
        }
    }
}

impl From<TelemetryLatencyMetrics> for LatencyMetricsResponse {
    fn from(metrics: TelemetryLatencyMetrics) -> Self {
        Self {
            inference_p50_ms: metrics.inference_p50_ms,
            inference_p95_ms: metrics.inference_p95_ms,
            inference_p99_ms: metrics.inference_p99_ms,
            router_p50_ms: metrics.router_p50_ms,
            router_p95_ms: metrics.router_p95_ms,
            router_p99_ms: metrics.router_p99_ms,
            kernel_p50_ms: metrics.kernel_p50_ms,
            kernel_p95_ms: metrics.kernel_p95_ms,
            kernel_p99_ms: metrics.kernel_p99_ms,
        }
    }
}

impl From<TelemetryQueueDepthMetrics> for QueueDepthMetricsResponse {
    fn from(metrics: TelemetryQueueDepthMetrics) -> Self {
        Self {
            request_queue: metrics.request_queue,
            adapter_queue: metrics.adapter_queue,
            kernel_queue: metrics.kernel_queue,
        }
    }
}

impl From<TelemetryThroughputMetrics> for ThroughputMetricsResponse {
    fn from(metrics: TelemetryThroughputMetrics) -> Self {
        Self {
            tokens_per_second: metrics.tokens_per_second,
            tokens_generated_total: metrics.tokens_generated_total,
            sessions_per_minute: metrics.sessions_per_minute,
        }
    }
}

impl From<TelemetrySystemMetrics> for SystemMetricsResponse {
    fn from(metrics: TelemetrySystemMetrics) -> Self {
        Self {
            active_sessions: metrics.active_sessions,
            memory_usage_mb: metrics.memory_usage_mb,
            cpu_usage_percent: metrics.cpu_usage_percent,
        }
    }
}

impl From<TelemetryPolicyMetrics> for PolicyMetricsResponse {
    fn from(metrics: TelemetryPolicyMetrics) -> Self {
        Self {
            violations_total: metrics.violations_total,
            abstain_events_total: metrics.abstain_events_total,
            violations_by_policy: metrics.violations_by_policy,
        }
    }
}

impl From<TelemetryAdapterMetrics> for AdapterMetricsResponse {
    fn from(metrics: TelemetryAdapterMetrics) -> Self {
        Self {
            activations_total: metrics.activations_total,
            evictions_total: metrics.evictions_total,
            active_adapters: metrics.active_adapters,
            activations_by_adapter: metrics.activations_by_adapter,
        }
    }
}

impl From<TelemetryDiskMetrics> for DiskMetricsResponse {
    fn from(metrics: TelemetryDiskMetrics) -> Self {
        Self {
            io_utilization: metrics.io_utilization,
        }
    }
}

impl From<TelemetryNetworkMetrics> for NetworkMetricsResponse {
    fn from(metrics: TelemetryNetworkMetrics) -> Self {
        Self {
            bandwidth_utilization: metrics.bandwidth_utilization,
        }
    }
}

impl From<TelemetryDeterminismMetrics> for DeterminismMetricsResponse {
    fn from(metrics: TelemetryDeterminismMetrics) -> Self {
        Self {
            seed_collision_count: metrics.seed_collision_count,
            seed_propagation_failure_count: metrics.seed_propagation_failure_count,
            active_seed_threads: metrics.active_seed_threads,
            thread_seed_generations: metrics.thread_seed_generations,
        }
    }
}

impl From<TelemetryMetricDataPoint> for MetricDataPointResponse {
    fn from(point: TelemetryMetricDataPoint) -> Self {
        Self {
            timestamp_ms: point.timestamp_ms,
            value: point.value,
        }
    }
}
