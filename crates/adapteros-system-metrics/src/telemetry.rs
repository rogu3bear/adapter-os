//! Telemetry integration for system metrics
//!
//! Provides telemetry event generation for system metrics following
//! AdapterOS telemetry patterns and canonical JSON serialization.
//! Enhanced with monitoring operations including alerts, anomalies, and baselines.

use crate::SystemMetrics;
use adapteros_core::Result;
use adapteros_telemetry::TelemetryWriter;
use anyhow::anyhow;
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
        // Always log critical system metrics for monitoring
        let event = SystemMetricsEvent::from_metrics(metrics);
        self.telemetry_writer.log("system.metrics", event)?;
        self.last_sample_time = SystemTime::now();

        // Log alerts for critical thresholds
        if metrics.cpu_usage > 90.0 {
            self.log_threshold_violation(&ThresholdViolationEvent::new(
                "cpu_usage".to_string(),
                metrics.cpu_usage,
                90.0,
                "critical".to_string(),
            ))?;
        }

        if metrics.memory_usage > 85.0 {
            self.log_threshold_violation(&ThresholdViolationEvent::new(
                "memory_usage".to_string(),
                metrics.memory_usage,
                85.0,
                "warning".to_string(),
            ))?;
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

    /// Log alert triggered event
    pub fn log_alert_triggered(&self, alert: &AlertTriggeredEvent) -> Result<()> {
        // Alerts are always logged at 100% sampling per Telemetry Ruleset #9
        self.telemetry_writer.log("alert.triggered", alert)
    }

    /// Log alert escalation event
    pub fn log_alert_escalated(&self, escalation: &AlertEscalatedEvent) -> Result<()> {
        // Escalations are always logged at 100% sampling per Telemetry Ruleset #9
        self.telemetry_writer.log("alert.escalated", escalation)
    }

    /// Log anomaly detection event
    pub fn log_anomaly_detected(&self, anomaly: &AnomalyDetectedEvent) -> Result<()> {
        // Anomalies are always logged at 100% sampling per Telemetry Ruleset #9
        self.telemetry_writer.log("anomaly.detected", anomaly)
    }

    /// Log baseline calculation event
    pub fn log_baseline_calculated(&self, baseline: &BaselineCalculatedEvent) -> Result<()> {
        // Baseline calculations are logged at configured sampling rate
        if self.should_sample() {
            self.telemetry_writer.log("baseline.calculated", baseline)
        } else {
            Ok(())
        }
    }

    /// Log notification sent event
    pub fn log_notification_sent(&self, notification: &NotificationSentEvent) -> Result<()> {
        // Notifications are always logged at 100% sampling per Telemetry Ruleset #9
        self.telemetry_writer.log("notification.sent", notification)
    }

    /// Log metrics collection event
    pub fn log_metrics_collection(&self, collection: &MetricsCollectionEvent) -> Result<()> {
        // Metrics collection events are logged at configured sampling rate
        if self.should_sample() {
            self.telemetry_writer.log("metrics.collection", collection)
        } else {
            Ok(())
        }
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

// ===== Enhanced Monitoring Telemetry Events =====

/// Alert triggered event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertTriggeredEvent {
    pub alert_id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub threshold_value: f64,
    pub severity: String,
    pub timestamp: u64,
}

/// Builder for alert triggered events
#[derive(Debug, Default)]
pub struct AlertTriggeredEventBuilder {
    alert_id: Option<String>,
    rule_id: Option<String>,
    rule_name: Option<String>,
    worker_id: Option<String>,
    tenant_id: Option<String>,
    metric_name: Option<String>,
    metric_value: Option<f64>,
    threshold_value: Option<f64>,
    severity: Option<String>,
}

/// Parameters for alert triggered events
#[derive(Debug)]
pub struct AlertTriggeredEventParams {
    pub alert_id: String,
    pub rule_id: String,
    pub rule_name: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub threshold_value: f64,
    pub severity: String,
}

impl AlertTriggeredEventBuilder {
    /// Create a new alert triggered event builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the alert ID (required)
    pub fn alert_id(mut self, alert_id: impl Into<String>) -> Self {
        self.alert_id = Some(alert_id.into());
        self
    }

    /// Set the rule ID (required)
    pub fn rule_id(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }

    /// Set the rule name (required)
    pub fn rule_name(mut self, rule_name: impl Into<String>) -> Self {
        self.rule_name = Some(rule_name.into());
        self
    }

    /// Set the worker ID (required)
    pub fn worker_id(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = Some(worker_id.into());
        self
    }

    /// Set the tenant ID (required)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the metric name (required)
    pub fn metric_name(mut self, metric_name: impl Into<String>) -> Self {
        self.metric_name = Some(metric_name.into());
        self
    }

    /// Set the metric value (required)
    pub fn metric_value(mut self, metric_value: f64) -> Self {
        self.metric_value = Some(metric_value);
        self
    }

    /// Set the threshold value (required)
    pub fn threshold_value(mut self, threshold_value: f64) -> Self {
        self.threshold_value = Some(threshold_value);
        self
    }

    /// Set severity level (required)
    pub fn severity(mut self, severity: impl Into<String>) -> Self {
        self.severity = Some(severity.into());
        self
    }

    /// Build the alert triggered event parameters
    pub fn build(self) -> Result<AlertTriggeredEventParams> {
        Ok(AlertTriggeredEventParams {
            alert_id: self
                .alert_id
                .ok_or_else(|| anyhow!("alert_id is required"))?,
            rule_id: self.rule_id.ok_or_else(|| anyhow!("rule_id is required"))?,
            rule_name: self
                .rule_name
                .ok_or_else(|| anyhow!("rule_name is required"))?,
            worker_id: self
                .worker_id
                .ok_or_else(|| anyhow!("worker_id is required"))?,
            tenant_id: self
                .tenant_id
                .ok_or_else(|| anyhow!("tenant_id is required"))?,
            metric_name: self
                .metric_name
                .ok_or_else(|| anyhow!("metric_name is required"))?,
            metric_value: self
                .metric_value
                .ok_or_else(|| anyhow!("metric_value is required"))?,
            threshold_value: self
                .threshold_value
                .ok_or_else(|| anyhow!("threshold_value is required"))?,
            severity: self
                .severity
                .ok_or_else(|| anyhow!("severity is required"))?,
        })
    }

    /// Build the alert triggered event directly
    pub fn build_event(self) -> Result<AlertTriggeredEvent> {
        self.build().map(AlertTriggeredEvent::new)
    }
}

impl AlertTriggeredEvent {
    /// Create a builder for constructing alert triggered events
    pub fn builder() -> AlertTriggeredEventBuilder {
        AlertTriggeredEventBuilder::new()
    }

    pub fn new(params: AlertTriggeredEventParams) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();

        Self {
            alert_id: params.alert_id,
            rule_id: params.rule_id,
            rule_name: params.rule_name,
            worker_id: params.worker_id,
            tenant_id: params.tenant_id,
            metric_name: params.metric_name,
            metric_value: params.metric_value,
            threshold_value: params.threshold_value,
            severity: params.severity,
            timestamp,
        }
    }
}

