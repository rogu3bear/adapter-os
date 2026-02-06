//! KV storage for document collections and membership links.
//!
//! Keys (per-tenant namespace):
//! - `tenant/{tenant_id}/collection/{id}` -> DocumentCollectionKv (JSON)
//! - `tenant/{tenant_id}/collections` -> Vec<collection_id> (ordering: created_at DESC, id ASC)
//! - `tenant/{tenant_id}/collection-by-name/{name}` -> collection_id (dedupe)
//! - `tenant/{tenant_id}/collection/{id}/doc/{doc_id}` -> CollectionDocumentLink (JSON)
//! - `tenant/{tenant_id}/document/{doc_id}/collections` -> Vec<collection_id> (membership reverse index)

use adapteros_core::{AosError, Result};
use adapteros_storage::KvBackend;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentCollectionKv {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollectionDocumentLink {
    pub collection_id: String,
    pub document_id: String,
    pub tenant_id: String,
    pub added_at: String,
}

pub struct CollectionKvRepository {
    backend: Arc<dyn KvBackend>,
}

impl CollectionKvRepository {
    pub fn new(backend: Arc<dyn KvBackend>) -> Self {
        Self { backend }
    }

    fn now() -> String {
        Utc::now().to_rfc3339()
    }

    fn collection_key(tenant_id: &str, id: &str) -> String {
        format!("tenant/{}/collection/{}", tenant_id, id)
    }

    /// Idempotent upsert for a collection (migration/repair).
    pub async fn put_collection(&self, coll: &DocumentCollectionKv) -> Result<()> {
        let payload = serde_json::to_vec(coll).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::collection_key(&coll.tenant_id, &coll.id), payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store collection: {}", e)))?;
        self.append_collection_index(&coll.tenant_id, &coll.id)
            .await?;
        self.backend
            .set(
                &Self::name_index_key(&coll.tenant_id, &coll.name),
                coll.id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update collection name index: {}", e))
            })?;
        Ok(())
    }

    fn collection_index_key(tenant_id: &str) -> String {
        format!("tenant/{}/collections", tenant_id)
    }

    fn name_index_key(tenant_id: &str, name: &str) -> String {
        format!("tenant/{}/collection-by-name/{}", tenant_id, name)
    }

    fn collection_doc_key(tenant_id: &str, collection_id: &str, document_id: &str) -> String {
        format!(
            "tenant/{}/collection/{}/doc/{}",
            tenant_id, collection_id, document_id
        )
    }

    fn doc_collection_index_key(tenant_id: &str, document_id: &str) -> String {
        format!("tenant/{}/document/{}/collections", tenant_id, document_id)
    }

    async fn append_collection_index(&self, tenant_id: &str, id: &str) -> Result<()> {
        let key = Self::collection_index_key(tenant_id);
        let mut ids: Vec<String> =
            match self.backend.get(&key).await.map_err(|e| {
                AosError::Database(format!("Failed to read collection index: {}", e))
            })? {
                Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
                None => Vec::new(),
            };
        if !ids.contains(&id.to_string()) {
            ids.push(id.to_string());
            let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
            self.backend.set(&key, payload).await.map_err(|e| {
                AosError::Database(format!("Failed to update collection index: {}", e))
            })?;
        }
        Ok(())
    }

    async fn remove_collection_index(&self, tenant_id: &str, id: &str) -> Result<()> {
        let key = Self::collection_index_key(tenant_id);
        if let Some(bytes) =
            self.backend.get(&key).await.map_err(|e| {
                AosError::Database(format!("Failed to read collection index: {}", e))
            })?
        {
            let mut ids: Vec<String> =
                serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
            ids.retain(|v| v != id);
            if ids.is_empty() {
                if let Err(e) = self.backend.delete(&key).await {
                    tracing::warn!(
                        target: "storage.kv",
                        tenant_id = %tenant_id,
                        collection_id = %id,
                        error = %e,
                        "Failed to delete empty collection index"
                    );
                }
            } else {
                let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
                self.backend.set(&key, payload).await.map_err(|e| {
                    AosError::Database(format!("Failed to update collection index: {}", e))
                })?;
            }
        }
        Ok(())
    }

    async fn append_doc_reverse_index(
        &self,
        tenant_id: &str,
        document_id: &str,
        collection_id: &str,
    ) -> Result<()> {
        let key = Self::doc_collection_index_key(tenant_id, document_id);
        let mut ids: Vec<String> = match self.backend.get(&key).await.map_err(|e| {
            AosError::Database(format!("Failed to read doc collection index: {}", e))
        })? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };
        if !ids.contains(&collection_id.to_string()) {
            ids.push(collection_id.to_string());
            let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
            self.backend.set(&key, payload).await.map_err(|e| {
                AosError::Database(format!("Failed to update doc collection index: {}", e))
            })?;
        }
        Ok(())
    }

    async fn remove_doc_reverse_index(
        &self,
        tenant_id: &str,
        document_id: &str,
        collection_id: &str,
    ) -> Result<()> {
        let key = Self::doc_collection_index_key(tenant_id, document_id);
        if let Some(bytes) = self.backend.get(&key).await.map_err(|e| {
            AosError::Database(format!("Failed to read doc collection index: {}", e))
        })? {
            let mut ids: Vec<String> =
                serde_json::from_slice(&bytes).map_err(AosError::Serialization)?;
            ids.retain(|v| v != collection_id);
            if ids.is_empty() {
                if let Err(e) = self.backend.delete(&key).await {
                    tracing::warn!(
                        target: "storage.kv",
                        tenant_id = %tenant_id,
                        document_id = %document_id,
                        collection_id = %collection_id,
                        error = %e,
                        "Failed to delete empty doc collection index"
                    );
                }
            } else {
                let payload = serde_json::to_vec(&ids).map_err(AosError::Serialization)?;
                self.backend.set(&key, payload).await.map_err(|e| {
                    AosError::Database(format!("Failed to update doc collection index: {}", e))
                })?;
            }
        }
        Ok(())
    }

    /// Create collection.
    pub async fn create_collection(
        &self,
        tenant_id: &str,
        id: &str,
        name: &str,
        description: Option<String>,
        metadata_json: Option<String>,
    ) -> Result<String> {
        if self
            .backend
            .get(&Self::name_index_key(tenant_id, name))
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to read collection name index: {}", e))
            })?
            .is_some()
        {
            return Err(AosError::Database(format!(
                "Collection name already exists: {}",
                name
            )));
        }

        let now = Self::now();
        let coll = DocumentCollectionKv {
            id: id.to_string(),
            tenant_id: tenant_id.to_string(),
            name: name.to_string(),
            description,
            created_at: now.clone(),
            updated_at: now,
            metadata_json,
        };

        let payload = serde_json::to_vec(&coll).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::collection_key(tenant_id, id), payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store collection: {}", e)))?;
        self.append_collection_index(tenant_id, id).await?;
        self.backend
            .set(
                &Self::name_index_key(tenant_id, name),
                id.as_bytes().to_vec(),
            )
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to update collection name index: {}", e))
            })?;
        Ok(id.to_string())
    }

    pub async fn get_collection(
        &self,
        tenant_id: &str,
        id: &str,
    ) -> Result<Option<DocumentCollectionKv>> {
        let Some(bytes) = self
            .backend
            .get(&Self::collection_key(tenant_id, id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to load collection: {}", e)))?
        else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map_err(AosError::Serialization)
            .map(Some)
    }

    pub async fn get_collection_by_name(
        &self,
        tenant_id: &str,
        name: &str,
    ) -> Result<Option<DocumentCollectionKv>> {
        let Some(id_bytes) = self
            .backend
            .get(&Self::name_index_key(tenant_id, name))
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to read collection name index: {}", e))
            })?
        else {
            return Ok(None);
        };
        let id = String::from_utf8(id_bytes).unwrap_or_default();
        self.get_collection(tenant_id, &id).await
    }

    pub async fn list_collections(&self, tenant_id: &str) -> Result<Vec<DocumentCollectionKv>> {
        let ids: Vec<String> = match self
            .backend
            .get(&Self::collection_index_key(tenant_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read collection index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };

        let mut cols = Vec::new();
        for id in ids {
            if let Some(c) = self.get_collection(tenant_id, &id).await? {
                cols.push(c);
            }
        }

        cols.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(cols)
    }

    pub async fn list_collections_paginated(
        &self,
        tenant_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<DocumentCollectionKv>, i64)> {
        let cols = self.list_collections(tenant_id).await?;
        let total = cols.len() as i64;
        let start = offset.min(cols.len());
        let end = (start + limit).min(cols.len());
        Ok((cols[start..end].to_vec(), total))
    }

    pub async fn update_collection_metadata(
        &self,
        tenant_id: &str,
        id: &str,
        name: Option<&str>,
        description: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        let Some(mut coll) = self.get_collection(tenant_id, id).await? else {
            return Ok(());
        };

        if let Some(new_name) = name {
            if new_name != coll.name {
                // remove old name index
                if let Err(e) = self
                    .backend
                    .delete(&Self::name_index_key(tenant_id, &coll.name))
                    .await
                {
                    tracing::warn!(
                        target: "storage.kv",
                        tenant_id = %tenant_id,
                        collection_id = %id,
                        old_name = %coll.name,
                        error = %e,
                        "Failed to delete old collection name index"
                    );
                }
                self.backend
                    .set(
                        &Self::name_index_key(tenant_id, new_name),
                        id.as_bytes().to_vec(),
                    )
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to update name index: {}", e))
                    })?;
                coll.name = new_name.to_string();
            }
        }
        if let Some(desc) = description {
            coll.description = Some(desc.to_string());
        }
        if let Some(meta) = metadata_json {
            coll.metadata_json = Some(meta.to_string());
        }
        coll.updated_at = Self::now();

        let payload = serde_json::to_vec(&coll).map_err(AosError::Serialization)?;
        self.backend
            .set(&Self::collection_key(tenant_id, id), payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store collection: {}", e)))?;
        Ok(())
    }

    pub async fn delete_collection(&self, tenant_id: &str, id: &str) -> Result<()> {
        // remove membership links
        let prefix = format!("tenant/{}/collection/{}/doc/", tenant_id, id);
        for key in self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan collection docs: {}", e)))?
        {
            if let Some(bytes) = self.backend.get(&key).await.map_err(|e| {
                AosError::Database(format!("Failed to load collection-doc link: {}", e))
            })? {
                if let Ok(link) = serde_json::from_slice::<CollectionDocumentLink>(&bytes) {
                    self.remove_doc_reverse_index(tenant_id, &link.document_id, id)
                        .await?;
                }
            }
            if let Err(e) = self.backend.delete(&key).await {
                tracing::warn!(
                    target: "storage.kv",
                    tenant_id = %tenant_id,
                    collection_id = %id,
                    key = %key,
                    error = %e,
                    "Failed to delete collection-doc link"
                );
            }
        }

        // remove collection
        self.backend
            .delete(&Self::collection_key(tenant_id, id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete collection: {}", e)))?;
        self.remove_collection_index(tenant_id, id).await?;
        if let Err(e) = self
            .backend
            .delete(&Self::name_index_key(tenant_id, &format!("{}_removed", id)))
            .await
        {
            tracing::warn!(
                target: "storage.kv",
                tenant_id = %tenant_id,
                collection_id = %id,
                error = %e,
                "Failed to delete collection name index"
            );
        }
        Ok(())
    }

    pub async fn add_document_to_collection(
        &self,
        tenant_id: &str,
        collection_id: &str,
        document_id: &str,
        added_at: Option<String>,
    ) -> Result<()> {
        let link_key = Self::collection_doc_key(tenant_id, collection_id, document_id);
        if self
            .backend
            .get(&link_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to check link: {}", e)))?
            .is_some()
        {
            return Ok(()); // already linked
        }
        let link = CollectionDocumentLink {
            collection_id: collection_id.to_string(),
            document_id: document_id.to_string(),
            tenant_id: tenant_id.to_string(),
            added_at: added_at.unwrap_or_else(Self::now),
        };
        let payload = serde_json::to_vec(&link).map_err(AosError::Serialization)?;
        self.backend
            .set(&link_key, payload)
            .await
            .map_err(|e| AosError::Database(format!("Failed to store link: {}", e)))?;
        self.append_doc_reverse_index(tenant_id, document_id, collection_id)
            .await?;
        Ok(())
    }

    pub async fn remove_document_from_collection(
        &self,
        tenant_id: &str,
        collection_id: &str,
        document_id: &str,
    ) -> Result<()> {
        let link_key = Self::collection_doc_key(tenant_id, collection_id, document_id);
        self.backend
            .delete(&link_key)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete link: {}", e)))?;
        self.remove_doc_reverse_index(tenant_id, document_id, collection_id)
            .await?;
        Ok(())
    }

    pub async fn get_collection_documents(
        &self,
        tenant_id: &str,
        collection_id: &str,
        load_document: impl Fn(&str) -> Result<Option<crate::documents::Document>>,
    ) -> Result<Vec<crate::documents::Document>> {
        let links = self.list_collection_links(tenant_id, collection_id).await?;

        let mut docs = Vec::new();
        for link in links {
            if let Some(doc) = load_document(&link.document_id)? {
                docs.push(doc);
            }
        }
        Ok(docs)
    }

    pub async fn list_collection_links(
        &self,
        tenant_id: &str,
        collection_id: &str,
    ) -> Result<Vec<CollectionDocumentLink>> {
        let prefix = format!("tenant/{}/collection/{}/doc/", tenant_id, collection_id);
        let mut links = Vec::new();
        for key in self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan collection docs: {}", e)))?
        {
            if let Some(bytes) = self.backend.get(&key).await.map_err(|e| {
                AosError::Database(format!("Failed to load collection-doc link: {}", e))
            })? {
                if let Ok(link) = serde_json::from_slice::<CollectionDocumentLink>(&bytes) {
                    links.push(link);
                }
            }
        }

        // order by added_at DESC
        links.sort_by(|a, b| {
            b.added_at
                .cmp(&a.added_at)
                .then_with(|| a.document_id.cmp(&b.document_id))
        });
        Ok(links)
    }

    pub async fn count_collection_documents(
        &self,
        tenant_id: &str,
        collection_id: &str,
    ) -> Result<i64> {
        let prefix = format!("tenant/{}/collection/{}/doc/", tenant_id, collection_id);
        let keys =
            self.backend.scan_prefix(&prefix).await.map_err(|e| {
                AosError::Database(format!("Failed to scan collection docs: {}", e))
            })?;
        Ok(keys.len() as i64)
    }

    pub async fn list_collection_document_ids(
        &self,
        tenant_id: &str,
        collection_id: &str,
    ) -> Result<Vec<String>> {
        let prefix = format!("tenant/{}/collection/{}/doc/", tenant_id, collection_id);
        let mut ids = Vec::new();
        for key in self
            .backend
            .scan_prefix(&prefix)
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan collection docs: {}", e)))?
        {
            if let Some(bytes) = self.backend.get(&key).await.map_err(|e| {
                AosError::Database(format!("Failed to load collection-doc link: {}", e))
            })? {
                if let Ok(link) = serde_json::from_slice::<CollectionDocumentLink>(&bytes) {
                    ids.push(link.document_id);
                }
            }
        }
        Ok(ids)
    }

    pub async fn get_document_collections(
        &self,
        tenant_id: &str,
        document_id: &str,
    ) -> Result<Vec<DocumentCollectionKv>> {
        let key = Self::doc_collection_index_key(tenant_id, document_id);
        let ids: Vec<String> = match self.backend.get(&key).await.map_err(|e| {
            AosError::Database(format!("Failed to read doc collection index: {}", e))
        })? {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };

        let mut cols = Vec::new();
        for cid in ids {
            if let Some(c) = self.get_collection(tenant_id, &cid).await? {
                cols.push(c);
            }
        }

        cols.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(cols)
    }

    pub async fn is_document_in_collection(
        &self,
        tenant_id: &str,
        collection_id: &str,
        document_id: &str,
    ) -> Result<bool> {
        let key = Self::collection_doc_key(tenant_id, collection_id, document_id);
        let exists = self.backend.get(&key).await.map_err(|e| {
            AosError::Database(format!("Failed to read collection-doc link: {}", e))
        })?;
        Ok(exists.is_some())
    }

    pub async fn count_collections_by_tenant(&self, tenant_id: &str) -> Result<i64> {
        let ids: Vec<String> = match self
            .backend
            .get(&Self::collection_index_key(tenant_id))
            .await
            .map_err(|e| AosError::Database(format!("Failed to read collection index: {}", e)))?
        {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(AosError::Serialization)?,
            None => Vec::new(),
        };
        Ok(ids.len() as i64)
    }
}
