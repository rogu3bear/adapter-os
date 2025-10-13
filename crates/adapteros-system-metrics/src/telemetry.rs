//! Telemetry integration for system metrics
//!
//! Provides telemetry event generation for system metrics following
//! AdapterOS telemetry patterns and canonical JSON serialization.

use crate::{GpuMetrics, SystemMetrics};
use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// System metrics telemetry event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetricsEvent {
    pub cpu_usage: f64,    // Align with SystemMetrics
    pub memory_usage: f64, // Align with SystemMetrics
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub gpu_utilization: Option<f64>, // Align with GpuMetrics
    pub gpu_memory_used: Option<u64>,
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub load_average: LoadAverageTelemetry,
    pub timestamp: u64,
}

/// Load average for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverageTelemetry {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// System metrics telemetry collector
pub struct SystemMetricsTelemetry {
    telemetry_writer: TelemetryWriter,
    sampling_rate: f32,
    last_sample_time: SystemTime,
}

impl SystemMetricsTelemetry {
    /// Create a new system metrics telemetry collector
    pub fn new(telemetry_writer: TelemetryWriter, sampling_rate: f32) -> Self {
        Self {
            telemetry_writer,
            sampling_rate,
            last_sample_time: SystemTime::now(),
        }
    }

    /// Log system metrics event if sampling criteria met
    pub fn log_metrics(&mut self, metrics: &SystemMetrics) -> Result<()> {
        if self.should_sample() {
            let event = SystemMetricsEvent::from_metrics(metrics);
            self.telemetry_writer.log("system.metrics", event)?;
            self.last_sample_time = SystemTime::now();
        }
        Ok(())
    }

    /// Log system health check event
    pub fn log_health_check(&self, health: &SystemHealthEvent) -> Result<()> {
        // Health checks are always logged at 100% sampling per Telemetry Ruleset #9
        self.telemetry_writer.log("system.health", health)
    }

    /// Log performance threshold violation
    pub fn log_threshold_violation(&self, violation: &ThresholdViolationEvent) -> Result<()> {
        // Violations are always logged at 100% sampling per Telemetry Ruleset #9
        self.telemetry_writer
            .log("system.threshold_violation", violation)
    }

    /// Check if we should sample this event
    fn should_sample(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f32>() < self.sampling_rate
    }
}

impl SystemMetricsEvent {
    /// Create system metrics event from collected metrics
    pub fn from_metrics(metrics: &SystemMetrics) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();

        Self {
            cpu_usage: metrics.cpu_usage,
            memory_usage: metrics.memory_usage,
            disk_read_bytes: metrics.disk_io.read_bytes,
            disk_write_bytes: metrics.disk_io.write_bytes,
            network_rx_bytes: metrics.network_io.rx_bytes,
            network_tx_bytes: metrics.network_io.tx_bytes,
            gpu_utilization: metrics.gpu_metrics.utilization,
            gpu_memory_used: metrics.gpu_metrics.memory_used,
            uptime_seconds: 0, // Will be set by collector
            process_count: 0,  // Will be set by collector
            load_average: LoadAverageTelemetry {
                load_1min: 0.0,
                load_5min: 0.0,
                load_15min: 0.0,
            },
            timestamp,
        }
    }
}

/// System health event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealthEvent {
    pub status: String,
    pub checks: Vec<HealthCheckTelemetry>,
    pub timestamp: u64,
}

/// Health check for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckTelemetry {
    pub name: String,
    pub status: String,
    pub message: String,
    pub value: Option<f32>,
    pub threshold: Option<f32>,
}

/// Threshold violation event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolationEvent {
    pub metric_name: String,
    pub current_value: f32,
    pub threshold_value: f32,
    pub severity: String,
    pub timestamp: u64,
}

impl ThresholdViolationEvent {
    pub fn new(
        metric_name: String,
        current_value: f32,
        threshold_value: f32,
        severity: String,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();

        Self {
            metric_name,
            current_value,
            threshold_value,
            severity,
            timestamp,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::SystemMetricsCollector;

    #[test]
    fn test_metrics_event_creation() {
        let mut collector = SystemMetricsCollector::new();
        let metrics = collector.collect_metrics();
        let event = SystemMetricsEvent::from_metrics(&metrics);

        assert!(event.cpu_usage >= 0.0 && event.cpu_usage <= 100.0);
        assert!(event.memory_usage >= 0.0 && event.memory_usage <= 100.0);
        assert!(event.timestamp > 0);
    }

    #[test]
    fn test_threshold_violation_event() {
        let violation = ThresholdViolationEvent::new(
            "cpu_usage".to_string(),
            95.0,
            90.0,
            "critical".to_string(),
        );

        assert_eq!(violation.metric_name, "cpu_usage");
        assert_eq!(violation.current_value, 95.0);
        assert_eq!(violation.threshold_value, 90.0);
        assert_eq!(violation.severity, "critical");
        assert!(violation.timestamp > 0);
    }
}
