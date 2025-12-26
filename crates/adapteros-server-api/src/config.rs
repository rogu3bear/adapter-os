use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fs;

pub use adapteros_config_types::{
    AlertingConfig, AuthConfig, DatabaseConfig, InvariantsConfig, MetricsConfig, PathsConfig,
    PoliciesConfig, RateLimitsConfig, SecurityConfig, ServerConfig,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub db: DatabaseConfig,
    pub security: SecurityConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    pub paths: PathsConfig,
    pub rate_limits: RateLimitsConfig,
    pub metrics: MetricsConfig,
    pub alerting: AlertingConfig,
    #[serde(default)]
    pub self_hosting: SelfHostingConfig,
    #[serde(default)]
    pub git: Option<adapteros_git::GitConfig>,
    #[serde(default)]
    pub policies: PoliciesConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    /// OpenTelemetry distributed tracing configuration
    #[serde(default)]
    pub otel: OtelConfig,
    /// Boot invariant check configuration (escape hatch for incidents)
    #[serde(default)]
    pub invariants: InvariantsConfig,
}

fn default_self_hosting_mode() -> String {
    "off".to_string()
}

fn default_self_hosting_threshold() -> f64 {
    0.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SelfHostingConfig {
    /// Self-hosting agent mode: off/on/safe
    #[serde(default = "default_self_hosting_mode")]
    pub mode: String,
    /// Repo IDs the agent is allowed to manage
    #[serde(default)]
    pub repo_allowlist: Vec<String>,
    /// Minimum evaluation score required for auto-promotion (on mode)
    #[serde(default = "default_self_hosting_threshold")]
    pub promotion_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level filter (e.g., "info", "debug", "aos_cp=debug,tower_http=info")
    #[serde(default = "default_log_level")]
    pub level: String,
    /// Directory for log files (None = stdout only)
    #[serde(default)]
    pub log_dir: Option<String>,
    /// Log file prefix (default: "aos-cp")
    #[serde(default = "default_log_prefix")]
    pub log_prefix: String,
    /// Enable JSON format for logs (useful for log aggregation)
    #[serde(default)]
    pub json_format: bool,
    /// Rotation strategy: "hourly", "daily", or "never" (default: "daily")
    #[serde(default = "default_rotation")]
    pub rotation: String,
    /// Maximum number of rotated log files to keep (0 = unlimited)
    #[serde(default)]
    pub max_log_files: usize,
    /// Log retention period in days (0 = keep forever, default: 14)
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    /// Include request IDs in log output
    #[serde(default = "adapteros_config_types::default_true")]
    pub include_request_id: bool,
    /// Enable panic capture to log file
    #[serde(default = "adapteros_config_types::default_true")]
    pub capture_panics: bool,
}

fn default_log_level() -> String {
    "aos_cp=info,aos_cp_api=info,tower_http=debug".to_string()
}

fn default_log_prefix() -> String {
    "aos-cp".to_string()
}

fn default_rotation() -> String {
    "daily".to_string()
}

fn default_retention_days() -> u32 {
    14
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            log_dir: None,
            log_prefix: default_log_prefix(),
            json_format: false,
            rotation: default_rotation(),
            max_log_files: 0,
            retention_days: default_retention_days(),
            include_request_id: true,
            capture_panics: true,
        }
    }
}

/// OpenTelemetry distributed tracing configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OtelConfig {
    /// Enable OpenTelemetry tracing (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Service name for traces (default: "adapteros-server" or OTEL_SERVICE_NAME env var)
    #[serde(default = "default_service_name")]
    pub service_name: String,
    /// OTLP endpoint (default: "http://localhost:4317" or OTEL_EXPORTER_OTLP_ENDPOINT env var)
    #[serde(default = "default_otlp_endpoint")]
    pub endpoint: String,
    /// Protocol: "grpc" or "http" (default: "grpc" or OTEL_EXPORTER_OTLP_PROTOCOL env var)
    #[serde(default = "default_protocol")]
    pub protocol: String,
    /// Sampling ratio 0.0-1.0 (default: 1.0 = sample all)
    #[serde(default = "default_sampling_ratio")]
    pub sampling_ratio: f64,
    /// Export timeout in seconds (default: 10)
    #[serde(default = "default_export_timeout")]
    pub export_timeout_secs: u64,
    /// Batch export max queue size (default: 2048)
    #[serde(default = "default_max_queue_size")]
    pub max_queue_size: usize,
    /// Graceful shutdown timeout in seconds (default: 5)
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_secs: u64,
}

