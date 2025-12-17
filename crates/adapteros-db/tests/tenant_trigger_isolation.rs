//! PRD-RECT-004: DB Trigger Tenant Isolation Revalidation Tests
//!
//! These tests validate that SQLite triggers properly enforce tenant isolation
//! at the database level, preventing cross-tenant references.

use adapteros_db::Db;
use sqlx::Row;

/// Create test tenants
async fn setup_tenants(db: &Db) -> (String, String) {
    let tenant_a = "tenant-a-isolation-test";
    let tenant_b = "tenant-b-isolation-test";

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_a)
        .bind("Tenant A")
        .execute(db.pool())
        .await
        .expect("create tenant A");

    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_b)
        .bind("Tenant B")
        .execute(db.pool())
        .await
        .expect("create tenant B");

    (tenant_a.to_string(), tenant_b.to_string())
}

/// Create a test repository for a tenant
async fn create_test_repo(db: &Db, tenant_id: &str, name: &str) -> String {
    let repo_id = format!("repo-{}-{}", tenant_id, uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name, default_branch) VALUES (?, ?, ?, ?)",
    )
    .bind(&repo_id)
    .bind(tenant_id)
    .bind(name)
    .bind("main")
    .execute(db.pool())
    .await
    .expect("create repo");

    repo_id
}

/// Create a valid adapter version for a tenant's repository
async fn create_test_version(
    db: &Db,
    repo_id: &str,
    tenant_id: &str,
    version: &str,
) -> Result<String, sqlx::Error> {
    let version_id = format!("ver-{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO adapter_versions (id, repo_id, tenant_id, version, branch, branch_classification, release_state)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&version_id)
    .bind(repo_id)
    .bind(tenant_id)
    .bind(version)
    .bind("main")
    .bind("protected")
    .bind("draft")
    .execute(db.pool())
    .await?;

    Ok(version_id)
}

// ============================================================================
// PRD-RECT-004: Trigger Validation Tests
// ============================================================================

#[tokio::test]
async fn trigger_rejects_cross_tenant_version_insert() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create repository in tenant A
    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;

    // Attempt to create version in tenant B that references repo from tenant A
    // This should fail due to trigger trg_adapter_versions_repo_tenant_match_insert
    let result = create_test_version(&db, &repo_a, &tenant_b, "1.0.0").await;

    assert!(
        result.is_err(),
        "Cross-tenant version insert should be rejected by trigger"
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Tenant mismatch") || err.contains("tenant_id must match"),
        "Error should mention tenant mismatch: {}",
        err
    );
}

#[tokio::test]
async fn trigger_allows_same_tenant_version_insert() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, _) = setup_tenants(&db).await;

    // Create repository in tenant A
    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;

    // Create version in tenant A referencing repo from tenant A - should succeed
    let result = create_test_version(&db, &repo_a, &tenant_a, "1.0.0").await;

    assert!(
        result.is_ok(),
        "Same-tenant version insert should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn trigger_rejects_cross_tenant_repo_id_update() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create repositories in both tenants
    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;
    let repo_b = create_test_repo(&db, &tenant_b, "Repo B").await;

    // Create valid version in tenant A
    let version_id = create_test_version(&db, &repo_a, &tenant_a, "1.0.0")
        .await
        .expect("create version");

    // Attempt to update repo_id to point to repo in tenant B
    // This should fail due to trigger trg_adapter_versions_repo_tenant_match_update_repo
    let result = sqlx::query("UPDATE adapter_versions SET repo_id = ? WHERE id = ?")
        .bind(&repo_b)
        .bind(&version_id)
        .execute(db.pool())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant repo_id update should be rejected by trigger"
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Tenant mismatch") || err.contains("tenant_id must match"),
        "Error should mention tenant mismatch: {}",
        err
    );
}

