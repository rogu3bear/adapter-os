//! Evidence generation for web browse results
//!
//! Creates evidence envelopes that link AI responses to their source data,
//! enabling verification and audit trails.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{RequestId, TenantId};

/// Evidence from web browsing operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseEvidence {
    /// Unique evidence ID
    pub evidence_id: String,

    /// Tenant ID
    pub tenant_id: TenantId,

    /// Request ID for correlation
    pub request_id: RequestId,

    /// Evidence type
    pub evidence_type: EvidenceType,

    /// Sources accessed
    pub sources: Vec<SourceRecord>,

    /// Timestamp of evidence creation
    pub created_at: DateTime<Utc>,

    /// Hash of the evidence content
    pub content_hash: String,

    /// Total latency in milliseconds
    pub total_latency_ms: u64,

    /// Whether any results were from cache
    pub used_cache: bool,
}

impl BrowseEvidence {
    /// Create new evidence for a request
    pub fn new(tenant_id: TenantId, request_id: RequestId) -> Self {
        let evidence_id = format!(
            "browse_{}_{}_{}",
            tenant_id,
            request_id,
            Utc::now().timestamp_millis()
        );

        Self {
            evidence_id,
            tenant_id,
            request_id,
            evidence_type: EvidenceType::WebSearch,
            sources: Vec::new(),
            created_at: Utc::now(),
            content_hash: String::new(),
            total_latency_ms: 0,
            used_cache: false,
        }
    }

    /// Add a source to the evidence
    pub fn add_source(&mut self, source: SourceRecord) {
        self.sources.push(source);
        self.update_hash();
    }

    /// Finalize the evidence with computed hash
    pub fn finalize(&mut self) {
        self.update_hash();
    }

    /// Update the content hash
    fn update_hash(&mut self) {
        let mut hasher = Sha256::new();

        hasher.update(self.evidence_id.as_bytes());
        hasher.update(self.tenant_id.as_bytes());
        hasher.update(self.request_id.as_bytes());

        for source in &self.sources {
            hasher.update(source.url.as_bytes());
            if let Some(content_hash) = &source.content_hash {
                hasher.update(content_hash.as_bytes());
            }
        }

        self.content_hash = hex::encode(hasher.finalize());
    }
}

/// Type of evidence
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceType {
    /// Web search results
    WebSearch,

    /// Fetched page content
    PageFetch,

    /// Image search results
    ImageSearch,

    /// Combined evidence from multiple operations
    Combined,
}

/// Record of a source accessed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceRecord {
    /// Source URL
    pub url: String,

    /// Source domain
    pub domain: String,

    /// Title of the source
    pub title: Option<String>,

    /// Content snippet used
    pub snippet: Option<String>,

    /// Hash of the full content (if fetched)
    pub content_hash: Option<String>,

    /// Timestamp when accessed
    pub accessed_at: DateTime<Utc>,

    /// Published date (if available)
    pub published_date: Option<String>,

    /// Whether this was from cache
    pub from_cache: bool,

    /// Freshness indicator
    pub freshness: Freshness,
}

impl SourceRecord {
    /// Create a new source record
    pub fn new(url: &str) -> Self {
        let domain = extract_domain(url).unwrap_or_default();

        Self {
            url: url.to_string(),
            domain,
            title: None,
            snippet: None,
            content_hash: None,
            accessed_at: Utc::now(),
            published_date: None,
            from_cache: false,
            freshness: Freshness::Unknown,
        }
    }

    /// Set the title
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = Some(title.to_string());
        self
    }

    /// Set the snippet
    pub fn with_snippet(mut self, snippet: &str) -> Self {
        self.snippet = Some(snippet.to_string());
        self
    }

    /// Set the content hash
    pub fn with_content_hash(mut self, content: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        self.content_hash = Some(hex::encode(hasher.finalize()));
        self
    }

    /// Mark as from cache
    pub fn cached(mut self) -> Self {
        self.from_cache = true;
        self
    }

    /// Set freshness
    pub fn with_freshness(mut self, freshness: Freshness) -> Self {
        self.freshness = freshness;
        self
    }
}

/// Freshness indicator for a source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Freshness {
    /// Less than 24 hours old
    Fresh,

    /// 1-7 days old
    Recent,

    /// 7-30 days old
    Moderate,

    /// More than 30 days old
    Stale,

    /// Unknown age
    Unknown,
}

impl Freshness {
    /// Determine freshness from age in days
    pub fn from_age_days(days: u32) -> Self {
        match days {
            0 => Freshness::Fresh,
            1..=7 => Freshness::Recent,
            8..=30 => Freshness::Moderate,
            _ => Freshness::Stale,
        }
    }
}

/// Builder for creating evidence
pub struct EvidenceBuilder {
    evidence: BrowseEvidence,
}

impl EvidenceBuilder {
    /// Create a new evidence builder
    pub fn new(tenant_id: TenantId, request_id: RequestId) -> Self {
        Self {
            evidence: BrowseEvidence::new(tenant_id, request_id),
        }
    }

    /// Set evidence type
    pub fn evidence_type(mut self, evidence_type: EvidenceType) -> Self {
        self.evidence.evidence_type = evidence_type;
        self
    }

    /// Add a source
    pub fn add_source(mut self, source: SourceRecord) -> Self {
        self.evidence.sources.push(source);
        self
    }

    /// Set total latency
    pub fn latency_ms(mut self, latency: u64) -> Self {
        self.evidence.total_latency_ms = latency;
        self
    }

    /// Mark as using cache
    pub fn used_cache(mut self, used: bool) -> Self {
        self.evidence.used_cache = used;
        self
    }

    /// Build the evidence
    pub fn build(mut self) -> BrowseEvidence {
        self.evidence.finalize();
        self.evidence
    }
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
    fn test_evidence_creation() {
        let evidence = BrowseEvidence::new("tenant1".to_string(), "req123".to_string());

        assert!(evidence.evidence_id.contains("tenant1"));
        assert_eq!(evidence.tenant_id, "tenant1");
        assert_eq!(evidence.request_id, "req123");
    }

    #[test]
    fn test_source_record() {
        let source = SourceRecord::new("https://example.com/article")
            .with_title("Test Article")
            .with_snippet("This is a test snippet")
            .with_freshness(Freshness::Fresh);

        assert_eq!(source.domain, "example.com");
        assert_eq!(source.title.unwrap(), "Test Article");
        assert_eq!(source.freshness, Freshness::Fresh);
    }

    #[test]
    fn test_evidence_builder() {
        let evidence = EvidenceBuilder::new("tenant1".to_string(), "req123".to_string())
            .evidence_type(EvidenceType::WebSearch)
            .add_source(SourceRecord::new("https://example.com").with_title("Example"))
            .latency_ms(150)
            .used_cache(false)
            .build();

        assert_eq!(evidence.sources.len(), 1);
        assert_eq!(evidence.total_latency_ms, 150);
        assert!(!evidence.content_hash.is_empty());
    }

    #[test]
    fn test_freshness_from_age() {
        assert_eq!(Freshness::from_age_days(0), Freshness::Fresh);
        assert_eq!(Freshness::from_age_days(3), Freshness::Recent);
        assert_eq!(Freshness::from_age_days(15), Freshness::Moderate);
        assert_eq!(Freshness::from_age_days(60), Freshness::Stale);
    }
}
