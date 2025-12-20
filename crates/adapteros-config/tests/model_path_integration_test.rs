//! Integration tests for model path resolution consistency
//!
//! These tests ensure that model path configuration works correctly across
//! all components (server, CLI, preflight) and that env var precedence is respected.
//!
//! Test coverage:
//! 1. Default resolution uses ./var/model-cache/models/qwen2.5-7b-mlx
//! 2. AOS_MODEL_CACHE_DIR + AOS_BASE_MODEL_ID takes precedence
//! 3. Explicit overrides take highest precedence
//! 4. Combinations work correctly
//!
//! Note: These tests run sequentially to avoid environment variable conflicts.

use adapteros_config::{
    resolve_base_model_location, DEFAULT_BASE_MODEL_ID, DEFAULT_MODEL_CACHE_ROOT,
};
use std::path::PathBuf;
use tempfile::TempDir;

fn new_test_tempdir() -> TempDir {
    let root = PathBuf::from("var").join("tmp");
    std::fs::create_dir_all(&root).expect("create var/tmp");
    TempDir::new_in(&root).expect("tempdir")
}

/// Guard that ensures env vars are cleaned up even on panic
struct EnvGuard {
    vars: Vec<String>,
}

impl EnvGuard {
    fn new() -> Self {
        let guard = Self {
            vars: vec![
                "AOS_MODEL_CACHE_DIR".to_string(),
                "AOS_BASE_MODEL_ID".to_string(),
                "AOS_MODEL_PATH".to_string(),
            ],
        };
        // Clear on creation
        for var in &guard.vars {
            std::env::remove_var(var);
        }
        guard
    }

    fn set(&self, cache_dir: Option<&str>, model_id: Option<&str>) {
        if let Some(dir) = cache_dir {
            std::env::set_var("AOS_MODEL_CACHE_DIR", dir);
        }
        if let Some(id) = model_id {
            std::env::set_var("AOS_BASE_MODEL_ID", id);
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for var in &self.vars {
            std::env::remove_var(var);
        }
    }
}

#[test]
fn test_default_model_path_resolution() {
    let _guard = EnvGuard::new();

    let result = resolve_base_model_location(None, None, false).unwrap();

    assert_eq!(
        result.cache_root,
        PathBuf::from(DEFAULT_MODEL_CACHE_ROOT),
        "Default cache root should be {}",
        DEFAULT_MODEL_CACHE_ROOT
    );
    assert_eq!(
        result.id, DEFAULT_BASE_MODEL_ID,
        "Default model ID should be {}",
        DEFAULT_BASE_MODEL_ID
    );
    assert_eq!(
        result.full_path,
        PathBuf::from(DEFAULT_MODEL_CACHE_ROOT).join(DEFAULT_BASE_MODEL_ID),
        "Full path should be cache_root/id"
    );
}

#[test]
fn test_model_cache_dir_env_precedence() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let cache_dir = tmp.path().join("custom-cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    _guard.set(Some(cache_dir.to_str().unwrap()), Some("test-model"));

    let result = resolve_base_model_location(None, None, false).unwrap();

    assert_eq!(
        result.cache_root, cache_dir,
        "Cache root should come from AOS_MODEL_CACHE_DIR env var"
    );
    assert_eq!(
        result.id, "test-model",
        "Model ID should come from AOS_BASE_MODEL_ID env var"
    );
    assert_eq!(
        result.full_path,
        cache_dir.join("test-model"),
        "Full path should be cache_dir/model_id"
    );
}

#[test]
fn test_explicit_override_takes_highest_precedence() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let env_cache_dir = tmp.path().join("env-cache");
    let override_cache_dir = tmp.path().join("override-cache");

    std::fs::create_dir_all(&env_cache_dir).unwrap();
    std::fs::create_dir_all(&override_cache_dir).unwrap();

    _guard.set(Some(env_cache_dir.to_str().unwrap()), Some("env-model"));

    let result =
        resolve_base_model_location(Some("override-model"), Some(&override_cache_dir), false)
            .unwrap();

    assert_eq!(
        result.id, "override-model",
        "Explicit ID override should take precedence over env var"
    );
    assert_eq!(
        result.cache_root, override_cache_dir,
        "Explicit cache_root override should take precedence over env var"
    );
    assert_eq!(
        result.full_path,
        override_cache_dir.join("override-model"),
        "Full path should use override values"
    );
}

#[test]
fn test_partial_override_id_only() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let env_cache_dir = tmp.path().join("env-cache");
    std::fs::create_dir_all(&env_cache_dir).unwrap();

    _guard.set(Some(env_cache_dir.to_str().unwrap()), Some("env-model"));

    let result = resolve_base_model_location(Some("override-id"), None, false).unwrap();

    assert_eq!(
        result.id, "override-id",
        "Explicit ID override should take precedence"
    );
    assert_eq!(
        result.cache_root, env_cache_dir,
        "Cache root should still come from env var"
    );
    assert_eq!(
        result.full_path,
        env_cache_dir.join("override-id"),
        "Full path should combine env cache_root with override ID"
    );
}

