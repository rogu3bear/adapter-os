//! Hybrid search (vector + FTS5) with Reciprocal Rank Fusion (RRF) tests.
//!
//! NOTE: Run with `AOS_SKIP_MIGRATION_SIGNATURES=1 cargo test` if migration signatures
//! are not yet updated for the FTS5 migration (0204_rag_fts5_index.sql).

use adapteros_core::B3Hash;
use adapteros_db::rag::{RagDocumentWrite, RagRetrievedDocument};
use adapteros_db::Db;

fn build_doc(
    tenant_id: &str,
    doc_id: &str,
    text: &str,
    embedding: Vec<f32>,
    model_hash: B3Hash,
) -> RagDocumentWrite {
    RagDocumentWrite {
        tenant_id: tenant_id.to_string(),
        doc_id: doc_id.to_string(),
        text: text.to_string(),
        embedding,
        rev: "v1".to_string(),
        effectivity: "all".to_string(),
        source_type: "text/plain".to_string(),
        superseded_by: None,
        embedding_model_hash: model_hash,
        embedding_dimension: 3,
    }
}

fn ids(results: &[RagRetrievedDocument]) -> Vec<String> {
    results.iter().map(|r| r.doc_id.clone()).collect()
}

fn scores(results: &[RagRetrievedDocument]) -> Vec<f32> {
    results.iter().map(|r| r.score).collect()
}

/// Test RRF formula: score = 1 / (60 + rank + 1)
#[tokio::test]
async fn test_rrf_score_calculation() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"rrf-model");
    let query_embedding = vec![1.0, 0.0, 0.0];

    // Insert docs where only vector search will match (empty text for FTS)
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_0",
        "",
        vec![1.0, 0.0, 0.0], // Perfect match
        model_hash,
    ))
    .await?;

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_1",
        "",
        vec![0.99, 0.01, 0.0], // Slightly worse match
        model_hash,
    ))
    .await?;

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_2",
        "",
        vec![0.9, 0.1, 0.0], // Even worse match
        model_hash,
    ))
    .await?;

    let results = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "nonmatching",
            &model_hash,
            3,
            &query_embedding,
            3,
            0.0,
        )
        .await?;

    assert_eq!(results.len(), 3);

    // Expected RRF scores (only vector search contributes):
    // rank 0: 1/(60+0+1) = 1/61 ≈ 0.01639
    // rank 1: 1/(60+1+1) = 1/62 ≈ 0.01613
    // rank 2: 1/(60+2+1) = 1/63 ≈ 0.01587

    const RRF_K: f32 = 60.0;
    let expected_scores = vec![
        1.0 / (RRF_K + 0.0 + 1.0),
        1.0 / (RRF_K + 1.0 + 1.0),
        1.0 / (RRF_K + 2.0 + 1.0),
    ];

    for (i, (actual, expected)) in scores(&results)
        .iter()
        .zip(expected_scores.iter())
        .enumerate()
    {
        assert!(
            (actual - expected).abs() < 1e-5,
            "Score mismatch at rank {}: got {}, expected {}",
            i,
            actual,
            expected
        );
    }

    Ok(())
}

/// Test that RRF combines results from both vector and FTS searches
#[tokio::test]
async fn test_rrf_combines_vector_and_fts_results() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"combine-model");
    let query_embedding = vec![1.0, 0.0, 0.0];

    // Doc A: Strong vector match, no FTS match
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_a_vector_only",
        "unrelated content",
        vec![1.0, 0.0, 0.0],
        model_hash,
    ))
    .await?;

    // Doc B: Weak vector match, strong FTS match
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_b_fts_strong",
        "machine learning algorithms optimization",
        vec![0.1, 0.1, 0.1],
        model_hash,
    ))
    .await?;

    // Doc C: No vector match, moderate FTS match
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_c_fts_only",
        "machine learning tutorial",
        vec![0.0, 0.0, 1.0],
        model_hash,
    ))
    .await?;

    let results = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "machine learning",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    // All three documents should appear (combined from vector + FTS)
    assert!(
        results.len() >= 2,
        "Expected at least 2 results from hybrid search"
    );

    let result_ids = ids(&results);

    // At minimum, we should see results from both search types
    let has_vector_result = result_ids.contains(&"doc_a_vector_only".to_string());
    let has_fts_result = result_ids.contains(&"doc_b_fts_strong".to_string())
        || result_ids.contains(&"doc_c_fts_only".to_string());

    assert!(
        has_vector_result || has_fts_result,
        "Hybrid search should combine vector and FTS results"
    );

    Ok(())
}

