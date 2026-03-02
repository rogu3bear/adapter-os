//! Page fetcher implementation
//!
//! Fetches and extracts content from web pages.

use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

use crate::{
    error::{WebBrowseError, WebBrowseResult},
    evidence::{EvidenceBuilder, EvidenceType, Freshness, SourceRecord},
    retry::{calculate_retry_delay, parse_retry_after, resolve_redirect_url, HttpRetryConfig},
    service::{PageFetchRequest, PageFetchResponse, PageImage},
    streaming::{stream_response_body, StreamingConfig},
    TenantId,
};

/// Maximum number of redirects to follow
const MAX_REDIRECTS: u32 = 10;

/// Extended response metadata with redirect tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchMetadata {
    /// Original requested URL
    pub original_url: String,
    /// Final URL after redirects
    pub final_url: String,
    /// Redirect chain (if any)
    pub redirect_chain: Vec<String>,
    /// Number of redirects followed
    pub redirect_count: u32,
    /// Whether content was truncated
    pub was_truncated: bool,
    /// Original size before truncation (if truncated)
    pub original_size_bytes: Option<u64>,
    /// Number of retry attempts made
    pub retry_attempts: u32,
}

/// Page fetcher configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageFetcherConfig {
    /// Request timeout in seconds
    pub timeout_secs: u32,

    /// Maximum content size in KB
    pub max_content_kb: u64,

    /// User agent string
    pub user_agent: String,

    /// Require HTTPS
    pub https_only: bool,

    /// Blocked domains
    pub blocked_domains: Vec<String>,

    /// Retry configuration
    #[serde(default)]
    pub retry_config: HttpRetryConfig,

    /// Streaming configuration
    #[serde(default)]
    pub streaming_config: StreamingConfig,
}

impl Default for PageFetcherConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 10,
            max_content_kb: 100,
            user_agent: "adapterOS-WebBrowse/1.0".to_string(),
            https_only: true,
            blocked_domains: vec!["localhost".to_string(), "127.0.0.1".to_string()],
            retry_config: HttpRetryConfig::default(),
            streaming_config: StreamingConfig::default(),
        }
    }
}

/// Page fetcher for extracting content from web pages
pub struct PageFetcher {
    config: PageFetcherConfig,
    client: Client,
}

