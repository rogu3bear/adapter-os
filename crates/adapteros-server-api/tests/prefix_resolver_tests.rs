//! Integration tests for PrefixResolver
//!
//! Tests the integration between PrefixResolver and the database layer,
//! including template resolution, tokenization, and fallback behavior.

use adapteros_api_types::prefix_templates::{
    CreatePrefixTemplateRequest, PrefixMode, UpdatePrefixTemplateRequest,
};
use adapteros_core::B3Hash;
use adapteros_server_api::prefix_resolver::{PrefixResolver, ResolvedPrefixBuilder};
use std::sync::Arc;

mod common;
use common::setup_state;

/// Mock tokenizer that converts text to fixed token IDs based on length
fn mock_tokenizer(text: &str) -> adapteros_core::Result<Vec<u32>> {
    // Simple mock: generate token IDs based on text length and characters
    Ok(text
        .chars()
        .enumerate()
        .map(|(i, c)| (i as u32 + 1) * (c as u32 % 100))
        .collect())
}

/// Mock tokenizer that always returns empty tokens (for edge case testing)
fn mock_empty_tokenizer(_text: &str) -> adapteros_core::Result<Vec<u32>> {
    Ok(vec![])
}

/// Mock tokenizer that fails (for error testing)
fn mock_failing_tokenizer(_text: &str) -> adapteros_core::Result<Vec<u32>> {
    Err(adapteros_core::AosError::Internal(
        "Tokenization failed".to_string(),
    ))
}

#[tokio::test]
async fn test_resolve_prefix_no_template() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Try to resolve prefix for tenant with no templates
    let result = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error");

    assert!(
        result.is_none(),
        "Should return None when no template exists"
    );
}

#[tokio::test]
async fn test_resolve_prefix_with_template() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create a template
    let template_text = "You are a helpful AI assistant.";
    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: template_text.to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .expect("create_prefix_template");

    // Resolve the prefix
    let resolved = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve template");

    // Verify the resolved prefix
    assert_eq!(resolved.template.id, created.id);
    assert_eq!(resolved.template.tenant_id, "tenant-1");
    assert_eq!(resolved.template.mode, PrefixMode::User);
    assert_eq!(resolved.template.template_text, template_text);
    assert!(!resolved.token_ids.is_empty(), "Should have token IDs");

    // Verify tokenized hash is deterministic
    let expected_tokens = mock_tokenizer(template_text).unwrap();
    assert_eq!(
        resolved.token_ids, expected_tokens,
        "Token IDs should match mock tokenizer output"
    );
}

#[tokio::test]
async fn test_resolve_prefix_mode_fallback_to_system() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create only a System mode template
    let system_text = "System-level prefix for all modes.";
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::System,
        template_text: system_text.to_string(),
        priority: Some(5),
        enabled: Some(true),
    })
    .await
    .expect("create system template");

    // Try to resolve User mode - should fall back to System
    let resolved = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should fall back to system template");

    assert_eq!(resolved.template.mode, PrefixMode::System);
    assert_eq!(resolved.template.template_text, system_text);
}

#[tokio::test]
async fn test_resolve_prefix_no_fallback_for_system_mode() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create only a User mode template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "User mode only".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create user template");

    // Try to resolve System mode - should NOT fall back to User
    let result = resolver
        .resolve_prefix("tenant-1", &PrefixMode::System, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error");

    assert!(
        result.is_none(),
        "System mode should not fall back to other modes"
    );
}

#[tokio::test]
async fn test_resolve_prefix_priority_selection() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create multiple templates for the same mode with different priorities
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Low priority template".to_string(),
        priority: Some(1),
        enabled: Some(true),
    })
    .await
    .expect("create low priority template");

    let high_priority_text = "High priority template";
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: high_priority_text.to_string(),
        priority: Some(100),
        enabled: Some(true),
    })
    .await
    .expect("create high priority template");

    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Medium priority template".to_string(),
        priority: Some(50),
        enabled: Some(true),
    })
    .await
    .expect("create medium priority template");

    // Resolve - should get the highest priority template
    let resolved = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve highest priority template");

    assert_eq!(
        resolved.template.template_text, high_priority_text,
        "Should select highest priority template"
    );
    assert_eq!(resolved.template.priority, 100);
}