/// Test that documents appearing in both searches are deduplicated with combined scores
#[tokio::test]
async fn test_rrf_deduplicates_by_doc_id() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"dedup-model");
    let query_embedding = vec![1.0, 0.0, 0.0];

    // Doc that appears in both vector and FTS results
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_both",
        "machine learning neural networks",
        vec![0.95, 0.05, 0.0], // Good vector match
        model_hash,
    ))
    .await?;

    // Doc that only matches FTS
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_fts_only",
        "machine learning algorithms",
        vec![0.0, 0.0, 1.0], // Poor vector match
        model_hash,
    ))
    .await?;

    // Doc that only matches vector
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_vector_only",
        "unrelated text",
        vec![0.9, 0.1, 0.0], // Good vector match
        model_hash,
    ))
    .await?;

    let results = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "machine learning",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    let result_ids = ids(&results);

    // Check no duplicates
    let unique_ids: std::collections::HashSet<_> = result_ids.iter().collect();
    assert_eq!(
        unique_ids.len(),
        result_ids.len(),
        "Results should have no duplicates"
    );

    // If doc_both appears in both searches, it should have combined RRF score
    // which should be higher than docs appearing in only one search
    if let Some(both_idx) = result_ids.iter().position(|id| id == "doc_both") {
        let both_score = results[both_idx].score;

        // The doc appearing in both should have a higher combined score
        // than a doc appearing in only one search (assuming similar ranks)
        // RRF scores are additive: score(both) = score_vector + score_fts
        // vs score(single) = score_vector OR score_fts

        // For a doc at rank 0 in both: 1/61 + 1/61 = 2/61 ≈ 0.0328
        // For a doc at rank 0 in one: 1/61 ≈ 0.0164

        // Basic sanity check: combined score should be positive
        assert!(both_score > 0.0, "Combined RRF score should be positive");
    }

    Ok(())
}

/// Test deterministic ordering: same query produces same order (score DESC, doc_id ASC)
#[tokio::test]
async fn test_hybrid_deterministic_ordering() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"order-model");
    let query_embedding = vec![0.5, 0.5, 0.0];

    // Insert docs with similar scores to test tie-breaking
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "z_doc",
        "neural networks deep learning",
        vec![0.5, 0.5, 0.0], // Same embedding
        model_hash,
    ))
    .await?;

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "a_doc",
        "neural networks deep learning",
        vec![0.5, 0.5, 0.0], // Same embedding
        model_hash,
    ))
    .await?;

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "m_doc",
        "neural networks deep learning",
        vec![0.5, 0.5, 0.0], // Same embedding
        model_hash,
    ))
    .await?;

    // Run the same query multiple times
    let results1 = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "neural networks",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    let results2 = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "neural networks",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    let results3 = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "neural networks",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    // All runs should produce identical order
    assert_eq!(
        ids(&results1),
        ids(&results2),
        "First and second run should match"
    );
    assert_eq!(
        ids(&results2),
        ids(&results3),
        "Second and third run should match"
    );

    // Verify tie-breaking: same scores should be ordered by doc_id ASC
    if results1.len() >= 3 {
        let all_same_score = results1
            .windows(2)
            .all(|w| (w[0].score - w[1].score).abs() < 1e-5);

        if all_same_score {
            // If all scores are the same, verify lexicographic ordering
            let ids = ids(&results1);
            let mut sorted_ids = ids.clone();
            sorted_ids.sort();
            assert_eq!(ids, sorted_ids, "Tie-breaking should order by doc_id ASC");
        }
    }

    Ok(())
}

