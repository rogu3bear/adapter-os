//! RAG embedding document model for KV storage.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// KV-backed RAG document with embedding payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagDocumentKv {
    pub doc_id: String,
    pub tenant_id: String,
    pub text: String,
    pub embedding: Vec<f32>,
    pub rev: String,
    pub effectivity: String,
    pub source_type: String,
    pub superseded_by: Option<String>,
    pub embedding_model_hash: String,
    pub embedding_dimension: u32,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

impl RagDocumentKv {
    /// Construct the KV primary key.
    pub fn primary_key(&self) -> String {
        format!("ragdoc:{}:{}", self.tenant_id, self.doc_id)
    }

    /// Index value for tenant + embedding model grouping.
    pub fn tenant_model_index_value(&self) -> String {
        format!("{}:{}", self.tenant_id, self.embedding_model_hash)
    }

    /// Create a document with current timestamp metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_now(
        doc_id: String,
        tenant_id: String,
        text: String,
        embedding: Vec<f32>,
        rev: String,
        effectivity: String,
        superseded_by: Option<String>,
        source_type: String,
        embedding_model_hash: String,
        embedding_dimension: u32,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            doc_id,
            tenant_id,
            text,
            embedding,
            rev,
            effectivity,
            source_type,
            superseded_by,
            embedding_model_hash,
            embedding_dimension,
            created_at_ms: now,
            updated_at_ms: now,
        }
    }
}
