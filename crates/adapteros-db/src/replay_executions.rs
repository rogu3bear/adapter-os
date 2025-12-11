//! Replay execution audit trail for deterministic replay feature.
//!
//! Records each replay attempt with match analysis and divergence details.
//! Multiple executions can exist per original inference for different replay modes.

use crate::replay_kv::record_replay_drift;
use crate::{Db, Result};
use adapteros_core::AosError;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

/// Replay execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ReplayExecution {
    pub id: String,
    pub original_inference_id: String,
    pub tenant_id: String,
    pub replay_mode: String,
    /// Prompt text snapshot at execution time
    pub prompt_text: String,
    /// JSON-encoded sampling parameters
    pub sampling_params_json: String,
    /// Backend used for replay (CoreML, MLX, Metal)
    pub backend: String,
    /// Model manifest hash
    pub manifest_hash: String,
    /// Router seed (if deterministic routing was used)
    pub router_seed: Option<String>,
    /// JSON array of adapter IDs used during replay
    pub adapter_ids_json: Option<String>,
    /// Generated response text
    pub response_text: Option<String>,
    /// Whether response was truncated
    pub response_truncated: i32,
    /// Number of tokens generated
    pub tokens_generated: Option<i32>,
    /// Latency in milliseconds
    pub latency_ms: Option<i32>,
    /// Match status: exact, semantic, divergent, error
    pub match_status: String,
    /// JSON-encoded divergence details: {position, backend_changed, manifest_changed, reasons}
    pub divergence_details_json: Option<String>,
    /// RAG reproducibility score (0.0-1.0, null if no RAG)
    pub rag_reproducibility_score: Option<f64>,
    /// JSON array of document IDs that were unavailable during replay
    pub missing_doc_ids_json: Option<String>,
    /// Timestamp of execution
    pub executed_at: String,
    /// User ID who triggered replay
    pub executed_by: Option<String>,
    /// Error details if match_status = 'error'
    pub error_message: Option<String>,
}

/// Parameters for creating a replay execution record
#[derive(Debug, Clone)]
pub struct CreateReplayExecutionParams {
    pub original_inference_id: String,
    pub tenant_id: String,
    pub replay_mode: String,
    pub prompt_text: String,
    pub sampling_params_json: String,
    pub backend: String,
    pub manifest_hash: String,
    pub router_seed: Option<String>,
    pub adapter_ids: Option<Vec<String>>,
    pub executed_by: Option<String>,
}

/// Parameters for updating replay execution results
#[derive(Debug, Clone)]
pub struct UpdateReplayExecutionParams {
    pub response_text: Option<String>,
    pub response_truncated: bool,
    pub tokens_generated: Option<i32>,
    pub latency_ms: Option<i32>,
    pub match_status: String,
    pub divergence_details: Option<serde_json::Value>,
    pub rag_reproducibility_score: Option<f64>,
    pub missing_doc_ids: Option<Vec<String>>,
    pub error_message: Option<String>,
}

