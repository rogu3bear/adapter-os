use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_core::AosError;
use adapteros_db::process_monitoring::{
    AlertFilters, AlertSeverity, AlertStatus, AnomalyFilters, AnomalyStatus, ProcessAlert,
    ProcessMonitoringRule, RuleType, ThresholdOperator,
};
use adapteros_db::users::Role;
use adapteros_system_metrics::monitoring_types::{
    AcknowledgeAlertRequest, AlertResponse, CreateMonitoringRuleApiRequest, MonitoringRuleResponse,
    UpdateMonitoringRuleApiRequest,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::{BTreeMap, HashMap};

fn is_missing_table_error(error: &sqlx::Error) -> bool {
    error.to_string().contains("no such table")
}

fn is_missing_db_table_error(error: &AosError, table: &str) -> bool {
    let message = error.to_string();
    message.contains("no such table") && message.contains(table)
}

fn parse_rule_type(value: &str) -> Option<RuleType> {
    match value {
        "cpu" => Some(RuleType::Cpu),
        "memory" => Some(RuleType::Memory),
        "latency" => Some(RuleType::Latency),
        "error_rate" => Some(RuleType::ErrorRate),
        "custom" => Some(RuleType::Custom),
        _ => None,
    }
}

fn parse_threshold_operator(value: &str) -> Option<ThresholdOperator> {
    match value {
        "gt" => Some(ThresholdOperator::Gt),
        "lt" => Some(ThresholdOperator::Lt),
        "eq" => Some(ThresholdOperator::Eq),
        "gte" => Some(ThresholdOperator::Gte),
        "lte" => Some(ThresholdOperator::Lte),
        _ => None,
    }
}

fn parse_alert_severity(value: &str) -> Option<AlertSeverity> {
    match value {
        "info" => Some(AlertSeverity::Info),
        "warning" => Some(AlertSeverity::Warning),
        "error" => Some(AlertSeverity::Error),
        "critical" => Some(AlertSeverity::Critical),
        _ => None,
    }
}

fn parse_alert_status(value: &str) -> Option<AlertStatus> {
    match value {
        "active" => Some(AlertStatus::Active),
        "acknowledged" => Some(AlertStatus::Acknowledged),
        "resolved" => Some(AlertStatus::Resolved),
        "suppressed" => Some(AlertStatus::Suppressed),
        _ => None,
    }
}

fn parse_anomaly_status(value: &str) -> Option<AnomalyStatus> {
    match value {
        "detected" => Some(AnomalyStatus::Detected),
        "investigating" => Some(AnomalyStatus::Investigating),
        "confirmed" => Some(AnomalyStatus::Confirmed),
        "false_positive" => Some(AnomalyStatus::FalsePositive),
        "resolved" => Some(AnomalyStatus::Resolved),
        _ => None,
    }
}

fn parse_json_value(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| json!({}))
}

fn is_missing_metrics_table(err: &adapteros_core::AosError) -> bool {
    err.to_string()
        .contains("no such table: process_health_metrics")
}

fn map_metrics(
    metrics: Vec<adapteros_system_metrics::ProcessHealthMetric>,
) -> Vec<ProcessHealthMetricResponse> {
    metrics
        .into_iter()
        .map(|metric| ProcessHealthMetricResponse {
            id: metric.id,
            worker_id: metric.worker_id,
            tenant_id: metric.tenant_id,
            metric_name: metric.metric_name,
            metric_value: metric.metric_value,
            metric_unit: metric.metric_unit,
            tags: metric.tags,
            collected_at: metric.collected_at.to_rfc3339(),
        })
        .collect()
}

async fn fetch_process_health_metrics_with_fallback(
    state: &AppState,
    filters: adapteros_system_metrics::MetricFilters,
) -> Result<Vec<ProcessHealthMetricResponse>, (StatusCode, Json<ErrorResponse>)> {
    match adapteros_system_metrics::ProcessHealthMetric::query(state.db.pool(), filters).await {
        Ok(metrics) => Ok(map_metrics(metrics)),
        Err(e) => {
            if is_missing_metrics_table(&e) {
                tracing::warn!(
                    error = %e,
                    "process_health_metrics table missing; returning empty metrics payload"
                );
                Ok(Vec::new())
            } else {
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ))
            }
        }
    }
}

