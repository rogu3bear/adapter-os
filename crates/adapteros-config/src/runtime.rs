//! Runtime configuration with type-safe accessors and validation
//!
//! This module provides a validated runtime configuration that:
//! - Loads all AOS_* environment variables at startup
//! - Validates values against the schema
//! - Provides type-safe accessors for common configuration
//! - Tracks unknown variables for warnings
//! - Computes a configuration hash for reproducibility

use crate::schema::{default_schema, parse_bool, validate_value, ConfigSchema, ConfigType};
use adapteros_core::defaults::{
    DEFAULT_DB_PATH, DEFAULT_LOG_LEVEL, DEFAULT_MODEL_BACKEND, DEFAULT_SERVER_HOST,
    DEFAULT_SERVER_PORT,
};
use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Storage backend selection for database abstraction layer
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackend {
    /// SQL only (current default, uses SQLite)
    Sql,
    /// Write to both SQL and KV, read from SQL (migration phase 1)
    Dual,
    /// Write to both SQL and KV, read from KV (migration phase 2)
    KvPrimary,
    /// KV only (future target, uses redb)
    KvOnly,
}

impl std::str::FromStr for StorageBackend {
    type Err = AosError;

    /// Parse from string (canonical underscore names + hyphen/short aliases).
    fn from_str(s: &str) -> Result<Self> {
        let s = s.to_lowercase();
        match s.as_str() {
            // Canonical (underscore) plus short alias
            "sql_only" | "sql" => Ok(Self::Sql),
            "dual_write" | "dual" => Ok(Self::Dual),
            "kv_primary" | "kv-primary" => Ok(Self::KvPrimary),
            "kv_only" | "kv-only" => Ok(Self::KvOnly),
            _ => Err(AosError::Config(format!(
                "Invalid storage backend: '{}'. Must be one of: sql_only, dual_write, kv_primary, kv_only (aliases: sql, dual, kv-primary, kv-only)",
                s
            ))),
        }
    }
}

impl StorageBackend {
    /// Convert to string (canonical underscore form)
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sql => "sql_only",
            Self::Dual => "dual_write",
            Self::KvPrimary => "kv_primary",
            Self::KvOnly => "kv_only",
        }
    }

    /// Check if KV store is used for reads
    pub fn reads_from_kv(&self) -> bool {
        matches!(self, Self::KvPrimary | Self::KvOnly)
    }

    /// Check if KV store is used for writes
    pub fn writes_to_kv(&self) -> bool {
        matches!(self, Self::Dual | Self::KvPrimary | Self::KvOnly)
    }

    /// Check if SQL is used for reads
    pub fn reads_from_sql(&self) -> bool {
        matches!(self, Self::Sql | Self::Dual)
    }

    /// Check if SQL is used for writes
    pub fn writes_to_sql(&self) -> bool {
        matches!(self, Self::Sql | Self::Dual | Self::KvPrimary)
    }
}

/// Source of a configuration value
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigSource {
    /// Value from environment variable
    Environment,
    /// Default value from schema
    Default,
    /// Value from manifest file
    Manifest,
    /// Value from CLI argument
    Cli,
}

/// Parsed configuration value
#[derive(Debug, Clone)]
pub enum ParsedValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    Path(PathBuf),
    Duration(std::time::Duration),
    ByteSize(u64),
}

impl ParsedValue {
    /// Get as string reference
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ParsedValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// Get as i64
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ParsedValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Get as f64
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            ParsedValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Get as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ParsedValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Get as path reference
    pub fn as_path(&self) -> Option<&Path> {
        match self {
            ParsedValue::Path(p) => Some(p),
            _ => None,
        }
    }
}

/// Runtime configuration with validated values
#[derive(Debug)]
pub struct RuntimeConfig {
    /// Parsed configuration values
    values: HashMap<String, ParsedValue>,
    /// Source of each value
    sources: HashMap<String, ConfigSource>,
    /// Unknown environment variables (AOS_* but not in schema)
    unknown_vars: Vec<String>,
    /// Configuration hash for reproducibility
    hash: String,
    /// The schema used for validation
    schema: ConfigSchema,
    /// Validation errors (for development mode)
    validation_errors: Vec<String>,
}

