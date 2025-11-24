//! Request ID middleware for AdapterOS
//!
//! Generates unique request IDs for all requests to enable tracing
//! and error correlation. Request IDs are:
//! - Added to response headers (X-Request-ID)
//! - Logged with all request processing
//! - Included in error responses for support correlation
//!
//! [source: crates/adapteros-server-api/src/middleware/request_id.rs]

use axum::{
    extract::Request,
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use tracing::Span;
use uuid::Uuid;

/// Extension type to store request ID
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

impl RequestId {
    /// Generate a new request ID
    pub fn new() -> Self {
        RequestId(Uuid::new_v4().to_string())
    }

    /// Extract from existing header value
    pub fn from_header(value: &str) -> Self {
        RequestId(value.to_string())
    }

    /// Get the ID as a string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
    }
}

/// Request ID middleware
///
/// Extracts or generates a request ID for each request:
/// - If X-Request-ID header is present, uses that value
/// - Otherwise generates a new UUID
/// - Adds request ID to request extensions for handler access
/// - Adds X-Request-ID header to response
/// - Records request ID in tracing span
pub async fn request_id_middleware(mut req: Request, next: Next) -> Response {
    // Extract or generate request ID
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .map(RequestId::from_header)
        .unwrap_or_else(RequestId::new);

    // Record in tracing span
    Span::current().record("request_id", request_id.as_str());

    // Add to request extensions for handler access
    req.extensions_mut().insert(request_id.clone());

    // Process request
    let mut response = next.run(req).await;

    // Add request ID to response headers
    if let Ok(header_value) = HeaderValue::from_str(request_id.as_str()) {
        response.headers_mut().insert("X-Request-ID", header_value);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_id_generation() {
        let id1 = RequestId::new();
        let id2 = RequestId::new();

        // IDs should be unique
        assert_ne!(id1.as_str(), id2.as_str());

        // IDs should be valid UUIDs
        assert!(Uuid::parse_str(id1.as_str()).is_ok());
        assert!(Uuid::parse_str(id2.as_str()).is_ok());
    }

    #[test]
    fn test_request_id_from_header() {
        let custom_id = "custom-request-id-123";
        let id = RequestId::from_header(custom_id);
        assert_eq!(id.as_str(), custom_id);
    }
}