async fn fetch_process_alert_response(
    pool: &sqlx::SqlitePool,
    alert_id: &str,
) -> Result<ProcessAlertResponse, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query("SELECT * FROM process_alerts WHERE id = ?")
        .bind(alert_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            if is_missing_table_error(&e) {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(
                        ErrorResponse::new("process_alerts table missing")
                            .with_code("MISSING_TABLE"),
                    ),
                );
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let row = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("alert not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(ProcessAlertResponse {
        id: row.get("id"),
        rule_id: row.get("rule_id"),
        worker_id: row.get("worker_id"),
        tenant_id: row.get("tenant_id"),
        alert_type: row.get("alert_type"),
        severity: AlertSeverity::from_string(row.get::<String, _>("severity")).to_string(),
        title: row.get("title"),
        message: row.get("message"),
        metric_value: row.get("metric_value"),
        threshold_value: row.get("threshold_value"),
        status: AlertStatus::from_string(row.get::<String, _>("status")).to_string(),
        acknowledged_by: row.get("acknowledged_by"),
        acknowledged_at: row
            .get::<Option<String>, _>("acknowledged_at")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339()),
        resolved_at: row
            .get::<Option<String>, _>("resolved_at")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339()),
        suppression_reason: row.get("suppression_reason"),
        suppression_until: row
            .get::<Option<String>, _>("suppression_until")
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339()),
        escalation_level: row.get::<i64, _>("escalation_level") as i32,
        notification_sent: row.get("notification_sent"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

/// List process monitoring rules
pub async fn list_process_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_filter = params
        .get("tenant_id")
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());
    validate_tenant_isolation(&claims, &tenant_filter)?;
    let rule_type_filter = params.get("rule_type").map(|value| value.to_lowercase());
    let is_active_filter = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    let rules =
        match ProcessMonitoringRule::list(state.db.pool(), Some(&tenant_filter), is_active_filter)
            .await
        {
            Ok(rules) => rules,
            Err(e) => {
                if is_missing_db_table_error(&e, "process_monitoring_rules") {
                    return Ok(Json(Vec::new()));
                }
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                ));
            }
        };

    let filtered = rules.into_iter().filter(|rule| {
        if let Some(ref rule_type) = rule_type_filter {
            rule.rule_type.to_string() == *rule_type
        } else {
            true
        }
    });

    let response = filtered
        .map(|rule| ProcessMonitoringRuleResponse {
            id: rule.id,
            name: rule.name,
            description: rule.description,
            tenant_id: rule.tenant_id,
            rule_type: rule.rule_type.to_string(),
            metric_name: rule.metric_name,
            threshold_value: rule.threshold_value,
            threshold_operator: rule.threshold_operator.to_string(),
            severity: rule.severity.to_string(),
            evaluation_window_seconds: rule.evaluation_window_seconds as i32,
            cooldown_seconds: rule.cooldown_seconds as i32,
            is_active: rule.is_active,
            notification_channels: rule.notification_channels,
            escalation_rules: rule.escalation_rules,
            created_by: rule.created_by,
            created_at: rule.created_at.to_rfc3339(),
            updated_at: rule.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(response))
}

