//! End-to-End Integration Tests for Document Chat Flow
//!
//! These tests verify the document processing lifecycle including:
//! - Document failure recovery and retry mechanisms
//! - Concurrent processing lock acquisition to prevent duplicate work
//! - Stale processing recovery for stuck documents
//!
//! Test Coverage:
//! 1. Document processing failure → error handling → retry → success
//! 2. Concurrent lock acquisition ensures exactly one processor wins
//! 3. Stale processing documents are automatically recovered
//!
//! Run with:
//! - cargo test --test document_chat_e2e
//! - cargo test --test document_chat_e2e -- --nocapture (with output)
//! - cargo test --test document_chat_e2e -- --test-threads=1 (sequential)

use adapteros_db::documents::CreateDocumentParams;
use adapteros_db::Db;
use std::sync::Arc;
use tokio::task::JoinSet;
use uuid::Uuid;

// =============================================================================
// Test Utilities
// =============================================================================

/// Initialize test database with required schema
///
/// Uses a temporary file-based database to avoid transaction nesting issues
/// with in-memory databases that have max_connections=1
async fn init_test_db() -> anyhow::Result<Arc<Db>> {
    std::env::set_var("AOS_SKIP_MIGRATION_SIGNATURES", "1");

    // Create a temporary file for the database
    let db_dir = std::path::PathBuf::from("var")
        .join("tmp")
        .join("document_chat_e2e");
    std::fs::create_dir_all(&db_dir)?;
    let temp_path = db_dir
        .join(format!("aos-test-{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .to_string();

    // Connect and run migrations
    let db = Db::connect(&temp_path).await?;
    db.migrate().await?;
    let db = Arc::new(db);

    // Create test tenant
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind("tenant-test")
        .bind("Test Tenant")
        .execute(db.pool_result().unwrap())
        .await?;

    Ok(db)
}

/// Create a test document in pending state
async fn create_test_document(db: &Db, tenant_id: &str, name: &str) -> anyhow::Result<String> {
    let doc_id = Uuid::new_v4().to_string();

    db.create_document(CreateDocumentParams {
        id: doc_id.clone(),
        tenant_id: tenant_id.to_string(),
        name: name.to_string(),
        content_hash: format!("hash-{}", Uuid::new_v4()),
        file_path: format!("var/test-documents/{}", name),
        file_size: 1024,
        mime_type: "application/pdf".to_string(),
        page_count: Some(5),
    })
    .await?;

    Ok(doc_id)
}

/// Create a document directly in processing state with a specific timestamp
async fn create_processing_document_with_time(
    db: &Db,
    tenant_id: &str,
    name: &str,
    minutes_ago: i64,
) -> anyhow::Result<String> {
    let doc_id = create_test_document(db, tenant_id, name).await?;

    // Update to processing state with backdated timestamp
    sqlx::query(
        "UPDATE documents
         SET status = 'processing',
             processing_started_at = datetime('now', '-' || ? || ' minutes'),
             updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(minutes_ago)
    .bind(&doc_id)
    .execute(db.pool_result().unwrap())
    .await?;

    Ok(doc_id)
}

// =============================================================================
// Test 1: Document Processing Failure Recovery
// =============================================================================

/// Test the full lifecycle of document processing with failure and retry
///
/// Flow:
/// 1. Upload document (status: pending)
/// 2. Simulate processing failure
/// 3. Verify error state is correctly recorded
/// 4. Call retry mechanism
/// 5. Verify document is ready for reprocessing
#[tokio::test]
async fn test_document_processing_failure_recovery() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let tenant_id = "tenant-test";

    // Step 1: Create a document in pending state
    let doc_id = create_test_document(&db, tenant_id, "test-doc.pdf").await?;

    // Verify initial state
    let doc = db
        .get_document(tenant_id, &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.status, "pending", "Initial status should be pending");
    assert_eq!(doc.retry_count, 0, "Initial retry count should be 0");
    assert!(doc.error_message.is_none(), "No error message initially");
    assert!(doc.error_code.is_none(), "No error code initially");

    // Step 2: Acquire processing lock (simulate processor starting work)
    let acquired = db.try_acquire_processing_lock(tenant_id, &doc_id).await?;
    assert!(acquired, "Should successfully acquire processing lock");

    // Verify document is now in processing state
    let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
    assert_eq!(doc.status, "processing", "Status should be processing");
    assert!(
        doc.processing_started_at.is_some(),
        "processing_started_at should be set"
    );

    // Step 3: Simulate processing failure
    let error_msg = "Failed to extract text from PDF: corrupt file header";
    let error_code = "PDF_CORRUPT";

    db.mark_document_failed(tenant_id, &doc_id, error_msg, error_code)
        .await?;

    // Verify failure state
    let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
    assert_eq!(doc.status, "failed", "Status should be failed");
    assert_eq!(
        doc.error_message.as_deref(),
        Some(error_msg),
        "Error message should match"
    );
    assert_eq!(
        doc.error_code.as_deref(),
        Some(error_code),
        "Error code should match"
    );
    assert!(
        doc.processing_completed_at.is_some(),
        "processing_completed_at should be set"
    );

    // Step 4: Prepare document for retry
    let retry_prepared = db.prepare_document_retry(tenant_id, &doc_id).await?;
    assert!(retry_prepared, "Retry should be prepared successfully");

    // Verify retry state
    let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
    assert_eq!(doc.status, "pending", "Status should be reset to pending");
    assert_eq!(doc.retry_count, 1, "Retry count should be incremented");
    assert!(
        doc.error_message.is_none(),
        "Error message should be cleared"
    );
    assert!(doc.error_code.is_none(), "Error code should be cleared");
    assert!(
        doc.processing_started_at.is_none(),
        "processing_started_at should be cleared"
    );
    assert!(
        doc.processing_completed_at.is_none(),
        "processing_completed_at should be cleared"
    );

    // Step 5: Verify retry limit enforcement
    // Default max_retries is 3, so we can retry 2 more times
    for expected_retry_count in 2..=3 {
        // Acquire lock, fail, and retry
        db.try_acquire_processing_lock(tenant_id, &doc_id).await?;
        db.mark_document_failed(tenant_id, &doc_id, "Still failing", "TEST_ERROR")
            .await?;

        let can_retry = db.prepare_document_retry(tenant_id, &doc_id).await?;

        if expected_retry_count < 3 {
            assert!(can_retry, "Should be able to retry");
            let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
            assert_eq!(
                doc.retry_count, expected_retry_count,
                "Retry count should be {}",
                expected_retry_count
            );
        } else {
            // At max_retries, can retry once more
            assert!(can_retry, "Should be able to retry at max_retries");
            let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
            assert_eq!(doc.retry_count, 3);
        }
    }

    // Step 6: Exceed max retries
    db.try_acquire_processing_lock(tenant_id, &doc_id).await?;
    db.mark_document_failed(tenant_id, &doc_id, "Final failure", "TEST_ERROR")
        .await?;

    let can_retry = db.prepare_document_retry(tenant_id, &doc_id).await?;
    assert!(
        !can_retry,
        "Should NOT be able to retry after exceeding max_retries"
    );

    let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
    assert_eq!(doc.status, "failed", "Should remain in failed state");
    assert_eq!(doc.retry_count, 3, "Retry count should be at max");

    Ok(())
}

// =============================================================================
// Test 2: Concurrent Processing Lock Acquisition
// =============================================================================

/// Test that exactly one processor can acquire the lock when multiple try concurrently
///
/// This test simulates multiple workers trying to process the same document.
/// Only one should succeed, preventing duplicate processing.
#[tokio::test]
async fn test_concurrent_processing_lock_acquisition() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let tenant_id = "tenant-test";

    // Create a document in pending state
    let doc_id = create_test_document(&db, tenant_id, "concurrent-test.pdf").await?;

    // Spawn 10 concurrent tasks all trying to acquire the processing lock
    let num_workers = 10;
    let mut join_set = JoinSet::new();

    for worker_id in 0..num_workers {
        let db = db.clone();
        let doc_id = doc_id.clone();
        let tenant_id = tenant_id.to_string();

        join_set.spawn(async move {
            // Each worker tries to acquire the lock
            // Note: Due to SQLite locking, some workers may get "database is locked" errors
            // which is acceptable - it means another worker has the lock
            let result = db.try_acquire_processing_lock(&tenant_id, &doc_id).await;

            let acquired = match result {
                Ok(true) => true,
                Ok(false) => false,
                Err(e) if e.to_string().contains("database is locked") => {
                    // Database locked error means another worker has the lock
                    false
                }
                Err(e) => panic!("Unexpected error during lock acquisition: {}", e),
            };

            (worker_id, acquired)
        });
    }

    // Collect results
    let mut successful_workers = Vec::new();
    let mut failed_workers = Vec::new();

    while let Some(result) = join_set.join_next().await {
        let (worker_id, acquired) = result?;
        if acquired {
            successful_workers.push(worker_id);
        } else {
            failed_workers.push(worker_id);
        }
    }

    // Assert: Exactly one worker should have acquired the lock
    assert_eq!(
        successful_workers.len(),
        1,
        "Exactly one worker should acquire the lock, got: {:?}",
        successful_workers
    );

    assert_eq!(
        failed_workers.len(),
        num_workers - 1,
        "All other workers should fail to acquire the lock"
    );

    // Verify the document is in processing state
    let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
    assert_eq!(
        doc.status, "processing",
        "Document should be in processing state"
    );
    assert!(
        doc.processing_started_at.is_some(),
        "processing_started_at should be set"
    );

    // Verify no duplicate state transitions
    // If we try to acquire again, it should fail since it's already processing
    let second_attempt = db.try_acquire_processing_lock(tenant_id, &doc_id).await?;
    assert!(
        !second_attempt,
        "Should not be able to acquire lock on already-processing document"
    );

    Ok(())
}

/// Test that lock acquisition respects document state
#[tokio::test]
async fn test_lock_acquisition_respects_document_state() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let tenant_id = "tenant-test";

    // Test 1: Can acquire lock on pending document
    let pending_doc = create_test_document(&db, tenant_id, "pending.pdf").await?;
    let acquired = db
        .try_acquire_processing_lock(tenant_id, &pending_doc)
        .await?;
    assert!(acquired, "Should acquire lock on pending document");

    // Test 2: Cannot acquire lock on already-processing document
    let acquired_again = db
        .try_acquire_processing_lock(tenant_id, &pending_doc)
        .await?;
    assert!(
        !acquired_again,
        "Should NOT acquire lock on processing document"
    );

    // Test 3: Can acquire lock on failed document (retry scenario)
    let failed_doc = create_test_document(&db, tenant_id, "failed.pdf").await?;
    db.try_acquire_processing_lock(tenant_id, &failed_doc)
        .await?;
    db.mark_document_failed(tenant_id, &failed_doc, "Test error", "TEST")
        .await?;

    let acquired_failed = db
        .try_acquire_processing_lock(tenant_id, &failed_doc)
        .await?;
    assert!(acquired_failed, "Should acquire lock on failed document");

    // Test 4: Cannot acquire lock on indexed document
    let indexed_doc = create_test_document(&db, tenant_id, "indexed.pdf").await?;
    db.try_acquire_processing_lock(tenant_id, &indexed_doc)
        .await?;
    db.mark_document_indexed(tenant_id, &indexed_doc, Some(10))
        .await?;

    let acquired_indexed = db
        .try_acquire_processing_lock(tenant_id, &indexed_doc)
        .await?;
    assert!(
        !acquired_indexed,
        "Should NOT acquire lock on indexed document"
    );

    Ok(())
}

