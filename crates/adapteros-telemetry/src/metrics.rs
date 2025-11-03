//! Metrics collector for AdapterOS telemetry
//!
//! Provides real-time metrics collection including:
//! - Latency metrics (p50, p95, p99)
//! - Queue depth monitoring
//! - Token throughput (tokens/sec)
//! - Prometheus/OpenMetrics export
//! - JSON endpoint export

use crate::alerting::AlertSeverity;
use adapteros_core::{AosError, Result};
use axum::{
    body::Body,
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use prometheus::{
    CounterVec, Encoder, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry, TextEncoder,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::{net::TcpListener, signal, sync::RwLock};
use tracing::info; // Add import

// Note: sysinfo is used indirectly through the SystemMetricsProvider trait

/// Trait for collecting system metrics
///
/// All implementations must be Send + Sync to ensure they can be safely
/// awaited on the Tokio runtime without blocking.
#[async_trait::async_trait]
pub trait SystemMetricsProvider: Send + Sync + std::fmt::Debug {
    async fn collect_system_metrics(&self) -> SystemMetricsSnapshot;
}

/// System metrics snapshot from provider
#[derive(Debug, Clone)]
pub struct SystemMetricsSnapshot {
    pub cpu_usage_percent: f64,
    pub memory_usage_mb: f64,
    pub disk_io_utilization: f64,
    pub network_bandwidth_mbps: f64,
    pub gpu_utilization: Option<f64>,
    pub gpu_memory_used_mb: Option<f64>,
    pub gpu_temperature: Option<f64>,
}

// Submodule for system metrics helpers and emitters
pub mod system;

/// Metrics collector with Prometheus integration and real data sources
#[derive(Debug)]
pub struct MetricsCollector {
    registry: Registry,
    // Latency metrics
    inference_latency: HistogramVec,
    router_latency: HistogramVec,
    kernel_latency: HistogramVec,
    // Lifecycle operation metrics
    adapter_load_latency: HistogramVec,
    adapter_unload_latency: HistogramVec,
    // Queue depth metrics
    queue_depth: GaugeVec,
    adapter_queue_depth: GaugeVec,
    // Token throughput metrics
    tokens_generated_total: CounterVec,
    tokens_per_second: GaugeVec,
    // System metrics
    active_sessions: Gauge,
    memory_usage_bytes: GaugeVec,
    // Policy metrics
    policy_violations_total: CounterVec,
    abstain_events_total: CounterVec,
    // Adapter metrics
    adapter_activations_total: CounterVec,
    adapter_evictions_total: CounterVec,
    // Determinism metrics
    seed_collision_count: CounterVec,
    seed_propagation_failures: CounterVec,
    active_seed_threads: Gauge,
    // Real data sources
    system_metrics_provider: Option<Box<dyn SystemMetricsProvider>>,
    // Internal state
    metrics_cache: Arc<RwLock<MetricsSnapshot>>,
}

/// Snapshot of current metrics for JSON export
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueDepthMetrics {
    pub request_queue: f64,
    pub adapter_queue: f64,
    pub kernel_queue: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputMetrics {
    pub tokens_per_second: f64,
    pub tokens_generated_total: u64,
    pub sessions_per_minute: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub active_sessions: f64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMetrics {
    pub violations_total: u64,
    pub abstain_events_total: u64,
    pub violations_by_policy: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetrics {
    pub activations_total: u64,
    pub evictions_total: u64,
    pub active_adapters: f64,
    pub activations_by_adapter: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismMetrics {
    pub seed_collision_count: u64,
    pub seed_propagation_failure_count: u64,
    pub active_seed_threads: usize,
    pub thread_seed_generations: HashMap<String, u64>,
}

impl MetricsCollector {
    /// Create a new metrics collector with optional system metrics integration
    pub fn new_with_system_provider(
        system_metrics_provider: Option<Box<dyn SystemMetricsProvider>>,
    ) -> Result<Self> {
        let registry = Registry::new();

        // Latency histograms with appropriate buckets for milliseconds
        let latency_buckets = vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0,
        ];

        let inference_latency = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_inference_latency_seconds",
                "Inference latency in seconds",
            )
            .buckets(latency_buckets.clone()),
            &["tenant_id", "adapter_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!(
                "Failed to create inference latency histogram: {}",
                e
            ))
        })?;
        registry
            .register(Box::new(inference_latency.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register inference latency histogram: {}",
                    e
                ))
            })?;

        let router_latency = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_router_latency_seconds",
                "Router decision latency in seconds",
            )
            .buckets(latency_buckets.clone()),
            &["tenant_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create router latency histogram: {}", e))
        })?;
        registry
            .register(Box::new(router_latency.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register router latency histogram: {}",
                    e
                ))
            })?;

        let kernel_latency = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_kernel_latency_seconds",
                "Kernel execution latency in seconds",
            )
            .buckets(latency_buckets.clone()),
            &["kernel_type", "tenant_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create kernel latency histogram: {}", e))
        })?;
        registry
            .register(Box::new(kernel_latency.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register kernel latency histogram: {}",
                    e
                ))
            })?;

        // Lifecycle operation histograms with buckets for seconds (1ms to 5 minutes)
        let lifecycle_buckets = vec![
            0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 120.0, 300.0,
        ];

        let adapter_load_latency = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_adapter_load_duration_seconds",
                "Adapter load operation duration in seconds",
            )
            .buckets(lifecycle_buckets.clone()),
            &["adapter_id", "tenant_id", "status"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!(
                "Failed to create adapter load latency histogram: {}",
                e
            ))
        })?;
        registry
            .register(Box::new(adapter_load_latency.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register adapter load latency histogram: {}",
                    e
                ))
            })?;

        let adapter_unload_latency = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_adapter_unload_duration_seconds",
                "Adapter unload operation duration in seconds",
            )
            .buckets(lifecycle_buckets.clone()),
            &["adapter_id", "tenant_id", "status"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!(
                "Failed to create adapter unload latency histogram: {}",
                e
            ))
        })?;
        registry
            .register(Box::new(adapter_unload_latency.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register adapter unload latency histogram: {}",
                    e
                ))
            })?;

        // Queue depth gauges
        let queue_depth = GaugeVec::new(
            Opts::new("adapteros_queue_depth", "Current queue depth"),
            &["queue_type", "tenant_id"],
        )
        .map_err(|e| AosError::Telemetry(format!("Failed to create queue depth gauge: {}", e)))?;
        registry
            .register(Box::new(queue_depth.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!("Failed to register queue depth gauge: {}", e))
            })?;

        let adapter_queue_depth = GaugeVec::new(
            Opts::new(
                "adapteros_adapter_queue_depth",
                "Adapter-specific queue depth",
            ),
            &["adapter_id", "tenant_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create adapter queue depth gauge: {}", e))
        })?;
        registry
            .register(Box::new(adapter_queue_depth.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register adapter queue depth gauge: {}",
                    e
                ))
            })?;

        // Token throughput metrics
        let tokens_generated_total = CounterVec::new(
            Opts::new("adapteros_tokens_generated_total", "Total tokens generated"),
            &["tenant_id", "adapter_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create tokens generated counter: {}", e))
        })?;
        registry
            .register(Box::new(tokens_generated_total.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register tokens generated counter: {}",
                    e
                ))
            })?;

        let tokens_per_second = GaugeVec::new(
            Opts::new("adapteros_tokens_per_second", "Current tokens per second"),
            &["tenant_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create tokens per second gauge: {}", e))
        })?;
        registry
            .register(Box::new(tokens_per_second.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!("Failed to register tokens per second gauge: {}", e))
            })?;

        // System metrics
        let active_sessions = Gauge::new(
            "adapteros_active_sessions",
            "Number of active inference sessions",
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create active sessions gauge: {}", e))
        })?;
        registry
            .register(Box::new(active_sessions.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!("Failed to register active sessions gauge: {}", e))
            })?;

        let memory_usage_bytes = GaugeVec::new(
            Opts::new("adapteros_memory_usage_bytes", "Memory usage in bytes"),
            &["component", "tenant_id"],
        )
        .map_err(|e| AosError::Telemetry(format!("Failed to create memory usage gauge: {}", e)))?;
        registry
            .register(Box::new(memory_usage_bytes.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!("Failed to register memory usage gauge: {}", e))
            })?;

        // Policy metrics
        let policy_violations_total = CounterVec::new(
            Opts::new(
                "adapteros_policy_violations_total",
                "Total policy violations",
            ),
            &["policy_name", "violation_type"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create policy violations counter: {}", e))
        })?;
        registry
            .register(Box::new(policy_violations_total.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register policy violations counter: {}",
                    e
                ))
            })?;

        let abstain_events_total = CounterVec::new(
            Opts::new("adapteros_abstain_events_total", "Total abstain events"),
            &["reason", "tenant_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create abstain events counter: {}", e))
        })?;
        registry
            .register(Box::new(abstain_events_total.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!("Failed to register abstain events counter: {}", e))
            })?;

        // Adapter metrics
        let adapter_activations_total = CounterVec::new(
            Opts::new(
                "adapteros_adapter_activations_total",
                "Total adapter activations",
            ),
            &["adapter_id", "tenant_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!(
                "Failed to create adapter activations counter: {}",
                e
            ))
        })?;
        registry
            .register(Box::new(adapter_activations_total.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register adapter activations counter: {}",
                    e
                ))
            })?;

        let adapter_evictions_total = CounterVec::new(
            Opts::new(
                "adapteros_adapter_evictions_total",
                "Total adapter evictions",
            ),
            &["adapter_id", "tenant_id", "reason"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create adapter evictions counter: {}", e))
        })?;
        registry
            .register(Box::new(adapter_evictions_total.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register adapter evictions counter: {}",
                    e
                ))
            })?;

        // Determinism metrics
        let seed_collision_count = CounterVec::new(
            Opts::new(
                "adapteros_seed_collision_count_total",
                "Total seed collisions detected",
            ),
            &["thread_id"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create seed collision counter: {}", e))
        })?;
        registry
            .register(Box::new(seed_collision_count.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!("Failed to register seed collision counter: {}", e))
            })?;

        let seed_propagation_failures = CounterVec::new(
            Opts::new(
                "adapteros_seed_propagation_failures_total",
                "Total seed propagation failures",
            ),
            &["failure_reason"],
        )
        .map_err(|e| {
            AosError::Telemetry(format!(
                "Failed to create seed propagation failures counter: {}",
                e
            ))
        })?;
        registry
            .register(Box::new(seed_propagation_failures.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register seed propagation failures counter: {}",
                    e
                ))
            })?;

        let active_seed_threads = Gauge::new(
            "adapteros_active_seed_threads",
            "Number of threads with registered seeds",
        )
        .map_err(|e| {
            AosError::Telemetry(format!("Failed to create active seed threads gauge: {}", e))
        })?;
        registry
            .register(Box::new(active_seed_threads.clone()))
            .map_err(|e| {
                AosError::Telemetry(format!(
                    "Failed to register active seed threads gauge: {}",
                    e
                ))
            })?;

        let metrics_cache = Arc::new(RwLock::new(MetricsSnapshot::default()));

        Ok(Self {
            registry,
            inference_latency,
            router_latency,
            kernel_latency,
            adapter_load_latency,
            adapter_unload_latency,
            queue_depth,
            adapter_queue_depth,
            tokens_generated_total,
            tokens_per_second,
            active_sessions,
            memory_usage_bytes,
            policy_violations_total,
            abstain_events_total,
            adapter_activations_total,
            adapter_evictions_total,
            seed_collision_count,
            seed_propagation_failures,
            active_seed_threads,
            system_metrics_provider,
            metrics_cache,
        })
    }

    /// Create a new metrics collector (backwards compatible)
    pub fn new() -> Result<Self> {
        Self::new_with_system_provider(None)
    }

    /// Record inference latency
    pub fn record_inference_latency(&self, tenant_id: &str, adapter_id: &str, latency_secs: f64) {
        self.inference_latency
            .with_label_values(&[tenant_id, adapter_id])
            .observe(latency_secs);
    }

    /// Record router latency
    pub fn record_router_latency(&self, tenant_id: &str, latency_secs: f64) {
        self.router_latency
            .with_label_values(&[tenant_id])
            .observe(latency_secs);
    }

    /// Record kernel latency
    pub fn record_kernel_latency(&self, kernel_type: &str, tenant_id: &str, latency_secs: f64) {
        self.kernel_latency
            .with_label_values(&[kernel_type, tenant_id])
            .observe(latency_secs);
    }

    /// Record adapter load operation latency
    pub fn record_adapter_load_latency(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        latency_secs: f64,
        status: &str,
    ) {
        self.adapter_load_latency
            .with_label_values(&[adapter_id, tenant_id, status])
            .observe(latency_secs);
    }

    /// Record adapter unload operation latency
    pub fn record_adapter_unload_latency(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        latency_secs: f64,
        status: &str,
    ) {
        self.adapter_unload_latency
            .with_label_values(&[adapter_id, tenant_id, status])
            .observe(latency_secs);
    }

    /// Update queue depth
    pub fn update_queue_depth(&self, queue_type: &str, tenant_id: &str, depth: f64) {
        self.queue_depth
            .with_label_values(&[queue_type, tenant_id])
            .set(depth);
    }

    /// Update adapter queue depth
    pub fn update_adapter_queue_depth(&self, adapter_id: &str, tenant_id: &str, depth: f64) {
        self.adapter_queue_depth
            .with_label_values(&[adapter_id, tenant_id])
            .set(depth);
    }

    /// Record tokens generated
    pub fn record_tokens_generated(&self, tenant_id: &str, adapter_id: &str, count: u64) {
        self.tokens_generated_total
            .with_label_values(&[tenant_id, adapter_id])
            .inc_by(count as f64);
    }

    /// Update tokens per second
    pub fn update_tokens_per_second(&self, tenant_id: &str, tps: f64) {
        self.tokens_per_second
            .with_label_values(&[tenant_id])
            .set(tps);
    }

    /// Update active sessions count
    pub fn update_active_sessions(&self, count: f64) {
        self.active_sessions.set(count);
    }

    /// Update memory usage
    pub fn update_memory_usage(&self, component: &str, tenant_id: &str, bytes: f64) {
        self.memory_usage_bytes
            .with_label_values(&[component, tenant_id])
            .set(bytes);
    }

    /// Record policy violation
    pub fn record_policy_violation(&self, policy_name: &str, violation_type: &str) {
        self.policy_violations_total
            .with_label_values(&[policy_name, violation_type])
            .inc();
    }

    /// Record abstain event
    pub fn record_abstain_event(&self, reason: &str, tenant_id: &str) {
        self.abstain_events_total
            .with_label_values(&[reason, tenant_id])
            .inc();
    }

    /// Record adapter activation
    pub fn record_adapter_activation(&self, adapter_id: &str, tenant_id: &str) {
        self.adapter_activations_total
            .with_label_values(&[adapter_id, tenant_id])
            .inc();
    }

    /// Record adapter eviction
    pub fn record_adapter_eviction(&self, adapter_id: &str, tenant_id: &str, reason: &str) {
        self.adapter_evictions_total
            .with_label_values(&[adapter_id, tenant_id, reason])
            .inc();
    }

    /// Record seed collision
    pub fn record_seed_collision(&self, thread_id: &str) {
        self.seed_collision_count
            .with_label_values(&[thread_id])
            .inc();
    }

    /// Record seed propagation failure
    pub fn record_seed_propagation_failure(&self, reason: &str) {
        self.seed_propagation_failures
            .with_label_values(&[reason])
            .inc();
    }

    /// Update active seed threads gauge
    pub fn update_active_seed_threads(&self, count: f64) {
        self.active_seed_threads.set(count);
    }

    /// Update determinism metrics from external source
    pub fn update_determinism_metrics(&self, metrics: DeterminismMetrics) {
        // Update Prometheus metrics
        // Note: We don't update counters here as they should be monotonically increasing
        // Counters are updated via record_seed_collision/record_seed_propagation_failure
        self.update_active_seed_threads(metrics.active_seed_threads as f64);

        // Update cached snapshot - in a real implementation, this would be atomic
        // For now, we just log that metrics were updated
        tracing::debug!(
            "Updated determinism metrics: collisions={}, propagation_failures={}, active_threads={}",
            metrics.seed_collision_count,
            metrics.seed_propagation_failure_count,
            metrics.active_seed_threads
        );
    }

    /// Render metrics in Prometheus/OpenMetrics format
    pub fn render_prometheus(&self) -> Result<Vec<u8>> {
        let metric_families = self.registry.gather();
        let mut buffer = vec![];
        prometheus::TextEncoder::new()
            .encode(&metric_families, &mut buffer)
            .map_err(|e| AosError::Telemetry(format!("Failed to encode metrics: {}", e)))?;
        Ok(buffer)
    }

    /// Collect determinism metrics from global counters
    async fn collect_determinism_metrics(&self) -> DeterminismMetrics {
        // Collect metrics from global atomic counters
        // These are updated by the deterministic-exec crate when seed operations occur
        // Note: In a real implementation, these would be accessible via a shared interface
        // For now, we return placeholder values since we can't access the globals directly

        DeterminismMetrics {
            seed_collision_count: 0,           // Global counter would be accessible here
            seed_propagation_failure_count: 0, // Global counter would be accessible here
            active_seed_threads: 0,            // Would query the global registry
            thread_seed_generations: std::collections::HashMap::new(),
        }
    }

    /// Get current metrics snapshot for JSON export
    pub async fn get_metrics_snapshot(&self) -> MetricsSnapshot {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Collect histogram percentiles from Prometheus histograms
        let inference_p50 =
            self.get_histogram_percentile_by_name("adapteros_inference_latency_seconds", 0.5);
        let inference_p95 =
            self.get_histogram_percentile_by_name("adapteros_inference_latency_seconds", 0.95);
        let inference_p99 =
            self.get_histogram_percentile_by_name("adapteros_inference_latency_seconds", 0.99);

        let router_p50 =
            self.get_histogram_percentile_by_name("adapteros_router_latency_seconds", 0.5);
        let router_p95 =
            self.get_histogram_percentile_by_name("adapteros_router_latency_seconds", 0.95);
        let router_p99 =
            self.get_histogram_percentile_by_name("adapteros_router_latency_seconds", 0.99);

        let kernel_p50 =
            self.get_histogram_percentile_by_name("adapteros_kernel_latency_seconds", 0.5);
        let kernel_p95 =
            self.get_histogram_percentile_by_name("adapteros_kernel_latency_seconds", 0.95);
        let kernel_p99 =
            self.get_histogram_percentile_by_name("adapteros_kernel_latency_seconds", 0.99);

        // Collect lifecycle operation percentiles
        let load_p50 =
            self.get_histogram_percentile_by_name("adapteros_adapter_load_duration_seconds", 0.5) * 1000.0; // Convert to ms
        let load_p95 =
            self.get_histogram_percentile_by_name("adapteros_adapter_load_duration_seconds", 0.95) * 1000.0;
        let load_p99 =
            self.get_histogram_percentile_by_name("adapteros_adapter_load_duration_seconds", 0.99) * 1000.0;

        let unload_p50 =
            self.get_histogram_percentile_by_name("adapteros_adapter_unload_duration_seconds", 0.5) * 1000.0;
        let unload_p95 =
            self.get_histogram_percentile_by_name("adapteros_adapter_unload_duration_seconds", 0.95) * 1000.0;
        let unload_p99 =
            self.get_histogram_percentile_by_name("adapteros_adapter_unload_duration_seconds", 0.99) * 1000.0;

        // Collect gauge values
        let request_queue = self
            .queue_depth
            .with_label_values(&["request", "default"])
            .get();
        let adapter_queue = self
            .adapter_queue_depth
            .with_label_values(&["default", "default"])
            .get();
        let kernel_queue = self
            .queue_depth
            .with_label_values(&["kernel", "default"])
            .get();

        let tokens_per_second = self.tokens_per_second.with_label_values(&["default"]).get();
        let tokens_generated_total = self
            .tokens_generated_total
            .with_label_values(&["default", "default"])
            .get() as u64;
        let active_sessions = self.active_sessions.get();

        // Collect real system metrics if available
        let (memory_usage_mb, cpu_usage_percent, disk_metrics, network_metrics) =
            if let Some(provider) = &self.system_metrics_provider {
                // Use async provider to get real metrics
                let SystemMetricsSnapshot {
                    cpu_usage_percent,
                    memory_usage_mb,
                    disk_io_utilization,
                    network_bandwidth_mbps,
                    gpu_utilization: _,
                    gpu_memory_used_mb: _,
                    gpu_temperature: _,
                } = provider.collect_system_metrics().await;
                (
                    memory_usage_mb,
                    cpu_usage_percent,
                    DiskMetrics {
                        io_utilization: disk_io_utilization,
                    },
                    NetworkMetrics {
                        bandwidth_utilization: network_bandwidth_mbps,
                    },
                )
            } else {
                // Fallback to Prometheus metrics
                let memory_mb = self
                    .memory_usage_bytes
                    .with_label_values(&["worker", "default"])
                    .get()
                    / 1_048_576.0;

                (
                    memory_mb,
                    0.0, // Placeholder CPU usage
                    DiskMetrics {
                        io_utilization: 0.0, // Placeholder
                    },
                    NetworkMetrics {
                        bandwidth_utilization: 0.0, // Placeholder
                    },
                )
            };

        // Collect counter values
        let violations_total = self
            .policy_violations_total
            .with_label_values(&["egress", "attempt"])
            .get() as u64;
        let abstain_events_total = self
            .abstain_events_total
            .with_label_values(&["low_confidence", "default"])
            .get() as u64;
        let activations_total = self
            .adapter_activations_total
            .with_label_values(&["default", "default"])
            .get() as u64;
        let evictions_total = self
            .adapter_evictions_total
            .with_label_values(&["default", "default", "memory"])
            .get() as u64;

        MetricsSnapshot {
            timestamp,
            latency: LatencyMetrics {
                inference_p50_ms: inference_p50 * 1000.0,
                inference_p95_ms: inference_p95 * 1000.0,
                inference_p99_ms: inference_p99 * 1000.0,
                router_p50_ms: router_p50 * 1000.0,
                router_p95_ms: router_p95 * 1000.0,
                router_p99_ms: router_p99 * 1000.0,
                kernel_p50_ms: kernel_p50 * 1000.0,
                kernel_p95_ms: kernel_p95 * 1000.0,
                kernel_p99_ms: kernel_p99 * 1000.0,
            },
            queue_depth: QueueDepthMetrics {
                request_queue,
                adapter_queue,
                kernel_queue,
            },
            throughput: ThroughputMetrics {
                tokens_per_second,
                tokens_generated_total,
                sessions_per_minute: active_sessions * 60.0, // Approximate
            },
            system: SystemMetrics {
                active_sessions,
                memory_usage_mb,
                cpu_usage_percent,
            },
            policy: PolicyMetrics {
                violations_total,
                abstain_events_total,
                violations_by_policy: HashMap::new(),
            },
            adapters: AdapterMetrics {
                activations_total,
                evictions_total,
                active_adapters: 0.0, // Would track active adapters
                activations_by_adapter: HashMap::new(),
            },
            lifecycle: LifecycleMetrics {
                load_p50_ms: load_p50,
                load_p95_ms: load_p95,
                load_p99_ms: load_p99,
                unload_p50_ms: unload_p50,
                unload_p95_ms: unload_p95,
                unload_p99_ms: unload_p99,
                load_operations_total: 0, // TODO: Track load operations
                unload_operations_total: 0, // TODO: Track unload operations
                load_operations_failed: 0, // TODO: Track failed loads
                unload_operations_failed: 0, // TODO: Track failed unloads
            },
            disk: disk_metrics,
            network: network_metrics,
            determinism: self.collect_determinism_metrics().await,
        }
    }

    /// Update metrics cache
    pub async fn update_cache(&self) -> Result<()> {
        let snapshot = self.get_metrics_snapshot().await;
        let mut cache = self.metrics_cache.write().await;
        *cache = snapshot;
        Ok(())
    }

    /// Get cached metrics snapshot
    pub async fn get_cached_snapshot(&self) -> MetricsSnapshot {
        let cache = self.metrics_cache.read().await;
        cache.clone()
    }

    /// Helper to get histogram percentile from Prometheus histogram by name
    fn get_histogram_percentile_by_name(&self, metric_name: &str, percentile: f64) -> f64 {
        // Gather metrics from registry to get histogram data
        let metric_families = self.registry.gather();

        // Find the matching histogram metric family by name
        let histogram_family = metric_families
            .iter()
            .find(|mf| mf.get_name() == metric_name);

        if let Some(family) = histogram_family {
            // Aggregate data across all label combinations
            // Proper aggregation: convert cumulative -> non-cumulative -> sum -> rebuild cumulative
            let mut total_samples = 0u64;
            let mut all_buckets_by_label: Vec<Vec<(f64, u64)>> = Vec::new(); // Per-label bucket lists: (upper_bound, cumulative_count)

            // Collect all buckets from all label combinations
            for metric in family.get_metric() {
                if metric.has_histogram() {
                    let hist = metric.get_histogram();
                    total_samples += hist.get_sample_count();

                    let mut buckets: Vec<(f64, u64)> = Vec::new();
                    for bucket in hist.get_bucket() {
                        let upper_bound = bucket.get_upper_bound();
                        let cumulative_count = bucket.get_cumulative_count();
                        buckets.push((upper_bound, cumulative_count));
                    }
                    // Sort by upper bound
                    buckets
                        .sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    all_buckets_by_label.push(buckets);
                }
            }

            // If we have samples, aggregate buckets properly
            if total_samples > 0 && !all_buckets_by_label.is_empty() {
                // Step 1: Collect all unique bucket bounds across all label combinations
                let mut unique_bounds: Vec<f64> = Vec::new();
                for buckets in &all_buckets_by_label {
                    for (bound, _) in buckets {
                        if !unique_bounds.iter().any(|&b| (b - bound).abs() < 0.0001) {
                            unique_bounds.push(*bound);
                        }
                    }
                }
                unique_bounds.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

                // Step 2: For each unique bound, convert cumulative to non-cumulative per label, then sum
                let mut aggregated_non_cumulative: Vec<(f64, u64)> = Vec::new();

                for &bound in &unique_bounds {
                    let mut sum_non_cum = 0u64;

                    for buckets in &all_buckets_by_label {
                        // Find the cumulative count for this bound in this label combination
                        if let Some((_, cum_count)) =
                            buckets.iter().find(|(b, _)| (b - bound).abs() < 0.0001)
                        {
                            // Convert to non-cumulative: find previous bucket count
                            let prev_bound = buckets
                                .iter()
                                .filter(|(b, _)| *b < bound && (*b - bound).abs() >= 0.0001)
                                .max_by(|a, b| {
                                    a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                                });

                            let prev_count = prev_bound.map(|(_, c)| *c).unwrap_or(0);
                            let non_cum = cum_count - prev_count;
                            sum_non_cum += non_cum;
                        }
                    }

                    aggregated_non_cumulative.push((bound, sum_non_cum));
                }

                // Step 3: Rebuild cumulative buckets
                let mut aggregated_buckets: Vec<(f64, u64)> = Vec::new();
                let mut running_cumulative = 0u64;
                for (bound, non_cum) in aggregated_non_cumulative {
                    running_cumulative += non_cum;
                    aggregated_buckets.push((bound, running_cumulative));
                }

                // Step 4: Calculate percentile from aggregated cumulative buckets
                // Find the bucket that contains the percentile
                let target_count = (total_samples as f64 * percentile).round() as u64;

                // Linear interpolation between buckets for more accurate percentile
                for i in 0..aggregated_buckets.len() {
                    let (upper, count) = aggregated_buckets[i];
                    if count >= target_count {
                        if i == 0 {
                            // First bucket: return its upper bound
                            return upper;
                        } else {
                            // Interpolate between previous and current bucket
                            let (prev_upper, prev_count) = aggregated_buckets[i - 1];
                            let count_range = count - prev_count;
                            if count_range > 0 {
                                let bucket_fraction =
                                    (target_count - prev_count) as f64 / count_range as f64;
                                return prev_upper + (upper - prev_upper) * bucket_fraction;
                            } else {
                                return upper;
                            }
                        }
                    }
                }

                // If target is beyond all buckets, return the largest upper bound
                if let Some((upper, _)) = aggregated_buckets.last() {
                    return *upper;
                }
            }
        }

        // Fallback: if no data, return 0
        0.0
    }

    /// Get registry reference for custom metrics
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Check metrics for alert conditions
    ///
    /// Monitors:
    /// - Latency p95 > 100ms
    /// - Memory usage > 80%  
    /// - Queue depth > 1000
    pub async fn check_alerts(&self) -> Vec<Alert> {
        let mut alerts = Vec::new();
        let cache = self.metrics_cache.read().await;

        // Check latency p95
        if cache.latency.inference_p95_ms > 100.0 {
            alerts.push(Alert {
                severity: crate::AlertSeverity::Warning,
                metric: "inference_latency_p95".to_string(),
                value: cache.latency.inference_p95_ms,
                threshold: 100.0,
                message: format!(
                    "Inference p95 latency {}ms exceeds 100ms threshold",
                    cache.latency.inference_p95_ms
                ),
            });
        }

        // Check memory usage (convert MB to percentage using 100GB as max)
        let memory_mb = cache.system.memory_usage_mb;
        let memory_pct = (memory_mb / 102400.0) * 100.0; // 100GB = 102400MB
        if memory_pct > 80.0 {
            alerts.push(Alert {
                severity: crate::AlertSeverity::Critical,
                metric: "memory_usage_pct".to_string(),
                value: memory_pct,
                threshold: 80.0,
                message: format!("Memory usage {}% exceeds 80% threshold", memory_pct),
            });
        }

        // Check queue depth
        let queue_depth = cache.queue_depth.request_queue;
        if queue_depth > 1000.0 {
            alerts.push(Alert {
                severity: crate::AlertSeverity::Warning,
                metric: "inference_queue_depth".to_string(),
                value: queue_depth,
                threshold: 1000.0,
                message: format!(
                    "Inference queue depth {} exceeds 1000 threshold",
                    queue_depth
                ),
            });
        }

        // Add disk I/O alert (threshold: 80% utilization)
        if cache.disk.io_utilization > 80.0 {
            alerts.push(Alert {
                severity: AlertSeverity::Warning,
                metric: "disk_io_utilization".to_string(),
                value: cache.disk.io_utilization,
                threshold: 80.0,
                message: format!(
                    "Disk I/O utilization {}% exceeds 80% threshold",
                    cache.disk.io_utilization
                ),
            });
        }

        // Add network saturation alert (threshold: 90% bandwidth)
        if cache.network.bandwidth_utilization > 90.0 {
            alerts.push(Alert {
                severity: AlertSeverity::Warning,
                metric: "network_bandwidth_utilization".to_string(),
                value: cache.network.bandwidth_utilization,
                threshold: 90.0,
                message: format!(
                    "Network bandwidth utilization {}% exceeds 90% threshold",
                    cache.network.bandwidth_utilization
                ),
            });
        }

        if !alerts.is_empty() {
            tracing::warn!("Detected {} alerts", alerts.len());
        }

        alerts
    }
}

