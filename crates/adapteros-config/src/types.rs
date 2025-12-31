//! Configuration types and structures

use crate::path_resolver::DEV_MODEL_PATH;
use adapteros_core::defaults::DEFAULT_DB_PATH;
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Configuration loader options
#[derive(Debug, Clone)]
pub struct LoaderOptions {
    pub strict_mode: bool,
    pub validate_types: bool,
    pub allow_unknown_keys: bool,
    pub env_prefix: String,
    /// Fail if manifest_path is provided but file is missing or unreadable.
    /// When true, explicitly provided config paths are treated as required.
    pub require_manifest: bool,
    /// Fail on empty/whitespace environment variable overrides in production mode.
    /// When true, empty AOS_* env vars cause an error instead of being silently skipped.
    pub reject_empty_env_vars: bool,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            strict_mode: true,
            validate_types: true,
            allow_unknown_keys: false,
            env_prefix: "ADAPTEROS_".to_string(),
            require_manifest: true,
            reject_empty_env_vars: true,
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
                default_value: Some(DEFAULT_DB_PATH.to_string()),
                description: Some("Database connection URL".to_string()),
                validation_rules: Some(vec!["url".to_string()]),
            },
        );

        fields.insert(
            "database.pool_size".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("20".to_string()),
                description: Some("Database connection pool size".to_string()),
                validation_rules: Some(vec!["range:1-100".to_string()]),
            },
        );

        fields.insert(
            "database.storage_mode".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("sql_only".to_string()),
                description: Some(
                    "Storage mode: sql_only, dual_write, kv_primary, kv_only".to_string(),
                ),
                validation_rules: Some(vec![
                    "enum:sql_only,dual_write,kv_primary,kv_only".to_string()
                ]),
            },
        );

        fields.insert(
            "database.kv_path".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("var/aos-kv.redb".to_string()),
                description: Some("Path to KV (redb) file".to_string()),
                validation_rules: None,
            },
        );

        fields.insert(
            "database.kv_tantivy_path".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("var/aos-kv-index".to_string()),
                description: Some("Path to KV search index (Tantivy)".to_string()),
                validation_rules: None,
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

        // Authentication configuration
        fields.insert(
            "auth.dev_algo".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("hs256".to_string()),
                description: Some("JWT algorithm in development (hs256/hmac)".to_string()),
                validation_rules: Some(vec!["enum:hs256,hmac,eddsa,ed25519".to_string()]),
            },
        );

        fields.insert(
            "auth.prod_algo".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("eddsa".to_string()),
                description: Some("JWT algorithm in production (eddsa/ed25519)".to_string()),
                validation_rules: Some(vec!["enum:hs256,hmac,eddsa,ed25519".to_string()]),
            },
        );

        fields.insert(
            "auth.session_lifetime".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some((12 * 3600).to_string()),
                description: Some("Session lifetime in seconds".to_string()),
                validation_rules: Some(vec!["range:60-86400".to_string()]),
            },
        );

        fields.insert(
            "auth.lockout_threshold".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("5".to_string()),
                description: Some("Failed login attempts before lockout".to_string()),
                validation_rules: Some(vec!["range:1-100".to_string()]),
            },
        );

        fields.insert(
            "auth.lockout_cooldown".to_string(),
            FieldDefinition {
                field_type: "integer".to_string(),
                required: false,
                default_value: Some("300".to_string()),
                description: Some("Lockout cooldown in seconds".to_string()),
                validation_rules: Some(vec!["range:60-86400".to_string()]),
            },
        );

        // Model configuration
        fields.insert(
            "model.path".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some(DEV_MODEL_PATH.to_string()),
                description: Some("Path to the model directory".to_string()),
                validation_rules: None,
            },
        );

        fields.insert(
            "model.backend".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("mlx".to_string()),
                description: Some("Model backend selection".to_string()),
                validation_rules: Some(vec!["enum:auto,coreml,metal,mlx".to_string()]),
            },
        );

        fields.insert(
            "model.architecture".to_string(),
            FieldDefinition {
                field_type: "string".to_string(),
                required: false,
                default_value: Some("qwen2.5".to_string()),
                description: Some("Model architecture type".to_string()),
                validation_rules: None,
            },
        );

        Self {
            version: "1.0.0".to_string(),
            fields,
        }
    }
}
