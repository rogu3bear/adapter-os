//! Tenant Foreign Key Enforcement Tests
//!
//! These tests verify that composite FK constraints prevent cross-tenant data access
//! at the database level. Migration 0131_harden_tenant_fks.sql provides:
//!
//! 1. document_chunks: FK(tenant_id, document_id) -> documents(tenant_id, id)
//! 2. inference_evidence: FK(tenant_id, document_id) -> documents(tenant_id, id)
//! 3. collection_documents: Both FKs require matching tenant_id
//! 4. adapter_training_snapshots: FK on collection_id requires tenant match
//!
//! The migration is applied automatically via `Db::new_in_memory()` which runs all
//! migrations in sequence, including the tenant FK hardening migration.

use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use uuid::Uuid;

/// Create a test tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;
    Ok(())
}

/// Create a test document
async fn create_test_document(db: &Db, doc_id: &str, tenant_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
         VALUES (?, ?, 'test.pdf', 'hash123', 'var/test.pdf', 1000, 'application/pdf', 'indexed')",
    )
    .bind(doc_id)
    .bind(tenant_id)
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to create document: {}", e)))?;
    Ok(())
}

/// Create a test document collection
async fn create_test_collection(db: &Db, collection_id: &str, tenant_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO document_collections (id, tenant_id, name, description, created_at)
         VALUES (?, ?, 'Test Collection', 'Test', datetime('now'))",
    )
    .bind(collection_id)
    .bind(tenant_id)
    .execute(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to create collection: {}", e)))?;
    Ok(())
}

// =============================================================================
// TEST: document_chunks composite FK enforcement
// =============================================================================

#[tokio::test]
async fn test_document_chunks_rejects_cross_tenant_insert() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create a document in tenant-a
    create_test_document(&db, "doc-a-1", "tenant-a").await?;

    // Try to insert document_chunk with tenant-b but referencing tenant-a's document
    // This should FAIL due to composite FK constraint
    let result = sqlx::query(
        "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash, text_preview)
         VALUES (?, ?, ?, 0, 'hash', 'preview')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind("tenant-b") // WRONG TENANT - should be tenant-a
    .bind("doc-a-1") // References tenant-a's document
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant document_chunk insert should be rejected by FK constraint"
    );

    // Verify the error is a FK constraint violation
    let err_msg = result.unwrap_err().to_string().to_lowercase();
    assert!(
        err_msg.contains("foreign key") || err_msg.contains("fk") || err_msg.contains("constraint"),
        "Error should be FK constraint violation, got: {}",
        err_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_document_chunks_allows_same_tenant_insert() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_document(&db, "doc-a-1", "tenant-a").await?;

    // Insert document_chunk with correct tenant
    let result = sqlx::query(
        "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash, text_preview)
         VALUES (?, ?, ?, 0, 'hash', 'preview')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind("tenant-a") // CORRECT TENANT
    .bind("doc-a-1")
    .execute(db.pool())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant document_chunk insert should succeed"
    );

    Ok(())
}

// =============================================================================
// TEST: collection_documents composite FK enforcement
// =============================================================================

#[tokio::test]
async fn test_collection_documents_rejects_cross_tenant_reference() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create collection in tenant-a
    create_test_collection(&db, "coll-a-1", "tenant-a").await?;

    // Create document in tenant-b
    create_test_document(&db, "doc-b-1", "tenant-b").await?;

    // Try to link tenant-a's collection to tenant-b's document
    // This should FAIL because tenants don't match
    let result = sqlx::query(
        "INSERT INTO collection_documents (tenant_id, collection_id, document_id, added_at)
         VALUES (?, ?, ?, datetime('now'))",
    )
    .bind("tenant-a") // Collection's tenant
    .bind("coll-a-1")
    .bind("doc-b-1") // Document from different tenant!
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant collection_document link should be rejected"
    );

    Ok(())
}

#[tokio::test]
async fn test_collection_documents_allows_same_tenant_link() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;
    create_test_collection(&db, "coll-a-1", "tenant-a").await?;
    create_test_document(&db, "doc-a-1", "tenant-a").await?;

    // Link collection and document from same tenant
    let result = sqlx::query(
        "INSERT INTO collection_documents (tenant_id, collection_id, document_id, added_at)
         VALUES (?, ?, ?, datetime('now'))",
    )
    .bind("tenant-a")
    .bind("coll-a-1")
    .bind("doc-a-1")
    .execute(db.pool())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant collection_document link should succeed"
    );

    Ok(())
}

// =============================================================================
// TEST: Trigger-based tenant validation (adapters.primary_dataset_id)
// =============================================================================

#[tokio::test]
async fn test_adapter_dataset_trigger_rejects_cross_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create dataset in tenant-b
    sqlx::query(
        "INSERT INTO training_datasets (id, name, tenant_id, format, storage_path, hash_b3, validation_status, created_at)
         VALUES (?, 'Dataset B', ?, 'jsonl', 'var/test-datasets', 'hash', 'valid', datetime('now'))",
    )
    .bind("dataset-b-1")
    .bind("tenant-b")
    .execute(db.pool())
    .await?;

    // Try to create adapter in tenant-a referencing tenant-b's dataset
    let adapter_id = Uuid::new_v4().to_string();
    let result = sqlx::query(
        "INSERT INTO adapters (id, tenant_id, name, hash_b3, rank, alpha, tier, targets_json, primary_dataset_id)
         VALUES (?, ?, 'Adapter A', ?, 16, 1.0, 'persistent', '[]', ?)",
    )
    .bind(&adapter_id)
    .bind("tenant-a") // Adapter tenant
    .bind(format!("hash-{}", adapter_id)) // Unique hash
    .bind("dataset-b-1") // Dataset from different tenant!
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Cross-tenant adapter->dataset reference should be rejected by trigger"
    );

    // Verify the error message mentions tenant mismatch (from the trigger)
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Tenant mismatch") || err_msg.to_lowercase().contains("tenant"),
        "Error should mention tenant mismatch, got: {}",
        err_msg
    );

    Ok(())
}