/// Create process monitoring rule
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/rules",
    request_body = CreateProcessMonitoringRuleRequest,
    responses(
        (status = 200, description = "Monitoring rule created", body = ProcessMonitoringRuleResponse)
    )
)]
pub async fn create_process_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringRuleRequest>,
) -> Result<Json<ProcessMonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let rule_type = parse_rule_type(req.rule_type.as_str()).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid rule_type")
                    .with_code("BAD_REQUEST")
                    .with_string_details(req.rule_type.clone()),
            ),
        )
    })?;

    let threshold_operator =
        parse_threshold_operator(req.threshold_operator.as_str()).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid threshold_operator")
                        .with_code("BAD_REQUEST")
                        .with_string_details(req.threshold_operator.clone()),
                ),
            )
        })?;

    let severity = parse_alert_severity(req.severity.as_str()).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid severity")
                    .with_code("BAD_REQUEST")
                    .with_string_details(req.severity.clone()),
            ),
        )
    })?;

    let create_request = adapteros_db::process_monitoring::CreateMonitoringRuleRequest {
        name: req.name,
        description: req.description,
        tenant_id: claims.tenant_id.clone(),
        rule_type,
        metric_name: req.metric_name,
        threshold_value: req.threshold_value,
        threshold_operator,
        severity,
        evaluation_window_seconds: req.evaluation_window_seconds.unwrap_or(300) as i64,
        cooldown_seconds: req.cooldown_seconds.unwrap_or(60) as i64,
        is_active: true,
        notification_channels: req.notification_channels,
        escalation_rules: req.escalation_rules,
        created_by: Some(claims.sub.clone()),
    };

    let rule_id = ProcessMonitoringRule::create(state.db.pool(), create_request)
        .await
        .map_err(|e| {
            if is_missing_db_table_error(&e, "process_monitoring_rules") {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(
                        ErrorResponse::new("process_monitoring_rules table missing")
                            .with_code("MISSING_TABLE"),
                    ),
                );
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let rules = ProcessMonitoringRule::list(state.db.pool(), Some(&claims.tenant_id), None)
        .await
        .map_err(|e| {
            if is_missing_db_table_error(&e, "process_monitoring_rules") {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(
                        ErrorResponse::new("process_monitoring_rules table missing")
                            .with_code("MISSING_TABLE"),
                    ),
                );
            }
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let rule = rules
        .into_iter()
        .find(|rule| rule.id == rule_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("rule not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(ProcessMonitoringRuleResponse {
        id: rule.id,
        name: rule.name,
        description: rule.description,
        tenant_id: rule.tenant_id,
        rule_type: rule.rule_type.to_string(),
        metric_name: rule.metric_name,
        threshold_value: rule.threshold_value,
        threshold_operator: rule.threshold_operator.to_string(),
        severity: rule.severity.to_string(),
        evaluation_window_seconds: rule.evaluation_window_seconds as i32,
        cooldown_seconds: rule.cooldown_seconds as i32,
        is_active: rule.is_active,
        notification_channels: rule.notification_channels,
        escalation_rules: rule.escalation_rules,
        created_by: rule.created_by,
        created_at: rule.created_at.to_rfc3339(),
        updated_at: rule.updated_at.to_rfc3339(),
    }))
}

/// List process alerts
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/alerts",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by alert status"),
        ("severity" = Option<String>, Query, description = "Filter by severity"),
        ("limit" = Option<i64>, Query, description = "Maximum number of alerts to return"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination")
    ),
    responses(
        (status = 200, description = "Process alerts", body = Vec<ProcessAlertResponse>)
    )
)]
pub async fn list_process_alerts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params
        .get("tenant_id")
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());
    validate_tenant_isolation(&claims, &tenant_id)?;

    let status = match params.get("status") {
        Some(value) => Some(parse_alert_status(value.as_str()).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid status")
                        .with_code("BAD_REQUEST")
                        .with_string_details(value.to_string()),
                ),
            )
        })?),
        None => None,
    };

    let severity = match params.get("severity") {
        Some(value) => Some(parse_alert_severity(value.as_str()).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("invalid severity")
                        .with_code("BAD_REQUEST")
                        .with_string_details(value.to_string()),
                ),
            )
        })?),
        None => None,
    };

    let filters = AlertFilters {
        tenant_id: Some(tenant_id),
        worker_id: params.get("worker_id").cloned(),
        status,
        severity,
        start_time: None,
        end_time: None,
        limit: params.get("limit").and_then(|s| s.parse::<i64>().ok()),
        offset: params.get("offset").and_then(|s| s.parse::<i64>().ok()),
    };

    let alerts = match ProcessAlert::list(state.db.pool(), filters).await {
        Ok(alerts) => alerts,
        Err(e) => {
            if is_missing_db_table_error(&e, "process_alerts") {
                return Ok(Json(Vec::new()));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let response = alerts
        .into_iter()
        .map(|alert| ProcessAlertResponse {
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
            escalation_level: alert.escalation_level as i32,
            notification_sent: alert.notification_sent,
            created_at: alert.created_at.to_rfc3339(),
            updated_at: alert.updated_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(response))
}

/// Acknowledge process alert
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/alerts/{alert_id}/acknowledge",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    request_body = AcknowledgeProcessAlertRequest,
    responses(
        (status = 200, description = "Alert acknowledged", body = ProcessAlertResponse)
    )
)]
pub async fn acknowledge_process_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
    Json(_req): Json<AcknowledgeProcessAlertRequest>,
) -> Result<Json<ProcessAlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let alert_id = crate::id_resolver::resolve_any_id(&state.db, &alert_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let existing = fetch_process_alert_response(state.db.pool(), &alert_id).await?;
    validate_tenant_isolation(&claims, &existing.tenant_id)?;

    ProcessAlert::update_status(
        state.db.pool(),
        &alert_id,
        AlertStatus::Acknowledged,
        Some(&claims.sub),
    )
    .await
    .map_err(|e| {
        if is_missing_db_table_error(&e, "process_alerts") {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorResponse::new("process_alerts table missing").with_code("MISSING_TABLE")),
            );
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response = fetch_process_alert_response(state.db.pool(), &alert_id).await?;

    Ok(Json(response))
}

/// List process anomalies
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/anomalies",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by anomaly status"),
        ("anomaly_type" = Option<String>, Query, description = "Filter by anomaly type"),
        ("limit" = Option<i64>, Query, description = "Maximum number of anomalies to return"),
        ("offset" = Option<i64>, Query, description = "Offset for pagination")
    ),
    responses(
        (status = 200, description = "Process anomalies", body = Vec<ProcessAnomalyResponse>)
    )
)]
pub async fn list_process_anomalies(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAnomalyResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params
        .get("tenant_id")
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());
    validate_tenant_isolation(&claims, &tenant_id)?;

    let filters = AnomalyFilters {
        tenant_id: Some(tenant_id),
        worker_id: params.get("worker_id").cloned(),
        status: params
            .get("status")
            .map(|s| {
                parse_anomaly_status(s.as_str()).ok_or_else(|| {
                    (
                        StatusCode::BAD_REQUEST,
                        Json(
                            ErrorResponse::new("invalid status")
                                .with_code("BAD_REQUEST")
                                .with_string_details(s.to_string()),
                        ),
                    )
                })
            })
            .transpose()?,
        anomaly_type: params.get("anomaly_type").cloned(),
        start_time: None,
        end_time: None,
        limit: params.get("limit").and_then(|s| s.parse::<i64>().ok()),
        offset: params.get("offset").and_then(|s| s.parse::<i64>().ok()),
    };

    let anomalies =
        adapteros_db::process_monitoring::ProcessAnomaly::list(state.db.pool(), filters)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

    let response: Vec<ProcessAnomalyResponse> = anomalies
        .into_iter()
        .map(|a| ProcessAnomalyResponse {
            id: a.id,
            worker_id: a.worker_id,
            tenant_id: a.tenant_id,
            anomaly_type: a.anomaly_type,
            metric_name: a.metric_name,
            detected_value: a.detected_value,
            expected_range_min: a.expected_range_min,
            expected_range_max: a.expected_range_max,
            confidence_score: a.confidence_score,
            severity: a.severity.to_string(),
            description: a.description,
            detection_method: a.detection_method,
            model_version: a.model_version,
            status: a.status.to_string(),
            investigated_by: a.investigated_by,
            investigation_notes: a.investigation_notes,
            resolved_at: a.resolved_at.map(|dt| dt.to_rfc3339()),
            created_at: a.created_at.to_rfc3339(),
        })
        .collect();

    Ok(Json(response))
}

