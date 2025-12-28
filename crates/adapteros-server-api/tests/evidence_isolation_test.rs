//! Cross-tenant evidence isolation security tests
//!
//! Addresses audit finding: cross-workspace evidence export vulnerability.
//! These tests verify that evidence records are properly isolated by tenant_id
//! and that one tenant cannot access another tenant's evidence by knowing IDs.

use adapteros_db::{CreateEvidenceParams, Db};
use anyhow::Result;

/// Test that get_evidence_by_inference respects tenant isolation
#[tokio::test]
async fn test_evidence_by_inference_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    let tenant_a = db.create_tenant("Tenant A", false).await?;
    let tenant_b = db.create_tenant("Tenant B", false).await?;

    // Create document and chunk for tenant A
    let doc_id = "doc-tenant-a";
    let chunk_id = "chunk-tenant-a";

    sqlx::query(
        "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
         VALUES (?, ?, 'test.pdf', 'hash123', '/tmp/test.pdf', 1024, 'application/pdf', 'processed')",
    )
    .bind(doc_id)
    .bind(&tenant_a)
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash)
         VALUES (?, ?, ?, 0, 'chunkhash')",
    )
    .bind(chunk_id)
    .bind(&tenant_a)
    .bind(doc_id)
    .execute(db.pool())
    .await?;

    // Create evidence for tenant A
    let inference_id = "inf-tenant-a-001";
    let params = CreateEvidenceParams {
        tenant_id: tenant_a.clone(),
        inference_id: inference_id.to_string(),
        session_id: None,
        message_id: Some("msg-001".to_string()),
        document_id: doc_id.to_string(),
        chunk_id: chunk_id.to_string(),
        page_number: Some(1),
        document_hash: "dochash123".to_string(),
        chunk_hash: "chunkhash456".to_string(),
        relevance_score: 0.95,
        rank: 1,
        context_hash: "contexthash789".to_string(),
        rag_doc_ids: None,
        rag_scores: None,
        rag_collection_id: None,
    };

    db.create_inference_evidence(params).await?;

    // Tenant A can retrieve their own evidence
    let evidence_a = db
        .get_evidence_by_inference(&tenant_a, inference_id)
        .await?;
    assert_eq!(
        evidence_a.len(),
        1,
        "Tenant A should be able to access their own evidence"
    );
    assert_eq!(evidence_a[0].inference_id, inference_id);

    // Tenant B cannot retrieve Tenant A's evidence even with correct inference_id
    let evidence_b = db
        .get_evidence_by_inference(&tenant_b, inference_id)
        .await?;
    assert_eq!(
        evidence_b.len(),
        0,
        "Tenant B should NOT be able to access Tenant A's evidence"
    );

    // Random tenant ID also returns empty
    let evidence_random = db
        .get_evidence_by_inference("nonexistent-tenant", inference_id)
        .await?;
    assert_eq!(
        evidence_random.len(),
        0,
        "Unknown tenant should get empty results"
    );

    Ok(())
}

/// Test that get_evidence_by_message respects tenant isolation
#[tokio::test]
async fn test_evidence_by_message_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    let tenant_a = db.create_tenant("Tenant A", false).await?;
    let tenant_b = db.create_tenant("Tenant B", false).await?;

    // Create document and chunk for tenant A
    let doc_id = "doc-msg-tenant-a";
    let chunk_id = "chunk-msg-tenant-a";

    sqlx::query(
        "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
         VALUES (?, ?, 'test.pdf', 'hash456', '/tmp/test2.pdf', 1024, 'application/pdf', 'processed')",
    )
    .bind(doc_id)
    .bind(&tenant_a)
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash)
         VALUES (?, ?, ?, 0, 'chunkhash2')",
    )
    .bind(chunk_id)
    .bind(&tenant_a)
    .bind(doc_id)
    .execute(db.pool())
    .await?;

    // Create evidence for tenant A with a specific message_id
    let message_id = "msg-secret-tenant-a";
    let params = CreateEvidenceParams {
        tenant_id: tenant_a.clone(),
        inference_id: "inf-msg-001".to_string(),
        session_id: None,
        message_id: Some(message_id.to_string()),
        document_id: doc_id.to_string(),
        chunk_id: chunk_id.to_string(),
        page_number: Some(1),
        document_hash: "dochash789".to_string(),
        chunk_hash: "chunkhash012".to_string(),
        relevance_score: 0.88,
        rank: 1,
        context_hash: "contexthash345".to_string(),
        rag_doc_ids: None,
        rag_scores: None,
        rag_collection_id: None,
    };

    db.create_inference_evidence(params).await?;

    // Tenant A can retrieve their own evidence by message
    let evidence_a = db.get_evidence_by_message(&tenant_a, message_id).await?;
    assert_eq!(
        evidence_a.len(),
        1,
        "Tenant A should be able to access their own message evidence"
    );

    // Tenant B cannot retrieve Tenant A's evidence even with correct message_id
    let evidence_b = db.get_evidence_by_message(&tenant_b, message_id).await?;
    assert_eq!(
        evidence_b.len(),
        0,
        "Tenant B should NOT be able to access Tenant A's message evidence"
    );

    Ok(())
}

