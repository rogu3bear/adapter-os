//! Two-tier caching for web browse results
//!
//! L1: In-memory moka cache for fast access
//! L2: Database cache for persistence across restarts

use adapteros_db::Db;
use moka::future::Cache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
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

/// Row type for sqlx query results
#[derive(Debug, sqlx::FromRow)]
struct CacheEntryRow {
    cache_key: String,
    tenant_id: String,
    query_type: String,
    query: String,
    response_json: String,
    created_at: String,
    expires_at: String,
}

/// Parse SQLite datetime string to Unix timestamp
fn parse_sqlite_datetime(s: &str) -> i64 {
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .map(|dt| dt.and_utc().timestamp())
        .unwrap_or(0)
}

/// Web browse cache
pub struct WebBrowseCache {
    config: CacheConfig,
    l1_cache: Option<Cache<String, CacheEntry>>,
    db: Option<Arc<Db>>,
}

impl WebBrowseCache {
    /// Create new cache without database (L1 only)
    pub fn new(config: CacheConfig) -> Self {
        Self::with_db(config, None)
    }

    /// Create new cache with database (L1 + L2)
    pub fn with_db(config: CacheConfig, db: Option<Arc<Db>>) -> Self {
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

        Self {
            config,
            l1_cache,
            db,
        }
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

    /// Get from L2 (database) cache
    async fn get_l2(&self, key: &str) -> WebBrowseResult<Option<CacheEntry>> {
        let Some(db) = &self.db else {
            return Ok(None);
        };
        if !self.config.enable_l2 {
            return Ok(None);
        }

        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        let row: Option<CacheEntryRow> = sqlx::query_as(
            r#"SELECT cache_key, tenant_id, query_type, query, response_json, created_at, expires_at
               FROM web_browse_cache
               WHERE cache_key = ? AND expires_at > ?"#,
        )
        .bind(key)
        .bind(&now)
        .fetch_optional(db.pool())
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "L2 cache get failed");
            crate::error::WebBrowseError::CacheError(format!("L2 cache get failed: {}", e))
        })?;

        match row {
            Some(row) => {
                let entry = CacheEntry {
                    key: row.cache_key,
                    tenant_id: row.tenant_id,
                    query_type: row.query_type,
                    query: row.query,
                    response_json: row.response_json,
                    created_at: parse_sqlite_datetime(&row.created_at),
                    expires_at: parse_sqlite_datetime(&row.expires_at),
                };
                tracing::debug!(key = %entry.key, "L2 cache hit");
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    /// Set in L2 (database) cache
    async fn set_l2(&self, entry: &CacheEntry) -> WebBrowseResult<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };
        if !self.config.enable_l2 {
            return Ok(());
        }

        let created_at = chrono::DateTime::from_timestamp(entry.created_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();
        let expires_at = chrono::DateTime::from_timestamp(entry.expires_at, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
            .unwrap_or_default();

        sqlx::query(
            r#"INSERT OR REPLACE INTO web_browse_cache
               (cache_key, tenant_id, query_type, query, response_json, created_at, expires_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&entry.key)
        .bind(&entry.tenant_id)
        .bind(&entry.query_type)
        .bind(&entry.query)
        .bind(&entry.response_json)
        .bind(&created_at)
        .bind(&expires_at)
        .execute(db.pool())
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "L2 cache set failed");
            crate::error::WebBrowseError::CacheError(format!("L2 cache set failed: {}", e))
        })?;

        tracing::debug!(key = %entry.key, "L2 cache set");
        Ok(())
    }

    /// Invalidate from L2 (database) cache
    async fn invalidate_l2(&self, key: &str) -> WebBrowseResult<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };

        sqlx::query("DELETE FROM web_browse_cache WHERE cache_key = ?")
            .bind(key)
            .execute(db.pool())
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "L2 cache invalidate failed");
                crate::error::WebBrowseError::CacheError(format!(
                    "L2 cache invalidate failed: {}",
                    e
                ))
            })?;

        Ok(())
    }

    /// Clear all L2 cache entries for a tenant
    async fn clear_tenant_l2(&self, tenant_id: &TenantId) -> WebBrowseResult<()> {
        let Some(db) = &self.db else {
            return Ok(());
        };

        sqlx::query("DELETE FROM web_browse_cache WHERE tenant_id = ?")
            .bind(tenant_id)
            .execute(db.pool())
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "L2 cache clear tenant failed");
                crate::error::WebBrowseError::CacheError(format!(
                    "L2 cache clear tenant failed: {}",
                    e
                ))
            })?;

        tracing::debug!(tenant_id = %tenant_id, "L2 cache cleared for tenant");
        Ok(())
    }

    /// Get from cache (L1, then L2)
    ///
    /// Tries L1 (memory) first, then falls back to L2 (database).
    /// If L2 hit occurs, promotes entry to L1 for faster subsequent access.
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
                tracing::debug!(key = %key, "L1 cache hit");
                return Ok(Some(entry));
            }
            // Entry expired, remove from L1
            if let Some(cache) = &self.l1_cache {
                cache.remove(&key).await;
            }
        }

        // Try L2 (database) cache
        if let Some(entry) = self.get_l2(&key).await? {
            // Promote to L1 for faster subsequent access
            self.set_l1(entry.clone()).await;
            return Ok(Some(entry));
        }

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
        // Use L2 TTL for persistence, L1 TTL for memory cache
        let l2_ttl = ttl_secs.unwrap_or(self.config.l2_ttl_secs);

        let entry = CacheEntry {
            key: key.clone(),
            tenant_id: tenant_id.clone(),
            query_type: query_type.to_string(),
            query: query.to_string(),
            response_json: response_json.to_string(),
            created_at: now,
            expires_at: now + l2_ttl as i64,
        };

        // Set in L1 (uses its own TTL via moka time_to_live)
        self.set_l1(entry.clone()).await;

        // Set in L2 (database) cache
        self.set_l2(&entry).await?;

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

        // Remove from L2
        self.invalidate_l2(&key).await?;

        Ok(())
    }

    /// Clear all cache entries for a tenant
    pub async fn clear_tenant(&self, tenant_id: &TenantId) -> WebBrowseResult<()> {
        // L1 cache: Moka doesn't support prefix deletion easily
        // Clear the entire L1 cache (safe but aggressive)
        if let Some(cache) = &self.l1_cache {
            cache.invalidate_all();
            tracing::debug!(tenant_id = %tenant_id, "L1 cache cleared (full invalidation)");
        }

        // Clear L2 cache for tenant (precise deletion)
        self.clear_tenant_l2(tenant_id).await?;

        Ok(())
    }

    /// Invalidate all cache entries matching a query type for a tenant
    ///
    /// Useful for invalidating all search results or all page fetches
    /// when the underlying data source changes.
    pub async fn invalidate_by_query_type(
        &self,
        query_type: &str,
        tenant_id: &TenantId,
    ) -> WebBrowseResult<()> {
        // L1 cache: Moka doesn't support prefix deletion
        // We'd need to iterate which is expensive, so just clear all
        if let Some(cache) = &self.l1_cache {
            cache.invalidate_all();
        }

        // L2 cache: Precise deletion by query type
        if let Some(db) = &self.db {
            sqlx::query("DELETE FROM web_browse_cache WHERE query_type = ? AND tenant_id = ?")
                .bind(query_type)
                .bind(tenant_id)
                .execute(db.pool())
                .await
                .map_err(|e| {
                    tracing::warn!(error = %e, "L2 cache invalidate by query type failed");
                    crate::error::WebBrowseError::CacheError(format!(
                        "L2 cache invalidate by query type failed: {}",
                        e
                    ))
                })?;

            tracing::debug!(
                query_type = %query_type,
                tenant_id = %tenant_id,
                "L2 cache invalidated by query type"
            );
        }

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
