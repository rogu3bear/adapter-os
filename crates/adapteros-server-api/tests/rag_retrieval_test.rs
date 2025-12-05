//! Integration tests for RAG retrieval functionality.
//!
//! Tests the collection-scoped RAG retrieval and evidence DB integration.
//! These tests verify:
//! - doc_id parsing for the `{document_id}__chunk_{index}` format
//! - Collection filtering by extracted document_id
//! - Batch evidence insertion
//! - Full evidence audit trail creation

use adapteros_core::{B3Hash, Result};
use adapteros_db::{CreateEvidenceParams, Db};
use std::collections::HashSet;

/// Helper: Parse a RAG doc_id to extract the base document_id and chunk_index.
/// This mirrors the logic in streaming_infer.rs.
fn parse_rag_doc_id(doc_id: &str) -> Option<(String, i32)> {
    const CHUNK_SEPARATOR: &str = "__chunk_";

    if let Some(pos) = doc_id.rfind(CHUNK_SEPARATOR) {
        let document_id = doc_id[..pos].to_string();
        let chunk_index_str = &doc_id[pos + CHUNK_SEPARATOR.len()..];
        if let Ok(chunk_index) = chunk_index_str.parse::<i32>() {
            return Some((document_id, chunk_index));
        }
    }
    None
}

/// Test that CreateEvidenceParams can be constructed correctly
#[test]
fn test_evidence_params_construction() {
    let params = CreateEvidenceParams {
        tenant_id: "tenant-test".to_string(),
        inference_id: "chatcmpl-test-123".to_string(),
        session_id: Some("session-456".to_string()),
        message_id: None,
        document_id: "doc-001".to_string(),
        chunk_id: "doc-001__chunk_0".to_string(),
        page_number: Some(5),
        document_hash: "abc123def456".to_string(),
        chunk_hash: "def456abc123".to_string(),
        relevance_score: 0.95,
        rank: 0,
        context_hash: "context-hash-789".to_string(),
        rag_doc_ids: None,
        rag_scores: None,
        rag_collection_id: None,
    };

    assert_eq!(params.inference_id, "chatcmpl-test-123");
    assert_eq!(params.session_id, Some("session-456".to_string()));
    assert_eq!(params.relevance_score, 0.95);
    assert_eq!(params.rank, 0);
    // Verify document_id vs chunk_id distinction
    assert_eq!(params.document_id, "doc-001");
    assert_eq!(params.chunk_id, "doc-001__chunk_0");
}

/// Test that context hash is computed correctly
#[test]
fn test_context_hash_computation() {
    let context = "This is some RAG context for testing.\n\n---\n\nAdditional context.";
    let hash = B3Hash::hash(context.as_bytes());

    // Verify hash is deterministic
    let hash2 = B3Hash::hash(context.as_bytes());
    assert_eq!(hash.to_hex(), hash2.to_hex());

    // Verify different content produces different hash
    let different_context = "Different context";
    let different_hash = B3Hash::hash(different_context.as_bytes());
    assert_ne!(hash.to_hex(), different_hash.to_hex());
}

/// Test parsing of RAG doc_id format: {document_id}__chunk_{index}
#[test]
fn test_parse_rag_doc_id() {
    // Standard UUID document ID
    let result = parse_rag_doc_id("550e8400-e29b-41d4-a716-446655440000__chunk_0");
    assert_eq!(
        result,
        Some(("550e8400-e29b-41d4-a716-446655440000".to_string(), 0))
    );

    // Simple document ID with multiple chunks
    let result = parse_rag_doc_id("doc-123__chunk_42");
    assert_eq!(result, Some(("doc-123".to_string(), 42)));

    // Document ID with underscores (edge case - uses rfind)
    let result = parse_rag_doc_id("my_document_name__chunk_5");
    assert_eq!(result, Some(("my_document_name".to_string(), 5)));

    // Document ID with path-like format (sanitized)
    let result = parse_rag_doc_id("reports_2024_quarterly_pdf__chunk_10");
    assert_eq!(result, Some(("reports_2024_quarterly_pdf".to_string(), 10)));

    // Invalid formats
    assert_eq!(parse_rag_doc_id("doc-123"), None); // No separator
    assert_eq!(parse_rag_doc_id("doc-123_chunk_0"), None); // Wrong separator
    assert_eq!(parse_rag_doc_id("doc-123__chunk_abc"), None); // Non-numeric index
}

