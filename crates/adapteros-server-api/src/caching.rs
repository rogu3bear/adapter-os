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

use adapteros_core::B3Hash;
use axum::{
    extract::Request,
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use lru::LruCache;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cache entry
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// ETag value
    pub etag: String,
}

/// In-memory cache for ETags and responses
#[derive(Clone)]
pub struct ResponseCache {
    /// LRU cache with proper eviction (replaces HashMap)
    entries: Arc<RwLock<LruCache<String, CacheEntry>>>,
    /// Maximum size for a single cache entry (0 = unlimited)
    max_entry_size: usize,
}

impl ResponseCache {
    /// Create a new response cache with specified max size
    pub fn new(max_size: usize) -> Self {
        Self::with_entry_limit(max_size, 0)
    }

    /// Create a new response cache with max size and max entry size limit
    ///
    /// # Arguments
    /// * `max_size` - Maximum number of entries in the cache
    /// * `max_entry_size` - Maximum size in bytes for a single entry (0 = unlimited)
    pub fn with_entry_limit(max_size: usize, max_entry_size: usize) -> Self {
        let capacity = NonZeroUsize::new(max_size).unwrap_or(NonZeroUsize::new(100).unwrap());
        Self {
            entries: Arc::new(RwLock::new(LruCache::new(capacity))),
            max_entry_size,
        }
    }

