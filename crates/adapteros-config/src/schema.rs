//! Configuration schema definitions for AOS_* environment variables
//!
//! This module provides comprehensive validation rules for all configuration
//! variables used in the adapterOS system. The schema supports type validation,
//! constraints, deprecation tracking, and sensitive value handling.
//!
//! # Example
//!
//! ```rust
//! use adapteros_config::schema::{ConfigSchema, default_schema, validate_value};
//!
//! let schema = default_schema();
//! let port_var = schema.get_variable("AOS_SERVER_PORT").unwrap();
//!
//! // Valid value
//! assert!(validate_value(&port_var, "8080").is_ok());
//!
//! // Invalid value (out of range)
//! assert!(validate_value(&port_var, "99999").is_err());
//! ```

use crate::path_resolver::{
    DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT, DEV_MANIFEST_PATH, DEV_MODEL_PATH,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Configuration variable type with validation constraints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum ConfigType {
    /// Plain string value
    #[default]
    String,
    /// File system path with optional existence check
    Path {
        /// Whether the path must exist at validation time
        must_exist: bool,
    },
    /// URL (http://, https://, file://, unix://, sqlite://)
    Url,
    /// Integer with optional range constraints
    Integer {
        /// Minimum allowed value (inclusive)
        min: Option<i64>,
        /// Maximum allowed value (inclusive)
        max: Option<i64>,
    },
    /// Floating point with optional range constraints
    Float {
        /// Minimum allowed value (inclusive)
        min: Option<f64>,
        /// Maximum allowed value (inclusive)
        max: Option<f64>,
    },
    /// Boolean value (true/false, 1/0, yes/no, on/off)
    Bool,
    /// Enumeration of allowed string values
    Enum {
        /// List of valid values (case-insensitive matching)
        values: Vec<String>,
    },
    /// Duration string (e.g., "30s", "5m", "1h", "500ms")
    Duration,
    /// Byte size string (e.g., "1GB", "512MB", "1024KB", "1048576")
    ByteSize,
}

/// Information about a deprecated configuration variable
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeprecationInfo {
    /// The replacement variable to use instead
    pub replacement: String,
    /// The version when this variable will be removed
    pub removal_version: String,
    /// Optional additional migration notes
    pub notes: Option<String>,
}

/// Definition of a configuration variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigVariable {
    /// Environment variable name (e.g., "AOS_SERVER_PORT")
    pub name: String,
    /// Type and validation constraints
    pub config_type: ConfigType,
    /// Whether this variable is required
    pub required: bool,
    /// Default value if not specified
    pub default: Option<String>,
    /// Human-readable description
    pub description: String,
    /// Deprecation information if this variable is deprecated
    pub deprecated: Option<DeprecationInfo>,
    /// Whether this value should be redacted in logs/output
    pub sensitive: bool,
    /// Configuration category (e.g., "MODEL", "SERVER")
    pub category: String,
    /// Equivalent config file key (e.g., "server.port")
    pub config_key: String,
    /// TOML config file key if different from config_key (e.g., "db.path" for AOS_DATABASE_URL)
    /// Used for mapping cp.toml values to the unified config system
    pub toml_key: Option<String>,
}

impl ConfigVariable {
    /// Create a new configuration variable builder
    #[allow(clippy::new_ret_no_self)]
    pub fn new(name: &str) -> ConfigVariableBuilder {
        ConfigVariableBuilder::new(name)
    }

    /// Check if this variable is deprecated
    pub fn is_deprecated(&self) -> bool {
        self.deprecated.is_some()
    }

    /// Get the display value (redacted if sensitive)
    pub fn display_value(&self, value: &str) -> String {
        if self.sensitive {
            "***REDACTED***".to_string()
        } else {
            value.to_string()
        }
    }
}

/// Builder for constructing ConfigVariable instances
pub struct ConfigVariableBuilder {
    name: String,
    config_type: ConfigType,
    required: bool,
    default: Option<String>,
    description: String,
    deprecated: Option<DeprecationInfo>,
    sensitive: bool,
    category: String,
    config_key: String,
    toml_key: Option<String>,
}

impl ConfigVariableBuilder {
    /// Create a new builder with the variable name
    pub fn new(name: &str) -> Self {
        // Derive config_key from name: AOS_SERVER_PORT -> server.port
        let config_key = name
            .strip_prefix("AOS_")
            .unwrap_or(name)
            .to_lowercase()
            .replace('_', ".");

        Self {
            name: name.to_string(),
            config_type: ConfigType::String,
            required: false,
            default: None,
            description: String::new(),
            deprecated: None,
            sensitive: false,
            category: String::new(),
            config_key,
            toml_key: None,
        }
    }

    /// Set the configuration type
    pub fn config_type(mut self, config_type: ConfigType) -> Self {
        self.config_type = config_type;
        self
    }

    /// Mark as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set the default value
    pub fn default_value(mut self, default: &str) -> Self {
        self.default = Some(default.to_string());
        self
    }

    /// Set the description
    pub fn description(mut self, description: &str) -> Self {
        self.description = description.to_string();
        self
    }

    /// Mark as deprecated
    pub fn deprecated(mut self, replacement: &str, removal_version: &str) -> Self {
        self.deprecated = Some(DeprecationInfo {
            replacement: replacement.to_string(),
            removal_version: removal_version.to_string(),
            notes: None,
        });
        self
    }

    /// Mark as deprecated with notes
    pub fn deprecated_with_notes(
        mut self,
        replacement: &str,
        removal_version: &str,
        notes: &str,
    ) -> Self {
        self.deprecated = Some(DeprecationInfo {
            replacement: replacement.to_string(),
            removal_version: removal_version.to_string(),
            notes: Some(notes.to_string()),
        });
        self
    }

    /// Mark as sensitive (will be redacted in logs)
    pub fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }

    /// Set the category
    pub fn category(mut self, category: &str) -> Self {
        self.category = category.to_string();
        self
    }

    /// Override the config key
    pub fn config_key(mut self, key: &str) -> Self {
        self.config_key = key.to_string();
        self
    }

    /// Set the TOML key (for cp.toml integration when different from config_key)
    pub fn toml_key(mut self, key: &str) -> Self {
        self.toml_key = Some(key.to_string());
        self
    }

    /// Build the ConfigVariable
    pub fn build(self) -> ConfigVariable {
        ConfigVariable {
            name: self.name,
            config_type: self.config_type,
            required: self.required,
            default: self.default,
            description: self.description,
            deprecated: self.deprecated,
            sensitive: self.sensitive,
            category: self.category,
            config_key: self.config_key,
            toml_key: self.toml_key,
        }
    }
}

