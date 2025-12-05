//! Effective configuration with type-safe sections
//!
//! This module provides a unified configuration facade that combines:
//! - TOML config file (cp.toml) as the manifest layer
//! - Environment variables (AOS_*) at higher precedence
//! - CLI arguments at highest precedence
//!
//! # Usage
//!
//! ```rust,ignore
//! use adapteros_config::{init_effective_config, effective_config};
//!
//! // Initialize at startup (call once)
//! init_effective_config("configs/cp.toml")?;
//!
//! // Access anywhere in the codebase
//! let cfg = effective_config();
//! println!("Server port: {}", cfg.server.port);
//! println!("Database: {}", cfg.database.path);
//! ```

use crate::precedence::DeterministicConfig;
use crate::ConfigLoader;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Global effective configuration instance
static EFFECTIVE_CONFIG: OnceLock<EffectiveConfig> = OnceLock::new();

/// Unified effective configuration with type-safe sections
///
/// This replaces both the old `adapteros-config::RuntimeConfig` and
/// `adapteros-server-api::Config` with a single source of truth.
#[derive(Debug, Clone)]
pub struct EffectiveConfig {
    /// The underlying deterministic config (for hashing, freeze checks)
    inner: DeterministicConfig,
    /// Server configuration
    pub server: ServerSection,
    /// Database configuration
    pub database: DatabaseSection,
    /// Security configuration
    pub security: SecuritySection,
    /// Path configuration
    pub paths: PathsSection,
    /// Logging configuration
    pub logging: LoggingSection,
    /// Rate limit configuration
    pub rate_limits: RateLimitsSection,
    /// Metrics configuration
    pub metrics: MetricsSection,
    /// Alerting configuration
    pub alerting: AlertingSection,
    /// Model configuration
    pub model: ModelSection,
    /// Source tracking for each config key
    sources: HashMap<String, String>,
    /// Whether running in production mode
    is_production: bool,
}

/// Server section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSection {
    /// Server port (1-65535, default: 8080)
    pub port: u16,
    /// Bind address (default: 127.0.0.1)
    pub host: String,
    /// Production mode flag
    pub production_mode: bool,
    /// Unix domain socket path (required in production)
    pub uds_socket: Option<PathBuf>,
    /// Drain timeout in seconds during shutdown (default: 30)
    pub drain_timeout_secs: u64,
    /// Number of worker threads (default: 4)
    pub workers: u16,
}

/// Database section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseSection {
    /// Database connection URL (sqlite://path or postgres://...)
    pub url: String,
    /// Connection pool size (default: 10)
    pub pool_size: u32,
    /// Connection timeout in seconds (default: 30)
    pub timeout_secs: u64,
}

/// Security section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecuritySection {
    /// JWT signing secret (sensitive, redacted in logs)
    pub jwt_secret: String,
    /// JWT signing mode: "eddsa" or "hmac"
    pub jwt_mode: String,
    /// JWT TTL in hours (default: 8)
    pub jwt_ttl_hours: u32,
    /// Require packet filter deny rules
    pub require_pf_deny: bool,
    /// Dev login bypass enabled (should be false in production)
    pub dev_login_enabled: bool,
    /// Ed25519 signing key for manifests (sensitive)
    pub signing_key: Option<String>,
}

/// Path section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsSection {
    /// Base var directory (default: var)
    pub var_dir: PathBuf,
    /// Adapters root directory
    pub adapters_root: PathBuf,
    /// Artifacts root directory
    pub artifacts_root: PathBuf,
    /// Datasets root directory
    pub datasets_root: PathBuf,
    /// Documents root directory
    pub documents_root: PathBuf,
    /// Bundles root directory
    pub bundles_root: PathBuf,
    /// Model cache directory
    pub model_cache_dir: PathBuf,
}

/// Logging section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingSection {
    /// Log level filter
    pub level: String,
    /// Log directory (None = stdout only)
    pub log_dir: Option<PathBuf>,
    /// Log file prefix
    pub log_prefix: String,
    /// JSON format output
    pub json_format: bool,
    /// Rotation strategy: hourly, daily, never
    pub rotation: String,
    /// Max rotated log files to keep
    pub max_log_files: usize,
}

/// Rate limits section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitsSection {
    /// Requests per minute limit
    pub requests_per_minute: u32,
    /// Burst size for rate limiting
    pub burst_size: u32,
    /// Inference requests per minute
    pub inference_per_minute: u32,
}

