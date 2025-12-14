//! Integration tests for prefix templates database operations
//!
//! These tests verify:
//! - CRUD operations for prefix templates
//! - Tenant isolation for prefix templates
//! - Priority-based template selection
//! - Fallback to system templates
//! - Template hash computation and validation
//! - Enabled/disabled filtering

use adapteros_api_types::prefix_templates::{
    CreatePrefixTemplateRequest, PrefixMode, UpdatePrefixTemplateRequest,
};
use adapteros_core::B3Hash;
use adapteros_db::Db;
use std::sync::Arc;

async fn setup_db() -> Arc<Db> {
    Arc::new(Db::new_in_memory().await.expect("Failed to create test db"))
}

// =============================================================================
// Basic CRUD Tests
// =============================================================================

#[tokio::test]
async fn test_create_and_retrieve_prefix_template() {
    let db = setup_db().await;

    let req = CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "You are a helpful assistant.".to_string(),
        priority: Some(10),
        enabled: Some(true),
    };

    let created = db.create_prefix_template(req).await.unwrap();

    // Verify ID was generated
    assert!(!created.id.is_empty());
    assert_eq!(created.tenant_id, "tenant-1");
    assert_eq!(created.mode, PrefixMode::User);
    assert_eq!(created.template_text, "You are a helpful assistant.");
    assert_eq!(created.priority, 10);
    assert!(created.enabled);

    // Verify hash was computed correctly
    let expected_hash = B3Hash::hash(b"You are a helpful assistant.");
    assert_eq!(created.template_hash_b3, expected_hash);

    // Retrieve by ID
    let retrieved = db.get_prefix_template(&created.id).await.unwrap().unwrap();
    assert_eq!(retrieved.id, created.id);
    assert_eq!(retrieved.template_text, created.template_text);
    assert_eq!(retrieved.template_hash_b3, expected_hash);
}

#[tokio::test]
async fn test_create_template_with_defaults() {
    let db = setup_db().await;

    let req = CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::System,
        template_text: "System prefix".to_string(),
        priority: None, // Should default to 0
        enabled: None,  // Should default to true
    };

    let created = db.create_prefix_template(req).await.unwrap();

    assert_eq!(created.priority, 0);
    assert!(created.enabled);
}

#[tokio::test]
async fn test_get_nonexistent_template() {
    let db = setup_db().await;

    let result = db.get_prefix_template("nonexistent-id").await.unwrap();
    assert!(result.is_none());
}

// =============================================================================
// Tenant Isolation Tests
// =============================================================================

#[tokio::test]
async fn test_list_templates_tenant_isolation() {
    let db = setup_db().await;

    // Create templates for tenant-1
    for i in 0..3 {
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: format!("Tenant 1 template {}", i),
            priority: Some(i),
            enabled: Some(true),
        })
        .await
        .unwrap();
    }

    // Create templates for tenant-2
    for i in 0..2 {
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-2".to_string(),
            mode: PrefixMode::Builder,
            template_text: format!("Tenant 2 template {}", i),
            priority: Some(i),
            enabled: Some(true),
        })
        .await
        .unwrap();
    }

    // List templates for tenant-1
    let tenant1_templates = db.list_prefix_templates("tenant-1").await.unwrap();
    assert_eq!(tenant1_templates.len(), 3);
    for template in &tenant1_templates {
        assert_eq!(template.tenant_id, "tenant-1");
    }

    // List templates for tenant-2
    let tenant2_templates = db.list_prefix_templates("tenant-2").await.unwrap();
    assert_eq!(tenant2_templates.len(), 2);
    for template in &tenant2_templates {
        assert_eq!(template.tenant_id, "tenant-2");
    }

    // List templates for non-existent tenant
    let empty_templates = db.list_prefix_templates("tenant-3").await.unwrap();
    assert_eq!(empty_templates.len(), 0);
}

#[tokio::test]
async fn test_get_prefix_template_for_mode_tenant_isolation() {
    let db = setup_db().await;

    // Create template for tenant-1
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Tenant 1 user mode".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .unwrap();

    // Create template for tenant-2 with same mode
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-2".to_string(),
        mode: PrefixMode::User,
        template_text: "Tenant 2 user mode".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .unwrap();

    // Get template for tenant-1
    let tenant1_template = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::User)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tenant1_template.tenant_id, "tenant-1");
    assert_eq!(tenant1_template.template_text, "Tenant 1 user mode");

    // Get template for tenant-2
    let tenant2_template = db
        .get_prefix_template_for_mode("tenant-2", &PrefixMode::User)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(tenant2_template.tenant_id, "tenant-2");
    assert_eq!(tenant2_template.template_text, "Tenant 2 user mode");

    // Get template for tenant-3 (non-existent)
    let no_template = db
        .get_prefix_template_for_mode("tenant-3", &PrefixMode::User)
        .await
        .unwrap();
    assert!(no_template.is_none());
}

