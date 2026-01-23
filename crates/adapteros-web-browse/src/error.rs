//! Error types for web browse service

use std::time::Duration;
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

    /// Blocked by robots.txt or similar access restrictions
    #[error("Access blocked (likely robots.txt) for domain: {domain}, status: {status}")]
    RobotsTxtBlocked { domain: String, status: u16 },

    /// Server rate limited with retry information
    #[error("Server rate limited: {url}, retry after {retry_after_secs:?}s")]
    ServerRateLimited {
        url: String,
        status: u16,
        retry_after_secs: Option<u64>,
        message: String,
    },

    /// Redirect loop or max redirects exceeded
    #[error("Redirect limit exceeded ({max_redirects}) for: {url}")]
    RedirectLoopExceeded {
        url: String,
        max_redirects: u32,
        redirect_chain: Vec<String>,
    },

    /// Request failed after retry attempts exhausted
    #[error("Request failed after {attempts} attempts: {last_error}")]
    RetryExhausted {
        url: String,
        attempts: u32,
        last_error: String,
    },
}

impl WebBrowseError {
    /// Check if this error is retriable
    pub fn is_retriable(&self) -> bool {
        match self {
            WebBrowseError::NetworkError(_) => true,
            WebBrowseError::Timeout { .. } => true,
            WebBrowseError::ServerRateLimited { .. } => true,
            WebBrowseError::HttpError { status, .. } => is_retriable_status(*status),
            _ => false,
        }
    }

    /// Get retry delay hint from Retry-After header (if available)
    pub fn retry_after_hint(&self) -> Option<Duration> {
        match self {
            WebBrowseError::ServerRateLimited {
                retry_after_secs: Some(secs),
                ..
            } => Some(Duration::from_secs(*secs)),
            _ => None,
        }
    }
}

/// Determines if an HTTP status code is retriable
pub fn is_retriable_status(status: u16) -> bool {
    match status {
        429 => true,       // Too Many Requests - always retry
        500..=599 => true, // Server errors - retry
        _ => false,        // 4xx (except 429) - don't retry
    }
}

impl From<reqwest::Error> for WebBrowseError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            WebBrowseError::Timeout {
                url: err.url().map(|u| u.to_string()).unwrap_or_default(),
                timeout_secs: 10,
            }
        } else if err.is_redirect() {
            // Redirect limit exceeded
            WebBrowseError::RedirectLoopExceeded {
                url: err.url().map(|u| u.to_string()).unwrap_or_default(),
                max_redirects: 10,
                redirect_chain: Vec::new(),
            }
        } else if err.is_status() {
            let status = err.status().map(|s| s.as_u16()).unwrap_or(0);
            WebBrowseError::HttpError {
                status,
                message: err.to_string(),
            }
        } else if err.is_connect() || err.is_request() {
            // Network-level errors are retriable
            WebBrowseError::NetworkError(err.to_string())
        } else {
            WebBrowseError::NetworkError(err.to_string())
        }
    }
}

// Using impl_error_from_for! macro for simple conversions
adapteros_core::impl_error_from_for!(WebBrowseError: url::ParseError => InvalidUrl);
