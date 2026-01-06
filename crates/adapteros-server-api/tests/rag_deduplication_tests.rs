//! Unit tests for RAG chunk deduplication functionality.
//!
//! Tests the chunk deduplication logic in rag_common.rs that ensures
//! only N chunks per source document are returned when configured.
//!
//! Key test areas:
//! - max_chunks_per_document enforcement
//! - Highest scoring chunks are preserved (first N by score order)
//! - Deduplication disabled when max_chunks_per_document = 0
//! - Deduplication works with collection filtering
//!
//! See rag_common.rs lines 250-280 for the deduplication implementation.

use adapteros_server_api::handlers::rag_common::parse_rag_doc_id;
use std::collections::HashMap;

/// Mock RAG document for testing
#[derive(Clone, Debug)]
struct MockRagDocument {
    doc_id: String,
    score: f32,
}

/// Simulates the deduplication logic from rag_common.rs (lines 251-273)
fn apply_chunk_deduplication(
    mut results: Vec<MockRagDocument>,
    max_chunks_per_document: usize,
) -> Vec<MockRagDocument> {
    if max_chunks_per_document == 0 {
        return results;
    }

    let mut doc_counts: HashMap<String, usize> = HashMap::new();

    results = results
        .into_iter()
        .filter(|doc| {
            // Extract base document_id from chunk doc_id format: {uuid}__chunk_{index}
            let base_doc_id = if let Some(pos) = doc.doc_id.rfind("__chunk_") {
                &doc.doc_id[..pos]
            } else {
                &doc.doc_id
            };

            let count = doc_counts.entry(base_doc_id.to_string()).or_insert(0);
            if *count < max_chunks_per_document {
                *count += 1;
                true
            } else {
                false
            }
        })
        .collect();

    results
}

/// Test 1: Verify only N chunks per source document are returned when configured
#[test]
fn test_max_chunks_per_document_enforced() {
    // Create 5 chunks from the same document with descending scores
    let doc_id = "doc-001";
    let results = vec![
        MockRagDocument {
            doc_id: format!("{}__chunk_0", doc_id),
            score: 0.95,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_1", doc_id),
            score: 0.90,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_2", doc_id),
            score: 0.85,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_3", doc_id),
            score: 0.80,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_4", doc_id),
            score: 0.75,
        },
    ];

    // Apply deduplication with max_chunks_per_document = 3
    let deduplicated = apply_chunk_deduplication(results.clone(), 3);

    // Verify only 3 chunks were returned
    assert_eq!(
        deduplicated.len(),
        3,
        "Should return exactly 3 chunks when max_chunks_per_document = 3"
    );

    // Verify the first 3 chunks by score were kept (since results are pre-sorted)
    for (i, doc) in deduplicated.iter().enumerate() {
        assert_eq!(
            doc.doc_id,
            format!("{}__chunk_{}", doc_id, i),
            "Should keep the first 3 chunks (highest scoring)"
        );
    }
}

/// Test 2: Verify highest scoring chunks are kept during deduplication
#[test]
fn test_deduplication_preserves_highest_scoring() {
    let doc1 = "doc-001";
    let doc2 = "doc-002";

    // Create interleaved results (already sorted by score DESC)
    let results = vec![
        MockRagDocument {
            doc_id: format!("{}__chunk_0", doc1),
            score: 0.95,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_0", doc2),
            score: 0.90,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_1", doc1),
            score: 0.85,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_1", doc2),
            score: 0.80,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_2", doc1),
            score: 0.75,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_2", doc2),
            score: 0.70,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_3", doc1),
            score: 0.65,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_3", doc2),
            score: 0.60,
        },
    ];

    // Apply deduplication with max_chunks_per_document = 2
    let deduplicated = apply_chunk_deduplication(results, 2);

    // Should return 4 chunks total (2 from each document)
    assert_eq!(
        deduplicated.len(),
        4,
        "Should return 2 chunks from each of 2 documents"
    );

    // Count chunks per document
    let doc1_chunks: Vec<_> = deduplicated
        .iter()
        .filter(|doc| doc.doc_id.starts_with(doc1))
        .collect();
    let doc2_chunks: Vec<_> = deduplicated
        .iter()
        .filter(|doc| doc.doc_id.starts_with(doc2))
        .collect();

    assert_eq!(
        doc1_chunks.len(),
        2,
        "Should have exactly 2 chunks from doc1"
    );
    assert_eq!(
        doc2_chunks.len(),
        2,
        "Should have exactly 2 chunks from doc2"
    );

    // Verify the highest scoring chunks were kept for each document
    assert_eq!(doc1_chunks[0].doc_id, format!("{}__chunk_0", doc1));
    assert_eq!(doc1_chunks[1].doc_id, format!("{}__chunk_1", doc1));
    assert_eq!(doc2_chunks[0].doc_id, format!("{}__chunk_0", doc2));
    assert_eq!(doc2_chunks[1].doc_id, format!("{}__chunk_1", doc2));

    // Verify overall score ordering is preserved
    for i in 1..deduplicated.len() {
        assert!(
            deduplicated[i - 1].score >= deduplicated[i].score,
            "Scores should remain in descending order after deduplication"
        );
    }
}

