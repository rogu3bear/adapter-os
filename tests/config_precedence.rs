#![cfg(all(test, feature = "extended-tests"))]

//! Configuration precedence and guard behavior tests aligned with the v3 API.

use adapteros_config::{
<<<<<<< HEAD
    guards::{safe_env_var, safe_env_var_or, strict_env_var, ConfigGuards},
    ConfigLoader,
};
use std::io::Write;
use std::sync::{Mutex, MutexGuard, OnceLock};
use tempfile::NamedTempFile;

fn write_manifest(contents: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("create manifest");
    write!(file, "{}", contents).expect("write manifest");
    file.flush().expect("flush manifest");
    file
}

fn loader() -> ConfigLoader {
    ConfigLoader::new()
}

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_env() -> MutexGuard<'static, ()> {
    env_lock().lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn test_config_precedence_order() {
    let _guard = lock_env();
    let manifest = write_manifest(
=======
    get_config, initialize_config, is_frozen, safe_env_var, safe_env_var_or, strict_env_var,
    ConfigGuards, ConfigLoader,
};
use adapteros_core::Result;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_config_precedence_order() {
    // Create a temporary manifest file
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
>>>>>>> integration-branch
        r#"
[server]
host = "127.0.0.1"
port = 8080

[database]
url = "sqlite://manifest.db"

[policy]
strict_mode = true
<<<<<<< HEAD
"#,
    );
=======
"#
    )
    .unwrap();
    temp_file.flush().unwrap();
>>>>>>> integration-branch

    std::env::set_var("ADAPTEROS_SERVER_PORT", "9090");
    std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://env.db");

<<<<<<< HEAD
    let cli_args = vec![
        "adapteros".to_string(),
        "--adapteros-server-host".to_string(),
        "0.0.0.0".to_string(),
    ];

    let config = loader()
        .load(
            cli_args,
            Some(manifest.path().to_string_lossy().to_string()),
        )
        .expect("load config");
=======
    // Test precedence: CLI > ENV > manifest
    let loader = ConfigLoader::new();
    let config = loader
        .load(
            vec!["--server.host".to_string(), "0.0.0.0".to_string()],
            Some(temp_file.path().to_string_lossy().to_string()),
        )
        .unwrap();
>>>>>>> integration-branch

    assert_eq!(config.get("server.host"), Some(&"0.0.0.0".to_string()));
<<<<<<< HEAD
    assert_eq!(config.get("server.port"), Some(&"9090".to_string()));
    assert_eq!(
        config.get("database.url"),
        Some(&"sqlite://env.db".to_string())
    );
=======

    // ENV should win for port (over manifest)
    assert_eq!(config.get("server.port"), Some(&"9090".to_string()));

    // Manifest should win for database.url (no override)
    assert_eq!(
        config.get("database.url"),
        Some(&"sqlite://test.db".to_string())
    );

    // Manifest should win for policy.strict_mode (no override)
>>>>>>> integration-branch
    assert_eq!(config.get("policy.strict_mode"), Some(&"true".to_string()));

    std::env::remove_var("ADAPTEROS_SERVER_PORT");
    std::env::remove_var("ADAPTEROS_DATABASE_URL");
}

#[test]
fn test_config_freeze_metadata() {
    let _guard = lock_env();
    std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://freeze.db");
    std::env::remove_var("ADAPTEROS_SERVER_PORT");

    let config = loader()
        .load(vec!["adapteros".to_string()], None)
        .expect("load default config");

    assert!(config.is_frozen());
    assert!(!config.get_metadata().hash.is_empty());

    std::env::remove_var("ADAPTEROS_DATABASE_URL");
}

#[test]
fn test_config_validation_required_field() {
    let _guard = lock_env();
    let manifest = write_manifest(
        r#"
[server]
host = "127.0.0.1"
port = 8080
"#,
    );

    let result = loader().load(
        vec!["adapteros".to_string()],
        Some(manifest.path().to_string_lossy().to_string()),
    );

    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(message.contains("validation failed"));
    assert!(message.contains("database.url"));
}

