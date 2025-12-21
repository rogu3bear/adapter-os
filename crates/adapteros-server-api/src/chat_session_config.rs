use adapteros_types::adapters::metadata::RoutingDeterminismMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Session-scoped chat configuration stored in `chat_sessions.metadata_json`.
///
/// This is intentionally flexible to avoid schema changes; future fields (e.g.,
/// `package_id`) can be added without a migration.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ChatSessionConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_determinism_mode: Option<RoutingDeterminismMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_strength_overrides: Option<HashMap<String, f32>>,
    // Reserved for future use (e.g., package_id)
}

impl ChatSessionConfig {
    /// Parse config from a metadata JSON blob.
    pub fn from_metadata(metadata_json: Option<&str>) -> Option<Self> {
        metadata_json.and_then(|raw| serde_json::from_str(raw).ok())
    }

    /// Merge the provided config into an existing metadata JSON blob.
    ///
    /// - Preserves unrelated metadata keys.
    /// - Overwrites existing config keys with provided values when `Some`.
    /// - Leaves keys untouched when `None`.
    pub fn merge_into_metadata(
        metadata_json: Option<&str>,
        config: &ChatSessionConfig,
    ) -> Option<String> {
        let mut root: serde_json::Map<String, serde_json::Value> = metadata_json
            .and_then(|raw| serde_json::from_str(raw).ok())
            .and_then(|value: serde_json::Value| value.as_object().cloned())
            .unwrap_or_default();

        let mut merged_config: ChatSessionConfig = root
            .get("chat_session_config")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        if let Some(stack_id) = config.stack_id.clone() {
            merged_config.stack_id = Some(stack_id);
        }
        if let Some(mode) = config.routing_determinism_mode {
            merged_config.routing_determinism_mode = Some(mode);
        }
        if let Some(overrides) = config.adapter_strength_overrides.clone() {
            merged_config.adapter_strength_overrides = Some(overrides);
        }

        root.insert(
            "chat_session_config".to_string(),
            serde_json::to_value(merged_config).unwrap_or_default(),
        );

        Some(serde_json::to_string(&root).unwrap_or_default())
    }
}
