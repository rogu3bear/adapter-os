//! Metrics persistence service
//!
//! Background service that collects metrics continuously and stores them in the database.
//! Integrates with existing SystemMonitor and telemetry writer.

use crate::monitoring_types::*;
use adapteros_core::Result;
use adapteros_db::Db;
use adapteros_telemetry::TelemetryWriter;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

/// Metrics persistence service
pub struct MetricsPersistenceService {
    db: Arc<Db>,
    telemetry_writer: Arc<TelemetryWriter>,
    config: PersistenceConfig,
    is_running: Arc<RwLock<bool>>,
    last_cleanup: Arc<RwLock<SystemTime>>,
}

#[derive(Debug, Clone)]
pub struct PersistenceConfig {
    pub collection_interval_secs: u64,
    pub retention_days: u32,
    pub cleanup_interval_hours: u32,
    pub batch_size: usize,
    pub enable_inference_metrics: bool,
    pub enable_gpu_metrics: bool,
    pub enable_performance_metrics: bool,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            collection_interval_secs: 30,
            retention_days: 30,
            cleanup_interval_hours: 24,
            batch_size: 100,
            enable_inference_metrics: true,
            enable_gpu_metrics: true,
            enable_performance_metrics: true,
        }
    }
}

impl MetricsPersistenceService {
    /// Create a new metrics persistence service
    pub fn new(
        db: Arc<Db>,
        telemetry_writer: Arc<TelemetryWriter>,
        config: PersistenceConfig,
    ) -> Self {
        Self {
            db,
            telemetry_writer,
            config,
            is_running: Arc::new(RwLock::new(false)),
            last_cleanup: Arc::new(RwLock::new(SystemTime::now())),
        }
    }

    /// Start the persistence service
    pub async fn start(&self) -> Result<()> {
        {
            let mut is_running = self.is_running.write().await;
            if *is_running {
                warn!("Metrics persistence service is already running");
                return Ok(());
            }
            *is_running = true;
        }

        info!(
            "Starting metrics persistence service with interval: {}s",
            self.config.collection_interval_secs
        );

        let collection_handle = {
            let service = self.clone();
            tokio::spawn(async move {
                if let Err(e) = service.collection_loop().await {
                    error!("Metrics collection loop failed: {}", e);
                }
            })
        };

        let cleanup_handle = {
            let service = self.clone();
            tokio::spawn(async move {
                if let Err(e) = service.cleanup_loop().await {
                    error!("Metrics cleanup loop failed: {}", e);
                }
            })
        };

        // Wait for both tasks to complete (they run indefinitely)
        tokio::select! {
            _ = collection_handle => {},
            _ = cleanup_handle => {},
        }

        Ok(())
    }

    /// Stop the persistence service
    pub async fn stop(&self) -> Result<()> {
        {
            let mut is_running = self.is_running.write().await;
            *is_running = false;
        }
        info!("Stopped metrics persistence service");
        Ok(())
    }

    /// Check if service is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Main collection loop
    async fn collection_loop(&self) -> Result<()> {
        let mut interval = interval(Duration::from_secs(self.config.collection_interval_secs));

        loop {
            interval.tick().await;

            // Check if we should stop
            if !*self.is_running.read().await {
                break;
            }

            if let Err(e) = self.collect_and_store_metrics().await {
                error!("Failed to collect and store metrics: {}", e);

                // Log error to telemetry
                if let Err(telemetry_err) = self.telemetry_writer.log(
                    "metrics.persistence.error",
                    serde_json::json!({
                        "error": e.to_string(),
                        "timestamp": SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs()
                    }),
                ) {
                    error!("Failed to log persistence error: {}", telemetry_err);
                }
            }
        }

        Ok(())
    }

    /// Cleanup loop for old metrics
    async fn cleanup_loop(&self) -> Result<()> {
        let cleanup_interval =
            Duration::from_secs(self.config.cleanup_interval_hours as u64 * 3600);
        let mut interval = interval(cleanup_interval);

        loop {
            interval.tick().await;

            // Check if we should stop
            if !*self.is_running.read().await {
                break;
            }

            if let Err(e) = self.cleanup_old_metrics().await {
                error!("Failed to cleanup old metrics: {}", e);
            }
        }

        Ok(())
    }

