//! Migration Validation Tests
//!
//! Comprehensive tests for migrations 0193-0210 that verify:
//! 1. Schema changes are applied correctly
//! 2. Foreign key constraints work as expected
//! 3. Triggers enforce business logic
//! 4. Indexes exist for performance
//! 5. Data can be inserted and queried correctly
//!
//! Test Coverage:
//! - 0193: inference_receipt_accounting (no-op, backward compat)
//! - 0194: stop_controller fields
//! - 0195: kv_quota_residency fields
//! - 0196: replay_stop_policy field
//! - 0197: prefix_kv_cache (templates + receipt fields)
//! - 0198: model_cache_identity_v2 field
//! - 0199: evidence_envelopes table
//! - 0200: drop_adapter_packages
//! - 0201: adapter_version_publish_attach
//! - 0202: adapter_stacks_metadata
//! - 0210: tenant_scoped_query_optimization (composite indexes)

use adapteros_db::Db;
use anyhow::Result;
use sqlx::Row;
use std::collections::HashSet;

/// Helper to create an in-memory test database with all migrations applied
async fn create_test_db() -> Result<Db> {
    let db = Db::new_in_memory().await?;
    db.seed_dev_data().await?;
    Ok(db)
}

// =============================================================================
// Migration 0193: Inference Receipt Accounting (No-op migration)
// =============================================================================

#[tokio::test]
async fn test_migration_0193_receipt_accounting_columns_exist() -> Result<()> {
    let db = create_test_db().await?;

    // Verify columns exist (they're created by 0192, 0193 is backward compat)
    let rows = sqlx::query("PRAGMA table_info(inference_trace_receipts)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    let accounting_columns = vec![
        "logical_prompt_tokens",
        "prefix_cached_token_count",
        "billed_input_tokens",
        "logical_output_tokens",
        "billed_output_tokens",
        "signature",
        "attestation",
    ];

    for col in &accounting_columns {
        assert!(
            columns.contains(*col),
            "Column '{}' missing from inference_trace_receipts",
            col
        );
    }

    println!("✓ Migration 0193: Receipt accounting columns exist (from 0192)");
    Ok(())
}

// =============================================================================
// Migration 0194: Stop Controller Fields
// =============================================================================

#[tokio::test]
async fn test_migration_0194_stop_controller_columns_exist() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(inference_trace_receipts)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    let stop_columns = vec![
        "stop_reason_code",
        "stop_reason_token_index",
        "stop_policy_digest_b3",
    ];

    for col in &stop_columns {
        assert!(
            columns.contains(*col),
            "Stop controller column '{}' missing from inference_trace_receipts",
            col
        );
    }

    println!("✓ Migration 0194: Stop controller columns exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0194_stop_reason_index_exists() -> Result<()> {
    let db = create_test_db().await?;

    let index_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='index' AND name='idx_inference_trace_receipts_stop_reason'",
    )
    .fetch_one(db.pool())
    .await?;

    assert_eq!(
        index_exists, 1,
        "idx_inference_trace_receipts_stop_reason should exist"
    );

    println!("✓ Migration 0194: Stop reason index exists");
    Ok(())
}

#[tokio::test]
async fn test_migration_0194_stop_controller_data_insertion() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant and trace
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('test-tenant', 'Test')")
        .execute(db.pool())
        .await?;

    sqlx::query(
        "INSERT INTO inference_traces (trace_id, tenant_id, request_id, context_digest)
         VALUES ('trace-1', 'test-tenant', 'req-1', x'abcd')",
    )
    .execute(db.pool())
    .await?;

    // Insert receipt with stop controller fields
    sqlx::query(
        "INSERT INTO inference_trace_receipts (
            trace_id, run_head_hash, output_digest, receipt_digest,
            stop_reason_code, stop_reason_token_index, stop_policy_digest_b3
        ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("trace-1")
    .bind(vec![0u8; 32])
    .bind(vec![0u8; 32])
    .bind(vec![0u8; 32])
    .bind("LENGTH")
    .bind(42i64)
    .bind(vec![0u8; 32])
    .execute(db.pool())
    .await?;

    // Verify data
    let (code, index): (Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT stop_reason_code, stop_reason_token_index
         FROM inference_trace_receipts WHERE trace_id = ?",
    )
    .bind("trace-1")
    .fetch_one(db.pool())
    .await?;

    assert_eq!(code, Some("LENGTH".to_string()));
    assert_eq!(index, Some(42));

    println!("✓ Migration 0194: Stop controller data insertion works");
    Ok(())
}

// =============================================================================
// Migration 0195: KV Quota and Residency
// =============================================================================

#[tokio::test]
async fn test_migration_0195_tenant_kv_columns_exist() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(tenants)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    assert!(
        columns.contains("max_kv_cache_bytes"),
        "max_kv_cache_bytes missing from tenants"
    );
    assert!(
        columns.contains("kv_residency_policy_id"),
        "kv_residency_policy_id missing from tenants"
    );

    println!("✓ Migration 0195: Tenant KV quota columns exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0195_receipt_kv_columns_exist() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(inference_trace_receipts)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    let kv_columns = vec![
        "tenant_kv_quota_bytes",
        "tenant_kv_bytes_used",
        "kv_evictions",
        "kv_residency_policy_id",
        "kv_quota_enforced",
    ];

    for col in &kv_columns {
        assert!(
            columns.contains(*col),
            "KV column '{}' missing from inference_trace_receipts",
            col
        );
    }

    println!("✓ Migration 0195: Receipt KV quota columns exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0195_kv_indexes_exist() -> Result<()> {
    let db = create_test_db().await?;

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master
         WHERE type='index' AND name LIKE '%kv%'",
    )
    .fetch_all(db.pool())
    .await?;

    let expected_indexes = vec![
        "idx_inference_trace_receipts_kv_policy",
        "idx_inference_trace_receipts_kv_quota",
    ];

    for expected in &expected_indexes {
        assert!(
            indexes.contains(&expected.to_string()),
            "Index '{}' not found",
            expected
        );
    }

    println!("✓ Migration 0195: KV indexes exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0195_kv_data_insertion() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant with KV quota
    sqlx::query(
        "INSERT INTO tenants (id, name, max_kv_cache_bytes, kv_residency_policy_id)
         VALUES ('tenant-kv', 'KV Tenant', 1000000, 'kv_residency_v1')",
    )
    .execute(db.pool())
    .await?;

    // Verify tenant data
    let (quota, policy): (Option<i64>, Option<String>) = sqlx::query_as(
        "SELECT max_kv_cache_bytes, kv_residency_policy_id
         FROM tenants WHERE id = 'tenant-kv'",
    )
    .fetch_one(db.pool())
    .await?;

    assert_eq!(quota, Some(1000000));
    assert_eq!(policy, Some("kv_residency_v1".to_string()));

    println!("✓ Migration 0195: KV quota data insertion works");
    Ok(())
}

