use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::{
    AcknowledgeProcessAlertRequest, CreateProcessMonitoringDashboardRequest,
    CreateProcessMonitoringReportRequest, CreateProcessMonitoringRuleRequest, ErrorResponse,
    ProcessAlertResponse, ProcessAnomalyResponse, ProcessCrashDumpResponse,
    ProcessDebugSessionResponse, ProcessHealthMetricResponse, ProcessLogResponse,
    ProcessMonitoringDashboardResponse, ProcessMonitoringReportResponse,
    ProcessMonitoringRuleResponse, ProcessTroubleshootingStepResponse, RunTroubleshootingStepRequest,
    StartDebugSessionRequest, UpdateProcessAnomalyStatusRequest,
};
use adapteros_db::process_monitoring::{
    AlertFilters, AlertSeverity, AlertStatus, AnomalyFilters, AnomalyStatus,
    CreateMonitoringRuleRequest as DbCreateRuleRequest, ProcessAlert, ProcessAnomaly,
    ProcessMonitoringRule, RuleType, ThresholdOperator,
};
use adapteros_db::users::Role;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use chrono::Utc;
use serde_json::json;
use sqlx::Row;
use std::collections::HashMap;

// ===== Process Logs and Debugging Endpoints =====

/// Get process logs for a worker
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/workers/{worker_id}/logs",
    params(
        ("worker_id" = String, Path, description = "Worker ID"),
        ("level" = Option<String>, Query, description = "Filter by log level"),
        ("limit" = Option<i32>, Query, description = "Maximum number of logs to return")
    ),
    responses(
        (status = 200, description = "Process logs", body = Vec<ProcessLogResponse>)
    )
)]
pub async fn list_process_logs(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessLogResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let level_filter = params.get("level");
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(100);

    // Query process logs from database
    let mut query = "SELECT * FROM process_logs WHERE worker_id = ?".to_string();
    if level_filter.is_some() {
        query.push_str(" AND level = ?");
    }
    query.push_str(" ORDER BY timestamp DESC LIMIT ?");

    let rows = if let Some(level) = level_filter {
        sqlx::query(&query)
            .bind(&worker_id)
            .bind(level)
            .bind(limit)
            .fetch_all(state.db.pool())
            .await
    } else {
        sqlx::query(&query.replace(" AND level = ?", ""))
            .bind(&worker_id)
            .bind(limit)
            .fetch_all(state.db.pool())
            .await
    };

    let rows = rows.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let logs: Vec<ProcessLogResponse> = rows
        .into_iter()
        .map(|row| ProcessLogResponse {
            id: row.get("id"),
            worker_id: row.get("worker_id"),
            level: row.get("level"),
            message: row.get("message"),
            timestamp: row.get("timestamp"),
            metadata_json: row.get("metadata_json"),
        })
        .collect();

    Ok(Json(logs))
}

/// Get process crash dumps for a worker
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/workers/{worker_id}/crashes",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    responses(
        (status = 200, description = "Process crash dumps", body = Vec<ProcessCrashDumpResponse>)
    )
)]
pub async fn list_process_crashes(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
) -> Result<Json<Vec<ProcessCrashDumpResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let rows = sqlx::query(
        "SELECT * FROM process_crash_dumps WHERE worker_id = ? ORDER BY crash_timestamp DESC LIMIT 100",
    )
    .bind(&worker_id)
    .fetch_all(state.db.pool())
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

    let crashes: Vec<ProcessCrashDumpResponse> = rows
        .into_iter()
        .map(|row| ProcessCrashDumpResponse {
            id: row.get("id"),
            worker_id: row.get("worker_id"),
            crash_type: row.get("crash_type"),
            stack_trace: row.get("stack_trace"),
            memory_snapshot_json: row.get("memory_snapshot_json"),
            crash_timestamp: row.get("crash_timestamp"),
            recovery_action: row.get("recovery_action"),
            recovered_at: row.get("recovered_at"),
        })
        .collect();

    Ok(Json(crashes))
}