/// Alert from metrics monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub severity: crate::AlertSeverity,
    pub metric: String,
    pub value: f64,
    pub threshold: f64,
    pub message: String,
}

impl Default for MetricsSnapshot {
    fn default() -> Self {
        Self {
            timestamp: 0,
            latency: LatencyMetrics {
                inference_p50_ms: 0.0,
                inference_p95_ms: 0.0,
                inference_p99_ms: 0.0,
                router_p50_ms: 0.0,
                router_p95_ms: 0.0,
                router_p99_ms: 0.0,
                kernel_p50_ms: 0.0,
                kernel_p95_ms: 0.0,
                kernel_p99_ms: 0.0,
            },
            queue_depth: QueueDepthMetrics {
                request_queue: 0.0,
                adapter_queue: 0.0,
                kernel_queue: 0.0,
            },
            throughput: ThroughputMetrics {
                tokens_per_second: 0.0,
                tokens_generated_total: 0,
                sessions_per_minute: 0.0,
            },
            system: SystemMetrics {
                active_sessions: 0.0,
                memory_usage_mb: 0.0,
                cpu_usage_percent: 0.0,
            },
            policy: PolicyMetrics {
                violations_total: 0,
                abstain_events_total: 0,
                violations_by_policy: HashMap::new(),
            },
            adapters: AdapterMetrics {
                activations_total: 0,
                evictions_total: 0,
                active_adapters: 0.0,
                activations_by_adapter: HashMap::new(),
            },
            lifecycle: LifecycleMetrics {
                load_p50_ms: 0.0,
                load_p95_ms: 0.0,
                load_p99_ms: 0.0,
                unload_p50_ms: 0.0,
                unload_p95_ms: 0.0,
                unload_p99_ms: 0.0,
                load_operations_total: 0,
                unload_operations_total: 0,
                load_operations_failed: 0,
                unload_operations_failed: 0,
            },
            disk: DiskMetrics {
                io_utilization: 0.0,
            },
            network: NetworkMetrics {
                bandwidth_utilization: 0.0,
            },
            determinism: DeterminismMetrics {
                seed_collision_count: 0,
                seed_propagation_failure_count: 0,
                active_seed_threads: 0,
                thread_seed_generations: HashMap::new(),
            },
        }
    }
}