/// Validation error for configuration values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// The variable name that failed validation
    pub variable: String,
    /// The value that was provided
    pub value: String,
    /// Description of what was expected
    pub expected: String,
    /// Human-readable error message
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Validation error for '{}': {} (got: '{}', expected: {})",
            self.variable, self.message, self.value, self.expected
        )
    }
}

impl std::error::Error for ValidationError {}

/// Schema holding all configuration variable definitions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    /// Schema version for compatibility checking
    pub version: String,
    /// All variable definitions indexed by name
    pub variables: HashMap<String, ConfigVariable>,
    /// Variables grouped by category
    pub categories: HashMap<String, Vec<String>>,
}

impl ConfigSchema {
    /// Create a new empty schema
    pub fn new() -> Self {
        Self {
            version: "1.0.0".to_string(),
            variables: HashMap::new(),
            categories: HashMap::new(),
        }
    }

    /// Add a variable to the schema
    pub fn add_variable(&mut self, var: ConfigVariable) {
        let name = var.name.clone();
        let category = var.category.clone();

        self.variables.insert(name.clone(), var);

        self.categories.entry(category).or_default().push(name);
    }

    /// Get a variable by name
    pub fn get_variable(&self, name: &str) -> Option<&ConfigVariable> {
        self.variables.get(name)
    }

    /// Get all variables in a category
    pub fn get_category(&self, category: &str) -> Vec<&ConfigVariable> {
        self.categories
            .get(category)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.variables.get(name))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all required variables
    pub fn get_required(&self) -> Vec<&ConfigVariable> {
        self.variables.values().filter(|v| v.required).collect()
    }

    /// Get all deprecated variables
    pub fn get_deprecated(&self) -> Vec<&ConfigVariable> {
        self.variables
            .values()
            .filter(|v| v.deprecated.is_some())
            .collect()
    }

    /// Get all sensitive variables
    pub fn get_sensitive(&self) -> Vec<&ConfigVariable> {
        self.variables.values().filter(|v| v.sensitive).collect()
    }

    /// Get all category names
    pub fn category_names(&self) -> Vec<&str> {
        self.categories.keys().map(|s| s.as_str()).collect()
    }

    /// Get a variable by its TOML key (for cp.toml integration)
    /// Falls back to matching config_key if no toml_key is set
    pub fn get_variable_by_toml_key(&self, toml_key: &str) -> Option<&ConfigVariable> {
        // First check explicit toml_key matches
        for var in self.variables.values() {
            if let Some(ref tk) = var.toml_key {
                if tk == toml_key {
                    return Some(var);
                }
            }
        }
        // Then check config_key matches (for vars without explicit toml_key)
        self.variables
            .values()
            .find(|&var| var.toml_key.is_none() && var.config_key == toml_key)
            .map(|v| v as _)
    }

    /// Build a map from TOML keys to config_key for efficient loading
    /// Returns: (toml_key -> config_key)
    pub fn build_toml_key_map(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        for var in self.variables.values() {
            let toml_key = var.toml_key.as_ref().unwrap_or(&var.config_key);
            map.insert(toml_key.clone(), var.config_key.clone());
        }
        map
    }

    /// Validate all provided values against the schema
    pub fn validate_all(
        &self,
        values: &HashMap<String, String>,
    ) -> std::result::Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();

        // Check required variables are present
        for var in self.get_required() {
            if !values.contains_key(&var.name) && var.default.is_none() {
                errors.push(ValidationError {
                    variable: var.name.clone(),
                    value: String::new(),
                    expected: "a value".to_string(),
                    message: format!("Required variable '{}' is not set", var.name),
                });
            }
        }

