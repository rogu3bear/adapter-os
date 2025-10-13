//! Prometheus/OpenMetrics exporter for AdapterOS control plane

use adapteros_db::Db;
use anyhow::Result;
use prometheus::{
    Counter, CounterVec, Encoder, Gauge, GaugeVec, HistogramOpts, HistogramVec, Opts, Registry,
    TextEncoder,
};
use std::sync::Arc;
use tracing::info;

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
    // System metrics
    promotions_total: Counter,
    policy_violations_total: Counter,
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
        let workers_active =
            Gauge::new("mplora_workers_active", "Number of active worker processes")?;
        registry.register(Box::new(workers_active.clone()))?;

        let workers_memory_headroom_pct = GaugeVec::new(
            Opts::new(
                "mplora_workers_memory_headroom_percent",
                "Worker memory headroom percentage",
            ),
            &["worker_id", "tenant_id"],
        )?;
        registry.register(Box::new(workers_memory_headroom_pct.clone()))?;

        let workers_adapters_loaded = GaugeVec::new(
            Opts::new(
                "mplora_workers_adapters_loaded",
                "Number of adapters loaded per worker",
            ),
            &["worker_id", "tenant_id"],
        )?;
        registry.register(Box::new(workers_adapters_loaded.clone()))?;

        // System metrics
        let promotions_total = Counter::new(
            "mplora_promotions_total",
            "Total number of control plane promotions",
        )?;
        registry.register(Box::new(promotions_total.clone()))?;

        let policy_violations_total = Counter::new(
            "mplora_policy_violations_total",
            "Total number of policy violations",
        )?;
        registry.register(Box::new(policy_violations_total.clone()))?;

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
            promotions_total,
            policy_violations_total,
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

    /// Update worker metrics from database
    pub async fn update_worker_metrics(&self, db: &Db) -> Result<()> {
        // Use db.list_all_workers() which uses actual Worker schema
        let workers = db.list_all_workers().await.unwrap_or_default();

        // Reset workers_active gauge
        self.workers_active.set(workers.len() as f64);

        info!("Updated worker metrics: {} workers", workers.len());

        // Use db.list_jobs() which uses actual Job schema
        let jobs = db.list_jobs(None).await.unwrap_or_default();

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
