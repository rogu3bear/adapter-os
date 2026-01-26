//! Configuration precedence system with deterministic loading

use crate::types::*;
use adapteros_core::{AosError, Result};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Deterministic configuration with precedence enforcement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterministicConfig {
    /// Frozen configuration values
    values: HashMap<String, String>,
    /// Configuration metadata
    metadata: ConfigMetadata,
    /// Configuration schema
    schema: DeterministicSchema,
    /// Freeze status
    frozen: bool,
}

impl DeterministicConfig {
    /// Create a new configuration instance
    pub fn new(
        values: HashMap<String, String>,
        metadata: ConfigMetadata,
        schema: DeterministicSchema,
    ) -> Self {
        Self {
            values,
            metadata,
            schema,
            frozen: false,
        }
    }

    /// Freeze the configuration, making it immutable
    pub fn freeze(&mut self) -> Result<()> {
        if self.frozen {
            return Err(AosError::Config("Configuration already frozen".to_string()));
        }

        // Compute hash of frozen configuration
        let hash = self.compute_hash()?;
        self.metadata.hash = hash;

        self.frozen = true;

        tracing::info!("Configuration frozen with hash: {}", self.metadata.hash);
        Ok(())
    }

    /// Check if configuration is frozen
    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    /// Get a configuration value by key
    pub fn get(&self, key: &str) -> Option<&String> {
        if !self.frozen {
            tracing::warn!("Accessing unfrozen configuration for key: {}", key);
        }
        self.values.get(key)
    }