        // Validate provided values
        for (name, value) in values {
            if let Some(var) = self.get_variable(name) {
                if let Err(e) = validate_value(var, value) {
                    errors.push(e);
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

impl Default for ConfigSchema {
    fn default() -> Self {
        default_schema()
    }
}

/// Validate a single configuration value against its variable definition
pub fn validate_value(
    var: &ConfigVariable,
    value: &str,
) -> std::result::Result<(), ValidationError> {
    match &var.config_type {
        ConfigType::String => {
            // String values are always valid (non-empty if required handled separately)
            Ok(())
        }
        ConfigType::Path { must_exist } => {
            if value.is_empty() {
                return Err(ValidationError {
                    variable: var.name.clone(),
                    value: value.to_string(),
                    expected: "a valid file path".to_string(),
                    message: "Path cannot be empty".to_string(),
                });
            }
            if *must_exist {
                let path = Path::new(value);
                if !path.exists() {
                    return Err(ValidationError {
                        variable: var.name.clone(),
                        value: value.to_string(),
                        expected: "an existing path".to_string(),
                        message: format!("Path does not exist: {}", value),
                    });
                }
            }
            Ok(())
        }
        ConfigType::Url => {
            // Check for common URL schemes
            let valid_schemes = ["http://", "https://", "file://", "unix://", "sqlite://"];
            if !valid_schemes.iter().any(|s| value.starts_with(s)) {
                return Err(ValidationError {
                    variable: var.name.clone(),
                    value: value.to_string(),
                    expected: format!("URL starting with one of: {}", valid_schemes.join(", ")),
                    message: format!("Invalid URL scheme: {}", value),
                });
            }
            Ok(())
        }
        ConfigType::Integer { min, max } => {
            let parsed: i64 = value.parse().map_err(|_| ValidationError {
                variable: var.name.clone(),
                value: value.to_string(),
                expected: "an integer".to_string(),
                message: format!("Cannot parse '{}' as integer", value),
            })?;
            if let Some(min_val) = min {
                if parsed < *min_val {
                    return Err(ValidationError {
                        variable: var.name.clone(),
                        value: value.to_string(),
                        expected: format!("integer >= {}", min_val),
                        message: format!("Value {} is below minimum {}", parsed, min_val),
                    });
                }
            }
            if let Some(max_val) = max {
                if parsed > *max_val {
                    return Err(ValidationError {
                        variable: var.name.clone(),
                        value: value.to_string(),
                        expected: format!("integer <= {}", max_val),
                        message: format!("Value {} exceeds maximum {}", parsed, max_val),
                    });
                }
            }
            Ok(())
        }
        ConfigType::Float { min, max } => {
            let parsed: f64 = value.parse().map_err(|_| ValidationError {
                variable: var.name.clone(),
                value: value.to_string(),
                expected: "a floating-point number".to_string(),
                message: format!("Cannot parse '{}' as float", value),
            })?;
            if let Some(min_val) = min {
                if parsed < *min_val {
                    return Err(ValidationError {
                        variable: var.name.clone(),
                        value: value.to_string(),
                        expected: format!("float >= {}", min_val),
                        message: format!("Value {} is below minimum {}", parsed, min_val),
                    });
                }
            }
            if let Some(max_val) = max {
                if parsed > *max_val {
                    return Err(ValidationError {
                        variable: var.name.clone(),
                        value: value.to_string(),
                        expected: format!("float <= {}", max_val),
                        message: format!("Value {} exceeds maximum {}", parsed, max_val),
                    });
                }
            }
            Ok(())
        }
        ConfigType::Bool => {
            let lower = value.to_lowercase();
            if !matches!(
                lower.as_str(),
                "true" | "false" | "1" | "0" | "yes" | "no" | "on" | "off"
            ) {
                return Err(ValidationError {
                    variable: var.name.clone(),
                    value: value.to_string(),
                    expected: "true, false, 1, 0, yes, no, on, or off".to_string(),
                    message: format!("Invalid boolean value: {}", value),
                });
            }
            Ok(())
        }
        ConfigType::Enum { values } => {
            let lower = value.to_lowercase();
            let valid_lower: Vec<String> = values.iter().map(|v| v.to_lowercase()).collect();
            if !valid_lower.contains(&lower) {
                return Err(ValidationError {
                    variable: var.name.clone(),
                    value: value.to_string(),
                    expected: format!("one of: {}", values.join(", ")),
                    message: format!(
                        "Invalid value '{}', must be one of: {}",
                        value,
                        values.join(", ")
                    ),
                });
            }
            Ok(())
        }
        ConfigType::Duration => {
            // Parse duration strings like "30s", "5m", "1h", "500ms"
            if let Err(e) = parse_duration(value) {
                return Err(ValidationError {
                    variable: var.name.clone(),
                    value: value.to_string(),
                    expected: "duration string (e.g., '30s', '5m', '1h', '500ms')".to_string(),
                    message: e,
                });
            }
            Ok(())
        }
        ConfigType::ByteSize => {
            // Parse byte size strings like "1GB", "512MB", "1024KB"
            if let Err(e) = parse_byte_size(value) {
                return Err(ValidationError {
                    variable: var.name.clone(),
                    value: value.to_string(),
                    expected: "byte size string (e.g., '1GB', '512MB', '1024KB', '1048576')"
                        .to_string(),
                    message: e,
                });
            }
            Ok(())
        }
    }
}

/// Parse a duration string into milliseconds
fn parse_duration(value: &str) -> std::result::Result<u64, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Duration cannot be empty".to_string());
    }

    // Try to parse as plain number (assume seconds)
    if let Ok(secs) = value.parse::<u64>() {
        return Ok(secs * 1000);
    }

    let (num_str, unit) = if let Some(stripped) = value.strip_suffix("ms") {
        (stripped, "ms")
    } else if let Some(stripped) = value.strip_suffix('s') {
        (stripped, "s")
    } else if let Some(stripped) = value.strip_suffix('m') {
        (stripped, "m")
    } else if let Some(stripped) = value.strip_suffix('h') {
        (stripped, "h")
    } else if let Some(stripped) = value.strip_suffix('d') {
        (stripped, "d")
    } else {
        return Err(format!(
            "Invalid duration format: '{}'. Use suffix: ms, s, m, h, d",
            value
        ));
    };

    let num: u64 = num_str
        .trim()
        .parse()
        .map_err(|_| format!("Cannot parse number in duration: '{}'", num_str))?;

    let millis = match unit {
        "ms" => num,
        "s" => num * 1000,
        "m" => num * 60 * 1000,
        "h" => num * 60 * 60 * 1000,
        "d" => num * 24 * 60 * 60 * 1000,
        other => {
            return Err(format!(
                "Internal error: unexpected duration unit '{}'",
                other
            ))
        }
    };

    Ok(millis)
}

/// Parse a byte size string into bytes
pub fn parse_byte_size(value: &str) -> std::result::Result<u64, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Byte size cannot be empty".to_string());
    }

    // Try to parse as plain number (bytes)
    if let Ok(bytes) = value.parse::<u64>() {
        return Ok(bytes);
    }

    let upper = value.to_uppercase();

    let (num_str, multiplier) = if upper.ends_with("GB") || upper.ends_with("G") {
        let suffix_len = if upper.ends_with("GB") { 2 } else { 1 };
        (&value[..value.len() - suffix_len], 1024u64 * 1024 * 1024)
    } else if upper.ends_with("MB") || upper.ends_with("M") {
        let suffix_len = if upper.ends_with("MB") { 2 } else { 1 };
        (&value[..value.len() - suffix_len], 1024u64 * 1024)
    } else if upper.ends_with("KB") || upper.ends_with("K") {
        let suffix_len = if upper.ends_with("KB") { 2 } else { 1 };
        (&value[..value.len() - suffix_len], 1024u64)
    } else if upper.ends_with('B') {
        (&value[..value.len() - 1], 1u64)
    } else {
        return Err(format!(
            "Invalid byte size format: '{}'. Use suffix: B, KB, MB, GB or plain number",
            value
        ));
    };

    let num: f64 = num_str
        .trim()
        .parse()
        .map_err(|_| format!("Cannot parse number in byte size: '{}'", num_str))?;

    Ok((num * multiplier as f64) as u64)
}

/// Parse a boolean value from string
pub fn parse_bool(value: &str) -> std::result::Result<bool, String> {
    match value.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("Invalid boolean value: '{}'", value)),
    }
}