/// Update process anomaly status
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/anomalies/{anomaly_id}/status",
    params(
        ("anomaly_id" = String, Path, description = "Anomaly ID")
    ),
    request_body = UpdateProcessAnomalyStatusRequest,
    responses(
        (status = 200, description = "Anomaly status updated", body = ProcessAnomalyResponse)
    )
)]
pub async fn update_process_anomaly_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(anomaly_id): Path<String>,
    Json(req): Json<UpdateProcessAnomalyStatusRequest>,
) -> Result<Json<ProcessAnomalyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let anomaly_id = crate::id_resolver::resolve_any_id(&state.db, &anomaly_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let status = parse_anomaly_status(req.status.as_str()).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid status")
                    .with_code("BAD_REQUEST")
                    .with_string_details(req.status.clone()),
            ),
        )
    })?;

    let now = Utc::now().to_rfc3339();
    let mut sql =
        "UPDATE process_anomalies SET status = ?, investigated_by = ?, investigation_notes = ?"
            .to_string();
    if matches!(
        status,
        AnomalyStatus::Resolved | AnomalyStatus::FalsePositive
    ) {
        sql.push_str(", resolved_at = ?");
    }
    sql.push_str(" WHERE id = ? AND tenant_id = ?");

    let mut query = sqlx::query(&sql)
        .bind(status.to_string())
        .bind(&claims.sub)
        .bind(&req.investigation_notes);

    if matches!(
        status,
        AnomalyStatus::Resolved | AnomalyStatus::FalsePositive
    ) {
        query = query.bind(&now);
    }
    let result = query
        .bind(&anomaly_id)
        .bind(&claims.tenant_id)
        .execute(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("anomaly not found").with_code("NOT_FOUND")),
        ));
    }

    let row = sqlx::query("SELECT * FROM process_anomalies WHERE id = ? AND tenant_id = ?")
        .bind(&anomaly_id)
        .bind(&claims.tenant_id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response = ProcessAnomalyResponse {
        id: row.get("id"),
        worker_id: row.get("worker_id"),
        tenant_id: row.get("tenant_id"),
        anomaly_type: row.get("anomaly_type"),
        metric_name: row.get("metric_name"),
        detected_value: row.get("detected_value"),
        expected_range_min: row.get("expected_range_min"),
        expected_range_max: row.get("expected_range_max"),
        confidence_score: row.get("confidence_score"),
        severity: row.get::<String, _>("severity").to_lowercase(),
        description: row.get("description"),
        detection_method: row.get("detection_method"),
        model_version: row.get("model_version"),
        status: row.get::<String, _>("status"),
        investigated_by: row.get("investigated_by"),
        investigation_notes: row.get("investigation_notes"),
        resolved_at: row.get("resolved_at"),
        created_at: row.get("created_at"),
    };

    Ok(Json(response))
}

