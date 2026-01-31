//! Routing Rules Handler
//!
//! Provides CRUD operations for Routing Rules.

use crate::api_error::ApiError;
use crate::auth::Principal;
use crate::state::AppState;
use adapteros_db::routing_rules::{CreateRoutingRuleParams, RoutingRule};
use axum::{
    extract::{Path, State},
    Json,
};

/// List routing rules for an Identity Set
#[utoipa::path(
    get,
    path = "/v1/routing-rules/identity/{identity_dataset_id}",
    params(
        ("identity_dataset_id" = String, Path, description = "Identity dataset ID")
    ),
    responses(
        (status = 200, description = "Routing rules"),
        (status = 500, description = "Internal error")
    ),
    tag = "routing"
)]
pub async fn list_rules(
    State(state): State<AppState>,
    _identity: Principal,
    Path(identity_dataset_id): Path<String>,
) -> Result<Json<Vec<RoutingRule>>, ApiError> {
    let rules = RoutingRule::list_by_identity(&state.db_pool, &identity_dataset_id)
        .await
        .map_err(ApiError::db_error)?;

    Ok(Json(rules))
}

/// Create a new routing rule
#[utoipa::path(
    post,
    path = "/v1/routing-rules",
    responses(
        (status = 200, description = "Routing rule created"),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal error")
    ),
    tag = "routing"
)]
pub async fn create_rule(
    State(state): State<AppState>,
    identity: Principal,
    Json(params): Json<CreateRoutingRuleParams>,
) -> Result<Json<RoutingRule>, ApiError> {
    // Optionally validate that the user has access to create rules
    // For now assuming ScopedIdentity implies permission

    let mut params = params;
    params.created_by = Some(identity.principal_id);

    let rule = RoutingRule::create(&state.db_pool, &params)
        .await
        .map_err(ApiError::db_error)?;

    Ok(Json(rule))
}

/// Delete a routing rule
#[utoipa::path(
    delete,
    path = "/v1/routing-rules/{rule_id}",
    params(
        ("rule_id" = String, Path, description = "Routing rule ID")
    ),
    responses(
        (status = 204, description = "Routing rule deleted"),
        (status = 404, description = "Routing rule not found"),
        (status = 500, description = "Internal error")
    ),
    tag = "routing"
)]
pub async fn delete_rule(
    State(state): State<AppState>,
    _identity: Principal,
    Path(rule_id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    RoutingRule::delete(&state.db_pool, &rule_id)
        .await
        .map_err(ApiError::db_error)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
