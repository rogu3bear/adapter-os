//! Database integration for system metrics
//!
#![allow(unused_variables)]

//! Provides database operations for storing and retrieving system metrics,
//! health checks, and threshold violations.

use crate::types::*;
use adapteros_core::{AosError, Result};
use sqlx::{AnyPool, Row};
use std::time::{SystemTime, UNIX_EPOCH};

/// Database operations for system metrics
pub struct SystemMetricsDb {
    pool: AnyPool,
}

// Migrations are available via embedded macro
const MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

impl SystemMetricsDb {
    /// Create a new system metrics database
    pub fn new(pool: AnyPool) -> Self {
        Self { pool }
    }

    /// Run database migrations
    pub async fn run_migrations(&self) -> Result<()> {
        MIGRATIONS.run(&self.pool).await
            .map_err(|e| AosError::Database(format!("Failed to run system metrics migrations: {}", e)))?;
        Ok(())
    }

    /// Store system metrics record
    pub async fn store_metrics(&self, metrics: &SystemMetricsRecord) -> Result<i64> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        let result = sqlx::query(
            r#"
            INSERT INTO system_metrics (
                timestamp, cpu_usage, memory_usage, disk_read_bytes, disk_write_bytes,
                network_rx_bytes, network_tx_bytes, gpu_utilization, gpu_memory_used,
                uptime_seconds, process_count, load_1min, load_5min, load_15min
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(metrics.timestamp)
        .bind(metrics.cpu_usage)
        .bind(metrics.memory_usage)
        .bind(metrics.disk_read_bytes)
        .bind(metrics.disk_write_bytes)
        .bind(metrics.network_rx_bytes)
        .bind(metrics.network_tx_bytes)
        .bind(metrics.gpu_utilization)
        .bind(metrics.gpu_memory_used)
        .bind(metrics.uptime_seconds)
        .bind(metrics.process_count)
        .bind(metrics.load_1min)
        .bind(metrics.load_5min)
        .bind(metrics.load_15min)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store system metrics: {}", e)))?;

        Ok(result.last_insert_id().expect("Failed to get last insert ID"))
    }

    /// Get system metrics history
    pub async fn get_metrics_history(
        &self,
        hours: u32,
        limit: Option<usize>,
    ) -> Result<Vec<SystemMetricsRecord>> {
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64
            - (hours as i64 * 3600);

        let limit = limit.unwrap_or(1000);
        let limit_i64 = limit as i64;

        let rows = sqlx::query(
            r#"
            SELECT id, timestamp, cpu_usage, memory_usage, disk_read_bytes, disk_write_bytes,
                   disk_usage_percent, network_rx_bytes, network_tx_bytes, network_rx_packets,
                   network_tx_packets, network_bandwidth_mbps, gpu_utilization, gpu_memory_used,
                   gpu_memory_total, uptime_seconds, process_count, load_1min, load_5min, load_15min
            FROM system_metrics
            WHERE timestamp >= ?
            ORDER BY timestamp DESC
            LIMIT ?
            "#
        )
        .bind(start_time)
        .bind(limit_i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get metrics history: {}", e)))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(SystemMetricsRecord {
                id: row.get("id"),
                timestamp: row.get("timestamp"),
                cpu_usage: row.get("cpu_usage"),
                memory_usage: row.get("memory_usage"),
                disk_read_bytes: row.get("disk_read_bytes"),
                disk_write_bytes: row.get("disk_write_bytes"),
                disk_usage_percent: row.get("disk_usage_percent"),
                network_rx_bytes: row.get("network_rx_bytes"),
                network_tx_bytes: row.get("network_tx_bytes"),
                network_rx_packets: row.get("network_rx_packets"),
                network_tx_packets: row.get("network_tx_packets"),
                network_bandwidth_mbps: row.get("network_bandwidth_mbps"),
                gpu_utilization: row.get("gpu_utilization"),
                gpu_memory_used: row.get("gpu_memory_used"),
                gpu_memory_total: row.get("gpu_memory_total"),
                uptime_seconds: row.get("uptime_seconds"),
                process_count: row.get::<i64, _>("process_count") as i32,
                load_1min: row.get("load_1min"),
                load_5min: row.get("load_5min"),
                load_15min: row.get("load_15min"),
            });
        }