    /// Generate ETag from content
    pub fn generate_etag(content: &[u8]) -> String {
        let hash = B3Hash::hash(content);
        format!(r#""{:x}""#, hash)
    }

    /// Store cache entry with size checking and LRU eviction
    pub async fn store(&self, key: String, etag: String) {
        // Calculate entry size
        let size_bytes = etag.len() + key.len();

        // Skip caching if entry exceeds max_entry_size
        if self.max_entry_size > 0 && size_bytes > self.max_entry_size {
            tracing::debug!(
                key = %key,
                size_bytes = size_bytes,
                max_entry_size = self.max_entry_size,
                "Skipping cache entry that exceeds max_entry_size"
            );
            return;
        }

        let mut entries = self.entries.write().await;

        // LRU cache automatically evicts least recently used entry when full
        entries.put(key, CacheEntry { etag });
    }

    /// Get cache entry (marks as recently used in LRU)
    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
        let mut entries = self.entries.write().await;
        // LRU get() updates access time
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

    // Process request
    let response = next.run(request).await;

    // Add caching headers
    add_cache_headers(response, &path)
}

/// Add cache headers to response
fn add_cache_headers(mut response: Response, path: &str) -> Response {
    let status = response.status();

    // Only add cache headers for successful responses
    if !status.is_success() {
        return response;
    }

    let headers = response.headers_mut();

    // Determine cache-ability based on path
    let cache_control = if path.ends_with(".html") || path == "/" || path.starts_with("/index.html")
    {
        // Never cache HTML shell; prevents stale index after deploy
        "no-cache, no-store, must-revalidate"
    } else if path.starts_with("/v1/metrics") || path.starts_with("/v1/infer") {
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

// ============================================================================
// Dashboard Cache for System Overview and Tenant Validation
// ============================================================================

/// TTL-based in-memory cache for expensive dashboard queries.
///
/// Provides caching for:
/// - System overview data (10s TTL)
/// - Service health checks (5s TTL)
/// - Tenant existence validation (60s TTL) - used by middleware
///
/// This reduces database load for frequently accessed dashboard endpoints
/// and enables efficient tenant validation without per-request DB queries.
///
/// Background pruning task removes expired entries every 5 minutes.
#[derive(Clone)]
pub struct DashboardCache {
    /// Cached tenant existence checks (tenant_id -> (exists, cached_at))
    tenant_exists: Arc<RwLock<HashMap<String, (bool, Instant)>>>,
    /// TTL for tenant existence cache entries (default: 60 seconds)
    tenant_ttl: Duration,
    /// Pruning interval for expired entries (default: 300 seconds)
    pruning_interval: Duration,
}

impl DashboardCache {
    /// Create a new DashboardCache with default TTLs and start background pruning
    pub fn new() -> Self {
        let cache = Self {
            tenant_exists: Arc::new(RwLock::new(HashMap::new())),
            tenant_ttl: Duration::from_secs(60),
            pruning_interval: Duration::from_secs(300),
        };
        cache.start_pruning_task();
        cache
    }

    /// Create a new DashboardCache with custom tenant TTL and start background pruning
    pub fn with_tenant_ttl(tenant_ttl_secs: u64) -> Self {
        let cache = Self {
            tenant_exists: Arc::new(RwLock::new(HashMap::new())),
            tenant_ttl: Duration::from_secs(tenant_ttl_secs),
            pruning_interval: Duration::from_secs(300),
        };
        cache.start_pruning_task();
        cache
    }

    /// Create a new DashboardCache for testing without background pruning task
    #[cfg(test)]
    pub fn for_testing(tenant_ttl_secs: u64) -> Self {
        Self {
            tenant_exists: Arc::new(RwLock::new(HashMap::new())),
            tenant_ttl: Duration::from_secs(tenant_ttl_secs),
            pruning_interval: Duration::from_secs(300),
        }
        // Note: Does NOT start pruning task for deterministic test behavior
    }

    /// Start background task to prune expired entries
    fn start_pruning_task(&self) {
        let tenant_exists: Arc<RwLock<HashMap<String, (bool, Instant)>>> =
            Arc::clone(&self.tenant_exists);
        let tenant_ttl = self.tenant_ttl;
        let pruning_interval = self.pruning_interval;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(pruning_interval);
            loop {
                interval.tick().await;

                let mut guard = tenant_exists.write().await;
                let before_count = guard.len();

                // Remove expired entries
                guard.retain(|_, (_, cached_at)| cached_at.elapsed() < tenant_ttl);

                let after_count = guard.len();
                let pruned = before_count.saturating_sub(after_count);

                if pruned > 0 {
                    tracing::debug!(
                        pruned_entries = pruned,
                        remaining_entries = after_count,
                        "Pruned expired tenant cache entries"
                    );
                }
            }
        });
    }

    /// Check if a tenant exists (from cache if valid)
    ///
    /// Returns:
    /// - `Some(true)` if tenant exists (cached or fresh)
    /// - `Some(false)` if tenant doesn't exist (cached)
    /// - `None` if not in cache or cache expired (need to query DB)
    pub async fn tenant_exists(&self, tenant_id: &str) -> Option<bool> {
        let guard = self.tenant_exists.read().await;
        guard
            .get(tenant_id)
            .filter(|(_, cached_at)| cached_at.elapsed() < self.tenant_ttl)
            .map(|(exists, _)| *exists)
    }

    /// Cache a tenant existence check result
    pub async fn set_tenant_exists(&self, tenant_id: String, exists: bool) {
        let mut guard = self.tenant_exists.write().await;
        guard.insert(tenant_id, (exists, Instant::now()));
    }

    /// Remove a tenant from the cache (e.g., after deletion)
    pub async fn invalidate_tenant(&self, tenant_id: &str) {
        let mut guard = self.tenant_exists.write().await;
        guard.remove(tenant_id);
    }

    /// Clear all cached data
    pub async fn clear(&self) {
        let mut tenant_guard = self.tenant_exists.write().await;
        tenant_guard.clear();
    }

    /// Get cache statistics for monitoring
    pub async fn stats(&self) -> DashboardCacheStats {
        let tenant_guard = self.tenant_exists.read().await;

        let tenant_total = tenant_guard.len();
        let tenant_valid = tenant_guard
            .values()
            .filter(|(_, cached_at)| cached_at.elapsed() < self.tenant_ttl)
            .count();

        DashboardCacheStats {
            tenant_entries_total: tenant_total,
            tenant_entries_valid: tenant_valid,
        }
    }
}

impl Default for DashboardCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for the dashboard cache
#[derive(Debug, Clone)]
pub struct DashboardCacheStats {
    pub tenant_entries_total: usize,
    pub tenant_entries_valid: usize,
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

        cache.store(key.clone(), etag.clone()).await;

        let entry = cache.get(&key).await;
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().etag, etag);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let cache = ResponseCache::new(2);