fn default_service_name() -> String {
    std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "adapteros-server".to_string())
}

fn default_otlp_endpoint() -> String {
    std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".to_string())
}

fn default_protocol() -> String {
    std::env::var("OTEL_EXPORTER_OTLP_PROTOCOL").unwrap_or_else(|_| "grpc".to_string())
}

fn default_sampling_ratio() -> f64 {
    std::env::var("OTEL_TRACES_SAMPLER_ARG")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0)
}

fn default_export_timeout() -> u64 {
    10
}

fn default_max_queue_size() -> usize {
    2048
}

fn default_shutdown_timeout() -> u64 {
    5
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_name: default_service_name(),
            endpoint: default_otlp_endpoint(),
            protocol: default_protocol(),
            sampling_ratio: default_sampling_ratio(),
            export_timeout_secs: default_export_timeout(),
            max_queue_size: default_max_queue_size(),
            shutdown_timeout_secs: default_shutdown_timeout(),
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Validate required secret material is present and non-trivial.
    pub fn validate_secrets(&self) -> Result<()> {
        let mut errors = Vec::new();

        let jwt_secret = self.security.jwt_secret.trim();
        if jwt_secret.is_empty() {
            errors.push("security.jwt_secret is required but missing".to_string());
        } else {
            let weak_markers = ["secret", "changeme", "password", "insecure"];
            if weak_markers
                .iter()
                .any(|w| jwt_secret.eq_ignore_ascii_case(w))
            {
                errors.push("security.jwt_secret uses a placeholder value".to_string());
            }
            if self.server.production_mode && jwt_secret.len() < 32 {
                errors.push(format!(
                    "security.jwt_secret must be at least 32 characters in production mode (got {})",
                    jwt_secret.len()
                ));
            }
        }

        if self.metrics.enabled && self.metrics.bearer_token.trim().is_empty() {
            errors.push("metrics.bearer_token must be set when metrics are enabled".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            bail!(errors.join("; "))
        }
    }
}

/// Check if production mode is enabled for the given config.
///
/// Production mode enables stricter security checks and disables dev features.
pub fn is_production(config: &Config) -> bool {
    config.server.production_mode
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_additional_jwt_keys() {
        let contents = r#"
[server]
port = 8080

[db]
path = "var/aos-cp.sqlite3"

[security]
jwt_secret = "secret"
jwt_additional_ed25519_public_keys = ["pem-1", "pem-2"]
jwt_additional_hmac_secrets = ["hmac-1", "hmac-2"]

[paths]
artifacts_root = "var/artifacts"
bundles_root = "var/bundles"

[rate_limits]
requests_per_minute = 100
burst_size = 50
inference_per_minute = 100

[metrics]
enabled = true
bearer_token = "token"
include_histogram = false
histogram_buckets = [0.1, 0.5, 1.0]

[alerting]
enabled = false
alert_dir = "var/alerts"
max_alerts_per_file = 10
rotate_size_mb = 5
        "#;

        let cfg: Config = toml::from_str(contents).expect("config parses");

        assert_eq!(
            cfg.security
                .jwt_additional_ed25519_public_keys
                .as_deref()
                .unwrap(),
            ["pem-1", "pem-2"]
        );
        assert_eq!(
            cfg.security.jwt_additional_hmac_secrets.as_deref().unwrap(),
            ["hmac-1", "hmac-2"]
        );
    }
}