/// Alert escalated event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEscalatedEvent {
    pub alert_id: String,
    pub rule_id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub old_level: i64,
    pub new_level: i64,
    pub escalation_reason: String,
    pub timestamp: u64,
}

impl AlertEscalatedEvent {
    pub fn new(
        alert_id: String,
        rule_id: String,
        worker_id: String,
        tenant_id: String,
        old_level: i64,
        new_level: i64,
        escalation_reason: String,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();

        Self {
            alert_id,
            rule_id,
            worker_id,
            tenant_id,
            old_level,
            new_level,
            escalation_reason,
            timestamp,
        }
    }
}

/// Builder for creating anomaly detected events
#[derive(Debug, Default)]
pub struct AnomalyDetectedEventBuilder {
    anomaly_id: Option<String>,
    worker_id: Option<String>,
    tenant_id: Option<String>,
    metric_name: Option<String>,
    detected_value: Option<f64>,
    confidence_score: Option<f64>,
    severity: Option<String>,
    detection_method: Option<String>,
    baseline_mean: Option<f64>,
    baseline_std_dev: Option<f64>,
}

impl AnomalyDetectedEventBuilder {
    /// Create a new anomaly detected event builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the anomaly ID (required)
    pub fn anomaly_id(mut self, anomaly_id: impl Into<String>) -> Self {
        self.anomaly_id = Some(anomaly_id.into());
        self
    }

    /// Set the worker ID (required)
    pub fn worker_id(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = Some(worker_id.into());
        self
    }

    /// Set the tenant ID (required)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the metric name (required)
    pub fn metric_name(mut self, metric_name: impl Into<String>) -> Self {
        self.metric_name = Some(metric_name.into());
        self
    }

