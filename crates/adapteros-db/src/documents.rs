//! Document database operations

use crate::query_helpers::db_err;
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

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
    /// Create a new document
    pub async fn create_document(&self, params: CreateDocumentParams) -> Result<String> {
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
        Ok(params.id)
    }

    /// Get document by ID with tenant isolation
    ///
    /// # Security
    /// This function enforces tenant isolation at the database layer.
    /// Documents are only returned if they belong to the specified tenant.
    pub async fn get_document(&self, tenant_id: &str, id: &str) -> Result<Option<Document>> {
        let document = sqlx::query_as::<_, Document>(
            "SELECT id, tenant_id, name, content_hash, file_path, file_size,
                    mime_type, page_count, status, created_at, updated_at, metadata_json
             FROM documents
             WHERE id = ? AND tenant_id = ?",
        )
        .bind(id)
        .bind(tenant_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(db_err("get document"))?;
        Ok(document)
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

        // Fetch all documents that match any of the IDs AND belong to the tenant
        // Using a hashmap for O(1) lookup during reordering
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

        // Build hashmap for efficient lookup
        let doc_map: std::collections::HashMap<String, Document> =
            documents.into_iter().map(|d| (d.id.clone(), d)).collect();

        // Reorder to match input order, with None for missing docs
        let result = doc_ids.iter().map(|id| doc_map.get(id).cloned()).collect();

        Ok(result)
    }

    /// List documents for a tenant
    pub async fn list_documents(&self, tenant_id: &str) -> Result<Vec<Document>> {
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

        Ok((documents, total))
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
        Ok(document)
    }

    /// Update document status
    pub async fn update_document_status(&self, id: &str, status: &str) -> Result<()> {
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
        Ok(())
    }

    /// Delete document
    pub async fn delete_document(&self, id: &str) -> Result<()> {
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

        // Commit transaction
        tx.commit().await.map_err(db_err("commit transaction"))?;

        Ok(())
    }

    /// Create a document chunk
    pub async fn create_document_chunk(&self, params: CreateChunkParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();
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
        Ok(chunks)
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

        Ok(chunks)
    }

    /// Get chunk by ID
    pub async fn get_chunk_by_id(&self, chunk_id: &str) -> Result<Option<DocumentChunk>> {
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
        Ok(chunk)
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