/// Test collection filtering with the actual doc_id pattern
#[test]
fn test_collection_filtering_with_doc_id_pattern() {
    // Collection contains these document IDs (from the database)
    let collection_doc_ids: HashSet<String> = vec![
        "doc-001".to_string(),
        "doc-003".to_string(),
        "doc-005".to_string(),
    ]
    .into_iter()
    .collect();

    // RAG results use the chunk format: {document_id}__chunk_{index}
    let rag_results = vec![
        ("doc-001__chunk_0", 0.95), // In collection
        ("doc-001__chunk_1", 0.92), // In collection (different chunk)
        ("doc-002__chunk_0", 0.90), // NOT in collection
        ("doc-003__chunk_0", 0.85), // In collection
        ("doc-004__chunk_0", 0.80), // NOT in collection
        ("doc-005__chunk_2", 0.75), // In collection
        ("doc-006__chunk_0", 0.70), // NOT in collection
    ];

    // Filter by collection membership using parsed document_id
    let filtered: Vec<_> = rag_results
        .into_iter()
        .filter(|(doc_id, _)| {
            if let Some((document_id, _)) = parse_rag_doc_id(doc_id) {
                collection_doc_ids.contains(&document_id)
            } else {
                false
            }
        })
        .take(5) // TOP_K
        .collect();

    // Should have 4 results (all from collection documents)
    assert_eq!(filtered.len(), 4);
    assert_eq!(filtered[0].0, "doc-001__chunk_0");
    assert_eq!(filtered[1].0, "doc-001__chunk_1");
    assert_eq!(filtered[2].0, "doc-003__chunk_0");
    assert_eq!(filtered[3].0, "doc-005__chunk_2");

    // Verify ranking is preserved
    for i in 1..filtered.len() {
        assert!(
            filtered[i - 1].1 > filtered[i].1,
            "Results should be sorted by score descending"
        );
    }
}

/// Test evidence ranking
#[test]
fn test_evidence_ranking() {
    // Simulate multiple evidence entries with different ranks
    let entries = vec![
        ("doc-1", 0.95, 0),
        ("doc-2", 0.85, 1),
        ("doc-3", 0.75, 2),
        ("doc-4", 0.65, 3),
        ("doc-5", 0.55, 4),
    ];

    for (i, (doc_id, score, rank)) in entries.iter().enumerate() {
        let params = CreateEvidenceParams {
            tenant_id: "tenant-evidence".to_string(),
            inference_id: "test-inference".to_string(),
            session_id: None,
            message_id: None,
            document_id: doc_id.to_string(),
            chunk_id: format!("{}__chunk_0", doc_id),
            page_number: None,
            document_hash: "hash".to_string(),
            chunk_hash: "hash".to_string(),
            relevance_score: *score,
            rank: *rank,
            context_hash: "ctx".to_string(),
            rag_doc_ids: None,
            rag_scores: None,
            rag_collection_id: None,
        };

        assert_eq!(params.rank, i as i32);
        // Verify scores are in descending order
        if i > 0 {
            assert!(params.relevance_score < entries[i - 1].1);
        }
    }
}

