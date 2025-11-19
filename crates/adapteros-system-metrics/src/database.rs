//! Database integration for system metrics
//!
#![allow(unused_variables)]

//! Provides database operations for storing and retrieving system metrics,
//! health checks, and threshold violations.

use crate::types::*;
use adapteros_core::{AosError, Result};
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};

/// Database operations for system metrics
pub struct SystemMetricsDb {
    pool: SqlitePool,
}

// Migrations are available via embedded macro
#[allow(dead_code)] // TODO: Implement database migrations in future iteration
const MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

impl SystemMetricsDb {
    /// Create a new system metrics database
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Store system metrics record
    pub async fn store_metrics(&self, metrics: &SystemMetricsRecord) -> Result<i64> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        let result = sqlx::query!(
            r#"
            INSERT INTO system_metrics (
                timestamp, cpu_usage, memory_usage, disk_read_bytes, disk_write_bytes,
                network_rx_bytes, network_tx_bytes, gpu_utilization, gpu_memory_used,
                uptime_seconds, process_count, load_1min, load_5min, load_15min
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            metrics.timestamp,
            metrics.cpu_usage,
            metrics.memory_usage,
            metrics.disk_read_bytes,
            metrics.disk_write_bytes,
            metrics.network_rx_bytes,
            metrics.network_tx_bytes,
            metrics.gpu_utilization,
            metrics.gpu_memory_used,
            metrics.uptime_seconds,
            metrics.process_count,
            metrics.load_1min,
            metrics.load_5min,
            metrics.load_15min
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store system metrics: {}", e)))?;

        Ok(result.last_insert_rowid())
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

        let rows = sqlx::query!(
            r#"
            SELECT id, timestamp, cpu_usage, memory_usage, disk_read_bytes, disk_write_bytes,
                   network_rx_bytes, network_tx_bytes, gpu_utilization, gpu_memory_used,
                   uptime_seconds, process_count, load_1min, load_5min, load_15min
            FROM system_metrics
            WHERE timestamp >= ?
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
            start_time,
            limit_i64
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get metrics history: {}", e)))?;

        let mut records = Vec::new();
        for row in rows {
            records.push(SystemMetricsRecord {
                id: row.id,
                timestamp: row.timestamp,
                cpu_usage: row.cpu_usage,
                memory_usage: row.memory_usage,
                disk_read_bytes: row.disk_read_bytes,
                disk_write_bytes: row.disk_write_bytes,
                disk_usage_percent: 0.0, // Not stored in DB yet, TODO: add to schema
                network_rx_bytes: row.network_rx_bytes,
                network_tx_bytes: row.network_tx_bytes,
                network_rx_packets: 0, // Not stored in DB yet, TODO: add to schema
                network_tx_packets: 0, // Not stored in DB yet, TODO: add to schema
                network_bandwidth_mbps: 0.0, // Not stored in DB yet, TODO: add to schema
                gpu_utilization: row.gpu_utilization,
                gpu_memory_used: row.gpu_memory_used,
                gpu_memory_total: None, // Not stored in DB yet, TODO: add to schema
                uptime_seconds: row.uptime_seconds,
                process_count: row.process_count as i32,
                load_1min: row.load_1min,
                load_5min: row.load_5min,
                load_15min: row.load_15min,
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

        let result = sqlx::query!(
            r#"
            INSERT INTO system_health_checks (
                timestamp, status, check_name, check_status, message, value, threshold
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
            timestamp,
            "healthy", // Overall status would be calculated
            check.name,
            check_status_str,
            check.message,
            check.value,
            check.threshold
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store health check: {}", e)))?;

        Ok(result.last_insert_rowid())
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

        let result = sqlx::query!(
            r#"
            INSERT INTO threshold_violations (
                timestamp, metric_name, current_value, threshold_value, severity
            ) VALUES (?, ?, ?, ?, ?)
            "#,
            timestamp,
            metric_name,
            current_value,
            threshold_value,
            severity
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store threshold violation: {}", e)))?;

        Ok(result.last_insert_rowid())
    }

    /// Get unresolved threshold violations
    pub async fn get_unresolved_violations(&self) -> Result<Vec<ThresholdViolationRecord>> {
        let rows = sqlx::query!(
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
                id: row.id,
                timestamp: row.timestamp,
                metric_name: row.metric_name,
                current_value: row.current_value,
                threshold_value: row.threshold_value,
                severity: row.severity,
                resolved_at: None,
                created_at: row.created_at.unwrap_or(row.timestamp),
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

        sqlx::query!(
            "UPDATE threshold_violations SET resolved_at = ? WHERE id = ?",
            timestamp,
            violation_id
        )
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
        let row = sqlx::query!(
            r#"
            SELECT window_start, window_end, avg_cpu_usage, max_cpu_usage, avg_memory_usage,
                   max_memory_usage, total_disk_read, total_disk_write, total_network_rx,
                   total_network_tx, sample_count
            FROM metrics_aggregations
            WHERE window_start = ? AND window_end = ? AND window_type = ?
            "#,
            window_start,
            window_end,
            window_type
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get aggregation: {}", e)))?;

        if let Some(row) = row {
            Ok(Some(MetricsAggregation {
                window_start: row.window_start as u64,
                window_end: row.window_end as u64,
                avg_cpu_usage: row.avg_cpu_usage,
                max_cpu_usage: row.max_cpu_usage,
                avg_memory_usage: row.avg_memory_usage,
                max_memory_usage: row.max_memory_usage,
                total_disk_read: row.total_disk_read as u64,
                total_disk_write: row.total_disk_write as u64,
                total_network_rx: row.total_network_rx as u64,
                total_network_tx: row.total_network_tx as u64,
                sample_count: row.sample_count as usize,
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

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO metrics_aggregations (
                window_start, window_end, window_type, avg_cpu_usage, max_cpu_usage,
                avg_memory_usage, max_memory_usage, total_disk_read, total_disk_write,
                total_network_rx, total_network_tx, sample_count
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            window_start,
            window_end,
            window_type,
            aggregation.avg_cpu_usage,
            aggregation.max_cpu_usage,
            aggregation.avg_memory_usage,
            aggregation.max_memory_usage,
            total_disk_read,
            total_disk_write,
            total_network_rx,
            total_network_tx,
            sample_count
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to store aggregation: {}", e)))?;

        Ok(())
    }

    /// Get configuration value
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query!(
            "SELECT config_value FROM system_metrics_config WHERE config_key = ?",
            key
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get config: {}", e)))?;

        Ok(row.map(|r| r.config_value))
    }

    /// Set configuration value
    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        sqlx::query!(
            r#"
            INSERT OR REPLACE INTO system_metrics_config (config_key, config_value, updated_at)
            VALUES (?, ?, ?)
            "#,
            key,
            value,
            timestamp
        )
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

        let result = sqlx::query!(
            "DELETE FROM system_metrics WHERE timestamp < ?",
            cutoff_time
        )
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
    use sqlx::SqlitePool;

    async fn create_test_pool() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:")
            .await
            .expect("Failed to create test pool");

        // Apply migrations
        sqlx::query(include_str!("../migrations/0001_system_metrics_init.sql"))
            .execute(&pool)
            .await
            .expect("Failed to apply migration 0001");

        sqlx::query(include_str!("../migrations/0002_add_missing_metrics_columns.sql"))
            .execute(&pool)
            .await
            .expect("Failed to apply migration 0002");

        pool
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
            disk_usage_percent: 45.0,
            network_rx_bytes: 3000,
            network_tx_bytes: 4000,
            network_rx_packets: 1500,
            network_tx_packets: 1600,
            network_bandwidth_mbps: 100.0,
            gpu_utilization: Some(70.0),
            gpu_memory_used: Some(5000),
            gpu_memory_total: Some(8000),
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
