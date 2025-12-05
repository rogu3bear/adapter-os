//! Inference replay metadata tracking for deterministic provenance.
//!
//! Records the replay key and content for each inference operation,
//! enabling exact reproduction of model outputs under identical conditions.

use crate::replay_kv::record_replay_drift;
use crate::{Db, Result};
use adapteros_core::AosError;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

/// Inference replay metadata record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct InferenceReplayMetadata {
    pub id: String,
    pub inference_id: String,
    pub tenant_id: String,
    /// BLAKE3 hash of manifest (model + adapters)
    pub manifest_hash: String,
    /// Seed for router selection (null if no routing)
    pub router_seed: Option<String>,
    /// JSON: {temperature, top_k, top_p, max_tokens, seed}
    pub sampling_params_json: String,
    /// Backend used: CoreML, MLX, Metal
    pub backend: String,
    /// Sampling algorithm version for compatibility tracking
    pub sampling_algorithm_version: String,
    /// BLAKE3 hash of sorted document hashes (null if no RAG)
    pub rag_snapshot_hash: Option<String>,
    /// JSON array of adapter IDs used (null if none)
    pub adapter_ids_json: Option<String>,
    /// Original prompt text (may be truncated)
    pub prompt_text: String,
    /// Whether prompt was truncated (0 = no, 1 = yes)
    pub prompt_truncated: i32,
    /// Response text (may be truncated)
    pub response_text: Option<String>,
    /// Whether response was truncated (0 = no, 1 = yes)
    pub response_truncated: i32,
    /// JSON array of document IDs used for RAG (null if no RAG)
    pub rag_doc_ids_json: Option<String>,
    /// BLAKE3 hash of sorted message IDs for multi-turn context (null if single-turn)
    pub chat_context_hash: Option<String>,
    /// Replay availability status
    pub replay_status: String,
    /// Inference latency in milliseconds
    pub latency_ms: Option<i32>,
    /// Number of tokens generated
    pub tokens_generated: Option<i32>,
    /// Determinism mode applied for this inference (strict, besteffort, relaxed)
    pub determinism_mode: Option<String>,
    /// Whether backend fallback occurred during execution
    pub fallback_triggered: Option<bool>,
    /// Replay guarantee level (exact, approximate, none)
    pub replay_guarantee: Option<String>,
    /// Execution policy ID applied (if any)
    pub execution_policy_id: Option<String>,
    /// Execution policy version applied (if any)
    pub execution_policy_version: Option<i32>,
    pub created_at: String,
}

/// Parameters for creating replay metadata
#[derive(Debug, Clone)]
pub struct CreateReplayMetadataParams {
    pub inference_id: String,
    pub tenant_id: String,
    pub manifest_hash: String,
    pub router_seed: Option<String>,
    pub sampling_params_json: String,
    pub backend: String,
    pub sampling_algorithm_version: Option<String>,
    pub rag_snapshot_hash: Option<String>,
    /// JSON-serializable list of adapter IDs
    pub adapter_ids: Option<Vec<String>>,
    pub prompt_text: String,
    pub prompt_truncated: bool,
    pub response_text: Option<String>,
    pub response_truncated: bool,
    /// JSON-serializable list of document IDs used for RAG
    pub rag_doc_ids: Option<Vec<String>>,
    /// BLAKE3 hash of sorted message IDs for multi-turn context verification
    pub chat_context_hash: Option<String>,
    pub replay_status: Option<String>,
    pub latency_ms: Option<i32>,
    pub tokens_generated: Option<i32>,
    pub determinism_mode: Option<String>,
    pub fallback_triggered: bool,
    pub replay_guarantee: Option<String>,
    pub execution_policy_id: Option<String>,
    pub execution_policy_version: Option<i32>,
}