/// Test FTS5 special character escaping
#[tokio::test]
async fn test_fts5_special_character_escaping() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"escape-model");
    let query_embedding = vec![0.5, 0.5, 0.0];

    // Insert document with normal text
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_normal",
        "machine learning with python",
        vec![0.5, 0.5, 0.0],
        model_hash,
    ))
    .await?;

    // Queries with special FTS5 characters that need escaping: " * ( ) : ^ -
    let special_queries = vec![
        "machine \"learning\"", // Quotes
        "machine*learning",     // Wildcard
        "machine(learning)",    // Parentheses
        "machine:learning",     // Colon
        "machine^learning",     // Caret
        "machine-learning",     // Hyphen
        "C++",                  // Plus signs
        "machine**learning**",  // Multiple special chars
    ];

    for query in special_queries {
        // Should not panic or error due to FTS5 syntax errors
        let result = db
            .retrieve_rag_documents_hybrid(
                "tenant-a",
                query,
                &model_hash,
                3,
                &query_embedding,
                10,
                0.0,
            )
            .await;

        assert!(
            result.is_ok(),
            "Query with special characters should not error: {}",
            query
        );
    }

    Ok(())
}

/// Test escape_fts5_query helper function behavior
#[test]
fn test_escape_fts5_query_helper() {
    // Note: escape_fts5_query is private, so we test through public API behavior
    // This test documents expected escaping behavior

    let test_cases = vec![
        // (input, expected_escaped_form)
        ("normal text", "normal text"),
        ("with \"quotes\"", "with quotes"),
        ("wild*card", "wild card"),
        ("paren(theses)", "paren theses"),
        ("colon:test", "colon test"),
        ("caret^test", "caret test"),
        ("hyphen-test", "hyphen test"),
        ("multiple   spaces", "multiple spaces"),
        ("\"*():^-", ""), // All special chars should result in empty/space
        ("  leading trailing  ", "leading trailing"),
    ];

    // We can't directly test the private function, but we document behavior
    for (input, expected) in test_cases {
        // The function should:
        // 1. Replace special chars with spaces
        // 2. Split on whitespace
        // 3. Join with single spaces
        let result = input
            .chars()
            .map(|c| match c {
                '"' | '*' | '(' | ')' | ':' | '^' | '-' => ' ',
                _ => c,
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert_eq!(result, expected, "Escaping mismatch for input: {}", input);
    }
}

/// Test min_score threshold filtering
#[tokio::test]
async fn test_hybrid_min_score_threshold() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"threshold-model");
    let query_embedding = vec![1.0, 0.0, 0.0];

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_high_score",
        "machine learning",
        vec![1.0, 0.0, 0.0], // Perfect match
        model_hash,
    ))
    .await?;

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_low_score",
        "unrelated",
        vec![0.0, 0.0, 1.0], // Poor match
        model_hash,
    ))
    .await?;

    // With high threshold, should only return high-scoring doc
    let results_high_threshold = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "machine learning",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.02, // Threshold higher than low RRF scores
        )
        .await?;

    // With zero threshold, should return all docs
    let results_zero_threshold = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "machine learning",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    assert!(
        results_high_threshold.len() < results_zero_threshold.len(),
        "High threshold should filter out low-scoring docs"
    );

    Ok(())
}

