//! Idempotency middleware for mutation endpoints.
//!
//! This middleware ensures that requests with the same `Idempotency-Key` header
//! receive the same response, preventing duplicate side effects from retries.

use super::store::IdempotencyStore;
use super::types::{
    CachedResponse, IdempotencyKey, IdempotencyStatus, IDEMPOTENCY_KEY_HEADER, MAX_CACHED_BODY_SIZE,
};
use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Request, Response, StatusCode},
    middleware::Next,
};
use bytes::Bytes;
use http_body_util::BodyExt;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// Idempotency middleware for handling duplicate mutation requests.
///
/// This middleware:
/// 1. Only applies to mutation methods (POST, PUT, PATCH, DELETE)
/// 2. Extracts the `Idempotency-Key` header
/// 3. Returns cached responses for completed requests
/// 4. Waits for in-progress requests to complete
/// 5. Caches successful responses (2xx and 4xx)
/// 6. Allows retry on 5xx errors
pub async fn idempotency_middleware(
    store: Arc<IdempotencyStore>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let method = req.method().clone();

    // Only apply to mutating methods
    if !matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    ) {
        return next.run(req).await;
    }

    // Extract idempotency key from header
    let key = match req.headers().get(IDEMPOTENCY_KEY_HEADER) {
        Some(value) => match value.to_str() {
            Ok(s) if !s.is_empty() && s.len() <= 256 => IdempotencyKey::new(s),
            Ok("") => {
                // Empty key - proceed without idempotency
                return next.run(req).await;
            }
            Ok(_) => {
                // Key too long
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "IDEMPOTENCY_KEY_TOO_LONG",
                    "Idempotency-Key must be 256 characters or less",
                );
            }
            Err(_) => {
                // Invalid UTF-8
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "INVALID_IDEMPOTENCY_KEY",
                    "Idempotency-Key must be valid UTF-8",
                );
            }
        },
        None => {
            // No key provided - proceed without idempotency
            return next.run(req).await;
        }
    };

    // Check cache status
    match store.check(&key) {
        IdempotencyStatus::Completed => {
            // Return cached response
            if let Some(cached) = store.get_response(&key) {
                debug!(key = %key.as_str(), "Returning cached idempotent response");
                return build_response_from_cache(cached);
            }
            // Cache miss (expired) - allow retry
            debug!(key = %key.as_str(), "Cached response expired, allowing retry");
        }
        IdempotencyStatus::InProgress => {
            // Wait for the other request to complete
            if let Some(lock) = store.get_lock(&key) {
                debug!(key = %key.as_str(), "Waiting for in-progress request to complete");

                // Wait with timeout to prevent indefinite blocking
                let wait_result = tokio::time::timeout(Duration::from_secs(30), async {
                    let _guard = lock.read().await;
                })
                .await;

                if wait_result.is_err() {
                    warn!(key = %key.as_str(), "Timeout waiting for in-progress request");
                    return error_response(
                        StatusCode::CONFLICT,
                        "IDEMPOTENCY_TIMEOUT",
                        "Timeout waiting for original request to complete",
                    );
                }

                // Check again after waiting
                if let Some(cached) = store.get_response(&key) {
                    debug!(key = %key.as_str(), "Returning cached response after wait");
                    return build_response_from_cache(cached);
                }
            }

            // Still no response - return conflict
            return error_response(
                StatusCode::CONFLICT,
                "IDEMPOTENCY_CONFLICT",
                "Request with this Idempotency-Key is already in progress",
            );
        }
        IdempotencyStatus::New => {
            // New request - mark as in-progress
            if !store.mark_in_progress(&key) {
                // Race condition - another request got there first
                return error_response(
                    StatusCode::CONFLICT,
                    "IDEMPOTENCY_CONFLICT",
                    "Request with this Idempotency-Key is already in progress",
                );
            }
        }
    }

    // Execute the actual request
    let response = next.run(req).await;
    let status = response.status();

    // Cache successful and client error responses (deterministic outcomes)
    // Do NOT cache 5xx errors - allow retry
    if status.is_success() || status.is_client_error() {
        let (parts, body) = response.into_parts();

        // Collect body bytes
        let bytes = match body.collect().await {
            Ok(collected) => collected.to_bytes(),
            Err(e) => {
                warn!(error = %e, "Failed to collect response body for caching");
                store.remove(&key);
                return Response::from_parts(parts, Body::empty());
            }
        };

        // Only cache if body is under size limit
        if bytes.len() <= MAX_CACHED_BODY_SIZE {
            let cached = CachedResponse {
                status_code: parts.status.as_u16(),
                headers: parts
                    .headers
                    .iter()
                    .filter_map(|(k, v)| {
                        v.to_str()
                            .ok()
                            .map(|v| (k.as_str().to_string(), v.to_string()))
                    })
                    .collect(),
                body: bytes.to_vec(),
                created_at: chrono::Utc::now().timestamp(),
            };
            store.store_response(&key, cached);
        } else {
            debug!(
                key = %key.as_str(),
                body_size = bytes.len(),
                "Response too large to cache"
            );
            store.remove(&key);
        }

        // Rebuild response
        let mut response = Response::from_parts(parts, Body::from(bytes));

        // Add header indicating this was processed idempotently
        response
            .headers_mut()
            .insert("X-Idempotency-Cached", HeaderValue::from_static("false"));

        return response;
    }

    // 5xx error - remove from cache to allow retry
    if status.is_server_error() {
        debug!(key = %key.as_str(), status = %status, "Removing failed request from idempotency cache");
        store.remove(&key);
    }

    response
}