impl Db {
    /// Create replay execution record
    ///
    /// Records the initial state of a replay attempt, capturing all inputs and configuration.
    /// Call `update_replay_execution_result` after execution completes to store results.
    ///
    /// # Arguments
    /// * `params` - Replay execution parameters including mode, prompt, and configuration
    ///
    /// # Returns
    /// The unique ID of the created replay execution record
    pub async fn create_replay_execution(
        &self,
        params: CreateReplayExecutionParams,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();

        // Serialize adapter IDs to JSON
        let adapter_ids_json = params
            .adapter_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids).unwrap_or_default());

        sqlx::query(
            r#"
            INSERT INTO replay_executions (
                id, original_inference_id, tenant_id, replay_mode,
                prompt_text, sampling_params_json, backend, manifest_hash,
                router_seed, adapter_ids_json, match_status, executed_by
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?)
            "#,
        )
        .bind(&id)
        .bind(&params.original_inference_id)
        .bind(&params.tenant_id)
        .bind(&params.replay_mode)
        .bind(&params.prompt_text)
        .bind(&params.sampling_params_json)
        .bind(&params.backend)
        .bind(&params.manifest_hash)
        .bind(&params.router_seed)
        .bind(&adapter_ids_json)
        .bind(&params.executed_by)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to create replay execution: {}", e)))?;

        if let Some(repo) = self.replay_repo_if_write() {
            let kv_exec = self.kv_replay_execution_from_create(&id, &params);
            if let Err(e) = repo.store_execution(kv_exec).await {
                self.record_kv_write_fallback("replay.execution.create");
                warn!(
                    tenant_id = %params.tenant_id,
                    inference_id = %params.original_inference_id,
                    error = %e,
                    "Failed to dual-write replay execution to KV"
                );
                record_replay_drift("replay_execution_dual_write_failed");
            }
        }

        Ok(id)
    }

    /// Update replay execution results
    ///
    /// Updates a replay execution record with results after execution completes.
    /// This includes the generated response, match analysis, and any divergence details.
    ///
    /// # Arguments
    /// * `id` - The replay execution ID to update
    /// * `params` - Execution results including response text, match status, and metrics
    pub async fn update_replay_execution_result(
        &self,
        id: &str,
        params: UpdateReplayExecutionParams,
    ) -> Result<()> {
        // Serialize JSON fields
        let divergence_details_json = params
            .divergence_details
            .as_ref()
            .map(|details| serde_json::to_string(details).unwrap_or_default());
        let missing_doc_ids_json = params
            .missing_doc_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids).unwrap_or_default());

        sqlx::query(
            r#"
            UPDATE replay_executions
            SET response_text = ?,
                response_truncated = ?,
                tokens_generated = ?,
                latency_ms = ?,
                match_status = ?,
                divergence_details_json = ?,
                rag_reproducibility_score = ?,
                missing_doc_ids_json = ?,
                error_message = ?
            WHERE id = ?
            "#,
        )
        .bind(&params.response_text)
        .bind(if params.response_truncated { 1 } else { 0 })
        .bind(params.tokens_generated)
        .bind(params.latency_ms)
        .bind(&params.match_status)
        .bind(&divergence_details_json)
        .bind(params.rag_reproducibility_score)
        .bind(&missing_doc_ids_json)
        .bind(&params.error_message)
        .bind(id)
        .execute(&*self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to update replay execution result: {}", e))
        })?;

        if let Some(repo) = self.replay_repo_if_write() {
            // Lookup tenant to enforce isolation
            let tenant_id: Option<String> =
                sqlx::query_scalar("SELECT tenant_id FROM replay_executions WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&*self.pool())
                    .await
                    .map_err(|e| {
                        AosError::Database(format!("Failed to lookup execution tenant: {}", e))
                    })?;

            if let Some(tid) = tenant_id {
                match repo.get_execution(&tid, id).await {
                    Ok(Some(mut kv_exec)) => {
                        self.kv_replay_execution_apply_update(&mut kv_exec, &params)?;
                        if let Err(e) = repo.update_execution(kv_exec).await {
                            self.record_kv_write_fallback("replay.execution.update");
                            warn!(
                                tenant_id = %tid,
                                execution_id = %id,
                                error = %e,
                                "Failed to dual-write replay execution update to KV"
                            );
                        }
                    }
                    Ok(None) => {
                        warn!(
                            tenant_id = %tid,
                            execution_id = %id,
                            "KV replay execution missing during update"
                        );
                        record_replay_drift("replay_execution_missing_on_update");
                    }
                    Err(e) => {
                        warn!(
                            tenant_id = %tid,
                            execution_id = %id,
                            error = %e,
                            "KV replay execution fetch failed during update"
                        );
                        record_replay_drift("replay_execution_fetch_failed");
                    }
                }
            }
        }

        Ok(())
    }

    /// Get a single replay execution by ID
    ///
    /// Retrieves a specific replay execution record with all its details.
    pub async fn get_replay_execution(&self, id: &str) -> Result<Option<ReplayExecution>> {
        if let Some(repo) = self.replay_repo_if_read() {
            match repo.get_execution_by_id(id).await {
                Ok(Some(exec)) => {
                    let record = self.kv_replay_execution_to_record(exec)?;

                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            if let Ok(Some(sql_exec)) = sqlx::query_as::<_, ReplayExecutionRow>(
                                r#"
                                SELECT id, original_inference_id, tenant_id, replay_mode,
                                       prompt_text, sampling_params_json, backend, manifest_hash,
                                       router_seed, adapter_ids_json, response_text, response_truncated,
                                       tokens_generated, latency_ms, match_status, divergence_details_json,
                                       rag_reproducibility_score, missing_doc_ids_json, executed_at,
                                       executed_by, error_message
                                FROM replay_executions
                                WHERE id = ?
                                "#,
                            )
                            .bind(id)
                            .fetch_optional(pool)
                            .await
                            {
                                let sql_rec: ReplayExecution = sql_exec.into();
                                if sql_rec.match_status != record.match_status
                                    || sql_rec.response_text != record.response_text
                                    || sql_rec.latency_ms != record.latency_ms
                                {
                                    record_replay_drift("replay_execution_drift_dual_write");
                                }
                            }
                        }
                    }

                    return Ok(Some(record));
                }
                Ok(None) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Ok(None);
                    }
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for replay execution: {}",
                            e
                        )));
                    }
                    self.record_kv_read_fallback("replay.execution.get.fallback");
                    warn!(execution_id = %id, error = %e, "KV replay execution read failed, falling back to SQL");
                }
            }
        }

        let record = sqlx::query_as::<_, ReplayExecutionRow>(
            r#"
            SELECT id, original_inference_id, tenant_id, replay_mode,
                   prompt_text, sampling_params_json, backend, manifest_hash,
                   router_seed, adapter_ids_json, response_text, response_truncated,
                   tokens_generated, latency_ms, match_status, divergence_details_json,
                   rag_reproducibility_score, missing_doc_ids_json, executed_at,
                   executed_by, error_message
            FROM replay_executions
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch replay execution: {}", e)))?;

        Ok(record.map(Into::into))
    }

    /// List replay executions for a specific inference
    ///
    /// Retrieves all replay attempts for a given original inference,
    /// ordered by execution time (most recent first).
    ///
    /// # Arguments
    /// * `inference_id` - The original inference ID to look up
    ///
    /// # Returns
    /// Vector of replay execution records, sorted by executed_at DESC
    pub async fn list_replay_executions_for_inference(
        &self,
        inference_id: &str,
    ) -> Result<Vec<ReplayExecution>> {
        if let Some(repo) = self.replay_repo_if_read() {
            match repo.list_executions_for_inference(inference_id).await {
                Ok(execs) => {
                    let mut mapped = Vec::new();
                    for exec in execs {
                        mapped.push(self.kv_replay_execution_to_record(exec)?);
                    }

                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            let sql_records = sqlx::query_as::<_, ReplayExecutionRow>(
                                r#"
                                SELECT id, original_inference_id, tenant_id, replay_mode,
                                       prompt_text, sampling_params_json, backend, manifest_hash,
                                       router_seed, adapter_ids_json, response_text, response_truncated,
                                       tokens_generated, latency_ms, match_status, divergence_details_json,
                                       rag_reproducibility_score, missing_doc_ids_json, executed_at,
                                       executed_by, error_message
                                FROM replay_executions
                                WHERE original_inference_id = ?
                                ORDER BY executed_at DESC, id DESC
                                "#,
                            )
                            .bind(inference_id)
                            .fetch_all(pool)
                            .await
                            .map_err(|e| AosError::Database(format!("Failed to list replay executions: {}", e)))?;

                            if sql_records.len() != mapped.len()
                                || sql_records.iter().zip(mapped.iter()).any(|(sql, kv)| {
                                    sql.id != kv.id || sql.match_status != kv.match_status
                                })
                            {
                                record_replay_drift("replay_execution_list_drift_dual_write");
                            }
                        }
                    }

                    return Ok(mapped);
                }
                Err(e) => {
                    if !self.storage_mode().sql_fallback_enabled() {
                        return Err(AosError::Database(format!(
                            "KV read failed for replay executions: {}",
                            e
                        )));
                    }
                    self.record_kv_read_fallback("replay.execution.list.fallback");
                    warn!(
                        inference_id = %inference_id,
                        error = %e,
                        "KV replay executions read failed, falling back to SQL"
                    );
                }
            }
        }

        let records = sqlx::query_as::<_, ReplayExecutionRow>(
            r#"
            SELECT id, original_inference_id, tenant_id, replay_mode,
                   prompt_text, sampling_params_json, backend, manifest_hash,
                   router_seed, adapter_ids_json, response_text, response_truncated,
                   tokens_generated, latency_ms, match_status, divergence_details_json,
                   rag_reproducibility_score, missing_doc_ids_json, executed_at,
                   executed_by, error_message
            FROM replay_executions
            WHERE original_inference_id = ?
            -- Deterministic ordering: newest executions first, tie-breaker by rowid
            ORDER BY executed_at DESC, rowid DESC
            "#,
        )
        .bind(inference_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list replay executions: {}", e)))?;

        Ok(records.into_iter().map(Into::into).collect())
    }
}

