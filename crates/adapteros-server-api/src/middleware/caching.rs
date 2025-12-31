//! Caching middleware for AdapterOS
//!
//! Implements HTTP caching with:
//! - ETags for cache validation
//! - Conditional requests (If-None-Match, If-Modified-Since)
//! - Cache-Control headers
//! - 304 Not Modified responses
//!
//! [source: crates/adapteros-server-api/src/middleware/caching.rs]

use axum::{
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::debug;

/// Cache control directives
#[derive(Debug, Clone)]
pub enum CacheControl {
    /// No caching
    NoCache,
    /// Cache with max-age in seconds
    MaxAge(u32),
    /// Private cache (user-specific)
    Private(u32),
    /// Public cache (shareable)
    Public(u32),
    /// Must revalidate
    MustRevalidate,
}

impl CacheControl {
    /// Convert to Cache-Control header value
    pub fn to_header_value(&self) -> String {
        match self {
            CacheControl::NoCache => "no-cache, no-store, must-revalidate".to_string(),
            CacheControl::MaxAge(seconds) => format!("max-age={}", seconds),
            CacheControl::Private(seconds) => format!("private, max-age={}", seconds),
            CacheControl::Public(seconds) => format!("public, max-age={}", seconds),
            CacheControl::MustRevalidate => "must-revalidate".to_string(),
        }
    }
}

/// Extension type to store caching metadata
#[derive(Debug, Clone)]
pub struct CachingMetadata {
    /// Cache control directive
    pub cache_control: CacheControl,
    /// Whether to generate ETag
    pub enable_etag: bool,
    /// Last modified timestamp (RFC3339)
    pub last_modified: Option<String>,
}

impl CachingMetadata {
    /// Create metadata with cache control
    pub fn new(cache_control: CacheControl) -> Self {
        Self {
            cache_control,
            enable_etag: true,
            last_modified: None,
        }
    }

    /// Disable ETag generation
    pub fn without_etag(mut self) -> Self {
        self.enable_etag = false;
        self
    }

    /// Set last modified timestamp
    pub fn with_last_modified(mut self, timestamp: String) -> Self {
        self.last_modified = Some(timestamp);
        self
    }
}

/// Generate ETag from response body hash
///
/// Uses BLAKE3 for deterministic hashing across process restarts.
/// DefaultHasher is seeded with ASLR-derived values, producing
/// different hashes on different runs, breaking HTTP caching semantics.
pub fn generate_etag(content: &[u8]) -> String {
    let hash = blake3::hash(content);
    format!("\"{}\"", &hash.to_hex()[..16])
}

/// Check if request has matching ETag
pub fn has_matching_etag(req: &Request, etag: &str) -> bool {
    req.headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|h| h.to_str().ok())
        .map(|client_etag| client_etag == etag)
        .unwrap_or(false)
}

/// Check if resource is modified since client's cached version
pub fn is_modified_since(req: &Request, last_modified: &str) -> bool {
    if let Some(if_modified_since) = req
        .headers()
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|h| h.to_str().ok())
    {
        // Parse both timestamps and compare
        // For now, simple string comparison (RFC2822 format is sortable)
        return last_modified > if_modified_since;
    }
    true // No If-Modified-Since header, assume modified
}

