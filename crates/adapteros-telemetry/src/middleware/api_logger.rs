//! API request/response logging middleware
//!
//! Provides structured logging for all API requests with emphasis on error tracking:
//! - Request method, path, and timing
//! - Response status codes
//! - Error details for 4xx/5xx responses
//! - Request ID correlation (when available)
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_telemetry::middleware::api_logger_middleware;
//! use axum::middleware;
//!
//! let app = Router::new()
//!     .route("/health", get(health))
//!     .layer(middleware::from_fn(api_logger_middleware));
//! ```

use axum::{
    body::Body,
    http::Request,
    middleware::{from_fn, Next},
    response::Response,
};
use std::time::Instant;
use tracing::{error, info, warn};

/// Create a middleware layer for full API logging
///
/// Returns a layer that can be added to an axum Router.
///
/// # Example
/// ```rust,ignore
/// use adapteros_telemetry::middleware::api_logger_layer;
///
/// let app = Router::new()
///     .route("/health", get(health))
///     .layer(api_logger_layer());
/// ```
pub fn api_logger_layer() -> axum::middleware::FromFnLayer<
    fn(
        Request<Body>,
        Next,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send + 'static>>,
    (),
    Request<Body>,
> {
    from_fn(|req, next| Box::pin(api_logger_middleware(req, next)) as _)
}

/// Create a middleware layer for error-only API logging
///
/// Returns a layer that logs only 4xx and 5xx responses.
/// Use for high-throughput internal servers.
pub fn api_error_logger_layer() -> axum::middleware::FromFnLayer<
    fn(
        Request<Body>,
        Next,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Response> + Send + 'static>>,
    (),
    Request<Body>,
> {
    from_fn(|req, next| Box::pin(api_error_only_middleware(req, next)) as _)
}

/// Full API logging middleware
///
/// Logs all requests with timing, status, and contextual information.
/// Error responses (4xx/5xx) receive enhanced logging with full context.
///
/// # Log Levels
/// - INFO: Successful requests (2xx, 3xx)
/// - WARN: Client errors (4xx)
/// - ERROR: Server errors (5xx)
pub async fn api_logger_middleware(req: Request<Body>, next: Next) -> Response {
    let start = Instant::now();

    // Extract request metadata before consuming the request
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().map(|q| q.to_string());

    // Try to extract request ID from headers (X-Request-Id)
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".to_string());

    // Execute the request
    let response = next.run(req).await;

    // Calculate duration
    let duration_ms = start.elapsed().as_millis();
    let status = response.status();

    // Log based on status code category
    match status.as_u16() {
        500..=599 => {
            error!(
                target: "api",
                request_id = %request_id,
                method = %method,
                path = %path,
                query = ?query,
                status = %status.as_u16(),
                duration_ms = %duration_ms,
                "API error: server error"
            );
        }
        400..=499 => {
            warn!(
                target: "api",
                request_id = %request_id,
                method = %method,
                path = %path,
                query = ?query,
                status = %status.as_u16(),
                duration_ms = %duration_ms,
                "API error: client error"
            );
        }
        _ => {
            info!(
                target: "api",
                request_id = %request_id,
                method = %method,
                path = %path,
                status = %status.as_u16(),
                duration_ms = %duration_ms,
                "API request completed"
            );
        }
    }

    response
}

/// Error-only API logging middleware (lightweight variant)
///
/// Only logs 4xx and 5xx responses. Use this for high-throughput
/// internal servers where logging every request would be too verbose.
///
/// # Log Levels
/// - WARN: Client errors (4xx)
/// - ERROR: Server errors (5xx)
pub async fn api_error_only_middleware(req: Request<Body>, next: Next) -> Response {
    let start = Instant::now();

    // Extract minimal request metadata
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".to_string());

    // Execute request
    let response = next.run(req).await;
    let duration_ms = start.elapsed().as_millis();
    let status = response.status();

    // Only log errors
    if status.is_server_error() {
        error!(
            target: "api",
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration_ms,
            "API server error"
        );
    } else if status.is_client_error() {
        warn!(
            target: "api",
            request_id = %request_id,
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration_ms,
            "API client error"
        );
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::{routing::get, Router};
    use tower::ServiceExt;

    #[test]
    fn test_status_code_categories() {
        assert!(StatusCode::INTERNAL_SERVER_ERROR.is_server_error());
        assert!(StatusCode::BAD_REQUEST.is_client_error());
        assert!(StatusCode::OK.is_success());
    }

    #[tokio::test]
    async fn test_api_logger_middleware_success() {
        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(api_logger_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_api_logger_middleware_404() {
        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(api_logger_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/nonexistent")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_error_only_middleware_success() {
        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(api_error_only_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_middleware_preserves_response() {
        let app = Router::new()
            .route("/data", get(|| async { "test response body" }))
            .layer(axum::middleware::from_fn(api_logger_middleware));

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/data")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&body[..], b"test response body");
    }
}
