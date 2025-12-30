//! Live Data Integration Module
//!
//! Provides query intent classification and web browse integration for
//! the inference pipeline. This module bridges the policy layer with
//! the web browse service.
//!
//! ## Integration Points
//!
//! 1. **Pre-Routing (Stage 3.5)**: Classify query intent and determine
//!    if web browsing is needed before inference.
//!
//! 2. **Post-Inference (Stage 9)**: Validate that responses requiring
//!    live data are properly grounded with citations.
//!
//! ## Usage
//!
//! ```ignore
//! let classifier = LiveDataClassifier::new();
//! let intent_result = classifier.classify(&prompt);
//!
//! if intent_result.requires_live_data {
//!     // Check tenant has web browsing enabled
//!     // Perform web search
//!     // Merge results into RAG context
//! }
//! ```

use adapteros_policy::packs::{
    FallbackBehavior, GroundingEvidence, LiveDataConfig, LiveDataPolicy, QueryCategory,
    QueryIntentConfig, QueryIntentPolicy, QueryIntentResult, RecencySensitivity,
    ValidationResult as LiveDataValidationResult,
};

/// Live data classifier for inference requests
pub struct LiveDataClassifier {
    query_intent_policy: QueryIntentPolicy,
    live_data_policy: LiveDataPolicy,
}

impl Default for LiveDataClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl LiveDataClassifier {
    /// Create a new live data classifier with default configuration
    pub fn new() -> Self {
        Self {
            query_intent_policy: QueryIntentPolicy::new(QueryIntentConfig::default()),
            live_data_policy: LiveDataPolicy::new(LiveDataConfig::default()),
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        query_intent_config: QueryIntentConfig,
        live_data_config: LiveDataConfig,
    ) -> Self {
        Self {
            query_intent_policy: QueryIntentPolicy::new(query_intent_config),
            live_data_policy: LiveDataPolicy::new(live_data_config),
        }
    }

    /// Classify a query's live data requirements
    ///
    /// Returns intent classification with details about what type of
    /// live data is needed and what grounding requirements apply.
    pub fn classify(&self, query: &str) -> LiveDataClassification {
        let intent_result = self.query_intent_policy.classify_query(query);
        let category = self.live_data_policy.classify_query(query);

        let requires_web_browse = requires_web_browse(&intent_result, &category);
        let requires_images = intent_result.requires_images;
        let requires_citations = intent_result.requires_citations;
        let requires_link_collection = intent_result.requires_link_collection;

        LiveDataClassification {
            query: query.to_string(),
            intent: intent_result,
            category,
            requires_web_browse,
            requires_images,
            requires_citations,
            requires_link_collection,
        }
    }

    /// Validate that a response is properly grounded for the query
    ///
    /// Call this after inference to check if the response meets
    /// grounding requirements for queries needing live data.
    pub fn validate_grounding(
        &self,
        query: &str,
        evidence: &[GroundingEvidence],
    ) -> LiveDataValidationResult {
        self.live_data_policy.validate_grounding(query, evidence)
    }

    /// Get the suggested fallback behavior when grounding fails
    pub fn fallback_behavior(&self, category: QueryCategory) -> FallbackBehavior {
        match category {
            QueryCategory::Static => FallbackBehavior::UseKnowledgeCutoff,
            QueryCategory::Politics => FallbackBehavior::Decline, // Critical: must have grounding
            _ => FallbackBehavior::UseCachedWithDisclaimer,
        }
    }
}

/// Combined classification result for live data requirements
#[derive(Debug, Clone)]
pub struct LiveDataClassification {
    /// Original query text
    pub query: String,

    /// Query intent classification from policy
    pub intent: QueryIntentResult,

    /// Query category for grounding requirements
    pub category: QueryCategory,

    /// Whether web browsing is recommended
    pub requires_web_browse: bool,

    /// Whether image search is beneficial
    pub requires_images: bool,

    /// Whether citations are required
    pub requires_citations: bool,

    /// Whether link collection is required (news queries)
    pub requires_link_collection: bool,
}

impl LiveDataClassification {
    /// Check if this classification indicates any live data requirements
    pub fn needs_live_data(&self) -> bool {
        self.requires_web_browse || self.requires_images || self.requires_citations
    }

    /// Get the recency sensitivity level
    pub fn recency_sensitivity(&self) -> RecencySensitivity {
        self.intent.recency_sensitivity
    }