impl RuntimeConfig {
    /// Create RuntimeConfig from current environment
    pub fn from_env() -> Result<Self> {
        let schema = default_schema();
        let mut values = HashMap::new();
        let mut sources = HashMap::new();
        let mut unknown_vars = Vec::new();
        let mut validation_errors = Vec::new();
        let mut hash_input = Vec::new();

        // Collect all AOS_* environment variables
        for (key, value) in std::env::vars() {
            if key.starts_with("AOS_") {
                if let Some(var) = schema.get_variable(&key) {
                    // Validate value against schema
                    if let Err(e) = validate_value(var, &value) {
                        validation_errors.push(format!("{}: {}", key, e));
                        continue;
                    }

                    // Parse value based on type
                    let parsed = Self::parse_value(&var.config_type, &value)?;
                    hash_input.push(format!("{}={}", key, value));
                    values.insert(key.clone(), parsed);
                    sources.insert(key, ConfigSource::Environment);
                } else {
                    unknown_vars.push(key);
                }
            }
        }

        // Apply defaults for missing values
        for (name, var) in &schema.variables {
            if !values.contains_key(name) {
                if let Some(default) = &var.default {
                    if let Ok(parsed) = Self::parse_value(&var.config_type, default) {
                        hash_input.push(format!("{}={}", name, default));
                        values.insert(name.clone(), parsed);
                        sources.insert(name.clone(), ConfigSource::Default);
                    }
                }
            }
        }

        // Sort hash input for determinism
        hash_input.sort();
        let hash = Self::compute_hash(&hash_input.join("\n"));

        Ok(Self {
            values,
            sources,
            unknown_vars,
            hash,
            schema,
            validation_errors,
        })
    }

    /// Parse a string value into a ParsedValue based on config type
    fn parse_value(config_type: &ConfigType, value: &str) -> Result<ParsedValue> {
        match config_type {
            ConfigType::String => Ok(ParsedValue::String(value.to_string())),
            ConfigType::Path { .. } => Ok(ParsedValue::Path(PathBuf::from(value))),
            ConfigType::Url => Ok(ParsedValue::String(value.to_string())),
            ConfigType::Integer { .. } => {
                let i: i64 = value
                    .parse()
                    .map_err(|_| AosError::Config(format!("Invalid integer: {}", value)))?;
                Ok(ParsedValue::Integer(i))
            }
            ConfigType::Float { .. } => {
                let f: f64 = value
                    .parse()
                    .map_err(|_| AosError::Config(format!("Invalid float: {}", value)))?;
                Ok(ParsedValue::Float(f))
            }
            ConfigType::Bool => {
                let b = parse_bool(value)
                    .map_err(|e| AosError::Config(format!("Invalid bool: {}", e)))?;
                Ok(ParsedValue::Bool(b))
            }
            ConfigType::Enum { .. } => Ok(ParsedValue::String(value.to_string())),
            ConfigType::Duration => {
                let millis = Self::parse_duration_ms(value)?;
                Ok(ParsedValue::Duration(std::time::Duration::from_millis(
                    millis,
                )))
            }
            ConfigType::ByteSize => {
                let bytes = Self::parse_byte_size(value)?;
                Ok(ParsedValue::ByteSize(bytes))
            }
        }
    }

    /// Parse duration string to milliseconds
    fn parse_duration_ms(value: &str) -> Result<u64> {
        let value = value.trim();
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
            return Err(AosError::Config(format!(
                "Invalid duration format: {}",
                value
            )));
        };

        let num: u64 = num_str
            .trim()
            .parse()
            .map_err(|_| AosError::Config(format!("Invalid duration number: {}", num_str)))?;

