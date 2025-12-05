//! Replay repository backed by KV storage.
//!
//! Provides parity with SQL replay tables for metadata, executions, and sessions.

use crate::error::StorageError;
use crate::kv::backend::KvBackend;
use crate::kv::indexing::{replay_indexes, IndexManager};
use crate::models::{ReplayExecutionKv, ReplayMetadataKv, ReplaySessionKv};
use crate::types::{KeyBuilder, VersionedRecord};
use std::sync::Arc;

/// Repository for replay metadata and executions
pub struct ReplayRepository {
    backend: Arc<dyn KvBackend>,
    index_manager: Arc<IndexManager>,
}

impl ReplayRepository {
    /// Create a new replay repository
    pub fn new(backend: Arc<dyn KvBackend>, index_manager: Arc<IndexManager>) -> Self {
        Self {
            backend,
            index_manager,
        }
    }

    /// Store replay metadata
    pub async fn store_metadata(&self, meta: ReplayMetadataKv) -> Result<(), StorageError> {
        let key = KeyBuilder::replay_metadata(&meta.tenant_id, &meta.inference_id).build();
        let record = VersionedRecord::new(meta.clone());
        let bytes = record.serialize()?;
        self.backend.set(&key, bytes).await?;

        self.index_manager
            .add_to_index(replay_indexes::META_BY_TENANT, &meta.tenant_id, &key)
            .await?;

        self.index_manager
            .add_to_index(replay_indexes::META_BY_INFERENCE, &meta.inference_id, &key)
            .await?;

        self.index_manager
            .add_to_index(replay_indexes::META_BY_ID, &meta.id, &key)
            .await?;

        Ok(())
    }

    /// Fetch replay metadata by inference id
    pub async fn get_metadata_by_inference(
        &self,
        tenant_id: &str,
        inference_id: &str,
    ) -> Result<Option<ReplayMetadataKv>, StorageError> {
        let key = KeyBuilder::replay_metadata(tenant_id, inference_id).build();
        let Some(bytes) = self.backend.get(&key).await? else {
            return Ok(None);
        };

        let record = VersionedRecord::<ReplayMetadataKv>::deserialize_and_migrate(&bytes)?;
        if record.data.tenant_id != tenant_id {
            return Ok(None);
        }

        Ok(Some(record.data))
    }

    /// Fetch replay metadata by inference id without tenant hint (used for KV-primary lookup)
    pub async fn get_metadata_by_inference_any(
        &self,
        inference_id: &str,
    ) -> Result<Option<ReplayMetadataKv>, StorageError> {
        let keys = self
            .index_manager
            .query_index(replay_indexes::META_BY_INFERENCE, inference_id)
            .await?;

        if let Some(first_key) = keys.first() {
            if let Some(bytes) = self.backend.get(first_key).await? {
                let record = VersionedRecord::<ReplayMetadataKv>::deserialize_and_migrate(&bytes)?;
                return Ok(Some(record.data));
            }
        }

        Ok(None)
    }

    /// Fetch replay metadata by record id
    pub async fn get_metadata_by_id(
        &self,
        id: &str,
    ) -> Result<Option<ReplayMetadataKv>, StorageError> {
        let keys = self
            .index_manager
            .query_index(replay_indexes::META_BY_ID, id)
            .await?;

        if let Some(first_key) = keys.first() {
            if let Some(bytes) = self.backend.get(first_key).await? {
                let record = VersionedRecord::<ReplayMetadataKv>::deserialize_and_migrate(&bytes)?;
                return Ok(Some(record.data));
            }
        }

        Ok(None)
    }

