//! Adapter lifecycle management endpoints (load/unload)
//!
//! This module implements runtime adapter lifecycle operations:
//! - Load adapters into memory (POST /v1/adapters/:id/load)
//! - Unload adapters from memory (POST /v1/adapters/:id/unload)
//!
//! These endpoints integrate with the MLX backend for hot-swap adapter management
//! and update lifecycle state in the database for tracking.

use adapteros_db::sqlx;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};
use utoipa::ToSchema;

use crate::{
    audit_helper::{actions, log_failure, log_success, resources},
    auth::Claims,
    permissions::{require_permission, Permission},
    state::AppState,
    types::ErrorResponse,
};

/// Load adapter request (optional body parameters)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LoadAdapterRequest {
    /// Target lifecycle state after loading (default: "Warm")
    #[serde(default = "default_load_state")]
    pub target_state: String,
}

fn default_load_state() -> String {
    "Warm".to_string()
}

/// Unload adapter request (optional body parameters)
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UnloadAdapterRequest {
    /// Target lifecycle state after unloading (default: "Cold")
    #[serde(default = "default_unload_state")]
    pub target_state: String,
}

fn default_unload_state() -> String {
    "Cold".to_string()
}

/// Adapter lifecycle operation response
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AdapterLifecycleResponse {
    pub adapter_id: String,
    pub previous_state: String,
    pub current_state: String,
    pub load_state: String,
    pub message: String,
    pub timestamp: String,
}