#[test]
fn test_partial_override_cache_root_only() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let override_cache_dir = tmp.path().join("override-cache");
    std::fs::create_dir_all(&override_cache_dir).unwrap();

    _guard.set(None, Some("env-model-id"));

    let result = resolve_base_model_location(None, Some(&override_cache_dir), false).unwrap();

    assert_eq!(
        result.id, "env-model-id",
        "Model ID should still come from env var"
    );
    assert_eq!(
        result.cache_root, override_cache_dir,
        "Explicit cache_root override should take precedence"
    );
    assert_eq!(
        result.full_path,
        override_cache_dir.join("env-model-id"),
        "Full path should combine override cache_root with env ID"
    );
}

#[test]
fn test_env_var_only_cache_dir() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let cache_dir = tmp.path().join("custom-cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    _guard.set(Some(cache_dir.to_str().unwrap()), None);

    let result = resolve_base_model_location(None, None, false).unwrap();

    assert_eq!(
        result.cache_root, cache_dir,
        "Cache root should come from env var"
    );
    assert_eq!(
        result.id, DEFAULT_BASE_MODEL_ID,
        "Model ID should use default when not specified in env"
    );
    assert_eq!(
        result.full_path,
        cache_dir.join(DEFAULT_BASE_MODEL_ID),
        "Full path should combine env cache_root with default ID"
    );
}

#[test]
fn test_env_var_only_model_id() {
    let _guard = EnvGuard::new();

    _guard.set(None, Some("custom-model-id"));

    let result = resolve_base_model_location(None, None, false).unwrap();

    assert_eq!(
        result.cache_root,
        PathBuf::from(DEFAULT_MODEL_CACHE_ROOT),
        "Cache root should use default when not specified in env"
    );
    assert_eq!(
        result.id, "custom-model-id",
        "Model ID should come from env var"
    );
    assert_eq!(
        result.full_path,
        PathBuf::from(DEFAULT_MODEL_CACHE_ROOT).join("custom-model-id"),
        "Full path should combine default cache_root with env ID"
    );
}

#[test]
fn test_require_existing_fails_when_path_missing() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let nonexistent_cache = tmp.path().join("nonexistent");

    _guard.set(
        Some(nonexistent_cache.to_str().unwrap()),
        Some("test-model"),
    );

    let result = resolve_base_model_location(None, None, true);

    assert!(
        result.is_err(),
        "Should fail when require_existing=true and path doesn't exist"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Model path does not exist"),
        "Error should mention path doesn't exist. Got: {}",
        err_msg
    );
    assert!(
        err_msg.contains("AOS_MODEL_CACHE_DIR") || err_msg.contains("base_model.cache_root"),
        "Error should mention configuration options. Got: {}",
        err_msg
    );
}

#[test]
fn test_require_existing_succeeds_when_path_exists() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let cache_dir = tmp.path().join("custom-cache");
    let model_dir = cache_dir.join("test-model");

    std::fs::create_dir_all(&model_dir).unwrap();

    _guard.set(Some(cache_dir.to_str().unwrap()), Some("test-model"));

    let result = resolve_base_model_location(None, None, true);

    assert!(
        result.is_ok(),
        "Should succeed when require_existing=true and path exists"
    );

    let resolved = result.unwrap();
    assert_eq!(resolved.full_path, model_dir);
}

#[test]
fn test_require_existing_false_allows_nonexistent_path() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let nonexistent_cache = tmp.path().join("nonexistent");

    _guard.set(
        Some(nonexistent_cache.to_str().unwrap()),
        Some("test-model"),
    );

    let result = resolve_base_model_location(None, None, false);

    assert!(
        result.is_ok(),
        "Should succeed when require_existing=false even if path doesn't exist"
    );

    let resolved = result.unwrap();
    assert_eq!(
        resolved.full_path,
        nonexistent_cache.join("test-model"),
        "Should return path even if it doesn't exist"
    );
}

