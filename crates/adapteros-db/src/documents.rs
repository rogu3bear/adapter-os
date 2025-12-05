//! Document database operations

use crate::documents_kv::{DocumentChunkKv, DocumentKv, DocumentKvRepository};
use crate::query_helpers::db_err;
use crate::{Db, KvBackend};
use adapteros_core::{AosError, Result};
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Document {
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentChunk {
    pub id: String,
    pub document_id: String,
    pub chunk_index: i32,
    pub page_number: Option<i32>,
    pub start_offset: Option<i32>,
    pub end_offset: Option<i32>,
    pub chunk_hash: String,
    pub text_preview: Option<String>,
    pub embedding_json: Option<String>,
}

impl From<DocumentKv> for Document {
    fn from(kv: DocumentKv) -> Self {
        Self {
            id: kv.id,
            tenant_id: kv.tenant_id,
            name: kv.name,
            content_hash: kv.content_hash,
            file_path: kv.file_path,
            file_size: kv.file_size,
            mime_type: kv.mime_type,
            page_count: kv.page_count,
            status: kv.status,
            created_at: kv.created_at,
            updated_at: kv.updated_at,
            metadata_json: kv.metadata_json,
        }
    }
}

impl From<DocumentChunkKv> for DocumentChunk {
    fn from(kv: DocumentChunkKv) -> Self {
        Self {
            id: kv.id,
            document_id: kv.document_id,
            chunk_index: kv.chunk_index,
            page_number: kv.page_number,
            start_offset: kv.start_offset,
            end_offset: kv.end_offset,
            chunk_hash: kv.chunk_hash,
            text_preview: kv.text_preview,
            embedding_json: kv.embedding_json,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDocumentParams {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub content_hash: String,
    pub file_path: String,
    pub file_size: i64,
    pub mime_type: String,
    pub page_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateChunkParams {
    pub tenant_id: String,
    pub document_id: String,
    pub chunk_index: i32,
    pub page_number: Option<i32>,
    pub start_offset: Option<i32>,
    pub end_offset: Option<i32>,
    pub chunk_hash: String,
    pub text_preview: Option<String>,
}

impl Db {
    fn get_document_kv_repo(&self) -> Option<DocumentKvRepository> {
        if self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv() {
            self.kv_backend().map(|kv| {
                let backend: Arc<dyn KvBackend> = kv.clone();
                DocumentKvRepository::new(backend)
            })
        } else {
            None
        }
    }

    async fn sql_create_document(&self, params: &CreateDocumentParams) -> Result<()> {
        sqlx::query(
            "INSERT INTO documents (
                id, tenant_id, name, content_hash, file_path, file_size,
                mime_type, page_count, status
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'pending')",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.name)
        .bind(&params.content_hash)
        .bind(&params.file_path)
        .bind(params.file_size)
        .bind(&params.mime_type)
        .bind(params.page_count)
        .execute(&*self.pool())
        .await
        .map_err(db_err("create document"))?;
        Ok(())
    }

    /// Create a new document
    pub async fn create_document(&self, params: CreateDocumentParams) -> Result<String> {
        // SQL write path if enabled
        if self.storage_mode().write_to_sql() {
            self.sql_create_document(&params).await?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No writable backend for create_document".to_string(),
            ));
        }

        // KV write path
        if let Some(repo) = self.get_document_kv_repo() {
            let res = repo
                .create_document(
                    &params.tenant_id,
                    &params.id,
                    &params.name,
                    &params.content_hash,
                    &params.file_path,
                    params.file_size,
                    &params.mime_type,
                    params.page_count,
                )
                .await;
            if let Err(e) = res {
                self.record_kv_write_fallback("documents.create");
                warn!(error = %e, doc_id = %params.id, "KV write failed for document");
            }
        }
        Ok(params.id)
    }

    /// Get document by ID with tenant isolation
    ///
    /// # Security
    /// This function enforces tenant isolation at the database layer.
    /// Documents are only returned if they belong to the specified tenant.
    async fn sql_get_document(&self, tenant_id: &str, id: &str) -> Result<Option<Document>> {
        sqlx::query_as::<_, Document>(
            "SELECT id, tenant_id, name, content_hash, file_path, file_size,
                    mime_type, page_count, status, created_at, updated_at, metadata_json
             FROM documents
             WHERE id = ? AND tenant_id = ?",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(db_err("get document"))
    }

    pub async fn get_document(&self, tenant_id: &str, id: &str) -> Result<Option<Document>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                let doc = repo.get_document(tenant_id, id).await?.map(Document::from);
                if doc.is_some() || !self.storage_mode().sql_fallback_enabled() {
                    return Ok(doc);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            return self.sql_get_document(tenant_id, id).await;
        }

        Ok(None)
    }

    /// Get multiple documents by their IDs, preserving input order with tenant isolation
    ///
    /// Returns documents in the same order as input IDs. Missing documents
    /// are returned as None in the result vector. This is used for replay
    /// with original RAG documents where some may have been deleted.
    ///
    /// # Security
    /// This function enforces tenant isolation at the database layer.
    /// Only documents belonging to the specified tenant are returned.
    ///
    /// # Arguments
    /// * `tenant_id` - Tenant ID for isolation
    /// * `doc_ids` - Slice of document IDs to retrieve, in desired order
    ///
    /// # Returns
    /// Vector of Option<Document> in same order as input IDs
    pub async fn get_documents_by_ids_ordered(
        &self,
        tenant_id: &str,
        doc_ids: &[String],
    ) -> Result<Vec<Option<Document>>> {
        if doc_ids.is_empty() {
            return Ok(Vec::new());
        }

        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                let docs = repo
                    .get_documents_by_ids_ordered(tenant_id, doc_ids)
                    .await?
                    .into_iter()
                    .map(|d| d.map(Document::from))
                    .collect();
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(docs);
                }
                // fallthrough to SQL merge for completeness
                let mut final_docs = docs;
                if self.storage_mode().read_from_sql() {
                    let sql_docs = self.sql_get_documents_by_ids(tenant_id, doc_ids).await?;
                    final_docs = sql_docs;
                }
                return Ok(final_docs);
            }
        }

        if self.storage_mode().read_from_sql() {
            return self.sql_get_documents_by_ids(tenant_id, doc_ids).await;
        }

        Ok(Vec::new())
    }

    async fn sql_get_documents_by_ids(
        &self,
        tenant_id: &str,
        doc_ids: &[String],
    ) -> Result<Vec<Option<Document>>> {
        if doc_ids.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders = doc_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let query = format!(
            "SELECT id, tenant_id, name, content_hash, file_path, file_size,
                    mime_type, page_count, status, created_at, updated_at, metadata_json
             FROM documents
             WHERE tenant_id = ? AND id IN ({})",
            placeholders
        );

        let mut query_builder = sqlx::query_as::<_, Document>(&query);
        query_builder = query_builder.bind(tenant_id);
        for id in doc_ids {
            query_builder = query_builder.bind(id);
        }

        let documents = query_builder
            .fetch_all(&*self.pool())
            .await
            .map_err(db_err("get documents by IDs"))?;

        let doc_map: std::collections::HashMap<String, Document> =
            documents.into_iter().map(|d| (d.id.clone(), d)).collect();
        let result = doc_ids.iter().map(|id| doc_map.get(id).cloned()).collect();
        Ok(result)
    }

    /// List documents for a tenant
    pub async fn list_documents(&self, tenant_id: &str) -> Result<Vec<Document>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                let docs = repo
                    .list_documents(tenant_id)
                    .await?
                    .into_iter()
                    .map(Document::from)
                    .collect();
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(docs);
                }
                if self.storage_mode().read_from_sql() {
                    return self.sql_list_documents(tenant_id).await;
                }
                return Ok(docs);
            }
        }

        self.sql_list_documents(tenant_id).await
    }

    async fn sql_list_documents(&self, tenant_id: &str) -> Result<Vec<Document>> {
        let documents = sqlx::query_as::<_, Document>(
            "SELECT id, tenant_id, name, content_hash, file_path, file_size,
                    mime_type, page_count, status, created_at, updated_at, metadata_json
             FROM documents
             WHERE tenant_id = ?
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("list documents"))?;
        Ok(documents)
    }

    /// List documents for a tenant with pagination
    pub async fn list_documents_paginated(
        &self,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<Document>, i64)> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                let (docs, total) = repo
                    .list_documents_paginated(tenant_id, limit as usize, offset as usize)
                    .await?;
                let docs = docs.into_iter().map(Document::from).collect();
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok((docs, total));
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            // Get total count for this tenant
            let total = sqlx::query("SELECT COUNT(*) as cnt FROM documents WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(&*self.pool())
                .await
                .map_err(db_err("count documents"))?
                .try_get::<i64, _>(0)
                .unwrap_or(0);

            // Get paginated results
            let documents = sqlx::query_as::<_, Document>(
                "SELECT id, tenant_id, name, content_hash, file_path, file_size,
                    mime_type, page_count, status, created_at, updated_at, metadata_json
             FROM documents
             WHERE tenant_id = ?
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(tenant_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&*self.pool())
            .await
            .map_err(db_err("list documents"))?;

            return Ok((documents, total));
        }

        Ok((Vec::new(), 0))
    }

    /// Find document by content hash within a tenant (for deduplication)
    ///
    /// Uses the existing idx_documents_content_hash index for efficient lookup.
    /// Returns the first document with matching hash, scoped to tenant for isolation.
    ///
    /// Evidence: migrations/0094_documents_collections.sql - idx_documents_content_hash index
    /// Pattern: Content-addressed deduplication
    pub async fn find_document_by_content_hash(
        &self,
        tenant_id: &str,
        content_hash: &str,
    ) -> Result<Option<Document>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                let doc = repo
                    .find_by_content_hash(tenant_id, content_hash)
                    .await?
                    .map(Document::from);
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(doc);
                }
                if doc.is_some() {
                    return Ok(doc);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let document = sqlx::query_as::<_, Document>(
                "SELECT id, tenant_id, name, content_hash, file_path, file_size,
                    mime_type, page_count, status, created_at, updated_at, metadata_json
             FROM documents
             WHERE tenant_id = ? AND content_hash = ?
             LIMIT 1",
            )
            .bind(tenant_id)
            .bind(content_hash)
            .fetch_optional(&*self.pool())
            .await
            .map_err(db_err("find document by hash"))?;
            return Ok(document);
        }

        Ok(None)
    }

    /// Update document status
    pub async fn update_document_status(&self, id: &str, status: &str) -> Result<()> {
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                if let Some(doc) = repo.get_document_any(id).await? {
                    if let Err(e) = repo.update_status(&doc.tenant_id, id, status).await {
                        self.record_kv_write_fallback("documents.update_status");
                        warn!(error = %e, document_id = %id, "KV update status failed");
                    }
                }
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE documents
             SET status = ?, updated_at = datetime('now')
             WHERE id = ?",
            )
            .bind(status)
            .bind(id)
            .execute(&*self.pool())
            .await
            .map_err(db_err("update document status"))?;
            return Ok(());
        }

        Ok(())
    }

    /// Delete document
    pub async fn delete_document(&self, id: &str) -> Result<()> {
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                if let Some(doc) = repo.get_document_any(id).await? {
                    if let Err(e) = repo.delete_document(&doc.tenant_id, id).await {
                        self.record_kv_write_fallback("documents.delete");
                        warn!(error = %e, document_id = %id, "KV delete failed");
                    }
                }
            }
        }

        if self.storage_mode().write_to_sql() {
            // Begin transaction for atomic multi-step deletion
            let mut tx = self
                .pool()
                .begin()
                .await
                .map_err(db_err("begin transaction"))?;

            // Delete chunks first (cascading)
            sqlx::query("DELETE FROM document_chunks WHERE document_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db_err("delete document chunks"))?;

            // Delete document
            sqlx::query("DELETE FROM documents WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db_err("delete document"))?;

            tx.commit().await.map_err(db_err("commit transaction"))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for delete_document".to_string(),
            ));
        }

        Ok(())
    }

    /// Create a document chunk
    pub async fn create_document_chunk(&self, params: CreateChunkParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "INSERT INTO document_chunks (
                id, tenant_id, document_id, chunk_index, page_number, start_offset,
                end_offset, chunk_hash, text_preview
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&params.tenant_id)
            .bind(&params.document_id)
            .bind(params.chunk_index)
            .bind(params.page_number)
            .bind(params.start_offset)
            .bind(params.end_offset)
            .bind(&params.chunk_hash)
            .bind(&params.text_preview)
            .execute(&*self.pool())
            .await
            .map_err(db_err("create document chunk"))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for create_document_chunk".to_string(),
            ));
        }

        if let Some(repo) = self.get_document_kv_repo() {
            if let Err(e) = repo
                .create_chunk(
                    &params.tenant_id,
                    &params.document_id,
                    params.chunk_index,
                    params.page_number,
                    params.start_offset,
                    params.end_offset,
                    &params.chunk_hash,
                    params.text_preview.clone(),
                )
                .await
            {
                self.record_kv_write_fallback("documents.create_chunk");
                warn!(error = %e, doc_id = %params.document_id, "KV chunk write failed");
            }
        }

        Ok(id)
    }

    /// Get chunks for a document with tenant isolation
    ///
    /// # Security
    /// This function enforces tenant isolation by joining with the documents table
    /// to verify the document belongs to the specified tenant.
    pub async fn get_document_chunks(
        &self,
        tenant_id: &str,
        document_id: &str,
    ) -> Result<Vec<DocumentChunk>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                let chunks = repo
                    .get_document_chunks(tenant_id, document_id)
                    .await?
                    .into_iter()
                    .map(DocumentChunk::from)
                    .collect();
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(chunks);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let chunks = sqlx::query_as::<_, DocumentChunk>(
                "SELECT dc.id, dc.document_id, dc.chunk_index, dc.page_number, dc.start_offset,
                    dc.end_offset, dc.chunk_hash, dc.text_preview, dc.embedding_json
             FROM document_chunks dc
             JOIN documents d ON dc.document_id = d.id
             WHERE dc.document_id = ? AND d.tenant_id = ?
             ORDER BY dc.chunk_index ASC",
            )
            .bind(document_id)
            .bind(tenant_id)
            .fetch_all(&*self.pool())
            .await
            .map_err(db_err("get document chunks"))?;
            return Ok(chunks);
        }

        Ok(Vec::new())
    }

    /// Get chunks for multiple documents with deterministic ordering.
    ///
    /// Returns all chunks from the specified documents, ordered by document_id ASC
    /// then chunk_index ASC. This deterministic ordering is critical for reproducible
    /// dataset generation (doc→dataset→adapter flow).
    ///
    /// # Arguments
    /// * `tenant_id` - Tenant ID for isolation (chunks must belong to this tenant)
    /// * `document_ids` - Slice of document IDs to fetch chunks for
    ///
    /// # Returns
    /// Vector of DocumentChunk sorted by (document_id, chunk_index)
    ///
    /// # Security
    /// This method enforces tenant isolation at the database level by filtering
    /// on tenant_id. Only chunks belonging to documents owned by the specified
    /// tenant will be returned.
    pub async fn get_chunks_for_documents(
        &self,
        tenant_id: &str,
        document_ids: &[String],
    ) -> Result<Vec<DocumentChunk>> {
        if document_ids.is_empty() {
            return Ok(Vec::new());
        }

        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                let chunks = repo
                    .get_chunks_for_documents(tenant_id, document_ids)
                    .await?
                    .into_iter()
                    .map(DocumentChunk::from)
                    .collect();
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(chunks);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let placeholders = document_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let query = format!(
                "SELECT id, document_id, chunk_index, page_number, start_offset,
                    end_offset, chunk_hash, text_preview, embedding_json
             FROM document_chunks
             WHERE tenant_id = ? AND document_id IN ({})
             ORDER BY document_id ASC, chunk_index ASC",
                placeholders
            );

            let mut query_builder = sqlx::query_as::<_, DocumentChunk>(&query);
            query_builder = query_builder.bind(tenant_id);
            for id in document_ids {
                query_builder = query_builder.bind(id);
            }

            let chunks = query_builder
                .fetch_all(&*self.pool())
                .await
                .map_err(db_err("get chunks for documents"))?;
            return Ok(chunks);
        }

        Ok(Vec::new())
    }

    /// Get chunk by ID
    pub async fn get_chunk_by_id(&self, chunk_id: &str) -> Result<Option<DocumentChunk>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_document_kv_repo() {
                let chunk = repo
                    .get_chunk_by_id(chunk_id)
                    .await?
                    .map(DocumentChunk::from);
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(chunk);
                }
                if chunk.is_some() {
                    return Ok(chunk);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let chunk = sqlx::query_as::<_, DocumentChunk>(
                "SELECT id, document_id, chunk_index, page_number, start_offset,
                    end_offset, chunk_hash, text_preview, embedding_json
             FROM document_chunks
             WHERE id = ?",
            )
            .bind(chunk_id)
            .fetch_optional(&*self.pool())
            .await
            .map_err(db_err("get chunk by ID"))?;
            return Ok(chunk);
        }

        Ok(None)
    }

    /// Get chunk by document_id and chunk_index
    ///
    /// Used to look up chunk metadata (especially page_number) when processing
    /// RAG results where we only have the doc_id in format `{document_id}__chunk_{index}`.
    pub async fn get_chunk_by_document_and_index(
        &self,
        document_id: &str,
        chunk_index: i32,
    ) -> Result<Option<DocumentChunk>> {
        let chunk = sqlx::query_as::<_, DocumentChunk>(
            "SELECT id, document_id, chunk_index, page_number, start_offset,
                    end_offset, chunk_hash, text_preview, embedding_json
             FROM document_chunks
             WHERE document_id = ? AND chunk_index = ?",
        )
        .bind(document_id)
        .bind(chunk_index)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get chunk by document and index: {}", e))
        })?;
        Ok(chunk)
    }

    /// Count chunks for a document
    pub async fn count_document_chunks(&self, document_id: &str) -> Result<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM document_chunks WHERE document_id = ?")
                .bind(document_id)
                .fetch_one(&*self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to count document chunks: {}", e))
                })?;
        Ok(count.0)
    }

    /// Get documents by status
    pub async fn get_documents_by_status(
        &self,
        tenant_id: &str,
        status: &str,
    ) -> Result<Vec<Document>> {
        let documents = sqlx::query_as::<_, Document>(
            "SELECT id, tenant_id, name, content_hash, file_path, file_size,
                    mime_type, page_count, status, created_at, updated_at, metadata_json
             FROM documents
             WHERE tenant_id = ? AND status = ?
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .bind(status)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("get documents by status"))?;
        Ok(documents)
    }

    /// Update document metadata
    pub async fn update_document_metadata(&self, id: &str, metadata_json: &str) -> Result<()> {
        sqlx::query(
            "UPDATE documents
             SET metadata_json = ?, updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(metadata_json)
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("update document metadata"))?;
        Ok(())
    }

    /// Count documents by tenant
    pub async fn count_documents_by_tenant(&self, tenant_id: &str) -> Result<i64> {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_one(&*self.pool())
            .await
            .map_err(db_err("count documents"))?;
        Ok(count.0)
    }

    /// Get total storage size for a tenant's documents
    pub async fn get_total_document_size(&self, tenant_id: &str) -> Result<i64> {
        let size: (Option<i64>,) =
            sqlx::query_as("SELECT SUM(file_size) FROM documents WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(&*self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to get total document size: {}", e))
                })?;
        Ok(size.0.unwrap_or(0))
    }
}