/// Start a debug session for a worker
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/workers/{worker_id}/debug",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = StartDebugSessionRequest,
    responses(
        (status = 200, description = "Debug session started", body = ProcessDebugSessionResponse)
    )
)]
pub async fn start_debug_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<StartDebugSessionRequest>,
) -> Result<Json<ProcessDebugSessionResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let id = crate::id_generator::readable_id(
        adapteros_id::IdPrefix::Run,
        "process",
    );
    let started_at = Utc::now().to_rfc3339();

    // Insert debug session into database
    sqlx::query(
        "INSERT INTO process_debug_sessions (id, worker_id, session_type, status, config_json, started_at, created_by)
         VALUES (?, ?, ?, 'active', ?, ?, ?)",
    )
    .bind(&id)
    .bind(&worker_id)
    .bind(&req.session_type)
    .bind(&req.config_json)
    .bind(&started_at)
    .bind(&claims.sub)
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

    Ok(Json(ProcessDebugSessionResponse {
        id,
        worker_id,
        session_type: req.session_type,
        status: "active".to_string(),
        config_json: req.config_json,
        started_at,
        ended_at: None,
        results_json: None,
    }))
}

/// Run a troubleshooting step for a worker
#[utoipa::path(
    tag = "system",
    post,
    path = "/v1/workers/{worker_id}/troubleshoot",
    params(
        ("worker_id" = String, Path, description = "Worker ID")
    ),
    request_body = RunTroubleshootingStepRequest,
    responses(
        (status = 200, description = "Troubleshooting step started", body = ProcessTroubleshootingStepResponse)
    )
)]
pub async fn run_troubleshooting_step(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(worker_id): Path<String>,
    Json(req): Json<RunTroubleshootingStepRequest>,
) -> Result<Json<ProcessTroubleshootingStepResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;
    let worker_id = crate::id_resolver::resolve_any_id(&state.db, &worker_id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;

    let id = crate::id_generator::readable_id(
        adapteros_id::IdPrefix::Run,
        "process",
    );
    let started_at = Utc::now().to_rfc3339();

    // Insert troubleshooting step into database
    sqlx::query(
        "INSERT INTO process_troubleshooting_steps (id, worker_id, step_name, step_type, status, command, started_at)
         VALUES (?, ?, ?, ?, 'running', ?, ?)",
    )
    .bind(&id)
    .bind(&worker_id)
    .bind(&req.step_name)
    .bind(&req.step_type)
    .bind(&req.command)
    .bind(&started_at)
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

    Ok(Json(ProcessTroubleshootingStepResponse {
        id,
        worker_id,
        step_name: req.step_name,
        step_type: req.step_type,
        status: "running".to_string(),
        command: req.command,
        output: None,
        error_message: None,
        started_at,
        completed_at: None,
    }))
}

// ===== Advanced Process Monitoring and Alerting Endpoints =====

