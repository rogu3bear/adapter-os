//! Web browse service trait and implementation

use crate::{
    cache::WebBrowseCache,
    config::{TenantBrowseConfig, WebBrowseConfig},
    error::{WebBrowseError, WebBrowseResult},
    evidence::BrowseEvidence,
    providers::{
        fetch::{PageFetcher, PageFetcherConfig},
        search::{BraveSearchProvider, SearchProvider, SearchProviderConfig},
    },
    rate_limit::{RateLimitConfig, RateLimiter},
    RequestId, TenantId,
};
use adapteros_db::Db;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Web search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchRequest {
    /// Search query
    pub query: String,

    /// Maximum number of results
    pub max_results: Option<u32>,

    /// Include content snippets
    pub include_snippets: bool,

    /// Request ID for tracing
    pub request_id: RequestId,

    /// Preferred search provider (overrides tenant default)
    pub preferred_provider: Option<String>,

    /// Filter by recency (e.g., "day", "week", "month")
    pub freshness: Option<String>,
}

/// Web search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResult {
    /// Result title
    pub title: String,

    /// Result URL
    pub url: String,

    /// Content snippet
    pub snippet: Option<String>,

    /// Published date (if available)
    pub published_date: Option<String>,

    /// Source domain
    pub domain: String,

    /// Relevance score (0.0 - 1.0)
    pub relevance_score: f32,
}

/// Web search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResponse {
    /// Search results
    pub results: Vec<WebSearchResult>,

    /// Total results found (may be more than returned)
    pub total_results: u64,

    /// Search provider used
    pub provider: String,

    /// Query executed
    pub query: String,

    /// Evidence for grounding
    pub evidence: BrowseEvidence,

    /// Response latency in milliseconds
    pub latency_ms: u64,

    /// Was result from cache
    pub from_cache: bool,
}

/// Page fetch request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageFetchRequest {
    /// URL to fetch
    pub url: String,

    /// Request ID for tracing
    pub request_id: RequestId,

    /// Extract main content only (strip navigation, ads, etc.)
    pub extract_main_content: bool,

    /// Include images
    pub include_images: bool,

    /// Maximum content length in KB
    pub max_content_kb: Option<u64>,
}

/// Page fetch response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageFetchResponse {
    /// Page title
    pub title: String,

    /// Page URL (may differ from request due to redirects)
    pub url: String,

    /// Main text content
    pub content: String,

    /// Content length in bytes
    pub content_length: usize,

    /// Page description (from meta)
    pub description: Option<String>,

    /// Images on the page
    pub images: Vec<PageImage>,

    /// Evidence for grounding
    pub evidence: BrowseEvidence,

    /// Response latency in milliseconds
    pub latency_ms: u64,

    /// Was result from cache
    pub from_cache: bool,
}

/// Image from a page
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageImage {
    /// Image URL
    pub url: String,

    /// Alt text
    pub alt: Option<String>,

    /// Width in pixels (if known)
    pub width: Option<u32>,

    /// Height in pixels (if known)
    pub height: Option<u32>,
}

/// Image search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSearchRequest {
    /// Search query
    pub query: String,

    /// Maximum number of results
    pub max_results: Option<u32>,

    /// Request ID for tracing
    pub request_id: RequestId,

    /// Filter by image size (small, medium, large)
    pub size_filter: Option<String>,

    /// Safe search level (off, moderate, strict)
    pub safe_search: Option<String>,
}

/// Image search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSearchResult {
    /// Image URL
    pub url: String,

    /// Thumbnail URL
    pub thumbnail_url: Option<String>,

    /// Image title
    pub title: String,

    /// Source page URL
    pub source_url: String,

    /// Source domain
    pub domain: String,

    /// Width in pixels
    pub width: Option<u32>,

    /// Height in pixels
    pub height: Option<u32>,
}