/// List process monitoring dashboards
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/dashboards",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("is_shared" = Option<bool>, Query, description = "Filter by shared status")
    ),
    responses(
        (status = 200, description = "Process monitoring dashboards", body = Vec<ProcessMonitoringDashboardResponse>)
    )
)]
pub async fn list_process_monitoring_dashboards(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringDashboardResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params
        .get("tenant_id")
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());
    validate_tenant_isolation(&claims, &tenant_id)?;
    let is_shared_filter = params.get("is_shared").and_then(|s| s.parse::<bool>().ok());

    let mut sql = String::from(
        "SELECT id, tenant_id, dashboard_name, dashboard_description, \
         dashboard_config_json, is_public, created_by, created_at, updated_at \
         FROM process_custom_dashboards WHERE tenant_id = ?",
    );
    if is_shared_filter.is_some() {
        sql.push_str(" AND is_public = ?");
    }
    sql.push_str(" ORDER BY updated_at DESC");

    let mut query = sqlx::query(&sql).bind(&tenant_id);
    if let Some(is_shared) = is_shared_filter {
        query = query.bind(if is_shared { 1 } else { 0 });
    }

    let rows = match query.fetch_all(state.db.pool()).await {
        Ok(rows) => rows,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok(Json(Vec::new()));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let dashboards = rows
        .into_iter()
        .map(|row| {
            let config_raw: String = row.get("dashboard_config_json");
            ProcessMonitoringDashboardResponse {
                id: row.get("id"),
                name: row.get("dashboard_name"),
                description: row.get("dashboard_description"),
                tenant_id: row.get("tenant_id"),
                dashboard_config: parse_json_value(&config_raw),
                is_shared: row.get::<i64, _>("is_public") != 0,
                created_by: row.get("created_by"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            }
        })
        .collect();

    Ok(Json(dashboards))
}

/// Create process monitoring dashboard
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/dashboards",
    request_body = CreateProcessMonitoringDashboardRequest,
    responses(
        (status = 200, description = "Dashboard created", body = ProcessMonitoringDashboardResponse)
    )
)]
pub async fn create_process_monitoring_dashboard(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringDashboardRequest>,
) -> Result<Json<ProcessMonitoringDashboardResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let dashboard_id =
        crate::id_generator::readable_id(adapteros_core::ids::IdKind::Report, "dashboard");
    let now = Utc::now().to_rfc3339();
    let config_json = serde_json::to_string(&req.dashboard_config).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid dashboard_config")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let layout_json = serde_json::to_string(&json!({"layout": []})).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid dashboard layout")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    sqlx::query(
        "INSERT INTO process_custom_dashboards \
         (id, tenant_id, dashboard_name, dashboard_description, dashboard_config_json, \
          dashboard_layout_json, dashboard_filters_json, dashboard_refresh_interval_seconds, \
          is_public, is_default, created_by, created_at, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&dashboard_id)
    .bind(&claims.tenant_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&config_json)
    .bind(&layout_json)
    .bind(Option::<String>::None)
    .bind(300_i64)
    .bind(if req.is_shared.unwrap_or(false) { 1 } else { 0 })
    .bind(0_i64)
    .bind(&claims.sub)
    .bind(&now)
    .bind(&now)
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        if is_missing_table_error(&e) {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("process_custom_dashboards table missing")
                        .with_code("MISSING_TABLE"),
                ),
            );
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(ProcessMonitoringDashboardResponse {
        id: dashboard_id,
        name: req.name,
        description: req.description,
        tenant_id: claims.tenant_id.clone(),
        dashboard_config: req.dashboard_config,
        is_shared: req.is_shared.unwrap_or(false),
        created_by: Some(claims.sub.clone()),
        created_at: now.clone(),
        updated_at: now,
    }))
}