// =============================================================================
// Test 3: Stale Processing Recovery
// =============================================================================

/// Test that documents stuck in processing state are recovered
///
/// Simulates a scenario where a worker crashes mid-processing, leaving
/// the document in "processing" state indefinitely. The recovery mechanism
/// should detect and reset these stale documents.
#[tokio::test]
async fn test_stale_processing_recovery() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let tenant_id = "tenant-test";

    // Create documents with different processing start times
    let stale_doc_1h =
        create_processing_document_with_time(&db, tenant_id, "stale-1h.pdf", 60).await?;
    let stale_doc_2h =
        create_processing_document_with_time(&db, tenant_id, "stale-2h.pdf", 120).await?;
    let recent_doc_15min =
        create_processing_document_with_time(&db, tenant_id, "recent-15min.pdf", 15).await?;
    let recent_doc_5min =
        create_processing_document_with_time(&db, tenant_id, "recent-5min.pdf", 5).await?;

    // Verify all documents are in processing state
    for doc_id in [
        &stale_doc_1h,
        &stale_doc_2h,
        &recent_doc_15min,
        &recent_doc_5min,
    ] {
        let doc = db.get_document(tenant_id, doc_id).await?.unwrap();
        assert_eq!(doc.status, "processing", "Document should be processing");
    }

    // Run stale recovery with 30-minute threshold
    let stale_threshold_minutes = 30;
    let reset_count = db
        .reset_stale_processing_documents(tenant_id, stale_threshold_minutes)
        .await?;

    // Should reset 2 documents (1h and 2h old)
    assert_eq!(
        reset_count, 2,
        "Should reset exactly 2 stale documents (1h and 2h old)"
    );

    // Verify stale documents are now pending
    let doc_1h = db.get_document(tenant_id, &stale_doc_1h).await?.unwrap();
    assert_eq!(
        doc_1h.status, "pending",
        "1-hour stale document should be reset to pending"
    );
    assert!(
        doc_1h.processing_started_at.is_none(),
        "processing_started_at should be cleared"
    );

    let doc_2h = db.get_document(tenant_id, &stale_doc_2h).await?.unwrap();
    assert_eq!(
        doc_2h.status, "pending",
        "2-hour stale document should be reset to pending"
    );
    assert!(
        doc_2h.processing_started_at.is_none(),
        "processing_started_at should be cleared"
    );

    // Verify recent documents are still processing
    let doc_15min = db
        .get_document(tenant_id, &recent_doc_15min)
        .await?
        .unwrap();
    assert_eq!(
        doc_15min.status, "processing",
        "15-minute document should still be processing"
    );
    assert!(
        doc_15min.processing_started_at.is_some(),
        "processing_started_at should remain set"
    );

    let doc_5min = db.get_document(tenant_id, &recent_doc_5min).await?.unwrap();
    assert_eq!(
        doc_5min.status, "processing",
        "5-minute document should still be processing"
    );

    Ok(())
}

