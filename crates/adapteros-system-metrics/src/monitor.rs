//! Continuous system monitoring pipeline
//!
//! Provides continuous monitoring of system metrics with telemetry integration,
//! policy enforcement, and alerting capabilities.

use crate::policy::SystemMetricsPolicy;
use crate::{MetricsConfig, SystemMetricsCollector};
use adapteros_core::Result;
use adapteros_telemetry::{SecurityEvent, TelemetryWriter};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{interval, sleep};
use tracing::{debug, error, info, warn};

/// System metrics monitor
pub struct SystemMonitor {
    collector: SystemMetricsCollector,
    policy: SystemMetricsPolicy,
    telemetry_writer: TelemetryWriter,
    config: MetricsConfig,
    last_collection: SystemTime,
    violation_count: u32,
}

impl SystemMonitor {
    /// Create a new system monitor
    pub fn new(telemetry_writer: TelemetryWriter, config: MetricsConfig) -> Self {
        let thresholds = config.thresholds.clone();
        let policy = SystemMetricsPolicy::new(thresholds);

        Self {
            collector: SystemMetricsCollector::new(),
            policy,
            telemetry_writer,
            config,
            last_collection: SystemTime::now(),
            violation_count: 0,
        }
    }

    /// Start continuous monitoring
    pub async fn start_monitoring(&mut self) -> Result<()> {
        info!(
            "Starting system metrics monitoring with interval: {} secs",
            self.config.collection_interval_secs
        );

        let mut interval = interval(Duration::from_secs(self.config.collection_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = self.collect_and_process_metrics().await {
                error!("Failed to collect system metrics: {}", e);
                self.violation_count += 1;

                // Log the error as a security event
                if let Err(telemetry_err) =
                    self.telemetry_writer
                        .log_security_event(SecurityEvent::PolicyViolation {
                            policy: "system_monitoring".to_string(),
                            violation_type: "collection_failure".to_string(),
                            details: e.to_string(),
                            timestamp: chrono::Utc::now().to_rfc3339(),
                        })
                {
                    error!("Failed to log monitoring error: {}", telemetry_err);
                }
            } else {
                // Reset violation count on successful collection
                self.violation_count = 0;
            }

            // Check if we should stop monitoring due to too many violations
            if self.violation_count >= 10 {
                error!(
                    "Too many monitoring violations ({}), stopping monitor",
                    self.violation_count
                );
                break;
            }
        }

        Ok(())
    }

    /// Collect and process system metrics
    async fn collect_and_process_metrics(&mut self) -> Result<()> {
        let metrics = self.collector.collect_metrics();

        // Log metrics to telemetry if sampling criteria met
        if self.should_sample() {
            let event = crate::telemetry::SystemMetricsEvent::from_metrics(&metrics);
            self.telemetry_writer.log("system.metrics", event)?;
        }

        // Check policy thresholds
        if let Err(e) = self.policy.check_thresholds(&metrics) {
            warn!("Policy threshold violation: {}", e);

            // Log threshold violation
            let violation = crate::telemetry::ThresholdViolationEvent::new(
                "system_metrics".to_string(),
                0.0, // Would be set to actual violating metric value
                0.0, // Would be set to actual threshold value
                "warning".to_string(),
            );

            self.telemetry_writer
                .log("system.threshold_violation", violation)?;

            // Log as security event if critical
            if self.policy.get_health_status(&metrics)
                == crate::policy::SystemHealthStatus::Critical
            {
                self.telemetry_writer
                    .log_security_event(SecurityEvent::PolicyViolation {
                        policy: "performance".to_string(),
                        violation_type: "threshold_exceeded".to_string(),
                        details: e.to_string(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    })?;
            }
        }

        // Check memory headroom policy
        let headroom = self.collector.headroom_pct();
        if let Err(e) = self.policy.check_memory_headroom(headroom) {
            warn!("Memory headroom violation: {}", e);

            self.telemetry_writer
                .log_security_event(SecurityEvent::PolicyViolation {
                    policy: "memory".to_string(),
                    violation_type: "insufficient_headroom".to_string(),
                    details: e.to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                })?;
        }

        self.last_collection = SystemTime::now();
        debug!("System metrics collected successfully");

        Ok(())
    }

    /// Check if we should sample this collection
    fn should_sample(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f32>() < self.config.sampling_rate
    }

    /// Get current system health status
    pub fn get_health_status(&mut self) -> crate::policy::SystemHealthStatus {
        let metrics = self.collector.collect_metrics();
        self.policy.get_health_status(&metrics)
    }

    /// Get current metrics
    pub fn get_current_metrics(&mut self) -> crate::SystemMetrics {
        self.collector.collect_metrics()
    }

    /// Get violation count
    pub fn get_violation_count(&self) -> u32 {
        self.violation_count
    }

    /// Reset violation count
    pub fn reset_violation_count(&mut self) {
        self.violation_count = 0;
    }
}

/// System monitoring service
pub struct SystemMonitoringService {
    monitor: Option<SystemMonitor>,
    config: MetricsConfig,
}

impl SystemMonitoringService {
    /// Create a new monitoring service
    pub fn new(config: MetricsConfig) -> Self {
        Self {
            monitor: None,
            config,
        }
    }

    /// Start the monitoring service
    pub async fn start(&mut self, telemetry_writer: TelemetryWriter) -> Result<()> {
        let mut monitor = SystemMonitor::new(telemetry_writer, self.config.clone());

        info!("Starting system monitoring service");
        monitor.start_monitoring().await?;

        Ok(())
    }

    /// Stop the monitoring service
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(monitor) = &mut self.monitor {
            info!("Stopping system monitoring service");
            // The monitor will stop when the start_monitoring loop exits
        }
        Ok(())
    }

    /// Get service status
    pub fn is_running(&self) -> bool {
        self.monitor.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::TelemetryWriter;
    use std::path::Path;

    #[tokio::test]
    async fn test_monitor_creation() {
        let config = MetricsConfig::default();
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024)
            .expect("Test telemetry writer creation should succeed");
        let monitor = SystemMonitor::new(telemetry_writer, config);

        assert_eq!(monitor.get_violation_count(), 0);
    }

    #[tokio::test]
    async fn test_metrics_collection() {
        let config = MetricsConfig::default();
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024)
            .expect("Test telemetry writer creation should succeed");
        let mut monitor = SystemMonitor::new(telemetry_writer, config);

        let metrics = monitor.get_current_metrics();
        assert!(metrics.cpu_usage >= 0.0 && metrics.cpu_usage <= 100.0);
        assert!(metrics.memory_usage >= 0.0 && metrics.memory_usage <= 100.0);
    }

    #[tokio::test]
    async fn test_health_status() {
        let config = MetricsConfig::default();
        let telemetry_writer = TelemetryWriter::new(Path::new("/tmp"), 1000, 1024 * 1024)
            .expect("Test telemetry writer creation should succeed");
        let mut monitor = SystemMonitor::new(telemetry_writer, config);

        let status = monitor.get_health_status();
        // Status should be one of the valid health statuses
        assert!(matches!(
            status,
            crate::policy::SystemHealthStatus::Healthy
                | crate::policy::SystemHealthStatus::Warning
                | crate::policy::SystemHealthStatus::Critical
        ));
    }
}