#[test]
fn test_config_validation_invalid_type() {
    let _guard = lock_env();
    std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://valid.db");
    std::env::set_var("ADAPTEROS_SERVER_PORT", "not_a_number");

    let result = loader().load(vec!["adapteros".to_string()], None);
    assert!(result.is_err());
    let message = result.unwrap_err().to_string();
    assert!(message.contains("validation failed"));
    assert!(message.contains("server.port"));

    std::env::remove_var("ADAPTEROS_DATABASE_URL");
    std::env::remove_var("ADAPTEROS_SERVER_PORT");
}

#[test]
<<<<<<< HEAD
fn test_config_metadata_sources() {
    let _guard = lock_env();
    let manifest = write_manifest(
        r#"
[logging]
level = "info"
"#,
    );

    std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://sources.db");

    let cli_args = vec![
        "adapteros".to_string(),
        "--adapteros-logging-level".to_string(),
        "debug".to_string(),
    ];

    let config = loader()
        .load(
            cli_args.clone(),
            Some(manifest.path().to_string_lossy().to_string()),
        )
        .expect("load config with metadata");

    let metadata = config.get_metadata();
    assert_eq!(metadata.cli_args, cli_args);
    assert_eq!(
        metadata.manifest_path.as_deref(),
        Some(manifest.path().to_str().unwrap())
    );

    let sources = &metadata.sources;
    assert!(sources.iter().any(|s| s.source == "environment"));
    assert!(sources.iter().any(|s| s.source == "cli"));

    std::env::remove_var("ADAPTEROS_DATABASE_URL");
=======
fn test_config_freeze() {
    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    // Configuration should be frozen after loading
    assert!(config.is_frozen());
    assert!(!config.get_metadata().hash.is_empty());

    // Global freeze status should be true
    assert!(is_frozen());
}

#[test]
fn test_config_validation() {
    // Test valid configuration
    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    let validation_errors = config.validate().unwrap();
    assert!(validation_errors.is_empty());

    // Test invalid configuration (missing required field)
    let mut temp_file = NamedTempFile::new().new().unwrap();
    writeln!(
        temp_file,
        r#"
[server]
host = "127.0.0.1"
port = 8080
"#
    )
    .unwrap();
    temp_file.flush().unwrap();

    let config = loader
        .load(vec![], Some(temp_file.path().to_string_lossy().to_string()))
        .unwrap();

    let validation_errors = config.validate().unwrap();
    assert!(!validation_errors.is_empty());
    assert!(validation_errors.iter().any(|e| e.key == "database.url"));
}

#[test]
fn test_config_guards() {
    // Initialize guards
    ConfigGuards::initialize().unwrap();
    assert!(!ConfigGuards::is_frozen());

    // Freeze guards
    ConfigGuards::freeze().unwrap();
    assert!(ConfigGuards::is_frozen());

    // Test violation recording
    ConfigGuards::record_violation("test_operation", "test_message").unwrap();

    let violations = ConfigGuards::get_violations().unwrap();
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].attempted_operation, "test_operation");
    assert_eq!(violations[0].message, "test_message");

    // Clear violations
    ConfigGuards::clear_violations().unwrap();
    assert_eq!(ConfigGuards::get_violations().unwrap().len(), 0);
}

#[test]
fn test_safe_env_access_before_freeze() {
    ConfigGuards::initialize().unwrap();

    // Should work before freeze
    let result = safe_env_var("PATH");
    assert!(result.is_ok());

    let result = safe_env_var_or("PATH", "default");
    assert!(result.is_ok());

    let result = strict_env_var("PATH");
    assert!(result.is_ok());
}

#[test]
fn test_safe_env_access_after_freeze() {
    ConfigGuards::initialize().unwrap();
    ConfigGuards::freeze().unwrap();

    // Should fail after freeze
    let result = safe_env_var("PATH");
    assert!(result.is_err());

    let result = safe_env_var_or("PATH", "default");
    assert!(result.is_err());

    let result = strict_env_var("PATH");
    assert!(result.is_err());
}

