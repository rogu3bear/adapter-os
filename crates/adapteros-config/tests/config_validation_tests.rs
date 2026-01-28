//! Comprehensive configuration validation tests
//!
//! Tests for:
//! - Path rejection (/tmp validation)
//! - Environment variable parsing
//! - Default value handling
//! - Invalid config rejection
//! - Edge cases and error conditions

use adapteros_config::schema::{
    default_schema, parse_bool, validate_value, ConfigType, ConfigVariable,
};
use adapteros_config::{
    reject_tmp_persistent_path, resolve_adapters_root, resolve_database_url,
    resolve_embedding_model_path, resolve_index_root, resolve_manifest_cache_dir,
    resolve_status_path, resolve_telemetry_dir, resolve_worker_socket_for_cp,
    resolve_worker_socket_for_worker,
};
mod test_env;
use std::collections::HashMap;
use std::path::PathBuf;
use test_env::TestEnvGuard;

fn setup_test_env() -> TestEnvGuard {
    TestEnvGuard::new()
}

// ============================================================================
// Path Rejection Tests (/tmp validation)
// ============================================================================

#[test]
fn test_reject_tmp_path_direct() {
    let path = PathBuf::from("/tmp/test.db");
    let err = reject_tmp_persistent_path(&path, "test-db")
        .unwrap_err()
        .to_string();
    assert!(err.contains("must not be under /tmp"));
    assert!(err.contains("test-db"));
}

#[test]
fn test_reject_private_tmp_path() {
    let path = PathBuf::from("/private/tmp/test.db");
    let err = reject_tmp_persistent_path(&path, "test-db")
        .unwrap_err()
        .to_string();
    assert!(err.contains("must not be under /tmp"));
    assert!(err.contains("test-db"));
}

#[test]
fn test_reject_tmp_with_sqlite_prefix() {
    let path = PathBuf::from("sqlite:///tmp/test.db");
    let err = reject_tmp_persistent_path(&path, "database")
        .unwrap_err()
        .to_string();
    assert!(err.contains("must not be under /tmp"));
}

#[test]
fn test_reject_tmp_with_sqlite_double_slash() {
    // sqlite://tmp is relative path, not /tmp - should pass validation
    let path = PathBuf::from("sqlite://tmp/test.db");
    // This should actually pass since "tmp" is relative, not "/tmp"
    assert!(reject_tmp_persistent_path(&path, "database").is_ok());
}

#[test]
fn test_reject_tmp_with_file_prefix() {
    let path = PathBuf::from("file:///tmp/test.db");
    let err = reject_tmp_persistent_path(&path, "database")
        .unwrap_err()
        .to_string();
    assert!(err.contains("must not be under /tmp"));
}

#[test]
fn test_accept_var_path() {
    let path = PathBuf::from("var/aos-cp.sqlite3");
    assert!(reject_tmp_persistent_path(&path, "database").is_ok());
}

#[test]
fn test_accept_absolute_var_path() {
    let path = PathBuf::from("/var/lib/adapteros/aos-cp.sqlite3");
    assert!(reject_tmp_persistent_path(&path, "database").is_ok());
}

#[test]
fn test_telemetry_dir_rejects_nested_tmp() {
    let _env = setup_test_env();
    std::env::set_var("AOS_TELEMETRY_DIR", "/tmp/nested/telemetry");
    let err = resolve_telemetry_dir().unwrap_err().to_string();
    assert!(err.contains("must not be under /tmp"));
    std::env::remove_var("AOS_TELEMETRY_DIR");
}

#[test]
fn test_index_root_rejects_private_tmp_nested() {
    let _env = setup_test_env();
    std::env::set_var("AOS_INDEX_DIR", "/private/tmp/nested/indices");
    let err = resolve_index_root().unwrap_err().to_string();
    assert!(err.contains("must not be under /tmp"));
    std::env::remove_var("AOS_INDEX_DIR");
}

