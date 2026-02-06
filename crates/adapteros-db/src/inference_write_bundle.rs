//! Atomic inference write bundle for evidence + telemetry consistency.
//!
//! Ensures that all inference-related database writes (evidence, replay metadata)
//! are committed atomically within a single transaction.
//!
//! ## Evidence + Telemetry Atomicity (ANCHOR, AUDIT, RECTIFY)
//!
//! - **ANCHOR**: `write_atomic()` wraps all writes in a single transaction
//! - **AUDIT**: Tracks `inference_bundle_commit_success` and `inference_bundle_commit_failed` counters
//! - **RECTIFY**: On failure, entire transaction rolls back - caller can retry

use crate::inference_evidence::CreateEvidenceParams;
use crate::replay_metadata::CreateReplayMetadataParams;
use crate::{Db, Result};
use adapteros_core::AosError;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, error, info};
use crate::new_id;
use adapteros_id::IdPrefix;

/// AUDIT: Global counter for successful bundle commits
static BUNDLE_COMMIT_SUCCESS: AtomicU64 = AtomicU64::new(0);
/// AUDIT: Global counter for failed bundle commits
static BUNDLE_COMMIT_FAILED: AtomicU64 = AtomicU64::new(0);

/// Get count of successful bundle commits
pub fn inference_bundle_commit_success() -> u64 {
    BUNDLE_COMMIT_SUCCESS.load(Ordering::Relaxed)
}

/// Get count of failed bundle commits
pub fn inference_bundle_commit_failed() -> u64 {
    BUNDLE_COMMIT_FAILED.load(Ordering::Relaxed)
}

/// Bundle of inference-related writes to be committed atomically.
///
/// Collects evidence and replay metadata params during inference execution,
/// then writes everything in a single transaction at the end.
#[derive(Debug, Clone, Default)]
pub struct InferenceWriteBundle {
    /// Evidence entries to write (from RAG context retrieval)
    pub evidence_params: Vec<CreateEvidenceParams>,
    /// Replay metadata to write (from inference completion)
    pub replay_metadata_params: Option<CreateReplayMetadataParams>,
}

impl InferenceWriteBundle {
    /// Create a new empty bundle
    pub fn new() -> Self {
        Self::default()
    }

    /// Add evidence params to the bundle
    pub fn add_evidence(&mut self, params: CreateEvidenceParams) {
        self.evidence_params.push(params);
    }

    /// Add multiple evidence params to the bundle
    pub fn add_evidence_batch(&mut self, params: Vec<CreateEvidenceParams>) {
        self.evidence_params.extend(params);
    }

    /// Set the replay metadata params
    pub fn set_replay_metadata(&mut self, params: CreateReplayMetadataParams) {
        self.replay_metadata_params = Some(params);
    }

    /// Check if the bundle has any data to write
    pub fn is_empty(&self) -> bool {
        self.evidence_params.is_empty() && self.replay_metadata_params.is_none()
    }

    /// Get the count of evidence entries in the bundle
    pub fn evidence_count(&self) -> usize {
        self.evidence_params.len()
    }

    /// Check if replay metadata is set
    pub fn has_replay_metadata(&self) -> bool {
        self.replay_metadata_params.is_some()
    }
}

