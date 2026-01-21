//! System metrics database operations
//!
//! Provides a trait and implementation for storing and retrieving system metrics
//! history and threshold violations. Uses the existing `system_metrics` and
//! `threshold_violations` tables created in migration 0011.
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_db::{Db, SystemMetricsDb};
//!
//! let db = Db::new(...).await?;
//!
//! // Record metrics
//! let metrics = SystemMetrics::new(50.0, 60.0, 45.0, Some(70.0));
//! db.record_metrics(&metrics).await?;
//!
//! // Query history
//! let history = db.get_metrics_history(24, 100).await?;
//!
//! // Check violations
//! let violations = db.get_violations(true).await?;
//! ```

use crate::Db;
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::time::{SystemTime, UNIX_EPOCH};

/// System metrics snapshot for database storage
///
/// Represents a point-in-time capture of system resource utilization.
/// Stored in the `system_metrics` table.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SystemMetrics {
    /// Database row ID (None for new records)
    #[sqlx(default)]
    pub id: Option<i64>,
    /// Unix timestamp when metrics were recorded
    pub timestamp: i64,
    /// CPU utilization percentage (0-100)
    pub cpu_usage: f64,
    /// Memory utilization percentage (0-100)
    pub memory_usage: f64,
    /// Disk read bytes since last sample
    #[sqlx(default)]
    pub disk_read_bytes: i64,
    /// Disk write bytes since last sample
    #[sqlx(default)]
    pub disk_write_bytes: i64,
    /// Network receive bytes since last sample
    #[sqlx(default)]
    pub network_rx_bytes: i64,
    /// Network transmit bytes since last sample
    #[sqlx(default)]
    pub network_tx_bytes: i64,
    /// GPU utilization percentage (0-100), None if no GPU
    #[sqlx(default)]
    pub gpu_utilization: Option<f64>,
    /// GPU memory used in bytes, None if no GPU
    #[sqlx(default)]
    pub gpu_memory_used: Option<i64>,
    /// System uptime in seconds
    #[sqlx(default)]
    pub uptime_seconds: i64,
    /// Number of running processes
    #[sqlx(default)]
    pub process_count: i64,
    /// 1-minute load average
    #[sqlx(default)]
    pub load_1min: f64,
    /// 5-minute load average
    #[sqlx(default)]
    pub load_5min: f64,
    /// 15-minute load average
    #[sqlx(default)]
    pub load_15min: f64,
    /// Disk usage percentage (0-100)
    #[sqlx(default)]
    pub disk_usage_percent: Option<f64>,
    /// Network bandwidth in Mbps
    #[sqlx(default)]
    pub network_bandwidth_mbps: Option<f64>,
    /// Total GPU memory in bytes
    #[sqlx(default)]
    pub gpu_memory_total: Option<i64>,
}

impl SystemMetrics {
    /// Create a new SystemMetrics with basic values
    ///
    /// Sets timestamp to current time and provides defaults for other fields.
    pub fn new(
        cpu_percent: f64,
        memory_percent: f64,
        disk_percent: Option<f64>,
        gpu_percent: Option<f64>,
    ) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        Self {
            id: None,
            timestamp,
            cpu_usage: cpu_percent,
            memory_usage: memory_percent,
            disk_read_bytes: 0,
            disk_write_bytes: 0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            gpu_utilization: gpu_percent,
            gpu_memory_used: None,
            uptime_seconds: 0,
            process_count: 0,
            load_1min: 0.0,
            load_5min: 0.0,
            load_15min: 0.0,
            disk_usage_percent: disk_percent,
            network_bandwidth_mbps: None,
            gpu_memory_total: None,
        }
    }

    /// Create SystemMetrics with full details
    #[allow(clippy::too_many_arguments)]
    pub fn with_full_details(
        timestamp: i64,
        cpu_usage: f64,
        memory_usage: f64,
        disk_read_bytes: i64,
        disk_write_bytes: i64,
        network_rx_bytes: i64,
        network_tx_bytes: i64,
        gpu_utilization: Option<f64>,
        gpu_memory_used: Option<i64>,
        uptime_seconds: i64,
        process_count: i64,
        load_1min: f64,
        load_5min: f64,
        load_15min: f64,
    ) -> Self {
        Self {
            id: None,
            timestamp,
            cpu_usage,
            memory_usage,
            disk_read_bytes,
            disk_write_bytes,
            network_rx_bytes,
            network_tx_bytes,
            gpu_utilization,
            gpu_memory_used,
            uptime_seconds,
            process_count,
            load_1min,
            load_5min,
            load_15min,
            disk_usage_percent: None,
            network_bandwidth_mbps: None,
            gpu_memory_total: None,
        }
    }
}

