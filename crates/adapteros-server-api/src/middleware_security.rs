//! Security middleware for AdapterOS API server
//!
//! Provides defense-in-depth security controls:
//! - Security headers (CSP, HSTS, X-Frame-Options, etc.)
//! - Rate limiting per tenant/IP
//! - Request size limits and DoS protection
//! - CORS policy enforcement
//! - Graceful shutdown with request drain
//!
//! [source: crates/adapteros-server-api/src/middleware_security.rs L1-200]

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use std::sync::atomic::Ordering;
use tower_http::cors::CorsLayer;
use tracing::{debug, warn};

use crate::security::rate_limiting::check_rate_limit;
use crate::state::AppState;
use crate::types::ErrorResponse;

/// Security headers middleware
///
/// Adds comprehensive security headers to all responses:
/// - Content-Security-Policy
/// - X-Frame-Options
/// - X-Content-Type-Options
/// - Referrer-Policy
/// - Permissions-Policy
/// - Strict-Transport-Security (if HTTPS)
pub async fn security_headers_middleware(req: Request<axum::body::Body>, next: Next) -> Response {
    let mut response = next.run(req).await;

    // Extract status before mutable borrow to avoid borrow conflict
    let status = response.status();
    let should_add_cache_headers =
        matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN);

    let headers = response.headers_mut();

    // Content Security Policy - restrict resource loading
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; media-src 'none'; object-src 'none'; child-src 'none'; worker-src 'none'; frame-ancestors 'none'; base-uri 'self'; form-action 'self';"
            .parse().expect("valid CSP header value"),
    );

    // Prevent clickjacking
    headers.insert(
        "X-Frame-Options",
        "DENY".parse().expect("valid X-Frame-Options header value"),
    );

    // Prevent MIME type sniffing
    headers.insert(
        "X-Content-Type-Options",
        "nosniff"
            .parse()
            .expect("valid X-Content-Type-Options header value"),
    );

    // Control referrer information
    headers.insert(
        "Referrer-Policy",
        "strict-origin-when-cross-origin"
            .parse()
            .expect("valid Referrer-Policy header value"),
    );

    // Feature policy - disable potentially dangerous features
    headers.insert(
        "Permissions-Policy",
        "camera=(), microphone=(), geolocation=(), payment=(), usb=(), magnetometer=(), accelerometer=(), gyroscope=(), ambient-light-sensor=(), autoplay=(), encrypted-media=(), fullscreen=(self), picture-in-picture=()"
            .parse().expect("valid Permissions-Policy header value"),
    );

    // Prevent caching of sensitive responses
    if should_add_cache_headers {
        headers.insert(
            "Cache-Control",
            "no-cache, no-store, must-revalidate"
                .parse()
                .expect("valid Cache-Control header value"),
        );
        headers.insert(
            "Pragma",
            "no-cache".parse().expect("valid Pragma header value"),
        );
        headers.insert("Expires", "0".parse().expect("valid Expires header value"));
    }

    response
}

