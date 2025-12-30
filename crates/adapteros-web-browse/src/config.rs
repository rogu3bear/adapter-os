//! Configuration types for web browse service

use serde::{Deserialize, Serialize};

/// Global web browse service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebBrowseConfig {
    /// Enable the web browse service globally
    pub enabled: bool,

    /// Default search provider (brave, bing, google)
    pub default_search_provider: String,

    /// Search API endpoints
    pub search_endpoints: SearchEndpoints,

    /// Global blocked domains (applies to all tenants)
    pub global_blocked_domains: Vec<String>,

    /// Maximum concurrent requests per worker
    pub max_concurrent_requests: u32,

    /// Default request timeout in seconds
    pub default_timeout_secs: u32,

    /// Cache configuration
    pub cache: CacheSettings,

    /// User agent string
    pub user_agent: String,
}

impl Default for WebBrowseConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_search_provider: "brave".to_string(),
            search_endpoints: SearchEndpoints::default(),
            global_blocked_domains: vec![
                "localhost".to_string(),
                "127.0.0.1".to_string(),
                "::1".to_string(),
                "*.local".to_string(),
                "*.internal".to_string(),
                "*.corp".to_string(),
                "10.*".to_string(),
                "172.16.*".to_string(),
                "192.168.*".to_string(),
            ],
            max_concurrent_requests: 10,
            default_timeout_secs: 10,
            cache: CacheSettings::default(),
            user_agent: "AdapterOS-WebBrowse/1.0".to_string(),
        }
    }
}

/// Search provider endpoint configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchEndpoints {
    pub brave_api_url: String,
    pub brave_api_key_env: String,
    pub bing_api_url: String,
    pub bing_api_key_env: String,
}

impl Default for SearchEndpoints {
    fn default() -> Self {
        Self {
            brave_api_url: "https://api.search.brave.com/res/v1/web/search".to_string(),
            brave_api_key_env: "BRAVE_SEARCH_API_KEY".to_string(),
            bing_api_url: "https://api.bing.microsoft.com/v7.0/search".to_string(),
            bing_api_key_env: "BING_SEARCH_API_KEY".to_string(),
        }
    }
}

/// Cache settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSettings {
    /// Enable L1 in-memory cache
    pub enable_l1_cache: bool,

    /// L1 cache max entries
    pub l1_max_entries: u64,

    /// L1 cache TTL in seconds
    pub l1_ttl_secs: u64,

    /// Enable L2 database cache
    pub enable_l2_cache: bool,

    /// L2 cache TTL in seconds
    pub l2_ttl_secs: u64,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self {
            enable_l1_cache: true,
            l1_max_entries: 1000,
            l1_ttl_secs: 300, // 5 minutes
            enable_l2_cache: true,
            l2_ttl_secs: 3600, // 1 hour
        }
    }
}

/// Per-tenant web browse configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantBrowseConfig {
    /// Tenant ID
    pub tenant_id: String,

    /// Enable web browsing for this tenant
    pub enabled: bool,

    /// Rate limit: requests per minute
    pub requests_per_minute: u32,

    /// Rate limit: requests per day
    pub requests_per_day: u32,

    /// Enable web search
    pub enable_web_search: bool,

    /// Enable page fetching
    pub enable_page_fetch: bool,

    /// Enable image search
    pub enable_image_search: bool,

    /// Allowed search providers
    pub allowed_search_providers: Vec<String>,

    /// Additional allowed domains (beyond global allowlist)
    pub allowed_domains: Vec<String>,

    /// Additional blocked domains (in addition to global blocklist)
    pub blocked_domains: Vec<String>,

    /// Cache TTL override in seconds
    pub cache_ttl_secs: u64,

    /// Maximum results per query
    pub max_results_per_query: u32,

    /// Maximum page content size in KB
    pub max_page_content_kb: u64,

    /// Require HTTPS
    pub https_only: bool,

    /// Maximum concurrent requests
    pub max_concurrent_requests: u32,

    /// Request timeout in seconds
    pub request_timeout_secs: u32,

    /// Fallback behavior when browsing fails
    pub fallback_behavior: FallbackBehavior,
}

impl Default for TenantBrowseConfig {
    fn default() -> Self {
        Self {
            tenant_id: String::new(),
            enabled: false,
            requests_per_minute: 10,
            requests_per_day: 100,
            enable_web_search: true,
            enable_page_fetch: false,
            enable_image_search: false,
            allowed_search_providers: vec!["brave".to_string()],
            allowed_domains: Vec::new(),
            blocked_domains: Vec::new(),
            cache_ttl_secs: 3600,
            max_results_per_query: 10,
            max_page_content_kb: 100,
            https_only: true,
            max_concurrent_requests: 3,
            request_timeout_secs: 10,
            fallback_behavior: FallbackBehavior::WarnAndAllow,
        }
    }
}

/// Fallback behavior when web browsing fails or is disabled
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackBehavior {
    /// Block the request with an explanation
    Deny,

    /// Allow the response with a staleness warning
    WarnAndAllow,

    /// Allow without modification (not recommended)
    AllowSilent,
}

impl std::str::FromStr for FallbackBehavior {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "deny" => Ok(FallbackBehavior::Deny),
            "warn_and_allow" | "warnandallow" => Ok(FallbackBehavior::WarnAndAllow),
            "allow_silent" | "allowsilent" => Ok(FallbackBehavior::AllowSilent),
            _ => Err(format!("Unknown fallback behavior: {}", s)),
        }
    }
}

impl std::fmt::Display for FallbackBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FallbackBehavior::Deny => write!(f, "deny"),
            FallbackBehavior::WarnAndAllow => write!(f, "warn_and_allow"),
            FallbackBehavior::AllowSilent => write!(f, "allow_silent"),
        }
    }
}