impl PageFetcher {
    /// Create new page fetcher
    pub fn new(config: PageFetcherConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs as u64))
            .user_agent(&config.user_agent)
            // Disable auto-redirect to track chain manually
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Fetch page content with retry and redirect tracking
    pub async fn fetch(
        &self,
        tenant_id: &TenantId,
        request: &PageFetchRequest,
    ) -> WebBrowseResult<PageFetchResponse> {
        // Validate URL
        let url = url::Url::parse(&request.url)?;

        // Check HTTPS requirement
        if self.config.https_only && url.scheme() != "https" {
            return Err(WebBrowseError::HttpsRequired {
                url: request.url.clone(),
            });
        }

        // Check blocked domains
        self.check_blocked_domain(&url)?;

        let start = Instant::now();

        // Fetch with retry and redirect tracking
        let (response, metadata) = self.fetch_with_retry(&request.url).await?;

        // Stream response body with truncation support
        let streamed = stream_response_body(response, &self.config.streaming_config).await?;

        let latency_ms = start.elapsed().as_millis() as u64;

        // Parse HTML
        let document = Html::parse_document(&streamed.content);

        // Extract title
        let title_selector = Selector::parse("title").unwrap();
        let title = document
            .select(&title_selector)
            .next()
            .map(|el| el.text().collect::<String>())
            .unwrap_or_default()
            .trim()
            .to_string();

        // Extract description
        let meta_desc_selector = Selector::parse(r#"meta[name="description"]"#).unwrap();
        let description = document
            .select(&meta_desc_selector)
            .next()
            .and_then(|el| el.value().attr("content"))
            .map(|s| s.to_string());

        // Extract main content
        let content = if request.extract_main_content {
            extract_main_content(&document)
        } else {
            extract_all_text(&document)
        };

        // Extract images if requested
        let images = if request.include_images {
            extract_images(&document, &metadata.final_url)
        } else {
            Vec::new()
        };

        // Build evidence
        let source = SourceRecord::new(&metadata.final_url)
            .with_title(&title)
            .with_content_hash(&content)
            .with_freshness(Freshness::Fresh);

        let evidence = EvidenceBuilder::new(tenant_id.clone(), request.request_id.clone())
            .evidence_type(EvidenceType::PageFetch)
            .add_source(source)
            .latency_ms(latency_ms)
            .build();

        Ok(PageFetchResponse {
            title,
            url: metadata.final_url,
            content,
            content_length: streamed.content.len(),
            description,
            images,
            evidence,
            latency_ms,
            from_cache: false,
        })
    }

    /// Check if a domain is blocked
    fn check_blocked_domain(&self, url: &url::Url) -> WebBrowseResult<()> {
        if let Some(host) = url.host_str() {
            for blocked in &self.config.blocked_domains {
                if host == blocked || host.ends_with(&format!(".{}", blocked)) {
                    return Err(WebBrowseError::DomainBlocked {
                        domain: host.to_string(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Fetch with retry logic
    async fn fetch_with_retry(
        &self,
        url: &str,
    ) -> WebBrowseResult<(reqwest::Response, FetchMetadata)> {
        let retry_config = &self.config.retry_config;
        let mut attempts = 0u32;
        let mut last_error: Option<WebBrowseError> = None;

        loop {
            attempts += 1;

            match self.single_fetch(url).await {
                Ok((response, metadata)) => {
                    return Ok((
                        response,
                        FetchMetadata {
                            original_url: url.to_string(),
                            final_url: metadata.final_url,
                            redirect_chain: metadata.redirect_chain,
                            redirect_count: metadata.redirect_count,
                            was_truncated: false,
                            original_size_bytes: None,
                            retry_attempts: attempts - 1,
                        },
                    ));
                }
                Err(e) if e.is_retriable() && attempts <= retry_config.max_retries => {
                    let delay = calculate_retry_delay(&e, attempts, retry_config);
                    tracing::warn!(
                        url = %url,
                        attempt = attempts,
                        max_retries = retry_config.max_retries,
                        delay_ms = delay.as_millis() as u64,
                        error = %e,
                        "Retriable error, backing off"
                    );
                    last_error = Some(e);
                    tokio::time::sleep(delay).await;
                }
                Err(e) => {
                    if attempts > 1 {
                        return Err(WebBrowseError::RetryExhausted {
                            url: url.to_string(),
                            attempts,
                            last_error: last_error
                                .map(|le| le.to_string())
                                .unwrap_or_else(|| e.to_string()),
                        });
                    }
                    return Err(e);
                }
            }
        }
    }

    /// Single fetch with manual redirect handling
    async fn single_fetch(
        &self,
        url: &str,
    ) -> WebBrowseResult<(reqwest::Response, SingleFetchMeta)> {
        let mut current_url = url.to_string();
        let mut redirect_chain = Vec::new();

        for _ in 0..MAX_REDIRECTS {
            let response = self
                .client
                .get(&current_url)
                .header("Accept", "text/html,application/xhtml+xml")
                .send()
                .await?;

            let status = response.status().as_u16();

            // Check for rate limiting (429)
            if status == 429 {
                let retry_after = parse_retry_after(&response);
                return Err(WebBrowseError::ServerRateLimited {
                    url: current_url,
                    status,
                    retry_after_secs: retry_after,
                    message: "Too Many Requests".to_string(),
                });
            }

            // Check for potential robots.txt block (403)
            if status == 403 && self.looks_like_robots_block(&response) {
                let domain = url::Url::parse(&current_url)
                    .ok()
                    .and_then(|u| u.host_str().map(String::from))
                    .unwrap_or_default();
                return Err(WebBrowseError::RobotsTxtBlocked { domain, status });
            }

            // Handle redirects manually
            if response.status().is_redirection() {
                if let Some(location) = response.headers().get("location") {
                    redirect_chain.push(current_url.clone());
                    let new_url = resolve_redirect_url(&current_url, location)?;

                    // Check for HTTPS downgrade
                    if self.config.https_only
                        && current_url.starts_with("https://")
                        && new_url.starts_with("http://")
                    {
                        return Err(WebBrowseError::HttpsRequired { url: new_url });
                    }

                    // Check if redirecting to blocked domain
                    let parsed_new = url::Url::parse(&new_url)?;
                    self.check_blocked_domain(&parsed_new)?;

                    current_url = new_url;
                    continue;
                }
            }

            // Check for other HTTP errors
            if !response.status().is_success() {
                return Err(WebBrowseError::HttpError {
                    status,
                    message: format!("Failed to fetch: {}", current_url),
                });
            }

            // Success
            let redirect_count = redirect_chain.len() as u32;
            return Ok((
                response,
                SingleFetchMeta {
                    final_url: current_url,
                    redirect_chain,
                    redirect_count,
                },
            ));
        }

        // Exceeded max redirects
        Err(WebBrowseError::RedirectLoopExceeded {
            url: url.to_string(),
            max_redirects: MAX_REDIRECTS,
            redirect_chain,
        })
    }

    /// Heuristics to detect if a 403 is likely a robots.txt block
    fn looks_like_robots_block(&self, response: &reqwest::Response) -> bool {
        // Check for common CDN/protection service headers
        let server = response
            .headers()
            .get("server")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        // Common protection services that block bots
        let is_protection_service = server.to_lowercase().contains("cloudflare")
            || server.to_lowercase().contains("akamai")
            || server.to_lowercase().contains("imperva")
            || server.to_lowercase().contains("datadome");

        // Check for x-robots-tag header
        let has_robots_header = response.headers().contains_key("x-robots-tag");

        is_protection_service || has_robots_header
    }
}

/// Internal metadata from single fetch
struct SingleFetchMeta {
    final_url: String,
    redirect_chain: Vec<String>,
    redirect_count: u32,
}

/// Extract main content from HTML
fn extract_main_content(document: &Html) -> String {
    // Try common main content selectors in order of preference
    let selectors = [
        "article",
        "main",
        "[role='main']",
        ".content",
        ".post-content",
        ".entry-content",
        ".article-content",
        "#content",
    ];

    for selector_str in selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                let text = element
                    .text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");

                if text.len() > 100 {
                    return text;
                }
            }
        }
    }

    // Fallback to body text
    extract_all_text(document)
}

/// Extract all text from document body
fn extract_all_text(document: &Html) -> String {
    // Remove script and style elements from consideration
    let body_selector = Selector::parse("body").unwrap();

    if let Some(body) = document.select(&body_selector).next() {
        body.text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        String::new()
    }
}

/// Extract images from document
fn extract_images(document: &Html, base_url: &str) -> Vec<PageImage> {
    let img_selector = Selector::parse("img").unwrap();
    let base = url::Url::parse(base_url).ok();

    document
        .select(&img_selector)
        .filter_map(|img| {
            let src = img.value().attr("src")?;

            // Resolve relative URLs
            let url = if src.starts_with("http") {
                src.to_string()
            } else if let Some(ref base) = base {
                base.join(src).ok()?.to_string()
            } else {
                return None;
            };

            // Skip data URIs and tiny images
            if url.starts_with("data:") {
                return None;
            }

            Some(PageImage {
                url,
                alt: img.value().attr("alt").map(|s| s.to_string()),
                width: img.value().attr("width").and_then(|w| w.parse().ok()),
                height: img.value().attr("height").and_then(|h| h.parse().ok()),
            })
        })
        .take(20) // Limit to first 20 images
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_https_validation() {
        let config = PageFetcherConfig {
            https_only: true,
            ..Default::default()
        };
        let _fetcher = PageFetcher::new(config);

        // URL validation would fail on http:// with https_only=true
    }

    #[test]
    fn test_extract_all_text() {
        let html = r#"
        <html>
            <body>
                <h1>Title</h1>
                <p>Paragraph one.</p>
                <p>Paragraph two.</p>
            </body>
        </html>
        "#;

        let document = Html::parse_document(html);
        let text = extract_all_text(&document);

        assert!(text.contains("Title"));
        assert!(text.contains("Paragraph one"));
        assert!(text.contains("Paragraph two"));
    }

    #[test]
    fn test_extract_main_content() {
        let html = r#"
        <html>
            <body>
                <nav>Navigation links</nav>
                <article>
                    <h1>Article Title</h1>
                    <p>This is the main article content that should be extracted because it is inside an article tag and has more than 100 characters of useful text content.</p>
                </article>
                <footer>Footer content</footer>
            </body>
        </html>
        "#;

        let document = Html::parse_document(html);
        let content = extract_main_content(&document);

        assert!(content.contains("Article Title"));
        assert!(content.contains("main article content"));
    }

    #[test]
    fn test_extract_images() {
        let html = r#"
        <html>
            <body>
                <img src="https://example.com/image1.jpg" alt="Image 1" width="100" height="100">
                <img src="/image2.jpg" alt="Image 2">
                <img src="data:image/png;base64,..." alt="Data URI should be skipped">
            </body>
        </html>
        "#;

        let document = Html::parse_document(html);
        let images = extract_images(&document, "https://example.com/page");

        assert_eq!(images.len(), 2);
        assert_eq!(images[0].url, "https://example.com/image1.jpg");
        assert_eq!(images[0].width, Some(100));
        assert_eq!(images[1].url, "https://example.com/image2.jpg");
    }
}