/// Internal row type for SQLx query mapping
#[derive(sqlx::FromRow)]
struct ReplayExecutionRow {
    id: String,
    original_inference_id: String,
    tenant_id: String,
    replay_mode: String,
    prompt_text: String,
    sampling_params_json: String,
    backend: String,
    manifest_hash: String,
    router_seed: Option<String>,
    adapter_ids_json: Option<String>,
    response_text: Option<String>,
    response_truncated: i32,
    tokens_generated: Option<i32>,
    latency_ms: Option<i32>,
    match_status: String,
    divergence_details_json: Option<String>,
    rag_reproducibility_score: Option<f64>,
    missing_doc_ids_json: Option<String>,
    executed_at: String,
    executed_by: Option<String>,
    error_message: Option<String>,
}

impl From<ReplayExecutionRow> for ReplayExecution {
    fn from(row: ReplayExecutionRow) -> Self {
        Self {
            id: row.id,
            original_inference_id: row.original_inference_id,
            tenant_id: row.tenant_id,
            replay_mode: row.replay_mode,
            prompt_text: row.prompt_text,
            sampling_params_json: row.sampling_params_json,
            backend: row.backend,
            manifest_hash: row.manifest_hash,
            router_seed: row.router_seed,
            adapter_ids_json: row.adapter_ids_json,
            response_text: row.response_text,
            response_truncated: row.response_truncated,
            tokens_generated: row.tokens_generated,
            latency_ms: row.latency_ms,
            match_status: row.match_status,
            divergence_details_json: row.divergence_details_json,
            rag_reproducibility_score: row.rag_reproducibility_score,
            missing_doc_ids_json: row.missing_doc_ids_json,
            executed_at: row.executed_at,
            executed_by: row.executed_by,
            error_message: row.error_message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_metadata::CreateReplayMetadataParams;

    // Helper to create parent records for FK constraints
    async fn setup_test_data(db: &Db, tenant_id: &str, inference_id: &str) {
        // Create tenant
        sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name, multi_tenant_mode)
             VALUES (?, 'Test Tenant', 0)",
        )
        .bind(tenant_id)
        .execute(db.pool())
        .await
        .expect("Failed to create tenant");

        // Create inference replay metadata using the canonical API to match schema
        db.create_replay_metadata(CreateReplayMetadataParams {
            inference_id: inference_id.to_string(),
            tenant_id: tenant_id.to_string(),
            manifest_hash: "manifest_hash".to_string(),
            base_model_id: None,
            router_seed: Some("seed123".to_string()),
            sampling_params_json: r#"{"temperature":0.0}"#.to_string(),
            backend: "CoreML".to_string(),
            backend_version: None,
            coreml_package_hash: None,
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: Some("v1.0.0".to_string()),
            rag_snapshot_hash: None,
            adapter_ids: Some(vec!["adapter1".to_string()]),
            base_only: None,
            prompt_text: "prompt".to_string(),
            prompt_truncated: false,
            response_text: Some("response".to_string()),
            response_truncated: false,
            rag_doc_ids: None,
            chat_context_hash: None,
            replay_status: Some("available".to_string()),
            latency_ms: Some(100),
            tokens_generated: Some(100),
            determinism_mode: Some("strict".to_string()),
            fallback_triggered: false,
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            fallback_backend: None,
            replay_guarantee: Some("exact".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
        })
        .await
        .expect("Failed to create inference metadata");
    }

    #[tokio::test]
    async fn test_create_and_retrieve_replay_execution() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = "tenant-001";
        let inference_id = "inf-001";

        setup_test_data(&db, tenant_id, inference_id).await;

        // Create replay execution
        let params = CreateReplayExecutionParams {
            original_inference_id: inference_id.to_string(),
            tenant_id: tenant_id.to_string(),
            replay_mode: "exact".to_string(),
            prompt_text: "Test prompt".to_string(),
            sampling_params_json: r#"{"temperature":0.0}"#.to_string(),
            backend: "CoreML".to_string(),
            manifest_hash: "manifest123".to_string(),
            router_seed: Some("seed123".to_string()),
            adapter_ids: Some(vec!["adapter1".to_string(), "adapter2".to_string()]),
            executed_by: Some("user1".to_string()),
        };

        let id = db.create_replay_execution(params).await.unwrap();
        assert!(!id.is_empty());

        // Retrieve by ID
        let execution = db.get_replay_execution(&id).await.unwrap();
        assert!(execution.is_some());
        let execution = execution.unwrap();
        assert_eq!(execution.original_inference_id, inference_id);
        assert_eq!(execution.replay_mode, "exact");
        assert_eq!(execution.match_status, "pending");
        assert_eq!(execution.executed_by, Some("user1".to_string()));

        // Verify adapter_ids_json is stored correctly
        let adapter_ids_json = execution.adapter_ids_json.as_ref().unwrap();
        let adapter_ids: Vec<String> = serde_json::from_str(adapter_ids_json).unwrap();
        assert_eq!(adapter_ids, vec!["adapter1", "adapter2"]);
    }

