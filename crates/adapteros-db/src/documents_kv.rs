//! KV storage for documents and document chunks.
//!
//! Keys (per-tenant namespace):
//! - `tenant/{tenant_id}/document/{id}` -> DocumentKv (JSON)
//! - `tenant/{tenant_id}/documents` -> Vec<document_id> (for deterministic ordering)
//! - `tenant/{tenant_id}/document-by-hash/{hash}` -> document_id (dedupe)
//! - `tenant/{tenant_id}/document/{id}/chunks` -> Vec<chunk_id> (per-document chunk index)
//! - `tenant/{tenant_id}/document/{id}/chunk/{chunk_id}` -> DocumentChunkKv (JSON)
//! - `document-chunk-lookup/{chunk_id}` -> {tenant_id}|{document_id} (fast lookup by chunk)

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

/// Document KV representation (parity with SQL schema)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentKv {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub content_hash: String,
    pub file_path: String,
    pub file_size: i64,
    pub mime_type: String,
    pub page_count: Option<i32>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub metadata_json: Option<String>,
}

/// Document chunk KV representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentChunkKv {
    pub id: String,
    pub tenant_id: String,
    pub document_id: String,
    pub chunk_index: i32,
    pub page_number: Option<i32>,
    pub start_offset: Option<i32>,
    pub end_offset: Option<i32>,
    pub chunk_hash: String,
    pub text_preview: Option<String>,
    pub embedding_json: Option<String>,
    pub created_at: String,
}