/// Image search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSearchResponse {
    /// Image results
    pub results: Vec<ImageSearchResult>,

    /// Total results found
    pub total_results: u64,

    /// Search provider used
    pub provider: String,

    /// Query executed
    pub query: String,

    /// Evidence for grounding
    pub evidence: BrowseEvidence,

    /// Response latency in milliseconds
    pub latency_ms: u64,

    /// Was result from cache
    pub from_cache: bool,
}

/// Web browse service trait
#[async_trait]
pub trait WebBrowseService: Send + Sync {
    /// Get tenant configuration
    async fn get_tenant_config(&self, tenant_id: &TenantId) -> WebBrowseResult<TenantBrowseConfig>;

    /// Check if tenant can browse
    async fn can_browse(&self, tenant_id: &TenantId) -> WebBrowseResult<bool>;

    /// Perform web search
    async fn search(
        &self,
        tenant_id: &TenantId,
        request: WebSearchRequest,
    ) -> WebBrowseResult<WebSearchResponse>;

    /// Fetch page content
    async fn fetch_page(
        &self,
        tenant_id: &TenantId,
        request: PageFetchRequest,
    ) -> WebBrowseResult<PageFetchResponse>;

    /// Search for images
    async fn search_images(
        &self,
        tenant_id: &TenantId,
        request: ImageSearchRequest,
    ) -> WebBrowseResult<ImageSearchResponse>;

    /// Check rate limit status for tenant
    async fn check_rate_limit(&self, tenant_id: &TenantId) -> WebBrowseResult<RateLimitStatus>;

    /// Get usage statistics for tenant
    async fn get_usage_stats(&self, tenant_id: &TenantId) -> WebBrowseResult<UsageStats>;
}

/// Rate limit status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitStatus {
    /// Requests remaining this minute
    pub remaining_per_minute: u32,

    /// Requests remaining today
    pub remaining_per_day: u32,

    /// Seconds until minute limit resets
    pub reset_minute_secs: u32,

    /// Seconds until daily limit resets
    pub reset_day_secs: u32,
}

/// Usage statistics for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    /// Total requests today
    pub requests_today: u32,

    /// Total searches today
    pub searches_today: u32,

    /// Total page fetches today
    pub page_fetches_today: u32,

    /// Total image searches today
    pub image_searches_today: u32,

    /// Cache hit rate (0.0 - 1.0)
    pub cache_hit_rate: f32,

    /// Average latency in milliseconds
    pub avg_latency_ms: u64,
}

/// Default web browse service implementation with fully-wired providers
pub struct DefaultWebBrowseService {
    config: Arc<WebBrowseConfig>,
    search_provider: Arc<dyn SearchProvider>,
    page_fetcher: Arc<PageFetcher>,
    cache: Arc<WebBrowseCache>,
    rate_limiter: Arc<RateLimiter>,
    db: Option<Arc<Db>>,
}

impl DefaultWebBrowseService {
    /// Create new web browse service with all components
    pub fn new(config: WebBrowseConfig) -> Self {
        Self::with_db(config, None)
    }

