//! Document Retry Logic Tests
//!
//! These tests verify the document retry functionality including:
//! - Marking documents as failed with error details
//! - Preparing documents for retry with retry count increments
//! - Respecting max retry limits
//! - Getting retryable documents (excluding exhausted retries)
//! - Tenant isolation for retry operations
//!
//! # Running Tests
//!
//! Run with: `AOS_SKIP_MIGRATION_SIGNATURES=1 cargo test -p adapteros-db --test document_retry_tests`

use adapteros_core::Result;
use adapteros_db::documents::CreateDocumentParams;
use adapteros_db::Db;
use uuid::Uuid;

/// Initialize an in-memory database for testing
async fn init_db() -> Result<Db> {
    let db = Db::connect("sqlite::memory:").await?;
    db.migrate().await?;
    Ok(db)
}

/// Create a test tenant
async fn create_test_tenant(db: &Db, tenant_id: &str) -> Result<()> {
    sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES (?, ?, 0)")
        .bind(tenant_id)
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to create tenant: {}", e))
        })?;
    Ok(())
}

/// Helper to create a test document
async fn create_test_document(db: &Db, tenant_id: &str, doc_id: &str) -> Result<()> {
    db.create_document(CreateDocumentParams {
        id: doc_id.to_string(),
        tenant_id: tenant_id.to_string(),
        name: format!("test-{}.pdf", doc_id),
        content_hash: format!("hash-{}", doc_id),
        file_path: format!("/tmp/{}.pdf", doc_id),
        file_size: 1000,
        mime_type: "application/pdf".to_string(),
        page_count: Some(10),
    })
    .await?;
    Ok(())
}

// =============================================================================
// TEST: mark_document_failed stores error details
// =============================================================================

#[tokio::test]
async fn test_mark_document_failed_stores_error_details() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;

    let doc_id = Uuid::new_v4().to_string();
    create_test_document(&db, "tenant-a", &doc_id).await?;

    // Mark document as failed with specific error details
    db.mark_document_failed(
        "tenant-a",
        &doc_id,
        "Failed to parse PDF: corrupt header",
        "PDF_PARSE_ERROR",
    )
    .await?;

    // Retrieve document and verify error details are stored
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");

    assert_eq!(doc.status, "failed", "Status should be 'failed'");
    assert_eq!(
        doc.error_message,
        Some("Failed to parse PDF: corrupt header".to_string()),
        "Error message should be stored"
    );
    assert_eq!(
        doc.error_code,
        Some("PDF_PARSE_ERROR".to_string()),
        "Error code should be stored"
    );
    assert!(
        doc.processing_completed_at.is_some(),
        "processing_completed_at should be set"
    );

    Ok(())
}

// =============================================================================
// TEST: prepare_document_retry increments retry_count
// =============================================================================

#[tokio::test]
async fn test_prepare_retry_increments_count() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;

    let doc_id = Uuid::new_v4().to_string();
    create_test_document(&db, "tenant-a", &doc_id).await?;

    // Initial state: retry_count = 0
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.retry_count, 0, "Initial retry_count should be 0");
    assert_eq!(doc.status, "pending", "Initial status should be 'pending'");

    // Mark as failed
    db.mark_document_failed("tenant-a", &doc_id, "First failure", "ERR_1")
        .await?;

    // First retry preparation
    let retry_ok = db.prepare_document_retry("tenant-a", &doc_id).await?;
    assert!(retry_ok, "First retry should succeed");

    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.retry_count, 1, "retry_count should be incremented to 1");
    assert_eq!(doc.status, "pending", "Status should be reset to 'pending'");
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

    // Mark as failed again
    db.mark_document_failed("tenant-a", &doc_id, "Second failure", "ERR_2")
        .await?;

    // Second retry preparation
    let retry_ok = db.prepare_document_retry("tenant-a", &doc_id).await?;
    assert!(retry_ok, "Second retry should succeed");

    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.retry_count, 2, "retry_count should be incremented to 2");
    assert_eq!(doc.status, "pending", "Status should be reset to 'pending'");

    Ok(())
}

