//! Route definitions for inference endpoints.
//!
//! These routes delegate directly to production handlers from
//! `adapteros-server-api::handlers`.

use axum::{extract::FromRef, routing::get, routing::post, Router};

use adapteros_server_api::state::AppState;

use crate::handlers::{
    batch_infer, create_batch_job, get_batch_items, get_batch_status, get_inference_provenance,
    infer, streaming_infer, streaming_infer_with_progress,
};

/// Build inference routes bound to `AppState`.
pub fn inference_routes() -> Router<AppState> {
    inference_routes_with_state::<AppState>()
}

/// Build inference routes with shared outer state.
///
/// `AppState` must be derivable from `S` via `FromRef`.
pub fn inference_routes_with_state<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    AppState: FromRef<S>,
{
    Router::new()
        .route("/v1/infer", post(infer))
        .route("/v1/infer/stream", post(streaming_infer))
        .route(
            "/v1/infer/stream/progress",
            post(streaming_infer_with_progress),
        )
        .route("/v1/infer/batch", post(batch_infer))
        .route("/v1/batches", post(create_batch_job))
        .route("/v1/batches/{batch_id}", get(get_batch_status))
        .route("/v1/batches/{batch_id}/items", get(get_batch_items))
        .route(
            "/v1/inference/{trace_id}/provenance",
            get(get_inference_provenance),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inference_routes_builds() {
        let router: Router<AppState> = inference_routes();
        let _ = router;
    }

    #[test]
    fn test_inference_routes_with_state_builds() {
        let router: Router<AppState> = inference_routes_with_state();
        let _ = router;
    }
}