/// Integration test for batch evidence DB storage
///
/// Note: The inference_evidence table has FK constraints on document_id and chunk_id.
/// chunk_id references document_chunks.id (the actual DB record ID), not the RAG doc_id pattern.
/// In production, retrieve_rag_context looks up the chunk metadata to get the actual chunk ID.
#[tokio::test]
async fn test_batch_evidence_storage() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create required parent records for foreign key constraints
    let tenant_id = db.create_tenant("Test Tenant", false).await?;

    // Create a document
    let doc_id = "doc-test-batch";
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
         VALUES (?, ?, 'test.pdf', 'hash123', '/tmp/test.pdf', 1024, 'application/pdf', 'processed')",
    )
    .bind(doc_id)
    .bind(&tenant_id)
    .execute(db.pool())
    .await?;

    // Create chunks for the document (chunk IDs are database record IDs)
    let chunk_ids: Vec<String> = (0..3).map(|i| format!("chunk-db-{}", i)).collect();
    for (i, chunk_id) in chunk_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO document_chunks (id, document_id, chunk_index, page_number, chunk_hash)
             VALUES (?, ?, ?, ?, 'chunkhash')",
        )
        .bind(chunk_id)
        .bind(doc_id)
        .bind(i as i32)
        .bind((i + 1) as i32) // page_number = chunk_index + 1
        .execute(db.pool())
        .await?;
    }

    // Build batch evidence params
    // Note: chunk_id here must be the actual document_chunks.id (FK constraint)
    let context_hash = B3Hash::hash(b"test context").to_hex();
    let inference_id = "chatcmpl-batch-test";

    let evidence_params: Vec<CreateEvidenceParams> = chunk_ids
        .iter()
        .enumerate()
        .map(|(i, chunk_id)| CreateEvidenceParams {
            tenant_id: tenant_id.clone(),
            inference_id: inference_id.to_string(),
            session_id: None,
            message_id: None,
            document_id: doc_id.to_string(),
            chunk_id: chunk_id.clone(), // Actual chunk DB ID for FK constraint
            page_number: Some((i + 1) as i32),
            document_hash: B3Hash::hash(b"doc content").to_hex(),
            chunk_hash: B3Hash::hash(format!("chunk {} content", i).as_bytes()).to_hex(),
            relevance_score: 0.95 - (i as f64 * 0.1),
            rank: i as i32,
            context_hash: context_hash.clone(),
            rag_doc_ids: None,
            rag_scores: None,
            rag_collection_id: None,
        })
        .collect();

    // Batch insert all evidence entries
    let ids = db.create_inference_evidence_batch(evidence_params).await?;

    assert_eq!(ids.len(), 3, "Should have created 3 evidence entries");

    // Verify evidence was stored correctly
    let stored_evidence = db.get_evidence_by_inference(inference_id).await?;

    assert_eq!(stored_evidence.len(), 3);

    // Verify ranking is preserved
    assert_eq!(stored_evidence[0].rank, 0);
    assert_eq!(stored_evidence[1].rank, 1);
    assert_eq!(stored_evidence[2].rank, 2);

    // Verify document_id and chunk_id
    for (i, evidence) in stored_evidence.iter().enumerate() {
        assert_eq!(evidence.document_id, doc_id);
        assert_eq!(evidence.chunk_id, chunk_ids[i]);
    }

    // Verify page numbers
    assert_eq!(stored_evidence[0].page_number, Some(1));
    assert_eq!(stored_evidence[1].page_number, Some(2));
    assert_eq!(stored_evidence[2].page_number, Some(3));

    // Verify context hash is the same for all entries
    for evidence in &stored_evidence {
        assert_eq!(evidence.context_hash, context_hash);
    }

    Ok(())
}

/// Test context truncation logic
#[test]
fn test_context_truncation() {
    const MAX_CONTEXT_CHARS: usize = 100;

    let chunks = vec![
        "First chunk with some content.",
        "Second chunk with more content.",
        "Third chunk that would exceed limit.",
        "Fourth chunk never included.",
    ];

    let mut context = String::new();
    for (i, chunk) in chunks.iter().enumerate() {
        if context.len() + chunk.len() > MAX_CONTEXT_CHARS {
            break;
        }
        if i > 0 {
            context.push_str("\n\n---\n\n");
        }
        context.push_str(chunk);
    }

    // Verify context was truncated
    assert!(context.len() <= MAX_CONTEXT_CHARS + 10); // Allow separator overhead
    assert!(context.contains("First chunk"));
    assert!(context.contains("Second chunk"));
    assert!(!context.contains("Fourth chunk")); // Should be excluded
}

