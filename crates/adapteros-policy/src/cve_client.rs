//! CVE Database Client Library
//!
//! Provides OSV API client with comprehensive caching, rate limiting, and error handling.
//! Implements fair-use rate limiting for the free OSV API endpoint.

use adapteros_core::{AosError, Result};
use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};

/// OSV API endpoint
const OSV_API_ENDPOINT: &str = "https://api.osv.dev/v1/query";

/// Fair-use rate limiting: max requests per second
const DEFAULT_RATE_LIMIT: u32 = 4; // Conservative rate limit for public OSV API

/// Request timeout in seconds
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Represents a package ecosystem supported by OSV
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum PackageEcosystem {
    /// Rust packages (crates.io)
    Rust,
    /// Python packages (PyPI)
    Python,
    /// Node.js packages (npm)
    Npm,
    /// Go packages
    Go,
    /// Java packages (Maven Central)
    Maven,
    /// PHP packages (Packagist)
    Composer,
    /// Ruby gems
    Gem,
    /// .NET packages (NuGet)
    NuGet,
    /// Debian packages
    Debian,
    /// Alpine packages
    Alpine,
    /// Ubuntu packages
    Ubuntu,
    /// macOS/Homebrew packages
    Homebrew,
}

impl PackageEcosystem {
    /// Get ecosystem name for API calls
    pub fn as_str(&self) -> &'static str {
        match self {
            PackageEcosystem::Rust => "crates.io",
            PackageEcosystem::Python => "PyPI",
            PackageEcosystem::Npm => "npm",
            PackageEcosystem::Go => "Go",
            PackageEcosystem::Maven => "Maven",
            PackageEcosystem::Composer => "Packagist",
            PackageEcosystem::Gem => "RubyGems",
            PackageEcosystem::NuGet => "NuGet",
            PackageEcosystem::Debian => "Debian",
            PackageEcosystem::Alpine => "Alpine",
            PackageEcosystem::Ubuntu => "Ubuntu",
            PackageEcosystem::Homebrew => "Homebrew",
        }
    }
}

/// OSV API request
#[derive(Debug, Clone, Serialize)]
struct OsvQueryRequest {
    /// Package name
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<OsvPackageInfo>,
    /// Commit hash (alternative to package)
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<String>,
}

/// Package information for OSV query
#[derive(Debug, Clone, Serialize)]
struct OsvPackageInfo {
    /// Package name
    pub name: String,
    /// Package ecosystem
    pub ecosystem: String,
    /// Package version
    pub version: String,
}

/// OSV API response
#[derive(Debug, Clone, Deserialize)]
pub struct OsvResponse {
    /// List of vulnerabilities
    #[serde(default)]
    pub vulns: Vec<OsvVulnerability>,
}

/// OSV vulnerability entry
#[derive(Debug, Clone, Deserialize)]
pub struct OsvVulnerability {
    /// OSV ID (e.g., "GHSA-xxxx-yyyy-zzzz")
    pub id: String,
    /// CVE ID if available (e.g., "CVE-2024-1234")
    #[serde(default)]
    pub cves: Vec<String>,
    /// Brief summary
    #[serde(default)]
    pub summary: Option<String>,
    /// Detailed description
    #[serde(default)]
    pub details: Option<String>,
    /// Published timestamp (RFC 3339)
    #[serde(default)]
    pub published: Option<String>,
    /// Modified timestamp (RFC 3339)
    #[serde(default)]
    pub modified: Option<String>,
    /// Affected versions and ranges
    #[serde(default)]
    pub affected: Vec<AffectedRange>,
    /// CVSS score if available
    #[serde(default)]
    pub severity: Option<String>,
    /// References
    #[serde(default)]
    pub references: Vec<OsvReference>,
    /// CWE IDs
    #[serde(default)]
    pub cwe_ids: Vec<String>,
}

/// Affected version range
#[derive(Debug, Clone, Deserialize)]
pub struct AffectedRange {
    /// Version type (e.g., "SEMVER", "GIT")
    pub r#type: String,
    /// Events that define the affected range
    pub events: Vec<VersionEvent>,
}

