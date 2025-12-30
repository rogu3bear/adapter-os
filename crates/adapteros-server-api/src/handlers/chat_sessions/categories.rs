//! Category handlers for chat sessions
//!
//! Provides list_chat_categories, create_chat_category, update_chat_category,
//! delete_chat_category, set_session_category
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_categories】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
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
pub async fn list_chat_categories(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<ChatCategory>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let categories = state
        .db
        .list_chat_categories(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list categories")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(categories))
}

/// Create a new category
///
/// POST /v1/chat/categories
pub async fn create_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<ChatCategory>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Category name cannot be empty").with_code("VALIDATION_ERROR")),
        ));
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
            let status = if e.to_string().contains("depth cannot exceed") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new("Failed to create category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok((StatusCode::CREATED, Json(category)))
}

/// Update a category
///
/// PUT /v1/chat/categories/:category_id
pub async fn update_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category_id): Path<String>,
    Json(req): Json<UpdateCategoryRequest>,
) -> Result<Json<ChatCategory>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify category belongs to tenant
    let category = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Category not found").with_code("NOT_FOUND")),
            )
        })?;

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
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let updated = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Category not found after update").with_code("NOT_FOUND")),
            )
        })?;

    Ok(Json(updated))
}

/// Delete a category
///
/// DELETE /v1/chat/categories/:category_id
pub async fn delete_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify category belongs to tenant
    let category = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Category not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &category.tenant_id)?;

    state
        .db
        .delete_chat_category(&category_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("Cannot delete category") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new("Failed to delete category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Set the category for a session
///
/// PUT /v1/chat/sessions/:session_id/category
pub async fn set_session_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<SetCategoryRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session belongs to tenant
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    state
        .db
        .set_session_category(&session_id, req.category_id.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to set category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}
