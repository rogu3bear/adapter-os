//! Metrics Time Series Handler
//!
//! Provides time series metrics including:
//! - CPU, memory, GPU/ANE usage over time
//! - Network I/O metrics
//! - Time range selection

use crate::permissions::{require_permission, Permission};
use crate::{AppState, Claims, ErrorResponse};
use adapteros_api_types::API_SCHEMA_VERSION;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Metrics time series response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MetricsTimeSeriesResponse {
    #[serde(default = "schema_version")]
    pub schema_version: String,
    pub metrics: Vec<MetricDataPoint>,
    pub aggregation: Option<MetricAggregation>,
    pub time_range: TimeRange,
    pub sample_count: usize,
}

/// Metric data point
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MetricDataPoint {
    pub timestamp: i64,
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_read_bytes: i64,
    pub disk_write_bytes: i64,
    pub disk_usage_percent: f64,
    pub network_rx_bytes: i64,
    pub network_tx_bytes: i64,
    pub network_bandwidth_mbps: f64,
    pub gpu_utilization: Option<f64>,
    pub gpu_memory_used: Option<i64>,
    pub uptime_seconds: i64,
    pub process_count: i32,
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// Metric aggregation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct MetricAggregation {
    pub avg_cpu: f64,
    pub max_cpu: f64,
    pub min_cpu: f64,
    pub avg_memory: f64,
    pub max_memory: f64,
    pub min_memory: f64,
    pub total_disk_read: i64,
    pub total_disk_write: i64,
    pub total_network_rx: i64,
    pub total_network_tx: i64,
}

/// Time range
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TimeRange {
    pub start: i64,
    pub end: i64,
    pub duration_seconds: i64,
}

/// Time series query parameters
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TimeSeriesQuery {
    /// Start time (Unix timestamp, seconds)
    pub start: Option<i64>,
    /// End time (Unix timestamp, seconds)
    pub end: Option<i64>,
    /// Duration in seconds (alternative to start/end)
    pub duration: Option<i64>,
    /// Aggregation interval in seconds
    pub interval: Option<i64>,
    /// Include aggregation statistics
    pub aggregate: Option<bool>,
}

fn schema_version() -> String {
    API_SCHEMA_VERSION.to_string()
}

/// Get metrics time series
#[utoipa::path(
    tag = "metrics",
    get,
    path = "/v1/metrics/time-series",
    params(
        ("start" = Option<i64>, Query, description = "Start time (Unix timestamp)"),
        ("end" = Option<i64>, Query, description = "End time (Unix timestamp)"),
        ("duration" = Option<i64>, Query, description = "Duration in seconds"),
        ("interval" = Option<i64>, Query, description = "Aggregation interval"),
        ("aggregate" = Option<bool>, Query, description = "Include aggregation stats")
    ),
    responses(
        (status = 200, description = "Metrics time series", body = MetricsTimeSeriesResponse)
    )
)]
pub async fn get_metrics_time_series(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<TimeSeriesQuery>,
) -> Result<Json<MetricsTimeSeriesResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    require_permission(&claims, Permission::MetricsView)?;

    use chrono::Utc;

    // Calculate time range
    let now = Utc::now().timestamp();
    let (start, end) = if let (Some(start), Some(end)) = (query.start, query.end) {
        (start, end)
    } else if let Some(duration) = query.duration {
        (now - duration, now)
    } else {
        // Default to last hour
        (now - 3600, now)
    };

    let time_range = TimeRange {
        start,
        end,
        duration_seconds: end - start,
    };

    // Fetch metrics from database
    let metrics = sqlx::query_as::<_, MetricDataPoint>(
        "SELECT timestamp, cpu_usage, memory_usage,
                disk_read_bytes, disk_write_bytes, disk_usage_percent,
                network_rx_bytes, network_tx_bytes, network_bandwidth_mbps,
                gpu_utilization, gpu_memory_used,
                uptime_seconds, process_count,
                load_1min, load_5min, load_15min
         FROM system_metrics
         WHERE timestamp >= ? AND timestamp <= ?
         ORDER BY timestamp ASC",
    )
    .bind(start)
    .bind(end)
    .fetch_all(state.db.pool())
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to fetch metrics")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let sample_count = metrics.len();

    // Calculate aggregation if requested
    let aggregation = if query.aggregate.unwrap_or(false) && !metrics.is_empty() {
        Some(calculate_aggregation(&metrics))
    } else {
        None
    };

    Ok(Json(MetricsTimeSeriesResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        metrics,
        aggregation,
        time_range,
        sample_count,
    }))
}