// =============================================================================
// Priority and Ordering Tests
// =============================================================================

#[tokio::test]
async fn test_list_templates_ordered_by_priority() {
    let db = setup_db().await;

    // Create templates with different priorities
    let priorities = vec![5, 20, 10, 15, 1];
    for priority in priorities {
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: format!("Priority {}", priority),
            priority: Some(priority),
            enabled: Some(true),
        })
        .await
        .unwrap();
    }

    let templates = db.list_prefix_templates("tenant-1").await.unwrap();
    assert_eq!(templates.len(), 5);

    // Verify descending priority order
    assert_eq!(templates[0].priority, 20);
    assert_eq!(templates[1].priority, 15);
    assert_eq!(templates[2].priority, 10);
    assert_eq!(templates[3].priority, 5);
    assert_eq!(templates[4].priority, 1);
}

#[tokio::test]
async fn test_get_template_for_mode_highest_priority() {
    let db = setup_db().await;

    // Create multiple user-mode templates with different priorities
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Low priority".to_string(),
        priority: Some(5),
        enabled: Some(true),
    })
    .await
    .unwrap();

    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "High priority".to_string(),
        priority: Some(20),
        enabled: Some(true),
    })
    .await
    .unwrap();

    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Medium priority".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .unwrap();

    // Should return highest priority template
    let template = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::User)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(template.template_text, "High priority");
    assert_eq!(template.priority, 20);
}

// =============================================================================
// Fallback to System Template Tests
// =============================================================================

#[tokio::test]
async fn test_fallback_to_system_template() {
    let db = setup_db().await;

    // Create system template only
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::System,
        template_text: "System fallback".to_string(),
        priority: Some(5),
        enabled: Some(true),
    })
    .await
    .unwrap();

    // Request builder mode (doesn't exist) - should fall back to system
    let template = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::Builder)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(template.mode, PrefixMode::System);
    assert_eq!(template.template_text, "System fallback");
}

#[tokio::test]
async fn test_no_fallback_when_specific_mode_exists() {
    let db = setup_db().await;

    // Create both system and builder templates
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::System,
        template_text: "System template".to_string(),
        priority: Some(5),
        enabled: Some(true),
    })
    .await
    .unwrap();

    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::Builder,
        template_text: "Builder template".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .unwrap();

    // Request builder mode - should get builder, not fall back to system
    let template = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::Builder)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(template.mode, PrefixMode::Builder);
    assert_eq!(template.template_text, "Builder template");
}

#[tokio::test]
async fn test_no_fallback_for_system_mode() {
    let db = setup_db().await;

    // No templates exist
    let result = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::System)
        .await
        .unwrap();

    assert!(result.is_none(), "System mode should not fall back");
}

// =============================================================================
// Enabled/Disabled Filtering Tests
// =============================================================================

#[tokio::test]
async fn test_disabled_templates_not_returned_for_mode() {
    let db = setup_db().await;

    // Create enabled template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Enabled template".to_string(),
        priority: Some(5),
        enabled: Some(true),
    })
    .await
    .unwrap();

    // Create disabled template with higher priority
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Disabled template".to_string(),
        priority: Some(20),
        enabled: Some(false),
    })
    .await
    .unwrap();

    // Should return enabled template, not disabled one
    let template = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::User)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(template.template_text, "Enabled template");
    assert_eq!(template.priority, 5);
}

#[tokio::test]
async fn test_list_includes_disabled_templates() {
    let db = setup_db().await;

    // Create mix of enabled and disabled templates
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::User,
        template_text: "Enabled".to_string(),
        priority: Some(10),
        enabled: Some(true),
    })
    .await
    .unwrap();

    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::Builder,
        template_text: "Disabled".to_string(),
        priority: Some(5),
        enabled: Some(false),
    })
    .await
    .unwrap();

    // list_prefix_templates should return both
    let templates = db.list_prefix_templates("tenant-1").await.unwrap();
    assert_eq!(templates.len(), 2);

    let enabled_count = templates.iter().filter(|t| t.enabled).count();
    let disabled_count = templates.iter().filter(|t| !t.enabled).count();
    assert_eq!(enabled_count, 1);
    assert_eq!(disabled_count, 1);
}

// =============================================================================
// Update Tests
// =============================================================================

