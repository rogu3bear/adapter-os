//! Tests for CLI configuration loading
//!
//! Tests that the CLI properly loads configuration from:
//! - Environment variables
//! - CLI arguments
//! - Precedence rules (CLI > ENV > defaults)

use adapteros_config::BackendPreference;
use clap::Parser;
use std::env;

// Helper to temporarily set env vars for tests
struct EnvGuard {
    key: String,
    old_value: Option<String>,
}

impl EnvGuard {
    fn set(key: &str, value: &str) -> Self {
        let old_value = env::var(key).ok();
        env::set_var(key, value);
        Self {
            key: key.to_string(),
            old_value,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.old_value {
            Some(val) => env::set_var(&self.key, val),
            None => env::remove_var(&self.key),
        }
    }
}

fn parse_cli(args: Vec<&str>) -> adapteros_cli::app::Cli {
    adapteros_cli::app::Cli::parse_from(args)
}

#[cfg(test)]
mod config_loading {
    use super::*;

    #[test]
    fn test_model_config_from_cli_args() {
        let cli = parse_cli(vec![
            "aosctl",
            "--model-path",
            "/custom/model/path",
            "--model-backend",
            "metal",
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        assert_eq!(config.path.to_str().unwrap(), "/custom/model/path");
        assert!(matches!(config.backend, BackendPreference::Metal));
    }

    #[test]
    fn test_model_config_from_env_vars() {
        let _model_path_guard = EnvGuard::set("AOS_MODEL_PATH", "/env/model/path");
        let _backend_guard = EnvGuard::set("AOS_MODEL_BACKEND", "coreml");

        let cli = parse_cli(vec!["aosctl", "adapter-list"]);

        let config = cli.get_model_config().expect("should build config");
        // ENV vars are picked up via clap's env attribute
        assert_eq!(cli.model_path.as_deref(), Some("/env/model/path"));
        assert_eq!(cli.model_backend, "coreml");
    }

    #[test]
    fn test_cli_overrides_env() {
        let _model_path_guard = EnvGuard::set("AOS_MODEL_PATH", "/env/model/path");
        let _backend_guard = EnvGuard::set("AOS_MODEL_BACKEND", "coreml");

        let cli = parse_cli(vec![
            "aosctl",
            "--model-path",
            "/cli/model/path",
            "--model-backend",
            "metal",
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        // CLI args should override ENV
        assert_eq!(config.path.to_str().unwrap(), "/cli/model/path");
        assert!(matches!(config.backend, BackendPreference::Metal));
    }

    #[test]
    fn test_default_backend_preference() {
        let cli = parse_cli(vec!["aosctl", "adapter-list"]);

        // Default should be "auto"
        assert_eq!(cli.model_backend, "auto");

        let config = cli.get_model_config().expect("should build config");
        assert!(matches!(config.backend, BackendPreference::Auto));
    }

    #[test]
    fn test_backend_preference_parsing() {
        // Test all valid backend preferences
        let backends = vec![
            ("auto", BackendPreference::Auto),
            ("metal", BackendPreference::Metal),
            ("coreml", BackendPreference::CoreML),
            ("mlx", BackendPreference::Mlx),
        ];

        for (backend_str, expected) in backends {
            let cli = parse_cli(vec![
                "aosctl",
                "--model-backend",
                backend_str,
                "adapter-list",
            ]);

            let config = cli.get_model_config().expect("should build config");
            assert!(
                std::mem::discriminant(&config.backend) == std::mem::discriminant(&expected),
                "backend string '{}' should parse to expected variant",
                backend_str
            );
        }
    }

    #[test]
    fn test_invalid_backend_preference() {
        let cli = parse_cli(vec![
            "aosctl",
            "--model-backend",
            "invalid_backend",
            "adapter-list",
        ]);

        let result = cli.get_model_config();
        assert!(result.is_err(), "invalid backend should return error");
    }

    #[test]
    fn test_model_path_expansion() {
        let cli = parse_cli(vec![
            "aosctl",
            "--model-path",
            "relative/path/to/model",
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        // Path should be stored as-is (PathBuf doesn't auto-expand)
        assert_eq!(config.path.to_str().unwrap(), "relative/path/to/model");
    }

    #[test]
    fn test_model_path_with_spaces() {
        let cli = parse_cli(vec![
            "aosctl",
            "--model-path",
            "/path with spaces/to model",
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        assert_eq!(config.path.to_str().unwrap(), "/path with spaces/to model");
    }

    #[test]
    fn test_model_path_with_special_chars() {
        let paths = vec![
            "/path/with-dashes/model",
            "/path/with_underscores/model",
            "/path/with.dots/model",
            "/path/with$dollar/model",
        ];

        for path in paths {
            let cli = parse_cli(vec!["aosctl", "--model-path", path, "adapter-list"]);

            let config = cli.get_model_config().expect("should build config");
            assert_eq!(config.path.to_str().unwrap(), path);
        }
    }

    #[test]
    fn test_config_independence_between_calls() {
        // First call with one config
        let cli1 = parse_cli(vec![
            "aosctl",
            "--model-path",
            "/path1",
            "--model-backend",
            "metal",
            "adapter-list",
        ]);
        let config1 = cli1.get_model_config().expect("should build config");

        // Second call with different config
        let cli2 = parse_cli(vec![
            "aosctl",
            "--model-path",
            "/path2",
            "--model-backend",
            "coreml",
            "adapter-list",
        ]);
        let config2 = cli2.get_model_config().expect("should build config");

        // Verify they're independent
        assert_ne!(config1.path, config2.path);
        assert!(
            std::mem::discriminant(&config1.backend) != std::mem::discriminant(&config2.backend)
        );
    }

    #[test]
    fn test_empty_model_path() {
        // CLI with no model path should use env or defaults
        let cli = parse_cli(vec!["aosctl", "adapter-list"]);
        let result = cli.get_model_config();

        // Should succeed with default from env
        assert!(result.is_ok());
    }

    #[test]
    fn test_case_sensitivity_backend() {
        // Backend should be case-sensitive (lowercase only)
        let cli = parse_cli(vec![
            "aosctl",
            "--model-backend",
            "Metal", // Capital M
            "adapter-list",
        ]);

        let result = cli.get_model_config();
        // Should fail as backend names are lowercase
        assert!(result.is_err());
    }

    #[test]
    fn test_global_flags_with_model_config() {
        let cli = parse_cli(vec![
            "aosctl",
            "--json",
            "--verbose",
            "--model-path",
            "/test/path",
            "--model-backend",
            "metal",
            "adapter-list",
        ]);

        // Global flags shouldn't interfere with config loading
        let config = cli.get_model_config().expect("should build config");
        assert_eq!(config.path.to_str().unwrap(), "/test/path");
        assert!(matches!(config.backend, BackendPreference::Metal));

        // And flags should still be set
        assert!(cli.json);
        assert!(cli.verbose);
    }
}
