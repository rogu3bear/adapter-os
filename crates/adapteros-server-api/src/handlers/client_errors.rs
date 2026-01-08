//! Client error reporting and querying endpoints
//!
//! Receives error reports from the UI for persistent server-side logging,
//! and provides query endpoints for the error dashboard.

use crate::auth::Claims;
use crate::services::ErrorAlertEvaluator;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::telemetry::{ClientErrorReport, ClientErrorResponse};
use adapteros_db::client_errors::{ClientError, ClientErrorQuery, ClientErrorStats};
use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use futures_util::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;
use uuid::Uuid;

/// Maximum message length (2000 chars)
const MAX_MESSAGE_LENGTH: usize = 2000;

/// Default tenant ID for anonymous errors
const ANONYMOUS_TENANT_ID: &str = "__anonymous__";

// =============================================================================
// Query Parameter Types
// =============================================================================

/// Query parameters for listing client errors
#[derive(Debug, Deserialize)]
pub struct ListClientErrorsQuery {
    pub error_type: Option<String>,
    pub http_status: Option<i32>,
    pub page: Option<String>,
    pub user_id: Option<String>,
    pub since: Option<String>,
    pub until: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Query parameters for error statistics
#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    pub since: Option<String>,
}

// =============================================================================
// Response Types
// =============================================================================

/// Response for list client errors endpoint
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ClientErrorsListResponse {
    pub errors: Vec<ClientErrorItem>,
    pub total: usize,
}

/// Individual client error item in list response
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ClientErrorItem {
    pub id: String,
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub error_type: String,
    pub message: String,
    pub code: Option<String>,
    pub failure_code: Option<String>,
    pub http_status: Option<i32>,
    pub page: Option<String>,
    pub client_timestamp: String,
    pub created_at: String,
}

impl From<ClientError> for ClientErrorItem {
    fn from(e: ClientError) -> Self {
        Self {
            id: e.id,
            tenant_id: e.tenant_id,
            user_id: e.user_id,
            error_type: e.error_type,
            message: e.message,
            code: e.code,
            failure_code: e.failure_code,
            http_status: e.http_status,
            page: e.page,
            client_timestamp: e.client_timestamp,
            created_at: e.created_at,
        }
    }
}