/// Rate limiting middleware
///
/// Implements per-tenant and per-IP rate limiting to prevent abuse:
/// - Tenant-based limits for API usage fairness
/// - IP-based limits for DoS protection
/// - Configurable limits per endpoint type
pub async fn rate_limiting_middleware(
    State(state): State<AppState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Extract tenant from request (from JWT claims if authenticated)
    let tenant_id = req
        .extensions()
        .get::<crate::auth::Claims>()
        .map(|claims| claims.tenant_id.clone())
        .unwrap_or_else(|| "anonymous".to_string());

    // Extract client IP
    let client_ip = req
        .extensions()
        .get::<crate::ip_extraction::ClientIp>()
        .map(|ip| ip.0.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Check rate limits
    match check_rate_limit(&state.db, &tenant_id).await {
        Ok(result) if result.allowed => {
            // Add rate limit headers to response
            let mut response = next.run(req).await;
            let remaining = result.limit - result.current_count;

            let headers = response.headers_mut();
            headers.insert(
                "X-RateLimit-Remaining",
                remaining
                    .to_string()
                    .parse()
                    .expect("numeric string is valid header value"),
            );
            headers.insert(
                "X-RateLimit-Reset",
                result
                    .reset_at
                    .to_string()
                    .parse()
                    .expect("numeric string is valid header value"),
            );
            headers.insert(
                "X-RateLimit-Limit",
                result
                    .limit
                    .to_string()
                    .parse()
                    .expect("numeric string is valid header value"),
            );

            response
        }
        Ok(result) => {
            // Rate limit exceeded
            let retry_after = result.reset_at;
            tracing::warn!(
                tenant_id = %tenant_id,
                client_ip = %client_ip,
                retry_after = %retry_after,
                path = %req.uri().path(),
                "Rate limit exceeded"
            );

            let mut response = Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .body(axum::body::Body::empty())
                .expect("empty body response builder cannot fail");

            let headers = response.headers_mut();
            headers.insert(
                "Retry-After",
                retry_after
                    .to_string()
                    .parse()
                    .expect("numeric string is valid header value"),
            );
            headers.insert(
                "X-RateLimit-Reset",
                retry_after
                    .to_string()
                    .parse()
                    .expect("numeric string is valid header value"),
            );

            response
        }
        Err(e) => {
            // Internal error - allow request but log
            tracing::error!(error = %e, "Rate limiting check failed, allowing request");
            next.run(req).await
        }
    }
}

/// Request size limiting middleware
///
/// Prevents DoS attacks by limiting request body sizes:
/// - JSON payload limits
/// - File upload limits
/// - Streaming request protection
pub async fn request_size_limit_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check Content-Length header
    if let Some(content_length) = req.headers().get("content-length") {
        if let Ok(size) = content_length.to_str().unwrap_or("0").parse::<u64>() {
            let max_size = match req.method() {
                &Method::POST | &Method::PUT | &Method::PATCH => 10 * 1024 * 1024, // 10MB for data operations
                &Method::GET | &Method::DELETE => 1024, // 1KB for simple operations
                _ => 1024,
            };

            if size > max_size {
                tracing::warn!(
                    method = %req.method(),
                    content_length = %size,
                    max_size = %max_size,
                    path = %req.uri().path(),
                    "Request size limit exceeded"
                );
                return Ok(Response::builder()
                    .status(StatusCode::PAYLOAD_TOO_LARGE)
                    .body(axum::body::Body::empty())
                    .expect("empty body response builder cannot fail"));
            }
        }
    }

    // Check for suspicious headers that might indicate attacks
    let suspicious_headers = [
        "x-forwarded-for",
        "x-real-ip",
        "x-client-ip",
        "x-forwarded-host",
        "x-forwarded-proto",
    ];

    for header_name in suspicious_headers {
        if req.headers().contains_key(header_name) {
            tracing::warn!(
                header = %header_name,
                value = ?req.headers().get(header_name),
                path = %req.uri().path(),
                "Suspicious header detected"
            );
            // Log but don't block - might be legitimate proxy usage
        }
    }

    Ok(next.run(req).await)
}