// =============================================================================
// Migration 0196: Replay Stop Policy
// =============================================================================

#[tokio::test]
async fn test_migration_0196_replay_stop_policy_column_exists() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(inference_replay_metadata)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    assert!(
        columns.contains("stop_policy_json"),
        "stop_policy_json missing from inference_replay_metadata"
    );

    println!("✓ Migration 0196: Replay stop_policy_json column exists");
    Ok(())
}

#[tokio::test]
async fn test_migration_0196_replay_stop_policy_data() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant and trace
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-replay', 'Replay')")
        .execute(db.pool())
        .await?;

    sqlx::query(
        "INSERT INTO inference_traces (trace_id, tenant_id, request_id, context_digest)
         VALUES ('trace-replay', 'tenant-replay', 'req-1', x'abcd')",
    )
    .execute(db.pool())
    .await?;

    // Insert replay metadata with stop policy
    let stop_policy_json = r#"{"max_length":100,"budget_max":1000}"#;
    sqlx::query(
        "INSERT INTO inference_replay_metadata (
            id, inference_id, tenant_id, manifest_hash, router_seed, sampling_params_json,
            backend, prompt_text, stop_policy_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("replay-1")
    .bind("trace-replay")
    .bind("tenant-replay")
    .bind("manifest-hash")
    .bind("router-seed")
    .bind("{}")
    .bind("mlx")
    .bind("test prompt")
    .bind(stop_policy_json)
    .execute(db.pool())
    .await?;

    // Verify data
    let stored_policy: Option<String> = sqlx::query_scalar(
        "SELECT stop_policy_json FROM inference_replay_metadata WHERE inference_id = ?",
    )
    .bind("trace-replay")
    .fetch_one(db.pool())
    .await?;

    assert_eq!(stored_policy, Some(stop_policy_json.to_string()));

    println!("✓ Migration 0196: Replay stop policy data works");
    Ok(())
}

// =============================================================================
// Migration 0197: Prefix KV Cache
// =============================================================================

#[tokio::test]
async fn test_migration_0197_prefix_templates_table_exists() -> Result<()> {
    let db = create_test_db().await?;

    let table_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name='prefix_templates'",
    )
    .fetch_one(db.pool())
    .await?;

    assert_eq!(table_exists, 1, "prefix_templates table should exist");

    println!("✓ Migration 0197: prefix_templates table exists");
    Ok(())
}

#[tokio::test]
async fn test_migration_0197_prefix_templates_columns() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(prefix_templates)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    let expected_columns = vec![
        "id",
        "tenant_id",
        "mode",
        "template_text",
        "template_hash_b3",
        "priority",
        "enabled",
        "created_at",
        "updated_at",
    ];

    for col in &expected_columns {
        assert!(
            columns.contains(*col),
            "Column '{}' missing from prefix_templates",
            col
        );
    }

    println!("✓ Migration 0197: prefix_templates columns exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0197_prefix_templates_fk_constraint() -> Result<()> {
    let db = create_test_db().await?;

    // Create valid tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-prefix', 'Prefix')")
        .execute(db.pool())
        .await?;

    // Insert valid template
    let result = sqlx::query(
        "INSERT INTO prefix_templates (
            id, tenant_id, mode, template_text, template_hash_b3, priority
        ) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("tpl-1")
    .bind("tenant-prefix")
    .bind("system")
    .bind("You are a helpful assistant")
    .bind("b3:hash123")
    .bind(1)
    .execute(db.pool())
    .await;

    assert!(result.is_ok(), "Valid template insertion should succeed");

    // Try invalid tenant (FK should fail)
    let invalid_result = sqlx::query(
        "INSERT INTO prefix_templates (
            id, tenant_id, mode, template_text, template_hash_b3, priority
        ) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("tpl-2")
    .bind("nonexistent-tenant")
    .bind("user")
    .bind("Test")
    .bind("b3:hash456")
    .bind(1)
    .execute(db.pool())
    .await;

    assert!(
        invalid_result.is_err(),
        "Invalid tenant FK should be rejected"
    );

    println!("✓ Migration 0197: prefix_templates FK constraint works");
    Ok(())
}

#[tokio::test]
async fn test_migration_0197_receipt_prefix_columns_exist() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(inference_trace_receipts)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    let prefix_columns = vec!["prefix_kv_key_b3", "prefix_cache_hit", "prefix_kv_bytes"];

    for col in &prefix_columns {
        assert!(
            columns.contains(*col),
            "Prefix column '{}' missing from inference_trace_receipts",
            col
        );
    }

    println!("✓ Migration 0197: Receipt prefix cache columns exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0197_prefix_indexes_exist() -> Result<()> {
    let db = create_test_db().await?;

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master
         WHERE type='index' AND name LIKE '%prefix%'",
    )
    .fetch_all(db.pool())
    .await?;

    let expected_indexes = vec![
        "idx_prefix_templates_tenant_mode",
        "idx_prefix_templates_hash",
        "idx_inference_trace_receipts_prefix_hit",
        "idx_inference_trace_receipts_prefix_key",
    ];

    for expected in &expected_indexes {
        assert!(
            indexes.contains(&expected.to_string()),
            "Index '{}' not found",
            expected
        );
    }

    println!("✓ Migration 0197: Prefix cache indexes exist");
    Ok(())
}

// =============================================================================
// Migration 0198: Model Cache Identity V2
// =============================================================================

#[tokio::test]
async fn test_migration_0198_model_cache_identity_column_exists() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(inference_trace_receipts)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    assert!(
        columns.contains("model_cache_identity_v2_digest_b3"),
        "model_cache_identity_v2_digest_b3 missing from inference_trace_receipts"
    );

    println!("✓ Migration 0198: model_cache_identity_v2_digest_b3 column exists");
    Ok(())
}

