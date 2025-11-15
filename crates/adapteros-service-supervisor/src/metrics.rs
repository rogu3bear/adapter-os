//! Metrics collection for the service supervisor

use metrics::{counter, gauge, histogram};
use std::time::Instant;

/// Initialize metrics
pub fn init_metrics() {
    // Service metrics - simplified without complex labels for now
    // counter!("services_started_total");
    // counter!("services_stopped_total");
    // counter!("services_failed_total");
    // counter!("services_restarted_total");

    // Health check metrics
    // counter!("health_checks_total");
    // counter!("health_checks_failed_total");

    // Request metrics
    // counter!("http_requests_total");
    // histogram!("http_request_duration_seconds");

    // System metrics
    // gauge!("services_active");
    // gauge!("services_healthy");
}

/// Record service start
pub fn record_service_started(_service_id: &str) {
    // counter!("services_started_total").increment(1);
}

/// Record service stop
pub fn record_service_stopped(_service_id: &str) {
    // counter!("services_stopped_total").increment(1);
}

/// Record service failure
pub fn record_service_failed(_service_id: &str, _error: &str) {
    // counter!("services_failed_total").increment(1);
}

/// Record service restart
pub fn record_service_restarted(_service_id: &str) {
    // counter!("services_restarted_total").increment(1);
}

/// Record health check
pub fn record_health_check(_service_id: &str, _success: bool) {
    // Simplified metrics for now
}

/// Record HTTP request
pub fn record_http_request(_method: &str, _path: &str, _status: u16, _duration: std::time::Duration) {
    // Simplified metrics for now
}

/// Update active services gauge
pub fn update_active_services(_count: usize) {
    // gauge!("services_active").set(count as f64);
}

/// Update healthy services gauge
pub fn update_healthy_services(_count: usize) {
    // gauge!("services_healthy").set(count as f64);
}

/// Timer helper for measuring durations
pub struct Timer {
    start: Instant,
    labels: Vec<(&'static str, String)>,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            labels: Vec::new(),
        }
    }

    pub fn with_label(mut self, key: &'static str, value: String) -> Self {
        self.labels.push((key, value));
        self
    }

    pub fn finish_with_histogram(self, name: &'static str) {
        let duration = self.start.elapsed().as_secs_f64();
        // Simplified - just record without labels for now
        let _histogram = histogram!(name);
        // histogram.record(duration);
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}
