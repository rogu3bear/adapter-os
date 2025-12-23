use serde::{Deserialize, Serialize};

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
    /// Token TTL in seconds (defaults to 8 hours). Legacy, superseded by access/session TTLs.
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
pub struct PoliciesConfig {
    #[serde(default)]
    pub drift: adapteros_core::DriftPolicy,
}

pub fn default_drain_timeout() -> u64 {
    30
}

pub fn default_boot_timeout() -> u64 {
    300
}

pub fn default_health_check_db_timeout_ms() -> u64 {
    2000
}

pub fn default_health_check_worker_timeout_ms() -> u64 {
    2000
}

pub fn default_health_check_models_timeout_ms() -> u64 {
    2000
}

pub fn default_bind() -> String {
    "127.0.0.1".to_string()
}

pub fn default_storage_mode() -> String {
    "sql_only".to_string()
}

pub fn default_pool_size() -> u32 {
    20
}

pub fn default_kv_path() -> String {
    "var/aos-kv.redb".to_string()
}

pub fn default_true() -> bool {
    true
}

pub fn default_false() -> bool {
    false
}

pub fn default_jwt_ttl_hours() -> u32 {
    8
}

pub fn default_key_provider_mode() -> String {
    "keychain".to_string()
}

pub fn default_jwt_issuer() -> String {
    "adapteros".to_string()
}

pub fn default_access_token_ttl_seconds() -> u64 {
    15 * 60
}

pub fn default_session_ttl_seconds() -> u64 {
    12 * 3600
}

pub fn default_cookie_same_site() -> String {
    "Lax".to_string()
}

pub fn default_dev_algo() -> String {
    "hs256".to_string()
}

pub fn default_prod_algo() -> String {
    "eddsa".to_string()
}

pub fn default_lockout_threshold() -> u32 {
    5
}

pub fn default_lockout_cooldown() -> u64 {
    300
}

pub fn default_adapters_root() -> String {
    "var/adapters/repo".to_string()
}

pub fn default_plan_dir() -> String {
    "plan".to_string()
}

pub fn default_datasets_root() -> String {
    "var/datasets".to_string()
}

pub fn default_documents_root() -> String {
    "var/documents".to_string()
}

// ============================================================================
// Boot Invariants Configuration
// ============================================================================

/// Configuration for boot-time invariant checks.
/// Allows operators to disable specific checks during incidents.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InvariantsConfig {
    /// Disable SEC-001: Dev auth bypass check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_sec_001_dev_bypass: bool,
    /// Disable SEC-002: Dual-write strict mode check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_sec_002_dual_write: bool,
    /// Disable SEC-003: Executor manifest seed check (NOT RECOMMENDED)
    #[serde(default)]
    pub disable_sec_003_executor_seed: bool,
    /// Disable SEC-005: Cookie security check
    #[serde(default)]
    pub disable_sec_005_cookie_security: bool,
}
