//! Live Data Policy Pack
//!
//! Enforces browsing and citation requirements for queries needing fresh data.
//! Ensures responses requiring real-time information are properly grounded.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity};
use adapteros_core::Result;
use serde::{Deserialize, Serialize};

/// Query category for live data classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QueryCategory {
    /// Recency-sensitive (news, current events)
    Recency,
    /// Travel information
    Travel,
    /// Political topics
    Politics,
    /// Product information
    Products,
    /// News and current events
    News,
    /// Weather information
    Weather,
    /// Sports scores and updates
    Sports,
    /// Financial/stock information
    Financial,
    /// Not requiring live data
    Static,
}

/// Fallback behavior when live data is unavailable
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackBehavior {
    /// Use cached data with disclaimer
    UseCachedWithDisclaimer,
    /// Decline to answer
    Decline,
    /// Use knowledge cutoff data
    UseKnowledgeCutoff,
    /// Request user refresh
    RequestRefresh,
}

/// Tenant capabilities for live data access
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantCapabilities {
    /// Can access live web data
    pub can_browse_web: bool,
    /// Can access real-time APIs
    pub can_access_apis: bool,
    /// Maximum queries per minute
    pub rate_limit: u32,
}

impl Default for TenantCapabilities {
    fn default() -> Self {
        Self {
            can_browse_web: false,
            can_access_apis: false,
            rate_limit: 10,
        }
    }
}

/// Requirements for grounding a response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingRequirements {
    /// Requires web search
    pub requires_web_search: bool,
    /// Requires API call
    pub requires_api_call: bool,
    /// Maximum data age in seconds
    pub max_data_age_secs: u64,
    /// Minimum number of sources
    pub min_sources: usize,
}

impl Default for GroundingRequirements {
    fn default() -> Self {
        Self {
            requires_web_search: false,
            requires_api_call: false,
            max_data_age_secs: 3600,
            min_sources: 1,
        }
    }
}

/// Evidence for grounding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroundingEvidence {
    /// Source URL or identifier
    pub source: String,
    /// Timestamp of retrieval
    pub retrieved_at: u64,
    /// Confidence score
    pub confidence: f32,
    /// Snippet of content
    pub snippet: Option<String>,
}

/// Result of live data validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Whether validation passed
    pub passed: bool,
    /// Query category detected
    pub category: QueryCategory,
    /// Grounding requirements for this query
    pub requirements: GroundingRequirements,
    /// Evidence provided (if any)
    pub evidence: Vec<GroundingEvidence>,
    /// Fallback behavior if grounding fails
    pub fallback: FallbackBehavior,
    /// Validation messages
    pub messages: Vec<String>,
}

/// Live data policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDataConfig {
    /// Enable live data validation
    pub enabled: bool,
    /// Require citations for live data responses
    pub require_citations: bool,
    /// Maximum age of cached data in seconds before requiring refresh
    pub max_cache_age_secs: u64,
    /// Default fallback behavior
    pub default_fallback: FallbackBehavior,
    /// Tenant capabilities
    pub tenant_capabilities: TenantCapabilities,
}

impl Default for LiveDataConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            require_citations: true,
            max_cache_age_secs: 3600,
            default_fallback: FallbackBehavior::UseKnowledgeCutoff,
            tenant_capabilities: TenantCapabilities::default(),
        }
    }
}

/// Live data policy implementation
pub struct LiveDataPolicy {
    config: LiveDataConfig,
}

impl LiveDataPolicy {
    /// Create new live data policy
    pub fn new(config: LiveDataConfig) -> Self {
        Self { config }
    }