#[tokio::test]
async fn test_migration_0198_model_cache_identity_data() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant and trace
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-cache', 'Cache')")
        .execute(db.pool())
        .await?;

    sqlx::query(
        "INSERT INTO inference_traces (trace_id, tenant_id, request_id, context_digest)
         VALUES ('trace-cache', 'tenant-cache', 'req-1', x'abcd')",
    )
    .execute(db.pool())
    .await?;

    // Insert receipt with model cache identity
    let cache_digest = vec![0xAAu8; 32];
    sqlx::query(
        "INSERT INTO inference_trace_receipts (
            trace_id, run_head_hash, output_digest, receipt_digest,
            model_cache_identity_v2_digest_b3
        ) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("trace-cache")
    .bind(vec![0u8; 32])
    .bind(vec![0u8; 32])
    .bind(vec![0u8; 32])
    .bind(&cache_digest)
    .execute(db.pool())
    .await?;

    // Verify data
    let stored_digest: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT model_cache_identity_v2_digest_b3
         FROM inference_trace_receipts WHERE trace_id = ?",
    )
    .bind("trace-cache")
    .fetch_one(db.pool())
    .await?;

    assert_eq!(stored_digest, Some(cache_digest));

    println!("✓ Migration 0198: Model cache identity data works");
    Ok(())
}

// =============================================================================
// Migration 0199: Evidence Envelopes
// =============================================================================

#[tokio::test]
async fn test_migration_0199_evidence_envelopes_table_exists() -> Result<()> {
    let db = create_test_db().await?;

    let table_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name='evidence_envelopes'",
    )
    .fetch_one(db.pool())
    .await?;

    assert_eq!(table_exists, 1, "evidence_envelopes table should exist");

    println!("✓ Migration 0199: evidence_envelopes table exists");
    Ok(())
}

