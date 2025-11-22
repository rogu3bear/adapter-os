//! Minimal model management handlers (stubs for unwired routes)
//!
//! These handlers provide basic API endpoints for model operations.
//! Full implementation details are in the original models.rs file.

use crate::auth::Claims;
use crate::middleware::require_any_role;
use crate::state::AppState;
use crate::types::ErrorResponse;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct ImportModelRequest {
    pub model_name: String,
    pub weights_path: String,
    pub config_path: String,
    pub tokenizer_path: String,
    pub tokenizer_config_path: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize, ToSchema)]
pub struct ImportModelResponse {
    pub import_id: String,
    pub status: String,
    pub message: String,
    pub progress: Option<i32>,
}

#[derive(Serialize, ToSchema)]
pub struct ModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    pub status: String,
    pub loaded_at: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
}

#[derive(Serialize, ToSchema)]
pub struct ModelValidationResponse {
    pub model_id: String,
    pub status: String,
    pub valid: bool,
    pub errors: Vec<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ModelRuntimeHealthResponse {
    pub model_id: String,
    pub health_status: String,
    pub memory_usage_mb: Option<i32>,
    pub last_accessed: Option<String>,
}

/// Load a base model into memory
#[utoipa::path(
    post,
    path = "/v1/models/{model_id}/load",
    params(
        ("model_id" = String, Path, description = "Model ID to load")
    ),
    responses(
        (status = 200, description = "Model loaded", body = ModelStatusResponse),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Load failed")
    ),
    tag = "models"
)]
pub async fn load_model(
    _state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &["admin", "operator"])
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse::new("access denied").with_code("FORBIDDEN")),
            )
        })?;

    Ok(Json(ModelStatusResponse {
        model_id,
        model_name: "model".to_string(),
        status: "loaded".to_string(),
        loaded_at: Some(chrono::Utc::now().to_rfc3339()),
        memory_usage_mb: Some(4096),
        is_loaded: true,
    }))
}

/// Unload a base model from memory
#[utoipa::path(
    post,
    path = "/v1/models/{model_id}/unload",
    params(
        ("model_id" = String, Path, description = "Model ID to unload")
    ),
    responses(
        (status = 200, description = "Model unloaded", body = ModelStatusResponse),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Unload failed")
    ),
    tag = "models"
)]
pub async fn unload_model(
    _state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    require_any_role(&claims, &["admin", "operator"])
        .map_err(|_| {
            (
                StatusCode::FORBIDDEN,
                Json(ErrorResponse::new("access denied").with_code("FORBIDDEN")),
            )
        })?;

    Ok(Json(ModelStatusResponse {
        model_id,
        model_name: "model".to_string(),
        status: "unloaded".to_string(),
        loaded_at: None,
        memory_usage_mb: None,
        is_loaded: false,
    }))
}

/// Get model status
#[utoipa::path(
    get,
    path = "/v1/models/{model_id}/status",
    params(
        ("model_id" = String, Path, description = "Model ID")
    ),
    responses(
        (status = 200, description = "Model status", body = ModelStatusResponse),
        (status = 404, description = "Model not found")
    ),
    tag = "models"
)]
pub async fn get_model_status(
    _state: State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(ModelStatusResponse {
        model_id,
        model_name: "model".to_string(),
        status: "unloaded".to_string(),
        loaded_at: None,
        memory_usage_mb: None,
        is_loaded: false,
    }))
}

/// Validate a model
#[utoipa::path(
    get,
    path = "/v1/models/{model_id}/validate",
    params(
        ("model_id" = String, Path, description = "Model ID to validate")
    ),
    responses(
        (status = 200, description = "Validation result", body = ModelValidationResponse),
        (status = 404, description = "Model not found")
    ),
    tag = "models"
)]
pub async fn validate_model(
    _state: State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelValidationResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(ModelValidationResponse {
        model_id,
        status: "valid".to_string(),
        valid: true,
        errors: vec![],
    }))
}
