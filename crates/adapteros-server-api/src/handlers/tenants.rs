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
use axum::{extract::Extension, extract::State, http::StatusCode, response::Json};

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
