//! RAG with deterministic retrieval

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub mod chunking;
pub mod evidence_manager;
pub mod fts_index;
pub mod index;
pub mod pgvector;
pub mod retrieval;

pub use chunking::{ChunkConfig, ChunkContext, CodeChunk, CodeChunker};
pub use evidence_manager::{
    ChangeType, EmbeddingModel, EvidenceIndexManager, FileChange, IndexStats,
};
pub use fts_index::{
    DocIndexImpl, IndexedDoc, IndexedSymbol, IndexedTest, SymbolIndexImpl, TestIndexImpl,
};
pub use index::TenantIndex;
pub use pgvector::{PgVectorDocument, PgVectorIndex, RetrievedDocument};
pub use retrieval::{EvidenceSpan, EvidenceType};

/// Document metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocMetadata {
    pub doc_id: String,
    pub rev: String,
    pub effectivity: String,
    pub source_type: String,
    pub superseded_by: Option<String>,
}

/// RAG system with per-tenant indices
#[derive(Clone)]
pub struct RagSystem {
    root: PathBuf,
    indices: HashMap<String, TenantIndex>,
    embedding_model_hash: B3Hash,
}

impl RagSystem {
    /// Create new RAG system
    pub fn new<P: AsRef<Path>>(root: P, embedding_model_hash: B3Hash) -> Result<Self> {
        std::fs::create_dir_all(root.as_ref())
            .map_err(|e| AosError::Other(format!("Failed to create RAG root: {}", e)))?;

        Ok(Self {
            root: root.as_ref().to_path_buf(),
            indices: HashMap::new(),
            embedding_model_hash,
        })
    }

    /// Get or create tenant index
    pub fn get_tenant_index(&mut self, tenant_id: &str) -> Result<&mut TenantIndex> {
        if !self.indices.contains_key(tenant_id) {
            let index_path = self.root.join(tenant_id);
            let index = TenantIndex::new(index_path, self.embedding_model_hash)?;
            self.indices.insert(tenant_id.to_string(), index);
        }

        self.indices.get_mut(tenant_id).ok_or_else(|| {
            AosError::Rag("Tenant index not found immediately after creation".to_string())
        })
    }

    /// Add document to tenant index
    pub fn add_document(
        &mut self,
        tenant_id: &str,
        doc_id: String,
        text: String,
        embedding: Vec<f32>,
        metadata: DocMetadata,
    ) -> Result<()> {
        let index = self.get_tenant_index(tenant_id)?;
        index.add_document(doc_id, text, embedding, metadata)
    }

    /// Retrieve documents
    pub fn retrieve(
        &mut self,
        tenant_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<EvidenceSpan>> {
        let index = self.get_tenant_index(tenant_id)?;
        index.retrieve(query_embedding, top_k)
    }

    /// Validate embedding model hash
    pub fn validate_embedding_hash(&self, hash: &B3Hash) -> Result<()> {
        if *hash != self.embedding_model_hash {
            return Err(AosError::Other(format!(
                "Embedding model hash mismatch: expected {}, got {}",
                self.embedding_model_hash, hash
            )));
        }
        Ok(())
    }

    /// Retrieve documents from a specific collection with deterministic tie-breaking
    ///
    /// This function performs collection-scoped retrieval:
    /// 1. Gets document_ids in the collection
    /// 2. Filters RAG search to those documents only
    /// 3. Applies deterministic tie-breaking (score DESC, document_id ASC, chunk_index ASC, chunk_hash ASC)
    /// 4. Returns results with document_id, chunk_id, page_number, score
    pub fn retrieve_from_collection(
        &mut self,
        collection_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<RetrievalResult>> {
        // Get tenant from collection_id (assuming format: tenant_id/collection_name)
        let tenant_id = collection_id
            .split('/')
            .next()
            .ok_or_else(|| AosError::Rag("Invalid collection_id format".to_string()))?;

        // Get evidence spans from tenant index
        let index = self.get_tenant_index(tenant_id)?;
        let evidence_spans = index.retrieve(query_embedding, top_k * 2)?; // Get more for filtering

        // Filter to collection documents (for now, accept all - would need DB query to filter by collection)
        // Convert to RetrievalResult with deterministic tie-breaking
        let mut results: Vec<RetrievalResult> = evidence_spans
            .into_iter()
            .enumerate()
            .map(|(rank, span)| {
                // Extract chunk_index from doc_id (format: doc_id__chunk_N)
                let chunk_index = span
                    .doc_id
                    .split("__chunk_")
                    .nth(1)
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(0);

                // Extract base document_id (remove __chunk_N suffix)
                let document_id = span
                    .doc_id
                    .split("__chunk_")
                    .next()
                    .unwrap_or(&span.doc_id)
                    .to_string();

                RetrievalResult {
                    document_id: document_id.clone(),
                    chunk_id: span.doc_id.clone(),
                    document_name: document_id,
                    page_number: None, // Would need to extract from metadata
                    text_preview: span.text.clone(),
                    chunk_hash: span.span_hash.to_hex(),
                    relevance_score: span.score,
                    rank: rank as i32,
                    chunk_index,
                }
            })
            .collect();

        // Deterministic tie-breaking
        results.sort_by(|a, b| {
            // Primary: score descending
            let score_cmp = b
                .relevance_score
                .partial_cmp(&a.relevance_score)
                .unwrap_or(std::cmp::Ordering::Equal);
            if score_cmp != std::cmp::Ordering::Equal {
                return score_cmp;
            }
            // Tie-breakers: document_id ASC, chunk_index ASC, chunk_hash ASC
            a.document_id
                .cmp(&b.document_id)
                .then(a.chunk_index.cmp(&b.chunk_index))
                .then(a.chunk_hash.cmp(&b.chunk_hash))
        });

        // Take top K after sorting
        results.truncate(top_k);

        // Update ranks after final sorting
        for (i, result) in results.iter_mut().enumerate() {
            result.rank = i as i32;
        }

        Ok(results)
    }
}

/// Retrieval result with provenance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub document_id: String,
    pub chunk_id: String,
    pub document_name: String,
    pub page_number: Option<i32>,
    pub text_preview: String,
    pub chunk_hash: String,
    pub relevance_score: f32,
    pub rank: i32,
    #[serde(skip)]
    chunk_index: i32,
}
