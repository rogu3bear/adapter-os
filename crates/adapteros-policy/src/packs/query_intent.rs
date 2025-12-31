//! Query Intent Classification Policy Pack
//!
//! Detects queries that require live/fresh data to provide accurate responses.
//! Classifies queries into 7 intent types that indicate need for web browsing,
//! current data verification, or source freshness requirements.
//!
//! Intent Types:
//! - Recency: "latest", "most recent", "today", "current" keywords
//! - TravelPlanning: destinations, hours, closures, safety, prices
//! - PoliticalQuery: officeholders, elections, political events
//! - ProductRecommendation: prices, availability, specs for purchases
//! - VisualContext: queries where images would help (person/location/history)
//! - TimeSensitiveSources: explicit request for recent sources
//! - NewsRoundup: current events requiring curated news links

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::Result;
use once_cell::sync::Lazy;
use regex::RegexSet;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Query intent types that indicate need for live/fresh data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveDataIntent {
    /// "latest", "most recent", "today", "current" keywords
    Recency,
    /// Destinations, hours, closures, safety, prices for travel
    TravelPlanning,
    /// Officeholders, elections, political events
    PoliticalQuery,
    /// Prices, availability, specs for purchases
    ProductRecommendation,
    /// Queries where images would help (person/location/history)
    VisualContext,
    /// Explicit request for recent sources
    TimeSensitiveSources,
    /// Current events requiring curated news links
    NewsRoundup,
    /// No live data requirement detected
    None,
}

impl LiveDataIntent {
    /// Get the name of the intent
    pub fn name(&self) -> &'static str {
        match self {
            LiveDataIntent::Recency => "recency",
            LiveDataIntent::TravelPlanning => "travel_planning",
            LiveDataIntent::PoliticalQuery => "political_query",
            LiveDataIntent::ProductRecommendation => "product_recommendation",
            LiveDataIntent::VisualContext => "visual_context",
            LiveDataIntent::TimeSensitiveSources => "time_sensitive_sources",
            LiveDataIntent::NewsRoundup => "news_roundup",
            LiveDataIntent::None => "none",
        }
    }

    /// Get the description of the intent
    pub fn description(&self) -> &'static str {
        match self {
            LiveDataIntent::Recency => "Query about latest, most recent, or current information",
            LiveDataIntent::TravelPlanning => "Travel planning requiring operational verification",
            LiveDataIntent::PoliticalQuery => "Political information requiring verification",
            LiveDataIntent::ProductRecommendation => {
                "Product recommendation requiring price/availability lookup"
            }
            LiveDataIntent::VisualContext => "Query where images would help understanding",
            LiveDataIntent::TimeSensitiveSources => {
                "Explicit request for recent or time-sensitive sources"
            }
            LiveDataIntent::NewsRoundup => "Current events requiring curated news links",
            LiveDataIntent::None => "No live data requirement detected",
        }
    }
}

/// How sensitive the query is to stale information
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecencySensitivity {
    /// Information changes daily (stock prices, weather, breaking news)
    High,
    /// Information changes weekly/monthly (political positions, product prices)
    Medium,
    /// Information relatively stable but should be recent (research, documentation)
    Low,
    /// Not recency-sensitive (historical facts, stable concepts)
    None,
}

impl RecencySensitivity {
    /// Get the maximum source age in days for this sensitivity level
    pub fn max_source_age_days(&self) -> u32 {
        match self {
            RecencySensitivity::High => 1,
            RecencySensitivity::Medium => 30,
            RecencySensitivity::Low => 365,
            RecencySensitivity::None => u32::MAX,
        }
    }
}

/// Classification result for a single intent type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentClassification {
    /// The detected intent type
    pub intent: LiveDataIntent,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
    /// Matched signals that triggered this classification
    pub matched_signals: Vec<String>,
}