    /// Set the detected value (required)
    pub fn detected_value(mut self, detected_value: f64) -> Self {
        self.detected_value = Some(detected_value);
        self
    }

    /// Set the confidence score (required)
    pub fn confidence_score(mut self, confidence_score: f64) -> Self {
        self.confidence_score = Some(confidence_score);
        self
    }

    /// Set the severity (required)
    pub fn severity(mut self, severity: impl Into<String>) -> Self {
        self.severity = Some(severity.into());
        self
    }

    /// Set the detection method (required)
    pub fn detection_method(mut self, detection_method: impl Into<String>) -> Self {
        self.detection_method = Some(detection_method.into());
        self
    }

    /// Set the baseline mean (required)
    pub fn baseline_mean(mut self, baseline_mean: f64) -> Self {
        self.baseline_mean = Some(baseline_mean);
        self
    }

    /// Set the baseline standard deviation (required)
    pub fn baseline_std_dev(mut self, baseline_std_dev: f64) -> Self {
        self.baseline_std_dev = Some(baseline_std_dev);
        self
    }

    /// Build the anomaly detected event
    pub fn build(self) -> Result<AnomalyDetectedEvent> {
        Ok(AnomalyDetectedEvent::new(
            self.anomaly_id
                .ok_or_else(|| anyhow::anyhow!("anomaly_id is required"))?,
            self.worker_id
                .ok_or_else(|| anyhow::anyhow!("worker_id is required"))?,
            self.tenant_id
                .ok_or_else(|| anyhow::anyhow!("tenant_id is required"))?,
            self.metric_name
                .ok_or_else(|| anyhow::anyhow!("metric_name is required"))?,
            self.detected_value
                .ok_or_else(|| anyhow::anyhow!("detected_value is required"))?,
            self.confidence_score
                .ok_or_else(|| anyhow::anyhow!("confidence_score is required"))?,
            self.severity
                .ok_or_else(|| anyhow::anyhow!("severity is required"))?,
            self.detection_method
                .ok_or_else(|| anyhow::anyhow!("detection_method is required"))?,
            self.baseline_mean
                .ok_or_else(|| anyhow::anyhow!("baseline_mean is required"))?,
            self.baseline_std_dev
                .ok_or_else(|| anyhow::anyhow!("baseline_std_dev is required"))?,
        ))
    }
}

/// Anomaly detected event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetectedEvent {
    pub anomaly_id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub detected_value: f64,
    pub confidence_score: f64,
    pub severity: String,
    pub detection_method: String,
    pub baseline_mean: f64,
    pub baseline_std_dev: f64,
    pub timestamp: u64,
}

impl AnomalyDetectedEvent {
    pub fn new(
        anomaly_id: String,
        worker_id: String,
        tenant_id: String,
        metric_name: String,
        detected_value: f64,
        confidence_score: f64,
        severity: String,
        detection_method: String,
        baseline_mean: f64,
        baseline_std_dev: f64,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();

        Self {
            anomaly_id,
            worker_id,
            tenant_id,
            metric_name,
            detected_value,
            confidence_score,
            severity,
            detection_method,
            baseline_mean,
            baseline_std_dev,
            timestamp,
        }
    }
}

/// Baseline calculated event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineCalculatedEvent {
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub baseline_type: String,
    pub calculation_period_days: i64,
    pub sample_count: usize,
    pub statistical_measures: StatisticalMeasuresTelemetry,
    pub timestamp: u64,
}

/// Builder for baseline calculated events
#[derive(Debug, Default)]
pub struct BaselineCalculatedEventBuilder {
    worker_id: Option<String>,
    tenant_id: Option<String>,
    metric_name: Option<String>,
    baseline_value: Option<f64>,
    baseline_type: Option<String>,
    calculation_period_days: Option<i64>,
    sample_count: Option<usize>,
    statistical_measures: Option<StatisticalMeasuresTelemetry>,
}

/// Parameters for baseline calculated events
#[derive(Debug)]
pub struct BaselineCalculatedEventParams {
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub baseline_type: String,
    pub calculation_period_days: i64,
    pub sample_count: usize,
    pub statistical_measures: StatisticalMeasuresTelemetry,
}

/// Statistical measures for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalMeasuresTelemetry {
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub min_value: f64,
    pub max_value: f64,
    pub iqr: f64,
    pub percentile_95: f64,
    pub percentile_99: f64,
}

impl BaselineCalculatedEvent {
    /// Create a builder for constructing baseline calculated events
    pub fn builder() -> BaselineCalculatedEventBuilder {
        BaselineCalculatedEventBuilder::new()
    }

