#![allow(unused_variables)]

//! Statistical anomaly detection
//!
//! Implements statistical baseline detection using Z-score, IQR, and rate-of-change methods.
//! Stores detected anomalies in database with confidence scores.

use crate::monitoring_types::*;
use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_telemetry::TelemetryWriter;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::interval;
use tracing::{error, info, warn};

/// Anomaly detection engine
pub struct AnomalyDetector {
    db: Arc<Db>,
    telemetry_writer: Arc<TelemetryWriter>,
    config: AnomalyConfig,
}

#[derive(Debug, Clone)]
pub struct AnomalyConfig {
    pub scan_interval_secs: u64,
    pub z_score_threshold: f64,
    pub iqr_multiplier: f64,
    pub rate_of_change_threshold: f64,
    pub min_samples_for_baseline: usize,
    pub baseline_window_days: u32,
    pub confidence_threshold: f64,
    pub enable_zscore: bool,
    pub enable_iqr: bool,
    pub enable_rate_of_change: bool,
}

impl Default for AnomalyConfig {
    fn default() -> Self {
        Self {
            scan_interval_secs: 300, // 5 minutes
            z_score_threshold: 3.0,
            iqr_multiplier: 1.5,
            rate_of_change_threshold: 2.0,
            min_samples_for_baseline: 100,
            baseline_window_days: 7,
            confidence_threshold: 0.7,
            enable_zscore: true,
            enable_iqr: true,
            enable_rate_of_change: true,
        }
    }
}

/// Statistical baseline
#[derive(Debug, Clone)]
pub struct Baseline {
    pub mean: f64,
    pub std_dev: f64,
    pub percentile_25: f64,
    pub percentile_75: f64,
    pub percentile_95: f64,
    pub percentile_99: f64,
    pub min_value: f64,
    pub max_value: f64,
    pub sample_count: usize,
    pub confidence_interval: (f64, f64),
}

/// Detected anomaly
#[derive(Debug, Clone)]
pub struct DetectedAnomaly {
    pub worker_id: String,
    pub tenant_id: String,
    pub anomaly_type: String,
    pub metric_name: String,
    pub detected_value: f64,
    pub expected_range_min: Option<f64>,
    pub expected_range_max: Option<f64>,
    pub confidence_score: f64,
    pub severity: AlertSeverity,
    pub description: String,
    pub detection_method: String,
    pub baseline: Baseline,
}

impl AnomalyDetector {
    /// Create a new anomaly detector
    pub fn new(db: Arc<Db>, telemetry_writer: Arc<TelemetryWriter>, config: AnomalyConfig) -> Self {
        Self {
            db,
            telemetry_writer,
            config,
        }
    }

    /// Start the anomaly detection service
    pub async fn start(&self) -> Result<()> {
        info!("Starting anomaly detection service");

        let scan_handle = {
            let detector = self.clone();
            tokio::spawn(async move {
                if let Err(e) = detector.scan_loop().await {
                    error!("Anomaly detection scan loop failed: {}", e);
                }
            })
        };

        // Wait for scan task to complete (runs indefinitely)
        scan_handle
            .await
            .map_err(|e| adapteros_core::AosError::Internal(format!("Scan task failed: {}", e)))?;

        Ok(())
    }

    /// Main scan loop
    async fn scan_loop(&self) -> Result<()> {
        let mut interval = interval(std::time::Duration::from_secs(
            self.config.scan_interval_secs,
        ));

        loop {
            interval.tick().await;

            if let Err(e) = self.scan_for_anomalies().await {
                error!("Failed to scan for anomalies: {}", e);
            }
        }
    }

    /// Scan for anomalies across all tenants
    async fn scan_for_anomalies(&self) -> Result<()> {
        // Get all active tenants
        let tenants = self.get_active_tenants().await?;

        for tenant in tenants {
            if let Err(e) = self.scan_tenant_anomalies(&tenant.id).await {
                error!("Failed to scan anomalies for tenant {}: {}", tenant.id, e);
            }
        }

        Ok(())
    }

