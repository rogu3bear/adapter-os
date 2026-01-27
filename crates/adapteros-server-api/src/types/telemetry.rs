//! Telemetry and metrics types for the API layer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use utoipa::ToSchema;

/// Event tracking progress of long-running model operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OperationProgressEvent {
    /// Unique operation identifier
    pub operation_id: String,
    /// Model ID being operated on
    pub model_id: String,
    /// Operation type: "load", "unload", "validate"
    pub operation: String,
    /// Current operation status: "started", "in_progress", "completed", "failed"
    pub status: String,
    /// Progress percentage (0-100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<u8>,
    /// Duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Error message if status is "failed"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Event creation timestamp (RFC3339)
    #[schema(value_type = String)]
    pub created_at: DateTime<Utc>,
}

impl OperationProgressEvent {
    /// Create a new operation progress event with "started" status
    pub fn new(operation_id: String, model_id: String, operation: String) -> Self {
        Self {
            operation_id,
            model_id,
            operation,
            status: "started".to_string(),
            progress_percent: None,
            duration_ms: None,
            error_message: None,
            created_at: Utc::now(),
        }
    }
}

/// Single metric data point with timestamp
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricDataPointResponse {
    /// Unix timestamp in milliseconds
    pub timestamp: u64,
    /// Metric value
    pub value: f64,
    /// Optional labels/tags for the data point
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<HashMap<String, String>>,
}

/// Time series data for a single metric
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsSeriesResponse {
    /// Name of the metric series
    pub series_name: String,
    /// Data points in the series
    pub points: Vec<MetricDataPointResponse>,
}

fn default_schema_version() -> String {
    adapteros_api_types::API_SCHEMA_VERSION.to_string()
}

/// Current metrics snapshot with counters, gauges, and histograms
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsSnapshotResponse {
    /// API schema version for frontend compatibility
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    /// Counter metrics (monotonically increasing values)
    pub counters: HashMap<String, f64>,
    /// Gauge metrics (point-in-time values)
    pub gauges: HashMap<String, f64>,
    /// Histogram metrics (distribution summaries)
    pub histograms: HashMap<String, Vec<f64>>,
    /// Timestamp when snapshot was taken (RFC3339)
    pub timestamp: String,
    /// Flattened metrics map for frontend compatibility (union of counters and gauges)
    #[serde(default)]
    pub metrics: HashMap<String, f64>,
}

/// Telemetry bundle response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TelemetryBundleResponse {
    pub id: String,
    pub cpid: String,
    pub event_count: u64,
    pub size_bytes: u64,
    pub created_at: String,
}

/// Bundle verification response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BundleVerificationResponse {
    pub bundle_id: String,
    pub verified: bool,
    pub signature_valid: bool,
    pub merkle_root_valid: bool,
    pub verified_at: String,
}

/// Purge bundles request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PurgeBundlesRequest {
    pub keep_count: Option<usize>,
    pub older_than_days: Option<i64>,
}

/// Purge bundles response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PurgeBundlesResponse {
    pub purged_count: usize,
    pub retained_count: usize,
}

/// Export telemetry bundle response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ExportTelemetryBundleResponse {
    pub bundle_id: String,
    pub events_count: i64,
    pub size_bytes: i64,
    pub download_url: String,
    pub expires_at: String,
}

/// Verify bundle signature response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct VerifyBundleSignatureResponse {
    pub bundle_id: String,
    pub valid: bool,
    pub signature: String,
    pub signed_by: String,
    pub signed_at: String,
    pub verification_error: Option<String>,
}

/// Purge old bundles request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PurgeOldBundlesRequest {
    pub keep_bundles_per_cpid: i32,
}

/// Purge old bundles response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PurgeOldBundlesResponse {
    pub purged_count: i32,
    pub retained_count: i32,
    pub freed_bytes: i64,
    pub purged_cpids: Vec<String>,
}