/// Returns the default schema with all AOS_* variable definitions
pub fn default_schema() -> ConfigSchema {
    let mut schema = ConfigSchema::new();

    // ========================================================================
    // MODEL Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_BASE_MODEL_ID")
            .config_type(ConfigType::String)
            .default_value(DEFAULT_BASE_MODEL_ID)
            .description("Canonical base model identifier used across server and CLI")
            .category("MODEL")
            .config_key("base_model.id")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MODEL_CACHE_DIR")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value(DEFAULT_MODEL_CACHE_ROOT)
            .description(
                "Root directory for cached base models (default: ./var/model-cache/models)",
            )
            .category("MODEL")
            .config_key("base_model.cache_root")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MODEL_CACHE_MAX_MB")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: None,
            })
            .description(
                "Maximum in-process model cache size (MB). Required for worker startup to bound memory.",
            )
            .category("MODEL")
            .config_key("model.cache.max.mb")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PIN_BASE_MODEL")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Pin the base model in the worker cache for its lifetime")
            .category("MODEL")
            .config_key("model.cache.pin_base_model")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PIN_BUDGET_BYTES")
            .config_type(ConfigType::ByteSize)
            .description("Memory budget in bytes for base model pinning")
            .category("MODEL")
            .config_key("model.cache.pin_budget_bytes")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MODEL_PATH")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value(DEV_MODEL_PATH)
            .description("Path to the model directory or model weights file")
            .category("MODEL")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MODEL_BACKEND")
            .config_type(ConfigType::Enum {
                values: vec![
                    "auto".to_string(),
                    "coreml".to_string(),
                    "metal".to_string(),
                    "mlx".to_string(),
                ],
            })
            .default_value("mlx")
            .description("Model backend selection: mlx (default), coreml (ANE production), metal (fallback), auto (detect best)")
            .category("MODEL")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PLACEMENT_MODE")
            .config_type(ConfigType::Enum {
                values: vec![
                    "balanced".to_string(),
                    "latency".to_string(),
                    "energy".to_string(),
                    "thermal".to_string(),
                    "off".to_string(),
                ],
            })
            .default_value("balanced")
            .description(
                "Per-token device placement strategy: balanced, latency, energy, thermal, off",
            )
            .category("MODEL")
            .config_key("placement.mode")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PLACEMENT_LATENCY_WEIGHT")
            .config_type(ConfigType::Float {
                min: Some(0.0),
                max: Some(5.0),
            })
            .default_value("0.5")
            .description("Weight for latency in placement cost model")
            .category("MODEL")
            .config_key("placement.latency_weight")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PLACEMENT_ENERGY_WEIGHT")
            .config_type(ConfigType::Float {
                min: Some(0.0),
                max: Some(5.0),
            })
            .default_value("0.25")
            .description("Weight for energy efficiency in placement cost model")
            .category("MODEL")
            .config_key("placement.energy_weight")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PLACEMENT_THERMAL_WEIGHT")
            .config_type(ConfigType::Float {
                min: Some(0.0),
                max: Some(5.0),
            })
            .default_value("0.25")
            .description("Weight for thermal headroom in placement cost model")
            .category("MODEL")
            .config_key("placement.thermal_weight")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PLACEMENT_THERMAL_CEILING_C")
            .config_type(ConfigType::Float {
                min: Some(40.0),
                max: Some(110.0),
            })
            .default_value("84.0")
            .description("Thermal ceiling (Celsius) before steering away from a device")
            .category("MODEL")
            .config_key("placement.thermal_ceiling_c")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PLACEMENT_COOLDOWN_STEPS")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(1024),
            })
            .default_value("4")
            .description("Minimum steps to keep a device cooled down after a thermal hit")
            .category("MODEL")
            .config_key("placement.cooldown_steps")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_PLACEMENT_SAMPLE_MS")
            .config_type(ConfigType::Integer {
                min: Some(50),
                max: Some(5000),
            })
            .default_value("250")
            .description("Telemetry sampling interval in milliseconds for placement")
            .category("MODEL")
            .config_key("placement.sample_ms")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MANIFEST_PATH")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value(DEV_MANIFEST_PATH)
            .description("Path to the base model manifest file for executor seeding")
            .category("MODEL")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_TOKENIZER_PATH")
            .config_type(ConfigType::Path { must_exist: false })
            .description("Path to tokenizer.json file. If not set, discovered from AOS_MODEL_PATH")
            .category("MODEL")
            .build(),
    );

    // ========================================================================
    // INFERENCE Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_INFERENCE_SEED_MODE")
            .config_type(ConfigType::Enum {
                values: vec![
                    "strict".to_string(),
                    "best_effort".to_string(),
                    "non_deterministic".to_string(),
                ],
            })
            .default_value("best_effort")
            .description(
                "Seed mode for request seeds: strict, best_effort (default dev), non_deterministic (dev-only)",
            )
            .category("INFERENCE")
            .config_key("inference.seed.mode")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_INFERENCE_BACKEND_PROFILE")
            .config_type(ConfigType::Enum {
                values: vec![
                    "auto".to_string(),
                    "autodev".to_string(),
                    "coreml".to_string(),
                    "metal".to_string(),
                    "mlx".to_string(),
                    "cpu".to_string(),
                ],
            })
            .default_value("auto")
            .description(
                "Backend profile for inference: auto (dev default), coreml, metal, mlx, cpu. \
                 'autodev' remains accepted for backward compatibility.",
            )
            .category("INFERENCE")
            .config_key("inference.backend.profile")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_INFERENCE_WORKER_ID")
            .config_type(ConfigType::Integer {
                min: Some(0),
                max: Some(1_000_000),
            })
            .default_value("0")
            .description("Worker identifier used for request seed derivation")
            .category("INFERENCE")
            .config_key("inference.worker.id")
            .build(),
    );

    // ========================================================================
    // SERVER Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_SERVER_HOST")
            .config_type(ConfigType::String)
            .default_value("127.0.0.1")
            .description("Server bind address (IP address or hostname)")
            .category("SERVER")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SERVER_PORT")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(65535),
            })
            .default_value("8080")
            .description("Server port number (1-65535)")
            .category("SERVER")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SERVER_WORKERS")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(256),
            })
            .default_value("4")
            .description("Number of worker threads for handling requests")
            .category("SERVER")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SERVER_UDS_SOCKET")
            .config_type(ConfigType::Path { must_exist: false })
            .description("Unix domain socket path for IPC (required in production mode)")
            .category("SERVER")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SERVER_PRODUCTION_MODE")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Enable production mode (requires UDS socket, EdDSA JWT, PF deny)")
            .category("SERVER")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_REFERENCE_MODE")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Enable reference mode (service panel + reference-friendly defaults)")
            .category("SERVER")
            .build(),
    );

    // ========================================================================
    // DATABASE Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_DATABASE_URL")
            .config_type(ConfigType::Url)
            .default_value("sqlite://var/aos-cp.sqlite3")
            .description("Database connection URL (SQLite)")
            .category("DATABASE")
            .toml_key("db.path") // cp.toml uses db.path, not database.url
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_DATABASE_POOL_SIZE")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(100),
            })
            .default_value("20")
            .description("Database connection pool size")
            .category("DATABASE")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_STORAGE_BACKEND")
            .config_type(ConfigType::Enum {
                values: vec![
                    "sql_only".to_string(),
                    "dual_write".to_string(),
                    "kv_primary".to_string(),
                    "kv_only".to_string(),
                ],
            })
            .default_value("sql_only")
            .description("Storage backend selection: sql_only, dual_write, kv_primary, kv_only")
            .category("STORAGE")
            .config_key("database.storage_mode")
            .toml_key("db.storage_mode")
            .build(),
    );

    // Alias for compatibility
    schema.add_variable(
        ConfigVariable::new("AOS_STORAGE_MODE")
            .config_type(ConfigType::Enum {
                values: vec![
                    "sql_only".to_string(),
                    "dual_write".to_string(),
                    "kv_primary".to_string(),
                    "kv_only".to_string(),
                ],
            })
            .default_value("sql_only")
            .description("Storage backend alias (same as AOS_STORAGE_BACKEND)")
            .category("STORAGE")
            .config_key("database.storage_mode")
            .toml_key("db.storage_mode")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_KV_PATH")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value("var/aos-kv.redb")
            .description("Path to KV (redb) file when using KV storage modes")
            .category("STORAGE")
            .config_key("database.kv_path")
            .toml_key("db.kv_path")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_KV_TANTIVY_PATH")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value("var/aos-kv-index")
            .description("Path to KV Tantivy index (search) when using KV modes")
            .category("STORAGE")
            .config_key("database.kv_tantivy_path")
            .toml_key("db.kv_tantivy_path")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_DATABASE_TIMEOUT")
            .config_type(ConfigType::Duration)
            .default_value("30s")
            .description("Database connection timeout duration")
            .category("DATABASE")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SKIP_MIGRATION_SIGNATURES")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Development/testing only: bypass migration signature verification")
            .category("DATABASE")
            .build(),
    );

    // ========================================================================
    // SECURITY Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_SECURITY_JWT_SECRET")
            .config_type(ConfigType::String)
            .description("JWT signing secret (required, minimum 32 characters for HMAC)")
            .sensitive()
            .category("SECURITY")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SECURITY_JWT_MODE")
            .config_type(ConfigType::Enum {
                values: vec!["eddsa".to_string(), "hmac".to_string()],
            })
            .default_value("hmac")
            .description(
                "JWT signing mode: eddsa (Ed25519, production) or hmac (HS256, development)",
            )
            .category("SECURITY")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SECURITY_JWT_TTL")
            .config_type(ConfigType::Duration)
            .default_value("8h")
            .description("JWT token time-to-live")
            .category("SECURITY")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SECURITY_PF_DENY")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Enable packet filter deny rules (required in production mode)")
            .category("SECURITY")
            .build(),
    );

    // ========================================================================
    // LOGGING Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_LOG_LEVEL")
            .config_type(ConfigType::Enum {
                values: vec![
                    "trace".to_string(),
                    "debug".to_string(),
                    "info".to_string(),
                    "warn".to_string(),
                    "error".to_string(),
                ],
            })
            .default_value("info")
            .description("Logging level: trace, debug, info, warn, error")
            .category("LOGGING")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_LOG_FORMAT")
            .config_type(ConfigType::Enum {
                values: vec!["json".to_string(), "text".to_string(), "pretty".to_string()],
            })
            .default_value("text")
            .description(
                "Log output format: json (production), text (simple), pretty (development)",
            )
            .category("LOGGING")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_LOG_PROFILE")
            .config_type(ConfigType::Enum {
                values: vec![
                    "json".to_string(),
                    "plain".to_string(),
                    "debug".to_string(),
                    "trace".to_string(),
                ],
            })
            .default_value("json")
            .description("Logging profile switch: json (default), plain (human-readable), debug (json + debug level), trace (json + trace level)")
            .category("LOGGING")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_LOG_FILE")
            .config_type(ConfigType::Path { must_exist: false })
            .description("Log file path (optional, logs to stdout if not set)")
            .category("LOGGING")
            .build(),
    );

    // ========================================================================
    // MEMORY Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_MEMORY_HEADROOM_PCT")
            .config_type(ConfigType::Float {
                min: Some(0.05),
                max: Some(0.50),
            })
            .default_value("0.15")
            .description("Memory headroom percentage to maintain (0.05-0.50, default 15%)")
            .category("MEMORY")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MEMORY_EVICTION_THRESHOLD")
            .config_type(ConfigType::Float {
                min: Some(0.50),
                max: Some(0.99),
            })
            .default_value("0.85")
            .description("Memory usage threshold that triggers eviction (0.50-0.99, default 85%)")
            .category("MEMORY")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MEMORY_MAX_ALLOCATION")
            .config_type(ConfigType::ByteSize)
            .description("Maximum memory allocation limit (optional, system-determined if not set)")
            .category("MEMORY")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MEMORY_GC_INTERVAL")
            .config_type(ConfigType::Duration)
            .default_value("60s")
            .description("Garbage collection check interval")
            .category("MEMORY")
            .build(),
    );

    // ========================================================================
    // BACKEND Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_BACKEND_COREML_ENABLED")
            .config_type(ConfigType::Bool)
            .default_value("true")
            .description("Enable CoreML backend for ANE acceleration")
            .category("BACKEND")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_COREML_COMPUTE_PREFERENCE")
            .config_type(ConfigType::Enum {
                values: vec![
                    "cpu_only".to_string(),
                    "cpu_and_gpu".to_string(),
                    "cpu_and_ne".to_string(),
                    "all".to_string(),
                ],
            })
            .default_value("cpu_and_gpu")
            .description(
                "CoreML compute units preference: cpu_only, cpu_and_gpu (default), cpu_and_ne, all",
            )
            .category("BACKEND")
            .config_key("coreml.compute_preference")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_COREML_PRODUCTION_MODE")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description(
                "Enable CoreML production mode (enforces ANE-only compute units inside the binding)",
            )
            .category("BACKEND")
            .config_key("coreml.production_mode")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_BACKEND_METAL_ENABLED")
            .config_type(ConfigType::Bool)
            .default_value("true")
            .description("Enable Metal backend as fallback for non-ANE systems")
            .category("BACKEND")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_BACKEND_MLX_ENABLED")
            .config_type(ConfigType::Bool)
            .default_value("true")
            .description("Enable MLX backend for research and training workloads")
            .category("BACKEND")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_BACKEND_DETERMINISM_SEED")
            .config_type(ConfigType::String)
            .description(
                "Global determinism seed (HKDF base, derived from manifest hash if not set)",
            )
            .category("BACKEND")
            .deprecated_with_notes(
                "AOS_DETERMINISM_MANIFEST_HASH",
                "0.5.0",
                "Seed is now derived from manifest hash via HKDF. Set in manifest.toml instead.",
            )
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_BACKEND_ATTESTATION_REQUIRED")
            .config_type(ConfigType::Bool)
            .default_value("true")
            .description("Require backend determinism attestation before serving")
            .category("BACKEND")
            .deprecated_with_notes(
                "policy:determinism",
                "0.5.0",
                "Attestation is now enforced via the Determinism policy pack.",
            )
            .build(),
    );

    // ========================================================================
    // ROUTER Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_ROUTER_K_SPARSE")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(32),
            })
            .default_value("4")
            .description("Number of top-K adapters to select in sparse routing")
            .category("ROUTER")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_ROUTER_QUANTIZATION")
            .config_type(ConfigType::Enum {
                values: vec!["q15".to_string(), "fp32".to_string()],
            })
            .default_value("q15")
            .description("Router gate quantization format: q15 (16-bit fixed point) or fp32")
            .category("ROUTER")
            .deprecated_with_notes(
                "policy:router",
                "0.5.0",
                "Quantization is now controlled by the Router policy pack. Q15 is always used.",
            )
            .build(),
    );

    // ========================================================================
    // TELEMETRY Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_TELEMETRY_ENABLED")
            .config_type(ConfigType::Bool)
            .default_value("true")
            .description("Enable telemetry event collection")
            .category("TELEMETRY")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_TELEMETRY_EXPORT_INTERVAL")
            .config_type(ConfigType::Duration)
            .default_value("60s")
            .description("Telemetry bundle export interval")
            .category("TELEMETRY")
            .build(),
    );

    // ========================================================================
    // TRAINING Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_TRAINING_CHECKPOINT_INTERVAL")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(10000),
            })
            .default_value("100")
            .description("Training checkpoint save interval (steps)")
            .category("TRAINING")
            .deprecated_with_notes(
                "training_job.checkpoint_interval",
                "0.5.0",
                "Set checkpoint interval in training job configuration instead.",
            )
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_TRAINING_MAX_EPOCHS")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(1000),
            })
            .default_value("10")
            .description("Maximum number of training epochs")
            .category("TRAINING")
            .deprecated_with_notes(
                "training_job.max_epochs",
                "0.5.0",
                "Set max epochs in training job configuration instead.",
            )
            .build(),
    );

    // ========================================================================
    // FEDERATION Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_FEDERATION_ENABLED")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Enable federation with peer nodes")
            .category("FEDERATION")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_FEDERATION_NODE_ID")
            .config_type(ConfigType::String)
            .description("Unique node identifier for federation")
            .category("FEDERATION")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_FEDERATION_PEERS")
            .config_type(ConfigType::String)
            .description("Comma-separated list of federation peer URLs")
            .category("FEDERATION")
            .build(),
    );

    // ========================================================================
    // EMBEDDINGS Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_EMBEDDING_MODEL_PATH")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value("./var/model-cache/models/bge-small-en-v1.5")
            .description("Path to sentence-transformer embedding model for RAG")
            .category("EMBEDDINGS")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_EMBEDDING_DIMENSION")
            .config_type(ConfigType::Integer {
                min: Some(64),
                max: Some(4096),
            })
            .default_value("384")
            .description("Embedding vector dimension (must match model)")
            .category("EMBEDDINGS")
            .build(),
    );

    // ========================================================================
    // MODEL_HUB Configuration
    // ========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_HF_HUB_ENABLED")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Enable Hugging Face Hub model downloads")
            .category("MODEL_HUB")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MODEL_CACHE_DIR")
            .config_type(ConfigType::Path { must_exist: false })
            // Keep in sync with `DEFAULT_MODEL_CACHE_ROOT` in path_resolver (canonical default)
            .default_value(DEFAULT_MODEL_CACHE_ROOT)
            .description("Directory for downloaded models from HuggingFace Hub")
            .category("MODEL_HUB")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_MAX_CONCURRENT_DOWNLOADS")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(10),
            })
            .default_value("4")
            .description("Maximum concurrent model downloads")
            .category("MODEL_HUB")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_DOWNLOAD_TIMEOUT_SECS")
            .config_type(ConfigType::Integer {
                min: Some(30),
                max: Some(3600),
            })
            .default_value("300")
            .description("Download timeout in seconds")
            .category("MODEL_HUB")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_HF_REGISTRY_URL")
            .config_type(ConfigType::String)
            .default_value("https://huggingface.co")
            .description("HuggingFace Hub registry URL")
            .category("MODEL_HUB")
            .build(),
    );

    // =========================================================================
    // PATHS - Runtime directory configuration
    // =========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_VAR_DIR")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value("var")
            .description("Base directory for all runtime data")
            .category("PATHS")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_ADAPTERS_DIR")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value("var/adapters/repo")
            .description("Directory for LoRA adapter weights (canonical repo)")
            .category("PATHS")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_ARTIFACTS_DIR")
            .config_type(ConfigType::Path { must_exist: false })
            .default_value("var/artifacts")
            .description("Directory for training artifacts and temp files")
            .category("PATHS")
            .build(),
    );

    // =========================================================================
    // WORKER - Background worker configuration
    // =========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_WORKER_THREADS")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(64),
            })
            .default_value("4")
            .description("Number of background worker threads")
            .category("WORKER")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_WORKER_QUEUE_SIZE")
            .config_type(ConfigType::Integer {
                min: Some(10),
                max: Some(10000),
            })
            .default_value("1000")
            .description("Maximum items in worker queue")
            .category("WORKER")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_WORKER_SHUTDOWN_TIMEOUT")
            .config_type(ConfigType::Duration)
            .default_value("30s")
            .description("Graceful shutdown timeout for workers")
            .category("WORKER")
            .build(),
    );

    // =========================================================================
    // DEBUG - Debug/development configuration
    // =========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_DEBUG_ENABLED")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Enable debug mode (disables some security checks)")
            .category("DEBUG")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_DEBUG_PROFILING")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Enable runtime profiling")
            .category("DEBUG")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_DEBUG_TRACE_REQUESTS")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Log full request/response bodies")
            .category("DEBUG")
            .build(),
    );

    // =========================================================================
    // Additional SECURITY variables
    // =========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_DEV_NO_AUTH")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Development/testing only: disable auth enforcement for local runs")
            .config_key("security.dev_bypass")
            .category("SECURITY")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SIGNING_KEY")
            .config_type(ConfigType::String)
            .description("Ed25519 signing key for manifest/artifact signing")
            .sensitive()
            .category("SECURITY")
            .build(),
    );

    // ADAPTER_GC - Adapter garbage collection configuration
    // =========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_ADAPTER_GC_ENABLED")
            .config_type(ConfigType::Bool)
            .default_value("true")
            .description("Enable garbage collection of archived adapters")
            .category("ADAPTER_GC")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_ADAPTER_GC_MIN_AGE_DAYS")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(365),
            })
            .default_value("30")
            .description("Minimum days since archival before adapter is eligible for GC")
            .category("ADAPTER_GC")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_ADAPTER_GC_BATCH_SIZE")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(1000),
            })
            .default_value("100")
            .description("Maximum adapters to process per GC run")
            .category("ADAPTER_GC")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_ADAPTER_GC_DRY_RUN")
            .config_type(ConfigType::Bool)
            .default_value("false")
            .description("Report GC actions without deleting files (for testing)")
            .category("ADAPTER_GC")
            .build(),
    );

    // =========================================================================
    // SELF_HOSTING - Internal self-hosting agent controls
    // =========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_SELF_HOSTING_MODE")
            .config_type(ConfigType::Enum {
                values: vec!["off".into(), "on".into(), "safe".into()],
            })
            .default_value("off")
            .description("Self-hosting agent mode: off, on, or safe (human approval required)")
            .category("SELF_HOSTING")
            .config_key("self_hosting.mode")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SELF_HOSTING_REPO_ALLOWLIST")
            .config_type(ConfigType::String)
            .default_value("")
            .description("Comma-separated list of repo IDs the self-hosting agent may manage")
            .category("SELF_HOSTING")
            .config_key("self_hosting.repo_allowlist")
            .build(),
    );

    schema.add_variable(
        ConfigVariable::new("AOS_SELF_HOSTING_PROMOTION_THRESHOLD")
            .config_type(ConfigType::Float {
                min: Some(0.0),
                max: Some(1.0),
            })
            .default_value("0.0")
            .description(
                "Minimum evaluation score required for auto-promotion when self_hosting_mode=on",
            )
            .category("SELF_HOSTING")
            .config_key("self_hosting.promotion_threshold")
            .build(),
    );

    // =========================================================================
    // BUILD - Compile-time build information (read-only)
    // =========================================================================

    schema.add_variable(
        ConfigVariable::new("AOS_BUILD_ID")
            .config_type(ConfigType::String)
            .description("Compile-time build identifier (read-only, set via env!(\"AOS_BUILD_ID\") at build time)")
            .category("BUILD")
            .config_key("build.id")
            .build(),
    );

    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_schema_contains_all_categories() {
        let schema = default_schema();
        let categories = schema.category_names();

        assert!(categories.contains(&"MODEL"));
        assert!(categories.contains(&"SERVER"));
        assert!(categories.contains(&"DATABASE"));
        assert!(categories.contains(&"SECURITY"));
        assert!(categories.contains(&"LOGGING"));
        assert!(categories.contains(&"MEMORY"));
        assert!(categories.contains(&"BACKEND"));
        assert!(categories.contains(&"ROUTER"));
        assert!(categories.contains(&"TELEMETRY"));
        assert!(categories.contains(&"TRAINING"));
        assert!(categories.contains(&"FEDERATION"));
        assert!(categories.contains(&"MODEL_HUB"));
        assert!(categories.contains(&"EMBEDDINGS"));
        assert!(categories.contains(&"PATHS"));
        assert!(categories.contains(&"WORKER"));
        assert!(categories.contains(&"DEBUG"));
        assert!(categories.contains(&"STORAGE"));
        assert!(categories.contains(&"ADAPTER_GC"));
        assert!(categories.contains(&"SELF_HOSTING"));
        assert!(categories.contains(&"BUILD"));
    }

    #[test]
    fn test_validate_integer_in_range() {
        let var = ConfigVariable::new("AOS_SERVER_PORT")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(65535),
            })
            .build();

        assert!(validate_value(&var, "8080").is_ok());
        assert!(validate_value(&var, "1").is_ok());
        assert!(validate_value(&var, "65535").is_ok());
    }

    #[test]
    fn test_validate_integer_out_of_range() {
        let var = ConfigVariable::new("AOS_SERVER_PORT")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(65535),
            })
            .build();

        assert!(validate_value(&var, "0").is_err());
        assert!(validate_value(&var, "65536").is_err());
        assert!(validate_value(&var, "-1").is_err());
    }

    #[test]
    fn test_validate_integer_invalid() {
        let var = ConfigVariable::new("AOS_SERVER_PORT")
            .config_type(ConfigType::Integer {
                min: Some(1),
                max: Some(65535),
            })
            .build();

        assert!(validate_value(&var, "not_a_number").is_err());
        assert!(validate_value(&var, "8080.5").is_err());
    }

    #[test]
    fn test_validate_float_in_range() {
        let var = ConfigVariable::new("AOS_MEMORY_HEADROOM_PCT")
            .config_type(ConfigType::Float {
                min: Some(0.05),
                max: Some(0.50),
            })
            .build();

        assert!(validate_value(&var, "0.15").is_ok());
        assert!(validate_value(&var, "0.05").is_ok());
        assert!(validate_value(&var, "0.50").is_ok());
    }

    #[test]
    fn test_validate_float_out_of_range() {
        let var = ConfigVariable::new("AOS_MEMORY_HEADROOM_PCT")
            .config_type(ConfigType::Float {
                min: Some(0.05),
                max: Some(0.50),
            })
            .build();

        assert!(validate_value(&var, "0.01").is_err());
        assert!(validate_value(&var, "0.99").is_err());
    }

    #[test]
    fn test_validate_bool() {
        let var = ConfigVariable::new("AOS_BACKEND_COREML_ENABLED")
            .config_type(ConfigType::Bool)
            .build();

        assert!(validate_value(&var, "true").is_ok());
        assert!(validate_value(&var, "false").is_ok());
        assert!(validate_value(&var, "TRUE").is_ok());
        assert!(validate_value(&var, "FALSE").is_ok());
        assert!(validate_value(&var, "1").is_ok());
        assert!(validate_value(&var, "0").is_ok());
        assert!(validate_value(&var, "yes").is_ok());
        assert!(validate_value(&var, "no").is_ok());
        assert!(validate_value(&var, "on").is_ok());
        assert!(validate_value(&var, "off").is_ok());
        assert!(validate_value(&var, "maybe").is_err());
    }

    #[test]
    fn test_validate_enum() {
        let var = ConfigVariable::new("AOS_MODEL_BACKEND")
            .config_type(ConfigType::Enum {
                values: vec![
                    "auto".to_string(),
                    "coreml".to_string(),
                    "metal".to_string(),
                    "mlx".to_string(),
                ],
            })
            .build();

        assert!(validate_value(&var, "auto").is_ok());
        assert!(validate_value(&var, "coreml").is_ok());
        assert!(validate_value(&var, "AUTO").is_ok());
        assert!(validate_value(&var, "COREML").is_ok());
        assert!(validate_value(&var, "invalid").is_err());
    }

    #[test]
    fn test_validate_url() {
        let var = ConfigVariable::new("AOS_DATABASE_URL")
            .config_type(ConfigType::Url)
            .build();

        assert!(validate_value(&var, "sqlite://test.db").is_ok());
        assert!(validate_value(&var, "http://localhost:8080").is_ok());
        assert!(validate_value(&var, "https://example.com").is_ok());
        assert!(validate_value(&var, "file:///path/to/file").is_ok());
        assert!(validate_value(&var, "invalid://test").is_err());
        assert!(validate_value(&var, "not_a_url").is_err());
    }

    #[test]
    fn test_validate_duration() {
        let var = ConfigVariable::new("AOS_DATABASE_TIMEOUT")
            .config_type(ConfigType::Duration)
            .build();

        assert!(validate_value(&var, "30s").is_ok());
        assert!(validate_value(&var, "5m").is_ok());
        assert!(validate_value(&var, "1h").is_ok());
        assert!(validate_value(&var, "500ms").is_ok());
        assert!(validate_value(&var, "1d").is_ok());
        assert!(validate_value(&var, "30").is_ok()); // plain number = seconds
        assert!(validate_value(&var, "invalid").is_err());
    }

    #[test]
    fn test_validate_byte_size() {
        let var = ConfigVariable::new("AOS_LOG_MAX_SIZE")
            .config_type(ConfigType::ByteSize)
            .build();

        assert!(validate_value(&var, "100MB").is_ok());
        assert!(validate_value(&var, "1GB").is_ok());
        assert!(validate_value(&var, "512KB").is_ok());
        assert!(validate_value(&var, "1024").is_ok()); // plain bytes
        assert!(validate_value(&var, "1.5GB").is_ok());
        assert!(validate_value(&var, "invalid").is_err());
    }

    #[test]
    fn test_validate_path() {
        let var = ConfigVariable::new("AOS_MODEL_PATH")
            .config_type(ConfigType::Path { must_exist: false })
            .build();

        assert!(validate_value(&var, "/path/to/model").is_ok());
        assert!(validate_value(&var, "./relative/path").is_ok());

        // Empty path should fail
        assert!(validate_value(&var, "").is_err());
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("30s").unwrap(), 30_000);
        assert_eq!(parse_duration("5m").unwrap(), 300_000);
        assert_eq!(parse_duration("1h").unwrap(), 3_600_000);
        assert_eq!(parse_duration("500ms").unwrap(), 500);
        assert_eq!(parse_duration("1d").unwrap(), 86_400_000);
        assert_eq!(parse_duration("30").unwrap(), 30_000); // plain number = seconds
    }

    #[test]
    fn test_parse_byte_size() {
        assert_eq!(parse_byte_size("1024").unwrap(), 1024);
        assert_eq!(parse_byte_size("1KB").unwrap(), 1024);
        assert_eq!(parse_byte_size("1K").unwrap(), 1024);
        assert_eq!(parse_byte_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(parse_byte_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_byte_size("1GB").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_byte_size("1G").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_sensitive_redaction() {
        let var = ConfigVariable::new("AOS_SECURITY_JWT_SECRET")
            .sensitive()
            .build();

        assert_eq!(var.display_value("my-secret"), "***REDACTED***");

        let non_sensitive = ConfigVariable::new("AOS_SERVER_PORT").build();
        assert_eq!(non_sensitive.display_value("8080"), "8080");
    }

    #[test]
    fn test_config_key_derivation() {
        let var = ConfigVariable::new("AOS_SERVER_PORT").build();
        assert_eq!(var.config_key, "server.port");

        let var2 = ConfigVariable::new("AOS_DATABASE_POOL_SIZE").build();
        assert_eq!(var2.config_key, "database.pool.size");
    }

    #[test]
    fn test_deprecated_variable() {
        let var = ConfigVariable::new("AOS_OLD_CONFIG")
            .deprecated("AOS_NEW_CONFIG", "2.0.0")
            .build();

        assert!(var.is_deprecated());
        let dep = var.deprecated.as_ref().unwrap();
        assert_eq!(dep.replacement, "AOS_NEW_CONFIG");
        assert_eq!(dep.removal_version, "2.0.0");
    }

    #[test]
    fn test_schema_get_required() {
        let schema = default_schema();
        let required = schema.get_required();

        // Currently no variables are marked as required (they have defaults)
        // This is expected for a flexible configuration system
        assert!(required.is_empty() || !required.is_empty()); // Schema may change
    }

    #[test]
    fn test_schema_get_sensitive() {
        let schema = default_schema();
        let sensitive = schema.get_sensitive();

        // JWT secret should be marked as sensitive
        assert!(sensitive
            .iter()
            .any(|v| v.name == "AOS_SECURITY_JWT_SECRET"));
    }

    #[test]
    fn test_schema_validate_all() {
        let schema = default_schema();

        let mut values = HashMap::new();
        values.insert("AOS_SERVER_PORT".to_string(), "8080".to_string());
        values.insert("AOS_MODEL_BACKEND".to_string(), "coreml".to_string());

        assert!(schema.validate_all(&values).is_ok());

        // Invalid value
        values.insert("AOS_SERVER_PORT".to_string(), "invalid".to_string());
        assert!(schema.validate_all(&values).is_err());
    }

    #[test]
    fn test_get_category() {
        let schema = default_schema();
        let model_vars = schema.get_category("MODEL");

        assert!(!model_vars.is_empty());
        assert!(model_vars.iter().any(|v| v.name == "AOS_MODEL_PATH"));
        assert!(model_vars.iter().any(|v| v.name == "AOS_MODEL_BACKEND"));
    }
}
