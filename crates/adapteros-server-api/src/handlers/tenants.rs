//! Tenant management handlers
//!
//! Provides REST endpoints for tenant CRUD operations, policy assignment,
//! and tenant lifecycle management.
//!
//! 【2025-01-20†modularity†tenant_handlers】

use crate::auth::Claims;
use crate::middleware::require_role;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*; // Re-exports adapteros_api_types::*
use adapteros_db::users::Role;
use axum::{extract::Extension, extract::Path, extract::Query, extract::State, http::StatusCode, response::Json};

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
    require_role(&claims, Role::Admin)?;

    let id = state
        .db
        .create_tenant(&req.name, req.itar_flag)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("failed to create tenant")
                        .with_code("INTERNAL_SERVER_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let tenant = state.db.get_tenant(&id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("tenant not found after creation").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: tenant created
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_CREATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant.id),
    )
    .await;

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

    let stack_id = state.db.get_default_stack(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let stack_id = stack_id.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("No default stack set for tenant").with_code("NOT_FOUND")),
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
                    Json(ErrorResponse::new("Stack not found for tenant").with_code("NOT_FOUND")),
                )
            } else {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("database error")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(error_str),
                    ),
                )
            }
        })?;

    // Audit log: default stack set
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await;

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

    // Audit log: default stack cleared
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await;

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
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update tenant name")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    // Update ITAR flag if provided
    if let Some(itar_flag) = req.itar_flag {
        state
            .db
            .update_tenant_itar_flag(&tenant_id, itar_flag)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update ITAR flag")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
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
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to update tenant limits")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    // Fetch updated tenant
    let tenant = state.db.get_tenant(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("tenant not found").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: tenant updated
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant.id),
    )
    .await;

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

    state.db.pause_tenant(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to pause tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Invalidate tenant from dashboard cache so middleware re-validates on next request
    state.dashboard_cache.invalidate_tenant(&tenant_id).await;

    let tenant = state.db.get_tenant(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("tenant not found").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: tenant paused
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant.id),
    )
    .await;

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
    require_role(&claims, Role::Admin)?;

    state.db.archive_tenant(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to archive tenant")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Invalidate tenant from dashboard cache so middleware rejects stale tokens
    state.dashboard_cache.invalidate_tenant(&tenant_id).await;

    let tenant = state.db.get_tenant(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let tenant = tenant.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("tenant not found").with_code("NOT_FOUND")),
        )
    })?;

    // Audit log: tenant archived
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant.id),
    )
    .await;

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

    let usage = state.db.get_tenant_usage(&tenant_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("failed to get tenant usage")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

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
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to assign policy")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    // Audit log: policies assigned
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await;

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
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("failed to assign adapter")
                            .with_code("INTERNAL_SERVER_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
    }

    // Audit log: adapters assigned
    let _ = crate::audit_helper::log_success(
        &state.db,
        &claims,
        crate::audit_helper::actions::TENANT_UPDATE,
        crate::audit_helper::resources::TENANT,
        Some(&tenant_id),
    )
    .await;

    Ok(StatusCode::OK)
}

// Aliases for backwards compatibility with existing routes
pub use assign_adapters_to_tenant as assign_tenant_adapters;
pub use assign_policies_to_tenant as assign_tenant_policies;
