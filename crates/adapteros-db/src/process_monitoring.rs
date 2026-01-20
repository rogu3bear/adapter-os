//! Process monitoring database operations
//!
//! Provides database access methods for process monitoring including:
//! - Process monitoring rules (CRUD operations)
//! - Process health metrics (insert, query, aggregation)
//! - Process alerts (create, update status, query by filters)
//! - Process anomalies (detect, track, resolve)
//! - Performance baselines (calculate, store, retrieve)
//! - Dashboard configurations (CRUD)
//! - Notification tracking

use crate::query_helpers::db_err;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

// ===== Type Definitions =====

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMonitoringRule {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub rule_type: RuleType,
    pub metric_name: String,
    pub threshold_value: f64,
    pub threshold_operator: ThresholdOperator,
    pub severity: AlertSeverity,
    pub evaluation_window_seconds: i64,
    pub cooldown_seconds: i64,
    pub is_active: bool,
    pub notification_channels: Option<serde_json::Value>,
    pub escalation_rules: Option<serde_json::Value>,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessHealthMetric {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_unit: Option<String>,
    pub tags: Option<serde_json::Value>,
    pub collected_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessAlert {
    pub id: String,
    pub rule_id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub alert_type: String,
    pub severity: AlertSeverity,
    pub title: String,
    pub message: String,
    pub metric_value: Option<f64>,
    pub threshold_value: Option<f64>,
    pub status: AlertStatus,
    pub acknowledged_by: Option<String>,
    pub acknowledged_at: Option<chrono::DateTime<chrono::Utc>>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub suppression_reason: Option<String>,
    pub suppression_until: Option<chrono::DateTime<chrono::Utc>>,
    pub escalation_level: i64,
    pub notification_sent: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessAnomaly {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub anomaly_type: String,
    pub metric_name: String,
    pub detected_value: f64,
    pub expected_range_min: Option<f64>,
    pub expected_range_max: Option<f64>,
    pub confidence_score: f64,
    pub severity: AlertSeverity,
    pub description: Option<String>,
    pub detection_method: String,
    pub model_version: Option<String>,
    pub status: AnomalyStatus,
    pub investigated_by: Option<String>,
    pub investigation_notes: Option<String>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBaseline {
    pub id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub baseline_type: BaselineType,
    pub calculation_period_days: i64,
    pub confidence_interval: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub percentile_95: Option<f64>,
    pub percentile_99: Option<f64>,
    pub is_active: bool,
    pub calculated_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringDashboard {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub dashboard_config: serde_json::Value,
    pub is_shared: bool,
    pub created_by: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringWidget {
    pub id: String,
    pub dashboard_id: String,
    pub widget_type: String,
    pub widget_config: serde_json::Value,
    pub position_x: i64,
    pub position_y: i64,
    pub width: i64,
    pub height: i64,
    pub refresh_interval_seconds: Option<i64>,
    pub is_visible: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringNotification {
    pub id: String,
    pub alert_id: String,
    pub notification_type: NotificationType,
    pub recipient: String,
    pub message: String,
    pub status: NotificationStatus,
    pub sent_at: Option<chrono::DateTime<chrono::Utc>>,
    pub delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_message: Option<String>,
    pub retry_count: i64,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ===== Enums =====

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleType {
    Cpu,
    Memory,
    Latency,
    ErrorRate,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThresholdOperator {
    Gt,
    Lt,
    Eq,
    Gte,
    Lte,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertStatus {
    Active,
    Acknowledged,
    Resolved,
    Suppressed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnomalyStatus {
    Detected,
    Investigating,
    Confirmed,
    FalsePositive,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaselineType {
    Historical,
    Statistical,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationType {
    Email,
    Slack,
    Webhook,
    Sms,
    Pagerduty,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationStatus {
    Pending,
    Sent,
    Failed,
    Delivered,
}

// ===== Filter Types =====

#[derive(Debug, Clone)]
pub struct MetricFilters {
    pub worker_id: Option<String>,
    pub tenant_id: Option<String>,
    pub metric_name: Option<String>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct AlertFilters {
    pub tenant_id: Option<String>,
    pub worker_id: Option<String>,
    pub status: Option<AlertStatus>,
    pub severity: Option<AlertSeverity>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct AnomalyFilters {
    pub tenant_id: Option<String>,
    pub worker_id: Option<String>,
    pub status: Option<AnomalyStatus>,
    pub anomaly_type: Option<String>,
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct TimeWindow {
    pub start: chrono::DateTime<chrono::Utc>,
    pub end: chrono::DateTime<chrono::Utc>,
    pub aggregation: AggregationType,
}

#[derive(Debug, Clone)]
pub enum AggregationType {
    Avg,
    Max,
    Min,
    Sum,
    Count,
}

#[derive(Debug, Clone)]
pub struct MetricsAggregation {
    pub window_start: chrono::DateTime<chrono::Utc>,
    pub window_end: chrono::DateTime<chrono::Utc>,
    pub avg_value: f64,
    pub max_value: f64,
    pub min_value: f64,
    pub sample_count: i64,
}

// ===== Database Operations =====

impl ProcessMonitoringRule {
    /// Create a new monitoring rule
    pub async fn create(pool: &SqlitePool, rule: CreateMonitoringRuleRequest) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();

        let rule_type_str = rule.rule_type.to_string();
        let threshold_operator_str = rule.threshold_operator.to_string();
        let severity_str = rule.severity.to_string();
        let notification_channels_str = rule
            .notification_channels
            .map(|v| serde_json::to_string(&v).unwrap());
        let escalation_rules_str = rule
            .escalation_rules
            .map(|v| serde_json::to_string(&v).unwrap());

        sqlx::query(
            r#"
            INSERT INTO process_monitoring_rules (
                id, name, description, tenant_id, rule_type, metric_name,
                threshold_value, threshold_operator, severity, evaluation_window_seconds,
                cooldown_seconds, is_active, notification_channels, escalation_rules, created_by
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&rule.name)
        .bind(&rule.description)
        .bind(&rule.tenant_id)
        .bind(&rule_type_str)
        .bind(&rule.metric_name)
        .bind(&rule.threshold_value)
        .bind(&threshold_operator_str)
        .bind(&severity_str)
        .bind(&rule.evaluation_window_seconds)
        .bind(&rule.cooldown_seconds)
        .bind(&rule.is_active)
        .bind(&notification_channels_str)
        .bind(&escalation_rules_str)
        .bind(&rule.created_by)
        .execute(pool)
        .await
        .map_err(db_err("create monitoring rule"))?;

        Ok(id)
    }

    /// List monitoring rules with optional filters
    pub async fn list(
        pool: &SqlitePool,
        tenant_id: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Vec<ProcessMonitoringRule>> {
        let mut query = "SELECT * FROM process_monitoring_rules WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> = vec![];
        let mut param_count = 0;

        if let Some(tenant) = tenant_id {
            param_count += 1;
            query.push_str(&format!(" AND tenant_id = ${}", param_count));
            params.push(Box::new(tenant.to_string()));
        }

        if let Some(active) = is_active {
            param_count += 1;
            query.push_str(&format!(" AND is_active = ${}", param_count));
            params.push(Box::new(active));
        }

        query.push_str(" ORDER BY created_at DESC");

        let rows = sqlx::query(&query)
            .fetch_all(pool)
            .await
            .map_err(db_err("list monitoring rules"))?;

        let mut rules = Vec::new();
        for row in rows {
            let rule = ProcessMonitoringRule {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                tenant_id: row.get("tenant_id"),
                rule_type: RuleType::from_string(row.get("rule_type")),
                metric_name: row.get("metric_name"),
                threshold_value: row.get("threshold_value"),
                threshold_operator: ThresholdOperator::from_string(row.get("threshold_operator")),
                severity: AlertSeverity::from_string(row.get("severity")),
                evaluation_window_seconds: row.get("evaluation_window_seconds"),
                cooldown_seconds: row.get("cooldown_seconds"),
                is_active: row.get("is_active"),
                notification_channels: row
                    .get::<Option<String>, _>("notification_channels")
                    .and_then(|s| serde_json::from_str(&s).ok()),
                escalation_rules: row
                    .get::<Option<String>, _>("escalation_rules")
                    .and_then(|s| serde_json::from_str(&s).ok()),
                created_by: row.get("created_by"),
                created_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("created_at"),
                )
                .map_err(|e| AosError::Database(format!("Invalid created_at: {}", e)))?
                .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("updated_at"),
                )
                .map_err(|e| AosError::Database(format!("Invalid updated_at: {}", e)))?
                .with_timezone(&chrono::Utc),
            };
            rules.push(rule);
        }

        Ok(rules)
    }

    /// Update a monitoring rule
    pub async fn update(
        pool: &SqlitePool,
        _id: &str,
        updates: UpdateMonitoringRuleRequest,
    ) -> Result<()> {
        let mut fields = Vec::new();
        let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> = vec![];
        let mut param_count = 0;

        if let Some(name) = updates.name {
            param_count += 1;
            fields.push(format!("name = ${}", param_count));
            params.push(Box::new(name));
        }

        if let Some(description) = updates.description {
            param_count += 1;
            fields.push(format!("description = ${}", param_count));
            params.push(Box::new(description));
        }

        if let Some(threshold_value) = updates.threshold_value {
            param_count += 1;
            fields.push(format!("threshold_value = ${}", param_count));
            params.push(Box::new(threshold_value));
        }

        if let Some(is_active) = updates.is_active {
            param_count += 1;
            fields.push(format!("is_active = ${}", param_count));
            params.push(Box::new(is_active));
        }

        if fields.is_empty() {
            return Ok(());
        }

        param_count += 1;
        fields.push("updated_at = CURRENT_TIMESTAMP".to_string());

        let query = format!(
            "UPDATE process_monitoring_rules SET {} WHERE id = ${}",
            fields.join(", "),
            param_count
        );

        sqlx::query(&query)
            .execute(pool)
            .await
            .map_err(db_err("update monitoring rule"))?;

        Ok(())
    }

    /// Delete a monitoring rule
    pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM process_monitoring_rules WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await
            .map_err(db_err("delete monitoring rule"))?;

        Ok(())
    }
}

impl ProcessHealthMetric {
    /// Insert a health metric
    pub async fn insert(pool: &SqlitePool, metric: CreateHealthMetricRequest) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();

        let tags_json = metric.tags.map(|v| serde_json::to_string(&v).unwrap());
        sqlx::query(
            r#"
            INSERT INTO process_health_metrics (
                id, worker_id, tenant_id, metric_name, metric_value, metric_unit, tags
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&metric.worker_id)
        .bind(&metric.tenant_id)
        .bind(&metric.metric_name)
        .bind(&metric.metric_value)
        .bind(&metric.metric_unit)
        .bind(&tags_json)
        .execute(pool)
        .await
        .map_err(db_err("insert health metric"))?;

        Ok(id)
    }

    /// Query health metrics with filters
    pub async fn query(
        pool: &SqlitePool,
        filters: MetricFilters,
    ) -> Result<Vec<ProcessHealthMetric>> {
        let mut query = "SELECT * FROM process_health_metrics WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> = vec![];
        let mut param_count = 0;

        if let Some(worker_id) = filters.worker_id {
            param_count += 1;
            query.push_str(&format!(" AND worker_id = ${}", param_count));
            params.push(Box::new(worker_id));
        }

        if let Some(tenant_id) = filters.tenant_id {
            param_count += 1;
            query.push_str(&format!(" AND tenant_id = ${}", param_count));
            params.push(Box::new(tenant_id));
        }

        if let Some(metric_name) = filters.metric_name {
            param_count += 1;
            query.push_str(&format!(" AND metric_name = ${}", param_count));
            params.push(Box::new(metric_name));
        }

        if let Some(start_time) = filters.start_time {
            param_count += 1;
            query.push_str(&format!(" AND collected_at >= ${}", param_count));
            params.push(Box::new(start_time.to_rfc3339()));
        }

        if let Some(end_time) = filters.end_time {
            param_count += 1;
            query.push_str(&format!(" AND collected_at <= ${}", param_count));
            params.push(Box::new(end_time.to_rfc3339()));
        }

        query.push_str(" ORDER BY collected_at DESC");

        if let Some(limit) = filters.limit {
            param_count += 1;
            query.push_str(&format!(" LIMIT ${}", param_count));
            params.push(Box::new(limit));
        }

        let rows = sqlx::query(&query)
            .fetch_all(pool)
            .await
            .map_err(db_err("query health metrics"))?;

        let mut metrics = Vec::new();
        for row in rows {
            let metric = ProcessHealthMetric {
                id: row.get("id"),
                worker_id: row.get("worker_id"),
                tenant_id: row.get("tenant_id"),
                metric_name: row.get("metric_name"),
                metric_value: row.get("metric_value"),
                metric_unit: row.get("metric_unit"),
                tags: row
                    .get::<Option<String>, _>("tags")
                    .and_then(|s| serde_json::from_str(&s).ok()),
                collected_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("collected_at"),
                )
                .map_err(|e| AosError::Database(format!("Invalid collected_at: {}", e)))?
                .with_timezone(&chrono::Utc),
            };
            metrics.push(metric);
        }

        Ok(metrics)
    }

    /// Aggregate metrics over a time window
    pub async fn aggregate(
        pool: &SqlitePool,
        window: TimeWindow,
        metric_name: &str,
        tenant_id: Option<&str>,
    ) -> Result<MetricsAggregation> {
        let aggregation_func = match window.aggregation {
            AggregationType::Avg => "AVG",
            AggregationType::Max => "MAX",
            AggregationType::Min => "MIN",
            AggregationType::Sum => "SUM",
            AggregationType::Count => "COUNT",
        };

        let mut query = format!(
            r#"
            SELECT 
                {} as aggregated_value,
                COUNT(*) as sample_count
            FROM process_health_metrics 
            WHERE metric_name = ? 
            AND collected_at >= ? 
            AND collected_at <= ?
            "#,
            aggregation_func
        );

        let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> = vec![
            Box::new(metric_name.to_string()),
            Box::new(window.start.to_rfc3339()),
            Box::new(window.end.to_rfc3339()),
        ];

        if let Some(tenant) = tenant_id {
            query.push_str(" AND tenant_id = ?");
            params.push(Box::new(tenant.to_string()));
        }

        let row = sqlx::query(&query)
            .fetch_one(pool)
            .await
            .map_err(db_err("aggregate metrics"))?;

        let aggregated_value: f64 = row.get("aggregated_value");
        let sample_count: i64 = row.get("sample_count");

        Ok(MetricsAggregation {
            window_start: window.start,
            window_end: window.end,
            avg_value: aggregated_value,
            max_value: aggregated_value, // For non-avg aggregations, this would need separate query
            min_value: aggregated_value, // For non-avg aggregations, this would need separate query
            sample_count,
        })
    }

    /// Delete health metrics older than the specified timestamp
    pub async fn delete_older_than(
        pool: &SqlitePool,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64> {
        let result = sqlx::query("DELETE FROM process_health_metrics WHERE collected_at < ?")
            .bind(timestamp.to_rfc3339())
            .execute(pool)
            .await
            .map_err(db_err("delete old health metrics"))?;

        Ok(result.rows_affected())
    }
}

impl ProcessAlert {
    /// Create a new alert
    pub async fn create(pool: &SqlitePool, alert: CreateAlertRequest) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();

        let severity_str = alert.severity.to_string();
        let status_str = alert.status.to_string();

        sqlx::query(
            r#"
            INSERT INTO process_alerts (
                id, rule_id, worker_id, tenant_id, alert_type, severity,
                title, message, metric_value, threshold_value, status
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&alert.rule_id)
        .bind(&alert.worker_id)
        .bind(&alert.tenant_id)
        .bind(&alert.alert_type)
        .bind(&severity_str)
        .bind(&alert.title)
        .bind(&alert.message)
        .bind(&alert.metric_value)
        .bind(&alert.threshold_value)
        .bind(&status_str)
        .execute(pool)
        .await
        .map_err(db_err("create alert"))?;

        Ok(id)
    }

    /// List alerts with filters
    pub async fn list(pool: &SqlitePool, filters: AlertFilters) -> Result<Vec<ProcessAlert>> {
        let mut query = "SELECT * FROM process_alerts WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> = vec![];
        let mut param_count = 0;

        if let Some(tenant_id) = filters.tenant_id {
            param_count += 1;
            query.push_str(&format!(" AND tenant_id = ${}", param_count));
            params.push(Box::new(tenant_id));
        }

        if let Some(worker_id) = filters.worker_id {
            param_count += 1;
            query.push_str(&format!(" AND worker_id = ${}", param_count));
            params.push(Box::new(worker_id));
        }

        if let Some(status) = filters.status {
            param_count += 1;
            query.push_str(&format!(" AND status = ${}", param_count));
            params.push(Box::new(status.to_string()));
        }

        if let Some(severity) = filters.severity {
            param_count += 1;
            query.push_str(&format!(" AND severity = ${}", param_count));
            params.push(Box::new(severity.to_string()));
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = filters.limit {
            param_count += 1;
            query.push_str(&format!(" LIMIT ${}", param_count));
            params.push(Box::new(limit));
        }

        let rows = sqlx::query(&query)
            .fetch_all(pool)
            .await
            .map_err(db_err("list alerts"))?;

        let mut alerts = Vec::new();
        for row in rows {
            let alert = ProcessAlert {
                id: row.get("id"),
                rule_id: row.get("rule_id"),
                worker_id: row.get("worker_id"),
                tenant_id: row.get("tenant_id"),
                alert_type: row.get("alert_type"),
                severity: AlertSeverity::from_string(row.get("severity")),
                title: row.get("title"),
                message: row.get("message"),
                metric_value: row.get("metric_value"),
                threshold_value: row.get("threshold_value"),
                status: AlertStatus::from_string(row.get("status")),
                acknowledged_by: row.get("acknowledged_by"),
                acknowledged_at: row
                    .get::<Option<String>, _>("acknowledged_at")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                resolved_at: row
                    .get::<Option<String>, _>("resolved_at")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                suppression_reason: row.get("suppression_reason"),
                suppression_until: row
                    .get::<Option<String>, _>("suppression_until")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                escalation_level: row.get("escalation_level"),
                notification_sent: row.get("notification_sent"),
                created_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("created_at"),
                )
                .map_err(|e| AosError::Database(format!("Invalid created_at: {}", e)))?
                .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("updated_at"),
                )
                .map_err(|e| AosError::Database(format!("Invalid updated_at: {}", e)))?
                .with_timezone(&chrono::Utc),
            };
            alerts.push(alert);
        }

        Ok(alerts)
    }

    /// Update alert status
    pub async fn update_status(
        pool: &SqlitePool,
        id: &str,
        status: AlertStatus,
        user: Option<&str>,
    ) -> Result<()> {
        let mut query =
            "UPDATE process_alerts SET status = ?, updated_at = CURRENT_TIMESTAMP".to_string();
        let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> =
            vec![Box::new(status.to_string())];

        match status {
            AlertStatus::Acknowledged => {
                query.push_str(", acknowledged_by = ?, acknowledged_at = CURRENT_TIMESTAMP");
                params.push(Box::new(user.unwrap_or("system").to_string()));
            }
            AlertStatus::Resolved => {
                query.push_str(", resolved_at = CURRENT_TIMESTAMP");
            }
            _ => {}
        }

        query.push_str(" WHERE id = ?");
        params.push(Box::new(id.to_string()));

        sqlx::query(&query)
            .execute(pool)
            .await
            .map_err(db_err("update alert status"))?;

        Ok(())
    }
}

impl ProcessAnomaly {
    /// Insert a new anomaly
    pub async fn insert(pool: &SqlitePool, anomaly: CreateAnomalyRequest) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();

        let severity_str = anomaly.severity.to_string();
        let status_str = anomaly.status.to_string();

        sqlx::query(
            r#"
            INSERT INTO process_anomalies (
                id, worker_id, tenant_id, anomaly_type, metric_name, detected_value,
                expected_range_min, expected_range_max, confidence_score, severity,
                description, detection_method, model_version, status
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&anomaly.worker_id)
        .bind(&anomaly.tenant_id)
        .bind(&anomaly.anomaly_type)
        .bind(&anomaly.metric_name)
        .bind(&anomaly.detected_value)
        .bind(&anomaly.expected_range_min)
        .bind(&anomaly.expected_range_max)
        .bind(&anomaly.confidence_score)
        .bind(&severity_str)
        .bind(&anomaly.description)
        .bind(&anomaly.detection_method)
        .bind(&anomaly.model_version)
        .bind(&status_str)
        .execute(pool)
        .await
        .map_err(db_err("insert anomaly"))?;

        Ok(id)
    }

    /// List anomalies with filters
    pub async fn list(pool: &SqlitePool, filters: AnomalyFilters) -> Result<Vec<ProcessAnomaly>> {
        let mut query = "SELECT * FROM process_anomalies WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn sqlx::Encode<'_, sqlx::Sqlite> + Send + Sync>> = vec![];
        let mut param_count = 0;

        if let Some(tenant_id) = filters.tenant_id {
            param_count += 1;
            query.push_str(&format!(" AND tenant_id = ${}", param_count));
            params.push(Box::new(tenant_id));
        }

        if let Some(worker_id) = filters.worker_id {
            param_count += 1;
            query.push_str(&format!(" AND worker_id = ${}", param_count));
            params.push(Box::new(worker_id));
        }

        if let Some(status) = filters.status {
            param_count += 1;
            query.push_str(&format!(" AND status = ${}", param_count));
            params.push(Box::new(status.to_string()));
        }

        if let Some(anomaly_type) = filters.anomaly_type {
            param_count += 1;
            query.push_str(&format!(" AND anomaly_type = ${}", param_count));
            params.push(Box::new(anomaly_type));
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(limit) = filters.limit {
            param_count += 1;
            query.push_str(&format!(" LIMIT ${}", param_count));
            params.push(Box::new(limit));
        }

        let rows = sqlx::query(&query)
            .fetch_all(pool)
            .await
            .map_err(db_err("list anomalies"))?;

        let mut anomalies = Vec::new();
        for row in rows {
            let anomaly = ProcessAnomaly {
                id: row.get("id"),
                worker_id: row.get("worker_id"),
                tenant_id: row.get("tenant_id"),
                anomaly_type: row.get("anomaly_type"),
                metric_name: row.get("metric_name"),
                detected_value: row.get("detected_value"),
                expected_range_min: row.get("expected_range_min"),
                expected_range_max: row.get("expected_range_max"),
                confidence_score: row.get("confidence_score"),
                severity: AlertSeverity::from_string(row.get("severity")),
                description: row.get("description"),
                detection_method: row.get("detection_method"),
                model_version: row.get("model_version"),
                status: AnomalyStatus::from_string(row.get("status")),
                investigated_by: row.get("investigated_by"),
                investigation_notes: row.get("investigation_notes"),
                resolved_at: row
                    .get::<Option<String>, _>("resolved_at")
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
                created_at: chrono::DateTime::parse_from_rfc3339(
                    &row.get::<String, _>("created_at"),
                )
                .map_err(|e| AosError::Database(format!("Invalid created_at: {}", e)))?
                .with_timezone(&chrono::Utc),
            };
            anomalies.push(anomaly);
        }

        Ok(anomalies)
    }
}

impl PerformanceBaseline {
    /// Upsert a performance baseline
    pub async fn upsert(pool: &SqlitePool, baseline: CreateBaselineRequest) -> Result<()> {
        let baseline_type_str = baseline.baseline_type.to_string();
        let expires_at_str = baseline.expires_at.map(|dt| dt.to_rfc3339());

        // First, try to get existing ID if any
        let existing_id: Option<String> = sqlx::query_scalar(
            "SELECT id FROM process_performance_baselines WHERE worker_id = ? AND metric_name = ? AND is_active = true"
        )
        .bind(&baseline.worker_id)
        .bind(&baseline.metric_name)
        .fetch_optional(pool)
        .await
        .map_err(db_err("check existing baseline"))?;

        let id = existing_id.unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO process_performance_baselines (
                id, worker_id, tenant_id, metric_name, baseline_value, baseline_type,
                calculation_period_days, confidence_interval, standard_deviation,
                percentile_95, percentile_99, is_active, calculated_at, expires_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP, ?)
            "#,
        )
        .bind(&id)
        .bind(&baseline.worker_id)
        .bind(&baseline.tenant_id)
        .bind(&baseline.metric_name)
        .bind(&baseline.baseline_value)
        .bind(&baseline_type_str)
        .bind(&baseline.calculation_period_days)
        .bind(&baseline.confidence_interval)
        .bind(&baseline.standard_deviation)
        .bind(&baseline.percentile_95)
        .bind(&baseline.percentile_99)
        .bind(&baseline.is_active)
        .bind(&expires_at_str)
        .execute(pool)
        .await
        .map_err(db_err("upsert baseline"))?;

        Ok(())
    }

    /// Get baseline for a worker and metric
    pub async fn get(
        pool: &SqlitePool,
        worker_id: &str,
        metric_name: &str,
    ) -> Result<Option<PerformanceBaseline>> {
        let row = sqlx::query_as::<
            _,
            (
                String,         // id
                String,         // worker_id
                String,         // tenant_id
                String,         // metric_name
                f64,            // baseline_value
                String,         // baseline_type
                i64,            // calculation_period_days
                Option<f64>,    // confidence_interval
                Option<f64>,    // standard_deviation
                Option<f64>,    // percentile_95
                Option<f64>,    // percentile_99
                bool,           // is_active
                String,         // calculated_at
                Option<String>, // expires_at
            ),
        >(
            "SELECT id, worker_id, tenant_id, metric_name, baseline_value, baseline_type,
             calculation_period_days, confidence_interval, standard_deviation,
             percentile_95, percentile_99, is_active, calculated_at, expires_at
             FROM process_performance_baselines
             WHERE worker_id = ? AND metric_name = ? AND is_active = true",
        )
        .bind(worker_id)
        .bind(metric_name)
        .fetch_optional(pool)
        .await
        .map_err(db_err("get baseline"))?;

        if let Some(row) = row {
            Ok(Some(PerformanceBaseline {
                id: row.0,
                worker_id: row.1,
                tenant_id: row.2,
                metric_name: row.3,
                baseline_value: row.4,
                baseline_type: BaselineType::from_string(row.5),
                calculation_period_days: row.6,
                confidence_interval: row.7,
                standard_deviation: row.8,
                percentile_95: row.9,
                percentile_99: row.10,
                is_active: row.11,
                calculated_at: chrono::DateTime::parse_from_rfc3339(&row.12)
                    .map_err(|e| AosError::Database(format!("Invalid calculated_at: {}", e)))?
                    .with_timezone(&chrono::Utc),
                expires_at: row
                    .13
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc)),
            }))
        } else {
            Ok(None)
        }
    }
}

// ===== Request/Response Types =====

#[derive(Debug, Clone)]
pub struct CreateMonitoringRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub tenant_id: String,
    pub rule_type: RuleType,
    pub metric_name: String,
    pub threshold_value: f64,
    pub threshold_operator: ThresholdOperator,
    pub severity: AlertSeverity,
    pub evaluation_window_seconds: i64,
    pub cooldown_seconds: i64,
    pub is_active: bool,
    pub notification_channels: Option<serde_json::Value>,
    pub escalation_rules: Option<serde_json::Value>,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateMonitoringRuleRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub threshold_value: Option<f64>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct CreateHealthMetricRequest {
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub metric_value: f64,
    pub metric_unit: Option<String>,
    pub tags: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct CreateAlertRequest {
    pub rule_id: String,
    pub worker_id: String,
    pub tenant_id: String,
    pub alert_type: String,
    pub severity: AlertSeverity,
    pub title: String,
    pub message: String,
    pub metric_value: Option<f64>,
    pub threshold_value: Option<f64>,
    pub status: AlertStatus,
}

#[derive(Debug, Clone)]
pub struct CreateAnomalyRequest {
    pub worker_id: String,
    pub tenant_id: String,
    pub anomaly_type: String,
    pub metric_name: String,
    pub detected_value: f64,
    pub expected_range_min: Option<f64>,
    pub expected_range_max: Option<f64>,
    pub confidence_score: f64,
    pub severity: AlertSeverity,
    pub description: Option<String>,
    pub detection_method: String,
    pub model_version: Option<String>,
    pub status: AnomalyStatus,
}

#[derive(Debug, Clone)]
pub struct CreateBaselineRequest {
    pub worker_id: String,
    pub tenant_id: String,
    pub metric_name: String,
    pub baseline_value: f64,
    pub baseline_type: BaselineType,
    pub calculation_period_days: i64,
    pub confidence_interval: Option<f64>,
    pub standard_deviation: Option<f64>,
    pub percentile_95: Option<f64>,
    pub percentile_99: Option<f64>,
    pub is_active: bool,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ===== Enum Implementations =====

impl RuleType {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "cpu" => RuleType::Cpu,
            "memory" => RuleType::Memory,
            "latency" => RuleType::Latency,
            "error_rate" => RuleType::ErrorRate,
            "custom" => RuleType::Custom,
            _ => RuleType::Custom,
        }
    }
}

impl ThresholdOperator {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "gt" => ThresholdOperator::Gt,
            "lt" => ThresholdOperator::Lt,
            "eq" => ThresholdOperator::Eq,
            "gte" => ThresholdOperator::Gte,
            "lte" => ThresholdOperator::Lte,
            _ => ThresholdOperator::Gt,
        }
    }
}

impl AlertSeverity {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "info" => AlertSeverity::Info,
            "warning" => AlertSeverity::Warning,
            "error" => AlertSeverity::Error,
            "critical" => AlertSeverity::Critical,
            _ => AlertSeverity::Info,
        }
    }
}

impl AlertStatus {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "active" => AlertStatus::Active,
            "acknowledged" => AlertStatus::Acknowledged,
            "resolved" => AlertStatus::Resolved,
            "suppressed" => AlertStatus::Suppressed,
            _ => AlertStatus::Active,
        }
    }
}

impl AnomalyStatus {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "detected" => AnomalyStatus::Detected,
            "investigating" => AnomalyStatus::Investigating,
            "confirmed" => AnomalyStatus::Confirmed,
            "false_positive" => AnomalyStatus::FalsePositive,
            "resolved" => AnomalyStatus::Resolved,
            _ => AnomalyStatus::Detected,
        }
    }
}

impl BaselineType {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "historical" => BaselineType::Historical,
            "statistical" => BaselineType::Statistical,
            "manual" => BaselineType::Manual,
            _ => BaselineType::Statistical,
        }
    }
}

impl NotificationType {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "email" => NotificationType::Email,
            "slack" => NotificationType::Slack,
            "webhook" => NotificationType::Webhook,
            "sms" => NotificationType::Sms,
            "pagerduty" => NotificationType::Pagerduty,
            _ => NotificationType::Email,
        }
    }
}

