//! Configuration management CLI commands
//!
//! Provides `aosctl config` subcommands for validation, migration, and display
//! of environment configuration per PRD-CONFIG-001.
//!
//! ## Commands
//!
//! - `aosctl config validate` - Validate configuration files and environment variables
//! - `aosctl config migrate` - Migrate legacy environment variables to new naming
//! - `aosctl config show` - Display effective configuration with source attribution
//!
//! ## Exit Codes
//!
//! - 0: Success / validation passed
//! - 1: Validation errors found
//! - 2: Configuration file not found or unreadable
//! - 3: Invalid command arguments
//! - 4: User cancelled (interactive mode)

use crate::output::OutputWriter;
use adapteros_core::{AosError, Result};
use chrono::Utc;
use clap::{Args, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use tracing::info;

// ============================================================================
// Constants
// ============================================================================

/// Legacy to new variable name mapping for migration
/// Format: (legacy_name, new_name)
pub const MIGRATION_MAP: &[(&str, &str)] = &[
    ("DATABASE_URL", "AOS_DATABASE_URL"),
    ("MLX_PATH", "AOS_MLX_PATH"),
    ("ADAPTEROS_SERVER_PORT", "AOS_SERVER_PORT"),
    ("ADAPTEROS_SERVER_HOST", "AOS_SERVER_HOST"),
    ("ADAPTEROS_DATABASE_URL", "AOS_DATABASE_URL"),
    ("ADAPTEROS_ENV", "AOS_ENVIRONMENT"),
    ("ADAPTEROS_KEYCHAIN_FALLBACK", "AOS_KEYCHAIN_FALLBACK"),
    ("AOS_DETERMINISTIC_DEBUG", "AOS_DEBUG_DETERMINISTIC"),
    (
        "AOS_SKIP_KERNEL_SIGNATURE_VERIFY",
        "AOS_DEBUG_SKIP_KERNEL_SIG",
    ),
    ("AOS_GPU_INDEX", "AOS_BACKEND_GPU_INDEX"),
    ("AOS_MLX_FFI_MODEL", "AOS_MODEL_PATH"),
];

/// Variables that should be redacted in output (contain sensitive data)
pub const SENSITIVE_VARS: &[&str] = &[
    "AOS_SECURITY_JWT_SECRET",
    "AOS_KEYCHAIN_FALLBACK",
    "AOS_KMS_ACCESS_KEY",
    "AOS_DATABASE_PASSWORD",
    "DATABASE_URL",
    "ADAPTEROS_DATABASE_URL",
    "AOS_DATABASE_URL",
    "JWT_SECRET",
];

/// Removal version for deprecated variables
const REMOVAL_VERSION: &str = "v0.03";

// ============================================================================
// Types
// ============================================================================

/// Configuration source origin
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    /// From CLI arguments
    Cli,
    /// From environment variable
    Env,
    /// From .env file
    EnvFile,
    /// From manifest file
    Manifest,
    /// Compiled default value
    Default,
}

impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigSource::Cli => write!(f, "cli"),
            ConfigSource::Env => write!(f, "env"),
            ConfigSource::EnvFile => write!(f, ".env"),
            ConfigSource::Manifest => write!(f, "manifest"),
            ConfigSource::Default => write!(f, "default"),
        }
    }
}

/// Validation status for a configuration variable
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    /// Variable is valid
    Valid,
    /// Variable uses deprecated name
    Deprecated,
    /// Variable has an invalid value
    Error,
    /// Variable is a warning (e.g., debug flag in production)
    Warning,
}

impl std::fmt::Display for ValidationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationStatus::Valid => write!(f, "valid"),
            ValidationStatus::Deprecated => write!(f, "deprecated"),
            ValidationStatus::Error => write!(f, "error"),
            ValidationStatus::Warning => write!(f, "warning"),
        }
    }
}

/// Result of validating a single variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Variable name
    pub name: String,
    /// Variable value (redacted if sensitive)
    pub value: String,
    /// Validation status
    pub status: ValidationStatus,
    /// Source of the value
    pub source: ConfigSource,
    /// Variable type (for JSON output)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub var_type: Option<String>,
    /// Replacement variable name (for deprecated vars)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    /// Version when deprecated variable will be removed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub removal_version: Option<String>,
    /// Error message (for invalid values)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Additional validation details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<HashMap<String, serde_json::Value>>,
}

impl ValidationResult {
    /// Check if this result represents an error
    pub fn is_error(&self) -> bool {
        matches!(self.status, ValidationStatus::Error)
    }

    /// Check if this result represents a warning
    pub fn is_warning(&self) -> bool {
        matches!(
            self.status,
            ValidationStatus::Warning | ValidationStatus::Deprecated
        )
    }
}

/// Result of migrating a single variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResult {
    /// Original variable name
    pub from: String,
    /// New variable name
    pub to: String,
    /// Variable value (redacted if sensitive)
    pub value: String,
    /// Migration status
    pub status: MigrationStatus,
}

/// Status of a migration operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    /// Successfully migrated
    Migrated,
    /// Skipped (user choice in interactive mode)
    Skipped,
    /// Kept both (user choice)
    KeptBoth,
    /// Conflict: both legacy and new exist
    Conflict,
}

/// Parsed line from .env file (preserves structure)
#[derive(Debug, Clone)]
pub enum EnvLine {
    /// Comment line (starts with #)
    Comment(String),
    /// Blank line
    Blank,
    /// Variable assignment
    Variable { name: String, value: String },
}

/// Output format for commands
#[derive(Debug, Clone, Copy, ValueEnum, Default)]
pub enum OutputFormat {
    /// Human-readable text
    #[default]
    Text,
    /// JSON format
    Json,
    /// SARIF format (for CI integration)
    Sarif,
    /// Diff format (for migrate command)
    Diff,
    /// Table format (for show command)
    Table,
    /// Env file format (for show command)
    Env,
}

/// Category filter for show command
#[derive(Debug, Clone, ValueEnum, Default)]
pub enum ConfigCategory {
    /// All categories
    #[default]
    All,
    /// Model configuration
    Model,
    /// Server configuration
    Server,
    /// Database configuration
    Database,
    /// Security configuration
    Security,
    /// Logging configuration
    Logging,
    /// Telemetry configuration
    Telemetry,
    /// Memory management
    Memory,
    /// Backend configuration
    Backend,
    /// Federation settings
    Federation,
    /// Debug flags
    Debug,
}

// ============================================================================
// Command Definitions
// ============================================================================

/// Configuration management commands
#[derive(Debug, Clone, Subcommand)]
pub enum ConfigCommand {
    /// Validate configuration files and environment variables
    #[command(
        after_help = "Examples:\n  aosctl config validate\n  aosctl config validate --env-file /etc/aos/production.env\n  aosctl config validate --production --strict --format json\n  aosctl config validate --production || exit 1"
    )]
    Validate(ValidateArgs),

    /// Migrate legacy environment variables to new naming
    #[command(
        after_help = "Examples:\n  aosctl config migrate --dry-run\n  aosctl config migrate --backup\n  aosctl config migrate --input .env.old --output .env.new\n  aosctl config migrate --dry-run --format diff > migration.patch\n  aosctl config migrate --interactive"
    )]
    Migrate(MigrateArgs),

    /// Show effective configuration with source attribution
    #[command(
        after_help = "Examples:\n  aosctl config show\n  aosctl config show --category model\n  aosctl config show --format env > exported.env\n  aosctl config show --format json | jq '.model.path'"
    )]
    Show(ShowArgs),
}

