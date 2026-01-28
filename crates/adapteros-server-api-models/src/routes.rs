//! Model API routes
//!
//! Defines the router for model-related endpoints.
//! Migrated from adapteros-server-api for spoke pattern.

use adapteros_server_api::state::AppState;
use axum::{routing::{get, post}, Router};

use crate::handlers;

/// Build the models routes router
///
/// Returns an Axum Router configured with all model-related endpoints.
/// These routes are designed to be merged into the main server router.
///
/// # Routes
///
/// - `GET /v1/models` - List all models with statistics
/// - `POST /v1/models/import` - Import a model from disk
/// - `GET /v1/models/download-progress` - Get download/import progress
/// - `GET /v1/models/status/all` - Get all models status
/// - `POST /v1/models/{model_id}/load` - Load a model into memory
/// - `POST /v1/models/{model_id}/unload` - Unload a model from memory
/// - `GET /v1/models/{model_id}/status` - Get single model status
/// - `GET /v1/models/{model_id}/validate` - Validate model integrity
pub fn models_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/models", get(handlers::list_models_with_stats))
        .route("/v1/models/import", post(handlers::import_model))
        .route(
            "/v1/models/download-progress",
            get(handlers::get_download_progress),
        )
        .route("/v1/models/status/all", get(handlers::get_all_models_status))
        .route(
            "/v1/models/{model_id}/load",
            post(handlers::load_model),
        )
        .route(
            "/v1/models/{model_id}/unload",
            post(handlers::unload_model),
        )
        .route(
            "/v1/models/{model_id}/status",
            get(handlers::get_model_status),
        )
        .route(
            "/v1/models/{model_id}/validate",
            get(handlers::validate_model),
        )
}