    /// Get a configuration value with default
    pub fn get_or_default(&self, key: &str, default: &str) -> String {
        self.get(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    /// Get all configuration values
    pub fn get_all(&self) -> &HashMap<String, String> {
        &self.values
    }

    /// Get configuration metadata
    pub fn get_metadata(&self) -> &ConfigMetadata {
        &self.metadata
    }

    /// Get configuration schema
    pub fn get_schema(&self) -> &DeterministicSchema {
        &self.schema
    }

    /// Compute BLAKE3 hash of the configuration
    pub fn compute_hash(&self) -> Result<String> {
        let mut hasher = Hasher::new();

        // Hash configuration values in deterministic order
        let mut sorted_keys: Vec<_> = self.values.keys().collect();
        sorted_keys.sort();

        for key in sorted_keys {
            if let Some(value) = self.values.get(key) {
                hasher.update(key.as_bytes());
                hasher.update(b"=");
                hasher.update(value.as_bytes());
                hasher.update(b"\n");
            }
        }

        // Hash metadata
        let metadata_json = serde_json::to_string(&self.metadata)
            .map_err(|e| AosError::Config(format!("Failed to serialize metadata: {}", e)))?;
        hasher.update(metadata_json.as_bytes());

        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Validate configuration against schema
    pub fn validate(&self) -> Result<Vec<ConfigValidationError>> {
        let mut errors = Vec::new();

        for (key, field_def) in &self.schema.fields {
            if field_def.required && !self.values.contains_key(key) {
                errors.push(ConfigValidationError {
                    key: key.clone(),
                    message: "Required field missing".to_string(),
                    expected_type: field_def.field_type.clone(),
                    actual_value: "missing".to_string(),
                });
                continue;
            }

            if let Some(value) = self.values.get(key) {
                if let Err(validation_error) = self.validate_field_value(key, value, field_def) {
                    errors.push(validation_error);
                }
            }
        }

        Ok(errors)
    }

    /// Validate a single field value
    fn validate_field_value(
        &self,
        key: &str,
        value: &str,
        field_def: &FieldDefinition,
    ) -> std::result::Result<(), ConfigValidationError> {
        match field_def.field_type.as_str() {
            "string" => {
                // String validation - check length if specified
                if let Some(rules) = &field_def.validation_rules {
                    for rule in rules {
                        if rule.starts_with("min_length:") {
                            // Parse min_length:N rule. If the rule is malformed, log and skip.
                            // This allows the config system to be resilient to schema errors.
                            let min_len: usize = match rule.split(':').nth(1) {
                                Some(len_str) => match len_str.parse() {
                                    Ok(len) => len,
                                    Err(_) => {
                                        tracing::warn!(
                                            rule = %rule,
                                            key = %key,
                                            "Malformed min_length validation rule (non-numeric value), skipping"
                                        );
                                        continue;
                                    }
                                },
                                None => {
                                    tracing::warn!(
                                        rule = %rule,
                                        key = %key,
                                        "Malformed min_length validation rule (missing value), skipping"
                                    );
                                    continue;
                                }
                            };
                            if value.len() < min_len {
                                return Err(ConfigValidationError {
                                    key: key.to_string(),
                                    message: format!(
                                        "String too short, minimum length: {}",
                                        min_len
                                    ),
                                    expected_type: "string".to_string(),
                                    actual_value: value.to_string(),
                                });
                            }
                        } else if rule.starts_with("enum:") {
                            let options: Vec<&str> =
                                rule.trim_start_matches("enum:").split(',').collect();
                            if !options.iter().any(|opt| opt == &value) {
                                return Err(ConfigValidationError {
                                    key: key.to_string(),
                                    message: format!("Value must be one of: {}", options.join(",")),
                                    expected_type: "enum".to_string(),
                                    actual_value: value.to_string(),
                                });
                            }
                        } else if rule == "ip_address" {
                            let is_localhost = value.eq_ignore_ascii_case("localhost");
                            let parsed = value.parse::<std::net::IpAddr>();
                            if !is_localhost && parsed.is_err() {
                                return Err(ConfigValidationError {
                                    key: key.to_string(),
                                    message: "Invalid IP address".to_string(),
                                    expected_type: "ip_address".to_string(),
                                    actual_value: value.to_string(),
                                });
                            }
                        } else if rule == "url" {
                            let allowed_prefixes =
                                ["http://", "https://", "file://", "unix://", "sqlite://"];
                            if !allowed_prefixes
                                .iter()
                                .any(|prefix| value.starts_with(prefix))
                            {
                                return Err(ConfigValidationError {
                                    key: key.to_string(),
                                    message: "Invalid URL".to_string(),
                                    expected_type: "url".to_string(),
                                    actual_value: value.to_string(),
                                });
                            }
                        }
                    }
                }
            }
            "integer" => {
                let parsed = match value.parse::<i64>() {
                    Ok(v) => v,
                    Err(_) => {
                        return Err(ConfigValidationError {
                            key: key.to_string(),
                            message: "Invalid integer value".to_string(),
                            expected_type: "integer".to_string(),
                            actual_value: value.to_string(),
                        })
                    }
                };

                if let Some(rules) = &field_def.validation_rules {
                    for rule in rules {
                        if let Some(range) = rule.strip_prefix("range:") {
                            // Parse range:MIN-MAX rule. The format is "range:min-max".
                            // If either bound cannot be parsed, log a warning and skip that bound.
                            let mut parts = range.splitn(2, '-');
                            let min_str = parts.next();
                            let max_str = parts.next();

                            let min: Option<i64> = match min_str {
                                Some(s) if !s.is_empty() => match s.parse() {
                                    Ok(v) => Some(v),
                                    Err(_) => {
                                        tracing::warn!(
                                            rule = %rule,
                                            key = %key,
                                            min_str = %s,
                                            "Malformed range rule (invalid min), skipping min bound"
                                        );
                                        None
                                    }
                                },
                                _ => None,
                            };

                            let max: Option<i64> = match max_str {
                                Some(s) if !s.is_empty() => match s.parse() {
                                    Ok(v) => Some(v),
                                    Err(_) => {
                                        tracing::warn!(
                                            rule = %rule,
                                            key = %key,
                                            max_str = %s,
                                            "Malformed range rule (invalid max), skipping max bound"
                                        );
                                        None
                                    }
                                },
                                _ => None,
                            };

                            if let Some(min) = min {
                                if parsed < min {
                                    return Err(ConfigValidationError {
                                        key: key.to_string(),
                                        message: format!("Value below minimum range: {}", min),
                                        expected_type: "integer".to_string(),
                                        actual_value: value.to_string(),
                                    });
                                }
                            }
                            if let Some(max) = max {
                                if parsed > max {
                                    return Err(ConfigValidationError {
                                        key: key.to_string(),
                                        message: format!("Value above maximum range: {}", max),
                                        expected_type: "integer".to_string(),
                                        actual_value: value.to_string(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            "boolean" => {
                if !matches!(value.to_lowercase().as_str(), "true" | "false" | "1" | "0") {
                    return Err(ConfigValidationError {
                        key: key.to_string(),
                        message: "Invalid boolean value".to_string(),
                        expected_type: "boolean".to_string(),
                        actual_value: value.to_string(),
                    });
                }
            }
            "float" => {
                if value.parse::<f64>().is_err() {
                    return Err(ConfigValidationError {
                        key: key.to_string(),
                        message: "Invalid float value".to_string(),
                        expected_type: "float".to_string(),
                        actual_value: value.to_string(),
                    });
                }
            }
            _ => {
                return Err(ConfigValidationError {
                    key: key.to_string(),
                    message: format!("Unknown field type: {}", field_def.field_type),
                    expected_type: field_def.field_type.clone(),
                    actual_value: value.to_string(),
                });
            }
        }

        Ok(())
    }

    /// Get configuration as JSON for tracing
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| AosError::Config(format!("Failed to serialize config: {}", e)))
    }

    /// Create a configuration for testing purposes
    #[cfg(test)]
    pub fn new_for_test(values: HashMap<String, String>) -> Self {
        let metadata = ConfigMetadata {
            frozen_at: chrono::Utc::now().to_rfc3339(),
            hash: String::new(),
            sources: values
                .iter()
                .map(|(k, v)| ConfigFieldSource {
                    level: PrecedenceLevel::Environment,
                    source: "test".to_string(),
                    key: k.clone(),
                    value: v.clone(),
                })
                .collect(),
            manifest_path: None,
            cli_args: vec![],
        };

        Self {
            values,
            metadata,
            schema: DeterministicSchema::default(),
            frozen: true,
        }
    }
}

impl fmt::Display for DeterministicConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "DeterministicConfig {{")?;
        writeln!(f, "  frozen: {}", self.frozen)?;
        writeln!(f, "  hash: {}", self.metadata.hash)?;
        writeln!(f, "  values: {} entries", self.values.len())?;
        writeln!(f, "  sources: {} entries", self.metadata.sources.len())?;
        writeln!(f, "}}")?;
        Ok(())
    }
}

/// Configuration builder for constructing deterministic configs
pub struct ConfigBuilder {
    values: HashMap<String, String>,
    sources: Vec<ConfigFieldSource>,
    schema: DeterministicSchema,
    manifest_path: Option<String>,
    cli_args: Vec<String>,
}

impl ConfigBuilder {
    /// Create a new configuration builder
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            sources: Vec::new(),
            schema: DeterministicSchema::default(),
            manifest_path: None,
            cli_args: Vec::new(),
        }
    }

    /// Set the configuration schema
    pub fn with_schema(mut self, schema: DeterministicSchema) -> Self {
        self.schema = schema;
        self
    }

    /// Add a configuration value with source information
    pub fn add_value(
        mut self,
        key: String,
        value: String,
        level: PrecedenceLevel,
        source: String,
    ) -> Self {
        // Check if key already exists with lower precedence
        if let Some(existing_source) = self.sources.iter().find(|s| s.key == key) {
            if existing_source.level < level {
                // Remove existing lower precedence value
                self.values.remove(&key);
                self.sources.retain(|s| s.key != key);
            } else {
                // Higher precedence value already exists, skip
                return self;
            }
        }

        self.values.insert(key.clone(), value.clone());
        self.sources.push(ConfigFieldSource {
            level,
            source,
            key,
            value,
        });

        self
    }

    /// Set manifest path
    pub fn with_manifest_path(mut self, path: String) -> Self {
        self.manifest_path = Some(path);
        self
    }

    /// Set CLI arguments
    pub fn with_cli_args(mut self, args: Vec<String>) -> Self {
        self.cli_args = args;
        self
    }

    /// Build the deterministic configuration
    pub fn build(self) -> Result<DeterministicConfig> {
        let mut values = self.values;
        let mut sources = self.sources;
        let schema = self.schema;
        let manifest_path = self.manifest_path;
        let cli_args = self.cli_args;

        // Apply schema defaults for missing values up front
        for (key, field_def) in &schema.fields {
            if !values.contains_key(key) {
                if let Some(default_value) = &field_def.default_value {
                    values.insert(key.clone(), default_value.clone());
                    sources.push(ConfigFieldSource {
                        level: PrecedenceLevel::Manifest,
                        source: "default".to_string(),
                        key: key.clone(),
                        value: default_value.clone(),
                    });
                }
            }
        }

        // Validate and fall back to defaults on type mismatches where possible
        loop {
            let metadata = ConfigMetadata {
                frozen_at: chrono::Utc::now().to_rfc3339(),
                hash: String::new(), // Will be set during freeze
                sources: sources.clone(),
                manifest_path: manifest_path.clone(),
                cli_args: cli_args.clone(),
            };

            let config = DeterministicConfig::new(values.clone(), metadata, schema.clone());
            let validation_errors = config.validate()?;
            if validation_errors.is_empty() {
                return Ok(config);
            }

            let mut fatal_errors = Vec::new();
            for err in validation_errors {
                if let Some(field_def) = schema.fields.get(&err.key) {
                    if let Some(default_value) = &field_def.default_value {
                        tracing::warn!(
                            key = %err.key,
                            expected = %err.expected_type,
                            actual = %err.actual_value,
                            "Invalid config value, falling back to default"
                        );
                        values.insert(err.key.clone(), default_value.clone());
                        sources.retain(|s| s.key != err.key);
                        sources.push(ConfigFieldSource {
                            level: PrecedenceLevel::Manifest,
                            source: "default".to_string(),
                            key: err.key.clone(),
                            value: default_value.clone(),
                        });
                        continue;
                    }
                }
                fatal_errors.push(err);
            }

            if !fatal_errors.is_empty() {
                return Err(AosError::Config(format!(
                    "Configuration validation failed: {:?}",
                    fatal_errors
                )));
            }
        }
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
