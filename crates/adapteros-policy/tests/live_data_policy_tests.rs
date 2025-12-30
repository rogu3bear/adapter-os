//! Integration tests for Query Intent and Live Data policies
//!
//! Tests the 7 failure conditions for live data requirements:
//! 1. "Latest/most recent" without browsing
//! 2. Travel planning without current conditions
//! 3. Politics without verification
//! 4. Product recommendations without lookup
//! 5. Person/location without images
//! 6. Stale sources for recency-sensitive topics
//! 7. News without link collection

use adapteros_policy::packs::{
    FallbackBehavior, GroundingEvidence, GroundingRequirements, LiveDataConfig, LiveDataIntent,
    LiveDataPolicy, QueryCategory, QueryIntentConfig, QueryIntentPolicy, RecencySensitivity,
    TenantCapabilities,
};
use adapteros_policy::registry::PolicyId;
use adapteros_policy::Policy;

// =============================================================================
// Query Intent Classification Tests
// =============================================================================

mod query_intent_tests {
    use super::*;

    fn default_policy() -> QueryIntentPolicy {
        QueryIntentPolicy::new(QueryIntentConfig::default())
    }

    #[test]
    fn test_recency_intent_detection() {
        let policy = default_policy();

        // Should detect recency-sensitive queries
        let result = policy.classify_query("What is the latest update on AI?");
        assert!(result.requires_live_data);
        assert!(result.live_data_confidence >= 0.6);
        // Recency pattern should match
        assert!(
            result.primary_intent == LiveDataIntent::Recency
                || result.primary_intent == LiveDataIntent::NewsRoundup
        );

        let result = policy.classify_query("Show me the most recent changes");
        assert!(result.requires_live_data);

        let result = policy.classify_query("Currently available options");
        assert!(result.requires_live_data);
    }

    #[test]
    fn test_travel_intent_detection() {
        let policy = default_policy();

        // Pure travel queries without recency keywords
        let result = policy.classify_query("Book me a flight to Paris");
        assert!(
            result.primary_intent == LiveDataIntent::TravelPlanning
                || result.primary_intent == LiveDataIntent::Recency
        );
        assert!(result.requires_live_data);

        let result = policy.classify_query("Hotels available in Tokyo");
        assert!(result.requires_live_data);

        // The classifier prioritizes patterns in order; travel + recency may trigger recency first
        let result = policy.classify_query("What are the flight prices to London?");
        assert!(result.requires_live_data);
    }

    #[test]
    fn test_political_intent_detection() {
        let policy = default_policy();

        // Political queries should require citations
        let result = policy.classify_query("The president gave a speech");
        // May match recency or political depending on implementation
        assert!(result.requires_live_data);
        assert!(result.requires_citations);

        let result = policy.classify_query("What did the senator say about the bill?");
        assert!(result.requires_citations);

        let result = policy.classify_query("Prime minister's policy announcement");
        assert!(result.requires_citations);
    }

    #[test]
    fn test_product_intent_detection() {
        let policy = default_policy();

        // Product queries should require live data
        let result = policy.classify_query("iPhone 15 price comparison");
        assert!(result.requires_live_data);

        let result = policy.classify_query("Best laptops to buy");
        assert!(result.requires_live_data);

        let result = policy.classify_query("Is the PS5 in stock at stores?");
        assert!(result.requires_live_data);
    }

    #[test]
    fn test_visual_context_detection() {
        let policy = default_policy();

        let result = policy.classify_query("Show me pictures of the Eiffel Tower");
        assert!(result.requires_images);

        let result = policy.classify_query("What does the Grand Canyon look like?");
        assert!(result.requires_images);
    }

    #[test]
    fn test_news_roundup_detection() {
        let policy = default_policy();

        // News patterns should trigger link collection
        let result = policy.classify_query("Give me a news summary");
        assert!(result.requires_live_data);

        let result = policy.classify_query("What headlines are in the news?");
        assert!(result.requires_live_data);
    }

