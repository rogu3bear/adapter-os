/// Schema Consistency Tests
///
/// These tests verify that:
/// 1. Migration application completes successfully
/// 2. Adapter struct fields match database schema columns
/// 3. INSERT statements reference valid columns
/// 4. SELECT queries reference existing columns
///
/// Citation: Multi-agent schema audit - Phase 3 schema validation
/// Priority: CRITICAL - Prevents struct-schema drift
use adapteros_db::{adapters::AdapterRegistrationBuilder, Db};
use anyhow::Result;
use sqlx::Row;
use std::collections::HashSet;
use std::path::PathBuf;
use tempfile::NamedTempFile;

/// Helper to create an in-memory test database with all migrations applied
async fn create_test_db() -> Result<Db> {
    let db = Db::new_in_memory().await?;
    // Seeding is required to satisfy foreign key constraints (e.g., tenants)
    db.seed_dev_data().await?;
    Ok(db)
}

fn new_test_aos_file() -> NamedTempFile {
    tempfile::Builder::new()
        .prefix("aos-test-")
        .suffix(".aos")
        .tempfile()
        .expect("Failed to create temp .aos file")
}

/// Test 1: Verify that all migrations apply successfully
#[tokio::test]
async fn test_migration_application() -> Result<()> {
    let db = create_test_db().await?;

    // Query the migrations table to verify migrations were applied
    // Note: Switched from refinery to sqlx migrations
    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(db.pool_result()?)
        .await
        .unwrap_or(0);

    // We should have at least 119 migrations (0001-0119)
    assert!(
        migration_count >= 119,
        "Expected at least 119 migrations, found {}",
        migration_count
    );

    println!("✓ All {} migrations applied successfully", migration_count);
    Ok(())
}

/// Test 2: Verify Adapter struct fields have corresponding database columns
#[tokio::test]
async fn test_adapter_struct_columns_exist() -> Result<()> {
    let db = create_test_db().await?;

    // Get all column names from adapters table
    let rows = sqlx::query("PRAGMA table_info(adapters)")
        .fetch_all(db.pool_result()?)
        .await?;

    let mut db_columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1); // column name is at index 1
        db_columns.insert(col_name);
    }

    // Required Adapter struct fields (from adapters.rs:219-275)
    let required_fields = vec![
        // Core fields (migration 0001)
        "id",
        "tenant_id",
        "name",
        "tier",
        "hash_b3",
        "rank",
        "alpha",
        "targets_json",
        "acl_json",
        "adapter_id",
        "languages_json",
        "framework",
        "active",
        // Code intelligence (migration 0012)
        "category",
        "scope",
        "framework_id",
        "framework_version",
        "repo_id",
        "commit_sha",
        "intent",
        // Lifecycle state (migration 0012)
        "current_state",
        "pinned",
        "memory_bytes",
        "last_activated",
        "activation_count",
        // Expiration (migration 0044)
        "expires_at",
        // Runtime load state (migration 0031)
        "load_state",
        "last_loaded_at",
        // .aos file support (migration 0045)
        "aos_file_path",
        "aos_file_hash",
        // Semantic naming (migration 0061)
        "adapter_name",
        "tenant_namespace",
        "domain",
        "purpose",
        "revision",
        "parent_id",
        "fork_type",
        "fork_reason",
        // Timestamps
        "created_at",
        "updated_at",
    ];

    let mut missing_columns = Vec::new();
    for field in &required_fields {
        if !db_columns.contains(*field) {
            missing_columns.push(*field);
        }
    }

    if !missing_columns.is_empty() {
        panic!(
            "Adapter struct fields missing from database schema: {:?}",
            missing_columns
        );
    }

    println!(
        "✓ All {} Adapter struct fields have corresponding database columns",
        required_fields.len()
    );
    Ok(())
}

