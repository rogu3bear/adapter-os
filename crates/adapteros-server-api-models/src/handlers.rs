//! Model endpoint handlers
//!
//! Placeholder handlers for model-related operations.
//! These will be populated when migrating from adapteros-server-api.

use axum::{extract::Path, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Model information response
#[derive(Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Request to register a new model
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterModelRequest {
    pub name: String,
    pub version: String,
    pub path: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

/// Response for registering a model
#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterModelResponse {
    pub id: Uuid,
    pub name: String,
    pub status: String,
}

/// List all registered models
///
/// Placeholder handler - will be implemented when migrating from adapteros-server-api
pub async fn list_models() -> Json<Vec<ModelInfo>> {
    tracing::debug!("Listing models (placeholder)");
    Json(vec![])
}

/// Get information about a specific model
///
/// Placeholder handler - will be implemented when migrating from adapteros-server-api
pub async fn get_model(Path(model_id): Path<Uuid>) -> Result<Json<ModelInfo>, StatusCode> {
    tracing::debug!(model_id = %model_id, "Getting model info (placeholder)");
    Err(StatusCode::NOT_FOUND)
}

/// Register a new model
///
/// Placeholder handler - will be implemented when migrating from adapteros-server-api
pub async fn register_model(
    Json(_request): Json<RegisterModelRequest>,
) -> Result<Json<RegisterModelResponse>, StatusCode> {
    tracing::debug!("Registering model (placeholder)");
    Err(StatusCode::NOT_IMPLEMENTED)
}

/// Delete a model
///
/// Placeholder handler - will be implemented when migrating from adapteros-server-api
pub async fn delete_model(Path(model_id): Path<Uuid>) -> StatusCode {
    tracing::debug!(model_id = %model_id, "Deleting model (placeholder)");
    StatusCode::NOT_IMPLEMENTED
}

/// Get model status
///
/// Placeholder handler - will be implemented when migrating from adapteros-server-api
pub async fn get_model_status(Path(model_id): Path<Uuid>) -> Result<Json<serde_json::Value>, StatusCode> {
    tracing::debug!(model_id = %model_id, "Getting model status (placeholder)");
    Err(StatusCode::NOT_FOUND)
}