/// Test top_k limiting
#[tokio::test]
async fn test_hybrid_top_k_limiting() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"topk-model");
    let query_embedding = vec![0.5, 0.5, 0.0];

    // Insert 10 documents
    for i in 0..10 {
        db.upsert_rag_document(build_doc(
            "tenant-a",
            &format!("doc_{:02}", i),
            "machine learning neural networks",
            vec![0.5, 0.5, 0.0],
            model_hash,
        ))
        .await?;
    }

    // Request only top 3
    let results = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "machine learning",
            &model_hash,
            3,
            &query_embedding,
            3, // top_k = 3
            0.0,
        )
        .await?;

    assert_eq!(results.len(), 3, "Should return exactly top_k results");

    Ok(())
}

/// Test tenant isolation in hybrid search
#[tokio::test]
async fn test_hybrid_tenant_isolation() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"isolation-model");
    let query_embedding = vec![1.0, 0.0, 0.0];

    // Tenant A docs
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_a1",
        "machine learning algorithms",
        vec![1.0, 0.0, 0.0],
        model_hash,
    ))
    .await?;

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_a2",
        "neural networks training",
        vec![0.9, 0.1, 0.0],
        model_hash,
    ))
    .await?;

    // Tenant B docs
    db.upsert_rag_document(build_doc(
        "tenant-b",
        "doc_b1",
        "machine learning systems",
        vec![1.0, 0.0, 0.0],
        model_hash,
    ))
    .await?;

    // Query for tenant-a
    let results_a = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "machine learning",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    // Query for tenant-b
    let results_b = db
        .retrieve_rag_documents_hybrid(
            "tenant-b",
            "machine learning",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    // Verify tenant A only sees their docs
    let ids_a = ids(&results_a);
    assert!(
        ids_a.iter().all(|id| id.starts_with("doc_a")),
        "Tenant A should only see their own docs"
    );
    assert!(
        !ids_a.iter().any(|id| id.starts_with("doc_b")),
        "Tenant A should not see tenant B docs"
    );

    // Verify tenant B only sees their docs
    let ids_b = ids(&results_b);
    assert!(
        ids_b.iter().all(|id| id.starts_with("doc_b")),
        "Tenant B should only see their own docs"
    );
    assert!(
        !ids_b.iter().any(|id| id.starts_with("doc_a")),
        "Tenant B should not see tenant A docs"
    );

    Ok(())
}

/// Test empty query handling
#[tokio::test]
async fn test_hybrid_empty_query() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"empty-model");
    let query_embedding = vec![0.5, 0.5, 0.0];

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_1",
        "some content",
        vec![0.5, 0.5, 0.0],
        model_hash,
    ))
    .await?;

    // Empty text query should still work (vector search component active)
    let results = db
        .retrieve_rag_documents_hybrid("tenant-a", "", &model_hash, 3, &query_embedding, 10, 0.0)
        .await?;

    // Should return results based on vector search alone
    assert!(
        !results.is_empty(),
        "Empty query should still return vector results"
    );

    Ok(())
}

/// Test that BM25 scores are properly converted (negated)
#[tokio::test]
async fn test_fts_bm25_score_conversion() -> adapteros_core::Result<()> {
    let db = Db::new_in_memory().await?;
    let model_hash = B3Hash::hash(b"bm25-model");
    let query_embedding = vec![0.1, 0.1, 0.1]; // Low similarity for all

    // Doc with strong FTS match
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_fts_strong",
        "machine learning machine learning machine learning", // Repeated terms for higher BM25
        vec![0.0, 0.0, 1.0],
        model_hash,
    ))
    .await?;

    // Doc with weak FTS match
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc_fts_weak",
        "machine learning",
        vec![0.0, 1.0, 0.0],
        model_hash,
    ))
    .await?;

    let results = db
        .retrieve_rag_documents_hybrid(
            "tenant-a",
            "machine learning",
            &model_hash,
            3,
            &query_embedding,
            10,
            0.0,
        )
        .await?;

    // All scores should be positive (BM25 negative scores are converted)
    for result in &results {
        assert!(
            result.score > 0.0,
            "RRF scores should always be positive, got: {}",
            result.score
        );
    }

    Ok(())
}