/// Type alias for main.rs integration
pub type ConfigArgs = ConfigCommand;

/// Arguments for the validate command
#[derive(Debug, Clone, Args)]
pub struct ValidateArgs {
    /// Path to .env file to validate
    #[arg(short = 'e', long, default_value = ".env")]
    pub env_file: PathBuf,

    /// Fail on deprecation warnings, not just errors
    #[arg(short = 's', long)]
    pub strict: bool,

    /// Validate against production requirements
    #[arg(short = 'p', long)]
    pub production: bool,

    /// Output format: text, json, sarif
    #[arg(short = 'f', long, value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Only output errors, suppress info/warnings
    #[arg(short = 'q', long)]
    pub quiet: bool,

    /// Optional manifest file to validate against
    #[arg(short = 'm', long)]
    pub manifest: Option<PathBuf>,
}

/// Arguments for the migrate command
#[derive(Debug, Clone, Args)]
pub struct MigrateArgs {
    /// Source .env file to migrate
    #[arg(short = 'i', long, default_value = ".env")]
    pub input: PathBuf,

    /// Destination file (use "-" for stdout)
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,

    /// Show changes without writing
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Create .env.backup before writing
    #[arg(short = 'b', long, default_value = "true")]
    pub backup: bool,

    /// Skip backup creation
    #[arg(long)]
    pub no_backup: bool,

    /// Output format: text, json, diff
    #[arg(short = 'f', long, value_enum, default_value = "text")]
    pub format: OutputFormat,

    /// Prompt for each migration decision
    #[arg(long)]
    pub interactive: bool,

    /// Remove deprecated vars after migration
    #[arg(long)]
    pub remove_deprecated: bool,
}

/// Arguments for the show command
#[derive(Debug, Clone, Args)]
pub struct ShowArgs {
    /// Output format: table, json, env
    #[arg(short = 'f', long, value_enum, default_value = "table")]
    pub format: OutputFormat,

    /// Filter by category
    #[arg(short = 'c', long, value_enum, default_value = "all")]
    pub category: ConfigCategory,

    /// Include default values
    #[arg(long)]
    pub show_defaults: bool,

    /// Include unset optional variables
    #[arg(long)]
    pub show_unset: bool,

    /// Show sensitive values (requires confirmation)
    #[arg(long)]
    pub no_redact: bool,
}

// ============================================================================
// Main Entry Point
// ============================================================================

/// Execute a config subcommand
pub async fn run_config_command(cmd: ConfigCommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        ConfigCommand::Validate(args) => validate(args, output).await,
        ConfigCommand::Migrate(args) => migrate(args, output).await,
        ConfigCommand::Show(args) => show(args, output).await,
    }
}

// ============================================================================
// Validate Command Implementation
// ============================================================================

/// Validate configuration files and environment variables
pub async fn validate(args: ValidateArgs, output: &OutputWriter) -> Result<()> {
    info!(env_file = ?args.env_file, production = args.production, "Validating configuration");

    let env_file_exists = args.env_file.exists();
    if !env_file_exists && args.env_file.to_string_lossy() != ".env" {
        output.error(&format!(
            "Configuration file not found: {}",
            args.env_file.display()
        ));
        std::process::exit(2);
    }

    let file_vars = if env_file_exists {
        parse_env_file(&args.env_file)?
    } else {
        HashMap::new()
    };

    let env_vars: HashMap<String, String> = std::env::vars().collect();

    let mut all_vars = file_vars.clone();
    for (k, v) in &env_vars {
        if k.starts_with("AOS_")
            || k.starts_with("ADAPTEROS_")
            || MIGRATION_MAP.iter().any(|(legacy, _)| legacy == k)
        {
            all_vars.insert(k.clone(), v.clone());
        }
    }

    let mut results = Vec::new();
    for (name, value) in &all_vars {
        let source = if env_vars.contains_key(name) && file_vars.get(name) != Some(value) {
            ConfigSource::Env
        } else if file_vars.contains_key(name) {
            ConfigSource::EnvFile
        } else {
            ConfigSource::Env
        };

        let result = validate_variable(name, value, source);
        results.push(result);
    }

    if args.production {
        let production_results = validate_production_requirements(&all_vars);
        results.extend(production_results);
    }

    let required_checks = validate_required_variables(&all_vars, args.production);
    results.extend(required_checks);

    let valid_count = results
        .iter()
        .filter(|r| matches!(r.status, ValidationStatus::Valid))
        .count();
    let warning_count = results
        .iter()
        .filter(|r| {
            matches!(
                r.status,
                ValidationStatus::Warning | ValidationStatus::Deprecated
            )
        })
        .count();
    let error_count = results
        .iter()
        .filter(|r| matches!(r.status, ValidationStatus::Error))
        .count();

    let passed = if args.strict {
        error_count == 0 && warning_count == 0
    } else {
        error_count == 0
    };

    match args.format {
        OutputFormat::Json => output_validation_json(
            &args,
            &results,
            valid_count,
            warning_count,
            error_count,
            passed,
        )?,
        OutputFormat::Sarif => output_validation_sarif(&args, &results)?,
        _ => output_validation_text(
            &args,
            &results,
            valid_count,
            warning_count,
            error_count,
            passed,
            output,
        )?,
    }

    if !passed {
        std::process::exit(1);
    }

    Ok(())
}