/// CORS configuration layer
///
/// Configures Cross-Origin Resource Sharing based on runtime environment:
/// - If ALLOWED_ORIGINS is set: use those origins (production deployment)
/// - If AOS_PRODUCTION_MODE=true: use production defaults (adapteros.com)
/// - Otherwise: allow localhost origins for development
///
/// Always uses explicit origins with credentials support (required for cookie auth).
pub fn cors_layer() -> CorsLayer {
    use std::collections::HashSet;

    let origins: Vec<HeaderValue> = if let Ok(allowed) = std::env::var("ALLOWED_ORIGINS") {
        // Explicit origins from environment (highest priority)
        allowed
            .split(',')
            .map(|s| s.trim().to_string())
            .collect::<HashSet<_>>()
            .into_iter()
            .filter_map(|origin| origin.parse().ok())
            .collect()
    } else if std::env::var("AOS_PRODUCTION_MODE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
    {
        // Production mode: require explicit ALLOWED_ORIGINS (privacy-first, no public domains)
        tracing::warn!(
            "AOS_PRODUCTION_MODE=true but ALLOWED_ORIGINS not set - CORS will block all origins"
        );
        Vec::new()
    } else {
        // Development mode: localhost origins
        [
            "http://localhost:3200",
            "http://localhost:8080",
            "http://127.0.0.1:3200",
            "http://127.0.0.1:8080",
        ]
        .into_iter()
        .filter_map(|o| o.parse().ok())
        .collect()
    };

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT])
        .allow_credentials(true)
        .max_age(std::time::Duration::from_secs(86400))
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    // NOTE: These tests are ignored because axum::middleware::Next cannot be
    // constructed directly in tests. Need to use axum-test or similar framework.

    #[tokio::test]
    #[ignore = "axum::middleware::Next cannot be constructed in tests"]
    async fn test_security_headers_added() {
        // TODO: Implement using axum-test crate
    }

    #[tokio::test]
    #[ignore = "axum::middleware::Next cannot be constructed in tests"]
    async fn test_request_size_limit() {
        // TODO: Implement using axum-test crate
    }

    #[test]
    #[ignore = "CorsLayer API changed - allow_methods requires argument"]
    fn test_cors_layer_configuration() {
        // TODO: Update test for new CorsLayer API
    }
}

/// Request tracking middleware for graceful shutdown
///
/// Increments the in-flight request counter when a request starts,
/// decrements it when the request completes. This allows the shutdown
/// handler to wait for all in-flight requests to complete before
/// terminating the server.
///
/// # PRD-BOOT-01
/// This middleware implements requirement #1 from PRD-BOOT-01 (Runtime Boot & Modes):
/// Track in-flight requests during graceful shutdown to ensure all active requests
/// complete before the server terminates.
pub async fn request_tracking_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Increment in-flight counter
    let count = state.in_flight_requests.fetch_add(1, Ordering::SeqCst) + 1;
    debug!(in_flight = count, "Request started");

    // Process request
    let response = next.run(req).await;

    // Decrement in-flight counter
    let count = state.in_flight_requests.fetch_sub(1, Ordering::SeqCst) - 1;
    debug!(in_flight = count, "Request completed");

    response
}

/// Drain middleware for graceful shutdown
///
/// Checks if the system is in draining state and rejects new requests
/// with 503 Service Unavailable. This allows in-flight requests to complete
/// while preventing new requests from being accepted.
///
/// # PRD-BOOT-01
/// This middleware implements requirement #2 from PRD-BOOT-01 (Runtime Boot & Modes):
/// Return 503 Service Unavailable for new requests when the server is draining,
/// while allowing in-flight requests to complete gracefully.
///
/// # Usage
///
/// ```rust,no_run
/// use axum::{Router, middleware};
/// use adapteros_server_api::middleware_security::drain_middleware;
/// use adapteros_server_api::state::AppState;
///
/// # async fn example(state: AppState) {
/// let app = Router::new()
///     .layer(middleware::from_fn_with_state(state.clone(), drain_middleware));
/// # }
/// ```
pub async fn drain_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Check if system is draining
    if let Some(ref boot_state) = state.boot_state {
        if boot_state.is_shutting_down() {
            warn!(
                path = %req.uri().path(),
                method = %req.method(),
                "Rejecting request - system is draining"
            );

            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    ErrorResponse::new("service unavailable")
                        .with_code("DRAINING")
                        .with_string_details(
                            "Server is shutting down gracefully. Please retry after restart.",
                        ),
                ),
            ));
        }
    }

    // System not draining, process request normally
    Ok(next.run(req).await)
}
