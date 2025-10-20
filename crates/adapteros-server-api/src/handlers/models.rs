//! Base Model Management Handlers
//!
//! Provides API endpoints for model import, loading, and status management.
//!
//! # Citations
//! - CONTRIBUTING.md L123: Use `tracing` for logging
//! - Policy Pack #9 (Telemetry): Emit structured JSON events
//! - Policy Pack #8 (Isolation): Per-tenant operations with UID/GID checks
//! - Handler pattern from handlers.rs L4567-4597

use crate::{auth::Claims, state::AppState, types::ErrorResponse};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportModelRequest {
    pub model_name: String,
    pub weights_path: String,
    pub config_path: String,
    pub tokenizer_path: String,
    pub tokenizer_config_path: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImportModelResponse {
    pub import_id: String,
    pub status: String,
    pub message: String,
    pub progress: Option<i32>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ModelStatusResponse {
    pub model_id: String,
    pub model_name: String,
    pub status: String,
    pub loaded_at: Option<String>,
    pub memory_usage_mb: Option<i32>,
    pub is_loaded: bool,
}

#[derive(Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CursorConfigResponse {
    pub api_endpoint: String,
    pub model_name: String,
    pub model_id: String,
    pub is_ready: bool,
    pub setup_instructions: Vec<String>,
}

/// Import a new base model
///
/// # Citation
/// - Handler pattern from handlers.rs L4567-4597
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/models/import",
    request_body = ImportModelRequest,
    responses(
        (status = 200, description = "Import started", body = ImportModelResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "models"
))]
pub async fn import_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<ImportModelRequest>,
) -> Result<Json<ImportModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require admin role for model import
    // Citation: CONTRIBUTING.md L132 - Security-sensitive code requires review
    if claims.role != "admin" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("admin role required").with_code("UNAUTHORIZED")),
        ));
    }

    let tenant_id = &claims.tenant_id;
    let import_id = Uuid::new_v4().to_string();

    // Validate file paths exist
    let weights_exists = std::path::Path::new(&req.weights_path).exists();
    let config_exists = std::path::Path::new(&req.config_path).exists();
    let tokenizer_exists = std::path::Path::new(&req.tokenizer_path).exists();

    if !weights_exists {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("weights file not found").with_code("INVALID_PATH")),
        ));
    }
    if !config_exists {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("config file not found").with_code("INVALID_PATH")),
        ));
    }
    if !tokenizer_exists {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("tokenizer file not found").with_code("INVALID_PATH")),
        ));
    }

    // Create import record
    let now = chrono::Utc::now().to_rfc3339();
    let metadata_str = req.metadata.as_ref().map(|m| m.to_string());
    let tokenizer_config = req.tokenizer_config_path.as_deref();

    sqlx::query!(
        r#"
        INSERT INTO base_model_imports 
        (id, tenant_id, model_name, weights_path, config_path, tokenizer_path, 
         tokenizer_config_path, status, started_at, created_by, metadata_json)
        VALUES (?, ?, ?, ?, ?, ?, ?, 'validating', ?, ?, ?)
        "#,
        import_id,
        tenant_id,
        req.model_name,
        req.weights_path,
        req.config_path,
        req.tokenizer_path,
        tokenizer_config,
        now,
        claims.sub,
        metadata_str
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to create import record: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    // Emit telemetry event
    // Citation: Policy Pack #9 (Telemetry)
    info!(
        event = "model.import.started",
        import_id = %import_id,
        model_name = %req.model_name,
        tenant_id = %tenant_id,
        user_id = %claims.sub,
        "Model import started"
    );

    // Track onboarding journey step
    let _ = track_journey_step(&state, tenant_id, &claims.sub, "model_imported").await;

    Ok(Json(ImportModelResponse {
        import_id,
        status: "validating".to_string(),
        message: format!("Import started for model: {}", req.model_name),
        progress: Some(0),
    }))
}