fn validate_variable(name: &str, value: &str, source: ConfigSource) -> ValidationResult {
    let is_sensitive = SENSITIVE_VARS.contains(&name);
    let display_value = if is_sensitive {
        "***REDACTED***".to_string()
    } else {
        value.to_string()
    };

    if let Some((_, new_name)) = MIGRATION_MAP.iter().find(|(legacy, _)| *legacy == name) {
        return ValidationResult {
            name: name.to_string(),
            value: display_value,
            status: ValidationStatus::Deprecated,
            source,
            var_type: None,
            replacement: Some(new_name.to_string()),
            removal_version: Some(REMOVAL_VERSION.to_string()),
            error: None,
            validation: None,
        };
    }

    let validation_result = match name {
        "AOS_MODEL_PATH" => validate_path(value, true),
        "AOS_MODEL_BACKEND" => validate_enum(value, &["auto", "coreml", "metal", "mlx"]),
        "AOS_MODEL_ARCHITECTURE" => Ok(()),
        "AOS_SERVER_HOST" | "ADAPTEROS_SERVER_HOST" => Ok(()),
        "AOS_SERVER_PORT" | "ADAPTEROS_SERVER_PORT" => validate_port(value),
        "AOS_SERVER_WORKERS" => validate_integer(value, 1, 1024),
        "AOS_SERVER_TIMEOUT" => validate_duration(value),
        "AOS_SERVER_UDS_SOCKET" => validate_path(value, false),
        "AOS_DATABASE_URL" | "DATABASE_URL" | "ADAPTEROS_DATABASE_URL" => {
            validate_database_url(value)
        }
        "AOS_DATABASE_POOL_SIZE" => validate_integer(value, 1, 100),
        "AOS_DATABASE_TIMEOUT" => validate_duration(value),
        "AOS_SECURITY_JWT_SECRET" => validate_jwt_secret(value),
        "AOS_SECURITY_JWT_MODE" => validate_enum(value, &["eddsa", "hs256", "rs256"]),
        "AOS_SECURITY_PF_DENY" => validate_bool(value),
        "AOS_LOG_LEVEL" | "RUST_LOG" => Ok(()),
        "AOS_LOG_FORMAT" => validate_enum(value, &["json", "text", "pretty"]),
        "AOS_LOG_FILE" => validate_path(value, false),
        "AOS_MEMORY_HEADROOM_PCT" => validate_float(value, 0.0, 1.0),
        "AOS_MEMORY_EVICTION_THRESHOLD" => validate_float(value, 0.0, 1.0),
        "AOS_BACKEND_COREML_ENABLED" | "AOS_BACKEND_METAL_ENABLED" | "AOS_BACKEND_MLX_ENABLED" => {
            validate_bool(value)
        }
        "AOS_BACKEND_GPU_INDEX" | "AOS_GPU_INDEX" => validate_integer(value, 0, 16),
        "AOS_DEBUG_DETERMINISTIC" | "AOS_DETERMINISTIC_DEBUG" => validate_bool(value),
        "AOS_DEBUG_TRACE_FFI" | "AOS_DEBUG_VERBOSE" => validate_bool(value),
        "AOS_DEBUG_SKIP_KERNEL_SIG" | "AOS_SKIP_KERNEL_SIGNATURE_VERIFY" => validate_bool(value),
        "AOS_ENVIRONMENT" | "ADAPTEROS_ENV" => {
            validate_enum(value, &["development", "staging", "production"])
        }
        "AOS_FEDERATION_ENABLED" => validate_bool(value),
        "AOS_FEDERATION_PEERS" => Ok(()),
        "AOS_TELEMETRY_ENABLED" => validate_bool(value),
        "AOS_TELEMETRY_ENDPOINT" => validate_url(value),
        "AOS_MLX_PATH" | "MLX_PATH" => validate_path(value, true),
        "AOS_KEYCHAIN_FALLBACK" | "ADAPTEROS_KEYCHAIN_FALLBACK" => Ok(()),
        _ => Ok(()),
    };

    match validation_result {
        Ok(()) => ValidationResult {
            name: name.to_string(),
            value: display_value,
            status: ValidationStatus::Valid,
            source,
            var_type: Some(get_var_type(name)),
            replacement: None,
            removal_version: None,
            error: None,
            validation: None,
        },
        Err(error_msg) => ValidationResult {
            name: name.to_string(),
            value: display_value,
            status: ValidationStatus::Error,
            source,
            var_type: Some(get_var_type(name)),
            replacement: None,
            removal_version: None,
            error: Some(error_msg),
            validation: None,
        },
    }
}

fn validate_production_requirements(vars: &HashMap<String, String>) -> Vec<ValidationResult> {
    let mut results = Vec::new();

    if !vars.contains_key("AOS_SERVER_UDS_SOCKET") {
        results.push(ValidationResult {
            name: "AOS_SERVER_UDS_SOCKET".to_string(),
            value: "(unset)".to_string(),
            status: ValidationStatus::Error,
            source: ConfigSource::Default,
            var_type: Some("path".to_string()),
            replacement: None,
            removal_version: None,
            error: Some("Production mode requires UDS socket (AOS_SERVER_UDS_SOCKET)".to_string()),
            validation: None,
        });
    }

    match vars.get("AOS_SECURITY_JWT_MODE") {
        Some(mode) if mode != "eddsa" => {
            results.push(ValidationResult {
                name: "AOS_SECURITY_JWT_MODE".to_string(),
                value: mode.clone(),
                status: ValidationStatus::Error,
                source: ConfigSource::EnvFile,
                var_type: Some("enum".to_string()),
                replacement: None,
                removal_version: None,
                error: Some(
                    "Production mode requires EdDSA JWT (AOS_SECURITY_JWT_MODE=eddsa)".to_string(),
                ),
                validation: None,
            });
        }
        None => {
            results.push(ValidationResult {
                name: "AOS_SECURITY_JWT_MODE".to_string(),
                value: "(unset)".to_string(),
                status: ValidationStatus::Error,
                source: ConfigSource::Default,
                var_type: Some("enum".to_string()),
                replacement: None,
                removal_version: None,
                error: Some(
                    "Production mode requires EdDSA JWT (AOS_SECURITY_JWT_MODE=eddsa)".to_string(),
                ),
                validation: None,
            });
        }
        _ => {}
    }

    match vars.get("AOS_SECURITY_PF_DENY") {
        Some(val) if val != "true" && val != "1" => {
            results.push(ValidationResult {
                name: "AOS_SECURITY_PF_DENY".to_string(),
                value: val.clone(),
                status: ValidationStatus::Error,
                source: ConfigSource::EnvFile,
                var_type: Some("bool".to_string()),
                replacement: None,
                removal_version: None,
                error: Some(
                    "Production mode requires PF deny (AOS_SECURITY_PF_DENY=true)".to_string(),
                ),
                validation: None,
            });
        }
        None => {
            results.push(ValidationResult {
                name: "AOS_SECURITY_PF_DENY".to_string(),
                value: "(unset)".to_string(),
                status: ValidationStatus::Error,
                source: ConfigSource::Default,
                var_type: Some("bool".to_string()),
                replacement: None,
                removal_version: None,
                error: Some(
                    "Production mode requires PF deny (AOS_SECURITY_PF_DENY=true)".to_string(),
                ),
                validation: None,
            });
        }
        _ => {}
    }

    for debug_var in &[
        "AOS_DEBUG_DETERMINISTIC",
        "AOS_DEBUG_TRACE_FFI",
        "AOS_DEBUG_VERBOSE",
        "AOS_DEBUG_SKIP_KERNEL_SIG",
        "AOS_DETERMINISTIC_DEBUG",
        "AOS_SKIP_KERNEL_SIGNATURE_VERIFY",
    ] {
        if let Some(val) = vars.get(*debug_var) {
            if val == "true" || val == "1" {
                results.push(ValidationResult {
                    name: debug_var.to_string(),
                    value: val.clone(),
                    status: ValidationStatus::Warning,
                    source: ConfigSource::EnvFile,
                    var_type: Some("bool".to_string()),
                    replacement: None,
                    removal_version: None,
                    error: Some("Production mode should not have debug flags enabled".to_string()),
                    validation: None,
                });
            }
        }
    }

    if let Some(secret) = vars.get("AOS_SECURITY_JWT_SECRET") {
        if secret == "changeme" || secret == "default" || secret.len() < 32 {
            results.push(ValidationResult {
                name: "AOS_SECURITY_JWT_SECRET".to_string(),
                value: "***REDACTED***".to_string(),
                status: ValidationStatus::Error,
                source: ConfigSource::EnvFile,
                var_type: Some("string".to_string()),
                replacement: None,
                removal_version: None,
                error: Some(
                    "Production mode requires custom JWT secret (min 32 chars)".to_string(),
                ),
                validation: None,
            });
        }
    }

    results
}

fn validate_required_variables(
    vars: &HashMap<String, String>,
    production: bool,
) -> Vec<ValidationResult> {
    let mut results = Vec::new();

    if production && !vars.contains_key("AOS_MODEL_PATH") {
        results.push(ValidationResult {
            name: "AOS_MODEL_PATH".to_string(),
            value: "(unset)".to_string(),
            status: ValidationStatus::Warning,
            source: ConfigSource::Default,
            var_type: Some("path".to_string()),
            replacement: None,
            removal_version: None,
            error: Some("AOS_MODEL_PATH is required for inference".to_string()),
            validation: None,
        });
    }

    results
}

// ============================================================================
// Validation Helper Functions
// ============================================================================

