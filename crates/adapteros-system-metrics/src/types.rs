//! Type definitions for system metrics
//!
//! Provides serializable types for system metrics that can be used
//! across telemetry, API, and database layers.

use serde::{Deserialize, Serialize};

/// Serializable system metrics for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetricsResponse {
    pub cpu_usage: f64,    // Align with SystemMetrics
    pub memory_usage: f64, // Align with SystemMetrics
    pub active_workers: i32,
    pub requests_per_second: f32,
    pub avg_latency_ms: f32,
    pub disk_usage: f32,
    pub network_bandwidth: f32,
    pub gpu_utilization: f64, // Align with GpuMetrics
    pub uptime_seconds: u64,
    pub process_count: usize,
    pub load_average: LoadAverage,
    pub timestamp: u64,
    pub memory_usage_pct: f32,
    pub adapter_count: i32,
    pub active_sessions: i32,
    pub tokens_per_second: f32,
    pub latency_p95_ms: f32,
    pub cpu_usage_percent: Option<f32>,
    pub memory_usage_percent: Option<f32>,
    pub disk_usage_percent: Option<f32>,
    pub network_rx_bytes: Option<i64>,
    pub network_tx_bytes: Option<i64>,
    pub network_rx_packets: Option<i64>,
    pub network_tx_packets: Option<i64>,
    pub network_bandwidth_mbps: Option<f32>,
    pub gpu_utilization_percent: Option<f32>,
    pub gpu_memory_used_gb: Option<f32>,
    pub gpu_memory_total_gb: Option<f32>,
}

/// System load average
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// Historical system metrics for database storage
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SystemMetricsRecord {
    pub id: Option<i64>,
    pub timestamp: i64,
    pub cpu_usage: f64,    // SQLite REAL → f64 (8-byte IEEE 754)
    pub memory_usage: f64, // SQLite REAL → f64
    pub disk_read_bytes: i64,
    pub disk_write_bytes: i64,
    pub disk_usage_percent: f64,
    pub network_rx_bytes: i64,
    pub network_tx_bytes: i64,
    pub network_rx_packets: i64,
    pub network_tx_packets: i64,
    pub network_bandwidth_mbps: f64,
    pub gpu_utilization: Option<f64>, // SQLite REAL → f64
    pub gpu_memory_used: Option<i64>,
    pub gpu_memory_total: Option<i64>,
    pub uptime_seconds: i64,
    pub process_count: i32,
    pub load_1min: f64,
    pub load_5min: f64,
    pub load_15min: f64,
}

/// Metrics aggregation for time windows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsAggregation {
    pub window_start: u64,
    pub window_end: u64,
    pub avg_cpu_usage: f64,    // Align with database f64
    pub max_cpu_usage: f64,    // Align with database f64
    pub avg_memory_usage: f64, // Align with database f64
    pub max_memory_usage: f64, // Align with database f64
    pub total_disk_read: u64,
    pub total_disk_write: u64,
    pub total_network_rx: u64,
    pub total_network_tx: u64,
    pub sample_count: usize,
}

/// System health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemHealth {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

impl SystemHealth {
    pub fn as_str(&self) -> &'static str {
        match self {
            SystemHealth::Healthy => "healthy",
            SystemHealth::Warning => "warning",
            SystemHealth::Critical => "critical",
            SystemHealth::Unknown => "unknown",
        }
    }
}

/// System health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub status: SystemHealth,
    pub checks: Vec<HealthCheckItem>,
    pub timestamp: u64,
}

/// Individual health check item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckItem {
    pub name: String,
    pub status: SystemHealth,
    pub message: String,
    pub value: Option<f32>,
    pub threshold: Option<f32>,
}

impl HealthCheckItem {
    pub fn new(name: String, status: SystemHealth, message: String) -> Self {
        Self {
            name,
            status,
            message,
            value: None,
            threshold: None,
        }
    }

    pub fn with_threshold(mut self, value: f32, threshold: f32) -> Self {
        self.value = Some(value);
        self.threshold = Some(threshold);
        self
    }
}

/// Metrics export format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
    Prometheus,
}

/// Metrics export request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    pub format: ExportFormat,
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub metrics: Vec<String>,
}

/// Metrics export response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResponse {
    pub format: ExportFormat,
    pub data: String,
    pub record_count: usize,
    pub file_size_bytes: usize,
}

/// System metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub collection_interval_secs: u64,
    pub sampling_rate: f32,
    pub enable_gpu_metrics: bool,
    pub enable_disk_metrics: bool,
    pub enable_network_metrics: bool,
    pub retention_days: u32,
    pub thresholds: ThresholdsConfig,
}

/// Thresholds configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdsConfig {
    pub cpu_warning: f32,
    pub cpu_critical: f32,
    pub memory_warning: f32,
    pub memory_critical: f32,
    pub disk_warning: f32,
    pub disk_critical: f32,
    pub gpu_warning: f32,
    pub gpu_critical: f32,
    pub min_memory_headroom: f32,
}

impl Default for ThresholdsConfig {
    fn default() -> Self {
        Self {
            cpu_warning: 70.0,
            cpu_critical: 90.0,
            memory_warning: 80.0,
            memory_critical: 95.0,
            disk_warning: 85.0,
            disk_critical: 95.0,
            gpu_warning: 80.0,
            gpu_critical: 95.0,
            min_memory_headroom: 15.0,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            collection_interval_secs: 30,
            sampling_rate: 0.05,
            enable_gpu_metrics: true,
            enable_disk_metrics: true,
            enable_network_metrics: true,
            retention_days: 30,
            thresholds: ThresholdsConfig::default(),
        }
    }
}
