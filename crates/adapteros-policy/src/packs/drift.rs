//! Drift Policy Pack
//!
//! Monitors and prevents model drift, performance degradation, and behavioral changes.
//! Ensures system stability and consistent performance over time.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};

/// Drift policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftConfig {
    /// Enable drift detection
    pub enable: bool,
    /// Drift detection thresholds
    pub thresholds: DriftThresholds,
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
    /// Alerting configuration
    pub alerting: AlertingConfig,
    /// Baseline configuration
    pub baseline: BaselineConfig,
}

/// Drift detection thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftThresholds {
    /// Performance drift threshold (percentage)
    pub performance_drift_pct: f32,
    /// Accuracy drift threshold (percentage)
    pub accuracy_drift_pct: f32,
    /// Latency drift threshold (percentage)
    pub latency_drift_pct: f32,
    /// Memory usage drift threshold (percentage)
    pub memory_drift_pct: f32,
    /// Behavioral drift threshold (percentage)
    pub behavioral_drift_pct: f32,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Monitoring interval (minutes)
    pub interval_minutes: u32,
    /// Enable continuous monitoring
    pub continuous: bool,
    /// Monitoring metrics
    pub metrics: Vec<MonitoringMetric>,
    /// Data collection window (hours)
    pub collection_window_hours: u32,
}

/// Monitoring metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MonitoringMetric {
    /// Inference latency
    InferenceLatency,
    /// Memory usage
    MemoryUsage,
    /// CPU usage
    CpuUsage,
    /// Accuracy metrics
    Accuracy,
    /// Throughput
    Throughput,
    /// Error rate
    ErrorRate,
}

/// Alerting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingConfig {
    /// Enable alerting
    pub enable: bool,
    /// Alert severity levels
    pub severity_levels: Vec<AlertSeverity>,
    /// Alert channels
    pub channels: Vec<AlertChannel>,
    /// Alert cooldown period (minutes)
    pub cooldown_minutes: u32,
}

/// Alert severity level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    /// Low severity
    Low,
    /// Medium severity
    Medium,
    /// High severity
    High,
    /// Critical severity
    Critical,
}

/// Alert channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertChannel {
    /// Log-based alerts
    Log,
    /// Email alerts
    Email,
    /// Webhook alerts
    Webhook,
    /// Slack alerts
    Slack,
}

/// Baseline configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineConfig {
    /// Baseline establishment period (days)
    pub establishment_period_days: u32,
    /// Baseline update frequency (days)
    pub update_frequency_days: u32,
    /// Baseline validation rules
    pub validation_rules: Vec<BaselineValidationRule>,
    /// Baseline storage
    pub storage: BaselineStorage,
}

/// Baseline validation rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BaselineValidationRule {
    /// Statistical significance
    StatisticalSignificance,
    /// Minimum sample size
    MinimumSampleSize,
    /// Temporal consistency
    TemporalConsistency,
    /// Performance bounds
    PerformanceBounds,
}

/// Baseline storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineStorage {
    /// Storage backend
    pub backend: StorageBackend,
    /// Retention period (days)
    pub retention_days: u32,
    /// Compression enabled
    pub compression: bool,
}

/// Storage backend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageBackend {
    /// Local file system
    Local,
    /// Database
    Database,
    /// Cloud storage
    Cloud,
}

impl Default for DriftConfig {
    fn default() -> Self {
        Self {
            enable: true,
            thresholds: DriftThresholds {
                performance_drift_pct: 10.0,
                accuracy_drift_pct: 5.0,
                latency_drift_pct: 15.0,
                memory_drift_pct: 20.0,
                behavioral_drift_pct: 8.0,
            },
            monitoring: MonitoringConfig {
                interval_minutes: 5,
                continuous: true,
                metrics: vec![
                    MonitoringMetric::InferenceLatency,
                    MonitoringMetric::MemoryUsage,
                    MonitoringMetric::Accuracy,
                    MonitoringMetric::Throughput,
                ],
                collection_window_hours: 24,
            },
            alerting: AlertingConfig {
                enable: true,
                severity_levels: vec![
                    AlertSeverity::Medium,
                    AlertSeverity::High,
                    AlertSeverity::Critical,
                ],
                channels: vec![AlertChannel::Log, AlertChannel::Email],
                cooldown_minutes: 30,
            },
            baseline: BaselineConfig {
                establishment_period_days: 7,
                update_frequency_days: 1,
                validation_rules: vec![
                    BaselineValidationRule::StatisticalSignificance,
                    BaselineValidationRule::MinimumSampleSize,
                    BaselineValidationRule::TemporalConsistency,
                ],
                storage: BaselineStorage {
                    backend: StorageBackend::Local,
                    retention_days: 90,
                    compression: true,
                },
            },
        }
    }
}

