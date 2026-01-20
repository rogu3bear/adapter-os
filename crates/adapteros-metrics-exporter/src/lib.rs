//! Prometheus/OpenMetrics exporter for adapterOS control plane

use adapteros_db::Db;
use anyhow::Result;
use prometheus::{
    Counter, CounterVec, Encoder, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, Opts,
    Registry, TextEncoder,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::{info, warn};

/// Snapshot of current metrics for health checks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    pub timestamp: u64,
    pub queue_depth: f64,
    pub total_requests: f64,
    pub avg_latency_ms: f64,
}

impl Default for MetricsSnapshot {
    fn default() -> Self {
        Self {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            queue_depth: 0.0,
            total_requests: 0.0,
            avg_latency_ms: 0.0,
        }
    }
}

/// Metrics exporter with Prometheus-compatible OpenMetrics format
#[allow(dead_code)] // Some fields are registered with Prometheus but not actively populated
pub struct MetricsExporter {
    registry: Registry,
    // Request metrics
    http_requests_total: CounterVec,
    http_request_duration_seconds: HistogramVec,
    // Job metrics
    jobs_total: CounterVec,
    jobs_failed_total: CounterVec,
    jobs_duration_seconds: HistogramVec,
    jobs_active: GaugeVec,
    // Worker metrics
    workers_active: Gauge,
    // Note: Per-worker metrics (_workers_memory_headroom_pct, _workers_adapters_loaded)
    // were removed as they were never populated. Add back when worker-level telemetry is implemented.

    // ==========================================================================
    // Prometheus Metrics Endpoint - Inference/Routing/Receipt metrics
    // ==========================================================================

    // Inference metrics
    inference_requests_total: CounterVec,
    inference_duration_seconds: HistogramVec,
    inference_tokens_generated_total: CounterVec,
    inference_tokens_per_second: GaugeVec,
    inference_queue_depth: GaugeVec,

    // Routing metrics
    routing_decisions_total: CounterVec,
    routing_entropy: HistogramVec,
    routing_k_value: HistogramVec,
    routing_gate_max: HistogramVec,

    // Resource metrics
    memory_bytes: GaugeVec,
    gpu_utilization_ratio: Gauge,
    adapter_cache_entries: Gauge,
    adapter_cache_bytes: Gauge,
    kv_cache_entries: GaugeVec,

    // Receipt metrics
    receipts_generated_total: CounterVec,
    receipt_verification_total: CounterVec,
    receipt_signature_seconds: Histogram,

    // Model load/unload metrics
    model_load_success_total: CounterVec,
    model_load_failure_total: CounterVec,
    model_unload_success_total: CounterVec,
    model_unload_failure_total: CounterVec,
    model_loaded_gauge: GaugeVec,
    // System metrics
    promotions_total: Counter,
    policy_violations_total: Counter,
    // Adapter lifecycle state transition metrics
    adapter_state_transitions_total: CounterVec,
    adapter_state_transition_failures_total: CounterVec,
    adapter_state_transition_duration_seconds: HistogramVec,
    adapters_by_state: GaugeVec,
    // Alert metrics for Prometheus/Alertmanager integration
    alerts_firing: GaugeVec,
    alerts_active_total: Gauge,
    // Determinism / seed metrics
    seed_collision_total: Counter,
    seed_propagation_failure_total: Counter,
    seed_active_threads: Gauge,
    // Determinism / observability event metrics
    determinism_violation_total: Counter,
    strict_violation_total: Counter,
    receipt_mismatch_total: Counter,
    audit_divergence_total: Counter,
    policy_override_total: Counter,
    // SQLite index health / maintenance metrics
    db_page_size_bytes: Gauge,
    db_page_count: Gauge,
    db_freelist_count: Gauge,
    db_freelist_ratio: Gauge,
    db_size_estimate_bytes: Gauge,
    db_freelist_bytes: Gauge,
    db_tenant_index_table_exists: GaugeVec,
    db_tenant_index_has_tenant_id_column: GaugeVec,
    db_tenant_index_leading_present: GaugeVec,
    db_dbstat_available: Gauge,
    db_index_unused_ratio: Gauge,
    db_index_unused_bytes: Gauge,
    db_index_bytes: Gauge,
    db_dbstat_top_index_bytes: GaugeVec,
    db_dbstat_top_index_unused_ratio: GaugeVec,
    db_index_probe_success: GaugeVec,
    db_index_probe_used_index: GaugeVec,
    db_index_probe_duration_seconds: HistogramVec,
    db_index_probe_failures_total: CounterVec,
    db_index_health_status: Gauge,
    db_index_regression_detected: Gauge,
    db_index_maintenance_total: CounterVec,
    db_index_maintenance_duration_seconds: HistogramVec,
    db_index_maintenance_last_run_timestamp_seconds: GaugeVec,
    db_index_maintenance_in_progress: Gauge,
    dbstat_tracked_top_indexes: Mutex<HashSet<String>>,
}