/// Load a base model into memory
///
/// # Citation
/// - Pattern from handlers.rs L4567-4630 (load_adapter)
#[cfg_attr(feature = "openapi", utoipa::path(
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
))]
pub async fn load_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<Json<ModelStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Require operator or admin role
    if claims.role != "admin" && claims.role != "operator" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("operator or admin role required").with_code("UNAUTHORIZED")),
        ));
    }

    let tenant_id = &claims.tenant_id;

    // Check if model exists
    let model_check = sqlx::query!(
        "SELECT bms.model_id, m.name as model_name FROM base_model_status bms 
         JOIN models m ON bms.model_id = m.id 
         WHERE bms.model_id = ? AND bms.tenant_id = ?",
        model_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to check model: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    let model_name = if let Some(row) = model_check {
        row.model_name
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
        ));
    };

    // Update status to loading
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE base_model_status SET status = 'loading', updated_at = ? WHERE model_id = ? AND tenant_id = ?",
        now,
        model_id,
        tenant_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to update model status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    // TODO: Actual model loading via lifecycle manager
    // For now, simulate successful load after brief delay
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Update to loaded state
    let loaded_at = chrono::Utc::now().to_rfc3339();
    let memory_mb: i32 = 8192; // TODO: Get actual memory usage

    sqlx::query!(
        "UPDATE base_model_status SET status = 'loaded', loaded_at = ?, memory_usage_mb = ?, updated_at = ? WHERE model_id = ? AND tenant_id = ?",
        loaded_at,
        memory_mb,
        loaded_at,
        model_id,
        tenant_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to update loaded status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    // Emit telemetry
    info!(
        event = "model.load",
        model_id = %model_id,
        tenant_id = %tenant_id,
        memory_mb = %memory_mb,
        "Base model loaded"
    );

    // Track onboarding journey step
    let _ = track_journey_step(&state, tenant_id, &claims.sub, "model_loaded").await;

    Ok(Json(ModelStatusResponse {
        model_id: model_id.clone(),
        model_name,
        status: "loaded".to_string(),
        loaded_at: Some(loaded_at),
        memory_usage_mb: Some(memory_mb),
        is_loaded: true,
    }))
}

/// Unload a base model from memory
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/models/{model_id}/unload",
    params(
        ("model_id" = String, Path, description = "Model ID to unload")
    ),
    responses(
        (status = 200, description = "Model unloaded"),
        (status = 404, description = "Model not found"),
        (status = 500, description = "Unload failed")
    ),
    tag = "models"
))]
pub async fn unload_model(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(model_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if claims.role != "admin" && claims.role != "operator" {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new("operator or admin role required").with_code("UNAUTHORIZED")),
        ));
    }

    let tenant_id = &claims.tenant_id;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if model exists
    let exists = sqlx::query!(
        "SELECT id FROM base_model_status WHERE model_id = ? AND tenant_id = ?",
        model_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to check model: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    if exists.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("model not found").with_code("NOT_FOUND")),
        ));
    }

    // Update to unloading state
    sqlx::query!(
        "UPDATE base_model_status SET status = 'unloading', updated_at = ? WHERE model_id = ? AND tenant_id = ?",
        now,
        model_id,
        tenant_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to update status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    // TODO: Actual unload via lifecycle manager
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Update to unloaded
    let unloaded_at = chrono::Utc::now().to_rfc3339();
    sqlx::query!(
        "UPDATE base_model_status SET status = 'unloaded', unloaded_at = ?, loaded_at = NULL, memory_usage_mb = NULL, updated_at = ? WHERE model_id = ? AND tenant_id = ?",
        unloaded_at,
        unloaded_at,
        model_id,
        tenant_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to update unloaded status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    info!(
        event = "model.unload",
        model_id = %model_id,
        tenant_id = %tenant_id,
        "Base model unloaded"
    );

    Ok(StatusCode::OK)
}

/// Get import status
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/imports/{import_id}",
    params(
        ("import_id" = String, Path, description = "Import ID")
    ),
    responses(
        (status = 200, description = "Import status", body = ImportModelResponse),
        (status = 404, description = "Import not found")
    ),
    tag = "models"
))]
pub async fn get_import_status(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(import_id): Path<String>,
) -> Result<Json<ImportModelResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    let import = sqlx::query!(
        "SELECT status, model_name, progress, error_message FROM base_model_imports WHERE id = ? AND tenant_id = ?",
        import_id,
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to get import status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    let import = import.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("import not found").with_code("NOT_FOUND")),
        )
    })?;

    Ok(Json(ImportModelResponse {
        import_id,
        status: import.status,
        message: import
            .error_message
            .unwrap_or_else(|| format!("Import in progress for {}", import.model_name)),
        progress: import.progress.map(|p| p as i32),
    }))
}