/// Metrics section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSection {
    /// Metrics enabled
    pub enabled: bool,
    /// Bearer token for metrics endpoint
    pub bearer_token: String,
    /// Include histogram metrics
    pub include_histogram: bool,
}

/// Alerting section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingSection {
    /// Alerting enabled
    pub enabled: bool,
    /// Alert directory
    pub alert_dir: PathBuf,
    /// Max alerts per file
    pub max_alerts_per_file: usize,
    /// Rotate size in MB
    pub rotate_size_mb: u64,
}

/// Model section configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSection {
    /// Path to model directory
    pub path: Option<PathBuf>,
    /// Model backend: auto, coreml, metal, mlx
    pub backend: String,
    /// Tokenizer path
    pub tokenizer_path: Option<PathBuf>,
    /// Manifest path
    pub manifest_path: Option<PathBuf>,
}

/// Source of a configuration value (for debugging/observability)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigValueSource {
    /// Value from TOML config file
    Toml(String),
    /// Value from environment variable
    Environment(String),
    /// Value from CLI argument
    Cli,
    /// Default value from schema
    Default,
}

impl std::fmt::Display for ConfigValueSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigValueSource::Toml(path) => write!(f, "toml:{}", path),
            ConfigValueSource::Environment(var) => write!(f, "env:{}", var),
            ConfigValueSource::Cli => write!(f, "cli"),
            ConfigValueSource::Default => write!(f, "default"),
        }
    }
}

impl EffectiveConfig {
    /// Build EffectiveConfig from DeterministicConfig
    pub fn from_deterministic(config: DeterministicConfig) -> Result<Self> {
        let sources = Self::build_sources(&config);
        let is_production = config
            .get("server.production.mode")
            .map(|v| v == "true")
            .unwrap_or(false);

        let server = Self::build_server_section(&config)?;
        let database = Self::build_database_section(&config)?;
        let security = Self::build_security_section(&config)?;
        let paths = Self::build_paths_section(&config)?;
        let logging = Self::build_logging_section(&config)?;
        let rate_limits = Self::build_rate_limits_section(&config);
        let metrics = Self::build_metrics_section(&config);
        let alerting = Self::build_alerting_section(&config);
        let model = Self::build_model_section(&config);

        let effective_config = Self {
            inner: config,
            server,
            database,
            security,
            paths,
            logging,
            rate_limits,
            metrics,
            alerting,
            model,
            sources,
            is_production,
        };

        // Validate production paths
        effective_config.validate_production_paths()?;

        // Validate critical configuration values
        effective_config.validate_critical_config()?;

        Ok(effective_config)
    }

    /// Get the configuration hash (BLAKE3)
    pub fn config_hash(&self) -> &str {
        &self.inner.get_metadata().hash
    }

    /// Check if running in production mode
    pub fn is_production(&self) -> bool {
        self.is_production
    }

    /// Get the source of a configuration value
    pub fn get_source(&self, key: &str) -> Option<&String> {
        self.sources.get(key)
    }

    /// Get all sources for debugging
    pub fn all_sources(&self) -> &HashMap<String, String> {
        &self.sources
    }

    /// Get a raw value by key
    pub fn get(&self, key: &str) -> Option<&String> {
        self.inner.get(key)
    }

    /// Get all raw values
    pub fn all_values(&self) -> &HashMap<String, String> {
        self.inner.get_all()
    }

    /// Get the underlying deterministic config
    pub fn inner(&self) -> &DeterministicConfig {
        &self.inner
    }