#[tokio::test]
async fn trigger_rejects_cross_tenant_tenant_id_update() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create repository in tenant A
    let repo_a = create_test_repo(&db, &tenant_a, "Repo A").await;

    // Create valid version in tenant A
    let version_id = create_test_version(&db, &repo_a, &tenant_a, "1.0.0")
        .await
        .expect("create version");

    // Attempt to update tenant_id to tenant B while repo_id still points to tenant A's repo
    // This should fail due to trigger trg_adapter_versions_repo_tenant_match_update_tenant
    let result = sqlx::query("UPDATE adapter_versions SET tenant_id = ? WHERE id = ?")
        .bind(&tenant_b)
        .bind(&version_id)
        .execute(db.pool())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant tenant_id update should be rejected by trigger"
    );

    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("Tenant mismatch") || err.contains("tenant_id must match"),
        "Error should mention tenant mismatch: {}",
        err
    );
}

#[tokio::test]
async fn valid_same_tenant_operations_succeed() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, _) = setup_tenants(&db).await;

    // Create two repositories in tenant A
    let repo_a1 = create_test_repo(&db, &tenant_a, "Repo A1").await;
    let repo_a2 = create_test_repo(&db, &tenant_a, "Repo A2").await;

    // Create version in tenant A
    let version_id = create_test_version(&db, &repo_a1, &tenant_a, "1.0.0")
        .await
        .expect("create version");

    // Update repo_id to another repo within the same tenant - should succeed
    let result = sqlx::query("UPDATE adapter_versions SET repo_id = ? WHERE id = ?")
        .bind(&repo_a2)
        .bind(&version_id)
        .execute(db.pool())
        .await;

    assert!(
        result.is_ok(),
        "Same-tenant repo_id update should succeed: {:?}",
        result.err()
    );

    // Verify the update took effect
    let row = sqlx::query("SELECT repo_id FROM adapter_versions WHERE id = ?")
        .bind(&version_id)
        .fetch_one(db.pool())
        .await
        .expect("fetch version");

    let updated_repo_id: String = row.get("repo_id");
    assert_eq!(updated_repo_id, repo_a2);
}

#[tokio::test]
async fn trigger_test_isolation_is_not_vacuous() {
    // This test ensures our tests are meaningful by verifying
    // that the database actually has the triggers installed
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Check that the triggers exist
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'trg_adapter_versions_repo_tenant_match%'"
    )
    .fetch_all(db.pool())
    .await
    .expect("fetch triggers");

    assert!(
        triggers.len() >= 3,
        "Expected at least 3 tenant isolation triggers, found {}: {:?}",
        triggers.len(),
        triggers
    );

    // Verify specific trigger names
    assert!(
        triggers.iter().any(|t| t.contains("insert")),
        "Missing insert trigger"
    );
    assert!(
        triggers.iter().any(|t| t.contains("update_repo")),
        "Missing update_repo trigger"
    );
    assert!(
        triggers.iter().any(|t| t.contains("update_tenant")),
        "Missing update_tenant trigger"
    );
}

// ============================================================================
// PRD-RECT-004: Comprehensive Trigger Coverage Tests
// ============================================================================

/// Create a test dataset for a tenant
async fn create_test_dataset(db: &Db, tenant_id: &str, name: &str) -> String {
    let dataset_id = format!("ds-{}-{}", tenant_id, uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO training_datasets (id, tenant_id, name, format, hash_b3, storage_path, purpose) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&dataset_id)
    .bind(tenant_id)
    .bind(name)
    .bind("jsonl")
    .bind("0000000000000000000000000000000000000000000000000000000000000000")
    .bind("/tmp/test")
    .bind("training")
    .execute(db.pool())
    .await
    .expect("create dataset");

    dataset_id
}

