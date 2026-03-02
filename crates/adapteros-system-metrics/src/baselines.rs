#![allow(unused_variables)]

//! Baseline calculation service
//!
//! Background service that calculates performance baselines daily/weekly.
//! Stores in process_performance_baselines table and supports historical,
//! statistical, and manual baseline types.

use crate::monitoring_types::*;
use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_telemetry::TelemetryWriter;
use futures_util::FutureExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Baseline calculation service
pub struct BaselineService {
    db: Arc<Db>,
    telemetry_writer: Arc<TelemetryWriter>,
    config: BaselineConfig,
}

#[derive(Debug, Clone)]
pub struct BaselineConfig {
    pub calculation_interval_hours: u64,
    pub historical_window_days: u32,
    pub statistical_window_days: u32,
    pub min_samples_for_calculation: usize,
    pub auto_expire_days: u32,
    pub enable_historical: bool,
    pub enable_statistical: bool,
    pub enable_manual: bool,
    pub percentile_levels: Vec<f64>,
    pub confidence_level: f64,
}

impl Default for BaselineConfig {
    fn default() -> Self {
        Self {
            calculation_interval_hours: 24, // Daily calculation
            historical_window_days: 30,
            statistical_window_days: 7,
            min_samples_for_calculation: 100,
            auto_expire_days: 90,
            enable_historical: true,
            enable_statistical: true,
            enable_manual: true,
            percentile_levels: vec![0.5, 0.75, 0.9, 0.95, 0.99],
            confidence_level: 0.95,
        }
    }
}

/// Calculated baseline
#[derive(Debug, Clone)]
pub struct CalculatedBaseline {
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub baseline_type: BaselineType,
    pub calculation_period_days: i64,
    pub confidence_interval: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub percentile_95: Option<f64>,
    pub percentile_99: Option<f64>,
    pub statistical_measures: StatisticalMeasures,
    pub sample_count: usize,
    pub calculation_timestamp: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Statistical measures
#[derive(Debug, Clone)]
pub struct StatisticalMeasures {
    pub mean: f64,
    pub median: f64,
    pub mode: Option<f64>,
    pub variance: f64,
    pub skewness: f64,
    pub kurtosis: f64,
    pub percentiles: Vec<(f64, f64)>,
    pub min_value: f64,
    pub max_value: f64,
    pub range: f64,
    pub iqr: f64,
}

impl BaselineService {
    /// Create a new baseline service
    pub fn new(
        db: Arc<Db>,
        telemetry_writer: Arc<TelemetryWriter>,
        config: BaselineConfig,
    ) -> Self {
        Self {
            db,
            telemetry_writer,
            config,
        }
    }

    /// Start the baseline calculation service
    pub async fn start(&self) -> Result<()> {
        info!("Starting baseline calculation service");

        let calculation_handle = {
            let service = self.clone();
            tokio::spawn(async move {
                if let Err(panic) = std::panic::AssertUnwindSafe(async move {
                    if let Err(e) = service.calculation_loop().await {
                        error!("Baseline calculation loop failed: {}", e);
                    }
                })
                .catch_unwind()
                .await
                {
                    tracing::error!(
                        task = "baseline_calculation_loop",
                        "background task panicked: {:?}",
                        panic
                    );
                }
            })
        };

        let cleanup_handle = {
            let service = self.clone();
            tokio::spawn(async move {
                if let Err(panic) = std::panic::AssertUnwindSafe(async move {
                    if let Err(e) = service.cleanup_loop().await {
                        error!("Baseline cleanup loop failed: {}", e);
                    }
                })
                .catch_unwind()
                .await
                {
                    tracing::error!(
                        task = "baseline_cleanup_loop",
                        "background task panicked: {:?}",
                        panic
                    );
                }
            })
        };

        // Wait for both tasks to complete (they run indefinitely)
        tokio::select! {
            _ = calculation_handle => {},
            _ = cleanup_handle => {},
        }

        Ok(())
    }