/// Test 3: Verify INSERT statement in register_adapter_extended matches schema
#[tokio::test]
async fn test_adapter_insert_statement_valid() -> Result<()> {
    let db = create_test_db().await?;
    let temp_aos = new_test_aos_file();
    let aos_path_str = temp_aos
        .path()
        .to_str()
        .expect("Invalid temp .aos path")
        .to_string();

    // Create a test adapter with all fields populated
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default") // Must match seeded tenant
        .adapter_id("test-adapter-001")
        .name("Test Adapter")
        .hash_b3("b3:test123")
        .rank(8)
        .tier("warm")
        .alpha(16.0)
        .category("code")
        .scope("global")
        .aos_file_path(Some(aos_path_str.clone())) // Set explicit path for validation
        .adapter_name(Some("test/code/review/r001")) // Set explicit semantic name
        .build()?;

    // This will fail if INSERT statement references non-existent columns
    let adapter_id = db.register_adapter(params).await?;

    assert!(!adapter_id.is_empty(), "Adapter ID should not be empty");

    // Verify the adapter was actually inserted with all fields
    let row = sqlx::query(
        "SELECT id, aos_file_path, aos_file_hash, adapter_name, tenant_namespace, domain, purpose, revision
         FROM adapters WHERE id = ?",
    )
    .bind(&adapter_id)
    .fetch_one(db.pool_result()?)
    .await?;

    let stored_aos_path: Option<String> = row.get("aos_file_path");
    let adapter_name: Option<String> = row.get("adapter_name");

    let normalize_path = |value: &str| {
        std::fs::canonicalize(value)
            .unwrap_or_else(|_| PathBuf::from(value))
            .to_string_lossy()
            .to_string()
    };
    let expected_path = normalize_path(&aos_path_str);
    let actual_path = stored_aos_path.as_deref().map(normalize_path);

    assert_eq!(actual_path, Some(expected_path));
    assert_eq!(adapter_name, Some("test/code/review/r001".to_string()));

    println!("✓ INSERT statement successfully populates all schema columns");
    Ok(())
}

/// Test 4: Verify SELECT queries in list_adapters and find_expired_adapters reference valid columns
#[tokio::test]
async fn test_adapter_select_queries_valid() -> Result<()> {
    let db = create_test_db().await?;

    // Create a test adapter to ensure there's data to query
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default") // Must match seeded tenant
        .adapter_id("test-adapter-002")
        .name("Test Adapter 2")
        .hash_b3("b3:test456")
        .rank(4)
        .build()?;

    db.register_adapter(params).await?;

    // Test list_all_adapters_system query (this will fail if SELECT references non-existent columns)
    let adapters = db.list_all_adapters_system().await?;
    assert!(!adapters.is_empty(), "Should have at least one adapter");

    // Verify all expected fields are populated
    let adapter = &adapters[0];
    assert!(!adapter.id.is_empty());
    assert!(!adapter.name.is_empty());
    assert!(!adapter.hash_b3.is_empty());

    println!("✓ list_adapters SELECT query references valid columns");

    // Test find_expired_adapters query
    // This won't return results but will fail if query is malformed
    let expired = db.find_expired_adapters().await?;
    // Should be empty since we didn't set expires_at
    assert!(expired.is_empty());

    println!("✓ find_expired_adapters SELECT query references valid columns");
    Ok(())
}