// =============================================================================
// TEST: prepare_document_retry respects max_retries
// =============================================================================

#[tokio::test]
async fn test_prepare_retry_respects_max_retries() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;

    let doc_id = Uuid::new_v4().to_string();
    create_test_document(&db, "tenant-a", &doc_id).await?;

    // Verify initial max_retries (default is 3)
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.max_retries, 3, "Default max_retries should be 3");

    // Exhaust all retries (retry_count: 0 -> 1 -> 2 -> 3)
    for i in 0..3 {
        db.mark_document_failed("tenant-a", &doc_id, &format!("Failure {}", i + 1), "ERR")
            .await?;

        let retry_ok = db.prepare_document_retry("tenant-a", &doc_id).await?;
        assert!(
            retry_ok,
            "Retry {} should succeed (retry_count will be {})",
            i + 1,
            i + 1
        );

        let doc = db
            .get_document("tenant-a", &doc_id)
            .await?
            .expect("Document should exist");
        assert_eq!(doc.retry_count, i + 1, "retry_count should be {}", i + 1);
    }

    // Mark as failed one more time
    db.mark_document_failed("tenant-a", &doc_id, "Final failure", "ERR")
        .await?;

    // Verify retry_count = 3, max_retries = 3
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.retry_count, 3, "retry_count should be at max (3)");
    assert_eq!(doc.max_retries, 3, "max_retries should be 3");

    // Attempt to retry when retry_count >= max_retries
    let retry_ok = db.prepare_document_retry("tenant-a", &doc_id).await?;
    assert!(
        !retry_ok,
        "Retry should fail when retry_count >= max_retries"
    );

    // Verify state hasn't changed
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.status, "failed", "Status should remain 'failed'");
    assert_eq!(doc.retry_count, 3, "retry_count should remain at max (3)");

    Ok(())
}

// =============================================================================
// TEST: get_retryable_documents excludes exhausted retries
// =============================================================================

#[tokio::test]
async fn test_get_retryable_excludes_exhausted() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;

    // Create three documents
    let doc1_id = Uuid::new_v4().to_string();
    let doc2_id = Uuid::new_v4().to_string();
    let doc3_id = Uuid::new_v4().to_string();

    create_test_document(&db, "tenant-a", &doc1_id).await?;
    create_test_document(&db, "tenant-a", &doc2_id).await?;
    create_test_document(&db, "tenant-a", &doc3_id).await?;

    // doc1: failed with retry_count = 0 (retryable)
    db.mark_document_failed("tenant-a", &doc1_id, "Error 1", "ERR_1")
        .await?;

    // doc2: failed with retry_count = 2 (retryable)
    db.mark_document_failed("tenant-a", &doc2_id, "Error 2", "ERR_2")
        .await?;
    db.prepare_document_retry("tenant-a", &doc2_id).await?;
    db.mark_document_failed("tenant-a", &doc2_id, "Error 2 retry 1", "ERR_2")
        .await?;
    db.prepare_document_retry("tenant-a", &doc2_id).await?;
    db.mark_document_failed("tenant-a", &doc2_id, "Error 2 retry 2", "ERR_2")
        .await?;

    // doc3: failed with retry_count = 3 (NOT retryable - exhausted)
    db.mark_document_failed("tenant-a", &doc3_id, "Error 3", "ERR_3")
        .await?;
    for _ in 0..3 {
        db.prepare_document_retry("tenant-a", &doc3_id).await?;
        db.mark_document_failed("tenant-a", &doc3_id, "Error 3 retry", "ERR_3")
            .await?;
    }

    // Verify doc3 is at max retries
    let doc3 = db
        .get_document("tenant-a", &doc3_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc3.retry_count, 3, "doc3 should have retry_count = 3");
    assert_eq!(doc3.max_retries, 3, "doc3 should have max_retries = 3");

    // Get retryable documents
    let retryable = db.get_retryable_documents("tenant-a", 100).await?;

    assert_eq!(
        retryable.len(),
        2,
        "Should return 2 retryable documents (doc1 and doc2)"
    );

    let retryable_ids: Vec<String> = retryable.iter().map(|d| d.id.clone()).collect();
    assert!(retryable_ids.contains(&doc1_id), "doc1 should be retryable");
    assert!(retryable_ids.contains(&doc2_id), "doc2 should be retryable");
    assert!(
        !retryable_ids.contains(&doc3_id),
        "doc3 should NOT be retryable (exhausted)"
    );

    // Verify all returned documents have status = 'failed'
    for doc in &retryable {
        assert_eq!(
            doc.status, "failed",
            "All retryable docs should be 'failed'"
        );
        assert!(
            doc.retry_count < doc.max_retries,
            "All retryable docs should have retry_count < max_retries"
        );
    }

    Ok(())
}