/// Version event in affected range
#[derive(Debug, Clone, Deserialize)]
pub struct VersionEvent {
    /// Introduced version (or "*" for all)
    #[serde(default)]
    pub introduced: Option<String>,
    /// Fixed version
    #[serde(default)]
    pub fixed: Option<String>,
}

/// OSV reference
#[derive(Debug, Clone, Deserialize)]
pub struct OsvReference {
    /// Reference type
    pub r#type: String,
    /// Reference URL
    pub url: String,
}

/// Cached OSV response with metadata
#[derive(Debug, Clone)]
pub struct CachedOsvResponse {
    /// Response data
    pub response: OsvResponse,
    /// Cache timestamp
    pub cached_at: DateTime<Utc>,
    /// Whether lookup was successful
    pub success: bool,
    /// Error message if unsuccessful
    pub error: Option<String>,
}

/// Configuration for OSV client
#[derive(Debug, Clone)]
pub struct OsvClientConfig {
    /// Rate limit (requests per second)
    pub rate_limit: u32,
    /// Request timeout in seconds
    pub request_timeout_secs: u64,
    /// Enable detailed logging
    pub verbose_logging: bool,
    /// Override OSV base URL (primarily for tests)
    pub base_url: String,
    /// Optional mock response for offline testing
    pub mock_response: Option<OsvResponse>,
}

impl Default for OsvClientConfig {
    fn default() -> Self {
        Self {
            rate_limit: DEFAULT_RATE_LIMIT,
            request_timeout_secs: REQUEST_TIMEOUT_SECS,
            verbose_logging: false,
            base_url: OSV_API_ENDPOINT.to_string(),
            mock_response: None,
        }
    }
}

/// OSV API client with rate limiting and error handling
pub struct OsvClient {
    config: OsvClientConfig,
    http_client: reqwest::Client,
    /// Rate limiter: semaphore-based token bucket
    rate_limiter: Arc<Semaphore>,
    /// Track last request time for rate limiting
    last_request_time: Arc<std::sync::Mutex<Instant>>,
    /// Request counter for stats
    request_count: Arc<AtomicU32>,
    /// Error counter for monitoring
    error_count: Arc<AtomicU32>,
}

impl OsvClient {
    /// Create new OSV client with default configuration
    pub fn new() -> Self {
        Self::with_config(OsvClientConfig::default())
    }

