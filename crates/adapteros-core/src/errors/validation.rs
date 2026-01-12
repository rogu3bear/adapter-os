//! Validation and parsing errors
//!
//! Covers input validation, manifest parsing, configuration, and serialization.

use thiserror::Error;

/// Validation and parsing errors
#[derive(Error, Debug)]
pub enum AosValidationError {
    /// Generic validation error
    #[error("Validation error: {0}")]
    Validation(String),

    /// Invalid manifest format or content
    #[error("Invalid manifest: {0}")]
    InvalidManifest(String),

    /// Parse error
    #[error("Parse error: {0}")]
    Parse(String),

    /// JSON serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Invalid CPID format
    #[error("Invalid CPID: {0}")]
    InvalidCPID(String),

    /// Chat template error
    #[error("Chat template error: {0}")]
    ChatTemplate(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Invalid input data
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Missing required version field in manifest
    #[error("Adapter version string is missing from metadata")]
    MissingVersion,

    /// Unknown fields in manifest that are not in the allowed list
    #[error("Adapter import contains unknown required fields: {0:?}")]
    UnknownManifestFields(Vec<String>),

    /// TTL timestamp is in the past
    #[error("Adapter pin TTL is in the past")]
    TtlInPast,

    /// Missing required SBOM artifacts for export
    #[error("Adapter export omits required artifacts: {0:?}")]
    MissingArtifacts(Vec<String>),

    // =========================================================================
    // Config file errors (Category 1)
    // =========================================================================
    /// Config file not found at expected path
    #[error("Config file not found: {path}")]
    ConfigFileNotFound {
        /// Path where config was expected
        path: String,
        /// List of locations that were searched
        tried_locations: Vec<String>,
    },

    /// Config file exists but cannot be read due to permissions
    #[error("Config file permission denied: {path} - {reason}")]
    ConfigFilePermissionDenied {
        /// Path to the config file
        path: String,
        /// Reason for the permission denial
        reason: String,
    },

    /// Config parses but contains invalid schema values
    #[error("Config schema violation: {field} = '{value}' - {constraint}")]
    ConfigSchemaViolation {
        /// The config field that failed validation
        field: String,
        /// The invalid value provided
        value: String,
        /// Description of the constraint that was violated
        constraint: String,
        /// Category of the config field (e.g., "database", "server", "auth")
        category: String,
    },

    /// Environment variable override with empty/whitespace value
    #[error(
        "Empty environment override (empty or whitespace): {variable} - set a value or unset the variable"
    )]
    EmptyEnvOverride {
        /// The environment variable name
        variable: String,
        /// The config key this variable maps to
        config_key: String,
    },

    /// Required secret is blank or contains placeholder value
    #[error("Invalid secret value for {variable}: {reason}")]
    BlankSecret {
        /// The environment variable or config key for the secret
        variable: String,
        /// Reason why the value is invalid (e.g., "blank", "whitespace", "placeholder")
        reason: String,
    },
}

impl From<serde_json::Error> for AosValidationError {
    fn from(err: serde_json::Error) -> Self {
        AosValidationError::Serialization(err.to_string())
    }
}