#[test]
fn test_config_initialization() {
    // Test successful initialization
    let config = initialize_config(vec![], None).unwrap();
    assert!(config.is_frozen());

    // Test second initialization fails
    let result = initialize_config(vec![], None);
    assert!(result.is_err());

    // Test get_config works
    let config = get_config().unwrap();
    assert!(config.is_frozen());
>>>>>>> integration-branch
}

#[test]
fn test_config_hash_determinism() {
<<<<<<< HEAD
    let _guard = lock_env();
    std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://hash.db");
    std::env::remove_var("ADAPTEROS_SERVER_PORT");

    let loader = loader();
    let config_a = loader
        .load(vec!["adapteros".to_string()], None)
        .expect("first load");
    let config_b = loader
        .load(vec!["adapteros".to_string()], None)
        .expect("second load");

    assert_eq!(config_a.get_all(), config_b.get_all());

    std::env::remove_var("ADAPTEROS_DATABASE_URL");
}

#[test]
fn test_config_guards_lifecycle() {
    let _guard = env_lock().lock().unwrap();
    ConfigGuards::reset_for_tests();
    assert!(!ConfigGuards::is_frozen());

    assert!(safe_env_var("PATH")
        .expect("pre-freeze env access")
        .is_some());
    assert!(safe_env_var_or("PATH", "default").is_ok());
    assert!(strict_env_var("PATH").is_ok());

    ConfigGuards::freeze().expect("freeze guards");
    assert!(ConfigGuards::is_frozen());

    assert!(safe_env_var("PATH").is_err());
    assert!(safe_env_var_or("PATH", "default").is_err());
    assert!(strict_env_var("PATH").is_err());

    let violations = ConfigGuards::get_violations().expect("fetch violations");
    assert!(!violations.is_empty());
=======
    let loader = ConfigLoader::new();

    // Load same configuration twice
    let config1 = loader.load(vec![], None).unwrap();
    let config2 = loader.load(vec![], None).unwrap();

    // Hashes should be identical
    assert_eq!(config1.get_metadata().hash, config2.get_metadata().hash);
}

#[test]
fn test_config_boolean_parsing() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
[policy]
strict_mode = true
audit_logging = false
"#
    )
    .unwrap();
    temp_file.flush().unwrap();

    let loader = ConfigLoader::new();
    let config = loader
        .load(vec![], Some(temp_file.path().to_string_lossy().to_string()))
        .unwrap();

    assert_eq!(config.get("policy.strict_mode"), Some(&"true".to_string()));
    assert_eq!(
        config.get("policy.audit_logging"),
        Some(&"false".to_string())
    );
}

#[test]
fn test_config_integer_parsing() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
[server]
port = 8080
workers = 4

[database]
pool_size = 10
"#
    )
    .unwrap();
    temp_file.flush().unwrap();

    let loader = ConfigLoader::new();
    let config = loader
        .load(vec![], Some(temp_file.path().to_string_lossy().to_string()))
        .unwrap();

    assert_eq!(config.get("server.port"), Some(&"8080".to_string()));
    assert_eq!(config.get("server.workers"), Some(&"4".to_string()));
    assert_eq!(config.get("database.pool_size"), Some(&"10".to_string()));
}

#[test]
fn test_config_cli_boolean_flags() {
    let loader = ConfigLoader::new();
    let config = loader
        .load(vec!["--policy.strict_mode".to_string()], None)
        .unwrap();

    // Boolean flag without value should default to "true"
    assert_eq!(config.get("policy.strict_mode"), Some(&"true".to_string()));
}

#[test]
fn test_config_cli_key_value_pairs() {
    let loader = ConfigLoader::new();
    let config = loader
        .load(
            vec![
                "--server.host".to_string(),
                "0.0.0.0".to_string(),
                "--server.port".to_string(),
                "9090".to_string(),
            ],
            None,
        )
        .unwrap();

    assert_eq!(config.get("server.host"), Some(&"0.0.0.0".to_string()));
    assert_eq!(config.get("server.port"), Some(&"9090".to_string()));
}

