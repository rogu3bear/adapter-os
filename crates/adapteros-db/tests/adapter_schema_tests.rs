//! Tests for adapter schema stability and query consistency
//!
//! Validates that:
//! - Adapter struct fields match database schema
//! - Expired adapter cleanup works correctly
//! - No schema drift between code and migrations
#![allow(deprecated)]

use adapteros_db::adapters::AdapterRegistrationBuilder;
use adapteros_db::Db;
use chrono::{Duration, Utc};

/// Helper function to set up test database with default tenant
async fn setup_test_db() -> Db {
    // Use new_in_memory() for better pool management
    let db = Db::new_in_memory().await.unwrap();

    // Create a default tenant using the db's create_tenant method when possible
    // Fall back to direct insert for consistency with existing tests
    sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('default-tenant', 'Default Test Tenant')",
    )
    .execute(db.pool_result().unwrap())
    .await
    .unwrap();
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES ('system', 'System')")
        .execute(db.pool_result().unwrap())
        .await
        .unwrap();

    db
}

/// Helper function to transition an adapter to active state for testing
/// This bypasses the full lifecycle transition API to avoid pool deadlocks
async fn set_adapter_active_for_test(db: &Db, adapter_id: &str) {
    // First seed required artifacts with unique content_hash_b3 per adapter
    let unique_content_hash = format!("testcontent_{}", adapter_id);
    sqlx::query(
        "UPDATE adapters SET aos_file_path = 'test/path.aos', aos_file_hash = 'testhash', content_hash_b3 = ? WHERE adapter_id = ?",
    )
    .bind(&unique_content_hash)
    .bind(adapter_id)
    .execute(db.pool_result().unwrap())
    .await
    .unwrap();

    // Temporarily drop BOTH lifecycle state triggers:
    // 1. validate_adapter_lifecycle_state_update - validates state is valid enum value
    // 2. enforce_adapter_lifecycle_transitions - enforces state machine rules
    sqlx::query("DROP TRIGGER IF EXISTS validate_adapter_lifecycle_state_update")
        .execute(db.pool_result().unwrap())
        .await
        .unwrap();
    sqlx::query("DROP TRIGGER IF EXISTS enforce_adapter_lifecycle_transitions")
        .execute(db.pool_result().unwrap())
        .await
        .unwrap();

    // Directly update to active state
    sqlx::query("UPDATE adapters SET lifecycle_state = 'active' WHERE adapter_id = ?")
        .bind(adapter_id)
        .execute(db.pool_result().unwrap())
        .await
        .unwrap();

    // Recreate the trigger (we can skip this for test cleanup, as each test gets fresh db)
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
    assert!(
        expired.aos_file_path.is_none(),
        "aos_file_path should be None by default"
    );
    assert!(
        expired.aos_file_hash.is_none(),
        "aos_file_hash should be None by default"
    );

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
        .tenant_id("system")
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
    assert!(
        adapter.aos_file_path.is_none(),
        "aos_file_path should be None by default"
    );
    assert!(
        adapter.aos_file_hash.is_none(),
        "aos_file_hash should be None by default"
    );

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

// =============================================================================
// Multi-Dimensional Scope Conflict Tests (Phase 4 - Harmonization)
// =============================================================================
//
// These tests validate the three-dimensional scope validation system:
// - repo_id: Repository identifier (e.g., "github.com/org/repo")
// - repo_path: Filesystem path to the repository
// - codebase_scope: Codebase adapter scope identifier
//
// The validation functions check each dimension independently and aggregate
// conflicts. There is no defined precedence - all violations are reported.
// =============================================================================

/// Test that an adapter can pass validation across multiple scope dimensions
/// when no conflicts exist.
#[tokio::test]
async fn test_multi_dimensional_scope_no_conflicts() {
    let db = setup_test_db().await;

    // Register an adapter with all three scope dimensions
    // Note: Codebase adapters (with codebase_scope) must use the system tenant
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("system")
        .adapter_id("adapter-multi-dim-1")
        .name("Multi-Dimensional Adapter 1")
        .hash_b3("b3:multi_dim_1")
        .rank(8)
        .tier("persistent")
        .repo_id(Some("github.com/org/repo-a"))
        .repo_path(Some("/Users/dev/repos/repo-a"))
        .codebase_scope(Some("org/repo-a"))
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Set adapter to active state for testing
    set_adapter_active_for_test(&db, "adapter-multi-dim-1").await;

    // Try to validate a NEW adapter with DIFFERENT values in all dimensions
    let result = db
        .validate_active_uniqueness(
            "adapter-multi-dim-2", // New adapter
            Some("github.com/org/repo-b".to_string()),
            Some("/Users/dev/repos/repo-b".to_string()),
            Some("org/repo-b".to_string()),
            None,
        )
        .await
        .unwrap();

    assert!(
        result.is_valid,
        "Should be valid when all dimensions differ: {:?}",
        result.conflict_reason
    );
    assert!(
        result.conflicting_adapters.is_empty(),
        "Should have no conflicts"
    );
}

/// Test that a conflict is detected when repo_id matches but other dimensions differ.
/// This validates that partial dimensional matches are detected.
#[tokio::test]
async fn test_conflict_repo_id_only() {
    let db = setup_test_db().await;

    // Register adapter with repo_id
    // Note: Codebase adapters (with codebase_scope) must use the system tenant
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("system")
        .adapter_id("adapter-repo-id-conflict")
        .name("Repo ID Conflict Adapter")
        .hash_b3("b3:repo_id_conflict")
        .rank(8)
        .tier("persistent")
        .repo_id(Some("github.com/org/shared-repo"))
        .repo_path(Some("/Users/dev/repos/original-path"))
        .codebase_scope(Some("org/scope-a"))
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Set adapter to active state for testing
    set_adapter_active_for_test(&db, "adapter-repo-id-conflict").await;

    // Validate with same repo_id but different repo_path and codebase_scope
    let result = db
        .validate_active_uniqueness(
            "adapter-new",
            Some("github.com/org/shared-repo".to_string()), // SAME repo_id
            Some("/Users/dev/repos/different-path".to_string()), // Different path
            Some("org/scope-b".to_string()),                // Different scope
            None,
        )
        .await
        .unwrap();

    assert!(
        !result.is_valid,
        "Should detect conflict on repo_id dimension"
    );
    assert!(
        !result.conflicting_adapters.is_empty(),
        "Should report conflicting adapter"
    );
    assert!(
        result
            .conflicting_adapters
            .contains(&"adapter-repo-id-conflict".to_string()),
        "Should identify the conflicting adapter by ID"
    );
}

/// Test that a conflict is detected when codebase_scope matches but repo_id differs.
/// This tests the codebase adapter dimension independently.
#[tokio::test]
async fn test_conflict_codebase_scope_only() {
    let db = setup_test_db().await;

    // Register adapter with codebase_scope
    // Note: Codebase adapters (with codebase_scope) must use the system tenant
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("system")
        .adapter_id("adapter-scope-conflict")
        .name("Codebase Scope Conflict Adapter")
        .hash_b3("b3:scope_conflict")
        .rank(8)
        .tier("persistent")
        .repo_id(Some("github.com/org/repo-original"))
        .codebase_scope(Some("shared/codebase/scope"))
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Set adapter to active state for testing
    set_adapter_active_for_test(&db, "adapter-scope-conflict").await;

    // Validate with different repo_id but same codebase_scope
    let result = db
        .validate_active_uniqueness(
            "adapter-new-scope",
            Some("github.com/org/repo-different".to_string()), // Different repo_id
            None,
            Some("shared/codebase/scope".to_string()), // SAME codebase_scope
            None,
        )
        .await
        .unwrap();

    assert!(
        !result.is_valid,
        "Should detect conflict on codebase_scope dimension"
    );
    assert!(
        result
            .conflicting_adapters
            .contains(&"adapter-scope-conflict".to_string()),
        "Should identify the conflicting adapter"
    );
}

/// Test that conflicts are detected and reported across ALL matching dimensions
/// when an adapter matches on multiple dimensions simultaneously.
/// Validates that the system reports multiple violations (no precedence/short-circuit).
#[tokio::test]
async fn test_all_three_dimensions_match() {
    let db = setup_test_db().await;

    // Register adapter with all three dimensions
    // Note: Codebase adapters (with codebase_scope) must use the system tenant
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("system")
        .adapter_id("adapter-all-dims")
        .name("All Dimensions Adapter")
        .hash_b3("b3:all_dims")
        .rank(8)
        .tier("persistent")
        .repo_id(Some("github.com/full/match"))
        .repo_path(Some("/Users/dev/full/match"))
        .codebase_scope(Some("full/match/scope"))
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Set adapter to active state for testing
    set_adapter_active_for_test(&db, "adapter-all-dims").await;

    // Validate with ALL dimensions matching
    let result = db
        .validate_active_uniqueness(
            "adapter-triple-match",
            Some("github.com/full/match".to_string()), // SAME repo_id
            Some("/Users/dev/full/match".to_string()), // SAME repo_path
            Some("full/match/scope".to_string()),      // SAME codebase_scope
            None,
        )
        .await
        .unwrap();

    assert!(
        !result.is_valid,
        "Should detect conflicts on all dimensions"
    );
    assert!(
        result
            .conflicting_adapters
            .contains(&"adapter-all-dims".to_string()),
        "Should identify the conflicting adapter"
    );
    // The adapter should only appear once (deduplication)
    let conflict_count = result
        .conflicting_adapters
        .iter()
        .filter(|a| *a == "adapter-all-dims")
        .count();
    assert_eq!(
        conflict_count, 1,
        "Same adapter should be deduplicated across dimensions"
    );
    // Conflict reason should mention multiple violations
    let reason = result.conflict_reason.as_deref().unwrap_or("");
    assert!(
        reason.contains("Multiple active uniqueness violations")
            || reason.contains("repo_id")
            || reason.contains("repo_path")
            || reason.contains("codebase_scope"),
        "Conflict reason should indicate multi-dimensional violation: {}",
        reason
    );
}

/// Test that conflict detection is per-dimension and aggregates correctly
/// when two different adapters conflict on different dimensions.
#[tokio::test]
async fn test_different_adapters_conflict_on_different_dimensions() {
    let db = setup_test_db().await;

    // First adapter: conflicts on repo_id
    // Note: Any adapter with repo_id is considered a codebase adapter and must use system tenant
    let params1 = AdapterRegistrationBuilder::new()
        .tenant_id("system")
        .adapter_id("adapter-dim-a")
        .name("Dimension A Adapter")
        .hash_b3("b3:dim_a")
        .rank(8)
        .tier("persistent")
        .repo_id(Some("github.com/org/conflict-repo"))
        .build()
        .unwrap();

    db.register_adapter(params1).await.unwrap();

    // Set adapter to active state for testing
    set_adapter_active_for_test(&db, "adapter-dim-a").await;

    // Second adapter: conflicts on codebase_scope (different repo_id)
    // Note: Codebase adapters (with codebase_scope) must use the system tenant
    let params2 = AdapterRegistrationBuilder::new()
        .tenant_id("system")
        .adapter_id("adapter-dim-b")
        .name("Dimension B Adapter")
        .hash_b3("b3:dim_b")
        .rank(8)
        .tier("persistent")
        .repo_id(Some("github.com/org/other-repo"))
        .codebase_scope(Some("shared/scope/target"))
        .build()
        .unwrap();

    db.register_adapter(params2).await.unwrap();

    // Set adapter to active state for testing
    set_adapter_active_for_test(&db, "adapter-dim-b").await;

    // Validate a new adapter that conflicts with BOTH on different dimensions
    let result = db
        .validate_active_uniqueness(
            "adapter-multi-conflict",
            Some("github.com/org/conflict-repo".to_string()), // Conflicts with adapter-dim-a
            None,
            Some("shared/scope/target".to_string()), // Conflicts with adapter-dim-b
            None,
        )
        .await
        .unwrap();

    assert!(
        !result.is_valid,
        "Should detect conflicts from both adapters"
    );
    assert!(
        result
            .conflicting_adapters
            .contains(&"adapter-dim-a".to_string()),
        "Should report adapter-dim-a as conflicting"
    );
    assert!(
        result
            .conflicting_adapters
            .contains(&"adapter-dim-b".to_string()),
        "Should report adapter-dim-b as conflicting"
    );
    assert_eq!(
        result.conflicting_adapters.len(),
        2,
        "Should report exactly two conflicting adapters"
    );
}

/// Test that an adapter can self-validate (validate itself for activation)
/// without reporting itself as a conflict.
#[tokio::test]
async fn test_self_validation_no_conflict() {
    let db = setup_test_db().await;

    // Register adapter
    // Note: Codebase adapters (with codebase_scope) must use the system tenant
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("system")
        .adapter_id("adapter-self")
        .name("Self Validation Adapter")
        .hash_b3("b3:self")
        .rank(8)
        .tier("persistent")
        .repo_id(Some("github.com/org/self-repo"))
        .codebase_scope(Some("self/scope"))
        .build()
        .unwrap();

    db.register_adapter(params).await.unwrap();

    // Set adapter to active state for testing
    set_adapter_active_for_test(&db, "adapter-self").await;

    // Validate the SAME adapter (re-activation scenario)
    let result = db
        .validate_active_uniqueness(
            "adapter-self", // Same adapter ID
            Some("github.com/org/self-repo".to_string()),
            None,
            Some("self/scope".to_string()),
            None,
        )
        .await
        .unwrap();

    assert!(
        result.is_valid,
        "Should allow self-validation without conflict: {:?}",
        result.conflict_reason
    );
    assert!(
        result.conflicting_adapters.is_empty(),
        "Should not report self as conflicting"
    );
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
    let code_adapters = db
        .list_adapters_by_category("default-tenant", "code")
        .await
        .unwrap();
    assert_eq!(code_adapters.len(), 1);
    assert_eq!(code_adapters[0].adapter_id.as_deref(), Some("nlp-adapter"));
    assert_eq!(code_adapters[0].load_state, "cold");

    // Test scope filtering
    let global_adapters = db
        .list_adapters_by_scope("default-tenant", "global")
        .await
        .unwrap();
    assert_eq!(global_adapters.len(), 1);
    assert_eq!(
        global_adapters[0].adapter_id.as_deref(),
        Some("nlp-adapter")
    );
    assert_eq!(global_adapters[0].load_state, "cold");

    // Test state filtering
    let unloaded_adapters = db
        .list_adapters_by_state("default-tenant", "unloaded")
        .await
        .unwrap();
    assert_eq!(unloaded_adapters.len(), 2);
    for adapter in &unloaded_adapters {
        assert_eq!(adapter.load_state, "cold");
        // aos_file_path not in Adapter struct
    }
}