/// List process health metrics
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/health-metrics",
    params(
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("metric_name" = Option<String>, Query, description = "Filter by metric name"),
        ("start_time" = Option<String>, Query, description = "Start time for metrics"),
        ("end_time" = Option<String>, Query, description = "End time for metrics")
    ),
    responses(
        (status = 200, description = "Process health metrics", body = Vec<ProcessHealthMetricResponse>)
    )
)]
pub async fn list_process_health_metrics(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessHealthMetricResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let worker_filter = params.get("worker_id");
    let metric_filter = params.get("metric_name");
    let start_time_filter = params.get("start_time");
    let end_time_filter = params.get("end_time");

    // Build filters for health metrics query
    let filters = adapteros_system_metrics::MetricFilters {
        worker_id: worker_filter.cloned(),
        tenant_id: Some(claims.tenant_id.clone()),
        metric_name: metric_filter.cloned(),
        start_time: start_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        end_time: end_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        limit: Some(1000), // Limit results
    };

    let response_metrics = fetch_process_health_metrics_with_fallback(&state, filters).await?;

    Ok(Json(response_metrics))
}

/// List process monitoring reports
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/reports",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("report_type" = Option<String>, Query, description = "Filter by report type")
    ),
    responses(
        (status = 200, description = "Process monitoring reports", body = Vec<ProcessMonitoringReportResponse>)
    )
)]
pub async fn list_process_monitoring_reports(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringReportResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params
        .get("tenant_id")
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());
    validate_tenant_isolation(&claims, &tenant_id)?;
    let report_type_filter = params.get("report_type");

    let mut sql = String::from(
        "SELECT id, tenant_id, report_name, report_type, report_data_json, \
         report_metadata_json, generated_by, generated_at, file_path, file_size_bytes \
         FROM process_usage_reports WHERE tenant_id = ?",
    );
    if report_type_filter.is_some() {
        sql.push_str(" AND report_type = ?");
    }
    sql.push_str(" ORDER BY generated_at DESC");

    let mut query = sqlx::query(&sql).bind(&tenant_id);
    if let Some(report_type) = report_type_filter {
        query = query.bind(report_type);
    }

    let rows = match query.fetch_all(state.db.pool()).await {
        Ok(rows) => rows,
        Err(e) => {
            if is_missing_table_error(&e) {
                return Ok(Json(Vec::new()));
            }
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            ));
        }
    };

    let reports = rows
        .into_iter()
        .map(|row| {
            let metadata = row
                .get::<Option<String>, _>("report_metadata_json")
                .map(|raw| parse_json_value(&raw))
                .unwrap_or_else(|| json!({}));
            let description = metadata
                .get("description")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string());
            let report_config = metadata.get("config").cloned().unwrap_or_else(|| json!({}));
            let report_data = row
                .get::<Option<String>, _>("report_data_json")
                .map(|raw| parse_json_value(&raw));

            ProcessMonitoringReportResponse {
                id: row.get("id"),
                name: row.get("report_name"),
                description,
                tenant_id: row.get("tenant_id"),
                report_type: row.get("report_type"),
                report_config,
                generated_at: row.get("generated_at"),
                report_data,
                file_path: row.get("file_path"),
                file_size_bytes: row.get("file_size_bytes"),
                created_by: row.get("generated_by"),
            }
        })
        .collect();

    Ok(Json(reports))
}

