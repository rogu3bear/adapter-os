//! RAG integration for ingested documents
//!
//! Provides functions to index ingested documents into the RAG vector store.

use crate::embeddings::EmbeddingModel;
use crate::types::IngestedDocument;
use adapteros_core::{B3Hash, Result};
use adapteros_db::Db;
use chrono::Utc;
use std::sync::Arc;
use tracing::{debug, info};

/// Parameters for indexing a document chunk in RAG
#[derive(Debug, Clone)]
pub struct RagChunkParams {
    pub tenant_id: String,
    pub doc_id: String,
    pub chunk_index: usize,
    pub text: String,
    pub embedding: Vec<f32>,
    pub rev: String,
    pub effectivity: String,
    pub source_type: String,
    pub page_number: Option<u32>,
    pub chunk_hash: String,
    pub text_preview: String,
}

/// Index a single ingested document into the RAG system with provenance
///
/// This function takes an ingested document and its chunks, generates embeddings
/// for each chunk, and populates both the RAG index and document_chunks table.
pub async fn index_document_with_provenance(
    db: &Db,
    document_id: &str,
    ingested_doc: &IngestedDocument,
    embedding_model: &Arc<dyn EmbeddingModel>,
) -> Result<Vec<String>> {
    info!(
        "Indexing document {} ({} chunks) with provenance",
        ingested_doc.source_name,
        ingested_doc.chunks.len()
    );

    let mut chunk_ids = Vec::new();

    for chunk in &ingested_doc.chunks {
        debug!(
            "Indexing chunk {}/{}",
            chunk.chunk_index + 1,
            chunk.total_chunks
        );

        // Generate embedding for chunk
        let embedding = embedding_model.encode_text(&chunk.text)?;

        // Compute chunk hash (BLAKE3 of normalized text)
        let chunk_hash = B3Hash::hash(chunk.text.as_bytes());
        let chunk_hash_str = chunk_hash.to_hex();

        // Create text preview (first 200 chars)
        let text_preview = if chunk.text.len() > 200 {
            format!("{}...", &chunk.text[..200])
        } else {
            chunk.text.clone()
        };

        // Generate chunk_id
        let chunk_id = format!("{}__chunk_{}", document_id, chunk.chunk_index);

        // Store embedding as JSON
        let embedding_json =
            serde_json::to_string(&embedding).map_err(adapteros_core::AosError::Serialization)?;

        // Insert into document_chunks table
        sqlx::query(
            r#"
            INSERT INTO document_chunks (
                id, document_id, chunk_index, page_number,
                start_offset, end_offset, chunk_hash, text_preview, embedding_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&chunk_id)
        .bind(document_id)
        .bind(chunk.chunk_index as i64)
        .bind(chunk.page_number.map(|p| p as i64))
        .bind(chunk.start_offset as i64)
        .bind(chunk.end_offset as i64)
        .bind(&chunk_hash_str)
        .bind(&text_preview)
        .bind(&embedding_json)
        .execute(db.pool())
        .await
        .map_err(|e| {
            adapteros_core::AosError::Database(format!("Failed to insert document chunk: {}", e))
        })?;

        chunk_ids.push(chunk_id);

        debug!(
            "Stored chunk {} with hash {}",
            chunk.chunk_index, chunk_hash_str
        );
    }

    info!(
        "Indexed {} chunks from document {} with provenance",
        chunk_ids.len(),
        ingested_doc.source_name
    );

    Ok(chunk_ids)
}

/// Index a single ingested document into the RAG system (legacy function)
///
/// This function takes an ingested document and its chunks, generates embeddings
/// for each chunk, and returns parameters that can be used to insert into the
/// RAG vector store.
pub async fn prepare_document_for_rag(
    tenant_id: &str,
    document: &IngestedDocument,
    embedding_model: &Arc<dyn EmbeddingModel>,
    rev: Option<&str>,
) -> Result<Vec<RagChunkParams>> {
    info!(
        "Preparing document {} ({} chunks) for RAG indexing",
        document.source_name,
        document.chunks.len()
    );

    let mut chunk_params = Vec::new();
    let document_rev = rev.unwrap_or("v1");
    let source_type = document.source.as_str();

    for chunk in &document.chunks {
        debug!(
            "Generating embedding for chunk {}/{}",
            chunk.chunk_index + 1,
            chunk.total_chunks
        );

        let embedding = embedding_model.encode_text(&chunk.text)?;

        // Compute chunk hash (BLAKE3 of normalized text)
        let chunk_hash = B3Hash::hash(chunk.text.as_bytes());
        let chunk_hash_str = chunk_hash.to_hex();

        // Create text preview (first 200 chars)
        let text_preview = if chunk.text.len() > 200 {
            format!("{}...", &chunk.text[..200])
        } else {
            chunk.text.clone()
        };

        // Create a unique doc_id for each chunk
        let chunk_doc_id = format!(
            "{}__chunk_{}",
            sanitize_doc_id(&document.source_name),
            chunk.chunk_index
        );

        chunk_params.push(RagChunkParams {
            tenant_id: tenant_id.to_string(),
            doc_id: chunk_doc_id,
            chunk_index: chunk.chunk_index,
            text: chunk.text.clone(),
            embedding,
            rev: document_rev.to_string(),
            effectivity: "all".to_string(),
            source_type: source_type.to_string(),
            page_number: chunk.page_number,
            chunk_hash: chunk_hash_str,
            text_preview,
        });
    }

    info!(
        "Prepared {} chunks from document {} for RAG",
        chunk_params.len(),
        document.source_name
    );

    Ok(chunk_params)
}

/// Sanitize a document name to create a valid doc_id
fn sanitize_doc_id(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Generate a document revision from the current timestamp
pub fn generate_revision() -> String {
    format!("v_{}", Utc::now().format("%Y%m%d_%H%M%S"))
}

/// Batch index multiple documents for RAG
pub async fn prepare_documents_for_rag(
    tenant_id: &str,
    documents: &[IngestedDocument],
    embedding_model: &Arc<dyn EmbeddingModel>,
    rev: Option<&str>,
) -> Result<Vec<RagChunkParams>> {
    let mut all_params = Vec::new();

    for document in documents {
        let params = prepare_document_for_rag(tenant_id, document, embedding_model, rev).await?;
        all_params.extend(params);
    }

    Ok(all_params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DocumentSource;
    use adapteros_core::B3Hash;
    use std::path::PathBuf;

    struct MockEmbeddingModel {
        dimension: usize,
    }

    impl EmbeddingModel for MockEmbeddingModel {
        fn encode_text(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1; self.dimension])
        }

        fn model_hash(&self) -> B3Hash {
            B3Hash::hash(b"mock")
        }

        fn dimension(&self) -> usize {
            self.dimension
        }
    }

    #[tokio::test]
    async fn test_prepare_document_for_rag() {
        let mock_model = Arc::new(MockEmbeddingModel { dimension: 384 }) as Arc<dyn EmbeddingModel>;

        let chunk =
            crate::types::DocumentChunk::new(0, Some(1), 0, 100, "Test chunk text".to_string())
                .with_total(1);

        let document = IngestedDocument {
            source: DocumentSource::Pdf,
            source_name: "test-document.pdf".to_string(),
            source_path: Some(PathBuf::from("var/test.pdf")),
            doc_hash: B3Hash::hash(b"test"),
            byte_len: 1000,
            page_count: Some(1),
            chunks: vec![chunk],
        };

        let params = prepare_document_for_rag("tenant-123", &document, &mock_model, None)
            .await
            .expect("Failed to prepare document");

        assert_eq!(params.len(), 1);
        assert_eq!(params[0].tenant_id, "tenant-123");
        assert_eq!(params[0].doc_id, "test-document_pdf__chunk_0");
        assert_eq!(params[0].text, "Test chunk text");
        assert_eq!(params[0].embedding.len(), 384);
        assert_eq!(params[0].source_type, "pdf");
        assert_eq!(params[0].text_preview, "Test chunk text");
        assert!(!params[0].chunk_hash.is_empty());
    }

    #[test]
    fn test_sanitize_doc_id() {
        assert_eq!(sanitize_doc_id("document.pdf"), "document_pdf");
        assert_eq!(sanitize_doc_id("my-file_v2.txt"), "my-file_v2_txt");
        assert_eq!(sanitize_doc_id("file with spaces"), "file_with_spaces");
    }
}
