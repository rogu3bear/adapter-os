//! Configuration for the Model Server

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::{DEFAULT_HOT_ADAPTER_THRESHOLD, DEFAULT_SOCKET_PATH, MAX_HOT_ADAPTERS};

/// Model server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelServerConfig {
    /// Whether the model server is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Path to the Unix domain socket
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,

    /// Path to the model directory
    pub model_path: Option<PathBuf>,

    /// Model ID for identification
    pub model_id: Option<String>,

    /// Maximum KV cache size in bytes (default: 4GB)
    #[serde(default = "default_kv_cache_bytes")]
    pub kv_cache_max_bytes: u64,

    /// Maximum number of concurrent sessions
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    /// Hot adapter activation threshold (0.0-1.0)
    #[serde(default = "default_hot_threshold")]
    pub hot_adapter_threshold: f64,

    /// Maximum number of hot adapters to cache
    #[serde(default = "default_max_hot_adapters")]
    pub max_hot_adapters: usize,

    /// Adapter warmup on startup (list of adapter IDs to preload)
    #[serde(default)]
    pub warmup_adapters: Vec<u32>,

    /// Grace period for drain in seconds
    #[serde(default = "default_drain_grace_secs")]
    pub drain_grace_secs: u32,

    /// Health check interval in seconds
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u32,
}

impl Default for ModelServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            socket_path: default_socket_path(),
            model_path: None,
            model_id: None,
            kv_cache_max_bytes: default_kv_cache_bytes(),
            max_sessions: default_max_sessions(),
            hot_adapter_threshold: default_hot_threshold(),
            max_hot_adapters: default_max_hot_adapters(),
            warmup_adapters: Vec::new(),
            drain_grace_secs: default_drain_grace_secs(),
            health_check_interval_secs: default_health_check_interval(),
        }
    }
}

fn default_socket_path() -> PathBuf {
    PathBuf::from(DEFAULT_SOCKET_PATH)
}

fn default_kv_cache_bytes() -> u64 {
    4 * 1024 * 1024 * 1024 // 4GB
}

fn default_max_sessions() -> usize {
    32
}

fn default_hot_threshold() -> f64 {
    DEFAULT_HOT_ADAPTER_THRESHOLD
}

fn default_max_hot_adapters() -> usize {
    MAX_HOT_ADAPTERS
}

fn default_drain_grace_secs() -> u32 {
    30
}

fn default_health_check_interval() -> u32 {
    10
}

impl ModelServerConfig {
    /// Create a new config with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder pattern: set model path
    pub fn with_model_path(mut self, path: PathBuf) -> Self {
        self.model_path = Some(path);
        self
    }

    /// Builder pattern: set socket path
    pub fn with_socket_path(mut self, path: PathBuf) -> Self {
        self.socket_path = path;
        self
    }

    /// Builder pattern: enable model server
    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.enabled && self.model_path.is_none() {
            return Err("model_path is required when model_server is enabled".to_string());
        }

        if self.hot_adapter_threshold < 0.0 || self.hot_adapter_threshold > 1.0 {
            return Err(format!(
                "hot_adapter_threshold must be between 0.0 and 1.0, got {}",
                self.hot_adapter_threshold
            ));
        }

        if self.max_hot_adapters == 0 {
            return Err("max_hot_adapters must be at least 1".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ModelServerConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.socket_path, PathBuf::from(DEFAULT_SOCKET_PATH));
        assert!(config.model_path.is_none());
    }

    #[test]
    fn test_builder_pattern() {
        let config = ModelServerConfig::new()
            .with_model_path(PathBuf::from("/var/models/test"))
            .with_socket_path(PathBuf::from("/tmp/test.sock"))
            .enabled();

        assert!(config.enabled);
        assert_eq!(config.model_path, Some(PathBuf::from("/var/models/test")));
        assert_eq!(config.socket_path, PathBuf::from("/tmp/test.sock"));
    }

    #[test]
    fn test_validation_requires_model_path() {
        let config = ModelServerConfig::new().enabled();
        assert!(config.validate().is_err());

        let config = config.with_model_path(PathBuf::from("/var/models/test"));
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_threshold_range() {
        let mut config = ModelServerConfig::new()
            .with_model_path(PathBuf::from("/var/models/test"))
            .enabled();

        config.hot_adapter_threshold = 0.5;
        assert!(config.validate().is_ok());

        config.hot_adapter_threshold = -0.1;
        assert!(config.validate().is_err());

        config.hot_adapter_threshold = 1.1;
        assert!(config.validate().is_err());
    }
}