    /// Collect and store metrics
    async fn collect_and_store_metrics(&self) -> Result<()> {
        use crate::SystemMetricsCollector;

        // Collect system metrics
        let mut collector = SystemMetricsCollector::new();
        let metrics = collector.collect_metrics();
        let load_avg = collector.load_average();
        let uptime = collector.uptime_seconds();
        let process_count = collector.process_count();

        let timestamp = SystemTime::now();
        let timestamp_secs = timestamp
            .duration_since(UNIX_EPOCH)
            .map_err(|e| adapteros_core::AosError::System(format!("Time error: {}", e)))?
            .as_secs();

        // Get active workers from database
        let workers = self.get_active_workers().await?;

        // Store metrics for each worker
        for worker in &workers {
            let mut worker_metrics = Vec::new();

            // Basic system metrics
            worker_metrics.push(CreateHealthMetricRequest {
                worker_id: worker.id.clone(),
                tenant_id: worker.tenant_id.clone(),
                metric_name: "cpu_usage".to_string(),
                metric_value: metrics.cpu_usage,
                metric_unit: Some("%".to_string()),
                tags: Some(serde_json::json!({
                    "source": "system_collector",
                    "collection_type": "system"
                })),
            });

            worker_metrics.push(CreateHealthMetricRequest {
                worker_id: worker.id.clone(),
                tenant_id: worker.tenant_id.clone(),
                metric_name: "memory_usage".to_string(),
                metric_value: metrics.memory_usage,
                metric_unit: Some("%".to_string()),
                tags: Some(serde_json::json!({
                    "source": "system_collector",
                    "collection_type": "system"
                })),
            });

            // GPU metrics if enabled
            if self.config.enable_gpu_metrics {
                if let Some(gpu_util) = metrics.gpu_metrics.utilization {
                    worker_metrics.push(CreateHealthMetricRequest {
                        worker_id: worker.id.clone(),
                        tenant_id: worker.tenant_id.clone(),
                        metric_name: "gpu_utilization".to_string(),
                        metric_value: gpu_util,
                        metric_unit: Some("%".to_string()),
                        tags: Some(serde_json::json!({
                            "source": "system_collector",
                            "collection_type": "gpu"
                        })),
                    });
                }

                if let Some(gpu_memory) = metrics.gpu_metrics.memory_used {
                    worker_metrics.push(CreateHealthMetricRequest {
                        worker_id: worker.id.clone(),
                        tenant_id: worker.tenant_id.clone(),
                        metric_name: "gpu_memory_used".to_string(),
                        metric_value: gpu_memory as f64,
                        metric_unit: Some("bytes".to_string()),
                        tags: Some(serde_json::json!({
                            "source": "system_collector",
                            "collection_type": "gpu"
                        })),
                    });
                }
            }

            // Performance metrics if enabled
            if self.config.enable_performance_metrics {
                worker_metrics.push(CreateHealthMetricRequest {
                    worker_id: worker.id.clone(),
                    tenant_id: worker.tenant_id.clone(),
                    metric_name: "uptime_seconds".to_string(),
                    metric_value: uptime as f64,
                    metric_unit: Some("seconds".to_string()),
                    tags: Some(serde_json::json!({
                        "source": "system_collector",
                        "collection_type": "performance"
                    })),
                });

                worker_metrics.push(CreateHealthMetricRequest {
                    worker_id: worker.id.clone(),
                    tenant_id: worker.tenant_id.clone(),
                    metric_name: "process_count".to_string(),
                    metric_value: process_count as f64,
                    metric_unit: Some("count".to_string()),
                    tags: Some(serde_json::json!({
                        "source": "system_collector",
                        "collection_type": "performance"
                    })),
                });

                // Load average metrics
                worker_metrics.push(CreateHealthMetricRequest {
                    worker_id: worker.id.clone(),
                    tenant_id: worker.tenant_id.clone(),
                    metric_name: "load_1min".to_string(),
                    metric_value: load_avg.0,
                    metric_unit: Some("load".to_string()),
                    tags: Some(serde_json::json!({
                        "source": "system_collector",
                        "collection_type": "performance"
                    })),
                });

                worker_metrics.push(CreateHealthMetricRequest {
                    worker_id: worker.id.clone(),
                    tenant_id: worker.tenant_id.clone(),
                    metric_name: "load_5min".to_string(),
                    metric_value: load_avg.1,
                    metric_unit: Some("load".to_string()),
                    tags: Some(serde_json::json!({
                        "source": "system_collector",
                        "collection_type": "performance"
                    })),
                });

                worker_metrics.push(CreateHealthMetricRequest {
                    worker_id: worker.id.clone(),
                    tenant_id: worker.tenant_id.clone(),
                    metric_name: "load_15min".to_string(),
                    metric_value: load_avg.2,
                    metric_unit: Some("load".to_string()),
                    tags: Some(serde_json::json!({
                        "source": "system_collector",
                        "collection_type": "performance"
                    })),
                });
            }

            // Inference metrics if enabled
            if self.config.enable_inference_metrics {
                // Get inference metrics from worker (this would need to be implemented)
                // For now, we'll add placeholder metrics
                worker_metrics.push(CreateHealthMetricRequest {
                    worker_id: worker.id.clone(),
                    tenant_id: worker.tenant_id.clone(),
                    metric_name: "inference_latency_p95".to_string(),
                    metric_value: 0.0, // Placeholder - would come from worker
                    metric_unit: Some("ms".to_string()),
                    tags: Some(serde_json::json!({
                        "source": "worker_collector",
                        "collection_type": "inference"
                    })),
                });

                worker_metrics.push(CreateHealthMetricRequest {
                    worker_id: worker.id.clone(),
                    tenant_id: worker.tenant_id.clone(),
                    metric_name: "active_inference_sessions".to_string(),
                    metric_value: 0.0, // Placeholder - would come from worker
                    metric_unit: Some("count".to_string()),
                    tags: Some(serde_json::json!({
                        "source": "worker_collector",
                        "collection_type": "inference"
                    })),
                });
            }

            // Store metrics in batches
            for chunk in worker_metrics.chunks(self.config.batch_size) {
                for metric in chunk {
                    if let Err(e) =
                        ProcessHealthMetric::insert(self.db.pool_result()?, metric.clone()).await
                    {
                        error!("Failed to insert health metric: {}", e);
                        // Continue with other metrics rather than failing completely
                    }
                }
            }

            debug!(
                "Stored {} metrics for worker {}",
                worker_metrics.len(),
                worker.id
            );
        }

        // Log successful collection to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "metrics.collection.success",
            serde_json::json!({
                "worker_count": workers.len(),
                "collection_interval_secs": self.config.collection_interval_secs,
                "timestamp": timestamp_secs
            }),
        ) {
            warn!("Failed to log collection success: {}", e);
        }

