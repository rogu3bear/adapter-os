use adapteros_verify::StrictnessLevel;
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
    pub cab: Option<CabConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    #[serde(default = "default_bind")]
    pub bind: String,
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_true")]
    pub require_pf_deny: bool,
    #[serde(default = "default_false")]
    pub mtls_required: bool,
    pub jwt_secret: String,
    /// JWT mode: "hmac" (default) or "eddsa"
    #[serde(default)]
    pub jwt_mode: Option<String>,
    /// Optional Ed25519 public key in PEM for JWT validation when jwt_mode = "eddsa"
    #[serde(default)]
    pub jwt_public_key_pem: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_false() -> bool {
    false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    pub artifacts_root: String,
    pub bundles_root: String,
    #[serde(default = "default_plan_dir")]
    pub plan_dir: String,
}

fn default_plan_dir() -> String {
    "plan".to_string()
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

/// Telemetry retention configuration for bundle GC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRetentionConfig {
    /// Keep last K bundles per CPID
    #[serde(default = "TelemetryRetentionConfig::default_keep")]
    pub keep_bundles_per_cpid: usize,
    /// Keep incident bundles from GC
    #[serde(default = "default_true")]
    pub keep_incident_bundles: bool,
    /// Keep promotion bundles from GC
    #[serde(default = "default_true")]
    pub keep_promotion_bundles: bool,
}

impl TelemetryRetentionConfig {
    fn default_keep() -> usize {
        12
    }
}

impl Default for TelemetryRetentionConfig {
    fn default() -> Self {
        Self {
            keep_bundles_per_cpid: Self::default_keep(),
            keep_incident_bundles: true,
            keep_promotion_bundles: true,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CabConfig {
    #[serde(default)]
    pub golden_gate: Option<GoldenGateConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoldenGateConfig {
    pub enabled: bool,
    pub baseline: String,
    /// Verification strictness level
    pub strictness: StrictnessLevel,
    #[serde(default)]
    pub skip_toolchain: bool,
    #[serde(default)]
    pub skip_signature: bool,
    #[serde(default)]
    pub verify_device: bool,
    #[serde(default)]
    pub bundle_path: Option<String>,
}
