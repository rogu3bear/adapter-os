use crate::auth::Claims;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::{TenantListResponse, TenantSummary};
use adapteros_api_types::API_SCHEMA_VERSION;
use axum::{extract::State, http::StatusCode, Extension, Json};

/// List tenants accessible to the current user
///
/// In real auth mode, this returns the user's tenant (from claims) and any
/// tenants they have been granted access to. For admins with wildcard access,
/// it returns all tenants.
#[utoipa::path(
    get,
    path = "/v1/auth/tenants",
    responses(
        (status = 200, description = "List of accessible tenants", body = TenantListResponse),
        (status = 500, description = "Database error")
    ),
    tag = "auth"
)]
pub async fn list_user_tenants_handler(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<TenantListResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Check if user has admin wildcard access to all tenants
    let has_wildcard = claims.admin_tenants.iter().any(|t| t == "*");

    let tenants = if has_wildcard {
        // Admin with wildcard: return all tenants
        let (db_tenants, _total) = state.db.list_tenants_paginated(100, 0).await.map_err(|e| {
            // Log detailed error internally for debugging
            tracing::error!(error = %e, user_id = %claims.sub, "Failed to list tenants for admin");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                // Return generic message to user - don't leak database details
                Json(ErrorResponse::new("Failed to retrieve tenants").with_code("DATABASE_ERROR")),
            )
        })?;

        db_tenants
            .into_iter()
            .map(|t| TenantSummary {
                schema_version: API_SCHEMA_VERSION.to_string(),
                id: t.id,
                name: t.name,
                status: t.status,
                created_at: Some(t.created_at),
            })
            .collect()
    } else {
        // Regular user: return their primary tenant + any granted access
        let mut tenant_summaries = Vec::new();

        // Add the user's primary tenant
        if let Ok(Some(primary)) = state.db.get_tenant(&claims.tenant_id).await {
            tenant_summaries.push(TenantSummary {
                schema_version: API_SCHEMA_VERSION.to_string(),
                id: primary.id,
                name: primary.name,
                status: primary.status,
                created_at: Some(primary.created_at),
            });
        }

        // Add any explicitly granted tenants from admin_tenants
        for tenant_id in &claims.admin_tenants {
            if tenant_id != &claims.tenant_id {
                if let Ok(Some(tenant)) = state.db.get_tenant(tenant_id).await {
                    tenant_summaries.push(TenantSummary {
                        schema_version: API_SCHEMA_VERSION.to_string(),
                        id: tenant.id,
                        name: tenant.name,
                        status: tenant.status,
                        created_at: Some(tenant.created_at),
                    });
                }
            }
        }

        tenant_summaries
    };

    Ok(Json(TenantListResponse {
        schema_version: API_SCHEMA_VERSION.to_string(),
        tenants,
    }))
}