impl Db {
    /// Write inference bundle atomically in a single transaction.
    ///
    /// All writes (evidence + replay metadata) are committed together.
    /// On any failure, the entire transaction is rolled back.
    ///
    /// # Arguments
    /// * `bundle` - The bundle of writes to commit
    ///
    /// # Returns
    /// * `Ok((evidence_ids, replay_id))` - IDs of created records
    /// * `Err(...)` - Transaction rolled back, caller can retry
    pub async fn write_inference_bundle_atomic(
        &self,
        bundle: InferenceWriteBundle,
    ) -> Result<(Vec<String>, Option<String>)> {
        if bundle.is_empty() {
            debug!("Empty inference bundle, nothing to write");
            return Ok((Vec::new(), None));
        }

        let evidence_count = bundle.evidence_count();
        let has_metadata = bundle.has_replay_metadata();

        // Begin transaction
        let mut tx = self.begin_write_tx().await.map_err(|e| {
            BUNDLE_COMMIT_FAILED.fetch_add(1, Ordering::Relaxed);
            error!(
                error = %e,
                evidence_count = evidence_count,
                has_metadata = has_metadata,
                total_failed = BUNDLE_COMMIT_FAILED.load(Ordering::Relaxed),
                "Failed to begin inference bundle transaction"
            );
            e
        })?;

        let mut evidence_ids = Vec::with_capacity(bundle.evidence_params.len());

        // Write evidence entries
        for params in &bundle.evidence_params {
            let id = new_id(IdPrefix::Trc);

            // Serialize RAG fields to JSON
            let rag_doc_ids_json = params
                .rag_doc_ids
                .as_ref()
                .map(|ids| serde_json::to_string(ids).unwrap_or_default());
            let rag_scores_json = params
                .rag_scores
                .as_ref()
                .map(|scores| serde_json::to_string(scores).unwrap_or_default());
            let adapter_ids_json = params
                .adapter_ids
                .as_ref()
                .map(|ids| serde_json::to_string(ids).unwrap_or_default());

            sqlx::query(
                r#"
                INSERT INTO inference_evidence (
                    id, tenant_id, inference_id, session_id, message_id,
                    document_id, chunk_id, page_number, document_hash, chunk_hash,
                    relevance_score, rank, context_hash, created_at,
                    rag_doc_ids, rag_scores, rag_collection_id,
                    base_model_id, adapter_ids, manifest_hash
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(&params.tenant_id)
            .bind(&params.inference_id)
            .bind(&params.session_id)
            .bind(&params.message_id)
            .bind(&params.document_id)
            .bind(&params.chunk_id)
            .bind(params.page_number)
            .bind(&params.document_hash)
            .bind(&params.chunk_hash)
            .bind(params.relevance_score)
            .bind(params.rank)
            .bind(&params.context_hash)
            .bind(&rag_doc_ids_json)
            .bind(&rag_scores_json)
            .bind(&params.rag_collection_id)
            .bind(&params.base_model_id)
            .bind(&adapter_ids_json)
            .bind(&params.manifest_hash)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                BUNDLE_COMMIT_FAILED.fetch_add(1, Ordering::Relaxed);
                error!(
                    error = %e,
                    evidence_id = %id,
                    total_failed = BUNDLE_COMMIT_FAILED.load(Ordering::Relaxed),
                    "Failed to insert evidence in bundle transaction"
                );
                AosError::Database(format!("Failed to insert evidence: {}", e))
            })?;

            evidence_ids.push(id);
        }

        // Write replay metadata if present
        let replay_id = if let Some(params) = &bundle.replay_metadata_params {
            let id = new_id(IdPrefix::Trc);

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
            let utf8_healing = params.utf8_healing.map(|v| if v { 1 } else { 0 });

            sqlx::query(
                r#"
                INSERT INTO inference_replay_metadata (
                    id, inference_id, tenant_id, manifest_hash, base_model_id, router_seed,
                    sampling_params_json, backend, backend_version, coreml_package_hash, coreml_expected_package_hash, coreml_hash_mismatch, sampling_algorithm_version,
                    rag_snapshot_hash, dataset_version_id, adapter_ids_json, base_only, prompt_text, prompt_truncated,
                    response_text, response_truncated, rag_doc_ids_json, chat_context_hash,
                    replay_status, latency_ms, tokens_generated, determinism_mode,
                    fallback_triggered, coreml_compute_preference, coreml_compute_units,
                    coreml_gpu_used, fallback_backend, replay_guarantee, execution_policy_id,
                    execution_policy_version, stop_policy_json, policy_mask_digest_b3, utf8_healing, created_at
                )
                VALUES (
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                    ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
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
            .bind(&params.dataset_version_id)
            .bind(&adapter_ids_json)
            .bind(base_only)
            .bind(&params.prompt_text)
            .bind(prompt_truncated)
            .bind(&params.response_text)
            .bind(response_truncated)
            .bind(&rag_doc_ids_json)
            .bind(&params.chat_context_hash)
            .bind(&replay_status)
            .bind(params.latency_ms)
            .bind(params.tokens_generated)
            .bind(&params.determinism_mode)
            .bind(fallback_triggered)
            .bind(&params.coreml_compute_preference)
            .bind(&params.coreml_compute_units)
            .bind(&coreml_gpu_used)
            .bind(&params.fallback_backend)
            .bind(&params.replay_guarantee)
            .bind(&params.execution_policy_id)
            .bind(params.execution_policy_version)
            .bind(&params.stop_policy_json)
            .bind(&params.policy_mask_digest_b3)
            .bind(&utf8_healing)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                BUNDLE_COMMIT_FAILED.fetch_add(1, Ordering::Relaxed);
                error!(
                    error = %e,
                    replay_id = %id,
                    total_failed = BUNDLE_COMMIT_FAILED.load(Ordering::Relaxed),
                    "Failed to insert replay metadata in bundle transaction"
                );
                AosError::Database(format!("Failed to insert replay metadata: {}", e))
            })?;

            Some(id)
        } else {
            None
        };

        // Commit transaction
        tx.commit().await.map_err(|e| {
            BUNDLE_COMMIT_FAILED.fetch_add(1, Ordering::Relaxed);
            error!(
                error = %e,
                evidence_count = evidence_ids.len(),
                has_metadata = replay_id.is_some(),
                total_failed = BUNDLE_COMMIT_FAILED.load(Ordering::Relaxed),
                "Failed to commit inference bundle transaction"
            );
            AosError::Database(format!("Failed to commit bundle transaction: {}", e))
        })?;

        // AUDIT: Track successful commit
        BUNDLE_COMMIT_SUCCESS.fetch_add(1, Ordering::Relaxed);

        info!(
            evidence_count = evidence_ids.len(),
            replay_id = ?replay_id,
            total_success = BUNDLE_COMMIT_SUCCESS.load(Ordering::Relaxed),
            "Inference bundle committed atomically"
        );

        Ok((evidence_ids, replay_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bundle_is_empty() {
        let bundle = InferenceWriteBundle::new();
        assert!(bundle.is_empty());
        assert_eq!(bundle.evidence_count(), 0);
        assert!(!bundle.has_replay_metadata());
    }

    #[test]
    fn test_bundle_with_evidence() {
        let mut bundle = InferenceWriteBundle::new();
        bundle.add_evidence(CreateEvidenceParams {
            tenant_id: "tenant1".to_string(),
            inference_id: "inf1".to_string(),
            session_id: None,
            message_id: None,
            document_id: "doc1".to_string(),
            chunk_id: "chunk1".to_string(),
            page_number: Some(1),
            document_hash: "hash1".to_string(),
            chunk_hash: "chash1".to_string(),
            relevance_score: 0.95,
            rank: 0,
            context_hash: "ctx1".to_string(),
            rag_doc_ids: None,
            rag_scores: None,
            rag_collection_id: None,
            base_model_id: None,
            adapter_ids: None,
            manifest_hash: None,
        });

        assert!(!bundle.is_empty());
        assert_eq!(bundle.evidence_count(), 1);
        assert!(!bundle.has_replay_metadata());
    }

    #[test]
    fn test_counters_initial_state() {
        // Counters start at 0 (may be non-zero if other tests ran)
        let _success = inference_bundle_commit_success();
        let _failed = inference_bundle_commit_failed();
        // Just verify they're accessible
    }
}
