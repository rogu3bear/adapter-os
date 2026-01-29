//! Tenant policy bindings handlers
//!
//! Provides REST endpoints for managing per-tenant policy pack bindings
//! and querying policy audit decisions.
//!
//! Citation: PRD-06 - Per-tenant policy customization

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::policy_audit::{is_audit_chain_divergence, AUDIT_CHAIN_DIVERGED_CODE};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use utoipa::ToSchema;

/// Tenant policy binding response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TenantPolicyBindingResponse {
    pub id: String,
    pub tenant_id: String,
    pub policy_pack_id: String,
    pub scope: String,
    pub enabled: bool,
    pub created_at: String,
    pub created_by: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

/// Toggle policy request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct TogglePolicyRequest {
    pub enabled: bool,
}

/// Policy audit decision response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct PolicyAuditDecision {
    pub id: String,
    pub tenant_id: String,
    pub policy_pack_id: String,
    pub hook: String,
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    pub timestamp: String,
    pub entry_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_hash: Option<String>,
    pub chain_sequence: i64,
}

/// Policy decisions query parameters
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "snake_case")]
pub struct PolicyDecisionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_pack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    100
}

/// Chain verification result
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct ChainVerificationResult {
    pub valid: bool,
    pub total_entries: i64,
    pub verified_entries: i64,
    pub broken_links: Vec<BrokenLink>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
}

/// Broken link in the audit chain
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct BrokenLink {
    pub sequence: i64,
    pub entry_id: String,
    pub expected_hash: String,
    pub actual_hash: String,
}

/// List tenant policy bindings
///
/// Returns all policy pack bindings for a specific tenant.
/// Requires PolicyView permission and respects tenant isolation.
#[utoipa::path(
    get,
    path = "/v1/tenants/{tenant_id}/policy-bindings",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID")
    ),
    responses(
        (status = 200, description = "Policy bindings retrieved", body = Vec<TenantPolicyBindingResponse>),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn list_tenant_policy_bindings(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Vec<TenantPolicyBindingResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Require PolicyView permission
    require_permission(&claims, Permission::PolicyView)?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &tenant_id)?;

    // Query database for policy bindings using Db method (PRD-06)
    let bindings = state
        .db
        .list_tenant_policy_bindings(&tenant_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to list policy bindings").with_details(e.to_string())
        })?;

    let response: Vec<TenantPolicyBindingResponse> = bindings
        .into_iter()
        .map(|b| TenantPolicyBindingResponse {
            id: b.id,
            tenant_id: b.tenant_id,
            policy_pack_id: b.policy_pack_id,
            scope: b.scope,
            enabled: b.enabled,
            created_at: b.created_at,
            created_by: b.created_by,
            updated_at: b.updated_at,
            updated_by: b.updated_by,
        })
        .collect();

    Ok(Json(response))
}

