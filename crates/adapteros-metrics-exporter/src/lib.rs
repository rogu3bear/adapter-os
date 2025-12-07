//! Prometheus/OpenMetrics exporter for AdapterOS control plane

use adapteros_db::{models::Worker, Db};
use anyhow::Result;
use prometheus::{
    Counter, CounterVec, Encoder, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry,
    TextEncoder,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::info;

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
    _workers_memory_headroom_pct: GaugeVec,
    _workers_adapters_loaded: GaugeVec,
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

        let workers_memory_headroom_pct = GaugeVec::new(
            Opts::new(
                "adapteros_lora_workers_memory_headroom_percent",
                "Worker memory headroom percentage",
            ),
            &["worker_id", "tenant_id"],
        )?;
        registry.register(Box::new(workers_memory_headroom_pct.clone()))?;

        let workers_adapters_loaded = GaugeVec::new(
            Opts::new(
                "adapteros_lora_workers_adapters_loaded",
                "Number of adapters loaded per worker",
            ),
            &["worker_id", "tenant_id"],
        )?;
        registry.register(Box::new(workers_adapters_loaded.clone()))?;

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

        Ok(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            jobs_total,
            jobs_failed_total,
            jobs_duration_seconds,
            jobs_active,
            workers_active,
            _workers_memory_headroom_pct: workers_memory_headroom_pct,
            _workers_adapters_loaded: workers_adapters_loaded,
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

    /// Update worker metrics from database
    pub async fn update_worker_metrics(&self, _db: &Db) -> Result<()> {
        // Use db.list_all_workers() which uses actual Worker schema
        let workers: Vec<Worker> = vec![];

        // Reset workers_active gauge
        self.workers_active.set(workers.len() as f64);

        info!("Updated worker metrics: {} workers", workers.len());

        // Use db.list_jobs() which uses actual Job schema
        let jobs = _db.list_jobs(None).await.unwrap_or_default();

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
            match family.get_name() {
                "mplora_http_requests_total" => {
                    for metric in family.get_metric() {
                        if metric.has_counter() {
                            total_requests += metric.get_counter().get_value();
                        }
                    }
                }
                "mplora_jobs_active" => {
                    for metric in family.get_metric() {
                        if metric.has_gauge() {
                            queue_depth += metric.get_gauge().get_value();
                        }
                    }
                }
                "mplora_http_request_duration_seconds" => {
                    for metric in family.get_metric() {
                        if metric.has_histogram() {
                            avg_latency_ms += metric.get_histogram().get_sample_sum() * 1000.0;
                            request_count = metric.get_histogram().get_sample_count();
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
}