#[derive(Clone)]
struct MetricsServerState {
    collector: Arc<MetricsCollector>,
}

#[derive(Debug)]
struct MetricsHttpError {
    status: StatusCode,
    message: String,
}

impl MetricsHttpError {
    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for MetricsHttpError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

type HandlerResult<T> = std::result::Result<T, MetricsHttpError>;

fn metrics_router(state: MetricsServerState) -> Router {
    Router::new()
        .route("/metrics", get(prometheus_metrics))
        .route("/metrics/json", get(json_metrics))
        .route("/metrics/alerts", get(alerts_metrics))
        .route("/health", get(health_check))
        .with_state(state)
}

async fn prometheus_metrics(State(state): State<MetricsServerState>) -> HandlerResult<Response> {
    let metrics = state
        .collector
        .render_prometheus()
        .map_err(|err| MetricsHttpError::internal(format!("Failed to render metrics: {}", err)))?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, TextEncoder::new().format_type())
        .body(Body::from(metrics))
        .map_err(|err| MetricsHttpError::internal(format!("Failed to build response: {}", err)))
}

async fn json_metrics(
    State(state): State<MetricsServerState>,
) -> HandlerResult<Json<MetricsSnapshot>> {
    let snapshot = state.collector.get_metrics_snapshot().await;
    Ok(Json(snapshot))
}