    pub fn new(params: BaselineCalculatedEventParams) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();

        Self {
            worker_id: params.worker_id,
            tenant_id: params.tenant_id,
            metric_name: params.metric_name,
            baseline_value: params.baseline_value,
            baseline_type: params.baseline_type,
            calculation_period_days: params.calculation_period_days,
            sample_count: params.sample_count,
            statistical_measures: params.statistical_measures,
            timestamp,
        }
    }
}

impl BaselineCalculatedEventBuilder {
    /// Create a new baseline calculated event builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the worker ID (required)
    pub fn worker_id(mut self, worker_id: impl Into<String>) -> Self {
        self.worker_id = Some(worker_id.into());
        self
    }

    /// Set the tenant ID (required)
    pub fn tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Set the metric name (required)
    pub fn metric_name(mut self, metric_name: impl Into<String>) -> Self {
        self.metric_name = Some(metric_name.into());
        self
    }

    /// Set the baseline value (required)
    pub fn baseline_value(mut self, baseline_value: f64) -> Self {
        self.baseline_value = Some(baseline_value);
        self
    }

    /// Set the baseline type (required)
    pub fn baseline_type(mut self, baseline_type: impl Into<String>) -> Self {
        self.baseline_type = Some(baseline_type.into());
        self
    }

    /// Set the calculation period in days (required)
    pub fn calculation_period_days(mut self, days: i64) -> Self {
        self.calculation_period_days = Some(days);
        self
    }

    /// Set the sample count (required)
    pub fn sample_count(mut self, count: usize) -> Self {
        self.sample_count = Some(count);
        self
    }

    /// Set the statistical measures (required)
    pub fn statistical_measures(mut self, measures: StatisticalMeasuresTelemetry) -> Self {
        self.statistical_measures = Some(measures);
        self
    }

    /// Build the baseline calculated event parameters
    pub fn build(self) -> Result<BaselineCalculatedEventParams> {
        Ok(BaselineCalculatedEventParams {
            worker_id: self
                .worker_id
                .ok_or_else(|| anyhow!("worker_id is required"))?,
            tenant_id: self
                .tenant_id
                .ok_or_else(|| anyhow!("tenant_id is required"))?,
            metric_name: self
                .metric_name
                .ok_or_else(|| anyhow!("metric_name is required"))?,
            baseline_value: self
                .baseline_value
                .ok_or_else(|| anyhow!("baseline_value is required"))?,
            baseline_type: self
                .baseline_type
                .ok_or_else(|| anyhow!("baseline_type is required"))?,
            calculation_period_days: self
                .calculation_period_days
                .ok_or_else(|| anyhow!("calculation_period_days is required"))?,
            sample_count: self
                .sample_count
                .ok_or_else(|| anyhow!("sample_count is required"))?,
            statistical_measures: self
                .statistical_measures
                .ok_or_else(|| anyhow!("statistical_measures is required"))?,
        })
    }

    /// Build the baseline calculated event directly
    pub fn build_event(self) -> Result<BaselineCalculatedEvent> {
        self.build().map(BaselineCalculatedEvent::new)
    }
}

/// Notification sent event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationSentEvent {
    pub notification_id: String,
    pub alert_id: String,
    pub notification_type: String,
    pub recipient: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub timestamp: u64,
}

impl NotificationSentEvent {
    pub fn new(
        notification_id: String,
        alert_id: String,
        notification_type: String,
        recipient: String,
        success: bool,
        error_message: Option<String>,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();

        Self {
            notification_id,
            alert_id,
            notification_type,
            recipient,
            success,
            error_message,
            timestamp,
        }
    }
}

/// Metrics collection event for telemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsCollectionEvent {
    pub worker_count: usize,
    pub metrics_collected: usize,
    pub collection_duration_ms: u64,
    pub collection_interval_secs: u64,
    pub errors_count: usize,
    pub timestamp: u64,
}