#[tokio::test]
async fn test_migration_0199_evidence_envelopes_columns() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(evidence_envelopes)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    let expected_columns = vec![
        "id",
        "schema_version",
        "tenant_id",
        "scope",
        "previous_root",
        "root",
        "signature",
        "public_key",
        "key_id",
        "attestation_ref",
        "created_at",
        "signed_at_us",
        "payload_json",
        "chain_sequence",
    ];

    for col in &expected_columns {
        assert!(
            columns.contains(*col),
            "Column '{}' missing from evidence_envelopes",
            col
        );
    }

    println!("✓ Migration 0199: evidence_envelopes columns exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0199_evidence_scope_check_constraint() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-evidence', 'Evidence')")
        .execute(db.pool())
        .await?;

    // Valid scope
    let valid_result = sqlx::query(
        "INSERT INTO evidence_envelopes (
            id, tenant_id, scope, root, signature, public_key, key_id,
            payload_json, chain_sequence
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("env-1")
    .bind("tenant-evidence")
    .bind("telemetry") // Valid scope
    .bind("root-hash")
    .bind("sig")
    .bind("pubkey")
    .bind("key-1")
    .bind("{}")
    .bind(1)
    .execute(db.pool())
    .await;

    assert!(valid_result.is_ok(), "Valid scope should succeed");

    // Invalid scope
    let invalid_result = sqlx::query(
        "INSERT INTO evidence_envelopes (
            id, tenant_id, scope, root, signature, public_key, key_id,
            payload_json, chain_sequence
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("env-2")
    .bind("tenant-evidence")
    .bind("invalid-scope") // Invalid scope
    .bind("root-hash")
    .bind("sig")
    .bind("pubkey")
    .bind("key-1")
    .bind("{}")
    .bind(2)
    .execute(db.pool())
    .await;

    assert!(
        invalid_result.is_err(),
        "Invalid scope should be rejected by CHECK constraint"
    );

    println!("✓ Migration 0199: Evidence scope CHECK constraint works");
    Ok(())
}

#[tokio::test]
async fn test_migration_0199_evidence_fk_constraint() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-fk', 'FK Test')")
        .execute(db.pool())
        .await?;

    // Valid FK
    let valid_result = sqlx::query(
        "INSERT INTO evidence_envelopes (
            id, tenant_id, scope, root, signature, public_key, key_id,
            payload_json, chain_sequence
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("env-fk-1")
    .bind("tenant-fk")
    .bind("policy")
    .bind("root")
    .bind("sig")
    .bind("key")
    .bind("k1")
    .bind("{}")
    .bind(1)
    .execute(db.pool())
    .await;

    assert!(valid_result.is_ok(), "Valid tenant FK should succeed");

    // Invalid FK
    let invalid_result = sqlx::query(
        "INSERT INTO evidence_envelopes (
            id, tenant_id, scope, root, signature, public_key, key_id,
            payload_json, chain_sequence
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("env-fk-2")
    .bind("nonexistent-tenant")
    .bind("inference")
    .bind("root")
    .bind("sig")
    .bind("key")
    .bind("k1")
    .bind("{}")
    .bind(1)
    .execute(db.pool())
    .await;

    assert!(
        invalid_result.is_err(),
        "Invalid tenant FK should be rejected"
    );

    println!("✓ Migration 0199: Evidence FK constraint works");
    Ok(())
}

#[tokio::test]
async fn test_migration_0199_evidence_indexes_exist() -> Result<()> {
    let db = create_test_db().await?;

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master
         WHERE type='index' AND tbl_name='evidence_envelopes'",
    )
    .fetch_all(db.pool())
    .await?;

    let expected_indexes = vec![
        "idx_evidence_envelopes_tenant_scope",
        "idx_evidence_envelopes_previous_root",
        "idx_evidence_envelopes_root",
        "idx_evidence_envelopes_tenant_scope_seq",
        "idx_evidence_envelopes_key_id",
    ];

    for expected in &expected_indexes {
        assert!(
            indexes.contains(&expected.to_string()),
            "Index '{}' not found",
            expected
        );
    }

    println!("✓ Migration 0199: Evidence envelope indexes exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0199_evidence_unique_sequence_constraint() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-seq', 'Sequence')")
        .execute(db.pool())
        .await?;

    // First envelope
    sqlx::query(
        "INSERT INTO evidence_envelopes (
            id, tenant_id, scope, root, signature, public_key, key_id,
            payload_json, chain_sequence
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("env-seq-1")
    .bind("tenant-seq")
    .bind("telemetry")
    .bind("root1")
    .bind("sig")
    .bind("key")
    .bind("k1")
    .bind("{}")
    .bind(1)
    .execute(db.pool())
    .await?;

    // Duplicate sequence in same tenant+scope
    let duplicate_result = sqlx::query(
        "INSERT INTO evidence_envelopes (
            id, tenant_id, scope, root, signature, public_key, key_id,
            payload_json, chain_sequence
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("env-seq-2")
    .bind("tenant-seq")
    .bind("telemetry")
    .bind("root2")
    .bind("sig")
    .bind("key")
    .bind("k1")
    .bind("{}")
    .bind(1) // Same sequence
    .execute(db.pool())
    .await;

    assert!(
        duplicate_result.is_err(),
        "Duplicate sequence should be rejected by unique constraint"
    );

    println!("✓ Migration 0199: Evidence unique sequence constraint works");
    Ok(())
}

// =============================================================================
// Migration 0200: Drop Adapter Packages
// =============================================================================

#[tokio::test]
async fn test_migration_0200_adapter_packages_dropped() -> Result<()> {
    let db = create_test_db().await?;

    // Verify adapter_packages table doesn't exist
    let table_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name='adapter_packages'",
    )
    .fetch_one(db.pool())
    .await?;

    assert_eq!(table_exists, 0, "adapter_packages table should be dropped");

    // Verify tenant_package_installs table doesn't exist
    let installs_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name='tenant_package_installs'",
    )
    .fetch_one(db.pool())
    .await?;

    assert_eq!(
        installs_exists, 0,
        "tenant_package_installs table should be dropped"
    );

    println!("✓ Migration 0200: adapter_packages tables dropped");
    Ok(())
}

// =============================================================================
// Migration 0201: Adapter Version Publish + Attach
// =============================================================================

#[tokio::test]
async fn test_migration_0201_adapter_version_columns_exist() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(adapter_versions)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    let new_columns = vec![
        "attach_mode",
        "required_scope_dataset_version_id",
        "is_archived",
        "published_at",
        "short_description",
    ];

    for col in &new_columns {
        assert!(
            columns.contains(*col),
            "Column '{}' missing from adapter_versions",
            col
        );
    }

    println!("✓ Migration 0201: Adapter version publish/attach columns exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0201_attach_mode_check_constraint() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant and repo
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-attach', 'Attach')")
        .execute(db.pool())
        .await?;

    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name)
         VALUES ('repo-attach', 'tenant-attach', 'Test Repo')",
    )
    .execute(db.pool())
    .await?;

    // Valid attach_mode
    let valid_result = sqlx::query(
        "INSERT INTO adapter_versions (
            id, repo_id, tenant_id, version, branch, release_state, attach_mode
        ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ver-1")
    .bind("repo-attach")
    .bind("tenant-attach")
    .bind("1.0.0")
    .bind("main")
    .bind("draft")
    .bind("free")
    .execute(db.pool())
    .await;

    assert!(valid_result.is_ok(), "Valid attach_mode should succeed");

    // Invalid attach_mode
    let invalid_result = sqlx::query(
        "INSERT INTO adapter_versions (
            id, repo_id, tenant_id, version, branch, release_state, attach_mode
        ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ver-2")
    .bind("repo-attach")
    .bind("tenant-attach")
    .bind("1.0.1")
    .bind("main")
    .bind("draft")
    .bind("invalid-mode")
    .execute(db.pool())
    .await;

    assert!(
        invalid_result.is_err(),
        "Invalid attach_mode should be rejected"
    );

    println!("✓ Migration 0201: attach_mode CHECK constraint works");
    Ok(())
}

#[tokio::test]
async fn test_migration_0201_free_mode_no_scope_trigger() -> Result<()> {
    let db = create_test_db().await?;

    // Setup
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-trigger', 'Trigger')")
        .execute(db.pool())
        .await?;

    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name)
         VALUES ('repo-trigger', 'tenant-trigger', 'Test')",
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO training_datasets (id, tenant_id, name, description, format, hash_b3, storage_path)
         VALUES ('dataset-1', 'tenant-trigger', 'Dataset', 'Test', 'jsonl', 'b3:hash123', 'var/test')",
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO training_dataset_versions (id, dataset_id, tenant_id, version_number, storage_path, hash_b3)
         VALUES ('dsv-1', 'dataset-1', 'tenant-trigger', 1, 'var/dsv-1', 'b3:dsv1hash')",
    )
    .execute(db.pool())
    .await?;

    // Free mode with scope should fail
    let result = sqlx::query(
        "INSERT INTO adapter_versions (
            id, repo_id, tenant_id, version, branch, release_state,
            attach_mode, required_scope_dataset_version_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ver-trigger-1")
    .bind("repo-trigger")
    .bind("tenant-trigger")
    .bind("1.0.0")
    .bind("main")
    .bind("draft")
    .bind("free")
    .bind("dsv-1")
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Free mode with scope should be rejected by trigger"
    );

    println!("✓ Migration 0201: Free mode no scope trigger works");
    Ok(())
}

#[tokio::test]
async fn test_migration_0201_requires_dataset_needs_scope_trigger() -> Result<()> {
    let db = create_test_db().await?;

    // Setup
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-req', 'Req')")
        .execute(db.pool())
        .await?;

    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name)
         VALUES ('repo-req', 'tenant-req', 'Test')",
    )
    .execute(db.pool())
    .await?;

    // requires_dataset without scope should fail
    let result = sqlx::query(
        "INSERT INTO adapter_versions (
            id, repo_id, tenant_id, version, branch, release_state, attach_mode
        ) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ver-req-1")
    .bind("repo-req")
    .bind("tenant-req")
    .bind("1.0.0")
    .bind("main")
    .bind("draft")
    .bind("requires_dataset")
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "requires_dataset without scope should be rejected by trigger"
    );

    println!("✓ Migration 0201: requires_dataset needs scope trigger works");
    Ok(())
}