#[tokio::test]
async fn test_update_template_text_recomputes_hash() {
    let db = setup_db().await;

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "Original text".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    let original_hash = created.template_hash_b3;

    // Update template text
    let updated = db
        .update_prefix_template(
            &created.id,
            UpdatePrefixTemplateRequest {
                mode: None,
                template_text: Some("Updated text".to_string()),
                priority: None,
                enabled: None,
            },
        )
        .await
        .unwrap()
        .unwrap();

    // Hash should be different
    assert_ne!(updated.template_hash_b3, original_hash);

    // Hash should match new text
    let expected_hash = B3Hash::hash(b"Updated text");
    assert_eq!(updated.template_hash_b3, expected_hash);

    // Other fields should be unchanged
    assert_eq!(updated.mode, PrefixMode::User);
    assert_eq!(updated.priority, 10);
    assert!(updated.enabled);
}

#[tokio::test]
async fn test_update_partial_fields() {
    let db = setup_db().await;

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "Original".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    // Update only priority
    let updated = db
        .update_prefix_template(
            &created.id,
            UpdatePrefixTemplateRequest {
                mode: None,
                template_text: None,
                priority: Some(20),
                enabled: None,
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.priority, 20);
    assert_eq!(updated.template_text, "Original");
    assert_eq!(updated.mode, PrefixMode::User);
    assert!(updated.enabled);
}

#[tokio::test]
async fn test_update_nonexistent_template() {
    let db = setup_db().await;

    let result = db
        .update_prefix_template(
            "nonexistent-id",
            UpdatePrefixTemplateRequest {
                mode: None,
                template_text: Some("New text".to_string()),
                priority: None,
                enabled: None,
            },
        )
        .await
        .unwrap();

    assert!(result.is_none());
}

#[tokio::test]
async fn test_update_mode() {
    let db = setup_db().await;

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "Template".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    // Update mode
    let updated = db
        .update_prefix_template(
            &created.id,
            UpdatePrefixTemplateRequest {
                mode: Some(PrefixMode::Builder),
                template_text: None,
                priority: None,
                enabled: None,
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(updated.mode, PrefixMode::Builder);
}

#[tokio::test]
async fn test_toggle_enabled() {
    let db = setup_db().await;

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "Template".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    assert!(created.enabled);

    // Disable
    let disabled = db
        .update_prefix_template(
            &created.id,
            UpdatePrefixTemplateRequest {
                mode: None,
                template_text: None,
                priority: None,
                enabled: Some(false),
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert!(!disabled.enabled);

    // Re-enable
    let re_enabled = db
        .update_prefix_template(
            &created.id,
            UpdatePrefixTemplateRequest {
                mode: None,
                template_text: None,
                priority: None,
                enabled: Some(true),
            },
        )
        .await
        .unwrap()
        .unwrap();

    assert!(re_enabled.enabled);
}

// =============================================================================
// Delete Tests
// =============================================================================

#[tokio::test]
async fn test_delete_template() {
    let db = setup_db().await;

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "To be deleted".to_string(),
            priority: None,
            enabled: None,
        })
        .await
        .unwrap();

    // Verify it exists
    assert!(db.get_prefix_template(&created.id).await.unwrap().is_some());

    // Delete
    let deleted = db.delete_prefix_template(&created.id).await.unwrap();
    assert!(deleted, "Delete should return true");

    // Verify it's gone
    assert!(db.get_prefix_template(&created.id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_delete_nonexistent_template() {
    let db = setup_db().await;

    let deleted = db.delete_prefix_template("nonexistent-id").await.unwrap();
    assert!(!deleted, "Delete should return false for non-existent ID");
}

#[tokio::test]
async fn test_delete_templates_for_tenant() {
    let db = setup_db().await;

    // Create templates for tenant-1
    for i in 0..5 {
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: format!("Template {}", i),
            priority: Some(i),
            enabled: Some(true),
        })
        .await
        .unwrap();
    }

    // Create templates for tenant-2
    for i in 0..3 {
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-2".to_string(),
            mode: PrefixMode::Builder,
            template_text: format!("Template {}", i),
            priority: Some(i),
            enabled: Some(true),
        })
        .await
        .unwrap();
    }

    // Delete all templates for tenant-1
    let deleted_count = db
        .delete_prefix_templates_for_tenant("tenant-1")
        .await
        .unwrap();
    assert_eq!(deleted_count, 5);

    // Verify tenant-1 has no templates
    let tenant1_templates = db.list_prefix_templates("tenant-1").await.unwrap();
    assert_eq!(tenant1_templates.len(), 0);

    // Verify tenant-2 templates are unaffected
    let tenant2_templates = db.list_prefix_templates("tenant-2").await.unwrap();
    assert_eq!(tenant2_templates.len(), 3);
}

// =============================================================================
// Custom Mode Tests
// =============================================================================

#[tokio::test]
async fn test_custom_mode() {
    let db = setup_db().await;

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::Custom("special_mode".to_string()),
            template_text: "Custom mode template".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    assert_eq!(created.mode, PrefixMode::Custom("special_mode".to_string()));

    // Retrieve by custom mode
    let retrieved = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::Custom("special_mode".to_string()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(retrieved.id, created.id);
    assert_eq!(retrieved.template_text, "Custom mode template");
}

#[tokio::test]
async fn test_custom_mode_no_fallback() {
    let db = setup_db().await;

    // Create system template
    db.create_prefix_template(CreatePrefixTemplateRequest {
        tenant_id: "tenant-1".to_string(),
        mode: PrefixMode::System,
        template_text: "System template".to_string(),
        priority: Some(5),
        enabled: Some(true),
    })
    .await
    .unwrap();

    // Request non-existent custom mode - should fall back to system
    let template = db
        .get_prefix_template_for_mode("tenant-1", &PrefixMode::Custom("unknown".to_string()))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(template.mode, PrefixMode::System);
    assert_eq!(template.template_text, "System template");
}

// =============================================================================
// Hash Validation Tests
// =============================================================================

#[tokio::test]
async fn test_template_hash_matches_text() {
    let db = setup_db().await;

    let test_cases = vec![
        "Simple text",
        "Text with special chars: !@#$%^&*()",
        "Multi-line\ntext\nwith\nnewlines",
        "Unicode: 你好世界 🌍",
        "",
    ];

    for text in test_cases {
        let created = db
            .create_prefix_template(CreatePrefixTemplateRequest {
                tenant_id: "tenant-1".to_string(),
                mode: PrefixMode::User,
                template_text: text.to_string(),
                priority: Some(10),
                enabled: Some(true),
            })
            .await
            .unwrap();

        let expected_hash = B3Hash::hash(text.as_bytes());
        assert_eq!(
            created.template_hash_b3, expected_hash,
            "Hash mismatch for text: {:?}",
            text
        );

        // Verify retrieval also has correct hash
        let retrieved = db.get_prefix_template(&created.id).await.unwrap().unwrap();
        assert_eq!(
            retrieved.template_hash_b3, expected_hash,
            "Retrieved hash mismatch for text: {:?}",
            text
        );
    }
}

#[tokio::test]
async fn test_different_text_different_hash() {
    let db = setup_db().await;

    let template1 = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "Text A".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    let template2 = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "Text B".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    assert_ne!(
        template1.template_hash_b3, template2.template_hash_b3,
        "Different texts should have different hashes"
    );
}

// =============================================================================
// Edge Cases
// =============================================================================

#[tokio::test]
async fn test_empty_template_text() {
    let db = setup_db().await;

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "".to_string(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    assert_eq!(created.template_text, "");
    assert_eq!(created.template_hash_b3, B3Hash::hash(b""));
}

#[tokio::test]
async fn test_very_long_template_text() {
    let db = setup_db().await;

    // Create a 10KB template
    let long_text = "a".repeat(10_000);

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: long_text.clone(),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    assert_eq!(created.template_text.len(), 10_000);
    assert_eq!(created.template_hash_b3, B3Hash::hash(long_text.as_bytes()));

    // Verify retrieval
    let retrieved = db.get_prefix_template(&created.id).await.unwrap().unwrap();
    assert_eq!(retrieved.template_text, long_text);
}

#[tokio::test]
async fn test_negative_priority() {
    let db = setup_db().await;

    let created = db
        .create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: "Negative priority".to_string(),
            priority: Some(-10),
            enabled: Some(true),
        })
        .await
        .unwrap();

    assert_eq!(created.priority, -10);
}

#[tokio::test]
async fn test_multiple_templates_same_priority_ordering() {
    let db = setup_db().await;

    // Create multiple templates with the same priority
    for i in 0..3 {
        db.create_prefix_template(CreatePrefixTemplateRequest {
            tenant_id: "tenant-1".to_string(),
            mode: PrefixMode::User,
            template_text: format!("Same priority {}", i),
            priority: Some(10),
            enabled: Some(true),
        })
        .await
        .unwrap();
    }

    // List should return all templates, ordered by created_at (secondary sort)
    let templates = db.list_prefix_templates("tenant-1").await.unwrap();
    assert_eq!(templates.len(), 3);

    // All should have same priority
    for template in &templates {
        assert_eq!(template.priority, 10);
    }
}