fn validate_path(value: &str, must_exist: bool) -> std::result::Result<(), String> {
    if value.is_empty() {
        return Err("Path cannot be empty".to_string());
    }
    if must_exist {
        let path = PathBuf::from(value);
        if !path.exists() {
            return Err(format!("Path does not exist: {}", value));
        }
    }
    Ok(())
}

fn validate_enum(value: &str, allowed: &[&str]) -> std::result::Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "Invalid value '{}'. Expected one of: {}",
            value,
            allowed.join(", ")
        ))
    }
}

fn validate_port(value: &str) -> std::result::Result<(), String> {
    match value.parse::<u16>() {
        Ok(port) if port > 0 => Ok(()),
        Ok(_) => Err("Port must be between 1 and 65535".to_string()),
        Err(_) => Err(format!("Expected integer 1-65535, got '{}'", value)),
    }
}

fn validate_integer(value: &str, min: i64, max: i64) -> std::result::Result<(), String> {
    match value.parse::<i64>() {
        Ok(n) if n >= min && n <= max => Ok(()),
        Ok(n) => Err(format!("Value {} out of range [{}, {}]", n, min, max)),
        Err(_) => Err(format!("Expected integer, got '{}'", value)),
    }
}

fn validate_float(value: &str, min: f64, max: f64) -> std::result::Result<(), String> {
    match value.parse::<f64>() {
        Ok(n) if n >= min && n <= max => Ok(()),
        Ok(n) => Err(format!("Value {} out of range [{}, {}]", n, min, max)),
        Err(_) => Err(format!("Expected float, got '{}'", value)),
    }
}

fn validate_bool(value: &str) -> std::result::Result<(), String> {
    match value.to_lowercase().as_str() {
        "true" | "false" | "1" | "0" | "yes" | "no" => Ok(()),
        _ => Err(format!(
            "Expected boolean (true/false, 1/0, yes/no), got '{}'",
            value
        )),
    }
}

fn validate_duration(value: &str) -> std::result::Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Duration cannot be empty".to_string());
    }

    let (num_str, unit) = if value.ends_with("ms") {
        (&value[..value.len() - 2], "ms")
    } else if value.ends_with('s') {
        (&value[..value.len() - 1], "s")
    } else if value.ends_with('m') {
        (&value[..value.len() - 1], "m")
    } else if value.ends_with('h') {
        (&value[..value.len() - 1], "h")
    } else {
        return value.parse::<u64>().map(|_| ()).map_err(|_| {
            format!(
                "Invalid duration format: '{}'. Expected: 30s, 5m, 1h",
                value
            )
        });
    };

    num_str
        .parse::<u64>()
        .map(|_| ())
        .map_err(|_| format!("Invalid {} duration: '{}'", unit, value))
}

fn validate_url(value: &str) -> std::result::Result<(), String> {
    if value.starts_with("http://") || value.starts_with("https://") || value.starts_with("unix://")
    {
        Ok(())
    } else {
        Err(format!(
            "Invalid URL format: '{}'. Expected http://, https://, or unix://",
            value
        ))
    }
}

fn validate_database_url(value: &str) -> std::result::Result<(), String> {
    if value.starts_with("sqlite:")
        || value.starts_with("postgres://")
        || value.starts_with("mysql://")
        || value.contains(".sqlite")
    {
        Ok(())
    } else {
        Err(format!(
            "Invalid database URL: '{}'. Expected sqlite:, postgres://, or mysql://",
            value
        ))
    }
}

fn validate_jwt_secret(value: &str) -> std::result::Result<(), String> {
    if value.len() < 16 {
        Err("JWT secret must be at least 16 characters".to_string())
    } else {
        Ok(())
    }
}

fn get_var_type(name: &str) -> String {
    match name {
        n if n.ends_with("_PATH")
            || n.ends_with("_FILE")
            || n.ends_with("_SOCKET")
            || n.ends_with("_DIR") =>
        {
            "path".to_string()
        }
        n if n.ends_with("_PORT")
            || n.ends_with("_SIZE")
            || n.ends_with("_INDEX")
            || n.ends_with("_WORKERS") =>
        {
            "integer".to_string()
        }
        n if n.ends_with("_PCT") || n.ends_with("_THRESHOLD") => "float".to_string(),
        n if n.ends_with("_ENABLED") || n.ends_with("_DENY") || n.starts_with("AOS_DEBUG_") => {
            "bool".to_string()
        }
        n if n.ends_with("_URL") || n.ends_with("_ENDPOINT") => "url".to_string(),
        n if n.ends_with("_TIMEOUT") => "duration".to_string(),
        n if n.ends_with("_MODE")
            || n.ends_with("_BACKEND")
            || n.ends_with("_FORMAT")
            || n.ends_with("_LEVEL") =>
        {
            "enum".to_string()
        }
        _ => "string".to_string(),
    }
}

// ============================================================================
// Output Formatting Functions
// ============================================================================

fn output_validation_text(
    args: &ValidateArgs,
    results: &[ValidationResult],
    valid_count: usize,
    warning_count: usize,
    error_count: usize,
    passed: bool,
    output: &OutputWriter,
) -> Result<()> {
    if !args.quiet {
        output.result("Configuration Validation Report");
        output.result("===============================");
        output.kv("Source", &args.env_file.display().to_string());
        output.kv(
            "Mode",
            if args.production {
                "Production"
            } else {
                "Development (use --production for production checks)"
            },
        );
        output.blank();
    }

    for result in results {
        if args.quiet && !result.is_error() {
            continue;
        }

        let status_icon = match result.status {
            ValidationStatus::Valid => "\u{2713}",
            ValidationStatus::Deprecated => "\u{26A0}",
            ValidationStatus::Warning => "\u{26A0}",
            ValidationStatus::Error => "\u{2717}",
        };

        let mut line = format!("{} {}: {}", status_icon, result.name, result.value);

        if let Some(ref err) = result.error {
            line.push_str(&format!(" ({})", err));
        } else if let Some(ref replacement) = result.replacement {
            line.push_str(&format!(
                " deprecated -> use {} (removal: {})",
                replacement,
                result.removal_version.as_deref().unwrap_or("unknown")
            ));
        } else if matches!(result.status, ValidationStatus::Valid) {
            line.push_str(&format!(" [{}]", result.source));
        }

        output.result(&line);
    }

    if !args.quiet {
        output.blank();
        output.result("Summary:");
        output.kv("  Valid", &valid_count.to_string());
        output.kv("  Warnings", &warning_count.to_string());
        output.kv("  Errors", &error_count.to_string());
        output.blank();
        output.result(&format!(
            "Status: {}",
            if passed { "PASSED" } else { "FAILED" }
        ));
    }

    Ok(())
}

fn output_validation_json(
    args: &ValidateArgs,
    results: &[ValidationResult],
    valid_count: usize,
    warning_count: usize,
    error_count: usize,
    passed: bool,
) -> Result<()> {
    let output = serde_json::json!({
        "source": args.env_file.display().to_string(),
        "mode": if args.production { "production" } else { "development" },
        "timestamp": Utc::now().to_rfc3339(),
        "variables": results,
        "summary": {
            "valid": valid_count,
            "warnings": warning_count,
            "errors": error_count
        },
        "passed": passed
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn output_validation_sarif(args: &ValidateArgs, results: &[ValidationResult]) -> Result<()> {
    let sarif_results: Vec<serde_json::Value> = results
        .iter()
        .filter(|r| r.is_error() || r.is_warning())
        .map(|r| {
            serde_json::json!({
                "ruleId": format!("config/{}", r.name),
                "level": if r.is_error() { "error" } else { "warning" },
                "message": {
                    "text": r.error.clone().unwrap_or_else(|| {
                        if let Some(ref replacement) = r.replacement {
                            format!("Deprecated variable, use {} instead", replacement)
                        } else {
                            format!("Invalid configuration for {}", r.name)
                        }
                    })
                },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": {
                            "uri": args.env_file.display().to_string()
                        }
                    }
                }]
            })
        })
        .collect();

    let sarif = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "aosctl config validate",
                    "version": env!("CARGO_PKG_VERSION")
                }
            },
            "results": sarif_results
        }]
    });

    println!("{}", serde_json::to_string_pretty(&sarif)?);
    Ok(())
}