/// Create process monitoring report
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/reports",
    request_body = CreateProcessMonitoringReportRequest,
    responses(
        (status = 200, description = "Report created", body = ProcessMonitoringReportResponse)
    )
)]
pub async fn create_process_monitoring_report(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateProcessMonitoringReportRequest>,
) -> Result<Json<ProcessMonitoringReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let worker_id = req
        .report_config
        .get("worker_id")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let metric_name = req
        .report_config
        .get("metric_name")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    let start_time = req
        .report_config
        .get("start_time")
        .and_then(|value| value.as_str())
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let end_time = req
        .report_config
        .get("end_time")
        .and_then(|value| value.as_str())
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let filters = adapteros_system_metrics::MetricFilters {
        worker_id,
        tenant_id: Some(claims.tenant_id.clone()),
        metric_name,
        start_time,
        end_time,
        limit: Some(1000),
    };

    let (metrics, metrics_table_missing) =
        match adapteros_system_metrics::ProcessHealthMetric::query(state.db.pool(), filters).await {
            Ok(metrics) => (metrics, false),
            Err(e) => {
                if is_missing_metrics_table(&e) {
                    (Vec::new(), true)
                } else {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("database error")
                                .with_code("DATABASE_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    ));
                }
            }
        };

    let mut summary: BTreeMap<String, (i64, f64, f64, f64)> = BTreeMap::new();
    for metric in &metrics {
        let entry = summary.entry(metric.metric_name.clone()).or_insert((
            0,
            0.0,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ));
        entry.0 += 1;
        entry.1 += metric.metric_value;
        entry.2 = entry.2.min(metric.metric_value);
        entry.3 = entry.3.max(metric.metric_value);
    }

    let metric_summaries: Vec<serde_json::Value> = summary
        .into_iter()
        .map(|(name, (count, sum, min, max))| {
            let avg = if count > 0 { sum / count as f64 } else { 0.0 };
            json!({
                "metric_name": name,
                "samples": count,
                "avg": avg,
                "min": if min.is_finite() { min } else { 0.0 },
                "max": if max.is_finite() { max } else { 0.0 }
            })
        })
        .collect();

    let report_data = json!({
        "samples": metrics.len(),
        "metrics": metric_summaries,
        "metrics_table_missing": metrics_table_missing
    });

    let metadata = json!({
        "description": req.description,
        "config": req.report_config
    });

    let report_id = crate::id_generator::readable_id(adapteros_core::ids::IdKind::Report, "report");
    let now = Utc::now().to_rfc3339();
    let report_data_json = serde_json::to_string(&report_data).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid report data")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;
    let metadata_json = serde_json::to_string(&metadata).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid report metadata")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let period_start = start_time.unwrap_or_else(Utc::now).to_rfc3339();
    let period_end = end_time.unwrap_or_else(Utc::now).to_rfc3339();

    sqlx::query(
        "INSERT INTO process_usage_reports \
         (id, tenant_id, report_name, report_type, report_period_start, report_period_end, \
          report_data_json, report_metadata_json, generated_by, generated_at, report_status) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&report_id)
    .bind(&claims.tenant_id)
    .bind(&req.name)
    .bind(&req.report_type)
    .bind(&period_start)
    .bind(&period_end)
    .bind(&report_data_json)
    .bind(&metadata_json)
    .bind(&claims.sub)
    .bind(&now)
    .bind("completed")
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        if is_missing_table_error(&e) {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("process_usage_reports table missing")
                        .with_code("MISSING_TABLE"),
                ),
            );
        }
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(ProcessMonitoringReportResponse {
        id: report_id,
        name: req.name,
        description: req.description,
        tenant_id: claims.tenant_id.clone(),
        report_type: req.report_type,
        report_config: metadata.get("config").cloned().unwrap_or_else(|| json!({})),
        generated_at: now,
        report_data: Some(report_data),
        file_path: None,
        file_size_bytes: None,
        created_by: Some(claims.sub.clone()),
    }))
}

/// List monitoring rules (from adapteros_system_metrics)
#[utoipa::path(
    get,
    path = "/v1/monitoring/rules",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("is_active" = Option<bool>, Query, description = "Filter by active status")
    ),
    responses(
        (status = 200, description = "Monitoring rules", body = Vec<MonitoringRuleResponse>),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    ),
    tag = "monitoring"
)]
pub async fn list_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<MonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params
        .get("tenant_id")
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());
    validate_tenant_isolation(&claims, &tenant_id)?;

    let is_active = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(
        state.db.pool(),
        Some(&tenant_id),
        is_active,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<MonitoringRuleResponse> = rules.into_iter().map(|rule| rule.into()).collect();

    Ok(Json(response))
}

