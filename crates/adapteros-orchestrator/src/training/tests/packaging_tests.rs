//! Packaging pipeline tests for adapter packaging and registration.
//!
//! These tests cover the packaging configuration, backend mapping,
//! and plan bytes loading functionality.

use crate::training::config::{map_preferred_backend, PostActions};
use adapteros_lora_worker::training::TrainingBackend as WorkerTrainingBackend;
use adapteros_types::training::TrainingBackendKind;

// ============================================================================
// PostActions Configuration Tests
// ============================================================================

/// Test that PostActions defaults are correct.
#[test]
fn test_post_actions_defaults() {
    let actions = PostActions::default();

    assert!(actions.package, "Default package should be true");
    assert!(actions.register, "Default register should be true");
    assert!(actions.create_stack, "Default create_stack should be true");
    assert!(
        !actions.activate_stack,
        "Default activate_stack should be false"
    );
    assert_eq!(actions.tier, "warm", "Default tier should be 'warm'");
    assert!(
        actions.adapters_root.is_none(),
        "Default adapters_root should be None"
    );
}

/// Test deserializing empty JSON gives defaults.
#[test]
fn test_post_actions_deserialize_empty_json() {
    let json = "{}";
    let actions: PostActions = serde_json::from_str(json).unwrap();

    assert!(actions.package);
    assert!(actions.register);
    assert!(actions.create_stack);
    assert!(!actions.activate_stack);
    assert_eq!(actions.tier, "warm");
}

/// Test deserializing with create_stack=false.
#[test]
fn test_post_actions_deserialize_create_stack_false() {
    let json = r#"{"create_stack": false}"#;
    let actions: PostActions = serde_json::from_str(json).unwrap();

    assert!(actions.package, "package should still default to true");
    assert!(actions.register, "register should still default to true");
    assert!(!actions.create_stack, "create_stack should be false");
    assert!(!actions.activate_stack, "activate_stack should be false");
}

/// Test deserializing with all fields specified.
#[test]
fn test_post_actions_deserialize_all_fields() {
    let json = r#"{
        "package": false,
        "register": false,
        "create_stack": false,
        "activate_stack": true,
        "tier": "ephemeral",
        "adapters_root": "/custom/path"
    }"#;
    let actions: PostActions = serde_json::from_str(json).unwrap();

    assert!(!actions.package);
    assert!(!actions.register);
    assert!(!actions.create_stack);
    assert!(actions.activate_stack);
    assert_eq!(actions.tier, "ephemeral");
    assert_eq!(actions.adapters_root, Some("/custom/path".to_string()));
}

/// Test that custom tier is preserved.
#[test]
fn test_post_actions_deserialize_custom_tier() {
    let json = r#"{"tier": "persistent"}"#;
    let actions: PostActions = serde_json::from_str(json).unwrap();

    assert_eq!(actions.tier, "persistent");
}

/// Test that create_stack does NOT imply activate_stack.
#[test]
fn test_post_actions_create_stack_does_not_imply_activate() {
    let json = r#"{"create_stack": true}"#;
    let actions: PostActions = serde_json::from_str(json).unwrap();

    assert!(actions.create_stack);
    assert!(
        !actions.activate_stack,
        "activate_stack should NOT be implied by create_stack"
    );
}

/// Test activate_stack=true works.
#[test]
fn test_post_actions_activate_stack_true() {
    let json = r#"{"activate_stack": true}"#;
    let actions: PostActions = serde_json::from_str(json).unwrap();

    assert!(actions.activate_stack);
}

/// Test package=false skips entire packaging pipeline.
#[test]
fn test_post_actions_package_false() {
    let json = r#"{"package": false}"#;
    let actions: PostActions = serde_json::from_str(json).unwrap();

    assert!(!actions.package);
    // Other flags should still have defaults
    assert!(actions.register);
    assert!(actions.create_stack);
}

/// Test register=false skips registration but still packages.
#[test]
fn test_post_actions_register_false() {
    let json = r#"{"register": false}"#;
    let actions: PostActions = serde_json::from_str(json).unwrap();

    assert!(actions.package);
    assert!(!actions.register);
}

// ============================================================================
// Backend Mapping Tests
// ============================================================================

/// Test mapping CoreML preferred backend.
#[test]
fn test_map_preferred_backend_coreml() {
    let result = map_preferred_backend(Some(TrainingBackendKind::CoreML), None);

    assert!(result.preferred.is_some());
    // CoreML should map to CoreML
    let pref = result.preferred.unwrap();
    assert_eq!(pref, WorkerTrainingBackend::CoreML);
}

/// Test mapping MLX preferred backend.
#[test]
fn test_map_preferred_backend_mlx() {
    let result = map_preferred_backend(Some(TrainingBackendKind::Mlx), None);

    assert!(result.preferred.is_some());
    let pref = result.preferred.unwrap();
    assert_eq!(pref, WorkerTrainingBackend::Mlx);
}

/// Test mapping CPU preferred backend.
#[test]
fn test_map_preferred_backend_cpu() {
    let result = map_preferred_backend(Some(TrainingBackendKind::Cpu), None);

    assert!(result.preferred.is_some());
    let pref = result.preferred.unwrap();
    assert_eq!(pref, WorkerTrainingBackend::Cpu);
}