// ============================================================================
// Migrate Command Implementation
// ============================================================================

/// Migrate legacy environment variables to new naming
pub async fn migrate(args: MigrateArgs, output: &OutputWriter) -> Result<()> {
    info!(input = ?args.input, dry_run = args.dry_run, "Migrating configuration");

    if !args.input.exists() {
        output.error(&format!("Source file not found: {}", args.input.display()));
        std::process::exit(2);
    }

    let content = fs::read_to_string(&args.input)
        .map_err(|e| AosError::Io(format!("Failed to read {}: {}", args.input.display(), e)))?;

    let mut lines = parse_env_with_structure(&content)?;

    let mut migrations = Vec::new();
    let mut conflicts = Vec::new();

    let existing_vars: HashMap<String, String> = lines
        .iter()
        .filter_map(|line| {
            if let EnvLine::Variable { name, value } = line {
                Some((name.clone(), value.clone()))
            } else {
                None
            }
        })
        .collect();

    for (legacy, new) in MIGRATION_MAP {
        let legacy_exists = existing_vars.contains_key(*legacy);
        let new_exists = existing_vars.contains_key(*new);

        if legacy_exists && new_exists {
            conflicts.push(MigrationResult {
                from: legacy.to_string(),
                to: new.to_string(),
                value: redact_if_sensitive(legacy, existing_vars.get(*legacy).unwrap()),
                status: MigrationStatus::Conflict,
            });
            continue;
        }

        if legacy_exists {
            let legacy_value = existing_vars.get(*legacy).unwrap();

            if args.interactive {
                let action = prompt_migration_action(legacy, new, legacy_value)?;
                match action {
                    MigrationAction::Migrate => {
                        rename_variable(&mut lines, legacy, new);
                        migrations.push(MigrationResult {
                            from: legacy.to_string(),
                            to: new.to_string(),
                            value: redact_if_sensitive(legacy, legacy_value),
                            status: MigrationStatus::Migrated,
                        });
                    }
                    MigrationAction::Skip => {
                        migrations.push(MigrationResult {
                            from: legacy.to_string(),
                            to: new.to_string(),
                            value: redact_if_sensitive(legacy, legacy_value),
                            status: MigrationStatus::Skipped,
                        });
                    }
                    MigrationAction::KeepBoth => {
                        add_variable_after(&mut lines, legacy, new, legacy_value);
                        migrations.push(MigrationResult {
                            from: legacy.to_string(),
                            to: new.to_string(),
                            value: redact_if_sensitive(legacy, legacy_value),
                            status: MigrationStatus::KeptBoth,
                        });
                    }
                    MigrationAction::Quit => {
                        output.warning("Migration cancelled by user");
                        std::process::exit(4);
                    }
                }
            } else {
                rename_variable(&mut lines, legacy, new);
                migrations.push(MigrationResult {
                    from: legacy.to_string(),
                    to: new.to_string(),
                    value: redact_if_sensitive(legacy, legacy_value),
                    status: MigrationStatus::Migrated,
                });
            }
        }
    }

    if args.remove_deprecated {
        for migration in &migrations {
            if migration.status == MigrationStatus::KeptBoth {
                remove_variable(&mut lines, &migration.from);
            }
        }
    }

    if args.dry_run {
        output_migration_preview(&args, &migrations, &conflicts, &lines, output)?;
        return Ok(());
    }

    if args.backup && !args.no_backup {
        let backup_path = format!("{}.backup", args.input.display());
        fs::copy(&args.input, &backup_path)
            .map_err(|e| AosError::Io(format!("Failed to create backup: {}", e)))?;
        info!(backup_path = %backup_path, "Created backup");
    }

    let output_path = args.output.clone().unwrap_or_else(|| args.input.clone());

    if output_path.to_string_lossy() == "-" {
        print!("{}", serialize_env(&lines));
    } else {
        atomic_write(&output_path, &serialize_env(&lines))?;
        info!(output_path = ?output_path, "Wrote migrated configuration");
    }

    output_migration_summary(&args, &migrations, &conflicts, output)?;

    Ok(())
}

fn parse_env_with_structure(content: &str) -> Result<Vec<EnvLine>> {
    let mut lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            lines.push(EnvLine::Blank);
        } else if trimmed.starts_with('#') {
            lines.push(EnvLine::Comment(line.to_string()));
        } else if let Some((name, value)) = parse_env_line(line) {
            lines.push(EnvLine::Variable {
                name: name.to_string(),
                value: value.to_string(),
            });
        } else {
            lines.push(EnvLine::Comment(format!("# {}", line)));
        }
    }

    Ok(lines)
}

fn parse_env_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if let Some(pos) = line.find('=') {
        let name = line[..pos].trim();
        let value = line[pos + 1..].trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        Some((name, value))
    } else {
        None
    }
}

