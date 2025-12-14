//! Migration Validation Tests
//!
//! Comprehensive tests for migrations 0193-0202 that verify:
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
         VALUES ('dataset-1', 'tenant-trigger', 'Dataset', 'Test', 'jsonl', 'b3:hash123', '/tmp/test')",
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO training_dataset_versions (id, dataset_id, tenant_id, version_number, storage_path, hash_b3)
         VALUES ('dsv-1', 'dataset-1', 'tenant-trigger', 1, '/tmp/dsv-1', 'b3:dsv1hash')",
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
         VALUES ('dataset-b', 'tenant-b', 'Dataset', 'Test', 'jsonl', 'b3:hash456', '/tmp/test-b')",
    )
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO training_dataset_versions (id, dataset_id, tenant_id, version_number, storage_path, hash_b3)
         VALUES ('dsv-b', 'dataset-b', 'tenant-b', 1, '/tmp/dsv-b', 'b3:dsvbhash')",
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