        Ok(records)
    }

    /// Store health check result
    pub async fn store_health_check(&self, check: &HealthCheckItem) -> Result<i64> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        let check_status_str = check.status.as_str();

        let result = sqlx::query(
            r#"
            INSERT INTO system_health_checks (
                timestamp, status, check_name, check_status, message, value, threshold
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(timestamp)
        .bind("healthy") // Overall status would be calculated
        .bind(&check.name)
        .bind(check_status_str)
        .bind(&check.message)
        .bind(check.value)
        .bind(check.threshold)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store health check: {}", e)))?;

        Ok(result.last_insert_id().expect("Failed to get last insert ID"))
    }

    /// Store threshold violation
    pub async fn store_threshold_violation(
        &self,
        metric_name: &str,
        current_value: f32,
        threshold_value: f32,
        severity: &str,
    ) -> Result<i64> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        let result = sqlx::query(
            r#"
            INSERT INTO threshold_violations (
                timestamp, metric_name, current_value, threshold_value, severity
            ) VALUES (?, ?, ?, ?, ?)
            "#
        )
        .bind(timestamp)
        .bind(metric_name)
        .bind(current_value)
        .bind(threshold_value)
        .bind(severity)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store threshold violation: {}", e)))?;

        Ok(result.last_insert_id().expect("Failed to get last insert ID"))
    }

    /// Get unresolved threshold violations
    pub async fn get_unresolved_violations(&self) -> Result<Vec<ThresholdViolationRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, timestamp, metric_name, current_value, threshold_value, severity, created_at
            FROM threshold_violations
            WHERE resolved_at IS NULL
            ORDER BY timestamp DESC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get violations: {}", e)))?;

        let mut violations = Vec::new();
        for row in rows {
            violations.push(ThresholdViolationRecord {
                id: row.get("id"),
                timestamp: row.get("timestamp"),
                metric_name: row.get("metric_name"),
                current_value: row.get("current_value"),
                threshold_value: row.get("threshold_value"),
                severity: row.get("severity"),
                resolved_at: None,
                created_at: row.get::<Option<i64>, _>("created_at").unwrap_or(row.get("timestamp")),
            });
        }

        Ok(violations)
    }

    /// Resolve threshold violation
    pub async fn resolve_violation(&self, violation_id: i64) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        sqlx::query(
            "UPDATE threshold_violations SET resolved_at = ? WHERE id = ?"
        )
        .bind(timestamp)
        .bind(violation_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to resolve violation: {}", e)))?;

        Ok(())
    }

    /// Get metrics aggregation for time window
    pub async fn get_metrics_aggregation(
        &self,
        window_start: i64,
        window_end: i64,
        window_type: &str,
    ) -> Result<Option<MetricsAggregation>> {
        let row = sqlx::query(
            r#"
            SELECT window_start, window_end, avg_cpu_usage, max_cpu_usage, avg_memory_usage,
                   max_memory_usage, total_disk_read, total_disk_write, total_network_rx,
                   total_network_tx, sample_count
            FROM metrics_aggregations
            WHERE window_start = ? AND window_end = ? AND window_type = ?
            "#
        )
        .bind(window_start)
        .bind(window_end)
        .bind(window_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get aggregation: {}", e)))?;

        if let Some(row) = row {
            Ok(Some(MetricsAggregation {
                window_start: row.get::<i64, _>("window_start") as u64,
                window_end: row.get::<i64, _>("window_end") as u64,
                avg_cpu_usage: row.get("avg_cpu_usage"),
                max_cpu_usage: row.get("max_cpu_usage"),
                avg_memory_usage: row.get("avg_memory_usage"),
                max_memory_usage: row.get("max_memory_usage"),
                total_disk_read: row.get::<i64, _>("total_disk_read") as u64,
                total_disk_write: row.get::<i64, _>("total_disk_write") as u64,
                total_network_rx: row.get::<i64, _>("total_network_rx") as u64,
                total_network_tx: row.get::<i64, _>("total_network_tx") as u64,
                sample_count: row.get::<i64, _>("sample_count") as usize,
            }))
        } else {
            Ok(None)
        }
    }

    /// Store metrics aggregation
    pub async fn store_metrics_aggregation(
        &self,
        aggregation: &MetricsAggregation,
        window_type: &str,
    ) -> Result<()> {
        let window_start = aggregation.window_start as i64;
        let window_end = aggregation.window_end as i64;
        let total_disk_read = aggregation.total_disk_read as i64;
        let total_disk_write = aggregation.total_disk_write as i64;
        let total_network_rx = aggregation.total_network_rx as i64;
        let total_network_tx = aggregation.total_network_tx as i64;
        let sample_count = aggregation.sample_count as i64;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO metrics_aggregations (
                window_start, window_end, window_type, avg_cpu_usage, max_cpu_usage,
                avg_memory_usage, max_memory_usage, total_disk_read, total_disk_write,
                total_network_rx, total_network_tx, sample_count
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(window_start)
        .bind(window_end)
        .bind(window_type)
        .bind(aggregation.avg_cpu_usage)
        .bind(aggregation.max_cpu_usage)
        .bind(aggregation.avg_memory_usage)
        .bind(aggregation.max_memory_usage)
        .bind(total_disk_read)
        .bind(total_disk_write)
        .bind(total_network_rx)
        .bind(total_network_tx)
        .bind(sample_count)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store aggregation: {}", e)))?;

        Ok(())
    }

    /// Get configuration value
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query(
            "SELECT config_value FROM system_metrics_config WHERE config_key = ?"
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get config: {}", e)))?;

        Ok(row.map(|r| r.get("config_value")))
    }

    /// Set configuration value
    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO system_metrics_config (config_key, config_value, updated_at)
            VALUES (?, ?, ?)
            "#
        )
        .bind(key)
        .bind(value)
        .bind(timestamp)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to set config: {}", e)))?;

        Ok(())
    }

    /// Clean up old metrics data
    pub async fn cleanup_old_metrics(&self, retention_days: u32) -> Result<u64> {
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64
            - (retention_days as i64 * 24 * 3600);

        let result = sqlx::query(
            "DELETE FROM system_metrics WHERE timestamp < ?"
        )
        .bind(cutoff_time)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to cleanup metrics: {}", e)))?;

        Ok(result.rows_affected())
    }
}