/// Test that empty batch insert returns empty vector
#[tokio::test]
async fn test_empty_batch_evidence() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let ids = db.create_inference_evidence_batch(vec![]).await?;

    assert!(ids.is_empty());

    Ok(())
}

/// Test document_id extraction edge cases
#[test]
fn test_document_id_extraction_edge_cases() {
    // Document ID that contains "__chunk_" in the name (edge case)
    // Uses rfind so it should find the rightmost occurrence
    let result = parse_rag_doc_id("doc__chunk_pattern__chunk_5");
    assert_eq!(result, Some(("doc__chunk_pattern".to_string(), 5)));

    // Very long chunk index
    let result = parse_rag_doc_id("doc__chunk_999999");
    assert_eq!(result, Some(("doc".to_string(), 999999)));

    // Empty document ID (still valid technically)
    let result = parse_rag_doc_id("__chunk_0");
    assert_eq!(result, Some(("".to_string(), 0)));
}

/// Test the unified document ID flow end-to-end
///
/// This test verifies the architectural fix for the RAG document ID disconnect:
/// 1. Document uploaded with UUID (documents.id)
/// 2. Document processed: chunks created with FK to document, RAG indexed with UUID-based doc_id
/// 3. Collection associates document by UUID
/// 4. RAG retrieval returns UUID-based doc_id
/// 5. Collection filtering works because UUID matches
/// 6. Evidence storage works because chunk.id lookup succeeds
#[tokio::test]
async fn test_unified_document_id_flow() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Step 1: Create tenant
    let tenant_id = db.create_tenant("Test Tenant", false).await?;

    // Step 2: Create document with UUID (simulates upload API)
    let document_uuid = "019350a2-7c8b-7f00-8a1b-123456789abc"; // V7 UUID format
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
         VALUES (?, ?, 'test.pdf', 'hash123', '/tmp/test.pdf', 1024, 'application/pdf', 'indexed')",
    )
    .bind(document_uuid)
    .bind(&tenant_id)
    .execute(db.pool())
    .await?;

    // Step 3: Create document chunks with proper FKs (simulates process_document endpoint)
    let chunk_ids: Vec<String> = (0..3)
        .map(|i| format!("019350a2-7c8b-7f00-chunk-00000000000{}", i))
        .collect();
    for (i, chunk_id) in chunk_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO document_chunks (id, document_id, chunk_index, page_number, chunk_hash, text_preview)
             VALUES (?, ?, ?, ?, 'chunkhash', 'preview text')",
        )
        .bind(chunk_id)
        .bind(document_uuid)
        .bind(i as i32)
        .bind((i + 1) as i32)
        .execute(db.pool())
        .await?;
    }

    // Step 4: Create collection and add document
    let collection_id = "test-collection-001";
    sqlx::query(
        "INSERT INTO document_collections (id, tenant_id, name) VALUES (?, ?, 'Test Collection')",
    )
    .bind(collection_id)
    .bind(&tenant_id)
    .execute(db.pool())
    .await?;

    sqlx::query("INSERT INTO collection_documents (collection_id, document_id) VALUES (?, ?)")
        .bind(collection_id)
        .bind(document_uuid)
        .execute(db.pool())
        .await?;

    // Step 5: Verify collection document IDs returns the UUID
    let collection_doc_ids: std::collections::HashSet<String> = db
        .list_collection_document_ids(collection_id)
        .await?
        .into_iter()
        .collect();

    assert!(
        collection_doc_ids.contains(document_uuid),
        "Collection should contain the document UUID"
    );

    // Step 6: Simulate RAG results with UUID-based doc_id format
    // (In production, rag_documents.doc_id would be "{document_uuid}__chunk_{index}")
    let rag_doc_ids = vec![
        format!("{}__chunk_0", document_uuid),
        format!("{}__chunk_1", document_uuid),
        format!("{}__chunk_2", document_uuid),
        "other-doc__chunk_0".to_string(), // Not in collection
    ];

    // Step 7: Filter by collection membership using parsed document_id
    let filtered_results: Vec<_> = rag_doc_ids
        .iter()
        .filter(|doc_id| {
            if let Some((parsed_doc_id, _)) = parse_rag_doc_id(doc_id) {
                collection_doc_ids.contains(&parsed_doc_id)
            } else {
                false
            }
        })
        .collect();

    assert_eq!(
        filtered_results.len(),
        3,
        "Should have 3 results from the collection document"
    );

    // Step 8: Verify chunk lookup works for evidence storage
    for doc_id in &filtered_results {
        let (parsed_doc_id, chunk_index) = parse_rag_doc_id(doc_id).unwrap();

        // This is the critical lookup that enables proper FK references
        let chunk = db
            .get_chunk_by_document_and_index(&parsed_doc_id, chunk_index)
            .await?;

        assert!(
            chunk.is_some(),
            "Chunk lookup should succeed for doc_id={}, chunk_index={}",
            parsed_doc_id,
            chunk_index
        );

        let chunk = chunk.unwrap();
        assert_eq!(chunk.document_id, document_uuid);
        assert_eq!(chunk.chunk_index, chunk_index);
    }

    // Step 9: Create evidence entries with proper FKs
    let context_hash = B3Hash::hash(b"test context").to_hex();
    let inference_id = "chatcmpl-unified-test";

    let evidence_params: Vec<CreateEvidenceParams> = filtered_results
        .iter()
        .enumerate()
        .map(|(i, doc_id)| {
            let (parsed_doc_id, chunk_index) = parse_rag_doc_id(doc_id).unwrap();
            CreateEvidenceParams {
                tenant_id: tenant_id.clone(),
                inference_id: inference_id.to_string(),
                session_id: None,
                message_id: None,
                document_id: parsed_doc_id,
                chunk_id: chunk_ids[chunk_index as usize].clone(), // Actual chunk DB ID for FK
                page_number: Some((chunk_index + 1) as i32),
                document_hash: "doc-hash".to_string(),
                chunk_hash: "chunk-hash".to_string(),
                relevance_score: 0.95 - (i as f64 * 0.1),
                rank: i as i32,
                context_hash: context_hash.clone(),
                rag_doc_ids: None,
                rag_scores: None,
                rag_collection_id: Some(collection_id.to_string()),
            }
        })
        .collect();

    let ids = db.create_inference_evidence_batch(evidence_params).await?;

    assert_eq!(ids.len(), 3, "Should have created 3 evidence entries");

    // Step 10: Verify evidence was stored correctly with valid FK references
    let stored_evidence = db.get_evidence_by_inference(inference_id).await?;
    assert_eq!(stored_evidence.len(), 3);

    for (i, evidence) in stored_evidence.iter().enumerate() {
        assert_eq!(evidence.document_id, document_uuid);
        assert_eq!(evidence.chunk_id, chunk_ids[i]);
        assert_eq!(evidence.rank, i as i32);
    }

    Ok(())
}

