//! Document collection database operations

use crate::documents::Document;
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DocumentCollection {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCollectionParams {
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub metadata_json: Option<String>,
}

impl Db {
    /// Create a new document collection
    pub async fn create_collection(&self, params: CreateCollectionParams) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO document_collections (
                id, tenant_id, name, description, metadata_json
            ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&params.tenant_id)
        .bind(&params.name)
        .bind(&params.description)
        .bind(&params.metadata_json)
        .execute(&*self.pool())
        .await
        .map_err(db_err("create collection"))?;
        Ok(id)
    }

    /// Get collection by ID
    pub async fn get_collection(&self, id: &str) -> Result<Option<DocumentCollection>> {
        let collection = sqlx::query_as::<_, DocumentCollection>(
            "SELECT id, tenant_id, name, description, created_at, updated_at, metadata_json
             FROM document_collections
             WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(db_err("get collection"))?;
        Ok(collection)
    }

    /// List collections for a tenant
    pub async fn list_collections(&self, tenant_id: &str) -> Result<Vec<DocumentCollection>> {
        let collections = sqlx::query_as::<_, DocumentCollection>(
            "SELECT id, tenant_id, name, description, created_at, updated_at, metadata_json
             FROM document_collections
             WHERE tenant_id = ?
             ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("list collections"))?;
        Ok(collections)
    }

    /// List collections for a tenant with pagination
    pub async fn list_collections_paginated(&self, tenant_id: &str, limit: i64, offset: i64) -> Result<(Vec<DocumentCollection>, i64)> {
        // Get total count for this tenant
        let total = sqlx::query("SELECT COUNT(*) as cnt FROM document_collections WHERE tenant_id = ?")
            .bind(tenant_id)
            .fetch_one(&*self.pool())
            .await
            .map_err(db_err("count collections"))?
            .get::<i64, _>(0);

        // Get paginated results
        let collections = sqlx::query_as::<_, DocumentCollection>(
            "SELECT id, tenant_id, name, description, created_at, updated_at, metadata_json
             FROM document_collections
             WHERE tenant_id = ?
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("list collections"))?;

        Ok((collections, total))
    }

    /// Delete collection
    pub async fn delete_collection(&self, id: &str) -> Result<()> {
        // Begin transaction for atomic multi-step deletion
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(db_err("begin transaction"))?;

        // Delete collection-document links first (cascading)
        sqlx::query("DELETE FROM collection_documents WHERE collection_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to delete collection-document links: {}", e))
            })?;

        // Delete collection
        sqlx::query("DELETE FROM document_collections WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(db_err("delete collection"))?;

        // Commit transaction
        tx.commit()
            .await
            .map_err(db_err("commit transaction"))?;

        Ok(())
    }

    /// Add document to collection
    pub async fn add_document_to_collection(
        &self,
        collection_id: &str,
        document_id: &str,
    ) -> Result<()> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO collection_documents (id, collection_id, document_id)
             VALUES (?, ?, ?)
             ON CONFLICT(collection_id, document_id) DO NOTHING",
        )
        .bind(&id)
        .bind(collection_id)
        .bind(document_id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("add document to collection"))?;
        Ok(())
    }

    /// Remove document from collection
    pub async fn remove_document_from_collection(
        &self,
        collection_id: &str,
        document_id: &str,
    ) -> Result<()> {
        sqlx::query(
            "DELETE FROM collection_documents
             WHERE collection_id = ? AND document_id = ?",
        )
        .bind(collection_id)
        .bind(document_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to remove document from collection: {}", e))
        })?;
        Ok(())
    }

    /// Get documents in a collection
    pub async fn get_collection_documents(&self, collection_id: &str) -> Result<Vec<Document>> {
        let documents = sqlx::query_as::<_, Document>(
            "SELECT d.id, d.tenant_id, d.name, d.content_hash, d.file_path, d.file_size,
                    d.mime_type, d.page_count, d.status, d.created_at, d.updated_at, d.metadata_json
             FROM documents d
             INNER JOIN collection_documents cd ON d.id = cd.document_id
             WHERE cd.collection_id = ?
             ORDER BY cd.created_at DESC",
        )
        .bind(collection_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(db_err("get collection documents"))?;
        Ok(documents)
    }

    /// Count documents in a collection
    pub async fn count_collection_documents(&self, collection_id: &str) -> Result<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM collection_documents WHERE collection_id = ?")
                .bind(collection_id)
                .fetch_one(&*self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to count collection documents: {}", e))
                })?;
        Ok(count.0)
    }

    /// Get just document IDs in a collection (efficient - no full document load)
    ///
    /// Returns only the document IDs without loading full document records.
    /// Use this for filtering operations where you only need to check membership.
    pub async fn list_collection_document_ids(&self, collection_id: &str) -> Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT document_id FROM collection_documents WHERE collection_id = ?")
                .bind(collection_id)
                .fetch_all(&*self.pool())
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to list collection document IDs: {}", e))
                })?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Get collections containing a document
    pub async fn get_document_collections(
        &self,
        document_id: &str,
    ) -> Result<Vec<DocumentCollection>> {
        let collections = sqlx::query_as::<_, DocumentCollection>(
            "SELECT dc.id, dc.tenant_id, dc.name, dc.description, dc.created_at, dc.updated_at, dc.metadata_json
             FROM document_collections dc
             INNER JOIN collection_documents cd ON dc.id = cd.collection_id
             WHERE cd.document_id = ?
             ORDER BY dc.created_at DESC",
        )
        .bind(document_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to get document collections: {}", e))
        })?;
        Ok(collections)
    }

    /// Update collection metadata
    pub async fn update_collection_metadata(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE document_collections
             SET name = COALESCE(?, name),
                 description = COALESCE(?, description),
                 metadata_json = COALESCE(?, metadata_json),
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(name)
        .bind(description)
        .bind(metadata_json)
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(db_err("update collection metadata"))?;
        Ok(())
    }

    /// Check if document is in collection
    pub async fn is_document_in_collection(
        &self,
        collection_id: &str,
        document_id: &str,
    ) -> Result<bool> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM collection_documents
             WHERE collection_id = ? AND document_id = ?",
        )
        .bind(collection_id)
        .bind(document_id)
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to check document in collection: {}", e))
        })?;
        Ok(count.0 > 0)
    }

    /// Get collection by name and tenant
    pub async fn get_collection_by_name(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<Option<DocumentCollection>> {
        let collection = sqlx::query_as::<_, DocumentCollection>(
            "SELECT id, tenant_id, name, description, created_at, updated_at, metadata_json
             FROM document_collections
             WHERE tenant_id = ? AND name = ?",
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_optional(&*self.pool())
        .await
        .map_err(db_err("get collection by name"))?;
        Ok(collection)
    }

    /// Count collections by tenant
    pub async fn count_collections_by_tenant(&self, tenant_id: &str) -> Result<i64> {
        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM document_collections WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(&*self.pool())
                .await
                .map_err(db_err("count collections"))?;
        Ok(count.0)
    }
}
