//! Configuration loader with precedence: CLI > ENV > manifest

use crate::model::load_dotenv;
use crate::precedence::ConfigBuilder;
use crate::types::PrecedenceLevel;
use crate::types::*;
use adapteros_core::errors::AosValidationError;
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
        // Always load .env first so environment reads are deterministic
        load_dotenv();

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
    ///
    /// Maps TOML keys to config_key using the schema's toml_key field.
    /// For example, `db.path` in cp.toml maps to `database.url` (AOS_DATABASE_URL).
    ///
    /// # Errors
    ///
    /// When `require_manifest` is enabled (default), returns an error if:
    /// - The config file does not exist
    /// - The config file cannot be read (permission denied or other I/O error)
    /// - The config file contains invalid TOML syntax
    fn load_manifest(&self, mut builder: ConfigBuilder, path: &str) -> Result<ConfigBuilder> {
        let manifest_path = Path::new(path);

        // Scenario 1: Required config file missing
        if !manifest_path.exists() {
            if self.options.require_manifest {
                return Err(AosError::from(AosValidationError::ConfigFileNotFound {
                    path: path.to_string(),
                    tried_locations: vec![path.to_string()],
                }));
            }
            tracing::warn!(
                manifest = %path,
                "Manifest not found, using compiled-in defaults and environment"
            );
            return Ok(builder);
        }

        builder = builder.with_manifest_path(path.to_string());

        // Scenario 2: Permission denied or other read errors
        let content = match fs::read_to_string(manifest_path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                // Always fail on permission errors - they indicate a real problem
                return Err(AosError::from(
                    AosValidationError::ConfigFilePermissionDenied {
                        path: path.to_string(),
                        reason: format!("chmod 644 '{}' to fix", path),
                    },
                ));
            }
            Err(e) => {
                if self.options.require_manifest {
                    return Err(AosError::Config(format!(
                        "Failed to read config file '{}': {}",
                        path, e
                    )));
                }
                tracing::warn!(
                    manifest = %path,
                    error = %e,
                    "Manifest unreadable, using compiled-in defaults and environment"
                );
                return Ok(builder);
            }
        };

        // Scenario 3: Invalid TOML syntax
        let manifest: Value = match toml::from_str(&content) {
            Ok(v) => v,
            Err(e) => {
                // Always fail on parse errors - invalid config should not silently fall back
                return Err(AosError::Config(format!(
                    "Invalid TOML in '{}': {}",
                    path, e
                )));
            }
        };

        builder = builder.with_manifest_path(path.to_string());

        // Build TOML key mapping from schema
        let schema = crate::schema::default_schema();
        let toml_key_map = schema.build_toml_key_map();

        // Flatten nested TOML structure
        let flattened = Self::flatten_toml_value(&manifest, String::new());
        let count = flattened.len();
        for (toml_key, value) in flattened {
            // Map TOML key to config_key (e.g., db.path -> database.url)
            let config_key = toml_key_map.get(&toml_key).cloned().unwrap_or(toml_key);
            builder = builder.add_value(
                config_key,
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
    /// - `AOS_*` - All AOS_* vars mapped to config keys (e.g., `AOS_SERVER_PORT` -> `server.port`)
    ///
    /// Mapping uses the schema's `config_key` field when available for proper TOML integration.
    ///
    /// # Empty Value Handling
    ///
    /// When `reject_empty_env_vars` is enabled (default) and production mode is active,
    /// empty or whitespace-only environment variables cause an error. In development mode,
    /// empty values are logged as warnings and skipped.
    fn load_environment(&self, mut builder: ConfigBuilder) -> Result<ConfigBuilder> {
        let schema = crate::schema::default_schema();

        // Check if we're in production mode for stricter validation
        let is_production = std::env::var("AOS_PRODUCTION_MODE")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);

        // Collect ALL vars with AOS_ prefix and map using schema (canonical)
        let aos_prefix = "AOS_";
        let mut aos_vars: HashMap<String, (String, String)> = HashMap::new();

        for (key, value) in std::env::vars().filter(|(k, _)| k.starts_with(aos_prefix)) {
            // Scenario 4: Empty environment variable override
            if value.trim().is_empty() {
                if self.options.reject_empty_env_vars && is_production {
                    // Get config_key for error reporting
                    let schema = crate::schema::default_schema();
                    let config_key = if let Some(var) = schema.get_variable(&key) {
                        var.config_key.clone()
                    } else {
                        key.strip_prefix(aos_prefix)
                            .unwrap_or(&key)
                            .to_lowercase()
                            .replace('_', ".")
                    };
                    return Err(AosError::from(AosValidationError::EmptyEnvOverride {
                        variable: key,
                        config_key,
                    }));
                }
                tracing::warn!(
                    var = %key,
                    "Environment variable is empty, ignoring (would fail in production)"
                );
                continue; // Skip empty values
            }

            // Use schema config_key if available for proper TOML mapping
            let config_key = if let Some(var) = schema.get_variable(&key) {
                var.config_key.clone()
            } else {
                // Fallback: Remove AOS_ prefix and convert to lowercase with dots
                key.strip_prefix(aos_prefix)
                    .unwrap_or(&key)
                    .to_lowercase()
                    .replace('_', ".")
            };
            tracing::debug!(env_var = %key, config_key = %config_key, "Mapped AOS_ env var");
            aos_vars.insert(config_key, (key, value));
        }

        // Collect vars with legacy ADAPTEROS_ prefix (deprecated, warn)
        // Also skip empty values with the same logic as AOS_* vars
        let mut adapteros_vars: HashMap<String, (String, String)> = HashMap::new();
        for (key, value) in
            std::env::vars().filter(|(k, _)| k.starts_with(&self.options.env_prefix))
        {
            // Skip empty values with same logic as AOS_* vars
            if value.trim().is_empty() {
                if self.options.reject_empty_env_vars && is_production {
                    let config_key = key
                        .strip_prefix(&self.options.env_prefix)
                        .unwrap_or(&key)
                        .to_lowercase()
                        .replace('_', ".");
                    return Err(AosError::from(AosValidationError::EmptyEnvOverride {
                        variable: key,
                        config_key,
                    }));
                }
                tracing::warn!(
                    var = %key,
                    "Environment variable is empty, ignoring (would fail in production)"
                );
                continue;
            }

            // Remove prefix and convert to lowercase with dots
            let config_key = key
                .strip_prefix(&self.options.env_prefix)
                .unwrap_or(&key)
                .to_lowercase()
                .replace('_', ".");
            adapteros_vars.insert(config_key, (key, value));
        }

        // Merge with canonical AOS_* taking precedence over legacy ADAPTEROS_*
        let mut env_vars: HashMap<String, (String, String)> = HashMap::new();
        for (config_key, (raw_key, value)) in aos_vars {
            env_vars.insert(config_key, (format!("env:{}", raw_key), value));
        }

        for (config_key, (raw_key, value)) in adapteros_vars {
            if env_vars.contains_key(&config_key) {
                let replacement = raw_key.replacen("ADAPTEROS_", "AOS_", 1);
                tracing::warn!(
                    deprecated_var = %raw_key,
                    replacement = %replacement,
                    "Ignoring legacy ADAPTEROS_ variable because AOS_ override is set"
                );
                continue;
            }

            let replacement = raw_key.replacen("ADAPTEROS_", "AOS_", 1);
            tracing::warn!(
                deprecated_var = %raw_key,
                replacement = %replacement,
                "Using legacy ADAPTEROS_ variable; please migrate to AOS_*"
            );
            env_vars.insert(config_key, (format!("env:legacy:{}", raw_key), value));
        }

        let count = env_vars.len();
        for (key, (source, value)) in env_vars {
            builder = builder.add_value(key, value, PrecedenceLevel::Environment, source);
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
    fn flatten_toml_value(value: &Value, prefix: String) -> HashMap<String, String> {
        let mut result = HashMap::new();

        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    let new_prefix = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", prefix, key)
                    };

                    let flattened = Self::flatten_toml_value(val, new_prefix);
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
    use crate::test_support::TestEnvGuard;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn new_temp_file() -> NamedTempFile {
        let temp_root = std::path::PathBuf::from("var/tmp");
        std::fs::create_dir_all(&temp_root).unwrap();
        NamedTempFile::new_in(&temp_root).unwrap()
    }

    #[test]
    fn test_load_manifest() {
        let _env = TestEnvGuard::new();
        let mut temp_file = new_temp_file();
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
        let _env = TestEnvGuard::new();
        let mut temp_file = new_temp_file();
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
        std::env::set_var("AOS_SERVER_PORT", "9090");

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
        std::env::remove_var("AOS_SERVER_PORT");
    }

    #[test]
    fn test_config_freeze() {
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
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
        let _env = TestEnvGuard::new();
        // Test that CLI > ENV > manifest precedence is maintained
        let mut temp_file = new_temp_file();
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
    fn test_aos_prefix_takes_precedence_over_adapteros() {
        let _env = TestEnvGuard::new();
        // ADAPTEROS_ prefix is deprecated and should not override canonical AOS_
        std::env::set_var("AOS_MODEL_PATH", "/aos/path");
        std::env::set_var("ADAPTEROS_MODEL_PATH", "/adapteros/path");
        // Required field for validation
        std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://test.db");

        let loader = ConfigLoader::new();
        let config = loader.load(vec![], None).unwrap();

        // AOS_ should win over legacy ADAPTEROS_
        assert_eq!(config.get("model.path"), Some(&"/aos/path".to_string()));

        // Clean up
        std::env::remove_var("AOS_MODEL_PATH");
        std::env::remove_var("ADAPTEROS_MODEL_PATH");
        std::env::remove_var("ADAPTEROS_DATABASE_URL");
    }

    #[test]
    fn test_adapteros_prefix_used_when_aos_missing() {
        let _env = TestEnvGuard::new();
        // Legacy ADAPTEROS_ variables still map when no AOS_ is provided
        std::env::set_var("ADAPTEROS_MODEL_PATH", "/adapteros/only/path");
        // Required field for validation
        std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://test.db");

        let loader = ConfigLoader::new();
        let config = loader.load(vec![], None).unwrap();

        assert_eq!(
            config.get("model.path"),
            Some(&"/adapteros/only/path".to_string())
        );

        // Clean up
        std::env::remove_var("ADAPTEROS_MODEL_PATH");
        std::env::remove_var("ADAPTEROS_DATABASE_URL");
    }

    #[test]
    fn test_all_aos_env_vars_loaded() {
        let _env = TestEnvGuard::new();
        // Test that ALL AOS_* env vars are loaded, not just AOS_MODEL_*
        std::env::set_var("AOS_SERVER_PORT", "9999");
        std::env::set_var("AOS_SERVER_HOST", "0.0.0.0");
        std::env::set_var("AOS_DATABASE_URL", "sqlite://test-env.db");
        std::env::set_var("AOS_LOG_LEVEL", "debug");

        let loader = ConfigLoader::new();
        let config = loader.load(vec![], None).unwrap();

        // Verify all AOS_* vars are mapped correctly using schema config_key
        assert_eq!(config.get("server.port"), Some(&"9999".to_string()));
        assert_eq!(config.get("server.host"), Some(&"0.0.0.0".to_string()));
        assert_eq!(
            config.get("database.url"),
            Some(&"sqlite://test-env.db".to_string())
        );
        assert_eq!(config.get("log.level"), Some(&"debug".to_string()));

        // Clean up
        std::env::remove_var("AOS_SERVER_PORT");
        std::env::remove_var("AOS_SERVER_HOST");
        std::env::remove_var("AOS_DATABASE_URL");
        std::env::remove_var("AOS_LOG_LEVEL");
    }

    #[test]
    fn test_toml_key_mapping() {
        let _env = TestEnvGuard::new();
        // Test that TOML keys are mapped to config_key via schema's toml_key field
        // cp.toml uses db.path, but schema uses database.url
        let mut temp_file = new_temp_file();
        writeln!(
            temp_file,
            r#"
[db]
path = "sqlite://toml-db.db"

[server]
port = 8888
"#
        )
        .unwrap();
        temp_file.flush().unwrap();

        let loader = ConfigLoader::new();
        let config = loader
            .load(vec![], Some(temp_file.path().to_string_lossy().to_string()))
            .unwrap();

        // db.path in TOML should map to database.url (config_key)
        assert_eq!(
            config.get("database.url"),
            Some(&"sqlite://toml-db.db".to_string())
        );
        // server.port should work as normal (no special mapping needed)
        assert_eq!(config.get("server.port"), Some(&"8888".to_string()));
    }

    #[test]
    fn test_env_overrides_toml() {
        let _env = TestEnvGuard::new();
        // Test that AOS_* env vars override TOML values
        let mut temp_file = new_temp_file();
        writeln!(
            temp_file,
            r#"
[db]
path = "sqlite://toml-db.db"

[server]
port = 8888
"#
        )
        .unwrap();
        temp_file.flush().unwrap();

        // Set env var that should override TOML
        std::env::set_var("AOS_SERVER_PORT", "7777");
        std::env::set_var("AOS_DATABASE_URL", "sqlite://env-db.db");

        let loader = ConfigLoader::new();
        let config = loader
            .load(vec![], Some(temp_file.path().to_string_lossy().to_string()))
            .unwrap();

        // ENV should override TOML
        assert_eq!(config.get("server.port"), Some(&"7777".to_string()));
        assert_eq!(
            config.get("database.url"),
            Some(&"sqlite://env-db.db".to_string())
        );

        // Clean up
        std::env::remove_var("AOS_SERVER_PORT");
        std::env::remove_var("AOS_DATABASE_URL");
    }

    #[test]
    fn test_missing_config_returns_error() {
        let _env = TestEnvGuard::new();
        let loader = ConfigLoader::new();

        // Try to load a non-existent config file
        let result = loader.load(vec![], Some("/nonexistent/config.toml".into()));

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("not found"),
            "Error message should mention file not found: {}",
            err_msg
        );
    }

    #[test]
    fn test_missing_config_allowed_when_not_required() {
        let _env = TestEnvGuard::new();
        let options = LoaderOptions {
            require_manifest: false,
            ..Default::default()
        };
        let loader = ConfigLoader::with_options(options);

        // Should succeed even with non-existent config when require_manifest is false
        let result = loader.load(vec![], Some("/nonexistent/config.toml".into()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_toml_returns_error() {
        let _env = TestEnvGuard::new();
        let mut temp_file = new_temp_file();

        // Write invalid TOML
        writeln!(temp_file, "this is not valid [TOML syntax").unwrap();
        temp_file.flush().unwrap();

        let loader = ConfigLoader::new();
        let result = loader.load(vec![], Some(temp_file.path().to_string_lossy().to_string()));

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Invalid TOML"),
            "Error message should mention invalid TOML: {}",
            err_msg
        );
    }

    #[test]
    fn test_empty_env_var_skipped_in_dev_mode() {
        let _env = TestEnvGuard::new();
        // Ensure we're NOT in production mode
        std::env::remove_var("AOS_PRODUCTION_MODE");

        // Set an empty env var - should be skipped
        std::env::set_var("AOS_SERVER_PORT", "   ");
        // Set a valid var to ensure config loads
        std::env::set_var("AOS_DATABASE_URL", "sqlite://test.db");

        let loader = ConfigLoader::new();
        let config = loader.load(vec![], None).unwrap();

        // Empty var should be skipped, so server.port should not be set from env
        // (it will have default or none)
        assert_ne!(
            config.get("server.port"),
            Some(&"   ".to_string()),
            "Empty env var should be skipped"
        );

        // Clean up
        std::env::remove_var("AOS_SERVER_PORT");
        std::env::remove_var("AOS_DATABASE_URL");
    }

    #[test]
    fn test_empty_env_var_fails_in_production() {
        let _env = TestEnvGuard::new();
        // Set production mode
        std::env::set_var("AOS_PRODUCTION_MODE", "true");
        // Set an empty env var
        std::env::set_var("AOS_SERVER_PORT", "   ");

        let loader = ConfigLoader::new();
        let result = loader.load(vec![], None);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("empty or whitespace"),
            "Error message should mention empty value: {}",
            err_msg
        );

        // Clean up
        std::env::remove_var("AOS_PRODUCTION_MODE");
        std::env::remove_var("AOS_SERVER_PORT");
    }
}