/// Metrics threshold violation record
///
/// Represents a detected threshold violation stored in `threshold_violations` table.
/// Violations can be resolved by setting `resolved_at`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MetricsViolation {
    /// Database row ID
    pub id: i64,
    /// Unix timestamp when violation was detected
    pub timestamp: i64,
    /// Name of the metric that violated threshold (e.g., "cpu_usage", "memory_usage")
    pub metric_name: String,
    /// Value that exceeded the threshold
    pub current_value: f64,
    /// Threshold value that was exceeded
    pub threshold_value: f64,
    /// Severity level: "warning" or "critical"
    pub severity: String,
    /// Unix timestamp when violation was resolved, None if still active
    #[sqlx(default)]
    pub resolved_at: Option<i64>,
    /// Unix timestamp when record was created
    #[sqlx(default)]
    /// Unix timestamp when record was created
    #[sqlx(default)]
    pub created_at: Option<i64>,
}

/// Metrics aggregation record for caching
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MetricsAggregation {
    pub window_start: u64, // Database INTEGER -> u64
    pub window_end: u64,   // Database INTEGER -> u64
    pub avg_cpu_usage: Option<f64>,
    pub max_cpu_usage: Option<f64>,
    pub avg_memory_usage: Option<f64>,
    pub max_memory_usage: Option<f64>,
    pub total_disk_read: u64,  // Database INTEGER -> u64
    pub total_disk_write: u64, // Database INTEGER -> u64
    pub total_network_rx: u64, // Database INTEGER -> u64
    pub total_network_tx: u64, // Database INTEGER -> u64
    pub sample_count: usize,   // Database INTEGER -> usize
}

impl MetricsViolation {
    /// Check if this violation is still active (not resolved)
    pub fn is_active(&self) -> bool {
        self.resolved_at.is_none()
    }

    /// Check if this is a critical severity violation
    pub fn is_critical(&self) -> bool {
        self.severity == "critical"
    }

    /// Check if this is a warning severity violation
    pub fn is_warning(&self) -> bool {
        self.severity == "warning"
    }
}

/// Trait for system metrics database operations
///
/// Provides methods to record metrics, query history, and manage violations.
/// Implemented for `Db` to enable metrics persistence in the main database.
#[async_trait]
pub trait SystemMetricsDbOps {
    /// Record a system metrics snapshot
    ///
    /// Stores the metrics in `system_metrics` table with current timestamp.
    /// Returns the database row ID of the inserted record.
    async fn record_metrics(&self, metrics: &SystemMetrics) -> Result<i64>;

    /// Get system metrics history
    ///
    /// Returns metrics recorded within the specified time window.
    ///
    /// # Arguments
    /// * `hours` - Number of hours to look back from now
    /// * `limit` - Maximum number of records to return
    ///
    /// # Returns
    /// Vector of metrics ordered by timestamp descending (newest first)
    async fn get_metrics_history(&self, hours: u32, limit: usize) -> Result<Vec<SystemMetrics>>;

    /// Get threshold violations
    ///
    /// Returns violations matching the filter criteria.
    ///
    /// # Arguments
    /// * `unresolved_only` - If true, only return violations where `resolved_at` is NULL
    ///
    /// # Returns
    /// Vector of violations ordered by timestamp descending
    async fn get_violations(&self, unresolved_only: bool) -> Result<Vec<MetricsViolation>>;

