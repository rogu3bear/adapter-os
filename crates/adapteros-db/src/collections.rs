//! Document collection database operations

use crate::collections_kv::{CollectionKvRepository, DocumentCollectionKv};
use crate::documents::Document;
use crate::new_id;
use crate::query_helpers::db_err;
use crate::{Db, KvBackend};
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::sync::Arc;
use tracing::warn;

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

impl From<DocumentCollectionKv> for DocumentCollection {
    fn from(kv: DocumentCollectionKv) -> Self {
        Self {
            id: kv.id,
            tenant_id: kv.tenant_id,
            name: kv.name,
            description: kv.description,
            created_at: kv.created_at,
            updated_at: kv.updated_at,
            metadata_json: kv.metadata_json,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCollectionParams {
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub metadata_json: Option<String>,
}

impl Db {
    fn get_collection_kv_repo(&self) -> Option<CollectionKvRepository> {
        if self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv() {
            self.kv_backend().map(|kv| {
                let backend: Arc<dyn KvBackend> = kv.clone();
                CollectionKvRepository::new(backend)
            })
        } else {
            None
        }
    }

    /// Create a new document collection
    pub async fn create_collection(&self, params: CreateCollectionParams) -> Result<String> {
        let id = new_id(IdPrefix::Col);
        if self.storage_mode().write_to_sql() {
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
            .execute(self.pool())
            .await
            .map_err(db_err("create collection"))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for create_collection".to_string(),
            ));
        }

        if let Some(repo) = self.get_collection_kv_repo() {
            if let Err(e) = repo
                .create_collection(
                    &params.tenant_id,
                    &id,
                    &params.name,
                    params.description.clone(),
                    params.metadata_json.clone(),
                )
                .await
            {
                self.record_kv_write_fallback("collections.create");
                warn!(error = %e, collection_id = %id, "KV write failed for collection");
            }
        }

        Ok(id)
    }

    /// Get collection by ID with tenant isolation
    ///
    /// # Security
    /// This function enforces tenant isolation at the database layer.
    /// Collections are only returned if they belong to the specified tenant.
    pub async fn get_collection(
        &self,
        tenant_id: &str,
        id: &str,
    ) -> Result<Option<DocumentCollection>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                let coll = repo
                    .get_collection(tenant_id, id)
                    .await?
                    .map(DocumentCollection::from);
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(coll);
                }
                if coll.is_some() {
                    return Ok(coll);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let collection = sqlx::query_as::<_, DocumentCollection>(
                "SELECT id, tenant_id, name, description, created_at, updated_at, metadata_json
             FROM document_collections
             WHERE id = ? AND tenant_id = ?",
            )
            .bind(id)
            .bind(tenant_id)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err("get collection"))?;
            return Ok(collection);
        }

        Ok(None)
    }

    /// List collections for a tenant
    pub async fn list_collections(&self, tenant_id: &str) -> Result<Vec<DocumentCollection>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                let cols = repo
                    .list_collections(tenant_id)
                    .await?
                    .into_iter()
                    .map(DocumentCollection::from)
                    .collect();
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(cols);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let collections = sqlx::query_as::<_, DocumentCollection>(
                "SELECT id, tenant_id, name, description, created_at, updated_at, metadata_json
             FROM document_collections
             WHERE tenant_id = ?
             ORDER BY created_at DESC",
            )
            .bind(tenant_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list collections"))?;
            return Ok(collections);
        }

        Ok(Vec::new())
    }

    /// List collections for a tenant with pagination
    pub async fn list_collections_paginated(
        &self,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<DocumentCollection>, i64)> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                let (cols, total) = repo
                    .list_collections_paginated(tenant_id, limit as usize, offset as usize)
                    .await?;
                let cols = cols.into_iter().map(DocumentCollection::from).collect();
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok((cols, total));
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let total =
                sqlx::query("SELECT COUNT(*) as cnt FROM document_collections WHERE tenant_id = ?")
                    .bind(tenant_id)
                    .fetch_one(self.pool())
                    .await
                    .map_err(db_err("count collections"))?
                    .get::<i64, _>(0);

            let collections = sqlx::query_as::<_, DocumentCollection>(
                "SELECT id, tenant_id, name, description, created_at, updated_at, metadata_json
             FROM document_collections
             WHERE tenant_id = ?
             ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(tenant_id)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("list collections"))?;

            return Ok((collections, total));
        }

        Ok((Vec::new(), 0))
    }

    /// Delete collection
    pub async fn delete_collection(&self, id: &str) -> Result<()> {
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                // Determine tenant from lookup (scan indexes)
                // Use best-effort: iterate tenant collection indexes
                if let Some(kv) = self.kv_backend() {
                    let backend: Arc<dyn KvBackend> = kv.clone();
                    for key in backend
                        .scan_prefix("tenant/")
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|k| k.ends_with("/collections"))
                    {
                        let tenant_id = key
                            .trim_start_matches("tenant/")
                            .trim_end_matches("/collections");
                        if repo.get_collection(tenant_id, id).await?.is_some() {
                            if let Err(e) = repo.delete_collection(tenant_id, id).await {
                                self.record_kv_write_fallback("collections.delete");
                                warn!(error = %e, collection_id = %id, "KV delete failed");
                            }
                            break;
                        }
                    }
                }
            }
        }

        if self.storage_mode().write_to_sql() {
            let mut tx = self.begin_write_tx().await?;

            sqlx::query("DELETE FROM collection_documents WHERE collection_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to delete collection-document links: {}", e))
                })?;

            sqlx::query("DELETE FROM document_collections WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(db_err("delete collection"))?;

            tx.commit().await.map_err(db_err("commit transaction"))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for delete_collection".to_string(),
            ));
        }

        Ok(())
    }

    /// Add document to collection with tenant isolation
    ///
    /// # Security
    /// Both the collection and document must belong to the specified tenant.
    /// The composite FK constraints in the schema enforce this at the database level.
    pub async fn add_document_to_collection(
        &self,
        tenant_id: &str,
        collection_id: &str,
        document_id: &str,
    ) -> Result<()> {
        let added_at = Utc::now().to_rfc3339();

        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                if let Err(e) = repo
                    .add_document_to_collection(
                        tenant_id,
                        collection_id,
                        document_id,
                        Some(added_at.clone()),
                    )
                    .await
                {
                    self.record_kv_write_fallback("collections.add_document");
                    warn!(error = %e, collection_id = %collection_id, doc_id = %document_id, "KV add doc->collection failed");
                }
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "INSERT INTO collection_documents (tenant_id, collection_id, document_id, added_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(collection_id, document_id) DO NOTHING",
            )
            .bind(tenant_id)
            .bind(collection_id)
            .bind(document_id)
            .bind(&added_at)
            .execute(self.pool())
            .await
            .map_err(db_err("add document to collection"))?;
            return Ok(());
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for add_document_to_collection".to_string(),
            ));
        }

        Ok(())
    }

    /// Remove document from collection
    pub async fn remove_document_from_collection(
        &self,
        collection_id: &str,
        document_id: &str,
    ) -> Result<()> {
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                // tenant_id needed; attempt best-effort scan using lookup of collection
                if let Some(kv) = self.kv_backend() {
                    let backend: Arc<dyn KvBackend> = kv.clone();
                    for key in backend
                        .scan_prefix("tenant/")
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|k| k.ends_with("/collections"))
                    {
                        let tenant_id = key
                            .trim_start_matches("tenant/")
                            .trim_end_matches("/collections");
                        if repo
                            .get_collection(tenant_id, collection_id)
                            .await?
                            .is_some()
                        {
                            let _ = repo
                                .remove_document_from_collection(
                                    tenant_id,
                                    collection_id,
                                    document_id,
                                )
                                .await;
                            break;
                        }
                    }
                }
            }
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "DELETE FROM collection_documents
             WHERE collection_id = ? AND document_id = ?",
            )
            .bind(collection_id)
            .bind(document_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to remove document from collection: {}", e))
            })?;
            return Ok(());
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for remove_document_from_collection".to_string(),
            ));
        }

        Ok(())
    }

    /// Get documents in a collection with tenant isolation
    ///
    /// # Security
    /// This function enforces tenant isolation by filtering on document tenant_id.
    /// Only documents belonging to the specified tenant are returned.
    pub async fn get_collection_documents(
        &self,
        tenant_id: &str,
        collection_id: &str,
    ) -> Result<Vec<Document>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                let mut docs = Vec::new();
                let links = repo.list_collection_links(tenant_id, collection_id).await?;
                for link in links {
                    if let Some(doc) = self.get_document(tenant_id, &link.document_id).await? {
                        docs.push(doc);
                    }
                }
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(docs);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let documents = sqlx::query_as::<_, Document>(
                "SELECT d.id, d.tenant_id, d.name, d.content_hash, d.file_path, d.file_size,
                    d.mime_type, d.page_count, d.status, d.created_at, d.updated_at, d.metadata_json,
                    d.error_message, d.error_code, d.retry_count, d.max_retries,
                    d.processing_started_at, d.processing_completed_at
             FROM documents d
             INNER JOIN collection_documents cd ON d.id = cd.document_id
             WHERE cd.collection_id = ? AND d.tenant_id = ?
             ORDER BY cd.added_at DESC",
            )
            .bind(collection_id)
            .bind(tenant_id)
            .fetch_all(self.pool())
            .await
            .map_err(db_err("get collection documents"))?;
            return Ok(documents);
        }

        Ok(Vec::new())
    }

    /// Count documents in a collection
    pub async fn count_collection_documents(&self, collection_id: &str) -> Result<i64> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                // tenant lookup via index
                if let Some(kv) = self.kv_backend() {
                    let backend: Arc<dyn KvBackend> = kv.clone();
                    for key in backend
                        .scan_prefix("tenant/")
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|k| k.ends_with("/collections"))
                    {
                        let tenant_id = key
                            .trim_start_matches("tenant/")
                            .trim_end_matches("/collections");
                        if repo
                            .get_collection(tenant_id, collection_id)
                            .await?
                            .is_some()
                        {
                            let count = repo
                                .count_collection_documents(tenant_id, collection_id)
                                .await?;
                            if !self.storage_mode().sql_fallback_enabled() {
                                return Ok(count);
                            }
                        }
                    }
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM collection_documents WHERE collection_id = ?")
                    .bind(collection_id)
                    .fetch_one(self.pool())
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to count collection documents: {}", e))
                    })?;
            return Ok(count.0);
        }

        Ok(0)
    }

    /// Get just document IDs in a collection (efficient - no full document load)
    ///
    /// Returns only the document IDs without loading full document records.
    /// Use this for filtering operations where you only need to check membership.
    pub async fn list_collection_document_ids(&self, collection_id: &str) -> Result<Vec<String>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                if let Some(kv) = self.kv_backend() {
                    let backend: Arc<dyn KvBackend> = kv.clone();
                    for key in backend
                        .scan_prefix("tenant/")
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|k| k.ends_with("/collections"))
                    {
                        let tenant_id = key
                            .trim_start_matches("tenant/")
                            .trim_end_matches("/collections");
                        if repo
                            .get_collection(tenant_id, collection_id)
                            .await?
                            .is_some()
                        {
                            let ids = repo
                                .list_collection_document_ids(tenant_id, collection_id)
                                .await?;
                            if !self.storage_mode().sql_fallback_enabled() {
                                return Ok(ids);
                            }
                        }
                    }
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT document_id FROM collection_documents WHERE collection_id = ?",
            )
            .bind(collection_id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to list collection document IDs: {}", e))
            })?;
            return Ok(rows.into_iter().map(|(id,)| id).collect());
        }

        Ok(Vec::new())
    }

    /// Get collections containing a document with tenant isolation
    ///
    /// # Security
    /// This function enforces tenant isolation by filtering on collection tenant_id.
    /// Only collections belonging to the specified tenant are returned.
    pub async fn get_document_collections(
        &self,
        tenant_id: &str,
        document_id: &str,
    ) -> Result<Vec<DocumentCollection>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                let cols = repo
                    .get_document_collections(tenant_id, document_id)
                    .await?
                    .into_iter()
                    .map(DocumentCollection::from)
                    .collect();
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(cols);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let collections = sqlx::query_as::<_, DocumentCollection>(
                "SELECT dc.id, dc.tenant_id, dc.name, dc.description, dc.created_at, dc.updated_at, dc.metadata_json
             FROM document_collections dc
             INNER JOIN collection_documents cd ON dc.id = cd.collection_id
             WHERE cd.document_id = ? AND dc.tenant_id = ?
             ORDER BY dc.created_at DESC",
            )
            .bind(document_id)
            .bind(tenant_id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to get document collections: {}", e))
            })?;
            return Ok(collections);
        }

        Ok(Vec::new())
    }

    /// Update collection metadata
    pub async fn update_collection_metadata(
        &self,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                if let Some(kv) = self.kv_backend() {
                    let backend: Arc<dyn KvBackend> = kv.clone();
                    for key in backend
                        .scan_prefix("tenant/")
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|k| k.ends_with("/collections"))
                    {
                        let tenant_id = key
                            .trim_start_matches("tenant/")
                            .trim_end_matches("/collections");
                        if repo.get_collection(tenant_id, id).await?.is_some() {
                            if let Err(e) = repo
                                .update_collection_metadata(
                                    tenant_id,
                                    id,
                                    name,
                                    description,
                                    metadata_json,
                                )
                                .await
                            {
                                self.record_kv_write_fallback("collections.update_metadata");
                                warn!(error = %e, collection_id = %id, "KV update collection metadata failed");
                            }
                            break;
                        }
                    }
                }
            }
        }

        if self.storage_mode().write_to_sql() {
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
            .execute(self.pool())
            .await
            .map_err(db_err("update collection metadata"))?;
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_collection_metadata".to_string(),
            ));
        }
        Ok(())
    }

    /// Check if document is in collection
    pub async fn is_document_in_collection(
        &self,
        collection_id: &str,
        document_id: &str,
    ) -> Result<bool> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                // find tenant via lookup
                if let Some(kv) = self.kv_backend() {
                    let backend: Arc<dyn KvBackend> = kv.clone();
                    for key in backend
                        .scan_prefix("tenant/")
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|k| k.ends_with("/collections"))
                    {
                        let tenant_id = key
                            .trim_start_matches("tenant/")
                            .trim_end_matches("/collections");
                        if repo
                            .get_collection(tenant_id, collection_id)
                            .await?
                            .is_some()
                        {
                            let exists = repo
                                .is_document_in_collection(tenant_id, collection_id, document_id)
                                .await?;
                            if !self.storage_mode().sql_fallback_enabled() {
                                return Ok(exists);
                            }
                            return Ok(exists);
                        }
                    }
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let count: (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM collection_documents
             WHERE collection_id = ? AND document_id = ?",
            )
            .bind(collection_id)
            .bind(document_id)
            .fetch_one(self.pool())
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to check document in collection: {}", e))
            })?;
            return Ok(count.0 > 0);
        }

        Ok(false)
    }

    /// Get collection by name and tenant
    pub async fn get_collection_by_name(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<Option<DocumentCollection>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                let coll = repo
                    .get_collection_by_name(tenant_id, name)
                    .await?
                    .map(DocumentCollection::from);
                if !self.storage_mode().sql_fallback_enabled() || coll.is_some() {
                    return Ok(coll);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let collection = sqlx::query_as::<_, DocumentCollection>(
                "SELECT id, tenant_id, name, description, created_at, updated_at, metadata_json
             FROM document_collections
             WHERE tenant_id = ? AND name = ?",
            )
            .bind(tenant_id)
            .bind(name)
            .fetch_optional(self.pool())
            .await
            .map_err(db_err("get collection by name"))?;
            return Ok(collection);
        }

        Ok(None)
    }

    /// Count collections by tenant
    pub async fn count_collections_by_tenant(&self, tenant_id: &str) -> Result<i64> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_collection_kv_repo() {
                let count = repo.count_collections_by_tenant(tenant_id).await?;
                if !self.storage_mode().sql_fallback_enabled() {
                    return Ok(count);
                }
            }
        }

        if self.storage_mode().read_from_sql() {
            let count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM document_collections WHERE tenant_id = ?")
                    .bind(tenant_id)
                    .fetch_one(self.pool())
                    .await
                    .map_err(db_err("count collections"))?;
            return Ok(count.0);
        }

        Ok(0)
    }
}
