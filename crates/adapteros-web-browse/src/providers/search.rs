//! Search provider implementations
//!
//! Provides web search capabilities through various APIs.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::{
    error::{WebBrowseError, WebBrowseResult},
    evidence::{EvidenceBuilder, EvidenceType, Freshness, SourceRecord},
    service::{WebSearchRequest, WebSearchResponse, WebSearchResult},
    TenantId,
};

/// Search provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchProviderConfig {
    /// API endpoint URL
    pub api_url: String,

    /// API key (from environment variable)
    pub api_key: Option<String>,

    /// Request timeout in seconds
    pub timeout_secs: u32,

    /// Maximum results per query
    pub max_results: u32,

    /// User agent string
    pub user_agent: String,
}

impl Default for SearchProviderConfig {
    fn default() -> Self {
        Self {
            api_url: String::new(),
            api_key: None,
            timeout_secs: 10,
            max_results: 10,
            user_agent: "adapterOS-WebBrowse/1.0".to_string(),
        }
    }
}

/// Search provider trait
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Check if provider is configured
    fn is_configured(&self) -> bool;

    /// Perform web search
    async fn search(
        &self,
        tenant_id: &TenantId,
        request: &WebSearchRequest,
    ) -> WebBrowseResult<WebSearchResponse>;
}

/// Brave Search API provider
pub struct BraveSearchProvider {
    config: SearchProviderConfig,
    client: Client,
}

impl BraveSearchProvider {
    /// Create new Brave search provider
    pub fn new(mut config: SearchProviderConfig) -> Self {
        // Set default URL if not provided
        if config.api_url.is_empty() {
            config.api_url = "https://api.search.brave.com/res/v1/web/search".to_string();
        }

        // Try to load API key from environment
        if config.api_key.is_none() {
            config.api_key = std::env::var("BRAVE_SEARCH_API_KEY").ok();
        }

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs as u64))
            .user_agent(&config.user_agent)
            .build()
            .unwrap_or_default();

        Self { config, client }
    }
}

#[async_trait]
impl SearchProvider for BraveSearchProvider {
    fn name(&self) -> &str {
        "brave"
    }

    fn is_configured(&self) -> bool {
        self.config.api_key.is_some()
    }

    async fn search(
        &self,
        tenant_id: &TenantId,
        request: &WebSearchRequest,
    ) -> WebBrowseResult<WebSearchResponse> {
        let api_key =
            self.config
                .api_key
                .as_ref()
                .ok_or_else(|| WebBrowseError::ProviderNotConfigured {
                    provider: "brave".to_string(),
                })?;

        let start = Instant::now();

        let max_results = request.max_results.unwrap_or(self.config.max_results);

        // Build request
        let mut query_params = vec![
            ("q", request.query.clone()),
            ("count", max_results.to_string()),
        ];

        if let Some(freshness) = &request.freshness {
            query_params.push(("freshness", freshness.clone()));
        }

        let response = self
            .client
            .get(&self.config.api_url)
            .header("X-Subscription-Token", api_key)
            .header("Accept", "application/json")
            .query(&query_params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(WebBrowseError::HttpError {
                status: response.status().as_u16(),
                message: response.text().await.unwrap_or_default(),
            });
        }

        let brave_response: BraveSearchResponse = response.json().await.map_err(|e| {
            WebBrowseError::ParseError(format!("Failed to parse Brave response: {}", e))
        })?;

        let latency_ms = start.elapsed().as_millis() as u64;

        // Convert to our response format
        let results: Vec<WebSearchResult> = brave_response
            .web
            .as_ref()
            .map(|web| &web.results)
            .unwrap_or(&Vec::new())
            .iter()
            .map(|r| WebSearchResult {
                title: r.title.clone(),
                url: r.url.clone(),
                snippet: r.description.clone(),
                published_date: r.age.clone(),
                domain: extract_domain(&r.url).unwrap_or_default(),
                relevance_score: 0.8, // Brave doesn't provide scores
            })
            .collect();

        // Build evidence
        let mut evidence_builder =
            EvidenceBuilder::new(tenant_id.clone(), request.request_id.clone())
                .evidence_type(EvidenceType::WebSearch)
                .latency_ms(latency_ms);

        for result in &results {
            let freshness = if let Some(ref age) = result.published_date {
                if age.contains("hour") || age.contains("minute") {
                    Freshness::Fresh
                } else if age.contains("day") {
                    Freshness::Recent
                } else {
                    Freshness::Moderate
                }
            } else {
                Freshness::Unknown
            };

            let source = SourceRecord::new(&result.url)
                .with_title(&result.title)
                .with_freshness(freshness);

            if let Some(ref snippet) = result.snippet {
                evidence_builder = evidence_builder.add_source(source.with_snippet(snippet));
            } else {
                evidence_builder = evidence_builder.add_source(source);
            }
        }

        let evidence = evidence_builder.build();

        Ok(WebSearchResponse {
            results,
            total_results: brave_response
                .web
                .as_ref()
                .map(|w| w.results.len() as u64)
                .unwrap_or(0),
            provider: "brave".to_string(),
            query: request.query.clone(),
            evidence,
            latency_ms,
            from_cache: false,
        })
    }
}

/// Brave Search API response structure
#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    web: Option<BraveWebResults>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResults {
    results: Vec<BraveWebResult>,
}

#[derive(Debug, Deserialize)]
struct BraveWebResult {
    title: String,
    url: String,
    description: Option<String>,
    age: Option<String>,
}

/// Extract domain from URL
fn extract_domain(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_not_configured() {
        let provider = BraveSearchProvider::new(SearchProviderConfig {
            api_key: None,
            ..Default::default()
        });

        assert!(!provider.is_configured());
    }

    #[test]
    fn test_provider_configured() {
        let provider = BraveSearchProvider::new(SearchProviderConfig {
            api_key: Some("test-key".to_string()),
            ..Default::default()
        });

        assert!(provider.is_configured());
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://www.example.com/path"),
            Some("www.example.com".to_string())
        );
        assert_eq!(
            extract_domain("https://example.com"),
            Some("example.com".to_string())
        );
        assert_eq!(extract_domain("not a url"), None);
    }
}
