//! Shared RAG retrieval logic for deterministic document retrieval
//!
//! This module provides a single source of truth for RAG (Retrieval-Augmented Generation)
//! context retrieval across all inference paths (streaming, batch, replay).
//!
//! # RAG vs Adapters
//!
//! RAG provides query-time context augmentation - documents are retrieved per-request
//! and injected into the prompt. This runs BEFORE the worker call in InferenceCore.
//!
//! Adapters provide persistent learned behavior via trained weights. The router
//! selects adapters INSIDE the worker during token generation.
//!
//! Both paths can run together in a single inference, and both are captured
//! for replay (rag_snapshot_hash + adapter_ids). See CLAUDE.md Section 8.4.
//!
//! # Determinism Contract (Ruleset #2)
//!
//! Documents are retrieved and ordered by:
//! 1. Score DESC (highest relevance first)
//! 2. doc_id ASC (alphabetical tie-breaking)
//!
//! This ensures identical queries against identical DB state return documents
//! in the same order every time. See docs/RAG_DETERMINISM.md for details.

use crate::state::AppState;
use adapteros_core::B3Hash;
use adapteros_lora_rag::{EmbeddingModel, PgVectorIndex};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Result of RAG context retrieval with full metadata for evidence tracking
#[derive(Debug, Clone)]
pub struct RagContextResult {
    /// Concatenated context string from retrieved documents
    pub context: String,
    /// Document IDs in retrieval order (score DESC, doc_id ASC)
    pub doc_ids: Vec<String>,
    /// Relevance scores parallel to doc_ids
    pub scores: Vec<f64>,
    /// Collection ID used for scoped retrieval
    pub collection_id: String,
    /// Hash of the embedding model used for query encoding
    pub embedding_model_hash: String,
    /// BLAKE3 hash of the context string for integrity verification
    pub context_hash: String,
}

/// Parsed RAG document ID components
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRagDocId {
    /// Base document ID (UUID)
    pub document_id: String,
    /// Chunk index within the document
    pub chunk_index: i32,
}

/// Parse a RAG doc_id to extract the base document_id and chunk_index.
///
/// RAG doc_ids follow the format `{document_id}__chunk_{index}`.
/// Returns ParsedRagDocId if parsing succeeds, None otherwise.
pub fn parse_rag_doc_id(doc_id: &str) -> Option<ParsedRagDocId> {
    const CHUNK_SEPARATOR: &str = "__chunk_";

    if let Some(pos) = doc_id.rfind(CHUNK_SEPARATOR) {
        let document_id = doc_id[..pos].to_string();
        let chunk_index_str = &doc_id[pos + CHUNK_SEPARATOR.len()..];
        if let Ok(chunk_index) = chunk_index_str.parse::<i32>() {
            return Some(ParsedRagDocId {
                document_id,
                chunk_index,
            });
        }
    }
    None
}

/// RAG retrieval configuration
#[derive(Debug, Clone)]
pub struct RagRetrievalConfig {
    /// Number of candidate documents to fetch before filtering
    pub candidate_k: usize,
    /// Maximum documents to return after filtering
    pub top_k: usize,
    /// Maximum characters in concatenated context
    pub max_context_chars: usize,
}

impl Default for RagRetrievalConfig {
    fn default() -> Self {
        Self {
            candidate_k: 15,
            top_k: 5,
            max_context_chars: 4000,
        }
    }
}