// =============================================================================
// TEST: get_retryable_documents respects limit parameter
// =============================================================================

#[tokio::test]
async fn test_get_retryable_respects_limit() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;

    // Create 5 failed documents
    for i in 0..5 {
        let doc_id = Uuid::new_v4().to_string();
        create_test_document(&db, "tenant-a", &doc_id).await?;
        db.mark_document_failed("tenant-a", &doc_id, &format!("Error {}", i), "ERR")
            .await?;
    }

    // Get with limit = 3
    let retryable = db.get_retryable_documents("tenant-a", 3).await?;
    assert_eq!(
        retryable.len(),
        3,
        "Should return only 3 documents when limit = 3"
    );

    // Get with limit = 10 (more than available)
    let retryable = db.get_retryable_documents("tenant-a", 10).await?;
    assert_eq!(
        retryable.len(),
        5,
        "Should return all 5 documents when limit > available"
    );

    Ok(())
}

// =============================================================================
// TEST: retry operations enforce tenant isolation
// =============================================================================

#[tokio::test]
async fn test_retry_enforces_tenant_isolation() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    // Create a failed document in tenant-a
    let doc_id = Uuid::new_v4().to_string();
    create_test_document(&db, "tenant-a", &doc_id).await?;
    db.mark_document_failed("tenant-a", &doc_id, "Error A", "ERR_A")
        .await?;

    // Verify tenant-a can see their failed document
    let retryable_a = db.get_retryable_documents("tenant-a", 100).await?;
    assert_eq!(
        retryable_a.len(),
        1,
        "tenant-a should see their own failed document"
    );
    assert_eq!(retryable_a[0].id, doc_id);

    // Verify tenant-b CANNOT see tenant-a's failed document
    let retryable_b = db.get_retryable_documents("tenant-b", 100).await?;
    assert_eq!(
        retryable_b.len(),
        0,
        "tenant-b should NOT see tenant-a's failed document"
    );

    // Verify tenant-b CANNOT retry tenant-a's document
    let retry_ok = db.prepare_document_retry("tenant-b", &doc_id).await?;
    assert!(
        !retry_ok,
        "tenant-b should NOT be able to retry tenant-a's document"
    );

    // Verify document state hasn't changed (still failed with retry_count = 0)
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.status, "failed", "Status should remain 'failed'");
    assert_eq!(doc.retry_count, 0, "retry_count should remain 0");

    // Verify tenant-a CAN retry their own document
    let retry_ok = db.prepare_document_retry("tenant-a", &doc_id).await?;
    assert!(
        retry_ok,
        "tenant-a should be able to retry their own document"
    );

    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(
        doc.status, "pending",
        "Status should be 'pending' after retry"
    );
    assert_eq!(doc.retry_count, 1, "retry_count should be 1 after retry");

    Ok(())
}

// =============================================================================
// TEST: mark_document_failed enforces tenant isolation
// =============================================================================

