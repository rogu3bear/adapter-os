//! Retry logic for HTTP requests with exponential backoff

use serde::{Deserialize, Serialize};

use crate::error::WebBrowseError;

/// Retry configuration for HTTP requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRetryConfig {
    /// Maximum number of retry attempts (excluding initial attempt)
    pub max_retries: u32,

    /// Base delay in milliseconds for exponential backoff
    pub base_delay_ms: u64,

    /// Maximum delay cap in milliseconds
    pub max_delay_ms: u64,

    /// Backoff multiplier (typically 2.0 for exponential)
    pub backoff_multiplier: f64,

    /// Jitter factor (0.0 - 1.0) to randomize delays
    pub jitter_factor: f64,

    /// Whether to respect Retry-After header from 429 responses
    pub respect_retry_after: bool,

    /// Maximum Retry-After value to honor in seconds (prevents server abuse)
    pub max_retry_after_secs: u64,
}

impl Default for HttpRetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 500,
            max_delay_ms: 30_000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.1,
            respect_retry_after: true,
            max_retry_after_secs: 120,
        }
    }
}

impl HttpRetryConfig {
    /// Validate the retry configuration
    ///
    /// # Errors
    /// Returns an error if the configuration is invalid, such as:
    /// - jitter_factor is zero (thundering herd prevention requires non-zero jitter)
    /// - max_delay_ms is less than base_delay_ms
    pub fn validate(&self) -> Result<(), crate::error::WebBrowseError> {
        if self.jitter_factor <= 0.0 {
            return Err(crate::error::WebBrowseError::ConfigError(
                "jitter_factor must be > 0 for thundering herd prevention".to_string(),
            ));
        }

        if self.jitter_factor > 1.0 {
            return Err(crate::error::WebBrowseError::ConfigError(
                "jitter_factor must be <= 1.0".to_string(),
            ));
        }

        if self.max_delay_ms < self.base_delay_ms {
            return Err(crate::error::WebBrowseError::ConfigError(format!(
                "max_delay_ms ({}) must be >= base_delay_ms ({})",
                self.max_delay_ms, self.base_delay_ms
            )));
        }

        if self.backoff_multiplier < 1.0 {
            return Err(crate::error::WebBrowseError::ConfigError(
                "backoff_multiplier must be >= 1.0".to_string(),
            ));
        }

        Ok(())
    }

    /// Create a validated configuration
    ///
    /// Returns default configuration which is always valid.
    pub fn validated() -> Self {
        let config = Self::default();
        // Default is always valid, but validate anyway in debug builds
        debug_assert!(config.validate().is_ok());
        config
    }
}

/// Parse Retry-After header value from HTTP response
///
/// Supports both:
/// - Delay in seconds (e.g., "120")
/// - HTTP-date format (e.g., "Wed, 21 Oct 2015 07:28:00 GMT")
pub fn parse_retry_after(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            // Try parsing as seconds first
            s.parse::<u64>().ok().or_else(|| {
                // Try parsing as HTTP-date
                chrono::DateTime::parse_from_rfc2822(s).ok().map(|dt| {
                    let now = chrono::Utc::now();
                    let target = dt.with_timezone(&chrono::Utc);
                    target.signed_duration_since(now).num_seconds().max(0) as u64
                })
            })
        })
}

/// Resolve a redirect URL, handling both absolute and relative URLs
pub fn resolve_redirect_url(
    base: &str,
    location: &reqwest::header::HeaderValue,
) -> Result<String, WebBrowseError> {
    let location_str = location
        .to_str()
        .map_err(|_| WebBrowseError::ParseError("Invalid redirect location header".to_string()))?;

    if location_str.starts_with("http://") || location_str.starts_with("https://") {
        Ok(location_str.to_string())
    } else {
        // Relative URL - resolve against base
        let base_url = url::Url::parse(base)?;
        base_url
            .join(location_str)
            .map(|u| u.to_string())
            .map_err(|e| WebBrowseError::InvalidUrl(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = HttpRetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_ms, 500);
        assert_eq!(config.max_delay_ms, 30_000);
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[test]
    fn test_resolve_redirect_url_absolute() {
        let base = "https://example.com/page";
        let location = reqwest::header::HeaderValue::from_static("https://other.com/new");

        let result = resolve_redirect_url(base, &location).unwrap();
        assert_eq!(result, "https://other.com/new");
    }

    #[test]
    fn test_resolve_redirect_url_relative() {
        let base = "https://example.com/path/page";
        let location = reqwest::header::HeaderValue::from_static("/new/path");

        let result = resolve_redirect_url(base, &location).unwrap();
        assert_eq!(result, "https://example.com/new/path");
    }

    #[test]
    fn test_resolve_redirect_url_relative_same_dir() {
        let base = "https://example.com/path/page";
        let location = reqwest::header::HeaderValue::from_static("other");

        let result = resolve_redirect_url(base, &location).unwrap();
        assert_eq!(result, "https://example.com/path/other");
    }

    #[test]
    fn test_validate_default_config() {
        let config = HttpRetryConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_jitter_fails() {
        let config = HttpRetryConfig {
            jitter_factor: 0.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_negative_jitter_fails() {
        let config = HttpRetryConfig {
            jitter_factor: -0.1,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_too_high_jitter_fails() {
        let config = HttpRetryConfig {
            jitter_factor: 1.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_delay_range_fails() {
        let config = HttpRetryConfig {
            base_delay_ms: 5000,
            max_delay_ms: 1000, // max < base is invalid
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validated_constructor() {
        let config = HttpRetryConfig::validated();
        assert!(config.validate().is_ok());
    }
}
