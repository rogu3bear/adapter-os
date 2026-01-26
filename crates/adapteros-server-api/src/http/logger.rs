use tower::ServiceBuilder;
use tower::{Layer, Service, ServiceExt};
use tower_http::trace::TraceLayer;
use tracing::Span;

// Basic error logger layer
pub fn error_logger_layer<S>() -> impl Layer<S> + Clone + Send + 'static
where
    S: Send + 'static,
{
    ServiceBuilder::new().layer(
        TraceLayer::new_for_http()
            .on_response(
                move |response: &axum::http::Response<_>,
                      _latency: std::time::Duration,
                      spans: &[Span]| {
                    if response.status().is_client_error() || response.status().is_server_error() {
                        tracing::warn!(
                            "Error response: {} - spans: {:?}",
                            response.status(),
                            spans
                        );
                    }
                },
            )
            .on_request(move |request: &axum::http::Request<_>, _span: &Span| {
                tracing::debug!("Request: {} {}", request.method(), request.uri());
            }),
    )
}

// Custom error logger for ApiError in catcher
// But since catcher already logs, enhance if needed
// For now, OnService for request/response logging with error detection
