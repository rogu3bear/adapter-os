//! Stub implementations for system metrics functionality
//!
//! This module provides placeholder types and functions for adapteros-system-metrics
//! when that crate is disabled due to SQLx validation issues.
//!
//! TODO: Remove this module once adapteros-system-metrics SQLx issues are resolved

use serde::{Deserialize, Serialize};

/// Stub for SystemMetricsCollector
pub struct SystemMetricsCollector;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

impl SystemMetricsCollector {
    pub fn new() -> Self {
        Self
    }

    pub fn collect_metrics(&mut self) -> SystemMetrics {
        SystemMetrics {
            cpu_usage: 0.0,
            memory_usage: 0.0,
        }
    }

    pub fn load_average(&self) -> LoadAverage {
        LoadAverage {
            one: 0.0,
            five: 0.0,
            fifteen: 0.0,
        }
    }
}

impl Default for SystemMetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

// Stub types for monitoring/alerting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricFilters {
    pub worker_id: Option<String>,
    pub tenant_id: Option<String>,
    pub metric_name: Option<String>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertFilters {
    pub tenant_id: Option<String>,
    pub worker_id: Option<String>,
    pub severity: Option<AlertSeverity>,
    pub status: Option<AlertStatus>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyFilters {
    pub tenant_id: Option<String>,
    pub status: Option<AnomalyStatus>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl AlertSeverity {
    pub fn from_string(s: String) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "critical" => Some(Self::Critical),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertStatus {
    Active,
    Acknowledged,
    Resolved,
}

impl AlertStatus {
    pub fn from_string(s: String) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "active" => Some(Self::Active),
            "acknowledged" => Some(Self::Acknowledged),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalyStatus {
    Detected,
    Investigating,
    Resolved,
    FalsePositive,
}

impl AnomalyStatus {
    pub fn from_string(s: String) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "detected" => Some(Self::Detected),
            "investigating" => Some(Self::Investigating),
            "resolved" => Some(Self::Resolved),
            "false_positive" => Some(Self::FalsePositive),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BaselineType {
    Cpu,
    Memory,
    Latency,
}

impl BaselineType {
    pub fn from_string(s: String) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cpu" => Some(Self::Cpu),
            "memory" => Some(Self::Memory),
            "latency" => Some(Self::Latency),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessHealthMetric {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_unit: String,
    pub tags: serde_json::Value,
    pub collected_at: chrono::DateTime<chrono::Utc>,
}

impl ProcessHealthMetric {
    pub async fn query(
        _pool: &sqlx::SqlitePool,
        _filters: MetricFilters,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // Return empty vec - system-metrics disabled
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMonitoringRule {
    pub id: String,
    pub name: String,
}

impl ProcessMonitoringRule {
    pub async fn list(
        _pool: &sqlx::SqlitePool,
        _tenant_id: Option<String>,
        _rule_type: Option<String>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Ok(Vec::new())
    }

    pub async fn create(
        _pool: &sqlx::SqlitePool,
        _request: CreateMonitoringRuleApiRequest,
    ) -> Result<Self, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    pub async fn update(
        _pool: &sqlx::SqlitePool,
        _id: &str,
        _request: UpdateMonitoringRuleApiRequest,
    ) -> Result<Self, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    pub async fn delete(_pool: &sqlx::SqlitePool, _id: &str) -> Result<(), sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessAlert {
    pub id: String,
    pub severity: AlertSeverity,
    pub status: AlertStatus,
}

impl ProcessAlert {
    pub async fn list(
        _pool: &sqlx::SqlitePool,
        _filters: AlertFilters,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Ok(Vec::new())
    }

    pub async fn update_status(
        _pool: &sqlx::SqlitePool,
        _id: &str,
        _status: AlertStatus,
    ) -> Result<(), sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessAnomaly {
    pub id: String,
    pub status: AnomalyStatus,
}

impl ProcessAnomaly {
    pub async fn list(
        _pool: &sqlx::SqlitePool,
        _filters: AnomalyFilters,
    ) -> Result<Vec<Self>, sqlx::Error> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBaseline {
    pub baseline_type: BaselineType,
    pub mean: f64,
    pub stddev: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub metrics: serde_json::Value,
}

pub struct DashboardService;

impl DashboardService {
    pub fn new(_pool: sqlx::SqlitePool) -> Self {
        Self
    }

    pub async fn get_dashboard_config(
        &self,
        _dashboard_id: &str,
    ) -> Result<DashboardConfig, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }

    pub async fn get_dashboard_data(
        &self,
        _dashboard_id: &str,
    ) -> Result<DashboardData, sqlx::Error> {
        Err(sqlx::Error::RowNotFound)
    }
}

// API request/response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMonitoringRuleApiRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMonitoringRuleApiRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringRuleResponse {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcknowledgeAlertRequest {
    pub alert_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertResponse {
    pub id: String,
    pub severity: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineResponse {
    pub baseline_type: String,
    pub mean: f64,
    pub stddev: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAnomalyStatusRequest {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecalculateBaselineRequest {
    pub baseline_type: String,
}