pub struct DocumentKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl DocumentKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn now() -> String {
        Utc::now().to_rfc3339()
    }

    fn doc_key(tenant_id: &str, id: &str) -> String {
        format!("tenant/{}/document/{}", tenant_id, id)
    }

    fn doc_index_key(tenant_id: &str) -> String {
        format!("tenant/{}/documents", tenant_id)
    }

    fn hash_index_key(tenant_id: &str, hash: &str) -> String {
        format!("tenant/{}/document-by-hash/{}", tenant_id, hash)
    }

    fn doc_lookup_key(id: &str) -> String {
        format!("document-lookup/{}", id)
    }

    fn chunk_index_key(tenant_id: &str, document_id: &str) -> String {
        format!("tenant/{}/document/{}/chunks", tenant_id, document_id)
    }

    fn chunk_key(tenant_id: &str, document_id: &str, chunk_id: &str) -> String {
        format!(
            "tenant/{}/document/{}/chunk/{}",
            tenant_id, document_id, chunk_id
        )
    }

    fn chunk_lookup_key(chunk_id: &str) -> String {
        format!("document-chunk-lookup/{}", chunk_id)
    }

    async fn append_index(&self, tenant_id: &str, id: &str) -> Result<()> {
        let key = Self::doc_index_key(tenant_id);
        let mut ids: Vec<String> = match self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load document index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };
        if !ids.contains(&id.to_string()) {
            ids.push(id.to_string());
            let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
            self.backend.set(&key, payload).await.map_err(|e| {
                AosError::Database(format!("Failed to update document index: {}", e))
            })?;
        }
        Ok(())
    }

    async fn remove_index(&self, tenant_id: &str, id: &str) -> Result<()> {
        let key = Self::doc_index_key(tenant_id);
        if let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load document index: {}", e)))?
        {
            let mut ids: Vec<String> =
                serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
            ids.retain(|v| v != id);
            if ids.is_empty() {
                let _ = self.backend.delete(&key).await;
            } else {
                let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
                self.backend.set(&key, payload).await.map_err(|e| {
                    AosError::Database(format!("Failed to update document index: {}", e))
                })?;
            }
        }
        Ok(())
    }

    async fn append_chunk_index(
        &self,
        tenant_id: &str,
        document_id: &str,
        chunk_id: &str,
    ) -> Result<()> {
        let key = Self::chunk_index_key(tenant_id, document_id);
        let mut ids: Vec<String> = match self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load chunk index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };
        if !ids.contains(&chunk_id.to_string()) {
            ids.push(chunk_id.to_string());
            let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
            self.backend
                .set(&key, payload)
                .await
                .map_err(|e| AosError::Database(format!("Failed to update chunk index: {}", e)))?;
        }
        Ok(())
    }

    #[allow(dead_code)]
    async fn remove_chunk_index(
        &self,
        tenant_id: &str,
        document_id: &str,
        chunk_id: &str,
    ) -> Result<()> {
        let key = Self::chunk_index_key(tenant_id, document_id);
        if let Some(bytes) = self
            .backend
            .get(&key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load chunk index: {}", e)))?
        {
            let mut ids: Vec<String> =
                serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
            ids.retain(|v| v != chunk_id);
            if ids.is_empty() {
                let _ = self.backend.delete(&key).await;
            } else {
                let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
                self.backend.set(&key, payload).await.map_err(|e| {
                    AosError::Database(format!("Failed to update chunk index: {}", e))
                })?;
            }
        }
        Ok(())
    }

    /// Store a document.
    pub async fn put_document(&self, doc: &DocumentKv) -> Result<()> {
        let payload = serde_json::to_vec(doc).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::doc_key(&doc.tenant_id, &doc.id), payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store document: {}", e)))?;
        self.append_index(&doc.tenant_id, &doc.id).await?;
        // hash index
        self.backend
            .set(
                &Self::hash_index_key(&doc.tenant_id, &doc.content_hash),
                doc.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update document hash index: {}", e))
            })?;
        self.backend
            .set(
                &Self::doc_lookup_key(&doc.id),
                doc.tenant_id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| AosError::Database(format!("Failed to update document lookup: {}", e)))?;
        Ok(())
    }

    /// Upsert a chunk with a provided ID (used in migration/repair).
    pub async fn put_chunk(&self, chunk: &DocumentChunkKv) -> Result<()> {
        // Ensure document exists
        if self
            .get_document(&chunk.tenant_id, &chunk.document_id)
            .await?
            .is_none()
        {
            return Err(AosError::NotFound(format!(
                "Document {} not found for tenant {}",
                chunk.document_id, chunk.tenant_id
            )));
        }

        let payload = serde_json::to_vec(chunk).map_err(AosError::Serialization)?;
        self.backend
            .set(
                &Self::chunk_key(&chunk.tenant_id, &chunk.document_id, &chunk.id),
                payload,
            )
            .await
            .map_err(|e| AosError::Database(format!("Failed to store chunk: {}", e)))?;

        self.append_chunk_index(&chunk.tenant_id, &chunk.document_id, &chunk.id)
            .await?;
        self.backend
            .set(
                &Self::chunk_lookup_key(&chunk.id),
                format!("{}|{}", &chunk.tenant_id, &chunk.document_id).into_bytes(),
            )
            .await
            .map_err(|e| AosError::Database(format!("Failed to store chunk lookup: {}", e)))?;
        Ok(())
    }

    /// Create a new document with generated timestamps.
    pub async fn create_document(
        &self,
        tenant_id: &str,
        doc_id: &str,
        name: &str,
        content_hash: &str,
        file_path: &str,
        file_size: i64,
        mime_type: &str,
        page_count: Option<i32>,
    ) -> Result<String> {
        let now = Self::now();
        let doc = DocumentKv {
            id: doc_id.to_string(),
            tenant_id: tenant_id.to_string(),
            name: name.to_string(),
            content_hash: content_hash.to_string(),
            file_path: file_path.to_string(),
            file_size,
            mime_type: mime_type.to_string(),
            page_count,
            status: "pending".to_string(),
            created_at: now.clone(),
            updated_at: now,
            metadata_json: None,
        };
        self.put_document(&doc).await?;
        Ok(doc_id.to_string())
    }

    /// Fetch a document by ID.
    pub async fn get_document(&self, tenant_id: &str, id: &str) -> Result<Option<DocumentKv>> {
        let Some(bytes) = self
            .backend
            .get(&Self::doc_key(tenant_id, id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to load document: {}", e)))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    /// Fetch by ID without tenant by using lookup table.
    pub async fn get_document_any(&self, id: &str) -> Result<Option<DocumentKv>> {
        let Some(tenant_bytes) = self
            .backend
            .get(&Self::doc_lookup_key(id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read document lookup: {}", e)))?
        else {
            return Ok(None);
        };
        let tenant_id = String::from_utf8(tenant_bytes).unwrap_or_default();
        self.get_document(&tenant_id, id).await
    }

    /// Fetch multiple documents preserving input order.
    pub async fn get_documents_by_ids_ordered(
        &self,
        tenant_id: &str,
        ids: &[String],
    ) -> Result<Vec<Option<DocumentKv>>> {
        let mut map = std::collections::HashMap::new();
        for id in ids {
            if let Some(doc) = self.get_document(tenant_id, id).await? {
                map.insert(id.clone(), doc);
            }
        }
        Ok(ids.iter().map(|id| map.get(id).cloned()).collect())
    }

    /// List documents for a tenant with deterministic ordering (created_at DESC, id ASC).
    pub async fn list_documents(&self, tenant_id: &str) -> Result<Vec<DocumentKv>> {
        let ids: Vec<String> = match self
            .backend
            .get(&Self::doc_index_key(tenant_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read document index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };

        let mut docs = Vec::new();
        for id in ids {
            if let Some(doc) = self.get_document(tenant_id, &id).await? {
                docs.push(doc);
            }
        }

        docs.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(docs)
    }

    /// List documents with pagination using deterministic ordering.
    pub async fn list_documents_paginated(
        &self,
        tenant_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<DocumentKv>, i64)> {
        let docs = self.list_documents(tenant_id).await?;
        let total = docs.len() as i64;
        let start = offset.min(docs.len());
        let end = (start + limit).min(docs.len());
        Ok((docs[start..end].to_vec(), total))
    }

    /// Find document by content hash (tenant-scoped).
    pub async fn find_by_content_hash(
        &self,
        tenant_id: &str,
        hash: &str,
    ) -> Result<Option<DocumentKv>> {
        let Some(id_bytes) = self
            .backend
            .get(&Self::hash_index_key(tenant_id, hash))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read hash index: {}", e)))?
        else {
            return Ok(None);
        };
        let id = String::from_utf8(id_bytes).unwrap_or_default();
        self.get_document(tenant_id, &id).await
    }

    /// Update document status.
    pub async fn update_status(&self, tenant_id: &str, id: &str, status: &str) -> Result<()> {
        let Some(mut doc) = self.get_document(tenant_id, id).await? else {
            return Ok(());
        };
        doc.status = status.to_string();
        doc.updated_at = Self::now();
        self.put_document(&doc).await
    }

    /// Delete document and its chunks.
    pub async fn delete_document(&self, tenant_id: &str, id: &str) -> Result<()> {
        // remove chunks
        let chunk_index_key = Self::chunk_index_key(tenant_id, id);
        if let Some(bytes) = self
            .backend
            .get(&chunk_index_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load chunk index: {}", e)))?
        {
            let chunk_ids: Vec<String> =
                serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
            for cid in chunk_ids {
                let _ = self
                    .backend
                    .delete(&Self::chunk_key(tenant_id, id, &cid))
                    .await;
                let _ = self.backend.delete(&Self::chunk_lookup_key(&cid)).await;
            }
            let _ = self.backend.delete(&chunk_index_key).await;
        }

        // remove document
        self.backend
            .delete(&Self::doc_key(tenant_id, id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete document: {}", e)))?;

        // remove index and hash
        self.remove_index(tenant_id, id).await?;
        if let Some(doc) = self.get_document(tenant_id, id).await? {
            let _ = self
                .backend
                .delete(&Self::hash_index_key(tenant_id, &doc.content_hash))
                .await;
        }
        let _ = self.backend.delete(&Self::doc_lookup_key(id)).await;

        Ok(())
    }

    /// Create a document chunk.
    pub async fn create_chunk(
        &self,
        tenant_id: &str,
        document_id: &str,
        chunk_index: i32,
        page_number: Option<i32>,
        start_offset: Option<i32>,
        end_offset: Option<i32>,
        chunk_hash: &str,
        text_preview: Option<String>,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        let chunk = DocumentChunkKv {
            id: id.clone(),
            tenant_id: tenant_id.to_string(),
            document_id: document_id.to_string(),
            chunk_index,
            page_number,
            start_offset,
            end_offset,
            chunk_hash: chunk_hash.to_string(),
            text_preview,
            embedding_json: None,
            created_at: Self::now(),
        };

        // Delegate to canonical upsert path to keep a single implementation.
        self.put_chunk(&chunk).await?;
        Ok(id)
    }

    /// Get chunks for a document ordered by chunk_index ASC.
    pub async fn get_document_chunks(
        &self,
        tenant_id: &str,
        document_id: &str,
    ) -> Result<Vec<DocumentChunkKv>> {
        let ids: Vec<String> = match self
            .backend
            .get(&Self::chunk_index_key(tenant_id, document_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read chunk index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };

        let mut chunks = Vec::new();
        for cid in ids {
            if let Some(bytes) = self
                .backend
                .get(&Self::chunk_key(tenant_id, document_id, &cid))
                .await
                .map_err(|e| AosError::Database(format!("Failed to read chunk: {}", e)))?
            {
                if let Ok(chunk) = serde_json::from_slice::<DocumentChunkKv>(&bytes) {
                    chunks.push(chunk);
                }
            }
        }

        chunks.sort_by(|a, b| {
            a.chunk_index
                .cmp(&b.chunk_index)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(chunks)
    }

    /// Get chunks for multiple documents, ordered by (document_id ASC, chunk_index ASC).
    pub async fn get_chunks_for_documents(
        &self,
        tenant_id: &str,
        document_ids: &[String],
    ) -> Result<Vec<DocumentChunkKv>> {
        let mut all = Vec::new();
        for doc_id in document_ids {
            all.extend(self.get_document_chunks(tenant_id, doc_id).await?);
        }
        all.sort_by(|a, b| {
            a.document_id
                .cmp(&b.document_id)
                .then_with(|| a.chunk_index.cmp(&b.chunk_index))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(all)
    }

    /// Lookup chunk by ID (cross-document).
    pub async fn get_chunk_by_id(&self, chunk_id: &str) -> Result<Option<DocumentChunkKv>> {
        let Some(loc_bytes) = self
            .backend
            .get(&Self::chunk_lookup_key(chunk_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read chunk lookup: {}", e)))?
        else {
            return Ok(None);
        };
        let loc = String::from_utf8(loc_bytes).unwrap_or_default();
        let mut parts = loc.split('|');
        let tenant = parts.next().unwrap_or_default();
        let doc = parts.next().unwrap_or_default();
        let Some(bytes) = self
            .backend
            .get(&Self::chunk_key(tenant, doc, chunk_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read chunk: {}", e)))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }
}