/// Process health metric
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessHealthMetricResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_unit: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub collected_at: String,
}

/// Process monitoring rule
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringRuleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub rule_type: String,
    pub metric_name: String,
    pub threshold_value: f64,
    pub threshold_operator: String,
    pub severity: String,
    pub evaluation_window_seconds: i32,
    pub cooldown_seconds: i32,
    pub is_active: bool,
    pub notification_channels: Option<serde_json::Value>,
    pub escalation_rules: Option<serde_json::Value>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process alert
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessAlertResponse {
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
    pub escalation_level: i32,
    pub notification_sent: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Process anomaly
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessAnomalyResponse {
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

/// Process performance baseline
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessPerformanceBaselineResponse {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub baseline_type: String,
    pub calculation_period_days: i32,
    pub confidence_interval: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub percentile_95: Option<f64>,
    pub percentile_99: Option<f64>,
    pub is_active: bool,
    pub calculated_at: String,
    pub expires_at: Option<String>,
}

/// Process monitoring dashboard
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringDashboardResponse {
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

/// Process monitoring widget
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringWidgetResponse {
    pub id: String,
    pub dashboard_id: String,
    pub widget_type: String,
    pub widget_config: serde_json::Value,
    pub position_x: i32,
    pub position_y: i32,
    pub width: i32,
    pub height: i32,
    pub refresh_interval_seconds: i32,
    pub is_visible: bool,
    pub created_at: String,
}

/// Process monitoring notification
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringNotificationResponse {
    pub id: String,
    pub alert_id: String,
    pub notification_type: String,
    pub recipient: String,
    pub message: String,
    pub status: String,
    pub sent_at: Option<String>,
    pub delivered_at: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: i32,
    pub created_at: String,
}

/// Process monitoring schedule
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ProcessMonitoringScheduleResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub schedule_type: String,
    pub schedule_config: serde_json::Value,
    pub is_active: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Process monitoring report
#[derive(Debug, Serialize, Deserialize, ToSchema)]
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

/// Create monitoring rule request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub rule_type: String,
    pub metric_name: String,
    pub threshold_value: f64,
    pub threshold_operator: String,
    pub severity: String,
    pub evaluation_window_seconds: Option<i32>,
    pub cooldown_seconds: Option<i32>,
    pub notification_channels: Option<serde_json::Value>,
    pub escalation_rules: Option<serde_json::Value>,
}

/// Create monitoring dashboard request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringDashboardRequest {
    pub name: String,
    pub description: Option<String>,
    pub dashboard_config: serde_json::Value,
    pub is_shared: Option<bool>,
}

/// Create monitoring widget request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringWidgetRequest {
    pub dashboard_id: String,
    pub widget_type: String,
    pub widget_config: serde_json::Value,
    pub position_x: i32,
    pub position_y: i32,
    pub width: i32,
    pub height: i32,
    pub refresh_interval_seconds: Option<i32>,
}

/// Create monitoring schedule request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringScheduleRequest {
    pub name: String,
    pub description: Option<String>,
    pub schedule_type: String,
    pub schedule_config: serde_json::Value,
}

/// Create monitoring report request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateProcessMonitoringReportRequest {
    pub name: String,
    pub description: Option<String>,
    pub report_type: String,
    pub report_config: serde_json::Value,
}

/// Acknowledge alert request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AcknowledgeProcessAlertRequest {
    pub alert_id: String,
    pub acknowledgment_note: Option<String>,
}

/// Resolve alert request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ResolveProcessAlertRequest {
    pub alert_id: String,
    pub resolution_note: Option<String>,
}

/// Suppress alert request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SuppressProcessAlertRequest {
    pub alert_id: String,
    pub suppression_reason: String,
    pub suppression_until: Option<String>,
}

/// Update anomaly status request
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateProcessAnomalyStatusRequest {
    pub anomaly_id: String,
    pub status: String,
    pub investigation_notes: Option<String>,
}