/// Test 5: Verify taxonomy validation triggers work correctly
#[tokio::test]
async fn test_taxonomy_validation() -> Result<()> {
    let db = create_test_db().await?;

    // Test valid semantic name format: {tenant}/{domain}/{purpose}/{revision}
    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default") // Must match seeded tenant
        .adapter_id("test-adapter-003")
        .name("Test Adapter 3")
        .hash_b3("b3:test789")
        .rank(8)
        .adapter_name(Some("tenant-a/engineering/code-review/r001"))
        .tenant_namespace(Some("tenant-a"))
        .domain(Some("engineering"))
        .purpose(Some("code-review"))
        .revision(Some("r001"))
        .build()?;

    let id = db.register_adapter(params).await?;

    // Verify the semantic name was stored correctly
    let row = sqlx::query("SELECT adapter_name, tenant_namespace, domain, purpose, revision FROM adapters WHERE id = ?")
        .bind(&id)
        .fetch_one(db.pool_result()?)
        .await?;

    let adapter_name: Option<String> = row.get("adapter_name");
    let tenant_namespace: Option<String> = row.get("tenant_namespace");
    let domain: Option<String> = row.get("domain");
    let purpose: Option<String> = row.get("purpose");
    let revision: Option<String> = row.get("revision");

    assert_eq!(
        adapter_name,
        Some("tenant-a/engineering/code-review/r001".to_string())
    );
    assert_eq!(tenant_namespace, Some("tenant-a".to_string()));
    assert_eq!(domain, Some("engineering".to_string()));
    assert_eq!(purpose, Some("code-review".to_string()));
    assert_eq!(revision, Some("r001".to_string()));

    println!("✓ Taxonomy fields stored and retrieved correctly");
    Ok(())
}

/// Test 6: Verify .aos file metadata is stored correctly
#[tokio::test]
async fn test_aos_file_metadata_storage() -> Result<()> {
    let db = create_test_db().await?;
    let temp_aos = new_test_aos_file();
    let aos_path_str = temp_aos
        .path()
        .to_str()
        .expect("Invalid temp .aos path")
        .to_string();

    let params = AdapterRegistrationBuilder::new()
        .tenant_id("default") // Must match seeded tenant
        .adapter_id("test-adapter-004")
        .name("Test Adapter 4")
        .hash_b3("b3:test012")
        .rank(16)
        .aos_file_path(Some(aos_path_str.clone()))
        .aos_file_hash(Some("b3:aosfilehash123"))
        .build()?;

    let id = db.register_adapter(params).await?;

    // Verify .aos metadata was stored
    let row = sqlx::query("SELECT aos_file_path, aos_file_hash FROM adapters WHERE id = ?")
        .bind(&id)
        .fetch_one(db.pool_result()?)
        .await?;

    let stored_aos_path: Option<String> = row.get("aos_file_path");
    let aos_hash: Option<String> = row.get("aos_file_hash");

    let normalize_path = |value: &str| {
        std::fs::canonicalize(value)
            .unwrap_or_else(|_| PathBuf::from(value))
            .to_string_lossy()
            .to_string()
    };
    let expected_path = normalize_path(&aos_path_str);
    let actual_path = stored_aos_path.as_deref().map(normalize_path);

    assert_eq!(actual_path, Some(expected_path));
    assert_eq!(aos_hash, Some("b3:aosfilehash123".to_string()));

    println!("✓ .aos file metadata stored correctly");
    Ok(())
}

/// Test 7: Verify pinned_adapters table exists and works correctly
#[tokio::test]
async fn test_pinned_adapters_table_exists() -> Result<()> {
    let db = create_test_db().await?;

    // Verify table exists by querying it
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pinned_adapters")
        .fetch_one(db.pool_result()?)
        .await?;

    assert_eq!(count, 0, "Initially should have no pinned adapters");

    // Verify the view exists
    let view_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM active_pinned_adapters")
        .fetch_one(db.pool_result()?)
        .await?;

    assert_eq!(view_count, 0, "Initially should have no active pins");

    println!("✓ pinned_adapters table and view exist");
    Ok(())
}

/// Test 8: Verify tick_ledger_entries has federation columns
#[tokio::test]
async fn test_tick_ledger_federation_columns() -> Result<()> {
    let db = create_test_db().await?;

    // Get column info for tick_ledger_entries
    let rows = sqlx::query("PRAGMA table_info(tick_ledger_entries)")
        .fetch_all(db.pool_result()?)
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    // Verify federation columns exist (migration 0035)
    let federation_columns = vec!["bundle_hash", "prev_host_hash", "federation_signature"];

    for col in &federation_columns {
        assert!(
            columns.contains(*col),
            "Federation column '{}' missing from tick_ledger_entries",
            col
        );
    }

    println!("✓ Federation columns exist in tick_ledger_entries (reserved for future use)");
    Ok(())
}