/// Toggle tenant policy
///
/// Enables or disables a specific policy pack for a tenant.
/// Requires PolicyApply permission and respects tenant isolation.
/// **Writes audit record to policy_audit_decisions for Merkle-chain compliance (PRD-06).**
#[utoipa::path(
    post,
    path = "/v1/tenants/{tenant_id}/policy-bindings/{policy_pack_id}/toggle",
    params(
        ("tenant_id" = String, Path, description = "Tenant ID"),
        ("policy_pack_id" = String, Path, description = "Policy pack ID")
    ),
    request_body = TogglePolicyRequest,
    responses(
        (status = 200, description = "Policy toggled successfully", body = TenantPolicyBindingResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 404, description = "Binding not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tenants"
)]
pub async fn toggle_tenant_policy(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((tenant_id, policy_pack_id)): Path<(String, String)>,
    Json(req): Json<TogglePolicyRequest>,
) -> Result<Json<TenantPolicyBindingResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require PolicyApply permission
    require_permission(&claims, Permission::PolicyApply)?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &tenant_id)?;

    // PRD-06: Use Db::toggle_tenant_policy which:
    // 1. Updates the binding state
    // 2. Writes audit record to policy_audit_decisions (Merkle chain)
    // 3. Returns the previous state for audit tracking
    let _previous_enabled = state
        .db
        .toggle_tenant_policy(&tenant_id, &policy_pack_id, req.enabled, &claims.sub)
        .await
        .map_err(|e| {
            if is_audit_chain_divergence(&e) {
                return (
                    StatusCode::CONFLICT,
                    Json(
                        ErrorResponse::new("policy audit chain diverged")
                            .with_code(AUDIT_CHAIN_DIVERGED_CODE)
                            .with_string_details(e.to_string()),
                    ),
                );
            }
            // Check if it's a "not found" case
            if e.to_string().contains("not found") || e.to_string().contains("0 rows") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Policy binding not found").with_code("NOT_FOUND")),
                )
            } else {
                ApiError::internal("failed to toggle policy binding")
                    .with_details(e.to_string())
                    .into()
            }
        })?;

    // Also log to the general audit log for backward compatibility
    let action = if req.enabled {
        crate::audit_helper::actions::POLICY_BINDING_ENABLE
    } else {
        crate::audit_helper::actions::POLICY_BINDING_DISABLE
    };

    if let Err(e) = crate::audit_helper::log_success(
        &state.db,
        &claims,
        action,
        crate::audit_helper::resources::POLICY_BINDING,
        Some(&format!("{}/{}", tenant_id, policy_pack_id)),
    )
    .await
    {
        tracing::warn!(error = %e, "Audit log failed");
    }

    // Fetch and return updated binding
    let bindings = state
        .db
        .list_tenant_policy_bindings(&tenant_id)
        .await
        .map_err(|e| {
            ApiError::internal("failed to fetch updated binding").with_details(e.to_string())
        })?;

    let binding = bindings
        .into_iter()
        .find(|b| b.policy_pack_id == policy_pack_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Policy binding not found after toggle")
                        .with_code("NOT_FOUND"),
                ),
            )
        })?;

    Ok(Json(TenantPolicyBindingResponse {
        id: binding.id,
        tenant_id: binding.tenant_id,
        policy_pack_id: binding.policy_pack_id,
        scope: binding.scope,
        enabled: binding.enabled,
        created_at: binding.created_at,
        created_by: binding.created_by,
        updated_at: binding.updated_at,
        updated_by: binding.updated_by,
    }))
}

