//! Tests for adapter schema stability and query consistency
//!
//! Validates that:
//! - Adapter struct fields match database schema
//! - Expired adapter cleanup works correctly
//! - No schema drift between code and migrations

use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use chrono::{Duration, Utc};

/// Helper function to set up test database with default tenant
async fn setup_test_db() -> Db {
    let db = Db::connect("sqlite::memory:").await.unwrap();
    db.migrate().await.unwrap();

    // Create a default tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')")
        .execute(db.pool())
        .await
        .unwrap();

    db
}

/// Test that find_expired_adapters correctly retrieves and deserializes expired adapters
///
/// This test validates the fix for the schema drift bug where SELECT * was used
/// with extra columns in the database that weren't in the Adapter struct.
#[tokio::test]
async fn test_find_expired_adapters_with_all_schema_fields() {
    let db = setup_test_db().await;

    // Create an expired adapter with all fields
    // Use SQLite datetime format: YYYY-MM-DD HH:MM:SS
    let expired_time = (Utc::now() - Duration::hours(1))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("expired-adapter-1")
        .name("Expired Test Adapter")
        .hash_b3("b3:test_hash_expired")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .expires_at(Some(expired_time))
        .build()
        .unwrap();

    let adapter_id = db.register_adapter(params).await.unwrap();
    assert!(!adapter_id.is_empty());

    // Create a non-expired adapter for comparison
    let future_time = (Utc::now() + Duration::days(7))
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let params2 = AdapterRegistrationBuilder::new()
        .adapter_id("active-adapter-1")
        .name("Active Test Adapter")
        .hash_b3("b3:test_hash_active")
        .rank(8)
        .tier("warm")
        .category("code")
        .scope("global")
        .expires_at(Some(future_time))
        .build()
        .unwrap();

    db.register_adapter(params2).await.unwrap();

    // Find expired adapters
    let expired_adapters = db.find_expired_adapters().await.unwrap();

    // Should only find the expired one
    assert_eq!(
        expired_adapters.len(),
        1,
        "Should find exactly one expired adapter"
    );

    let expired = &expired_adapters[0];
    assert_eq!(expired.adapter_id.as_deref(), Some("expired-adapter-1"));
    assert_eq!(expired.name, "Expired Test Adapter");
    assert_eq!(expired.hash_b3, "b3:test_hash_expired");
    assert_eq!(expired.rank, 8);
    assert_eq!(expired.tier, "warm");
    assert_eq!(expired.category, "code");
    assert_eq!(expired.scope, "global");

    // Verify new schema fields are populated
    assert_eq!(
        expired.load_state, "cold",
        "Default load_state should be 'cold'"
    );
    assert!(
        expired.last_loaded_at.is_none(),
        "last_loaded_at should initially be None"
    );
    // Note: aos_file_path and aos_file_hash are in AdapterRegistrationParams
    // but not in the Adapter struct - they would need to be added to the schema
    // TODO: Add aos_file_path and aos_file_hash to Adapter struct when DB migration is added

    assert_eq!(expired.active, 1, "Adapter should be active");
    assert!(
        expired.expires_at.is_some(),
        "Expired adapter should have expires_at"
    );
}

/// Test that adapters without expiration are not returned by find_expired_adapters
#[tokio::test]
async fn test_find_expired_adapters_excludes_non_expiring() {
    let db = setup_test_db().await;

    // Create adapter without expiration
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("permanent-adapter")
        .name("Permanent Adapter")
        .hash_b3("b3:permanent_hash")
        .rank(8)
        .tier("persistent")
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Should find no expired adapters
    let expired_adapters = db.find_expired_adapters().await.unwrap();
    assert_eq!(
        expired_adapters.len(),
        0,
        "Should not find any expired adapters when none exist"
    );
}