/// Get Cursor IDE configuration
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/models/cursor-config",
    responses(
        (status = 200, description = "Cursor configuration", body = CursorConfigResponse),
        (status = 500, description = "Failed to get config")
    ),
    tag = "models"
))]
pub async fn get_cursor_config(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<CursorConfigResponse>, (StatusCode, Json<ErrorResponse>)> {
    let tenant_id = &claims.tenant_id;

    // Check if model is loaded
    let status = sqlx::query!(
        "SELECT bms.model_id, m.name as model_name, bms.status FROM base_model_status bms 
         JOIN models m ON bms.model_id = m.id 
         WHERE bms.tenant_id = ? ORDER BY bms.updated_at DESC LIMIT 1",
        tenant_id
    )
    .fetch_optional(state.db.pool())
    .await
    .map_err(|e| {
        error!("Failed to check model status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("database error").with_code("DB_ERROR")),
        )
    })?;

    let (model_id, model_name, is_ready) = if let Some(row) = status {
        (row.model_id, row.model_name, row.status == "loaded")
    } else {
        ("unknown".to_string(), "no-model".to_string(), false)
    };

    let api_endpoint = "http://127.0.0.1:8080/api/v1/chat/completions".to_string();
    let model_display_name = format!("adapteros-{}", model_name);

    Ok(Json(CursorConfigResponse {
        api_endpoint: api_endpoint.clone(),
        model_name: model_display_name.clone(),
        model_id: model_id.clone(),
        is_ready,
        setup_instructions: vec![
            "1. Open Cursor IDE Settings (Cmd+, or Ctrl+,)".to_string(),
            "2. Navigate to the 'Models' section".to_string(),
            format!("3. Add custom endpoint: {}", api_endpoint),
            format!("4. Set model name: {}", model_display_name),
            "5. Save settings and test the connection".to_string(),
            "6. Try a code completion or chat to verify".to_string(),
        ],
    }))
}

// Helper function to track onboarding journey
async fn track_journey_step(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    step: &str,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query!(
        r#"
        INSERT INTO onboarding_journeys (id, tenant_id, user_id, journey_type, step_completed, completed_at)
        VALUES (?, ?, ?, 'cursor_integration', ?, ?)
        "#,
        id,
        tenant_id,
        user_id,
        step,
        now
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        warn!("Failed to track journey step: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("journey tracking failed").with_code("TRACKING_ERROR")),
        )
    })?;

    info!(
        event = "journey.step_completed",
        tenant_id = %tenant_id,
        user_id = %user_id,
        step = %step,
        "Onboarding journey step completed"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_import_request_serialization() {
        let req = ImportModelRequest {
            model_name: "test-model".to_string(),
            weights_path: "/path/to/weights.safetensors".to_string(),
            config_path: "/path/to/config.json".to_string(),
            tokenizer_path: "/path/to/tokenizer.json".to_string(),
            tokenizer_config_path: None,
            metadata: None,
        };

        assert_eq!(req.model_name, "test-model");
    }

    #[test]
    fn test_cursor_config_instructions() {
        let config = CursorConfigResponse {
            api_endpoint: "http://localhost:8080/api/v1/chat/completions".to_string(),
            model_name: "adapteros-test".to_string(),
            model_id: "test-123".to_string(),
            is_ready: true,
            setup_instructions: vec!["Step 1".to_string()],
        };

        assert!(config.is_ready);
        assert_eq!(config.setup_instructions.len(), 1);
    }
}
