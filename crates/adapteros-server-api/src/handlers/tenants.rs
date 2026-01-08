//! Tenant management handlers
//!
//! Provides REST endpoints for tenant CRUD operations, policy assignment,
//! and tenant lifecycle management.
//!
//! 【2025-01-20†modularity†tenant_handlers】

use super::utils::aos_error_to_response;
use crate::auth::Claims;
use crate::error_helpers::{db_error_msg, db_error_with_details};
use crate::handlers::event_applier::{apply_event, parse_event, TenantEvent};
use crate::middleware::require_role;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*; // Re-exports adapteros_api_types::*
use adapteros_core::tenant_snapshot::TenantStateSnapshot;
use adapteros_core::AosError;
use adapteros_db::users::Role;
use axum::{
    extract::Extension, extract::Path, extract::Query, extract::State, http::StatusCode,
    response::Json,
};
use serde_json::Value;

/// List all tenants with pagination
pub async fn list_tenants(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(pagination): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<TenantResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // SECURITY FIX: Require Admin role to list all tenants
    // This prevents any authenticated user from seeing all tenant information
    require_role(&claims, Role::Admin)?;

    let offset = (pagination.page.saturating_sub(1)) * pagination.limit;
    let (tenants, total) = state
        .db
        .list_tenants_paginated(pagination.limit as i64, offset as i64)
        .await
        .map_err(db_error_with_details)?;

    let data: Vec<TenantResponse> = tenants
        .into_iter()
        .map(|t| TenantResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: t.id.clone(),
            name: t.name,
            itar_flag: t.itar_flag,
            created_at: t.created_at,
            status: t.status.unwrap_or_else(|| "active".to_string()),
            updated_at: t.updated_at,
            default_stack_id: t.default_stack_id,
            max_adapters: t.max_adapters,
            max_training_jobs: t.max_training_jobs,
            max_storage_gb: t.max_storage_gb,
            rate_limit_rpm: t.rate_limit_rpm,
        })
        .collect();

    let pages = ((total as f64) / (pagination.limit as f64)).ceil() as u32;
    let response = PaginatedResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        data,
        total: total as u64,
        page: pagination.page,
        limit: pagination.limit,
        pages,
    };

    Ok(Json(response))
}

/// Create tenant (admin only)
pub async fn create_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require TenantManage permission (Admin role has this)
    require_permission(&claims, Permission::TenantManage)?;

    let id = state
        .db
        .create_tenant(&req.name, req.itar_flag)
        .await
        .map_err(|e| db_error_msg("failed to create tenant", e))?;

    let tenant = state
        .db
        .get_tenant(&id)
        .await
        .map_err(db_error_with_details)?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("tenant not found after creation")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!(
                        "Tenant '{}' was created but could not be retrieved",
                        id
                    )),
            ),
        )
    })?;

    // Audit log: tenant created
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_CREATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant.id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(TenantResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: tenant.id.clone(),
        name: tenant.name,
        itar_flag: tenant.itar_flag,
        created_at: tenant.created_at,
        status: tenant.status.unwrap_or_else(|| "active".to_string()),
        updated_at: tenant.updated_at,
        default_stack_id: tenant.default_stack_id,
        max_adapters: tenant.max_adapters,
        max_training_jobs: tenant.max_training_jobs,
        max_storage_gb: tenant.max_storage_gb,
        rate_limit_rpm: tenant.rate_limit_rpm,
    }))
}

/// Get default stack for a tenant
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/default-stack",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Default stack retrieved", body = DefaultStackResponse),
        (status = 404, description = "No default stack set", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn get_default_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<DefaultStackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // SECURITY FIX: Validate tenant isolation before accessing tenant data
    // Users can only view default stack for their own tenant (or admin with explicit access)
    validate_tenant_isolation(&claims, &tenant_id)?;

    let stack_id = state
        .db
        .get_default_stack(&tenant_id)
        .await
        .map_err(db_error_with_details)?;

    let stack_id = stack_id.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(
                ErrorResponse::new("No default stack set for tenant")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!(
                        "Tenant '{}' does not have a default stack configured",
                        tenant_id
                    )),
            ),
        )
    })?;

    Ok(Json(DefaultStackResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        tenant_id,
        stack_id,
    }))
}