// =============================================================================
// TEST: dataset_files trigger tenant validation
// =============================================================================

#[tokio::test]
async fn test_dataset_files_trigger_rejects_missing_dataset() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;

    // Try to add file to non-existent dataset
    let result = sqlx::query(
        "INSERT INTO dataset_files (id, dataset_id, tenant_id, file_name, file_path, size_bytes, hash_b3)
         VALUES (?, ?, ?, 'test.jsonl', 'var/test.jsonl', 1000, 'hash123')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind("non-existent-dataset")
    .bind("tenant-a")
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Insert to non-existent dataset should be rejected by trigger"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("dataset does not exist") || err_msg.contains("Invalid dataset_id"),
        "Error should mention missing dataset, got: {}",
        err_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_dataset_files_trigger_rejects_null_tenant_on_parent() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;

    // Create dataset WITHOUT tenant_id (NULL)
    sqlx::query(
        "INSERT INTO training_datasets (id, name, format, storage_path, hash_b3, validation_status, created_at)
         VALUES (?, 'Dataset No Tenant', 'jsonl', 'var/test-datasets', 'hash', 'valid', datetime('now'))",
    )
    .bind("dataset-no-tenant")
    .execute(db.pool())
    .await?;

    // Try to add file to dataset that has NULL tenant_id
    let result = sqlx::query(
        "INSERT INTO dataset_files (id, dataset_id, tenant_id, file_name, file_path, size_bytes, hash_b3)
         VALUES (?, ?, ?, 'test.jsonl', 'var/test.jsonl', 1000, 'hash123')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind("dataset-no-tenant")
    .bind("tenant-a")
    .execute(db.pool())
    .await;

    assert!(
        result.is_err(),
        "Insert to dataset with NULL tenant_id should be rejected by trigger"
    );

    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("tenant_id") || err_msg.contains("Tenant"),
        "Error should mention tenant_id requirement, got: {}",
        err_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_dataset_files_allows_same_tenant() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;

    // Create dataset with proper tenant_id
    sqlx::query(
        "INSERT INTO training_datasets (id, name, tenant_id, format, storage_path, hash_b3, validation_status, created_at)
         VALUES (?, 'Dataset A', ?, 'jsonl', 'var/test-datasets', 'hash', 'valid', datetime('now'))",
    )
    .bind("dataset-a")
    .bind("tenant-a")
    .execute(db.pool())
    .await?;

    // Add file with matching tenant_id - should succeed
    let result = sqlx::query(
        "INSERT INTO dataset_files (id, dataset_id, tenant_id, file_name, file_path, size_bytes, hash_b3)
         VALUES (?, ?, ?, 'test.jsonl', 'var/test.jsonl', 1000, 'hash123')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind("dataset-a")
    .bind("tenant-a")
    .execute(db.pool())
    .await;

    assert!(
        result.is_ok(),
        "Same-tenant dataset_file insert should succeed: {:?}",
        result.err()
    );

    Ok(())
}

#[tokio::test]
async fn test_dataset_files_allows_null_file_tenant_id() -> Result<()> {
    let db = Db::new_in_memory().await?;

    create_test_tenant(&db, "tenant-a").await?;

    // Create dataset with proper tenant_id
    sqlx::query(
        "INSERT INTO training_datasets (id, name, tenant_id, format, storage_path, hash_b3, validation_status, created_at)
         VALUES (?, 'Dataset A', ?, 'jsonl', 'var/test-datasets', 'hash', 'valid', datetime('now'))",
    )
    .bind("dataset-a")
    .bind("tenant-a")
    .execute(db.pool())
    .await?;

    // Add file without tenant_id (NULL) - should succeed per trigger logic
    let result = sqlx::query(
        "INSERT INTO dataset_files (id, dataset_id, file_name, file_path, size_bytes, hash_b3)
         VALUES (?, ?, 'test.jsonl', 'var/test.jsonl', 1000, 'hash123')",
    )
    .bind(Uuid::new_v4().to_string())
    .bind("dataset-a")
    .execute(db.pool())
    .await;

    assert!(
        result.is_ok(),
        "NULL tenant_id on dataset_file should succeed: {:?}",
        result.err()
    );

    Ok(())
}

// =============================================================================
// TEST: Orphan detection fails migration
// =============================================================================

#[tokio::test]
async fn test_orphan_check_constraint_behavior() -> Result<()> {
    // This test verifies that the CHECK constraint pattern works
    let db = Db::new_in_memory().await?;

    // Create a table with CHECK(count = 0) constraint
    sqlx::query(
        "CREATE TABLE _test_check (
            name TEXT PRIMARY KEY,
            count INTEGER NOT NULL CHECK(count = 0)
        )",
    )
    .execute(db.pool())
    .await?;

    // Insert with count=0 should succeed
    let result_ok = sqlx::query("INSERT INTO _test_check (name, count) VALUES ('test1', 0)")
        .execute(db.pool())
        .await;
    assert!(result_ok.is_ok(), "Insert with count=0 should succeed");

    // Insert with count=1 should fail due to CHECK constraint
    let result_fail = sqlx::query("INSERT INTO _test_check (name, count) VALUES ('test2', 1)")
        .execute(db.pool())
        .await;
    assert!(
        result_fail.is_err(),
        "Insert with count=1 should fail CHECK constraint"
    );

    // Cleanup
    sqlx::query("DROP TABLE _test_check")
        .execute(db.pool())
        .await?;

    Ok(())
}
