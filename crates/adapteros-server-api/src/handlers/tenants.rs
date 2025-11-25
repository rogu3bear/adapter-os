//! Tenant management handlers
//!
//! Provides REST endpoints for tenant CRUD operations, policy assignment,
//! and tenant lifecycle management.
//!
//! 【2025-01-20†modularity†tenant_handlers】

use crate::auth::Claims;
use crate::middleware::require_role;
use crate::state::AppState;
use crate::types::*; // Re-exports adapteros_api_types::*
use adapteros_db::users::Role;
use axum::{extract::Extension, extract::Path, extract::State, http::StatusCode, response::Json};

/// List all tenants
pub async fn list_tenants(
    State(state): State<AppState>,
    Extension(_claims): Extension<Claims>,
) -> Result<Json<Vec<TenantResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let tenants = state.db.list_tenants().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("database error")
                    .with_code("INTERNAL_SERVER_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    let response: Vec<TenantResponse> = tenants
        .into_iter()
        .map(|t| TenantResponse {
            schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
            id: t.id,
            name: t.name,
            itar_flag: t.itar_flag,
            created_at: t.created_at,
            status: "active".to_string(),
        })
        .collect();

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
        id: tenant.id,
        name: tenant.name,
        itar_flag: tenant.itar_flag,
        created_at: tenant.created_at,
        status: "active".to_string(),
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
    // Allow any authenticated user to view default stack
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
    // Allow any authenticated user to set default stack (they can only set for their own tenant)
    // In production, you might want to add tenant_id validation from claims

    state.db.set_default_stack(&tenant_id, &req.stack_id).await.map_err(|e| {
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
    // Allow any authenticated user to clear default stack

    state.db.clear_default_stack(&tenant_id).await.map_err(|e| {
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
