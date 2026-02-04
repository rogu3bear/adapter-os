//! Error alert rule handlers
//!
//! CRUD operations for error alert rules and alert history management.

use crate::auth::Claims;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::telemetry::{
    AcknowledgeAlertRequest, CreateErrorAlertRuleRequest, ErrorAlertHistoryListResponse,
    ErrorAlertHistoryResponse, ErrorAlertRuleResponse, ErrorAlertRulesListResponse,
    ResolveAlertRequest, UpdateErrorAlertRuleRequest,
};
use adapteros_db::client_errors::CreateAlertRuleParams;
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

/// Query parameters for alert history listing
#[derive(Debug, Deserialize)]
pub struct AlertHistoryQuery {
    /// Only show unresolved alerts
    #[serde(default)]
    pub unresolved_only: bool,
    /// Maximum number of alerts to return
    pub limit: Option<i64>,
}

/// List all error alert rules for the tenant
#[utoipa::path(
    get,
    path = "/v1/error-alerts/rules",
    tag = "Error Alerts",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Alert rules retrieved", body = ErrorAlertRulesListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_error_alert_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ErrorAlertRulesListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    let rules = state
        .db
        .list_error_alert_rules(tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    let response_rules: Vec<ErrorAlertRuleResponse> = rules
        .into_iter()
        .map(|r| ErrorAlertRuleResponse {
            id: r.id,
            tenant_id: r.tenant_id,
            name: r.name,
            description: r.description,
            error_type_pattern: r.error_type_pattern,
            http_status_pattern: r.http_status_pattern,
            page_pattern: r.page_pattern,
            threshold_count: r.threshold_count,
            threshold_window_minutes: r.threshold_window_minutes,
            cooldown_minutes: r.cooldown_minutes,
            severity: r.severity,
            is_active: r.is_active != 0,
            notification_channels: r
                .notification_channels_json
                .and_then(|s| serde_json::from_str(&s).ok()),
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: Some(r.updated_at),
        })
        .collect();

    let total = response_rules.len();

    Ok(Json(ErrorAlertRulesListResponse {
        rules: response_rules,
        total,
    }))
}

/// Get a single error alert rule by ID
#[utoipa::path(
    get,
    path = "/v1/error-alerts/rules/{id}",
    tag = "Error Alerts",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Alert rule ID")
    ),
    responses(
        (status = 200, description = "Alert rule retrieved", body = ErrorAlertRuleResponse),
        (status = 404, description = "Rule not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_error_alert_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<ErrorAlertRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let id = crate::id_resolver::resolve_any_id(&state.db, &id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    let tenant_id = &claims.tenant_id;

    let rule = state
        .db
        .get_error_alert_rule(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Alert rule not found")),
            )
        })?;

    // Verify tenant isolation
    if rule.tenant_id != *tenant_id {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Alert rule not found")),
        ));
    }

    Ok(Json(ErrorAlertRuleResponse {
        id: rule.id,
        tenant_id: rule.tenant_id,
        name: rule.name,
        description: rule.description,
        error_type_pattern: rule.error_type_pattern,
        http_status_pattern: rule.http_status_pattern,
        page_pattern: rule.page_pattern,
        threshold_count: rule.threshold_count,
        threshold_window_minutes: rule.threshold_window_minutes,
        cooldown_minutes: rule.cooldown_minutes,
        severity: rule.severity,
        is_active: rule.is_active != 0,
        notification_channels: rule
            .notification_channels_json
            .and_then(|s| serde_json::from_str(&s).ok()),
        created_by: rule.created_by,
        created_at: rule.created_at,
        updated_at: Some(rule.updated_at),
    }))
}