/// Test schema-query consistency by verifying all Adapter struct fields
/// can be populated from a database query
#[tokio::test]
async fn test_adapter_struct_schema_consistency() {
    let db = setup_test_db().await;

    // Create adapter with all optional fields set
    let params = AdapterRegistrationBuilder::new()
        .adapter_id("full-adapter")
        .name("Full Feature Adapter")
        .hash_b3("b3:full_hash_123")
        .rank(16)
        .tier("persistent")
        .languages_json(Some(r#"["rust","python"]"#))
        .framework(Some("pytorch"))
        .category("code")
        .scope("global")
        .framework_id(Some("pytorch-2.0"))
        .framework_version(Some("2.0.1"))
        .repo_id(Some("github.com/test/repo"))
        .commit_sha(Some("abc123def456"))
        .intent(Some("text-classification"))
        .expires_at(Some(
            (Utc::now() + Duration::days(30))
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        ))
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Retrieve adapter using get_adapter
    let adapter = db
        .get_adapter("full-adapter")
        .await
        .unwrap()
        .expect("Adapter should exist");

    // Verify all fields are correctly populated
    assert_eq!(adapter.adapter_id.as_deref(), Some("full-adapter"));
    assert_eq!(adapter.name, "Full Feature Adapter");
    assert_eq!(adapter.hash_b3, "b3:full_hash_123");
    assert_eq!(adapter.rank, 16);
    assert_eq!(adapter.tier, "persistent");
    assert_eq!(
        adapter.languages_json.as_deref(),
        Some(r#"["rust","python"]"#)
    );
    assert_eq!(adapter.framework.as_deref(), Some("pytorch"));
    assert_eq!(adapter.category, "code");
    assert_eq!(adapter.scope, "global");
    assert_eq!(adapter.framework_id.as_deref(), Some("pytorch-2.0"));
    assert_eq!(adapter.framework_version.as_deref(), Some("2.0.1"));
    assert_eq!(adapter.repo_id.as_deref(), Some("github.com/test/repo"));
    assert_eq!(adapter.commit_sha.as_deref(), Some("abc123def456"));
    assert_eq!(adapter.intent.as_deref(), Some("text-classification"));

    // Verify lifecycle fields
    assert_eq!(adapter.current_state, "unloaded");
    assert_eq!(adapter.pinned, 0);
    assert_eq!(adapter.memory_bytes, 0);
    assert_eq!(adapter.activation_count, 0);
    assert_eq!(adapter.active, 1);

    // Verify new schema fields from migration 0031
    assert_eq!(adapter.load_state, "cold");
    assert!(adapter.last_loaded_at.is_none());
    // Note: aos_file_path and aos_file_hash are not in Adapter struct yet
    // TODO: Add these fields when schema migration is implemented

    // Verify timestamps exist
    assert!(!adapter.created_at.is_empty());
    assert!(!adapter.updated_at.is_empty());
    assert!(adapter.expires_at.is_some());
}

/// Test that list_adapters also works with the updated schema
#[tokio::test]
async fn test_list_adapters_with_new_schema_fields() {
    let db = setup_test_db().await;

    // Create multiple adapters
    let tiers = ["ephemeral", "warm", "persistent"];
    for i in 1..=3 {
        let params = AdapterRegistrationBuilder::new()
            .adapter_id(format!("adapter-{}", i))
            .name(format!("Test Adapter {}", i))
            .hash_b3(format!("b3:hash_{}", i))
            .rank(8)
            .tier(tiers[i - 1])
            .build()
            .unwrap();

        db.register_adapter(params).await.unwrap();
    }

    // List all adapters (system-level for tests)
    let adapters = db.list_all_adapters_system().await.unwrap();
    assert_eq!(adapters.len(), 3, "Should list all 3 adapters");

    // Verify each adapter has all fields including new schema fields
    for adapter in &adapters {
        assert_eq!(adapter.load_state, "cold");
        assert!(adapter.last_loaded_at.is_none());
        // aos_file_path and aos_file_hash not in Adapter struct
        assert_eq!(adapter.active, 1);
    }
}

/// Test that category and scope queries work with new schema
#[tokio::test]
async fn test_filtered_queries_with_new_schema() {
    let db = setup_test_db().await;

    // Create adapters with different categories
    let params1 = AdapterRegistrationBuilder::new()
        .adapter_id("nlp-adapter")
        .name("NLP Adapter")
        .hash_b3("b3:nlp_hash")
        .rank(8)
        .tier("persistent")
        .category("code")
        .scope("global")
        .build()
        .unwrap();

    let params2 = AdapterRegistrationBuilder::new()
        .adapter_id("vision-adapter")
        .name("Vision Adapter")
        .hash_b3("b3:vision_hash")
        .rank(8)
        .tier("persistent")
        .category("framework")
        .scope("tenant")
        .build()
        .unwrap();

    db.register_adapter(params1).await.unwrap();
    db.register_adapter(params2).await.unwrap();

    // Test category filtering
    let code_adapters = db.list_adapters_by_category("default-tenant", "code").await.unwrap();
    assert_eq!(code_adapters.len(), 1);
    assert_eq!(code_adapters[0].adapter_id.as_deref(), Some("nlp-adapter"));
    assert_eq!(code_adapters[0].load_state, "cold");

    // Test scope filtering
    let global_adapters = db.list_adapters_by_scope("default-tenant", "global").await.unwrap();
    assert_eq!(global_adapters.len(), 1);
    assert_eq!(
        global_adapters[0].adapter_id.as_deref(),
        Some("nlp-adapter")
    );
    assert_eq!(global_adapters[0].load_state, "cold");

    // Test state filtering
    let unloaded_adapters = db.list_adapters_by_state("default-tenant", "unloaded").await.unwrap();
    assert_eq!(unloaded_adapters.len(), 2);
    for adapter in &unloaded_adapters {
        assert_eq!(adapter.load_state, "cold");
        // aos_file_path not in Adapter struct
    }
}
