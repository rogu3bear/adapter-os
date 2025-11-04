//! Metrics integration for retry operations

use adapteros_core::RetryMetricsReporter;
use adapteros_metrics_exporter::MetricsExporter;
use std::sync::Arc;
use std::time::Duration;

/// Metrics reporter that integrates with adapteros-metrics-exporter
pub struct MetricsExporterRetryReporter {
    exporter: Arc<MetricsExporter>,
}

impl MetricsExporterRetryReporter {
    /// Create a new metrics reporter
    pub fn new(exporter: Arc<MetricsExporter>) -> Self {
        Self { exporter }
    }
}

impl RetryMetricsReporter for MetricsExporterRetryReporter {
    fn record_retry_start(&self, service_type: &str) {
        // Record that a retry operation started
        self.exporter.record_operation("retry_start", service_type, false, 0);
    }

    fn record_retry_attempt(&self, service_type: &str, attempt: u32) {
        // Record each retry attempt
        self.exporter.record_retry_attempt(service_type, attempt);
    }

    fn record_retry_success(&self, service_type: &str, _duration: Duration) {
        // Record successful retry completion
        self.exporter.record_retry_success(service_type, 0); // attempts_used not tracked here
    }

    fn record_retry_failure(&self, service_type: &str, _duration: Duration) {
        // Record failed retry operation
        self.exporter.record_retry_failure(service_type);
    }
}
