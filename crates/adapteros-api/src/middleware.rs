use axum::{
    error_handling::{default_on_panic, HandleErrorLayer},
    extract::rejection::JsonRejection,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures::future::BoxFuture;
use std::pin::Pin;
use std::task::{Context, Poll};
use tower::{Layer, Service, ServiceBuilder};
use tower_http::request_id::RequestIdLayer;
use tracing::error;

// Panic recovery layer using ServiceBuilder with CatchPanicLayer
pub fn panic_recovery_layer<S>() -> impl Layer<S> + Clone + Send + 'static
where
    S: Send + 'static,
{
    ServiceBuilder::new().layer(tower::catch_panic::CatchPanicLayer::default())
}

// Extractor error layer using axum HandleErrorLayer for JsonRejection
pub fn extractor_error_layer(
) -> HandleErrorLayer<Box<dyn std::future::Future<Output = Response> + Send + Sync>> {
    HandleErrorLayer::new(|error: JsonRejection| async move {
        let (status, error_message) = match &error {
            JsonRejection::IncorrectFormat { .. } => {
                (StatusCode::BAD_REQUEST, "Invalid JSON".to_string())
            }
            JsonRejection::MissingJsonContent { .. } => {
                (StatusCode::BAD_REQUEST, "Missing JSON content".to_string())
            }
            JsonRejection::PayloadTooLarge { .. } => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "Payload too large".to_string(),
            ),
            _ => (StatusCode::BAD_REQUEST, "Invalid request".to_string()),
        };
        axum::Json(serde_json::json!({
            "error": error_message,
            "status": status.as_u16()
        }))
        .into_response()
    })
}

// Error catcher layer - simple map_err
pub fn error_catcher_layer<S>() -> impl Layer<S> + Clone + Send + 'static
where
    S: Service<http::Request<axum::body::Body>, Error = std::convert::Infallible> + Send + 'static,
{
    ServiceBuilder::new().layer(tower::layer_fn(|inner| {
        inner.map_err(|err| {
            error!("Caught error: {:?}", err);
            http::Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(axum::body::Body::from("Internal server error"))
                .unwrap()
        })
    }))
}
