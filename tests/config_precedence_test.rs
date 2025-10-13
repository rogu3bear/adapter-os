#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    use adapteros_config::{
        get_config,
        guards::{get_env_var, is_config_frozen, set_config_frozen, ConfigGuards},
        init_config, is_frozen,
        loader::ConfigLoader,
        precedence::DeterministicConfig,
        types::{ConfigValidationError, PrecedenceLevel},
    };

    #[test]
    fn test_config_precedence_cli_overrides_env() {
        // Set up environment variable
        env::set_var("ADAPTEROS_APP_NAME", "EnvApp");

        // Initialize config with CLI argument that should override env
        let cli_args = vec![
            "adapteros".to_string(),
            "--adapteros-app-name".to_string(),
            "CliApp".to_string(),
        ];

        let config = init_config(cli_args, None).expect("Failed to initialize config");

        // CLI should override environment
        assert_eq!(config.get("app.name"), Some(&"CliApp".to_string()));

        // Clean up
        env::remove_var("ADAPTEROS_APP_NAME");
    }

    #[test]
    fn test_config_precedence_env_overrides_manifest() {
        // Create a temporary manifest file
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let manifest_path = temp_dir.path().join("test.toml");

        fs::write(
            &manifest_path,
            r#"
[app]
name = "ManifestApp"
version = "1.0.0"

[log]
level = "info"
"#,
        )
        .expect("Failed to write manifest");

        // Set environment variable
        env::set_var("ADAPTEROS_APP_NAME", "EnvApp");

        // Initialize config
        let cli_args = vec!["adapteros".to_string()];
        let config = init_config(cli_args, Some(manifest_path.to_string_lossy().to_string()))
            .expect("Failed to initialize config");

        // Environment should override manifest
        assert_eq!(config.get("app.name"), Some(&"EnvApp".to_string()));
        // But manifest values should still be present for other keys
        assert_eq!(config.get("app.version"), Some(&"1.0.0".to_string()));

        // Clean up
        env::remove_var("ADAPTEROS_APP_NAME");
    }

    #[test]
    fn test_config_freeze_and_hash() {
        let cli_args = vec!["adapteros".to_string()];
        let config = init_config(cli_args, None).expect("Failed to initialize config");

        // Configuration should be frozen
        assert!(is_frozen());

        // Hash should be present
        let metadata = config.get_metadata();
        assert!(!metadata.hash.is_empty());

        // Timestamp should be recent
        let now = chrono::Utc::now();
        let diff = now.signed_duration_since(metadata.timestamp);
        assert!(diff.num_seconds() < 10); // Should be within 10 seconds
    }

    #[test]
    fn test_env_var_access_after_freeze() {
        // Initialize config to trigger freeze
        let cli_args = vec!["adapteros".to_string()];
        let _config = init_config(cli_args, None).expect("Failed to initialize config");

        // Set a test environment variable
        env::set_var("TEST_VAR_AFTER_FREEZE", "value");

        // Attempting to access env var after freeze should fail
        let result = get_env_var("TEST_VAR_AFTER_FREEZE");
        assert!(result.is_err());

        // Should have recorded a violation
        assert!(ConfigGuards::has_violations());

        // Clean up
        env::remove_var("TEST_VAR_AFTER_FREEZE");
    }

    #[test]
    fn test_config_validation_required_field() {
        // Create a manifest with missing required field
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let manifest_path = temp_dir.path().join("incomplete.toml");

        fs::write(
            &manifest_path,
            r#"
[log]
level = "debug"
"#,
        )
        .expect("Failed to write manifest");

        // This should fail because app.name is required but missing
        let cli_args = vec!["adapteros".to_string()];
        let result = init_config(cli_args, Some(manifest_path.to_string_lossy().to_string()));

        // Should fail validation
        assert!(result.is_err());
        if let Err(adapteros_core::AosError::Config(msg)) = result {
            assert!(msg.contains("validation failed"));
        } else {
            panic!("Expected Config validation error");
        }
    }

    #[test]
    fn test_config_validation_invalid_type() {
        // Set invalid environment variable
        env::set_var("ADAPTEROS_SERVER_PORT", "not_a_number");

        let cli_args = vec!["adapteros".to_string()];
        let result = init_config(cli_args, None);

        // Should fail validation
        assert!(result.is_err());
        if let Err(adapteros_core::AosError::Config(msg)) = result {
            assert!(msg.contains("validation failed"));
        } else {
            panic!("Expected Config validation error");
        }

        // Clean up
        env::remove_var("ADAPTEROS_SERVER_PORT");
    }

    #[test]
    fn test_config_double_initialization() {
        // First initialization should succeed
        let cli_args = vec!["adapteros".to_string()];
        let _config1 = init_config(cli_args.clone(), None).expect("First init should succeed");

        // Second initialization should fail
        let result = init_config(cli_args, None);
        assert!(result.is_err());
        if let Err(adapteros_core::AosError::Config(msg)) = result {
            assert!(msg.contains("already initialized"));
        } else {
            panic!("Expected Config initialization error");
        }
    }

    #[test]
    fn test_config_get_or_default() {
        let cli_args = vec!["adapteros".to_string()];
        let config = init_config(cli_args, None).expect("Failed to initialize config");

        // Test existing value
        assert_eq!(config.get_or_default("app.name", "DefaultApp"), "AdapterOS");

        // Test non-existing value
        assert_eq!(
            config.get_or_default("non.existing.key", "DefaultValue"),
            "DefaultValue"
        );
    }

    #[test]
    fn test_config_metadata_sources() {
        // Set environment variable
        env::set_var("ADAPTEROS_LOG_LEVEL", "debug");

        let cli_args = vec![
            "adapteros".to_string(),
            "--adapteros-server-port".to_string(),
            "9000".to_string(),
        ];

        let config = init_config(cli_args, None).expect("Failed to initialize config");
        let metadata = config.get_metadata();

        // Should have sources from both CLI and environment
        assert!(!metadata.sources.is_empty());

        // Should have CLI args recorded
        assert!(!metadata.cli_args.is_empty());

        // Clean up
        env::remove_var("ADAPTEROS_LOG_LEVEL");
    }

    #[test]
    fn test_config_guards_violation_recording() {
        // Clear any existing violations
        ConfigGuards::get_violations().unwrap().clear();

        // Record a violation
        ConfigGuards::record_violation("test_rule", "test_message").unwrap();

        // Should have violations
        assert!(ConfigGuards::has_violations());

        let violations = ConfigGuards::get_violations().unwrap();
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("test_rule: test_message"));

        // Recording the same violation again should not increase count
        ConfigGuards::record_violation("test_rule", "test_message").unwrap();
        let violations_after_duplicate = ConfigGuards::get_violations().unwrap();
        assert_eq!(violations_after_duplicate.len(), 1);
    }
}
