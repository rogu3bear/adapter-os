//! Token generation and tenant summary helpers
//!
//! Contains utilities for collecting tenant summaries and shared token logic.

use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::auth::TenantSummary;
use axum::http::StatusCode;
use axum::Json;
use std::collections::HashSet;
use tracing::warn;

use super::helpers::ADMIN_TENANT_WILDCARD;

/// Collect tenant summaries for a user based on their role and grants
pub async fn collect_tenant_summaries(
    state: &AppState,
    user_id: &str,
    role: &str,
    active_tenant: &str,
    admin_tenants: &[String],
) -> Result<Vec<TenantSummary>, (StatusCode, Json<ErrorResponse>)> {
    let has_wildcard = admin_tenants.iter().any(|t| t == ADMIN_TENANT_WILDCARD);
    // Wildcard admin: return all tenants
    if role == "admin" && has_wildcard {
        let (all_tenants, _) = state.db.list_tenants_paginated(200, 0).await.map_err(|e| {
            warn!(
                error = %e,
                user_id = %user_id,
                role = %role,
                active_tenant = %active_tenant,
                "Failed to list tenants for admin"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })?;

        let tenants = all_tenants
            .into_iter()
            .map(|t| TenantSummary {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                id: t.id,
                name: t.name,
                status: t.status,
                created_at: Some(t.created_at),
            })
            .collect();

        return Ok(tenants);
    }

    let mut tenant_ids: HashSet<String> = HashSet::new();
    tenant_ids.insert(active_tenant.to_string());

    if role == "admin" && !has_wildcard {
        for t in admin_tenants {
            if t != ADMIN_TENANT_WILDCARD {
                tenant_ids.insert(t.clone());
            }
        }

        if let Ok(db_grants) = adapteros_db::get_user_tenant_access(&state.db, user_id).await {
            for t in db_grants {
                tenant_ids.insert(t);
            }
        }
    }

    let mut tenants: Vec<TenantSummary> = Vec::new();
    for tenant_id in tenant_ids {
        if let Some(t) = state.db.get_tenant(&tenant_id).await.map_err(|e| {
            warn!(
                error = %e,
                tenant_id = %tenant_id,
                user_id = %user_id,
                active_tenant = %active_tenant,
                "Failed to fetch tenant"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("internal error").with_code("INTERNAL_ERROR")),
            )
        })? {
            tenants.push(TenantSummary {
                schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
                id: t.id,
                name: t.name,
                status: t.status,
                created_at: Some(t.created_at),
            });
        }
    }

    Ok(tenants)
}