    /// Create new OSV client with custom configuration
    pub fn with_config(config: OsvClientConfig) -> Self {
        let rate_limit = config.rate_limit;
        let timeout_secs = config.request_timeout_secs;

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .user_agent("adapteros-policy/0.1.0 (+https://github.com/rogu3bear/aos)")
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        info!(
            rate_limit = rate_limit,
            timeout_secs = timeout_secs,
            "Initialized OSV API client"
        );

        Self {
            config,
            http_client,
            rate_limiter: Arc::new(Semaphore::new(rate_limit as usize)),
            last_request_time: Arc::new(std::sync::Mutex::new(Instant::now())),
            request_count: Arc::new(AtomicU32::new(0)),
            error_count: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Query OSV database for vulnerabilities affecting a package version
    ///
    /// Returns vulnerability information if found, or empty list if none.
    pub async fn query_package(
        &self,
        ecosystem: PackageEcosystem,
        name: &str,
        version: &str,
    ) -> Result<OsvResponse> {
        // Apply rate limiting
        self.apply_rate_limit().await?;

        let request_body = OsvQueryRequest {
            package: Some(OsvPackageInfo {
                name: name.to_string(),
                ecosystem: ecosystem.as_str().to_string(),
                version: version.to_string(),
            }),
            commit: None,
        };

        debug!(
            ecosystem = ecosystem.as_str(),
            package = name,
            version = version,
            "Querying OSV database"
        );

        match self.execute_request(&request_body).await {
            Ok(response) => {
                self.request_count.fetch_add(1, Ordering::Relaxed);

                if !response.vulns.is_empty() {
                    info!(
                        ecosystem = ecosystem.as_str(),
                        package = name,
                        version = version,
                        vuln_count = response.vulns.len(),
                        "Found vulnerabilities in OSV database"
                    );
                } else if self.config.verbose_logging {
                    debug!(
                        ecosystem = ecosystem.as_str(),
                        package = name,
                        version = version,
                        "No vulnerabilities found in OSV database"
                    );
                }

                Ok(response)
            }
            Err(e) => {
                self.error_count.fetch_add(1, Ordering::Relaxed);
                error!(
                    ecosystem = ecosystem.as_str(),
                    package = name,
                    version = version,
                    error = %e,
                    "Failed to query OSV database"
                );
                Err(e)
            }
        }
    }

    /// Query OSV by commit hash
    pub async fn query_commit(&self, commit_hash: &str) -> Result<OsvResponse> {
        // Apply rate limiting
        self.apply_rate_limit().await?;

        let request_body = OsvQueryRequest {
            package: None,
            commit: Some(commit_hash.to_string()),
        };

        debug!(commit = commit_hash, "Querying OSV by commit");

        match self.execute_request(&request_body).await {
            Ok(response) => {
                self.request_count.fetch_add(1, Ordering::Relaxed);

                if !response.vulns.is_empty() {
                    info!(
                        commit = commit_hash,
                        vuln_count = response.vulns.len(),
                        "Found vulnerabilities for commit"
                    );
                } else if self.config.verbose_logging {
                    debug!(commit = commit_hash, "No vulnerabilities found for commit");
                }

                Ok(response)
            }
            Err(e) => {
                self.error_count.fetch_add(1, Ordering::Relaxed);
                error!(commit = commit_hash, error = %e, "Failed to query commit");
                Err(e)
            }
        }
    }

    /// Execute HTTP request with error handling
    async fn execute_request(&self, request_body: &OsvQueryRequest) -> Result<OsvResponse> {
        if let Some(mock) = &self.config.mock_response {
            return Ok(mock.clone());
        }

        let request = self
            .http_client
            .post(&self.config.base_url)
            .json(request_body)
            .build()
            .map_err(|e| AosError::Network(format!("Failed to build OSV request: {}", e)))?;

        let response = self
            .http_client
            .execute(request)
            .await
            .map_err(|e| AosError::Network(format!("OSV API request failed: {}", e)))?;

        let status = response.status();

        match status {
            StatusCode::OK => response
                .json::<OsvResponse>()
                .await
                .map_err(|e| AosError::Network(format!("Failed to parse OSV response: {}", e))),
            StatusCode::BAD_REQUEST => Err(AosError::Validation(
                "Invalid OSV query parameters".to_string(),
            )),
            StatusCode::TOO_MANY_REQUESTS => {
                warn!("OSV API rate limited (429)");
                Err(AosError::Network(
                    "OSV API rate limited - please retry later".to_string(),
                ))
            }
            StatusCode::SERVICE_UNAVAILABLE => {
                warn!("OSV API temporarily unavailable (503)");
                Err(AosError::Network(
                    "OSV API temporarily unavailable".to_string(),
                ))
            }
            status => {
                let body = response.text().await.unwrap_or_default();
                error!(
                    status = %status,
                    body = %body,
                    "Unexpected OSV API response"
                );
                Err(AosError::Network(format!(
                    "OSV API error: {} - {}",
                    status, body
                )))
            }
        }
    }

    /// Apply rate limiting using token bucket algorithm
    async fn apply_rate_limit(&self) -> Result<()> {
        // Acquire a permit from the semaphore
        self.rate_limiter
            .acquire()
            .await
            .map_err(|_| AosError::Network("Rate limiter error".to_string()))?
            .forget();

        // Calculate delay to maintain rate limit
        // Note: We must NOT hold the MutexGuard across an await point, as it's not Send
        let delay_millis = {
            let now = Instant::now();
            let last_time = self
                .last_request_time
                .lock()
                .map_err(|_| AosError::Network("Lock poisoned".to_string()))?;

            let min_interval_millis = 1000 / self.config.rate_limit as u64;
            let elapsed = now.duration_since(*last_time).as_millis() as u64;

            if elapsed < min_interval_millis {
                Some(min_interval_millis - elapsed)
            } else {
                None
            }
            // MutexGuard is dropped here before any await
        };

        // Now we can safely await without holding the lock
        if let Some(delay) = delay_millis {
            if self.config.verbose_logging {
                debug!("Rate limit: sleeping {}ms", delay);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
        }

        // Update last request time - acquire lock briefly
        {
            let mut last_time = self
                .last_request_time
                .lock()
                .map_err(|_| AosError::Network("Lock poisoned".to_string()))?;
            *last_time = Instant::now();
        }

        // Return a permit for future requests
        let available_permits = self.rate_limiter.available_permits();
        if available_permits < (self.config.rate_limit as usize) {
            // Re-add a permit since we acquired and forgot it
            self.rate_limiter.add_permits(1);
        }

        Ok(())
    }

    /// Get client statistics
    pub fn stats(&self) -> OsvClientStats {
        OsvClientStats {
            total_requests: self.request_count.load(Ordering::Relaxed),
            total_errors: self.error_count.load(Ordering::Relaxed),
            rate_limit: self.config.rate_limit,
        }
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        self.request_count.store(0, Ordering::Relaxed);
        self.error_count.store(0, Ordering::Relaxed);
        info!("Reset OSV client statistics");
    }
}

impl Default for OsvClient {
    fn default() -> Self {
        Self::new()
    }
}

/// OSV client statistics
#[derive(Debug, Clone)]
pub struct OsvClientStats {
    /// Total requests made
    pub total_requests: u32,
    /// Total errors encountered
    pub total_errors: u32,
    /// Configured rate limit (requests/sec)
    pub rate_limit: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_ecosystem_display() {
        assert_eq!(PackageEcosystem::Rust.as_str(), "crates.io");
        assert_eq!(PackageEcosystem::Python.as_str(), "PyPI");
        assert_eq!(PackageEcosystem::Npm.as_str(), "npm");
        assert_eq!(PackageEcosystem::Go.as_str(), "Go");
    }

    #[test]
    fn test_osv_client_creation() {
        let client = OsvClient::new();
        assert_eq!(client.config.rate_limit, DEFAULT_RATE_LIMIT);
        assert_eq!(client.config.request_timeout_secs, REQUEST_TIMEOUT_SECS);
    }

    #[test]
    fn test_osv_client_with_custom_config() {
        let config = OsvClientConfig {
            rate_limit: 2,
            request_timeout_secs: 60,
            verbose_logging: true,
            ..OsvClientConfig::default()
        };
        let client = OsvClient::with_config(config.clone());
        assert_eq!(client.config.rate_limit, 2);
        assert_eq!(client.config.request_timeout_secs, 60);
        assert!(client.config.verbose_logging);
    }

    #[test]
    fn test_osv_client_stats() {
        let client = OsvClient::new();
        let stats = client.stats();
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.total_errors, 0);
        assert_eq!(stats.rate_limit, DEFAULT_RATE_LIMIT);
    }

    #[test]
    fn test_osv_response_deserialization() {
        let json = r#"{
            "vulns": [
                {
                    "id": "GHSA-1234-5678-9abc",
                    "cves": ["CVE-2024-1234"],
                    "summary": "Test vulnerability",
                    "details": "Test details",
                    "published": "2024-01-01T00:00:00Z",
                    "modified": "2024-01-02T00:00:00Z",
                    "affected": [
                        {
                            "type": "SEMVER",
                            "events": [
                                {"introduced": "1.0.0"},
                                {"fixed": "1.0.5"}
                            ]
                        }
                    ],
                    "severity": "HIGH",
                    "references": [
                        {"type": "WEB", "url": "https://example.com"}
                    ],
                    "cwe_ids": ["CWE-79"]
                }
            ]
        }"#;

        let response: OsvResponse = serde_json::from_str(json).expect("Failed to parse JSON");
        assert_eq!(response.vulns.len(), 1);

        let vuln = &response.vulns[0];
        assert_eq!(vuln.id, "GHSA-1234-5678-9abc");
        assert_eq!(vuln.cves.len(), 1);
        assert_eq!(vuln.cves[0], "CVE-2024-1234");
        assert_eq!(vuln.affected.len(), 1);
        assert_eq!(vuln.cwe_ids.len(), 1);
    }

    #[test]
    fn test_osv_response_with_minimal_fields() {
        let json = r#"{
            "vulns": [
                {
                    "id": "GHSA-minimal",
                    "cves": [],
                    "affected": [],
                    "references": [],
                    "cwe_ids": []
                }
            ]
        }"#;

        let response: OsvResponse = serde_json::from_str(json).expect("Failed to parse JSON");
        assert_eq!(response.vulns.len(), 1);
        assert!(response.vulns[0].summary.is_none());
    }

