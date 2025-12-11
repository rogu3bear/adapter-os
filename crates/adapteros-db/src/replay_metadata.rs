//! Inference replay metadata tracking for deterministic provenance.
//!
//! Records the replay key and content for each inference operation,
//! enabling exact reproduction of model outputs under identical conditions.

use crate::crypto_at_rest::{crypto_from_env_runtime, redact_for_log, CryptoAtRest, EncryptedField};
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
    /// Base model ID used for this inference (if known)
    pub base_model_id: Option<String>,
    /// Seed for router selection (null if no routing)
    pub router_seed: Option<String>,
    /// JSON: {temperature, top_k, top_p, max_tokens, seed}
    pub sampling_params_json: String,
    /// Backend used: CoreML, MLX, Metal
    pub backend: String,
    /// Backend/FFI version hash or identifier (optional)
    pub backend_version: Option<String>,
    /// Hash of the fused CoreML package manifest (if applicable)
    pub coreml_package_hash: Option<String>,
    /// Expected fused CoreML package hash (if available from registry/manifest)
    pub coreml_expected_package_hash: Option<String>,
    /// Whether verification detected a mismatch between expected and actual hash.
    pub coreml_hash_mismatch: Option<bool>,
    /// Sampling algorithm version for compatibility tracking
    pub sampling_algorithm_version: String,
    /// BLAKE3 hash of sorted document hashes (null if no RAG)
    pub rag_snapshot_hash: Option<String>,
    /// JSON array of adapter IDs used (null if none)
    pub adapter_ids_json: Option<String>,
    /// Whether this inference was executed in base-only mode (no adapters)
    pub base_only: Option<bool>,
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
    /// CoreML compute preference requested (if applicable)
    pub coreml_compute_preference: Option<String>,
    /// CoreML compute units actually used (if applicable)
    pub coreml_compute_units: Option<String>,
    /// Whether CoreML leveraged GPU for this run (if applicable)
    pub coreml_gpu_used: Option<bool>,
    /// Backend selected after fallback (if different from requested)
    pub fallback_backend: Option<String>,
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
    pub base_model_id: Option<String>,
    pub router_seed: Option<String>,
    pub sampling_params_json: String,
    pub backend: String,
    pub backend_version: Option<String>,
    pub coreml_package_hash: Option<String>,
    pub coreml_expected_package_hash: Option<String>,
    pub coreml_hash_mismatch: Option<bool>,
    pub sampling_algorithm_version: Option<String>,
    pub rag_snapshot_hash: Option<String>,
    /// JSON-serializable list of adapter IDs
    pub adapter_ids: Option<Vec<String>>,
    /// Whether the inference was base-only (no adapters)
    pub base_only: Option<bool>,
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
    pub coreml_compute_preference: Option<String>,
    pub coreml_compute_units: Option<String>,
    pub coreml_gpu_used: Option<bool>,
    pub fallback_backend: Option<String>,
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
        let base_only = params.base_only.map(|b| if b { 1 } else { 0 });
        let prompt_truncated = if params.prompt_truncated { 1 } else { 0 };
        let response_truncated = if params.response_truncated { 1 } else { 0 };
        let fallback_triggered = if params.fallback_triggered { 1 } else { 0 };
        let coreml_gpu_used = params.coreml_gpu_used.map(|v| if v { 1 } else { 0 });
        let coreml_hash_mismatch = params.coreml_hash_mismatch.map(|v| if v { 1 } else { 0 });

        // Encrypt prompt/response at rest when enabled
        let mut stored_prompt_text = params.prompt_text.clone();
        let mut stored_response_text = params.response_text.clone();
        if let Some(crypto) = crypto_from_env_runtime() {
            let sealed_prompt = crypto.seal(&params.tenant_id, &params.prompt_text).await?;
            stored_prompt_text = CryptoAtRest::encode(&sealed_prompt)?;

            if let Some(resp) = &params.response_text {
                let sealed_resp = crypto.seal(&params.tenant_id, resp).await?;
                stored_response_text = Some(CryptoAtRest::encode(&sealed_resp)?);
            }

            tracing::debug!(
                tenant_id = %params.tenant_id,
                prompt = %redact_for_log(&params.prompt_text),
                response = %params
                    .response_text
                    .as_deref()
                    .map(redact_for_log)
                    .unwrap_or(""),
                "Stored prompt/response with crypto-at-rest"
            );
        }

        sqlx::query(
            r#"
            INSERT INTO inference_replay_metadata (
                id, inference_id, tenant_id, manifest_hash, base_model_id, router_seed,
                sampling_params_json, backend, backend_version, coreml_package_hash, coreml_expected_package_hash, coreml_hash_mismatch, sampling_algorithm_version,
                rag_snapshot_hash, adapter_ids_json, base_only, prompt_text, prompt_truncated,
                response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                replay_status, latency_ms, tokens_generated, determinism_mode,
                fallback_triggered, coreml_compute_preference, coreml_compute_units,
                coreml_gpu_used, fallback_backend, replay_guarantee, execution_policy_id,
                execution_policy_version, created_at
            )
            VALUES (
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                datetime('now')
            )
            "#,
        )
        .bind(&id)
        .bind(&params.inference_id)
        .bind(&params.tenant_id)
        .bind(&params.manifest_hash)
        .bind(&params.base_model_id)
        .bind(&params.router_seed)
        .bind(&params.sampling_params_json)
        .bind(&params.backend)
        .bind(&params.backend_version)
        .bind(&params.coreml_package_hash)
        .bind(&params.coreml_expected_package_hash)
        .bind(&coreml_hash_mismatch)
        .bind(&sampling_algorithm_version)
        .bind(&params.rag_snapshot_hash)
        .bind(&adapter_ids_json)
        .bind(base_only)
        .bind(&stored_prompt_text)
        .bind(prompt_truncated)
        .bind(&stored_response_text)
        .bind(response_truncated)
        .bind(&rag_doc_ids_json)
        .bind(&params.chat_context_hash)
        .bind(&replay_status)
        .bind(&params.latency_ms)
        .bind(&params.tokens_generated)
        .bind(&params.determinism_mode)
        .bind(fallback_triggered)
        .bind(&params.coreml_compute_preference)
        .bind(&params.coreml_compute_units)
        .bind(coreml_gpu_used)
        .bind(&params.fallback_backend)
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
                            || fetched.base_only != kv_meta.base_only
                        {
                            record_replay_drift("replay_metadata_mismatch");
                        }
                    }
                }
                Err(e) => {
                    self.record_kv_write_fallback("replay.metadata.create");
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

    async fn decrypt_payloads(
        &self,
        tenant_id: &str,
        prompt_text: String,
        response_text: Option<String>,
    ) -> Result<(String, Option<String>)> {
        async fn decode_single(tenant_id: &str, raw: String) -> Result<String> {
            if let Some(field) = EncryptedField::decode(&raw) {
                if let Some(crypto) = crypto_from_env_runtime() {
                    return match crypto.unseal(tenant_id, &field).await? {
                        Some(plaintext) => Ok(plaintext),
                        None => Ok("[redacted]".to_string()),
                    };
                }
            }
            Ok(raw)
        }

        let prompt = decode_single(tenant_id, prompt_text).await?;
        let response = if let Some(r) = response_text {
            Some(decode_single(tenant_id, r).await?)
        } else {
            None
        };

        Ok((prompt, response))
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
                Ok(Some(meta)) => {
                    let record = self.kv_replay_metadata_to_record(meta)?;

                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            if let Ok(Some(sql_meta)) = sqlx::query_as::<_, InferenceReplayMetadataRow>(
                                r#"
                                SELECT id, inference_id, tenant_id, manifest_hash, base_model_id, router_seed,
                                       sampling_params_json, backend, backend_version, coreml_package_hash, coreml_expected_package_hash, coreml_hash_mismatch, sampling_algorithm_version,
                                       rag_snapshot_hash, adapter_ids_json, base_only, prompt_text, prompt_truncated,
                                       response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                                       replay_status, latency_ms, tokens_generated, determinism_mode,
                                       fallback_triggered, coreml_compute_preference, coreml_compute_units,
                                       coreml_gpu_used, fallback_backend, replay_guarantee, execution_policy_id,
                                       execution_policy_version, created_at
                                FROM inference_replay_metadata
                                WHERE inference_id = ?
                                "#,
                            )
                            .bind(inference_id)
                            .fetch_optional(pool)
                            .await
                            {
                                let sql_rec: InferenceReplayMetadata = sql_meta.into();
                                if sql_rec.manifest_hash != record.manifest_hash
                                    || sql_rec.sampling_params_json != record.sampling_params_json
                                    || sql_rec.backend != record.backend
                                    || sql_rec.replay_status != record.replay_status
                                    || sql_rec.base_only != record.base_only
                                {
                                    record_replay_drift("replay_metadata_drift_dual_write");
                                }
                            }
                        }
                    }

                    let (prompt, response) = self
                        .decrypt_payloads(
                            &record.tenant_id,
                            record.prompt_text.clone(),
                            record.response_text.clone(),
                        )
                        .await?;
                    let mut record = record;
                    record.prompt_text = prompt;
                    record.response_text = response;

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
                            "KV read failed for replay metadata: {}",
                            e
                        )));
                    }
                    self.record_kv_read_fallback("replay.metadata.by_inference.fallback");
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
            SELECT id, inference_id, tenant_id, manifest_hash, base_model_id, router_seed,
                   sampling_params_json, backend, backend_version, coreml_package_hash, coreml_expected_package_hash, coreml_hash_mismatch, sampling_algorithm_version,
                   rag_snapshot_hash, adapter_ids_json, base_only, prompt_text, prompt_truncated,
                   response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                   replay_status, latency_ms, tokens_generated, determinism_mode,
                   fallback_triggered, coreml_compute_preference, coreml_compute_units,
                   coreml_gpu_used, fallback_backend, replay_guarantee, execution_policy_id,
                   execution_policy_version, created_at
            FROM inference_replay_metadata
            WHERE inference_id = ?
            "#,
        )
        .bind(inference_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch replay metadata: {}", e)))?;

        if let Some(row) = record {
            let mut rec: InferenceReplayMetadata = row.into();
            let (prompt, response) = self
                .decrypt_payloads(
                    &rec.tenant_id,
                    rec.prompt_text.clone(),
                    rec.response_text.clone(),
                )
                .await?;
            rec.prompt_text = prompt;
            rec.response_text = response;
            Ok(Some(rec))
        } else {
            Ok(None)
        }
    }

    /// Get replay metadata by ID
    ///
    /// Retrieves replay metadata by its unique ID.
    /// Returns None if no metadata exists with the given ID.
    pub async fn get_replay_metadata(&self, id: &str) -> Result<Option<InferenceReplayMetadata>> {
        if let Some(repo) = self.replay_repo_if_read() {
            match repo.get_metadata_by_id(id).await {
                Ok(Some(meta)) => {
                    let record = self.kv_replay_metadata_to_record(meta)?;

                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            if let Ok(Some(sql_meta)) = sqlx::query_as::<_, InferenceReplayMetadataRow>(
                                r#"
                                SELECT id, inference_id, tenant_id, manifest_hash, base_model_id, router_seed,
                                       sampling_params_json, backend, backend_version, coreml_package_hash, coreml_expected_package_hash, coreml_hash_mismatch, sampling_algorithm_version,
                                       rag_snapshot_hash, adapter_ids_json, base_only, prompt_text, prompt_truncated,
                                       response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                                       replay_status, latency_ms, tokens_generated, determinism_mode,
                                       fallback_triggered, coreml_compute_preference, coreml_compute_units,
                                       coreml_gpu_used, fallback_backend, replay_guarantee, execution_policy_id,
                                       execution_policy_version, created_at
                                FROM inference_replay_metadata
                                WHERE id = ?
                                "#,
                            )
                            .bind(id)
                            .fetch_optional(pool)
                            .await
                            {
                                let sql_rec: InferenceReplayMetadata = sql_meta.into();
                                if sql_rec.manifest_hash != record.manifest_hash
                                    || sql_rec.sampling_params_json != record.sampling_params_json
                                    || sql_rec.backend != record.backend
                                    || sql_rec.replay_status != record.replay_status
                                    || sql_rec.base_only != record.base_only
                                {
                                    record_replay_drift("replay_metadata_drift_dual_write");
                                }
                            }
                        }
                    }

                    let (prompt, response) = self
                        .decrypt_payloads(
                            &record.tenant_id,
                            record.prompt_text.clone(),
                            record.response_text.clone(),
                        )
                        .await?;
                    let mut record = record;
                    record.prompt_text = prompt;
                    record.response_text = response;

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
                            "KV read failed for replay metadata: {}",
                            e
                        )));
                    }
                    self.record_kv_read_fallback("replay.metadata.by_id.fallback");
                    warn!(metadata_id = %id, error = %e, "KV replay metadata read failed, falling back to SQL");
                }
            }
        }

        let record = sqlx::query_as::<_, InferenceReplayMetadataRow>(
            r#"
            SELECT id, inference_id, tenant_id, manifest_hash, base_model_id, router_seed,
                   sampling_params_json, backend, backend_version, coreml_package_hash, coreml_expected_package_hash, coreml_hash_mismatch, sampling_algorithm_version,
                   rag_snapshot_hash, adapter_ids_json, base_only, prompt_text, prompt_truncated,
                   response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                   replay_status, latency_ms, tokens_generated, determinism_mode,
                   fallback_triggered, coreml_compute_preference, coreml_compute_units,
                   coreml_gpu_used, fallback_backend, replay_guarantee, execution_policy_id,
                   execution_policy_version, created_at
            FROM inference_replay_metadata
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch replay metadata: {}", e)))?;

        if let Some(row) = record {
            let mut rec: InferenceReplayMetadata = row.into();
            let (prompt, response) = self
                .decrypt_payloads(
                    &rec.tenant_id,
                    rec.prompt_text.clone(),
                    rec.response_text.clone(),
                )
                .await?;
            rec.prompt_text = prompt;
            rec.response_text = response;
            Ok(Some(rec))
        } else {
            Ok(None)
        }
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

                    for rec in mapped.iter_mut() {
                        let (prompt, response) = self
                            .decrypt_payloads(
                                &rec.tenant_id,
                                rec.prompt_text.clone(),
                                rec.response_text.clone(),
                            )
                            .await?;
                        rec.prompt_text = prompt;
                        rec.response_text = response;
                    }

                    if self.storage_mode().is_dual_write() && self.storage_mode().read_from_sql() {
                        if let Some(pool) = self.pool_opt() {
                            let sql_records = sqlx::query_as::<_, InferenceReplayMetadataRow>(
                                r#"
                                SELECT id, inference_id, tenant_id, manifest_hash, base_model_id, router_seed,
                                       sampling_params_json, backend, backend_version, coreml_package_hash, coreml_expected_package_hash, coreml_hash_mismatch, sampling_algorithm_version,
                                       rag_snapshot_hash, adapter_ids_json, base_only, prompt_text, prompt_truncated,
                                       response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                                       replay_status, latency_ms, tokens_generated, determinism_mode,
                                       fallback_triggered, coreml_compute_preference, coreml_compute_units,
                                       coreml_gpu_used, fallback_backend, replay_guarantee, execution_policy_id,
                                       execution_policy_version, created_at
                                FROM inference_replay_metadata
                                WHERE tenant_id = ?
                                ORDER BY created_at DESC, inference_id DESC
                                LIMIT ? OFFSET ?
                                "#,
                            )
                            .bind(tenant_id)
                            .bind(limit)
                            .bind(offset)
                            .fetch_all(pool)
                            .await
                            .map_err(|e| AosError::Database(format!("Failed to list replay metadata: {}", e)))?;

                            if sql_records.len() != mapped.len()
                                || sql_records.iter().zip(mapped.iter()).any(|(sql, kv)| {
                                    sql.inference_id != kv.inference_id
                                        || sql.manifest_hash != kv.manifest_hash
                                        || sql.base_only.map(|v| v != 0) != kv.base_only
                                })
                            {
                                record_replay_drift("replay_metadata_list_drift_dual_write");
                            }
                        }
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
                    self.record_kv_read_fallback("replay.metadata.list_by_tenant.fallback");
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
            SELECT id, inference_id, tenant_id, manifest_hash, base_model_id, router_seed,
                   sampling_params_json, backend, backend_version, coreml_package_hash, coreml_expected_package_hash, coreml_hash_mismatch, sampling_algorithm_version,
                   rag_snapshot_hash, adapter_ids_json, base_only, prompt_text, prompt_truncated,
                   response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                   replay_status, latency_ms, tokens_generated, determinism_mode,
                   fallback_triggered, coreml_compute_preference, coreml_compute_units,
                   coreml_gpu_used, fallback_backend, replay_guarantee, execution_policy_id,
                   execution_policy_version, created_at
            FROM inference_replay_metadata
            WHERE tenant_id = ?
            ORDER BY created_at DESC, inference_id DESC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list replay metadata: {}", e)))?;

        let mut records: Vec<InferenceReplayMetadata> =
            records.into_iter().map(Into::into).collect();
        for rec in records.iter_mut() {
            let (prompt, response) = self
                .decrypt_payloads(
                    &rec.tenant_id,
                    rec.prompt_text.clone(),
                    rec.response_text.clone(),
                )
                .await?;
            rec.prompt_text = prompt;
            rec.response_text = response;
        }

        Ok(records)
    }
}

