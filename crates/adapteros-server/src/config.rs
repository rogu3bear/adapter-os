use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;

pub use adapteros_config::{
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
    pub git: Option<adapteros_git::GitConfig>,
    #[serde(default)]
    pub policies: PoliciesConfig,
    #[serde(default)]
    pub routing: RoutingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoutingConfig {
    /// Allow routing to inherit session.stack_id when no explicit adapters/stack_id provided
    #[serde(default)]
    pub use_session_stack_for_routing: bool,
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }
}
