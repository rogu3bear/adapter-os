#![cfg(feature = "rag-pgvector")]

use adapteros_core::B3Hash;
use adapteros_lora_rag::PgVectorIndex;
use sqlx::postgres::PgPool;

#[tokio::test]
#[ignore = "Requires PostgreSQL with pgvector extension and DATABASE_URL - run with: cargo test --release -- --ignored"]
async fn test_pgvector_ordering_ties_by_doc_id() {
    // Skip if no DATABASE_URL is provided
    let url = match std::env::var("DATABASE_URL") {
        Ok(s) => s,
        Err(_) => return, // nothing to assert without a DB
    };

    let pool = PgPool::connect(&url)
        .await
        .expect("Failed to connect to Postgres");

    let embedding_hash = B3Hash::hash(b"test-model");
    let index = PgVectorIndex::new_postgres(pool.clone(), embedding_hash, 4);

    // Reset tenant data
    index
        .clear_tenant_documents("test-tenant")
        .await
        .expect("Failed to clear tenant docs");

    let emb = vec![0.5f32, 0.5, 0.5, 0.5];

    // Insert two docs with identical embeddings to force a tie on score
    index
        .add_document(
            "test-tenant",
            "doc-001".to_string(),
            "A".to_string(),
            emb.clone(),
            "v1".to_string(),
            "all".to_string(),
            "test".to_string(),
            None,
        )
        .await
        .expect("insert doc-001");
    index
        .add_document(
            "test-tenant",
            "doc-002".to_string(),
            "B".to_string(),
            emb.clone(),
            "v1".to_string(),
            "all".to_string(),
            "test".to_string(),
            None,
        )
        .await
        .expect("insert doc-002");

    let results = index
        .retrieve("test-tenant", &emb, 2)
        .await
        .expect("retrieve");

    assert_eq!(results.len(), 2);
    // Deterministic tie-breaker: (score DESC, doc_id ASC)
    assert_eq!(results[0].doc_id, "doc-001");
    assert_eq!(results[1].doc_id, "doc-002");
}