/// Complete query intent analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryIntentResult {
    /// All detected intents (may have multiple)
    pub intents: Vec<IntentClassification>,
    /// Primary (highest confidence) intent
    pub primary_intent: LiveDataIntent,
    /// Whether live data is recommended
    pub requires_live_data: bool,
    /// Overall confidence in live data requirement
    pub live_data_confidence: f32,
    /// Recency sensitivity level
    pub recency_sensitivity: RecencySensitivity,
    /// Suggested search query (if applicable)
    pub suggested_query: Option<String>,
    /// Whether images should be included
    pub requires_images: bool,
    /// Whether citations are required
    pub requires_citations: bool,
    /// Whether link collection is required
    pub requires_link_collection: bool,
    /// Classification latency in microseconds
    pub classification_latency_us: u64,
}

/// Configuration for query intent classification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryIntentConfig {
    /// Minimum confidence threshold for live data flag
    pub live_data_threshold: f32,
    /// Enable semantic classification fallback (future enhancement)
    pub enable_semantic_fallback: bool,
    /// Maximum classification latency budget (microseconds)
    pub max_latency_us: u64,
}

impl Default for QueryIntentConfig {
    fn default() -> Self {
        Self {
            live_data_threshold: 0.6,
            enable_semantic_fallback: false,
            max_latency_us: 500, // 0.5ms budget
        }
    }
}

/// Pre-compiled regex patterns for fast keyword matching
struct KeywordClassifier {
    recency_patterns: RegexSet,
    travel_patterns: RegexSet,
    political_patterns: RegexSet,
    product_patterns: RegexSet,
    visual_patterns: RegexSet,
    time_sensitive_patterns: RegexSet,
    news_patterns: RegexSet,
}

impl KeywordClassifier {
    fn new() -> Self {
        Self {
            // Recency patterns: "latest", "most recent", "today", "current", "now", "this week"
            recency_patterns: RegexSet::new([
                r"(?i)\b(latest|newest|most recent|current(ly)?|today('s)?|right now)\b",
                r"(?i)\b(this (week|month|year)|as of (today|now)|up to date)\b",
                r"(?i)\b(breaking|just (announced|released|happened|updated))\b",
                r"(?i)\b20(2[4-9]|[3-9]\d)\b", // Current/future year references
                r"(?i)\bwhat (is|are) (the )?(current|latest)\b",
                r"(?i)\b(tomorrow|tonight|next (week|month|day))\b", // Near-future references
                r"(?i)\b(weather|forecast)\b", // Weather is inherently time-sensitive
            ])
            .expect("Invalid recency regex patterns"),

            // Travel patterns
            travel_patterns: RegexSet::new([
                r"(?i)\b(flight(s)?|hotel(s)?|travel|trip|vacation|booking|reservation)\b",
                r"(?i)\b(airport|airline|visa|passport|itinerary)\b",
                r"(?i)\b(hours?|open(ing)?|clos(ed|ing|ure)|schedule)\b.*\b(restaurant|museum|store|shop|attraction)\b",
                r"(?i)\b(safe(ty)?|danger(ous)?|warning|advisory|travel ban)\b.*\b(travel|visit|country|region)\b",
                r"(?i)\b(price|cost|fare|rate)\b.*\b(flight|hotel|room|ticket|airfare)\b",
                r"(?i)\b(visit(ing)?|go(ing)? to|travel(ing)? to)\b.*\b(city|country|destination)\b",
            ])
            .expect("Invalid travel regex patterns"),

            // Political patterns
            political_patterns: RegexSet::new([
                r"(?i)\b(president|prime minister|senator|governor|mayor|minister|secretary)\b",
                r"(?i)\b(election|vote|poll|campaign|candidate|ballot)\b",
                r"(?i)\b(congress|parliament|senate|legislation|bill|law|policy)\b",
                r"(?i)\bwho (is|are) (the )?(current(ly)?|now)\b",
                r"(?i)\b(political|government|administration|cabinet|party)\b",
                r"(?i)\b(democrat|republican|conservative|liberal|labour|tory)\b",
            ])
            .expect("Invalid political regex patterns"),

            // Product patterns
            product_patterns: RegexSet::new([
                r"(?i)\b(buy|purchase|price|cost|deal|discount|sale|order)\b",
                r"(?i)\b(recommend|best|top|review|compare|vs|versus)\b.*\b(product|item|model|brand)\b",
                r"(?i)\b(in stock|available|out of stock|inventory|shipping)\b",
                r"(?i)\b(spec(ification)?s|feature(s)?|performance)\b.*\b(compare|vs|versus|review)\b",
                r"(?i)\b(amazon|ebay|walmart|target|best buy|newegg)\b",
                r"(?i)\b(should i (buy|get)|worth (buying|getting)|good (deal|price))\b",
            ])
            .expect("Invalid product regex patterns"),

            // Visual context patterns
            visual_patterns: RegexSet::new([
                r"(?i)\b(what does .* look like|show me|picture|image|photo)\b",
                r"(?i)\b(appearance|visual|see|watch|view)\b",
                r"(?i)\b(map|location|where is|directions? to|how to get to)\b",
                r"(?i)\b(face|portrait|person|celebrity|actor|politician)\b.*\b(look|appear)\b",
                r"(?i)\b(famous|notable|historical) (person|figure|place|building)\b",
            ])
            .expect("Invalid visual regex patterns"),

            // Time-sensitive source patterns
            time_sensitive_patterns: RegexSet::new([
                r"(?i)\b(recent|new|latest) (study|research|paper|report|article|finding)\b",
                r"(?i)\b(updated|revised|amended|new version)\b",
                r"(?i)\b(as of|since|after) (january|february|march|april|may|june|july|august|september|october|november|december|jan|feb|mar|apr|jun|jul|aug|sep|oct|nov|dec)\b",
                r"(?i)\b(source|citation|reference).*(recent|current|latest|new)\b",
                r"(?i)\b(peer[- ]?reviewed|published|released) (in |this |last )?(year|month|week)\b",
            ])
            .expect("Invalid time-sensitive regex patterns"),

            // News roundup patterns
            news_patterns: RegexSet::new([
                r"(?i)\b(news|headline|story|stories|update(s)?|coverage)\b",
                r"(?i)\b(happening|occurred|event(s)?)\b.*\b(today|recently|this week|now)\b",
                r"(?i)\b(summary|roundup|recap|digest|briefing)\b.*\b(news|events)\b",
                r"(?i)\b(current events|what('s| is)? (happening|going on))\b",
                r"(?i)\b(breaking news|latest news|top stories|headlines)\b",
            ])
            .expect("Invalid news regex patterns"),
        }
    }