/// Threshold violation record for database
#[derive(Debug, Clone)]
pub struct ThresholdViolationRecord {
    pub id: Option<i64>,
    pub timestamp: i64,
    pub metric_name: String,
    pub current_value: f64,   // Align with database f64
    pub threshold_value: f64, // Align with database f64
    pub severity: String,
    pub resolved_at: Option<i64>,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::AnyPool;

    async fn create_test_pool() -> AnyPool {
        AnyPool::connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool")
    }

    #[tokio::test]
    async fn test_metrics_storage() {
        let pool = create_test_pool().await;
        let db = SystemMetricsDb::new(pool);

        let metrics = SystemMetricsRecord {
            id: None,
            timestamp: 1234567890,
            cpu_usage: 50.0,
            memory_usage: 60.0,
            disk_read_bytes: 1000,
            disk_write_bytes: 2000,
            disk_usage_percent: 75.0,
            network_rx_bytes: 3000,
            network_tx_bytes: 4000,
            network_rx_packets: 150,
            network_tx_packets: 200,
            network_bandwidth_mbps: 10.5,
            gpu_utilization: Some(70.0),
            gpu_memory_used: Some(5000),
            gpu_memory_total: Some(8192),
            uptime_seconds: 3600,
            process_count: 100,
            load_1min: 1.5,
            load_5min: 1.2,
            load_15min: 1.0,
        };

        let id = db
            .store_metrics(&metrics)
            .await
            .expect("Failed to store metrics");
        assert!(id > 0);
    }

    #[tokio::test]
    async fn test_config_operations() {
        let pool = create_test_pool().await;
        let db = SystemMetricsDb::new(pool);

        db.set_config("test_key", "test_value")
            .await
            .expect("Failed to set config");
        let value = db
            .get_config("test_key")
            .await
            .expect("Failed to get config");
        assert_eq!(value, Some("test_value".to_string()));
    }
}