/// Response for error statistics endpoint
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ClientErrorStatsResponse {
    pub total_count: i64,
    pub error_type_counts: Vec<TypeCount>,
    pub http_status_counts: Vec<StatusCount>,
    pub errors_per_hour: Vec<HourlyCount>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct TypeCount {
    pub error_type: String,
    pub count: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StatusCount {
    pub http_status: i32,
    pub count: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct HourlyCount {
    pub hour: String,
    pub count: i64,
}

impl From<ClientErrorStats> for ClientErrorStatsResponse {
    fn from(stats: ClientErrorStats) -> Self {
        Self {
            total_count: stats.total_count,
            error_type_counts: stats
                .error_type_counts
                .into_iter()
                .map(|(error_type, count)| TypeCount { error_type, count })
                .collect(),
            http_status_counts: stats
                .http_status_counts
                .into_iter()
                .map(|(http_status, count)| StatusCount { http_status, count })
                .collect(),
            errors_per_hour: stats
                .errors_per_hour
                .into_iter()
                .map(|h| HourlyCount {
                    hour: h.hour,
                    count: h.count,
                })
                .collect(),
        }
    }
}

// =============================================================================
// Error Reporting Endpoints
// =============================================================================

/// POST /v1/telemetry/client-errors - Report client error (authenticated)
#[utoipa::path(
    post,
    path = "/v1/telemetry/client-errors",
    request_body = ClientErrorReport,
    responses(
        (status = 201, description = "Error report received", body = ClientErrorResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn report_client_error(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(report): Json<ClientErrorReport>,
) -> Result<(StatusCode, Json<ClientErrorResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Validate payload
    validate_report(&report)?;

    let error_id = Uuid::now_v7().to_string();
    let received_at = chrono::Utc::now().to_rfc3339();

    // Build client error for database storage
    let client_error = ClientError {
        id: error_id.clone(),
        tenant_id: claims.tenant_id.clone(),
        user_id: Some(claims.sub.clone()),
        error_type: report.error_type.clone(),
        message: truncate_to_max(&report.message),
        code: report.code.clone(),
        failure_code: report.failure_code.clone(),
        http_status: report.http_status.map(|s| s as i32),
        page: report.page.clone(),
        user_agent: report.user_agent.clone(),
        client_timestamp: report.timestamp.clone(),
        details_json: report.details.as_ref().map(|d| d.to_string()),
        ip_address: None, // Could extract from request extensions if needed
        session_id: None,
        created_at: received_at.clone(),
    };

    // Persist to database
    let db_success = match state.db.insert_client_error(&client_error).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(
                error = %e,
                error_id = %error_id,
                "Failed to persist client error to database"
            );
            // Continue even if DB write fails - we still log it
            false
        }
    };

    // Evaluate alert rules (only if DB write succeeded)
    if db_success {
        let evaluator = ErrorAlertEvaluator::new(state.db.as_db_arc());
        if let Err(e) = evaluator.evaluate(&client_error).await {
            tracing::warn!(
                error = %e,
                error_id = %error_id,
                "Failed to evaluate error alert rules"
            );
        }
    }

    // Log the client error with structured fields
    tracing::info!(
        target: "client_error",
        error_id = %error_id,
        tenant_id = %claims.tenant_id,
        user_id = %claims.sub,
        error_type = %report.error_type,
        message = %truncate_message(&report.message),
        code = ?report.code,
        failure_code = ?report.failure_code,
        http_status = ?report.http_status,
        page = ?report.page,
        client_timestamp = %report.timestamp,
        "Client error reported"
    );

    Ok((
        StatusCode::CREATED,
        Json(ClientErrorResponse {
            error_id,
            received_at,
        }),
    ))
}

/// POST /v1/telemetry/client-errors/anonymous - Report client error (pre-auth)
///
/// Used for errors that occur before authentication (login failures, bootstrap errors).
/// Has stricter rate limiting and validation.
#[utoipa::path(
    post,
    path = "/v1/telemetry/client-errors/anonymous",
    request_body = ClientErrorReport,
    responses(
        (status = 201, description = "Error report received", body = ClientErrorResponse),
        (status = 400, description = "Invalid request"),
        (status = 429, description = "Rate limited"),
    ),
    tag = "telemetry"
)]
pub async fn report_client_error_anonymous(
    State(state): State<AppState>,
    Json(report): Json<ClientErrorReport>,
) -> Result<(StatusCode, Json<ClientErrorResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Validate payload
    validate_report(&report)?;

    let error_id = Uuid::now_v7().to_string();
    let received_at = chrono::Utc::now().to_rfc3339();

    // Build client error for database storage (anonymous tenant)
    let client_error = ClientError {
        id: error_id.clone(),
        tenant_id: ANONYMOUS_TENANT_ID.to_string(),
        user_id: None,
        error_type: report.error_type.clone(),
        message: truncate_to_max(&report.message),
        code: report.code.clone(),
        failure_code: report.failure_code.clone(),
        http_status: report.http_status.map(|s| s as i32),
        page: report.page.clone(),
        user_agent: report.user_agent.clone(),
        client_timestamp: report.timestamp.clone(),
        details_json: report.details.as_ref().map(|d| d.to_string()),
        ip_address: None,
        session_id: None,
        created_at: received_at.clone(),
    };

    // Persist to database
    let db_success = match state.db.insert_client_error(&client_error).await {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(
                error = %e,
                error_id = %error_id,
                "Failed to persist anonymous client error to database"
            );
            false
        }
    };

    // Evaluate alert rules (only if DB write succeeded)
    // Note: Anonymous errors use a shared tenant ID for alert rules
    if db_success {
        let evaluator = ErrorAlertEvaluator::new(state.db.as_db_arc());
        if let Err(e) = evaluator.evaluate(&client_error).await {
            tracing::warn!(
                error = %e,
                error_id = %error_id,
                "Failed to evaluate error alert rules for anonymous error"
            );
        }
    }

    // Log the anonymous client error
    tracing::info!(
        target: "client_error",
        error_id = %error_id,
        anonymous = true,
        error_type = %report.error_type,
        message = %truncate_message(&report.message),
        code = ?report.code,
        failure_code = ?report.failure_code,
        http_status = ?report.http_status,
        page = ?report.page,
        client_timestamp = %report.timestamp,
        "Anonymous client error reported"
    );

    Ok((
        StatusCode::CREATED,
        Json(ClientErrorResponse {
            error_id,
            received_at,
        }),
    ))
}

// =============================================================================
// Query Endpoints
// =============================================================================

