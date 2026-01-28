//! TUI Configuration persistence
//!
//! Handles loading and saving TUI configuration to ~/.config/adapteros/tui.toml

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// TUI-specific configuration that persists across sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    /// Server URL for API connections
    #[serde(default = "default_server_url")]
    pub server_url: String,

    /// Refresh interval in milliseconds
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_ms: u64,

    /// Color theme (future use)
    #[serde(default = "default_theme")]
    pub theme: String,

    /// Last screen the user was on (for session restore)
    #[serde(default)]
    pub last_screen: Option<String>,
}

fn default_server_url() -> String {
    adapteros_api_types::defaults::DEFAULT_SERVER_URL.to_string()
}

fn default_refresh_interval() -> u64 {
    1000
}

fn default_theme() -> String {
    "default".to_string()
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            server_url: default_server_url(),
            refresh_interval_ms: default_refresh_interval(),
            theme: default_theme(),
            last_screen: None,
        }
    }
}

impl TuiConfig {
    /// Get the configuration file path
    /// Uses ~/.config/adapteros/tui.toml on Unix systems
    pub fn config_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("adapteros").join("tui.toml")
        } else {
            // Fallback to current directory
            PathBuf::from("tui.toml")
        }
    }

    /// Load configuration from file, or return defaults if file doesn't exist
    pub fn load() -> Result<Self> {
        let path = Self::config_path();

        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            let config: TuiConfig = toml::from_str(&content)?;
            tracing::info!("Loaded TUI config from {}", path.display());
            Ok(config)
        } else {
            tracing::debug!("No config file at {}, using defaults", path.display());
            Ok(Self::default())
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;

        tracing::info!("Saved TUI config to {}", path.display());
        Ok(())
    }

    /// Update a single field and save
    #[allow(dead_code)]
    pub fn update_and_save<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut Self),
    {
        f(self);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TuiConfig::default();
        assert_eq!(config.server_url, "http://localhost:8080");
        assert_eq!(config.refresh_interval_ms, 1000);
        assert_eq!(config.theme, "default");
    }

    #[test]
    fn test_config_serialization() {
        let config = TuiConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("server_url"));

        let parsed: TuiConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.server_url, config.server_url);
    }
}