#[tokio::test]
async fn test_resolve_prefix_disabled_template_skipped() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create a disabled high-priority template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Disabled template".to_string(),
        priority: Some(100),
        enabled: Some(false),
    })
    .await
    .expect("create disabled template");

    // Create an enabled low-priority template
    let enabled_text = "Enabled template";
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: enabled_text.to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create enabled template");

    // Should resolve to the enabled template, skipping disabled one
    let resolved = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve enabled template");

    assert_eq!(
        resolved.template.template_text, enabled_text,
        "Should skip disabled templates"
    );
}

#[tokio::test]
async fn test_resolve_prefix_empty_tokenization() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create a template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Test template".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create template");

    // Resolve with a tokenizer that returns empty tokens
    let result = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_empty_tokenizer)
        .await
        .expect("resolve_prefix should not error");

    assert!(
        result.is_none(),
        "Should return None when tokenization produces empty sequence"
    );
}

#[tokio::test]
async fn test_resolve_prefix_tokenization_error() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create a template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Test template".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create template");

    // Resolve with a failing tokenizer
    let result = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_failing_tokenizer)
        .await;

    assert!(result.is_err(), "Should propagate tokenization error");
}

#[tokio::test]
async fn test_resolve_prefix_custom_mode() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create a custom mode template
    let custom_mode = PrefixMode::Custom("customer_support".to_string());
    let custom_text = "You are a customer support agent.";
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: custom_mode.clone(),
        template_text: custom_text.to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create custom mode template");

    // Resolve custom mode
    let resolved = resolver
        .resolve_prefix("tenant-1", &custom_mode, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve custom mode template");

    assert_eq!(resolved.template.mode, custom_mode);
    assert_eq!(resolved.template.template_text, custom_text);
}

#[tokio::test]
async fn test_resolve_prefix_tenant_isolation() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create template for tenant-1
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Tenant 1 template".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create tenant-1 template");

    // Try to resolve for different tenant
    let result = resolver
        .resolve_prefix("different-tenant", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error");

    assert!(
        result.is_none(),
        "Should not resolve templates from different tenant"
    );
}

#[tokio::test]
async fn test_resolve_prefix_deterministic_hash() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create a template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Test template".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create template");

    // Resolve multiple times
    let resolved1 = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve template");

    let resolved2 = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve template");

    assert_eq!(
        resolved1.tokenized_hash, resolved2.tokenized_hash,
        "Tokenized hash should be deterministic"
    );
}

#[tokio::test]
async fn test_resolve_template_without_tokenization() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create a template
    let template_text = "Test template";
    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: template_text.to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .expect("create template");

    // Resolve without tokenization
    let resolved = resolver
        .resolve_template("tenant-1", &PrefixMode::User)
        .await
        .expect("resolve_template should not error")
        .expect("Should resolve template");

    assert_eq!(resolved.id, created.id);
    assert_eq!(resolved.template_text, template_text);
}

#[tokio::test]
async fn test_has_prefix_templates() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Initially no templates
    let has_templates = resolver
        .has_prefix_templates("tenant-1")
        .await
        .expect("has_prefix_templates should not error");
    assert!(!has_templates, "Should have no templates initially");

    // Create a template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Test template".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create template");

    // Now has templates
    let has_templates = resolver
        .has_prefix_templates("tenant-1")
        .await
        .expect("has_prefix_templates should not error");
    assert!(has_templates, "Should have templates after creation");

    // Different tenant still has none
    let has_templates = resolver
        .has_prefix_templates("different-tenant")
        .await
        .expect("has_prefix_templates should not error");
    assert!(!has_templates, "Different tenant should have no templates");
}

#[tokio::test]
async fn test_list_templates() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Initially empty
    let templates = resolver
        .list_templates("tenant-1")
        .await
        .expect("list_templates should not error");
    assert_eq!(templates.len(), 0);

    // Create multiple templates
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "User template".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .expect("create user template");

    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::Builder,
        template_text: "Builder template".to_string(),
        priority: Some(20),
        enabled: Some(true),
    })
    .await
    .expect("create builder template");

    // List templates
    let templates = resolver
        .list_templates("tenant-1")
        .await
        .expect("list_templates should not error");
    assert_eq!(templates.len(), 2);

    // Verify ordering by priority (descending)
    assert_eq!(templates[0].priority, 20);
    assert_eq!(templates[1].priority, 10);
}