#[test]
fn test_manifest_cache_rejects_tmp_nested() {
    let _env = setup_test_env();
    std::env::set_var("AOS_MANIFEST_CACHE_DIR", "/tmp/nested/cache");
    let err = resolve_manifest_cache_dir().unwrap_err().to_string();
    assert!(err.contains("must not be under /tmp"));
    std::env::remove_var("AOS_MANIFEST_CACHE_DIR");
}

#[test]
fn test_status_path_rejects_tmp_nested() {
    let _env = setup_test_env();
    std::env::set_var("AOS_STATUS_PATH", "/tmp/nested/status.json");
    let err = resolve_status_path().unwrap_err().to_string();
    assert!(err.contains("must not be under /tmp"));
    std::env::remove_var("AOS_STATUS_PATH");
}

#[test]
fn test_worker_socket_rejects_tmp() {
    let _env = setup_test_env();
    std::env::set_var("AOS_WORKER_SOCKET", "/tmp/worker.sock");
    let err = resolve_worker_socket_for_worker("tenant-x", None)
        .unwrap_err()
        .to_string();
    assert!(err.contains("must not be under /tmp"));
    std::env::remove_var("AOS_WORKER_SOCKET");
}

#[test]
fn test_cp_socket_rejects_private_tmp() {
    let _env = setup_test_env();
    std::env::set_var("AOS_WORKER_SOCKET", "/private/tmp/cp.sock");
    let err = resolve_worker_socket_for_cp().unwrap_err().to_string();
    assert!(err.contains("must not be under /tmp"));
    std::env::remove_var("AOS_WORKER_SOCKET");
}

// ============================================================================
// Environment Variable Parsing Tests
// ============================================================================

#[test]
fn test_parse_bool_true_variants() {
    assert!(parse_bool("true").unwrap());
    assert!(parse_bool("TRUE").unwrap());
    assert!(parse_bool("True").unwrap());
    assert!(parse_bool("1").unwrap());
    assert!(parse_bool("yes").unwrap());
    assert!(parse_bool("YES").unwrap());
    assert!(parse_bool("on").unwrap());
    assert!(parse_bool("ON").unwrap());
}

#[test]
fn test_parse_bool_false_variants() {
    assert!(!parse_bool("false").unwrap());
    assert!(!parse_bool("FALSE").unwrap());
    assert!(!parse_bool("False").unwrap());
    assert!(!parse_bool("0").unwrap());
    assert!(!parse_bool("no").unwrap());
    assert!(!parse_bool("NO").unwrap());
    assert!(!parse_bool("off").unwrap());
    assert!(!parse_bool("OFF").unwrap());
}

#[test]
fn test_parse_bool_invalid() {
    assert!(parse_bool("maybe").is_err());
    assert!(parse_bool("2").is_err());
    assert!(parse_bool("").is_err());
    assert!(parse_bool("truthy").is_err());
}

#[test]
fn test_env_var_with_empty_value() {
    std::env::set_var("AOS_TEST_EMPTY", "");
    // Empty env vars should be treated as not set
    let value = std::env::var("AOS_TEST_EMPTY").unwrap_or_default();
    assert_eq!(value, "");
    std::env::remove_var("AOS_TEST_EMPTY");
}

#[test]
fn test_env_var_with_whitespace() {
    std::env::set_var("AOS_TEST_WHITESPACE", "  value  ");
    let value = std::env::var("AOS_TEST_WHITESPACE").unwrap();
    // Env vars preserve whitespace
    assert_eq!(value, "  value  ");
    std::env::remove_var("AOS_TEST_WHITESPACE");
}

// ============================================================================
// Default Value Handling Tests
// ============================================================================