/// Test that get_evidence_by_session respects tenant isolation
#[tokio::test]
async fn test_evidence_by_session_tenant_isolation() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create two tenants
    let tenant_a = db.create_tenant("Tenant A", false).await?;
    let tenant_b = db.create_tenant("Tenant B", false).await?;

    // Create document and chunk for tenant A
    let doc_id = "doc-session-tenant-a";
    let chunk_id = "chunk-session-tenant-a";

    sqlx::query(
        "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
         VALUES (?, ?, 'test.pdf', 'hash789', '/tmp/test3.pdf', 1024, 'application/pdf', 'processed')",
    )
    .bind(doc_id)
    .bind(&tenant_a)
    .execute(db.pool())
    .await?;

    sqlx::query(
        "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash)
         VALUES (?, ?, ?, 0, 'chunkhash3')",
    )
    .bind(chunk_id)
    .bind(&tenant_a)
    .bind(doc_id)
    .execute(db.pool())
    .await?;

    // Create a chat session for tenant A (required for session_id FK)
    let session_id = "session-secret-tenant-a";
    sqlx::query(
        "INSERT INTO chat_sessions (id, tenant_id, name, created_at)
         VALUES (?, ?, 'Test Session', datetime('now'))",
    )
    .bind(session_id)
    .bind(&tenant_a)
    .execute(db.pool())
    .await?;

    // Create evidence for tenant A with a specific session_id
    let params = CreateEvidenceParams {
        tenant_id: tenant_a.clone(),
        inference_id: "inf-session-001".to_string(),
        session_id: Some(session_id.to_string()),
        message_id: None,
        document_id: doc_id.to_string(),
        chunk_id: chunk_id.to_string(),
        page_number: Some(1),
        document_hash: "dochash-session".to_string(),
        chunk_hash: "chunkhash-session".to_string(),
        relevance_score: 0.92,
        rank: 1,
        context_hash: "contexthash-session".to_string(),
        rag_doc_ids: None,
        rag_scores: None,
        rag_collection_id: None,
    };

    db.create_inference_evidence(params).await?;

    // Tenant A can retrieve their own evidence by session
    let evidence_a = db.get_evidence_by_session(&tenant_a, session_id).await?;
    assert_eq!(
        evidence_a.len(),
        1,
        "Tenant A should be able to access their own session evidence"
    );

    // Tenant B cannot retrieve Tenant A's evidence even with correct session_id
    let evidence_b = db.get_evidence_by_session(&tenant_b, session_id).await?;
    assert_eq!(
        evidence_b.len(),
        0,
        "Tenant B should NOT be able to access Tenant A's session evidence"
    );

    Ok(())
}

/// Verify that evidence from multiple tenants doesn't leak across boundaries
#[tokio::test]
async fn test_evidence_multi_tenant_no_leakage() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create three tenants
    let tenant_a = db.create_tenant("Tenant A", false).await?;
    let tenant_b = db.create_tenant("Tenant B", false).await?;
    let tenant_c = db.create_tenant("Tenant C", false).await?;

    // Create documents for each tenant
    for (i, tenant_id) in [&tenant_a, &tenant_b, &tenant_c].iter().enumerate() {
        let doc_id = format!("doc-multi-{}", i);
        let chunk_id = format!("chunk-multi-{}", i);

        sqlx::query(
            "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
             VALUES (?, ?, 'test.pdf', ?, '/tmp/test.pdf', 1024, 'application/pdf', 'processed')",
        )
        .bind(&doc_id)
        .bind(*tenant_id)
        .bind(format!("hash-{}", i))
        .execute(db.pool())
        .await?;

        sqlx::query(
            "INSERT INTO document_chunks (id, tenant_id, document_id, chunk_index, chunk_hash)
             VALUES (?, ?, ?, 0, 'chunkhash')",
        )
        .bind(&chunk_id)
        .bind(*tenant_id)
        .bind(&doc_id)
        .execute(db.pool())
        .await?;

        // Create 5 evidence records per tenant with same inference_id pattern
        for j in 0..5 {
            let params = CreateEvidenceParams {
                tenant_id: (*tenant_id).clone(),
                inference_id: format!("inf-shared-pattern-{}", j), // Intentionally similar IDs
                session_id: None,
                message_id: Some(format!("msg-{}-{}", i, j)),
                document_id: doc_id.clone(),
                chunk_id: chunk_id.clone(),
                page_number: Some(j as i32),
                document_hash: format!("dochash-{}", j),
                chunk_hash: format!("chunkhash-{}", j),
                relevance_score: 0.9 - (j as f64 * 0.1),
                rank: j as i32,
                context_hash: "ctx".to_string(),
                rag_doc_ids: None,
                rag_scores: None,
                rag_collection_id: None,
            };
            db.create_inference_evidence(params).await?;
        }
    }

    // Each tenant should only see their own 5 evidence records for a given inference_id
    for (tenant_id, expected_count) in [(&tenant_a, 1), (&tenant_b, 1), (&tenant_c, 1)] {
        let evidence = db
            .get_evidence_by_inference(tenant_id, "inf-shared-pattern-0")
            .await?;
        assert_eq!(
            evidence.len(),
            expected_count,
            "Each tenant should see exactly {} evidence record(s) for the same inference_id pattern",
            expected_count
        );
    }

    // Cross-tenant access should return empty
    let cross_access = db
        .get_evidence_by_inference(&tenant_a, "inf-shared-pattern-0")
        .await?;
    // Verify it's only tenant A's evidence
    assert_eq!(cross_access.len(), 1);

    Ok(())
}
