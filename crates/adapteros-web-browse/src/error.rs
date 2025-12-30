//! Error types for web browse service

use thiserror::Error;

/// Result type alias for web browse operations
pub type WebBrowseResult<T> = Result<T, WebBrowseError>;

/// Web browse service errors
#[derive(Error, Debug)]
pub enum WebBrowseError {
    /// Tenant does not have web browsing enabled
    #[error("Web browsing not enabled for tenant: {tenant_id}")]
    NotEnabled { tenant_id: String },

    /// Rate limit exceeded
    #[error("Rate limit exceeded for tenant: {tenant_id} (limit: {limit}/min)")]
    RateLimitExceeded { tenant_id: String, limit: u32 },

    /// Daily quota exceeded
    #[error("Daily quota exceeded for tenant: {tenant_id} (limit: {limit}/day)")]
    DailyQuotaExceeded { tenant_id: String, limit: u32 },

    /// Domain blocked
    #[error("Domain blocked by policy: {domain}")]
    DomainBlocked { domain: String },

    /// Domain not in allowlist
    #[error("Domain not in allowlist: {domain}")]
    DomainNotAllowed { domain: String },

    /// HTTPS required
    #[error("HTTPS required but URL uses HTTP: {url}")]
    HttpsRequired { url: String },

    /// Request timeout
    #[error("Request timeout after {timeout_secs}s: {url}")]
    Timeout { url: String, timeout_secs: u32 },

    /// HTTP error from remote server
    #[error("HTTP error {status}: {message}")]
    HttpError { status: u16, message: String },

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),

    /// Invalid URL
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    /// Provider not configured
    #[error("Search provider not configured: {provider}")]
    ProviderNotConfigured { provider: String },

    /// Provider error
    #[error("Provider error ({provider}): {message}")]
    ProviderError { provider: String, message: String },

    /// Content too large
    #[error("Content exceeds size limit: {size_kb}KB > {limit_kb}KB")]
    ContentTooLarge { size_kb: u64, limit_kb: u64 },

    /// Parse error
    #[error("Failed to parse content: {0}")]
    ParseError(String),

    /// Cache error
    #[error("Cache error: {0}")]
    CacheError(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigError(String),

    /// Internal error
    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<reqwest::Error> for WebBrowseError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            WebBrowseError::Timeout {
                url: err.url().map(|u| u.to_string()).unwrap_or_default(),
                timeout_secs: 10,
            }
        } else if err.is_status() {
            let status = err.status().map(|s| s.as_u16()).unwrap_or(0);
            WebBrowseError::HttpError {
                status,
                message: err.to_string(),
            }
        } else {
            WebBrowseError::NetworkError(err.to_string())
        }
    }
}

impl From<url::ParseError> for WebBrowseError {
    fn from(err: url::ParseError) -> Self {
        WebBrowseError::InvalidUrl(err.to_string())
    }
}
