#![cfg(all(test, feature = "extended-tests"))]

//! Configuration precedence and guard behavior tests aligned with the v3 API.

use adapteros_config::{
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
        r#"
[server]
host = "127.0.0.1"
port = 8080

[database]
url = "sqlite://manifest.db"

[policy]
strict_mode = true
"#,
    );

    std::env::set_var("ADAPTEROS_SERVER_PORT", "9090");
    std::env::set_var("ADAPTEROS_DATABASE_URL", "sqlite://env.db");

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

    assert_eq!(config.get("server.host"), Some(&"0.0.0.0".to_string()));
    assert_eq!(config.get("server.port"), Some(&"9090".to_string()));
    assert_eq!(
        config.get("database.url"),
        Some(&"sqlite://env.db".to_string())
    );
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
}

#[test]
fn test_config_hash_determinism() {
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
}
