//! Document database operations

use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;
use crate::query_helpers::db_err;

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

    /// Get document by ID
    pub async fn get_document(&self, id: &str) -> Result<Option<Document>> {
        let document = sqlx::query_as::<_, Document>(
            "SELECT id, tenant_id, name, content_hash, file_path, file_size,
                    mime_type, page_count, status, created_at, updated_at, metadata_json
             FROM documents
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(db_err("get document"))?;
        Ok(document)
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
    pub async fn list_documents_paginated(&self, tenant_id: &str, limit: i64, offset: i64) -> Result<(Vec<Document>, i64)> {
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
        tx.commit()
            .await
            .map_err(db_err("commit transaction"))?;

        Ok(())
    }

    /// Create a document chunk
    pub async fn create_document_chunk(&self, params: CreateChunkParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO document_chunks (
                id, document_id, chunk_index, page_number, start_offset,
                end_offset, chunk_hash, text_preview
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
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

    /// Get chunks for a document
    pub async fn get_document_chunks(&self, document_id: &str) -> Result<Vec<DocumentChunk>> {
        let chunks = sqlx::query_as::<_, DocumentChunk>(
            "SELECT id, document_id, chunk_index, page_number, start_offset,
                    end_offset, chunk_hash, text_preview
             FROM document_chunks
             WHERE document_id = ?
             ORDER BY chunk_index ASC",
        )
        .bind(document_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("get document chunks"))?;
        Ok(chunks)
    }

    /// Get chunk by ID
    pub async fn get_chunk_by_id(&self, chunk_id: &str) -> Result<Option<DocumentChunk>> {
        let chunk = sqlx::query_as::<_, DocumentChunk>(
            "SELECT id, document_id, chunk_index, page_number, start_offset,
                    end_offset, chunk_hash, text_preview
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
                    end_offset, chunk_hash, text_preview
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
