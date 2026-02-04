//! Request ID tracking middleware
//!
//! Generates unique request IDs for tracing and error correlation
//!
//! Citations:
//! - Request ID pattern: Standard HTTP tracing practices
//! - UUID generation: RFC 4122

use crate::id_generator::readable_request_id;
use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};
use tracing::{debug, Span};

/// Request ID header name
pub const REQUEST_ID_HEADER: &str = "X-Request-ID";

thread_local! {
    /// Thread-local storage for current request ID
    static CURRENT_REQUEST_ID: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// Set the current request ID for this thread
pub fn set_request_id(id: String) {
    CURRENT_REQUEST_ID.with(|cell| {
        *cell.borrow_mut() = Some(id);
    });
}

/// Get the current request ID for this thread
pub fn get_request_id() -> Option<String> {
    CURRENT_REQUEST_ID.with(|cell| cell.borrow().clone())
}

/// Clear the current request ID for this thread
pub fn clear_request_id() {
    CURRENT_REQUEST_ID.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// Request ID middleware
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    // Check if request already has an ID from client
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(readable_request_id);

    // Store in thread-local for handlers to access
    set_request_id(request_id.clone());

    // Add to tracing span
    Span::current().record("request_id", &request_id);

    debug!(request_id = %request_id, "Processing request");

    // Add request ID to request extensions for handlers
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    // Process request
    let mut response = next.run(request).await;

    // Add request ID to response headers
    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(REQUEST_ID_HEADER, header_value);
    }

    // Clear thread-local
    clear_request_id();

    response
}

/// Request ID extension type
#[derive(Clone, Debug)]
pub struct RequestId(pub String);

/// Extension trait for extracting request ID from extensions
pub trait RequestIdExt {
    fn request_id(&self) -> Option<String>;
}

impl RequestIdExt for axum::http::Extensions {
    fn request_id(&self) -> Option<String> {
        self.get::<RequestId>().map(|r| r.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_storage() {
        let id = "test-request-123".to_string();
        set_request_id(id.clone());
        assert_eq!(get_request_id(), Some(id));
        clear_request_id();
        assert_eq!(get_request_id(), None);
    }

    #[test]
    fn test_request_id_generation() {
        let id1 = readable_request_id();
        let id2 = readable_request_id();
        assert_ne!(id1, id2);
        assert!(id1.starts_with("req."));
    }
}
