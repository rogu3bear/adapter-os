use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default)]
    pub production_mode: bool,
    #[serde(default)]
    pub uds_socket: Option<String>,
    /// Timeout in seconds for draining in-flight requests during shutdown (default: 30)
    #[serde(default = "default_drain_timeout")]
    pub drain_timeout_secs: u64,
    /// Timeout in seconds for the entire boot sequence (default: 300)
    #[serde(default = "default_boot_timeout")]
    pub boot_timeout_secs: u64,
    /// Timeout in milliseconds for health check database probe (default: 2000)
    #[serde(default = "default_health_check_db_timeout_ms")]
    pub health_check_db_timeout_ms: u64,
    /// Timeout in milliseconds for health check worker probe (default: 2000)
    #[serde(default = "default_health_check_worker_timeout_ms")]
    pub health_check_worker_timeout_ms: u64,
    /// Timeout in milliseconds for health check models probe (default: 2000)
    #[serde(default = "default_health_check_models_timeout_ms")]
    pub health_check_models_timeout_ms: u64,
}

fn default_drain_timeout() -> u64 {
    30
}

fn default_boot_timeout() -> u64 {
    300
}

fn default_health_check_db_timeout_ms() -> u64 {
    2000
}

fn default_health_check_worker_timeout_ms() -> u64 {
    2000
}

fn default_health_check_models_timeout_ms() -> u64 {
    2000
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_storage_mode() -> String {
    "sql_only".to_string()
}

fn default_pool_size() -> u32 {
    20
}

fn default_self_hosting_mode() -> String {
    "off".to_string()
}

fn default_self_hosting_threshold() -> f64 {
    0.0
}

fn default_kv_path() -> String {
    "var/aos-kv.redb".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
    #[serde(default = "default_pool_size")]
    pub pool_size: u32,
    #[serde(default = "default_storage_mode")]
    pub storage_mode: String,
    #[serde(default = "default_kv_path")]
    pub kv_path: String,
    #[serde(default)]
    pub kv_tantivy_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub require_pf_deny: bool,
    #[serde(default = "default_false")]
    pub mtls_required: bool,
    pub jwt_secret: String,
    #[serde(default = "default_jwt_ttl_hours")]
    pub jwt_ttl_hours: u32,
    #[serde(default = "default_key_provider_mode")]
    pub key_provider_mode: String,
    #[serde(default)]
    pub key_file_path: Option<String>,
    #[serde(default = "default_jwt_issuer")]
    pub jwt_issuer: String,
    #[serde(default)]
    pub jwt_audience: Option<String>,
    /// Enable dev login bypass (defaults to false for security)
    #[serde(default = "default_false")]
    pub dev_login_enabled: bool,
    /// MFA requirement (defaults to false)
    #[serde(default)]
    pub require_mfa: Option<bool>,
    /// Token TTL in seconds (defaults to 8 hours) — legacy, superseded by access/session specific TTLs
    #[serde(default)]
    pub token_ttl_seconds: Option<u64>,
    /// Access token TTL in seconds (short-lived; defaults to 15 minutes)
    #[serde(default = "default_access_token_ttl_seconds")]
    pub access_token_ttl_seconds: u64,
    /// Session/cookie TTL in seconds (defaults to 12 hours)
    #[serde(default = "default_session_ttl_seconds")]
    pub session_ttl_seconds: u64,
    /// JWT algorithm mode (eddsa or hs256)
    #[serde(default)]
    pub jwt_mode: Option<String>,
    /// Additional Ed25519 public keys (PEM) accepted for JWT verification
    #[serde(default)]
    pub jwt_additional_ed25519_public_keys: Option<Vec<String>>,
    /// Additional HMAC secrets accepted for JWT verification
    #[serde(default)]
    pub jwt_additional_hmac_secrets: Option<Vec<String>>,
    /// Cookie SameSite policy: "Lax", "Strict", or "None" (default: Lax)
    #[serde(default = "default_cookie_same_site")]
    pub cookie_same_site: String,
    /// Optional cookie domain for split-origin dev setups
    #[serde(default)]
    pub cookie_domain: Option<String>,
    /// Force Secure flag on cookies (defaults to true in production_mode)
    #[serde(default)]
    pub cookie_secure: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthConfig {
    /// JWT algorithm to use in development (hs256/hmac)
    #[serde(default = "default_dev_algo")]
    pub dev_algo: String,
    /// JWT algorithm to use in production (eddsa/ed25519)
    #[serde(default = "default_prod_algo")]
    pub prod_algo: String,
    /// Session lifetime in seconds
    #[serde(default = "default_session_ttl_seconds")]
    pub session_lifetime: u64,
    /// Failed attempts before lockout
    #[serde(default = "default_lockout_threshold")]
    pub lockout_threshold: u32,
    /// Lockout cooldown in seconds
    #[serde(default = "default_lockout_cooldown")]
    pub lockout_cooldown: u64,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

fn default_jwt_ttl_hours() -> u32 {
    8
}

fn default_key_provider_mode() -> String {
    "keychain".to_string()
}

fn default_jwt_issuer() -> String {
    "adapteros".to_string()
}

fn default_access_token_ttl_seconds() -> u64 {
    15 * 60
}

fn default_session_ttl_seconds() -> u64 {
    12 * 3600
}

fn default_cookie_same_site() -> String {
    "Lax".to_string()
}

fn default_dev_algo() -> String {
    "hs256".to_string()
}

fn default_prod_algo() -> String {
    "eddsa".to_string()
}

fn default_lockout_threshold() -> u32 {
    5
}

fn default_lockout_cooldown() -> u64 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub artifacts_root: String,
    pub bundles_root: String,
    #[serde(default = "default_adapters_root")]
    pub adapters_root: String,
    #[serde(default = "default_plan_dir")]
    pub plan_dir: String,
    #[serde(default = "default_datasets_root")]
    pub datasets_root: String,
    #[serde(default = "default_documents_root")]
    pub documents_root: String,
}

fn default_adapters_root() -> String {
    "var/adapters/repo".to_string()
}

fn default_plan_dir() -> String {
    "plan".to_string()
}

fn default_datasets_root() -> String {
    "var/datasets".to_string()
}

fn default_documents_root() -> String {
    "var/documents".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitsConfig {
    pub requests_per_minute: u32,
    pub burst_size: u32,
    pub inference_per_minute: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub bearer_token: String,
    pub include_histogram: bool,
    pub histogram_buckets: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingConfig {
    pub enabled: bool,
    pub alert_dir: String,
    pub max_alerts_per_file: usize,
    pub rotate_size_mb: u64,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoliciesConfig {
    #[serde(default)]
    pub drift: adapteros_core::DriftPolicy,
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
    #[serde(default = "default_true")]
    pub include_request_id: bool,
    /// Enable panic capture to log file
    #[serde(default = "default_true")]
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