    /// Validate that all critical paths are absolute in production mode
    ///
    /// Returns Ok(()) if:
    /// - Not in production mode, OR
    /// - All critical paths are absolute (start with /)
    ///
    /// Returns error if production mode requires absolute paths but relative paths are found.
    pub fn validate_production_paths(&self) -> Result<()> {
        // Skip validation if not in production mode
        if !self.is_production {
            return Ok(());
        }

        let mut errors = Vec::new();

        // Check database URL path (extract from sqlite:// URLs)
        if let Some(db_path) = self.extract_db_path(&self.database.url) {
            let path = std::path::Path::new(&db_path);
            if !Self::is_absolute_or_url(path) {
                errors.push(format!(
                    "Production mode requires absolute path for database.url: got '{}'\nHint: Use absolute paths starting with / (e.g., /var/lib/adapteros/aos-cp.sqlite3)",
                    db_path
                ));
            }
        }

        // Check adapters_root
        if !Self::is_absolute_or_url(&self.paths.adapters_root) {
            errors.push(format!(
                "Production mode requires absolute path for paths.adapters_root: got '{}'\nHint: Use absolute paths starting with / (e.g., /var/lib/adapteros/adapters)",
                self.paths.adapters_root.display()
            ));
        }

        // Check datasets_root
        if !Self::is_absolute_or_url(&self.paths.datasets_root) {
            errors.push(format!(
                "Production mode requires absolute path for paths.datasets_root: got '{}'\nHint: Use absolute paths starting with / (e.g., /var/lib/adapteros/datasets)",
                self.paths.datasets_root.display()
            ));
        }

        // Check documents_root
        if !Self::is_absolute_or_url(&self.paths.documents_root) {
            errors.push(format!(
                "Production mode requires absolute path for paths.documents_root: got '{}'\nHint: Use absolute paths starting with / (e.g., /var/lib/adapteros/documents)",
                self.paths.documents_root.display()
            ));
        }

        // Check log_dir if set
        if let Some(ref log_dir) = self.logging.log_dir {
            if !Self::is_absolute_or_url(log_dir) {
                errors.push(format!(
                    "Production mode requires absolute path for logging.log_dir: got '{}'\nHint: Use absolute paths starting with / (e.g., /var/log/adapteros)",
                    log_dir.display()
                ));
            }
        }

        if !errors.is_empty() {
            return Err(AosError::Config(errors.join("\n")));
        }

        Ok(())
    }

    /// Validate critical configuration values
    ///
    /// Ensures that security-critical configuration values are set correctly.
    /// Some checks only apply in production mode.
    pub fn validate_critical_config(&self) -> Result<()> {
        let mut errors = Vec::new();

        // Rate limits must be positive
        if self.rate_limits.requests_per_minute == 0 {
            errors.push("rate_limits.requests_per_minute must be > 0".to_string());
        }
        if self.rate_limits.burst_size == 0 {
            errors.push("rate_limits.burst_size must be > 0".to_string());
        }
        if self.rate_limits.inference_per_minute == 0 {
            errors.push("rate_limits.inference_per_minute must be > 0".to_string());
        }

        // Production-only checks
        if self.is_production {
            // JWT secret must be set in production (not empty or default)
            if self.security.jwt_secret.is_empty()
                || self.security.jwt_secret == "change-me-in-production"
            {
                errors.push(
                    "Production mode requires security.jwt_secret to be set to a secure value\n\
                     Hint: Generate with: openssl rand -base64 32"
                        .to_string(),
                );
            }

            // Database URL must be set
            if self.database.url.is_empty() {
                errors.push("Production mode requires database.url to be set".to_string());
            }
        }

        if !errors.is_empty() {
            return Err(AosError::Config(errors.join("\n")));
        }

        Ok(())
    }

    /// Check if a path is absolute or a URL
    fn is_absolute_or_url(path: &std::path::Path) -> bool {
        // Check if it's an absolute path
        if path.is_absolute() {
            return true;
        }

        // Check if it's a URL (contains ://)
        if let Some(path_str) = path.to_str() {
            if path_str.contains("://") {
                return true;
            }
        }

        false
    }

    /// Extract filesystem path from database URL
    ///
    /// Handles URLs like:
    /// - "sqlite://var/aos-cp.sqlite3" -> Some("var/aos-cp.sqlite3")
    /// - "sqlite:///var/lib/adapteros/aos-cp.sqlite3" -> Some("/var/lib/adapteros/aos-cp.sqlite3")
    /// - "postgres://..." -> None (not a file path)
    fn extract_db_path(&self, url: &str) -> Option<String> {
        if let Some(stripped) = url.strip_prefix("sqlite://") {
            Some(stripped.to_string())
        } else {
            None
        }
    }

    // Builder methods for each section
    fn build_sources(config: &DeterministicConfig) -> HashMap<String, String> {
        let mut sources = HashMap::new();
        for source in &config.get_metadata().sources {
            sources.insert(source.key.clone(), source.source.clone());
        }
        sources
    }

    fn build_server_section(config: &DeterministicConfig) -> Result<ServerSection> {
        Ok(ServerSection {
            port: config
                .get("server.port")
                .and_then(|v| v.parse().ok())
                .unwrap_or(8080),
            host: config
                .get("server.host")
                .cloned()
                .unwrap_or_else(|| "127.0.0.1".to_string()),
            production_mode: config
                .get("server.production.mode")
                .map(|v| v == "true")
                .unwrap_or(false),
            uds_socket: config.get("server.uds.socket").map(PathBuf::from),
            drain_timeout_secs: config
                .get("server.drain.timeout.secs")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            workers: config
                .get("server.workers")
                .and_then(|v| v.parse().ok())
                .unwrap_or(4),
        })
    }