    #[tokio::test]
    async fn test_update_replay_execution_result() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = "tenant-002";
        let inference_id = "inf-002";

        setup_test_data(&db, tenant_id, inference_id).await;

        // Create replay execution
        let create_params = CreateReplayExecutionParams {
            original_inference_id: inference_id.to_string(),
            tenant_id: tenant_id.to_string(),
            replay_mode: "exact".to_string(),
            prompt_text: "Test prompt".to_string(),
            sampling_params_json: r#"{"temperature":0.0}"#.to_string(),
            backend: "MLX".to_string(),
            manifest_hash: "manifest456".to_string(),
            router_seed: None,
            adapter_ids: None,
            executed_by: None,
        };

        let id = db.create_replay_execution(create_params).await.unwrap();

        // Update with results
        let update_params = UpdateReplayExecutionParams {
            response_text: Some("Generated response".to_string()),
            response_truncated: false,
            tokens_generated: Some(50),
            latency_ms: Some(250),
            match_status: "exact".to_string(),
            divergence_details: None,
            rag_reproducibility_score: Some(0.98),
            missing_doc_ids: None,
            error_message: None,
        };

        db.update_replay_execution_result(&id, update_params)
            .await
            .unwrap();

        // Retrieve and verify updates
        let execution = db.get_replay_execution(&id).await.unwrap().unwrap();
        assert_eq!(
            execution.response_text,
            Some("Generated response".to_string())
        );
        assert_eq!(execution.response_truncated, 0);
        assert_eq!(execution.tokens_generated, Some(50));
        assert_eq!(execution.latency_ms, Some(250));
        assert_eq!(execution.match_status, "exact");
        assert_eq!(execution.rag_reproducibility_score, Some(0.98));
    }

    #[tokio::test]
    async fn test_list_replay_executions_for_inference() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = "tenant-003";
        let inference_id = "inf-003";

        setup_test_data(&db, tenant_id, inference_id).await;

        // Create multiple replay executions
        for (mode, backend) in [
            ("exact", "CoreML"),
            ("approximate", "MLX"),
            ("degraded", "Metal"),
        ] {
            let params = CreateReplayExecutionParams {
                original_inference_id: inference_id.to_string(),
                tenant_id: tenant_id.to_string(),
                replay_mode: mode.to_string(),
                prompt_text: "Test prompt".to_string(),
                sampling_params_json: r#"{"temperature":0.0}"#.to_string(),
                backend: backend.to_string(),
                manifest_hash: "manifest789".to_string(),
                router_seed: Some("seed123".to_string()),
                adapter_ids: None,
                executed_by: None,
            };

            db.create_replay_execution(params).await.unwrap();
        }

        // List all executions
        let executions = db
            .list_replay_executions_for_inference(inference_id)
            .await
            .unwrap();

        assert_eq!(executions.len(), 3);
        // Should be ordered by executed_at DESC (most recent first)
        assert_eq!(executions[0].replay_mode, "degraded");
        assert_eq!(executions[1].replay_mode, "approximate");
        assert_eq!(executions[2].replay_mode, "exact");
    }

    #[tokio::test]
    async fn test_replay_execution_with_divergence_details() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = "tenant-004";
        let inference_id = "inf-004";

        setup_test_data(&db, tenant_id, inference_id).await;

        // Create replay execution
        let create_params = CreateReplayExecutionParams {
            original_inference_id: inference_id.to_string(),
            tenant_id: tenant_id.to_string(),
            replay_mode: "exact".to_string(),
            prompt_text: "Test prompt".to_string(),
            sampling_params_json: r#"{"temperature":0.0}"#.to_string(),
            backend: "CoreML".to_string(),
            manifest_hash: "manifest_new".to_string(),
            router_seed: Some("seed123".to_string()),
            adapter_ids: None,
            executed_by: None,
        };

        let id = db.create_replay_execution(create_params).await.unwrap();

        // Update with divergence details
        let divergence = serde_json::json!({
            "position": 42,
            "backend_changed": false,
            "manifest_changed": true,
            "reasons": ["Model weights updated"]
        });

        let update_params = UpdateReplayExecutionParams {
            response_text: Some("Different response".to_string()),
            response_truncated: true,
            tokens_generated: Some(100),
            latency_ms: Some(500),
            match_status: "divergent".to_string(),
            divergence_details: Some(divergence),
            rag_reproducibility_score: Some(0.65),
            missing_doc_ids: Some(vec!["doc1".to_string(), "doc2".to_string()]),
            error_message: None,
        };

        db.update_replay_execution_result(&id, update_params)
            .await
            .unwrap();

        // Retrieve and verify
        let execution = db.get_replay_execution(&id).await.unwrap().unwrap();
        assert_eq!(execution.match_status, "divergent");
        assert_eq!(execution.response_truncated, 1);

        // Verify divergence details JSON
        let divergence_json = execution.divergence_details_json.as_ref().unwrap();
        let divergence: serde_json::Value = serde_json::from_str(divergence_json).unwrap();
        assert_eq!(divergence["position"], 42);
        assert_eq!(divergence["manifest_changed"], true);

        // Verify missing doc IDs
        let missing_docs_json = execution.missing_doc_ids_json.as_ref().unwrap();
        let missing_docs: Vec<String> = serde_json::from_str(missing_docs_json).unwrap();
        assert_eq!(missing_docs, vec!["doc1", "doc2"]);
    }

    #[tokio::test]
    async fn test_replay_execution_with_error() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = "tenant-005";
        let inference_id = "inf-005";

        setup_test_data(&db, tenant_id, inference_id).await;

        // Create replay execution
        let create_params = CreateReplayExecutionParams {
            original_inference_id: inference_id.to_string(),
            tenant_id: tenant_id.to_string(),
            replay_mode: "exact".to_string(),
            prompt_text: "Test prompt".to_string(),
            sampling_params_json: r#"{"temperature":0.0}"#.to_string(),
            backend: "MLX".to_string(),
            manifest_hash: "manifest123".to_string(),
            router_seed: None,
            adapter_ids: None,
            executed_by: Some("user2".to_string()),
        };

        let id = db.create_replay_execution(create_params).await.unwrap();

        // Update with error
        let update_params = UpdateReplayExecutionParams {
            response_text: None,
            response_truncated: false,
            tokens_generated: None,
            latency_ms: None,
            match_status: "error".to_string(),
            divergence_details: None,
            rag_reproducibility_score: None,
            missing_doc_ids: None,
            error_message: Some("Backend initialization failed".to_string()),
        };

        db.update_replay_execution_result(&id, update_params)
            .await
            .unwrap();

        // Retrieve and verify error state
        let execution = db.get_replay_execution(&id).await.unwrap().unwrap();
        assert_eq!(execution.match_status, "error");
        assert_eq!(
            execution.error_message,
            Some("Backend initialization failed".to_string())
        );
        assert!(execution.response_text.is_none());
    }
}
