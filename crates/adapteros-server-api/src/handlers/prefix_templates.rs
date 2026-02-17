//! Prefix template handlers
//!
//! Provides REST endpoints for managing prefix templates used in
//! KV cache prefilling. Templates define static text prefixes that
//! can be cached to skip redundant prefill computation.
//!
//! Endpoints:
//! - GET    /v1/prefix-templates              - List templates
//! - POST   /v1/prefix-templates              - Create template
//! - GET    /v1/prefix-templates/{template_id} - Get template
//! - PUT    /v1/prefix-templates/{template_id} - Update template
//! - DELETE /v1/prefix-templates/{template_id} - Delete template

use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_api_types::prefix_templates::{
    CreatePrefixTemplateRequest, ListPrefixTemplatesResponse, PrefixTemplateResponse,
    UpdatePrefixTemplateRequest,
};
use axum::{extract::Extension, extract::Path, extract::State, http::StatusCode, response::Json};
use tracing::info;

/// List prefix templates for the authenticated tenant
pub async fn list_prefix_templates(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<ListPrefixTemplatesResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    let templates = state
        .db
        .list_prefix_templates(tenant_id)
        .await
        .map_err(ApiError::db_error)?;

    let total = templates.len() as u64;
    Ok(Json(ListPrefixTemplatesResponse { templates, total }))
}

/// Create a new prefix template
pub async fn create_prefix_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreatePrefixTemplateRequest>,
) -> Result<(StatusCode, Json<PrefixTemplateResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Validate tenant isolation: request tenant_id must match auth
    validate_tenant_isolation(&claims, &req.tenant_id)?;

    let template = state
        .db
        .create_prefix_template(req)
        .await
        .map_err(ApiError::db_error)?;

    info!(
        template_id = %template.id,
        tenant_id = %template.tenant_id,
        mode = %template.mode,
        "Prefix template created"
    );

    Ok((
        StatusCode::CREATED,
        Json(PrefixTemplateResponse { template }),
    ))
}

/// Get a prefix template by ID
pub async fn get_prefix_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(template_id): Path<String>,
) -> Result<Json<PrefixTemplateResponse>, (StatusCode, Json<ErrorResponse>)> {
    let template = state
        .db
        .get_prefix_template(&template_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(format!(
                    "Prefix template not found: {}",
                    template_id
                ))),
            )
        })?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &template.tenant_id)?;

    Ok(Json(PrefixTemplateResponse { template }))
}

/// Update a prefix template
pub async fn update_prefix_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(template_id): Path<String>,
    Json(req): Json<UpdatePrefixTemplateRequest>,
) -> Result<Json<PrefixTemplateResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Fetch first to validate tenant isolation before mutation
    let existing = state
        .db
        .get_prefix_template(&template_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(format!(
                    "Prefix template not found: {}",
                    template_id
                ))),
            )
        })?;

    validate_tenant_isolation(&claims, &existing.tenant_id)?;

    let template = state
        .db
        .update_prefix_template(&template_id, req)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Prefix template not found")),
            )
        })?;

    info!(
        template_id = %template.id,
        tenant_id = %template.tenant_id,
        "Prefix template updated"
    );

    Ok(Json(PrefixTemplateResponse { template }))
}

/// Delete a prefix template
pub async fn delete_prefix_template(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(template_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Fetch first to validate tenant isolation before deletion
    let existing = state
        .db
        .get_prefix_template(&template_id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(format!(
                    "Prefix template not found: {}",
                    template_id
                ))),
            )
        })?;

    validate_tenant_isolation(&claims, &existing.tenant_id)?;

    let deleted = state
        .db
        .delete_prefix_template(&template_id)
        .await
        .map_err(ApiError::db_error)?;

    if deleted {
        info!(
            template_id = %template_id,
            tenant_id = %existing.tenant_id,
            "Prefix template deleted"
        );
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Prefix template not found")),
        ))
    }
}