/// Create a new error alert rule
#[utoipa::path(
    post,
    path = "/v1/error-alerts/rules",
    tag = "Error Alerts",
    security(("bearer_auth" = [])),
    request_body = CreateErrorAlertRuleRequest,
    responses(
        (status = 201, description = "Alert rule created", body = ErrorAlertRuleResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn create_error_alert_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CreateErrorAlertRuleRequest>,
) -> Result<(StatusCode, Json<ErrorAlertRuleResponse>), (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;
    let user_id = &claims.sub;

    // Validate threshold values
    if request.threshold_count <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("threshold_count must be positive")),
        ));
    }
    if request.threshold_window_minutes <= 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                "threshold_window_minutes must be positive",
            )),
        ));
    }

    let notification_channels_json = request
        .notification_channels
        .and_then(|v| serde_json::to_string(&v).ok());

    let params = CreateAlertRuleParams {
        tenant_id: tenant_id.to_string(),
        name: request.name,
        description: request.description,
        error_type_pattern: request.error_type_pattern,
        http_status_pattern: request.http_status_pattern,
        page_pattern: request.page_pattern,
        threshold_count: request.threshold_count,
        threshold_window_minutes: request.threshold_window_minutes,
        cooldown_minutes: request.cooldown_minutes,
        severity: request.severity,
        notification_channels_json,
        created_by: Some(user_id.clone()),
    };

    let id = state
        .db
        .create_error_alert_rule(&params)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    // Fetch the created rule
    let rule = state
        .db
        .get_error_alert_rule(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to fetch created rule")),
            )
        })?;

    Ok((
        StatusCode::CREATED,
        Json(ErrorAlertRuleResponse {
            id: rule.id,
            tenant_id: rule.tenant_id,
            name: rule.name,
            description: rule.description,
            error_type_pattern: rule.error_type_pattern,
            http_status_pattern: rule.http_status_pattern,
            page_pattern: rule.page_pattern,
            threshold_count: rule.threshold_count,
            threshold_window_minutes: rule.threshold_window_minutes,
            cooldown_minutes: rule.cooldown_minutes,
            severity: rule.severity,
            is_active: rule.is_active != 0,
            notification_channels: rule
                .notification_channels_json
                .and_then(|s| serde_json::from_str(&s).ok()),
            created_by: rule.created_by,
            created_at: rule.created_at,
            updated_at: Some(rule.updated_at),
        }),
    ))
}

/// Update an error alert rule
#[utoipa::path(
    put,
    path = "/v1/error-alerts/rules/{id}",
    tag = "Error Alerts",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Alert rule ID")
    ),
    request_body = UpdateErrorAlertRuleRequest,
    responses(
        (status = 200, description = "Alert rule updated", body = ErrorAlertRuleResponse),
        (status = 404, description = "Rule not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn update_error_alert_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(request): Json<UpdateErrorAlertRuleRequest>,
) -> Result<Json<ErrorAlertRuleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let id = crate::id_resolver::resolve_any_id(&state.db, &id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    let tenant_id = &claims.tenant_id;

    // Fetch existing rule
    let mut rule = state
        .db
        .get_error_alert_rule(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Alert rule not found")),
            )
        })?;

    // Verify tenant isolation
    if rule.tenant_id != *tenant_id {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Alert rule not found")),
        ));
    }

    // Apply updates
    if let Some(name) = request.name {
        rule.name = name;
    }
    if let Some(description) = request.description {
        rule.description = Some(description);
    }
    if let Some(error_type_pattern) = request.error_type_pattern {
        rule.error_type_pattern = Some(error_type_pattern);
    }
    if let Some(http_status_pattern) = request.http_status_pattern {
        rule.http_status_pattern = Some(http_status_pattern);
    }
    if let Some(page_pattern) = request.page_pattern {
        rule.page_pattern = Some(page_pattern);
    }
    if let Some(threshold_count) = request.threshold_count {
        if threshold_count <= 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new("threshold_count must be positive")),
            ));
        }
        rule.threshold_count = threshold_count;
    }
    if let Some(threshold_window_minutes) = request.threshold_window_minutes {
        if threshold_window_minutes <= 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "threshold_window_minutes must be positive",
                )),
            ));
        }
        rule.threshold_window_minutes = threshold_window_minutes;
    }
    if let Some(cooldown_minutes) = request.cooldown_minutes {
        rule.cooldown_minutes = cooldown_minutes;
    }
    if let Some(severity) = request.severity {
        rule.severity = severity;
    }
    if let Some(is_active) = request.is_active {
        rule.is_active = if is_active { 1 } else { 0 };
    }
    if let Some(notification_channels) = request.notification_channels {
        rule.notification_channels_json =
            Some(serde_json::to_string(&notification_channels).unwrap_or_default());
    }

    // Save updates
    state.db.update_error_alert_rule(&rule).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    Ok(Json(ErrorAlertRuleResponse {
        id: rule.id,
        tenant_id: rule.tenant_id,
        name: rule.name,
        description: rule.description,
        error_type_pattern: rule.error_type_pattern,
        http_status_pattern: rule.http_status_pattern,
        page_pattern: rule.page_pattern,
        threshold_count: rule.threshold_count,
        threshold_window_minutes: rule.threshold_window_minutes,
        cooldown_minutes: rule.cooldown_minutes,
        severity: rule.severity,
        is_active: rule.is_active != 0,
        notification_channels: rule
            .notification_channels_json
            .and_then(|s| serde_json::from_str(&s).ok()),
        created_by: rule.created_by,
        created_at: rule.created_at,
        updated_at: Some(rule.updated_at),
    }))
}

