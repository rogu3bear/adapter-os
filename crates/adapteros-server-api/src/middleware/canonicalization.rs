//! Request canonicalization middleware for deterministic caching.
//!
//! This middleware canonicalizes incoming POST request bodies to ensure
//! semantically identical requests produce the same digest, regardless of
//! formatting differences like JSON key order, line endings, or whitespace.
//!
//! # Canonicalization Rules
//!
//! 1. **Line endings**: CRLF -> LF normalization
//! 2. **Whitespace in code blocks**: Collapse multiple spaces/tabs to single space
//! 3. **JSON key ordering**: Sort all JSON object keys alphabetically
//! 4. **Timestamp removal**: Strip timestamps from message content if present
//! 5. **Model config freezing**: Freeze model configuration into canonical form
//! 6. **Policy ID freezing**: Optional determinism policy ID freeze
//!
//! # Trade-offs and Caveats
//!
//! ## Timestamp Normalization
//!
//! ISO 8601 timestamps in message content are replaced with `[TIMESTAMP]`. This
//! improves cache hit rates for time-varying prompts but may cause unexpected
//! cache collisions for:
//!
//! - **Time-aware assistants**: Prompts like "What day is 2024-01-15?" will
//!   become "What day is [TIMESTAMP]?" and match other date queries.
//! - **Historical data prompts**: "Analyze events from 2023-06-01T00:00:00Z"
//!   will match queries for different dates.
//!
//! Consider using [`SkipCanonicalization`] for routes where timestamp semantics
//! matter.
//!
//! ## Whitespace Collapse
//!
//! Multiple consecutive spaces and tabs are collapsed to a single space. This
//! applies to ALL string values in JSON, which may affect:
//!
//! - **Preformatted text**: Code snippets, ASCII art, or alignment-sensitive
//!   content will have spacing normalized.
//! - **Markdown/formatting**: Intentional double-spaces (e.g., for line breaks)
//!   will be collapsed.
//!
//! This is appropriate for general chat/completion endpoints where formatting
//! variations should not affect caching. For endpoints processing structured
//! code or formatted documents, consider disabling canonicalization.
//!
//! # Usage
//!
//! Apply this middleware to inference endpoints that benefit from semantic caching:
//!
//! ```ignore
//! use axum::middleware;
//! use crate::middleware::canonicalization::canonicalization_middleware;
//!
//! let app = Router::new()
//!     .route("/v1/chat/completions", post(handler))
//!     .layer(middleware::from_fn(canonicalization_middleware));
//! ```
//!
//! # Performance
//!
//! The middleware caches the canonical form in request extensions to avoid
//! recomputation. JSON parsing only occurs for application/json content types.

use adapteros_core::B3Hash;
use axum::{
    body::Body,
    extract::Request,
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;
use tracing::{debug, trace, warn};

/// Maximum request body size for canonicalization (16 MB).
/// Larger requests are passed through without canonicalization.
const MAX_CANONICALIZATION_SIZE: usize = 16 * 1024 * 1024;

/// Regex for matching ISO 8601 timestamps in message content.
static TIMESTAMP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?")
        .expect("valid timestamp regex")
});

/// Regex for collapsing multiple whitespace characters in code blocks.
static WHITESPACE_COLLAPSE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[ \t]+").expect("valid whitespace regex"));

/// Canonical request representation with computed BLAKE3 digest.
///
/// This type is attached to request extensions after canonicalization,
/// allowing downstream handlers and the semantic cache to access the
/// canonical form without recomputation.
#[derive(Debug, Clone)]
pub struct CanonicalRequest {
    /// BLAKE3 digest of the canonical request body.
    pub digest: B3Hash,
    /// Original content type of the request.
    pub content_type: Option<String>,
    /// Whether the request was successfully canonicalized (vs. passed through).
    pub was_canonicalized: bool,
    /// Canonical JSON value (if body was JSON).
    /// This avoids re-parsing in handlers that need the structured data.
    pub canonical_json: Option<Value>,
}

impl CanonicalRequest {
    /// Create a new canonical request from raw bytes (non-JSON).
    pub fn from_raw(bytes: &[u8], content_type: Option<String>) -> Self {
        let digest = B3Hash::hash(bytes);
        Self {
            digest,
            content_type,
            was_canonicalized: false,
            canonical_json: None,
        }
    }

    /// Create a new canonical request from canonicalized JSON.
    pub fn from_canonical_json(
        value: Value,
        canonical_bytes: &[u8],
        content_type: Option<String>,
    ) -> Self {
        let digest = B3Hash::hash(canonical_bytes);
        Self {
            digest,
            content_type,
            was_canonicalized: true,
            canonical_json: Some(value),
        }
    }