    /// Main calculation loop
    async fn calculation_loop(&self) -> Result<()> {
        let mut interval = interval(std::time::Duration::from_secs(
            self.config.calculation_interval_hours * 3600,
        ));

        loop {
            interval.tick().await;

            if let Err(e) = self.calculate_all_baselines().await {
                error!("Failed to calculate baselines: {}", e);
            }
        }
    }

    /// Cleanup loop for expired baselines
    async fn cleanup_loop(&self) -> Result<()> {
        let mut interval = interval(std::time::Duration::from_secs(3600)); // Hourly cleanup

        loop {
            interval.tick().await;

            if let Err(e) = self.cleanup_expired_baselines().await {
                error!("Failed to cleanup expired baselines: {}", e);
            }
        }
    }

    /// Calculate baselines for all tenants and workers
    async fn calculate_all_baselines(&self) -> Result<()> {
        // Get all active tenants
        let tenants = self.get_active_tenants().await?;

        for tenant in tenants {
            if let Err(e) = self.calculate_tenant_baselines(&tenant.id).await {
                error!(
                    "Failed to calculate baselines for tenant {}: {}",
                    tenant.id, e
                );
            }
        }

        Ok(())
    }

    /// Calculate baselines for a specific tenant
    async fn calculate_tenant_baselines(&self, tenant_id: &str) -> Result<()> {
        // Get active workers for this tenant
        let workers = self.get_active_workers_for_tenant(tenant_id).await?;

        for worker in workers {
            if let Err(e) = self.calculate_worker_baselines(&worker.id, tenant_id).await {
                error!(
                    "Failed to calculate baselines for worker {}: {}",
                    worker.id, e
                );
            }
        }

        Ok(())
    }