    fn build_database_section(config: &DeterministicConfig) -> Result<DatabaseSection> {
        Ok(DatabaseSection {
            url: config
                .get("database.url")
                .cloned()
                .unwrap_or_else(|| "sqlite://var/aos-cp.sqlite3".to_string()),
            pool_size: config
                .get("database.pool.size")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            timeout_secs: config
                .get("database.timeout")
                .and_then(|v| Self::parse_duration_secs(v))
                .unwrap_or(30),
        })
    }

    fn build_security_section(config: &DeterministicConfig) -> Result<SecuritySection> {
        Ok(SecuritySection {
            jwt_secret: config
                .get("security.jwt.secret")
                .cloned()
                .unwrap_or_default(),
            jwt_mode: config
                .get("security.jwt.mode")
                .cloned()
                .unwrap_or_else(|| "hmac".to_string()),
            jwt_ttl_hours: config
                .get("security.jwt.ttl")
                .and_then(|v| Self::parse_duration_hours(v))
                .unwrap_or(8),
            require_pf_deny: config
                .get("security.pf.deny")
                .map(|v| v == "true")
                .unwrap_or(false),
            dev_login_enabled: config
                .get("security.dev.login.enabled")
                .map(|v| v == "true")
                .unwrap_or(false),
            signing_key: config.get("signing.key").cloned(),
        })
    }