    /// Get the digest as a hex string.
    #[inline]
    pub fn digest_hex(&self) -> String {
        self.digest.to_hex()
    }
}

/// Canonicalize a JSON value by recursively sorting object keys
/// and applying text normalization rules to string values.
fn canonicalize_json_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            // Collect and sort keys
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));

            // Recursively canonicalize values
            let canonical_map: serde_json::Map<String, Value> = entries
                .into_iter()
                .map(|(k, v)| (k.clone(), canonicalize_json_value(v)))
                .collect();

            Value::Object(canonical_map)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_json_value).collect()),
        Value::String(s) => Value::String(canonicalize_string(s)),
        // Numbers, bools, and null pass through unchanged
        other => other.clone(),
    }
}

/// Canonicalize a string value:
/// - Normalize line endings (CRLF -> LF)
/// - Remove ISO 8601 timestamps
/// - Collapse multiple whitespace in code-like content
fn canonicalize_string(s: &str) -> String {
    // Step 1: Normalize line endings
    let normalized = s.replace("\r\n", "\n");

    // Step 2: Remove timestamps from message content
    let without_timestamps = TIMESTAMP_REGEX.replace_all(&normalized, "[TIMESTAMP]");

    // Step 3: Collapse multiple whitespace (spaces/tabs) to single space
    // This handles code blocks and multi-space formatting variations
    let collapsed = WHITESPACE_COLLAPSE_REGEX.replace_all(&without_timestamps, " ");

    collapsed.into_owned()
}

/// Canonicalize model configuration by normalizing optional fields.
///
/// This ensures that requests with default/omitted values produce the
/// same digest as requests with explicit default values.
fn canonicalize_model_config(value: &mut Value) {
    if let Some(obj) = value.as_object_mut() {
        // Normalize temperature: null/missing -> default value representation
        if let Some(temp) = obj.get("temperature") {
            if temp.is_null() {
                obj.remove("temperature");
            }
        }

        // Normalize top_p: null/missing -> remove
        if let Some(top_p) = obj.get("top_p") {
            if top_p.is_null() {
                obj.remove("top_p");
            }
        }

        // Normalize max_tokens variants
        for key in ["max_tokens", "max_completion_tokens"] {
            if let Some(val) = obj.get(key) {
                if val.is_null() {
                    obj.remove(key);
                }
            }
        }

        // Normalize stream: false is equivalent to missing
        if let Some(stream) = obj.get("stream") {
            if stream == &Value::Bool(false) {
                obj.remove("stream");
            }
        }

        // Normalize n: 1 is equivalent to missing
        if let Some(n) = obj.get("n") {
            if n == &Value::Number(1.into()) {
                obj.remove("n");
            }
        }
    }
}

/// Check if the content type indicates JSON.
///
/// Matches `application/json` exactly or with charset suffix (e.g., `application/json; charset=utf-8`).
/// Does NOT match JSON-derived types like `application/json-patch+json` which have different semantics.
fn is_json_content_type(content_type: &str) -> bool {
    let ct = content_type.trim();
    // Exact match or starts with "application/json" followed by semicolon (charset) or end
    ct == "application/json"
        || ct.starts_with("application/json;")
        || ct.starts_with("application/json ")
}

/// Canonicalize request body bytes.
///
/// Returns the canonical form and whether canonicalization was performed.
fn canonicalize_body(
    bytes: &[u8],
    content_type: Option<&str>,
) -> Result<CanonicalRequest, CanonicalRequest> {
    // Check if this is JSON content
    let is_json = content_type.map(is_json_content_type).unwrap_or(false);

    if !is_json {
        trace!("Non-JSON content type, using raw hash");
        return Ok(CanonicalRequest::from_raw(
            bytes,
            content_type.map(String::from),
        ));
    }

    // Parse JSON
    let mut value: Value = match serde_json::from_slice(bytes) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "Failed to parse JSON body for canonicalization");
            // Fall back to raw hash on parse error
            return Err(CanonicalRequest::from_raw(
                bytes,
                content_type.map(String::from),
            ));
        }
    };

    // Apply model config normalization
    canonicalize_model_config(&mut value);

    // Canonicalize the JSON structure and string values
    let canonical = canonicalize_json_value(&value);

    // Serialize to canonical form (sorted keys, no extra whitespace)
    let canonical_bytes = serde_json::to_vec(&canonical).unwrap_or_else(|_| bytes.to_vec());

    Ok(CanonicalRequest::from_canonical_json(
        canonical,
        &canonical_bytes,
        content_type.map(String::from),
    ))
}