        Ok(())
    }

    /// Cleanup old metrics based on retention policy
    async fn cleanup_old_metrics(&self) -> Result<()> {
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| adapteros_core::AosError::System(format!("Time error: {}", e)))?
            .as_secs()
            - (self.config.retention_days as u64 * 24 * 3600);

        let cutoff = chrono::DateTime::from_timestamp(cutoff_time as i64, 0)
            .ok_or_else(|| adapteros_core::AosError::System("Invalid timestamp".to_string()))?
            .with_timezone(&chrono::Utc);
        let cutoff_rfc3339 = cutoff.to_rfc3339();

        // Delete old health metrics
        let deleted_count =
            ProcessHealthMetric::delete_older_than(self.db.pool_result()?, cutoff).await?;

        info!("Cleaned up {} old health metrics", deleted_count);

        // Update last cleanup time
        {
            let mut last_cleanup = self.last_cleanup.write().await;
            *last_cleanup = SystemTime::now();
        }

        // Log cleanup to telemetry
        if let Err(e) = self.telemetry_writer.log(
            "metrics.cleanup.success",
            serde_json::json!({
                "deleted_count": deleted_count,
                "retention_days": self.config.retention_days,
                "cutoff_time": cutoff_rfc3339
            }),
        ) {
            warn!("Failed to log cleanup success: {}", e);
        }

        Ok(())
    }

    /// Get active workers from database
    async fn get_active_workers(&self) -> Result<Vec<WorkerInfo>> {
        let workers = self.db.list_active_workers().await?;

        Ok(workers
            .into_iter()
            .map(|w| WorkerInfo {
                id: w.id,
                tenant_id: w.tenant_id,
            })
            .collect())
    }

    /// Get metrics aggregation for a time window
    pub async fn get_metrics_aggregation(
        &self,
        _worker_id: &str,
        metric_name: &str,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
        aggregation: AggregationType,
    ) -> Result<MetricsAggregation> {
        let window = TimeWindow {
            start: start_time,
            end: end_time,
            aggregation,
        };

        ProcessHealthMetric::aggregate(
            self.db.pool_result()?,
            window,
            metric_name,
            None, // No tenant filter for now
        )
        .await
    }

    /// Get recent metrics for a worker
    pub async fn get_recent_metrics(
        &self,
        worker_id: &str,
        metric_name: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<ProcessHealthMetric>> {
        let filters = MetricFilters {
            worker_id: Some(worker_id.to_string()),
            tenant_id: None,
            metric_name: metric_name.map(|s| s.to_string()),
            start_time: None,
            end_time: None,
            limit,
        };

        ProcessHealthMetric::query(self.db.pool_result()?, filters).await
    }
}

#[derive(Debug, Clone)]
struct WorkerInfo {
    id: String,
    tenant_id: String,
}

impl Clone for MetricsPersistenceService {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            telemetry_writer: self.telemetry_writer.clone(),
            config: self.config.clone(),
            is_running: self.is_running.clone(),
            last_cleanup: self.last_cleanup.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_telemetry::TelemetryWriter;

    #[tokio::test]
    async fn test_persistence_service_creation() {
        let db = Arc::new(
            Db::connect(":memory:")
                .await
                .expect("Failed to create test database"),
        );

        let temp_dir = adapteros_core::tempdir_in_var("aos-test-").unwrap();
        let telemetry_writer = Arc::new(
            TelemetryWriter::new(temp_dir.path(), 1000, 1024 * 1024)
                .expect("Failed to create telemetry writer"),
        );

        let config = PersistenceConfig::default();
        let service = MetricsPersistenceService::new(db, telemetry_writer, config);

        assert!(!service.is_running().await);
    }

    #[tokio::test]
    async fn test_persistence_config_defaults() {
        let config = PersistenceConfig::default();

        assert_eq!(config.collection_interval_secs, 30);
        assert_eq!(config.retention_days, 30);
        assert_eq!(config.cleanup_interval_hours, 24);
        assert_eq!(config.batch_size, 100);
        assert!(config.enable_inference_metrics);
        assert!(config.enable_gpu_metrics);
        assert!(config.enable_performance_metrics);
    }
}