    fn build_paths_section(config: &DeterministicConfig) -> Result<PathsSection> {
        Ok(PathsSection {
            var_dir: config
                .get("var.dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("var")),
            adapters_root: config
                .get("adapters.dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("var/adapters")),
            artifacts_root: config
                .get("artifacts.dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("var/artifacts")),
            datasets_root: config
                .get("paths.datasets.root")
                .or_else(|| config.get("datasets.root"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("var/datasets")),
            documents_root: config
                .get("paths.documents.root")
                .or_else(|| config.get("documents.root"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("var/documents")),
            bundles_root: config
                .get("paths.bundles.root")
                .or_else(|| config.get("bundles.root"))
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("var/bundles")),
            model_cache_dir: config
                .get("model.cache.dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("var/model-cache")),
        })
    }

    fn build_logging_section(config: &DeterministicConfig) -> Result<LoggingSection> {
        Ok(LoggingSection {
            level: config
                .get("log.level")
                .cloned()
                .unwrap_or_else(|| "info".to_string()),
            log_dir: config.get("log.file").map(PathBuf::from),
            log_prefix: config
                .get("logging.log.prefix")
                .cloned()
                .unwrap_or_else(|| "aos-cp".to_string()),
            json_format: config
                .get("log.format")
                .map(|v| v == "json")
                .unwrap_or(false),
            rotation: config
                .get("logging.rotation")
                .cloned()
                .unwrap_or_else(|| "daily".to_string()),
            max_log_files: config
                .get("logging.max.log.files")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
        })
    }

    fn build_rate_limits_section(config: &DeterministicConfig) -> RateLimitsSection {
        RateLimitsSection {
            requests_per_minute: config
                .get("rate.limits.requests.per.minute")
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
            burst_size: config
                .get("rate.limits.burst.size")
                .and_then(|v| v.parse().ok())
                .unwrap_or(20),
            inference_per_minute: config
                .get("rate.limits.inference.per.minute")
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        }
    }

    fn build_metrics_section(config: &DeterministicConfig) -> MetricsSection {
        MetricsSection {
            enabled: config
                .get("metrics.enabled")
                .map(|v| v == "true")
                .unwrap_or(true),
            bearer_token: config
                .get("metrics.bearer.token")
                .cloned()
                .unwrap_or_default(),
            include_histogram: config
                .get("metrics.include.histogram")
                .map(|v| v == "true")
                .unwrap_or(true),
        }
    }

    fn build_alerting_section(config: &DeterministicConfig) -> AlertingSection {
        AlertingSection {
            enabled: config
                .get("alerting.enabled")
                .map(|v| v == "true")
                .unwrap_or(true),
            alert_dir: config
                .get("alerting.alert.dir")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("var/alerts")),
            max_alerts_per_file: config
                .get("alerting.max.alerts.per.file")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000),
            rotate_size_mb: config
                .get("alerting.rotate.size.mb")
                .and_then(|v| v.parse().ok())
                .unwrap_or(100),
        }
    }

    fn build_model_section(config: &DeterministicConfig) -> ModelSection {
        ModelSection {
            path: config.get("model.path").map(PathBuf::from),
            backend: config
                .get("model.backend")
                .cloned()
                .unwrap_or_else(|| "auto".to_string()),
            tokenizer_path: config.get("tokenizer.path").map(PathBuf::from),
            manifest_path: config.get("manifest.path").map(PathBuf::from),
        }
    }

    // Helper to parse duration strings to seconds
    fn parse_duration_secs(value: &str) -> Option<u64> {
        // Handle "30s", "5m", "1h" etc
        let value = value.trim();
        if let Ok(secs) = value.parse::<u64>() {
            return Some(secs);
        }
        if let Some(stripped) = value.strip_suffix("ms") {
            return stripped.trim().parse::<u64>().ok().map(|ms| ms / 1000);
        }
        if let Some(stripped) = value.strip_suffix('s') {
            return stripped.trim().parse().ok();
        }
        if let Some(stripped) = value.strip_suffix('m') {
            return stripped.trim().parse::<u64>().ok().map(|m| m * 60);
        }
        if let Some(stripped) = value.strip_suffix('h') {
            return stripped.trim().parse::<u64>().ok().map(|h| h * 3600);
        }
        None
    }

    // Helper to parse duration strings to hours
    fn parse_duration_hours(value: &str) -> Option<u32> {
        Self::parse_duration_secs(value).map(|s| (s / 3600) as u32)
    }
}

/// Initialize the global effective configuration
///
/// Call this once at startup. The config will be frozen and immutable after initialization.
///
/// # Arguments
/// * `toml_path` - Optional path to the TOML config file (e.g., "configs/cp.toml")
/// * `cli_args` - CLI arguments in `--key value` format
///
/// # Example
/// ```rust,ignore
/// init_effective_config(Some("configs/cp.toml"), vec![])?;
/// ```
pub fn init_effective_config(
    toml_path: Option<&str>,
    cli_args: Vec<String>,
) -> Result<&'static EffectiveConfig> {
    // Load .env file first
    crate::model::load_dotenv();

    // Load via ConfigLoader with precedence: CLI > ENV > TOML > defaults
    let loader = ConfigLoader::new();
    let config = loader.load(cli_args, toml_path.map(String::from))?;

    // Build EffectiveConfig from DeterministicConfig
    let effective = EffectiveConfig::from_deterministic(config)?;

    EFFECTIVE_CONFIG
        .set(effective)
        .map_err(|_| AosError::Config("EffectiveConfig already initialized".to_string()))?;

    Ok(EFFECTIVE_CONFIG.get().unwrap())
}

/// Get the global effective configuration
///
/// Returns an error if `init_effective_config` hasn't been called yet.
pub fn effective_config() -> Result<&'static EffectiveConfig> {
    EFFECTIVE_CONFIG.get().ok_or_else(|| {
        AosError::Config(
            "EffectiveConfig not initialized. Call init_effective_config() first.".to_string(),
        )
    })
}

/// Try to get the global effective configuration (returns None if not initialized)
pub fn try_effective_config() -> Option<&'static EffectiveConfig> {
    EFFECTIVE_CONFIG.get()
}