/// Drift measurement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftMeasurement {
    /// Metric type
    pub metric_type: MonitoringMetric,
    /// Current value
    pub current_value: f64,
    /// Baseline value
    pub baseline_value: f64,
    /// Drift percentage
    pub drift_percentage: f64,
    /// Measurement timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Confidence level
    pub confidence: f64,
}

/// Drift alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftAlert {
    /// Alert ID
    pub alert_id: String,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Alert message
    pub message: String,
    /// Drift measurement
    pub measurement: DriftMeasurement,
    /// Alert timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Alert channels
    pub channels: Vec<AlertChannel>,
}

/// Drift policy enforcement
pub struct DriftPolicy {
    config: DriftConfig,
}

impl DriftPolicy {
    /// Create a new drift policy
    pub fn new(config: DriftConfig) -> Self {
        Self { config }
    }

    /// Calculate drift percentage
    pub fn calculate_drift_percentage(&self, current: f64, baseline: f64) -> f64 {
        if baseline == 0.0 {
            return 0.0;
        }
        ((current - baseline) / baseline).abs() * 100.0
    }

    /// Detect drift in measurement
    pub fn detect_drift(&self, measurement: &DriftMeasurement) -> Result<Option<DriftAlert>> {
        if !self.config.enable {
            return Ok(None);
        }

        let threshold = match measurement.metric_type {
            MonitoringMetric::InferenceLatency => self.config.thresholds.latency_drift_pct,
            MonitoringMetric::MemoryUsage => self.config.thresholds.memory_drift_pct,
            MonitoringMetric::Accuracy => self.config.thresholds.accuracy_drift_pct,
            MonitoringMetric::Throughput => self.config.thresholds.performance_drift_pct,
            MonitoringMetric::CpuUsage => self.config.thresholds.performance_drift_pct,
            MonitoringMetric::ErrorRate => self.config.thresholds.behavioral_drift_pct,
        };

        if measurement.drift_percentage > threshold as f64 {
            let severity = self.determine_alert_severity(measurement.drift_percentage);
            let alert = DriftAlert {
                alert_id: format!(
                    "drift_{}_{}",
                    format!("{:?}", measurement.metric_type).to_lowercase(),
                    measurement.timestamp.timestamp()
                ),
                severity,
                message: format!(
                    "Drift detected in {:?}: {:.2}% (threshold: {:.2}%)",
                    measurement.metric_type, measurement.drift_percentage, threshold
                ),
                measurement: measurement.clone(),
                timestamp: chrono::Utc::now(),
                channels: self.config.alerting.channels.clone(),
            };

            Ok(Some(alert))
        } else {
            Ok(None)
        }
    }

    /// Determine alert severity based on drift percentage
    fn determine_alert_severity(&self, drift_percentage: f64) -> AlertSeverity {
        if drift_percentage > 50.0 {
            AlertSeverity::Critical
        } else if drift_percentage > 25.0 {
            AlertSeverity::High
        } else if drift_percentage > 10.0 {
            AlertSeverity::Medium
        } else {
            AlertSeverity::Low
        }
    }

    /// Validate baseline data
    pub fn validate_baseline_data(&self, baseline_data: &[DriftMeasurement]) -> Result<()> {
        if baseline_data.is_empty() {
            return Err(AosError::PolicyViolation(
                "Baseline data is empty".to_string(),
            ));
        }

        // Check minimum sample size
        let min_samples = 100; // Minimum samples for statistical significance
        if baseline_data.len() < min_samples {
            return Err(AosError::PolicyViolation(format!(
                "Insufficient baseline samples: {} < {}",
                baseline_data.len(),
                min_samples
            )));
        }

        // Check temporal consistency
        self.validate_temporal_consistency(baseline_data)?;

        Ok(())
    }

