//! Route definitions for inference endpoints
//!
//! This module defines the Axum router configuration for inference-related endpoints.
//! The actual handlers are in `adapteros-server-api` due to tight coupling with AppState.
//!
//! # Route Summary
//!
//! | Method | Path | Handler | Auth |
//! |--------|------|---------|------|
//! | POST | `/v1/infer` | `infer` | Protected |
//! | POST | `/v1/infer/stream` | `streaming_infer` | Protected |
//! | POST | `/v1/infer/stream/progress` | `streaming_infer_with_progress` | Protected |
//! | POST | `/v1/infer/batch` | `batch_infer` | Protected |
//! | POST | `/v1/batches` | `create_batch_job` | Protected |
//! | GET | `/v1/batches/{batch_id}` | `get_batch_status` | Protected |
//! | GET | `/v1/batches/{batch_id}/items` | `get_batch_items` | Protected |
//! | GET | `/v1/inference/{trace_id}/provenance` | `get_inference_provenance` | Protected |
//!
//! # Integration
//!
//! These routes are mounted by `adapteros-server` in the finalization phase.
//! The hub crate (`adapteros-server-api`) provides the actual handler implementations
//! via its `inference_routes()` function.
//!
//! ```ignore
//! // In adapteros-server finalization:
//! let app = Router::new()
//!     .merge(adapteros_server_api::routes::inference_routes(state.clone()));
//! ```

use axum::{routing::get, routing::post, Router};

use crate::handlers::{
    batch_infer_placeholder, create_batch_job_placeholder, get_batch_items_placeholder,
    get_batch_status_placeholder, get_inference_provenance_placeholder, infer_placeholder,
    streaming_infer_placeholder, streaming_infer_with_progress_placeholder,
};

/// Build inference routes with placeholder handlers
///
/// This returns a router with placeholder handlers for documentation and testing.
/// Production deployments should use the hub crate's `inference_routes()` function
/// which provides the actual handler implementations.
///
/// # Example
///
/// ```ignore
/// use adapteros_server_api_inference::inference_routes;
///
/// // For documentation/testing only
/// let test_router = Router::new()
///     .nest("/api", inference_routes());
/// ```
pub fn inference_routes() -> Router {
    Router::new()
        // Standard inference
        .route("/v1/infer", post(infer_placeholder))
        // Streaming inference
        .route("/v1/infer/stream", post(streaming_infer_placeholder))
        .route(
            "/v1/infer/stream/progress",
            post(streaming_infer_with_progress_placeholder),
        )
        // Batch inference
        .route("/v1/infer/batch", post(batch_infer_placeholder))
        // Async batch jobs
        .route("/v1/batches", post(create_batch_job_placeholder))
        .route("/v1/batches/{batch_id}", get(get_batch_status_placeholder))
        .route(
            "/v1/batches/{batch_id}/items",
            get(get_batch_items_placeholder),
        )
        // Provenance
        .route(
            "/v1/inference/{trace_id}/provenance",
            get(get_inference_provenance_placeholder),
        )
}

/// Build inference routes with shared state
///
/// Use this variant when you need to pass application state to handlers.
/// The state type parameter allows integration with different application
/// state types while maintaining type safety.
///
/// # Note
///
/// This function returns placeholder handlers. For production use with actual
/// inference capabilities, use `adapteros-server-api::routes::inference_routes()`
/// which provides full handler implementations with InferenceCore integration.
pub fn inference_routes_with_state<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        // Standard inference
        .route("/v1/infer", post(infer_placeholder))
        // Streaming inference
        .route("/v1/infer/stream", post(streaming_infer_placeholder))
        .route(
            "/v1/infer/stream/progress",
            post(streaming_infer_with_progress_placeholder),
        )
        // Batch inference
        .route("/v1/infer/batch", post(batch_infer_placeholder))
        // Async batch jobs
        .route("/v1/batches", post(create_batch_job_placeholder))
        .route("/v1/batches/{batch_id}", get(get_batch_status_placeholder))
        .route(
            "/v1/batches/{batch_id}/items",
            get(get_batch_items_placeholder),
        )
        // Provenance
        .route(
            "/v1/inference/{trace_id}/provenance",
            get(get_inference_provenance_placeholder),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inference_routes_builds() {
        let router: Router = inference_routes();
        // Router builds without panic
        let _ = router;
    }

    #[test]
    fn test_inference_routes_with_state_builds() {
        let router: Router<()> = inference_routes_with_state();
        // Router builds without panic
        let _ = router;
    }
}