/// Internal row type for SQLx query mapping
#[derive(sqlx::FromRow)]
struct InferenceReplayMetadataRow {
    id: String,
    inference_id: String,
    tenant_id: String,
    manifest_hash: String,
    base_model_id: Option<String>,
    router_seed: Option<String>,
    sampling_params_json: String,
    backend: String,
    backend_version: Option<String>,
    coreml_package_hash: Option<String>,
    coreml_expected_package_hash: Option<String>,
    coreml_hash_mismatch: Option<i32>,
    sampling_algorithm_version: String,
    rag_snapshot_hash: Option<String>,
    adapter_ids_json: Option<String>,
    base_only: Option<i32>,
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
    coreml_compute_preference: Option<String>,
    coreml_compute_units: Option<String>,
    coreml_gpu_used: Option<i32>,
    fallback_backend: Option<String>,
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
            base_model_id: row.base_model_id,
            router_seed: row.router_seed,
            sampling_params_json: row.sampling_params_json,
            backend: row.backend,
            backend_version: row.backend_version,
            coreml_package_hash: row.coreml_package_hash,
            coreml_expected_package_hash: row.coreml_expected_package_hash,
            coreml_hash_mismatch: row.coreml_hash_mismatch.map(|v| v != 0),
            sampling_algorithm_version: row.sampling_algorithm_version,
            rag_snapshot_hash: row.rag_snapshot_hash,
            adapter_ids_json: row.adapter_ids_json,
            base_only: row.base_only.map(|v| v != 0),
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
            coreml_compute_preference: row.coreml_compute_preference,
            coreml_compute_units: row.coreml_compute_units,
            coreml_gpu_used: row.coreml_gpu_used.map(|v| v != 0),
            fallback_backend: row.fallback_backend,
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
            base_model_id: Some("base-model-123".to_string()),
            router_seed: Some("seed-456".to_string()),
            sampling_params_json: r#"{"temperature":0.7,"top_k":50,"seed":42}"#.to_string(),
            backend: "CoreML".to_string(),
            backend_version: Some("v1.0.0".to_string()),
            coreml_package_hash: Some("coreml-fused-hash".to_string()),
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: Some("v1.0.0".to_string()),
            rag_snapshot_hash: Some("rag-hash-789".to_string()),
            adapter_ids: Some(vec!["adapter-1".to_string(), "adapter-2".to_string()]),
            base_only: None,
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
            coreml_compute_preference: Some("cpu_and_neural_engine".to_string()),
            coreml_compute_units: Some("cpu_and_neural_engine".to_string()),
            coreml_gpu_used: Some(false),
            fallback_backend: None,
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
        assert_eq!(
            metadata.coreml_compute_preference.as_deref(),
            Some("cpu_and_neural_engine")
        );
        assert_eq!(
            metadata.coreml_compute_units.as_deref(),
            Some("cpu_and_neural_engine")
        );
        assert_eq!(metadata.coreml_gpu_used, Some(false));
        assert_eq!(metadata.fallback_backend, None);

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
            base_model_id: None,
            router_seed: None,
            sampling_params_json: r#"{"temperature":0.7}"#.to_string(),
            backend: "MLX".to_string(),
            backend_version: None,
            coreml_package_hash: None,
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: None,
            rag_snapshot_hash: None,
            adapter_ids: None,
            base_only: None,
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
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            fallback_backend: None,
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
                base_model_id: Some(format!("base-{}", i)),
                router_seed: None,
                sampling_params_json: r#"{"temperature":0.7}"#.to_string(),
                backend: "Metal".to_string(),
                backend_version: None,
                coreml_package_hash: None,
                coreml_expected_package_hash: None,
                coreml_hash_mismatch: None,
                sampling_algorithm_version: None,
                rag_snapshot_hash: None,
                adapter_ids: None,
                base_only: None,
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
                coreml_compute_preference: None,
                coreml_compute_units: None,
                coreml_gpu_used: None,
                fallback_backend: None,
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
            base_model_id: None,
            router_seed: None,
            sampling_params_json: r#"{}"#.to_string(),
            backend: "CoreML".to_string(),
            backend_version: None,
            coreml_package_hash: None,
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: None,
            rag_snapshot_hash: None,
            adapter_ids: None,
            base_only: None,
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
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            fallback_backend: None,
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
    async fn test_fallback_flag_persists() {
        let db = Db::new_in_memory().await.unwrap();
        let tenant_id = setup_test_tenant(&db).await;

        let params = CreateReplayMetadataParams {
            inference_id: "inf-fallback".to_string(),
            tenant_id: tenant_id.clone(),
            manifest_hash: "hash-fallback".to_string(),
            base_model_id: None,
            router_seed: None,
            sampling_params_json: "{}".to_string(),
            backend: "Metal".to_string(),
            backend_version: None,
            coreml_package_hash: None,
            coreml_expected_package_hash: None,
            coreml_hash_mismatch: None,
            sampling_algorithm_version: None,
            rag_snapshot_hash: None,
            adapter_ids: Some(vec!["adapter-fb".to_string()]),
            base_only: None,
            prompt_text: "prompt".to_string(),
            prompt_truncated: false,
            response_text: Some("resp".to_string()),
            response_truncated: false,
            rag_doc_ids: None,
            chat_context_hash: None,
            replay_status: Some("available".to_string()),
            latency_ms: Some(10),
            tokens_generated: Some(5),
            determinism_mode: Some("strict".to_string()),
            fallback_triggered: true,
            coreml_compute_preference: None,
            coreml_compute_units: None,
            coreml_gpu_used: None,
            fallback_backend: None,
            replay_guarantee: Some("exact".to_string()),
            execution_policy_id: None,
            execution_policy_version: None,
        };

        let id = db.create_replay_metadata(params).await.unwrap();
        let record = db.get_replay_metadata(&id).await.unwrap().unwrap();
        assert_eq!(record.fallback_triggered, Some(true));
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
