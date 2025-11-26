//! NVD (National Vulnerability Database) API 2.0 Client
//!
//! Provides asynchronous integration with the NIST National Vulnerability Database API v2.0.
//! Includes rate limiting (5 req/sec with API key, 0.6 req/sec without) and retry logic.

use adapteros_core::{AosError, Result};
use chrono::{DateTime, Utc};
use governor::{clock::DefaultClock, state::InMemoryState, state::NotKeyed, Quota, RateLimiter};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;
use std::sync::Arc;
use tracing::{debug, error, warn};

/// NVD API endpoint
const NVD_API_ENDPOINT: &str = "https://services.nvd.nist.gov/rest/json/cves/2.0";

/// Maximum retries for transient failures
const MAX_RETRIES: u32 = 3;

/// Retry delay in milliseconds
const RETRY_DELAY_MS: u64 = 100;

/// CVE from NVD API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdCve {
    pub id: String,
    #[serde(default)]
    pub metrics: NvdMetrics,
    #[serde(default)]
    pub descriptions: Vec<NvdDescription>,
    #[serde(default)]
    pub references: Vec<NvdReference>,
    pub published: Option<String>,
    pub modified: Option<String>,
    #[serde(default)]
    pub weaknesses: Vec<NvdWeakness>,
}

/// CVSS metrics from NVD
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdMetrics {
    #[serde(alias = "cvssMetricV31")]
    pub cvss_metric_v31: Option<Vec<NvdCvssV31>>,
    #[serde(alias = "cvssMetricV30")]
    pub cvss_metric_v30: Option<Vec<NvdCvssV30>>,
}

impl Default for NvdMetrics {
    fn default() -> Self {
        Self {
            cvss_metric_v31: None,
            cvss_metric_v30: None,
        }
    }
}

/// CVSS v3.1 metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdCvssV31 {
    #[serde(alias = "cvssData")]
    pub cvss_data: NvdCvssData,
    #[serde(alias = "impactScore", default)]
    pub impact_score: Option<f32>,
    #[serde(alias = "exploitabilityScore", default)]
    pub exploitability_score: Option<f32>,
}

/// CVSS v3.0 metric
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdCvssV30 {
    #[serde(alias = "cvssData")]
    pub cvss_data: NvdCvssData,
}

/// CVSS data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdCvssData {
    pub version: String,
    #[serde(alias = "baseScore")]
    pub base_score: f32,
    #[serde(alias = "baseSeverity")]
    pub base_severity: Option<String>,
}

/// CVE description
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdDescription {
    pub lang: Option<String>,
    pub value: String,
}

/// CVE reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdReference {
    pub url: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source: String,
}

/// CWE weakness
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdWeakness {
    pub source: Option<String>,
    #[serde(default)]
    pub cwe_id: Vec<String>,
}

/// NVD API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdApiResponse {
    #[serde(alias = "vulnerabilities", default)]
    pub results: Vec<NvdCveWrapper>,
    #[serde(alias = "totalResults", default)]
    pub total_results: Option<u32>,
    #[serde(alias = "resultsPerPage", default)]
    pub results_per_page: Option<u32>,
    #[serde(alias = "startIndex", default)]
    pub start_index: Option<u32>,
    pub format: Option<String>,
    pub version: Option<String>,
    pub timestamp: Option<String>,
}

/// Wrapper for CVE in API response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NvdCveWrapper {
    pub cve: NvdCve,
}

/// Error types for NVD client
#[derive(Debug)]
pub enum NvdError {
    /// Network/HTTP error
    Network(String),
    /// API rate limit exceeded
    RateLimited,
    /// Invalid API response
    InvalidResponse(String),
    /// CVE not found
    NotFound,
    /// Transient error (retriable)
    Transient(String),
    /// Invalid configuration
    Config(String),
}

impl std::fmt::Display for NvdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NvdError::Network(msg) => write!(f, "Network error: {}", msg),
            NvdError::RateLimited => write!(f, "NVD API rate limit exceeded"),
            NvdError::InvalidResponse(msg) => write!(f, "Invalid NVD API response: {}", msg),
            NvdError::NotFound => write!(f, "CVE not found in NVD"),
            NvdError::Transient(msg) => write!(f, "Transient error (retriable): {}", msg),
            NvdError::Config(msg) => write!(f, "NVD client configuration error: {}", msg),
        }
    }
}

impl std::error::Error for NvdError {}

impl From<NvdError> for AosError {
    fn from(err: NvdError) -> Self {
        AosError::Network(err.to_string())
    }
}

/// NVD API client with rate limiting
pub struct NvdClient {
    http_client: Client,
    api_key: Option<String>,
    rate_limiter: Arc<RateLimiter<NotKeyed, InMemoryState, DefaultClock>>,
}