impl MetricsCollectionEvent {
    pub fn new(
        worker_count: usize,
        metrics_collected: usize,
        collection_duration_ms: u64,
        collection_interval_secs: u64,
        errors_count: usize,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs();

        Self {
            worker_count,
            metrics_collected,
            collection_duration_ms,
            collection_interval_secs,
            errors_count,
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

    #[test]
    fn test_alert_triggered_event() {
        let alert = AlertTriggeredEvent::builder()
            .alert_id("alert-123")
            .rule_id("rule-456")
            .rule_name("High CPU Usage")
            .worker_id("worker-789")
            .tenant_id("tenant-001")
            .metric_name("cpu_usage")
            .metric_value(95.0)
            .threshold_value(90.0)
            .severity("critical")
            .build_event()
            .expect("alert builder succeeds");

        assert_eq!(alert.alert_id, "alert-123");
        assert_eq!(alert.rule_id, "rule-456");
        assert_eq!(alert.rule_name, "High CPU Usage");
        assert_eq!(alert.worker_id, "worker-789");
        assert_eq!(alert.tenant_id, "tenant-001");
        assert_eq!(alert.metric_name, "cpu_usage");
        assert_eq!(alert.metric_value, 95.0);
        assert_eq!(alert.threshold_value, 90.0);
        assert_eq!(alert.severity, "critical");
        assert!(alert.timestamp > 0);
    }

    #[test]
    fn test_anomaly_detected_event() {
        let anomaly = AnomalyDetectedEvent::new(
            "anomaly-123".to_string(),
            "worker-789".to_string(),
            "tenant-001".to_string(),
            "cpu_usage".to_string(),
            95.0,
            0.85,
            "warning".to_string(),
            "z_score".to_string(),
            50.0,
            10.0,
        );

        assert_eq!(anomaly.anomaly_id, "anomaly-123");
        assert_eq!(anomaly.worker_id, "worker-789");
        assert_eq!(anomaly.tenant_id, "tenant-001");
        assert_eq!(anomaly.metric_name, "cpu_usage");
        assert_eq!(anomaly.detected_value, 95.0);
        assert_eq!(anomaly.confidence_score, 0.85);
        assert_eq!(anomaly.severity, "warning");
        assert_eq!(anomaly.detection_method, "z_score");
        assert_eq!(anomaly.baseline_mean, 50.0);
        assert_eq!(anomaly.baseline_std_dev, 10.0);
        assert!(anomaly.timestamp > 0);
    }

    #[test]
    fn test_baseline_calculated_event() {
        let statistical_measures = StatisticalMeasuresTelemetry {
            mean: 50.0,
            median: 48.0,
            std_dev: 10.0,
            min_value: 20.0,
            max_value: 80.0,
            iqr: 15.0,
            percentile_95: 70.0,
            percentile_99: 75.0,
        };

        let baseline = BaselineCalculatedEvent::builder()
            .worker_id("worker-789")
            .tenant_id("tenant-001")
            .metric_name("cpu_usage")
            .baseline_value(50.0)
            .baseline_type("statistical")
            .calculation_period_days(7)
            .sample_count(1000)
            .statistical_measures(statistical_measures)
            .build_event()
            .expect("baseline builder succeeds");

        assert_eq!(baseline.worker_id, "worker-789");
        assert_eq!(baseline.tenant_id, "tenant-001");
        assert_eq!(baseline.metric_name, "cpu_usage");
        assert_eq!(baseline.baseline_value, 50.0);
        assert_eq!(baseline.baseline_type, "statistical");
        assert_eq!(baseline.calculation_period_days, 7);
        assert_eq!(baseline.sample_count, 1000);
        assert_eq!(baseline.statistical_measures.mean, 50.0);
        assert_eq!(baseline.statistical_measures.median, 48.0);
        assert_eq!(baseline.statistical_measures.std_dev, 10.0);
        assert!(baseline.timestamp > 0);
    }

    #[test]
    fn test_notification_sent_event() {
        let notification = NotificationSentEvent::new(
            "notification-123".to_string(),
            "alert-456".to_string(),
            "email".to_string(),
            "admin@example.com".to_string(),
            true,
            None,
        );

        assert_eq!(notification.notification_id, "notification-123");
        assert_eq!(notification.alert_id, "alert-456");
        assert_eq!(notification.notification_type, "email");
        assert_eq!(notification.recipient, "admin@example.com");
        assert!(notification.success);
        assert!(notification.error_message.is_none());
        assert!(notification.timestamp > 0);
    }

    #[test]
    fn test_metrics_collection_event() {
        let collection = MetricsCollectionEvent::new(5, 150, 250, 30, 0);

        assert_eq!(collection.worker_count, 5);
        assert_eq!(collection.metrics_collected, 150);
        assert_eq!(collection.collection_duration_ms, 250);
        assert_eq!(collection.collection_interval_secs, 30);
        assert_eq!(collection.errors_count, 0);
        assert!(collection.timestamp > 0);
    }
}