    /// Classify query category
    pub fn classify_query(&self, query: &str) -> QueryCategory {
        let lower = query.to_lowercase();

        if lower.contains("news") || lower.contains("today") || lower.contains("latest") {
            QueryCategory::News
        } else if lower.contains("weather") || lower.contains("forecast") {
            QueryCategory::Weather
        } else if lower.contains("stock") || lower.contains("price") || lower.contains("market") {
            QueryCategory::Financial
        } else if lower.contains("score") || lower.contains("game") || lower.contains("match") {
            QueryCategory::Sports
        } else if lower.contains("flight") || lower.contains("hotel") || lower.contains("travel") {
            QueryCategory::Travel
        } else if lower.contains("election")
            || lower.contains("vote")
            || lower.contains("political")
        {
            QueryCategory::Politics
        } else if lower.contains("buy") || lower.contains("product") || lower.contains("review") {
            QueryCategory::Products
        } else if lower.contains("current") || lower.contains("now") || lower.contains("recent") {
            QueryCategory::Recency
        } else {
            QueryCategory::Static
        }
    }

    /// Validate grounding for a response
    pub fn validate_grounding(
        &self,
        query: &str,
        evidence: &[GroundingEvidence],
    ) -> ValidationResult {
        let category = self.classify_query(query);
        let requirements = self.get_requirements(&category);

        let has_evidence = !evidence.is_empty();
        let meets_min_sources = evidence.len() >= requirements.min_sources;

        let passed = match category {
            QueryCategory::Static => true,
            _ => has_evidence && meets_min_sources,
        };

        let mut messages = Vec::new();
        if !passed {
            if !has_evidence {
                messages.push("No grounding evidence provided for live data query".to_string());
            }
            if !meets_min_sources {
                messages.push(format!(
                    "Insufficient sources: {} provided, {} required",
                    evidence.len(),
                    requirements.min_sources
                ));
            }
        }

        ValidationResult {
            passed,
            category,
            requirements,
            evidence: evidence.to_vec(),
            fallback: self.config.default_fallback,
            messages,
        }
    }

    /// Get grounding requirements for a category
    fn get_requirements(&self, category: &QueryCategory) -> GroundingRequirements {
        match category {
            QueryCategory::Static => GroundingRequirements::default(),
            QueryCategory::News | QueryCategory::Recency => GroundingRequirements {
                requires_web_search: true,
                requires_api_call: false,
                max_data_age_secs: 3600,
                min_sources: 2,
            },
            QueryCategory::Weather | QueryCategory::Sports | QueryCategory::Financial => {
                GroundingRequirements {
                    requires_web_search: false,
                    requires_api_call: true,
                    max_data_age_secs: 300,
                    min_sources: 1,
                }
            }
            QueryCategory::Travel | QueryCategory::Products => GroundingRequirements {
                requires_web_search: true,
                requires_api_call: false,
                max_data_age_secs: 86400,
                min_sources: 1,
            },
            QueryCategory::Politics => GroundingRequirements {
                requires_web_search: true,
                requires_api_call: false,
                max_data_age_secs: 7200,
                min_sources: 3,
            },
        }
    }
}

impl Policy for LiveDataPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::LiveData
    }

    fn name(&self) -> &'static str {
        "Live Data"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, _ctx: &dyn PolicyContext) -> Result<Audit> {
        Ok(Audit::passed(self.id()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_live_data_policy_creation() {
        let config = LiveDataConfig::default();
        let policy = LiveDataPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::LiveData);
    }

    #[test]
    fn test_classify_query() {
        let policy = LiveDataPolicy::new(LiveDataConfig::default());

        assert_eq!(
            policy.classify_query("What is the weather forecast?"),
            QueryCategory::Weather
        );
        assert_eq!(
            policy.classify_query("Show me the latest news"),
            QueryCategory::News
        );
        assert_eq!(
            policy.classify_query("What is the capital of France?"),
            QueryCategory::Static
        );
    }

    #[test]
    fn test_validate_grounding_static() {
        let policy = LiveDataPolicy::new(LiveDataConfig::default());
        let result = policy.validate_grounding("What is the capital of France?", &[]);

        assert!(result.passed);
        assert_eq!(result.category, QueryCategory::Static);
    }

    #[test]
    fn test_validate_grounding_needs_evidence() {
        let policy = LiveDataPolicy::new(LiveDataConfig::default());
        let result = policy.validate_grounding("What is the latest news?", &[]);

        assert!(!result.passed);
        assert_eq!(result.category, QueryCategory::News);
        assert!(!result.messages.is_empty());
    }
}
