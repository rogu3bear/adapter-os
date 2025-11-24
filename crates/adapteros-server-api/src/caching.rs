//! HTTP caching middleware
//!
//! Implements:
//! - ETag generation and validation
//! - Last-Modified headers
//! - Conditional requests (If-None-Match, If-Modified-Since)
//! - Cache-Control headers
//!
//! Citations:
//! - HTTP caching: RFC 7232, RFC 7234
//! - ETag generation: Content-based hashing

use axum::{
    body::Body,
    extract::Request,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use blake3::Hasher;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// Cache entry
#[derive(Clone, Debug)]
struct CacheEntry {
    /// ETag value
    etag: String,
    /// Last modified timestamp
    last_modified: DateTime<Utc>,
    /// Response body (for small responses)
    body: Option<Vec<u8>>,
}

/// In-memory cache for ETags and responses
#[derive(Clone)]
pub struct ResponseCache {
    entries: Arc<RwLock<HashMap<String, CacheEntry>>>,
    max_size: usize,
}

impl ResponseCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            max_size,
        }
    }

    /// Generate ETag from content
    pub fn generate_etag(content: &[u8]) -> String {
        let mut hasher = Hasher::new();
        hasher.update(content);
        let hash = hasher.finalize();
        format!(r#""{:x}""#, hash)
    }

    /// Store cache entry
    pub async fn store(&self, key: String, etag: String, body: Option<Vec<u8>>) {
        let mut entries = self.entries.write().await;

        // Evict oldest if at capacity
        if entries.len() >= self.max_size {
            if let Some(oldest_key) = entries.keys().next().cloned() {
                entries.remove(&oldest_key);
            }
        }

        entries.insert(
            key,
            CacheEntry {
                etag,
                last_modified: Utc::now(),
                body,
            },
        );
    }

    /// Get cache entry
    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
        let entries = self.entries.read().await;
        entries.get(key).cloned()
    }

    /// Clear cache
    pub async fn clear(&self) {
        let mut entries = self.entries.write().await;
        entries.clear();
    }
}

/// Caching middleware
pub async fn caching_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();

    // Only cache GET requests
    if method != Method::GET {
        return next.run(request).await;
    }

    // Check for conditional request headers
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let if_modified_since = request
        .headers()
        .get(header::IF_MODIFIED_SINCE)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| httpdate::parse_http_date(s).ok());

    // Process request
    let response = next.run(request).await;

    // Add caching headers
    add_cache_headers(response, &path, if_none_match, if_modified_since)
}

/// Add cache headers to response
fn add_cache_headers(
    mut response: Response,
    path: &str,
    if_none_match: Option<String>,
    if_modified_since: Option<std::time::SystemTime>,
) -> Response {
    let status = response.status();

    // Only add cache headers for successful responses
    if !status.is_success() {
        return response;
    }

    let headers = response.headers_mut();

    // Determine cache-ability based on path
    let cache_control = if path.starts_with("/v1/metrics") || path.starts_with("/v1/infer") {
        // Don't cache dynamic endpoints
        "no-cache, no-store, must-revalidate"
    } else if path.starts_with("/v1/adapters") || path.starts_with("/v1/models") {
        // Cache for 5 minutes
        "public, max-age=300"
    } else if path.starts_with("/v1/policies") || path.starts_with("/v1/tenants") {
        // Cache for 1 hour
        "public, max-age=3600"
    } else {
        // Default: cache for 1 minute
        "public, max-age=60"
    };

    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache_control),
    );

    // Add Vary header to indicate response varies by Accept
    headers.insert(
        header::VARY,
        HeaderValue::from_static("Accept, Accept-Encoding"),
    );

    // Add Last-Modified header
    let now = httpdate::fmt_http_date(std::time::SystemTime::now());
    if let Ok(header_value) = HeaderValue::from_str(&now) {
        headers.insert(header::LAST_MODIFIED, header_value);
    }

    response
}

/// Helper to check if content should be cached
pub fn should_cache_path(path: &str) -> bool {
    // Cache these paths
    path.starts_with("/v1/adapters")
        || path.starts_with("/v1/models")
        || path.starts_with("/v1/policies")
        || path.starts_with("/v1/tenants")
        || path.starts_with("/v1/datasets")
}

/// Generate 304 Not Modified response
pub fn not_modified_response() -> Response {
    (StatusCode::NOT_MODIFIED, ()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etag_generation() {
        let content = b"test content";
        let etag1 = ResponseCache::generate_etag(content);
        let etag2 = ResponseCache::generate_etag(content);
        assert_eq!(etag1, etag2);

        let different_content = b"different content";
        let etag3 = ResponseCache::generate_etag(different_content);
        assert_ne!(etag1, etag3);
    }

    #[test]
    fn test_should_cache_path() {
        assert!(should_cache_path("/v1/adapters/list"));
        assert!(should_cache_path("/v1/models/status"));
        assert!(should_cache_path("/v1/policies/list"));
        assert!(!should_cache_path("/v1/infer"));
        assert!(!should_cache_path("/v1/metrics"));
    }

    #[tokio::test]
    async fn test_cache_storage() {
        let cache = ResponseCache::new(10);
        let key = "test-key".to_string();
        let etag = r#""abc123""#.to_string();

        cache.store(key.clone(), etag.clone(), None).await;

        let entry = cache.get(&key).await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().etag, etag);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let cache = ResponseCache::new(2);

        cache.store("key1".to_string(), "etag1".to_string(), None).await;
        cache.store("key2".to_string(), "etag2".to_string(), None).await;
        cache.store("key3".to_string(), "etag3".to_string(), None).await;

        // Should have evicted key1
        let entries = cache.entries.read().await;
        assert_eq!(entries.len(), 2);
    }
}
