//! Seed isolation middleware for deterministic inference.
//!
//! This middleware ensures thread-local seed state isolation at request boundaries.
//! It prevents cross-request seed state leakage which would break deterministic replay.
//!
//! # Determinism Guarantee
//!
//! Thread-local seed state from a previous request can corrupt determinism by:
//! - Carrying over nonce counters, causing different seed derivations
//! - Leaking tenant context across request boundaries
//! - Making replay non-deterministic if prior state affects seed derivation
//!
//! # Usage
//!
//! This middleware should be applied early in the middleware stack (after request ID
//! but before authentication and context middleware) to ensure clean state before
//! any seed-dependent operations.
//!
//! ```ignore
//! use axum::middleware;
//! use crate::middleware::seed_isolation::seed_isolation_middleware;
//!
//! let app = Router::new()
//!     .route("/api/infer", post(infer_handler))
//!     .layer(middleware::from_fn(seed_isolation_middleware));
//! ```
//!
//! # Debug Assertions
//!
//! In debug builds, the middleware will panic if it detects leaked thread-local state.
//! This fails fast to catch determinism bugs during development.
//!
//! In release builds, the middleware silently resets state with near-zero overhead.

use adapteros_core::seed_override::{assert_thread_local_clean, reset_thread_local_state};
#[cfg(not(debug_assertions))]
use adapteros_core::seed_override::{get_leaked_state_info, is_thread_local_clean};
use axum::{extract::Request, middleware::Next, response::Response};

/// Middleware that enforces thread-local seed state isolation at request boundaries.
///
/// This middleware:
/// 1. Resets all thread-local seed state at the start of each request
/// 2. In debug builds, asserts that state was clean (panics on leakage)
/// 3. In release builds, silently resets with near-zero overhead
///
/// # Panics (Debug Builds Only)
///
/// Panics if thread-local seed state is not clean at request entry.
/// This indicates a determinism bug where a previous request leaked state.
pub async fn seed_isolation_middleware(req: Request, next: Next) -> Response {
    // Check for leaked state before resetting (diagnostic logging in non-debug)
    #[cfg(not(debug_assertions))]
    {
        if !is_thread_local_clean() {
            if let Some(info) = get_leaked_state_info() {
                tracing::warn!(
                    target: "determinism.seed_isolation",
                    tenant_id = ?info.tenant_id,
                    request_id = ?info.request_id,
                    nonce_counter = ?info.nonce_counter,
                    "Thread-local seed state leaked from previous request (cleaned)"
                );
            }
        }
    }

    // In debug builds, this will panic on leakage
    // In release builds, this is a no-op
    assert_thread_local_clean();

    // Reset all thread-local seed state to ensure clean slate
    reset_thread_local_state();

    // Process the request
    let response = next.run(req).await;

    // Clear state at the end of request processing (belt and suspenders)
    // This ensures we don't leak even if an async boundary causes re-entry
    reset_thread_local_state();

    response
}

/// Lightweight version of seed isolation middleware for high-traffic endpoints.
///
/// This variant skips the debug assertions and leaked state diagnostics,
/// making it suitable for endpoints where minimal overhead is critical.
///
/// Use this for health checks, metrics endpoints, or other high-frequency routes
/// that don't involve seed-dependent operations but still want isolation.
pub async fn seed_isolation_middleware_fast(req: Request, next: Next) -> Response {
    // Unconditionally reset without diagnostics
    reset_thread_local_state();

    let response = next.run(req).await;

    reset_thread_local_state();

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::seed_override::{
        is_thread_local_clean, set_thread_seed_context, SeedContext,
    };
    use adapteros_core::{seed::SeedMode, B3Hash};
    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    async fn dummy_handler() -> StatusCode {
        StatusCode::OK
    }

    fn test_app() -> Router {
        Router::new()
            .route("/", get(dummy_handler))
            .layer(axum::middleware::from_fn(seed_isolation_middleware))
    }

    fn fast_test_app() -> Router {
        Router::new()
            .route("/", get(dummy_handler))
            .layer(axum::middleware::from_fn(seed_isolation_middleware_fast))
    }

    #[tokio::test]
    async fn test_middleware_resets_clean_state() {
        // Start with clean state
        reset_thread_local_state();
        assert!(is_thread_local_clean());

        let req = HttpRequest::builder().uri("/").body(Body::empty()).unwrap();

        let resp = test_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // State should still be clean after request
        assert!(is_thread_local_clean());
    }

    #[tokio::test]
    async fn test_fast_middleware_resets_dirty_state() {
        // Pollute the thread-local state
        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(
            global,
            None,
            SeedMode::BestEffort,
            1,
            "leaked-tenant".to_string(),
        );
        set_thread_seed_context(ctx);
        assert!(!is_thread_local_clean());

        let req = HttpRequest::builder().uri("/").body(Body::empty()).unwrap();

        let resp = fast_test_app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // State should be clean after request
        assert!(is_thread_local_clean());
    }

    #[tokio::test]
    async fn test_sequential_requests_isolated() {
        reset_thread_local_state();

        // First request
        let req1 = HttpRequest::builder().uri("/").body(Body::empty()).unwrap();

        let app = test_app();
        let resp1 = app.clone().oneshot(req1).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);

        // Simulate state being set during request processing
        // (In real usage, this would be done by downstream handlers)
        let global = B3Hash::hash(b"test-global");
        let ctx = SeedContext::new(
            global,
            None,
            SeedMode::BestEffort,
            1,
            "tenant-1".to_string(),
        );
        set_thread_seed_context(ctx);

        // Second request should start with clean state
        let req2 = HttpRequest::builder().uri("/").body(Body::empty()).unwrap();

        // The fast middleware won't panic, just reset
        let resp2 = fast_test_app().oneshot(req2).await.unwrap();
        assert_eq!(resp2.status(), StatusCode::OK);

        assert!(is_thread_local_clean());
    }
}