#[test]
fn test_schema_default_values_exist() {
    let schema = default_schema();

    // Critical defaults that should always be set
    let server_port = schema.get_variable("AOS_SERVER_PORT").unwrap();
    assert_eq!(server_port.default.as_deref(), Some("8080"));

    let log_level = schema.get_variable("AOS_LOG_LEVEL").unwrap();
    assert_eq!(log_level.default.as_deref(), Some("info"));

    let model_backend = schema.get_variable("AOS_MODEL_BACKEND").unwrap();
    assert_eq!(model_backend.default.as_deref(), Some("mlx"));
}

#[test]
fn test_default_paths_not_in_tmp() {
    let schema = default_schema();

    // Verify all path defaults avoid /tmp
    let path_vars = [
        "AOS_DATABASE_URL",
        "AOS_MODEL_PATH",
        "AOS_ADAPTERS_DIR",
        "AOS_ARTIFACTS_DIR",
        "AOS_EMBEDDING_MODEL_PATH",
    ];

    for var_name in &path_vars {
        let var = schema.get_variable(var_name).unwrap();
        if let Some(default) = &var.default {
            assert!(
                !default.contains("/tmp") && !default.contains("/private/tmp"),
                "{} default should not use /tmp: {}",
                var_name,
                default
            );
        }
    }
}

#[test]
fn test_adapters_root_default_value() {
    let _env = setup_test_env();
    std::env::remove_var("AOS_ADAPTERS_ROOT");
    std::env::remove_var("AOS_ADAPTERS_DIR");

    let resolved = resolve_adapters_root().unwrap();
    // Path may be absolute or relative depending on resolution context
    assert!(
        resolved.path.ends_with("var/adapters"),
        "Expected path to end with 'var/adapters', got: {:?}",
        resolved.path
    );
}

#[test]
fn test_embedding_model_default_value() {
    let _env = setup_test_env();
    std::env::remove_var("AOS_EMBEDDING_MODEL_PATH");

    let resolved = resolve_embedding_model_path().unwrap();
    // Path may be absolute or relative depending on resolution context
    // Default may resolve to var/models or var/model-cache/models
    let path_str = resolved.path.to_string_lossy();
    assert!(
        path_str.contains("bge-small-en-v1.5"),
        "Expected path to contain 'bge-small-en-v1.5', got: {:?}",
        resolved.path
    );
}

#[test]
fn test_database_url_default_value() {
    let _env = setup_test_env();
    std::env::remove_var("AOS_DATABASE_URL");
    std::env::remove_var("DATABASE_URL");

    let resolved = resolve_database_url().unwrap();
    assert_eq!(resolved.path, PathBuf::from("sqlite://var/aos-cp.sqlite3"));
}

// ============================================================================
// Invalid Config Rejection Tests
// ============================================================================

#[test]
fn test_validate_integer_non_numeric() {
    let var = ConfigVariable::new("AOS_SERVER_PORT")
        .config_type(ConfigType::Integer {
            min: Some(1),
            max: Some(65535),
        })
        .build();

    let err = validate_value(&var, "not-a-number").unwrap_err();
    assert!(err.message.contains("Cannot parse"));
    assert_eq!(err.variable, "AOS_SERVER_PORT");
}

#[test]
fn test_validate_integer_float_value() {
    let var = ConfigVariable::new("AOS_SERVER_PORT")
        .config_type(ConfigType::Integer {
            min: Some(1),
            max: Some(65535),
        })
        .build();

    let err = validate_value(&var, "8080.5").unwrap_err();
    assert!(err.message.contains("Cannot parse"));
}

#[test]
fn test_validate_integer_out_of_range_low() {
    let var = ConfigVariable::new("AOS_SERVER_PORT")
        .config_type(ConfigType::Integer {
            min: Some(1),
            max: Some(65535),
        })
        .build();

    let err = validate_value(&var, "-100").unwrap_err();
    assert!(err.message.contains("below minimum"));
}

#[test]
fn test_validate_integer_out_of_range_high() {
    let var = ConfigVariable::new("AOS_SERVER_PORT")
        .config_type(ConfigType::Integer {
            min: Some(1),
            max: Some(65535),
        })
        .build();

    let err = validate_value(&var, "99999").unwrap_err();
    assert!(err.message.contains("exceeds maximum"));
}