    /// Classify query using keyword patterns
    /// Returns list of IntentClassification results
    fn classify(&self, query: &str) -> Vec<IntentClassification> {
        let mut results = Vec::new();

        // Check each pattern set
        let checks = [
            (&self.recency_patterns, LiveDataIntent::Recency),
            (&self.travel_patterns, LiveDataIntent::TravelPlanning),
            (&self.political_patterns, LiveDataIntent::PoliticalQuery),
            (
                &self.product_patterns,
                LiveDataIntent::ProductRecommendation,
            ),
            (&self.visual_patterns, LiveDataIntent::VisualContext),
            (
                &self.time_sensitive_patterns,
                LiveDataIntent::TimeSensitiveSources,
            ),
            (&self.news_patterns, LiveDataIntent::NewsRoundup),
        ];

        for (patterns, intent) in checks {
            let matches: Vec<usize> = patterns.matches(query).into_iter().collect();
            if !matches.is_empty() {
                // Confidence increases with more pattern matches
                let confidence = (0.6 + 0.1 * matches.len() as f32).min(0.95);
                results.push(IntentClassification {
                    intent,
                    confidence,
                    matched_signals: matches
                        .iter()
                        .map(|i| format!("{}:pattern_{}", intent.name(), i))
                        .collect(),
                });
            }
        }

        results
    }
}

static KEYWORD_CLASSIFIER: Lazy<KeywordClassifier> = Lazy::new(KeywordClassifier::new);

/// Query Intent Classification Policy
pub struct QueryIntentPolicy {
    config: QueryIntentConfig,
}