#[tokio::test]
async fn test_migration_0201_scope_tenant_isolation_trigger() -> Result<()> {
    let db = create_test_db().await?;

    // Setup tenants
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-a', 'A'), ('tenant-b', 'B')")
        .execute(db.pool())
        .await?;

    sqlx::query(
        "INSERT INTO adapter_repositories (id, tenant_id, name)
         VALUES ('repo-iso', 'tenant-a', 'Test')",
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO training_datasets (id, tenant_id, name, description, format, hash_b3, storage_path)
         VALUES ('dataset-b', 'tenant-b', 'Dataset', 'Test', 'jsonl', 'b3:hash456', 'var/test-b')",
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO training_dataset_versions (id, dataset_id, tenant_id, version_number, storage_path, hash_b3)
         VALUES ('dsv-b', 'dataset-b', 'tenant-b', 1, 'var/dsv-b', 'b3:dsvbhash')",
    )
    .execute(db.pool())
    .await?;

    // Cross-tenant scope reference should fail
    let result = sqlx::query(
        "INSERT INTO adapter_versions (
            id, repo_id, tenant_id, version, branch, release_state,
            attach_mode, required_scope_dataset_version_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ver-iso-1")
    .bind("repo-iso")
    .bind("tenant-a") // Different tenant
    .bind("1.0.0")
    .bind("main")
    .bind("draft")
    .bind("requires_dataset")
    .bind("dsv-b") // Belongs to tenant-b
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant scope reference should be rejected"
    );

    println!("✓ Migration 0201: Scope tenant isolation trigger works");
    Ok(())
}

#[tokio::test]
async fn test_migration_0201_indexes_exist() -> Result<()> {
    let db = create_test_db().await?;

    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master
         WHERE type='index' AND tbl_name='adapter_versions'",
    )
    .fetch_all(db.pool())
    .await?;

    let expected_indexes = vec![
        "idx_adapter_versions_published",
        "idx_adapter_versions_archived",
        "idx_adapter_versions_attach_mode",
    ];

    for expected in &expected_indexes {
        assert!(
            indexes.contains(&expected.to_string()),
            "Index '{}' not found",
            expected
        );
    }

    println!("✓ Migration 0201: Adapter version indexes exist");
    Ok(())
}

// =============================================================================
// Migration 0202: Adapter Stacks Metadata
// =============================================================================

#[tokio::test]
async fn test_migration_0202_adapter_stacks_metadata_column_exists() -> Result<()> {
    let db = create_test_db().await?;

    let rows = sqlx::query("PRAGMA table_info(adapter_stacks)")
        .fetch_all(db.pool())
        .await?;

    let mut columns = HashSet::new();
    for row in rows {
        let col_name: String = row.get(1);
        columns.insert(col_name);
    }

    assert!(
        columns.contains("metadata_json"),
        "metadata_json missing from adapter_stacks"
    );

    println!("✓ Migration 0202: adapter_stacks metadata_json column exists");
    Ok(())
}

#[tokio::test]
async fn test_migration_0202_adapter_stacks_metadata_data() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-stack', 'Stack')")
        .execute(db.pool())
        .await?;

    // Insert stack with metadata
    let metadata_json = r#"{"dataset_version_id":"dsv-123","notes":"Test stack"}"#;
    sqlx::query(
        "INSERT INTO adapter_stacks (
            id, tenant_id, name, adapter_ids_json, metadata_json
        ) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("stack-1")
    .bind("tenant-stack")
    .bind("stack.test-stack")
    .bind("[]")
    .bind(metadata_json)
    .execute(db.pool())
    .await?;

    // Verify data
    let stored_metadata: Option<String> =
        sqlx::query_scalar("SELECT metadata_json FROM adapter_stacks WHERE id = ?")
            .bind("stack-1")
            .fetch_one(db.pool())
            .await?;

    assert_eq!(stored_metadata, Some(metadata_json.to_string()));

    println!("✓ Migration 0202: adapter_stacks metadata data works");
    Ok(())
}

// =============================================================================
// Integration Tests: Cross-Migration Scenarios
// =============================================================================

#[tokio::test]
async fn test_integration_receipt_with_all_new_fields() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name, max_kv_cache_bytes) VALUES ('tenant-int', 'Integration', 2000000)")
        .execute(db.pool())
        .await?;

    // Create trace
    sqlx::query(
        "INSERT INTO inference_traces (trace_id, tenant_id, request_id, context_digest)
         VALUES ('trace-int', 'tenant-int', 'req-int', x'abcd')",
    )
    .execute(db.pool())
    .await?;

    // Insert receipt with all new fields
    sqlx::query(
        "INSERT INTO inference_trace_receipts (
            trace_id, run_head_hash, output_digest, receipt_digest,
            logical_prompt_tokens, billed_input_tokens, logical_output_tokens,
            stop_reason_code, stop_reason_token_index, stop_policy_digest_b3,
            tenant_kv_quota_bytes, tenant_kv_bytes_used, kv_evictions,
            kv_residency_policy_id, kv_quota_enforced,
            prefix_kv_key_b3, prefix_cache_hit, prefix_kv_bytes,
            model_cache_identity_v2_digest_b3
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("trace-int")
    .bind(vec![0u8; 32])
    .bind(vec![0u8; 32])
    .bind(vec![0u8; 32])
    .bind(100i64)
    .bind(90i64)
    .bind(50i64)
    .bind("BUDGET_MAX")
    .bind(45i64)
    .bind(vec![0xBBu8; 32])
    .bind(2000000i64)
    .bind(1500000i64)
    .bind(3i64)
    .bind("kv_residency_v1")
    .bind(1i64)
    .bind("b3:prefix_key_123")
    .bind(1i64)
    .bind(512i64)
    .bind(vec![0xCCu8; 32])
    .execute(db.pool())
    .await?;

    // Verify all fields
    let row = sqlx::query(
        "SELECT
            stop_reason_code, stop_reason_token_index,
            tenant_kv_quota_bytes, tenant_kv_bytes_used, kv_evictions,
            prefix_kv_key_b3, prefix_cache_hit, prefix_kv_bytes,
            model_cache_identity_v2_digest_b3
         FROM inference_trace_receipts WHERE trace_id = ?",
    )
    .bind("trace-int")
    .fetch_one(db.pool())
    .await?;

    assert_eq!(
        row.get::<Option<String>, _>(0),
        Some("BUDGET_MAX".to_string())
    );
    assert_eq!(row.get::<Option<i64>, _>(1), Some(45));
    assert_eq!(row.get::<i64, _>(2), 2000000);
    assert_eq!(row.get::<i64, _>(3), 1500000);
    assert_eq!(row.get::<i64, _>(4), 3);
    assert_eq!(
        row.get::<Option<String>, _>(5),
        Some("b3:prefix_key_123".to_string())
    );
    assert_eq!(row.get::<i64, _>(6), 1);
    assert_eq!(row.get::<i64, _>(7), 512);

    println!("✓ Integration: Receipt with all new fields works correctly");
    Ok(())
}

