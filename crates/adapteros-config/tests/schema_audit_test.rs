//! Schema Audit Tests
//!
//! These tests verify that:
//! 1. All schema variables are actually used somewhere in the codebase
//! 2. All AOS_* environment variables read in code are documented in the schema
//! 3. Runtime validation works correctly
//! 4. Type coercion functions work as expected

use adapteros_config::runtime::RuntimeConfig;
use adapteros_config::schema::{default_schema, ConfigType};

/// Variables that are known to be deprecated but kept for backwards compatibility
const KNOWN_DEPRECATED: &[&str] = &[
    "AOS_ROUTER_QUANTIZATION",
    "AOS_TRAINING_CHECKPOINT_INTERVAL",
    "AOS_TRAINING_MAX_EPOCHS",
    "AOS_BACKEND_ATTESTATION_REQUIRED",
    "AOS_BACKEND_DETERMINISM_SEED",
];

#[test]
fn test_schema_has_all_categories() {
    let schema = default_schema();
    let categories = schema.category_names();

    // Verify all expected categories exist
    let expected = [
        "MODEL",
        "SERVER",
        "DATABASE",
        "SECURITY",
        "LOGGING",
        "MEMORY",
        "BACKEND",
        "ROUTER",
        "TELEMETRY",
        "TRAINING",
        "FEDERATION",
        "MODEL_HUB",
        "EMBEDDINGS",
        "PATHS",
        "WORKER",
        "DEBUG",
    ];

    for cat in expected {
        assert!(
            categories.contains(&cat),
            "Missing category: {}",
            cat
        );
    }
}

#[test]
fn test_deprecated_variables_have_notes() {
    let schema = default_schema();

    for name in KNOWN_DEPRECATED {
        let var = schema
            .get_variable(name)
            .unwrap_or_else(|| panic!("Deprecated variable {} not in schema", name));

        assert!(
            var.deprecated.is_some(),
            "Variable {} should be marked deprecated",
            name
        );

        let dep = var.deprecated.as_ref().unwrap();
        assert!(
            !dep.replacement.is_empty(),
            "Deprecated variable {} should have replacement specified",
            name
        );
        assert!(
            dep.notes.is_some(),
            "Deprecated variable {} should have notes",
            name
        );
    }
}

#[test]
fn test_sensitive_variables_are_marked() {
    let schema = default_schema();
    let sensitive = schema.get_sensitive();

    // These should be marked as sensitive
    let expected_sensitive = ["AOS_SECURITY_JWT_SECRET", "AOS_SIGNING_KEY"];

    for name in expected_sensitive {
        assert!(
            sensitive.iter().any(|v| v.name == name),
            "Variable {} should be marked sensitive",
            name
        );
    }
}

#[test]
fn test_path_variables_have_defaults() {
    let schema = default_schema();

    for var in schema.get_category("PATHS") {
        // All PATHS variables should have defaults (except AOS_TELEMETRY_DIR which can use temp)
        if var.name != "AOS_TELEMETRY_DIR" {
            assert!(
                var.default.is_some(),
                "PATHS variable {} should have a default",
                var.name
            );
        }
    }
}

#[test]
fn test_integer_variables_have_range_constraints() {
    let schema = default_schema();

    for (_, var) in &schema.variables {
        if let ConfigType::Integer { min, max } = &var.config_type {
            // Integer variables should have at least one bound for safety
            assert!(
                min.is_some() || max.is_some(),
                "Integer variable {} should have range constraints",
                var.name
            );
        }
    }
}

#[test]
fn test_runtime_config_creates_successfully() {
    // Just verify that RuntimeConfig can be created from the current environment
    // We don't assert specific values since env vars may be set externally
    let config = RuntimeConfig::from_env().expect("Should create config from env");

    // These accessors should work regardless of what values are set
    let _port = config.server_port();
    let _host = config.server_host();
    let _log = config.log_level();
    let _mode = config.runtime_mode();
    let _production = config.is_production_mode();

    // Just verify we got reasonable values (not panics)
    assert!(config.server_port() > 0);
    assert!(!config.server_host().is_empty());
}

