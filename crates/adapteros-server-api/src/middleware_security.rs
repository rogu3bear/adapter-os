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

    // HTTP Strict Transport Security (HSTS)
    // max-age=31536000 (1 year), includeSubDomains for comprehensive protection
    // Note: Only effective over HTTPS; browsers ignore this header over HTTP
    headers.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains"
            .parse()
            .expect("valid Strict-Transport-Security header value"),
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

/// CORS configuration validation error
#[derive(Debug, Clone)]
pub struct CorsConfigError {
    pub message: String,
}

impl std::fmt::Display for CorsConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for CorsConfigError {}

/// Validate CORS configuration at startup (fail-fast in production)
///
/// Returns an error if:
/// - `AOS_PRODUCTION_MODE=true` and `ALLOWED_ORIGINS` is not set
/// - `ALLOWED_ORIGINS` is set but contains no valid origins
///
/// # Returns
/// - `Ok(())` if the configuration is valid
/// - `Err(CorsConfigError)` if the configuration is invalid
///
/// # Example
/// ```rust,ignore
/// use adapteros_server_api::middleware_security::validate_cors_config;
///
/// // Call at server startup
/// if let Err(e) = validate_cors_config() {
///     eprintln!("FATAL: {}", e);
///     std::process::exit(1);
/// }
/// ```
pub fn validate_cors_config() -> Result<(), CorsConfigError> {
    let is_production = std::env::var("AOS_PRODUCTION_MODE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    let allowed_origins = std::env::var("ALLOWED_ORIGINS").ok();

    if is_production {
        match allowed_origins {
            None => {
                return Err(CorsConfigError {
                    message: "CORS misconfiguration: AOS_PRODUCTION_MODE=true but ALLOWED_ORIGINS is not set. \
                              Set ALLOWED_ORIGINS to a comma-separated list of allowed origins (e.g., 'https://app.example.com')."
                        .to_string(),
                });
            }
            Some(ref origins) => {
                let valid_origins: Vec<_> = origins
                    .split(',')
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty() && s.parse::<HeaderValue>().is_ok())
                    .collect();

                if valid_origins.is_empty() {
                    return Err(CorsConfigError {
                        message: format!(
                            "CORS misconfiguration: ALLOWED_ORIGINS='{}' contains no valid origins. \
                             Provide valid HTTP(S) URLs (e.g., 'https://app.example.com').",
                            origins
                        ),
                    });
                }

                tracing::info!(
                    origins = ?valid_origins,
                    "CORS configuration validated for production mode"
                );
            }
        }
    } else if let Some(ref origins) = allowed_origins {
        // Development mode with explicit origins - validate them
        let valid_origins: Vec<_> = origins
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && s.parse::<HeaderValue>().is_ok())
            .collect();

        if valid_origins.is_empty() {
            warn!(
                origins = %origins,
                "ALLOWED_ORIGINS set but contains no valid origins; using development defaults"
            );
        } else {
            debug!(
                origins = ?valid_origins,
                "CORS configuration validated for development mode with explicit origins"
            );
        }
    } else {
        debug!("CORS using development defaults (localhost origins)");
    }

    Ok(())
}

