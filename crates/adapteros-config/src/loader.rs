//! Configuration loader with precedence: CLI > ENV > manifest

use crate::precedence::ConfigBuilder;
use crate::types::PrecedenceLevel;
use crate::types::*;
use adapteros_core::{AosError, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Configuration loader with deterministic precedence
pub struct ConfigLoader {
    options: LoaderOptions,
}

impl ConfigLoader {
    /// Create a new configuration loader
    pub fn new() -> Self {
        Self {
            options: LoaderOptions::default(),
        }
    }

    /// Create a configuration loader with custom options
    pub fn with_options(options: LoaderOptions) -> Self {
        Self { options }
    }

    /// Load configuration with precedence: CLI > ENV > manifest
    pub fn load(
        &self,
        cli_args: Vec<String>,
        manifest_path: Option<String>,
    ) -> Result<crate::precedence::DeterministicConfig> {
        let mut builder = ConfigBuilder::new().with_cli_args(cli_args.clone());

        // Load manifest file first (lowest precedence)
        if let Some(ref path) = manifest_path {
            builder = self.load_manifest(builder, path)?;
        }

        // Load environment variables (medium precedence)
        builder = self.load_environment(builder)?;

        // Load CLI arguments (highest precedence)
        builder = self.load_cli_args(builder, cli_args)?;

        // Build and freeze configuration
        let mut config = builder.build()?;
        config.freeze()?;

        tracing::info!(
            "Configuration loaded and frozen: {}",
            config.get_metadata().hash
        );
        Ok(config)
    }

    /// Load configuration from manifest file
    fn load_manifest(&self, mut builder: ConfigBuilder, path: &str) -> Result<ConfigBuilder> {
        let manifest_path = Path::new(path);
        if !manifest_path.exists() {
            return Err(AosError::Config(format!(
                "Manifest file not found: {}",
                path
            )));
        }

        let content = fs::read_to_string(manifest_path).map_err(|e| {
            AosError::Config(format!("Failed to read manifest file {}: {}", path, e))
        })?;

        let manifest: Value = toml::from_str(&content).map_err(|e| {
            AosError::Config(format!("Failed to parse manifest file {}: {}", path, e))
        })?;

        builder = builder.with_manifest_path(path.to_string());

        // Flatten nested TOML structure
        let flattened = self.flatten_toml_value(&manifest, String::new());
        let count = flattened.len();
        for (key, value) in flattened {
            builder = builder.add_value(
                key,
                value,
                PrecedenceLevel::Manifest,
                format!("manifest:{}", path),
            );
        }

        tracing::debug!("Loaded {} values from manifest: {}", count, path);
        Ok(builder)
    }

    /// Load configuration from environment variables
    ///
    /// Supports two prefixes:
    /// - `ADAPTEROS_*` - Standard prefix (e.g., `ADAPTEROS_SERVER_PORT` -> `server.port`)
    /// - `AOS_*` - Short prefix for model-related vars (e.g., `AOS_MODEL_PATH` -> `model.path`)
    fn load_environment(&self, mut builder: ConfigBuilder) -> Result<ConfigBuilder> {
        // Collect vars with ADAPTEROS_ prefix
        let adapteros_vars: HashMap<String, String> = std::env::vars()
            .filter(|(key, _)| key.starts_with(&self.options.env_prefix))
            .map(|(key, value)| {
                // Remove prefix and convert to lowercase with dots
                let config_key = key
                    .strip_prefix(&self.options.env_prefix)
                    .unwrap_or(&key)
                    .to_lowercase()
                    .replace('_', ".");
                (config_key, value)
            })
            .collect();

        // Collect vars with AOS_ prefix (for model-related config)
        let aos_prefix = "AOS_";
        let aos_vars: HashMap<String, String> = std::env::vars()
            .filter(|(key, _)| {
                key.starts_with(aos_prefix) && !key.starts_with("AOS_")
                    || key.starts_with(aos_prefix)
            })
            .filter(|(key, _)| {
                // Only allow specific AOS_ prefixed vars for model configuration
                key.starts_with("AOS_MODEL_")
            })
            .map(|(key, value)| {
                // Remove AOS_ prefix and convert to lowercase with dots
                let config_key = key
                    .strip_prefix(aos_prefix)
                    .unwrap_or(&key)
                    .to_lowercase()
                    .replace('_', ".");
                tracing::debug!(env_var = %key, config_key = %config_key, "Mapped AOS_ env var");
                (config_key, value)
            })
            .collect();

        // Merge both sets (AOS_ vars don't override ADAPTEROS_ vars)
        let mut env_vars = aos_vars;
        for (key, value) in adapteros_vars {
            // ADAPTEROS_ prefix takes precedence over AOS_ prefix
            env_vars.insert(key, value);
        }

        let count = env_vars.len();
        for (key, value) in env_vars {
            builder = builder.add_value(
                key,
                value,
                PrecedenceLevel::Environment,
                "environment".to_string(),
            );
        }

        tracing::debug!("Loaded {} environment variables", count);
        Ok(builder)
    }

    /// Load configuration from CLI arguments
    fn load_cli_args(
        &self,
        mut builder: ConfigBuilder,
        cli_args: Vec<String>,
    ) -> Result<ConfigBuilder> {
        let mut i = 0;
        while i < cli_args.len() {
            let arg = &cli_args[i];

            if arg.starts_with("--") {
                let key = arg.strip_prefix("--").unwrap().to_string();
                let value = if i + 1 < cli_args.len() && !cli_args[i + 1].starts_with("--") {
                    i += 1;
                    cli_args[i].clone()
                } else {
                    "true".to_string() // Boolean flag
                };

                // Convert CLI key format to schema key format
                let schema_key = self.convert_cli_key_to_schema_key(&key);

                builder =
                    builder.add_value(schema_key, value, PrecedenceLevel::Cli, "cli".to_string());
            }

            i += 1;
        }

        tracing::debug!("Loaded {} CLI arguments", cli_args.len());
        Ok(builder)
    }

    /// Convert CLI key format to schema key format
    /// e.g., "adapteros-database-url" -> "database.url"
    fn convert_cli_key_to_schema_key(&self, cli_key: &str) -> String {
        if cli_key.starts_with("adapteros-") {
            let without_prefix = cli_key.strip_prefix("adapteros-").unwrap();
            // Convert kebab-case to dot notation
            without_prefix.replace('-', ".")
        } else {
            cli_key.to_string()
        }
    }

    /// Flatten nested TOML value into dot-notation keys
    fn flatten_toml_value(&self, value: &Value, prefix: String) -> HashMap<String, String> {
        let mut result = HashMap::new();

        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    let new_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };

                    let flattened = self.flatten_toml_value(val, new_prefix);
                    result.extend(flattened);
                }
            }
            Value::String(s) => {
                result.insert(prefix, s.clone());
            }
            Value::Number(n) => {
                result.insert(prefix, n.to_string());
            }
            Value::Bool(b) => {
                result.insert(prefix, b.to_string());
            }
            Value::Array(arr) => {
                // Convert array to comma-separated string
                let values: Vec<String> = arr
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => format!("{:?}", v),
                    })
                    .collect();
                result.insert(prefix, values.join(","));
            }
            Value::Null => {
                result.insert(prefix, "null".to_string());
            }
        }

        result
    }

    /// Validate configuration file format
    pub fn validate_manifest(&self, path: &str) -> Result<()> {
        let content = fs::read_to_string(path).map_err(|e| {
            AosError::Config(format!("Failed to read manifest file {}: {}", path, e))
        })?;

        toml::from_str::<Value>(&content)
            .map_err(|e| AosError::Config(format!("Invalid TOML format in {}: {}", path, e)))?;

        Ok(())
    }

    /// Get configuration schema
    pub fn get_schema(&self) -> ConfigSchema {
        ConfigSchema::default()
    }
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_manifest() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"
[server]
host = "127.0.0.1"
port = 8080