#[test]
fn test_config_environment_variable_prefix() {
    // Set environment variables with different prefixes
    std::env::set_var("ADAPTEROS_SERVER_HOST", "0.0.0.0");
    std::env::set_var("OTHER_SERVER_PORT", "9090"); // Should be ignored
    std::env::set_var("ADAPTEROS_POLICY_STRICT_MODE", "false");

    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    // Only ADAPTEROS_ prefixed vars should be loaded
    assert_eq!(config.get("server.host"), Some(&"0.0.0.0".to_string()));
    assert_eq!(config.get("policy.strict_mode"), Some(&"false".to_string()));

    // Non-prefixed vars should not be loaded
    assert_eq!(config.get("server.port"), None);

    // Clean up
    std::env::remove_var("ADAPTEROS_SERVER_HOST");
    std::env::remove_var("OTHER_SERVER_PORT");
    std::env::remove_var("ADAPTEROS_POLICY_STRICT_MODE");
}

#[test]
fn test_config_environment_variable_conversion() {
    // Test underscore to dot conversion
    std::env::set_var("ADAPTEROS_SERVER_HOST", "0.0.0.0");
    std::env::set_var("ADAPTEROS_DATABASE_POOL_SIZE", "20");

    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    assert_eq!(config.get("server.host"), Some(&"0.0.0.0".to_string()));
    assert_eq!(config.get("database.pool.size"), Some(&"20".to_string()));

    // Clean up
    std::env::remove_var("ADAPTEROS_SERVER_HOST");
    std::env::remove_var("ADAPTEROS_DATABASE_POOL_SIZE");
}

#[test]
fn test_config_manifest_validation() {
    let loader = ConfigLoader::new();

    // Test valid manifest
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
[server]
host = "127.0.0.1"
port = 8080
"#
    )
    .unwrap();
    temp_file.flush().unwrap();

    let result = loader.validate_manifest(temp_file.path().to_string_lossy().as_ref());
    assert!(result.is_ok());

    // Test invalid manifest
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(temp_file, "invalid toml content").unwrap();
    temp_file.flush().unwrap();

    let result = loader.validate_manifest(temp_file.path().to_string_lossy().as_ref());
    assert!(result.is_err());
}

#[test]
fn test_config_get_or_default() {
    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    // Test existing key
    let host = config.get_or_default("server.host", "127.0.0.1");
    assert_eq!(host, "127.0.0.1");

    // Test non-existing key
    let non_existing = config.get_or_default("non.existing.key", "default_value");
    assert_eq!(non_existing, "default_value");
}

#[test]
fn test_config_metadata() {
    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    let metadata = config.get_metadata();

    // Check metadata fields
    assert!(!metadata.frozen_at.is_empty());
    assert!(!metadata.hash.is_empty());
    assert!(metadata.sources.is_empty()); // No sources for empty config
    assert!(metadata.manifest_path.is_none());
    assert!(metadata.cli_args.is_empty());
}

#[test]
fn test_config_schema() {
    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    let schema = config.get_schema();

    // Check schema fields
    assert_eq!(schema.version, "1.0.0");
    assert!(!schema.fields.is_empty());

    // Check specific field definitions
    assert!(schema.fields.contains_key("server.host"));
    assert!(schema.fields.contains_key("server.port"));
    assert!(schema.fields.contains_key("database.url"));
    assert!(schema.fields.contains_key("policy.strict_mode"));
}

#[test]
fn test_config_json_serialization() {
    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    // Test JSON serialization
    let json = config.to_json().unwrap();
    assert!(!json.is_empty());

    // Parse JSON to verify it's valid
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_object());
}

#[test]
fn test_config_display() {
    let loader = ConfigLoader::new();
    let config = loader.load(vec![], None).unwrap();

    // Test Display implementation
    let display_string = format!("{}", config);
    assert!(display_string.contains("DeterministicConfig"));
    assert!(display_string.contains("frozen: true"));
    assert!(display_string.contains("hash:"));
    assert!(display_string.contains("values: 0 entries"));
    assert!(display_string.contains("sources: 0 entries"));
>>>>>>> integration-branch
}