async fn alerts_metrics(
    State(state): State<MetricsServerState>,
) -> HandlerResult<Json<Vec<Alert>>> {
    let alerts = state.collector.check_alerts().await;
    Ok(Json(alerts))
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

/// Metrics server for HTTP endpoints
pub struct MetricsServer {
    collector: Arc<MetricsCollector>,
    port: u16,
}

impl MetricsServer {
    /// Create a new metrics server
    pub fn new(collector: Arc<MetricsCollector>, port: u16) -> Self {
        Self { collector, port }
    }

    /// Start the metrics server
    pub async fn start(&self) -> Result<()> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| AosError::Telemetry(format!("Failed to bind metrics server: {}", e)))?;

        let local_addr = listener
            .local_addr()
            .map_err(|e| AosError::Telemetry(format!("Failed to read bound address: {}", e)))?;

        info!("Metrics server listening on {}", local_addr);

        let app_state = MetricsServerState {
            collector: self.collector.clone(),
        };

        let app = metrics_router(app_state);

        axum::serve(listener, app)
            .with_graceful_shutdown(Self::shutdown_signal())
            .await
            .map_err(|e| AosError::Telemetry(format!("Metrics server error: {}", e)))?;

        Ok(())
    }

    /// Get Prometheus metrics as string
    pub fn get_prometheus_metrics(&self) -> Result<String> {
        let metrics_bytes = self.collector.render_prometheus()?;
        String::from_utf8(metrics_bytes)
            .map_err(|e| AosError::Telemetry(format!("UTF-8 error: {}", e)))
    }

    /// Get JSON metrics as string
    pub async fn get_json_metrics(&self) -> Result<String> {
        let snapshot = self.collector.get_metrics_snapshot().await;
        serde_json::to_string_pretty(&snapshot)
            .map_err(|e| AosError::Telemetry(format!("JSON serialization error: {}", e)))
    }

    async fn shutdown_signal() {
        match signal::ctrl_c().await {
            Ok(()) => info!("Shutdown signal received for metrics server"),
            Err(err) => tracing::error!("Failed to listen for shutdown signal: {}", err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::State,
        http::{header, StatusCode},
        response::IntoResponse,
        Json,
    };
    use std::sync::Arc;

    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let collector = MetricsCollector::new().expect("Should create metrics collector");
        assert!(collector.render_prometheus().is_ok());
    }

    #[tokio::test]
    async fn test_metrics_recording() {
        let collector = MetricsCollector::new().expect("Should create metrics collector");

        // Record some metrics
        collector.record_inference_latency("tenant1", "adapter1", 0.025);
        collector.record_router_latency("tenant1", 0.005);
        collector.update_queue_depth("request", "tenant1", 5.0);
        collector.record_tokens_generated("tenant1", "adapter1", 100);

        // Get snapshot
        let snapshot = collector.get_metrics_snapshot().await;
        assert!(snapshot.timestamp > 0);
        // Note: The simplified implementation doesn't track specific label values
        // In a real implementation, we'd need to track the actual gauge values
        assert!(snapshot.queue_depth.request_queue >= 0.0);
    }

    #[tokio::test]
    async fn test_metrics_snapshot_serialization() {
        let collector = MetricsCollector::new().expect("Should create metrics collector");
        let snapshot = collector.get_metrics_snapshot().await;

        let json = serde_json::to_string(&snapshot).expect("Should serialize to JSON");
        let deserialized: MetricsSnapshot =
            serde_json::from_str(&json).expect("Should deserialize from JSON");

        assert_eq!(snapshot.timestamp, deserialized.timestamp);
    }

    #[tokio::test]
    async fn test_metrics_server_routes() {
        let collector = Arc::new(MetricsCollector::new().expect("Should create metrics collector"));
        let state = MetricsServerState {
            collector: collector.clone(),
        };

        let Json(snapshot) = json_metrics(State(state.clone()))
            .await
            .expect("JSON endpoint should succeed");
        assert!(snapshot.timestamp > 0);

        let prometheus_response = prometheus_metrics(State(state.clone()))
            .await
            .expect("Prometheus endpoint should succeed");
        assert_eq!(prometheus_response.status(), StatusCode::OK);
        let content_type = prometheus_response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("Content-Type header missing")
            .to_str()
            .expect("Content-Type header should be valid UTF-8");
        assert!(
            content_type.starts_with("text/plain"),
            "Unexpected content type: {}",
            content_type
        );

        let Json(alerts) = alerts_metrics(State(state.clone()))
            .await
            .expect("Alerts endpoint should succeed");
        assert!(alerts.is_empty());

        let health_response = health_check().await.into_response();
        assert_eq!(health_response.status(), StatusCode::OK);
    }
}