    /// Record a threshold violation
    ///
    /// Stores a new violation record in `threshold_violations` table.
    ///
    /// # Arguments
    /// * `metric_name` - Name of the metric (e.g., "cpu_usage")
    /// * `current_value` - Value that exceeded threshold
    /// * `threshold_value` - The threshold that was exceeded
    /// * `severity` - Either "warning" or "critical"
    ///
    /// # Returns
    /// Database row ID of the inserted record
    async fn record_violation(
        &self,
        metric_name: &str,
        current_value: f64,
        threshold_value: f64,
        severity: &str,
    ) -> Result<i64>;

    /// Resolve a threshold violation
    ///
    /// Marks a violation as resolved by setting `resolved_at` to current time.
    ///
    /// # Arguments
    /// * `violation_id` - Database ID of the violation to resolve
    async fn resolve_violation(&self, violation_id: i64) -> Result<()>;

    /// Get the latest metrics snapshot
    ///
    /// Returns the most recent metrics record, or None if no records exist.
    async fn get_latest_metrics(&self) -> Result<Option<SystemMetrics>>;

    /// Delete old metrics data
    ///
    /// Removes metrics older than the specified retention period.
    ///
    /// # Arguments
    /// * `retention_days` - Delete metrics older than this many days
    ///
    /// # Returns
    /// Number of rows deleted
    async fn cleanup_old_metrics(&self, retention_days: u32) -> Result<u64>;

    /// Store metrics aggregation
    async fn store_metrics_aggregation(
        &self,
        aggregation: &MetricsAggregation,
        window_type: &str,
    ) -> Result<()>;

    /// Get metrics aggregation for time window
    async fn get_metrics_aggregation(
        &self,
        window_start: i64,
        window_end: i64,
        window_type: &str,
    ) -> Result<Option<MetricsAggregation>>;

    /// Get configuration value
    async fn get_config(&self, key: &str) -> Result<Option<String>>;

    /// Set configuration value
    async fn set_config(&self, key: &str, value: &str) -> Result<()>;
}

