//! Idempotency support for safe request retries.
//!
//! This module provides infrastructure for making API endpoints idempotent,
//! ensuring that retried requests produce the same result as the original.
//!
//! # Usage
//!
//! Clients include an `Idempotency-Key` header with a unique identifier:
//!
//! ```http
//! POST /v1/adapters HTTP/1.1
//! Idempotency-Key: 550e8400-e29b-41d4-a716-446655440000
//! Content-Type: application/json
//!
//! {"name": "my-adapter"}
//! ```
//!
//! The server caches the response and returns it for duplicate requests.
//!
//! # Behavior
//!
//! - **New request**: Executes normally, caches response
//! - **Duplicate (completed)**: Returns cached response with `X-Idempotency-Cached: true`
//! - **Duplicate (in-progress)**: Waits for original to complete, returns same response
//! - **5xx error**: Removes from cache to allow retry
//! - **Cache expiry**: 24 hours (configurable)
//!
//! # Integration
//!
//! Add the middleware to your router:
//!
//! ```ignore
//! use adapteros_server_api::idempotency::{IdempotencyStore, idempotency_middleware};
//! use std::sync::Arc;
//!
//! let store = Arc::new(IdempotencyStore::new());
//! let app = Router::new()
//!     .route("/api/resource", post(create_resource))
//!     .layer(axum::middleware::from_fn(move |req, next| {
//!         let store = store.clone();
//!         async move { idempotency_middleware(store, req, next).await }
//!     }));
//! ```

mod middleware;
mod store;
mod types;

pub use middleware::idempotency_middleware;
pub use store::IdempotencyStore;
pub use types::{
    CachedResponse, IdempotencyKey, IdempotencyStatus, IDEMPOTENCY_KEY_HEADER, IDEMPOTENCY_TTL,
    MAX_CACHED_BODY_SIZE,
};