    #[test]
    fn test_osv_response_empty() {
        let json = r#"{"vulns": []}"#;
        let response: OsvResponse = serde_json::from_str(json).expect("Failed to parse JSON");
        assert_eq!(response.vulns.len(), 0);
    }

    #[tokio::test]
    async fn test_osv_client_rate_limiting() {
        let config = OsvClientConfig {
            rate_limit: 100, // High limit to avoid blocking in tests
            request_timeout_secs: 5,
            verbose_logging: false,
            ..OsvClientConfig::default()
        };
        let client = OsvClient::with_config(config);

        // Apply rate limit multiple times - should not block
        for _ in 0..3 {
            let result = client.apply_rate_limit().await;
            assert!(result.is_ok());
        }

        let stats = client.stats();
        assert_eq!(stats.total_requests, 0); // No actual requests made
    }

    #[test]
    fn test_osv_vulnerability_with_affected_range() {
        let json = r#"{
            "vulns": [
                {
                    "id": "GHSA-1234-5678-9abc",
                    "cves": ["CVE-2024-1234"],
                    "affected": [
                        {
                            "type": "SEMVER",
                            "events": [
                                {"introduced": "0.0.0"},
                                {"fixed": "1.2.0"}
                            ]
                        },
                        {
                            "type": "SEMVER",
                            "events": [
                                {"introduced": "2.0.0"},
                                {"fixed": "2.0.8"}
                            ]
                        }
                    ],
                    "references": [],
                    "cwe_ids": []
                }
            ]
        }"#;