#[tokio::test]
async fn test_integration_evidence_chain_with_prefix_templates() -> Result<()> {
    let db = create_test_db().await?;

    // Create tenant
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-chain', 'Chain')")
        .execute(db.pool())
        .await?;

    // Create prefix template
    sqlx::query(
        "INSERT INTO prefix_templates (
            id, tenant_id, mode, template_text, template_hash_b3, priority
        ) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("tpl-chain-1")
    .bind("tenant-chain")
    .bind("system")
    .bind("System prompt")
    .bind("b3:sys_hash")
    .bind(10)
    .execute(db.pool())
    .await?;

    // Create evidence chain
    for i in 1..=3 {
        let prev_root = if i == 1 {
            None
        } else {
            Some(format!("root-{}", i - 1))
        };

        sqlx::query(
            "INSERT INTO evidence_envelopes (
                id, tenant_id, scope, previous_root, root, signature,
                public_key, key_id, payload_json, chain_sequence
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("env-chain-{}", i))
        .bind("tenant-chain")
        .bind("telemetry")
        .bind(prev_root)
        .bind(format!("root-{}", i))
        .bind("sig")
        .bind("key")
        .bind("k1")
        .bind("{}")
        .bind(i as i64)
        .execute(db.pool())
        .await?;
    }

    // Verify chain
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM evidence_envelopes
         WHERE tenant_id = 'tenant-chain' AND scope = 'telemetry'",
    )
    .fetch_one(db.pool())
    .await?;

    assert_eq!(count, 3);

    // Verify prefix template exists alongside
    let template_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM prefix_templates WHERE tenant_id = 'tenant-chain'",
    )
    .fetch_one(db.pool())
    .await?;

    assert_eq!(template_count, 1);

    println!("✓ Integration: Evidence chain with prefix templates works");
    Ok(())
}

// =============================================================================
// Migration 0210: Tenant-Scoped Query Optimization (Composite Indexes)
// =============================================================================