/// GET /v1/telemetry/client-errors - List client errors with filtering
#[utoipa::path(
    get,
    path = "/v1/telemetry/client-errors",
    params(
        ("error_type" = Option<String>, Query, description = "Filter by error type"),
        ("http_status" = Option<i32>, Query, description = "Filter by HTTP status"),
        ("page" = Option<String>, Query, description = "Filter by page pattern (glob)"),
        ("user_id" = Option<String>, Query, description = "Filter by user ID"),
        ("since" = Option<String>, Query, description = "ISO 8601 start timestamp"),
        ("until" = Option<String>, Query, description = "ISO 8601 end timestamp"),
        ("limit" = Option<i64>, Query, description = "Max results (default 50)"),
        ("offset" = Option<i64>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Client errors list", body = ClientErrorsListResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn list_client_errors(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListClientErrorsQuery>,
) -> Result<Json<ClientErrorsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let query = ClientErrorQuery {
        tenant_id: claims.tenant_id.clone(),
        error_type: params.error_type,
        http_status: params.http_status,
        page_pattern: params.page,
        user_id: params.user_id,
        since: params.since,
        until: params.until,
        limit: Some(params.limit.unwrap_or(50).min(500)), // Cap at 500
        offset: params.offset,
    };

    let errors = state
        .db
        .list_client_errors(&query)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to list client errors");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to retrieve errors").with_code("DATABASE_ERROR")),
            )
        })?;

    let total = errors.len();
    let items: Vec<ClientErrorItem> = errors.into_iter().map(|e| e.into()).collect();

    Ok(Json(ClientErrorsListResponse {
        errors: items,
        total,
    }))
}

/// GET /v1/telemetry/client-errors/stats - Get error statistics
#[utoipa::path(
    get,
    path = "/v1/telemetry/client-errors/stats",
    params(
        ("since" = Option<String>, Query, description = "Stats since timestamp (ISO 8601)"),
    ),
    responses(
        (status = 200, description = "Error statistics", body = ClientErrorStatsResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn get_client_error_stats(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<StatsQuery>,
) -> Result<Json<ClientErrorStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let stats = state
        .db
        .get_client_error_stats(&claims.tenant_id, params.since.as_deref())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to get client error stats");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to retrieve stats").with_code("DATABASE_ERROR")),
            )
        })?;

    Ok(Json(stats.into()))
}

/// GET /v1/telemetry/client-errors/{id} - Get a single client error by ID
#[utoipa::path(
    get,
    path = "/v1/telemetry/client-errors/{id}",
    params(
        ("id" = String, Path, description = "Error ID"),
    ),
    responses(
        (status = 200, description = "Client error details", body = ClientErrorItem),
        (status = 404, description = "Error not found"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn get_client_error(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<Json<ClientErrorItem>, (StatusCode, Json<ErrorResponse>)> {
    let error = state
        .db
        .get_client_error(&id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, error_id = %id, "Failed to get client error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to retrieve error").with_code("DATABASE_ERROR")),
            )
        })?;

    match error {
        Some(e) if e.tenant_id == claims.tenant_id || e.tenant_id == ANONYMOUS_TENANT_ID => {
            Ok(Json(e.into()))
        }
        Some(_) => Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Access denied").with_code("FORBIDDEN")),
        )),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Error not found").with_code("NOT_FOUND")),
        )),
    }
}

// =============================================================================
// SSE Streaming Endpoint
// =============================================================================

/// State for the SSE stream polling
struct StreamState {
    db: adapteros_db::ProtectedDb,
    tenant_id: String,
    last_timestamp: String,
    sent_initial: bool,
}

