//! Execution policy handlers
//!
//! Provides REST endpoints for managing tenant execution policies
//! that govern determinism, routing, and golden verification.
//!
//! Endpoints:
//! - GET /v1/tenants/{tenant_id}/execution-policy - Get active policy
//! - POST /v1/tenants/{tenant_id}/execution-policy - Create new policy
//! - DELETE /v1/tenants/{tenant_id}/execution-policy/{policy_id} - Deactivate policy
//! - GET /v1/tenants/{tenant_id}/execution-policy/history - Get policy history

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_api_types::{CreateExecutionPolicyRequest, TenantExecutionPolicy};
use axum::{
    extract::Extension, extract::Path, extract::Query, extract::State, http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use tracing::info;
use utoipa::ToSchema;

/// Query parameters for policy history
#[derive(Debug, Deserialize)]
pub struct PolicyHistoryQuery {
    /// Maximum number of policies to return (default: 10)
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

/// Get the active execution policy for a tenant
///
/// Returns the currently active execution policy. If no explicit policy
/// is configured, returns a permissive default policy (marked with is_implicit=true).
///
/// # Errors
/// - 401: Unauthorized - missing or invalid authentication
/// - 403: Forbidden - tenant isolation violation
/// - 500: Internal server error
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/execution-policy",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Active execution policy", body = TenantExecutionPolicy),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "execution-policy"
)]
pub async fn get_execution_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantExecutionPolicy>, (StatusCode, Json<ErrorResponse>)> {
    // Validate tenant isolation
    validate_tenant_isolation(&claims, &tenant_id)?;

    let policy = state
        .db
        .get_execution_policy_or_default(&tenant_id)
        .await
        .map_err(ApiError::db_error)?;

    Ok(Json(policy))
}

/// Create a new execution policy for a tenant
///
/// Creates a new policy version, deactivating any existing active policy.
/// Policy versions are tracked for audit trail.
///
/// # Errors
/// - 400: Bad Request - invalid policy configuration
/// - 401: Unauthorized - missing or invalid authentication
/// - 403: Forbidden - tenant isolation violation
/// - 500: Internal server error
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/execution-policy",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    request_body = CreateExecutionPolicyRequest,
    responses(
        (status = 200, description = "Policy created", body = CreatePolicyResponse),
        (status = 400, description = "Invalid policy configuration", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "execution-policy"
)]
pub async fn create_execution_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(request): Json<CreateExecutionPolicyRequest>,
) -> Result<Json<CreatePolicyResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate tenant isolation
    validate_tenant_isolation(&claims, &tenant_id)?;

    // Get user ID for audit trail
    let created_by = Some(claims.sub.as_str());

    let policy_id = state
        .db
        .create_execution_policy(&tenant_id, request, created_by)
        .await
        .map_err(ApiError::db_error)?;

    info!(
        tenant_id = %tenant_id,
        policy_id = %policy_id,
        created_by = ?created_by,
        "Execution policy created"
    );

    Ok(Json(CreatePolicyResponse { policy_id }))
}

/// Deactivate an execution policy
///
/// Deactivates the specified policy. The tenant will revert to the next
/// most recent active policy, or the permissive default if none exists.
///
/// # Errors
/// - 401: Unauthorized - missing or invalid authentication
/// - 403: Forbidden - tenant isolation violation
/// - 404: Not Found - policy not found or already inactive
/// - 500: Internal server error
#[utoipa::path(
    delete,
    path = "/v1/tenants/{tenant_id}/execution-policy/{policy_id}",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
        ("policy_id" = String, Path, description = "Policy ID to deactivate")
    ),
    responses(
        (status = 204, description = "Policy deactivated"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Policy not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "execution-policy"
)]
pub async fn deactivate_execution_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((tenant_id, policy_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Validate tenant isolation
    validate_tenant_isolation(&claims, &tenant_id)?;

    let deactivated = state
        .db
        .deactivate_execution_policy(&policy_id)
        .await
        .map_err(ApiError::db_error)?;

    if deactivated {
        info!(
            tenant_id = %tenant_id,
            policy_id = %policy_id,
            "Execution policy deactivated"
        );
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Policy not found or already inactive")),
        ))
    }
}

/// Get execution policy history for a tenant
///
/// Returns all policy versions (active and inactive) ordered by version
/// descending (most recent first).
///
/// # Errors
/// - 401: Unauthorized - missing or invalid authentication
/// - 403: Forbidden - tenant isolation violation
/// - 500: Internal server error
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/execution-policy/history",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of policies to return (default: 10)")
    ),
    responses(
        (status = 200, description = "Policy history", body = PolicyHistoryResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "execution-policy"
)]
pub async fn get_execution_policy_history(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Query(query): Query<PolicyHistoryQuery>,
) -> Result<Json<PolicyHistoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate tenant isolation
    validate_tenant_isolation(&claims, &tenant_id)?;

    let policies = state
        .db
        .get_execution_policy_history(&tenant_id, query.limit)
        .await
        .map_err(ApiError::db_error)?;

    Ok(Json(PolicyHistoryResponse { policies }))
}

/// Response for policy creation
#[derive(Debug, serde::Serialize, ToSchema)]
pub struct CreatePolicyResponse {
    pub policy_id: String,
}

/// Response for policy history
#[derive(Debug, serde::Serialize, ToSchema)]
pub struct PolicyHistoryResponse {
    pub policies: Vec<TenantExecutionPolicy>,
}
