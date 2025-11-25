//! Adapter training snapshot tracking for deterministic provenance.
//!
//! Records the exact documents and chunking configuration used to train each adapter,
//! enabling reproducibility and audit trails.

use crate::{Db, Result};
use adapteros_core::AosError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Adapter training snapshot record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct AdapterTrainingSnapshot {
    pub id: String,
    pub adapter_id: String,
    pub training_job_id: String,
    pub collection_id: Option<String>,
    pub documents_json: String, // [{doc_id, doc_hash, version}]
    pub chunk_manifest_hash: String,
    pub chunking_config_json: String,
    pub created_at: String,
}

/// Parameters for creating a training snapshot
#[derive(Debug, Clone)]
pub struct CreateSnapshotParams {
    pub adapter_id: String,
    pub training_job_id: String,
    pub collection_id: Option<String>,
    pub documents_json: String,
    pub chunk_manifest_hash: String,
    pub chunking_config_json: String,
}

impl Db {
    /// Create adapter training snapshot
    ///
    /// Records the exact documents and chunking configuration used to train an adapter.
    /// This creates an immutable audit trail for reproducibility.
    ///
    /// # Arguments
    /// * `params` - Snapshot parameters including documents and chunking config
    ///
    /// # Returns
    /// The unique ID of the created snapshot record
    pub async fn create_training_snapshot(&self, params: CreateSnapshotParams) -> Result<String> {
        let id = Uuid::new_v4().to_string();

        sqlx::query(
            r#"
            INSERT INTO adapter_training_snapshots (
                id, adapter_id, training_job_id, collection_id, documents_json,
                chunk_manifest_hash, chunking_config_json, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(&params.adapter_id)
        .bind(&params.training_job_id)
        .bind(&params.collection_id)
        .bind(&params.documents_json)
        .bind(&params.chunk_manifest_hash)
        .bind(&params.chunking_config_json)
        .execute(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to create training snapshot: {}", e))
        })?;

        Ok(id)
    }

    /// Get adapter training snapshot
    ///
    /// Retrieves the training snapshot for a specific adapter, showing exactly
    /// which documents and configuration were used during training.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to look up
    ///
    /// # Returns
    /// The training snapshot if found, None otherwise
    pub async fn get_adapter_training_snapshot(
        &self,
        adapter_id: &str,
    ) -> Result<Option<AdapterTrainingSnapshot>> {
        let record = sqlx::query_as::<_, AdapterTrainingSnapshotRow>(
            r#"
            SELECT id, adapter_id, training_job_id, collection_id, documents_json,
                   chunk_manifest_hash, chunking_config_json, created_at
            FROM adapter_training_snapshots
            WHERE adapter_id = ?
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(adapter_id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch training snapshot: {}", e)))?;

        Ok(record.map(Into::into))
    }

    /// Get all training snapshots for a training job
    ///
    /// Retrieves all adapter snapshots associated with a specific training job.
    /// Useful for tracking which adapters were trained from the same dataset.
    pub async fn get_snapshots_by_training_job(
        &self,
        training_job_id: &str,
    ) -> Result<Vec<AdapterTrainingSnapshot>> {
        let records = sqlx::query_as::<_, AdapterTrainingSnapshotRow>(
            r#"
            SELECT id, adapter_id, training_job_id, collection_id, documents_json,
                   chunk_manifest_hash, chunking_config_json, created_at
            FROM adapter_training_snapshots
            WHERE training_job_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(training_job_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to fetch training job snapshots: {}", e))
        })?;

        Ok(records.into_iter().map(Into::into).collect())
    }

    /// Get all training snapshots for a collection
    ///
    /// Retrieves all adapter snapshots that used documents from a specific collection.
    /// Useful for tracking downstream effects of document collection changes.
    pub async fn get_snapshots_by_collection(
        &self,
        collection_id: &str,
    ) -> Result<Vec<AdapterTrainingSnapshot>> {
        let records = sqlx::query_as::<_, AdapterTrainingSnapshotRow>(
            r#"
            SELECT id, adapter_id, training_job_id, collection_id, documents_json,
                   chunk_manifest_hash, chunking_config_json, created_at
            FROM adapter_training_snapshots
            WHERE collection_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(collection_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to fetch collection snapshots: {}", e))
        })?;

        Ok(records.into_iter().map(Into::into).collect())
    }
}

/// Internal row type for SQLx query mapping
#[derive(sqlx::FromRow)]
struct AdapterTrainingSnapshotRow {
    id: String,
    adapter_id: String,
    training_job_id: String,
    collection_id: Option<String>,
    documents_json: String,
    chunk_manifest_hash: String,
    chunking_config_json: String,
    created_at: String,
}

impl From<AdapterTrainingSnapshotRow> for AdapterTrainingSnapshot {
    fn from(row: AdapterTrainingSnapshotRow) -> Self {
        Self {
            id: row.id,
            adapter_id: row.adapter_id,
            training_job_id: row.training_job_id,
            collection_id: row.collection_id,
            documents_json: row.documents_json,
            chunk_manifest_hash: row.chunk_manifest_hash,
            chunking_config_json: row.chunking_config_json,
            created_at: row.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_retrieve_snapshot() {
        let db = Db::new_in_memory().await.unwrap();

        let adapter_id = "adapter-001";
        let training_job_id = "job-001";

        let documents_json = serde_json::json!([
            {"doc_id": "doc1", "doc_hash": "hash1", "version": 1},
            {"doc_id": "doc2", "doc_hash": "hash2", "version": 1}
        ])
        .to_string();

        let chunking_config_json = serde_json::json!({
            "chunk_size": 512,
            "overlap": 50,
            "strategy": "semantic"
        })
        .to_string();

        let params = CreateSnapshotParams {
            adapter_id: adapter_id.to_string(),
            training_job_id: training_job_id.to_string(),
            collection_id: Some("collection-001".to_string()),
            documents_json,
            chunk_manifest_hash: "manifest_hash_123".to_string(),
            chunking_config_json,
        };

        let id = db.create_training_snapshot(params).await.unwrap();
        assert!(!id.is_empty());

        // Retrieve snapshot
        let snapshot = db
            .get_adapter_training_snapshot(adapter_id)
            .await
            .unwrap();
        assert!(snapshot.is_some());

        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.adapter_id, adapter_id);
        assert_eq!(snapshot.training_job_id, training_job_id);
        assert_eq!(snapshot.chunk_manifest_hash, "manifest_hash_123");

        // Verify JSON fields can be parsed
        let docs: serde_json::Value =
            serde_json::from_str(&snapshot.documents_json).unwrap();
        assert!(docs.is_array());
        assert_eq!(docs.as_array().unwrap().len(), 2);

        let config: serde_json::Value =
            serde_json::from_str(&snapshot.chunking_config_json).unwrap();
        assert_eq!(config["chunk_size"], 512);
    }

    #[tokio::test]
    async fn test_get_snapshots_by_training_job() {
        let db = Db::new_in_memory().await.unwrap();

        let training_job_id = "job-002";

        // Create multiple snapshots for the same training job
        for i in 1..=3 {
            let params = CreateSnapshotParams {
                adapter_id: format!("adapter-{}", i),
                training_job_id: training_job_id.to_string(),
                collection_id: None,
                documents_json: "[]".to_string(),
                chunk_manifest_hash: format!("hash-{}", i),
                chunking_config_json: "{}".to_string(),
            };

            db.create_training_snapshot(params).await.unwrap();
        }

        // Retrieve all snapshots for the training job
        let snapshots = db
            .get_snapshots_by_training_job(training_job_id)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 3);

        // Verify all have the same training job ID
        for snapshot in &snapshots {
            assert_eq!(snapshot.training_job_id, training_job_id);
        }
    }

    #[tokio::test]
    async fn test_get_snapshots_by_collection() {
        let db = Db::new_in_memory().await.unwrap();

        let collection_id = "collection-003";

        // Create multiple snapshots for the same collection
        for i in 1..=2 {
            let params = CreateSnapshotParams {
                adapter_id: format!("adapter-{}", i),
                training_job_id: format!("job-{}", i),
                collection_id: Some(collection_id.to_string()),
                documents_json: "[]".to_string(),
                chunk_manifest_hash: format!("hash-{}", i),
                chunking_config_json: "{}".to_string(),
            };

            db.create_training_snapshot(params).await.unwrap();
        }

        // Retrieve all snapshots for the collection
        let snapshots = db
            .get_snapshots_by_collection(collection_id)
            .await
            .unwrap();
        assert_eq!(snapshots.len(), 2);

        // Verify all have the same collection ID
        for snapshot in &snapshots {
            assert_eq!(snapshot.collection_id.as_deref(), Some(collection_id));
        }
    }
}