        cache.store("key1".to_string(), "etag1".to_string()).await;
        cache.store("key2".to_string(), "etag2".to_string()).await;
        cache.store("key3".to_string(), "etag3".to_string()).await;

        // Should have evicted key1
        let entries = cache.entries.read().await;
        assert_eq!(entries.len(), 2);
    }

    // ========================================================================
    // DashboardCache Tests
    // ========================================================================

    #[tokio::test]
    async fn test_dashboard_cache_tenant_exists_miss() {
        let cache = DashboardCache::new();

        // Cache miss returns None
        let result = cache.tenant_exists("tenant-1").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_dashboard_cache_tenant_exists_hit() {
        let cache = DashboardCache::new();

        // Set a tenant as existing
        cache.set_tenant_exists("tenant-1".to_string(), true).await;

        // Cache hit returns the value
        let result = cache.tenant_exists("tenant-1").await;
        assert_eq!(result, Some(true));

        // Set a tenant as not existing
        cache.set_tenant_exists("tenant-2".to_string(), false).await;
        let result = cache.tenant_exists("tenant-2").await;
        assert_eq!(result, Some(false));
    }

    #[tokio::test]
    async fn test_dashboard_cache_tenant_invalidation() {
        let cache = DashboardCache::new();

        // Set and verify
        cache.set_tenant_exists("tenant-1".to_string(), true).await;
        assert_eq!(cache.tenant_exists("tenant-1").await, Some(true));

        // Invalidate
        cache.invalidate_tenant("tenant-1").await;

        // Should be a cache miss now
        assert!(cache.tenant_exists("tenant-1").await.is_none());
    }

    #[tokio::test]
    async fn test_dashboard_cache_ttl_expiration() {
        // Create cache with very short TTL (no pruning for deterministic test)
        let cache = DashboardCache::for_testing(0);

        cache.set_tenant_exists("tenant-1".to_string(), true).await;

        // Wait for TTL to expire (with 0 TTL, it's immediately expired)
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Should be expired (returns None)
        let result = cache.tenant_exists("tenant-1").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_dashboard_cache_clear() {
        let cache = DashboardCache::new();

        cache.set_tenant_exists("tenant-1".to_string(), true).await;
        cache.set_tenant_exists("tenant-2".to_string(), false).await;

        // Clear all
        cache.clear().await;

        // Both should be cache misses
        assert!(cache.tenant_exists("tenant-1").await.is_none());
        assert!(cache.tenant_exists("tenant-2").await.is_none());
    }

    #[tokio::test]
    async fn test_dashboard_cache_stats() {
        let cache = DashboardCache::new();

        cache.set_tenant_exists("tenant-1".to_string(), true).await;
        cache.set_tenant_exists("tenant-2".to_string(), false).await;

        let stats = cache.stats().await;
        assert_eq!(stats.tenant_entries_total, 2);
        assert_eq!(stats.tenant_entries_valid, 2);
    }

    #[tokio::test]
    async fn test_dashboard_cache_stats_with_expired() {
        // Create cache with very short TTL (no pruning task for deterministic test)
        let cache = DashboardCache::for_testing(0);

        cache.set_tenant_exists("tenant-1".to_string(), true).await;

        // Wait for TTL to expire (with 0 TTL, it's immediately expired)
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let stats = cache.stats().await;
        assert_eq!(stats.tenant_entries_total, 1); // Still in map (no pruning)
        assert_eq!(stats.tenant_entries_valid, 0); // But expired
    }

    #[tokio::test]
    async fn test_dashboard_cache_concurrent_access() {
        use std::sync::Arc;

        let cache = Arc::new(DashboardCache::new());
        let mut handles = vec![];

        // Spawn multiple tasks writing to the cache
        for i in 0..10 {
            let cache_clone = cache.clone();
            handles.push(tokio::spawn(async move {
                cache_clone
                    .set_tenant_exists(format!("tenant-{}", i), true)
                    .await;
            }));
        }

        // Wait for all writes
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all entries exist
        for i in 0..10 {
            assert_eq!(
                cache.tenant_exists(&format!("tenant-{}", i)).await,
                Some(true)
            );
        }
    }
}