/// Set default stack for a tenant
#[utoipa::path(
    put,
    path = "/v1/tenants/{tenant_id}/default-stack",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    request_body = SetDefaultStackRequest,
    responses(
        (status = 200, description = "Default stack set", body = DefaultStackResponse),
        (status = 404, description = "Stack not found", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn set_default_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<SetDefaultStackRequest>,
) -> Result<Json<DefaultStackResponse>, (StatusCode, Json<ErrorResponse>)> {
    // SECURITY FIX: Validate tenant isolation before modifying tenant data
    // Users can only set default stack for their own tenant (or admin with explicit access)
    validate_tenant_isolation(&claims, &tenant_id)?;

    state
        .db
        .set_default_stack(&tenant_id, &req.stack_id)
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            if error_str.contains("not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(
                        ErrorResponse::new("Stack not found for tenant")
                            .with_code("NOT_FOUND")
                            .with_string_details(format!(
                                "Stack '{}' does not exist for tenant '{}'",
                                req.stack_id, tenant_id
                            )),
                    ),
                )
            } else {
                db_error_with_details(error_str)
            }
        })?;

    // Audit log: default stack set
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(DefaultStackResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        tenant_id,
        stack_id: req.stack_id,
    }))
}

/// Clear default stack for a tenant
#[utoipa::path(
    delete,
    path = "/v1/tenants/{tenant_id}/default-stack",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 204, description = "Default stack cleared"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn clear_default_stack(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // SECURITY FIX: Validate tenant isolation before modifying tenant data
    // Users can only clear default stack for their own tenant (or admin with explicit access)
    validate_tenant_isolation(&claims, &tenant_id)?;

    state
        .db
        .clear_default_stack(&tenant_id)
        .await
        .map_err(db_error_with_details)?;

    // Audit log: default stack cleared
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Update tenant (admin only)
#[utoipa::path(
    put,
    path = "/v1/tenants/{tenant_id}",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    request_body = UpdateTenantRequest,
    responses(
        (status = 200, description = "Tenant updated", body = TenantResponse),
        (status = 404, description = "Tenant not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn update_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<UpdateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    // Update name if provided
    if let Some(name) = req.name {
        state
            .db
            .rename_tenant(&tenant_id, &name)
            .await
            .map_err(|e| db_error_msg("failed to update tenant name", e))?;
    }

    // Update ITAR flag if provided
    if let Some(itar_flag) = req.itar_flag {
        state
            .db
            .update_tenant_itar_flag(&tenant_id, itar_flag)
            .await
            .map_err(|e| db_error_msg("failed to update ITAR flag", e))?;
    }

    // Update limits if any provided
    if req.max_adapters.is_some()
        || req.max_training_jobs.is_some()
        || req.max_storage_gb.is_some()
        || req.rate_limit_rpm.is_some()
    {
        state
            .db
            .update_tenant_limits(
                &tenant_id,
                req.max_adapters,
                req.max_training_jobs,
                req.max_storage_gb,
                req.rate_limit_rpm,
            )
            .await
            .map_err(|e| db_error_msg("failed to update tenant limits", e))?;
    }

    // Fetch updated tenant
    let tenant = state
        .db
        .get_tenant(&tenant_id)
        .await
        .map_err(db_error_with_details)?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("tenant not found").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: tenant updated
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant.id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(TenantResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: tenant.id,
        name: tenant.name,
        itar_flag: tenant.itar_flag,
        created_at: tenant.created_at,
        status: tenant.status.unwrap_or_else(|| "active".to_string()),
        updated_at: tenant.updated_at,
        default_stack_id: tenant.default_stack_id,
        max_adapters: tenant.max_adapters,
        max_training_jobs: tenant.max_training_jobs,
        max_storage_gb: tenant.max_storage_gb,
        rate_limit_rpm: tenant.rate_limit_rpm,
    }))
}