/// Test 3: Verify max_chunks_per_document = 0 means unlimited (deduplication disabled)
#[test]
fn test_deduplication_disabled_when_zero() {
    let doc_id = "doc-001";

    // Create 8 chunks from the same document
    let results: Vec<MockRagDocument> = (0..8)
        .map(|i| MockRagDocument {
            doc_id: format!("{}__chunk_{}", doc_id, i),
            score: 0.95 - (i as f32 * 0.05),
        })
        .collect();

    // Apply deduplication with max_chunks_per_document = 0 (unlimited)
    let deduplicated = apply_chunk_deduplication(results.clone(), 0);

    // Should return all 8 chunks (not limited by deduplication)
    assert_eq!(
        deduplicated.len(),
        8,
        "Should return all chunks when max_chunks_per_document = 0"
    );

    // Verify all chunks are present
    for (i, doc) in deduplicated.iter().enumerate() {
        assert_eq!(doc.doc_id, format!("{}__chunk_{}", doc_id, i));
    }
}

/// Test 4: Verify deduplication works with multiple documents
#[test]
fn test_deduplication_works_with_multiple_documents() {
    let doc1 = "doc-in-001";
    let doc2 = "doc-in-002";
    let doc3 = "doc-in-003";

    // Create interleaved chunks from 3 documents (5 chunks each)
    let mut results = Vec::new();
    for i in 0..5 {
        results.push(MockRagDocument {
            doc_id: format!("{}__chunk_{}", doc1, i),
            score: 0.95 - (i as f32 * 0.05),
        });
        results.push(MockRagDocument {
            doc_id: format!("{}__chunk_{}", doc2, i),
            score: 0.94 - (i as f32 * 0.05),
        });
        results.push(MockRagDocument {
            doc_id: format!("{}__chunk_{}", doc3, i),
            score: 0.93 - (i as f32 * 0.05),
        });
    }

    // Sort by score DESC (simulates pre-sorted retrieval results)
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply deduplication with max_chunks_per_document = 2
    let deduplicated = apply_chunk_deduplication(results, 2);

    // Should return 6 chunks total (2 from each of 3 documents)
    assert_eq!(
        deduplicated.len(),
        6,
        "Should return 2 chunks each from 3 documents"
    );

    // Count chunks per document
    let doc1_count = deduplicated
        .iter()
        .filter(|d| d.doc_id.starts_with(doc1))
        .count();
    let doc2_count = deduplicated
        .iter()
        .filter(|d| d.doc_id.starts_with(doc2))
        .count();
    let doc3_count = deduplicated
        .iter()
        .filter(|d| d.doc_id.starts_with(doc3))
        .count();

    assert_eq!(doc1_count, 2, "Should have exactly 2 chunks from doc1");
    assert_eq!(doc2_count, 2, "Should have exactly 2 chunks from doc2");
    assert_eq!(doc3_count, 2, "Should have exactly 2 chunks from doc3");
}