    /// Get a suggested search query (may be refined from original)
    pub fn suggested_search_query(&self) -> Option<&str> {
        self.intent.suggested_query.as_deref()
    }

    /// Get the primary intent category
    pub fn primary_intent(&self) -> adapteros_policy::packs::LiveDataIntent {
        self.intent.primary_intent
    }
}

/// Determine if web browsing should be triggered
fn requires_web_browse(intent: &QueryIntentResult, category: &QueryCategory) -> bool {
    if !intent.requires_live_data {
        return false;
    }

    // Categories that strongly benefit from web browsing
    matches!(
        category,
        QueryCategory::News
            | QueryCategory::Recency
            | QueryCategory::Politics
            | QueryCategory::Weather
            | QueryCategory::Sports
            | QueryCategory::Financial
            | QueryCategory::Travel
            | QueryCategory::Products
    )
}

/// Web browse context to be passed through the inference pipeline
///
/// This struct captures web browse results that can be merged with RAG context.
#[derive(Debug, Clone, Default)]
pub struct WebBrowseContext {
    /// Web search results (if any)
    pub search_results: Vec<WebSearchResult>,

    /// Whether web browsing was performed
    pub browsing_performed: bool,

    /// Whether results were from cache
    pub from_cache: bool,

    /// Latency of web browse operations in milliseconds
    pub latency_ms: u64,

    /// Evidence for grounding validation
    pub evidence: Vec<GroundingEvidence>,
}

/// Simplified web search result for context injection
#[derive(Debug, Clone)]
pub struct WebSearchResult {
    /// Result title
    pub title: String,
    /// Source URL
    pub url: String,
    /// Content snippet
    pub snippet: String,
    /// Source domain
    pub domain: String,
    /// Published date (if available)
    pub published_date: Option<String>,
}

impl WebBrowseContext {
    /// Create an empty context (no browsing performed)
    pub fn empty() -> Self {
        Self::default()
    }

    /// Convert search results to RAG-compatible context string
    pub fn to_context_string(&self) -> String {
        if self.search_results.is_empty() {
            return String::new();
        }

        let mut context = String::from("Web Search Results:\n\n");
        for (i, result) in self.search_results.iter().enumerate() {
            context.push_str(&format!(
                "[{}] {}\n    Source: {} ({})\n    {}\n\n",
                i + 1,
                result.title,
                result.domain,
                result.published_date.as_deref().unwrap_or("unknown date"),
                result.snippet
            ));
        }
        context
    }

    /// Convert to grounding evidence for validation
    pub fn to_grounding_evidence(&self) -> Vec<GroundingEvidence> {
        self.search_results
            .iter()
            .map(|r| GroundingEvidence {
                source: r.url.clone(),
                retrieved_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                confidence: 0.8, // Default confidence for web results
                snippet: Some(r.snippet.clone()),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classifier_creation() {
        let classifier = LiveDataClassifier::new();
        let result = classifier.classify("What is the capital of France?");

        // Static query shouldn't require live data
        assert!(!result.needs_live_data());
        assert_eq!(result.category, QueryCategory::Static);
    }

    #[test]
    fn test_news_query_classification() {
        let classifier = LiveDataClassifier::new();
        let result = classifier.classify("What is the latest news about AI?");

        // News query should require live data
        assert!(result.needs_live_data());
        assert_eq!(result.category, QueryCategory::News);
        assert!(result.requires_web_browse);
    }

    #[test]
    fn test_weather_query_classification() {
        let classifier = LiveDataClassifier::new();
        let result = classifier.classify("What is the weather forecast for tomorrow?");

        assert_eq!(result.category, QueryCategory::Weather);
        assert!(result.requires_web_browse);
    }

    #[test]
    fn test_web_browse_context_to_string() {
        let mut context = WebBrowseContext::empty();
        context.browsing_performed = true;
        context.search_results = vec![WebSearchResult {
            title: "Test Article".to_string(),
            url: "https://example.com/article".to_string(),
            snippet: "This is a test snippet.".to_string(),
            domain: "example.com".to_string(),
            published_date: Some("2025-01-01".to_string()),
        }];

        let context_str = context.to_context_string();
        assert!(context_str.contains("Test Article"));
        assert!(context_str.contains("example.com"));
    }
}