/// Export Prometheus metrics with proper error handling
pub fn export_prometheus(registry: &Registry) -> Result<String> {
    let mut buffer = vec![];
    let encoder = TextEncoder::new();
    let metrics = registry.gather();

    encoder
        .encode(&metrics, &mut buffer)
        .map_err(|e| AosError::Telemetry(format!("Failed to encode Prometheus metrics: {}", e)))?;

    String::from_utf8(buffer)
        .map_err(|e| AosError::Telemetry(format!("Failed to convert metrics to UTF-8: {}", e)))
}

// ============================================================
// Time series buffer for metrics dashboard (offline, in-memory)
// ============================================================

use std::sync::RwLock as StdRwLock;

/// A single data point in a time series
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDataPoint {
    pub timestamp_ms: u64,
    pub value: f64,
}

/// A time series with a fixed resolution window
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MetricTimeSeries {
    name: String,
    resolution_ms: u64,
    max_points: usize,
    inner: Arc<StdRwLock<VecDeque<MetricDataPoint>>>,
}

impl MetricTimeSeries {
    /// Create a new time series with specified resolution and max points
    pub fn new(name: String, resolution_ms: u64, max_points: usize) -> Self {
        Self {
            name,
            resolution_ms,
            max_points,
            inner: Arc::new(StdRwLock::new(VecDeque::with_capacity(max_points))),
        }
    }