    /// Validate temporal consistency
    fn validate_temporal_consistency(&self, data: &[DriftMeasurement]) -> Result<()> {
        if data.len() < 2 {
            return Ok(());
        }

        // Check that timestamps are in order
        for i in 1..data.len() {
            if data[i].timestamp < data[i - 1].timestamp {
                return Err(AosError::PolicyViolation(
                    "Baseline data timestamps are not in chronological order".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Update baseline
    pub fn update_baseline(&self, current_data: &[DriftMeasurement]) -> Result<DriftMeasurement> {
        if current_data.is_empty() {
            return Err(AosError::PolicyViolation(
                "No data provided for baseline update".to_string(),
            ));
        }

        // Calculate new baseline (simple average for now)
        let sum: f64 = current_data.iter().map(|m| m.current_value).sum();
        let average = sum / current_data.len() as f64;

        // Create new baseline measurement
        let baseline = DriftMeasurement {
            metric_type: current_data[0].metric_type.clone(),
            current_value: average,
            baseline_value: average,
            drift_percentage: 0.0,
            timestamp: chrono::Utc::now(),
            confidence: 0.95, // 95% confidence
        };

        Ok(baseline)
    }

    /// Check if monitoring interval has elapsed
    pub fn should_monitor(&self, last_monitoring: chrono::DateTime<chrono::Utc>) -> bool {
        let interval = chrono::Duration::minutes(self.config.monitoring.interval_minutes as i64);
        let next_monitoring = last_monitoring + interval;
        chrono::Utc::now() > next_monitoring
    }

    /// Check if baseline should be updated
    pub fn should_update_baseline(&self, last_update: chrono::DateTime<chrono::Utc>) -> bool {
        let update_interval =
            chrono::Duration::days(self.config.baseline.update_frequency_days as i64);
        let next_update = last_update + update_interval;
        chrono::Utc::now() > next_update
    }

    /// Validate drift measurement
    pub fn validate_drift_measurement(&self, measurement: &DriftMeasurement) -> Result<()> {
        // Check confidence level
        if measurement.confidence < 0.8 {
            return Err(AosError::PolicyViolation(
                "Drift measurement confidence is too low".to_string(),
            ));
        }

        // Check for reasonable values
        if measurement.current_value < 0.0 || measurement.baseline_value < 0.0 {
            return Err(AosError::PolicyViolation(
                "Drift measurement values cannot be negative".to_string(),
            ));
        }

        Ok(())
    }

    /// Generate drift report
    pub fn generate_drift_report(&self, measurements: &[DriftMeasurement]) -> String {
        let mut report = String::new();
        report.push_str("Drift Detection Report\n");
        report.push_str("=====================\n\n");

        for measurement in measurements {
            report.push_str(&format!("Metric: {:?}\n", measurement.metric_type));
            report.push_str(&format!("Current: {:.2}\n", measurement.current_value));
            report.push_str(&format!("Baseline: {:.2}\n", measurement.baseline_value));
            report.push_str(&format!("Drift: {:.2}%\n", measurement.drift_percentage));
            report.push_str(&format!("Confidence: {:.2}\n\n", measurement.confidence));
        }

        report
    }
}

impl Policy for DriftPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Drift
    }

    fn name(&self) -> &'static str {
        "Drift"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        let violations = Vec::new();

        // Basic validation - in a real implementation, this would check
        // specific policy requirements

        if violations.is_empty() {
            Ok(Audit::passed(self.id()))
        } else {
            Ok(Audit::failed(self.id(), violations))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_drift_policy_creation() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Drift);
        assert_eq!(policy.name(), "Drift");
        assert_eq!(policy.severity(), Severity::Medium);
    }

    #[test]
    fn test_drift_config_default() {
        let config = DriftConfig::default();
        assert!(config.enable);
        assert_eq!(config.thresholds.performance_drift_pct, 10.0);
        assert_eq!(config.thresholds.accuracy_drift_pct, 5.0);
        assert!(config.monitoring.continuous);
        assert!(config.alerting.enable);
    }

    #[test]
    fn test_calculate_drift_percentage() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);

        // Normal case
        let drift = policy.calculate_drift_percentage(110.0, 100.0);
        assert_eq!(drift, 10.0);

        // Zero baseline
        let drift_zero = policy.calculate_drift_percentage(100.0, 0.0);
        assert_eq!(drift_zero, 0.0);

        // Negative drift
        let drift_negative = policy.calculate_drift_percentage(90.0, 100.0);
        assert_eq!(drift_negative, 10.0);
    }

