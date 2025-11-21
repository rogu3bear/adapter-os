//! Configuration types and structures

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration precedence levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PrecedenceLevel {
    /// Manifest file (lowest priority)
    Manifest = 0,
    /// Environment variables (medium priority)
    Environment = 1,
    /// CLI arguments (highest priority)
    Cli = 2,
}

/// Configuration source information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSource {
    pub level: PrecedenceLevel,
    pub source: String, // "manifest", "env", "cli"
    pub key: String,
    pub value: String,
}

/// Configuration metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigMetadata {
    pub frozen_at: String, // ISO timestamp
    pub hash: String,      // BLAKE3 hash of frozen config
    pub sources: Vec<ConfigSource>,
    pub manifest_path: Option<String>,
    pub cli_args: Vec<String>,
}

/// Configuration validation error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigValidationError {
    pub key: String,
    pub message: String,
    pub expected_type: String,
    pub actual_value: String,
}

/// Configuration freeze error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFreezeError {
    pub message: String,
    pub attempted_operation: String,
    pub stack_trace: Option<String>,
}

/// Feature flag definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    /// Unique identifier for the feature
    pub name: String,
    /// Whether the feature is enabled
    pub enabled: bool,
    /// Description of the feature
    pub description: Option<String>,
    /// Conditions for automatic enablement
    pub conditions: Option<FeatureFlagConditions>,
}

/// Conditions for automatic feature flag enablement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlagConditions {
    /// Enable only in specific environments
    pub environments: Option<Vec<String>>,
    /// Enable for specific tenant IDs
    pub tenant_ids: Option<Vec<String>>,
    /// Enable after a specific date (ISO 8601)
    pub enabled_after: Option<String>,
    /// Enable before a specific date (ISO 8601)
    pub enabled_before: Option<String>,
    /// Percentage rollout (0-100)
    pub rollout_percentage: Option<u8>,
}

impl Default for FeatureFlag {
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: false,
            description: None,
            conditions: None,
        }
    }
}

/// Configuration loader options
#[derive(Debug, Clone)]
pub struct LoaderOptions {
    pub strict_mode: bool,
    pub validate_types: bool,
    pub allow_unknown_keys: bool,
    pub env_prefix: String,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            strict_mode: true,
            validate_types: true,
            allow_unknown_keys: false,
            env_prefix: "ADAPTEROS_".to_string(),
        }
    }
}

/// Configuration schema definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub version: String,
    pub fields: HashMap<String, FieldDefinition>,
}

/// Field definition for configuration validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub field_type: String, // "string", "integer", "boolean", "float"
    pub required: bool,
    pub default_value: Option<String>,
    pub description: Option<String>,
    pub validation_rules: Option<Vec<String>>,
}

impl Default for ConfigSchema {
    fn default() -> Self {
        let mut fields = HashMap::new();

        // Core server configuration
        fields.insert(
            "server.host".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("127.0.0.1".to_string()),
                description: Some("Server bind address".to_string()),
                validation_rules: Some(vec!["ip_address".to_string()]),
            },
        );

        fields.insert(
            "server.port".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("8080".to_string()),
                description: Some("Server port number".to_string()),
                validation_rules: Some(vec!["range:1-65535".to_string()]),
            },
        );

        fields.insert(
            "server.workers".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("4".to_string()),
                description: Some("Number of worker threads".to_string()),
                validation_rules: Some(vec!["range:1-64".to_string()]),
            },
        );

        // Database configuration
        fields.insert(
            "database.url".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: true,
                default_value: None,
                description: Some("Database connection URL".to_string()),
                validation_rules: Some(vec!["url".to_string()]),
            },
        );

        fields.insert(
            "database.pool_size".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("10".to_string()),
                description: Some("Database connection pool size".to_string()),
                validation_rules: Some(vec!["range:1-100".to_string()]),
            },
        );

        // Policy configuration
        fields.insert(
            "policy.strict_mode".to_string(),
            FieldDefinition {
                field_type: "boolean".to_string(),
                required: false,
                default_value: Some("true".to_string()),
                description: Some("Enable strict policy enforcement".to_string()),
                validation_rules: None,
            },
        );

        fields.insert(
            "policy.audit_logging".to_string(),
            FieldDefinition {
                field_type: "boolean".to_string(),
                required: false,
                default_value: Some("true".to_string()),
                description: Some("Enable policy audit logging".to_string()),
                validation_rules: None,
            },
        );

        // Logging configuration
        fields.insert(
            "logging.level".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("info".to_string()),
                description: Some("Logging level".to_string()),
                validation_rules: Some(vec!["enum:debug,info,warn,error".to_string()]),
            },
        );

        fields.insert(
            "logging.format".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("json".to_string()),
                description: Some("Logging format".to_string()),
                validation_rules: Some(vec!["enum:json,text".to_string()]),
            },
        );

        Self {
            version: "1.0.0".to_string(),
            fields,
        }
    }
}