    /// Create new web browse service with database for tenant config and L2 cache
    pub fn with_db(config: WebBrowseConfig, db: Option<Arc<Db>>) -> Self {
        // Initialize search provider from config
        let search_config = SearchProviderConfig {
            api_url: config.search_endpoints.brave_api_url.clone(),
            api_key: std::env::var(&config.search_endpoints.brave_api_key_env).ok(),
            timeout_secs: config.default_timeout_secs,
            max_results: 10,
            user_agent: config.user_agent.clone(),
        };
        let search_provider: Arc<dyn SearchProvider> =
            Arc::new(BraveSearchProvider::new(search_config));

        // Initialize page fetcher
        let fetch_config = PageFetcherConfig {
            timeout_secs: config.default_timeout_secs,
            max_content_kb: 100,
            user_agent: config.user_agent.clone(),
            https_only: true,
            blocked_domains: config.global_blocked_domains.clone(),
            ..Default::default()
        };
        let page_fetcher = Arc::new(PageFetcher::new(fetch_config));

        // Initialize cache with L2 database support
        // Convert CacheSettings to CacheConfig
        let cache_config = crate::cache::CacheConfig {
            enable_l1: config.cache.enable_l1_cache,
            l1_max_entries: config.cache.l1_max_entries,
            l1_ttl_secs: config.cache.l1_ttl_secs,
            enable_l2: config.cache.enable_l2_cache,
            l2_ttl_secs: config.cache.l2_ttl_secs,
        };
        let cache = Arc::new(WebBrowseCache::with_db(cache_config, db.clone()));

        // Initialize rate limiter
        let rate_config = RateLimitConfig::default();
        let rate_limiter = Arc::new(RateLimiter::new(rate_config));

        Self {
            config: Arc::new(config),
            search_provider,
            page_fetcher,
            cache,
            rate_limiter,
            db,
        }
    }

    /// Load tenant configuration from database, falling back to defaults
    async fn load_tenant_config(&self, tenant_id: &TenantId) -> WebBrowseResult<TenantBrowseConfig> {
        // Try database lookup if available
        if let Some(ref db) = self.db {
            #[derive(sqlx::FromRow)]
            struct TenantConfigRow {
                tenant_id: String,
                enabled: bool,
                enable_web_search: bool,
                enable_page_fetch: bool,
                enable_image_search: bool,
                requests_per_minute: i32,
                requests_per_day: i32,
                https_only: bool,
                max_page_content_kb: i64,
                max_results_per_query: i32,
            }

            let row: Option<TenantConfigRow> = sqlx::query_as(
                r#"SELECT tenant_id, enabled, enable_web_search, enable_page_fetch,
                          enable_image_search, requests_per_minute, requests_per_day,
                          https_only, max_page_content_kb, max_results_per_query
                   FROM tenant_web_browse_config WHERE tenant_id = ?"#,
            )
            .bind(tenant_id)
            .fetch_optional(db.pool())
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, tenant_id = %tenant_id, "Failed to load tenant config");
                WebBrowseError::ConfigError(format!("DB error: {}", e))
            })?;

            if let Some(row) = row {
                return Ok(TenantBrowseConfig {
                    tenant_id: row.tenant_id,
                    enabled: row.enabled,
                    enable_web_search: row.enable_web_search,
                    enable_page_fetch: row.enable_page_fetch,
                    enable_image_search: row.enable_image_search,
                    requests_per_minute: row.requests_per_minute as u32,
                    requests_per_day: row.requests_per_day as u32,
                    https_only: row.https_only,
                    max_page_content_kb: row.max_page_content_kb as u64,
                    max_results_per_query: row.max_results_per_query as u32,
                    ..Default::default()
                });
            }
        }

        // Return default config for tenant
        Ok(TenantBrowseConfig {
            tenant_id: tenant_id.clone(),
            ..Default::default()
        })
    }
}

#[async_trait]
impl WebBrowseService for DefaultWebBrowseService {
    async fn get_tenant_config(&self, tenant_id: &TenantId) -> WebBrowseResult<TenantBrowseConfig> {
        self.load_tenant_config(tenant_id).await
    }

    async fn can_browse(&self, tenant_id: &TenantId) -> WebBrowseResult<bool> {
        if !self.config.enabled {
            return Ok(false);
        }
        let tenant_config = self.get_tenant_config(tenant_id).await?;
        Ok(tenant_config.enabled)
    }

