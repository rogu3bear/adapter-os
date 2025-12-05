//! KV helpers for replay metadata, executions, and sessions.

use crate::replay_executions::{
    CreateReplayExecutionParams, ReplayExecution, UpdateReplayExecutionParams,
};
use crate::replay_metadata::{CreateReplayMetadataParams, InferenceReplayMetadata};
use crate::replay_sessions::ReplaySession;
use crate::Db;
use adapteros_core::{AosError, Result};
use adapteros_storage::{ReplayExecutionKv, ReplayMetadataKv, ReplayRepository, ReplaySessionKv};
use chrono::Utc;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, warn};

static REPLAY_DRIFT_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Db {
    pub(crate) fn replay_repo(&self) -> Option<ReplayRepository> {
        self.kv_backend()
            .map(|kv| ReplayRepository::new(kv.backend().clone(), kv.index_manager().clone()))
    }

    pub fn replay_repo_if_write(&self) -> Option<ReplayRepository> {
        if self.storage_mode().write_to_kv() {
            self.replay_repo()
        } else {
            None
        }
    }

    pub(crate) fn replay_repo_if_read(&self) -> Option<ReplayRepository> {
        if self.storage_mode().read_from_kv() {
            self.replay_repo()
        } else {
            None
        }
    }

    pub(crate) fn kv_replay_metadata_from_params(
        &self,
        id: &str,
        params: &CreateReplayMetadataParams,
    ) -> ReplayMetadataKv {
        ReplayMetadataKv {
            id: id.to_string(),
            inference_id: params.inference_id.clone(),
            tenant_id: params.tenant_id.clone(),
            manifest_hash: params.manifest_hash.clone(),
            router_seed: params.router_seed.clone(),
            sampling_params_json: params.sampling_params_json.clone(),
            backend: params.backend.clone(),
            sampling_algorithm_version: params
                .sampling_algorithm_version
                .clone()
                .unwrap_or_else(|| "v1.0.0".to_string()),
            rag_snapshot_hash: params.rag_snapshot_hash.clone(),
            adapter_ids_json: params
                .adapter_ids
                .as_ref()
                .map(|ids| serde_json::to_string(ids).unwrap_or_default()),
            prompt_text: params.prompt_text.clone(),
            prompt_truncated: params.prompt_truncated as i32,
            response_text: params.response_text.clone(),
            response_truncated: params.response_truncated as i32,
            rag_doc_ids_json: params
                .rag_doc_ids
                .as_ref()
                .map(|ids| serde_json::to_string(ids).unwrap_or_default()),
            chat_context_hash: params.chat_context_hash.clone(),
            replay_status: params
                .replay_status
                .clone()
                .unwrap_or_else(|| "available".to_string()),
            latency_ms: params.latency_ms,
            tokens_generated: params.tokens_generated,
            determinism_mode: params.determinism_mode.clone(),
            fallback_triggered: Some(params.fallback_triggered),
            replay_guarantee: params.replay_guarantee.clone(),
            execution_policy_id: params.execution_policy_id.clone(),
            execution_policy_version: params.execution_policy_version,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    pub(crate) fn kv_replay_metadata_to_record(
        &self,
        meta: ReplayMetadataKv,
    ) -> Result<InferenceReplayMetadata> {
        Ok(InferenceReplayMetadata {
            id: meta.id,
            inference_id: meta.inference_id,
            tenant_id: meta.tenant_id,
            manifest_hash: meta.manifest_hash,
            router_seed: meta.router_seed,
            sampling_params_json: meta.sampling_params_json,
            backend: meta.backend,
            sampling_algorithm_version: meta.sampling_algorithm_version,
            rag_snapshot_hash: meta.rag_snapshot_hash,
            adapter_ids_json: meta.adapter_ids_json,
            prompt_text: meta.prompt_text,
            prompt_truncated: meta.prompt_truncated,
            response_text: meta.response_text,
            response_truncated: meta.response_truncated,
            rag_doc_ids_json: meta.rag_doc_ids_json,
            chat_context_hash: meta.chat_context_hash,
            replay_status: meta.replay_status,
            latency_ms: meta.latency_ms,
            tokens_generated: meta.tokens_generated,
            determinism_mode: meta.determinism_mode,
            fallback_triggered: meta.fallback_triggered,
            replay_guarantee: meta.replay_guarantee,
            execution_policy_id: meta.execution_policy_id,
            execution_policy_version: meta.execution_policy_version,
            created_at: meta.created_at,
        })
    }

    pub(crate) fn kv_replay_execution_from_create(
        &self,
        id: &str,
        params: &CreateReplayExecutionParams,
    ) -> ReplayExecutionKv {
        ReplayExecutionKv {
            id: id.to_string(),
            original_inference_id: params.original_inference_id.clone(),
            tenant_id: params.tenant_id.clone(),
            replay_mode: params.replay_mode.clone(),
            prompt_text: params.prompt_text.clone(),
            sampling_params_json: params.sampling_params_json.clone(),
            backend: params.backend.clone(),
            manifest_hash: params.manifest_hash.clone(),
            router_seed: params.router_seed.clone(),
            adapter_ids_json: params
                .adapter_ids
                .as_ref()
                .map(|ids| serde_json::to_string(ids).unwrap_or_default()),
            response_text: None,
            response_truncated: 0,
            tokens_generated: None,
            latency_ms: None,
            match_status: "pending".to_string(),
            divergence_details_json: None,
            rag_reproducibility_score: None,
            missing_doc_ids_json: None,
            executed_at: Utc::now().to_rfc3339(),
            executed_by: params.executed_by.clone(),
            error_message: None,
        }
    }

    pub(crate) fn kv_replay_execution_apply_update(
        &self,
        exec: &mut ReplayExecutionKv,
        params: &UpdateReplayExecutionParams,
    ) -> Result<()> {
        exec.response_text = params.response_text.clone();
        exec.response_truncated = params.response_truncated as i32;
        exec.tokens_generated = params.tokens_generated;
        exec.latency_ms = params.latency_ms;
        exec.match_status = params.match_status.clone();
        exec.divergence_details_json = params
            .divergence_details
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());
        exec.rag_reproducibility_score = params.rag_reproducibility_score;
        exec.missing_doc_ids_json = params
            .missing_doc_ids
            .as_ref()
            .map(|ids| serde_json::to_string(ids).unwrap_or_default());
        exec.error_message = params.error_message.clone();
        Ok(())
    }

    pub(crate) fn kv_replay_execution_to_record(
        &self,
        exec: ReplayExecutionKv,
    ) -> Result<ReplayExecution> {
        Ok(ReplayExecution {
            id: exec.id,
            original_inference_id: exec.original_inference_id,
            tenant_id: exec.tenant_id,
            replay_mode: exec.replay_mode,
            prompt_text: exec.prompt_text,
            sampling_params_json: exec.sampling_params_json,
            backend: exec.backend,
            manifest_hash: exec.manifest_hash,
            router_seed: exec.router_seed,
            adapter_ids_json: exec.adapter_ids_json,
            response_text: exec.response_text,
            response_truncated: exec.response_truncated,
            tokens_generated: exec.tokens_generated,
            latency_ms: exec.latency_ms,
            match_status: exec.match_status,
            divergence_details_json: exec.divergence_details_json,
            rag_reproducibility_score: exec.rag_reproducibility_score,
            missing_doc_ids_json: exec.missing_doc_ids_json,
            executed_at: exec.executed_at,
            executed_by: exec.executed_by,
            error_message: exec.error_message,
        })
    }

    pub fn kv_replay_session_from_record(session: &ReplaySession) -> ReplaySessionKv {
        ReplaySessionKv {
            id: session.id.clone(),
            tenant_id: session.tenant_id.clone(),
            cpid: session.cpid.clone(),
            plan_id: session.plan_id.clone(),
            snapshot_at: session.snapshot_at.clone(),
            seed_global_b3: session.seed_global_b3.clone(),
            manifest_hash_b3: session.manifest_hash_b3.clone(),
            policy_hash_b3: session.policy_hash_b3.clone(),
            kernel_hash_b3: session.kernel_hash_b3.clone(),
            telemetry_bundle_ids_json: session.telemetry_bundle_ids_json.clone(),
            adapter_state_json: session.adapter_state_json.clone(),
            routing_decisions_json: session.routing_decisions_json.clone(),
            inference_traces_json: session.inference_traces_json.clone(),
            rng_state_json: session.rng_state_json.clone(),
            signature: session.signature.clone(),
            rag_state_json: session.rag_state_json.clone(),
            created_at: session.created_at.clone(),
        }
    }

    pub(crate) fn kv_replay_session_to_record(
        &self,
        session: ReplaySessionKv,
    ) -> Result<ReplaySession> {
        Ok(ReplaySession {
            id: session.id,
            tenant_id: session.tenant_id,
            cpid: session.cpid,
            plan_id: session.plan_id,
            snapshot_at: session.snapshot_at,
            seed_global_b3: session.seed_global_b3,
            manifest_hash_b3: session.manifest_hash_b3,
            policy_hash_b3: session.policy_hash_b3,
            kernel_hash_b3: session.kernel_hash_b3,
            telemetry_bundle_ids_json: session.telemetry_bundle_ids_json,
            adapter_state_json: session.adapter_state_json,
            routing_decisions_json: session.routing_decisions_json,
            inference_traces_json: session.inference_traces_json,
            rng_state_json: session.rng_state_json,
            signature: session.signature,
            created_at: session.created_at,
            rag_state_json: session.rag_state_json,
        })
    }
}

#[allow(dead_code)]
pub(crate) fn replay_drift_count() -> u64 {
    REPLAY_DRIFT_COUNTER.load(Ordering::Relaxed)
}

pub(crate) fn record_replay_drift(reason: &str) {
    warn!(reason = reason, "Replay KV/SQL drift detected");
    REPLAY_DRIFT_COUNTER.fetch_add(1, Ordering::Relaxed);
    debug!(
        drift_total = replay_drift_count(),
        "Replay drift counter updated"
    );
}
