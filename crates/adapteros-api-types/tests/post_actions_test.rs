//! Tests for PostActionsRequest serialization and default values
//!
//! Verifies that the create_stack field defaults correctly and
//! serializes/deserializes properly.

use adapteros_api_types::training::PostActionsRequest;

#[test]
fn test_post_actions_default_has_no_fields_set() {
    let default = PostActionsRequest::default();

    // All fields should be None by default (use ?? defaults at runtime)
    assert!(default.package.is_none());
    assert!(default.register.is_none());
    assert!(default.create_stack.is_none());
    assert!(default.activate_stack.is_none());
    assert!(default.auto_promote.is_none());
    assert!(default.tier.is_none());
    assert!(default.adapters_root.is_none());
}

#[test]
fn test_post_actions_deserialize_with_create_stack_true() {
    let json = r#"{"create_stack": true}"#;
    let parsed: PostActionsRequest = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.create_stack, Some(true));
}

#[test]
fn test_post_actions_deserialize_with_create_stack_false() {
    let json = r#"{"create_stack": false}"#;
    let parsed: PostActionsRequest = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.create_stack, Some(false));
}

#[test]
fn test_post_actions_deserialize_without_create_stack() {
    let json = r#"{"package": true, "register": true}"#;
    let parsed: PostActionsRequest = serde_json::from_str(json).unwrap();

    // create_stack should be None when not provided
    assert!(parsed.create_stack.is_none());
    assert_eq!(parsed.package, Some(true));
    assert_eq!(parsed.register, Some(true));
}

#[test]
fn test_post_actions_serialize_roundtrip() {
    let original = PostActionsRequest {
        package: Some(true),
        register: Some(true),
        create_stack: Some(true),
        activate_stack: Some(false),
        auto_promote: Some(true),
        tier: Some("warm".to_string()),
        adapters_root: Some("./adapters".to_string()),
    };

    let json = serde_json::to_string(&original).unwrap();
    let parsed: PostActionsRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.package, original.package);
    assert_eq!(parsed.register, original.register);
    assert_eq!(parsed.create_stack, original.create_stack);
    assert_eq!(parsed.activate_stack, original.activate_stack);
    assert_eq!(parsed.auto_promote, original.auto_promote);
    assert_eq!(parsed.tier, original.tier);
    assert_eq!(parsed.adapters_root, original.adapters_root);
}

#[test]
fn test_post_actions_all_fields() {
    let json = r#"{
        "package": false,
        "register": true,
        "create_stack": false,
        "activate_stack": true,
        "auto_promote": true,
        "tier": "persistent",
        "adapters_root": "/custom/path"
    }"#;

    let parsed: PostActionsRequest = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.package, Some(false));
    assert_eq!(parsed.register, Some(true));
    assert_eq!(parsed.create_stack, Some(false));
    assert_eq!(parsed.activate_stack, Some(true));
    assert_eq!(parsed.auto_promote, Some(true));
    assert_eq!(parsed.tier, Some("persistent".to_string()));
    assert_eq!(parsed.adapters_root, Some("/custom/path".to_string()));
}

#[test]
fn test_post_actions_auto_promote_defaults_to_none() {
    let json = r#"{"package": true}"#;
    let parsed: PostActionsRequest = serde_json::from_str(json).unwrap();
    assert!(parsed.auto_promote.is_none());
    assert!(parsed.activate_stack.is_none());
}

#[test]
fn test_post_actions_auto_promote_true() {
    let json = r#"{"auto_promote": true, "activate_stack": true}"#;
    let parsed: PostActionsRequest = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.auto_promote, Some(true));
    assert_eq!(parsed.activate_stack, Some(true));
}