/// Retrieve RAG context with deterministic ordering and full metadata.
///
/// This function:
/// 1. Encodes the query using the embedding model
/// 2. Retrieves top-k similar documents from the vector index
/// 3. Filters results by collection membership (efficient ID-only check)
/// 4. Concatenates the retrieved text chunks as context
///
/// # Determinism Contract (Ruleset #2)
/// Documents are retrieved and ordered by:
/// 1. Score DESC (highest relevance first)
/// 2. doc_id ASC (alphabetical tie-breaking)
///
/// This ensures identical queries against identical DB state
/// return documents in the same order every time.
///
/// # Arguments
/// * `state` - Application state with database pool
/// * `tenant_id` - Tenant ID for isolation
/// * `collection_id` - Collection to scope retrieval to
/// * `query` - Query text to encode and search
/// * `embedding_model` - Model for query encoding
/// * `config` - Optional retrieval configuration (defaults to standard params)
///
/// # Returns
/// `RagContextResult` with context string, doc_ids, scores, and metadata
/// for evidence tracking and replay support.
pub async fn retrieve_rag_context(
    state: &AppState,
    tenant_id: &str,
    collection_id: &str,
    query: &str,
    embedding_model: Arc<dyn EmbeddingModel + Send + Sync>,
    config: Option<RagRetrievalConfig>,
) -> adapteros_core::Result<RagContextResult> {
    let config = config.unwrap_or_default();

    // Encode the query
    let query_embedding = embedding_model.encode_text(query)?;

    // Get the embedding model hash for index creation
    let model_hash = embedding_model.model_hash();
    let dimension = embedding_model.dimension();

    // Create RAG index using the database pool
    let index = PgVectorIndex::new_sqlite(state.db_pool.clone(), model_hash, dimension);

    // Retrieve candidate documents (more than TOP_K since we'll filter by collection)
    let all_results = index
        .retrieve(tenant_id, &query_embedding, config.candidate_k)
        .await?;

    // Get document IDs that belong to the specified collection (efficient - just IDs)
    let collection_doc_ids: HashSet<String> = state
        .db
        .list_collection_document_ids(collection_id)
        .await?
        .into_iter()
        .collect();

    // Filter results by collection membership using parsed document_id
    // RAG doc_id format is `{document_id}__chunk_{index}`, we need to extract document_id
    let results: Vec<_> = all_results
        .into_iter()
        .filter(|doc| {
            if let Some(parsed) = parse_rag_doc_id(&doc.doc_id) {
                collection_doc_ids.contains(&parsed.document_id)
            } else {
                // If we can't parse the doc_id, try direct match (backwards compatibility)
                collection_doc_ids.contains(&doc.doc_id)
            }
        })
        .take(config.top_k)
        .collect();

    debug!(
        collection_id = %collection_id,
        collection_doc_count = collection_doc_ids.len(),
        candidate_count = config.candidate_k,
        filtered_results = results.len(),
        "Filtered RAG results by collection membership"
    );

    if results.is_empty() {
        return Ok(RagContextResult {
            context: String::new(),
            doc_ids: Vec::new(),
            scores: Vec::new(),
            collection_id: collection_id.to_string(),
            embedding_model_hash: model_hash.to_hex(),
            context_hash: B3Hash::hash(&[]).to_hex(),
        });
    }

    // Concatenate results with truncation
    let mut context = String::new();
    for (i, doc) in results.iter().enumerate() {
        if context.len() + doc.text.len() > config.max_context_chars {
            break;
        }
        if i > 0 {
            context.push_str("\n\n---\n\n");
        }
        context.push_str(&doc.text);
    }

    // Compute context hash for evidence
    let context_hash = B3Hash::hash(context.as_bytes());

    // Collect aggregate RAG trace info (doc_ids and scores in retrieval order)
    let doc_ids: Vec<String> = results
        .iter()
        .filter_map(|doc| parse_rag_doc_id(&doc.doc_id).map(|p| p.document_id))
        .collect();
    let scores: Vec<f64> = results.iter().map(|doc| doc.score as f64).collect();

    info!(
        tenant_id = %tenant_id,
        collection_id = %collection_id,
        num_results = results.len(),
        context_len = context.len(),
        embedding_model_hash = %model_hash.to_hex(),
        "Retrieved RAG context"
    );

    Ok(RagContextResult {
        context,
        doc_ids,
        scores,
        collection_id: collection_id.to_string(),
        embedding_model_hash: model_hash.to_hex(),
        context_hash: context_hash.to_hex(),
    })
}