/// Test that None preferred gives None mapping.
#[test]
fn test_map_preferred_backend_none() {
    let result = map_preferred_backend(None, None);

    assert!(
        result.preferred.is_none(),
        "No preferred backend should map to None"
    );
    assert!(
        result.coreml_fallback.is_none(),
        "No fallback should map to None"
    );
}

/// Test CoreML with MLX fallback.
#[test]
fn test_map_preferred_backend_coreml_with_mlx_fallback() {
    let result = map_preferred_backend(
        Some(TrainingBackendKind::CoreML),
        Some(TrainingBackendKind::Mlx),
    );

    assert!(result.preferred.is_some());
    assert!(result.coreml_fallback.is_some());

    let fallback = result.coreml_fallback.unwrap();
    assert_eq!(fallback, WorkerTrainingBackend::Mlx);
}

/// Test CoreML with CPU fallback.
#[test]
fn test_map_preferred_backend_coreml_with_cpu_fallback() {
    let result = map_preferred_backend(
        Some(TrainingBackendKind::CoreML),
        Some(TrainingBackendKind::Cpu),
    );

    assert!(result.preferred.is_some());
    assert!(result.coreml_fallback.is_some());

    let fallback = result.coreml_fallback.unwrap();
    assert_eq!(fallback, WorkerTrainingBackend::Cpu);
}

/// Test that fallback IS set even for non-CoreML preferred backends.
/// The defensive check at lines 106-118 sets fallback regardless of preferred.
#[test]
fn test_map_preferred_backend_mlx_still_sets_fallback() {
    let result = map_preferred_backend(
        Some(TrainingBackendKind::Mlx),
        Some(TrainingBackendKind::Cpu),
    );

    // Preferred should be MLX
    assert!(result.preferred.is_some());
    let pref = result.preferred.unwrap();
    assert_eq!(pref, WorkerTrainingBackend::Mlx);

    // But fallback is still set because of the defensive check
    // (fallback is set even when preferred isn't CoreML)
    assert!(result.coreml_fallback.is_some());
}

/// Test Auto backend maps to None preferred (auto-select).
#[test]
fn test_map_preferred_backend_auto() {
    let result = map_preferred_backend(Some(TrainingBackendKind::Auto), None);

    // Auto is not a concrete backend, so preferred should be None
    assert!(
        result.preferred.is_none(),
        "Auto should map to None (auto-select)"
    );
}

// ============================================================================
// Integration-style Tests for Config Flow
// ============================================================================

/// Test parsing post_actions from typical training job JSON.
#[test]
fn test_post_actions_from_training_job_json() {
    // Simulate what comes from the API
    let json = r#"{
        "package": true,
        "register": true,
        "create_stack": true,
        "activate_stack": false,
        "tier": "warm"
    }"#;

    let actions: PostActions = serde_json::from_str(json).unwrap();
    assert!(actions.package);
    assert!(actions.register);
    assert!(actions.create_stack);
    assert!(!actions.activate_stack);
    assert_eq!(actions.tier, "warm");
}

/// Test parsing post_actions with only tier override.
#[test]
fn test_post_actions_tier_only_override() {
    let json = r#"{"tier": "ephemeral"}"#;

    let actions: PostActions = serde_json::from_str(json).unwrap();

    // All booleans should be defaults
    assert!(actions.package);
    assert!(actions.register);
    assert!(actions.create_stack);
    assert!(!actions.activate_stack);
    // Only tier should be different
    assert_eq!(actions.tier, "ephemeral");
}

/// Test that invalid JSON fails gracefully.
#[test]
fn test_post_actions_invalid_json() {
    let invalid_json = r#"{"package": "not a boolean"}"#;

    let result: Result<PostActions, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err(), "Invalid JSON should fail to parse");
}

/// Test empty string is valid JSON object.
#[test]
fn test_post_actions_empty_string_invalid() {
    let result: Result<PostActions, _> = serde_json::from_str("");
    assert!(result.is_err(), "Empty string is not valid JSON");
}

// ============================================================================
// Versioned Artifact Path Tests
// ============================================================================

/// Test that versioning metadata flows through correctly.
#[test]
fn test_versioning_snapshot_fields() {
    use crate::training::versioning::VersioningSnapshot;

    let snapshot = VersioningSnapshot {
        adapter_version_id: Some("version-123".to_string()),
        version_label: Some("v1.0.0".to_string()),
        target_branch: Some("main".to_string()),
        repo_name: Some("my-adapter-repo".to_string()),
        repo_id: Some("repo-456".to_string()),
        base_version_id: Some("base-789".to_string()),
        code_commit_sha: Some("abc123".to_string()),
        data_spec_hash: Some("hash456".to_string()),
        dataset_version_ids: None,
    };

    // Verify fields are accessible
    assert_eq!(snapshot.adapter_version_id, Some("version-123".to_string()));
    assert_eq!(snapshot.version_label, Some("v1.0.0".to_string()));
    assert_eq!(snapshot.repo_name, Some("my-adapter-repo".to_string()));
}