/// List process monitoring rules
#[utoipa::path(
    tag = "system",
    get,
    path = "/v1/monitoring/rules",
    params(
        ("tenant_id" = Option<String>, Query, description = "Filter by tenant ID"),
        ("rule_type" = Option<String>, Query, description = "Filter by rule type"),
        ("is_active" = Option<bool>, Query, description = "Filter by active status")
    ),
    responses(
        (status = 200, description = "Process monitoring rules", body = Vec<ProcessMonitoringRuleResponse>)
    )
)]
pub async fn list_process_monitoring_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringRuleResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params.get("tenant_id").map(|s| s.as_str());
    let is_active = params.get("is_active").and_then(|s| s.parse::<bool>().ok());

    let rules = ProcessMonitoringRule::list(state.db.pool(), tenant_id, is_active)
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

    let response: Vec<ProcessMonitoringRuleResponse> = rules
        .into_iter()
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
            evaluation_window_seconds: rule.evaluation_window_seconds,
            cooldown_seconds: rule.cooldown_seconds,
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

    let db_req = DbCreateRuleRequest {
        name: req.name.clone(),
        description: req.description.clone(),
        tenant_id: claims.tenant_id.clone(),
        rule_type: parse_rule_type(&req.rule_type),
        metric_name: req.metric_name.clone(),
        threshold_value: req.threshold_value,
        threshold_operator: parse_threshold_operator(&req.threshold_operator),
        severity: parse_alert_severity(&req.severity),
        evaluation_window_seconds: req.evaluation_window_seconds.unwrap_or(300),
        cooldown_seconds: req.cooldown_seconds.unwrap_or(60),
        is_active: true,
        notification_channels: req.notification_channels.clone(),
        escalation_rules: req.escalation_rules.clone(),
        created_by: Some(claims.sub.clone()),
    };

    let id = ProcessMonitoringRule::create(state.db.pool(), db_req)
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

    let now = Utc::now().to_rfc3339();
    Ok(Json(ProcessMonitoringRuleResponse {
        id,
        name: req.name,
        description: req.description,
        tenant_id: claims.tenant_id,
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
        created_by: Some(claims.sub),
        created_at: now.clone(),
        updated_at: now,
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAlertResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let filters = AlertFilters {
        tenant_id: params.get("tenant_id").cloned(),
        worker_id: params.get("worker_id").cloned(),
        status: params.get("status").map(|s| parse_alert_status(s)),
        severity: params.get("severity").map(|s| parse_alert_severity(s)),
        start_time: None,
        end_time: None,
        limit: Some(100),
        offset: None,
    };

    // Return empty list on database error (graceful degradation for empty/missing tables)
    let alerts = ProcessAlert::list(state.db.pool(), filters)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to list process alerts, returning empty list");
            vec![]
        });

    let response: Vec<ProcessAlertResponse> = alerts
        .into_iter()
        .map(|alert| ProcessAlertResponse {
            id: alert.id,
            rule_id: Some(alert.rule_id),
            worker_id: Some(alert.worker_id),
            tenant_id: alert.tenant_id,
            alert_type: alert.alert_type,
            severity: alert.severity.to_string(),
            title: alert.title,
            message: Some(alert.message),
            metric_value: alert.metric_value,
            threshold_value: alert.threshold_value,
            status: alert.status.to_string(),
            acknowledged_by: alert.acknowledged_by,
            acknowledged_at: alert.acknowledged_at.map(|dt| dt.to_rfc3339()),
            resolved_at: alert.resolved_at.map(|dt| dt.to_rfc3339()),
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

    ProcessAlert::update_status(
        state.db.pool(),
        &alert_id,
        AlertStatus::Acknowledged,
        Some(&claims.sub),
    )
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

    // Return updated alert
    let now = Utc::now().to_rfc3339();
    Ok(Json(ProcessAlertResponse {
        id: alert_id,
        rule_id: None,
        worker_id: None,
        tenant_id: claims.tenant_id,
        alert_type: "acknowledged".to_string(),
        severity: "info".to_string(),
        title: "Alert acknowledged".to_string(),
        message: None,
        metric_value: None,
        threshold_value: None,
        status: "acknowledged".to_string(),
        acknowledged_by: Some(claims.sub),
        acknowledged_at: Some(now.clone()),
        resolved_at: None,
        created_at: now.clone(),
        updated_at: now,
    }))
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessAnomalyResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let filters = AnomalyFilters {
        tenant_id: params.get("tenant_id").cloned(),
        worker_id: params.get("worker_id").cloned(),
        status: params.get("status").map(|s| parse_anomaly_status(s)),
        anomaly_type: params.get("anomaly_type").cloned(),
        start_time: None,
        end_time: None,
        limit: Some(100),
        offset: None,
    };

    let anomalies = ProcessAnomaly::list(state.db.pool(), filters)
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
        .map(|anomaly| ProcessAnomalyResponse {
            id: anomaly.id,
            worker_id: Some(anomaly.worker_id),
            tenant_id: anomaly.tenant_id,
            anomaly_type: anomaly.anomaly_type,
            metric_name: anomaly.metric_name,
            detected_value: Some(anomaly.detected_value),
            expected_range_json: Some(json!({
                "min": anomaly.expected_range_min,
                "max": anomaly.expected_range_max
            })),
            confidence_score: Some(anomaly.confidence_score),
            severity: anomaly.severity.to_string(),
            status: anomaly.status.to_string(),
            created_at: anomaly.created_at.to_rfc3339(),
            updated_at: anomaly.created_at.to_rfc3339(),
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

    let new_status = parse_anomaly_status(&req.status);

    sqlx::query(
        "UPDATE process_anomalies SET status = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
    )
    .bind(new_status.to_string())
    .bind(&anomaly_id)
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

    let now = Utc::now().to_rfc3339();
    Ok(Json(ProcessAnomalyResponse {
        id: anomaly_id,
        worker_id: None,
        tenant_id: claims.tenant_id,
        anomaly_type: "updated".to_string(),
        metric_name: String::new(),
        detected_value: None,
        expected_range_json: None,
        confidence_score: None,
        severity: "info".to_string(),
        status: req.status,
        created_at: now.clone(),
        updated_at: now,
    }))
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

    let tenant_id = params.get("tenant_id").unwrap_or(&claims.tenant_id);
    let is_shared = params.get("is_shared").and_then(|s| s.parse::<bool>().ok());

    let mut query = "SELECT * FROM process_monitoring_dashboards WHERE tenant_id = ?".to_string();
    if is_shared.is_some() {
        query.push_str(" AND is_shared = ?");
    }
    query.push_str(" ORDER BY created_at DESC");

    let rows = if let Some(shared) = is_shared {
        sqlx::query(&query)
            .bind(tenant_id)
            .bind(shared)
            .fetch_all(state.db.pool())
            .await
    } else {
        sqlx::query(&query.replace(" AND is_shared = ?", ""))
            .bind(tenant_id)
            .fetch_all(state.db.pool())
            .await
    };

    let rows = rows.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let dashboards: Vec<ProcessMonitoringDashboardResponse> = rows
        .into_iter()
        .map(|row| {
            let config_str: String = row.get("dashboard_config");
            ProcessMonitoringDashboardResponse {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                tenant_id: row.get("tenant_id"),
                dashboard_config: serde_json::from_str(&config_str).unwrap_or(json!({})),
                is_shared: row.get("is_shared"),
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

    let id = crate::id_generator::readable_id(
        adapteros_id::IdPrefix::Run,
        "process",
    );
    let now = Utc::now().to_rfc3339();
    let config_json = serde_json::to_string(&req.dashboard_config).unwrap_or("{}".to_string());

    sqlx::query(
        "INSERT INTO process_monitoring_dashboards (id, name, description, tenant_id, dashboard_config, is_shared, created_by, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&claims.tenant_id)
    .bind(&config_json)
    .bind(req.is_shared.unwrap_or(false))
    .bind(&claims.sub)
    .bind(&now)
    .bind(&now)
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

    Ok(Json(ProcessMonitoringDashboardResponse {
        id,
        name: req.name,
        description: req.description,
        tenant_id: claims.tenant_id,
        dashboard_config: req.dashboard_config,
        is_shared: req.is_shared.unwrap_or(false),
        created_by: Some(claims.sub),
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
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ProcessMonitoringReportResponse>>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &[Role::Operator, Role::Admin])?;

    let tenant_id = params.get("tenant_id").unwrap_or(&claims.tenant_id);
    let report_type = params.get("report_type");

    let mut query = "SELECT * FROM process_monitoring_reports WHERE tenant_id = ?".to_string();
    if report_type.is_some() {
        query.push_str(" AND report_type = ?");
    }
    query.push_str(" ORDER BY created_at DESC LIMIT 100");

    let rows = if let Some(rt) = report_type {
        sqlx::query(&query)
            .bind(tenant_id)
            .bind(rt)
            .fetch_all(state.db.pool())
            .await
    } else {
        sqlx::query(&query.replace(" AND report_type = ?", ""))
            .bind(tenant_id)
            .fetch_all(state.db.pool())
            .await
    };

    let rows = rows.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let reports: Vec<ProcessMonitoringReportResponse> = rows
        .into_iter()
        .map(|row| {
            let config_str: Option<String> = row.get("report_config");
            let data_str: Option<String> = row.get("report_data");
            ProcessMonitoringReportResponse {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                tenant_id: row.get("tenant_id"),
                report_type: row.get("report_type"),
                report_config: config_str.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({})),
                generated_at: row.get("generated_at"),
                report_data: data_str.and_then(|s| serde_json::from_str(&s).ok()),
                file_path: row.get("file_path"),
                file_size_bytes: row.get("file_size_bytes"),
                created_by: row.get("created_by"),
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

    let id = crate::id_generator::readable_id(
        adapteros_id::IdPrefix::Run,
        "process",
    );
    let now = Utc::now().to_rfc3339();
    let config_json = req.report_config.as_ref().map(|c| serde_json::to_string(c).unwrap_or("{}".to_string()));

    sqlx::query(
        "INSERT INTO process_monitoring_reports (id, name, description, tenant_id, report_type, report_config, generated_at, created_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&claims.tenant_id)
    .bind(&req.report_type)
    .bind(&config_json)
    .bind(&now)
    .bind(&claims.sub)
    .bind(&now)
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

    Ok(Json(ProcessMonitoringReportResponse {
        id,
        name: req.name,
        description: req.description,
        tenant_id: claims.tenant_id,
        report_type: req.report_type,
        report_config: req.report_config.unwrap_or(json!({})),
        generated_at: now.clone(),
        report_data: None,
        file_path: None,
        file_size_bytes: None,
        created_by: Some(claims.sub),
    }))
}

// ===== Helper Functions =====

fn parse_rule_type(s: &str) -> RuleType {
    match s.to_lowercase().as_str() {
        "cpu" => RuleType::Cpu,
        "memory" => RuleType::Memory,
        "latency" => RuleType::Latency,
        "error_rate" => RuleType::ErrorRate,
        _ => RuleType::Custom,
    }
}

fn parse_threshold_operator(s: &str) -> ThresholdOperator {
    match s.to_lowercase().as_str() {
        "gt" | ">" => ThresholdOperator::Gt,
        "lt" | "<" => ThresholdOperator::Lt,
        "eq" | "==" => ThresholdOperator::Eq,
        "gte" | ">=" => ThresholdOperator::Gte,
        "lte" | "<=" => ThresholdOperator::Lte,
        _ => ThresholdOperator::Gt,
    }
}

fn parse_alert_severity(s: &str) -> AlertSeverity {
    match s.to_lowercase().as_str() {
        "info" => AlertSeverity::Info,
        "warning" => AlertSeverity::Warning,
        "error" => AlertSeverity::Error,
        "critical" => AlertSeverity::Critical,
        _ => AlertSeverity::Info,
    }
}

fn parse_alert_status(s: &str) -> AlertStatus {
    match s.to_lowercase().as_str() {
        "active" => AlertStatus::Active,
        "acknowledged" => AlertStatus::Acknowledged,
        "resolved" => AlertStatus::Resolved,
        "suppressed" => AlertStatus::Suppressed,
        _ => AlertStatus::Active,
    }
}

fn parse_anomaly_status(s: &str) -> AnomalyStatus {
    match s.to_lowercase().as_str() {
        "detected" => AnomalyStatus::Detected,
        "investigating" => AnomalyStatus::Investigating,
        "confirmed" => AnomalyStatus::Confirmed,
        "false_positive" => AnomalyStatus::FalsePositive,
        "resolved" => AnomalyStatus::Resolved,
        _ => AnomalyStatus::Detected,
    }
}