#[test]
fn test_validate_float_non_numeric() {
    let var = ConfigVariable::new("AOS_MEMORY_HEADROOM_PCT")
        .config_type(ConfigType::Float {
            min: Some(0.0),
            max: Some(1.0),
        })
        .build();

    let err = validate_value(&var, "not-a-float").unwrap_err();
    assert!(err.message.contains("Cannot parse"));
}

#[test]
fn test_validate_float_out_of_range_low() {
    let var = ConfigVariable::new("AOS_MEMORY_HEADROOM_PCT")
        .config_type(ConfigType::Float {
            min: Some(0.05),
            max: Some(0.50),
        })
        .build();

    let err = validate_value(&var, "0.01").unwrap_err();
    assert!(err.message.contains("below minimum"));
}

#[test]
fn test_validate_float_out_of_range_high() {
    let var = ConfigVariable::new("AOS_MEMORY_HEADROOM_PCT")
        .config_type(ConfigType::Float {
            min: Some(0.05),
            max: Some(0.50),
        })
        .build();

    let err = validate_value(&var, "0.99").unwrap_err();
    assert!(err.message.contains("exceeds maximum"));
}

#[test]
fn test_validate_enum_invalid_value() {
    let var = ConfigVariable::new("AOS_MODEL_BACKEND")
        .config_type(ConfigType::Enum {
            values: vec![
                "auto".to_string(),
                "coreml".to_string(),
                "metal".to_string(),
            ],
        })
        .build();

    let err = validate_value(&var, "invalid-backend").unwrap_err();
    assert!(err.message.contains("Invalid value"));
    assert!(err.message.contains("must be one of"));
}

#[test]
fn test_validate_enum_case_insensitive() {
    let var = ConfigVariable::new("AOS_MODEL_BACKEND")
        .config_type(ConfigType::Enum {
            values: vec![
                "auto".to_string(),
                "coreml".to_string(),
                "metal".to_string(),
            ],
        })
        .build();

    // Enum validation is case-insensitive
    assert!(validate_value(&var, "AUTO").is_ok());
    assert!(validate_value(&var, "CoreML").is_ok());
    assert!(validate_value(&var, "METAL").is_ok());
}

#[test]
fn test_validate_url_invalid_scheme() {
    let var = ConfigVariable::new("AOS_DATABASE_URL")
        .config_type(ConfigType::Url)
        .build();

    let err = validate_value(&var, "invalid://test.db").unwrap_err();
    assert!(err.message.contains("Invalid URL scheme"));
}

#[test]
fn test_validate_url_no_scheme() {
    let var = ConfigVariable::new("AOS_DATABASE_URL")
        .config_type(ConfigType::Url)
        .build();

    let err = validate_value(&var, "just-a-path.db").unwrap_err();
    assert!(err.message.contains("Invalid URL scheme"));
}

#[test]
fn test_validate_duration_invalid_format() {
    let var = ConfigVariable::new("AOS_DATABASE_TIMEOUT")
        .config_type(ConfigType::Duration)
        .build();

    let err = validate_value(&var, "invalid-duration").unwrap_err();
    assert!(err.message.contains("duration"));
}

#[test]
fn test_validate_duration_invalid_unit() {
    let var = ConfigVariable::new("AOS_DATABASE_TIMEOUT")
        .config_type(ConfigType::Duration)
        .build();

    let err = validate_value(&var, "30x").unwrap_err();
    assert!(err.message.contains("duration"));
}

#[test]
fn test_validate_byte_size_invalid_format() {
    let var = ConfigVariable::new("AOS_LOG_MAX_SIZE")
        .config_type(ConfigType::ByteSize)
        .build();

    let err = validate_value(&var, "invalid-size").unwrap_err();
    assert!(err.message.contains("byte size"));
}