fn serialize_env(lines: &[EnvLine]) -> String {
    lines
        .iter()
        .map(|line| match line {
            EnvLine::Comment(s) => s.clone(),
            EnvLine::Blank => String::new(),
            EnvLine::Variable { name, value } => {
                if value.contains(' ') || value.contains('"') || value.contains('\'') {
                    format!("{}=\"{}\"", name, value.replace('"', "\\\""))
                } else {
                    format!("{}={}", name, value)
                }
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn rename_variable(lines: &mut [EnvLine], old_name: &str, new_name: &str) {
    for line in lines.iter_mut() {
        if let EnvLine::Variable { name, .. } = line {
            if name == old_name {
                *name = new_name.to_string();
            }
        }
    }
}

fn add_variable_after(lines: &mut Vec<EnvLine>, after_name: &str, new_name: &str, value: &str) {
    if let Some(pos) = lines
        .iter()
        .position(|line| matches!(line, EnvLine::Variable { name, .. } if name == after_name))
    {
        lines.insert(
            pos + 1,
            EnvLine::Variable {
                name: new_name.to_string(),
                value: value.to_string(),
            },
        );
    }
}

fn remove_variable(lines: &mut Vec<EnvLine>, var_name: &str) {
    lines.retain(|line| !matches!(line, EnvLine::Variable { name, .. } if name == var_name));
}

fn atomic_write(path: &PathBuf, content: &str) -> Result<()> {
    let parent = path.parent().unwrap_or(std::path::Path::new("."));
    let temp_path = parent.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));

    fs::write(&temp_path, content)
        .map_err(|e| AosError::Io(format!("Failed to write temp file: {}", e)))?;

    fs::rename(&temp_path, path)
        .map_err(|e| AosError::Io(format!("Failed to rename temp file: {}", e)))?;

    Ok(())
}

fn redact_if_sensitive(name: &str, value: &str) -> String {
    if SENSITIVE_VARS.contains(&name) {
        "***REDACTED***".to_string()
    } else {
        value.to_string()
    }
}

enum MigrationAction {
    Migrate,
    Skip,
    KeepBoth,
    Quit,
}

fn prompt_migration_action(legacy: &str, new: &str, value: &str) -> Result<MigrationAction> {
    println!("\nMigration: {} -> {}", legacy, new);
    println!("  Current value: {}", redact_if_sensitive(legacy, value));
    println!();
    println!("  [M]igrate  [S]kip  [K]eep both  [Q]uit");
    print!("  > ");
    std::io::stdout()
        .flush()
        .map_err(|e| AosError::Io(e.to_string()))?;

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| AosError::Io(e.to_string()))?;

    match input.trim().to_lowercase().as_str() {
        "m" | "migrate" => Ok(MigrationAction::Migrate),
        "s" | "skip" => Ok(MigrationAction::Skip),
        "k" | "keep" | "keepboth" | "keep both" => Ok(MigrationAction::KeepBoth),
        "q" | "quit" => Ok(MigrationAction::Quit),
        _ => {
            println!("Invalid choice, defaulting to Skip");
            Ok(MigrationAction::Skip)
        }
    }
}

fn output_migration_preview(
    args: &MigrateArgs,
    migrations: &[MigrationResult],
    conflicts: &[MigrationResult],
    _lines: &[EnvLine],
    output: &OutputWriter,
) -> Result<()> {
    match args.format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "source": args.input.display().to_string(),
                "target": args.output.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| args.input.display().to_string()),
                "dry_run": true,
                "migrations": migrations,
                "conflicts": conflicts,
                "summary": {
                    "migrated": migrations.iter().filter(|m| m.status == MigrationStatus::Migrated).count(),
                    "skipped": migrations.iter().filter(|m| m.status == MigrationStatus::Skipped).count(),
                    "conflicts": conflicts.len()
                }
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        OutputFormat::Diff => {
            println!("--- {}.original", args.input.display());
            println!("+++ {}.migrated", args.input.display());

            for migration in migrations {
                if migration.status == MigrationStatus::Migrated {
                    println!("-{}={}", migration.from, migration.value);
                    println!("+{}={}", migration.to, migration.value);
                    println!();
                }
            }
        }
        _ => {
            output.result("Configuration Migration Preview");
            output.result("===============================");
            output.kv("Source", &args.input.display().to_string());
            output.kv(
                "Target",
                &args
                    .output
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| args.input.display().to_string() + " (in-place)"),
            );
            output.blank();

            if !migrations.is_empty() {
                output.result("Migrations:");
                for migration in migrations {
                    output.result(&format!("  {} -> {}", migration.from, migration.to));
                    output.result(&format!("    Current: {}", migration.value));
                }
            }

            if !conflicts.is_empty() {
                output.blank();
                output.warning("Conflicts (both legacy and new variable exist):");
                for conflict in conflicts {
                    output.result(&format!(
                        "  {} and {} both exist",
                        conflict.from, conflict.to
                    ));
                }
            }

            output.blank();
            output.result("No changes made (dry-run mode).");
            output.result("Run without --dry-run to apply changes.");
            output.blank();
            output.result("Summary:");
            output.kv(
                "  Variables to migrate",
                &migrations
                    .iter()
                    .filter(|m| m.status == MigrationStatus::Migrated)
                    .count()
                    .to_string(),
            );
            output.kv("  Conflicts", &conflicts.len().to_string());
        }
    }

    Ok(())
}

fn output_migration_summary(
    args: &MigrateArgs,
    migrations: &[MigrationResult],
    conflicts: &[MigrationResult],
    output: &OutputWriter,
) -> Result<()> {
    let migrated_count = migrations
        .iter()
        .filter(|m| m.status == MigrationStatus::Migrated)
        .count();
    let skipped_count = migrations
        .iter()
        .filter(|m| m.status == MigrationStatus::Skipped)
        .count();

    match args.format {
        OutputFormat::Json => {
            let json = serde_json::json!({
                "source": args.input.display().to_string(),
                "target": args.output.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| args.input.display().to_string()),
                "backup": if args.backup && !args.no_backup { Some(format!("{}.backup", args.input.display())) } else { None },
                "dry_run": false,
                "migrations": migrations,
                "summary": {
                    "migrated": migrated_count,
                    "skipped": skipped_count,
                    "conflicts": conflicts.len(),
                    "errors": 0
                }
            });
            println!("{}", serde_json::to_string_pretty(&json)?);
        }
        _ => {
            output.success(&format!(
                "Migration completed: {} variables migrated",
                migrated_count
            ));
            if !conflicts.is_empty() {
                output.warning(&format!(
                    "{} conflicts detected (both legacy and new exist)",
                    conflicts.len()
                ));
            }
            if skipped_count > 0 {
                output.result(&format!("  {} skipped", skipped_count));
            }
        }
    }

    Ok(())
}

// ============================================================================
// Show Command Implementation
// ============================================================================

/// Show effective configuration with source attribution
pub async fn show(args: ShowArgs, output: &OutputWriter) -> Result<()> {
    info!(category = ?args.category, "Showing configuration");

    let should_redact = if args.no_redact {
        println!("WARNING: About to display sensitive values. Continue? [y/N]");
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| AosError::Io(e.to_string()))?;
        if input.trim().to_lowercase() != "y" {
            output.warning("Cancelled - sensitive values will be redacted");
            true
        } else {
            false
        }
    } else {
        true
    };

    let config = collect_effective_config(&args, should_redact)?;

    match args.format {
        OutputFormat::Json => output_show_json(&config)?,
        OutputFormat::Env => output_show_env(&config)?,
        _ => output_show_table(&config, output)?,
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize)]
struct ConfigEntry {
    name: String,
    value: String,
    source: ConfigSource,
    category: String,
}