    /// Calculate baselines for a specific worker
    async fn calculate_worker_baselines(&self, worker_id: &str, tenant_id: &str) -> Result<()> {
        // Get available metrics for this worker
        let metrics = self.get_worker_metrics(worker_id, tenant_id).await?;

        // Group metrics by name
        let mut metrics_by_name: HashMap<String, Vec<f64>> = HashMap::new();
        for metric in metrics {
            metrics_by_name
                .entry(metric.metric_name)
                .or_default()
                .push(metric.metric_value);
        }

        // Calculate baselines for each metric
        for (metric_name, values) in metrics_by_name {
            if values.len() < self.config.min_samples_for_calculation {
                debug!(
                    "Insufficient samples for baseline calculation: {} (need {})",
                    values.len(),
                    self.config.min_samples_for_calculation
                );
                continue;
            }

            // Calculate different types of baselines
            if self.config.enable_historical {
                if let Err(e) = self
                    .calculate_historical_baseline(worker_id, tenant_id, &metric_name, &values)
                    .await
                {
                    error!("Failed to calculate historical baseline: {}", e);
                }
            }

            if self.config.enable_statistical {
                if let Err(e) = self
                    .calculate_statistical_baseline(worker_id, tenant_id, &metric_name, &values)
                    .await
                {
                    error!("Failed to calculate statistical baseline: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Calculate historical baseline
    async fn calculate_historical_baseline(
        &self,
        worker_id: &str,
        tenant_id: &str,
        metric_name: &str,
        values: &[f64],
    ) -> Result<()> {
        let end_time = chrono::Utc::now();
        let start_time =
            end_time - chrono::Duration::days(self.config.historical_window_days as i64);

        let statistical_measures = self.calculate_statistical_measures(values);
        let baseline_value = statistical_measures.mean; // Use mean as baseline

        let baseline = CalculatedBaseline {
            worker_id: worker_id.to_string(),
            tenant_id: tenant_id.to_string(),
            metric_name: metric_name.to_string(),
            baseline_value,
            baseline_type: BaselineType::Historical,
            calculation_period_days: self.config.historical_window_days as i64,
            confidence_interval: Some(
                self.calculate_confidence_interval(values, statistical_measures.mean),
            ),
            standard_deviation: Some(statistical_measures.variance.sqrt()),
            percentile_95: statistical_measures
                .percentiles
                .iter()
                .find(|(p, _)| (*p - 0.95).abs() < 0.001)
                .map(|(_, v)| *v),
            percentile_99: statistical_measures
                .percentiles
                .iter()
                .find(|(p, _)| (*p - 0.99).abs() < 0.001)
                .map(|(_, v)| *v),
            statistical_measures: statistical_measures.clone(),
            sample_count: values.len(),
            calculation_timestamp: chrono::Utc::now(),
            expires_at: Some(
                chrono::Utc::now() + chrono::Duration::days(self.config.auto_expire_days as i64),
            ),
        };

        self.store_baseline(baseline).await?;

        info!(
            "Historical baseline calculated for worker {} metric {}: {:.2} ({} samples)",
            worker_id,
            metric_name,
            baseline_value,
            values.len()
        );

        Ok(())
    }

    /// Calculate statistical baseline
    async fn calculate_statistical_baseline(
        &self,
        worker_id: &str,
        tenant_id: &str,
        metric_name: &str,
        values: &[f64],
    ) -> Result<()> {
        let end_time = chrono::Utc::now();
        let start_time =
            end_time - chrono::Duration::days(self.config.statistical_window_days as i64);

        let statistical_measures = self.calculate_statistical_measures(values);

        // Use median as baseline for statistical (more robust to outliers)
        let baseline_value = statistical_measures.median;

        let baseline = CalculatedBaseline {
            worker_id: worker_id.to_string(),
            tenant_id: tenant_id.to_string(),
            metric_name: metric_name.to_string(),
            baseline_value,
            baseline_type: BaselineType::Statistical,
            calculation_period_days: self.config.statistical_window_days as i64,
            confidence_interval: Some(
                self.calculate_confidence_interval(values, statistical_measures.mean),
            ),
            standard_deviation: Some(statistical_measures.variance.sqrt()),
            percentile_95: statistical_measures
                .percentiles
                .iter()
                .find(|(p, _)| (*p - 0.95).abs() < 0.001)
                .map(|(_, v)| *v),
            percentile_99: statistical_measures
                .percentiles
                .iter()
                .find(|(p, _)| (*p - 0.99).abs() < 0.001)
                .map(|(_, v)| *v),
            statistical_measures: statistical_measures.clone(),
            sample_count: values.len(),
            calculation_timestamp: chrono::Utc::now(),
            expires_at: Some(
                chrono::Utc::now() + chrono::Duration::days(self.config.auto_expire_days as i64),
            ),
        };

        self.store_baseline(baseline).await?;

        info!(
            "Statistical baseline calculated for worker {} metric {}: {:.2} ({} samples)",
            worker_id,
            metric_name,
            baseline_value,
            values.len()
        );

        Ok(())
    }

    /// Calculate statistical measures from values
    fn calculate_statistical_measures(&self, values: &[f64]) -> StatisticalMeasures {
        let mut sorted_values = values.to_vec();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let mean = self.calculate_mean(values);
        let median = self.calculate_median(&sorted_values);
        let mode = self.calculate_mode(values);
        let variance = self.calculate_variance(values, mean);
        let skewness = self.calculate_skewness(values, mean);
        let kurtosis = self.calculate_kurtosis(values, mean);
        let percentiles = self.calculate_percentiles(&sorted_values);
        let min_value = sorted_values[0];
        let max_value = sorted_values[sorted_values.len() - 1];
        let range = max_value - min_value;
        let p75 = percentiles
            .iter()
            .find(|(p, _)| (*p - 0.75).abs() < 0.001)
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let p25 = percentiles
            .iter()
            .find(|(p, _)| (*p - 0.25).abs() < 0.001)
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let iqr = p75 - p25;

        StatisticalMeasures {
            mean,
            median,
            mode,
            variance,
            skewness,
            kurtosis,
            percentiles,
            min_value,
            max_value,
            range,
            iqr,
        }
    }

    /// Calculate mean
    fn calculate_mean(&self, values: &[f64]) -> f64 {
        if values.is_empty() {
            return 0.0;
        }
        values.iter().sum::<f64>() / values.len() as f64
    }

    /// Calculate median
    fn calculate_median(&self, sorted_values: &[f64]) -> f64 {
        if sorted_values.is_empty() {
            return 0.0;
        }

        let len = sorted_values.len();
        if len.is_multiple_of(2) {
            (sorted_values[len / 2 - 1] + sorted_values[len / 2]) / 2.0
        } else {
            sorted_values[len / 2]
        }
    }

    /// Calculate mode (most frequent value)
    fn calculate_mode(&self, values: &[f64]) -> Option<f64> {
        if values.is_empty() {
            return None;
        }

        let mut frequency_map: HashMap<String, usize> = HashMap::new();

        // Round values to avoid floating point precision issues
        for &value in values {
            let rounded = format!("{:.2}", value);
            *frequency_map.entry(rounded).or_insert(0) += 1;
        }

        frequency_map
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(value, _)| value.parse().unwrap_or(0.0))
    }

    /// Calculate variance
    fn calculate_variance(&self, values: &[f64], mean: f64) -> f64 {
        if values.len() <= 1 {
            return 0.0;
        }

        values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64
    }

    /// Calculate skewness
    fn calculate_skewness(&self, values: &[f64], mean: f64) -> f64 {
        if values.len() < 3 {
            return 0.0;
        }

        let variance = self.calculate_variance(values, mean);
        if variance == 0.0 {
            return 0.0;
        }

        let std_dev = variance.sqrt();
        let n = values.len() as f64;

        let sum_cubed_deviations = values
            .iter()
            .map(|x| ((x - mean) / std_dev).powi(3))
            .sum::<f64>();

        (n / ((n - 1.0) * (n - 2.0))) * sum_cubed_deviations
    }

    /// Calculate kurtosis
    fn calculate_kurtosis(&self, values: &[f64], mean: f64) -> f64 {
        if values.len() < 4 {
            return 0.0;
        }

        let variance = self.calculate_variance(values, mean);
        if variance == 0.0 {
            return 0.0;
        }

        let std_dev = variance.sqrt();
        let n = values.len() as f64;

        let sum_fourth_deviations = values
            .iter()
            .map(|x| ((x - mean) / std_dev).powi(4))
            .sum::<f64>();

        (n * (n + 1.0) / ((n - 1.0) * (n - 2.0) * (n - 3.0))) * sum_fourth_deviations
            - (3.0 * (n - 1.0).powi(2)) / ((n - 2.0) * (n - 3.0))
    }

    /// Calculate percentiles
    fn calculate_percentiles(&self, sorted_values: &[f64]) -> Vec<(f64, f64)> {
        let mut percentiles = Vec::new();

        for &percentile in &self.config.percentile_levels {
            let index = (percentile * (sorted_values.len() - 1) as f64).round() as usize;
            let index = index.min(sorted_values.len() - 1);
            percentiles.push((percentile, sorted_values[index]));
        }

        percentiles
    }

    /// Calculate confidence interval
    fn calculate_confidence_interval(&self, values: &[f64], mean: f64) -> f64 {
        if values.len() <= 1 {
            return 0.0;
        }

        let variance = self.calculate_variance(values, mean);
        let std_dev = variance.sqrt();
        let n = values.len() as f64;

        // Use t-distribution approximation for small samples
        let t_value = if n < 30.0 {
            2.0 // Approximate t-value for 95% confidence
        } else {
            1.96 // Z-value for 95% confidence
        };

        t_value * std_dev / n.sqrt()
    }

    /// Store baseline in database
    async fn store_baseline(&self, baseline: CalculatedBaseline) -> Result<()> {
        let baseline_request = CreateBaselineRequest {
            worker_id: baseline.worker_id.clone(),
            tenant_id: baseline.tenant_id.clone(),
            metric_name: baseline.metric_name.clone(),
            baseline_value: baseline.baseline_value,
            baseline_type: baseline.baseline_type.clone(),
            calculation_period_days: baseline.calculation_period_days,
            confidence_interval: baseline.confidence_interval,
            standard_deviation: baseline.standard_deviation,
            percentile_95: baseline.percentile_95,
            percentile_99: baseline.percentile_99,
            is_active: true,
            expires_at: baseline.expires_at,
        };

        PerformanceBaseline::upsert(self.db.pool_result()?, baseline_request).await?;

        // Log baseline calculation to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "baseline.calculated",
            serde_json::json!({
                "worker_id": baseline.worker_id,
                "tenant_id": baseline.tenant_id,
                "metric_name": baseline.metric_name,
                "baseline_value": baseline.baseline_value,
                "baseline_type": baseline.baseline_type.to_string(),
                "calculation_period_days": baseline.calculation_period_days,
                "sample_count": baseline.sample_count,
                "statistical_measures": {
                    "mean": baseline.statistical_measures.mean,
                    "median": baseline.statistical_measures.median,
                    "std_dev": baseline.statistical_measures.variance.sqrt(),
                    "min": baseline.statistical_measures.min_value,
                    "max": baseline.statistical_measures.max_value,
                    "iqr": baseline.statistical_measures.iqr
                },
                "timestamp": SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }),
        ) {
            warn!("Failed to log baseline calculation to telemetry: {}", e);
        }

        Ok(())
    }

    /// Cleanup expired baselines
    async fn cleanup_expired_baselines(&self) -> Result<()> {
        let cutoff_time = chrono::Utc::now();

        let cutoff_time_str = cutoff_time.to_rfc3339();
        let deleted_count =
            sqlx::query("DELETE FROM process_performance_baselines WHERE expires_at < ?")
                .bind(&cutoff_time_str)
                .execute(self.db.pool_result()?)
                .await
                .map_err(|e| {
                    adapteros_core::AosError::Database(format!(
                        "Failed to cleanup baselines: {}",
                        e
                    ))
                })?
                .rows_affected();

        if deleted_count > 0 {
            info!("Cleaned up {} expired baselines", deleted_count);
        }

        Ok(())
    }

    /// Get worker metrics
    async fn get_worker_metrics(
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
            limit: Some(10000), // Get up to 10k recent metrics
        };

        ProcessHealthMetric::query(self.db.pool_result()?, filters).await
    }

    /// Get active tenants
    async fn get_active_tenants(&self) -> Result<Vec<TenantInfo>> {
        let rows = sqlx::query("SELECT id FROM tenants")
            .fetch_all(self.db.pool_result()?)
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
            .fetch_all(self.db.pool_result()?)
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

    /// Manually recalculate baseline for a specific worker and metric
    pub async fn recalculate_baseline(
        &self,
        worker_id: &str,
        metric_name: &str,
        baseline_type: BaselineType,
    ) -> Result<CalculatedBaseline> {
        let metrics = self.get_worker_metrics(worker_id, "").await?; // Empty tenant_id for now
        let values: Vec<f64> = metrics.iter().map(|m| m.metric_value).collect();

        if values.len() < self.config.min_samples_for_calculation {
            return Err(adapteros_core::AosError::Validation(format!(
                "Insufficient samples: {} (need {})",
                values.len(),
                self.config.min_samples_for_calculation
            )));
        }

        let statistical_measures = self.calculate_statistical_measures(&values);
        let baseline_value = match baseline_type {
            BaselineType::Historical => statistical_measures.mean,
            BaselineType::Statistical => statistical_measures.median,
            BaselineType::Manual => statistical_measures.mean, // Default to mean for manual
        };

        let baseline = CalculatedBaseline {
            worker_id: worker_id.to_string(),
            tenant_id: "".to_string(), // Will be set by caller
            metric_name: metric_name.to_string(),
            baseline_value,
            baseline_type,
            calculation_period_days: self.config.historical_window_days as i64,
            confidence_interval: Some(
                self.calculate_confidence_interval(&values, statistical_measures.mean),
            ),
            standard_deviation: Some(statistical_measures.variance.sqrt()),
            percentile_95: statistical_measures
                .percentiles
                .iter()
                .find(|(p, _)| (*p - 0.95).abs() < 0.001)
                .map(|(_, v)| *v),
            percentile_99: statistical_measures
                .percentiles
                .iter()
                .find(|(p, _)| (*p - 0.99).abs() < 0.001)
                .map(|(_, v)| *v),
            statistical_measures,
            sample_count: values.len(),
            calculation_timestamp: chrono::Utc::now(),
            expires_at: Some(
                chrono::Utc::now() + chrono::Duration::days(self.config.auto_expire_days as i64),
            ),
        };

        self.store_baseline(baseline.clone()).await?;

        Ok(baseline)
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

impl Clone for BaselineService {
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

    #[tokio::test]
    async fn test_baseline_config_defaults() {
        let config = BaselineConfig::default();

        assert_eq!(config.calculation_interval_hours, 24);
        assert_eq!(config.historical_window_days, 30);
        assert_eq!(config.statistical_window_days, 7);
        assert_eq!(config.min_samples_for_calculation, 100);
        assert_eq!(config.auto_expire_days, 90);
        assert!(config.enable_historical);
        assert!(config.enable_statistical);
        assert!(config.enable_manual);
        assert_eq!(config.percentile_levels, vec![0.5, 0.75, 0.9, 0.95, 0.99]);
        assert_eq!(config.confidence_level, 0.95);
    }

    #[tokio::test]
    async fn test_statistical_calculations() {
        let temp_dir = adapteros_core::tempdir_in_var("aos-test-").unwrap();
        let telemetry_writer =
            Arc::new(TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap());
        let service = BaselineService::new(
            Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap()),
            telemetry_writer,
            BaselineConfig::default(),
        );

        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];

        let mean = service.calculate_mean(&values);
        assert_eq!(mean, 5.5);

        let variance = service.calculate_variance(&values, mean);
        assert!((variance - 9.166666666666666).abs() < 1e-10);

        let mut sorted_values = values.clone();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = service.calculate_median(&sorted_values);
        assert_eq!(median, 5.5);

        let confidence_interval = service.calculate_confidence_interval(&values, mean);
        assert!(confidence_interval > 0.0);
    }

    #[tokio::test]
    async fn test_percentile_calculation() {
        let temp_dir = adapteros_core::tempdir_in_var("aos-test-").unwrap();
        let telemetry_writer =
            Arc::new(TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap());
        let service = BaselineService::new(
            Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap()),
            telemetry_writer,
            BaselineConfig::default(),
        );

        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let mut sorted_values = values.clone();
        sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let percentiles = service.calculate_percentiles(&sorted_values);

        assert_eq!(percentiles[0].1, 6.0); // Median (50th percentile) - index 5
        assert_eq!(percentiles[1].1, 8.0); // 75th percentile - index 7
        assert_eq!(percentiles[2].1, 9.0); // 90th percentile - index 8
        assert_eq!(percentiles[3].1, 10.0); // 95th percentile - index 9
        assert_eq!(percentiles[4].1, 10.0); // 99th percentile - index 9
    }

    #[tokio::test]
    async fn test_mode_calculation() {
        let temp_dir = adapteros_core::tempdir_in_var("aos-test-").unwrap();
        let telemetry_writer =
            Arc::new(TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap());
        let service = BaselineService::new(
            Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap()),
            telemetry_writer,
            BaselineConfig::default(),
        );

        let values = vec![1.0, 2.0, 2.0, 3.0, 4.0, 4.0, 4.0, 5.0];
        let mode = service.calculate_mode(&values);
        assert_eq!(mode, Some(4.0));

        let unique_values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let mode = service.calculate_mode(&unique_values);
        assert!(mode.is_some()); // Should return some value when all are equally frequent
        assert!(unique_values.contains(&mode.unwrap())); // Should be one of the input values
    }

    #[tokio::test]
    async fn test_skewness_and_kurtosis() {
        let temp_dir = adapteros_core::tempdir_in_var("aos-test-").unwrap();
        let telemetry_writer =
            Arc::new(TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024).unwrap());
        let service = BaselineService::new(
            Arc::new(adapteros_db::Db::connect(":memory:").await.unwrap()),
            telemetry_writer,
            BaselineConfig::default(),
        );

        // Normal distribution-like data
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        let mean = service.calculate_mean(&values);

        let skewness = service.calculate_skewness(&values, mean);
        assert!(skewness.abs() < 1.0); // Should be close to 0 for symmetric data

        let kurtosis = service.calculate_kurtosis(&values, mean);
        assert!(kurtosis.abs() < 5.0); // Should be reasonable for normal data
    }
}