impl MetricsExporter {
    /// Create a new metrics exporter with custom histogram buckets
    pub fn new(histogram_buckets: Vec<f64>) -> Result<Self> {
        let registry = Registry::new();

        // HTTP request metrics
        let http_requests_total = CounterVec::new(
            Opts::new(
                "mplora_http_requests_total",
                "Total number of HTTP requests",
            ),
            &["method", "path", "status"],
        )?;
        registry.register(Box::new(http_requests_total.clone()))?;

        let http_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "mplora_http_request_duration_seconds",
                "HTTP request latency in seconds",
            )
            .buckets(histogram_buckets.clone()),
            &["method", "path"],
        )?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;

        // Job metrics
        let jobs_total = CounterVec::new(
            Opts::new("mplora_jobs_total", "Total number of jobs by type"),
            &["kind"],
        )?;
        registry.register(Box::new(jobs_total.clone()))?;

        let jobs_failed_total = CounterVec::new(
            Opts::new("mplora_jobs_failed_total", "Total number of failed jobs"),
            &["kind"],
        )?;
        registry.register(Box::new(jobs_failed_total.clone()))?;

        let jobs_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "mplora_jobs_duration_seconds",
                "Job execution duration in seconds",
            )
            .buckets(histogram_buckets.clone()),
            &["kind", "status"],
        )?;
        registry.register(Box::new(jobs_duration_seconds.clone()))?;

        let jobs_active = GaugeVec::new(
            Opts::new("mplora_jobs_active", "Currently active jobs"),
            &["kind"],
        )?;
        registry.register(Box::new(jobs_active.clone()))?;

        // Worker metrics
        let workers_active = Gauge::new(
            "adapteros_lora_workers_active",
            "Number of active worker processes",
        )?;
        registry.register(Box::new(workers_active.clone()))?;

        // Note: Per-worker metrics (workers_memory_headroom_pct, workers_adapters_loaded)
        // were removed as they were never populated. Add back when worker-level telemetry is implemented.

        // ==========================================================================
        // Prometheus Metrics Endpoint - Inference/Routing/Receipt metrics
        // ==========================================================================

        // Inference metrics
        let inference_requests_total = CounterVec::new(
            Opts::new("aos_inference_requests_total", "Total inference requests"),
            &["tenant", "status", "model"],
        )?;
        registry.register(Box::new(inference_requests_total.clone()))?;

        let inference_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "aos_inference_duration_seconds",
                "Inference request duration in seconds",
            )
            .buckets(vec![0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
            &["tenant", "model"],
        )?;
        registry.register(Box::new(inference_duration_seconds.clone()))?;

        let inference_tokens_generated_total = CounterVec::new(
            Opts::new(
                "aos_inference_tokens_generated_total",
                "Total tokens generated",
            ),
            &["tenant", "model"],
        )?;
        registry.register(Box::new(inference_tokens_generated_total.clone()))?;

        let inference_tokens_per_second = GaugeVec::new(
            Opts::new("aos_inference_tokens_per_second", "Current throughput"),
            &["tenant", "model"],
        )?;
        registry.register(Box::new(inference_tokens_per_second.clone()))?;

        let inference_queue_depth = GaugeVec::new(
            Opts::new("aos_inference_queue_depth", "Pending requests"),
            &["tenant"],
        )?;
        registry.register(Box::new(inference_queue_depth.clone()))?;

        // Routing metrics
        let routing_decisions_total = CounterVec::new(
            Opts::new("aos_routing_decisions_total", "Routing decisions made"),
            &["tenant", "adapter_count"],
        )?;
        registry.register(Box::new(routing_decisions_total.clone()))?;

        let routing_entropy = HistogramVec::new(
            HistogramOpts::new("aos_routing_entropy", "Routing decision entropy")
                .buckets(vec![0.0, 0.1, 0.25, 0.5, 0.75, 1.0, 1.5, 2.0]),
            &["tenant"],
        )?;
        registry.register(Box::new(routing_entropy.clone()))?;

        let routing_k_value = HistogramVec::new(
            HistogramOpts::new("aos_routing_k_value", "K-sparse value used")
                .buckets(vec![1.0, 2.0, 3.0, 4.0, 5.0, 8.0, 10.0, 16.0]),
            &["tenant"],
        )?;
        registry.register(Box::new(routing_k_value.clone()))?;

        let routing_gate_max = HistogramVec::new(
            HistogramOpts::new("aos_routing_gate_max", "Maximum gate value")
                .buckets(vec![0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 0.99, 1.0]),
            &["tenant"],
        )?;
        registry.register(Box::new(routing_gate_max.clone()))?;

        // Resource metrics
        let memory_bytes = GaugeVec::new(Opts::new("aos_memory_bytes", "Memory usage"), &["type"])?;
        registry.register(Box::new(memory_bytes.clone()))?;

        let gpu_utilization_ratio = Gauge::new("aos_gpu_utilization_ratio", "GPU utilization 0-1")?;
        registry.register(Box::new(gpu_utilization_ratio.clone()))?;

        let adapter_cache_entries =
            Gauge::new("aos_adapter_cache_entries", "Cached adapter count")?;
        registry.register(Box::new(adapter_cache_entries.clone()))?;

        let adapter_cache_bytes = Gauge::new("aos_adapter_cache_bytes", "Cache memory usage")?;
        registry.register(Box::new(adapter_cache_bytes.clone()))?;

        let kv_cache_entries = GaugeVec::new(
            Opts::new("aos_kv_cache_entries", "KV cache entries"),
            &["tenant"],
        )?;
        registry.register(Box::new(kv_cache_entries.clone()))?;

        // Receipt metrics
        let receipts_generated_total = CounterVec::new(
            Opts::new("aos_receipts_generated_total", "Receipts created"),
            &["tenant", "type"],
        )?;
        registry.register(Box::new(receipts_generated_total.clone()))?;

        let receipt_verification_total = CounterVec::new(
            Opts::new("aos_receipt_verification_total", "Verification outcomes"),
            &["result"],
        )?;
        registry.register(Box::new(receipt_verification_total.clone()))?;

        let receipt_signature_seconds = Histogram::with_opts(
            HistogramOpts::new("aos_receipt_signature_seconds", "Signing duration")
                .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1]),
        )?;
        registry.register(Box::new(receipt_signature_seconds.clone()))?;

        // Base model load/unload metrics
        let model_load_success_total = CounterVec::new(
            Opts::new(
                "adapteros_model_load_success_total",
                "Successful base model load operations",
            ),
            &["model_id", "tenant_id"],
        )?;
        registry.register(Box::new(model_load_success_total.clone()))?;

        let model_load_failure_total = CounterVec::new(
            Opts::new(
                "adapteros_model_load_failure_total",
                "Failed base model load operations",
            ),
            &["model_id", "tenant_id"],
        )?;
        registry.register(Box::new(model_load_failure_total.clone()))?;

        let model_unload_success_total = CounterVec::new(
            Opts::new(
                "adapteros_model_unload_success_total",
                "Successful base model unload operations",
            ),
            &["model_id", "tenant_id"],
        )?;
        registry.register(Box::new(model_unload_success_total.clone()))?;

        let model_unload_failure_total = CounterVec::new(
            Opts::new(
                "adapteros_model_unload_failure_total",
                "Failed base model unload operations",
            ),
            &["model_id", "tenant_id"],
        )?;
        registry.register(Box::new(model_unload_failure_total.clone()))?;

        let model_loaded_gauge = GaugeVec::new(
            Opts::new(
                "adapteros_model_loaded",
                "Whether a base model is ready (1) or not (0)",
            ),
            &["model_id", "tenant_id"],
        )?;
        registry.register(Box::new(model_loaded_gauge.clone()))?;

        // System metrics
        let promotions_total = Counter::new(
            "mplora_promotions_total",
            "Total number of control plane promotions",
        )?;
        registry.register(Box::new(promotions_total.clone()))?;

        let policy_violations_total = Counter::new(
            "adapteros_policy_violations_total",
            "Total number of policy violations",
        )?;
        registry.register(Box::new(policy_violations_total.clone()))?;

        // Adapter lifecycle state transition metrics
        let adapter_state_transitions_total = CounterVec::new(
            Opts::new(
                "adapteros_adapter_state_transitions_total",
                "Total number of adapter state transitions",
            ),
            &["old_state", "new_state", "tenant_id"],
        )?;
        registry.register(Box::new(adapter_state_transitions_total.clone()))?;

        let adapter_state_transition_failures_total = CounterVec::new(
            Opts::new(
                "adapteros_adapter_state_transition_failures_total",
                "Total number of failed state transitions (CAS conflicts, validation errors)",
            ),
            &["old_state", "new_state", "reason"],
        )?;
        registry.register(Box::new(adapter_state_transition_failures_total.clone()))?;

        let adapter_state_transition_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_adapter_state_transition_duration_seconds",
                "Duration of adapter state transitions in seconds",
            )
            .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
            &["old_state", "new_state"],
        )?;
        registry.register(Box::new(adapter_state_transition_duration_seconds.clone()))?;

        let adapters_by_state = GaugeVec::new(
            Opts::new(
                "adapteros_adapters_by_state",
                "Number of adapters in each lifecycle state",
            ),
            &["state", "tenant_id"],
        )?;
        registry.register(Box::new(adapters_by_state.clone()))?;

        // Alert metrics for Prometheus/Alertmanager integration
        let alerts_firing = GaugeVec::new(
            Opts::new(
                "adapteros_alerts",
                "Alert state: 1 if firing, 0 if resolved",
            ),
            &["alertname", "severity", "tenant_id", "worker_id", "status"],
        )?;
        registry.register(Box::new(alerts_firing.clone()))?;

        let alerts_active_total = Gauge::new(
            "adapteros_alerts_active_total",
            "Total number of currently active (firing) alerts",
        )?;
        registry.register(Box::new(alerts_active_total.clone()))?;

        // Determinism / seed metrics
        let seed_collision_total = Counter::new(
            "adapteros_seed_collision_total",
            "Total number of seed collisions detected (different seeds for same thread)",
        )?;
        registry.register(Box::new(seed_collision_total.clone()))?;

        let seed_propagation_failure_total = Counter::new(
            "adapteros_seed_propagation_failure_total",
            "Total number of seed propagation failures across async tasks",
        )?;
        registry.register(Box::new(seed_propagation_failure_total.clone()))?;

        let seed_active_threads = Gauge::new(
            "adapteros_seed_active_threads",
            "Number of threads with active registered seeds",
        )?;
        registry.register(Box::new(seed_active_threads.clone()))?;

        // Determinism / observability event metrics
        let determinism_violation_total = Counter::new(
            "adapteros_determinism_violation_total",
            "Total determinism violations detected (includes strict mode)",
        )?;
        registry.register(Box::new(determinism_violation_total.clone()))?;

        let strict_violation_total = Counter::new(
            "adapteros_strict_violation_total",
            "Total strict mode violations (subset of determinism violations)",
        )?;
        registry.register(Box::new(strict_violation_total.clone()))?;

        let receipt_mismatch_total = Counter::new(
            "adapteros_receipt_mismatch_total",
            "Total receipt validation failures (signature/hash mismatches)",
        )?;
        registry.register(Box::new(receipt_mismatch_total.clone()))?;

        let audit_divergence_total = Counter::new(
            "adapteros_audit_divergence_total",
            "Total audit chain divergences (hash mismatch or broken linkage)",
        )?;
        registry.register(Box::new(audit_divergence_total.clone()))?;

        let policy_override_total = Counter::new(
            "adapteros_policy_override_total",
            "Total policy deny overrides (fail-open path)",
        )?;
        registry.register(Box::new(policy_override_total.clone()))?;

        // SQLite index health / maintenance metrics
        let db_page_size_bytes = Gauge::new(
            "adapteros_db_page_size_bytes",
            "SQLite database page size in bytes",
        )?;
        registry.register(Box::new(db_page_size_bytes.clone()))?;

        let db_page_count = Gauge::new("adapteros_db_page_count", "SQLite database page count")?;
        registry.register(Box::new(db_page_count.clone()))?;

        let db_freelist_count = Gauge::new(
            "adapteros_db_freelist_count",
            "SQLite database freelist page count",
        )?;
        registry.register(Box::new(db_freelist_count.clone()))?;

        let db_freelist_ratio = Gauge::new(
            "adapteros_db_freelist_ratio",
            "SQLite freelist ratio (freelist_count / page_count)",
        )?;
        registry.register(Box::new(db_freelist_ratio.clone()))?;

        let db_size_estimate_bytes = Gauge::new(
            "adapteros_db_size_estimate_bytes",
            "Estimated SQLite database size in bytes (page_size * page_count)",
        )?;
        registry.register(Box::new(db_size_estimate_bytes.clone()))?;

        let db_freelist_bytes = Gauge::new(
            "adapteros_db_freelist_bytes",
            "Estimated SQLite freelist bytes (page_size * freelist_count)",
        )?;
        registry.register(Box::new(db_freelist_bytes.clone()))?;

        let db_tenant_index_table_exists = GaugeVec::new(
            Opts::new(
                "adapteros_db_tenant_index_table_exists",
                "Tenant index coverage: table existence (1 = exists, 0 = missing)",
            ),
            &["table"],
        )?;
        registry.register(Box::new(db_tenant_index_table_exists.clone()))?;

        let db_tenant_index_has_tenant_id_column = GaugeVec::new(
            Opts::new(
                "adapteros_db_tenant_index_has_tenant_id_column",
                "Tenant index coverage: presence of tenant_id column (1 = present, 0 = missing)",
            ),
            &["table"],
        )?;
        registry.register(Box::new(db_tenant_index_has_tenant_id_column.clone()))?;

        let db_tenant_index_leading_present = GaugeVec::new(
            Opts::new(
                "adapteros_db_tenant_index_leading_present",
                "Tenant index coverage: index with leading tenant_id exists (1 = present, 0 = missing)",
            ),
            &["table"],
        )?;
        registry.register(Box::new(db_tenant_index_leading_present.clone()))?;

        let db_dbstat_available = Gauge::new(
            "adapteros_db_dbstat_available",
            "Whether SQLite dbstat virtual table is available (1 = available, 0 = unavailable)",
        )?;
        registry.register(Box::new(db_dbstat_available.clone()))?;

        let db_index_unused_ratio = Gauge::new(
            "adapteros_db_index_unused_ratio",
            "SQLite dbstat aggregated index unused ratio (unused_bytes / bytes)",
        )?;
        registry.register(Box::new(db_index_unused_ratio.clone()))?;

        let db_index_unused_bytes = Gauge::new(
            "adapteros_db_index_unused_bytes",
            "SQLite dbstat aggregated index unused bytes",
        )?;
        registry.register(Box::new(db_index_unused_bytes.clone()))?;

        let db_index_bytes = Gauge::new(
            "adapteros_db_index_bytes",
            "SQLite dbstat aggregated index bytes",
        )?;
        registry.register(Box::new(db_index_bytes.clone()))?;

        let db_dbstat_top_index_bytes = GaugeVec::new(
            Opts::new(
                "adapteros_db_dbstat_top_index_bytes",
                "SQLite dbstat top indexes by size (bytes)",
            ),
            &["index_name"],
        )?;
        registry.register(Box::new(db_dbstat_top_index_bytes.clone()))?;

        let db_dbstat_top_index_unused_ratio = GaugeVec::new(
            Opts::new(
                "adapteros_db_dbstat_top_index_unused_ratio",
                "SQLite dbstat top index unused ratio (unused_bytes / bytes)",
            ),
            &["index_name"],
        )?;
        registry.register(Box::new(db_dbstat_top_index_unused_ratio.clone()))?;

        let db_index_probe_success = GaugeVec::new(
            Opts::new(
                "adapteros_db_index_probe_success",
                "Index probe query success (1 = success, 0 = failure)",
            ),
            &["probe"],
        )?;
        registry.register(Box::new(db_index_probe_success.clone()))?;

        let db_index_probe_used_index = GaugeVec::new(
            Opts::new(
                "adapteros_db_index_probe_used_index",
                "Index probe query plan used an index (1 = used, 0 = not used)",
            ),
            &["probe"],
        )?;
        registry.register(Box::new(db_index_probe_used_index.clone()))?;

        let db_index_probe_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_db_index_probe_duration_seconds",
                "Duration of index probe queries in seconds",
            )
            .buckets(vec![
                0.0005, 0.001, 0.0025, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0,
            ]),
            &["probe"],
        )?;
        registry.register(Box::new(db_index_probe_duration_seconds.clone()))?;

        let db_index_probe_failures_total = CounterVec::new(
            Opts::new(
                "adapteros_db_index_probe_failures_total",
                "Total number of failed index probe queries",
            ),
            &["probe", "reason"],
        )?;
        registry.register(Box::new(db_index_probe_failures_total.clone()))?;

        let db_index_health_status = Gauge::new(
            "adapteros_db_index_health_status",
            "Index health status (0 = healthy, 1 = degraded, 2 = critical)",
        )?;
        registry.register(Box::new(db_index_health_status.clone()))?;

        let db_index_regression_detected = Gauge::new(
            "adapteros_db_index_regression_detected",
            "Index regression detected (1 = regression, 0 = normal)",
        )?;
        registry.register(Box::new(db_index_regression_detected.clone()))?;

        let db_index_maintenance_total = CounterVec::new(
            Opts::new(
                "adapteros_db_index_maintenance_total",
                "Total number of index maintenance actions",
            ),
            &["action", "result"],
        )?;
        registry.register(Box::new(db_index_maintenance_total.clone()))?;

        let db_index_maintenance_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "adapteros_db_index_maintenance_duration_seconds",
                "Duration of index maintenance actions in seconds",
            )
            .buckets(vec![
                0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0,
            ]),
            &["action"],
        )?;
        registry.register(Box::new(db_index_maintenance_duration_seconds.clone()))?;

        let db_index_maintenance_last_run_timestamp_seconds = GaugeVec::new(
            Opts::new(
                "adapteros_db_index_maintenance_last_run_timestamp_seconds",
                "Unix timestamp of last index maintenance action run",
            ),
            &["action"],
        )?;
        registry.register(Box::new(
            db_index_maintenance_last_run_timestamp_seconds.clone(),
        ))?;

        let db_index_maintenance_in_progress = Gauge::new(
            "adapteros_db_index_maintenance_in_progress",
            "Whether index maintenance is currently running (1 = running, 0 = idle)",
        )?;
        registry.register(Box::new(db_index_maintenance_in_progress.clone()))?;

        Ok(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            jobs_total,
            jobs_failed_total,
            jobs_duration_seconds,
            jobs_active,
            workers_active,
            // Inference metrics
            inference_requests_total,
            inference_duration_seconds,
            inference_tokens_generated_total,
            inference_tokens_per_second,
            inference_queue_depth,
            routing_decisions_total,
            routing_entropy,
            routing_k_value,
            routing_gate_max,
            memory_bytes,
            gpu_utilization_ratio,
            adapter_cache_entries,
            adapter_cache_bytes,
            kv_cache_entries,
            receipts_generated_total,
            receipt_verification_total,
            receipt_signature_seconds,
            model_load_success_total,
            model_load_failure_total,
            model_unload_success_total,
            model_unload_failure_total,
            model_loaded_gauge,
            promotions_total,
            policy_violations_total,
            adapter_state_transitions_total,
            adapter_state_transition_failures_total,
            adapter_state_transition_duration_seconds,
            adapters_by_state,
            alerts_firing,
            alerts_active_total,
            seed_collision_total,
            seed_propagation_failure_total,
            seed_active_threads,
            determinism_violation_total,
            strict_violation_total,
            receipt_mismatch_total,
            audit_divergence_total,
            policy_override_total,
            db_page_size_bytes,
            db_page_count,
            db_freelist_count,
            db_freelist_ratio,
            db_size_estimate_bytes,
            db_freelist_bytes,
            db_tenant_index_table_exists,
            db_tenant_index_has_tenant_id_column,
            db_tenant_index_leading_present,
            db_dbstat_available,
            db_index_unused_ratio,
            db_index_unused_bytes,
            db_index_bytes,
            db_dbstat_top_index_bytes,
            db_dbstat_top_index_unused_ratio,
            db_index_probe_success,
            db_index_probe_used_index,
            db_index_probe_duration_seconds,
            db_index_probe_failures_total,
            db_index_health_status,
            db_index_regression_detected,
            db_index_maintenance_total,
            db_index_maintenance_duration_seconds,
            db_index_maintenance_last_run_timestamp_seconds,
            db_index_maintenance_in_progress,
            dbstat_tracked_top_indexes: Mutex::new(HashSet::new()),
        })
    }

    /// Record an HTTP request
    pub fn record_request(&self, method: &str, path: &str, status: u16, duration_secs: f64) {
        self.http_requests_total
            .with_label_values(&[method, path, &status.to_string()])
            .inc();

        self.http_request_duration_seconds
            .with_label_values(&[method, path])
            .observe(duration_secs);
    }

    /// Record a job event
    pub fn record_job(&self, kind: &str, status: &str, duration_secs: f64) {
        self.jobs_total.with_label_values(&[kind]).inc();

        if status == "failed" {
            self.jobs_failed_total.with_label_values(&[kind]).inc();
        }

        self.jobs_duration_seconds
            .with_label_values(&[kind, status])
            .observe(duration_secs);
    }

    /// Record a promotion event
    pub fn record_promotion(&self) {
        self.promotions_total.inc();
    }

    /// Record a policy violation
    pub fn record_policy_violation(&self) {
        self.policy_violations_total.inc();
    }

    // ==========================================================================
    // Prometheus Metrics Endpoint - Recording methods
    // ==========================================================================

    /// Record an inference request
    pub fn record_inference_request(
        &self,
        tenant: &str,
        model: &str,
        status: &str,
        duration_secs: f64,
        tokens: u64,
    ) {
        self.inference_requests_total
            .with_label_values(&[tenant, status, model])
            .inc();
        self.inference_duration_seconds
            .with_label_values(&[tenant, model])
            .observe(duration_secs);
        self.inference_tokens_generated_total
            .with_label_values(&[tenant, model])
            .inc_by(tokens as f64);
    }

    /// Update inference throughput gauge
    pub fn set_inference_throughput(&self, tenant: &str, model: &str, tokens_per_sec: f64) {
        self.inference_tokens_per_second
            .with_label_values(&[tenant, model])
            .set(tokens_per_sec);
    }

    /// Update inference queue depth
    pub fn set_inference_queue_depth(&self, tenant: &str, depth: f64) {
        self.inference_queue_depth
            .with_label_values(&[tenant])
            .set(depth);
    }

    /// Record a routing decision
    pub fn record_routing_decision(
        &self,
        tenant: &str,
        adapter_count: usize,
        entropy: f64,
        k_value: usize,
        gate_max: f64,
    ) {
        self.routing_decisions_total
            .with_label_values(&[tenant, &adapter_count.to_string()])
            .inc();
        self.routing_entropy
            .with_label_values(&[tenant])
            .observe(entropy);
        self.routing_k_value
            .with_label_values(&[tenant])
            .observe(k_value as f64);
        self.routing_gate_max
            .with_label_values(&[tenant])
            .observe(gate_max);
    }

    /// Update resource metrics
    pub fn set_memory_bytes(&self, memory_type: &str, bytes: f64) {
        self.memory_bytes
            .with_label_values(&[memory_type])
            .set(bytes);
    }

    /// Update GPU utilization
    pub fn set_gpu_utilization(&self, ratio: f64) {
        self.gpu_utilization_ratio.set(ratio);
    }

    /// Update adapter cache metrics
    pub fn set_adapter_cache_metrics(&self, entries: f64, bytes: f64) {
        self.adapter_cache_entries.set(entries);
        self.adapter_cache_bytes.set(bytes);
    }

    /// Update KV cache entries
    pub fn set_kv_cache_entries(&self, tenant: &str, entries: f64) {
        self.kv_cache_entries
            .with_label_values(&[tenant])
            .set(entries);
    }

    /// Record receipt generation
    pub fn record_receipt_generated(&self, tenant: &str, receipt_type: &str) {
        self.receipts_generated_total
            .with_label_values(&[tenant, receipt_type])
            .inc();
    }

    /// Record receipt verification
    pub fn record_receipt_verification(&self, success: bool) {
        let result = if success { "success" } else { "failure" };
        self.receipt_verification_total
            .with_label_values(&[result])
            .inc();
    }

    /// Record receipt signature duration
    pub fn record_receipt_signature_duration(&self, duration_secs: f64) {
        self.receipt_signature_seconds.observe(duration_secs);
    }

    /// Record model load attempt outcome
    pub fn record_model_load(&self, model_id: &str, tenant_id: &str, success: bool) {
        if success {
            self.model_load_success_total
                .with_label_values(&[model_id, tenant_id])
                .inc();
            self.model_loaded_gauge
                .with_label_values(&[model_id, tenant_id])
                .set(1.0);
        } else {
            self.model_load_failure_total
                .with_label_values(&[model_id, tenant_id])
                .inc();
            self.model_loaded_gauge
                .with_label_values(&[model_id, tenant_id])
                .set(0.0);
        }
    }

    /// Record model unload attempt outcome
    pub fn record_model_unload(&self, model_id: &str, tenant_id: &str, success: bool) {
        if success {
            self.model_unload_success_total
                .with_label_values(&[model_id, tenant_id])
                .inc();
            self.model_loaded_gauge
                .with_label_values(&[model_id, tenant_id])
                .set(0.0);
        } else {
            self.model_unload_failure_total
                .with_label_values(&[model_id, tenant_id])
                .inc();
        }
    }

    /// Explicitly set the loaded gauge for a model (e.g., after aggregation)
    pub fn set_model_loaded_gauge(&self, model_id: &str, tenant_id: &str, loaded: bool) {
        self.model_loaded_gauge
            .with_label_values(&[model_id, tenant_id])
            .set(if loaded { 1.0 } else { 0.0 });
    }

    /// Record a successful adapter state transition
    ///
    /// # Arguments
    /// * `old_state` - Previous adapter state (unloaded, cold, warm, hot, resident)
    /// * `new_state` - New adapter state
    /// * `tenant_id` - Tenant identifier
    /// * `duration_secs` - Time taken for the transition in seconds
    pub fn record_state_transition(
        &self,
        old_state: &str,
        new_state: &str,
        tenant_id: &str,
        duration_secs: f64,
    ) {
        self.adapter_state_transitions_total
            .with_label_values(&[old_state, new_state, tenant_id])
            .inc();

        self.adapter_state_transition_duration_seconds
            .with_label_values(&[old_state, new_state])
            .observe(duration_secs);
    }

    /// Record a failed adapter state transition (CAS conflict, validation error)
    ///
    /// # Arguments
    /// * `old_state` - Expected old state
    /// * `new_state` - Attempted new state
    /// * `reason` - Failure reason (e.g., "cas_conflict", "validation_error", "not_found")
    pub fn record_state_transition_failure(&self, old_state: &str, new_state: &str, reason: &str) {
        self.adapter_state_transition_failures_total
            .with_label_values(&[old_state, new_state, reason])
            .inc();
    }

    /// Update the gauge showing adapters by state
    ///
    /// # Arguments
    /// * `state` - Adapter state (unloaded, cold, warm, hot, resident)
    /// * `tenant_id` - Tenant identifier
    /// * `count` - Number of adapters in this state
    pub fn set_adapters_by_state(&self, state: &str, tenant_id: &str, count: f64) {
        self.adapters_by_state
            .with_label_values(&[state, tenant_id])
            .set(count);
    }

    /// Update alert metrics from a list of alerts
    ///
    /// This method resets all alert gauges and then sets them based on the current
    /// active alerts. Call this before `render()` to ensure alert metrics are current.
    ///
    /// # Arguments
    /// * `alerts` - List of alert tuples: (alertname, severity, tenant_id, worker_id, status)
    ///   where status is "active", "acknowledged", or "resolved"
    pub fn update_alert_metrics(&self, alerts: &[(String, String, String, String, String)]) {
        // Reset all alert metrics to clear stale data
        self.alerts_firing.reset();

        // Count active alerts
        let active_count = alerts
            .iter()
            .filter(|(_, _, _, _, status)| status == "active")
            .count();
        self.alerts_active_total.set(active_count as f64);

        // Set gauge for each active alert
        for (alertname, severity, tenant_id, worker_id, status) in alerts {
            if status == "active" {
                self.alerts_firing
                    .with_label_values(&[alertname, severity, tenant_id, worker_id, status])
                    .set(1.0);
            }
        }
    }

    /// Update seed metrics from the global SeedMetrics collector
    ///
    /// This should be called periodically (e.g., every 15 seconds) to update
    /// seed collision, propagation failure, and active thread metrics.
    ///
    /// # Arguments
    /// * `collision_count` - Total seed collisions detected
    /// * `propagation_failure_count` - Total seed propagation failures
    /// * `active_threads` - Number of threads with registered seeds
    pub fn update_seed_metrics(
        &self,
        collision_count: u64,
        propagation_failure_count: u64,
        active_threads: usize,
    ) {
        // Counters in prometheus-rust don't support set(), so we need to compute delta
        // For simplicity, we use gauges disguised as counters (common pattern for exported metrics)
        // The _total suffix convention indicates these are cumulative values
        let current_collision = self.seed_collision_total.get() as u64;
        if collision_count > current_collision {
            self.seed_collision_total
                .inc_by((collision_count - current_collision) as f64);
        }

        let current_propagation = self.seed_propagation_failure_total.get() as u64;
        if propagation_failure_count > current_propagation {
            self.seed_propagation_failure_total
                .inc_by((propagation_failure_count - current_propagation) as f64);
        }

        self.seed_active_threads.set(active_threads as f64);
    }

    /// Update observability event metrics from global counters in adapteros-core.
    ///
    /// This should be called periodically (e.g., every 15 seconds) to update
    /// determinism violation, receipt mismatch, and audit divergence metrics.
    pub fn update_observability_metrics(
        &self,
        determinism_violations: u64,
        strict_violations: u64,
        receipt_mismatches: u64,
        audit_divergences: u64,
        policy_overrides: u64,
    ) {
        // Counters in prometheus-rust don't support set(), so we compute delta
        let current = self.determinism_violation_total.get() as u64;
        if determinism_violations > current {
            self.determinism_violation_total
                .inc_by((determinism_violations - current) as f64);
        }

        let current = self.strict_violation_total.get() as u64;
        if strict_violations > current {
            self.strict_violation_total
                .inc_by((strict_violations - current) as f64);
        }

        let current = self.receipt_mismatch_total.get() as u64;
        if receipt_mismatches > current {
            self.receipt_mismatch_total
                .inc_by((receipt_mismatches - current) as f64);
        }

        let current = self.audit_divergence_total.get() as u64;
        if audit_divergences > current {
            self.audit_divergence_total
                .inc_by((audit_divergences - current) as f64);
        }

        let current = self.policy_override_total.get() as u64;
        if policy_overrides > current {
            self.policy_override_total
                .inc_by((policy_overrides - current) as f64);
        }
    }

    /// Update worker metrics from database
    pub async fn update_worker_metrics(&self, db: &Db) -> Result<()> {
        // Query workers from database - log errors instead of silently defaulting
        let workers = match db.list_all_workers().await {
            Ok(w) => w,
            Err(e) => {
                warn!(error = %e, "Failed to query workers for metrics update");
                // Don't reset the metric - keep stale value rather than reporting 0
                return Ok(());
            }
        };

        // Count healthy workers only for the active metric
        let healthy_count = workers.iter().filter(|w| w.status == "healthy").count();

        // Set workers_active gauge to healthy worker count
        self.workers_active.set(healthy_count as f64);

        info!(
            "Updated worker metrics: {} healthy / {} total workers",
            healthy_count,
            workers.len()
        );

        // Use db.list_jobs() which uses actual Job schema - log errors instead of silently defaulting
        let jobs = match db.list_jobs(None).await {
            Ok(j) => j,
            Err(e) => {
                warn!(error = %e, "Failed to query jobs for metrics update");
                // Don't reset the metric - keep stale value rather than reporting 0
                return Ok(());
            }
        };

        // Count active jobs (queued + running)
        let active_jobs = jobs
            .iter()
            .filter(|j| j.status == "queued" || j.status == "running")
            .count();

        // Update gauge
        self.jobs_active
            .with_label_values(&["all"])
            .set(active_jobs as f64);

        info!("Updated job metrics: {} active jobs", active_jobs);

        Ok(())
    }

    fn unix_timestamp_seconds() -> f64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
    }

    pub fn record_index_probe_success(&self, probe: &str, used_index: bool, duration_secs: f64) {
        self.db_index_probe_success
            .with_label_values(&[probe])
            .set(1.0);
        self.db_index_probe_used_index
            .with_label_values(&[probe])
            .set(if used_index { 1.0 } else { 0.0 });
        self.db_index_probe_duration_seconds
            .with_label_values(&[probe])
            .observe(duration_secs);
    }

    pub fn record_index_probe_failure(&self, probe: &str, reason: &str) {
        self.db_index_probe_success
            .with_label_values(&[probe])
            .set(0.0);
        self.db_index_probe_used_index
            .with_label_values(&[probe])
            .set(0.0);
        self.db_index_probe_failures_total
            .with_label_values(&[probe, reason])
            .inc();
    }

    pub fn set_index_health_status(&self, status: u8) {
        self.db_index_health_status.set(status as f64);
    }

    pub fn set_index_regression_detected(&self, detected: bool) {
        self.db_index_regression_detected
            .set(if detected { 1.0 } else { 0.0 });
    }

    pub fn set_index_maintenance_in_progress(&self, in_progress: bool) {
        self.db_index_maintenance_in_progress
            .set(if in_progress { 1.0 } else { 0.0 });
    }

    pub fn record_index_maintenance(&self, action: &str, success: bool, duration_secs: f64) {
        let result = if success { "success" } else { "failure" };
        self.db_index_maintenance_total
            .with_label_values(&[action, result])
            .inc();
        self.db_index_maintenance_duration_seconds
            .with_label_values(&[action])
            .observe(duration_secs);
        self.db_index_maintenance_last_run_timestamp_seconds
            .with_label_values(&[action])
            .set(Self::unix_timestamp_seconds());
    }

    /// Render metrics in OpenMetrics/Prometheus text format
    pub fn render(&self) -> Result<Vec<u8>> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = vec![];
        encoder
            .encode(&metric_families, &mut buffer)
            .map_err(|e| anyhow::anyhow!("Failed to encode metrics: {}", e))?;
        Ok(buffer)
    }

    /// Get a reference to the registry for custom metrics
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Get a snapshot of current metrics for health checks
    pub fn snapshot(&self) -> MetricsSnapshot {
        // Gather current metrics from registry
        let metrics = self.registry.gather();

        let mut total_requests = 0.0;
        let mut queue_depth = 0.0;
        let mut avg_latency_ms = 0.0;
        let mut request_count: u64 = 0;

        for family in metrics {
            match family.name() {
                "mplora_http_requests_total" => {
                    for metric in family.get_metric() {
                        if let Some(counter) = metric.get_counter().as_ref() {
                            total_requests += counter.value();
                        }
                    }
                }
                "mplora_jobs_active" => {
                    for metric in family.get_metric() {
                        if let Some(gauge) = metric.get_gauge().as_ref() {
                            queue_depth += gauge.value();
                        }
                    }
                }
                "mplora_http_request_duration_seconds" => {
                    for metric in family.get_metric() {
                        if let Some(histogram) = metric.get_histogram().as_ref() {
                            avg_latency_ms += histogram.sample_sum() * 1000.0;
                            request_count = histogram.sample_count();
                        }
                    }
                }
                _ => {}
            }
        }

        // Calculate average latency
        if request_count > 0 {
            avg_latency_ms /= request_count as f64;
        }

        MetricsSnapshot {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            queue_depth,
            total_requests,
            avg_latency_ms,
        }
    }
}