/// Test stale recovery with no stale documents
#[tokio::test]
async fn test_stale_recovery_with_no_stale_documents() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let tenant_id = "tenant-test";

    // Create only recent documents
    create_processing_document_with_time(&db, tenant_id, "recent-1.pdf", 5).await?;
    create_processing_document_with_time(&db, tenant_id, "recent-2.pdf", 10).await?;

    // Run stale recovery with 30-minute threshold
    let reset_count = db.reset_stale_processing_documents(tenant_id, 30).await?;

    assert_eq!(
        reset_count, 0,
        "Should not reset any documents when none are stale"
    );

    Ok(())
}

/// Test stale recovery is tenant-isolated
#[tokio::test]
async fn test_stale_recovery_tenant_isolation() -> anyhow::Result<()> {
    let db = init_test_db().await?;

    // Create second tenant
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind("tenant-other")
        .bind("Other Tenant")
        .execute(db.pool_result().unwrap())
        .await?;

    // Create stale documents in both tenants
    let stale_test =
        create_processing_document_with_time(&db, "tenant-test", "stale.pdf", 60).await?;
    let stale_other =
        create_processing_document_with_time(&db, "tenant-other", "stale.pdf", 60).await?;

    // Reset stale documents for tenant-test only
    let reset_count = db
        .reset_stale_processing_documents("tenant-test", 30)
        .await?;

    assert_eq!(reset_count, 1, "Should reset only tenant-test document");

    // Verify tenant-test document is reset
    let doc_test = db.get_document("tenant-test", &stale_test).await?.unwrap();
    assert_eq!(doc_test.status, "pending");

    // Verify tenant-other document is unchanged
    let doc_other = db
        .get_document("tenant-other", &stale_other)
        .await?
        .unwrap();
    assert_eq!(
        doc_other.status, "processing",
        "Other tenant's document should be unaffected"
    );

    Ok(())
}