#[tokio::test]
async fn test_mark_failed_enforces_tenant_isolation() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;
    create_test_tenant(&db, "tenant-b").await?;

    let doc_id = Uuid::new_v4().to_string();
    create_test_document(&db, "tenant-a", &doc_id).await?;

    // Verify initial state
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.status, "pending");
    assert!(doc.error_message.is_none());

    // Tenant-b attempts to mark tenant-a's document as failed
    db.mark_document_failed("tenant-b", &doc_id, "Cross-tenant error", "ERR_CROSS")
        .await?;

    // Verify document state is unchanged (tenant-b has no permission)
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(
        doc.status, "pending",
        "Status should remain 'pending' (not modified by tenant-b)"
    );
    assert!(
        doc.error_message.is_none(),
        "Error message should remain None (not modified by tenant-b)"
    );

    // Tenant-a marks their own document as failed
    db.mark_document_failed("tenant-a", &doc_id, "Tenant A error", "ERR_A")
        .await?;

    // Verify document is now failed with correct error
    let doc = db
        .get_document("tenant-a", &doc_id)
        .await?
        .expect("Document should exist");
    assert_eq!(doc.status, "failed", "Status should be 'failed'");
    assert_eq!(
        doc.error_message,
        Some("Tenant A error".to_string()),
        "Error message should be set by tenant-a"
    );
    assert_eq!(
        doc.error_code,
        Some("ERR_A".to_string()),
        "Error code should be set by tenant-a"
    );

    Ok(())
}

// =============================================================================
// TEST: prepare_retry only works on failed documents
// =============================================================================

#[tokio::test]
async fn test_prepare_retry_only_works_on_failed_status() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;

    // Document in 'pending' status
    let doc_pending = Uuid::new_v4().to_string();
    create_test_document(&db, "tenant-a", &doc_pending).await?;

    let retry_ok = db.prepare_document_retry("tenant-a", &doc_pending).await?;
    assert!(!retry_ok, "Cannot retry document in 'pending' status");

    // Document in 'indexed' status
    let doc_indexed = Uuid::new_v4().to_string();
    create_test_document(&db, "tenant-a", &doc_indexed).await?;
    db.mark_document_indexed("tenant-a", &doc_indexed, Some(10))
        .await?;

    let retry_ok = db.prepare_document_retry("tenant-a", &doc_indexed).await?;
    assert!(!retry_ok, "Cannot retry document in 'indexed' status");

    // Document in 'failed' status CAN be retried
    let doc_failed = Uuid::new_v4().to_string();
    create_test_document(&db, "tenant-a", &doc_failed).await?;
    db.mark_document_failed("tenant-a", &doc_failed, "Test error", "ERR")
        .await?;

    let retry_ok = db.prepare_document_retry("tenant-a", &doc_failed).await?;
    assert!(
        retry_ok,
        "Should be able to retry document in 'failed' status"
    );

    Ok(())
}

// =============================================================================
// TEST: get_retryable_documents returns oldest first
// =============================================================================

#[tokio::test]
async fn test_get_retryable_returns_oldest_first() -> Result<()> {
    let db = init_db().await?;
    create_test_tenant(&db, "tenant-a").await?;

    let mut doc_ids = Vec::new();
    for i in 0..3 {
        let doc_id = Uuid::new_v4().to_string();
        create_test_document(&db, "tenant-a", &doc_id).await?;
        db.mark_document_failed("tenant-a", &doc_id, &format!("Error {}", i), "ERR")
            .await?;
        doc_ids.push(doc_id);

        // Sleep to ensure different updated_at timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }

    // Get retryable documents (should be ordered by updated_at ASC)
    let retryable = db.get_retryable_documents("tenant-a", 100).await?;

    assert_eq!(retryable.len(), 3, "Should return all 3 documents");

    // Verify ordering: first created should be first in results
    assert_eq!(
        retryable[0].id, doc_ids[0],
        "Oldest document should be first"
    );
    assert_eq!(
        retryable[1].id, doc_ids[1],
        "Second oldest document should be second"
    );
    assert_eq!(
        retryable[2].id, doc_ids[2],
        "Newest document should be last"
    );

    Ok(())
}