#[test]
fn test_validate_byte_size_invalid_unit() {
    let var = ConfigVariable::new("AOS_LOG_MAX_SIZE")
        .config_type(ConfigType::ByteSize)
        .build();

    let err = validate_value(&var, "100TB").unwrap_err();
    assert!(err.message.contains("byte size"));
}

#[test]
fn test_validate_path_empty() {
    let var = ConfigVariable::new("AOS_MODEL_PATH")
        .config_type(ConfigType::Path { must_exist: false })
        .build();

    let err = validate_value(&var, "").unwrap_err();
    assert!(err.message.contains("Path cannot be empty"));
}

#[test]
fn test_validate_path_must_exist_missing() {
    let var = ConfigVariable::new("AOS_MODEL_PATH")
        .config_type(ConfigType::Path { must_exist: true })
        .build();

    let err = validate_value(&var, "/nonexistent/path/to/model").unwrap_err();
    assert!(err.message.contains("does not exist"));
}

// ============================================================================
// Schema Validation Tests
// ============================================================================

#[test]
fn test_schema_validate_all_with_invalid_values() {
    let schema = default_schema();
    let mut values = HashMap::new();

    // Add invalid values
    values.insert("AOS_SERVER_PORT".to_string(), "invalid".to_string());
    values.insert("AOS_MODEL_BACKEND".to_string(), "nonexistent".to_string());

    let result = schema.validate_all(&values);
    assert!(result.is_err());

    let errors = result.unwrap_err();
    assert!(errors.len() >= 2);
}

#[test]
fn test_schema_validate_all_with_valid_values() {
    let schema = default_schema();
    let mut values = HashMap::new();

    // Add valid values
    values.insert("AOS_SERVER_PORT".to_string(), "8080".to_string());
    values.insert("AOS_MODEL_BACKEND".to_string(), "mlx".to_string());
    values.insert("AOS_LOG_LEVEL".to_string(), "info".to_string());

    let result = schema.validate_all(&values);
    assert!(result.is_ok());
}

#[test]
fn test_schema_get_variable_by_name() {
    let schema = default_schema();

    let var = schema.get_variable("AOS_SERVER_PORT");
    assert!(var.is_some());
    assert_eq!(var.unwrap().name, "AOS_SERVER_PORT");
}

#[test]
fn test_schema_get_variable_missing() {
    let schema = default_schema();

    let var = schema.get_variable("AOS_NONEXISTENT_VAR");
    assert!(var.is_none());
}

#[test]
fn test_schema_get_category() {
    let schema = default_schema();

    let model_vars = schema.get_category("MODEL");
    assert!(!model_vars.is_empty());

    let server_vars = schema.get_category("SERVER");
    assert!(!server_vars.is_empty());
}

#[test]
fn test_schema_get_deprecated() {
    let schema = default_schema();

    let deprecated = schema.get_deprecated();
    // Schema should have some deprecated variables
    assert!(!deprecated.is_empty());
}

#[test]
fn test_schema_get_sensitive() {
    let schema = default_schema();

    let sensitive = schema.get_sensitive();
    // Should include JWT secret at minimum
    assert!(sensitive
        .iter()
        .any(|v| v.name == "AOS_SECURITY_JWT_SECRET"));
}

// ============================================================================
// Edge Cases and Error Conditions
// ============================================================================

#[test]
fn test_env_var_precedence_over_default() {
    let _env = setup_test_env();
    std::env::set_var("AOS_TELEMETRY_DIR", "var/custom-telemetry");

    let resolved = resolve_telemetry_dir().unwrap();
    // Path may be resolved to absolute, so check it ends with the custom path
    assert!(
        resolved.path.ends_with("var/custom-telemetry"),
        "Expected path to end with 'var/custom-telemetry', got: {:?}",
        resolved.path
    );

    std::env::remove_var("AOS_TELEMETRY_DIR");
}

