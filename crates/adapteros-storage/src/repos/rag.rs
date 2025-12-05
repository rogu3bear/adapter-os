//! RAG repository over KV backend.

use crate::error::StorageError;
use crate::kv::backend::KvBackend;
use crate::kv::indexing::IndexManager;
use crate::models::RagDocumentKv;
use std::sync::Arc;
use tracing::{info, warn};

/// Index names for RAG documents.
pub mod rag_indexes {
    /// Index of documents by tenant and embedding model hash.
    pub const BY_TENANT_MODEL: &str = "rag_by_tenant_model";
}

/// Repository for RAG embeddings stored in KV.
pub struct RagRepository {
    backend: Arc<dyn KvBackend>,
    index_manager: Arc<IndexManager>,
}

impl RagRepository {
    /// Create a new repository.
    pub fn new(backend: Arc<dyn KvBackend>, index_manager: Arc<IndexManager>) -> Self {
        Self {
            backend,
            index_manager,
        }
    }

    /// Upsert a RAG document and maintain secondary indexes.
    pub async fn upsert(&self, doc: RagDocumentKv) -> Result<(), StorageError> {
        let key = doc.primary_key();
        let new_index_value = doc.tenant_model_index_value();

        // Load any existing doc for index updates
        let existing = self
            .backend
            .get(&key)
            .await?
            .map(|bytes| bincode::deserialize::<RagDocumentKv>(&bytes))
            .transpose()?;

        // Update secondary index (remove old entry if needed)
        match existing {
            Some(ref old) => {
                if old.tenant_id != doc.tenant_id {
                    return Err(StorageError::ConflictError(format!(
                        "Tenant mismatch for doc {} (expected {}, got {})",
                        doc.doc_id, old.tenant_id, doc.tenant_id
                    )));
                }

                if old.tenant_model_index_value() != new_index_value {
                    self.index_manager
                        .update_index(
                            rag_indexes::BY_TENANT_MODEL,
                            Some(&old.tenant_model_index_value()),
                            &new_index_value,
                            &doc.doc_id,
                        )
                        .await?;
                }
            }
            None => {
                self.index_manager
                    .add_to_index(rag_indexes::BY_TENANT_MODEL, &new_index_value, &doc.doc_id)
                    .await?;
            }
        }

        // Persist
        let value = bincode::serialize(&doc)?;
        self.backend.set(&key, value).await?;

        info!(
            doc_id = %doc.doc_id,
            tenant_id = %doc.tenant_id,
            "RAG doc upserted into KV"
        );

        Ok(())
    }

    /// Get a single document by doc_id (scoped by tenant).
    pub async fn get(
        &self,
        tenant_id: &str,
        doc_id: &str,
    ) -> Result<Option<RagDocumentKv>, StorageError> {
        let key = format!("ragdoc:{}:{}", tenant_id, doc_id);
        match self.backend.get(&key).await? {
            Some(bytes) => {
                let doc: RagDocumentKv = bincode::deserialize(&bytes)?;
                if doc.tenant_id == tenant_id {
                    Ok(Some(doc))
                } else {
                    warn!(
                        doc_id = %doc_id,
                        key = %key,
                        "Tenant mismatch encountered when fetching RAG doc"
                    );
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// List documents for a tenant and embedding model.
    pub async fn list_by_tenant_and_model(
        &self,
        tenant_id: &str,
        embedding_model_hash: &str,
    ) -> Result<Vec<RagDocumentKv>, StorageError> {
        let index_value = format!("{}:{}", tenant_id, embedding_model_hash);
        let doc_ids = self
            .index_manager
            .query_index(rag_indexes::BY_TENANT_MODEL, &index_value)
            .await?;

        let mut docs = Vec::with_capacity(doc_ids.len());
        for doc_id in doc_ids {
            if let Some(doc) = self.get(tenant_id, &doc_id).await? {
                docs.push(doc);
            }
        }

        Ok(docs)
    }
}
