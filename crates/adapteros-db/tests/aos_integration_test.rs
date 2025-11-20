// Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
//
// .aos Upload Integration Test (Agent 9 - Integration Verifier)
//
// Purpose: Comprehensive integration test for .aos file upload functionality
// Tests: Database operations, file operations, foreign key constraints

use adapteros_core::hash::B3Hash;
use adapteros_db::{adapters::AdapterRegistrationParams, Db};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

/// Helper to initialize test database with schema
async fn init_test_db() -> anyhow::Result<Db> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;

    // Create a default tenant for tests
    sqlx::query("INSERT INTO tenants (id, name) VALUES ('tenant-1', 'Test Tenant')")
        .execute(db.pool())
        .await?;

    Ok(db)
}

/// Helper to create AdapterRegistrationParams with required fields
fn make_aos_params(
    adapter_id: &str,
    name: &str,
    hash: &str,
    rank: i32,
    file_path: &str,
    file_hash: &str,
) -> AdapterRegistrationParams {
    AdapterRegistrationParams {
        adapter_id: adapter_id.to_string(),
        tenant_id: "tenant-1".to_string(),
        name: name.to_string(),
        hash_b3: hash.to_string(),
        rank,
        tier: "ephemeral".to_string(),
        alpha: (rank * 2) as f64,
        targets_json: "[]".to_string(),
        acl_json: None,
        languages_json: None,
        framework: None,
        category: "test".to_string(),
        scope: "general".to_string(),
        framework_id: None,
        framework_version: None,
        repo_id: None,
        commit_sha: None,
        intent: None,
        expires_at: None,
        aos_file_path: Some(file_path.to_string()),
        aos_file_hash: Some(file_hash.to_string()),
        adapter_name: None,
        tenant_namespace: None,
        domain: None,
        purpose: None,
        revision: None,
        parent_id: None,
        fork_type: None,
        fork_reason: None,
    }
}

/// Create a minimal valid .aos file for testing
fn create_test_aos_file() -> Vec<u8> {
    // Create a minimal manifest
    let manifest = r#"{
        "version": "1.0.0",
        "name": "test-adapter",
        "description": "Test adapter for integration testing",
        "model_type": "lora",
        "base_model": "llama",
        "rank": 4,
        "alpha": 8.0
    }"#;

    let manifest_bytes = manifest.as_bytes();
    let manifest_len = manifest_bytes.len() as u32;

    // Create minimal safetensors content (just headers, no actual tensors)
    let safetensors = b"{}";

    // Build .aos file structure
    let mut aos_file = Vec::new();

    // Write header
    aos_file.extend_from_slice(&0u32.to_le_bytes()); // manifest_offset (will update)
    aos_file.extend_from_slice(&manifest_len.to_le_bytes()); // manifest_len

    // Write manifest
    let manifest_offset = aos_file.len() as u32;
    aos_file.extend_from_slice(manifest_bytes);

    // Write weights
    aos_file.extend_from_slice(safetensors);

    // Update manifest_offset in header
    aos_file[0..4].copy_from_slice(&manifest_offset.to_le_bytes());

    aos_file
}

// ============================================================================
// Test Case 1: register_adapter_with_aos - Success Path
// ============================================================================

#[tokio::test]
async fn test_register_adapter_with_aos_success() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let temp_dir = TempDir::new()?;
    let adapters_dir = temp_dir.path().join("adapters");
    fs::create_dir_all(&adapters_dir).await?;

    // Create test .aos file
    let aos_content = create_test_aos_file();
    let aos_hash = B3Hash::hash(&aos_content).to_hex();

    // Write to temp location
    let temp_file = adapters_dir.join("test-upload.aos.tmp");
    fs::write(&temp_file, &aos_content).await?;

    // Create final destination path
    let final_path = adapters_dir.join(format!("{}.aos", aos_hash));

    // Atomically rename
    fs::rename(&temp_file, &final_path).await?;

    // Register adapter with .aos metadata
    let params = make_aos_params(
        "test-adapter-001",
        "Test Adapter",
        &aos_hash,
        4,
        &final_path.to_string_lossy(),
        &aos_hash,
    );

    let adapter_id = db.register_adapter_with_aos(params).await?;

    // Verify adapter was registered
    assert_eq!(adapter_id, "test-adapter-001");

    // Verify adapter in database
    let adapters = db.list_adapters().await?;
    let adapter = adapters
        .iter()
        .find(|a| a.id == adapter_id)
        .expect("Adapter should exist");

    assert_eq!(adapter.name, "Test Adapter");
    assert_eq!(adapter.hash_b3, aos_hash);
    assert_eq!(adapter.rank, 4);

    // Verify aos_adapter_metadata was populated
    let metadata: (String, String) = sqlx::query_as(
        "SELECT aos_file_path, aos_file_hash FROM aos_adapter_metadata WHERE adapter_id = ?",
    )
    .bind(&adapter_id)
    .fetch_one(db.pool())
    .await?;

    assert!(metadata.0.ends_with(".aos"));
    assert_eq!(metadata.1, aos_hash);

    // Verify file exists on disk
    assert!(final_path.exists());

    Ok(())
}

// ============================================================================
// Test Case 2: Foreign Key Constraint - Adapter Must Exist
// ============================================================================

#[tokio::test]
async fn test_aos_metadata_foreign_key_constraint() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Try to insert aos_adapter_metadata without corresponding adapter
    let result = sqlx::query(
        "INSERT INTO aos_adapter_metadata (adapter_id, aos_file_path, aos_file_hash)
         VALUES (?, ?, ?)",
    )
    .bind("non-existent-adapter")
    .bind("/path/to/file.aos")
    .bind("b3:some_hash")
    .execute(db.pool())
    .await;

    // Should fail due to foreign key constraint
    assert!(result.is_err(), "Should enforce foreign key constraint");

    Ok(())
}