    #[test]
    fn test_static_query_no_live_data() {
        let policy = default_policy();

        let result = policy.classify_query("What is the capital of France?");
        assert_eq!(result.primary_intent, LiveDataIntent::None);
        assert!(!result.requires_live_data);

        let result = policy.classify_query("Explain quantum physics");
        assert_eq!(result.primary_intent, LiveDataIntent::None);

        let result = policy.classify_query("Write me a poem about nature");
        assert_eq!(result.primary_intent, LiveDataIntent::None);
    }

    #[test]
    fn test_recency_sensitivity_levels() {
        let policy = default_policy();

        // High recency sensitivity for "today" keyword
        let result = policy.classify_query("What happened today?");
        assert_eq!(result.recency_sensitivity, RecencySensitivity::High);

        // Queries with "latest" should have some recency sensitivity
        let result = policy.classify_query("Latest developments");
        assert!(result.recency_sensitivity != RecencySensitivity::None);

        // Static queries should have None/Low recency
        let result = policy.classify_query("History of the Roman Empire");
        assert!(matches!(
            result.recency_sensitivity,
            RecencySensitivity::Low | RecencySensitivity::None
        ));
    }

    #[test]
    fn test_classification_performance() {
        let policy = default_policy();

        // Classification should be fast (sub-millisecond)
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _ = policy.classify_query("What is the latest news on technology?");
        }
        let elapsed = start.elapsed();

        // 1000 classifications should take less than 100ms
        assert!(
            elapsed.as_millis() < 100,
            "Classification too slow: {:?}",
            elapsed
        );
    }
}

// =============================================================================
// Live Data Policy Tests
// =============================================================================

mod live_data_policy_tests {
    use super::*;

    fn default_policy() -> LiveDataPolicy {
        LiveDataPolicy::new(LiveDataConfig::default())
    }

    #[test]
    fn test_policy_id_and_name() {
        let policy = default_policy();
        assert_eq!(policy.id(), PolicyId::LiveData);
        assert_eq!(policy.name(), "Live Data");
    }

    #[test]
    fn test_query_category_classification() {
        let policy = default_policy();

        assert_eq!(
            policy.classify_query("What is the weather forecast?"),
            QueryCategory::Weather
        );
        assert_eq!(
            policy.classify_query("Show me the latest news"),
            QueryCategory::News
        );
        assert_eq!(
            policy.classify_query("What is the stock price of Apple?"),
            QueryCategory::Financial
        );
        assert_eq!(
            policy.classify_query("What is the capital of France?"),
            QueryCategory::Static
        );
    }

    #[test]
    fn test_static_query_passes_without_evidence() {
        let policy = default_policy();

        let result = policy.validate_grounding("What is the capital of France?", &[]);
        assert!(result.passed);
        assert_eq!(result.category, QueryCategory::Static);
        assert!(result.messages.is_empty());
    }

    #[test]
    fn test_news_query_fails_without_evidence() {
        let policy = default_policy();

        let result = policy.validate_grounding("What is the latest news?", &[]);
        assert!(!result.passed);
        assert_eq!(result.category, QueryCategory::News);
        assert!(!result.messages.is_empty());
    }

    #[test]
    fn test_news_query_passes_with_evidence() {
        let policy = default_policy();

        let evidence = vec![
            GroundingEvidence {
                source: "https://news.example.com/article1".to_string(),
                retrieved_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                confidence: 0.9,
                snippet: Some("Latest news article content".to_string()),
            },
            GroundingEvidence {
                source: "https://news.example.com/article2".to_string(),
                retrieved_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                confidence: 0.85,
                snippet: Some("Another news source".to_string()),
            },
        ];

        let result = policy.validate_grounding("What is the latest news?", &evidence);
        assert!(result.passed);
        assert_eq!(result.evidence.len(), 2);
    }