/// Test that the unified flow properly handles the case where document is NOT in collection
#[tokio::test]
async fn test_unified_flow_collection_filtering() -> Result<()> {
    let db = Db::new_in_memory().await?;
    let tenant_id = db.create_tenant("Test Tenant", false).await?;

    // Create two documents - only one will be in the collection
    let doc_in_collection = "019350a2-0001-7f00-8a1b-000000000001";
    let doc_not_in_collection = "019350a2-0002-7f00-8a1b-000000000002";

    for doc_id in [doc_in_collection, doc_not_in_collection] {
        sqlx::query(
            "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
             VALUES (?, ?, 'test.pdf', 'hash', '/tmp/test.pdf', 1024, 'application/pdf', 'indexed')",
        )
        .bind(doc_id)
        .bind(&tenant_id)
        .execute(db.pool())
        .await?;
    }

    // Create collection with only one document
    let collection_id = "filter-test-collection";
    sqlx::query(
        "INSERT INTO document_collections (id, tenant_id, name) VALUES (?, ?, 'Filter Test')",
    )
    .bind(collection_id)
    .bind(&tenant_id)
    .execute(db.pool())
    .await?;

    sqlx::query("INSERT INTO collection_documents (collection_id, document_id) VALUES (?, ?)")
        .bind(collection_id)
        .bind(doc_in_collection)
        .execute(db.pool())
        .await?;

    // Simulate RAG results from both documents
    let rag_results = vec![
        (format!("{}__chunk_0", doc_in_collection), 0.95),
        (format!("{}__chunk_0", doc_not_in_collection), 0.90),
        (format!("{}__chunk_1", doc_in_collection), 0.85),
        (format!("{}__chunk_1", doc_not_in_collection), 0.80),
    ];

    // Get collection document IDs
    let collection_doc_ids: std::collections::HashSet<String> = db
        .list_collection_document_ids(collection_id)
        .await?
        .into_iter()
        .collect();

    // Filter by collection membership
    let filtered: Vec<_> = rag_results
        .iter()
        .filter(|(doc_id, _)| {
            if let Some((parsed_doc_id, _)) = parse_rag_doc_id(doc_id) {
                collection_doc_ids.contains(&parsed_doc_id)
            } else {
                false
            }
        })
        .collect();

    // Should only have results from doc_in_collection
    assert_eq!(filtered.len(), 2);
    for (doc_id, _) in &filtered {
        let (parsed_doc_id, _) = parse_rag_doc_id(doc_id).unwrap();
        assert_eq!(
            parsed_doc_id, doc_in_collection,
            "Filtered results should only be from the collection document"
        );
    }

    Ok(())
}