impl NotificationStatus {
    pub fn from_string(s: String) -> Self {
        match s.as_str() {
            "pending" => NotificationStatus::Pending,
            "sent" => NotificationStatus::Sent,
            "failed" => NotificationStatus::Failed,
            "delivered" => NotificationStatus::Delivered,
            _ => NotificationStatus::Pending,
        }
    }
}

// ===== Trait Implementations =====

impl std::fmt::Display for RuleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleType::Cpu => write!(f, "cpu"),
            RuleType::Memory => write!(f, "memory"),
            RuleType::Latency => write!(f, "latency"),
            RuleType::ErrorRate => write!(f, "error_rate"),
            RuleType::Custom => write!(f, "custom"),
        }
    }
}

impl std::fmt::Display for ThresholdOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThresholdOperator::Gt => write!(f, "gt"),
            ThresholdOperator::Lt => write!(f, "lt"),
            ThresholdOperator::Eq => write!(f, "eq"),
            ThresholdOperator::Gte => write!(f, "gte"),
            ThresholdOperator::Lte => write!(f, "lte"),
        }
    }
}

impl std::fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "info"),
            AlertSeverity::Warning => write!(f, "warning"),
            AlertSeverity::Error => write!(f, "error"),
            AlertSeverity::Critical => write!(f, "critical"),
        }
    }
}