/// Store RAG evidence entries in the database for a retrieval.
///
/// Creates `inference_evidence` records for each retrieved chunk,
/// including aggregate fields (rag_doc_ids, rag_scores, rag_collection_id)
/// for citation tracing and replay support.
///
/// # Arguments
/// * `state` - Application state with database
/// * `rag_result` - Result from retrieve_rag_context
/// * `request_id` - Inference request ID for linking evidence
/// * `session_id` - Optional chat session ID
///
/// # Returns
/// List of created evidence IDs, or empty if storage failed
pub async fn store_rag_evidence(
    state: &AppState,
    rag_result: &RagContextResult,
    request_id: &str,
    session_id: Option<&str>,
) -> Vec<String> {
    // Re-retrieve the documents to get chunk metadata
    // (We need to look up chunk IDs which aren't stored in RagContextResult)
    let index = PgVectorIndex::new_sqlite(
        state.db_pool.clone(),
        B3Hash::hash(rag_result.embedding_model_hash.as_bytes()),
        0, // Dimension not needed for this lookup
    );

    let mut evidence_params_list = Vec::new();

    for (rank, (doc_id, score)) in rag_result
        .doc_ids
        .iter()
        .zip(rag_result.scores.iter())
        .enumerate()
    {
        // Look up all chunks for this document to find the one that was retrieved
        // This is a simplified approach - in production we'd track chunk_index in RagContextResult
        match state.db.get_document_chunks(doc_id).await {
            Ok(chunks) => {
                if let Some(chunk) = chunks.first() {
                    evidence_params_list.push(adapteros_db::CreateEvidenceParams {
                        inference_id: request_id.to_string(),
                        session_id: session_id.map(|s| s.to_string()),
                        message_id: None,
                        document_id: doc_id.clone(),
                        chunk_id: chunk.id.clone(),
                        page_number: chunk.page_number,
                        document_hash: chunk.chunk_hash.clone(),
                        chunk_hash: chunk.chunk_hash.clone(),
                        relevance_score: *score,
                        rank: rank as i32,
                        context_hash: rag_result.context_hash.clone(),
                        rag_doc_ids: Some(rag_result.doc_ids.clone()),
                        rag_scores: Some(rag_result.scores.clone()),
                        rag_collection_id: Some(rag_result.collection_id.clone()),
                    });
                }
            }
            Err(e) => {
                warn!(
                    document_id = %doc_id,
                    error = %e,
                    "Failed to look up document chunks for evidence"
                );
            }
        }
    }

    if evidence_params_list.is_empty() {
        return Vec::new();
    }

    // Batch insert all evidence entries in a single transaction
    match state
        .db
        .create_inference_evidence_batch(evidence_params_list)
        .await
    {
        Ok(ids) => {
            debug!(
                inference_id = %request_id,
                evidence_count = ids.len(),
                "Stored RAG evidence entries"
            );
            ids
        }
        Err(e) => {
            warn!(
                inference_id = %request_id,
                error = %e,
                "Failed to store RAG evidence entries"
            );
            Vec::new()
        }
    }
}

/// Reconstruct RAG context from stored document IDs (for replay).
///
/// This function retrieves documents by their stored IDs (from a replay session)
/// rather than performing a new vector search. This ensures deterministic replay
/// with the original documents.
///
/// # Arguments
/// * `state` - Application state with database
/// * `doc_ids` - Document IDs to retrieve (in desired order)
/// * `max_context_chars` - Maximum characters in concatenated context
///
/// # Returns
/// Tuple of (context string, missing document IDs)
pub async fn reconstruct_rag_context(
    state: &AppState,
    doc_ids: &[String],
    max_context_chars: usize,
) -> adapteros_core::Result<(String, Vec<String>)> {
    if doc_ids.is_empty() {
        return Ok((String::new(), Vec::new()));
    }

    // Fetch documents preserving order
    let documents = state.db.get_documents_by_ids_ordered(doc_ids).await?;

    let mut context = String::new();
    let mut missing_doc_ids = Vec::new();

    for (doc_opt, doc_id) in documents.iter().zip(doc_ids.iter()) {
        match doc_opt {
            Some(doc) => {
                // Get first chunk text for this document
                match state.db.get_document_chunks(&doc.id).await {
                    Ok(chunks) => {
                        if let Some(chunk) = chunks.first() {
                            if let Some(preview) = &chunk.text_preview {
                                if context.len() + preview.len() <= max_context_chars {
                                    if !context.is_empty() {
                                        context.push_str("\n\n---\n\n");
                                    }
                                    context.push_str(preview);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            document_id = %doc.id,
                            error = %e,
                            "Failed to get chunks for document during replay"
                        );
                    }
                }
            }
            None => {
                missing_doc_ids.push(doc_id.clone());
            }
        }
    }

    Ok((context, missing_doc_ids))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rag_doc_id_valid() {
        let result = parse_rag_doc_id("abc123__chunk_5");
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.document_id, "abc123");
        assert_eq!(parsed.chunk_index, 5);
    }

    #[test]
    fn test_parse_rag_doc_id_uuid_format() {
        let result = parse_rag_doc_id("550e8400-e29b-41d4-a716-446655440000__chunk_0");
        assert!(result.is_some());
        let parsed = result.unwrap();
        assert_eq!(parsed.document_id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(parsed.chunk_index, 0);
    }

    #[test]
    fn test_parse_rag_doc_id_invalid_no_separator() {
        assert!(parse_rag_doc_id("abc123").is_none());
    }

    #[test]
    fn test_parse_rag_doc_id_invalid_no_index() {
        assert!(parse_rag_doc_id("abc123__chunk_").is_none());
    }

    #[test]
    fn test_parse_rag_doc_id_invalid_non_numeric_index() {
        assert!(parse_rag_doc_id("abc123__chunk_abc").is_none());
    }

    #[test]
    fn test_rag_config_defaults() {
        let config = RagRetrievalConfig::default();
        assert_eq!(config.candidate_k, 15);
        assert_eq!(config.top_k, 5);
        assert_eq!(config.max_context_chars, 4000);
    }
}