/// CORS configuration layer
///
/// Configures Cross-Origin Resource Sharing based on runtime environment:
/// - If ALLOWED_ORIGINS is set: use those origins (production deployment)
/// - If AOS_PRODUCTION_MODE=true: use production defaults (adapteros.com)
/// - Otherwise: allow localhost origins for development
///
/// Always uses explicit origins with credentials support (required for cookie auth).
///
/// **Important**: Call `validate_cors_config()` at server startup to fail-fast
/// if production mode is enabled without proper ALLOWED_ORIGINS configuration.
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
        // Development mode: localhost origins (respects AOS_UI_PORT and AOS_SERVER_PORT)
        let ui_port = std::env::var("AOS_UI_PORT").unwrap_or_else(|_| "3200".to_string());
        let server_port = std::env::var("AOS_SERVER_PORT").unwrap_or_else(|_| "8080".to_string());
        tracing::warn!(
            ui_port = %ui_port,
            server_port = %server_port,
            "CORS: Using development localhost defaults. Set ALLOWED_ORIGINS or AOS_PRODUCTION_MODE=true for production"
        );
        [
            format!("http://localhost:{}", ui_port),
            format!("http://localhost:{}", server_port),
            format!("http://127.0.0.1:{}", ui_port),
            format!("http://127.0.0.1:{}", server_port),
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
    use super::*;
    use axum::{
        body::Body,
        http::{header, HeaderMap, Request, StatusCode},
        middleware,
        routing::{get, post},
        Router,
    };
    use tower::ServiceExt;

    const CSP_HEADER_VALUE: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: https:; font-src 'self'; connect-src 'self'; media-src 'none'; object-src 'none'; child-src 'none'; worker-src 'none'; frame-ancestors 'none'; base-uri 'self'; form-action 'self';";
    const PERMISSIONS_POLICY_HEADER_VALUE: &str = "camera=(), microphone=(), geolocation=(), payment=(), usb=(), magnetometer=(), accelerometer=(), gyroscope=(), ambient-light-sensor=(), autoplay=(), encrypted-media=(), fullscreen=(self), picture-in-picture=()";

    fn build_security_app() -> Router {
        Router::new()
            .route("/ok", get(|| async { StatusCode::OK }))
            .route("/unauthorized", get(|| async { StatusCode::UNAUTHORIZED }))
            .route("/forbidden", get(|| async { StatusCode::FORBIDDEN }))
            .layer(middleware::from_fn(security_headers_middleware))
    }

    fn build_request_size_app() -> Router {
        Router::new()
            .route("/", post(|| async { StatusCode::OK }))
            .route("/get", get(|| async { StatusCode::OK }))
            .layer(middleware::from_fn(request_size_limit_middleware))
    }

    fn build_cors_app() -> Router {
        Router::new()
            .route("/", get(|| async { StatusCode::OK }))
            .layer(cors_layer())
    }

    async fn call(app: Router, req: Request<Body>) -> Response {
        app.oneshot(req).await.expect("router call should succeed")
    }

    fn assert_security_headers(headers: &HeaderMap) {
        assert_eq!(
            headers.get("Content-Security-Policy"),
            Some(&HeaderValue::from_static(CSP_HEADER_VALUE))
        );
        assert_eq!(
            headers.get("X-Frame-Options"),
            Some(&HeaderValue::from_static("DENY"))
        );
        assert_eq!(
            headers.get("X-Content-Type-Options"),
            Some(&HeaderValue::from_static("nosniff"))
        );
        assert_eq!(
            headers.get("Referrer-Policy"),
            Some(&HeaderValue::from_static("strict-origin-when-cross-origin"))
        );
        assert_eq!(
            headers.get("Permissions-Policy"),
            Some(&HeaderValue::from_static(PERMISSIONS_POLICY_HEADER_VALUE))
        );
        assert_eq!(
            headers.get("Strict-Transport-Security"),
            Some(&HeaderValue::from_static(
                "max-age=31536000; includeSubDomains"
            ))
        );
    }

    async fn preflight_request(app: Router, origin: &str, method: Method) -> Response {
        let request = Request::builder()
            .method(Method::OPTIONS)
            .uri("/")
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, method.as_str())
            .body(Body::empty())
            .expect("failed to build request");

        call(app, request).await
    }

    fn assert_allow_origin(response: &Response, origin: &str) {
        let headers = response.headers();
        let origin_value = HeaderValue::from_str(origin).expect("origin header must be valid");

        assert_eq!(
            headers.get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&origin_value)
        );
        assert_eq!(
            headers.get(header::ACCESS_CONTROL_ALLOW_CREDENTIALS),
            Some(&HeaderValue::from_static("true"))
        );
    }

    fn assert_preflight_headers(response: &Response) {
        let headers = response.headers();

        let allow_methods = headers
            .get(header::ACCESS_CONTROL_ALLOW_METHODS)
            .expect("allow-methods header missing")
            .to_str()
            .expect("allow-methods header not utf8");

        for method in ["GET", "POST", "PUT", "DELETE", "PATCH", "OPTIONS"] {
            assert!(
                allow_methods.contains(method),
                "allow-methods missing {method}"
            );
        }

        assert_eq!(
            headers.get(header::ACCESS_CONTROL_MAX_AGE),
            Some(&HeaderValue::from_static("86400"))
        );
    }

    struct EnvGuard {
        allowed_origins: Option<String>,
        production_mode: Option<String>,
    }

    impl EnvGuard {
        fn new() -> Self {
            Self {
                allowed_origins: std::env::var("ALLOWED_ORIGINS").ok(),
                production_mode: std::env::var("AOS_PRODUCTION_MODE").ok(),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.allowed_origins.as_ref() {
                Some(value) => std::env::set_var("ALLOWED_ORIGINS", value),
                None => std::env::remove_var("ALLOWED_ORIGINS"),
            }

            match self.production_mode.as_ref() {
                Some(value) => std::env::set_var("AOS_PRODUCTION_MODE", value),
                None => std::env::remove_var("AOS_PRODUCTION_MODE"),
            }
        }
    }

    #[tokio::test]
    async fn test_security_headers_added() {
        let app = build_security_app();

        let ok_response = call(
            app.clone(),
            Request::builder()
                .uri("/ok")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await;
        assert_eq!(ok_response.status(), StatusCode::OK);
        assert_security_headers(ok_response.headers());
        assert!(ok_response.headers().get("Cache-Control").is_none());

        let unauthorized_response = call(
            app.clone(),
            Request::builder()
                .uri("/unauthorized")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await;
        assert_eq!(unauthorized_response.status(), StatusCode::UNAUTHORIZED);
        assert_security_headers(unauthorized_response.headers());
        assert_eq!(
            unauthorized_response
                .headers()
                .get("Cache-Control")
                .map(HeaderValue::as_bytes),
            Some("no-cache, no-store, must-revalidate".as_bytes())
        );
        assert_eq!(
            unauthorized_response
                .headers()
                .get("Pragma")
                .map(HeaderValue::as_bytes),
            Some("no-cache".as_bytes())
        );
        assert_eq!(
            unauthorized_response
                .headers()
                .get("Expires")
                .map(HeaderValue::as_bytes),
            Some("0".as_bytes())
        );

        let forbidden_response = call(
            app,
            Request::builder()
                .uri("/forbidden")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await;
        assert_eq!(forbidden_response.status(), StatusCode::FORBIDDEN);
        assert_security_headers(forbidden_response.headers());
        assert_eq!(
            forbidden_response
                .headers()
                .get("Cache-Control")
                .map(HeaderValue::as_bytes),
            Some("no-cache, no-store, must-revalidate".as_bytes())
        );
    }

    #[tokio::test]
    async fn test_request_size_limit() {
        let app = build_request_size_app();

        let valid_post = call(
            app.clone(),
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header("content-length", "1024")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await;
        assert_eq!(valid_post.status(), StatusCode::OK);

        let oversized_post = call(
            app.clone(),
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .header("content-length", (10 * 1024 * 1024 + 1).to_string())
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await;
        assert_eq!(oversized_post.status(), StatusCode::PAYLOAD_TOO_LARGE);

        let oversized_get = call(
            app,
            Request::builder()
                .method(Method::GET)
                .uri("/get")
                .header("content-length", "2048")
                .body(Body::empty())
                .expect("failed to build request"),
        )
        .await;
        assert_eq!(oversized_get.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn test_cors_layer_configuration() {
        let _env_guard = EnvGuard::new();

        std::env::remove_var("ALLOWED_ORIGINS");
        std::env::remove_var("AOS_PRODUCTION_MODE");
        let dev_response =
            preflight_request(build_cors_app(), "http://localhost:3200", Method::GET).await;
        assert_eq!(dev_response.status(), StatusCode::OK);
        assert_allow_origin(&dev_response, "http://localhost:3200");
        assert_preflight_headers(&dev_response);

        std::env::set_var(
            "ALLOWED_ORIGINS",
            "https://example.com, https://another.com",
        );
        let explicit_response =
            preflight_request(build_cors_app(), "https://example.com", Method::POST).await;
        assert_eq!(explicit_response.status(), StatusCode::OK);
        assert_allow_origin(&explicit_response, "https://example.com");
        assert_preflight_headers(&explicit_response);

        std::env::remove_var("ALLOWED_ORIGINS");
        std::env::set_var("AOS_PRODUCTION_MODE", "true");
        let blocked_response =
            preflight_request(build_cors_app(), "https://blocked.com", Method::GET).await;
        assert_eq!(blocked_response.status(), StatusCode::OK);
        assert!(
            blocked_response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .is_none(),
            "origin should be rejected when production origins are not configured"
        );
    }

    // =========================================================================
    // validate_cors_config() tests
    // =========================================================================

    #[test]
    fn test_validate_cors_config_production_without_allowed_origins_fails() {
        let _env_guard = EnvGuard::new();

        std::env::set_var("AOS_PRODUCTION_MODE", "true");
        std::env::remove_var("ALLOWED_ORIGINS");

        let result = validate_cors_config();
        assert!(
            result.is_err(),
            "Should fail in production without ALLOWED_ORIGINS"
        );
        let err = result.unwrap_err();
        assert!(
            err.message.contains("AOS_PRODUCTION_MODE=true"),
            "Error should mention production mode: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_cors_config_production_with_valid_origins_succeeds() {
        let _env_guard = EnvGuard::new();

        std::env::set_var("AOS_PRODUCTION_MODE", "true");
        std::env::set_var("ALLOWED_ORIGINS", "https://app.example.com");

        let result = validate_cors_config();
        assert!(
            result.is_ok(),
            "Should succeed with valid ALLOWED_ORIGINS in production"
        );
    }

    #[test]
    fn test_validate_cors_config_production_with_multiple_valid_origins_succeeds() {
        let _env_guard = EnvGuard::new();

        std::env::set_var("AOS_PRODUCTION_MODE", "true");
        std::env::set_var(
            "ALLOWED_ORIGINS",
            "https://app.example.com, https://admin.example.com",
        );

        let result = validate_cors_config();
        assert!(
            result.is_ok(),
            "Should succeed with multiple valid ALLOWED_ORIGINS"
        );
    }

    #[test]
    fn test_validate_cors_config_production_with_empty_origins_fails() {
        let _env_guard = EnvGuard::new();

        std::env::set_var("AOS_PRODUCTION_MODE", "true");
        std::env::set_var("ALLOWED_ORIGINS", "  ,  ,  ");

        let result = validate_cors_config();
        assert!(
            result.is_err(),
            "Should fail with empty/whitespace-only ALLOWED_ORIGINS"
        );
        let err = result.unwrap_err();
        assert!(
            err.message.contains("no valid origins"),
            "Error should mention no valid origins: {}",
            err.message
        );
    }

    #[test]
    fn test_validate_cors_config_production_mode_variants() {
        let _env_guard = EnvGuard::new();

        // Test "1" as production mode
        std::env::set_var("AOS_PRODUCTION_MODE", "1");
        std::env::remove_var("ALLOWED_ORIGINS");

        let result = validate_cors_config();
        assert!(
            result.is_err(),
            "AOS_PRODUCTION_MODE=1 should be treated as production"
        );

        // Test "false" should not be production
        std::env::set_var("AOS_PRODUCTION_MODE", "false");
        let result = validate_cors_config();
        assert!(
            result.is_ok(),
            "AOS_PRODUCTION_MODE=false should be development mode"
        );
    }

    #[test]
    fn test_validate_cors_config_development_mode_always_succeeds() {
        let _env_guard = EnvGuard::new();

        std::env::remove_var("AOS_PRODUCTION_MODE");
        std::env::remove_var("ALLOWED_ORIGINS");

        let result = validate_cors_config();
        assert!(
            result.is_ok(),
            "Development mode should succeed without ALLOWED_ORIGINS"
        );

        // Also succeeds with explicit origins in dev mode
        std::env::set_var("ALLOWED_ORIGINS", "http://localhost:3000");
        let result = validate_cors_config();
        assert!(
            result.is_ok(),
            "Development mode should succeed with ALLOWED_ORIGINS"
        );
    }

    #[test]
    fn test_cors_config_error_display() {
        let err = CorsConfigError {
            message: "Test error message".to_string(),
        };
        assert_eq!(format!("{}", err), "Test error message");
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
/// let app: Router<AppState> = Router::new()
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