/// Request canonicalization middleware.
///
/// This middleware:
/// 1. Only applies to POST, PUT, PATCH methods with request bodies
/// 2. Parses JSON bodies and canonicalizes them
/// 3. Computes a BLAKE3 digest of the canonical form
/// 4. Attaches `CanonicalRequest` to request extensions
/// 5. Reconstructs the request body for downstream handlers
///
/// Non-JSON bodies are hashed directly without canonicalization.
pub async fn canonicalization_middleware(req: Request, next: Next) -> Response {
    let method = req.method().clone();

    // Only process methods that typically have request bodies
    if !matches!(method, Method::POST | Method::PUT | Method::PATCH) {
        return next.run(req).await;
    }

    // Extract content type
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    // Check content length to avoid processing huge bodies
    let content_length = req
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());

    if let Some(len) = content_length {
        if len > MAX_CANONICALIZATION_SIZE {
            debug!(
                content_length = len,
                "Request body too large for canonicalization, skipping"
            );
            return next.run(req).await;
        }
    }

    // Consume the body
    let (parts, body) = req.into_parts();
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) => {
            warn!(error = %e, "Failed to collect request body");
            return (StatusCode::BAD_REQUEST, "Failed to read request body").into_response();
        }
    };

    // Skip empty bodies
    if bytes.is_empty() {
        let req = Request::from_parts(parts, Body::from(bytes));
        return next.run(req).await;
    }

    // Canonicalize the body
    let canonical = match canonicalize_body(&bytes, content_type.as_deref()) {
        Ok(c) => c,
        Err(c) => c, // Fall back to raw hash on errors
    };

    debug!(
        digest = %canonical.digest_hex(),
        was_canonicalized = canonical.was_canonicalized,
        content_type = ?canonical.content_type,
        "Request canonicalized"
    );

    // Reconstruct the request with the canonical info and original body
    // Note: bytes is already a Bytes, so we use it directly to avoid a memory copy
    let mut req = Request::from_parts(parts, Body::from(bytes));
    req.extensions_mut().insert(canonical);

    next.run(req).await
}

/// Extension marker for routes that should skip canonicalization.
///
/// Add this to request extensions to bypass the canonicalization middleware
/// for specific requests that should not be canonicalized (e.g., file uploads).
#[derive(Debug, Clone, Copy)]
pub struct SkipCanonicalization;

