use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub db: DatabaseConfig,
    pub security: SecurityConfig,
    pub paths: PathsConfig,
    pub rate_limits: RateLimitsConfig,
    pub metrics: MetricsConfig,
    pub alerting: AlertingConfig,
    #[serde(default)]
    pub git: Option<adapteros_git::GitConfig>,
    #[serde(default)]
    pub policies: PoliciesConfig,
    #[serde(default)]
    pub routing: RoutingConfig,
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
}

fn default_drain_timeout() -> u64 {
    30
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
    /// Token TTL in seconds (defaults to 8 hours)
    #[serde(default)]
    pub token_ttl_seconds: Option<u64>,
    /// JWT algorithm mode (eddsa or hs256)
    #[serde(default)]
    pub jwt_mode: Option<String>,
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
    "var/adapters".to_string()
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
pub struct RoutingConfig {
    /// Allow routing to inherit session.stack_id when no explicit adapters/stack_id provided
    #[serde(default)]
    pub use_session_stack_for_routing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoliciesConfig {
    #[serde(default)]
    pub drift: adapteros_core::DriftPolicy,
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }
}