    /// Scan for anomalies in a specific tenant
    async fn scan_tenant_anomalies(&self, tenant_id: &str) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(tenant_id).await?;

        for worker in workers {
            if let Err(e) = self.scan_worker_anomalies(&worker.id, tenant_id).await {
                error!("Failed to scan anomalies for worker {}: {}", worker.id, e);
            }
        }

        Ok(())
    }

    /// Scan for anomalies for a specific worker
    async fn scan_worker_anomalies(&self, worker_id: &str, tenant_id: &str) -> Result<()> {
        // Get recent metrics for this worker
        let recent_metrics = self.get_recent_worker_metrics(worker_id, tenant_id).await?;

        // Group metrics by name
        let mut metrics_by_name: HashMap<String, Vec<f64>> = HashMap::new();
        for metric in recent_metrics {
            metrics_by_name
                .entry(metric.metric_name)
                .or_default()
                .push(metric.metric_value);
        }

        // Check each metric for anomalies
        for (metric_name, values) in metrics_by_name {
            if values.len() < self.config.min_samples_for_baseline {
                continue; // Not enough data for baseline
            }

            // Calculate baseline from historical data
            let baseline = self
                .calculate_baseline(worker_id, &metric_name, self.config.baseline_window_days)
                .await?;

            // Check current value against baseline
            if let Some(current_value) = values.last().copied() {
                let anomalies = self
                    .detect_anomalies(worker_id, tenant_id, &metric_name, current_value, &baseline)
                    .await?;

                // Store detected anomalies
                for anomaly in anomalies {
                    self.store_anomaly(anomaly).await?;
                }
            }
        }

        Ok(())
    }

    /// Calculate baseline from historical data
    async fn calculate_baseline(
        &self,
        worker_id: &str,
        metric_name: &str,
        days: u32,
    ) -> Result<Baseline> {
        let end_time = chrono::Utc::now();
        let start_time = end_time - chrono::Duration::days(days as i64);

        let filters = MetricFilters {
            worker_id: Some(worker_id.to_string()),
            tenant_id: None,
            metric_name: Some(metric_name.to_string()),
            start_time: Some(start_time),
            end_time: Some(end_time),
            limit: None,
        };

        let metrics = ProcessHealthMetric::query(self.db.pool(), filters).await?;
        let values: Vec<f64> = metrics.iter().map(|m| m.metric_value).collect();

        if values.is_empty() {
            return Err(adapteros_core::AosError::Validation(
                "No historical data available".to_string(),
            ));
        }

        // Calculate statistical measures
        let mean = self.calculate_mean(&values);
        let std_dev = self.calculate_std_dev(&values, mean);
        let percentiles = self.calculate_percentiles(&values);
        let min_value = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max_value = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

        // Calculate confidence interval (95%)
        let confidence_margin = 1.96 * std_dev / (values.len() as f64).sqrt();
        let confidence_interval = (mean - confidence_margin, mean + confidence_margin);

        Ok(Baseline {
            mean,
            std_dev,
            percentile_25: percentiles[0],
            percentile_75: percentiles[2],
            percentile_95: percentiles[4],
            percentile_99: percentiles[5],
            min_value,
            max_value,
            sample_count: values.len(),
            confidence_interval,
        })
    }

    /// Detect anomalies using multiple methods
    async fn detect_anomalies(
        &self,
        worker_id: &str,
        tenant_id: &str,
        metric_name: &str,
        current_value: f64,
        baseline: &Baseline,
    ) -> Result<Vec<DetectedAnomaly>> {
        let mut anomalies = Vec::new();

        // Z-score detection
        if self.config.enable_zscore {
            if let Some(anomaly) = self
                .detect_zscore_anomaly(worker_id, tenant_id, metric_name, current_value, baseline)
                .await?
            {
                anomalies.push(anomaly);
            }
        }

        // IQR detection
        if self.config.enable_iqr {
            if let Some(anomaly) = self
                .detect_iqr_anomaly(worker_id, tenant_id, metric_name, current_value, baseline)
                .await?
            {
                anomalies.push(anomaly);
            }
        }

        // Rate of change detection
        if self.config.enable_rate_of_change {
            if let Some(anomaly) = self
                .detect_rate_of_change_anomaly(
                    worker_id,
                    tenant_id,
                    metric_name,
                    current_value,
                    baseline,
                )
                .await?
            {
                anomalies.push(anomaly);
            }
        }

        Ok(anomalies)
    }

    /// Detect anomaly using Z-score method
    async fn detect_zscore_anomaly(
        &self,
        worker_id: &str,
        tenant_id: &str,
        metric_name: &str,
        current_value: f64,
        baseline: &Baseline,
    ) -> Result<Option<DetectedAnomaly>> {
        if baseline.std_dev == 0.0 {
            return Ok(None); // No variation, can't detect anomalies
        }

        let z_score = (current_value - baseline.mean).abs() / baseline.std_dev;

        if z_score > self.config.z_score_threshold {
            let confidence_score = (z_score / self.config.z_score_threshold).min(1.0);
            let severity = if z_score > self.config.z_score_threshold * 2.0 {
                AlertSeverity::Critical
            } else if z_score > self.config.z_score_threshold * 1.5 {
                AlertSeverity::Error
            } else {
                AlertSeverity::Warning
            };

            Ok(Some(DetectedAnomaly {
                worker_id: worker_id.to_string(),
                tenant_id: tenant_id.to_string(),
                anomaly_type: "statistical_outlier".to_string(),
                metric_name: metric_name.to_string(),
                detected_value: current_value,
                expected_range_min: Some(
                    baseline.mean - self.config.z_score_threshold * baseline.std_dev,
                ),
                expected_range_max: Some(
                    baseline.mean + self.config.z_score_threshold * baseline.std_dev,
                ),
                confidence_score,
                severity,
                description: format!(
                    "Z-score anomaly detected: value {} has Z-score {:.2} (threshold: {})",
                    current_value, z_score, self.config.z_score_threshold
                ),
                detection_method: "z_score".to_string(),
                baseline: baseline.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    /// Detect anomaly using IQR method
    async fn detect_iqr_anomaly(
        &self,
        worker_id: &str,
        tenant_id: &str,
        metric_name: &str,
        current_value: f64,
        baseline: &Baseline,
    ) -> Result<Option<DetectedAnomaly>> {
        let iqr = baseline.percentile_75 - baseline.percentile_25;
        let lower_bound = baseline.percentile_25 - self.config.iqr_multiplier * iqr;
        let upper_bound = baseline.percentile_75 + self.config.iqr_multiplier * iqr;

        if current_value < lower_bound || current_value > upper_bound {
            let distance_from_bounds = if current_value < lower_bound {
                lower_bound - current_value
            } else {
                current_value - upper_bound
            };

            let confidence_score = (distance_from_bounds / iqr).min(1.0);
            let severity = if distance_from_bounds > iqr * 2.0 {
                AlertSeverity::Critical
            } else if distance_from_bounds > iqr {
                AlertSeverity::Error
            } else {
                AlertSeverity::Warning
            };

            Ok(Some(DetectedAnomaly {
                worker_id: worker_id.to_string(),
                tenant_id: tenant_id.to_string(),
                anomaly_type: "iqr_outlier".to_string(),
                metric_name: metric_name.to_string(),
                detected_value: current_value,
                expected_range_min: Some(lower_bound),
                expected_range_max: Some(upper_bound),
                confidence_score,
                severity,
                description: format!(
                    "IQR anomaly detected: value {} outside range [{:.2}, {:.2}]",
                    current_value, lower_bound, upper_bound
                ),
                detection_method: "iqr".to_string(),
                baseline: baseline.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    /// Detect anomaly using rate of change method
    async fn detect_rate_of_change_anomaly(
        &self,
        worker_id: &str,
        tenant_id: &str,
        metric_name: &str,
        current_value: f64,
        baseline: &Baseline,
    ) -> Result<Option<DetectedAnomaly>> {
        // Get recent values to calculate rate of change
        let recent_values = self
            .get_recent_metric_values(worker_id, metric_name, 10)
            .await?;

        if recent_values.len() < 2 {
            return Ok(None); // Need at least 2 values to calculate rate of change
        }

        // Calculate rate of change
        let previous_value = recent_values[recent_values.len() - 2];
        let rate_of_change = if previous_value != 0.0 {
            (current_value - previous_value).abs() / previous_value.abs()
        } else {
            0.0
        };

        if rate_of_change > self.config.rate_of_change_threshold {
            let confidence_score = (rate_of_change / self.config.rate_of_change_threshold).min(1.0);
            let severity = if rate_of_change > self.config.rate_of_change_threshold * 3.0 {
                AlertSeverity::Critical
            } else if rate_of_change > self.config.rate_of_change_threshold * 2.0 {
                AlertSeverity::Error
            } else {
                AlertSeverity::Warning
            };

            Ok(Some(DetectedAnomaly {
                worker_id: worker_id.to_string(),
                tenant_id: tenant_id.to_string(),
                anomaly_type: "sudden_change".to_string(),
                metric_name: metric_name.to_string(),
                detected_value: current_value,
                expected_range_min: Some(
                    previous_value * (1.0 - self.config.rate_of_change_threshold),
                ),
                expected_range_max: Some(
                    previous_value * (1.0 + self.config.rate_of_change_threshold),
                ),
                confidence_score,
                severity,
                description: format!(
                    "Rate of change anomaly detected: {:.2}% change from {} to {}",
                    rate_of_change * 100.0,
                    previous_value,
                    current_value
                ),
                detection_method: "rate_of_change".to_string(),
                baseline: baseline.clone(),
            }))
        } else {
            Ok(None)
        }
    }

    /// Store detected anomaly in database
    async fn store_anomaly(&self, anomaly: DetectedAnomaly) -> Result<()> {
        let anomaly_request = CreateAnomalyRequest {
            worker_id: anomaly.worker_id.clone(),
            tenant_id: anomaly.tenant_id.clone(),
            anomaly_type: anomaly.anomaly_type.clone(),
            metric_name: anomaly.metric_name.clone(),
            detected_value: anomaly.detected_value,
            expected_range_min: anomaly.expected_range_min,
            expected_range_max: anomaly.expected_range_max,
            confidence_score: anomaly.confidence_score,
            severity: anomaly.severity.clone(),
            description: Some(anomaly.description.clone()),
            detection_method: anomaly.detection_method.clone(),
            model_version: Some("v1.0".to_string()),
            status: AnomalyStatus::Detected,
        };

        let anomaly_id = ProcessAnomaly::insert(self.db.pool(), anomaly_request).await?;

        // Log anomaly to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "anomaly.detected",
            serde_json::json!({
                "anomaly_id": anomaly_id,
                "worker_id": anomaly.worker_id,
                "tenant_id": anomaly.tenant_id,
                "metric_name": anomaly.metric_name,
                "detected_value": anomaly.detected_value,
                "confidence_score": anomaly.confidence_score,
                "severity": anomaly.severity.to_string(),
                "detection_method": anomaly.detection_method,
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log anomaly to telemetry: {}", e);
        }

        info!(
            "Anomaly detected: {} for worker {} (metric: {} = {}, confidence: {:.2})",
            anomaly.anomaly_type,
            anomaly.worker_id,
            anomaly.metric_name,
            anomaly.detected_value,
            anomaly.confidence_score
        );

        Ok(())
    }

    /// Get recent metrics for a worker
    async fn get_recent_worker_metrics(
        &self,
        worker_id: &str,
        tenant_id: &str,
    ) -> Result<Vec<ProcessHealthMetric>> {
        let filters = MetricFilters {
            worker_id: Some(worker_id.to_string()),
            tenant_id: Some(tenant_id.to_string()),
            metric_name: None,
            start_time: None,
            end_time: None,
            limit: Some(1000), // Get last 1000 metrics
        };

        ProcessHealthMetric::query(self.db.pool(), filters).await
    }

    /// Get recent metric values for rate of change calculation
    async fn get_recent_metric_values(
        &self,
        worker_id: &str,
        metric_name: &str,
        limit: i64,
    ) -> Result<Vec<f64>> {
        let filters = MetricFilters {
            worker_id: Some(worker_id.to_string()),
            tenant_id: None,
            metric_name: Some(metric_name.to_string()),
            start_time: None,
            end_time: None,
            limit: Some(limit),
        };

        let metrics = ProcessHealthMetric::query(self.db.pool(), filters).await?;
        Ok(metrics.iter().map(|m| m.metric_value).collect())
    }

    /// Calculate mean of values
    fn calculate_mean(&self, values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().sum::<f64>() / values.len() as f64
    }

    /// Calculate standard deviation
    fn calculate_std_dev(&self, values: &[f64], mean: f64) -> f64 {
        if values.len() <= 1 {
            return 0.0;
        }

        let variance =
            values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;

        variance.sqrt()
    }

    /// Calculate percentiles
    fn calculate_percentiles(&self, values: &[f64]) -> Vec<f64> {
        let mut sorted_values = values.to_vec();
        sorted_values.sort_by(|a, b| a.total_cmp(b));

        let percentiles = vec![0.25, 0.5, 0.75, 0.9, 0.95, 0.99];
        let mut results = Vec::new();

        for percentile in percentiles {
            let index = (percentile * (sorted_values.len() - 1) as f64).round() as usize;
            let index = index.min(sorted_values.len() - 1);
            results.push(sorted_values[index]);
        }

        results
    }

    /// Get active tenants
    async fn get_active_tenants(&self) -> Result<Vec<TenantInfo>> {
        let rows = sqlx::query("SELECT id FROM tenants")
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!("Failed to get tenants: {}", e))
            })?;

        let tenants = rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                TenantInfo {
                    id: row.get::<Option<String>, _>("id").unwrap_or_default(),
                }
            })
            .collect();

        Ok(tenants)
    }

    /// Get active workers for a tenant
    async fn get_active_workers_for_tenant(&self, tenant_id: &str) -> Result<Vec<WorkerInfo>> {
        let rows = sqlx::query("SELECT id FROM workers WHERE tenant_id = ? AND status = 'active'")
            .bind(tenant_id)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| {
                adapteros_core::AosError::Database(format!("Failed to get workers: {}", e))
            })?;

        let workers = rows
            .into_iter()
            .map(|row| {
                use sqlx::Row;
                WorkerInfo {
                    id: row.get::<Option<String>, _>("id").unwrap_or_default(),
                    tenant_id: tenant_id.to_string(),
                }
            })
            .collect();

        Ok(workers)
    }
}

#[derive(Debug, Clone)]
struct TenantInfo {
    id: String,
}

#[derive(Debug, Clone)]
struct WorkerInfo {
    id: String,
    tenant_id: String,
}

impl Clone for AnomalyDetector {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            telemetry_writer: self.telemetry_writer.clone(),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::TelemetryWriter;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_anomaly_config_defaults() {
        let config = AnomalyConfig::default();

        assert_eq!(config.scan_interval_secs, 300);
        assert_eq!(config.z_score_threshold, 3.0);
        assert_eq!(config.iqr_multiplier, 1.5);
        assert_eq!(config.rate_of_change_threshold, 2.0);
        assert_eq!(config.min_samples_for_baseline, 100);
        assert_eq!(config.baseline_window_days, 7);
        assert_eq!(config.confidence_threshold, 0.7);
        assert!(config.enable_zscore);
        assert!(config.enable_iqr);
        assert!(config.enable_rate_of_change);
    }

    #[tokio::test]
    async fn test_statistical_calculations() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let telemetry_writer =
            Arc::new(TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap());
        let detector = AnomalyDetector::new(
            Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap()),
            telemetry_writer,
            AnomalyConfig::default(),
        );

        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];

        let mean = detector.calculate_mean(&values);
        assert_eq!(mean, 3.0);

        let std_dev = detector.calculate_std_dev(&values, mean);
        assert!((std_dev - 1.5811388300841898).abs() < 1e-10);

        let percentiles = detector.calculate_percentiles(&values);
        assert_eq!(percentiles.len(), 6);
        assert_eq!(percentiles[0], 2.0); // 25th percentile
        assert_eq!(percentiles[1], 3.0); // 50th percentile
        assert_eq!(percentiles[2], 4.0); // 75th percentile
    }

    #[tokio::test]
    async fn test_zscore_detection() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let telemetry_writer =
            Arc::new(TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap());
        let detector = AnomalyDetector::new(
            Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap()),
            telemetry_writer,
            AnomalyConfig::default(),
        );

        let baseline = Baseline {
            mean: 100.0,
            std_dev: 10.0,
            percentile_25: 90.0,
            percentile_75: 110.0,
            percentile_95: 120.0,
            percentile_99: 130.0,
            min_value: 80.0,
            max_value: 140.0,
            sample_count: 1000,
            confidence_interval: (95.0, 105.0),
        };

        // Test normal value (should not be anomaly)
        let result = detector
            .detect_zscore_anomaly("worker-1", "tenant-1", "cpu_usage", 105.0, &baseline)
            .await
            .unwrap();
        assert!(result.is_none());

        // Test anomalous value (should be anomaly)
        let result = detector
            .detect_zscore_anomaly("worker-1", "tenant-1", "cpu_usage", 150.0, &baseline)
            .await
            .unwrap();
        assert!(result.is_some());

        let anomaly = result.unwrap();
        assert_eq!(anomaly.detection_method, "z_score");
        assert_eq!(anomaly.detected_value, 150.0);
        assert!(anomaly.confidence_score > 0.0);
    }

    #[tokio::test]
    async fn test_iqr_detection() {
        let temp_dir = TempDir::new_in(".").unwrap();
        let telemetry_writer =
            Arc::new(TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap());
        let detector = AnomalyDetector::new(
            Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap()),
            telemetry_writer,
            AnomalyConfig::default(),
        );

        let baseline = Baseline {
            mean: 100.0,
            std_dev: 10.0,
            percentile_25: 90.0,
            percentile_75: 110.0,
            percentile_95: 120.0,
            percentile_99: 130.0,
            min_value: 80.0,
            max_value: 140.0,
            sample_count: 1000,
            confidence_interval: (95.0, 105.0),
        };

        // Test normal value (should not be anomaly)
        let result = detector
            .detect_iqr_anomaly("worker-1", "tenant-1", "cpu_usage", 100.0, &baseline)
            .await
            .unwrap();
        assert!(result.is_none());

        // Test anomalous value (should be anomaly)
        let result = detector
            .detect_iqr_anomaly("worker-1", "tenant-1", "cpu_usage", 150.0, &baseline)
            .await
            .unwrap();
        assert!(result.is_some());

        let anomaly = result.unwrap();
        assert_eq!(anomaly.detection_method, "iqr");
        assert_eq!(anomaly.detected_value, 150.0);
        assert!(anomaly.confidence_score > 0.0);
    }
}