fn collect_effective_config(args: &ShowArgs, should_redact: bool) -> Result<Vec<ConfigEntry>> {
    let mut entries = Vec::new();

    let env_file_vars = if PathBuf::from(".env").exists() {
        parse_env_file(&PathBuf::from(".env"))?
    } else {
        HashMap::new()
    };

    let env_vars: HashMap<String, String> = std::env::vars().collect();

    let var_definitions: Vec<(&str, &str, &str)> = vec![
        ("AOS_MODEL_PATH", "model", "./models/qwen2.5-7b-mlx"),
        ("AOS_MODEL_BACKEND", "model", "auto"),
        ("AOS_MODEL_ARCHITECTURE", "model", ""),
        ("AOS_SERVER_HOST", "server", "127.0.0.1"),
        ("AOS_SERVER_PORT", "server", "8080"),
        ("AOS_SERVER_WORKERS", "server", "4"),
        ("AOS_SERVER_TIMEOUT", "server", "30s"),
        ("AOS_SERVER_UDS_SOCKET", "server", ""),
        ("AOS_DATABASE_URL", "database", "sqlite:var/aos-cp.sqlite3"),
        ("AOS_DATABASE_POOL_SIZE", "database", "10"),
        ("AOS_DATABASE_TIMEOUT", "database", "30s"),
        ("AOS_SECURITY_JWT_SECRET", "security", ""),
        ("AOS_SECURITY_JWT_MODE", "security", "hs256"),
        ("AOS_SECURITY_PF_DENY", "security", "false"),
        ("AOS_LOG_LEVEL", "logging", "info"),
        ("AOS_LOG_FORMAT", "logging", "text"),
        ("AOS_LOG_FILE", "logging", ""),
        ("AOS_MEMORY_HEADROOM_PCT", "memory", "0.15"),
        ("AOS_MEMORY_EVICTION_THRESHOLD", "memory", "0.85"),
        ("AOS_BACKEND_COREML_ENABLED", "backend", "true"),
        ("AOS_BACKEND_METAL_ENABLED", "backend", "true"),
        ("AOS_BACKEND_MLX_ENABLED", "backend", "true"),
        ("AOS_BACKEND_GPU_INDEX", "backend", "0"),
        ("AOS_FEDERATION_ENABLED", "federation", "false"),
        ("AOS_FEDERATION_PEERS", "federation", ""),
        ("AOS_TELEMETRY_ENABLED", "telemetry", "true"),
        ("AOS_TELEMETRY_ENDPOINT", "telemetry", ""),
        ("AOS_DEBUG_DETERMINISTIC", "debug", "false"),
        ("AOS_DEBUG_TRACE_FFI", "debug", "false"),
        ("AOS_DEBUG_VERBOSE", "debug", "false"),
        ("AOS_DEBUG_SKIP_KERNEL_SIG", "debug", "false"),
        ("AOS_ENVIRONMENT", "server", "development"),
        ("AOS_MLX_PATH", "backend", ""),
        ("AOS_KEYCHAIN_FALLBACK", "security", ""),
    ];

    let category_filter = match args.category {
        ConfigCategory::All => None,
        ConfigCategory::Model => Some("model"),
        ConfigCategory::Server => Some("server"),
        ConfigCategory::Database => Some("database"),
        ConfigCategory::Security => Some("security"),
        ConfigCategory::Logging => Some("logging"),
        ConfigCategory::Telemetry => Some("telemetry"),
        ConfigCategory::Memory => Some("memory"),
        ConfigCategory::Backend => Some("backend"),
        ConfigCategory::Federation => Some("federation"),
        ConfigCategory::Debug => Some("debug"),
    };

    for (name, category, default) in var_definitions {
        if let Some(filter) = category_filter {
            if category != filter {
                continue;
            }
        }

        let (value, source) = if let Some(v) = env_vars.get(name) {
            (v.clone(), ConfigSource::Env)
        } else if let Some(v) = env_file_vars.get(name) {
            (v.clone(), ConfigSource::EnvFile)
        } else if !default.is_empty() {
            if args.show_defaults {
                (default.to_string(), ConfigSource::Default)
            } else {
                continue;
            }
        } else {
            if args.show_unset {
                ("(unset)".to_string(), ConfigSource::Default)
            } else {
                continue;
            }
        };

        let display_value = if SENSITIVE_VARS.contains(&name) && should_redact {
            "***REDACTED***".to_string()
        } else {
            value
        };

        entries.push(ConfigEntry {
            name: name.to_string(),
            value: display_value,
            source,
            category: category.to_string(),
        });
    }

    Ok(entries)
}

fn output_show_table(config: &[ConfigEntry], output: &OutputWriter) -> Result<()> {
    output.result("Effective Configuration");
    output.result("=======================");
    output.result("Source priority: CLI > ENV > .env > Manifest > Defaults");
    output.blank();

    let mut current_category = String::new();

    for entry in config {
        if entry.category != current_category {
            output.blank();
            output.result(&format!("Category: {}", capitalize(&entry.category)));
            current_category = entry.category.clone();
        }

        output.result(&format!(
            "  {:30} = {:30} [{}]",
            entry.name,
            truncate(&entry.value, 30),
            entry.source
        ));
    }

    output.blank();
    output.result("Legend: [cli] [env] [.env] [manifest] [default]");

    Ok(())
}

fn output_show_json(config: &[ConfigEntry]) -> Result<()> {
    let mut grouped: HashMap<String, HashMap<String, serde_json::Value>> = HashMap::new();

    for entry in config {
        let category_map = grouped.entry(entry.category.clone()).or_default();
        category_map.insert(
            entry.name.clone(),
            serde_json::json!({
                "value": entry.value,
                "source": entry.source.to_string()
            }),
        );
    }

    println!("{}", serde_json::to_string_pretty(&grouped)?);
    Ok(())
}

fn output_show_env(config: &[ConfigEntry]) -> Result<()> {
    println!("# AdapterOS Configuration Export");
    println!("# Generated: {}", Utc::now().to_rfc3339());
    println!();

    let mut current_category = String::new();

    for entry in config {
        if entry.category != current_category {
            println!();
            println!("# === {} ===", entry.category.to_uppercase());
            current_category = entry.category.clone();
        }

        if entry.value == "(unset)" || entry.value == "***REDACTED***" {
            println!("# {}=", entry.name);
        } else if entry.value.contains(' ') {
            println!("{}=\"{}\"", entry.name, entry.value);
        } else {
            println!("{}={}", entry.name, entry.value);
        }
    }

    Ok(())
}

// ============================================================================
// Helper Functions
// ============================================================================