        Ok(match unit {
            "ms" => num,
            "s" => num * 1000,
            "m" => num * 60 * 1000,
            "h" => num * 60 * 60 * 1000,
            "d" => num * 24 * 60 * 60 * 1000,
            _ => unreachable!(),
        })
    }

    /// Parse byte size string to bytes
    fn parse_byte_size(value: &str) -> Result<u64> {
        let value = value.trim();
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
            return Err(AosError::Config(format!(
                "Invalid byte size format: {}",
                value
            )));
        };

        let num: f64 = num_str
            .trim()
            .parse()
            .map_err(|_| AosError::Config(format!("Invalid byte size number: {}", num_str)))?;

        Ok((num * multiplier as f64) as u64)
    }

    /// Compute a hash of the configuration for reproducibility
    fn compute_hash(input: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        input.hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    // =========================================================================
    // Type-safe accessors for common configuration values
    // =========================================================================

    /// Get server port
    pub fn server_port(&self) -> u16 {
        self.get_i64("AOS_SERVER_PORT")
            .unwrap_or(DEFAULT_SERVER_PORT as i64) as u16
    }

    /// Get server host
    pub fn server_host(&self) -> &str {
        self.get_string("AOS_SERVER_HOST")
            .unwrap_or(DEFAULT_SERVER_HOST)
    }

    /// Get model path
    pub fn model_path(&self) -> Option<&Path> {
        self.get_path("AOS_MODEL_PATH")
    }

    /// Get model backend preference
    pub fn model_backend(&self) -> &str {
        // Schema default is "mlx" (canonical)
        self.get_string("AOS_MODEL_BACKEND")
            .unwrap_or(DEFAULT_MODEL_BACKEND)
    }

    /// Get database URL
    pub fn database_url(&self) -> &str {
        self.get_string("AOS_DATABASE_URL")
            .unwrap_or(DEFAULT_DB_PATH)
    }

    /// Get log level
    pub fn log_level(&self) -> &str {
        self.get_string("AOS_LOG_LEVEL")
            .unwrap_or(DEFAULT_LOG_LEVEL)
    }

    /// Get var directory
    pub fn var_dir(&self) -> PathBuf {
        self.get_path("AOS_VAR_DIR")
            .map(adapteros_core::rebase_var_path)
            .unwrap_or_else(|| adapteros_core::rebase_var_path("var"))
    }

    /// Get model cache directory
    pub fn model_cache_dir(&self) -> PathBuf {
        self.get_path("AOS_MODEL_CACHE_DIR")
            .map(adapteros_core::rebase_var_path)
            .unwrap_or_else(|| self.var_dir().join("model-cache"))
    }

    /// Get adapters directory
    pub fn adapters_dir(&self) -> PathBuf {
        self.get_path("AOS_ADAPTERS_DIR")
            .map(adapteros_core::rebase_var_path)
            .unwrap_or_else(|| self.var_dir().join("adapters"))
    }

    /// Check if production mode is enabled
    pub fn is_production_mode(&self) -> bool {
        self.get_bool("AOS_SERVER_PRODUCTION_MODE").unwrap_or(false)
    }

    /// Get runtime mode
    pub fn runtime_mode(&self) -> &str {
        self.get_string("AOS_RUNTIME_MODE").unwrap_or("development")
    }

    /// Get tenant ID
    pub fn tenant_id(&self) -> &str {
        self.get_string("AOS_TENANT_ID").unwrap_or("default")
    }

    /// Get router k-sparse value
    pub fn router_k_sparse(&self) -> usize {
        self.get_i64("AOS_ROUTER_K_SPARSE").unwrap_or(4) as usize
    }

    /// Get storage backend mode
    ///
    /// Returns the configured storage backend, falling back to `StorageBackend::Sql` (default)
    /// if the value is not set or cannot be parsed. Invalid values are logged as warnings.
    pub fn storage_backend(&self) -> StorageBackend {
        match self.get_string("AOS_STORAGE_BACKEND") {
            Some(s) => match StorageBackend::from_str(s) {
                Ok(backend) => backend,
                Err(e) => {
                    tracing::warn!(
                        value = %s,
                        error = %e,
                        "Invalid AOS_STORAGE_BACKEND value, falling back to sql_only"
                    );
                    StorageBackend::Sql
                }
            },
            None => StorageBackend::Sql,
        }
    }

    /// Get KV database path
    pub fn kv_path(&self) -> PathBuf {
        self.get_path("AOS_KV_PATH")
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| adapteros_core::rebase_var_path("var/aos-kv.redb"))
    }

    /// Get Tantivy search index path
    pub fn tantivy_path(&self) -> PathBuf {
        self.get_path("AOS_TANTIVY_PATH")
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| adapteros_core::rebase_var_path("var/aos-search"))
    }

    // =========================================================================
    // Generic accessors
    // =========================================================================

    /// Get a string value
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.values.get(key).and_then(|v| match v {
            ParsedValue::String(s) => Some(s.as_str()),
            _ => None,
        })
    }

    /// Get a bool value
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.values.get(key).and_then(|v| v.as_bool())
    }

    /// Get an i64 value
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.values.get(key).and_then(|v| v.as_i64())
    }

    /// Get an f64 value
    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.values.get(key).and_then(|v| v.as_f64())
    }

    /// Get a path value
    pub fn get_path(&self, key: &str) -> Option<&Path> {
        self.values.get(key).and_then(|v| v.as_path())
    }

    /// Get the source of a configuration value
    pub fn get_source(&self, key: &str) -> Option<&ConfigSource> {
        self.sources.get(key)
    }

    /// Get unknown environment variables
    pub fn unknown_vars(&self) -> &[String] {
        &self.unknown_vars
    }

    /// Get validation errors
    pub fn validation_errors(&self) -> &[String] {
        &self.validation_errors
    }

    /// Get configuration hash
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Check if there are validation errors
    pub fn has_errors(&self) -> bool {
        !self.validation_errors.is_empty()
    }

    /// Check if there are unknown variables
    pub fn has_unknown_vars(&self) -> bool {
        !self.unknown_vars.is_empty()
    }

    /// Get deprecated variables that are set
    pub fn deprecated_vars_in_use(&self) -> Vec<(&str, &str, &str)> {
        let mut deprecated = Vec::new();
        for name in self.values.keys() {
            if let Some(var) = self.schema.get_variable(name) {
                if let Some(dep) = &var.deprecated {
                    deprecated.push((
                        name.as_str(),
                        dep.replacement.as_str(),
                        dep.notes.as_deref().unwrap_or(""),
                    ));
                }
            }
        }
        deprecated
    }

    /// Format a validation report for logging
    pub fn validation_report(&self) -> String {
        let mut report = String::new();

        if self.has_errors() {
            report.push_str("Configuration Errors:\n");
            for err in &self.validation_errors {
                report.push_str(&format!("  - {}\n", err));
            }
        }

        if self.has_unknown_vars() {
            report.push_str("\nUnknown AOS_* Variables:\n");
            for var in &self.unknown_vars {
                report.push_str(&format!("  - {}\n", var));
            }
        }

        let deprecated = self.deprecated_vars_in_use();
        if !deprecated.is_empty() {
            report.push_str("\nDeprecated Variables:\n");
            for (name, replacement, notes) in deprecated {
                report.push_str(&format!("  - {} -> {}", name, replacement));
                if !notes.is_empty() {
                    report.push_str(&format!(" ({})", notes));
                }
                report.push('\n');
            }
        }

        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestEnvGuard;
    use std::str::FromStr;

    #[test]
    fn parses_all_canonical_and_aliases() {
        // Canonical (underscore)
        assert!(matches!(
            StorageBackend::from_str("sql_only").unwrap(),
            StorageBackend::Sql
        ));
        assert!(matches!(
            StorageBackend::from_str("dual_write").unwrap(),
            StorageBackend::Dual
        ));
        assert!(matches!(
            StorageBackend::from_str("kv_primary").unwrap(),
            StorageBackend::KvPrimary
        ));
        assert!(matches!(
            StorageBackend::from_str("kv_only").unwrap(),
            StorageBackend::KvOnly
        ));

        // Aliases
        assert!(matches!(
            StorageBackend::from_str("sql").unwrap(),
            StorageBackend::Sql
        ));
        assert!(matches!(
            StorageBackend::from_str("dual").unwrap(),
            StorageBackend::Dual
        ));
        assert!(matches!(
            StorageBackend::from_str("kv-primary").unwrap(),
            StorageBackend::KvPrimary
        ));
        assert!(matches!(
            StorageBackend::from_str("kv-only").unwrap(),
            StorageBackend::KvOnly
        ));
    }

    #[test]
    fn rejects_garbage_values() {
        assert!(StorageBackend::from_str("bogus").is_err());
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(RuntimeConfig::parse_duration_ms("30s").unwrap(), 30_000);
        assert_eq!(RuntimeConfig::parse_duration_ms("5m").unwrap(), 300_000);
        assert_eq!(RuntimeConfig::parse_duration_ms("1h").unwrap(), 3_600_000);
        assert_eq!(RuntimeConfig::parse_duration_ms("500ms").unwrap(), 500);
        assert_eq!(RuntimeConfig::parse_duration_ms("30").unwrap(), 30_000);
    }

    #[test]
    fn test_parse_byte_size() {
        assert_eq!(RuntimeConfig::parse_byte_size("1024").unwrap(), 1024);
        assert_eq!(RuntimeConfig::parse_byte_size("1KB").unwrap(), 1024);
        assert_eq!(RuntimeConfig::parse_byte_size("1MB").unwrap(), 1024 * 1024);
        assert_eq!(
            RuntimeConfig::parse_byte_size("1GB").unwrap(),
            1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_parsed_value_accessors() {
        let s = ParsedValue::String("test".to_string());
        assert_eq!(s.as_str(), Some("test"));
        assert_eq!(s.as_i64(), None);

        let i = ParsedValue::Integer(42);
        assert_eq!(i.as_i64(), Some(42));
        assert_eq!(i.as_str(), None);

        let b = ParsedValue::Bool(true);
        assert_eq!(b.as_bool(), Some(true));
    }

    #[test]
    fn test_runtime_config_defaults() {
        let _env = TestEnvGuard::new();
        // Clear any test pollution
        std::env::remove_var("AOS_SERVER_PORT");
        std::env::remove_var("AOS_LOG_LEVEL");

        let config = RuntimeConfig::from_env().unwrap();

        // Check defaults are applied
        assert_eq!(config.server_port(), 8080);
        assert_eq!(config.server_host(), "127.0.0.1");
        assert_eq!(config.log_level(), "info");
        assert_eq!(config.runtime_mode(), "development");
    }
}
