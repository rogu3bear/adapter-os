use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Rule for selecting an adapter stack during orchestration
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationRule {
    pub id: String,
    pub name: String,
    pub condition: String,
    pub adapter_stack: String,
    pub priority: i32,
    pub enabled: bool,
}

/// Prompt orchestration configuration (single-node aware)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub struct OrchestrationConfig {
    pub enabled: bool,
    /// Routing strategy used when multiple adapter stacks are available
    pub routing_strategy: String,
    /// Optional default adapter stack to prefer
    pub default_adapter_stack: Option<String>,
    /// Maximum adapters evaluated per request
    pub max_adapters_per_request: u32,
    /// Timeout for orchestration operations (ms)
    pub timeout_ms: u32,
    /// Whether to fall back to a default adapter on routing miss
    pub fallback_enabled: bool,
    /// Optional fallback adapter stack identifier
    pub fallback_adapter: Option<String>,
    /// Minimum entropy threshold for adaptive routing (0.0-1.0)
    pub entropy_threshold: Option<f32>,
    /// Minimum confidence threshold for router recommendations (0.0-1.0)
    pub confidence_threshold: Option<f32>,
    /// Enable caching of orchestration decisions
    pub cache_enabled: bool,
    /// TTL for orchestration cache entries (seconds)
    pub cache_ttl_seconds: u32,
    /// Emit telemetry for orchestration decisions
    pub telemetry_enabled: bool,
    /// Custom rules evaluated before default routing
    #[serde(default)]
    pub custom_rules: Vec<OrchestrationRule>,
}

impl Default for OrchestrationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            routing_strategy: "entropy".to_string(),
            default_adapter_stack: None,
            max_adapters_per_request: 1,
            timeout_ms: 5_000,
            fallback_enabled: false,
            fallback_adapter: None,
            entropy_threshold: None,
            confidence_threshold: None,
            cache_enabled: true,
            cache_ttl_seconds: 300,
            telemetry_enabled: true,
            custom_rules: Vec::new(),
        }
    }
}