impl Db {
    /// Create inference replay metadata record
    ///
    /// Records the replay key and content for an inference operation.
    /// This creates an immutable record enabling exact reproduction.
    ///
    /// # Arguments
    /// * `params` - Replay metadata parameters including manifest hash, sampling params, etc.
    ///
    /// # Returns
    /// The unique ID of the created replay metadata record
    pub async fn create_replay_metadata(
        &self,
        params: CreateReplayMetadataParams,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();

        // Serialize adapter IDs and RAG doc IDs to JSON
        let adapter_ids_json = params
            .adapter_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids).unwrap_or_default());
        let rag_doc_ids_json = params
            .rag_doc_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids).unwrap_or_default());

        let sampling_algorithm_version = params
            .sampling_algorithm_version
            .clone()
            .unwrap_or_else(|| "v1.0.0".to_string());
        let replay_status = params
            .replay_status
            .clone()
            .unwrap_or_else(|| "available".to_string());
        let prompt_truncated = if params.prompt_truncated { 1 } else { 0 };
        let response_truncated = if params.response_truncated { 1 } else { 0 };
        let fallback_triggered = if params.fallback_triggered { 1 } else { 0 };

        sqlx::query(
            r#"
            INSERT INTO inference_replay_metadata (
                id, inference_id, tenant_id, manifest_hash, router_seed,
                sampling_params_json, backend, sampling_algorithm_version,
                rag_snapshot_hash, adapter_ids_json, prompt_text, prompt_truncated,
                response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                replay_status, latency_ms, tokens_generated, determinism_mode,
                fallback_triggered, replay_guarantee, execution_policy_id,
                execution_policy_version, created_at
            )
            VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                datetime('now')
            )
            "#,
        )
        .bind(&id)
        .bind(&params.inference_id)
        .bind(&params.tenant_id)
        .bind(&params.manifest_hash)
        .bind(&params.router_seed)
        .bind(&params.sampling_params_json)
        .bind(&params.backend)
        .bind(&sampling_algorithm_version)
        .bind(&params.rag_snapshot_hash)
        .bind(&adapter_ids_json)
        .bind(&params.prompt_text)
        .bind(prompt_truncated)
        .bind(&params.response_text)
        .bind(response_truncated)
        .bind(&rag_doc_ids_json)
        .bind(&params.chat_context_hash)
        .bind(&replay_status)
        .bind(&params.latency_ms)
        .bind(&params.tokens_generated)
        .bind(&params.determinism_mode)
        .bind(fallback_triggered)
        .bind(&params.replay_guarantee)
        .bind(&params.execution_policy_id)
        .bind(&params.execution_policy_version)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create replay metadata: {}", e)))?;

        // Dual-write to KV when enabled
        if let Some(repo) = self.replay_repo_if_write() {
            let kv_meta = self.kv_replay_metadata_from_params(&id, &params);
            match repo.store_metadata(kv_meta.clone()).await {
                Ok(_) => {
                    if let Ok(Some(fetched)) = repo
                        .get_metadata_by_inference_any(&kv_meta.inference_id)
                        .await
                    {
                        if fetched.manifest_hash != kv_meta.manifest_hash
                            || fetched.sampling_params_json != kv_meta.sampling_params_json
                        {
                            record_replay_drift("replay_metadata_mismatch");
                        }
                    }
                }
                Err(e) => {
                    warn!(
                        tenant_id = %params.tenant_id,
                        inference_id = %params.inference_id,
                        error = %e,
                        "Failed to dual-write replay metadata to KV"
                    );
                }
            }
        }

        Ok(id)
    }

    /// Get replay metadata by inference ID
    ///
    /// Retrieves the replay metadata for a specific inference operation.
    /// Returns None if no metadata exists for the given inference ID.
    pub async fn get_replay_metadata_by_inference(
        &self,
        inference_id: &str,
    ) -> Result<Option<InferenceReplayMetadata>> {
        if let Some(repo) = self.replay_repo_if_read() {
            match repo.get_metadata_by_inference_any(inference_id).await {
                Ok(Some(meta)) => return self.kv_replay_metadata_to_record(meta).map(Some),
                Ok(None) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Ok(None);
                    }
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for replay metadata: {}",
                            e
                        )));
                    }
                    warn!(
                        inference_id = %inference_id,
                        error = %e,
                        "KV replay metadata read failed, falling back to SQL"
                    );
                }
            }
        }

        let record = sqlx::query_as::<_, InferenceReplayMetadataRow>(
            r#"
            SELECT id, inference_id, tenant_id, manifest_hash, router_seed,
                   sampling_params_json, backend, sampling_algorithm_version,
                   rag_snapshot_hash, adapter_ids_json, prompt_text, prompt_truncated,
                   response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                   replay_status, latency_ms, tokens_generated, determinism_mode,
                   fallback_triggered, replay_guarantee, execution_policy_id,
                   execution_policy_version, created_at
            FROM inference_replay_metadata
            WHERE inference_id = ?
            "#,
        )
        .bind(inference_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch replay metadata: {}", e)))?;

        Ok(record.map(Into::into))
    }

    /// Get replay metadata by ID
    ///
    /// Retrieves replay metadata by its unique ID.
    /// Returns None if no metadata exists with the given ID.
    pub async fn get_replay_metadata(&self, id: &str) -> Result<Option<InferenceReplayMetadata>> {
        if let Some(repo) = self.replay_repo_if_read() {
            match repo.get_metadata_by_id(id).await {
                Ok(Some(meta)) => return self.kv_replay_metadata_to_record(meta).map(Some),
                Ok(None) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Ok(None);
                    }
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for replay metadata: {}",
                            e
                        )));
                    }
                    warn!(metadata_id = %id, error = %e, "KV replay metadata read failed, falling back to SQL");
                }
            }
        }

        let record = sqlx::query_as::<_, InferenceReplayMetadataRow>(
            r#"
            SELECT id, inference_id, tenant_id, manifest_hash, router_seed,
                   sampling_params_json, backend, sampling_algorithm_version,
                   rag_snapshot_hash, adapter_ids_json, prompt_text, prompt_truncated,
                   response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                   replay_status, latency_ms, tokens_generated, determinism_mode,
                   fallback_triggered, replay_guarantee, execution_policy_id,
                   execution_policy_version, created_at
            FROM inference_replay_metadata
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch replay metadata: {}", e)))?;

        Ok(record.map(Into::into))
    }

    /// Update replay status
    ///
    /// Updates the replay status for an inference operation.
    /// Status values: available, approximate, degraded, unavailable
    pub async fn update_replay_status(&self, inference_id: &str, status: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE inference_replay_metadata
            SET replay_status = ?
            WHERE inference_id = ?
            "#,
        )
        .bind(status)
        .bind(inference_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update replay status: {}", e)))?;

        Ok(())
    }

    /// List replay metadata by tenant
    ///
    /// Retrieves replay metadata for a tenant with pagination.
    /// Results are ordered by creation time (newest first).
    pub async fn list_replay_metadata_by_tenant(
        &self,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<InferenceReplayMetadata>> {
        if let Some(repo) = self.replay_repo_if_read() {
            match repo
                .list_metadata_by_tenant(tenant_id, limit.max(0) as usize, offset.max(0) as usize)
                .await
            {
                Ok(records) => {
                    let mut mapped = Vec::new();
                    for meta in records {
                        mapped.push(self.kv_replay_metadata_to_record(meta)?);
                    }
                    return Ok(mapped);
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for replay metadata by tenant: {}",
                            e
                        )));
                    }
                    warn!(
                        tenant_id = %tenant_id,
                        error = %e,
                        "KV replay metadata list failed, falling back to SQL"
                    );
                }
            }
        }

        let records = sqlx::query_as::<_, InferenceReplayMetadataRow>(
            r#"
            SELECT id, inference_id, tenant_id, manifest_hash, router_seed,
                   sampling_params_json, backend, sampling_algorithm_version,
                   rag_snapshot_hash, adapter_ids_json, prompt_text, prompt_truncated,
                   response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                   replay_status, latency_ms, tokens_generated, determinism_mode,
                   fallback_triggered, replay_guarantee, execution_policy_id,
                   execution_policy_version, created_at
            FROM inference_replay_metadata
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list replay metadata: {}", e)))?;

        Ok(records.into_iter().map(Into::into).collect())
    }
}