/// Check if effective configuration is initialized
pub fn is_effective_initialized() -> bool {
    EFFECTIVE_CONFIG.get().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_secs() {
        assert_eq!(EffectiveConfig::parse_duration_secs("30"), Some(30));
        assert_eq!(EffectiveConfig::parse_duration_secs("30s"), Some(30));
        assert_eq!(EffectiveConfig::parse_duration_secs("5m"), Some(300));
        assert_eq!(EffectiveConfig::parse_duration_secs("1h"), Some(3600));
        assert_eq!(EffectiveConfig::parse_duration_secs("500ms"), Some(0)); // 500ms = 0s
        assert_eq!(EffectiveConfig::parse_duration_secs("invalid"), None);
    }

    #[test]
    fn test_config_value_source_display() {
        assert_eq!(
            ConfigValueSource::Toml("cp.toml".to_string()).to_string(),
            "toml:cp.toml"
        );
        assert_eq!(
            ConfigValueSource::Environment("AOS_SERVER_PORT".to_string()).to_string(),
            "env:AOS_SERVER_PORT"
        );
        assert_eq!(ConfigValueSource::Cli.to_string(), "cli");
        assert_eq!(ConfigValueSource::Default.to_string(), "default");
    }

    #[test]
    fn test_production_path_validation() {
        use crate::precedence::DeterministicConfig;
        use std::collections::HashMap;

        // Test case 1: Development mode with relative paths - should pass
        let mut dev_values = HashMap::new();
        dev_values.insert("server.production.mode".to_string(), "false".to_string());
        dev_values.insert(
            "database.url".to_string(),
            "sqlite://var/aos-cp.sqlite3".to_string(),
        );
        dev_values.insert("adapters.dir".to_string(), "var/adapters".to_string());
        dev_values.insert(
            "paths.datasets.root".to_string(),
            "var/datasets".to_string(),
        );
        dev_values.insert(
            "paths.documents.root".to_string(),
            "var/documents".to_string(),
        );
        dev_values.insert("log.file".to_string(), "var/logs".to_string());

        let dev_config = DeterministicConfig::new_for_test(dev_values);
        let dev_effective = EffectiveConfig::from_deterministic(dev_config);
        assert!(
            dev_effective.is_ok(),
            "Development mode should accept relative paths"
        );

        // Test case 2: Production mode with absolute paths - should pass
        let mut prod_abs_values = HashMap::new();
        prod_abs_values.insert("server.production.mode".to_string(), "true".to_string());
        prod_abs_values.insert(
            "database.url".to_string(),
            "sqlite:///var/lib/adapteros/aos-cp.sqlite3".to_string(),
        );
        prod_abs_values.insert(
            "adapters.dir".to_string(),
            "/var/lib/adapteros/adapters".to_string(),
        );
        prod_abs_values.insert(
            "paths.datasets.root".to_string(),
            "/var/lib/adapteros/datasets".to_string(),
        );
        prod_abs_values.insert(
            "paths.documents.root".to_string(),
            "/var/lib/adapteros/documents".to_string(),
        );
        prod_abs_values.insert("log.file".to_string(), "/var/log/adapteros".to_string());

        let prod_abs_config = DeterministicConfig::new_for_test(prod_abs_values);
        let prod_abs_effective = EffectiveConfig::from_deterministic(prod_abs_config);
        assert!(
            prod_abs_effective.is_ok(),
            "Production mode should accept absolute paths"
        );

        // Test case 3: Production mode with relative paths - should fail
        let mut prod_rel_values = HashMap::new();
        prod_rel_values.insert("server.production.mode".to_string(), "true".to_string());
        prod_rel_values.insert(
            "database.url".to_string(),
            "sqlite://var/aos-cp.sqlite3".to_string(),
        );
        prod_rel_values.insert("adapters.dir".to_string(), "var/adapters".to_string());
        prod_rel_values.insert(
            "paths.datasets.root".to_string(),
            "var/datasets".to_string(),
        );
        prod_rel_values.insert(
            "paths.documents.root".to_string(),
            "var/documents".to_string(),
        );

        let prod_rel_config = DeterministicConfig::new_for_test(prod_rel_values);
        let prod_rel_effective = EffectiveConfig::from_deterministic(prod_rel_config);
        assert!(
            prod_rel_effective.is_err(),
            "Production mode should reject relative paths"
        );

        let err = prod_rel_effective.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("Production mode requires absolute path"),
            "Error should mention production mode"
        );
        assert!(
            err_msg.contains("database.url"),
            "Error should mention database.url"
        );
        assert!(
            err_msg.contains("paths.adapters_root"),
            "Error should mention paths.adapters_root"
        );
        assert!(err_msg.contains("Hint:"), "Error should include hints");

        // Test case 4: Production mode with mixed paths - should fail
        let mut prod_mixed_values = HashMap::new();
        prod_mixed_values.insert("server.production.mode".to_string(), "true".to_string());
        prod_mixed_values.insert(
            "database.url".to_string(),
            "sqlite:///var/lib/adapteros/aos-cp.sqlite3".to_string(),
        );
        prod_mixed_values.insert("adapters.dir".to_string(), "var/adapters".to_string()); // Relative
        prod_mixed_values.insert(
            "paths.datasets.root".to_string(),
            "/var/lib/adapteros/datasets".to_string(),
        );
        prod_mixed_values.insert(
            "paths.documents.root".to_string(),
            "/var/lib/adapteros/documents".to_string(),
        );

        let prod_mixed_config = DeterministicConfig::new_for_test(prod_mixed_values);
        let prod_mixed_effective = EffectiveConfig::from_deterministic(prod_mixed_config);
        assert!(
            prod_mixed_effective.is_err(),
            "Production mode should reject mixed paths"
        );
    }
}