// ============================================================
// PRD-08: RAG Determinism & Citation Trace Tests
// ============================================================

/// Test that RAG evidence includes aggregate trace fields (rag_doc_ids, rag_scores, rag_collection_id)
#[tokio::test]
async fn test_rag_evidence_with_trace_fields() -> Result<()> {
    let db = Db::new_in_memory().await?;

    // Create required parent records
    let tenant_id = db.create_tenant("Test Tenant", false).await?;

    // Create a document
    let doc_id = "doc-trace-test";
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
         VALUES (?, ?, 'test.pdf', 'hash123', '/tmp/test.pdf', 1024, 'application/pdf', 'processed')",
    )
    .bind(doc_id)
    .bind(&tenant_id)
    .execute(db.pool())
    .await?;

    // Create chunk
    let chunk_id = "chunk-trace-0";
    sqlx::query(
        "INSERT INTO document_chunks (id, document_id, chunk_index, page_number, chunk_hash)
         VALUES (?, ?, 0, 1, 'chunkhash')",
    )
    .bind(chunk_id)
    .bind(doc_id)
    .execute(db.pool())
    .await?;

    // Create collection
    let collection_id = "collection-trace-test";
    sqlx::query(
        "INSERT INTO document_collections (id, tenant_id, name) VALUES (?, ?, 'Trace Test Collection')",
    )
    .bind(collection_id)
    .bind(&tenant_id)
    .execute(db.pool())
    .await?;

    // Create evidence with RAG trace fields
    let rag_doc_ids = vec!["doc-1".to_string(), "doc-2".to_string()];
    let rag_scores = vec![0.95, 0.85];

    let params = CreateEvidenceParams {
        tenant_id: tenant_id.clone(),
        inference_id: "chatcmpl-trace-test".to_string(),
        session_id: None,
        message_id: None,
        document_id: doc_id.to_string(),
        chunk_id: chunk_id.to_string(),
        page_number: Some(1),
        document_hash: "doc-hash".to_string(),
        chunk_hash: "chunk-hash".to_string(),
        relevance_score: 0.95,
        rank: 0,
        context_hash: "context-hash".to_string(),
        rag_doc_ids: Some(rag_doc_ids.clone()),
        rag_scores: Some(rag_scores.clone()),
        rag_collection_id: Some(collection_id.to_string()),
    };

    let ids = db.create_inference_evidence_batch(vec![params]).await?;
    assert_eq!(ids.len(), 1);

    // Retrieve and verify trace fields
    let evidence = db.get_evidence_by_inference("chatcmpl-trace-test").await?;
    assert_eq!(evidence.len(), 1);

    let ev = &evidence[0];
    assert_eq!(
        ev.rag_collection_id.as_ref(),
        Some(&collection_id.to_string())
    );

    // Parse and verify rag_doc_ids
    if let Some(ref doc_ids_json) = ev.rag_doc_ids {
        let parsed_doc_ids: Vec<String> = serde_json::from_str(doc_ids_json).unwrap();
        assert_eq!(parsed_doc_ids, rag_doc_ids);
    } else {
        panic!("rag_doc_ids should not be None");
    }

    // Parse and verify rag_scores
    if let Some(ref scores_json) = ev.rag_scores {
        let parsed_scores: Vec<f64> = serde_json::from_str(scores_json).unwrap();
        assert_eq!(parsed_scores.len(), rag_scores.len());
        for (parsed, expected) in parsed_scores.iter().zip(rag_scores.iter()) {
            assert!((parsed - expected).abs() < 0.001);
        }
    } else {
        panic!("rag_scores should not be None");
    }

    Ok(())
}

