//! System resource monitoring and metrics collection for AdapterOS
//!
//! This crate provides comprehensive system resource monitoring including:
//! - CPU usage (per-core and aggregated)
//! - Memory usage (RSS, heap, GPU memory)
//! - Disk I/O (read/write rates, latency)
//! - Network I/O (bandwidth, packet statistics)
//! - GPU utilization (Metal/MLX integration)
//! - Policy enforcement integration
//! - Telemetry event generation
//!
//! Follows AdapterOS patterns for telemetry, policy enforcement, and error handling.

#![allow(clippy::collapsible_match)]
#![allow(dead_code)]
#![allow(clippy::too_many_arguments)]

pub mod alerting;
pub mod ane;
pub mod anomaly;
pub mod baselines;
pub mod collector;
pub mod dashboard;
pub mod database;
pub mod gpu;
pub mod monitor;
pub mod monitoring_types;
pub mod notifications;
pub mod persistence;
pub mod policy;
pub mod telemetry;
pub mod types;

pub use alerting::{AlertEvaluator, AlertingConfig};
pub use ane::{AneMemoryStats, AneMetricsCollector};
pub use anomaly::{AnomalyConfig, AnomalyDetector};
pub use baselines::{BaselineConfig, BaselineService};
pub use collector::SystemMetricsCollector;
pub use dashboard::{DashboardConfig, DashboardService};
pub use database::SystemMetricsDb;
pub use monitor::{SystemMonitor, SystemMonitoringService};
pub use notifications::{NotificationConfig, NotificationService};
pub use persistence::MetricsPersistenceService;
pub use policy::SystemMetricsPolicy;
pub use types::{MetricsConfig, ThresholdsConfig};

// Re-export monitoring types from adapteros_db via monitoring_types module
pub use monitoring_types::{
    AcknowledgeAlertRequest, AlertFilters, AlertResponse, AlertSeverity, AlertStatus,
    AnomalyFilters, AnomalyResponse, AnomalyStatus, BaselineResponse, BaselineType,
    CreateMonitoringRuleApiRequest, DashboardData, MetricFilters, PerformanceBaseline,
    ProcessAlert, ProcessAnomaly, ProcessHealthMetric, ProcessMonitoringRule,
};

use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// System metrics collection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage: f64,    // Align with SQLite REAL storage
    pub memory_usage: f64, // Align with SQLite REAL storage
    pub disk_io: DiskMetrics,
    pub network_io: NetworkMetrics,
    pub gpu_metrics: GpuMetrics,
    pub timestamp: SystemTime,
}

/// Disk I/O metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskMetrics {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
    pub usage_percent: f32,
    pub available_bytes: u64,
    pub total_bytes: u64,
}

/// Network I/O metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub bandwidth_mbps: f32,
}

/// GPU metrics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuMetrics {
    pub utilization: Option<f64>, // Align with SQLite REAL storage
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub temperature: Option<f64>, // Align with SQLite REAL storage
    pub power_usage: Option<f64>, // Align with SQLite REAL storage
    pub mlx_memory_used: Option<u64>,
    pub mlx_utilization: Option<f64>, // Align with SQLite REAL storage
}