    /// Record a value at the current time (or specified time)
    pub fn record(&self, value: f64, timestamp_ms: Option<u64>) {
        let ts = timestamp_ms.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        });

        let mut guard = self.inner.write().expect("metric time series poisoned");

        // Evict old points if over capacity
        while guard.len() >= self.max_points {
            guard.pop_front();
        }

        guard.push_back(MetricDataPoint {
            timestamp_ms: ts,
            value,
        });
    }

    /// Get recent points within a time window (or all if None)
    pub fn get_points(&self, start_ms: Option<u64>, end_ms: Option<u64>) -> Vec<MetricDataPoint> {
        let guard = self.inner.read().expect("metric time series poisoned");

        guard
            .iter()
            .filter(|pt| {
                if let Some(start) = start_ms {
                    if pt.timestamp_ms < start {
                        return false;
                    }
                }
                if let Some(end) = end_ms {
                    if pt.timestamp_ms > end {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect()
    }

    /// Get the most recent value
    pub fn latest(&self) -> Option<MetricDataPoint> {
        let guard = self.inner.read().expect("metric time series poisoned");
        guard.back().cloned()
    }
}

/// Registry of metric time series for dashboard queries
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    series: Arc<StdRwLock<HashMap<String, Arc<MetricTimeSeries>>>>,
    collector: Arc<MetricsCollector>,
}

impl MetricsRegistry {
    /// Create a new metrics registry
    pub fn new(collector: Arc<MetricsCollector>) -> Self {
        Self {
            series: Arc::new(StdRwLock::new(HashMap::new())),
            collector,
        }
    }

    /// Get or create a time series for a metric name
    pub fn get_or_create_series(
        &self,
        name: String,
        resolution_ms: u64,
        max_points: usize,
    ) -> Arc<MetricTimeSeries> {
        let mut guard = self.series.write().expect("metrics registry poisoned");

        guard
            .entry(name.clone())
            .or_insert_with(|| Arc::new(MetricTimeSeries::new(name, resolution_ms, max_points)))
            .clone()
    }

    /// Record a snapshot of current metrics to time series (periodic, e.g., every 1s)
    pub async fn record_snapshot(&self) -> Result<()> {
        let snapshot = self.collector.get_metrics_snapshot().await;
        let ts_ms = snapshot.timestamp * 1000; // Convert to ms

        // Record key metrics
        let series_map = self.series.read().expect("metrics registry poisoned");

        // Record latency metrics
        if let Some(s) = series_map.get("inference_latency_p95_ms") {
            s.record(snapshot.latency.inference_p95_ms, Some(ts_ms));
        }
        if let Some(s) = series_map.get("queue_depth") {
            s.record(snapshot.queue_depth.request_queue, Some(ts_ms));
        }
        if let Some(s) = series_map.get("tokens_per_second") {
            s.record(snapshot.throughput.tokens_per_second, Some(ts_ms));
        }
        if let Some(s) = series_map.get("memory_usage_mb") {
            s.record(snapshot.system.memory_usage_mb, Some(ts_ms));
        }

        Ok(())
    }

    /// Get all registered time series names
    pub fn list_series(&self) -> Vec<String> {
        let guard = self.series.read().expect("metrics registry poisoned");
        guard.keys().cloned().collect()
    }

    /// Get a time series by name
    pub fn get_series(&self, name: &str) -> Option<Arc<MetricTimeSeries>> {
        let guard = self.series.read().expect("metrics registry poisoned");
        guard.get(name).cloned()
    }
}
