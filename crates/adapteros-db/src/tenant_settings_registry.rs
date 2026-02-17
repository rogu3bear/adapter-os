//! Tenant settings known-key registry and validation.
//!
//! Known keys are validated with typed serde deserialization while unknown keys
//! are preserved for backward/forward compatibility.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;

/// Known top-level keys accepted in tenant settings JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TenantSettingsKnownKey {
    RouterWeights,
}

impl TenantSettingsKnownKey {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RouterWeights => "router_weights",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "router_weights" => Some(Self::RouterWeights),
            _ => None,
        }
    }

    pub fn settings_path(self) -> String {
        format!("settings_json.{}", self.as_str())
    }
}

/// Typed schema for `settings_json.router_weights`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterWeightsSetting {
    pub language_weight: f32,
    pub framework_weight: f32,
    pub symbol_hits_weight: f32,
    pub path_tokens_weight: f32,
    pub prompt_verb_weight: f32,
    pub orthogonal_weight: f32,
    pub diversity_weight: f32,
    pub similarity_penalty: f32,
}

/// Validation error tied to a concrete settings key path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TenantSettingsValidationError {
    key: String,
    message: String,
}

impl TenantSettingsValidationError {
    pub fn new(key: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            message: message.into(),
        }
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for TenantSettingsValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Invalid tenant setting '{}': {}", self.key, self.message)
    }
}

impl std::error::Error for TenantSettingsValidationError {}

/// Parsed tenant settings JSON preserving unknown keys.
#[derive(Debug, Clone, Default)]
pub struct TenantSettingsRegistry {
    settings: Map<String, Value>,
}

impl TenantSettingsRegistry {
    /// Parse from the DB JSON string and validate known keys.
    pub fn from_settings_json(
        settings_json: Option<&str>,
    ) -> Result<Self, TenantSettingsValidationError> {
        let value = match settings_json {
            Some(raw) if !raw.trim().is_empty() => serde_json::from_str::<Value>(raw)
                .map_err(|e| TenantSettingsValidationError::new("settings_json", e.to_string()))?,
            _ => Value::Object(Map::new()),
        };

        Self::from_value(&value)
    }

    /// Parse from request/body JSON and validate known keys.
    pub fn from_value(value: &Value) -> Result<Self, TenantSettingsValidationError> {
        let settings = value.as_object().cloned().ok_or_else(|| {
            TenantSettingsValidationError::new("settings_json", "must be a JSON object")
        })?;

        validate_known_keys(&settings)?;
        Ok(Self { settings })
    }

    pub fn validate_value(value: &Value) -> Result<(), TenantSettingsValidationError> {
        Self::from_value(value).map(|_| ())
    }

    pub fn get_router_weights(
        &self,
    ) -> Result<Option<RouterWeightsSetting>, TenantSettingsValidationError> {
        match self
            .settings
            .get(TenantSettingsKnownKey::RouterWeights.as_str())
        {
            Some(raw) => parse_router_weights(raw).map(Some),
            None => Ok(None),
        }
    }

    pub fn set_router_weights(
        &mut self,
        weights: RouterWeightsSetting,
    ) -> Result<(), TenantSettingsValidationError> {
        let value = serde_json::to_value(weights).map_err(|e| {
            TenantSettingsValidationError::new(
                TenantSettingsKnownKey::RouterWeights.settings_path(),
                e.to_string(),
            )
        })?;
        self.settings.insert(
            TenantSettingsKnownKey::RouterWeights.as_str().to_string(),
            value,
        );
        Ok(())
    }

    pub fn remove_known_key(&mut self, key: TenantSettingsKnownKey) -> bool {
        self.settings.remove(key.as_str()).is_some()
    }

    pub fn to_json_string(&self) -> Result<String, TenantSettingsValidationError> {
        serde_json::to_string(&self.settings)
            .map_err(|e| TenantSettingsValidationError::new("settings_json", e.to_string()))
    }
}

fn validate_known_keys(settings: &Map<String, Value>) -> Result<(), TenantSettingsValidationError> {
    for (key, value) in settings {
        if let Some(known_key) = TenantSettingsKnownKey::from_key(key) {
            validate_known_key_value(known_key, value)?;
        }
    }
    Ok(())
}

fn validate_known_key_value(
    key: TenantSettingsKnownKey,
    value: &Value,
) -> Result<(), TenantSettingsValidationError> {
    match key {
        TenantSettingsKnownKey::RouterWeights => {
            parse_router_weights(value)?;
            Ok(())
        }
    }
}

fn parse_router_weights(
    value: &Value,
) -> Result<RouterWeightsSetting, TenantSettingsValidationError> {
    serde_json::from_value::<RouterWeightsSetting>(value.clone()).map_err(|e| {
        TenantSettingsValidationError::new(
            TenantSettingsKnownKey::RouterWeights.settings_path(),
            e.to_string(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_router_weights_with_key_path() {
        let value = json!({
            "router_weights": {
                "language_weight": "bad"
            }
        });

        let err =
            TenantSettingsRegistry::from_value(&value).expect_err("expected validation error");
        assert_eq!(err.key(), "settings_json.router_weights");
    }

    #[test]
    fn preserves_unknown_keys_when_mutating_known_key() {
        let value = json!({
            "unknown_flag": true,
            "nested_unknown": {"a": 1}
        });
        let mut registry = TenantSettingsRegistry::from_value(&value).expect("valid object");

        registry
            .set_router_weights(RouterWeightsSetting {
                language_weight: 0.3,
                framework_weight: 0.2,
                symbol_hits_weight: 0.15,
                path_tokens_weight: 0.1,
                prompt_verb_weight: 0.1,
                orthogonal_weight: 0.05,
                diversity_weight: 0.05,
                similarity_penalty: 0.05,
            })
            .expect("set router weights");

        let round_tripped: Value =
            serde_json::from_str(&registry.to_json_string().expect("serialize")).expect("parse");
        assert_eq!(round_tripped["unknown_flag"], json!(true));
        assert_eq!(round_tripped["nested_unknown"], json!({"a": 1}));
        assert!(round_tripped.get("router_weights").is_some());
    }

    #[test]
    fn allows_unknown_top_level_keys() {
        let value = json!({
            "feature_x_enabled": true
        });
        let registry =
            TenantSettingsRegistry::from_value(&value).expect("unknown keys should pass");
        let serialized = registry.to_json_string().expect("serialize");
        let parsed: Value = serde_json::from_str(&serialized).expect("parse");
        assert_eq!(parsed["feature_x_enabled"], json!(true));
    }
}