// =============================================================================
// Test 4: Get Retryable Documents
// =============================================================================

/// Test that get_retryable_documents correctly filters and limits results
#[tokio::test]
async fn test_get_retryable_documents() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let tenant_id = "tenant-test";

    // Create several documents in different states
    let failed_doc_1 = create_test_document(&db, tenant_id, "failed-1.pdf").await?;
    db.try_acquire_processing_lock(tenant_id, &failed_doc_1)
        .await?;
    db.mark_document_failed(tenant_id, &failed_doc_1, "Error 1", "ERR1")
        .await?;

    let failed_doc_2 = create_test_document(&db, tenant_id, "failed-2.pdf").await?;
    db.try_acquire_processing_lock(tenant_id, &failed_doc_2)
        .await?;
    db.mark_document_failed(tenant_id, &failed_doc_2, "Error 2", "ERR2")
        .await?;

    // Create a failed document that has exceeded retry limit
    let failed_max_retries = create_test_document(&db, tenant_id, "failed-max.pdf").await?;
    // Manually set retry_count to max_retries
    sqlx::query("UPDATE documents SET status = 'failed', retry_count = max_retries WHERE id = ?")
        .bind(&failed_max_retries)
        .execute(db.pool_result().unwrap())
        .await?;

    // Create documents in other states (should not be returned)
    create_test_document(&db, tenant_id, "pending.pdf").await?;
    let indexed_doc = create_test_document(&db, tenant_id, "indexed.pdf").await?;
    db.try_acquire_processing_lock(tenant_id, &indexed_doc)
        .await?;
    db.mark_document_indexed(tenant_id, &indexed_doc, Some(5))
        .await?;

    // Get retryable documents
    let retryable = db.get_retryable_documents(tenant_id, 10).await?;

    assert_eq!(
        retryable.len(),
        2,
        "Should return 2 retryable documents (failed but not at max_retries)"
    );

    // Verify all returned documents are in failed state with retries remaining
    for doc in &retryable {
        assert_eq!(doc.status, "failed");
        assert!(doc.retry_count < doc.max_retries);
    }

    // Test limit parameter
    let retryable_limited = db.get_retryable_documents(tenant_id, 1).await?;
    assert_eq!(retryable_limited.len(), 1, "Should respect limit parameter");

    Ok(())
}

