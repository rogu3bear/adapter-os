//! Monitoring type definitions
//!
//! Provides Rust structs matching database schemas for process monitoring.
//! Includes serialization, validation, and API response types.

use serde::{Deserialize, Serialize};

// Re-export types from database module
pub use adapteros_db::process_monitoring::{
    AggregationType, AlertFilters, AlertSeverity, AlertStatus, AnomalyFilters, AnomalyStatus,
    BaselineType, CreateAlertRequest, CreateAnomalyRequest, CreateBaselineRequest,
    CreateHealthMetricRequest, CreateMonitoringRuleRequest, MetricFilters, MetricsAggregation,
    MonitoringDashboard, MonitoringNotification, MonitoringWidget, NotificationStatus,
    NotificationType, PerformanceBaseline, ProcessAlert, ProcessAnomaly, ProcessHealthMetric,
    ProcessMonitoringRule, RuleType, ThresholdOperator, TimeWindow, UpdateMonitoringRuleRequest,
};

// ===== API Response Types =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringRuleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub rule_type: String,
    pub metric_name: String,
    pub threshold_value: f64,
    pub threshold_operator: String,
    pub severity: String,
    pub evaluation_window_seconds: i64,
    pub cooldown_seconds: i64,
    pub is_active: bool,
    pub notification_channels: Option<serde_json::Value>,
    pub escalation_rules: Option<serde_json::Value>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ProcessMonitoringRule> for MonitoringRuleResponse {
    fn from(rule: ProcessMonitoringRule) -> Self {
        Self {
            id: rule.id,
            name: rule.name,
            description: rule.description,
            tenant_id: rule.tenant_id,
            rule_type: rule.rule_type.to_string(),
            metric_name: rule.metric_name,
            threshold_value: rule.threshold_value,
            threshold_operator: rule.threshold_operator.to_string(),
            severity: rule.severity.to_string(),
            evaluation_window_seconds: rule.evaluation_window_seconds,
            cooldown_seconds: rule.cooldown_seconds,
            is_active: rule.is_active,
            notification_channels: rule.notification_channels,
            escalation_rules: rule.escalation_rules,
            created_by: rule.created_by,
            created_at: rule.created_at.to_rfc3339(),
            updated_at: rule.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetricResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_unit: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub collected_at: String,
}

impl From<ProcessHealthMetric> for HealthMetricResponse {
    fn from(metric: ProcessHealthMetric) -> Self {
        Self {
            id: metric.id,
            worker_id: metric.worker_id,
            tenant_id: metric.tenant_id,
            metric_name: metric.metric_name,
            metric_value: metric.metric_value,
            metric_unit: metric.metric_unit,
            tags: metric.tags,
            collected_at: metric.collected_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertResponse {
    pub id: String,
    pub rule_id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub alert_type: String,
    pub severity: String,
    pub title: String,
    pub message: String,
    pub metric_value: Option<f64>,
    pub threshold_value: Option<f64>,
    pub status: String,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<String>,
    pub resolved_at: Option<String>,
    pub suppression_reason: Option<String>,
    pub suppression_until: Option<String>,
    pub escalation_level: i64,
    pub notification_sent: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ProcessAlert> for AlertResponse {
    fn from(alert: ProcessAlert) -> Self {
        Self {
            id: alert.id,
            rule_id: alert.rule_id,
            worker_id: alert.worker_id,
            tenant_id: alert.tenant_id,
            alert_type: alert.alert_type,
            severity: alert.severity.to_string(),
            title: alert.title,
            message: alert.message,
            metric_value: alert.metric_value,
            threshold_value: alert.threshold_value,
            status: alert.status.to_string(),
            acknowledged_by: alert.acknowledged_by,
            acknowledged_at: alert.acknowledged_at.map(|dt| dt.to_rfc3339()),
            resolved_at: alert.resolved_at.map(|dt| dt.to_rfc3339()),
            suppression_reason: alert.suppression_reason,
            suppression_until: alert.suppression_until.map(|dt| dt.to_rfc3339()),
            escalation_level: alert.escalation_level,
            notification_sent: alert.notification_sent,
            created_at: alert.created_at.to_rfc3339(),
            updated_at: alert.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub anomaly_type: String,
    pub metric_name: String,
    pub detected_value: f64,
    pub expected_range_min: Option<f64>,
    pub expected_range_max: Option<f64>,
    pub confidence_score: f64,
    pub severity: String,
    pub description: Option<String>,
    pub detection_method: String,
    pub model_version: Option<String>,
    pub status: String,
    pub investigated_by: Option<String>,
    pub investigation_notes: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

impl From<ProcessAnomaly> for AnomalyResponse {
    fn from(anomaly: ProcessAnomaly) -> Self {
        Self {
            id: anomaly.id,
            worker_id: anomaly.worker_id,
            tenant_id: anomaly.tenant_id,
            anomaly_type: anomaly.anomaly_type,
            metric_name: anomaly.metric_name,
            detected_value: anomaly.detected_value,
            expected_range_min: anomaly.expected_range_min,
            expected_range_max: anomaly.expected_range_max,
            confidence_score: anomaly.confidence_score,
            severity: anomaly.severity.to_string(),
            description: anomaly.description,
            detection_method: anomaly.detection_method,
            model_version: anomaly.model_version,
            status: anomaly.status.to_string(),
            investigated_by: anomaly.investigated_by,
            investigation_notes: anomaly.investigation_notes,
            resolved_at: anomaly.resolved_at.map(|dt| dt.to_rfc3339()),
            created_at: anomaly.created_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub baseline_type: String,
    pub calculation_period_days: i64,
    pub confidence_interval: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub percentile_95: Option<f64>,
    pub percentile_99: Option<f64>,
    pub is_active: bool,
    pub calculated_at: String,
    pub expires_at: Option<String>,
}

impl From<PerformanceBaseline> for BaselineResponse {
    fn from(baseline: PerformanceBaseline) -> Self {
        Self {
            id: baseline.id,
            worker_id: baseline.worker_id,
            tenant_id: baseline.tenant_id,
            metric_name: baseline.metric_name,
            baseline_value: baseline.baseline_value,
            baseline_type: baseline.baseline_type.to_string(),
            calculation_period_days: baseline.calculation_period_days,
            confidence_interval: baseline.confidence_interval,
            standard_deviation: baseline.standard_deviation,
            percentile_95: baseline.percentile_95,
            percentile_99: baseline.percentile_99,
            is_active: baseline.is_active,
            calculated_at: baseline.calculated_at.to_rfc3339(),
            expires_at: baseline.expires_at.map(|dt| dt.to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub dashboard_config: serde_json::Value,
    pub is_shared: bool,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<MonitoringDashboard> for DashboardResponse {
    fn from(dashboard: MonitoringDashboard) -> Self {
        Self {
            id: dashboard.id,
            name: dashboard.name,
            description: dashboard.description,
            tenant_id: dashboard.tenant_id,
            dashboard_config: dashboard.dashboard_config,
            is_shared: dashboard.is_shared,
            created_by: dashboard.created_by,
            created_at: dashboard.created_at.to_rfc3339(),
            updated_at: dashboard.updated_at.to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMonitoringReportResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub report_type: String,
    pub report_config: serde_json::Value,
    pub generated_at: String,
    pub report_data: Option<serde_json::Value>,
    pub file_path: Option<String>,
    pub file_size_bytes: Option<i64>,
    pub created_by: Option<String>,
}

// ===== Request Types =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMonitoringRuleApiRequest {
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub rule_type: String,
    pub metric_name: String,
    pub threshold_value: f64,
    pub threshold_operator: String,
    pub severity: String,
    pub evaluation_window_seconds: Option<i64>,
    pub cooldown_seconds: Option<i64>,
    pub is_active: Option<bool>,
    pub notification_channels: Option<serde_json::Value>,
    pub escalation_rules: Option<serde_json::Value>,
}

impl TryFrom<CreateMonitoringRuleApiRequest> for CreateMonitoringRuleRequest {
    type Error = String;

    fn try_from(req: CreateMonitoringRuleApiRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            name: req.name,
            description: req.description,
            tenant_id: req.tenant_id,
            rule_type: RuleType::from_string(req.rule_type),
            metric_name: req.metric_name,
            threshold_value: req.threshold_value,
            threshold_operator: ThresholdOperator::from_string(req.threshold_operator),
            severity: AlertSeverity::from_string(req.severity),
            evaluation_window_seconds: req.evaluation_window_seconds.unwrap_or(300),
            cooldown_seconds: req.cooldown_seconds.unwrap_or(60),
            is_active: req.is_active.unwrap_or(true),
            notification_channels: req.notification_channels,
            escalation_rules: req.escalation_rules,
            created_by: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMonitoringRuleApiRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub threshold_value: Option<f64>,
    pub is_active: Option<bool>,
}

impl From<UpdateMonitoringRuleApiRequest> for UpdateMonitoringRuleRequest {
    fn from(req: UpdateMonitoringRuleApiRequest) -> Self {
        Self {
            name: req.name,
            description: req.description,
            threshold_value: req.threshold_value,
            is_active: req.is_active,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDashboardApiRequest {
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub dashboard_config: serde_json::Value,
    pub is_shared: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDashboardApiRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub dashboard_config: Option<serde_json::Value>,
    pub is_shared: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProcessMonitoringReportRequest {
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub report_type: String,
    pub report_config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcknowledgeAlertRequest {
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuppressAlertRequest {
    pub reason: String,
    pub until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAnomalyStatusRequest {
    pub status: String,
    pub investigation_notes: Option<String>,
    pub investigated_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecalculateBaselineRequest {
    pub worker_id: String,
    pub metric_name: String,
    pub calculation_period_days: Option<i64>,
}

// ===== Dashboard Widget Types =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardWidget {
    pub id: String,
    pub widget_type: WidgetType,
    pub config: serde_json::Value,
    pub position: WidgetPosition,
    pub size: WidgetSize,
    pub refresh_interval_seconds: Option<i64>,
    pub is_visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WidgetType {
    TimeSeries,
    Gauge,
    AlertList,
    AnomalyHeatmap,
    MetricCard,
    StatusIndicator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetPosition {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetSize {
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesWidgetConfig {
    pub metric: String,
    pub aggregation: String,
    pub window: String,
    pub workers: Option<Vec<String>>,
    pub threshold_warning: Option<f64>,
    pub threshold_critical: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaugeWidgetConfig {
    pub metric: String,
    pub threshold_warning: f64,
    pub threshold_critical: f64,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertListWidgetConfig {
    pub severities: Vec<String>,
    pub limit: i64,
    pub workers: Option<Vec<String>>,
    pub show_acknowledged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyHeatmapWidgetConfig {
    pub workers: Vec<String>,
    pub metric: String,
    pub time_window: String,
    pub detection_methods: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricCardWidgetConfig {
    pub metric: String,
    pub aggregation: String,
    pub window: String,
    pub format: Option<String>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusIndicatorWidgetConfig {
    pub metric: String,
    pub healthy_threshold: f64,
    pub warning_threshold: f64,
    pub critical_threshold: f64,
    pub operator: String,
}

// ===== Dashboard Data Types =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub dashboard_id: String,
    pub widgets: Vec<WidgetData>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetData {
    pub widget_id: String,
    pub widget_type: String,
    pub data: serde_json::Value,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesData {
    pub metric: String,
    pub points: Vec<TimeSeriesPoint>,
    pub aggregation: String,
    pub window: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    pub timestamp: String,
    pub value: f64,
    pub worker_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaugeData {
    pub metric: String,
    pub current_value: f64,
    pub threshold_warning: f64,
    pub threshold_critical: f64,
    pub status: String,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertListData {
    pub alerts: Vec<AlertSummary>,
    pub total_count: i64,
    pub unacknowledged_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSummary {
    pub id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub worker_id: String,
    pub created_at: String,
    pub acknowledged_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyHeatmapData {
    pub workers: Vec<WorkerAnomalyData>,
    pub metric: String,
    pub time_window: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerAnomalyData {
    pub worker_id: String,
    pub anomaly_count: i64,
    pub latest_anomaly: Option<AnomalySummary>,
    pub confidence_scores: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalySummary {
    pub id: String,
    pub anomaly_type: String,
    pub confidence_score: f64,
    pub severity: String,
    pub detected_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricCardData {
    pub metric: String,
    pub value: f64,
    pub aggregation: String,
    pub window: String,
    pub trend: Option<TrendData>,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendData {
    pub direction: String, // "up", "down", "stable"
    pub percentage_change: f64,
    pub period: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusIndicatorData {
    pub metric: String,
    pub status: String, // "healthy", "warning", "critical"
    pub current_value: f64,
    pub thresholds: ThresholdData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdData {
    pub healthy: f64,
    pub warning: f64,
    pub critical: f64,
}

// ===== Validation Helpers =====

impl WidgetType {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "time_series" => WidgetType::TimeSeries,
            "gauge" => WidgetType::Gauge,
            "alert_list" => WidgetType::AlertList,
            "anomaly_heatmap" => WidgetType::AnomalyHeatmap,
            "metric_card" => WidgetType::MetricCard,
            "status_indicator" => WidgetType::StatusIndicator,
            _ => WidgetType::MetricCard,
        }
    }
}

impl std::fmt::Display for WidgetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WidgetType::TimeSeries => write!(f, "time_series"),
            WidgetType::Gauge => write!(f, "gauge"),
            WidgetType::AlertList => write!(f, "alert_list"),
            WidgetType::AnomalyHeatmap => write!(f, "anomaly_heatmap"),
            WidgetType::MetricCard => write!(f, "metric_card"),
            WidgetType::StatusIndicator => write!(f, "status_indicator"),
        }
    }
}

// ===== Conversion Helpers =====

impl From<MonitoringWidget> for DashboardWidget {
    fn from(widget: MonitoringWidget) -> Self {
        Self {
            id: widget.id,
            widget_type: WidgetType::from_string(widget.widget_type),
            config: widget.widget_config,
            position: WidgetPosition {
                x: widget.position_x,
                y: widget.position_y,
            },
            size: WidgetSize {
                width: widget.width,
                height: widget.height,
            },
            refresh_interval_seconds: widget.refresh_interval_seconds,
            is_visible: widget.is_visible,
        }
    }
}

// ===== Error Types =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringError {
    pub error: String,
    pub details: Option<String>,
    pub code: Option<String>,
}

impl From<String> for MonitoringError {
    fn from(error: String) -> Self {
        Self {
            error,
            details: None,
            code: None,
        }
    }
}

impl From<&str> for MonitoringError {
    fn from(error: &str) -> Self {
        Self {
            error: error.to_string(),
            details: None,
            code: None,
        }
    }
}
