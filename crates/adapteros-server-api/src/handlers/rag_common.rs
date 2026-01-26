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
//! for replay (rag_snapshot_hash + adapter_ids). See AGENTS.md Section 8.4.
//!
//! # Determinism Contract (Ruleset #2)
//!
//! Documents are retrieved and ordered by:
//! 1. Score DESC (highest relevance first)
//! 2. doc_id ASC (alphabetical tie-breaking)
//!
//! This ensures identical queries against identical DB state return documents
//! in the same order every time. See docs/DETERMINISM.md for details.

use crate::state::AppState;
use adapteros_core::B3Hash;
use adapteros_retrieval::rag::EmbeddingModel;
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Result of RAG context retrieval with full metadata for evidence tracking
#[derive(Debug, Clone)]
pub struct RagContextResult {
    /// Tenant ID for evidence creation
    pub tenant_id: String,
    /// Concatenated context string from retrieved documents
    pub context: String,
    /// Document IDs in retrieval order (score DESC, doc_id ASC)
    pub doc_ids: Vec<String>,
    /// Chunk indices parallel to doc_ids for evidence creation
    pub chunk_indices: Vec<i32>,
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

/// Truncation strategy for context building
#[derive(Debug, Clone, Default)]
pub enum TruncationStrategy {
    /// Hard cut: stop at first document that doesn't fit (backward compatible)
    #[default]
    HardCut,
    /// Priority-based: allocate budget proportional to relevance score
    PriorityBased,
}

/// RAG retrieval configuration
#[derive(Debug, Clone)]
pub struct RagRetrievalConfig {
    /// Number of candidate documents to fetch before filtering
    pub candidate_k: usize,
    /// Maximum documents to return after all processing
    pub top_k: usize,
    /// Maximum characters in concatenated context
    pub max_context_chars: usize,
    /// Minimum relevance score (0.0-1.0). Documents below this are filtered.
    pub min_relevance_score: f32,
    /// Maximum chunks from the same source document (0 = unlimited)
    pub max_chunks_per_document: usize,
    /// Context truncation strategy
    pub truncation_strategy: TruncationStrategy,
    /// Enable hybrid search (vector + FTS5)
    pub enable_hybrid_search: bool,
}

impl Default for RagRetrievalConfig {
    fn default() -> Self {
        Self {
            candidate_k: 20,
            top_k: 5,
            max_context_chars: 4000,
            min_relevance_score: 0.3,
            max_chunks_per_document: 3,
            truncation_strategy: TruncationStrategy::HardCut,
            enable_hybrid_search: false,
        }
    }
}

/// Truncate text at a word boundary near the target length
fn truncate_at_boundary(text: &str, max_len: usize) -> &str {
    if text.len() <= max_len {
        return text;
    }

    // Look for word boundary
    if let Some(space_pos) = text[..max_len].rfind(char::is_whitespace) {
        return &text[..space_pos];
    }

    &text[..max_len]
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

    let model_hash = embedding_model.model_hash();
    let dimension = embedding_model.dimension();

    // Retrieve candidate documents via storage-mode aware Db API
    // Use hybrid search (vector + FTS5) if enabled, otherwise vector-only
    let all_results = if config.enable_hybrid_search {
        state
            .db
            .retrieve_rag_documents_hybrid(
                tenant_id,
                query,
                &model_hash,
                dimension,
                &query_embedding,
                config.candidate_k,
                config.min_relevance_score,
            )
            .await?
    } else {
        state
            .db
            .retrieve_rag_documents(
                tenant_id,
                &model_hash,
                dimension,
                &query_embedding,
                config.candidate_k,
            )
            .await?
    };

    // Get document IDs that belong to the specified collection (efficient - just IDs)
    let collection_doc_ids: HashSet<String> = state
        .db
        .list_collection_document_ids(collection_id)
        .await?
        .into_iter()
        .collect();

    // Filter results by collection membership using parsed document_id
    // RAG doc_id format is `{document_id}__chunk_{index}`, we need to extract document_id
    let mut results: Vec<_> = all_results
        .into_iter()
        .filter(|doc| {
            if let Some(parsed) = parse_rag_doc_id(&doc.doc_id) {
                collection_doc_ids.contains(&parsed.document_id)
            } else {
                // If we can't parse the doc_id, try direct match (backwards compatibility)
                collection_doc_ids.contains(&doc.doc_id)
            }
        })
        .collect();

    debug!(
        collection_id = %collection_id,
        collection_doc_count = collection_doc_ids.len(),
        candidate_count = config.candidate_k,
        before_filter_count = results.len(),
        "Filtered RAG results by collection membership"
    );

    // Apply relevance score filtering
    results.retain(|doc| doc.score >= config.min_relevance_score);

    debug!(
        after_score_filter_count = results.len(),
        min_score = config.min_relevance_score,
        "Applied relevance score filtering"
    );

    // Apply chunk deduplication if configured
    if config.max_chunks_per_document > 0 {
        use std::collections::HashMap;
        let mut doc_counts: HashMap<String, usize> = HashMap::new();

        results.retain(|doc| {
            // Extract base document_id from chunk doc_id format: {uuid}__chunk_{index}
            let base_doc_id = if let Some(pos) = doc.doc_id.rfind("__chunk_") {
                &doc.doc_id[..pos]
            } else {
                &doc.doc_id
            };

            let count = doc_counts.entry(base_doc_id.to_string()).or_insert(0);
            if *count < config.max_chunks_per_document {
                *count += 1;
                true
            } else {
                false
            }
        });

        debug!(
            after_dedup_count = results.len(),
            max_per_doc = config.max_chunks_per_document,
            "Applied chunk deduplication"
        );
    }

    // Take top_k after all filtering
    results.truncate(config.top_k);

    if results.is_empty() {
        return Ok(RagContextResult {
            tenant_id: tenant_id.to_string(),
            context: String::new(),
            doc_ids: Vec::new(),
            chunk_indices: Vec::new(),
            scores: Vec::new(),
            collection_id: collection_id.to_string(),
            embedding_model_hash: model_hash.to_hex(),
            context_hash: B3Hash::hash(&[]).to_hex(),
        });
    }

    // Build context with configurable truncation strategy
    let mut context = String::new();
    let mut included_indices = Vec::new();

    match config.truncation_strategy {
        TruncationStrategy::HardCut => {
            for (i, doc) in results.iter().enumerate() {
                let separator_len = if i > 0 { 7 } else { 0 }; // "\n\n---\n\n"
                if context.len() + separator_len + doc.text.len() > config.max_context_chars {
                    break;
                }
                if i > 0 {
                    context.push_str("\n\n---\n\n");
                }
                context.push_str(&doc.text);
                included_indices.push(i);
            }
        }
        TruncationStrategy::PriorityBased => {
            let total_score: f32 = results.iter().map(|d| d.score).sum();
            let mut remaining_budget = config.max_context_chars;

            for (i, doc) in results.iter().enumerate() {
                let separator_len = if i > 0 { 7 } else { 0 };

                if remaining_budget <= separator_len {
                    break;
                }

                // Allocate budget proportional to score
                let proportional_budget = if total_score > 0.0 {
                    ((doc.score / total_score) * config.max_context_chars as f32) as usize
                } else {
                    remaining_budget / (results.len() - i).max(1)
                };

                let doc_budget =
                    proportional_budget.min(remaining_budget.saturating_sub(separator_len));

                if doc_budget == 0 {
                    continue;
                }

                if i > 0 && remaining_budget > separator_len {
                    context.push_str("\n\n---\n\n");
                    remaining_budget -= separator_len;
                }

                if doc.text.len() <= doc_budget {
                    context.push_str(&doc.text);
                    remaining_budget -= doc.text.len();
                    included_indices.push(i);
                } else if doc_budget > 100 {
                    // Partial inclusion: truncate at word boundary
                    let truncated = truncate_at_boundary(&doc.text, doc_budget);
                    context.push_str(truncated);
                    context.push_str("...");
                    remaining_budget -= truncated.len() + 3;
                    included_indices.push(i);
                }
            }
        }
    }

    // Compute context hash for evidence
    let context_hash = B3Hash::hash(context.as_bytes());

    // Collect aggregate RAG trace info (doc_ids, chunk_indices, and scores in retrieval order)
    // Only include documents that made it into the final context
    let mut doc_ids = Vec::new();
    let mut chunk_indices = Vec::new();
    for &idx in included_indices.iter() {
        if let Some(doc) = results.get(idx) {
            if let Some(parsed) = parse_rag_doc_id(&doc.doc_id) {
                doc_ids.push(parsed.document_id);
                chunk_indices.push(parsed.chunk_index);
            }
        }
    }
    let scores: Vec<f64> = included_indices
        .iter()
        .filter_map(|&idx| results.get(idx).map(|doc| doc.score as f64))
        .collect();

    info!(
        tenant_id = %tenant_id,
        collection_id = %collection_id,
        num_results = results.len(),
        context_len = context.len(),
        embedding_model_hash = %model_hash.to_hex(),
        "Retrieved RAG context"
    );

    Ok(RagContextResult {
        tenant_id: tenant_id.to_string(),
        context,
        doc_ids,
        chunk_indices,
        scores,
        collection_id: collection_id.to_string(),
        embedding_model_hash: model_hash.to_hex(),
        context_hash: context_hash.to_hex(),
    })
}

/// Result of evidence storage with partial success tracking
#[derive(Debug)]
pub struct EvidenceStorageResult {
    pub stored_ids: Vec<String>,
    pub failed_entries: Vec<FailedEvidence>,
    pub total_attempted: usize,
}

#[derive(Debug)]
pub struct FailedEvidence {
    pub document_id: String,
    pub chunk_index: i32,
    pub error: String,
}

/// Model context captured at inference time for evidence audit trail.
/// This ensures evidence remains accurate even if workspace state changes later.
#[derive(Debug, Clone)]
pub struct EvidenceModelContext {
    /// The base model ID active at inference time
    pub base_model_id: Option<String>,
    /// Adapter IDs active at inference time
    pub adapter_ids: Option<Vec<String>>,
    /// Manifest hash for the workspace state
    pub manifest_hash: Option<String>,
}

impl EvidenceStorageResult {
    pub fn success_rate(&self) -> f32 {
        if self.total_attempted == 0 {
            1.0
        } else {
            self.stored_ids.len() as f32 / self.total_attempted as f32
        }
    }
}

/// Store RAG evidence with partial success handling.
/// Unlike the original, this continues on individual entry failures.
pub async fn store_rag_evidence_resilient(
    state: &AppState,
    rag_result: &RagContextResult,
    inference_id: &str,
    session_id: Option<&str>,
) -> EvidenceStorageResult {
    let mut stored_ids = Vec::new();
    let mut failed_entries = Vec::new();
    let total_attempted = rag_result.doc_ids.len();

    for (rank, ((doc_id, chunk_index), score)) in rag_result
        .doc_ids
        .iter()
        .zip(rag_result.chunk_indices.iter())
        .zip(rag_result.scores.iter())
        .enumerate()
    {
        // Look up chunk with explicit error handling
        let chunk_result = state
            .db
            .get_chunk_by_document_and_index(doc_id, *chunk_index)
            .await;

        let chunk = match chunk_result {
            Ok(Some(chunk)) => chunk,
            Ok(None) => {
                warn!(
                    document_id = %doc_id,
                    chunk_index = %chunk_index,
                    "Chunk not found for evidence creation"
                );
                failed_entries.push(FailedEvidence {
                    document_id: doc_id.clone(),
                    chunk_index: *chunk_index,
                    error: "Chunk not found".to_string(),
                });
                continue;
            }
            Err(e) => {
                warn!(
                    document_id = %doc_id,
                    chunk_index = %chunk_index,
                    error = %e,
                    "Failed to look up chunk"
                );
                failed_entries.push(FailedEvidence {
                    document_id: doc_id.clone(),
                    chunk_index: *chunk_index,
                    error: e.to_string(),
                });
                continue;
            }
        };

        // Create evidence params
        let params = adapteros_db::CreateEvidenceParams {
            tenant_id: rag_result.tenant_id.clone(),
            inference_id: inference_id.to_string(),
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
            base_model_id: None,
            adapter_ids: None,
            manifest_hash: None,
        };

        // Insert individual evidence entry
        match state.db.create_inference_evidence(params).await {
            Ok(id) => {
                stored_ids.push(id);
            }
            Err(e) => {
                warn!(
                    document_id = %doc_id,
                    chunk_id = %chunk.id,
                    error = %e,
                    "Failed to store evidence entry"
                );
                failed_entries.push(FailedEvidence {
                    document_id: doc_id.clone(),
                    chunk_index: *chunk_index,
                    error: e.to_string(),
                });
            }
        }
    }

    if !failed_entries.is_empty() {
        info!(
            stored = stored_ids.len(),
            failed = failed_entries.len(),
            total = total_attempted,
            "Evidence storage completed with partial failures"
        );
    }

    EvidenceStorageResult {
        stored_ids,
        failed_entries,
        total_attempted,
    }
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
    message_id: Option<&str>,
    model_context: Option<&EvidenceModelContext>,
) -> Vec<String> {
    let mut evidence_params_list = Vec::new();

    // Iterate over all retrieved chunks (doc_id, chunk_index, score)
    for (rank, ((doc_id, chunk_index), score)) in rag_result
        .doc_ids
        .iter()
        .zip(rag_result.chunk_indices.iter())
        .zip(rag_result.scores.iter())
        .enumerate()
    {
        // Look up the specific chunk by document_id and chunk_index
        match state
            .db
            .get_chunk_by_document_and_index(doc_id, *chunk_index)
            .await
        {
            Ok(Some(chunk)) => {
                evidence_params_list.push(adapteros_db::CreateEvidenceParams {
                    tenant_id: rag_result.tenant_id.clone(),
                    inference_id: request_id.to_string(),
                    session_id: session_id.map(|s| s.to_string()),
                    message_id: message_id.map(|s| s.to_string()),
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
                    base_model_id: model_context.and_then(|mc| mc.base_model_id.clone()),
                    adapter_ids: model_context.and_then(|mc| mc.adapter_ids.clone()),
                    manifest_hash: model_context.and_then(|mc| mc.manifest_hash.clone()),
                });
            }
            Ok(None) => {
                warn!(
                    document_id = %doc_id,
                    chunk_index = %chunk_index,
                    "Chunk not found for evidence creation"
                );
            }
            Err(e) => {
                warn!(
                    document_id = %doc_id,
                    chunk_index = %chunk_index,
                    error = %e,
                    "Failed to look up document chunk for evidence"
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
/// Doc IDs are expected to be in the format `{document_id}__chunk_{index}`.
/// For legacy doc IDs without chunk suffix, falls back to first chunk.
///
/// # Arguments
/// * `state` - Application state with database
/// * `tenant_id` - Tenant ID for isolation
/// * `doc_ids` - Document IDs to retrieve (in desired order, with chunk indices)
/// * `max_context_chars` - Maximum characters in concatenated context
///
/// # Returns
/// Tuple of (context string, missing document IDs)
pub async fn reconstruct_rag_context(
    state: &AppState,
    tenant_id: &str,
    doc_ids: &[String],
    max_context_chars: usize,
) -> adapteros_core::Result<(String, Vec<String>)> {
    if doc_ids.is_empty() {
        return Ok((String::new(), Vec::new()));
    }

    let mut context = String::new();
    let mut missing_doc_ids = Vec::new();

    for doc_id in doc_ids.iter() {
        // Parse doc_id to extract document_id and chunk_index
        let (document_id, chunk_index) = if let Some(parsed) = parse_rag_doc_id(doc_id) {
            (parsed.document_id, Some(parsed.chunk_index))
        } else {
            // Legacy doc_id without chunk suffix - use doc_id as-is and fetch first chunk
            (doc_id.clone(), None)
        };

        // Fetch the specific chunk
        let chunk_result = if let Some(chunk_idx) = chunk_index {
            // Fetch specific chunk by index
            state
                .db
                .get_chunk_by_document_and_index(&document_id, chunk_idx)
                .await
        } else {
            // Fallback: fetch first chunk for legacy doc_ids
            match state.db.get_document_chunks(tenant_id, &document_id).await {
                Ok(chunks) => Ok(chunks.first().cloned()),
                Err(e) => Err(e),
            }
        };

        match chunk_result {
            Ok(Some(chunk)) => {
                if let Some(preview) = &chunk.text_preview {
                    if context.len() + preview.len() <= max_context_chars {
                        if !context.is_empty() {
                            context.push_str("\n\n---\n\n");
                        }
                        context.push_str(preview);
                    } else {
                        // Reached max context size, stop adding more chunks
                        break;
                    }
                }
            }
            Ok(None) => {
                warn!(
                    doc_id = %doc_id,
                    document_id = %document_id,
                    chunk_index = ?chunk_index,
                    "Chunk not found during RAG context reconstruction"
                );
                missing_doc_ids.push(doc_id.clone());
            }
            Err(e) => {
                warn!(
                    doc_id = %doc_id,
                    document_id = %document_id,
                    chunk_index = ?chunk_index,
                    error = %e,
                    "Failed to retrieve chunk during RAG context reconstruction"
                );
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
        assert_eq!(config.candidate_k, 20);
        assert_eq!(config.top_k, 5);
        assert_eq!(config.max_context_chars, 4000);
        assert_eq!(config.min_relevance_score, 0.3);
        assert_eq!(config.max_chunks_per_document, 3);
        assert!(!config.enable_hybrid_search);
    }

    #[test]
    fn test_truncate_at_boundary() {
        let text = "The quick brown fox jumps over the lazy dog";

        // Truncate at word boundary
        let result = truncate_at_boundary(text, 20);
        assert_eq!(result, "The quick brown fox");

        // Text shorter than max
        let result = truncate_at_boundary(text, 100);
        assert_eq!(result, text);

        // No space found - hard cut
        let text = "abcdefghijklmnop";
        let result = truncate_at_boundary(text, 10);
        assert_eq!(result, "abcdefghij");
    }
}