/// Create a test adapter for a tenant
async fn create_test_adapter(db: &Db, tenant_id: &str, name: &str) -> String {
    let adapter_id = format!("adapter-{}-{}", tenant_id, uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO adapters (id, tenant_id, name, adapter_id, hash_b3, tier, rank, alpha, targets_json, lifecycle_state, active)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&adapter_id)
    .bind(tenant_id)
    .bind(name)
    .bind(&adapter_id)
    .bind("0000000000000000000000000000000000000000000000000000000000000000")
    .bind("persistent")
    .bind(16)
    .bind(1.0f64)
    .bind("[]")
    .bind("active")
    .bind(1)
    .execute(db.pool())
    .await
    .expect("create adapter");

    adapter_id
}

/// Create a test stack for a tenant
async fn create_test_stack(db: &Db, tenant_id: &str, name: &str) -> String {
    let stack_id = format!("stack-{}-{}", tenant_id, uuid::Uuid::new_v4());
    // Stack names must match format: stack.{namespace}[.{identifier}]
    let stack_name = format!("stack.test.{}", name.to_lowercase().replace(' ', "-"));

    sqlx::query(
        "INSERT INTO adapter_stacks (id, tenant_id, name, lifecycle_state, adapter_ids_json)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&stack_id)
    .bind(tenant_id)
    .bind(&stack_name)
    .bind("active")
    .bind("[]")
    .execute(db.pool())
    .await
    .expect("create stack");

    stack_id
}

/// Create a test collection for a tenant
async fn create_test_collection(db: &Db, tenant_id: &str, name: &str) -> String {
    let collection_id = format!("col-{}-{}", tenant_id, uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO document_collections (id, tenant_id, name)
         VALUES (?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(tenant_id)
    .bind(name)
    .execute(db.pool())
    .await
    .expect("create collection");

    collection_id
}

// ----------------------------------------------------------------------------
// Tests for adapters table triggers (primary_dataset_id, eval_dataset_id)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn trigger_rejects_adapter_cross_tenant_primary_dataset() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create adapter in tenant A and dataset in tenant B
    let adapter_id = create_test_adapter(&db, &tenant_a, "Adapter A").await;
    let dataset_b = create_test_dataset(&db, &tenant_b, "Dataset B").await;

    // Attempt to link adapter to cross-tenant dataset
    let result = sqlx::query("UPDATE adapters SET primary_dataset_id = ? WHERE id = ?")
        .bind(&dataset_b)
        .bind(&adapter_id)
        .execute(db.pool())
        .await;

    assert!(
        result.is_err(),
        "Cross-tenant primary_dataset_id should be rejected by trigger"
    );
}

#[tokio::test]
async fn trigger_allows_adapter_same_tenant_primary_dataset() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, _) = setup_tenants(&db).await;

    // Create adapter and dataset in same tenant
    let adapter_id = create_test_adapter(&db, &tenant_a, "Adapter A").await;
    let dataset_a = create_test_dataset(&db, &tenant_a, "Dataset A").await;

    // Link adapter to same-tenant dataset should succeed
    let result = sqlx::query("UPDATE adapters SET primary_dataset_id = ? WHERE id = ?")
        .bind(&dataset_a)
        .bind(&adapter_id)
        .execute(db.pool())
        .await;

    assert!(
        result.is_ok(),
        "Same-tenant primary_dataset_id should succeed: {:?}",
        result.err()
    );
}

// ----------------------------------------------------------------------------
// Tests for chat_sessions table triggers (stack_id, collection_id)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn trigger_rejects_chat_session_cross_tenant_stack() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create stack in tenant B
    let stack_b = create_test_stack(&db, &tenant_b, "Stack B").await;

    // Attempt to create chat session in tenant A with stack from tenant B
    let session_id = format!("session-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO chat_sessions (id, tenant_id, stack_id, name) VALUES (?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(&tenant_a)
    .bind(&stack_b)
    .bind("Test Session")
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant chat_sessions.stack_id should be rejected by trigger"
    );
}