/// Delete an error alert rule
#[utoipa::path(
    delete,
    path = "/v1/error-alerts/rules/{id}",
    tag = "Error Alerts",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Alert rule ID")
    ),
    responses(
        (status = 204, description = "Alert rule deleted"),
        (status = 404, description = "Rule not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn delete_error_alert_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let id = crate::id_resolver::resolve_any_id(&state.db, &id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    let tenant_id = &claims.tenant_id;

    // Verify rule exists and belongs to tenant
    let rule = state
        .db
        .get_error_alert_rule(&id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Alert rule not found")),
            )
        })?;

    if rule.tenant_id != *tenant_id {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Alert rule not found")),
        ));
    }

    state.db.delete_error_alert_rule(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// List error alert history
#[utoipa::path(
    get,
    path = "/v1/error-alerts/history",
    tag = "Error Alerts",
    security(("bearer_auth" = [])),
    params(
        ("unresolved_only" = Option<bool>, Query, description = "Only show unresolved alerts"),
        ("limit" = Option<i64>, Query, description = "Maximum number of alerts to return")
    ),
    responses(
        (status = 200, description = "Alert history retrieved", body = ErrorAlertHistoryListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn list_error_alert_history(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<AlertHistoryQuery>,
) -> Result<Json<ErrorAlertHistoryListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    let history = state
        .db
        .list_error_alert_history(tenant_id, query.limit, query.unresolved_only)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    // Fetch rule names for display
    let rules = state
        .db
        .list_error_alert_rules(tenant_id)
        .await
        .unwrap_or_default();

    let rule_names: std::collections::HashMap<String, String> =
        rules.into_iter().map(|r| (r.id, r.name)).collect();

    let alerts: Vec<ErrorAlertHistoryResponse> = history
        .into_iter()
        .map(|h| ErrorAlertHistoryResponse {
            id: h.id,
            rule_id: h.rule_id.clone(),
            rule_name: rule_names.get(&h.rule_id).cloned(),
            tenant_id: h.tenant_id,
            triggered_at: h.triggered_at,
            error_count: h.error_count,
            sample_error_ids: h
                .sample_error_ids_json
                .and_then(|s| serde_json::from_str(&s).ok()),
            acknowledged_at: h.acknowledged_at,
            acknowledged_by: h.acknowledged_by,
            resolved_at: h.resolved_at,
            resolution_note: h.resolution_note,
        })
        .collect();

    let total = alerts.len();

    Ok(Json(ErrorAlertHistoryListResponse { alerts, total }))
}

/// Acknowledge an alert
#[utoipa::path(
    post,
    path = "/v1/error-alerts/{id}/acknowledge",
    tag = "Error Alerts",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Alert history ID")
    ),
    responses(
        (status = 200, description = "Alert acknowledged"),
        (status = 404, description = "Alert not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn acknowledge_error_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let id = crate::id_resolver::resolve_any_id(&state.db, &id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    let user_id = &claims.sub;

    state
        .db
        .acknowledge_error_alert(&id, user_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    Ok(StatusCode::OK)
}

/// Resolve an alert
#[utoipa::path(
    post,
    path = "/v1/error-alerts/{id}/resolve",
    tag = "Error Alerts",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Alert history ID")
    ),
    request_body = ResolveAlertRequest,
    responses(
        (status = 200, description = "Alert resolved"),
        (status = 404, description = "Alert not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn resolve_error_alert(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(request): Json<ResolveAlertRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let id = crate::id_resolver::resolve_any_id(&state.db, &id)
        .await
        .map_err(|e| <(StatusCode, Json<ErrorResponse>)>::from(e))?;
    state
        .db
        .resolve_error_alert(&id, request.resolution_note.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    Ok(StatusCode::OK)
}