/// Build a response from cached data
fn build_response_from_cache(cached: CachedResponse) -> Response<Body> {
    let status = StatusCode::from_u16(cached.status_code).unwrap_or(StatusCode::OK);

    let mut builder = Response::builder().status(status);

    for (key, value) in cached.headers {
        if let Ok(v) = HeaderValue::from_str(&value) {
            builder = builder.header(&key, v);
        }
    }

    // Add header indicating this is a cached response
    builder = builder.header("X-Idempotency-Cached", "true");

    builder
        .body(Body::from(Bytes::from(cached.body)))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

/// Build an error response
fn error_response(status: StatusCode, code: &str, message: &str) -> Response<Body> {
    let body = serde_json::json!({
        "code": code,
        "error": message
    });

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Router};
    use std::sync::atomic::{AtomicU32, Ordering};
    use tower::ServiceExt;

    async fn counter_handler(
        counter: axum::extract::State<Arc<AtomicU32>>,
    ) -> (StatusCode, String) {
        let count = counter.fetch_add(1, Ordering::SeqCst) + 1;
        (StatusCode::OK, format!("count: {}", count))
    }

    fn test_app(counter: Arc<AtomicU32>, store: Arc<IdempotencyStore>) -> Router {
        Router::new()
            .route("/", post(counter_handler))
            .with_state(counter)
            .layer(axum::middleware::from_fn(move |req, next| {
                let store = store.clone();
                async move { idempotency_middleware(store, req, next).await }
            }))
    }

    #[tokio::test]
    async fn test_idempotent_requests_return_same_response() {
        let counter = Arc::new(AtomicU32::new(0));
        let store = Arc::new(IdempotencyStore::new());
        let app = test_app(counter.clone(), store);

        // First request
        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(IDEMPOTENCY_KEY_HEADER, "test-key-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp1.status(), StatusCode::OK);
        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body1[..], b"count: 1");

        // Second request with same key
        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(IDEMPOTENCY_KEY_HEADER, "test-key-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp2.status(), StatusCode::OK);

        // Should have X-Idempotency-Cached header
        assert_eq!(
            resp2
                .headers()
                .get("X-Idempotency-Cached")
                .map(|v| v.to_str().unwrap()),
            Some("true")
        );

        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body2[..], b"count: 1"); // Same response

        // Counter should only have been incremented once
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_different_keys_execute_separately() {
        let counter = Arc::new(AtomicU32::new(0));
        let store = Arc::new(IdempotencyStore::new());
        let app = test_app(counter.clone(), store);

        // First request with key-1
        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(IDEMPOTENCY_KEY_HEADER, "key-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body1[..], b"count: 1");

        // Second request with key-2
        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(IDEMPOTENCY_KEY_HEADER, "key-2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body2[..], b"count: 2");

        // Counter should have been incremented twice
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_no_key_bypasses_idempotency() {
        let counter = Arc::new(AtomicU32::new(0));
        let store = Arc::new(IdempotencyStore::new());
        let app = test_app(counter.clone(), store);

        // Request without key
        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            &resp1.into_body().collect().await.unwrap().to_bytes()[..],
            b"count: 1"
        );

        // Another request without key
        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            &resp2.into_body().collect().await.unwrap().to_bytes()[..],
            b"count: 2"
        );

        // Both should execute
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_get_requests_bypass_idempotency() {
        let counter = Arc::new(AtomicU32::new(0));
        let store = Arc::new(IdempotencyStore::new());

        let app = Router::new()
            .route(
                "/",
                axum::routing::get(|counter: axum::extract::State<Arc<AtomicU32>>| async move {
                    let count = counter.fetch_add(1, Ordering::SeqCst) + 1;
                    format!("count: {}", count)
                }),
            )
            .with_state(counter.clone())
            .layer(axum::middleware::from_fn(move |req, next| {
                let store = store.clone();
                async move { idempotency_middleware(store, req, next).await }
            }));

        // GET with idempotency key should still execute each time
        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/")
                    .header(IDEMPOTENCY_KEY_HEADER, "test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp1
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .starts_with(b"count: 1"));

        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/")
                    .header(IDEMPOTENCY_KEY_HEADER, "test-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(resp2
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .starts_with(b"count: 2"));

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