/// Pause tenant (admin only)
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/pause",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Tenant paused", body = TenantResponse),
        (status = 404, description = "Tenant not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn pause_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    state
        .db
        .pause_tenant(&tenant_id)
        .await
        .map_err(|e| db_error_msg("failed to pause tenant", e))?;

    // Invalidate tenant from dashboard cache so middleware re-validates on next request
    state.dashboard_cache.invalidate_tenant(&tenant_id).await;

    let tenant = state
        .db
        .get_tenant(&tenant_id)
        .await
        .map_err(db_error_with_details)?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("tenant not found").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: tenant paused
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant.id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(TenantResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: tenant.id,
        name: tenant.name,
        itar_flag: tenant.itar_flag,
        created_at: tenant.created_at,
        status: tenant.status.unwrap_or_else(|| "paused".to_string()),
        updated_at: tenant.updated_at,
        default_stack_id: tenant.default_stack_id,
        max_adapters: tenant.max_adapters,
        max_training_jobs: tenant.max_training_jobs,
        max_storage_gb: tenant.max_storage_gb,
        rate_limit_rpm: tenant.rate_limit_rpm,
    }))
}

/// Archive tenant (admin only)
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/archive",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Tenant archived", body = TenantResponse),
        (status = 404, description = "Tenant not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn archive_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require TenantManage permission (Admin role has this)
    require_permission(&claims, Permission::TenantManage)?;

    state
        .db
        .archive_tenant(&tenant_id)
        .await
        .map_err(|e| db_error_msg("failed to archive tenant", e))?;

    // Invalidate tenant from dashboard cache so middleware rejects stale tokens
    state.dashboard_cache.invalidate_tenant(&tenant_id).await;

    let tenant = state
        .db
        .get_tenant(&tenant_id)
        .await
        .map_err(db_error_with_details)?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("tenant not found").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: tenant archived
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant.id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(Json(TenantResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        id: tenant.id,
        name: tenant.name,
        itar_flag: tenant.itar_flag,
        created_at: tenant.created_at,
        status: tenant.status.unwrap_or_else(|| "archived".to_string()),
        updated_at: tenant.updated_at,
        default_stack_id: tenant.default_stack_id,
        max_adapters: tenant.max_adapters,
        max_training_jobs: tenant.max_training_jobs,
        max_storage_gb: tenant.max_storage_gb,
        rate_limit_rpm: tenant.rate_limit_rpm,
    }))
}

/// Get tenant usage statistics
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/usage",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Tenant usage statistics", body = TenantUsageResponse),
        (status = 404, description = "Tenant not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn get_tenant_usage(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TenantUsageResponse>, (StatusCode, Json<ErrorResponse>)> {
    // SECURITY FIX: Validate tenant isolation before accessing usage data
    // Users can only view usage for their own tenant (or admin with explicit access)
    validate_tenant_isolation(&claims, &tenant_id)?;

    let usage = state
        .db
        .get_tenant_usage(&tenant_id)
        .await
        .map_err(|e| db_error_msg("failed to get tenant usage", e))?;

    Ok(Json(TenantUsageResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        tenant_id: usage.tenant_id,
        cpu_usage_pct: usage.cpu_usage_pct,
        gpu_usage_pct: usage.gpu_usage_pct,
        memory_used_gb: usage.memory_used_gb,
        memory_total_gb: usage.memory_total_gb,
        inference_count_24h: usage.inference_count_24h,
        active_adapters_count: usage.active_adapters_count,
        avg_latency_ms: None,
        estimated_cost_usd: None,
    }))
}

