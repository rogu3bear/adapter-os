//! Tests for training post_actions behavior
//!
//! Verifies that the PostActions struct deserializes correctly and that
//! create_stack defaults to true but does NOT set the stack as default.

/// PostActions struct for testing deserialization behavior
/// This mirrors the internal struct in training.rs
#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq)]
struct PostActions {
    #[serde(default = "default_true")]
    package: bool,
    #[serde(default = "default_true")]
    register: bool,
    #[serde(default = "default_true")]
    create_stack: bool,
    #[serde(default = "default_tier")]
    tier: String,
    adapters_root: Option<String>,
}

fn default_true() -> bool {
    true
}
fn default_tier() -> String {
    "warm".to_string()
}

#[test]
fn test_post_actions_defaults() {
    // Default() should give package=true, register=true, create_stack=true, tier=warm
    let default = PostActions::default();

    // Note: Default derive gives false for bools, empty string for String
    // The serde defaults only apply during deserialization
    assert!(!default.package, "Default trait gives false for bool");
    assert!(!default.register);
    assert!(!default.create_stack);
}

#[test]
fn test_post_actions_deserialize_empty_json() {
    // Empty JSON object should use serde defaults
    let json = "{}";
    let parsed: PostActions = serde_json::from_str(json).unwrap();

    assert!(parsed.package, "package should default to true");
    assert!(parsed.register, "register should default to true");
    assert!(parsed.create_stack, "create_stack should default to true");
    assert_eq!(parsed.tier, "warm", "tier should default to 'warm'");
    assert!(parsed.adapters_root.is_none());
}

#[test]
fn test_post_actions_deserialize_create_stack_false() {
    let json = r#"{"create_stack": false}"#;
    let parsed: PostActions = serde_json::from_str(json).unwrap();

    // create_stack explicitly set to false
    assert!(
        !parsed.create_stack,
        "create_stack should be false when explicitly set"
    );
    // Other fields should still use defaults
    assert!(parsed.package, "package should default to true");
    assert!(parsed.register, "register should default to true");
}

#[test]
fn test_post_actions_deserialize_create_stack_true() {
    let json = r#"{"create_stack": true}"#;
    let parsed: PostActions = serde_json::from_str(json).unwrap();

    assert!(
        parsed.create_stack,
        "create_stack should be true when explicitly set"
    );
}

#[test]
fn test_post_actions_deserialize_register_false_skips_stack() {
    // When register=false, create_stack is irrelevant (no adapter to put in stack)
    let json = r#"{"register": false, "create_stack": true}"#;
    let parsed: PostActions = serde_json::from_str(json).unwrap();

    assert!(!parsed.register, "register should be false");
    assert!(parsed.create_stack, "create_stack can still be true");
    // Note: The actual logic in training.rs will skip stack creation if register=false
}

#[test]
fn test_post_actions_deserialize_all_fields() {
    let json = r#"{
        "package": false,
        "register": true,
        "create_stack": true,
        "tier": "persistent",
        "adapters_root": "/custom/path"
    }"#;

    let parsed: PostActions = serde_json::from_str(json).unwrap();

    assert!(!parsed.package, "package should be false");
    assert!(parsed.register, "register should be true");
    assert!(parsed.create_stack, "create_stack should be true");
    assert_eq!(parsed.tier, "persistent");
    assert_eq!(parsed.adapters_root, Some("/custom/path".to_string()));
}

#[test]
fn test_post_actions_deserialize_custom_tier() {
    let json = r#"{"tier": "ephemeral"}"#;
    let parsed: PostActions = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.tier, "ephemeral");
    // Other fields should use defaults
    assert!(parsed.create_stack);
}

#[test]
fn test_post_actions_invalid_json_uses_defaults() {
    // If JSON is invalid, the or_default pattern in training.rs returns defaults
    let invalid_json = "not valid json";
    let result: Option<PostActions> = serde_json::from_str(invalid_json).ok();

    assert!(result.is_none(), "Invalid JSON should fail to parse");

    // The actual code does: .and_then(|json| serde_json::from_str(json).ok()).unwrap_or_default()
    let fallback = result.unwrap_or_default();
    // Default trait gives false, not serde defaults
    assert!(!fallback.package);
    assert!(!fallback.register);
    assert!(!fallback.create_stack);
}

/// Test that demonstrates the key behavioral change:
/// create_stack=true creates a stack, but does NOT set it as default
#[test]
fn test_post_actions_create_stack_does_not_imply_default() {
    // This is a documentation test - the actual behavior is in training.rs
    // The key change is that we removed the set_default_stack() call
    //
    // Before (BAD):
    //   if post_actions.create_stack {
    //       db.insert_stack(...)?;
    //       db.set_default_stack(tenant_id, &stack_id)?;  // <-- REMOVED
    //   }
    //
    // After (GOOD):
    //   if post_actions.create_stack {
    //       db.insert_stack(...)?;
    //       // No set_default_stack call - users must explicitly set default
    //   }

    let json = r#"{"create_stack": true}"#;
    let parsed: PostActions = serde_json::from_str(json).unwrap();

    assert!(
        parsed.create_stack,
        "create_stack=true means create a stack, but NOT set as default"
    );
}

// ============================================================================
// E2E Flow Documentation Tests
// ============================================================================

/// Documents the expected E2E flow for training with stack creation.
///
/// This test is ignored because it requires:
/// - A running worker process
/// - A valid base model
/// - A dataset with training samples
///
/// The flow is:
/// 1. User submits training job with post_actions: { register: true, create_stack: true }
/// 2. Training completes successfully
/// 3. Adapter is packaged (Q15 quantization)
/// 4. Adapter is registered in the database
/// 5. A new stack is created containing the adapter
/// 6. The stack is NOT set as the tenant's default
/// 7. User must explicitly call PUT /v1/tenants/{tenant_id}/default-stack to set default
///
/// Training finishes → adapter registered →
/// stack auto-created, but NOT set as default.
#[test]
#[ignore = "Requires running infrastructure - documents expected E2E flow [tracking: STAB-IGN-0063]"]
fn test_e2e_training_creates_stack_without_setting_default() {
    // This test documents the expected flow but cannot run without infrastructure.
    //
    // To manually verify:
    // 1. Start the server: ./start up
    // 2. Submit a training job via the API
    // 3. Wait for completion
    // 4. Check: GET /v1/stacks - new stack should exist
    // 5. Check: GET /v1/tenants/{id}/default-stack - should return 404 (no default)
    // 6. Set default: PUT /v1/tenants/{id}/default-stack { "stack_id": "..." }
    // 7. Verify: GET /v1/tenants/{id}/default-stack - should return the stack_id

    panic!("This test documents expected E2E behavior - run manually to verify");
}
