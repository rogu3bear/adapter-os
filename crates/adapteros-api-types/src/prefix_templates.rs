//! Prefix template types for KV cache prefilling
//!
//! Prefix templates define static text prefixes that can be cached at the
//! KV level to avoid redundant prefill computation on repeated requests.
//!
//! See PRD: PrefixKvCache v1

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Prefix template mode indicating the context in which the prefix applies
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrefixMode {
    /// Global system boilerplate (applies to all requests)
    System,
    /// User-facing mode prefix
    User,
    /// Builder/developer mode prefix
    Builder,
    /// Audit mode prefix
    Audit,
    /// Custom mode with a user-defined identifier
    Custom(String),
}

impl PrefixMode {
    /// Convert to string representation for database storage
    pub fn as_str(&self) -> &str {
        match self {
            PrefixMode::System => "system",
            PrefixMode::User => "user",
            PrefixMode::Builder => "builder",
            PrefixMode::Audit => "audit",
            PrefixMode::Custom(s) => s.as_str(),
        }
    }

    /// Parse from string representation
    pub fn from_str(s: &str) -> Self {
        match s {
            "system" => PrefixMode::System,
            "user" => PrefixMode::User,
            "builder" => PrefixMode::Builder,
            "audit" => PrefixMode::Audit,
            other => PrefixMode::Custom(other.to_string()),
        }
    }
}

impl std::fmt::Display for PrefixMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A prefix template configuration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PrefixTemplate {
    /// Unique identifier for this template
    pub id: String,
    /// Tenant that owns this template
    pub tenant_id: String,
    /// Mode this template applies to
    pub mode: PrefixMode,
    /// The actual prefix text to be tokenized and cached
    pub template_text: String,
    /// BLAKE3 hash of template_text
    #[schema(value_type = String)]
    pub template_hash_b3: B3Hash,
    /// Priority for template selection (higher = matched first)
    #[serde(default)]
    pub priority: i32,
    /// Whether this template is enabled
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Request to create a new prefix template
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreatePrefixTemplateRequest {
    /// Tenant that owns this template
    pub tenant_id: String,
    /// Mode this template applies to
    pub mode: PrefixMode,
    /// The actual prefix text to be tokenized and cached
    pub template_text: String,
    /// Priority for template selection (higher = matched first)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// Whether this template is enabled (defaults to true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Request to update an existing prefix template
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdatePrefixTemplateRequest {
    /// Updated mode (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<PrefixMode>,
    /// Updated template text (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_text: Option<String>,
    /// Updated priority (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    /// Updated enabled state (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Response for prefix template operations
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PrefixTemplateResponse {
    pub template: PrefixTemplate,
}

/// Response listing prefix templates
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ListPrefixTemplatesResponse {
    pub templates: Vec<PrefixTemplate>,
    pub total: u64,
}

/// Query parameters for listing prefix templates
#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
pub struct ListPrefixTemplatesQuery {
    /// Filter by mode
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Filter by enabled state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Pagination: page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Pagination: items per page
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_page() -> u32 {
    1
}

fn default_limit() -> u32 {
    50
}

/// Prefix KV cache statistics for observability
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PrefixKvCacheStats {
    /// Total number of cached prefix entries
    pub entry_count: u64,
    /// Total bytes used by cached KV tensors
    pub used_bytes: u64,
    /// Maximum byte budget for the cache
    pub max_bytes: u64,
    /// Cache hit count since last reset
    pub hits: u64,
    /// Cache miss count since last reset
    pub misses: u64,
    /// Number of evictions due to capacity
    pub evictions: u64,
    /// Number of in-flight builds (concurrent miss dedup)
    pub in_flight_builds: u64,
}

impl PrefixKvCacheStats {
    /// Compute hit rate as a percentage (0.0 to 100.0)
    pub fn hit_rate_percent(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }

    /// Compute usage percentage (0.0 to 100.0)
    pub fn usage_percent(&self) -> f64 {
        if self.max_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f64 / self.max_bytes as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefix_mode_roundtrip() {
        let modes = vec![
            PrefixMode::System,
            PrefixMode::User,
            PrefixMode::Builder,
            PrefixMode::Audit,
            PrefixMode::Custom("my_custom_mode".to_string()),
        ];

        for mode in modes {
            let s = mode.as_str();
            let parsed = PrefixMode::from_str(s);
            assert_eq!(mode, parsed);
        }
    }

    #[test]
    fn test_prefix_mode_display() {
        assert_eq!(format!("{}", PrefixMode::System), "system");
        assert_eq!(format!("{}", PrefixMode::User), "user");
        assert_eq!(
            format!("{}", PrefixMode::Custom("test".to_string())),
            "test"
        );
    }

    #[test]
    fn test_prefix_template_serialization() {
        let template = PrefixTemplate {
            id: "tpl_123".to_string(),
            tenant_id: "tenant_1".to_string(),
            mode: PrefixMode::System,
            template_text: "You are a helpful assistant.".to_string(),
            template_hash_b3: B3Hash::hash(b"You are a helpful assistant."),
            priority: 10,
            enabled: true,
        };

        let json = serde_json::to_string(&template).unwrap();
        let parsed: PrefixTemplate = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, template.id);
        assert_eq!(parsed.mode, template.mode);
    }

    #[test]
    fn test_cache_stats_calculations() {
        let stats = PrefixKvCacheStats {
            entry_count: 10,
            used_bytes: 500_000_000,
            max_bytes: 1_000_000_000,
            hits: 80,
            misses: 20,
            evictions: 5,
            in_flight_builds: 0,
        };

        assert!((stats.hit_rate_percent() - 80.0).abs() < 0.01);
        assert!((stats.usage_percent() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_zero_division() {
        let stats = PrefixKvCacheStats {
            entry_count: 0,
            used_bytes: 0,
            max_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
            in_flight_builds: 0,
        };

        assert_eq!(stats.hit_rate_percent(), 0.0);
        assert_eq!(stats.usage_percent(), 0.0);
    }
}