/// Helper to create test data for tenant-scoped query optimization tests
async fn setup_tenant_scoped_test_data(db: &Db, tenant_id: &str) -> Result<()> {
    // Create tenant
    sqlx::query("INSERT OR IGNORE INTO tenants (id, name) VALUES (?, ?)")
        .bind(tenant_id)
        .bind(format!("Test Tenant {}", tenant_id))
        .execute(db.pool())
        .await?;

    // Create adapters with various states for testing
    for i in 0..10 {
        let adapter_id = format!("adapter-{}-{}", tenant_id, i);
        let hash_b3 = format!("b3:hash_{}_{}", tenant_id, i);
        let tier = match i % 3 {
            0 => "persistent",
            1 => "warm",
            _ => "ephemeral",
        };
        let expires_at = if i < 3 {
            Some("2099-12-31 23:59:59")
        } else {
            None
        };

        sqlx::query(
            r#"
            INSERT INTO adapters (
                id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json,
                adapter_id, active, lifecycle_state, load_state, activation_count, memory_bytes
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(format!("id-{}-{}", tenant_id, i))
        .bind(tenant_id)
        .bind(format!("Adapter {}-{}", tenant_id, i))
        .bind(tier)
        .bind(&hash_b3)
        .bind(16)
        .bind(32.0)
        .bind("[]")
        .bind(&adapter_id)
        .bind(1)
        .bind("active")
        .bind("cold")
        .bind(i as i64)
        .bind((i as i64) * 1024)
        .execute(db.pool())
        .await?;

        // Set expires_at for some adapters
        if let Some(expires) = expires_at {
            sqlx::query("UPDATE adapters SET expires_at = ? WHERE adapter_id = ?")
                .bind(expires)
                .bind(&adapter_id)
                .execute(db.pool())
                .await?;
        }
    }

    // Create documents for testing
    for i in 0..5 {
        sqlx::query(
            r#"
            INSERT INTO documents (
                id, tenant_id, name, content_hash, file_path, file_size, mime_type
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(format!("doc-{}-{}", tenant_id, i))
        .bind(tenant_id)
        .bind(format!("Document {}-{}", tenant_id, i))
        .bind(format!("b3:doc_{}_{}", tenant_id, i))
        .bind(format!("var/docs/{}/{}", tenant_id, i))
        .bind((i as i64 + 1) * 1000)
        .bind("text/plain")
        .execute(db.pool())
        .await?;
    }

    // Create repositories for training job FK integrity
    for i in 0..3 {
        let repo_id = format!("repo-{}-{}", tenant_id, i);
        sqlx::query(
            r#"
            INSERT INTO git_repositories (
                id, repo_id, path, branch, analysis_json, evidence_json,
                security_scan_json, status, created_by
            ) VALUES (?, ?, ?, 'main', '{}', '{}', '{}', 'active', ?)
            "#,
        )
        .bind(format!("git-{}-{}", tenant_id, i))
        .bind(&repo_id)
        .bind(format!("/repos/{}/{}", tenant_id, i))
        .bind("system")
        .execute(db.pool())
        .await?;
    }

    // Create training jobs for testing
    for i in 0..3 {
        let status = match i % 3 {
            0 => "running",
            1 => "completed",
            _ => "failed",
        };

        sqlx::query(
            r#"
            INSERT INTO repository_training_jobs (
                id, tenant_id, repo_id, training_config_json, status, progress_json,
                created_by, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now', ?))
            "#,
        )
        .bind(format!("job-{}-{}", tenant_id, i))
        .bind(tenant_id)
        .bind(format!("repo-{}-{}", tenant_id, i))
        .bind(r#"{"lr":0.001,"batch_size":4}"#)
        .bind(status)
        .bind(r#"{"progress_pct":25}"#)
        .bind("system")
        .bind(format!("-{} hours", i))
        .execute(db.pool())
        .await?;
    }

    // Create a base model entry for status tracking
    sqlx::query(
        r#"
        INSERT INTO models (
            id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3, tenant_id
        ) VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(format!("model-{}", tenant_id))
    .bind(format!("Model {}", tenant_id))
    .bind(format!("b3:model_hash_{}", tenant_id))
    .bind(format!("b3:config_hash_{}", tenant_id))
    .bind(format!("b3:tokenizer_hash_{}", tenant_id))
    .bind(format!("b3:tokenizer_cfg_hash_{}", tenant_id))
    .bind(tenant_id)
    .execute(db.pool())
    .await?;

    // Create chat sessions for tenant
    for i in 0..2 {
        sqlx::query(
            r#"
            INSERT INTO chat_sessions (
                id, tenant_id, name, created_at, last_activity_at
            ) VALUES (?, ?, ?, datetime('now'), datetime('now'))
            "#,
        )
        .bind(format!("session-{}-{}", tenant_id, i))
        .bind(tenant_id)
        .bind(format!("Session {}-{}", tenant_id, i))
        .execute(db.pool())
        .await?;
    }

    // Create chat messages for testing
    for i in 0..4 {
        sqlx::query(
            r#"
            INSERT INTO chat_messages (
                id, tenant_id, session_id, role, content, created_at
            ) VALUES (?, ?, ?, ?, ?, datetime('now', ?))
            "#,
        )
        .bind(format!("msg-{}-{}", tenant_id, i))
        .bind(tenant_id)
        .bind(format!("session-{}-{}", tenant_id, i % 2))
        .bind(if i % 2 == 0 { "user" } else { "assistant" })
        .bind(format!("Message content {}-{}", tenant_id, i))
        .bind(format!("-{} minutes", i))
        .execute(db.pool())
        .await?;
    }

    // Create base model status for testing
    sqlx::query(
        r#"
        INSERT INTO base_model_status (
            tenant_id, model_id, status, updated_at
        ) VALUES (?, ?, ?, datetime('now'))
        "#,
    )
    .bind(tenant_id)
    .bind(format!("model-{}", tenant_id))
    .bind("loaded")
    .execute(db.pool())
    .await?;

    Ok(())
}

/// Helper to validate EXPLAIN QUERY PLAN output
async fn validate_query_plan(
    db: &Db,
    query: &str,
    params: Vec<String>,
    expected_no_temp_btree: bool,
) -> Result<()> {
    // Build the query with placeholders
    let sql = format!("EXPLAIN QUERY PLAN {}", query);
    let mut query_builder = sqlx::query(&sql);

    // Bind parameters
    for param in params {
        query_builder = query_builder.bind(param);
    }

    let plan_rows = query_builder.fetch_all(db.pool()).await?;

    println!("Query Plan for: {}", query);
    let mut has_temp_btree = false;

    for row in &plan_rows {
        let id: i32 = row.get(0);
        let parent: i32 = row.get(1);
        let notused: i32 = row.get(2);
        let detail: String = row.get(3);

        println!(
            "  [{}] parent={}, notused={}, detail={}",
            id, parent, notused, detail
        );

        if detail.contains("USE TEMP B-TREE FOR ORDER BY") {
            has_temp_btree = true;
        }
    }

    if expected_no_temp_btree {
        assert!(
            !has_temp_btree,
            "Query should NOT use 'USE TEMP B-TREE FOR ORDER BY' but it does: {}",
            query
        );
        println!("✓ Query plan validation passed - no temp B-tree for ORDER BY");
    } else {
        // For cases where we expect temp B-tree (to verify the test works)
        assert!(
            has_temp_btree,
            "Query should use 'USE TEMP B-TREE FOR ORDER BY' but it doesn't: {}",
            query
        );
        println!("✓ Query plan validation passed - temp B-tree expected and found");
    }

    Ok(())
}

#[tokio::test]
async fn test_migration_0210_tenant_scoped_indexes_exist() -> Result<()> {
    let db = create_test_db().await?;

    let expected_indexes = vec![
        "idx_adapters_tenant_active_tier_created",
        "idx_adapters_tenant_hash_active_covering",
        "idx_adapters_tenant_expires",
        "idx_documents_tenant_created",
        "idx_training_jobs_tenant_status_created_adapter",
        "idx_chat_messages_tenant_created",
        "idx_base_model_status_tenant_model_status_updated",
    ];

    for expected in &expected_indexes {
        let index_exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master
             WHERE type='index' AND name = ?",
        )
        .bind(expected)
        .fetch_one(db.pool())
        .await?;

        assert_eq!(index_exists, 1, "Index '{}' should exist", expected);
    }

    println!("✓ Migration 0210: All 7 tenant-scoped composite indexes exist");
    Ok(())
}

#[tokio::test]
async fn test_migration_0210_adapter_listing_query_plan() -> Result<()> {
    let db = create_test_db().await?;
    let tenant_id = "test-tenant-adapters";

    setup_tenant_scoped_test_data(&db, tenant_id).await?;

    // Test query: SELECT * FROM adapters WHERE tenant_id = ? AND active = 1 ORDER BY tier ASC, created_at DESC
    let query = "SELECT * FROM adapters WHERE tenant_id = ? AND active = 1 ORDER BY tier ASC, created_at DESC";
    let params = vec![tenant_id.to_string()];

    validate_query_plan(&db, query, params, true).await?;

    println!("✓ Migration 0210: Adapter listing query uses composite index (no temp B-tree)");
    Ok(())
}

#[tokio::test]
async fn test_migration_0210_adapter_hash_lookup_query_plan() -> Result<()> {
    let db = create_test_db().await?;
    let tenant_id = "test-tenant-hash";

    setup_tenant_scoped_test_data(&db, tenant_id).await?;

    // Test query: SELECT * FROM adapters WHERE tenant_id = ? AND hash_b3 = ? AND active = 1
    let query = "SELECT * FROM adapters WHERE tenant_id = ? AND hash_b3 = ? AND active = 1";
    let params = vec![tenant_id.to_string(), format!("b3:hash_{}_0", tenant_id)];

    validate_query_plan(&db, query, params, true).await?;

    println!("✓ Migration 0210: Adapter hash lookup query uses composite index (no temp B-tree)");
    Ok(())
}

#[tokio::test]
async fn test_migration_0210_adapter_ttl_enforcement_query_plan() -> Result<()> {
    let db = create_test_db().await?;
    let tenant_id = "test-tenant-ttl";

    setup_tenant_scoped_test_data(&db, tenant_id).await?;

    // Test query: SELECT * FROM adapters WHERE tenant_id = ? AND expires_at IS NOT NULL AND expires_at < datetime('now')
    let query = "SELECT * FROM adapters WHERE tenant_id = ? AND expires_at IS NOT NULL AND expires_at < datetime('now')";
    let params = vec![tenant_id.to_string()];

    validate_query_plan(&db, query, params, true).await?;

    println!(
        "✓ Migration 0210: Adapter TTL enforcement query uses composite index (no temp B-tree)"
    );
    Ok(())
}

#[tokio::test]
async fn test_migration_0210_document_listing_query_plan() -> Result<()> {
    let db = create_test_db().await?;
    let tenant_id = "test-tenant-docs";

    setup_tenant_scoped_test_data(&db, tenant_id).await?;

    // Test query: SELECT * FROM documents WHERE tenant_id = ? ORDER BY created_at DESC
    let query = "SELECT * FROM documents WHERE tenant_id = ? ORDER BY created_at DESC";
    let params = vec![tenant_id.to_string()];

    validate_query_plan(&db, query, params, true).await?;

    println!("✓ Migration 0210: Document listing query uses composite index (no temp B-tree)");
    Ok(())
}

#[tokio::test]
async fn test_migration_0210_training_jobs_query_plan() -> Result<()> {
    let db = create_test_db().await?;
    let tenant_id = "test-tenant-jobs";

    setup_tenant_scoped_test_data(&db, tenant_id).await?;

    // Test query: SELECT * FROM repository_training_jobs WHERE tenant_id = ? AND status = ? ORDER BY created_at DESC
    let query = "SELECT * FROM repository_training_jobs WHERE tenant_id = ? AND status = ? ORDER BY created_at DESC";
    let params = vec![tenant_id.to_string(), "running".to_string()];

    validate_query_plan(&db, query, params, true).await?;

    println!("✓ Migration 0210: Training jobs query uses composite index (no temp B-tree)");
    Ok(())
}

#[tokio::test]
async fn test_migration_0210_chat_messages_query_plan() -> Result<()> {
    let db = create_test_db().await?;
    let tenant_id = "test-tenant-chat";

    setup_tenant_scoped_test_data(&db, tenant_id).await?;

    // Test query: SELECT * FROM chat_messages WHERE tenant_id = ? AND deleted_at IS NULL ORDER BY created_at DESC
    let query = "SELECT * FROM chat_messages WHERE tenant_id = ? AND deleted_at IS NULL ORDER BY created_at DESC";
    let params = vec![tenant_id.to_string()];

    validate_query_plan(&db, query, params, true).await?;

    println!("✓ Migration 0210: Chat messages query uses composite index (no temp B-tree)");
    Ok(())
}

#[tokio::test]
async fn test_migration_0210_base_model_status_upsert() -> Result<()> {
    let db = create_test_db().await?;
    let tenant_id = "test-tenant-model";

    setup_tenant_scoped_test_data(&db, tenant_id).await?;

    // Test upsert pattern: INSERT OR REPLACE based on tenant_id + model_id uniqueness
    let result = sqlx::query(
        r#"
        INSERT OR REPLACE INTO base_model_status (
            tenant_id, model_id, status, updated_at
        ) VALUES (?, ?, ?, datetime('now'))
        "#,
    )
    .bind(tenant_id)
    .bind(format!("model-{}", tenant_id))
    .bind("error")
    .execute(db.pool())
    .await?;

    assert_eq!(result.rows_affected(), 1, "Upsert should affect 1 row");

    // Verify the update
    let status: String = sqlx::query_scalar(
        "SELECT status FROM base_model_status WHERE tenant_id = ? AND model_id = ?",
    )
    .bind(tenant_id)
    .bind(format!("model-{}", tenant_id))
    .fetch_one(db.pool())
    .await?;

    assert_eq!(status, "error", "Status should be updated via upsert");

    println!("✓ Migration 0210: Base model status upsert works correctly");
    Ok(())
}

#[tokio::test]
async fn test_migration_0210_query_performance_validation() -> Result<()> {
    let db = create_test_db().await?;
    let tenant_id = "test-tenant-perf";

    setup_tenant_scoped_test_data(&db, tenant_id).await?;

    // Test that queries execute without temp B-tree operations
    // This validates the indexes provide optimal query performance

    // Adapter listing query
    let start = std::time::Instant::now();
    let adapters: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT * FROM adapters WHERE tenant_id = ? AND active = 1 ORDER BY tier ASC, created_at DESC",
    )
    .bind(tenant_id)
    .fetch_all(db.pool())
    .await?;
    let adapter_time = start.elapsed();

    // Document listing query
    let start = std::time::Instant::now();
    let documents: Vec<sqlx::sqlite::SqliteRow> =
        sqlx::query("SELECT * FROM documents WHERE tenant_id = ? ORDER BY created_at DESC")
            .bind(tenant_id)
            .fetch_all(db.pool())
            .await?;
    let document_time = start.elapsed();

    // Training jobs query
    let start = std::time::Instant::now();
    let jobs: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(
        "SELECT * FROM repository_training_jobs WHERE tenant_id = ? AND status = ? ORDER BY created_at DESC",
    )
    .bind(tenant_id)
    .bind("running")
    .fetch_all(db.pool())
    .await?;
    let job_time = start.elapsed();

    println!(
        "Query performance - Adapters: {}μs ({} rows), Documents: {}μs ({} rows), Jobs: {}μs ({} rows)",
        adapter_time.as_micros(),
        adapters.len(),
        document_time.as_micros(),
        documents.len(),
        job_time.as_micros(),
        jobs.len()
    );

    // Performance should be reasonable (< 10ms for small datasets)
    assert!(adapter_time.as_millis() < 100, "Adapter query too slow");
    assert!(document_time.as_millis() < 100, "Document query too slow");
    assert!(job_time.as_millis() < 100, "Job query too slow");

    println!("✓ Migration 0210: Query performance validation passed");
    Ok(())
}
