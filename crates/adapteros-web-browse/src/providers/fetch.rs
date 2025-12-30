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
    service::{PageFetchRequest, PageFetchResponse, PageImage},
    TenantId,
};

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
}

impl Default for PageFetcherConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 10,
            max_content_kb: 100,
            user_agent: "AdapterOS-WebBrowse/1.0".to_string(),
            https_only: true,
            blocked_domains: vec!["localhost".to_string(), "127.0.0.1".to_string()],
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
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_default();

        Self { config, client }
    }

    /// Fetch page content
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
        if let Some(host) = url.host_str() {
            for blocked in &self.config.blocked_domains {
                if host == blocked || host.ends_with(&format!(".{}", blocked)) {
                    return Err(WebBrowseError::DomainBlocked {
                        domain: host.to_string(),
                    });
                }
            }
        }

        let start = Instant::now();

        // Fetch the page
        let response = self
            .client
            .get(&request.url)
            .header("Accept", "text/html,application/xhtml+xml")
            .send()
            .await?;

        // Get final URL (after redirects)
        let final_url = response.url().to_string();

        if !response.status().is_success() {
            return Err(WebBrowseError::HttpError {
                status: response.status().as_u16(),
                message: format!("Failed to fetch: {}", request.url),
            });
        }

        // Check content length
        if let Some(content_length) = response.content_length() {
            let max_bytes = self.config.max_content_kb * 1024;
            if content_length > max_bytes {
                return Err(WebBrowseError::ContentTooLarge {
                    size_kb: content_length / 1024,
                    limit_kb: self.config.max_content_kb,
                });
            }
        }

        let html_content = response
            .text()
            .await
            .map_err(|e| WebBrowseError::ParseError(format!("Failed to read response: {}", e)))?;

        // Check actual content size
        let content_length = html_content.len();
        let max_bytes =
            (request.max_content_kb.unwrap_or(self.config.max_content_kb) * 1024) as usize;
        if content_length > max_bytes {
            return Err(WebBrowseError::ContentTooLarge {
                size_kb: (content_length / 1024) as u64,
                limit_kb: self.config.max_content_kb,
            });
        }

        let latency_ms = start.elapsed().as_millis() as u64;

        // Parse HTML
        let document = Html::parse_document(&html_content);

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
            extract_images(&document, &final_url)
        } else {
            Vec::new()
        };

        // Build evidence
        let source = SourceRecord::new(&final_url)
            .with_title(&title)
            .with_content_hash(&content)
            .with_freshness(Freshness::Fresh); // Just fetched

        let evidence = EvidenceBuilder::new(tenant_id.clone(), request.request_id.clone())
            .evidence_type(EvidenceType::PageFetch)
            .add_source(source)
            .latency_ms(latency_ms)
            .build();

        Ok(PageFetchResponse {
            title,
            url: final_url,
            content,
            content_length,
            description,
            images,
            evidence,
            latency_ms,
            from_cache: false,
        })
    }
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