/// Assign policies to tenant (admin only)
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/policies",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    request_body = AssignPoliciesRequest,
    responses(
        (status = 200, description = "Policies assigned successfully"),
        (status = 404, description = "Tenant not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn assign_policies_to_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignPoliciesRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    let assigned_by = claims.sub.clone();

    for policy_id in req.policy_ids {
        state
            .db
            .assign_policy_to_tenant(&tenant_id, &policy_id, &assigned_by)
            .await
            .map_err(|e| db_error_msg("failed to assign policy", e))?;
    }

    // Audit log: policies assigned
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(StatusCode::OK)
}

/// Assign adapters to tenant (admin only)
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/adapters",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    request_body = AssignAdaptersRequest,
    responses(
        (status = 200, description = "Adapters assigned successfully"),
        (status = 404, description = "Tenant not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn assign_adapters_to_tenant(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
    Json(req): Json<AssignAdaptersRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    let assigned_by = claims.sub.clone();

    for adapter_id in req.adapter_ids {
        state
            .db
            .assign_adapter_to_tenant(&tenant_id, &adapter_id, &assigned_by)
            .await
            .map_err(|e| db_error_msg("failed to assign adapter", e))?;
    }

    // Audit log: adapters assigned
    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    Ok(StatusCode::OK)
}

/// Revoke all tokens for a tenant - PRD-03
///
/// Sets the token_issued_at_min to current timestamp, invalidating all tokens
/// issued before this time. This is a high-impact security action.
///
/// POST /v1/tenants/{tenant_id}/revoke-all-tokens
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/revoke-all-tokens",
    tag = "tenants",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "All tokens revoked", body = TokenRevocationResponse),
        (status = 403, description = "Permission denied", body = ErrorResponse),
        (status = 404, description = "Tenant not found", body = ErrorResponse)
    )
)]
pub async fn revoke_tenant_tokens(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<TokenRevocationResponse>, (StatusCode, Json<ErrorResponse>)> {
    use crate::permissions::{require_permission, Permission};
    use crate::security::{set_tenant_token_baseline, validate_tenant_isolation};
    use chrono::Utc;

    // Require new permission
    require_permission(&claims, Permission::TenantTokenRevoke).map_err(|_e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Validate tenant access (admin can only revoke for tenants they manage)
    validate_tenant_isolation(&claims, &tenant_id)?;

    // Verify tenant exists
    let tenant_exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tenants WHERE id = ?")
        .bind(&tenant_id)
        .fetch_one(state.db.pool())
        .await
        .map_err(|_e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Database error").with_code("DATABASE_ERROR")),
            )
        })?;

    if tenant_exists == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Tenant not found").with_code("NOT_FOUND")),
        ));
    }

    // Set baseline to now
    let baseline = Utc::now().to_rfc3339();
    set_tenant_token_baseline(&state.db, &tenant_id, &baseline)
        .await
        .map_err(|_e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to set token baseline").with_code("DATABASE_ERROR"),
                ),
            )
        })?;

    // Audit log
    tracing::info!(
        tenant_id = %tenant_id,
        actor = %claims.sub,
        baseline = %baseline,
        "Tenant-wide token revocation executed"
    );

    Ok(Json(TokenRevocationResponse {
        revoked_at: baseline,
        message: "All tokens issued before this timestamp are now invalid".to_string(),
    }))
}

// Aliases for backwards compatibility with existing routes
pub use assign_adapters_to_tenant as assign_tenant_adapters;
pub use assign_policies_to_tenant as assign_tenant_policies;