impl std::fmt::Display for AlertStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertStatus::Active => write!(f, "active"),
            AlertStatus::Acknowledged => write!(f, "acknowledged"),
            AlertStatus::Resolved => write!(f, "resolved"),
            AlertStatus::Suppressed => write!(f, "suppressed"),
        }
    }
}

impl std::fmt::Display for AnomalyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnomalyStatus::Detected => write!(f, "detected"),
            AnomalyStatus::Investigating => write!(f, "investigating"),
            AnomalyStatus::Confirmed => write!(f, "confirmed"),
            AnomalyStatus::FalsePositive => write!(f, "false_positive"),
            AnomalyStatus::Resolved => write!(f, "resolved"),
        }
    }
}

impl std::fmt::Display for BaselineType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BaselineType::Historical => write!(f, "historical"),
            BaselineType::Statistical => write!(f, "statistical"),
            BaselineType::Manual => write!(f, "manual"),
        }
    }
}

impl std::fmt::Display for NotificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationType::Email => write!(f, "email"),
            NotificationType::Slack => write!(f, "slack"),
            NotificationType::Webhook => write!(f, "webhook"),
            NotificationType::Sms => write!(f, "sms"),
            NotificationType::Pagerduty => write!(f, "pagerduty"),
        }
    }
}

impl std::fmt::Display for NotificationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationStatus::Pending => write!(f, "pending"),
            NotificationStatus::Sent => write!(f, "sent"),
            NotificationStatus::Failed => write!(f, "failed"),
            NotificationStatus::Delivered => write!(f, "delivered"),
        }
    }
}