// ============================================================================
// Test Case 3: Missing aos_file_path - Should Fail Validation
// ============================================================================

#[tokio::test]
async fn test_register_adapter_with_aos_missing_path() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    let mut params = make_aos_params(
        "test-adapter-002",
        "Test Adapter",
        "b3:test_hash",
        4,
        "/path/to/file.aos",
        "b3:test_hash",
    );
    params.aos_file_path = None; // Missing!

    let result = db.register_adapter_with_aos(params).await;

    // Should fail validation
    assert!(result.is_err(), "Should fail when aos_file_path is missing");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("aos_file_path"),
        "Error should mention missing field"
    );

    Ok(())
}

// ============================================================================
// Test Case 4: Missing aos_file_hash - Should Fail Validation
// ============================================================================

#[tokio::test]
async fn test_register_adapter_with_aos_missing_hash() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    let mut params = make_aos_params(
        "test-adapter-003",
        "Test Adapter",
        "b3:test_hash",
        4,
        "/path/to/file.aos",
        "b3:test_hash",
    );
    params.aos_file_hash = None; // Missing!

    let result = db.register_adapter_with_aos(params).await;

    // Should fail validation
    assert!(result.is_err(), "Should fail when aos_file_hash is missing");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(
        err_msg.contains("aos_file_hash"),
        "Error should mention missing field"
    );

    Ok(())
}

// ============================================================================
// Test Case 5: Atomic File Operations - Cleanup on Error
// ============================================================================

#[tokio::test]
async fn test_atomic_file_cleanup_on_error() -> anyhow::Result<()> {
    let temp_dir = TempDir::new()?;
    let adapters_dir = temp_dir.path().join("adapters");
    fs::create_dir_all(&adapters_dir).await?;

    // Create test .aos file
    let aos_content = create_test_aos_file();
    let temp_file = adapters_dir.join("test-upload.aos.tmp");
    fs::write(&temp_file, &aos_content).await?;

    assert!(temp_file.exists(), "Temp file should exist");

    // Simulate error scenario - try to rename to non-existent directory
    let invalid_dest = PathBuf::from("/non/existent/path/file.aos");
    let result = fs::rename(&temp_file, &invalid_dest).await;

    assert!(
        result.is_err(),
        "Rename should fail for invalid destination"
    );

    // Original temp file should still exist
    assert!(
        temp_file.exists(),
        "Temp file should still exist after failed rename"
    );

    // Cleanup
    fs::remove_file(&temp_file).await?;
    assert!(!temp_file.exists(), "Cleanup should remove temp file");

    Ok(())
}

// ============================================================================
// Test Case 6: Multiple Adapters with .aos Files
// ============================================================================

#[tokio::test]
async fn test_multiple_aos_adapters() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let temp_dir = TempDir::new()?;
    let adapters_dir = temp_dir.path().join("adapters");
    fs::create_dir_all(&adapters_dir).await?;

    // Create and register 3 adapters
    for i in 1..=3 {
        let aos_content = create_test_aos_file();
        let aos_hash = format!("b3:test_hash_{}", i);
        let final_path = adapters_dir.join(format!("{}.aos", aos_hash));

        fs::write(&final_path, &aos_content).await?;

        let params = make_aos_params(
            &format!("test-adapter-{:03}", i),
            &format!("Test Adapter {}", i),
            &aos_hash,
            4,
            &final_path.to_string_lossy(),
            &aos_hash,
        );

        db.register_adapter_with_aos(params).await?;
    }

    // Verify all 3 adapters exist
    let adapters = db.list_adapters().await?;
    assert_eq!(adapters.len(), 3, "Should have 3 adapters");

    // Verify all have aos metadata
    let metadata_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM aos_adapter_metadata")
        .fetch_one(db.pool())
        .await?;

    assert_eq!(
        metadata_count.0, 3,
        "Should have 3 aos_adapter_metadata records"
    );

    Ok(())
}

// ============================================================================
// Test Case 7: Query Adapter with .aos Metadata
// ============================================================================

#[tokio::test]
async fn test_query_adapter_with_aos_metadata() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let temp_dir = TempDir::new()?;
    let adapters_dir = temp_dir.path().join("adapters");
    fs::create_dir_all(&adapters_dir).await?;

    let aos_content = create_test_aos_file();
    let aos_hash = B3Hash::hash(&aos_content).to_hex();
    let final_path = adapters_dir.join(format!("{}.aos", aos_hash));

    fs::write(&final_path, &aos_content).await?;

    let params = make_aos_params(
        "query-test-001",
        "Query Test Adapter",
        &aos_hash,
        8,
        &final_path.to_string_lossy(),
        &aos_hash,
    );

    db.register_adapter_with_aos(params).await?;

    // Query adapter with JOIN to aos_adapter_metadata
    let result: (String, i32, String, String) = sqlx::query_as(
        "SELECT a.id, a.rank, am.aos_file_path, am.aos_file_hash
         FROM adapters a
         JOIN aos_adapter_metadata am ON a.id = am.adapter_id
         WHERE a.id = ?",
    )
    .bind("query-test-001")
    .fetch_one(db.pool())
    .await?;

    let (adapter_id, rank, file_path, file_hash) = result;

    assert_eq!(adapter_id, "query-test-001");
    assert_eq!(rank, 8);
    assert!(file_path.ends_with(".aos"));
    assert_eq!(file_hash, aos_hash);

    Ok(())
}