/// Hydrate tenant from telemetry bundle
pub async fn hydrate_tenant_from_bundle(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<HydrateTenantRequest>,
) -> Result<Json<TenantHydrationResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_role(&claims, Role::Admin)?;

    let events = state
        .telemetry_bundle_store
        .read()
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    "Failed to acquire lock on telemetry bundle store",
                )),
            )
        })?
        .get_bundle_events(&req.bundle_id)
        .map_err(|e: AosError| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    // Sort events canonical: timestamp asc, then event_type asc
    let mut sorted_events: Vec<&serde_json::Value> = events.iter().collect();
    sorted_events.sort_by(|e1: &&serde_json::Value, e2: &&serde_json::Value| {
        let ts1 = e1
            .get("timestamp")
            .and_then(|v: &serde_json::Value| v.as_i64())
            .unwrap_or(0);
        let ts2 = e2
            .get("timestamp")
            .and_then(|v: &serde_json::Value| v.as_i64())
            .unwrap_or(0);
        ts1.cmp(&ts2).then_with(|| {
            e1.get("event_type")
                .and_then(|v: &serde_json::Value| v.as_str())
                .unwrap_or("")
                .cmp(
                    e2.get("event_type")
                        .and_then(|v: &serde_json::Value| v.as_str())
                        .unwrap_or(""),
                )
        })
    });

    let events_vec: Vec<serde_json::Value> = sorted_events.iter().cloned().cloned().collect();
    let sim_snapshot = TenantStateSnapshot::from_bundle_events(&events_vec);
    let sim_hash = sim_snapshot.compute_hash();

    let typed_events: Vec<TenantEvent> = sorted_events
        .iter()
        .map(|event| {
            parse_event(event).map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(format!("Invalid event: {}", err))),
                )
            })
        })
        .collect::<Result<_, _>>()?;

    if req.dry_run {
        if let Some(expected) = &req.expected_state_hash {
            if expected != &sim_hash.to_hex() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(
                        "Computed state hash does not match expected",
                    )),
                ));
            }
        }
        return Ok(Json(TenantHydrationResponse {
            tenant_id: req.tenant_id.clone(),
            state_hash: sim_hash.to_hex(),
            status: "dry_run_success".to_string(),
            errors: vec![],
        }));
    }

    // Full hydration
    let current_opt = state
        .db
        .get_tenant_snapshot_hash(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    if let Some(current_hash) = current_opt {
        if current_hash != sim_hash {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse::new(
                    "Tenant state mismatch: cannot hydrate non-idempotently",
                )),
            ));
        }
        // Already hydrated with same bundle, idempotent ok
        tracing::info!(
            "Tenant {} already hydrated with matching state hash {}",
            req.tenant_id,
            sim_hash
        );
        let tenant = state
            .db
            .get_tenant(&req.tenant_id)
            .await
            .map_err(|e| {
                aos_error_to_response(AosError::Database(format!("Failed to get tenant: {}", e)))
            })?
            .ok_or_else(|| {
                aos_error_to_response(AosError::NotFound(format!(
                    "Tenant {} not found",
                    req.tenant_id
                )))
            })?;
        return Ok(Json(TenantHydrationResponse {
            tenant_id: req.tenant_id.clone(),
            state_hash: sim_hash.to_hex(),
            status: "already_hydrated".to_string(),
            errors: vec![],
        }));
    }

    // New tenant or mismatch (but mismatch already errored), create and apply
    let tenant_exists = state
        .db
        .get_tenant(&req.tenant_id)
        .await
        .map_err(|e| {
            aos_error_to_response(AosError::Database(format!(
                "Failed to check tenant existence: {}",
                e
            )))
        })?
        .is_some();

    if !tenant_exists {
        state
            .db
            .create_tenant(&req.tenant_id, false)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(e.to_string())),
                )
            })?;
    }

    // Apply in transaction
    let mut tx = state.db.pool().begin().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    for event in &typed_events {
        if let Err(e) = apply_event(&mut tx, &req.tenant_id, event).await {
            tracing::error!(
                identity = ?event.identity_label(),
                error = %e,
                "Failed to apply event in hydration"
            );
            let _ = tx.rollback().await;
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(format!(
                    "Hydration failed on event: {}",
                    e
                ))),
            ));
        }
    }

    tx.commit().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e.to_string())),
        )
    })?;

    // Build and store snapshot
    let snapshot = state
        .db
        .build_tenant_snapshot(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    let final_hash = snapshot.compute_hash();
    // Verify matches sim
    if final_hash != sim_hash {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(
                "Post-hydration state hash mismatch (internal error)",
            )),
        ));
    }

    state
        .db
        .store_tenant_snapshot_hash(&req.tenant_id, &final_hash)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    // Rebuild indexes
    state
        .db
        .rebuild_all_indexes(&req.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e.to_string())),
            )
        })?;

    let tenant = state
        .db
        .get_tenant(&req.tenant_id)
        .await
        .map_err(|e| {
            aos_error_to_response(AosError::Database(format!("Failed to get tenant: {}", e)))
        })?
        .ok_or_else(|| {
            aos_error_to_response(AosError::NotFound(format!(
                "Tenant {} not found",
                req.tenant_id
            )))
        })?;

    Ok(Json(TenantHydrationResponse {
        tenant_id: req.tenant_id.clone(),
        state_hash: final_hash.to_hex(),
        status: "hydrated".to_string(),
        errors: vec![],
    }))
}

// Define response
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct TenantHydrationResponse {
    pub tenant_id: String,
    pub state_hash: String,
    pub status: String,
    pub errors: Vec<String>,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct HydrateTenantRequest {
    pub bundle_id: String,
    pub tenant_id: String,
    pub dry_run: bool,
    pub expected_state_hash: Option<String>,
}

// Re-export tenant handler path types from parent module for OpenAPI
pub use super::__path_hydrate_tenant_from_bundle;
