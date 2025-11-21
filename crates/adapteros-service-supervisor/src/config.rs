//! Configuration management for the service supervisor

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Top-level supervisor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    pub server: ServerConfig,
    pub services: HashMap<String, ServiceConfig>,
    pub auth: AuthConfig,
    pub monitoring: MonitoringConfig,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub max_connections: usize,
    pub timeout_seconds: u64,
    pub cors_allowed_origins: Vec<String>,
    /// Unix Domain Socket path for production mode (egress policy compliance)
    pub uds_socket: Option<PathBuf>,
    /// Enable production mode (requires UDS, disables TCP)
    pub production_mode: bool,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub token_ttl_hours: i64,
    pub refresh_grace_period_minutes: i64,
    pub max_login_attempts: u32,
    pub lockout_duration_minutes: u64,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub metrics_enabled: bool,
    pub health_check_interval_seconds: u64,
    pub log_level: String,
    pub prometheus_port: Option<u16>,
}

/// Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub args: Vec<String>,
    pub working_directory: Option<PathBuf>,
    pub environment: HashMap<String, String>,
    pub port: Option<u16>,
    pub health_check: HealthCheckConfig,
    pub restart_policy: RestartPolicy,
    pub dependencies: Vec<String>,
    pub startup_order: i32,
    pub category: ServiceCategory,
    pub essential: bool,
    pub resource_limits: ResourceLimits,
}

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    pub enabled: bool,
    pub check_type: HealthCheckType,
    pub endpoint: Option<String>,
    pub command: Option<String>,
    pub interval_seconds: u64,
    pub timeout_seconds: u64,
    pub max_failures: u32,
    pub initial_delay_seconds: u64,
    /// Unix Domain Socket path for health checks (production mode)
    pub uds_socket: Option<PathBuf>,
}

/// Health check types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckType {
    Http,
    Tcp,
    Command,
    Process, // Just check if process is running
    None,
}

/// Restart policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestartPolicy {
    pub policy: RestartPolicyType,
    pub max_attempts: u32,
    pub backoff_base_seconds: u64,
    pub backoff_max_seconds: u64,
    pub window_seconds: u64,
}

/// Restart policy types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicyType {
    Always,
    OnFailure,
    Never,
    UnlessStopped,
}

/// Service category
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCategory {
    Core,
    Management,
    Worker,
    Utility,
    Development,
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_cpu_percent: Option<f64>,
    pub max_file_descriptors: Option<u64>,
    pub nice_level: Option<i32>,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            services: HashMap::new(),
            auth: AuthConfig::default(),
            monitoring: MonitoringConfig::default(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3301,
            workers: 4,
            max_connections: 1000,
            timeout_seconds: 30,
            cors_allowed_origins: vec!["http://localhost:3200".to_string(), "http://localhost:3300".to_string()],
            uds_socket: None,
            production_mode: false,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            token_ttl_hours: 8,
            refresh_grace_period_minutes: 60,
            max_login_attempts: 5,
            lockout_duration_minutes: 15,
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            metrics_enabled: true,
            health_check_interval_seconds: 30,
            log_level: "info".to_string(),
            prometheus_port: Some(9091),
        }
    }
}

impl Default for HealthCheckConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_type: HealthCheckType::Process,
            endpoint: None,
            command: None,
            interval_seconds: 30,
            timeout_seconds: 10,
            max_failures: 3,
            initial_delay_seconds: 5,
            uds_socket: None,
        }
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            policy: RestartPolicyType::OnFailure,
            max_attempts: 5,
            backoff_base_seconds: 1,
            backoff_max_seconds: 300,
            window_seconds: 300,
        }
    }
}

impl SupervisorConfig {
    /// Load configuration from file
    pub fn from_file(path: &std::path::Path) -> Result<Self, config::ConfigError> {
        let settings = config::Config::builder()
            .add_source(config::File::from(path))
            .add_source(config::Environment::with_prefix("SUPERVISOR"))
            .build()?;

        settings.try_deserialize()
    }

    /// Load configuration with defaults and environment overrides
    pub fn load() -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder()
            .set_default("server.host", "127.0.0.1")?
            .set_default("server.port", 3301)?
            .set_default("auth.token_ttl_hours", 8)?
            .set_default("monitoring.metrics_enabled", true)?;

        // Try to load from default config file
        if std::path::Path::new("config/supervisor.yaml").exists() {
            builder = builder.add_source(config::File::with_name("config/supervisor.yaml"));
        } else if std::path::Path::new("config/supervisor.yml").exists() {
            builder = builder.add_source(config::File::with_name("config/supervisor.yml"));
        } else if std::path::Path::new("supervisor.yaml").exists() {
            builder = builder.add_source(config::File::with_name("supervisor.yaml"));
        }

        // Add environment variables
        builder = builder.add_source(config::Environment::with_prefix("SUPERVISOR"));

        let settings = builder.build()?;
        settings.try_deserialize()
    }
}