/// Internal row type for SQLx query mapping
#[derive(sqlx::FromRow)]
struct InferenceReplayMetadataRow {
    id: String,
    inference_id: String,
    tenant_id: String,
    manifest_hash: String,
    router_seed: Option<String>,
    sampling_params_json: String,
    backend: String,
    sampling_algorithm_version: String,
    rag_snapshot_hash: Option<String>,
    adapter_ids_json: Option<String>,
    prompt_text: String,
    prompt_truncated: i32,
    response_text: Option<String>,
    response_truncated: i32,
    rag_doc_ids_json: Option<String>,
    chat_context_hash: Option<String>,
    replay_status: String,
    latency_ms: Option<i32>,
    tokens_generated: Option<i32>,
    determinism_mode: Option<String>,
    fallback_triggered: Option<i32>,
    replay_guarantee: Option<String>,
    execution_policy_id: Option<String>,
    execution_policy_version: Option<i32>,
    created_at: String,
}

impl From<InferenceReplayMetadataRow> for InferenceReplayMetadata {
    fn from(row: InferenceReplayMetadataRow) -> Self {
        Self {
            id: row.id,
            inference_id: row.inference_id,
            tenant_id: row.tenant_id,
            manifest_hash: row.manifest_hash,
            router_seed: row.router_seed,
            sampling_params_json: row.sampling_params_json,
            backend: row.backend,
            sampling_algorithm_version: row.sampling_algorithm_version,
            rag_snapshot_hash: row.rag_snapshot_hash,
            adapter_ids_json: row.adapter_ids_json,
            prompt_text: row.prompt_text,
            prompt_truncated: row.prompt_truncated,
            response_text: row.response_text,
            response_truncated: row.response_truncated,
            rag_doc_ids_json: row.rag_doc_ids_json,
            chat_context_hash: row.chat_context_hash,
            replay_status: row.replay_status,
            latency_ms: row.latency_ms,
            tokens_generated: row.tokens_generated,
            determinism_mode: row.determinism_mode,
            fallback_triggered: row.fallback_triggered.map(|v| v != 0),
            replay_guarantee: row.replay_guarantee,
            execution_policy_id: row.execution_policy_id,
            execution_policy_version: row.execution_policy_version,
            created_at: row.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create tenant for FK constraints
    async fn setup_test_tenant(db: &Db) -> String {
        match db.create_tenant("Test Tenant", false).await {
            Ok(id) => id,
            Err(_) => {
                // Tenant already exists, just use a simple query to get one
                sqlx::query_scalar::<_, String>("SELECT id FROM tenants LIMIT 1")
                    .fetch_one(db.pool())
                    .await
                    .expect("No tenant found")
            }
        }
    }

    #[tokio::test]
    async fn test_create_and_retrieve_replay_metadata() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = setup_test_tenant(&db).await;

        let inference_id = "inf-replay-001";

        // Create replay metadata
        let params = CreateReplayMetadataParams {
            inference_id: inference_id.to_string(),
            tenant_id: tenant_id.clone(),
            manifest_hash: "manifest-hash-123".to_string(),
            router_seed: Some("seed-456".to_string()),
            sampling_params_json: r#"{"temperature":0.7,"top_k":50,"seed":42}"#.to_string(),
            backend: "CoreML".to_string(),
            sampling_algorithm_version: Some("v1.0.0".to_string()),
            rag_snapshot_hash: Some("rag-hash-789".to_string()),
            adapter_ids: Some(vec!["adapter-1".to_string(), "adapter-2".to_string()]),
            prompt_text: "Test prompt".to_string(),
            prompt_truncated: false,
            response_text: Some("Test response".to_string()),
            response_truncated: false,
            rag_doc_ids: Some(vec!["doc-1".to_string(), "doc-2".to_string()]),
            chat_context_hash: Some("chat-context-hash-abc".to_string()),
            replay_status: Some("available".to_string()),
            latency_ms: Some(150),
            tokens_generated: Some(25),
            determinism_mode: Some("strict".to_string()),
            fallback_triggered: false,
            replay_guarantee: Some("exact".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
        };

        let id = db.create_replay_metadata(params).await.unwrap();
        assert!(!id.is_empty());

        // Retrieve by inference ID
        let metadata = db
            .get_replay_metadata_by_inference(inference_id)
            .await
            .unwrap();
        assert!(metadata.is_some());

        let metadata = metadata.unwrap();
        assert_eq!(metadata.inference_id, inference_id);
        assert_eq!(metadata.tenant_id, tenant_id);
        assert_eq!(metadata.manifest_hash, "manifest-hash-123");
        assert_eq!(metadata.backend, "CoreML");
        assert_eq!(metadata.replay_status, "available");
        assert_eq!(metadata.latency_ms, Some(150));
        assert_eq!(metadata.tokens_generated, Some(25));
        assert_eq!(metadata.prompt_truncated, 0);
        assert_eq!(metadata.response_truncated, 0);

        // Verify JSON fields
        let adapter_ids: Vec<String> =
            serde_json::from_str(&metadata.adapter_ids_json.unwrap()).unwrap();
        assert_eq!(adapter_ids, vec!["adapter-1", "adapter-2"]);

        let rag_doc_ids: Vec<String> =
            serde_json::from_str(&metadata.rag_doc_ids_json.unwrap()).unwrap();
        assert_eq!(rag_doc_ids, vec!["doc-1", "doc-2"]);

        // Retrieve by ID
        let metadata_by_id = db.get_replay_metadata(&id).await.unwrap();
        assert!(metadata_by_id.is_some());
        assert_eq!(metadata_by_id.unwrap().inference_id, inference_id);
    }

    #[tokio::test]
    async fn test_update_replay_status() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = setup_test_tenant(&db).await;

        let inference_id = "inf-replay-002";

        // Create replay metadata
        let params = CreateReplayMetadataParams {
            inference_id: inference_id.to_string(),
            tenant_id: tenant_id.clone(),
            manifest_hash: "hash-001".to_string(),
            router_seed: None,
            sampling_params_json: r#"{"temperature":0.7}"#.to_string(),
            backend: "MLX".to_string(),
            sampling_algorithm_version: None,
            rag_snapshot_hash: None,
            adapter_ids: None,
            prompt_text: "Prompt".to_string(),
            prompt_truncated: false,
            response_text: None,
            response_truncated: false,
            rag_doc_ids: None,
            chat_context_hash: None,
            replay_status: Some("available".to_string()),
            latency_ms: None,
            tokens_generated: None,
            determinism_mode: Some("strict".to_string()),
            fallback_triggered: false,
            replay_guarantee: Some("exact".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
        };

        db.create_replay_metadata(params).await.unwrap();

        // Update status
        db.update_replay_status(inference_id, "degraded")
            .await
            .unwrap();

        // Verify update
        let metadata = db
            .get_replay_metadata_by_inference(inference_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(metadata.replay_status, "degraded");
    }

    #[tokio::test]
    async fn test_list_replay_metadata_by_tenant() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = setup_test_tenant(&db).await;

        // Create multiple replay metadata records
        for i in 1..=5 {
            let params = CreateReplayMetadataParams {
                inference_id: format!("inf-{}", i),
                tenant_id: tenant_id.clone(),
                manifest_hash: format!("hash-{}", i),
                router_seed: None,
                sampling_params_json: r#"{"temperature":0.7}"#.to_string(),
                backend: "Metal".to_string(),
                sampling_algorithm_version: None,
                rag_snapshot_hash: None,
                adapter_ids: None,
                prompt_text: format!("Prompt {}", i),
                prompt_truncated: false,
                response_text: Some(format!("Response {}", i)),
                response_truncated: false,
                rag_doc_ids: None,
                chat_context_hash: None,
                replay_status: None,
                latency_ms: Some(100 + i),
                tokens_generated: Some(20 + i),
                determinism_mode: Some("strict".to_string()),
                fallback_triggered: false,
                replay_guarantee: Some("exact".to_string()),
                execution_policy_id: None,
                execution_policy_version: None,
            };

            db.create_replay_metadata(params).await.unwrap();
        }

        // List with pagination
        let page1 = db
            .list_replay_metadata_by_tenant(&tenant_id, 3, 0)
            .await
            .unwrap();
        assert_eq!(page1.len(), 3);

        let page2 = db
            .list_replay_metadata_by_tenant(&tenant_id, 3, 3)
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);

        // Verify ordering (newest first)
        assert_eq!(page1[0].inference_id, "inf-5");
        assert_eq!(page1[1].inference_id, "inf-4");
        assert_eq!(page1[2].inference_id, "inf-3");
    }

    #[tokio::test]
    async fn test_truncation_flags() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = setup_test_tenant(&db).await;

        let inference_id = "inf-truncated";

        // Create with truncation flags set
        let params = CreateReplayMetadataParams {
            inference_id: inference_id.to_string(),
            tenant_id: tenant_id.clone(),
            manifest_hash: "hash".to_string(),
            router_seed: None,
            sampling_params_json: r#"{}"#.to_string(),
            backend: "CoreML".to_string(),
            sampling_algorithm_version: None,
            rag_snapshot_hash: None,
            adapter_ids: None,
            prompt_text: "Very long prompt...".to_string(),
            prompt_truncated: true,
            response_text: Some("Very long response...".to_string()),
            response_truncated: true,
            rag_doc_ids: None,
            chat_context_hash: None,
            replay_status: None,
            latency_ms: None,
            tokens_generated: None,
            determinism_mode: None,
            fallback_triggered: false,
            replay_guarantee: None,
            execution_policy_id: None,
            execution_policy_version: None,
        };

        db.create_replay_metadata(params).await.unwrap();

        // Verify truncation flags
        let metadata = db
            .get_replay_metadata_by_inference(inference_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(metadata.prompt_truncated, 1);
        assert_eq!(metadata.response_truncated, 1);
    }

    #[tokio::test]
    async fn test_nonexistent_replay_metadata() {
        let db = Db::new_in_memory().await.unwrap();

        // Try to retrieve non-existent metadata
        let result = db
            .get_replay_metadata_by_inference("nonexistent")
            .await
            .unwrap();
        assert!(result.is_none());

        let result = db.get_replay_metadata("nonexistent-id").await.unwrap();
        assert!(result.is_none());
    }
}
