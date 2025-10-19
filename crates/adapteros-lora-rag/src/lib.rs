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

/// Backend storage for RagSystem
pub enum RagBackend {
    /// Default in-memory per-tenant indices
    InMemory(HashMap<String, TenantIndex>),
    /// PostgreSQL pgvector backend (feature-gated)
    #[cfg(feature = "rag-pgvector")]
    Pg(PgVectorIndex),
}

/// RAG system with pluggable backend
pub struct RagSystem {
    root: PathBuf,
    backend: RagBackend,
    embedding_model_hash: B3Hash,
}

impl RagSystem {
    /// Create new RAG system (in-memory backend by default)
    pub fn new<P: AsRef<Path>>(root: P, embedding_model_hash: B3Hash) -> Result<Self> {
        std::fs::create_dir_all(root.as_ref())
            .map_err(|e| AosError::Other(format!("Failed to create RAG root: {}", e)))?;

        Ok(Self {
            root: root.as_ref().to_path_buf(),
            backend: RagBackend::InMemory(HashMap::new()),
            embedding_model_hash,
        })
    }

    /// Create RAG system from a PostgreSQL pgvector index (feature-gated)
    #[cfg(feature = "rag-pgvector")]
    pub fn from_pg_index(index: PgVectorIndex, embedding_model_hash: B3Hash) -> Self {
        Self {
            root: PathBuf::new(),
            backend: RagBackend::Pg(index),
            embedding_model_hash,
        }
    }

    /// Get or create tenant index
    pub fn get_tenant_index(&mut self, tenant_id: &str) -> Result<&mut TenantIndex> {
        match self.backend {
            RagBackend::InMemory(ref mut indices) => {
                if !indices.contains_key(tenant_id) {
                    let index_path = self.root.join(tenant_id);
                    let index = TenantIndex::new(index_path, self.embedding_model_hash)?;
                    indices.insert(tenant_id.to_string(), index);
                }

                indices.get_mut(tenant_id).ok_or_else(|| {
                    AosError::Rag("Tenant index not found immediately after creation".to_string())
                })
            }
            #[cfg(feature = "rag-pgvector")]
            RagBackend::Pg(_) => Err(AosError::Rag(
                "get_tenant_index is not available for pgvector backend".to_string(),
            )),
        }
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
        match self.backend {
            RagBackend::InMemory(_) => {
                let index = self.get_tenant_index(tenant_id)?;
                index.add_document(doc_id, text, embedding, metadata)
            }
            #[cfg(feature = "rag-pgvector")]
            RagBackend::Pg(ref pg) => {
                // Forward to pgvector index (async under the hood)
                Self::block_on(pg.add_document(
                    tenant_id,
                    doc_id,
                    text,
                    embedding,
                    metadata.rev,
                    metadata.effectivity,
                    metadata.source_type,
                    metadata.superseded_by,
                ))
            }
        }
    }

    /// Retrieve documents
    pub fn retrieve(
        &mut self,
        tenant_id: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<EvidenceSpan>> {
        match self.backend {
            RagBackend::InMemory(_) => {
                let index = self.get_tenant_index(tenant_id)?;
                index.retrieve(query_embedding, top_k)
            }
            #[cfg(feature = "rag-pgvector")]
            RagBackend::Pg(ref pg) => {
                let docs: Vec<RetrievedDocument> =
                    Self::block_on(pg.retrieve(tenant_id, query_embedding, top_k))?;
                // Map RetrievedDocument -> EvidenceSpan
                let spans = docs
                    .into_iter()
                    .map(|d| EvidenceSpan {
                        doc_id: d.doc_id,
                        rev: d.rev,
                        text: d.text,
                        score: d.score,
                        span_hash: d.span_hash,
                        superseded: d.superseded,
                        evidence_type: None,
                        file_path: None,
                        start_line: None,
                        end_line: None,
                        metadata: std::collections::HashMap::new(),
                    })
                    .collect();
                Ok(spans)
            }
        }
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

    #[cfg(feature = "rag-pgvector")]
    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.block_on(fut)
        } else {
            let rt = tokio::runtime::Runtime::new()
                .expect("Failed to create Tokio runtime for RAG operations");
            rt.block_on(fut)
        }
    }
}