/// Shared metrics exporter wrapped in Arc for multi-threaded access
pub type SharedMetrics = Arc<MetricsExporter>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_exporter() {
        let exporter = MetricsExporter::new(vec![0.001, 0.01, 0.1, 1.0])
            .expect("Test metrics exporter creation should succeed");
        assert!(exporter.render().is_ok());
    }

    #[test]
    fn test_record_request() {
        let exporter = MetricsExporter::new(vec![0.001, 0.01, 0.1, 1.0])
            .expect("Test metrics exporter creation should succeed");
        exporter.record_request("GET", "/healthz", 200, 0.005);
        let output = exporter
            .render()
            .expect("Test metrics render should succeed");
        let output_str = String::from_utf8(output).expect("Test UTF-8 conversion should succeed");
        assert!(output_str.contains("mplora_http_requests_total"));
    }

    #[test]
    fn test_record_job() {
        let exporter = MetricsExporter::new(vec![0.001, 0.01, 0.1, 1.0])
            .expect("Test metrics exporter creation should succeed");
        exporter.record_job("build_plan", "finished", 45.3);
        let output = exporter
            .render()
            .expect("Test metrics render should succeed");
        let output_str = String::from_utf8(output).expect("Test UTF-8 conversion should succeed");
        assert!(output_str.contains("mplora_jobs_total"));
    }

    #[test]
    fn test_db_index_metrics_render() {
        let exporter = MetricsExporter::new(vec![0.001, 0.01, 0.1, 1.0])
            .expect("Test metrics exporter creation should succeed");

        exporter.record_index_probe_success("adapters_by_tenant", true, 0.002);
        exporter.record_index_maintenance("optimize", true, 0.01);
        exporter.set_index_health_status(0);
        exporter.set_index_regression_detected(false);

        let output = exporter
            .render()
            .expect("Test metrics render should succeed");
        let output_str = String::from_utf8(output).expect("Test UTF-8 conversion should succeed");

        assert!(output_str.contains("adapteros_db_index_probe_duration_seconds"));
        assert!(output_str.contains("adapteros_db_index_maintenance_total"));
        assert!(output_str.contains("adapteros_db_index_health_status"));
    }

    // ==========================================================================
    // Prometheus Metrics Endpoint - Tests
    // ==========================================================================

    #[test]
    fn test_prd005_inference_metrics() {
        let exporter = MetricsExporter::new(vec![0.001, 0.01, 0.1, 1.0])
            .expect("Test metrics exporter creation should succeed");

        exporter.record_inference_request("tenant-1", "qwen-7b", "success", 0.5, 100);
        exporter.set_inference_throughput("tenant-1", "qwen-7b", 200.0);
        exporter.set_inference_queue_depth("tenant-1", 5.0);

        let output = exporter
            .render()
            .expect("Test metrics render should succeed");
        let output_str = String::from_utf8(output).expect("Test UTF-8 conversion should succeed");

        assert!(output_str.contains("aos_inference_requests_total"));
        assert!(output_str.contains("aos_inference_duration_seconds"));
        assert!(output_str.contains("aos_inference_tokens_generated_total"));
        assert!(output_str.contains("aos_inference_tokens_per_second"));
        assert!(output_str.contains("aos_inference_queue_depth"));
    }

    #[test]
    fn test_prd005_routing_metrics() {
        let exporter = MetricsExporter::new(vec![0.001, 0.01, 0.1, 1.0])
            .expect("Test metrics exporter creation should succeed");

        exporter.record_routing_decision("tenant-1", 3, 0.8, 2, 0.95);

        let output = exporter
            .render()
            .expect("Test metrics render should succeed");
        let output_str = String::from_utf8(output).expect("Test UTF-8 conversion should succeed");

        assert!(output_str.contains("aos_routing_decisions_total"));
        assert!(output_str.contains("aos_routing_entropy"));
        assert!(output_str.contains("aos_routing_k_value"));
        assert!(output_str.contains("aos_routing_gate_max"));
    }

    #[test]
    fn test_prd005_resource_metrics() {
        let exporter = MetricsExporter::new(vec![0.001, 0.01, 0.1, 1.0])
            .expect("Test metrics exporter creation should succeed");

        exporter.set_memory_bytes("heap", 1024.0 * 1024.0 * 512.0);
        exporter.set_gpu_utilization(0.75);
        exporter.set_adapter_cache_metrics(10.0, 1024.0 * 1024.0 * 100.0);
        exporter.set_kv_cache_entries("tenant-1", 1000.0);

        let output = exporter
            .render()
            .expect("Test metrics render should succeed");
        let output_str = String::from_utf8(output).expect("Test UTF-8 conversion should succeed");

        assert!(output_str.contains("aos_memory_bytes"));
        assert!(output_str.contains("aos_gpu_utilization_ratio"));
        assert!(output_str.contains("aos_adapter_cache_entries"));
        assert!(output_str.contains("aos_adapter_cache_bytes"));
        assert!(output_str.contains("aos_kv_cache_entries"));
    }

    #[test]
    fn test_prd005_receipt_metrics() {
        let exporter = MetricsExporter::new(vec![0.001, 0.01, 0.1, 1.0])
            .expect("Test metrics exporter creation should succeed");

        exporter.record_receipt_generated("tenant-1", "inference");
        exporter.record_receipt_verification(true);
        exporter.record_receipt_verification(false);
        exporter.record_receipt_signature_duration(0.015);

        let output = exporter
            .render()
            .expect("Test metrics render should succeed");
        let output_str = String::from_utf8(output).expect("Test UTF-8 conversion should succeed");

        assert!(output_str.contains("aos_receipts_generated_total"));
        assert!(output_str.contains("aos_receipt_verification_total"));
        assert!(output_str.contains("aos_receipt_signature_seconds"));
    }
}