    async fn search(
        &self,
        tenant_id: &TenantId,
        request: WebSearchRequest,
    ) -> WebBrowseResult<WebSearchResponse> {
        // Validate tenant can browse
        let tenant_config = self.get_tenant_config(tenant_id).await?;
        if !tenant_config.enabled {
            return Err(WebBrowseError::NotEnabled {
                tenant_id: tenant_id.clone(),
            });
        }

        if !tenant_config.enable_web_search {
            return Err(WebBrowseError::ConfigError(
                "Web search not enabled for tenant".to_string(),
            ));
        }

        // Check rate limit
        let rate_config = RateLimitConfig {
            requests_per_minute: tenant_config.requests_per_minute,
            requests_per_day: tenant_config.requests_per_day,
            enabled: true,
        };
        self.rate_limiter.check_with_config(tenant_id, &rate_config).await?;

        // Check cache first
        let cache_key_query = format!("search:{}", &request.query);
        if let Ok(Some(entry)) = self.cache.get("search", &cache_key_query, tenant_id).await {
            if let Ok(cached_response) = serde_json::from_str::<WebSearchResponse>(&entry.response_json) {
                tracing::debug!(tenant_id = %tenant_id, query = %request.query, "Cache hit for search");
                return Ok(WebSearchResponse {
                    from_cache: true,
                    ..cached_response
                });
            }
        }

        // Check if provider is configured
        if !self.search_provider.is_configured() {
            return Err(WebBrowseError::ProviderNotConfigured {
                provider: self.search_provider.name().to_string(),
            });
        }

        // Perform actual search
        let response = self.search_provider.search(tenant_id, &request).await?;

        // Cache the result
        if let Ok(json) = serde_json::to_string(&response) {
            let _ = self.cache.set(
                "search",
                &cache_key_query,
                tenant_id,
                &json,
                Some(tenant_config.cache_ttl_secs),
            ).await;
        }

        Ok(response)
    }

    async fn fetch_page(
        &self,
        tenant_id: &TenantId,
        request: PageFetchRequest,
    ) -> WebBrowseResult<PageFetchResponse> {
        let tenant_config = self.get_tenant_config(tenant_id).await?;
        if !tenant_config.enabled {
            return Err(WebBrowseError::NotEnabled {
                tenant_id: tenant_id.clone(),
            });
        }

        if !tenant_config.enable_page_fetch {
            return Err(WebBrowseError::ConfigError(
                "Page fetch not enabled for tenant".to_string(),
            ));
        }

        // Check rate limit
        let rate_config = RateLimitConfig {
            requests_per_minute: tenant_config.requests_per_minute,
            requests_per_day: tenant_config.requests_per_day,
            enabled: true,
        };
        self.rate_limiter.check_with_config(tenant_id, &rate_config).await?;

        // Check cache first
        let cache_key_query = format!("fetch:{}", &request.url);
        if let Ok(Some(entry)) = self.cache.get("page_fetch", &cache_key_query, tenant_id).await {
            if let Ok(cached_response) = serde_json::from_str::<PageFetchResponse>(&entry.response_json) {
                tracing::debug!(tenant_id = %tenant_id, url = %request.url, "Cache hit for page fetch");
                return Ok(PageFetchResponse {
                    from_cache: true,
                    ..cached_response
                });
            }
        }

        // Perform actual page fetch
        let response = self.page_fetcher.fetch(tenant_id, &request).await?;

        // Cache the result
        if let Ok(json) = serde_json::to_string(&response) {
            let _ = self.cache.set(
                "page_fetch",
                &cache_key_query,
                tenant_id,
                &json,
                Some(tenant_config.cache_ttl_secs),
            ).await;
        }

        Ok(response)
    }