    /// List replay metadata by tenant ordered by created_at DESC
    pub async fn list_metadata_by_tenant(
        &self,
        tenant_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<ReplayMetadataKv>, StorageError> {
        let keys = self
            .index_manager
            .query_index(replay_indexes::META_BY_TENANT, tenant_id)
            .await?;

        let mut items = Vec::new();
        for key in keys {
            if let Some(bytes) = self.backend.get(&key).await? {
                if let Ok(record) =
                    VersionedRecord::<ReplayMetadataKv>::deserialize_and_migrate(&bytes)
                {
                    items.push(record.data);
                }
            }
        }

        // Sort by created_at DESC (string ISO-8601)
        items.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let sliced = items.into_iter().skip(offset).take(limit).collect();
        Ok(sliced)
    }

    /// Store replay execution
    pub async fn store_execution(&self, exec: ReplayExecutionKv) -> Result<(), StorageError> {
        let key = KeyBuilder::replay_execution(&exec.tenant_id, &exec.id).build();
        let record = VersionedRecord::new(exec.clone());
        let bytes = record.serialize()?;
        self.backend.set(&key, bytes).await?;

        self.index_manager
            .add_to_index(replay_indexes::EXEC_BY_TENANT, &exec.tenant_id, &key)
            .await?;

        self.index_manager
            .add_to_index(
                replay_indexes::EXEC_BY_INFERENCE,
                &exec.original_inference_id,
                &key,
            )
            .await?;

        self.index_manager
            .add_to_index(replay_indexes::EXEC_BY_ID, &exec.id, &key)
            .await?;

        Ok(())
    }

    /// Update replay execution (preserves original timestamps)
    pub async fn update_execution(&self, exec: ReplayExecutionKv) -> Result<(), StorageError> {
        let key = KeyBuilder::replay_execution(&exec.tenant_id, &exec.id).build();

        let mut record = if let Some(bytes) = self.backend.get(&key).await? {
            VersionedRecord::<ReplayExecutionKv>::deserialize_and_migrate(&bytes)?
        } else {
            VersionedRecord::new(exec.clone())
        };

        record.update(exec);
        let bytes = record.serialize()?;
        self.backend.set(&key, bytes).await
    }

    /// Get replay execution by id
    pub async fn get_execution(
        &self,
        tenant_id: &str,
        execution_id: &str,
    ) -> Result<Option<ReplayExecutionKv>, StorageError> {
        let key = KeyBuilder::replay_execution(tenant_id, execution_id).build();
        let Some(bytes) = self.backend.get(&key).await? else {
            return Ok(None);
        };

        let record = VersionedRecord::<ReplayExecutionKv>::deserialize_and_migrate(&bytes)?;
        if record.data.tenant_id != tenant_id {
            return Ok(None);
        }

        Ok(Some(record.data))
    }

    /// Get replay execution by id without tenant hint
    pub async fn get_execution_by_id(
        &self,
        execution_id: &str,
    ) -> Result<Option<ReplayExecutionKv>, StorageError> {
        let keys = self
            .index_manager
            .query_index(replay_indexes::EXEC_BY_ID, execution_id)
            .await?;

        if let Some(first_key) = keys.first() {
            if let Some(bytes) = self.backend.get(first_key).await? {
                let record = VersionedRecord::<ReplayExecutionKv>::deserialize_and_migrate(&bytes)?;
                return Ok(Some(record.data));
            }
        }

        Ok(None)
    }

    /// List executions for an inference ordered by executed_at DESC
    pub async fn list_executions_for_inference(
        &self,
        inference_id: &str,
    ) -> Result<Vec<ReplayExecutionKv>, StorageError> {
        let keys = self
            .index_manager
            .query_index(replay_indexes::EXEC_BY_INFERENCE, inference_id)
            .await?;

        let mut items = Vec::new();
        for key in keys {
            if let Some(bytes) = self.backend.get(&key).await? {
                if let Ok(record) =
                    VersionedRecord::<ReplayExecutionKv>::deserialize_and_migrate(&bytes)
                {
                    items.push(record.data);
                }
            }
        }

        items.sort_by(|a, b| b.executed_at.cmp(&a.executed_at));
        Ok(items)
    }

    /// Store replay session
    pub async fn store_session(&self, session: ReplaySessionKv) -> Result<(), StorageError> {
        let key = KeyBuilder::replay_session(&session.tenant_id, &session.id).build();
        let record = VersionedRecord::new(session.clone());
        let bytes = record.serialize()?;
        self.backend.set(&key, bytes).await?;

        self.index_manager
            .add_to_index(replay_indexes::SESSIONS_BY_TENANT, &session.tenant_id, &key)
            .await?;

        self.index_manager
            .add_to_index(replay_indexes::SESSIONS_BY_ID, &session.id, &key)
            .await?;

        Ok(())
    }

    /// Get session by id
    pub async fn get_session(
        &self,
        tenant_id: &str,
        session_id: &str,
    ) -> Result<Option<ReplaySessionKv>, StorageError> {
        let key = KeyBuilder::replay_session(tenant_id, session_id).build();
        let Some(bytes) = self.backend.get(&key).await? else {
            return Ok(None);
        };

        let record = VersionedRecord::<ReplaySessionKv>::deserialize_and_migrate(&bytes)?;
        if record.data.tenant_id != tenant_id {
            return Ok(None);
        }
        Ok(Some(record.data))
    }

    /// Get session by id without tenant hint
    pub async fn get_session_by_id(
        &self,
        session_id: &str,
    ) -> Result<Option<ReplaySessionKv>, StorageError> {
        let keys = self
            .index_manager
            .query_index(replay_indexes::SESSIONS_BY_ID, session_id)
            .await?;

        if let Some(first_key) = keys.first() {
            if let Some(bytes) = self.backend.get(first_key).await? {
                let record =
                    VersionedRecord::<ReplaySessionKv>::deserialize_and_migrate(&bytes)?;
                return Ok(Some(record.data));
            }
        }

        Ok(None)
    }

    /// List replay sessions for a tenant ordered by snapshot_at DESC
    pub async fn list_sessions_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<ReplaySessionKv>, StorageError> {
        let keys = self
            .index_manager
            .query_index(replay_indexes::SESSIONS_BY_TENANT, tenant_id)
            .await?;

        let mut items = Vec::new();
        for key in keys {
            if let Some(bytes) = self.backend.get(&key).await? {
                if let Ok(record) =
                    VersionedRecord::<ReplaySessionKv>::deserialize_and_migrate(&bytes)
                {
                    items.push(record.data);
                }
            }
        }

        items.sort_by(|a, b| b.snapshot_at.cmp(&a.snapshot_at));
        Ok(items)
    }

    /// Delete a replay session by id
    pub async fn delete_session(&self, session_id: &str) -> Result<bool, StorageError> {
        if let Some(session) = self.get_session_by_id(session_id).await? {
            let key = KeyBuilder::replay_session(&session.tenant_id, &session.id).build();
            let deleted = self.backend.delete(&key).await.unwrap_or(false);

            self.index_manager
                .remove_from_index(replay_indexes::SESSIONS_BY_TENANT, &session.tenant_id, &key)
                .await
                .ok();

            self.index_manager
                .remove_from_index(replay_indexes::SESSIONS_BY_ID, &session.id, &key)
                .await
                .ok();

            return Ok(deleted);
        }

        Ok(false)
    }
}
