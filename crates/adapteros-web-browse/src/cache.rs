//! Two-tier caching for web browse results
//!
//! L1: In-memory moka cache for fast access
//! L2: Database cache for persistence across restarts

use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

use crate::{error::WebBrowseResult, TenantId};

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Enable L1 (memory) cache
    pub enable_l1: bool,

    /// L1 max entries
    pub l1_max_entries: u64,

    /// L1 TTL in seconds
    pub l1_ttl_secs: u64,

    /// Enable L2 (database) cache
    pub enable_l2: bool,

    /// L2 TTL in seconds
    pub l2_ttl_secs: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enable_l1: true,
            l1_max_entries: 1000,
            l1_ttl_secs: 300, // 5 minutes
            enable_l2: true,
            l2_ttl_secs: 3600, // 1 hour
        }
    }
}

/// Cached entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// Cache key
    pub key: String,

    /// Tenant ID
    pub tenant_id: TenantId,

    /// Query type (search, page_fetch, image_search)
    pub query_type: String,

    /// Original query
    pub query: String,

    /// Cached response as JSON
    pub response_json: String,

    /// When this entry was created
    pub created_at: i64,

    /// When this entry expires
    pub expires_at: i64,
}

/// Web browse cache
pub struct WebBrowseCache {
    config: CacheConfig,
    l1_cache: Option<Cache<String, CacheEntry>>,
}

impl WebBrowseCache {
    /// Create new cache
    pub fn new(config: CacheConfig) -> Self {
        let l1_cache = if config.enable_l1 {
            Some(
                Cache::builder()
                    .max_capacity(config.l1_max_entries)
                    .time_to_live(Duration::from_secs(config.l1_ttl_secs))
                    .build(),
            )
        } else {
            None
        };

        Self { config, l1_cache }
    }

    /// Generate cache key
    pub fn generate_key(query_type: &str, query: &str, tenant_id: &TenantId) -> String {
        let mut hasher = Sha256::new();
        hasher.update(query_type.as_bytes());
        hasher.update(query.as_bytes());
        hasher.update(tenant_id.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Get from L1 cache
    pub async fn get_l1(&self, key: &str) -> Option<CacheEntry> {
        if let Some(cache) = &self.l1_cache {
            cache.get(key).await
        } else {
            None
        }
    }

    /// Set in L1 cache
    pub async fn set_l1(&self, entry: CacheEntry) {
        if let Some(cache) = &self.l1_cache {
            cache.insert(entry.key.clone(), entry).await;
        }
    }

    /// Get from cache (L1, then L2)
    pub async fn get(
        &self,
        query_type: &str,
        query: &str,
        tenant_id: &TenantId,
    ) -> WebBrowseResult<Option<CacheEntry>> {
        let key = Self::generate_key(query_type, query, tenant_id);

        // Try L1 first
        if let Some(entry) = self.get_l1(&key).await {
            let now = chrono::Utc::now().timestamp();
            if entry.expires_at > now {
                return Ok(Some(entry));
            }
            // Entry expired, remove from L1
            if let Some(cache) = &self.l1_cache {
                cache.remove(&key).await;
            }
        }

        // TODO: Try L2 (database) cache
        // For now, return None

        Ok(None)
    }

    /// Set in cache (both L1 and L2)
    pub async fn set(
        &self,
        query_type: &str,
        query: &str,
        tenant_id: &TenantId,
        response_json: &str,
        ttl_secs: Option<u64>,
    ) -> WebBrowseResult<()> {
        let key = Self::generate_key(query_type, query, tenant_id);
        let now = chrono::Utc::now().timestamp();
        let ttl = ttl_secs.unwrap_or(self.config.l1_ttl_secs);

        let entry = CacheEntry {
            key: key.clone(),
            tenant_id: tenant_id.clone(),
            query_type: query_type.to_string(),
            query: query.to_string(),
            response_json: response_json.to_string(),
            created_at: now,
            expires_at: now + ttl as i64,
        };

        // Set in L1
        self.set_l1(entry.clone()).await;

        // TODO: Set in L2 (database) cache

        Ok(())
    }

    /// Invalidate cache entry
    pub async fn invalidate(
        &self,
        query_type: &str,
        query: &str,
        tenant_id: &TenantId,
    ) -> WebBrowseResult<()> {
        let key = Self::generate_key(query_type, query, tenant_id);

        // Remove from L1
        if let Some(cache) = &self.l1_cache {
            cache.remove(&key).await;
        }

        // TODO: Remove from L2

        Ok(())
    }

    /// Clear all cache entries for a tenant
    pub async fn clear_tenant(&self, _tenant_id: &TenantId) -> WebBrowseResult<()> {
        // L1 cache doesn't support prefix deletion easily
        // Would need to iterate or use a different structure
        // For now, this is a no-op for L1

        // TODO: Clear L2 cache for tenant

        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        if let Some(cache) = &self.l1_cache {
            CacheStats {
                l1_entries: cache.entry_count(),
                l1_max_entries: self.config.l1_max_entries,
                l1_enabled: true,
                l2_enabled: self.config.enable_l2,
            }
        } else {
            CacheStats {
                l1_entries: 0,
                l1_max_entries: 0,
                l1_enabled: false,
                l2_enabled: self.config.enable_l2,
            }
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    /// Current L1 entry count
    pub l1_entries: u64,

    /// L1 max capacity
    pub l1_max_entries: u64,

    /// L1 enabled
    pub l1_enabled: bool,

    /// L2 enabled
    pub l2_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_key_generation() {
        let key1 = WebBrowseCache::generate_key("search", "query1", &"tenant1".to_string());
        let key2 = WebBrowseCache::generate_key("search", "query1", &"tenant1".to_string());
        let key3 = WebBrowseCache::generate_key("search", "query2", &"tenant1".to_string());

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[tokio::test]
    async fn test_cache_set_get() {
        let cache = WebBrowseCache::new(CacheConfig::default());

        cache
            .set(
                "search",
                "test query",
                &"tenant1".to_string(),
                r#"{"results": []}"#,
                Some(60),
            )
            .await
            .unwrap();

        let entry = cache
            .get("search", "test query", &"tenant1".to_string())
            .await
            .unwrap();

        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.query, "test query");
    }

    #[tokio::test]
    async fn test_cache_invalidate() {
        let cache = WebBrowseCache::new(CacheConfig::default());

        cache
            .set(
                "search",
                "test query",
                &"tenant1".to_string(),
                r#"{"results": []}"#,
                Some(60),
            )
            .await
            .unwrap();

        cache
            .invalidate("search", "test query", &"tenant1".to_string())
            .await
            .unwrap();

        let entry = cache
            .get("search", "test query", &"tenant1".to_string())
            .await
            .unwrap();

        assert!(entry.is_none());
    }
}