#[tokio::test]
async fn test_resolved_prefix_builder() {
    // Test the builder utility
    let resolved = ResolvedPrefixBuilder::new()
        .template_id("tpl-123")
        .tenant_id("tenant-1")
        .mode(PrefixMode::User)
        .template_text("You are helpful.")
        .token_ids(vec![100, 200, 300])
        .build();

    assert_eq!(resolved.template.id, "tpl-123");
    assert_eq!(resolved.template.tenant_id, "tenant-1");
    assert_eq!(resolved.template.mode, PrefixMode::User);
    assert_eq!(resolved.template.template_text, "You are helpful.");
    assert_eq!(resolved.token_ids, vec![100, 200, 300]);
    assert!(!resolved.tokenized_hash.to_hex().is_empty());

    // Verify template hash is computed correctly
    let expected_hash = B3Hash::hash(b"You are helpful.");
    assert_eq!(resolved.template.template_hash_b3, expected_hash);
}

#[tokio::test]
async fn test_resolved_prefix_builder_defaults() {
    // Test builder with minimal fields (using defaults)
    let resolved = ResolvedPrefixBuilder::new()
        .template_text("Test")
        .token_ids(vec![1, 2, 3])
        .build();

    assert_eq!(resolved.template.id, "test-template");
    assert_eq!(resolved.template.tenant_id, "test-tenant");
    assert_eq!(resolved.template.mode, PrefixMode::System);
    assert_eq!(resolved.template.priority, 0);
    assert!(resolved.template.enabled);
}

#[tokio::test]
async fn test_resolve_prefix_mode_precedence() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create system template with low priority
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::System,
        template_text: "System fallback".to_string(),
        priority: Some(100),
        enabled: Some(true),
    })
    .await
    .expect("create system template");

    // Create user template with even lower priority
    let user_text = "User specific";
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: user_text.to_string(),
        priority: Some(1),
        enabled: Some(true),
    })
    .await
    .expect("create user template");

    // Resolve User mode - should prefer exact mode match over higher-priority fallback
    let resolved = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve user template");

    assert_eq!(
        resolved.template.mode,
        PrefixMode::User,
        "Should prefer exact mode match"
    );
    assert_eq!(resolved.template.template_text, user_text);
}

#[tokio::test]
async fn test_resolve_prefix_after_update() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create initial template
    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "Original text".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .expect("create template");

    // Resolve initial
    let resolved1 = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve template");
    assert_eq!(resolved1.template.template_text, "Original text");
    let hash1 = resolved1.tokenized_hash;

    // Update template
    let updated_text = "Updated text";
    db.update_prefix_template(
        &created.id,
        UpdatePrefixTemplateRequest {
            mode: None,
            template_text: Some(updated_text.to_string()),
            priority: None,
            enabled: None,
        },
    )
    .await
    .expect("update template");

    // Resolve again - should get updated template
    let resolved2 = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error")
        .expect("Should resolve updated template");

    assert_eq!(resolved2.template.template_text, updated_text);
    assert_ne!(
        resolved2.tokenized_hash, hash1,
        "Hash should change with updated text"
    );
}

#[tokio::test]
async fn test_resolve_prefix_after_delete() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create template
    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "To be deleted".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .expect("create template");

    // Verify it resolves
    let resolved = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error");
    assert!(resolved.is_some());

    // Delete template
    db.delete_prefix_template(&created.id)
        .await
        .expect("delete template");

    // Resolve again - should return None
    let result = resolver
        .resolve_prefix("tenant-1", &PrefixMode::User, mock_tokenizer)
        .await
        .expect("resolve_prefix should not error");
    assert!(result.is_none(), "Should return None after deletion");
}

#[tokio::test]
async fn test_all_standard_modes() {
    let state = setup_state(None).await.expect("setup_state");
    let db = Arc::new(state.db);
    let resolver = PrefixResolver::new(Arc::clone(&db));

    // Create templates for all standard modes
    let modes = vec![
        PrefixMode::System,
        PrefixMode::User,
        PrefixMode::Builder,
        PrefixMode::Audit,
    ];

    for mode in &modes {
        let template_text = format!("Prefix for {:?}", mode);
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: mode.clone(),
            template_text: template_text.clone(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .expect("create template");

        // Verify each mode resolves correctly
        let resolved = resolver
            .resolve_prefix("tenant-1", mode, mock_tokenizer)
            .await
            .expect("resolve_prefix should not error")
            .expect("Should resolve template");

        assert_eq!(resolved.template.mode, *mode);
        assert_eq!(resolved.template.template_text, template_text);
    }
}