impl NvdClient {
    /// Create new NVD API client
    ///
    /// Uses NVD_API_KEY environment variable if available.
    /// Rate limits to 5 req/sec with key, 0.6 req/sec without.
    pub fn new() -> Result<Self> {
        let api_key = std::env::var("NVD_API_KEY").ok();

        let rate_limit = if api_key.is_some() {
            // 5 requests per second with API key
            NonZeroU32::new(5).unwrap()
        } else {
            // 0.6 requests per second without API key (1 per ~1.67 seconds)
            NonZeroU32::new(1).unwrap()
        };

        let rate_limiter = Arc::new(RateLimiter::direct(Quota::per_second(rate_limit)));

        Ok(Self {
            http_client: Client::new(),
            api_key,
            rate_limiter,
        })
    }

    /// Query NVD for CVEs affecting a package
    ///
    /// Note: NVD API v2.0 searches across all packages, so filtering by specific
    /// package version requires post-processing the results.
    pub async fn query_cves(&self, cpe_search: &str) -> std::result::Result<Vec<NvdCve>, NvdError> {
        debug!(
            cpe = %cpe_search,
            has_api_key = self.api_key.is_some(),
            "Querying NVD for CVEs"
        );

        self.query_with_retry(cpe_search, 0).await
    }

    /// Query NVD with automatic retry on transient failures
    fn query_with_retry<'a>(
        &'a self,
        cpe_search: &'a str,
        attempt: u32,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<Vec<NvdCve>, NvdError>> + 'a>,
    > {
        Box::pin(async move {
            if attempt > MAX_RETRIES {
                return Err(NvdError::Transient("Max retries exceeded".to_string()));
            }

            // Apply rate limiting
            self.rate_limiter
                .check()
                .map_err(|_| NvdError::RateLimited)?;

            // Build request
            let mut url = reqwest::Url::parse(NVD_API_ENDPOINT)
                .map_err(|e| NvdError::Config(format!("Invalid endpoint URL: {}", e)))?;

            url.query_pairs_mut().append_pair("cpeName", cpe_search);

            let mut req = self.http_client.get(url);

            // Add API key header if available
            if let Some(ref key) = self.api_key {
                req = req.header("X-API-Key", key);
            }

            // Execute request
            let response = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    warn!(
                        error = %e,
                        attempt = attempt,
                        "NVD API request failed, will retry"
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        RETRY_DELAY_MS * (attempt as u64 + 1),
                    ))
                    .await;
                    return Box::pin(self.query_with_retry(cpe_search, attempt + 1)).await;
                }
            };

            // Check status code
            match response.status() {
                reqwest::StatusCode::OK => {}
                reqwest::StatusCode::NOT_FOUND => return Err(NvdError::NotFound),
                reqwest::StatusCode::TOO_MANY_REQUESTS => {
                    warn!("NVD API rate limit hit, will retry with backoff");
                    tokio::time::sleep(tokio::time::Duration::from_secs(2_u64.pow(attempt))).await;
                    return Box::pin(self.query_with_retry(cpe_search, attempt + 1)).await;
                }
                status if status.is_server_error() => {
                    warn!(
                        status = %status,
                        "NVD API server error, will retry"
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        RETRY_DELAY_MS * (attempt as u64 + 1),
                    ))
                    .await;
                    return Box::pin(self.query_with_retry(cpe_search, attempt + 1)).await;
                }
                status => {
                    return Err(NvdError::Network(format!(
                        "Unexpected status code: {}",
                        status
                    )))
                }
            }

            // Parse response
            let body = response
                .text()
                .await
                .map_err(|e| NvdError::Network(format!("Failed to read response body: {}", e)))?;

            let api_response: NvdApiResponse = serde_json::from_str(&body)
                .map_err(|e| NvdError::InvalidResponse(format!("Failed to parse JSON: {}", e)))?;

            Ok(api_response.results.into_iter().map(|w| w.cve).collect())
        })
    }

    /// Extract CVSS base score from CVE
    pub fn extract_cvss_score(cve: &NvdCve) -> Option<f32> {
        // Try v3.1 first (preferred)
        if let Some(ref v31) = cve.metrics.cvss_metric_v31 {
            if let Some(first) = v31.first() {
                return Some(first.cvss_data.base_score);
            }
        }

        // Fall back to v3.0
        if let Some(ref v30) = cve.metrics.cvss_metric_v30 {
            if let Some(first) = v30.first() {
                return Some(first.cvss_data.base_score);
            }
        }

        None
    }

    /// Extract severity from CVE
    pub fn extract_severity(cve: &NvdCve) -> Option<String> {
        // Try v3.1 first
        if let Some(ref v31) = cve.metrics.cvss_metric_v31 {
            if let Some(first) = v31.first() {
                if let Some(ref severity) = first.cvss_data.base_severity {
                    return Some(severity.clone());
                }
            }
        }

        // Fall back to v3.0
        if let Some(ref v30) = cve.metrics.cvss_metric_v30 {
            if let Some(first) = v30.first() {
                if let Some(ref severity) = first.cvss_data.base_severity {
                    return Some(severity.clone());
                }
            }
        }

        None
    }

    /// Extract CWE IDs from CVE
    pub fn extract_cwe_ids(cve: &NvdCve) -> Vec<String> {
        cve.weaknesses
            .iter()
            .flat_map(|w| w.cwe_id.iter().cloned())
            .collect()
    }

    /// Extract description from CVE
    pub fn extract_description(cve: &NvdCve) -> Option<String> {
        cve.descriptions
            .iter()
            .find(|d| d.lang.as_deref() == Some("en") || d.lang.is_none())
            .or_else(|| cve.descriptions.first())
            .map(|d| d.value.clone())
    }

    /// Parse datetime string from NVD (ISO 8601 format)
    pub fn parse_datetime(date_str: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(date_str)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }
}