#[tokio::test]
async fn trigger_allows_chat_session_same_tenant_stack() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, _) = setup_tenants(&db).await;

    // Create stack in same tenant
    let stack_a = create_test_stack(&db, &tenant_a, "Stack A").await;

    // Create chat session with same-tenant stack should succeed
    let session_id = format!("session-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO chat_sessions (id, tenant_id, stack_id, name) VALUES (?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(&tenant_a)
    .bind(&stack_a)
    .bind("Test Session")
    .execute(db.pool())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant chat_sessions.stack_id should succeed: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn trigger_rejects_chat_session_cross_tenant_collection() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create collection in tenant B
    let collection_b = create_test_collection(&db, &tenant_b, "Collection B").await;

    // Attempt to create chat session in tenant A with collection from tenant B
    let session_id = format!("session-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO chat_sessions (id, tenant_id, collection_id, name) VALUES (?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(&tenant_a)
    .bind(&collection_b)
    .bind("Test Session")
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant chat_sessions.collection_id should be rejected by trigger"
    );
}

// ----------------------------------------------------------------------------
// Tests for pinned_adapters table triggers
// ----------------------------------------------------------------------------

#[tokio::test]
async fn trigger_rejects_pinned_adapter_cross_tenant() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, tenant_b) = setup_tenants(&db).await;

    // Create adapter in tenant B
    let adapter_b = create_test_adapter(&db, &tenant_b, "Adapter B").await;

    // Attempt to create pinned_adapter in tenant A referencing adapter from tenant B
    // adapter_pk is TEXT and references adapters.id directly
    let pinned_id = format!("pinned-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO pinned_adapters (id, tenant_id, adapter_pk, pinned_by) VALUES (?, ?, ?, ?)",
    )
    .bind(&pinned_id)
    .bind(&tenant_a)
    .bind(&adapter_b) // Use adapter ID directly (TEXT, not rowid)
    .bind("test-user")
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant pinned_adapters should be rejected by trigger"
    );
}

#[tokio::test]
async fn trigger_allows_pinned_adapter_same_tenant() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");
    let (tenant_a, _) = setup_tenants(&db).await;

    // Create adapter in same tenant
    let adapter_a = create_test_adapter(&db, &tenant_a, "Adapter A").await;

    // Create pinned_adapter with same-tenant adapter should succeed
    // adapter_pk is TEXT and references adapters.id directly
    let pinned_id = format!("pinned-{}", uuid::Uuid::new_v4());
    let result = sqlx::query(
        "INSERT INTO pinned_adapters (id, tenant_id, adapter_pk, pinned_by) VALUES (?, ?, ?, ?)",
    )
    .bind(&pinned_id)
    .bind(&tenant_a)
    .bind(&adapter_a) // Use adapter ID directly (TEXT, not rowid)
    .bind("test-user")
    .execute(db.pool())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant pinned_adapters should succeed: {:?}",
        result.err()
    );
}

// ----------------------------------------------------------------------------
// Non-Vacuity Test: Verify all 0131 triggers exist
// ----------------------------------------------------------------------------

#[tokio::test]
async fn all_0131_tenant_triggers_exist() {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");
    let db = Db::new_in_memory().await.expect("db");

    // Get all tenant-related triggers
    let triggers: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'trigger' AND name LIKE 'trg_%tenant%'"
    )
    .fetch_all(db.pool())
    .await
    .expect("fetch triggers");

    // Expected minimum trigger count from 0131 + later migrations
    // This ensures we haven't accidentally dropped triggers
    assert!(
        triggers.len() >= 15,
        "Expected at least 15 tenant isolation triggers, found {}: {:?}",
        triggers.len(),
        triggers
    );

    // Spot check for key trigger categories
    let trigger_str = triggers.join(",");

    // adapters table triggers
    assert!(
        trigger_str.contains("adapters") || trigger_str.contains("adapter"),
        "Missing adapters table triggers"
    );

    // chat_sessions table triggers
    assert!(
        trigger_str.contains("chat_session"),
        "Missing chat_sessions table triggers"
    );

    // pinned_adapters table triggers
    assert!(
        trigger_str.contains("pinned"),
        "Missing pinned_adapters table triggers"
    );
}