fn parse_env_file(path: &PathBuf) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path)
        .map_err(|e| AosError::Io(format!("Failed to read {}: {}", path.display(), e)))?;

    let mut vars = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((name, value)) = parse_env_line(line) {
            vars.insert(name.to_string(), value.to_string());
        }
    }

    Ok(vars)
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_path_exists() {
        assert!(validate_path(".", true).is_ok());
        assert!(validate_path("/nonexistent/path/12345", true).is_err());
        assert!(validate_path("", true).is_err());
    }

    #[test]
    fn test_validate_path_no_exist_check() {
        assert!(validate_path("/some/future/path", false).is_ok());
    }

    #[test]
    fn test_validate_enum() {
        assert!(validate_enum("auto", &["auto", "metal", "mlx"]).is_ok());
        assert!(validate_enum("metal", &["auto", "metal", "mlx"]).is_ok());
        assert!(validate_enum("invalid", &["auto", "metal", "mlx"]).is_err());
    }

    #[test]
    fn test_validate_port() {
        assert!(validate_port("8080").is_ok());
        assert!(validate_port("1").is_ok());
        assert!(validate_port("65535").is_ok());
        assert!(validate_port("0").is_err());
        assert!(validate_port("abc").is_err());
        assert!(validate_port("99999").is_err());
    }

    #[test]
    fn test_validate_integer() {
        assert!(validate_integer("5", 1, 10).is_ok());
        assert!(validate_integer("1", 1, 10).is_ok());
        assert!(validate_integer("10", 1, 10).is_ok());
        assert!(validate_integer("0", 1, 10).is_err());
        assert!(validate_integer("11", 1, 10).is_err());
        assert!(validate_integer("abc", 1, 10).is_err());
    }

    #[test]
    fn test_validate_float() {
        assert!(validate_float("0.5", 0.0, 1.0).is_ok());
        assert!(validate_float("0.0", 0.0, 1.0).is_ok());
        assert!(validate_float("1.0", 0.0, 1.0).is_ok());
        assert!(validate_float("-0.1", 0.0, 1.0).is_err());
        assert!(validate_float("1.1", 0.0, 1.0).is_err());
        assert!(validate_float("abc", 0.0, 1.0).is_err());
    }

    #[test]
    fn test_validate_bool() {
        assert!(validate_bool("true").is_ok());
        assert!(validate_bool("false").is_ok());
        assert!(validate_bool("1").is_ok());
        assert!(validate_bool("0").is_ok());
        assert!(validate_bool("yes").is_ok());
        assert!(validate_bool("no").is_ok());
        assert!(validate_bool("TRUE").is_ok());
        assert!(validate_bool("maybe").is_err());
    }

    #[test]
    fn test_validate_duration() {
        assert!(validate_duration("30s").is_ok());
        assert!(validate_duration("5m").is_ok());
        assert!(validate_duration("1h").is_ok());
        assert!(validate_duration("100ms").is_ok());
        assert!(validate_duration("1000").is_ok());
        assert!(validate_duration("").is_err());
        assert!(validate_duration("abc").is_err());
    }

    #[test]
    fn test_validate_url() {
        assert!(validate_url("http://localhost:8080").is_ok());
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("unix:///var/run/aos.sock").is_ok());
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("example.com").is_err());
    }

    #[test]
    fn test_validate_database_url() {
        assert!(validate_database_url("sqlite:var/aos.db").is_ok());
        assert!(validate_database_url("postgres://localhost/db").is_ok());
        assert!(validate_database_url("mysql://localhost/db").is_ok());
        assert!(validate_database_url("/path/to/file.sqlite3").is_ok());
        assert!(validate_database_url("redis://localhost").is_err());
    }

    #[test]
    fn test_parse_env_line() {
        assert_eq!(parse_env_line("KEY=value"), Some(("KEY", "value")));
        assert_eq!(
            parse_env_line("KEY=\"value with spaces\""),
            Some(("KEY", "value with spaces"))
        );
        assert_eq!(
            parse_env_line("KEY='single quotes'"),
            Some(("KEY", "single quotes"))
        );
        assert_eq!(parse_env_line("  KEY = value  "), Some(("KEY", "value")));
        assert_eq!(parse_env_line("no_equals"), None);
    }

    #[test]
    fn test_redact_if_sensitive() {
        assert_eq!(
            redact_if_sensitive("AOS_SECURITY_JWT_SECRET", "secret"),
            "***REDACTED***"
        );
        assert_eq!(
            redact_if_sensitive("AOS_MODEL_PATH", "/path/to/model"),
            "/path/to/model"
        );
    }

    #[test]
    fn test_get_var_type() {
        assert_eq!(get_var_type("AOS_MODEL_PATH"), "path");
        assert_eq!(get_var_type("AOS_SERVER_PORT"), "integer");
        assert_eq!(get_var_type("AOS_MEMORY_HEADROOM_PCT"), "float");
        assert_eq!(get_var_type("AOS_DEBUG_VERBOSE"), "bool");
        assert_eq!(get_var_type("AOS_DATABASE_URL"), "url");
        assert_eq!(get_var_type("AOS_SERVER_TIMEOUT"), "duration");
        assert_eq!(get_var_type("AOS_MODEL_BACKEND"), "enum");
        assert_eq!(get_var_type("SOME_RANDOM_VAR"), "string");
    }

    #[test]
    fn test_migration_map_has_entries() {
        assert!(!MIGRATION_MAP.is_empty());
        for (legacy, new) in MIGRATION_MAP {
            assert!(!legacy.is_empty());
            assert!(!new.is_empty());
            assert!(new.starts_with("AOS_"));
        }
    }

    #[test]
    fn test_sensitive_vars_defined() {
        assert!(!SENSITIVE_VARS.is_empty());
        assert!(SENSITIVE_VARS.contains(&"AOS_SECURITY_JWT_SECRET"));
    }

    #[test]
    fn test_config_source_display() {
        assert_eq!(ConfigSource::Cli.to_string(), "cli");
        assert_eq!(ConfigSource::Env.to_string(), "env");
        assert_eq!(ConfigSource::EnvFile.to_string(), ".env");
        assert_eq!(ConfigSource::Manifest.to_string(), "manifest");
        assert_eq!(ConfigSource::Default.to_string(), "default");
    }

    #[test]
    fn test_validation_status_display() {
        assert_eq!(ValidationStatus::Valid.to_string(), "valid");
        assert_eq!(ValidationStatus::Deprecated.to_string(), "deprecated");
        assert_eq!(ValidationStatus::Warning.to_string(), "warning");
        assert_eq!(ValidationStatus::Error.to_string(), "error");
    }

    #[test]
    fn test_validation_result_is_error() {
        let valid = ValidationResult {
            name: "TEST".to_string(),
            value: "value".to_string(),
            status: ValidationStatus::Valid,
            source: ConfigSource::Env,
            var_type: None,
            replacement: None,
            removal_version: None,
            error: None,
            validation: None,
        };
        assert!(!valid.is_error());

        let error = ValidationResult {
            name: "TEST".to_string(),
            value: "value".to_string(),
            status: ValidationStatus::Error,
            source: ConfigSource::Env,
            var_type: None,
            replacement: None,
            removal_version: None,
            error: Some("error".to_string()),
            validation: None,
        };
        assert!(error.is_error());
    }

    #[test]
    fn test_validation_result_is_warning() {
        let warning = ValidationResult {
            name: "TEST".to_string(),
            value: "value".to_string(),
            status: ValidationStatus::Warning,
            source: ConfigSource::Env,
            var_type: None,
            replacement: None,
            removal_version: None,
            error: None,
            validation: None,
        };
        assert!(warning.is_warning());

        let deprecated = ValidationResult {
            name: "TEST".to_string(),
            value: "value".to_string(),
            status: ValidationStatus::Deprecated,
            source: ConfigSource::Env,
            var_type: None,
            replacement: Some("NEW_TEST".to_string()),
            removal_version: Some("v0.03".to_string()),
            error: None,
            validation: None,
        };
        assert!(deprecated.is_warning());
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactly10c", 10), "exactly10c");
        assert_eq!(truncate("this is a longer string", 10), "this is...");
    }

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("model"), "Model");
        assert_eq!(capitalize(""), "");
        assert_eq!(capitalize("ALREADY"), "ALREADY");
    }

    #[test]
    fn test_env_line_parsing() {
        let content = r#"
# Comment line
KEY1=value1
KEY2="quoted value"
KEY3='single quoted'

# Another comment
KEY4=value4
"#;
        let lines = parse_env_with_structure(content).unwrap();
        let var_count = lines
            .iter()
            .filter(|l| matches!(l, EnvLine::Variable { .. }))
            .count();
        assert_eq!(var_count, 4);
        let comment_count = lines
            .iter()
            .filter(|l| matches!(l, EnvLine::Comment(_)))
            .count();
        assert_eq!(comment_count, 2);
    }

    #[test]
    fn test_serialize_env() {
        let lines = vec![
            EnvLine::Comment("# Test comment".to_string()),
            EnvLine::Blank,
            EnvLine::Variable {
                name: "KEY1".to_string(),
                value: "value1".to_string(),
            },
            EnvLine::Variable {
                name: "KEY2".to_string(),
                value: "value with spaces".to_string(),
            },
        ];

        let output = serialize_env(&lines);
        assert!(output.contains("# Test comment"));
        assert!(output.contains("KEY1=value1"));
        assert!(output.contains("KEY2=\"value with spaces\""));
    }

    #[test]
    fn test_rename_variable() {
        let mut lines = vec![
            EnvLine::Variable {
                name: "OLD_NAME".to_string(),
                value: "value".to_string(),
            },
            EnvLine::Variable {
                name: "OTHER".to_string(),
                value: "other".to_string(),
            },
        ];

        rename_variable(&mut lines, "OLD_NAME", "NEW_NAME");

        if let EnvLine::Variable { name, value } = &lines[0] {
            assert_eq!(name, "NEW_NAME");
            assert_eq!(value, "value");
        } else {
            panic!("Expected Variable");
        }
    }
}