/// Get current metrics snapshot
#[utoipa::path(
    tag = "metrics",
    get,
    path = "/v1/metrics/snapshot",
    responses(
        (status = 200, description = "Current metrics snapshot", body = MetricDataPoint)
    )
)]
pub async fn get_metrics_snapshot(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<MetricDataPoint>, (StatusCode, Json<ErrorResponse>)> {
    // Role check: All roles can view metrics
    require_permission(&claims, Permission::MetricsView)?;

    use adapteros_system_metrics::SystemMetricsCollector;
    use chrono::Utc;

    // Collect current metrics
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    let timestamp = Utc::now().timestamp();

    let data_point = MetricDataPoint {
        timestamp,
        cpu_usage: metrics.cpu_usage,
        memory_usage: metrics.memory_usage,
        disk_read_bytes: metrics.disk_io.read_bytes as i64,
        disk_write_bytes: metrics.disk_io.write_bytes as i64,
        disk_usage_percent: metrics.disk_io.usage_percent as f64,
        network_rx_bytes: metrics.network_io.rx_bytes as i64,
        network_tx_bytes: metrics.network_io.tx_bytes as i64,
        network_bandwidth_mbps: metrics.network_io.bandwidth_mbps as f64,
        gpu_utilization: metrics.gpu_metrics.utilization,
        gpu_memory_used: metrics.gpu_metrics.memory_used.map(|v| v as i64),
        uptime_seconds: collector.uptime_seconds() as i64,
        process_count: collector.process_count() as i32,
        load_1min: load_avg.0,
        load_5min: load_avg.1,
        load_15min: load_avg.2,
    };

    // Store in database for historical tracking
    let _ = sqlx::query(
        "INSERT INTO system_metrics (
            timestamp, cpu_usage, memory_usage,
            disk_read_bytes, disk_write_bytes, disk_usage_percent,
            network_rx_bytes, network_tx_bytes, network_bandwidth_mbps,
            gpu_utilization, gpu_memory_used, gpu_memory_total,
            uptime_seconds, process_count,
            load_1min, load_5min, load_15min
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(timestamp)
    .bind(data_point.cpu_usage)
    .bind(data_point.memory_usage)
    .bind(data_point.disk_read_bytes)
    .bind(data_point.disk_write_bytes)
    .bind(data_point.disk_usage_percent)
    .bind(data_point.network_rx_bytes)
    .bind(data_point.network_tx_bytes)
    .bind(data_point.network_bandwidth_mbps)
    .bind(data_point.gpu_utilization)
    .bind(data_point.gpu_memory_used)
    .bind(metrics.gpu_metrics.memory_total.map(|v| v as i64))
    .bind(data_point.uptime_seconds)
    .bind(data_point.process_count)
    .bind(data_point.load_1min)
    .bind(data_point.load_5min)
    .bind(data_point.load_15min)
    .execute(state.db.pool())
    .await;

    Ok(Json(data_point))
}

/// Calculate aggregation statistics
fn calculate_aggregation(metrics: &[MetricDataPoint]) -> MetricAggregation {
    let mut sum_cpu = 0.0;
    let mut sum_memory = 0.0;
    let mut max_cpu = f64::MIN;
    let mut min_cpu = f64::MAX;
    let mut max_memory = f64::MIN;
    let mut min_memory = f64::MAX;
    let mut total_disk_read = 0i64;
    let mut total_disk_write = 0i64;
    let mut total_network_rx = 0i64;
    let mut total_network_tx = 0i64;

    for metric in metrics {
        sum_cpu += metric.cpu_usage;
        sum_memory += metric.memory_usage;
        max_cpu = max_cpu.max(metric.cpu_usage);
        min_cpu = min_cpu.min(metric.cpu_usage);
        max_memory = max_memory.max(metric.memory_usage);
        min_memory = min_memory.min(metric.memory_usage);
        total_disk_read += metric.disk_read_bytes;
        total_disk_write += metric.disk_write_bytes;
        total_network_rx += metric.network_rx_bytes;
        total_network_tx += metric.network_tx_bytes;
    }

    let count = metrics.len() as f64;

    MetricAggregation {
        avg_cpu: sum_cpu / count,
        max_cpu,
        min_cpu,
        avg_memory: sum_memory / count,
        max_memory,
        min_memory,
        total_disk_read,
        total_disk_write,
        total_network_rx,
        total_network_tx,
    }
}

impl sqlx::FromRow<'_, sqlx::sqlite::SqliteRow> for MetricDataPoint {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;

        Ok(MetricDataPoint {
            timestamp: row.try_get("timestamp")?,
            cpu_usage: row.try_get("cpu_usage")?,
            memory_usage: row.try_get("memory_usage")?,
            disk_read_bytes: row.try_get("disk_read_bytes")?,
            disk_write_bytes: row.try_get("disk_write_bytes")?,
            disk_usage_percent: row.try_get("disk_usage_percent")?,
            network_rx_bytes: row.try_get("network_rx_bytes")?,
            network_tx_bytes: row.try_get("network_tx_bytes")?,
            network_bandwidth_mbps: row.try_get("network_bandwidth_mbps")?,
            gpu_utilization: row.try_get("gpu_utilization").ok(),
            gpu_memory_used: row.try_get("gpu_memory_used").ok(),
            uptime_seconds: row.try_get("uptime_seconds")?,
            process_count: row.try_get("process_count")?,
            load_1min: row.try_get("load_1min")?,
            load_5min: row.try_get("load_5min")?,
            load_15min: row.try_get("load_15min")?,
        })
    }
}
