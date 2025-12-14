use crate::Db;
use adapteros_core::{emit_observability_event, receipt_mismatch_event, AosError, B3Hash, Result};
use async_trait::async_trait;
use serde_json;
use sqlx::Row;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct TraceStart {
    pub trace_id: String,
    pub tenant_id: String,
    pub request_id: Option<String>,
    pub context_digest: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct TraceTokenInput {
    pub token_index: u32,
    pub adapter_ids: Vec<String>,
    pub gates_q15: Vec<i16>,
    pub policy_mask_digest: Option<[u8; 32]>,
    pub allowed_mask: Option<Vec<bool>>,
    pub policy_overrides_applied: Option<adapteros_api_types::inference::PolicyOverrideFlags>,
    pub backend_id: Option<String>,
    pub kernel_version_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TraceReceipt {
    pub trace_id: String,
    pub run_head_hash: B3Hash,
    pub output_digest: B3Hash,
    pub receipt_digest: B3Hash,
    pub signature: Option<Vec<u8>>,
    pub attestation: Option<Vec<u8>>,
    pub logical_prompt_tokens: u32,
    pub prefix_cached_token_count: u32,
    pub billed_input_tokens: u32,
    pub logical_output_tokens: u32,
    pub billed_output_tokens: u32,
    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    pub stop_reason_code: Option<String>,
    pub stop_reason_token_index: Option<u32>,
    pub stop_policy_digest_b3: Option<B3Hash>,
    // Model Cache Identity (PRD-06: ModelCacheIdentity v2)
    /// BLAKE3-256 digest of ModelCacheIdentityV2 canonical bytes
    pub model_cache_identity_v2_digest_b3: Option<B3Hash>,
}

#[derive(Debug, Clone)]
pub struct TraceFinalization<'a> {
    pub output_tokens: &'a [u32],
    pub logical_prompt_tokens: u32,
    pub prefix_cached_token_count: u32,
    pub billed_input_tokens: u32,
    pub logical_output_tokens: u32,
    pub billed_output_tokens: u32,
    // Stop Controller Fields (PRD: Hard Deterministic Stop Controller)
    pub stop_reason_code: Option<String>,
    pub stop_reason_token_index: Option<u32>,
    pub stop_policy_digest_b3: Option<B3Hash>,
    // KV quota/residency fields (PRD: KvResidencyAndQuotas v1)
    pub tenant_kv_quota_bytes: u64,
    pub tenant_kv_bytes_used: u64,
    pub kv_evictions: u32,
    pub kv_residency_policy_id: Option<String>,
    pub kv_quota_enforced: bool,
    // Prefix KV cache fields (PRD: PrefixKvCache v1)
    pub prefix_kv_key_b3: Option<B3Hash>,
    pub prefix_cache_hit: bool,
    pub prefix_kv_bytes: u64,
    // Model Cache Identity (PRD-06: ModelCacheIdentity v2)
    /// BLAKE3-256 digest of ModelCacheIdentityV2 canonical bytes
    pub model_cache_identity_v2_digest_b3: Option<B3Hash>,
}

#[derive(Debug, Clone)]
pub struct TraceReceiptVerification {
    pub matches: bool,
    pub mismatched_token: Option<u32>,
    pub tenant_id: String,
    pub context_digest: [u8; 32],
    pub stored: Option<TraceReceipt>,
    pub recomputed: TraceReceipt,
}

#[async_trait]
pub trait TraceSink: Send {
    async fn record_token(&mut self, token: TraceTokenInput) -> Result<()>;
    async fn finalize(&mut self, finalization: TraceFinalization<'_>) -> Result<TraceReceipt>;
    async fn flush(&mut self) -> Result<()>;
}

struct TraceTokenRow {
    token_index: u32,
    adapter_ids_blob: Vec<u8>,
    gates_blob: Vec<u8>,
    decision_hash: B3Hash,
    policy_mask_digest: Option<[u8; 32]>,
    allowed_mask_blob: Option<Vec<u8>>,
    policy_overrides_applied: Option<adapteros_api_types::inference::PolicyOverrideFlags>,
    backend_id: Option<String>,
    kernel_version_id: Option<String>,
}

pub struct SqlTraceSink {
    db: Arc<Db>,
    start: TraceStart,
    buffer: Vec<TraceTokenRow>,
    flush_every: usize,
    run_head_hash: B3Hash,
}

impl SqlTraceSink {
    pub async fn new(db: Arc<Db>, start: TraceStart, flush_every: usize) -> Result<Self> {
        if !db.storage_mode().write_to_sql() {
            return Err(AosError::Validation(
                "SQL write disabled - cannot persist inference trace".to_string(),
            ));
        }

        sqlx::query(
            r#"
            INSERT INTO inference_traces (trace_id, tenant_id, request_id, context_digest, created_at)
            VALUES (?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&start.trace_id)
        .bind(&start.tenant_id)
        .bind(&start.request_id)
        .bind(&start.context_digest[..])
        .execute(db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert inference_trace: {e}")))?;

        Ok(Self {
            db,
            start,
            buffer: Vec::with_capacity(flush_every.max(1)),
            flush_every: flush_every.max(1),
            run_head_hash: B3Hash::zero(),
        })
    }

    fn encode_adapter_ids(ids: &[String]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + ids.iter().map(|s| s.len() + 4).sum::<usize>());
        out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
        for id in ids {
            let bytes = id.as_bytes();
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        out
    }

    fn encode_gates_q15(gates: &[i16]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + gates.len() * 2);
        out.extend_from_slice(&(gates.len() as u32).to_le_bytes());
        for g in gates {
            out.extend_from_slice(&g.to_le_bytes());
        }
        out
    }

    fn encode_allowed_mask(mask: &[bool]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + mask.len());
        out.extend_from_slice(&(mask.len() as u32).to_le_bytes());
        out.extend(mask.iter().map(|b| if *b { 1u8 } else { 0u8 }));
        out
    }

    fn decode_allowed_mask(bytes: &[u8]) -> Result<Vec<bool>> {
        if bytes.len() < 4 {
            return Err(AosError::InvalidHash(
                "allowed_mask blob missing length".to_string(),
            ));
        }
        let mut cursor = 4;
        let count = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        let mut mask = Vec::with_capacity(count);
        for _ in 0..count {
            if bytes.len() < cursor + 1 {
                return Err(AosError::InvalidHash(
                    "allowed_mask blob truncated (data)".to_string(),
                ));
            }
            mask.push(bytes[cursor] == 1);
            cursor += 1;
        }
        Ok(mask)
    }

    fn encode_overrides_json(
        overrides: &Option<adapteros_api_types::inference::PolicyOverrideFlags>,
    ) -> Result<Option<String>> {
        overrides
            .as_ref()
            .map(|o| serde_json::to_string(o).map_err(|e| AosError::InvalidHash(e.to_string())))
            .transpose()
    }

    fn decode_overrides_json(
        json: Option<String>,
    ) -> Result<Option<adapteros_api_types::inference::PolicyOverrideFlags>> {
        json.map(|s| {
            serde_json::from_str(&s)
                .map_err(|e| AosError::InvalidHash(format!("policy_overrides decode error: {e}")))
        })
        .transpose()
    }

    fn hash_decision(
        context_digest: &[u8; 32],
        token_index: u32,
        adapter_blob: &[u8],
        gates_blob: &[u8],
        policy_mask_digest: Option<[u8; 32]>,
        allowed_mask_blob: Option<&[u8]>,
        policy_overrides_json: Option<&str>,
        backend_id: Option<&str>,
        kernel_version_id: Option<&str>,
    ) -> B3Hash {
        let policy_bytes = policy_mask_digest
            .map(|d| d.to_vec())
            .unwrap_or_else(Vec::new);
        let allowed_bytes = allowed_mask_blob.unwrap_or(&[]);
        let overrides_bytes = policy_overrides_json
            .map(|s| s.as_bytes().to_vec())
            .unwrap_or_else(Vec::new);
        let backend_bytes = backend_id.unwrap_or("").as_bytes().to_vec();
        let kernel_bytes = kernel_version_id.unwrap_or("").as_bytes().to_vec();

        B3Hash::hash_multi(&[
            &context_digest[..],
            &token_index.to_le_bytes(),
            &(adapter_blob.len() as u32).to_le_bytes(),
            adapter_blob,
            &(gates_blob.len() as u32).to_le_bytes(),
            gates_blob,
            &(policy_bytes.len() as u32).to_le_bytes(),
            &policy_bytes,
            &(allowed_bytes.len() as u32).to_le_bytes(),
            allowed_bytes,
            &(overrides_bytes.len() as u32).to_le_bytes(),
            &overrides_bytes,
            &(backend_bytes.len() as u32).to_le_bytes(),
            &backend_bytes,
            &(kernel_bytes.len() as u32).to_le_bytes(),
            &kernel_bytes,
        ])
    }

    fn update_head(prev: &B3Hash, token_index: u32, decision_hash: &B3Hash) -> B3Hash {
        B3Hash::hash_multi(&[
            prev.as_bytes(),
            decision_hash.as_bytes(),
            &token_index.to_le_bytes(),
        ])
    }

    fn output_digest(output_tokens: &[u32]) -> B3Hash {
        let mut buf = Vec::with_capacity(4 + output_tokens.len() * 4);
        buf.extend_from_slice(&(output_tokens.len() as u32).to_le_bytes());
        for t in output_tokens {
            buf.extend_from_slice(&t.to_le_bytes());
        }
        B3Hash::hash(&buf)
    }

    fn compute_receipt_digest(
        context_digest: &[u8; 32],
        run_head_hash: &B3Hash,
        output_digest: &B3Hash,
        logical_prompt_tokens: u32,
        prefix_cached_token_count: u32,
        billed_input_tokens: u32,
        logical_output_tokens: u32,
        billed_output_tokens: u32,
        stop_reason_code: Option<&str>,
        stop_reason_token_index: Option<u32>,
        stop_policy_digest_b3: Option<&B3Hash>,
        tenant_kv_quota_bytes: u64,
        tenant_kv_bytes_used: u64,
        kv_evictions: u32,
        kv_residency_policy_id: Option<&str>,
        kv_quota_enforced: bool,
        prefix_kv_key_b3: Option<&B3Hash>,
        prefix_cache_hit: bool,
        prefix_kv_bytes: u64,
        model_cache_identity_v2_digest_b3: Option<&B3Hash>, // PRD-06
    ) -> B3Hash {
        // Stop fields are serialized deterministically:
        // - Empty string if None for stop_reason_code
        // - 0xFFFFFFFF sentinel if None for stop_reason_token_index
        // - 32 zero bytes if None for stop_policy_digest_b3
        let stop_reason_bytes = stop_reason_code.unwrap_or("").as_bytes();
        let stop_token_index_bytes = stop_reason_token_index.unwrap_or(0xFFFFFFFF).to_le_bytes();
        let stop_policy_bytes = stop_policy_digest_b3
            .map(|d| d.as_bytes().to_vec())
            .unwrap_or_else(|| vec![0u8; 32]);

        // Prefix KV cache fields serialized deterministically:
        // - 32 zero bytes if None for prefix_kv_key_b3
        let prefix_kv_key_bytes = prefix_kv_key_b3
            .map(|d| d.as_bytes().to_vec())
            .unwrap_or_else(|| vec![0u8; 32]);

        // Model cache identity V2 digest (PRD-06):
        // - 32 zero bytes if None (backward compatibility)
        let model_cache_identity_bytes = model_cache_identity_v2_digest_b3
            .map(|d| d.as_bytes().to_vec())
            .unwrap_or_else(|| vec![0u8; 32]);

        B3Hash::hash_multi(&[
            context_digest,
            run_head_hash.as_bytes(),
            output_digest.as_bytes(),
            &logical_prompt_tokens.to_le_bytes(),
            &prefix_cached_token_count.to_le_bytes(),
            &billed_input_tokens.to_le_bytes(),
            &logical_output_tokens.to_le_bytes(),
            &billed_output_tokens.to_le_bytes(),
            // Stop controller fields (PRD: Hard Deterministic Stop Controller)
            &(stop_reason_bytes.len() as u32).to_le_bytes(),
            stop_reason_bytes,
            &stop_token_index_bytes,
            &stop_policy_bytes,
            // KV quota/residency fields (PRD: KvResidencyAndQuotas v1)
            &tenant_kv_quota_bytes.to_le_bytes(),
            &tenant_kv_bytes_used.to_le_bytes(),
            &kv_evictions.to_le_bytes(),
            &(kv_residency_policy_id.map(|s| s.len() as u32).unwrap_or(0)).to_le_bytes(),
            kv_residency_policy_id.map(|s| s.as_bytes()).unwrap_or(&[]),
            &[if kv_quota_enforced { 1u8 } else { 0u8 }],
            // Prefix KV cache fields (PRD: PrefixKvCache v1)
            &prefix_kv_key_bytes,
            &[if prefix_cache_hit { 1u8 } else { 0u8 }],
            &prefix_kv_bytes.to_le_bytes(),
            // Model cache identity V2 (PRD-06)
            &model_cache_identity_bytes,
        ])
    }

    fn to_digest(bytes: Vec<u8>) -> Result<[u8; 32]> {
        if bytes.len() != 32 {
            return Err(AosError::InvalidHash(format!(
                "expected 32-byte digest, got {}",
                bytes.len()
            )));
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(&bytes);
        Ok(out)
    }

    async fn insert_buffer(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut tx = self.db.pool().begin().await.map_err(|e| {
            AosError::Database(format!(
                "Failed to begin inference trace token transaction: {e}"
            ))
        })?;

        for row in self.buffer.drain(..) {
            let overrides_json = Self::encode_overrides_json(&row.policy_overrides_applied)
                .map_err(|e| {
                    AosError::Database(format!(
                        "Failed to serialize policy_overrides_applied for trace {}: {e}",
                        self.start.trace_id
                    ))
                })?;

            sqlx::query(
                r#"
                INSERT INTO inference_trace_tokens (
                    trace_id, token_index, selected_adapter_ids, gates_q15,
                    decision_hash, policy_mask_digest, allowed_mask, policy_overrides_json,
                    backend_id, kernel_version_id, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                "#,
            )
            .bind(&self.start.trace_id)
            .bind(row.token_index as i64)
            .bind(row.adapter_ids_blob)
            .bind(row.gates_blob)
            .bind(&row.decision_hash.as_bytes()[..])
            .bind(row.policy_mask_digest.as_ref().map(|d| &d[..]))
            .bind(row.allowed_mask_blob.as_ref())
            .bind(overrides_json)
            .bind(row.backend_id)
            .bind(row.kernel_version_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::Database(format!("Failed to insert trace token: {e}")))?;
        }

        tx.commit()
            .await
            .map_err(|e| AosError::Database(format!("Failed to commit trace tokens: {e}")))?;

        Ok(())
    }
}

#[async_trait]
impl TraceSink for SqlTraceSink {
    async fn record_token(&mut self, token: TraceTokenInput) -> Result<()> {
        let adapter_blob = Self::encode_adapter_ids(&token.adapter_ids);
        let gates_blob = Self::encode_gates_q15(&token.gates_q15);
        let allowed_blob = token
            .allowed_mask
            .as_ref()
            .map(|mask| Self::encode_allowed_mask(mask));
        let overrides_json = Self::encode_overrides_json(&token.policy_overrides_applied)?;
        let decision_hash = Self::hash_decision(
            &self.start.context_digest,
            token.token_index,
            &adapter_blob,
            &gates_blob,
            token.policy_mask_digest,
            allowed_blob.as_deref(),
            overrides_json.as_deref(),
            token.backend_id.as_deref(),
            token.kernel_version_id.as_deref(),
        );

        self.run_head_hash =
            Self::update_head(&self.run_head_hash, token.token_index, &decision_hash);

        self.buffer.push(TraceTokenRow {
            token_index: token.token_index,
            adapter_ids_blob: adapter_blob,
            gates_blob,
            decision_hash,
            policy_mask_digest: token.policy_mask_digest,
            allowed_mask_blob: allowed_blob,
            policy_overrides_applied: token.policy_overrides_applied,
            backend_id: token.backend_id,
            kernel_version_id: token.kernel_version_id,
        });

        if self.buffer.len() >= self.flush_every {
            self.insert_buffer().await?;
        }

        Ok(())
    }

    async fn finalize(&mut self, finalization: TraceFinalization<'_>) -> Result<TraceReceipt> {
        self.insert_buffer().await?;

        let computed_billed_input = finalization
            .logical_prompt_tokens
            .saturating_sub(finalization.prefix_cached_token_count);
        if computed_billed_input != finalization.billed_input_tokens {
            return Err(AosError::Validation(
                "billed_input_tokens must equal logical_prompt_tokens - prefix_cached_token_count"
                    .to_string(),
            ));
        }
        if finalization.billed_output_tokens != finalization.logical_output_tokens {
            return Err(AosError::Validation(
                "billed_output_tokens must equal logical_output_tokens (v1)".to_string(),
            ));
        }

        let output_digest = Self::output_digest(finalization.output_tokens);
        let receipt_digest = Self::compute_receipt_digest(
            &self.start.context_digest,
            &self.run_head_hash,
            &output_digest,
            finalization.logical_prompt_tokens,
            finalization.prefix_cached_token_count,
            finalization.billed_input_tokens,
            finalization.logical_output_tokens,
            finalization.billed_output_tokens,
            finalization.stop_reason_code.as_deref(),
            finalization.stop_reason_token_index,
            finalization.stop_policy_digest_b3.as_ref(),
            finalization.tenant_kv_quota_bytes,
            finalization.tenant_kv_bytes_used,
            finalization.kv_evictions,
            finalization.kv_residency_policy_id.as_deref(),
            finalization.kv_quota_enforced,
            finalization.prefix_kv_key_b3.as_ref(),
            finalization.prefix_cache_hit,
            finalization.prefix_kv_bytes,
            finalization.model_cache_identity_v2_digest_b3.as_ref(),
        );

        // Serialize stop_policy_digest_b3 to bytes for storage
        let stop_policy_digest_bytes = finalization
            .stop_policy_digest_b3
            .as_ref()
            .map(|d| d.as_bytes().to_vec());

        // Serialize model_cache_identity_v2_digest_b3 to bytes for storage (PRD-06)
        let model_cache_identity_v2_digest_bytes = finalization
            .model_cache_identity_v2_digest_b3
            .as_ref()
            .map(|d| d.as_bytes().to_vec());

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO inference_trace_receipts (
                trace_id,
                run_head_hash,
                output_digest,
                receipt_digest,
                logical_prompt_tokens,
                prefix_cached_token_count,
                billed_input_tokens,
                logical_output_tokens,
                billed_output_tokens,
                signature,
                attestation,
                stop_reason_code,
                stop_reason_token_index,
                stop_policy_digest_b3,
                model_cache_identity_v2_digest_b3,
                created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, NULL, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&self.start.trace_id)
        .bind(&self.run_head_hash.as_bytes()[..])
        .bind(&output_digest.as_bytes()[..])
        .bind(&receipt_digest.as_bytes()[..])
        .bind(finalization.logical_prompt_tokens as i64)
        .bind(finalization.prefix_cached_token_count as i64)
        .bind(finalization.billed_input_tokens as i64)
        .bind(finalization.logical_output_tokens as i64)
        .bind(finalization.billed_output_tokens as i64)
        .bind(&finalization.stop_reason_code)
        .bind(finalization.stop_reason_token_index.map(|i| i as i64))
        .bind(stop_policy_digest_bytes.as_ref().map(|b| &b[..]))
        .bind(
            model_cache_identity_v2_digest_bytes
                .as_ref()
                .map(|b| &b[..]),
        )
        .execute(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert trace receipt: {e}")))?;

        Ok(TraceReceipt {
            trace_id: self.start.trace_id.clone(),
            run_head_hash: self.run_head_hash,
            output_digest,
            receipt_digest,
            signature: None,
            attestation: None,
            logical_prompt_tokens: finalization.logical_prompt_tokens,
            prefix_cached_token_count: finalization.prefix_cached_token_count,
            billed_input_tokens: finalization.billed_input_tokens,
            logical_output_tokens: finalization.logical_output_tokens,
            billed_output_tokens: finalization.billed_output_tokens,
            stop_reason_code: finalization.stop_reason_code.clone(),
            stop_reason_token_index: finalization.stop_reason_token_index,
            stop_policy_digest_b3: finalization.stop_policy_digest_b3,
            model_cache_identity_v2_digest_b3: finalization.model_cache_identity_v2_digest_b3,
        })
    }

    async fn flush(&mut self) -> Result<()> {
        self.insert_buffer().await
    }
}

pub async fn recompute_receipt(db: &Db, trace_id: &str) -> Result<TraceReceiptVerification> {
    let Some(pool) = db.pool_opt() else {
        return Err(AosError::Database(
            "SQL backend unavailable - cannot recompute trace receipt".to_string(),
        ));
    };

    let (context_digest, trace_tenant_id, trace_request_id): ([u8; 32], String, Option<String>) = {
        let row = sqlx::query(
            "SELECT context_digest, tenant_id, request_id FROM inference_traces WHERE trace_id = ? LIMIT 1",
        )
        .bind(trace_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to load inference_trace: {e}")))?;

        let Some(row) = row else {
            return Err(AosError::NotFound(format!("Trace {} not found", trace_id)));
        };
        let bytes: Vec<u8> = row.get("context_digest");
        let tenant: String = row.get("tenant_id");
        let request: Option<String> = row.get("request_id");
        (SqlTraceSink::to_digest(bytes)?, tenant, request)
    };

    let tokens = sqlx::query(
        r#"
        SELECT token_index, selected_adapter_ids, gates_q15, decision_hash,
               policy_mask_digest, allowed_mask, policy_overrides_json, backend_id, kernel_version_id
        FROM inference_trace_tokens
        WHERE trace_id = ?
        ORDER BY token_index ASC
        "#,
    )
    .bind(trace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| AosError::Database(format!("Failed to load trace tokens: {e}")))?;

    let mut run_head = B3Hash::zero();
    let mut mismatched_token: Option<u32> = None;

    for row in &tokens {
        let token_index: i64 = row.get("token_index");
        let adapter_blob: Vec<u8> = row.get("selected_adapter_ids");
        let gates_blob: Vec<u8> = row.get("gates_q15");
        let policy_digest: Option<Vec<u8>> = row.get("policy_mask_digest");
        let allowed_mask_blob: Option<Vec<u8>> = row.get("allowed_mask");
        let policy_overrides_json: Option<String> = row.get("policy_overrides_json");
        let backend_id: Option<String> = row.get("backend_id");
        let kernel_version_id: Option<String> = row.get("kernel_version_id");

        let policy_mask_digest = match policy_digest {
            Some(bytes) if !bytes.is_empty() => Some(SqlTraceSink::to_digest(bytes)?),
            _ => None,
        };
        let allowed_mask = match allowed_mask_blob {
            Some(bytes) if !bytes.is_empty() => Some(SqlTraceSink::decode_allowed_mask(&bytes)?),
            _ => None,
        };
        let policy_overrides_applied = SqlTraceSink::decode_overrides_json(policy_overrides_json)?;

        let recomputed = SqlTraceSink::hash_decision(
            &context_digest,
            token_index as u32,
            &adapter_blob,
            &gates_blob,
            policy_mask_digest,
            allowed_mask
                .as_ref()
                .map(|mask| SqlTraceSink::encode_allowed_mask(mask))
                .as_deref(),
            policy_overrides_applied
                .as_ref()
                .map(|o| serde_json::to_string(o).unwrap_or_default())
                .as_deref(),
            backend_id.as_deref(),
            kernel_version_id.as_deref(),
        );

        let stored_decision: Vec<u8> = row.get("decision_hash");
        let stored_decision = SqlTraceSink::to_digest(stored_decision)?;
        if mismatched_token.is_none() && recomputed.as_bytes() != &stored_decision {
            mismatched_token = Some(token_index as u32);
        }

        run_head = SqlTraceSink::update_head(&run_head, token_index as u32, &recomputed);
    }

    let stored_receipt = sqlx::query(
        r#"
        SELECT run_head_hash,
               output_digest,
               receipt_digest,
               signature,
               attestation,
               logical_prompt_tokens,
               prefix_cached_token_count,
               billed_input_tokens,
               logical_output_tokens,
               billed_output_tokens,
               stop_reason_code,
               stop_reason_token_index,
               stop_policy_digest_b3,
               model_cache_identity_v2_digest_b3
        FROM inference_trace_receipts
        WHERE trace_id = ?
        LIMIT 1
        "#,
    )
    .bind(trace_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AosError::Database(format!("Failed to load trace receipt: {e}")))?;

    let (output_digest, stored) = if let Some(row) = stored_receipt {
        let stored_run_head = SqlTraceSink::to_digest(row.get::<Vec<u8>, _>("run_head_hash"))?;
        let stored_output = SqlTraceSink::to_digest(row.get::<Vec<u8>, _>("output_digest"))?;
        let stored_receipt_digest =
            SqlTraceSink::to_digest(row.get::<Vec<u8>, _>("receipt_digest"))?;
        let logical_prompt_tokens: i64 = row.get("logical_prompt_tokens");
        let prefix_cached_token_count: i64 = row.get("prefix_cached_token_count");
        let billed_input_tokens: i64 = row.get("billed_input_tokens");
        let logical_output_tokens: i64 = row.get("logical_output_tokens");
        let billed_output_tokens: i64 = row.get("billed_output_tokens");
        // Stop controller fields
        let stop_reason_code: Option<String> = row.get("stop_reason_code");
        let stop_reason_token_index: Option<i64> = row.get("stop_reason_token_index");
        let stop_policy_digest_bytes: Option<Vec<u8>> = row.get("stop_policy_digest_b3");
        let stop_policy_digest_b3 = match stop_policy_digest_bytes {
            Some(bytes) if bytes.len() == 32 => {
                Some(B3Hash::from_bytes(SqlTraceSink::to_digest(bytes)?))
            }
            _ => None,
        };
        // Model cache identity v2 digest (PRD-06)
        let model_cache_identity_v2_digest_bytes: Option<Vec<u8>> = row
            .try_get("model_cache_identity_v2_digest_b3")
            .ok()
            .flatten();
        let model_cache_identity_v2_digest_b3 = match model_cache_identity_v2_digest_bytes {
            Some(bytes) if bytes.len() == 32 => {
                Some(B3Hash::from_bytes(SqlTraceSink::to_digest(bytes)?))
            }
            _ => None,
        };

        let stored = TraceReceipt {
            trace_id: trace_id.to_string(),
            run_head_hash: B3Hash::from_bytes(stored_run_head),
            output_digest: B3Hash::from_bytes(stored_output),
            receipt_digest: B3Hash::from_bytes(stored_receipt_digest),
            signature: row.get("signature"),
            attestation: row.get("attestation"),
            logical_prompt_tokens: logical_prompt_tokens as u32,
            prefix_cached_token_count: prefix_cached_token_count as u32,
            billed_input_tokens: billed_input_tokens as u32,
            logical_output_tokens: logical_output_tokens as u32,
            billed_output_tokens: billed_output_tokens as u32,
            stop_reason_code,
            stop_reason_token_index: stop_reason_token_index.map(|i| i as u32),
            stop_policy_digest_b3,
            model_cache_identity_v2_digest_b3,
        };
        (stored.output_digest, Some(stored))
    } else {
        (B3Hash::zero(), None)
    };

    // Derive accounting counts from stored values when available; otherwise default to zero.
    let (
        logical_prompt_tokens,
        prefix_cached_token_count,
        billed_input_tokens,
        logical_output_tokens,
        billed_output_tokens,
    ) = if let Some(stored) = &stored {
        (
            stored.logical_prompt_tokens,
            stored.prefix_cached_token_count,
            stored.billed_input_tokens,
            stored.logical_output_tokens,
            stored.billed_output_tokens,
        )
    } else {
        (0, 0, 0, tokens.len() as u32, tokens.len() as u32)
    };

    // Enforce billed input/output invariants during recomputation and surface mismatches.
    let canonical_billed_input = logical_prompt_tokens.saturating_sub(prefix_cached_token_count);
    let canonical_billed_output = logical_output_tokens;
    let billing_mismatch = billed_input_tokens != canonical_billed_input
        || billed_output_tokens != canonical_billed_output;

    // Extract stop fields from stored receipt for recomputation
    let (stop_reason_code, stop_reason_token_index, stop_policy_digest_b3) =
        if let Some(stored) = &stored {
            (
                stored.stop_reason_code.clone(),
                stored.stop_reason_token_index,
                stored.stop_policy_digest_b3,
            )
        } else {
            (None, None, None)
        };

    // Extract KV fields from stored receipt for recomputation (default to 0/None/false for backward compat)
    let (
        tenant_kv_quota_bytes,
        tenant_kv_bytes_used,
        kv_evictions,
        kv_residency_policy_id,
        kv_quota_enforced,
    ) = (0u64, 0u64, 0u32, None::<String>, false);

    // Extract prefix KV fields from stored receipt for recomputation (default to None/false/0 for backward compat)
    let (prefix_kv_key_b3, prefix_cache_hit, prefix_kv_bytes) = (None::<B3Hash>, false, 0u64);

    // Extract model cache identity v2 digest from stored receipt (default to None for backward compat)
    let model_cache_identity_v2_digest_b3 = stored
        .as_ref()
        .and_then(|s| s.model_cache_identity_v2_digest_b3);

    let recomputed_receipt_digest = SqlTraceSink::compute_receipt_digest(
        &context_digest,
        &run_head,
        &output_digest,
        logical_prompt_tokens,
        prefix_cached_token_count,
        billed_input_tokens,
        logical_output_tokens,
        billed_output_tokens,
        stop_reason_code.as_deref(),
        stop_reason_token_index,
        stop_policy_digest_b3.as_ref(),
        tenant_kv_quota_bytes,
        tenant_kv_bytes_used,
        kv_evictions,
        kv_residency_policy_id.as_deref(),
        kv_quota_enforced,
        prefix_kv_key_b3.as_ref(),
        prefix_cache_hit,
        prefix_kv_bytes,
        model_cache_identity_v2_digest_b3.as_ref(),
    );

    let recomputed = TraceReceipt {
        trace_id: trace_id.to_string(),
        run_head_hash: run_head,
        output_digest,
        receipt_digest: recomputed_receipt_digest,
        signature: None,
        attestation: None,
        logical_prompt_tokens,
        prefix_cached_token_count,
        billed_input_tokens,
        logical_output_tokens,
        billed_output_tokens,
        stop_reason_code,
        stop_reason_token_index,
        stop_policy_digest_b3,
        model_cache_identity_v2_digest_b3,
    };

    let matches = stored
        .as_ref()
        .map(|s| {
            s.receipt_digest == recomputed.receipt_digest
                && mismatched_token.is_none()
                && !billing_mismatch
        })
        .unwrap_or(false);

    if !matches {
        let expected = stored
            .as_ref()
            .map(|r| r.receipt_digest.to_string())
            .unwrap_or_else(|| "missing".to_string());
        let observed = recomputed.receipt_digest.to_string();

        emit_observability_event(&receipt_mismatch_event(
            expected,
            observed,
            trace_id,
            None,
            Some(trace_tenant_id.clone()),
            trace_request_id.clone(),
        ));
    }

    Ok(TraceReceiptVerification {
        matches,
        mismatched_token,
        tenant_id: trace_tenant_id,
        context_digest,
        stored,
        recomputed,
    })
}
