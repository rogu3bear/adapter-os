//! First-class errors API: ErrorInstance + ErrorBucket.

use crate::auth::Claims;
use crate::state::AppState;
use adapteros_api_types::errors::{
    ErrorBucket, ErrorInstance, ErrorKind, ErrorSeverity, ErrorSource, GetErrorResponse,
    ListErrorBucketsQuery, ListErrorBucketsResponse, ListErrorsQuery, ListErrorsResponse,
};
use adapteros_api_types::ErrorResponse;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    Extension,
};

fn source_to_str(s: &ErrorSource) -> &'static str {
    match s {
        ErrorSource::Ui => "ui",
        ErrorSource::Api => "api",
        ErrorSource::Worker => "worker",
    }
}

fn kind_to_str(k: &ErrorKind) -> &'static str {
    match k {
        ErrorKind::Network => "network",
        ErrorKind::Auth => "auth",
        ErrorKind::Validation => "validation",
        ErrorKind::Server => "server",
        ErrorKind::Decode => "decode",
        ErrorKind::Timeout => "timeout",
        ErrorKind::Worker => "worker",
        ErrorKind::Unknown => "unknown",
    }
}

fn severity_to_str(s: &ErrorSeverity) -> &'static str {
    match s {
        ErrorSeverity::Info => "info",
        ErrorSeverity::Warn => "warn",
        ErrorSeverity::Error => "error",
        ErrorSeverity::Fatal => "fatal",
    }
}

fn parse_source(s: &str) -> ErrorSource {
    match s {
        "ui" => ErrorSource::Ui,
        "worker" => ErrorSource::Worker,
        _ => ErrorSource::Api,
    }
}

fn parse_kind(s: &str) -> ErrorKind {
    match s {
        "network" => ErrorKind::Network,
        "auth" => ErrorKind::Auth,
        "validation" => ErrorKind::Validation,
        "decode" => ErrorKind::Decode,
        "timeout" => ErrorKind::Timeout,
        "worker" => ErrorKind::Worker,
        "server" => ErrorKind::Server,
        _ => ErrorKind::Unknown,
    }
}

fn parse_severity(s: &str) -> ErrorSeverity {
    match s {
        "info" => ErrorSeverity::Info,
        "warn" => ErrorSeverity::Warn,
        "fatal" => ErrorSeverity::Fatal,
        _ => ErrorSeverity::Error,
    }
}

fn to_instance(row: adapteros_db::errors::ErrorInstanceRow) -> ErrorInstance {
    ErrorInstance {
        id: row.id,
        created_at_unix_ms: row.created_at_unix_ms,
        tenant_id: row.tenant_id,
        source: parse_source(&row.source),
        error_code: row.error_code,
        kind: parse_kind(&row.kind),
        severity: parse_severity(&row.severity),
        message_user: row.message_user,
        message_dev: row.message_dev,
        fingerprint: row.fingerprint,
        tags_json: row.tags_json,
        session_id: row.session_id,
        request_id: row.request_id,
        diag_trace_id: row.diag_trace_id,
        otel_trace_id: row.otel_trace_id,
        http_method: row.http_method,
        http_path: row.http_path,
        http_status: row.http_status,
        run_id: row.run_id,
        receipt_hash: row.receipt_hash,
        route_digest: row.route_digest,
    }
}

fn to_bucket(row: adapteros_db::errors::ErrorBucketRow) -> ErrorBucket {
    ErrorBucket {
        fingerprint: row.fingerprint,
        tenant_id: row.tenant_id,
        error_code: row.error_code,
        kind: parse_kind(&row.kind),
        severity: parse_severity(&row.severity),
        first_seen_unix_ms: row.first_seen_unix_ms,
        last_seen_unix_ms: row.last_seen_unix_ms,
        count: row.count,
        sample_error_ids_json: row.sample_error_ids_json,
    }
}

/// GET /v1/errors/{error_id} - Get a single persisted error instance (tenant-scoped)
#[utoipa::path(
    get,
    path = "/v1/errors/{error_id}",
    params(
        ("error_id" = String, Path, description = "Error instance ID (err-...)")
    ),
    responses(
        (status = 200, description = "Error instance", body = GetErrorResponse),
        (status = 404, description = "Not found"),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "errors",
    security(("bearer_token" = []))
)]
pub async fn get_error_instance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(error_id): Path<String>,
) -> Result<Json<GetErrorResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = state
        .db
        .get_error_instance(&claims.tenant_id, &error_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, error_id = %error_id, "Failed to get error instance");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to retrieve error").with_code("DATABASE_ERROR")),
            )
        })?;

    match row {
        Some(r) => Ok(Json(GetErrorResponse {
            item: to_instance(r),
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Error not found").with_code("NOT_FOUND")),
        )),
    }
}

/// GET /v1/errors - List persisted error instances (tenant-scoped)
#[utoipa::path(
    get,
    path = "/v1/errors",
    params(ListErrorsQuery),
    responses(
        (status = 200, description = "Error instances list", body = ListErrorsResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "errors",
    security(("bearer_token" = []))
)]
pub async fn list_error_instances(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(q): Query<ListErrorsQuery>,
) -> Result<Json<ListErrorsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let dbq = adapteros_db::errors::ListErrorsDbQuery {
        tenant_id: claims.tenant_id.clone(),
        since_unix_ms: q.since_unix_ms,
        until_unix_ms: q.until_unix_ms,
        limit: q.limit,
        after_created_at_unix_ms: q.after_created_at_unix_ms,
        error_code: q.error_code,
        fingerprint: q.fingerprint,
        request_id: q.request_id,
        diag_trace_id: q.diag_trace_id,
        session_id: q.session_id,
        source: q.source.as_ref().map(|s| source_to_str(s).to_string()),
        severity: q.severity.as_ref().map(|s| severity_to_str(s).to_string()),
        kind: q.kind.as_ref().map(|s| kind_to_str(s).to_string()),
    };

    let rows = state.db.list_error_instances(&dbq).await.map_err(|e| {
        tracing::error!(error = %e, "Failed to list error instances");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to retrieve errors").with_code("DATABASE_ERROR")),
        )
    })?;

    Ok(Json(ListErrorsResponse {
        items: rows.into_iter().map(to_instance).collect(),
    }))
}

/// GET /v1/error-buckets - List error buckets (fingerprint groups)
#[utoipa::path(
    get,
    path = "/v1/error-buckets",
    params(ListErrorBucketsQuery),
    responses(
        (status = 200, description = "Error buckets list", body = ListErrorBucketsResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "errors",
    security(("bearer_token" = []))
)]
pub async fn list_error_buckets(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(q): Query<ListErrorBucketsQuery>,
) -> Result<Json<ListErrorBucketsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let rows = state
        .db
        .list_error_buckets(
            &claims.tenant_id,
            q.limit.unwrap_or(50),
            q.error_code.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to list error buckets");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Failed to retrieve buckets").with_code("DATABASE_ERROR")),
            )
        })?;

    Ok(Json(ListErrorBucketsResponse {
        items: rows.into_iter().map(to_bucket).collect(),
    }))
}