    #[test]
    fn test_detect_drift() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);

        let measurement = DriftMeasurement {
            metric_type: MonitoringMetric::InferenceLatency,
            current_value: 120.0,
            baseline_value: 100.0,
            drift_percentage: 20.0,
            timestamp: Utc::now(),
            confidence: 0.95,
        };

        let result = policy.detect_drift(&measurement);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        let no_drift_measurement = DriftMeasurement {
            metric_type: MonitoringMetric::InferenceLatency,
            current_value: 105.0,
            baseline_value: 100.0,
            drift_percentage: 5.0,
            timestamp: Utc::now(),
            confidence: 0.95,
        };

        let result_no_drift = policy.detect_drift(&no_drift_measurement);
        assert!(result_no_drift.is_ok());
        assert!(result_no_drift.unwrap().is_none());
    }

    #[test]
    fn test_validate_baseline_data() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);

        // Empty baseline
        assert!(policy.validate_baseline_data(&[]).is_err());

        // Insufficient samples
        let insufficient_data = vec![
            DriftMeasurement {
                metric_type: MonitoringMetric::InferenceLatency,
                current_value: 100.0,
                baseline_value: 100.0,
                drift_percentage: 0.0,
                timestamp: Utc::now(),
                confidence: 0.95,
            };
            50 // Less than minimum
        ];
        assert!(policy.validate_baseline_data(&insufficient_data).is_err());

        // Valid baseline
        let valid_data = vec![
            DriftMeasurement {
                metric_type: MonitoringMetric::InferenceLatency,
                current_value: 100.0,
                baseline_value: 100.0,
                drift_percentage: 0.0,
                timestamp: Utc::now(),
                confidence: 0.95,
            };
            150 // More than minimum
        ];
        assert!(policy.validate_baseline_data(&valid_data).is_ok());
    }

    #[test]
    fn test_validate_drift_measurement() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);

        let valid_measurement = DriftMeasurement {
            metric_type: MonitoringMetric::InferenceLatency,
            current_value: 100.0,
            baseline_value: 100.0,
            drift_percentage: 0.0,
            timestamp: Utc::now(),
            confidence: 0.95,
        };

        assert!(policy
            .validate_drift_measurement(&valid_measurement)
            .is_ok());

        let invalid_measurement = DriftMeasurement {
            metric_type: MonitoringMetric::InferenceLatency,
            current_value: 100.0,
            baseline_value: 100.0,
            drift_percentage: 0.0,
            timestamp: Utc::now(),
            confidence: 0.5, // Too low
        };

        assert!(policy
            .validate_drift_measurement(&invalid_measurement)
            .is_err());
    }

    #[test]
    fn test_update_baseline() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);

        let current_data = vec![
            DriftMeasurement {
                metric_type: MonitoringMetric::InferenceLatency,
                current_value: 100.0,
                baseline_value: 100.0,
                drift_percentage: 0.0,
                timestamp: Utc::now(),
                confidence: 0.95,
            },
            DriftMeasurement {
                metric_type: MonitoringMetric::InferenceLatency,
                current_value: 110.0,
                baseline_value: 100.0,
                drift_percentage: 10.0,
                timestamp: Utc::now(),
                confidence: 0.95,
            },
        ];

        let baseline = policy.update_baseline(&current_data);
        assert!(baseline.is_ok());
        assert_eq!(baseline.unwrap().current_value, 105.0); // Average of 100 and 110
    }

    #[test]
    fn test_should_monitor() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);

        let recent_time = Utc::now() - chrono::Duration::minutes(2);
        assert!(!policy.should_monitor(recent_time));

        let old_time = Utc::now() - chrono::Duration::minutes(10);
        assert!(policy.should_monitor(old_time));
    }

    #[test]
    fn test_should_update_baseline() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);

        let recent_time = Utc::now() - chrono::Duration::hours(12);
        assert!(!policy.should_update_baseline(recent_time));

        let old_time = Utc::now() - chrono::Duration::days(2);
        assert!(policy.should_update_baseline(old_time));
    }

    #[test]
    fn test_generate_drift_report() {
        let config = DriftConfig::default();
        let policy = DriftPolicy::new(config);

        let measurements = vec![DriftMeasurement {
            metric_type: MonitoringMetric::InferenceLatency,
            current_value: 120.0,
            baseline_value: 100.0,
            drift_percentage: 20.0,
            timestamp: Utc::now(),
            confidence: 0.95,
        }];

        let report = policy.generate_drift_report(&measurements);
        assert!(report.contains("Drift Detection Report"));
        assert!(report.contains("InferenceLatency"));
        assert!(report.contains("120.00"));
        assert!(report.contains("100.00"));
        assert!(report.contains("20.00"));
    }
}