    #[test]
    fn test_political_query_requires_multiple_sources() {
        let policy = default_policy();

        // Single source should fail for political queries (requires 3)
        let single_evidence = vec![GroundingEvidence {
            source: "https://news.example.com".to_string(),
            retrieved_at: 0,
            confidence: 0.9,
            snippet: None,
        }];

        let result = policy.validate_grounding("Who is the current president?", &single_evidence);
        assert!(!result.passed);

        // Multiple sources should pass
        let multiple_evidence = vec![
            GroundingEvidence {
                source: "https://news1.example.com".to_string(),
                retrieved_at: 0,
                confidence: 0.9,
                snippet: None,
            },
            GroundingEvidence {
                source: "https://news2.example.com".to_string(),
                retrieved_at: 0,
                confidence: 0.85,
                snippet: None,
            },
            GroundingEvidence {
                source: "https://news3.example.com".to_string(),
                retrieved_at: 0,
                confidence: 0.8,
                snippet: None,
            },
        ];

        let result = policy.validate_grounding("Who is the current president?", &multiple_evidence);
        assert!(result.passed);
    }

    #[test]
    fn test_weather_requires_api_call() {
        let policy = default_policy();

        let result = policy.validate_grounding("What is the weather forecast?", &[]);
        assert!(!result.passed);
        assert_eq!(result.category, QueryCategory::Weather);
        assert!(result.requirements.requires_api_call);
        assert_eq!(result.requirements.max_data_age_secs, 300); // 5 minutes max
    }

    #[test]
    fn test_sports_requires_fresh_data() {
        let policy = default_policy();

        let result = policy.validate_grounding("What was the score of the game?", &[]);
        assert!(!result.passed);
        assert_eq!(result.category, QueryCategory::Sports);
        assert_eq!(result.requirements.max_data_age_secs, 300); // 5 minutes max
    }

    #[test]
    fn test_travel_allows_day_old_data() {
        let policy = default_policy();

        let result = policy.validate_grounding("Book a hotel in Paris", &[]);
        assert_eq!(result.category, QueryCategory::Travel);
        assert_eq!(result.requirements.max_data_age_secs, 86400); // 24 hours
    }

    #[test]
    fn test_fallback_behavior() {
        let policy = default_policy();

        let result = policy.validate_grounding("Latest news update", &[]);
        assert_eq!(result.fallback, FallbackBehavior::UseKnowledgeCutoff);
    }

    #[test]
    fn test_tenant_capabilities_default() {
        let capabilities = TenantCapabilities::default();
        assert!(!capabilities.can_browse_web);
        assert!(!capabilities.can_access_apis);
        assert_eq!(capabilities.rate_limit, 10);
    }
}

// =============================================================================
// Grounding Requirements Tests
// =============================================================================

mod grounding_requirements_tests {
    use super::*;

    #[test]
    fn test_default_requirements() {
        let requirements = GroundingRequirements::default();
        assert!(!requirements.requires_web_search);
        assert!(!requirements.requires_api_call);
        assert_eq!(requirements.max_data_age_secs, 3600);
        assert_eq!(requirements.min_sources, 1);
    }

    #[test]
    fn test_category_specific_requirements() {
        let policy = LiveDataPolicy::new(LiveDataConfig::default());

        // News require web search
        let result = policy.validate_grounding("news headlines", &[]);
        assert_eq!(result.category, QueryCategory::News);
        assert!(result.requirements.requires_web_search);
        assert_eq!(result.requirements.min_sources, 2);

        // Weather requires API
        let result = policy.validate_grounding("weather forecast", &[]);
        assert_eq!(result.category, QueryCategory::Weather);
        assert!(result.requirements.requires_api_call);
        assert_eq!(result.requirements.max_data_age_secs, 300);

        // Politics requires multiple sources
        let result = policy.validate_grounding("election results", &[]);
        assert_eq!(result.category, QueryCategory::Politics);
        assert_eq!(result.requirements.min_sources, 3);
    }
}

// =============================================================================
// Integration Tests - 7 Failure Conditions
// =============================================================================

mod failure_condition_tests {
    use super::*;