impl QueryIntentPolicy {
    /// Create a new query intent policy with the given configuration
    pub fn new(config: QueryIntentConfig) -> Self {
        Self { config }
    }

    /// Classify a query and return the result
    pub fn classify_query(&self, query: &str) -> QueryIntentResult {
        let start = Instant::now();

        // Fast path: keyword classification
        let mut intents = KEYWORD_CLASSIFIER.classify(query);

        // Determine primary intent and live data requirement
        let (primary_intent, live_data_confidence) = if intents.is_empty() {
            (LiveDataIntent::None, 0.0)
        } else {
            // Sort by confidence descending
            intents.sort_by(|a, b| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let primary = intents[0].intent;
            // Aggregate confidence: max single confidence + bonus for multiple detections
            let max_conf = intents[0].confidence;
            let bonus = (intents.len() as f32 - 1.0) * 0.05;
            let aggregated = (max_conf + bonus).min(0.99);
            (primary, aggregated)
        };

        let requires_live_data = live_data_confidence >= self.config.live_data_threshold
            && primary_intent != LiveDataIntent::None;

        // Determine recency sensitivity based on primary intent
        let recency_sensitivity = match primary_intent {
            LiveDataIntent::Recency | LiveDataIntent::NewsRoundup => RecencySensitivity::High,
            LiveDataIntent::PoliticalQuery
            | LiveDataIntent::ProductRecommendation
            | LiveDataIntent::TravelPlanning => RecencySensitivity::Medium,
            LiveDataIntent::TimeSensitiveSources => RecencySensitivity::Low,
            LiveDataIntent::VisualContext | LiveDataIntent::None => RecencySensitivity::None,
        };

        // Determine additional requirements based on intent
        let requires_images = primary_intent == LiveDataIntent::VisualContext;
        let requires_citations = requires_live_data;
        let requires_link_collection = primary_intent == LiveDataIntent::NewsRoundup;

        // Generate suggested query if needed
        let suggested_query = if requires_live_data {
            Some(query.to_string())
        } else {
            None
        };

        QueryIntentResult {
            intents,
            primary_intent,
            requires_live_data,
            live_data_confidence,
            recency_sensitivity,
            suggested_query,
            requires_images,
            requires_citations,
            requires_link_collection,
            classification_latency_us: start.elapsed().as_micros() as u64,
        }
    }
}

impl Policy for QueryIntentPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::QueryIntent
    }

    fn name(&self) -> &'static str {
        "Query Intent"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        // Query intent classification is informational, doesn't block
        // It adds metadata to the context for downstream policies
        Ok(Audit::passed(self.id()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> QueryIntentConfig {
        QueryIntentConfig::default()
    }

    #[test]
    fn test_recency_detection() {
        let policy = QueryIntentPolicy::new(default_config());

        let cases = [
            ("What is the latest news?", true, LiveDataIntent::Recency),
            (
                "Show me current stock prices",
                true,
                LiveDataIntent::Recency,
            ),
            ("What happened today?", true, LiveDataIntent::Recency),
            (
                "What are the latest updates?",
                true,
                LiveDataIntent::Recency,
            ),
            ("Who wrote Romeo and Juliet?", false, LiveDataIntent::None),
        ];

        for (query, should_require, expected_intent) in cases {
            let result = policy.classify_query(query);
            assert_eq!(
                result.requires_live_data, should_require,
                "Query '{}' should require_live_data={}",
                query, should_require
            );
            if should_require {
                assert_eq!(
                    result.primary_intent, expected_intent,
                    "Query '{}' should have intent {:?}",
                    query, expected_intent
                );
            }
        }
    }

    #[test]
    fn test_travel_detection() {
        let policy = QueryIntentPolicy::new(default_config());

        let result = policy.classify_query("What are flight prices to Paris?");
        assert!(result.requires_live_data);
        assert_eq!(result.primary_intent, LiveDataIntent::TravelPlanning);

        let result = policy.classify_query("What time does the Louvre close?");
        assert!(result.requires_live_data);
        assert_eq!(result.primary_intent, LiveDataIntent::TravelPlanning);

        let result = policy.classify_query("Is it safe to travel to Thailand?");
        assert!(result.requires_live_data);
    }

    #[test]
    fn test_political_detection() {
        let policy = QueryIntentPolicy::new(default_config());

        let result = policy.classify_query("Who is the current president of France?");
        assert!(result.requires_live_data);
        assert_eq!(result.primary_intent, LiveDataIntent::PoliticalQuery);

        let result = policy.classify_query("When is the next election?");
        assert!(result.requires_live_data);
        assert_eq!(result.primary_intent, LiveDataIntent::PoliticalQuery);
    }

    #[test]
    fn test_product_detection() {
        let policy = QueryIntentPolicy::new(default_config());

        let result = policy.classify_query("Best laptop to buy under $1000?");
        assert!(result.requires_live_data);
        assert_eq!(result.primary_intent, LiveDataIntent::ProductRecommendation);

        let result = policy.classify_query("Is the iPhone 15 in stock at Amazon?");
        assert!(result.requires_live_data);
    }

    #[test]
    fn test_visual_context_detection() {
        let policy = QueryIntentPolicy::new(default_config());

        let result = policy.classify_query("What does the Eiffel Tower look like?");
        assert!(result.requires_live_data);
        assert_eq!(result.primary_intent, LiveDataIntent::VisualContext);
        assert!(result.requires_images);

        let result = policy.classify_query("Show me pictures of the Grand Canyon");
        assert!(result.requires_images);
    }

    #[test]
    fn test_news_detection() {
        let policy = QueryIntentPolicy::new(default_config());

        let result = policy.classify_query("Give me a news roundup for this week");
        assert!(result.requires_live_data);
        assert_eq!(result.primary_intent, LiveDataIntent::NewsRoundup);
        assert!(result.requires_link_collection);

        let result = policy.classify_query("What are the latest headlines?");
        assert!(result.requires_live_data);
    }

    #[test]
    fn test_time_sensitive_sources() {
        let policy = QueryIntentPolicy::new(default_config());

        let result = policy.classify_query("Show me recent research on climate change");
        assert!(result.requires_live_data);
        assert_eq!(result.primary_intent, LiveDataIntent::TimeSensitiveSources);
    }

    #[test]
    fn test_multiple_intents() {
        let policy = QueryIntentPolicy::new(default_config());

        // Query with multiple signals
        let result = policy.classify_query(
            "What are the latest flight prices for the presidential campaign trail?",
        );
        assert!(result.requires_live_data);
        assert!(result.intents.len() >= 2);
    }

    #[test]
    fn test_no_live_data_required() {
        let policy = QueryIntentPolicy::new(default_config());

        let cases = [
            "What is the Pythagorean theorem?",
            "Explain photosynthesis",
            "How do I write a for loop in Python?",
            "What year was the Eiffel Tower built?",
        ];

        for query in cases {
            let result = policy.classify_query(query);
            assert!(
                !result.requires_live_data,
                "Query '{}' should NOT require live data",
                query
            );
        }
    }

    #[test]
    fn test_performance_budget() {
        let config = QueryIntentConfig {
            max_latency_us: 1000, // 1ms budget
            ..default_config()
        };
        let policy = QueryIntentPolicy::new(config);

        // Classification should be fast
        let result = policy.classify_query("What is the latest news about technology?");
        assert!(
            result.classification_latency_us < 1000,
            "Classification took {}us, exceeds 1ms budget",
            result.classification_latency_us
        );
    }

    #[test]
    fn test_recency_sensitivity() {
        let policy = QueryIntentPolicy::new(default_config());

        // High sensitivity for news
        let result = policy.classify_query("Breaking news today");
        assert_eq!(result.recency_sensitivity, RecencySensitivity::High);

        // Medium sensitivity for products
        let result = policy.classify_query("Best laptop to buy");
        assert_eq!(result.recency_sensitivity, RecencySensitivity::Medium);

        // None for general queries
        let result = policy.classify_query("How to bake a cake");
        assert_eq!(result.recency_sensitivity, RecencySensitivity::None);
    }
}