#[test]
fn test_precedence_order_all_sources() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let env_cache_dir = tmp.path().join("env-cache");
    let override_cache_dir = tmp.path().join("override-cache");

    std::fs::create_dir_all(&env_cache_dir).unwrap();
    std::fs::create_dir_all(&override_cache_dir).unwrap();

    _guard.set(Some(env_cache_dir.to_str().unwrap()), Some("env-model"));

    // Explicit overrides should win
    let result =
        resolve_base_model_location(Some("override-model"), Some(&override_cache_dir), false)
            .unwrap();
    assert_eq!(result.id, "override-model");
    assert_eq!(result.cache_root, override_cache_dir);

    // Without overrides - env vars should be used
    let result2 = resolve_base_model_location(None, None, false).unwrap();
    assert_eq!(result2.id, "env-model");
    assert_eq!(result2.cache_root, env_cache_dir);

    drop(_guard);

    // Now test without any configuration - defaults should be used
    let _guard2 = EnvGuard::new();
    let result3 = resolve_base_model_location(None, None, false).unwrap();
    assert_eq!(result3.id, DEFAULT_BASE_MODEL_ID);
    assert_eq!(result3.cache_root, PathBuf::from(DEFAULT_MODEL_CACHE_ROOT));
}

#[test]
fn test_path_consistency_across_calls() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    _guard.set(Some(cache_dir.to_str().unwrap()), Some("consistent-model"));

    let result1 = resolve_base_model_location(None, None, false).unwrap();
    let result2 = resolve_base_model_location(None, None, false).unwrap();
    let result3 = resolve_base_model_location(None, None, false).unwrap();

    assert_eq!(
        result1.full_path, result2.full_path,
        "Multiple calls should return consistent paths"
    );
    assert_eq!(
        result2.full_path, result3.full_path,
        "Multiple calls should return consistent paths"
    );
    assert_eq!(
        result1.id, result2.id,
        "Multiple calls should return consistent IDs"
    );
    assert_eq!(
        result1.cache_root, result2.cache_root,
        "Multiple calls should return consistent cache roots"
    );
}

#[test]
fn test_full_path_construction() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let cache_dir = tmp.path().join("models");
    let model_id = "qwen2.5-7b-mlx";

    std::fs::create_dir_all(&cache_dir).unwrap();
    _guard.set(Some(cache_dir.to_str().unwrap()), Some(model_id));

    let result = resolve_base_model_location(None, None, false).unwrap();

    assert_eq!(
        result.full_path,
        result.cache_root.join(&result.id),
        "full_path should equal cache_root/id"
    );
    assert_eq!(
        result.full_path,
        cache_dir.join(model_id),
        "full_path should be constructed correctly"
    );
}

#[test]
fn test_special_characters_in_model_id() {
    let _guard = EnvGuard::new();
    let tmp = new_test_tempdir();
    let cache_dir = tmp.path().join("cache");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let test_ids = vec![
        "model-with-dashes",
        "model.with.dots",
        "model_with_underscores",
        "model-v1.5",
        "Qwen2.5-7B-Instruct-4bit",
    ];

    for model_id in test_ids {
        _guard.set(Some(cache_dir.to_str().unwrap()), Some(model_id));

        let result = resolve_base_model_location(None, None, false).unwrap();

        assert_eq!(
            result.id, model_id,
            "Model ID should be preserved exactly: {}",
            model_id
        );
        assert_eq!(
            result.full_path,
            cache_dir.join(model_id),
            "Path should handle special characters in model ID: {}",
            model_id
        );
    }
}

#[test]
fn test_relative_and_absolute_cache_paths() {
    let _guard = EnvGuard::new();

    // Test relative path
    _guard.set(Some("./var/custom-cache"), Some("test-model"));
    let result1 = resolve_base_model_location(None, None, false).unwrap();
    assert_eq!(result1.cache_root, PathBuf::from("./var/custom-cache"));

    drop(_guard);

    // Test absolute path
    let _guard2 = EnvGuard::new();
    let tmp = new_test_tempdir();
    let abs_cache = tmp.path().join("abs-cache");
    std::fs::create_dir_all(&abs_cache).unwrap();
    _guard2.set(Some(abs_cache.to_str().unwrap()), Some("test-model"));
    let result2 = resolve_base_model_location(None, None, false).unwrap();
    assert_eq!(result2.cache_root, abs_cache);
    assert!(
        result2.cache_root.is_absolute(),
        "Absolute path should be preserved"
    );
}

#[test]
fn test_default_constants_match_schema() {
    use adapteros_config::schema::default_schema;

    let schema = default_schema();

    let base_model_var = schema
        .get_variable("AOS_BASE_MODEL_ID")
        .expect("AOS_BASE_MODEL_ID should exist in schema");
    assert_eq!(
        base_model_var.default.as_deref(),
        Some(DEFAULT_BASE_MODEL_ID),
        "Schema default for AOS_BASE_MODEL_ID should match constant"
    );

    let cache_dir_var = schema
        .get_variable("AOS_MODEL_CACHE_DIR")
        .expect("AOS_MODEL_CACHE_DIR should exist in schema");
    assert_eq!(
        cache_dir_var.default.as_deref(),
        Some(DEFAULT_MODEL_CACHE_ROOT),
        "Schema default for AOS_MODEL_CACHE_DIR should match constant"
    );
}