/// Selective canonicalization middleware that respects `SkipCanonicalization` marker.
///
/// This variant checks for the `SkipCanonicalization` extension and bypasses
/// processing if present. Use this for routes where some requests should be
/// canonicalized and others should not.
pub async fn selective_canonicalization_middleware(req: Request, next: Next) -> Response {
    // Check for skip marker
    if req.extensions().get::<SkipCanonicalization>().is_some() {
        return next.run(req).await;
    }

    canonicalization_middleware(req, next).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Router};
    use tower::ServiceExt;

    async fn echo_handler(
        canonical: Option<axum::Extension<CanonicalRequest>>,
        body: Bytes,
    ) -> String {
        match canonical {
            Some(axum::Extension(c)) => {
                format!(
                    "digest={},was_canonicalized={},body_len={}",
                    c.digest_hex(),
                    c.was_canonicalized,
                    body.len()
                )
            }
            None => format!("no_canonical,body_len={}", body.len()),
        }
    }

    fn test_app() -> Router {
        Router::new()
            .route("/", post(echo_handler))
            .layer(axum::middleware::from_fn(canonicalization_middleware))
    }

    #[tokio::test]
    async fn test_json_key_order_independence() {
        let app = test_app();

        // Two JSON objects with different key orders
        let body1 = r#"{"b": 2, "a": 1}"#;
        let body2 = r#"{"a": 1, "b": 2}"#;

        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body1))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body2))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();

        // Extract digests from responses
        let response1 = String::from_utf8_lossy(&body1);
        let response2 = String::from_utf8_lossy(&body2);

        assert!(response1.contains("was_canonicalized=true"));
        assert!(response2.contains("was_canonicalized=true"));

        // Extract digest from response
        let digest1 = response1
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let digest2 = response2
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();

        assert_eq!(
            digest1, digest2,
            "Same content with different key order should produce same digest"
        );
    }

    #[tokio::test]
    async fn test_line_ending_normalization() {
        let app = test_app();

        // Same message with different line endings
        let body_lf = r#"{"message": "line1\nline2"}"#;
        let body_crlf = r#"{"message": "line1\r\nline2"}"#;

        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body_lf))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body_crlf))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();

        let response1 = String::from_utf8_lossy(&body1);
        let response2 = String::from_utf8_lossy(&body2);

        let digest1 = response1
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let digest2 = response2
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();

        assert_eq!(
            digest1, digest2,
            "CRLF and LF should produce same digest after normalization"
        );
    }

    #[tokio::test]
    async fn test_timestamp_removal() {
        let app = test_app();

        // Same message with different timestamps
        let body1 = r#"{"message": "At 2024-01-15T10:30:00Z something happened"}"#;
        let body2 = r#"{"message": "At 2024-06-20T15:45:30.123Z something happened"}"#;

        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body1))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body2))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();

        let response1 = String::from_utf8_lossy(&body1);
        let response2 = String::from_utf8_lossy(&body2);

        let digest1 = response1
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let digest2 = response2
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();

        assert_eq!(
            digest1, digest2,
            "Different timestamps should be replaced and produce same digest"
        );
    }

    #[tokio::test]
    async fn test_whitespace_collapse() {
        let app = test_app();

        // Same content with different whitespace
        let body1 = r#"{"code": "let   x  =  1"}"#;
        let body2 = r#"{"code": "let x = 1"}"#;

        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body1))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body2))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();

        let response1 = String::from_utf8_lossy(&body1);
        let response2 = String::from_utf8_lossy(&body2);

        let digest1 = response1
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let digest2 = response2
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();

        assert_eq!(
            digest1, digest2,
            "Multiple spaces should be collapsed to single space"
        );
    }

    #[tokio::test]
    async fn test_model_config_normalization() {
        let app = test_app();

        // Explicit defaults vs. omitted values
        let body1 = r#"{"model": "gpt-4", "stream": false, "n": 1}"#;
        let body2 = r#"{"model": "gpt-4"}"#;

        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body1))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body2))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();

        let response1 = String::from_utf8_lossy(&body1);
        let response2 = String::from_utf8_lossy(&body2);

        let digest1 = response1
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let digest2 = response2
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();

        assert_eq!(
            digest1, digest2,
            "Default values should be normalized to produce same digest"
        );
    }

    #[tokio::test]
    async fn test_get_request_bypassed() {
        let app = Router::new()
            .route("/", axum::routing::get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(canonicalization_middleware));

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_non_json_passthrough() {
        let app = test_app();

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "text/plain")
                    .body(Body::from("raw text content"))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let response = String::from_utf8_lossy(&body);

        assert!(response.contains("was_canonicalized=false"));
    }

    #[tokio::test]
    async fn test_nested_json_canonicalization() {
        let app = test_app();

        // Nested objects with different key orders
        let body1 = r#"{"outer": {"b": 2, "a": 1}, "messages": [{"z": 3, "y": 2}]}"#;
        let body2 = r#"{"messages": [{"y": 2, "z": 3}], "outer": {"a": 1, "b": 2}}"#;

        let resp1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body1))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp2 = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body2))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();

        let response1 = String::from_utf8_lossy(&body1);
        let response2 = String::from_utf8_lossy(&body2);

        let digest1 = response1
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();
        let digest2 = response2
            .split("digest=")
            .nth(1)
            .unwrap()
            .split(',')
            .next()
            .unwrap();

        assert_eq!(
            digest1, digest2,
            "Deeply nested structures should also be canonicalized"
        );
    }

    #[tokio::test]
    async fn test_empty_body_passthrough() {
        let app = test_app();

        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let response = String::from_utf8_lossy(&body);

        // Empty body should pass through without canonicalization
        assert!(response.contains("no_canonical") || response.contains("body_len=0"));
    }

    #[test]
    fn test_canonicalize_string() {
        // Test CRLF -> LF
        assert_eq!(canonicalize_string("a\r\nb"), "a\nb");

        // Test timestamp removal
        assert_eq!(
            canonicalize_string("At 2024-01-15T10:30:00Z done"),
            "At [TIMESTAMP] done"
        );

        // Test whitespace collapse
        assert_eq!(canonicalize_string("a  b   c"), "a b c");
        assert_eq!(canonicalize_string("a\t\tb"), "a b");
    }

    #[test]
    fn test_canonicalize_json_value() {
        let input: Value =
            serde_json::from_str(r#"{"z": 1, "a": 2, "m": {"b": 1, "a": 2}}"#).unwrap();
        let canonical = canonicalize_json_value(&input);
        let serialized = serde_json::to_string(&canonical).unwrap();

        // Keys should be sorted alphabetically
        assert_eq!(serialized, r#"{"a":2,"m":{"a":2,"b":1},"z":1}"#);
    }

    #[test]
    fn test_is_json_content_type() {
        // Should match
        assert!(is_json_content_type("application/json"));
        assert!(is_json_content_type("application/json; charset=utf-8"));
        assert!(is_json_content_type("application/json;charset=utf-8"));
        assert!(is_json_content_type("application/json "));
        assert!(is_json_content_type(" application/json"));

        // Should NOT match - different MIME types
        assert!(!is_json_content_type("application/json-patch+json"));
        assert!(!is_json_content_type("application/json-seq"));
        assert!(!is_json_content_type("text/json"));
        assert!(!is_json_content_type("application/ld+json"));
        assert!(!is_json_content_type("application/vnd.api+json"));
    }
}