#[async_trait]
impl SystemMetricsDbOps for Db {
    async fn record_metrics(&self, metrics: &SystemMetrics) -> Result<i64> {
        let result = sqlx::query(
            r#"
            INSERT INTO system_metrics (
                timestamp, cpu_usage, memory_usage, disk_read_bytes, disk_write_bytes,
                network_rx_bytes, network_tx_bytes, gpu_utilization, gpu_memory_used,
                uptime_seconds, process_count, load_1min, load_5min, load_15min,
                disk_usage_percent, network_bandwidth_mbps, gpu_memory_total
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
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
        .bind(metrics.disk_usage_percent)
        .bind(metrics.network_bandwidth_mbps)
        .bind(metrics.gpu_memory_total)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to record metrics: {}", e)))?;

        Ok(result.last_insert_rowid())
    }

    async fn get_metrics_history(&self, hours: u32, limit: usize) -> Result<Vec<SystemMetrics>> {
        let start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64
            - (hours as i64 * 3600);

        let limit_i64 = limit as i64;

        let metrics = sqlx::query_as::<_, SystemMetrics>(
            r#"
            SELECT
                id, timestamp, cpu_usage, memory_usage, disk_read_bytes, disk_write_bytes,
                network_rx_bytes, network_tx_bytes, gpu_utilization, gpu_memory_used,
                uptime_seconds, process_count, load_1min, load_5min, load_15min,
                disk_usage_percent, network_bandwidth_mbps, gpu_memory_total
            FROM system_metrics
            WHERE timestamp >= ?
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(start_time)
        .bind(limit_i64)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get metrics history: {}", e)))?;

        Ok(metrics)
    }

    async fn get_violations(&self, unresolved_only: bool) -> Result<Vec<MetricsViolation>> {
        let violations = if unresolved_only {
            sqlx::query_as::<_, MetricsViolation>(
                r#"
                SELECT id, timestamp, metric_name, current_value, threshold_value,
                       severity, resolved_at, created_at
                FROM threshold_violations
                WHERE resolved_at IS NULL
                ORDER BY timestamp DESC
                "#,
            )
            .fetch_all(self.pool())
            .await
        } else {
            sqlx::query_as::<_, MetricsViolation>(
                r#"
                SELECT id, timestamp, metric_name, current_value, threshold_value,
                       severity, resolved_at, created_at
                FROM threshold_violations
                ORDER BY timestamp DESC
                "#,
            )
            .fetch_all(self.pool())
            .await
        }
        .map_err(|e| AosError::Database(format!("Failed to get violations: {}", e)))?;

        Ok(violations)
    }

    async fn record_violation(
        &self,
        metric_name: &str,
        current_value: f64,
        threshold_value: f64,
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
            "#,
        )
        .bind(timestamp)
        .bind(metric_name)
        .bind(current_value)
        .bind(threshold_value)
        .bind(severity)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to record violation: {}", e)))?;

        Ok(result.last_insert_rowid())
    }

    async fn resolve_violation(&self, violation_id: i64) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        sqlx::query("UPDATE threshold_violations SET resolved_at = ? WHERE id = ?")
            .bind(timestamp)
            .bind(violation_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to resolve violation: {}", e)))?;

        Ok(())
    }

    async fn get_latest_metrics(&self) -> Result<Option<SystemMetrics>> {
        let metrics = sqlx::query_as::<_, SystemMetrics>(
            r#"
            SELECT
                id, timestamp, cpu_usage, memory_usage, disk_read_bytes, disk_write_bytes,
                network_rx_bytes, network_tx_bytes, gpu_utilization, gpu_memory_used,
                uptime_seconds, process_count, load_1min, load_5min, load_15min,
                disk_usage_percent, network_bandwidth_mbps, gpu_memory_total
            FROM system_metrics
            ORDER BY timestamp DESC
            LIMIT 1
            "#,
        )
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get latest metrics: {}", e)))?;

        Ok(metrics)
    }

    async fn cleanup_old_metrics(&self, retention_days: u32) -> Result<u64> {
        let cutoff_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64
            - (retention_days as i64 * 24 * 3600);

        let result = sqlx::query("DELETE FROM system_metrics WHERE timestamp < ?")
            .bind(cutoff_time)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to cleanup metrics: {}", e)))?;

        Ok(result.rows_affected())
    }

    async fn store_metrics_aggregation(
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
            "#,
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
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to store aggregation: {}", e)))?;

        Ok(())
    }

    async fn get_metrics_aggregation(
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
            "#,
        )
        .bind(window_start)
        .bind(window_end)
        .bind(window_type)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to get aggregation: {}", e)))?;

        if let Some(row) = row {
            use sqlx::Row;
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

    async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let row =
            sqlx::query("SELECT config_value FROM system_metrics_config WHERE config_key = ?")
                .bind(key)
                .fetch_optional(self.pool())
                .await
                .map_err(|e| AosError::Database(format!("Failed to get config: {}", e)))?;

        Ok(row.map(|r| {
            use sqlx::Row;
            r.get("config_value")
        }))
    }

    async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time before UNIX epoch")
            .as_secs() as i64;

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO system_metrics_config (config_key, config_value, updated_at)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(key)
        .bind(value)
        .bind(timestamp)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to set config: {}", e)))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_metrics_new() {
        let metrics = SystemMetrics::new(50.0, 60.0, Some(45.0), Some(70.0));
        assert_eq!(metrics.cpu_usage, 50.0);
        assert_eq!(metrics.memory_usage, 60.0);
        assert_eq!(metrics.disk_usage_percent, Some(45.0));
        assert_eq!(metrics.gpu_utilization, Some(70.0));
        assert!(metrics.id.is_none());
        assert!(metrics.timestamp > 0);
    }

    #[test]
    fn test_metrics_violation_helpers() {
        let violation = MetricsViolation {
            id: 1,
            timestamp: 1234567890,
            metric_name: "cpu_usage".to_string(),
            current_value: 95.0,
            threshold_value: 90.0,
            severity: "critical".to_string(),
            resolved_at: None,
            created_at: Some(1234567890),
        };

        assert!(violation.is_active());
        assert!(violation.is_critical());
        assert!(!violation.is_warning());

        let warning_violation = MetricsViolation {
            severity: "warning".to_string(),
            resolved_at: Some(1234567900),
            ..violation.clone()
        };

        assert!(!warning_violation.is_active());
        assert!(!warning_violation.is_critical());
        assert!(warning_violation.is_warning());
    }
}
