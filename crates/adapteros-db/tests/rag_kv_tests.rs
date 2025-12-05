//! KV-backed RAG determinism and isolation tests.

use adapteros_core::B3Hash;
use adapteros_db::rag::{RagDocumentWrite, RagRetrievedDocument};
use adapteros_db::{Db, KvDb, StorageMode};

fn build_doc(
    tenant_id: &str,
    doc_id: &str,
    embedding: Vec<f32>,
    model_hash: B3Hash,
) -> RagDocumentWrite {
    RagDocumentWrite {
        tenant_id: tenant_id.to_string(),
        doc_id: doc_id.to_string(),
        text: format!("text-{doc_id}"),
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

#[tokio::test]
async fn deterministic_order_and_isolation_dual_write() -> adapteros_core::Result<()> {
    let mut db = Db::new_in_memory().await?;
    db.attach_kv_backend(KvDb::init_in_memory()?);
    db.set_storage_mode(StorageMode::DualWrite);

    let model_hash = B3Hash::hash(b"model");
    let embedding = vec![1.0, 0.0, 0.0];

    // Two docs with identical embeddings to force tie-breaking by doc_id asc
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "b__chunk_0",
        embedding.clone(),
        model_hash,
    ))
    .await?;
    db.upsert_rag_document(build_doc(
        "tenant-a",
        "a__chunk_0",
        embedding.clone(),
        model_hash,
    ))
    .await?;

    // Cross-tenant doc should not appear
    db.upsert_rag_document(build_doc(
        "tenant-b",
        "z__chunk_0",
        embedding.clone(),
        model_hash,
    ))
    .await?;

    let results = db
        .retrieve_rag_documents("tenant-a", &model_hash, 3, &embedding, 5)
        .await?;

    assert_eq!(
        ids(&results),
        vec!["a__chunk_0".to_string(), "b__chunk_0".to_string()]
    );

    Ok(())
}

#[tokio::test]
async fn kv_primary_reads_from_kv() -> adapteros_core::Result<()> {
    let mut db = Db::new_in_memory().await?;
    db.attach_kv_backend(KvDb::init_in_memory()?);
    db.set_storage_mode(StorageMode::DualWrite);

    let model_hash = B3Hash::hash(b"model-kv");
    let embedding = vec![0.2, 0.3, 0.4];

    db.upsert_rag_document(build_doc(
        "tenant-a",
        "doc__chunk_0",
        embedding.clone(),
        model_hash,
    ))
    .await?;

    // Switch to KV-primary (reads from KV, writes still dual)
    db.set_storage_mode(StorageMode::KvPrimary);

    let results = db
        .retrieve_rag_documents("tenant-a", &model_hash, 3, &embedding, 3)
        .await?;

    assert_eq!(ids(&results), vec!["doc__chunk_0".to_string()]);

    Ok(())
}