    async fn search_images(
        &self,
        tenant_id: &TenantId,
        request: ImageSearchRequest,
    ) -> WebBrowseResult<ImageSearchResponse> {
        let tenant_config = self.get_tenant_config(tenant_id).await?;
        if !tenant_config.enabled {
            return Err(WebBrowseError::NotEnabled {
                tenant_id: tenant_id.clone(),
            });
        }

        if !tenant_config.enable_image_search {
            return Err(WebBrowseError::ConfigError(
                "Image search not enabled for tenant".to_string(),
            ));
        }

        // Check rate limit
        let rate_config = RateLimitConfig {
            requests_per_minute: tenant_config.requests_per_minute,
            requests_per_day: tenant_config.requests_per_day,
            enabled: true,
        };
        self.rate_limiter.check_with_config(tenant_id, &rate_config).await?;

        // Image search provider not yet implemented - return empty with evidence
        // Future: wire up image search provider when available
        Ok(ImageSearchResponse {
            results: Vec::new(),
            total_results: 0,
            provider: "none".to_string(),
            query: request.query,
            evidence: BrowseEvidence::new(tenant_id.clone(), request.request_id),
            latency_ms: 0,
            from_cache: false,
        })
    }

    async fn check_rate_limit(&self, tenant_id: &TenantId) -> WebBrowseResult<RateLimitStatus> {
        let status = self.rate_limiter.status(tenant_id).await;

        // Calculate approximate reset times
        let now = chrono::Utc::now();
        let seconds_into_minute = now.timestamp() % 60;
        let seconds_into_day = now.timestamp() % 86400;

        Ok(RateLimitStatus {
            remaining_per_minute: status.remaining_per_minute,
            remaining_per_day: status.remaining_per_day,
            reset_minute_secs: (60 - seconds_into_minute) as u32,
            reset_day_secs: (86400 - seconds_into_day) as u32,
        })
    }

    async fn get_usage_stats(&self, tenant_id: &TenantId) -> WebBrowseResult<UsageStats> {
        // Query usage stats from database if available
        if let Some(ref db) = self.db {
            #[derive(sqlx::FromRow)]
            struct UsageRow {
                total_requests: i64,
                search_requests: i64,
                fetch_requests: i64,
                image_requests: i64,
                cache_hits: i64,
                total_latency_ms: i64,
            }

            let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

            let row: Option<UsageRow> = sqlx::query_as(
                r#"SELECT
                      COALESCE(SUM(request_count), 0) as total_requests,
                      COALESCE(SUM(CASE WHEN request_type = 'search' THEN request_count ELSE 0 END), 0) as search_requests,
                      COALESCE(SUM(CASE WHEN request_type = 'page_fetch' THEN request_count ELSE 0 END), 0) as fetch_requests,
                      COALESCE(SUM(CASE WHEN request_type = 'image_search' THEN request_count ELSE 0 END), 0) as image_requests,
                      COALESCE(SUM(cache_hits), 0) as cache_hits,
                      COALESCE(SUM(total_latency_ms), 0) as total_latency_ms
                   FROM tenant_web_browse_usage
                   WHERE tenant_id = ? AND date(created_at) = ?"#,
            )
            .bind(tenant_id)
            .bind(&today)
            .fetch_optional(db.pool())
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "Failed to fetch usage stats");
                WebBrowseError::ConfigError(format!("DB error: {}", e))
            })?;

            if let Some(row) = row {
                let total = row.total_requests.max(1) as f32;
                let cache_hit_rate = row.cache_hits as f32 / total;
                let avg_latency = if row.total_requests > 0 {
                    row.total_latency_ms as u64 / row.total_requests as u64
                } else {
                    0
                };

                return Ok(UsageStats {
                    requests_today: row.total_requests as u32,
                    searches_today: row.search_requests as u32,
                    page_fetches_today: row.fetch_requests as u32,
                    image_searches_today: row.image_requests as u32,
                    cache_hit_rate,
                    avg_latency_ms: avg_latency,
                });
            }
        }

        // Return rate limiter stats as fallback
        let status = self.rate_limiter.status(tenant_id).await;
        Ok(UsageStats {
            requests_today: status.daily_count,
            searches_today: 0,
            page_fetches_today: 0,
            image_searches_today: 0,
            cache_hit_rate: 0.0,
            avg_latency_ms: 0,
        })
    }
}