/// GET /v1/stream/client-errors - SSE stream of client errors
///
/// Streams new client errors as they are reported in real-time.
/// Uses polling-based approach with delta queries.
#[utoipa::path(
    get,
    path = "/v1/stream/client-errors",
    responses(
        (status = 200, description = "SSE stream of client errors"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "telemetry",
    security(("bearer_token" = []))
)]
pub async fn stream_client_errors(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let initial_timestamp = chrono::Utc::now().to_rfc3339();

    let stream_state = StreamState {
        db: state.db.clone(),
        tenant_id: claims.tenant_id.clone(),
        last_timestamp: initial_timestamp.clone(),
        sent_initial: false,
    };

    // Use unfold to create a polling stream
    let stream = futures_util::stream::unfold(stream_state, |mut state| async move {
        // First iteration: send connection event
        if !state.sent_initial {
            state.sent_initial = true;
            let event = Event::default()
                .event("connected")
                .data(format!(
                    r#"{{"tenant_id":"{}","connected_at":"{}"}}"#,
                    state.tenant_id, state.last_timestamp
                ));
            return Some((Ok(event), state));
        }

        // Wait before polling
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Query for new errors since last timestamp
        match state
            .db
            .list_client_errors_since(&state.tenant_id, &state.last_timestamp, Some(50))
            .await
        {
            Ok(errors) if !errors.is_empty() => {
                // Update last_timestamp to the newest error
                if let Some(newest) = errors.last() {
                    state.last_timestamp = newest.created_at.clone();
                }

                // For simplicity, send the first error and re-enter the loop
                // (unfold only yields one item per iteration)
                if let Some(error) = errors.into_iter().next() {
                    let item: ClientErrorItem = error.into();
                    if let Ok(json) = serde_json::to_string(&item) {
                        let event = Event::default().event("client_error").data(json);
                        return Some((Ok(event), state));
                    }
                }
                // Fallback to heartbeat
                let event = Event::default().comment("heartbeat");
                Some((Ok(event), state))
            }
            Ok(_) => {
                // No new errors, send heartbeat
                let event = Event::default().comment("heartbeat");
                Some((Ok(event), state))
            }
            Err(e) => {
                tracing::warn!(error = %e, "Error polling for client errors");
                // Continue polling despite errors - send heartbeat
                let event = Event::default().comment("heartbeat");
                Some((Ok(event), state))
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Validate the client error report
fn validate_report(report: &ClientErrorReport) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Check message is not empty
    if report.message.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("message is required").with_code("BAD_REQUEST")),
        ));
    }

    // Check message length
    if report.message.len() > MAX_MESSAGE_LENGTH {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new(format!(
                    "message exceeds maximum length of {} characters",
                    MAX_MESSAGE_LENGTH
                ))
                .with_code("BAD_REQUEST"),
            ),
        ));
    }

    // Check error_type is not empty
    if report.error_type.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("error_type is required").with_code("BAD_REQUEST")),
        ));
    }

    // Validate timestamp format (ISO 8601)
    if chrono::DateTime::parse_from_rfc3339(&report.timestamp).is_err() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("timestamp must be in ISO 8601 format")
                    .with_code("BAD_REQUEST"),
            ),
        ));
    }

    Ok(())
}

/// Truncate message for logging (avoid huge log entries)
fn truncate_message(message: &str) -> &str {
    if message.len() <= 500 {
        message
    } else {
        &message[..500]
    }
}

/// Truncate message to max length for storage
fn truncate_to_max(message: &str) -> String {
    if message.len() <= MAX_MESSAGE_LENGTH {
        message.to_string()
    } else {
        message[..MAX_MESSAGE_LENGTH].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_report_valid() {
        let report = ClientErrorReport {
            error_type: "Network".to_string(),
            message: "Connection failed".to_string(),
            code: Some("NETWORK_ERROR".to_string()),
            failure_code: None,
            http_status: None,
            page: Some("/dashboard".to_string()),
            user_agent: "Mozilla/5.0".to_string(),
            timestamp: "2024-01-08T12:00:00Z".to_string(),
            details: None,
        };
        assert!(validate_report(&report).is_ok());
    }

    #[test]
    fn test_validate_report_empty_message() {
        let report = ClientErrorReport {
            error_type: "Network".to_string(),
            message: "   ".to_string(),
            code: None,
            failure_code: None,
            http_status: None,
            page: None,
            user_agent: "Mozilla/5.0".to_string(),
            timestamp: "2024-01-08T12:00:00Z".to_string(),
            details: None,
        };
        assert!(validate_report(&report).is_err());
    }

    #[test]
    fn test_validate_report_invalid_timestamp() {
        let report = ClientErrorReport {
            error_type: "Network".to_string(),
            message: "Test error".to_string(),
            code: None,
            failure_code: None,
            http_status: None,
            page: None,
            user_agent: "Mozilla/5.0".to_string(),
            timestamp: "invalid-timestamp".to_string(),
            details: None,
        };
        assert!(validate_report(&report).is_err());
    }

    #[test]
    fn test_truncate_message_short() {
        let msg = "Short message";
        assert_eq!(truncate_message(msg), msg);
    }

    #[test]
    fn test_truncate_message_long() {
        let msg = "x".repeat(600);
        assert_eq!(truncate_message(&msg).len(), 500);
    }

    #[test]
    fn test_truncate_to_max_short() {
        let msg = "Short message";
        assert_eq!(truncate_to_max(msg), msg);
    }

    #[test]
    fn test_truncate_to_max_long() {
        let msg = "x".repeat(2500);
        assert_eq!(truncate_to_max(&msg).len(), MAX_MESSAGE_LENGTH);
    }
}
