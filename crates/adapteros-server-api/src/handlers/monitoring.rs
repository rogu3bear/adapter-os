use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::*;
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
use std::collections::HashMap;

/// List process monitoring rules
pub async fn list_process_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_filter = params.get("tenant_id");
    let rule_type_filter = params.get("rule_type");
    let is_active_filter = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    // Database query for monitoring rules - placeholder implementation
    // For now, return empty list
    Ok(Json(vec![]))
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

    // Monitoring rule creation - placeholder implementation
    Ok(Json(ProcessMonitoringRuleResponse {
        id: uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string(),
        name: req.name,
        description: req.description,
        tenant_id: "default".to_string(), // Placeholder - would extract from claims.sub
        rule_type: req.rule_type,
        metric_name: req.metric_name,
        threshold_value: req.threshold_value,
        threshold_operator: req.threshold_operator,
        severity: req.severity,
        evaluation_window_seconds: req.evaluation_window_seconds.unwrap_or(300),
        cooldown_seconds: req.cooldown_seconds.unwrap_or(60),
        is_active: true,
        notification_channels: req.notification_channels,
        escalation_rules: req.escalation_rules,
        created_by: Some(claims.sub.clone()),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
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
        ("severity" = Option<String>, Query, description = "Filter by severity")
    ),
    responses(
        (status = 200, description = "Process alerts", body = Vec<ProcessAlertResponse>)
    )
)]
pub async fn list_process_alerts(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
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
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(_alert_id): Path<String>,
    Json(_req): Json<AcknowledgeProcessAlertRequest>,
) -> Result<Json<ProcessAlertResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
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
        ("severity" = Option<String>, Query, description = "Filter by severity")
    ),
    responses(
        (status = 200, description = "Process anomalies", body = Vec<ProcessAnomalyResponse>)
    )
)]
pub async fn list_process_anomalies(
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAnomalyResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
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
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(_anomaly_id): Path<String>,
    Json(_req): Json<UpdateProcessAnomalyStatusRequest>,
) -> Result<Json<ProcessAnomalyResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
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
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringDashboardResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
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
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(_req): Json<CreateProcessMonitoringDashboardRequest>,
) -> Result<Json<ProcessMonitoringDashboardResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
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
        tenant_id: None, // Will be filtered by user's tenant access
        metric_name: metric_filter.cloned(),
        start_time: start_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        end_time: end_time_filter
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc)),
        limit: Some(1000), // Limit results
    };

    // Query health metrics from database
    let metrics = adapteros_system_metrics::ProcessHealthMetric::query(state.db.pool(), filters)
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

    // Convert ProcessHealthMetric to ProcessHealthMetricResponse
    let response_metrics: Vec<ProcessHealthMetricResponse> = metrics
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
        .collect();

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
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(_params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringReportResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
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
    State(_state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(_req): Json<CreateProcessMonitoringReportRequest>,
) -> Result<Json<ProcessMonitoringReportResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse::new("Endpoint not yet implemented").with_code("NOT_IMPLEMENTED")),
    ))
}

/// List monitoring rules (from adapteros_system_metrics)
pub async fn list_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<MonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params.get("tenant_id");
    let is_active = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    let rules = adapteros_system_metrics::ProcessMonitoringRule::list(
        state.db.pool(),
        tenant_id.map(|s| s.as_str()),
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

    let filters = adapteros_system_metrics::AlertFilters {
        tenant_id: params.get("tenant_id").cloned(),
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