        let response: OsvResponse = serde_json::from_str(json).expect("Failed to parse JSON");
        let vuln = &response.vulns[0];
        assert_eq!(vuln.affected.len(), 2);

        // Check first affected range
        assert_eq!(vuln.affected[0].r#type, "SEMVER");
        assert_eq!(vuln.affected[0].events.len(), 2);
        assert_eq!(
            vuln.affected[0].events[0].introduced,
            Some("0.0.0".to_string())
        );
        assert_eq!(vuln.affected[0].events[1].fixed, Some("1.2.0".to_string()));

        // Check second affected range
        assert_eq!(
            vuln.affected[1].events[0].introduced,
            Some("2.0.0".to_string())
        );
        assert_eq!(vuln.affected[1].events[1].fixed, Some("2.0.8".to_string()));
    }

    #[tokio::test]
    async fn test_osv_client_query_package_offline() {
        let mock_response = OsvResponse {
            vulns: vec![OsvVulnerability {
                id: "GHSA-offline-1234".to_string(),
                cves: vec!["CVE-2024-0001".to_string()],
                summary: Some("Offline response".to_string()),
                details: None,
                published: None,
                modified: None,
                affected: vec![],
                severity: Some("HIGH".to_string()),
                references: vec![],
                cwe_ids: vec![],
            }],
        };

        let client = OsvClient::with_config(OsvClientConfig {
            rate_limit: 1000,
            request_timeout_secs: 5,
            verbose_logging: false,
            mock_response: Some(mock_response.clone()),
            ..OsvClientConfig::default()
        });

        let response = client
            .query_package(PackageEcosystem::Rust, "offline-crate", "1.0.0")
            .await
            .expect("OSV query should use mock response");

        assert_eq!(response.vulns.len(), 1);
        assert_eq!(response.vulns[0].id, "GHSA-offline-1234");
        assert_eq!(
            response.vulns[0].cves.first(),
            Some(&"CVE-2024-0001".to_string())
        );
        assert_eq!(
            response.vulns[0].summary.as_deref(),
            Some("Offline response")
        );
    }
}