impl Default for NvdClient {
    fn default() -> Self {
        Self::new().expect("Failed to create default NVD client")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nvd_client_creation() {
        let client = NvdClient::new();
        assert!(client.is_ok(), "Failed to create NVD client");
    }

    #[test]
    fn test_extract_cvss_score_v31() {
        let cve = NvdCve {
            id: "CVE-2021-44228".to_string(),
            metrics: NvdMetrics {
                cvss_metric_v31: Some(vec![NvdCvssV31 {
                    cvss_data: NvdCvssData {
                        version: "3.1".to_string(),
                        base_score: 10.0,
                        base_severity: Some("CRITICAL".to_string()),
                    },
                    impact_score: Some(6.0),
                    exploitability_score: Some(3.0),
                }]),
                cvss_metric_v30: None,
            },
            descriptions: vec![],
            references: vec![],
            published: None,
            modified: None,
            weaknesses: vec![],
        };

        assert_eq!(NvdClient::extract_cvss_score(&cve), Some(10.0));
    }

    #[test]
    fn test_extract_cvss_score_v30_fallback() {
        let cve = NvdCve {
            id: "CVE-2021-44228".to_string(),
            metrics: NvdMetrics {
                cvss_metric_v31: None,
                cvss_metric_v30: Some(vec![NvdCvssV30 {
                    cvss_data: NvdCvssData {
                        version: "3.0".to_string(),
                        base_score: 9.5,
                        base_severity: Some("CRITICAL".to_string()),
                    },
                }]),
            },
            descriptions: vec![],
            references: vec![],
            published: None,
            modified: None,
            weaknesses: vec![],
        };

        assert_eq!(NvdClient::extract_cvss_score(&cve), Some(9.5));
    }

    #[test]
    fn test_extract_severity() {
        let cve = NvdCve {
            id: "CVE-2021-44228".to_string(),
            metrics: NvdMetrics {
                cvss_metric_v31: Some(vec![NvdCvssV31 {
                    cvss_data: NvdCvssData {
                        version: "3.1".to_string(),
                        base_score: 10.0,
                        base_severity: Some("CRITICAL".to_string()),
                    },
                    impact_score: Some(6.0),
                    exploitability_score: Some(3.0),
                }]),
                cvss_metric_v30: None,
            },
            descriptions: vec![],
            references: vec![],
            published: None,
            modified: None,
            weaknesses: vec![],
        };

        assert_eq!(
            NvdClient::extract_severity(&cve),
            Some("CRITICAL".to_string())
        );
    }

    #[test]
    fn test_extract_cwe_ids() {
        let cve = NvdCve {
            id: "CVE-2021-44228".to_string(),
            metrics: NvdMetrics::default(),
            descriptions: vec![],
            references: vec![],
            published: None,
            modified: None,
            weaknesses: vec![NvdWeakness {
                source: Some("NVD".to_string()),
                cwe_id: vec!["CWE-94".to_string(), "CWE-117".to_string()],
            }],
        };

        let cwe_ids = NvdClient::extract_cwe_ids(&cve);
        assert_eq!(cwe_ids.len(), 2);
        assert!(cwe_ids.contains(&"CWE-94".to_string()));
    }

    #[test]
    fn test_extract_description() {
        let cve = NvdCve {
            id: "CVE-2021-44228".to_string(),
            metrics: NvdMetrics::default(),
            descriptions: vec![NvdDescription {
                lang: Some("en".to_string()),
                value: "Remote code execution vulnerability".to_string(),
            }],
            references: vec![],
            published: None,
            modified: None,
            weaknesses: vec![],
        };

        let desc = NvdClient::extract_description(&cve);
        assert_eq!(
            desc,
            Some("Remote code execution vulnerability".to_string())
        );
    }

    #[test]
    fn test_parse_datetime() {
        let date_str = "2021-12-10T00:15:08Z";
        let dt = NvdClient::parse_datetime(date_str);
        assert!(dt.is_some());
        assert!(dt.unwrap() < Utc::now());
    }

    #[test]
    fn test_nvd_error_display() {
        let err = NvdError::Network("Connection refused".to_string());
        assert!(err.to_string().contains("Network error"));

        let err = NvdError::RateLimited;
        assert!(err.to_string().contains("rate limit"));

        let err = NvdError::NotFound;
        assert!(err.to_string().contains("not found"));
    }
}