/// Query policy decisions
///
/// Returns paginated policy audit decisions with optional filters.
/// Requires AuditView permission.
#[utoipa::path(
    get,
    path = "/v1/audit/policy-decisions",
    params(PolicyDecisionsQuery),
    responses(
        (status = 200, description = "Policy decisions retrieved", body = Vec<PolicyAuditDecision>),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "audit"
)]
pub async fn query_policy_decisions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<PolicyDecisionsQuery>,
) -> Result<Json<Vec<PolicyAuditDecision>>, (StatusCode, Json<ErrorResponse>)> {
    // Require AuditView permission
    require_permission(&claims, Permission::AuditView)?;

    // Build WHERE clause based on filters
    let mut where_clauses = vec![];
    let mut params: Vec<String> = vec![];

    if let Some(ref tenant_id) = query.tenant_id {
        // Validate tenant isolation if tenant_id is specified
        validate_tenant_isolation(&claims, tenant_id)?;
        where_clauses.push("tenant_id = ?");
        params.push(tenant_id.clone());
    } else {
        // If no tenant_id specified, restrict to user's tenant (unless admin)
        validate_tenant_isolation(&claims, &claims.tenant_id)?;
        where_clauses.push("tenant_id = ?");
        params.push(claims.tenant_id.clone());
    }

    if let Some(ref policy_pack_id) = query.policy_pack_id {
        where_clauses.push("policy_pack_id = ?");
        params.push(policy_pack_id.clone());
    }

    if let Some(ref hook) = query.hook {
        where_clauses.push("hook = ?");
        params.push(hook.clone());
    }

    if let Some(ref decision) = query.decision {
        where_clauses.push("decision = ?");
        params.push(decision.clone());
    }

    if let Some(ref from) = query.from {
        where_clauses.push("timestamp >= ?");
        params.push(from.clone());
    }

    if let Some(ref to) = query.to {
        where_clauses.push("timestamp <= ?");
        params.push(to.clone());
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    // Build and execute query
    let sql = format!(
        r#"
        SELECT id, tenant_id, policy_pack_id, hook, decision, reason,
               request_id, user_id, resource_type, resource_id, metadata_json,
               timestamp, entry_hash, previous_hash, chain_sequence
        FROM policy_audit_decisions
        {}
        ORDER BY chain_sequence DESC
        LIMIT ? OFFSET ?
        "#,
        where_sql
    );

    let mut query_builder = sqlx::query(&sql);
    for param in params {
        query_builder = query_builder.bind(param);
    }
    query_builder = query_builder.bind(query.limit).bind(query.offset);

    let rows = query_builder
        .fetch_all(state.db.pool())
        .await
        .map_err(|e| {
            ApiError::internal("failed to query policy decisions").with_details(e.to_string())
        })?;

    let decisions: Vec<PolicyAuditDecision> = rows
        .into_iter()
        .map(|row| PolicyAuditDecision {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            policy_pack_id: row.get("policy_pack_id"),
            hook: row.get("hook"),
            decision: row.get("decision"),
            reason: row.get("reason"),
            request_id: row.get("request_id"),
            user_id: row.get("user_id"),
            resource_type: row.get("resource_type"),
            resource_id: row.get("resource_id"),
            metadata_json: row.get("metadata_json"),
            timestamp: row.get("timestamp"),
            entry_hash: row.get("entry_hash"),
            previous_hash: row.get("previous_hash"),
            chain_sequence: row.get("chain_sequence"),
        })
        .collect();

    Ok(Json(decisions))
}

/// Verify policy audit chain
///
/// Verifies the integrity of the policy audit decision chain.
/// Optionally scoped to a specific tenant.
/// Requires AuditView permission.
#[utoipa::path(
    get,
    path = "/v1/audit/policy-decisions/verify-chain",
    params(
        ("tenant_id" = Option<String>, Query, description = "Optional tenant ID filter")
    ),
    responses(
        (status = 200, description = "Chain verification result", body = ChainVerificationResult),
        (status = 403, description = "Forbidden", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "audit"
)]
pub async fn verify_policy_audit_chain(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ChainVerificationResult>, (StatusCode, Json<ErrorResponse>)> {
    // Require AuditView permission
    require_permission(&claims, Permission::AuditView)?;

    let tenant_id = query.get("tenant_id");

    // Validate tenant isolation
    if let Some(tid) = tenant_id {
        validate_tenant_isolation(&claims, tid)?;
    } else {
        // Default to user's tenant
        validate_tenant_isolation(&claims, &claims.tenant_id)?;
    }

    // Build query based on tenant filter
    let (sql, param) = if let Some(tid) = tenant_id {
        (
            r#"
            SELECT id, entry_hash, previous_hash, chain_sequence
            FROM policy_audit_decisions
            WHERE tenant_id = ?
            ORDER BY chain_sequence ASC
            "#,
            Some(tid.as_str()),
        )
    } else {
        (
            r#"
            SELECT id, entry_hash, previous_hash, chain_sequence
            FROM policy_audit_decisions
            WHERE tenant_id = ?
            ORDER BY chain_sequence ASC
            "#,
            Some(claims.tenant_id.as_str()),
        )
    };

    let mut query_builder = sqlx::query(sql);
    if let Some(p) = param {
        query_builder = query_builder.bind(p);
    }

    let rows = query_builder
        .fetch_all(state.db.pool())
        .await
        .map_err(|e| {
            ApiError::internal("failed to fetch audit chain").with_details(e.to_string())
        })?;

    let total_entries = rows.len() as i64;
    let mut verified_entries = 0i64;
    let mut broken_links = Vec::new();
    let mut previous_hash: Option<String> = None;

    for row in rows {
        let id: String = row.get("id");
        let entry_hash: String = row.get("entry_hash");
        let prev_hash: Option<String> = row.get("previous_hash");
        let sequence: i64 = row.get("chain_sequence");

        // Verify hash linkage
        if sequence == 1 {
            // First entry should have no previous hash
            if prev_hash.is_some() {
                broken_links.push(BrokenLink {
                    sequence,
                    entry_id: id.clone(),
                    expected_hash: "null".to_string(),
                    actual_hash: prev_hash.unwrap_or_default(),
                });
            } else {
                verified_entries += 1;
            }
        } else {
            // Subsequent entries should link to previous
            if prev_hash.as_ref() == previous_hash.as_ref() {
                verified_entries += 1;
            } else {
                broken_links.push(BrokenLink {
                    sequence,
                    entry_id: id.clone(),
                    expected_hash: previous_hash.clone().unwrap_or_else(|| "null".to_string()),
                    actual_hash: prev_hash.clone().unwrap_or_else(|| "null".to_string()),
                });
            }
        }

        previous_hash = Some(entry_hash);
    }

    let valid = broken_links.is_empty();

    Ok(Json(ChainVerificationResult {
        valid,
        total_entries,
        verified_entries,
        broken_links,
        tenant_id: tenant_id.map(|s| s.to_string()),
    }))
}

// Re-export tenant policy handlers from parent module for routes.rs
pub use super::{
    __path_assign_policy, __path_list_policy_assignments, __path_list_violations, assign_policy,
    list_policy_assignments, list_violations,
};