/// Test deterministic ordering: score DESC, doc_id ASC (Ruleset #2)
#[test]
fn test_rag_deterministic_ordering() {
    // Simulate RAG results with same scores (requires tie-breaking)
    let mut results = vec![
        ("doc-c__chunk_0", 0.90),
        ("doc-a__chunk_0", 0.95),
        ("doc-b__chunk_0", 0.90), // Same score as doc-c, should come before it alphabetically
        ("doc-d__chunk_0", 0.85),
        ("doc-a__chunk_1", 0.95), // Same doc as first, different chunk
    ];

    // Sort by score DESC, then doc_id ASC (deterministic)
    results.sort_by(|a, b| {
        // First compare by score (descending)
        match b.1.partial_cmp(&a.1) {
            Some(std::cmp::Ordering::Equal) => {
                // If scores equal, compare by doc_id (ascending)
                a.0.cmp(b.0)
            }
            Some(ordering) => ordering,
            None => std::cmp::Ordering::Equal,
        }
    });

    // Verify order
    assert_eq!(results[0].0, "doc-a__chunk_0"); // 0.95, alphabetically first
    assert_eq!(results[1].0, "doc-a__chunk_1"); // 0.95, same doc, higher chunk
    assert_eq!(results[2].0, "doc-b__chunk_0"); // 0.90, alphabetically before doc-c
    assert_eq!(results[3].0, "doc-c__chunk_0"); // 0.90, alphabetically after doc-b
    assert_eq!(results[4].0, "doc-d__chunk_0"); // 0.85
}

/// Test that identical queries produce identical document ordering
#[test]
fn test_rag_identical_queries_same_ordering() {
    // Simulate the same set of results appearing in different initial orders
    let initial_order_1 = vec![
        ("doc-a__chunk_0", 0.95),
        ("doc-b__chunk_0", 0.90),
        ("doc-c__chunk_0", 0.85),
    ];

    let initial_order_2 = vec![
        ("doc-c__chunk_0", 0.85),
        ("doc-a__chunk_0", 0.95),
        ("doc-b__chunk_0", 0.90),
    ];

    // Apply deterministic sorting to both
    let sort_deterministically = |mut results: Vec<(&'static str, f64)>| {
        results.sort_by(|a, b| match b.1.partial_cmp(&a.1) {
            Some(std::cmp::Ordering::Equal) => a.0.cmp(b.0),
            Some(ordering) => ordering,
            None => std::cmp::Ordering::Equal,
        });
        results
    };

    let sorted_1 = sort_deterministically(initial_order_1);
    let sorted_2 = sort_deterministically(initial_order_2);

    // Both should produce identical ordering
    assert_eq!(sorted_1.len(), sorted_2.len());
    for (i, (r1, r2)) in sorted_1.iter().zip(sorted_2.iter()).enumerate() {
        assert_eq!(r1.0, r2.0, "doc_id mismatch at position {}", i);
        assert!(
            (r1.1 - r2.1).abs() < 0.001,
            "score mismatch at position {}",
            i
        );
    }
}