#[test]
fn test_multiple_path_resolvers_consistency() {
    let _env = setup_test_env();
    // All path resolvers should reject /tmp consistently
    std::env::set_var("AOS_TELEMETRY_DIR", "/tmp/telemetry");
    std::env::set_var("AOS_INDEX_DIR", "/tmp/indices");
    std::env::set_var("AOS_MANIFEST_CACHE_DIR", "/tmp/cache");

    assert!(resolve_telemetry_dir().is_err());
    assert!(resolve_index_root().is_err());
    assert!(resolve_manifest_cache_dir().is_err());

    std::env::remove_var("AOS_TELEMETRY_DIR");
    std::env::remove_var("AOS_INDEX_DIR");
    std::env::remove_var("AOS_MANIFEST_CACHE_DIR");
}

#[test]
fn test_config_variable_display_value_sensitive() {
    let var = ConfigVariable::new("AOS_SECURITY_JWT_SECRET")
        .sensitive()
        .build();

    assert_eq!(var.display_value("my-secret-key"), "***REDACTED***");
}

#[test]
fn test_config_variable_display_value_non_sensitive() {
    let var = ConfigVariable::new("AOS_SERVER_PORT").build();

    assert_eq!(var.display_value("8080"), "8080");
}

#[test]
fn test_validation_error_display() {
    let var = ConfigVariable::new("AOS_SERVER_PORT")
        .config_type(ConfigType::Integer {
            min: Some(1),
            max: Some(65535),
        })
        .build();

    let err = validate_value(&var, "999999").unwrap_err();
    let err_str = format!("{}", err);

    assert!(err_str.contains("AOS_SERVER_PORT"));
    assert!(err_str.contains("999999"));
    assert!(err_str.contains("expected"));
}

#[test]
fn test_duration_validation_all_units() {
    let var = ConfigVariable::new("AOS_TIMEOUT")
        .config_type(ConfigType::Duration)
        .build();

    // All valid duration units
    assert!(validate_value(&var, "500ms").is_ok());
    assert!(validate_value(&var, "30s").is_ok());
    assert!(validate_value(&var, "5m").is_ok());
    assert!(validate_value(&var, "1h").is_ok());
    assert!(validate_value(&var, "1d").is_ok());
    assert!(validate_value(&var, "30").is_ok()); // Plain number = seconds
}

#[test]
fn test_byte_size_validation_all_units() {
    let var = ConfigVariable::new("AOS_SIZE")
        .config_type(ConfigType::ByteSize)
        .build();

    // All valid byte size units
    assert!(validate_value(&var, "1024").is_ok()); // Plain bytes
    assert!(validate_value(&var, "1KB").is_ok());
    assert!(validate_value(&var, "1K").is_ok());
    assert!(validate_value(&var, "1MB").is_ok());
    assert!(validate_value(&var, "1M").is_ok());
    assert!(validate_value(&var, "1GB").is_ok());
    assert!(validate_value(&var, "1G").is_ok());
    assert!(validate_value(&var, "1.5GB").is_ok()); // Decimal values
}

#[test]
fn test_bool_validation_comprehensive() {
    let var = ConfigVariable::new("AOS_ENABLED")
        .config_type(ConfigType::Bool)
        .build();

    // All valid boolean representations
    assert!(validate_value(&var, "true").is_ok());
    assert!(validate_value(&var, "false").is_ok());
    assert!(validate_value(&var, "1").is_ok());
    assert!(validate_value(&var, "0").is_ok());
    assert!(validate_value(&var, "yes").is_ok());
    assert!(validate_value(&var, "no").is_ok());
    assert!(validate_value(&var, "on").is_ok());
    assert!(validate_value(&var, "off").is_ok());

    // Case insensitive
    assert!(validate_value(&var, "TRUE").is_ok());
    assert!(validate_value(&var, "FALSE").is_ok());

    // Invalid values
    assert!(validate_value(&var, "maybe").is_err());
    assert!(validate_value(&var, "2").is_err());
}