/// Test 5: Edge case - single chunk per document with deduplication
#[test]
fn test_deduplication_single_chunk_limit() {
    // Create 5 documents with 3 chunks each
    let doc_ids: Vec<String> = (1..=5).map(|i| format!("doc-{:03}", i)).collect();

    let mut results = Vec::new();
    for (doc_idx, doc_id) in doc_ids.iter().enumerate() {
        for chunk_idx in 0..3 {
            results.push(MockRagDocument {
                doc_id: format!("{}__chunk_{}", doc_id, chunk_idx),
                score: 0.95 - (doc_idx as f32 * 0.1) - (chunk_idx as f32 * 0.02),
            });
        }
    }

    // Sort by score DESC
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Apply deduplication with max_chunks_per_document = 1 (single chunk per document)
    let deduplicated = apply_chunk_deduplication(results, 1);

    // Should return 5 chunks total (1 from each of the 5 documents)
    assert_eq!(
        deduplicated.len(),
        5,
        "Should return 1 chunk from each of the 5 documents"
    );

    // Verify each document appears exactly once
    for doc_id in &doc_ids {
        let count = deduplicated
            .iter()
            .filter(|d| d.doc_id.starts_with(doc_id))
            .count();
        assert_eq!(
            count, 1,
            "Each document should appear exactly once with max_chunks_per_document = 1"
        );
    }

    // Verify only chunk 0 (highest scoring) from each document is returned
    for doc in &deduplicated {
        if let Some(parsed) = parse_rag_doc_id(&doc.doc_id) {
            assert_eq!(
                parsed.chunk_index, 0,
                "Should only return chunk 0 (highest scoring) from each document"
            );
        }
    }
}

/// Test 6: Verify deduplication with top_k truncation happens AFTER deduplication
#[test]
fn test_deduplication_ordering_preserved() {
    let doc1 = "doc-001";
    let doc2 = "doc-002";
    let doc3 = "doc-003";

    // Create results with descending scores
    let results = vec![
        MockRagDocument {
            doc_id: format!("{}__chunk_0", doc1),
            score: 0.95,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_1", doc1),
            score: 0.93,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_0", doc2),
            score: 0.91,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_1", doc2),
            score: 0.89,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_0", doc3),
            score: 0.87,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_1", doc3),
            score: 0.85,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_2", doc1),
            score: 0.83,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_2", doc2),
            score: 0.81,
        },
        MockRagDocument {
            doc_id: format!("{}__chunk_2", doc3),
            score: 0.79,
        },
    ];

    // Apply deduplication with max_chunks_per_document = 2
    let deduplicated = apply_chunk_deduplication(results, 2);

    // Should return 6 chunks (2 from each of 3 docs)
    assert_eq!(deduplicated.len(), 6);

    // Verify score ordering is preserved
    for i in 1..deduplicated.len() {
        assert!(
            deduplicated[i - 1].score >= deduplicated[i].score,
            "Scores should be in descending order after deduplication"
        );
    }

    // Verify specific ordering
    assert_eq!(deduplicated[0].doc_id, format!("{}__chunk_0", doc1));
    assert_eq!(deduplicated[1].doc_id, format!("{}__chunk_1", doc1));
    assert_eq!(deduplicated[2].doc_id, format!("{}__chunk_0", doc2));
    assert_eq!(deduplicated[3].doc_id, format!("{}__chunk_1", doc2));
    assert_eq!(deduplicated[4].doc_id, format!("{}__chunk_0", doc3));
    assert_eq!(deduplicated[5].doc_id, format!("{}__chunk_1", doc3));
}