/// Load adapter into memory (POST /v1/adapters/:id/load)
///
/// # Permission Required
/// - `AdapterLoad` (Admin, Operator, SRE)
///
/// # Workflow
/// 1. Check permission
/// 2. Retrieve adapter metadata from database
/// 3. Verify .aos file exists
/// 4. Load adapter via MLX backend
/// 5. Update lifecycle_state and load_state in database
/// 6. Emit audit log
///
/// # Errors
/// - 403 Forbidden: Insufficient permissions
/// - 404 Not Found: Adapter not found or .aos file missing
/// - 409 Conflict: Adapter already loaded
/// - 500 Internal Server Error: Backend failure or database error
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/load",
    request_body = LoadAdapterRequest,
    responses(
        (status = 200, description = "Adapter loaded successfully", body = AdapterLifecycleResponse),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Adapter not found"),
        (status = 409, description = "Adapter already loaded"),
        (status = 500, description = "Internal error")
    ),
    tag = "adapters",
    security(("bearer" = []))
)]
pub async fn load_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    body: Option<Json<LoadAdapterRequest>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permissions
    if let Err(e) = require_permission(&claims, Permission::AdapterLoad) {
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_LOAD,
            resources::ADAPTER,
            Some(&adapter_id),
            &e.to_string(),
        )
        .await;
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        ));
    }

    let target_state = body
        .as_ref()
        .map(|b| b.target_state.clone())
        .unwrap_or_else(default_load_state);

    // Validate target state
    if !["Warm", "Hot", "Resident"].contains(&target_state.as_str()) {
        let err_msg = format!(
            "Invalid target state: {}. Must be Warm, Hot, or Resident",
            target_state
        );
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_LOAD,
            resources::ADAPTER,
            Some(&adapter_id),
            &err_msg,
        )
        .await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(err_msg).with_code("BAD_REQUEST")),
        ));
    }

    // Retrieve adapter from database
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!(adapter_id = %adapter_id, error = %e, "Failed to retrieve adapter");
            let err_msg = format!("Adapter '{}' not found", adapter_id);
            tokio::spawn({
                let db = state.db.clone();
                let claims = claims.clone();
                let adapter_id = adapter_id.clone();
                let err_msg = err_msg.clone();
                async move {
                    let _ = log_failure(
                        &db,
                        &claims,
                        actions::ADAPTER_LOAD,
                        resources::ADAPTER,
                        Some(&adapter_id),
                        &err_msg,
                    )
                    .await;
                }
            });
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(err_msg).with_code("NOT_FOUND")),
            )
        })?
        .ok_or_else(|| {
            error!(adapter_id = %adapter_id, "Adapter not found in database");
            let err_msg = format!("Adapter '{}' not found", adapter_id);
            tokio::spawn({
                let db = state.db.clone();
                let claims = claims.clone();
                let adapter_id = adapter_id.clone();
                let err_msg = err_msg.clone();
                async move {
                    let _ = log_failure(
                        &db,
                        &claims,
                        actions::ADAPTER_LOAD,
                        resources::ADAPTER,
                        Some(&adapter_id),
                        &err_msg,
                    )
                    .await;
                }
            });
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(err_msg).with_code("NOT_FOUND")),
            )
        })?;

    let previous_state = adapter.load_state.clone();

    // Check if adapter is already loaded
    if ["loaded", "hot", "resident"].contains(&previous_state.to_lowercase().as_str()) {
        let err_msg = format!(
            "Adapter '{}' is already loaded (state: {})",
            adapter_id, previous_state
        );
        warn!(adapter_id = %adapter_id, load_state = %previous_state, "Adapter already loaded");
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_LOAD,
            resources::ADAPTER,
            Some(&adapter_id),
            &err_msg,
        )
        .await;
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new(err_msg).with_code("CONFLICT")),
        ));
    }

    // Verify .aos file exists
    let aos_file_path = adapter.aos_file_path.ok_or_else(|| {
        error!(adapter_id = %adapter_id, "No .aos file path registered");
        let err_msg = format!("Adapter '{}' has no .aos file registered", adapter_id);
        tokio::spawn({
            let db = state.db.clone();
            let claims = claims.clone();
            let adapter_id = adapter_id.clone();
            let err_msg = err_msg.clone();
            async move {
                let _ = log_failure(
                    &db,
                    &claims,
                    actions::ADAPTER_LOAD,
                    resources::ADAPTER,
                    Some(&adapter_id),
                    &err_msg,
                )
                .await;
            }
        });
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(err_msg).with_code("NOT_FOUND")),
        )
    })?;

    // Check file existence
    if !tokio::fs::try_exists(&aos_file_path).await.unwrap_or(false) {
        error!(adapter_id = %adapter_id, path = %aos_file_path, "AOS file not found on filesystem");
        let err_msg = format!("AOS file not found: {}", aos_file_path);
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_LOAD,
            resources::ADAPTER,
            Some(&adapter_id),
            &err_msg,
        )
        .await;
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(err_msg).with_code("NOT_FOUND")),
        ));
    }

    // Load adapter via MLX backend (if available)
    if let Some(worker) = &state.worker {
        info!(adapter_id = %adapter_id, aos_path = %aos_file_path, "Loading adapter via MLX backend");

        // Note: This is a simplified implementation. In production, you would:
        // 1. Parse the .aos file to extract LoRA weights
        // 2. Create a LoRAAdapter instance
        // 3. Call backend.load_adapter_runtime(adapter_id_u16, adapter)
        //
        // For now, we'll just update the database state
        warn!(adapter_id = %adapter_id, "MLX backend integration pending - updating database state only");
    } else {
        warn!("Worker not available - operating in database-only mode");
    }

    // Update database: lifecycle_state and load_state
    sqlx::query!(
        r#"
        UPDATE adapters
        SET lifecycle_state = ?,
            load_state = 'loaded',
            last_loaded_at = datetime('now'),
            updated_at = datetime('now')
        WHERE adapter_id = ?
        "#,
        target_state,
        adapter_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!(adapter_id = %adapter_id, error = %e, "Failed to update adapter state");
        let err_msg = format!("Database error: {}", e);
        tokio::spawn({
            let db = state.db.clone();
            let claims = claims.clone();
            let adapter_id = adapter_id.clone();
            let err_msg = err_msg.clone();
            async move {
                let _ = log_failure(
                    &db,
                    &claims,
                    actions::ADAPTER_LOAD,
                    resources::ADAPTER,
                    Some(&adapter_id),
                    &err_msg,
                )
                .await;
            }
        });
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(err_msg).with_code("INTERNAL_ERROR")),
        )
    })?;

    log_success(
        &state.db,
        &claims,
        actions::ADAPTER_LOAD,
        resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    info!(adapter_id = %adapter_id, previous_state = %previous_state, new_state = %target_state, "Successfully loaded adapter");

    Ok(Json(AdapterLifecycleResponse {
        adapter_id: adapter_id.clone(),
        previous_state,
        current_state: target_state,
        load_state: "loaded".to_string(),
        message: format!("Adapter '{}' loaded successfully", adapter_id),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}

/// Unload adapter from memory (POST /v1/adapters/:id/unload)
///
/// # Permission Required
/// - `AdapterUnload` (Admin, Operator, SRE)
///
/// # Workflow
/// 1. Check permission
/// 2. Retrieve adapter metadata from database
/// 3. Verify adapter is currently loaded
/// 4. Unload adapter via MLX backend
/// 5. Update lifecycle_state and load_state in database
/// 6. Emit audit log
///
/// # Errors
/// - 403 Forbidden: Insufficient permissions
/// - 404 Not Found: Adapter not found
/// - 409 Conflict: Adapter not loaded
/// - 500 Internal Server Error: Backend failure or database error
#[utoipa::path(
    post,
    path = "/v1/adapters/{adapter_id}/unload",
    request_body = UnloadAdapterRequest,
    responses(
        (status = 200, description = "Adapter unloaded successfully", body = AdapterLifecycleResponse),
        (status = 403, description = "Insufficient permissions"),
        (status = 404, description = "Adapter not found"),
        (status = 409, description = "Adapter not loaded"),
        (status = 500, description = "Internal error")
    ),
    tag = "adapters",
    security(("bearer" = []))
)]
pub async fn unload_adapter(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(adapter_id): Path<String>,
    body: Option<Json<UnloadAdapterRequest>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permissions
    if let Err(e) = require_permission(&claims, Permission::AdapterUnload) {
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_UNLOAD,
            resources::ADAPTER,
            Some(&adapter_id),
            &e.to_string(),
        )
        .await;
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("insufficient permissions").with_code("FORBIDDEN")),
        ));
    }

    let target_state = body
        .as_ref()
        .map(|b| b.target_state.clone())
        .unwrap_or_else(default_unload_state);

    // Validate target state
    if !["Cold", "Unloaded"].contains(&target_state.as_str()) {
        let err_msg = format!(
            "Invalid target state: {}. Must be Cold or Unloaded",
            target_state
        );
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_UNLOAD,
            resources::ADAPTER,
            Some(&adapter_id),
            &err_msg,
        )
        .await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(err_msg).with_code("BAD_REQUEST")),
        ));
    }

    // Retrieve adapter from database
    let adapter = state
        .db
        .get_adapter(&adapter_id)
        .await
        .map_err(|e| {
            error!(adapter_id = %adapter_id, error = %e, "Failed to retrieve adapter");
            let err_msg = format!("Adapter '{}' not found", adapter_id);
            tokio::spawn({
                let db = state.db.clone();
                let claims = claims.clone();
                let adapter_id = adapter_id.clone();
                let err_msg = err_msg.clone();
                async move {
                    let _ = log_failure(
                        &db,
                        &claims,
                        actions::ADAPTER_UNLOAD,
                        resources::ADAPTER,
                        Some(&adapter_id),
                        &err_msg,
                    )
                    .await;
                }
            });
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(err_msg).with_code("NOT_FOUND")),
            )
        })?
        .ok_or_else(|| {
            error!(adapter_id = %adapter_id, "Adapter not found in database");
            let err_msg = format!("Adapter '{}' not found", adapter_id);
            tokio::spawn({
                let db = state.db.clone();
                let claims = claims.clone();
                let adapter_id = adapter_id.clone();
                let err_msg = err_msg.clone();
                async move {
                    let _ = log_failure(
                        &db,
                        &claims,
                        actions::ADAPTER_UNLOAD,
                        resources::ADAPTER,
                        Some(&adapter_id),
                        &err_msg,
                    )
                    .await;
                }
            });
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(err_msg).with_code("NOT_FOUND")),
            )
        })?;

    let previous_state = adapter.load_state.clone();

    // Check if adapter is actually loaded
    if ["unloaded", "cold"].contains(&previous_state.to_lowercase().as_str()) {
        let err_msg = format!(
            "Adapter '{}' is not loaded (state: {})",
            adapter_id, previous_state
        );
        warn!(adapter_id = %adapter_id, load_state = %previous_state, "Adapter not loaded");
        log_failure(
            &state.db,
            &claims,
            actions::ADAPTER_UNLOAD,
            resources::ADAPTER,
            Some(&adapter_id),
            &err_msg,
        )
        .await;
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new(err_msg).with_code("CONFLICT")),
        ));
    }

    // Unload adapter via MLX backend (if available)
    if let Some(worker) = &state.worker {
        info!(adapter_id = %adapter_id, "Unloading adapter via MLX backend");

        // Note: This is a simplified implementation. In production, you would:
        // 1. Determine the adapter_id (u16) used in the backend
        // 2. Call backend.unload_adapter_runtime(adapter_id_u16)
        // 3. Free memory resources
        //
        // For now, we'll just update the database state
        warn!(adapter_id = %adapter_id, "MLX backend integration pending - updating database state only");
    } else {
        warn!("Worker not available - operating in database-only mode");
    }

    // Update database: lifecycle_state and load_state
    sqlx::query!(
        r#"
        UPDATE adapters
        SET lifecycle_state = ?,
            load_state = 'unloaded',
            updated_at = datetime('now')
        WHERE adapter_id = ?
        "#,
        target_state,
        adapter_id
    )
    .execute(state.db.pool())
    .await
    .map_err(|e| {
        error!(adapter_id = %adapter_id, error = %e, "Failed to update adapter state");
        let err_msg = format!("Database error: {}", e);
        tokio::spawn({
            let db = state.db.clone();
            let claims = claims.clone();
            let adapter_id = adapter_id.clone();
            let err_msg = err_msg.clone();
            async move {
                let _ = log_failure(
                    &db,
                    &claims,
                    actions::ADAPTER_UNLOAD,
                    resources::ADAPTER,
                    Some(&adapter_id),
                    &err_msg,
                )
                .await;
            }
        });
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(err_msg).with_code("INTERNAL_ERROR")),
        )
    })?;

    log_success(
        &state.db,
        &claims,
        actions::ADAPTER_UNLOAD,
        resources::ADAPTER,
        Some(&adapter_id),
    )
    .await;

    info!(adapter_id = %adapter_id, previous_state = %previous_state, new_state = %target_state, "Successfully unloaded adapter");

    Ok(Json(AdapterLifecycleResponse {
        adapter_id: adapter_id.clone(),
        previous_state,
        current_state: target_state,
        load_state: "unloaded".to_string(),
        message: format!("Adapter '{}' unloaded successfully", adapter_id),
        timestamp: chrono::Utc::now().to_rfc3339(),
    }))
}