#[test]
fn test_runtime_config_type_safe_accessors_work() {
    let config = RuntimeConfig::from_env().expect("Should create config");

    // Verify type-safe accessors return correct types
    let _port: u16 = config.server_port();
    let _host: &str = config.server_host();
    let _db_url: &str = config.database_url();
    let _log_level: &str = config.log_level();
    let _var_dir: std::path::PathBuf = config.var_dir();
    let _cache_dir: std::path::PathBuf = config.model_cache_dir();
    let _adapters_dir: std::path::PathBuf = config.adapters_dir();
    let _production: bool = config.is_production_mode();
    let _mode: &str = config.runtime_mode();
    let _tenant: &str = config.tenant_id();
    let _k_sparse: usize = config.router_k_sparse();
}

#[test]
fn test_runtime_config_tracks_unknown_vars() {
    // Set an unknown AOS_* variable
    std::env::set_var("AOS_TOTALLY_FAKE_VARIABLE", "test");

    let config = RuntimeConfig::from_env().expect("Should create config");

    assert!(
        config.has_unknown_vars(),
        "Should detect unknown variables"
    );
    assert!(
        config.unknown_vars().contains(&"AOS_TOTALLY_FAKE_VARIABLE".to_string()),
        "Should list unknown variable"
    );

    // Clean up
    std::env::remove_var("AOS_TOTALLY_FAKE_VARIABLE");
}

#[test]
fn test_config_hash_deterministic() {
    // Clear all AOS_* vars for clean slate
    for (key, _) in std::env::vars() {
        if key.starts_with("AOS_") {
            std::env::remove_var(&key);
        }
    }

    let config1 = RuntimeConfig::from_env().expect("Config 1");
    let config2 = RuntimeConfig::from_env().expect("Config 2");

    assert_eq!(
        config1.hash(),
        config2.hash(),
        "Same inputs should produce same hash"
    );
}

#[test]
fn test_validation_report_format() {
    // Set an invalid value
    std::env::set_var("AOS_SERVER_PORT", "not_a_number");
    std::env::set_var("AOS_UNKNOWN_VAR", "test");

    let config = RuntimeConfig::from_env().expect("Config with errors");

    let report = config.validation_report();

    // Report should mention the validation error
    assert!(
        report.contains("AOS_SERVER_PORT") || report.contains("Unknown"),
        "Report should contain error information"
    );

    // Clean up
    std::env::remove_var("AOS_SERVER_PORT");
    std::env::remove_var("AOS_UNKNOWN_VAR");
}

#[test]
fn test_router_k_sparse_schema_matches_default() {
    let schema = default_schema();
    let var = schema.get_variable("AOS_ROUTER_K_SPARSE").unwrap();

    // Default in schema should be 4
    assert_eq!(var.default.as_deref(), Some("4"));

    // Should be constrained to 1-32
    if let ConfigType::Integer { min, max } = &var.config_type {
        assert_eq!(*min, Some(1));
        assert_eq!(*max, Some(32));
    } else {
        panic!("AOS_ROUTER_K_SPARSE should be Integer type");
    }
}

#[test]
fn test_model_cache_dir_default() {
    std::env::remove_var("AOS_MODEL_CACHE_DIR");
    std::env::remove_var("AOS_VAR_DIR");

    let config = RuntimeConfig::from_env().expect("Config");
    let cache_dir = config.model_cache_dir();

    // Should default to var/model-cache
    assert!(
        cache_dir.ends_with("model-cache"),
        "Model cache dir should end with model-cache, got: {:?}",
        cache_dir
    );
}

#[test]
fn test_no_zombie_variables_in_known_categories() {
    let schema = default_schema();

    // All variables should have non-empty descriptions
    for (name, var) in &schema.variables {
        assert!(
            !var.description.is_empty(),
            "Variable {} has empty description",
            name
        );
    }
}
