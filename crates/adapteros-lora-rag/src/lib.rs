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
}