[database]
url = "sqlite://test.db"
pool_size = 10

[policy]
strict_mode = true
"#
        )
        .unwrap();
        temp_file.flush().unwrap();

        let loader = ConfigLoader::new();
        let config = loader
            .load(vec![], Some(temp_file.path().to_string_lossy().to_string()))
            .unwrap();

        assert_eq!(config.get("server.host"), Some(&"127.0.0.1".to_string()));
        assert_eq!(config.get("server.port"), Some(&"8080".to_string()));
        assert_eq!(
            config.get("database.url"),
            Some(&"sqlite://test.db".to_string())
        );
        assert_eq!(config.get("policy.strict_mode"), Some(&"true".to_string()));
    }

    #[test]
    fn test_precedence_order() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"
[server]
port = 8080

[database]
url = "sqlite://manifest.db"
"#
        )
        .unwrap();
        temp_file.flush().unwrap();

        // Set environment variable
        std::env::set_var("ADAPTEROS_SERVER_PORT", "9090");

        let loader = ConfigLoader::new();
        let config = loader
            .load(
                vec!["--server.port".to_string(), "7070".to_string()],
                Some(temp_file.path().to_string_lossy().to_string()),
            )
            .unwrap();

        // CLI should win
        assert_eq!(config.get("server.port"), Some(&"7070".to_string()));

        // Clean up
        std::env::remove_var("ADAPTEROS_SERVER_PORT");
    }

    #[test]
    fn test_config_freeze() {
        // Required field for validation
        std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://test.db");

        let loader = ConfigLoader::new();
        let config = loader.load(vec![], None).unwrap();

        assert!(config.is_frozen());
        assert!(!config.get_metadata().hash.is_empty());

        // Clean up
        std::env::remove_var("ADAPTEROS_DATABASE_URL");
    }

    #[test]
    fn test_aos_model_path_env() {
        // Test that AOS_MODEL_PATH maps to model.path
        std::env::set_var("AOS_MODEL_PATH", "/path/to/custom/model");
        std::env::set_var("AOS_MODEL_BACKEND", "mlx");
        std::env::set_var("AOS_MODEL_ARCHITECTURE", "llama");
        // Required field for validation
        std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://test.db");

        let loader = ConfigLoader::new();
        let config = loader.load(vec![], None).unwrap();

        // Verify AOS_ env vars are mapped correctly
        assert_eq!(
            config.get("model.path"),
            Some(&"/path/to/custom/model".to_string())
        );
        assert_eq!(config.get("model.backend"), Some(&"mlx".to_string()));
        assert_eq!(config.get("model.architecture"), Some(&"llama".to_string()));

        // Clean up
        std::env::remove_var("AOS_MODEL_PATH");
        std::env::remove_var("AOS_MODEL_BACKEND");
        std::env::remove_var("AOS_MODEL_ARCHITECTURE");
        std::env::remove_var("ADAPTEROS_DATABASE_URL");
    }

    #[test]
    fn test_aos_model_path_precedence() {
        // Test that CLI > ENV > manifest precedence is maintained
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(
            temp_file,
            r#"
[model]
path = "/manifest/model/path"

[database]
url = "sqlite://manifest.db"
"#
        )
        .unwrap();
        temp_file.flush().unwrap();

        // Set AOS_ environment variable
        std::env::set_var("AOS_MODEL_PATH", "/env/model/path");

        let loader = ConfigLoader::new();

        // Test ENV > manifest
        let config = loader
            .load(vec![], Some(temp_file.path().to_string_lossy().to_string()))
            .unwrap();
        assert_eq!(
            config.get("model.path"),
            Some(&"/env/model/path".to_string())
        );

        // Test CLI > ENV > manifest
        let config_with_cli = loader
            .load(
                vec!["--model.path".to_string(), "/cli/model/path".to_string()],
                Some(temp_file.path().to_string_lossy().to_string()),
            )
            .unwrap();
        assert_eq!(
            config_with_cli.get("model.path"),
            Some(&"/cli/model/path".to_string())
        );

        // Clean up
        std::env::remove_var("AOS_MODEL_PATH");
    }

    #[test]
    fn test_adapteros_prefix_takes_precedence_over_aos() {
        // ADAPTEROS_ prefix should take precedence over AOS_ prefix
        std::env::set_var("AOS_MODEL_PATH", "/aos/path");
        std::env::set_var("ADAPTEROS_MODEL_PATH", "/adapteros/path");
        // Required field for validation
        std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://test.db");

        let loader = ConfigLoader::new();
        let config = loader.load(vec![], None).unwrap();

        // ADAPTEROS_ should win
        assert_eq!(
            config.get("model.path"),
            Some(&"/adapteros/path".to_string())
        );

        // Clean up
        std::env::remove_var("AOS_MODEL_PATH");
        std::env::remove_var("ADAPTEROS_MODEL_PATH");
        std::env::remove_var("ADAPTEROS_DATABASE_URL");
    }
}