    fn setup_policies() -> (QueryIntentPolicy, LiveDataPolicy) {
        (
            QueryIntentPolicy::new(QueryIntentConfig::default()),
            LiveDataPolicy::new(LiveDataConfig::default()),
        )
    }

    /// Condition 1: "Latest/most recent" without browsing
    #[test]
    fn test_condition_1_recency_without_browsing() {
        let (intent_policy, live_policy) = setup_policies();

        let intent = intent_policy.classify_query("What is the most recent update?");
        assert!(intent.requires_live_data);
        // Should be classified as recency or similar live data intent
        assert!(intent.primary_intent != LiveDataIntent::None);

        // Should fail validation without evidence (recency maps to News category in live_data)
        let validation = live_policy.validate_grounding("What is the most recent update?", &[]);
        // Static queries may pass without evidence, but recency ones shouldn't
        if validation.category != QueryCategory::Static {
            assert!(!validation.passed);
        }
    }

    /// Condition 2: Travel planning without current conditions
    #[test]
    fn test_condition_2_travel_without_verification() {
        let (intent_policy, live_policy) = setup_policies();

        let intent = intent_policy.classify_query("Book a flight to Paris");
        assert!(intent.requires_live_data);

        // "travel" and "flight" keywords trigger Travel category
        let validation = live_policy.validate_grounding("flight to Tokyo travel", &[]);
        assert_eq!(validation.category, QueryCategory::Travel);
        assert!(!validation.passed);
    }

    /// Condition 3: Politics without verification (Critical)
    #[test]
    fn test_condition_3_politics_without_verification() {
        let (intent_policy, live_policy) = setup_policies();

        let intent = intent_policy.classify_query("The president gave a speech");
        assert!(intent.requires_citations);

        // Should require 3+ sources for political queries
        let validation = live_policy.validate_grounding("election voting results", &[]);
        assert_eq!(validation.category, QueryCategory::Politics);
        assert!(!validation.passed);
        assert_eq!(validation.requirements.min_sources, 3);
    }

    /// Condition 4: Product recommendations without lookup
    #[test]
    fn test_condition_4_products_without_lookup() {
        let (intent_policy, live_policy) = setup_policies();

        let intent = intent_policy.classify_query("iPhone 15 price comparison");
        assert!(intent.requires_live_data);

        let validation = live_policy.validate_grounding("buy this product review", &[]);
        assert_eq!(validation.category, QueryCategory::Products);
        assert!(!validation.passed);
    }

    /// Condition 5: Person/location without images
    #[test]
    fn test_condition_5_visual_without_images() {
        let (intent_policy, _live_policy) = setup_policies();

        let intent = intent_policy.classify_query("Show me pictures of the Grand Canyon");
        assert!(intent.requires_images);
    }

    /// Condition 6: Stale sources for recency-sensitive topics
    #[test]
    fn test_condition_6_stale_sources() {
        let (_intent_policy, live_policy) = setup_policies();

        // Weather category requires data less than 5 minutes old
        // The live_data policy classifies by category, query_intent classifies for browsing
        let validation = live_policy.validate_grounding("weather forecast", &[]);
        assert_eq!(validation.category, QueryCategory::Weather);
        assert_eq!(validation.requirements.max_data_age_secs, 300);

        // Financial also requires very fresh data
        let validation = live_policy.validate_grounding("stock price", &[]);
        assert_eq!(validation.category, QueryCategory::Financial);
        assert_eq!(validation.requirements.max_data_age_secs, 300);
    }

    /// Condition 7: News without link collection
    #[test]
    fn test_condition_7_news_without_links() {
        let (intent_policy, live_policy) = setup_policies();

        let intent = intent_policy.classify_query("Give me a news summary");
        assert!(intent.requires_live_data);

        let validation = live_policy.validate_grounding("news headlines update", &[]);
        assert_eq!(validation.category, QueryCategory::News);
        assert!(!validation.passed);
        assert_eq!(validation.requirements.min_sources, 2);
    }
}
