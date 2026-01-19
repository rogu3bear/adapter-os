//! Tests for CLI configuration loading
//!
//! Tests that the CLI properly loads configuration from:
//! - Environment variables
//! - CLI arguments
//! - Precedence rules (CLI > ENV > defaults)

#![allow(unused_variables)]

use adapteros_config::BackendPreference;
use clap::Parser;
use std::env;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

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

    fn unset(key: &str) -> Self {
        let old_value = env::var(key).ok();
        env::remove_var(key);
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
    use serial_test::serial;

    fn new_temp_model_dir() -> TempDir {
        let root = PathBuf::from("var").join("tmp");
        fs::create_dir_all(&root).expect("create var/tmp");
        TempDir::new_in(&root).expect("temp dir")
    }

    fn new_temp_model_subdir(name: &str) -> (TempDir, PathBuf) {
        let root = new_temp_model_dir();
        let path = root.path().join(name);
        fs::create_dir_all(&path).expect("create model dir");
        (root, path)
    }

    fn set_temp_model_path_env() -> (TempDir, EnvGuard) {
        let temp_dir = new_temp_model_dir();
        let guard = EnvGuard::set("AOS_MODEL_PATH", temp_dir.path().to_str().unwrap());
        (temp_dir, guard)
    }

    #[test]
    #[serial]
    fn test_model_config_from_cli_args() {
        let (temp_dir, _model_path_guard) = set_temp_model_path_env();
        let cli = parse_cli(vec![
            "aosctl",
            "--model-path",
            temp_dir.path().to_str().unwrap(),
            "--model-backend",
            "metal",
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        assert_eq!(
            config.path.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        );
        assert!(matches!(config.backend, BackendPreference::Metal));
    }

    #[test]
    #[serial]
    fn test_model_config_from_env_vars() {
        let temp_dir = new_temp_model_dir();
        let _model_path_guard = EnvGuard::set("AOS_MODEL_PATH", temp_dir.path().to_str().unwrap());
        let _backend_guard = EnvGuard::set("AOS_MODEL_BACKEND", "coreml");

        let cli = parse_cli(vec!["aosctl", "adapter-list"]);

        let config = cli.get_model_config().expect("should build config");
        // ENV vars are picked up via clap's env attribute
        assert_eq!(
            cli.model_path.as_deref(),
            Some(temp_dir.path().to_str().unwrap())
        );
        assert_eq!(cli.model_backend, "coreml");
    }

    #[test]
    #[serial]
    fn test_cli_overrides_env() {
        let env_dir = new_temp_model_dir();
        let cli_dir = new_temp_model_dir();
        let _model_path_guard = EnvGuard::set("AOS_MODEL_PATH", env_dir.path().to_str().unwrap());
        let _backend_guard = EnvGuard::set("AOS_MODEL_BACKEND", "coreml");

        let cli = parse_cli(vec![
            "aosctl",
            "--model-path",
            cli_dir.path().to_str().unwrap(),
            "--model-backend",
            "metal",
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        // CLI args should override ENV
        assert_eq!(
            config.path.to_str().unwrap(),
            cli_dir.path().to_str().unwrap()
        );
        assert!(matches!(config.backend, BackendPreference::Metal));
    }

    #[test]
    #[serial]
    fn test_default_backend_preference() {
        let (_temp_dir, _model_path_guard) = set_temp_model_path_env();
        let _backend_guard = EnvGuard::unset("AOS_MODEL_BACKEND");
        let cli = parse_cli(vec!["aosctl", "adapter-list"]);

        // Default should be "auto"
        assert_eq!(cli.model_backend, "auto");

        let config = cli.get_model_config().expect("should build config");
        assert!(matches!(config.backend, BackendPreference::Auto));
    }

    #[test]
    #[serial]
    fn test_backend_preference_parsing() {
        let (_temp_dir, _model_path_guard) = set_temp_model_path_env();
        let _backend_guard = EnvGuard::unset("AOS_MODEL_BACKEND");
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
    #[serial]
    fn test_invalid_backend_preference() {
        let (_temp_dir, _model_path_guard) = set_temp_model_path_env();
        let _backend_guard = EnvGuard::unset("AOS_MODEL_BACKEND");
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
    #[serial]
    fn test_model_path_expansion() {
        let (temp_dir, _model_path_guard) = set_temp_model_path_env();
        let cli = parse_cli(vec![
            "aosctl",
            "--model-path",
            temp_dir.path().to_str().unwrap(),
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        // Path should be stored as-is (PathBuf doesn't auto-expand)
        assert_eq!(
            config.path.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        );
    }

    #[test]
    #[serial]
    fn test_model_path_with_spaces() {
        let (_temp_dir, path) = new_temp_model_subdir("path with spaces/to model");
        let _model_path_guard = EnvGuard::set("AOS_MODEL_PATH", path.to_str().unwrap());
        let cli = parse_cli(vec![
            "aosctl",
            "--model-path",
            path.to_str().unwrap(),
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        assert_eq!(config.path.to_str().unwrap(), path.to_str().unwrap());
    }

    #[test]
    #[serial]
    fn test_model_path_with_special_chars() {
        let root = new_temp_model_dir();
        let _model_path_guard = EnvGuard::set("AOS_MODEL_PATH", root.path().to_str().unwrap());
        let paths = vec![
            "with-dashes",
            "with_underscores",
            "with.dots",
            "with$dollar",
        ];

        for path in paths {
            let model_path = root.path().join(path).join("model");
            fs::create_dir_all(&model_path).expect("create model dir");
            let cli = parse_cli(vec![
                "aosctl",
                "--model-path",
                model_path.to_str().unwrap(),
                "adapter-list",
            ]);

            let config = cli.get_model_config().expect("should build config");
            assert_eq!(config.path.to_str().unwrap(), model_path.to_str().unwrap());
        }
    }

    #[test]
    #[serial]
    fn test_config_independence_between_calls() {
        let (temp_dir1, _model_path_guard) = set_temp_model_path_env();
        let temp_dir2 = new_temp_model_dir();
        // First call with one config
        let cli1 = parse_cli(vec![
            "aosctl",
            "--model-path",
            temp_dir1.path().to_str().unwrap(),
            "--model-backend",
            "metal",
            "adapter-list",
        ]);
        let config1 = cli1.get_model_config().expect("should build config");

        // Second call with different config
        let cli2 = parse_cli(vec![
            "aosctl",
            "--model-path",
            temp_dir2.path().to_str().unwrap(),
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
    #[serial]
    fn test_empty_model_path() {
        let (_temp_dir, _model_path_guard) = set_temp_model_path_env();
        let _backend_guard = EnvGuard::unset("AOS_MODEL_BACKEND");
        // CLI with no model path should use env or defaults
        let cli = parse_cli(vec!["aosctl", "adapter-list"]);
        let result = cli.get_model_config();

        // Should succeed with default from env
        assert!(result.is_ok());
    }

    #[test]
    #[serial]
    fn test_case_sensitivity_backend() {
        let (_temp_dir, _model_path_guard) = set_temp_model_path_env();
        let _backend_guard = EnvGuard::unset("AOS_MODEL_BACKEND");
        // Backend names should be case-insensitive
        let cli = parse_cli(vec![
            "aosctl",
            "--model-backend",
            "Metal", // Capital M
            "adapter-list",
        ]);

        let config = cli.get_model_config().expect("should build config");
        assert!(matches!(config.backend, BackendPreference::Metal));
    }

    #[test]
    #[serial]
    fn test_global_flags_with_model_config() {
        let (temp_dir, _model_path_guard) = set_temp_model_path_env();
        let cli = parse_cli(vec![
            "aosctl",
            "--json",
            "--verbose",
            "--model-path",
            temp_dir.path().to_str().unwrap(),
            "--model-backend",
            "metal",
            "adapter-list",
        ]);

        // Global flags shouldn't interfere with config loading
        let config = cli.get_model_config().expect("should build config");
        assert_eq!(
            config.path.to_str().unwrap(),
            temp_dir.path().to_str().unwrap()
        );
        assert!(matches!(config.backend, BackendPreference::Metal));

        // And flags should still be set
        assert!(cli.is_json());
        assert!(cli.is_verbose());
    }
}
