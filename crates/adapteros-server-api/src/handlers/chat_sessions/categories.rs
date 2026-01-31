//! Category handlers for chat sessions
//!
//! Provides list_chat_categories, create_chat_category, update_chat_category,
//! delete_chat_category, set_session_category
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_categories】

use crate::api_error::{ApiError, ApiResult};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use adapteros_db::ChatCategory;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

use super::types::{CreateCategoryRequest, SetCategoryRequest, UpdateCategoryRequest};

/// List all categories for the tenant
///
/// GET /v1/chat/categories
#[utoipa::path(
    get,
    path = "/v1/chat/categories",
    responses(
        (status = 200, description = "Chat categories"),
        (status = 403, description = "Forbidden")
    ),
    tag = "chat"
)]
pub async fn list_chat_categories(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> ApiResult<Vec<ChatCategory>> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let categories = state
        .db
        .list_chat_categories(&claims.tenant_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(Json(categories))
}

/// Create a new category
///
/// POST /v1/chat/categories
#[utoipa::path(
    post,
    path = "/v1/chat/categories",
    request_body = CreateCategoryRequest,
    responses(
        (status = 201, description = "Category created"),
        (status = 400, description = "Invalid request"),
        (status = 403, description = "Forbidden")
    ),
    tag = "chat"
)]
pub async fn create_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<ChatCategory>), ApiError> {
    require_permission(&claims, Permission::WorkspaceManage)
        .map_err(|_| ApiError::forbidden("Permission denied - requires WorkspaceManage"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    if req.name.trim().is_empty() {
        return Err(ApiError::bad_request("Category name cannot be empty"));
    }

    let category = state
        .db
        .create_chat_category(
            &claims.tenant_id,
            &req.name,
            req.parent_id.as_deref(),
            req.icon.as_deref(),
            req.color.as_deref(),
        )
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            if error_str.contains("depth cannot exceed") {
                ApiError::bad_request("Failed to create category").with_details(error_str)
            } else {
                ApiError::db_error(&e).with_details(error_str)
            }
        })?;

    Ok((StatusCode::CREATED, Json(category)))
}

/// Update a category
///
/// PUT /v1/chat/categories/:category_id
#[utoipa::path(
    put,
    path = "/v1/chat/categories/{category_id}",
    request_body = UpdateCategoryRequest,
    params(
        ("category_id" = String, Path, description = "Category ID")
    ),
    responses(
        (status = 200, description = "Category updated"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Category not found")
    ),
    tag = "chat"
)]
pub async fn update_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category_id): Path<String>,
    Json(req): Json<UpdateCategoryRequest>,
) -> ApiResult<ChatCategory> {
    require_permission(&claims, Permission::WorkspaceManage)
        .map_err(|_| ApiError::forbidden("Permission denied - requires WorkspaceManage"))?;

    // Verify category belongs to tenant
    let category = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Category"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &category.tenant_id)?;

    state
        .db
        .update_chat_category(
            &category_id,
            req.name.as_deref(),
            req.icon.as_deref(),
            req.color.as_deref(),
        )
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    let updated = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Category not found after update"))?;

    Ok(Json(updated))
}

/// Delete a category
///
/// DELETE /v1/chat/categories/:category_id
#[utoipa::path(
    delete,
    path = "/v1/chat/categories/{category_id}",
    params(
        ("category_id" = String, Path, description = "Category ID")
    ),
    responses(
        (status = 204, description = "Category deleted"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Category not found")
    ),
    tag = "chat"
)]
pub async fn delete_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::WorkspaceManage)
        .map_err(|_| ApiError::forbidden("Permission denied - requires WorkspaceManage"))?;

    // Verify category belongs to tenant
    let category = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Category"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &category.tenant_id)?;

    state
        .db
        .delete_chat_category(&category_id)
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            if error_str.contains("Cannot delete category") {
                ApiError::bad_request("Failed to delete category").with_details(error_str)
            } else {
                ApiError::db_error(&e).with_details(error_str)
            }
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Set the category for a session
///
/// PUT /v1/chat/sessions/:session_id/category
#[utoipa::path(
    put,
    path = "/v1/chat/sessions/{session_id}/category",
    request_body = SetCategoryRequest,
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Category set"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Session not found")
    ),
    tag = "chat"
)]
pub async fn set_session_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<SetCategoryRequest>,
) -> Result<StatusCode, ApiError> {
    require_permission(&claims, Permission::InferenceExecute)
        .map_err(|_| ApiError::forbidden("Permission denied"))?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
        .ok_or_else(|| ApiError::not_found("Session"))?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    state
        .db
        .set_session_category(&session_id, req.category_id.as_deref())
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