// =============================================================================
// Test 5: Complete Success Path
// =============================================================================

/// Test the happy path: pending → processing → indexed
#[tokio::test]
async fn test_document_processing_success_path() -> anyhow::Result<()> {
    let db = init_test_db().await?;
    let tenant_id = "tenant-test";

    // Create document
    let doc_id = create_test_document(&db, tenant_id, "success.pdf").await?;

    // Verify pending state
    let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
    assert_eq!(doc.status, "pending");
    assert_eq!(doc.retry_count, 0);

    // Acquire processing lock
    let acquired = db.try_acquire_processing_lock(tenant_id, &doc_id).await?;
    assert!(acquired);

    // Verify processing state
    let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
    assert_eq!(doc.status, "processing");
    assert!(doc.processing_started_at.is_some());
    assert!(doc.processing_completed_at.is_none());

    // Mark as successfully indexed
    let page_count = 42i64;
    db.mark_document_indexed(tenant_id, &doc_id, Some(page_count))
        .await?;

    // Verify indexed state
    let doc = db.get_document(tenant_id, &doc_id).await?.unwrap();
    assert_eq!(doc.status, "indexed");
    assert_eq!(doc.page_count, Some(42i32));
    assert!(doc.processing_started_at.is_some());
    assert!(doc.processing_completed_at.is_some());
    assert!(doc.error_message.is_none());
    assert!(doc.error_code.is_none());

    // Verify cannot acquire lock on indexed document
    let cannot_acquire = db.try_acquire_processing_lock(tenant_id, &doc_id).await?;
    assert!(!cannot_acquire, "Cannot process indexed document");

    Ok(())
}
