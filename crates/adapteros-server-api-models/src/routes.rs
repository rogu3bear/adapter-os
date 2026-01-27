//! Model API routes
//!
//! Defines the router for model-related endpoints.

use axum::{
    routing::{delete, get, post},
    Router,
};

use crate::handlers;

/// Build the models routes router
///
/// Returns an Axum Router configured with all model-related endpoints.
/// This router should be nested under `/api/models` in the main server.
///
/// # Routes
///
/// - `GET /` - List all registered models
/// - `POST /` - Register a new model
/// - `GET /:model_id` - Get information about a specific model
/// - `DELETE /:model_id` - Delete a model
/// - `GET /:model_id/status` - Get model status
pub fn models_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(handlers::list_models))
        .route("/", post(handlers::register_model))
        .route("/{model_id}", get(handlers::get_model))
        .route("/{model_id}", delete(handlers::delete_model))
        .route("/{model_id}/status", get(handlers::get_model_status))
}