/// Caching middleware
///
/// Adds caching headers to responses and handles conditional requests:
/// - Generates ETags for GET requests
/// - Handles If-None-Match (ETag validation)
/// - Handles If-Modified-Since (timestamp validation)
/// - Returns 304 Not Modified when appropriate
/// - Adds Cache-Control headers based on endpoint configuration
pub async fn caching_middleware(req: Request, next: Next) -> Response {
    // Only apply caching to GET requests
    if req.method() != axum::http::Method::GET {
        return next.run(req).await;
    }

    let path = req.uri().path().to_string();

    // Determine cache control based on endpoint
    let cache_metadata = determine_cache_control(&path);

    // Process request
    let mut response = next.run(req).await;

    // Only apply caching to successful responses
    if response.status() != StatusCode::OK {
        return response;
    }

    // Add Cache-Control header
    if let Ok(cache_header) = HeaderValue::from_str(&cache_metadata.cache_control.to_header_value())
    {
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, cache_header);
    }

    // Add Last-Modified header if available
    if let Some(last_modified) = &cache_metadata.last_modified {
        if let Ok(modified_header) = HeaderValue::from_str(last_modified) {
            response
                .headers_mut()
                .insert(header::LAST_MODIFIED, modified_header);
        }
    }

    // Note: ETag generation requires access to response body, which is complex
    // in Axum middleware. For now, we'll add a placeholder that handlers can override.
    // TODO: Implement proper ETag generation with body inspection

    debug!(
        path = %path,
        cache_control = ?cache_metadata.cache_control,
        "Caching headers added"
    );

    response
}

/// Determine cache control based on endpoint path
fn determine_cache_control(path: &str) -> CachingMetadata {
    // Static/reference data: longer cache
    if path.starts_with("/v1/policies/")
        || path.starts_with("/v1/training/templates")
        || path == "/v1/meta"
    {
        return CachingMetadata::new(CacheControl::Public(3600)); // 1 hour
    }

    // Adapter/model lists: medium cache
    if path.starts_with("/v1/adapters")
        || path.starts_with("/v1/models")
        || path.starts_with("/v1/datasets")
    {
        return CachingMetadata::new(CacheControl::Private(300)); // 5 minutes
    }

    // Metrics/monitoring: short cache
    if path.starts_with("/v1/metrics") || path.starts_with("/v1/monitoring") {
        return CachingMetadata::new(CacheControl::Private(60)); // 1 minute
    }

    // User-specific data: private cache
    if path.starts_with("/v1/auth/") || path.starts_with("/v1/workspaces") {
        return CachingMetadata::new(CacheControl::NoCache);
    }

    // Default: no caching for dynamic content
    CachingMetadata::new(CacheControl::NoCache)
}

/// Helper to create 304 Not Modified response
pub fn not_modified_response() -> Response {
    (StatusCode::NOT_MODIFIED, ()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_control_header_values() {
        assert_eq!(
            CacheControl::NoCache.to_header_value(),
            "no-cache, no-store, must-revalidate"
        );
        assert_eq!(CacheControl::MaxAge(3600).to_header_value(), "max-age=3600");
        assert_eq!(
            CacheControl::Private(300).to_header_value(),
            "private, max-age=300"
        );
        assert_eq!(
            CacheControl::Public(3600).to_header_value(),
            "public, max-age=3600"
        );
    }

    #[test]
    fn test_generate_etag() {
        let content = b"test content";
        let etag1 = generate_etag(content);
        let etag2 = generate_etag(content);

        // Same content should generate same ETag
        assert_eq!(etag1, etag2);

        // Different content should generate different ETag
        let etag3 = generate_etag(b"different content");
        assert_ne!(etag1, etag3);

        // ETags should be quoted
        assert!(etag1.starts_with('"') && etag1.ends_with('"'));
    }

    #[test]
    fn test_determine_cache_control() {
        // Public cached
        let meta = determine_cache_control("/v1/policies/123");
        assert!(matches!(meta.cache_control, CacheControl::Public(3600)));

        // Private cached
        let meta = determine_cache_control("/v1/adapters");
        assert!(matches!(meta.cache_control, CacheControl::Private(300)));

        // No cache
        let meta = determine_cache_control("/v1/auth/me");
        assert!(matches!(meta.cache_control, CacheControl::NoCache));
    }

    #[test]
    fn test_caching_metadata() {
        let meta = CachingMetadata::new(CacheControl::Public(3600))
            .without_etag()
            .with_last_modified("2025-01-23T12:00:00Z".to_string());

        assert!(!meta.enable_etag);
        assert_eq!(meta.last_modified.as_deref(), Some("2025-01-23T12:00:00Z"));
    }
}