/// Test replay RAG state serialization and deserialization
#[tokio::test]
async fn test_replay_rag_state_serialization() -> Result<()> {
    use adapteros_db::rag_retrieval_audit::RagReplayState;

    let rag_state = RagReplayState {
        doc_ids: vec![
            "doc-1".to_string(),
            "doc-2".to_string(),
            "doc-3".to_string(),
        ],
        scores: vec![0.95, 0.85, 0.75],
        collection_id: Some("collection-123".to_string()),
        embedding_model_hash: "abc123def456".to_string(),
    };

    // Serialize to JSON (as stored in replay_sessions.rag_state_json)
    let json = serde_json::to_string(&rag_state)?;

    // Deserialize back
    let restored: RagReplayState = serde_json::from_str(&json)?;

    // Verify all fields match
    assert_eq!(restored.doc_ids, rag_state.doc_ids);
    assert_eq!(restored.scores.len(), rag_state.scores.len());
    for (r, o) in restored.scores.iter().zip(rag_state.scores.iter()) {
        assert!((r - o).abs() < 0.001);
    }
    assert_eq!(restored.collection_id, rag_state.collection_id);
    assert_eq!(
        restored.embedding_model_hash,
        rag_state.embedding_model_hash
    );

    Ok(())
}

/// Test get_documents_by_ids_ordered preserves input order
#[tokio::test]
async fn test_get_documents_by_ids_preserves_order() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let tenant_id = db.create_tenant("Test Tenant", false).await?;

    // Create documents in a specific order
    let doc_ids = vec![
        "doc-z".to_string(),
        "doc-a".to_string(),
        "doc-m".to_string(),
    ];

    for doc_id in &doc_ids {
        sqlx::query(
            "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
             VALUES (?, ?, 'test.pdf', 'hash', '/tmp/test.pdf', 1024, 'application/pdf', 'processed')",
        )
        .bind(doc_id)
        .bind(&tenant_id)
        .execute(db.pool())
        .await?;
    }

    // Retrieve in a different order
    let request_order = vec![
        "doc-a".to_string(),
        "doc-z".to_string(),
        "doc-m".to_string(),
    ];
    let results = db
        .get_documents_by_ids_ordered(&tenant_id, &request_order)
        .await?;

    // Verify order is preserved
    assert_eq!(results.len(), 3);
    assert!(results[0].is_some());
    assert_eq!(results[0].as_ref().unwrap().id, "doc-a");
    assert!(results[1].is_some());
    assert_eq!(results[1].as_ref().unwrap().id, "doc-z");
    assert!(results[2].is_some());
    assert_eq!(results[2].as_ref().unwrap().id, "doc-m");

    Ok(())
}

/// Test get_documents_by_ids_ordered handles missing documents
#[tokio::test]
async fn test_get_documents_handles_missing() -> Result<()> {
    let db = Db::new_in_memory().await?;

    let tenant_id = db.create_tenant("Test Tenant", false).await?;

    // Create only one document
    sqlx::query(
        "INSERT INTO documents (id, tenant_id, name, content_hash, file_path, file_size, mime_type, status)
         VALUES ('existing-doc', ?, 'test.pdf', 'hash', '/tmp/test.pdf', 1024, 'application/pdf', 'processed')",
    )
    .bind(&tenant_id)
    .execute(db.pool())
    .await?;

    // Request including non-existent documents
    let request = vec![
        "missing-1".to_string(),
        "existing-doc".to_string(),
        "missing-2".to_string(),
    ];

    let results = db
        .get_documents_by_ids_ordered(&tenant_id, &request)
        .await?;

    // Verify: None for missing, Some for existing, order preserved
    assert_eq!(results.len(), 3);
    assert!(results[0].is_none()); // missing-1
    assert!(results[1].is_some()); // existing-doc
    assert_eq!(results[1].as_ref().unwrap().id, "existing-doc");
    assert!(results[2].is_none()); // missing-2

    Ok(())
}