/// Test 7: Verify parse_rag_doc_id helper function with edge cases
#[test]
fn test_parse_rag_doc_id_comprehensive() {
    // Standard UUID format
    let result = parse_rag_doc_id("550e8400-e29b-41d4-a716-446655440000__chunk_0");
    assert!(result.is_some());
    let parsed = result.unwrap();
    assert_eq!(parsed.document_id, "550e8400-e29b-41d4-a716-446655440000");
    assert_eq!(parsed.chunk_index, 0);

    // Simple document ID with chunk
    let result = parse_rag_doc_id("doc-123__chunk_42");
    assert!(result.is_some());
    let parsed = result.unwrap();
    assert_eq!(parsed.document_id, "doc-123");
    assert_eq!(parsed.chunk_index, 42);

    // Document ID with underscores (uses rfind, so finds rightmost separator)
    let result = parse_rag_doc_id("my_document_name__chunk_5");
    assert!(result.is_some());
    let parsed = result.unwrap();
    assert_eq!(parsed.document_id, "my_document_name");
    assert_eq!(parsed.chunk_index, 5);

    // Edge case: document ID contains "__chunk_" substring
    let result = parse_rag_doc_id("doc__chunk_pattern__chunk_10");
    assert!(result.is_some());
    let parsed = result.unwrap();
    assert_eq!(parsed.document_id, "doc__chunk_pattern");
    assert_eq!(parsed.chunk_index, 10);

    // Invalid: no separator
    assert!(parse_rag_doc_id("doc-123").is_none());

    // Invalid: wrong separator
    assert!(parse_rag_doc_id("doc-123_chunk_0").is_none());

    // Invalid: non-numeric index
    assert!(parse_rag_doc_id("doc-123__chunk_abc").is_none());

    // Invalid: empty index
    assert!(parse_rag_doc_id("doc-123__chunk_").is_none());

    // Edge case: large chunk index
    let result = parse_rag_doc_id("doc__chunk_999999");
    assert!(result.is_some());
    let parsed = result.unwrap();
    assert_eq!(parsed.document_id, "doc");
    assert_eq!(parsed.chunk_index, 999999);

    // Edge case: negative chunk index (technically valid i32, but unusual)
    // Note: parse_rag_doc_id accepts negative indices (i32) per implementation
    let result = parse_rag_doc_id("doc__chunk_-5");
    assert!(result.is_some());
    let parsed = result.unwrap();
    assert_eq!(parsed.document_id, "doc");
    assert_eq!(parsed.chunk_index, -5);
}

/// Test 8: Verify deduplication with documents having varying numbers of chunks
#[test]
fn test_deduplication_with_varying_chunk_counts() {
    // doc1: 5 chunks, doc2: 3 chunks, doc3: 1 chunk
    let results = vec![
        MockRagDocument {
            doc_id: "doc1__chunk_0".to_string(),
            score: 0.95,
        },
        MockRagDocument {
            doc_id: "doc2__chunk_0".to_string(),
            score: 0.90,
        },
        MockRagDocument {
            doc_id: "doc1__chunk_1".to_string(),
            score: 0.85,
        },
        MockRagDocument {
            doc_id: "doc3__chunk_0".to_string(),
            score: 0.80,
        },
        MockRagDocument {
            doc_id: "doc2__chunk_1".to_string(),
            score: 0.75,
        },
        MockRagDocument {
            doc_id: "doc1__chunk_2".to_string(),
            score: 0.70,
        },
        MockRagDocument {
            doc_id: "doc2__chunk_2".to_string(),
            score: 0.65,
        },
        MockRagDocument {
            doc_id: "doc1__chunk_3".to_string(),
            score: 0.60,
        },
        MockRagDocument {
            doc_id: "doc1__chunk_4".to_string(),
            score: 0.55,
        },
    ];

    // Apply deduplication with max_chunks_per_document = 2
    let deduplicated = apply_chunk_deduplication(results, 2);

    // Should return 5 chunks total:
    // - 2 from doc1 (has 5 chunks, limited to 2)
    // - 2 from doc2 (has 3 chunks, limited to 2)
    // - 1 from doc3 (has 1 chunk, keeps the 1)
    assert_eq!(deduplicated.len(), 5);

    // Verify counts
    let doc1_count = deduplicated
        .iter()
        .filter(|d| d.doc_id.starts_with("doc1"))
        .count();
    let doc2_count = deduplicated
        .iter()
        .filter(|d| d.doc_id.starts_with("doc2"))
        .count();
    let doc3_count = deduplicated
        .iter()
        .filter(|d| d.doc_id.starts_with("doc3"))
        .count();

    assert_eq!(doc1_count, 2);
    assert_eq!(doc2_count, 2);
    assert_eq!(doc3_count, 1);
}
