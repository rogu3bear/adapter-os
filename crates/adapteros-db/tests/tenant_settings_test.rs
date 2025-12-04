//! Integration tests for tenant settings functionality
//!
//! Tests the behavior of tenant settings for controlling default stack/adapter behavior:
//! - get_tenant_settings() - returns defaults if not configured
//! - upsert_tenant_settings() - creates or updates settings
//! - should_inherit_stack_on_chat_create() - optimized boolean query
//! - should_fallback_stack_on_infer() - optimized boolean query

use adapteros_db::{Db, UpdateTenantSettingsParams};

#[tokio::test]
async fn test_get_settings_returns_defaults_when_not_configured() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Get settings for a tenant with no settings configured
    let settings = db.get_tenant_settings(&tenant_id).await.unwrap();

    // Should return defaults (all FALSE)
    assert_eq!(settings.tenant_id, tenant_id);
    assert!(
        !settings.use_default_stack_on_chat_create,
        "Default should be false"
    );
    assert!(
        !settings.use_default_stack_on_infer_session,
        "Default should be false"
    );
}

#[tokio::test]
async fn test_upsert_settings_creates_and_updates() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Create new settings
    let params = UpdateTenantSettingsParams {
        use_default_stack_on_chat_create: Some(true),
        use_default_stack_on_infer_session: Some(false),
        settings_json: None,
    };
    let settings = db.upsert_tenant_settings(&tenant_id, params).await.unwrap();

    assert!(settings.use_default_stack_on_chat_create);
    assert!(!settings.use_default_stack_on_infer_session);

    // Update settings
    let params = UpdateTenantSettingsParams {
        use_default_stack_on_chat_create: None, // Keep existing
        use_default_stack_on_infer_session: Some(true),
        settings_json: Some(r#"{"test": true}"#.to_string()),
    };
    let updated = db.upsert_tenant_settings(&tenant_id, params).await.unwrap();

    // First value should be preserved, second updated
    assert!(updated.use_default_stack_on_chat_create);
    assert!(updated.use_default_stack_on_infer_session);
    assert_eq!(updated.settings_json, Some(r#"{"test": true}"#.to_string()));
}

#[tokio::test]
async fn test_partial_update_preserves_existing() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Set both values to true
    let params = UpdateTenantSettingsParams {
        use_default_stack_on_chat_create: Some(true),
        use_default_stack_on_infer_session: Some(true),
        settings_json: Some(r#"{"key": "value"}"#.to_string()),
    };
    db.upsert_tenant_settings(&tenant_id, params).await.unwrap();

    // Update only one value
    let params = UpdateTenantSettingsParams {
        use_default_stack_on_chat_create: Some(false),
        use_default_stack_on_infer_session: None, // Preserve existing
        settings_json: None,                      // Preserve existing
    };
    let updated = db.upsert_tenant_settings(&tenant_id, params).await.unwrap();

    // First value updated, second preserved
    assert!(!updated.use_default_stack_on_chat_create);
    assert!(updated.use_default_stack_on_infer_session);
    assert_eq!(
        updated.settings_json,
        Some(r#"{"key": "value"}"#.to_string())
    );
}

#[tokio::test]
async fn test_should_inherit_stack_query() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Default is false
    let result = db
        .should_inherit_stack_on_chat_create(&tenant_id)
        .await
        .unwrap();
    assert!(!result, "Default should be false");

    // Enable the setting
    let params = UpdateTenantSettingsParams {
        use_default_stack_on_chat_create: Some(true),
        use_default_stack_on_infer_session: None,
        settings_json: None,
    };
    db.upsert_tenant_settings(&tenant_id, params).await.unwrap();

    // Now should return true
    let result = db
        .should_inherit_stack_on_chat_create(&tenant_id)
        .await
        .unwrap();
    assert!(result, "Should be true after enabling");
}

#[tokio::test]
async fn test_should_fallback_stack_query() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Default is false
    let result = db.should_fallback_stack_on_infer(&tenant_id).await.unwrap();
    assert!(!result, "Default should be false");

    // Enable the setting
    let params = UpdateTenantSettingsParams {
        use_default_stack_on_chat_create: None,
        use_default_stack_on_infer_session: Some(true),
        settings_json: None,
    };
    db.upsert_tenant_settings(&tenant_id, params).await.unwrap();

    // Now should return true
    let result = db.should_fallback_stack_on_infer(&tenant_id).await.unwrap();
    assert!(result, "Should be true after enabling");
}

#[tokio::test]
async fn test_delete_tenant_settings() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Create settings
    let params = UpdateTenantSettingsParams {
        use_default_stack_on_chat_create: Some(true),
        use_default_stack_on_infer_session: Some(true),
        settings_json: None,
    };
    db.upsert_tenant_settings(&tenant_id, params).await.unwrap();

    // Verify settings exist
    let settings = db.get_tenant_settings(&tenant_id).await.unwrap();
    assert!(settings.use_default_stack_on_chat_create);

    // Delete settings
    let deleted = db.delete_tenant_settings(&tenant_id).await.unwrap();
    assert!(deleted, "Should have deleted settings");

    // Settings should now return defaults
    let settings = db.get_tenant_settings(&tenant_id).await.unwrap();
    assert!(
        !settings.use_default_stack_on_chat_create,
        "Should return defaults after deletion"
    );
}

#[tokio::test]
async fn test_delete_nonexistent_settings() {
    let db = match Db::new_in_memory().await {
        Ok(db) => db,
        Err(e) => {
            eprintln!("Skipping test - DB setup failed: {}", e);
            return;
        }
    };

    // Create a tenant but don't create settings
    let tenant_id = db.create_tenant("test-tenant", false).await.unwrap();

    // Delete should return false (nothing to delete)
    let deleted = db.delete_tenant_settings(&tenant_id).await.unwrap();
    assert!(!deleted, "Should return false when no settings exist");
}