/// Create monitoring rule
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/rules",
    request_body = CreateMonitoringRuleApiRequest,
    responses(
        (status = 200, description = "Rule created", body = MonitoringRuleResponse)
    )
)]
pub async fn create_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateMonitoringRuleApiRequest>,
) -> Result<Json<MonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let rule_request = req.try_into().map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("invalid request")
                    .with_code("BAD_REQUEST")
                    .with_string_details(e),
            ),
        )
    })?;

    let rule_id =
        adapteros_system_metrics::ProcessMonitoringRule::create(state.db.pool(), rule_request)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;

    // Get the created rule
    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(state.db.pool(), None, None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let rule = rules.into_iter().find(|r| r.id == rule_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("rule not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(rule.into()))
}

/// Update monitoring rule
#[utoipa::path(
    tag = "system",
    put,
    path = "/v1/monitoring/rules/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "Rule ID")
    ),
    request_body = UpdateMonitoringRuleApiRequest,
    responses(
        (status = 200, description = "Rule updated", body = MonitoringRuleResponse)
    )
)]
pub async fn update_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(rule_id): Path<String>,
    Json(req): Json<UpdateMonitoringRuleApiRequest>,
) -> Result<Json<MonitoringRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let rule_id = crate::id_resolver::resolve_any_id(&state.db, &rule_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let update_request = req.into();

    adapteros_system_metrics::ProcessMonitoringRule::update(
        state.db.pool(),
        &rule_id,
        update_request,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated rule
    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(state.db.pool(), None, None)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let rule = rules.into_iter().find(|r| r.id == rule_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("rule not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(rule.into()))
}

/// Delete monitoring rule
#[utoipa::path(
    tag = "system",
    delete,
    path = "/v1/monitoring/rules/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "Rule ID")
    ),
    responses(
        (status = 200, description = "Rule deleted")
    )
)]
pub async fn delete_monitoring_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(rule_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let rule_id = crate::id_resolver::resolve_any_id(&state.db, &rule_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    // Fetch the rule first to validate tenant isolation
    let row = sqlx::query("SELECT tenant_id FROM process_monitoring_rules WHERE id = ?")
        .bind(&rule_id)
        .fetch_optional(state.db.pool())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let row = row.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("rule not found").with_code("NOT_FOUND")),
        )
    })?;

    let rule_tenant_id: String = row.get("tenant_id");
    validate_tenant_isolation(&claims, &rule_tenant_id)?;

    adapteros_system_metrics::ProcessMonitoringRule::delete(state.db.pool(), &rule_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::OK)
}

/// List alerts
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/alerts",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("worker_id" = Option<String>, Query, description = "Filter by worker ID"),
        ("status" = Option<String>, Query, description = "Filter by status"),
        ("severity" = Option<String>, Query, description = "Filter by severity"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "Alerts", body = Vec<AlertResponse>)
    )
)]
pub async fn list_alerts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<AlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params
        .get("tenant_id")
        .cloned()
        .unwrap_or_else(|| claims.tenant_id.clone());
    validate_tenant_isolation(&claims, &tenant_id)?;

    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: Some(tenant_id),
        worker_id: params.get("worker_id").cloned(),
        status: params
            .get("status")
            .and_then(|s| adapteros_system_metrics::AlertStatus::from_string(s.to_string()).into()),
        severity: params.get("severity").and_then(|s| {
            adapteros_system_metrics::AlertSeverity::from_string(s.to_string()).into()
        }),
        start_time: None,
        end_time: None,
        limit: params.get("limit").and_then(|s| s.parse::<i64>().ok()),
        offset: params.get("offset").and_then(|s| s.parse::<i64>().ok()),
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let response: Vec<AlertResponse> = alerts.into_iter().map(|alert| alert.into()).collect();

    Ok(Json(response))
}

/// Acknowledge alert
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/alerts/{alert_id}/acknowledge",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    request_body = AcknowledgeAlertRequest,
    responses(
        (status = 200, description = "Alert acknowledged", body = AlertResponse)
    )
)]
pub async fn acknowledge_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
    Json(req): Json<AcknowledgeAlertRequest>,
) -> Result<Json<AlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let alert_id = crate::id_resolver::resolve_any_id(&state.db, &alert_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    adapteros_system_metrics::ProcessAlert::update_status(
        state.db.pool(),
        &alert_id,
        adapteros_system_metrics::AlertStatus::Acknowledged,
        Some(&req.user),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated alert
    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        severity: None,
        start_time: None,
        end_time: None,
        limit: Some(1),
        offset: None,
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let alert = alerts
        .into_iter()
        .find(|a| a.id == alert_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("alert not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(alert.into()))
}

/// Resolve alert
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/monitoring/alerts/{alert_id}/resolve",
    params(
        ("alert_id" = String, Path, description = "Alert ID")
    ),
    responses(
        (status = 200, description = "Alert resolved", body = AlertResponse)
    )
)]
pub async fn resolve_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(alert_id): Path<String>,
) -> Result<Json<AlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let alert_id = crate::id_resolver::resolve_any_id(&state.db, &alert_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    adapteros_system_metrics::ProcessAlert::update_status(
        state.db.pool(),
        &alert_id,
        adapteros_system_metrics::AlertStatus::Resolved,
        None,
    )
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Get the updated alert
    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: None,
        worker_id: None,
        status: None,
        severity: None,
        start_time: None,
        end_time: None,
        limit: Some(1),
        offset: None,
    };

    let alerts = adapteros_system_metrics::ProcessAlert::list(state.db.pool(), filters)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("database error")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let alert = alerts
        .into_iter()
        .find(|a| a.id == alert_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("alert not found").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(alert.into()))
}
