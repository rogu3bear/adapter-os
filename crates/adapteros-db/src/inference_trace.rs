use crate::Db;
use adapteros_core::{
    compute_input_digest_v2, compute_output_digest, emit_observability_event,
    hash_token_decision, receipt_mismatch_event,
    receipt_digest::{compute_receipt_digest, ReceiptDigestInput, RECEIPT_SCHEMA_V4},
    update_run_head, AosError, B3Hash, EquipmentProfile, Result,
};
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
    pub policy_mask_digest_b3: Option<[u8; 32]>,
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
    // Cryptographic Receipt Fields (Patent 3535886.0002)
    /// BLAKE3 digest of input token sequence
    pub input_digest_b3: Option<B3Hash>,
    /// Equipment profile capturing processor and engine versions
    pub equipment_profile: Option<EquipmentProfile>,
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
    /// Optional attestation payload (e.g., determinism report)
    pub attestation: Option<Vec<u8>>,
    // Equipment Profile (Patent 3535886.0002: Cryptographic Receipt)
    /// Pre-computed equipment profile from worker initialization
    pub equipment_profile: Option<EquipmentProfile>,
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
    /// Input digest computed from input tokens (Patent 3535886.0002)
    input_digest_b3: Option<B3Hash>,
}

impl SqlTraceSink {
    pub async fn new(db: Arc<Db>, start: TraceStart, flush_every: usize) -> Result<Self> {
        Self::new_with_input_tokens(db, start, flush_every, None).await
    }

    /// Create a new SqlTraceSink with input token digest computation.
    ///
    /// When `input_tokens` is provided, computes and stores the input digest
    /// (BLAKE3 hash of the input token sequence) for cryptographic receipt
    /// binding per Patent 3535886.0002.
    pub async fn new_with_input_tokens(
        db: Arc<Db>,
        start: TraceStart,
        flush_every: usize,
        input_tokens: Option<&[u32]>,
    ) -> Result<Self> {
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

        // Compute input digest if tokens provided
        let input_digest_b3 = input_tokens.map(compute_input_digest_v2);

        Ok(Self {
            db,
            start,
            buffer: Vec::with_capacity(flush_every.max(1)),
            flush_every: flush_every.max(1),
            run_head_hash: B3Hash::zero(),
            input_digest_b3,
        })
    }

    /// Get the computed input digest (if available)
    pub fn input_digest(&self) -> Option<B3Hash> {
        self.input_digest_b3
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

    /// Decode adapter IDs from blob format
    pub fn decode_adapter_ids(bytes: &[u8]) -> Vec<String> {
        if bytes.len() < 4 {
            return Vec::new();
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut ids = Vec::with_capacity(count);
        let mut cursor = 4;

        for _ in 0..count {
            if cursor + 4 > bytes.len() {
                break;
            }
            let len = u32::from_le_bytes([
                bytes[cursor],
                bytes[cursor + 1],
                bytes[cursor + 2],
                bytes[cursor + 3],
            ]) as usize;
            cursor += 4;

            if cursor + len > bytes.len() {
                break;
            }
            if let Ok(s) = std::str::from_utf8(&bytes[cursor..cursor + len]) {
                ids.push(s.to_string());
            }
            cursor += len;
        }
        ids
    }

    /// Decode gates from Q15 blob format
    pub fn decode_gates_q15(bytes: &[u8]) -> Vec<i16> {
        if bytes.len() < 4 {
            return Vec::new();
        }
        let count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut gates = Vec::with_capacity(count);
        let mut cursor = 4;

        for _ in 0..count {
            if cursor + 2 > bytes.len() {
                break;
            }
            let gate = i16::from_le_bytes([bytes[cursor], bytes[cursor + 1]]);
            gates.push(gate);
            cursor += 2;
        }
        gates
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

    /// Compute hash of a token decision using the canonical algorithm.
    ///
    /// This delegates to `receipt_digest::hash_token_decision` to ensure
    /// parity with offline verification and crypto_receipt module.
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
        // Use canonical implementation from receipt_digest
        hash_token_decision(
            context_digest,
            token_index,
            adapter_blob,
            gates_blob,
            policy_mask_digest,
            allowed_mask_blob,
            policy_overrides_json,
            backend_id,
            kernel_version_id,
        )
    }

    /// Update run_head chain with a new token decision using canonical algorithm.
    ///
    /// This delegates to `receipt_digest::update_run_head` to ensure
    /// parity with offline verification and crypto_receipt module.
    fn update_head(prev: &B3Hash, token_index: u32, decision_hash: &B3Hash) -> B3Hash {
        // Use canonical implementation from receipt_digest
        update_run_head(prev, token_index, decision_hash)
    }

    /// Compute output digest from tokens using canonical algorithm.
    ///
    /// This delegates to `receipt_digest::compute_output_digest` to ensure
    /// parity with offline verification and crypto_receipt module.
    fn output_digest(output_tokens: &[u32]) -> B3Hash {
        // Use canonical implementation from receipt_digest
        compute_output_digest(output_tokens)
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

        let mut tx = self.db.begin_write_tx().await.map_err(|e| {
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
            token.policy_mask_digest_b3,
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
            policy_mask_digest: token.policy_mask_digest_b3,
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

        // Use canonical receipt digest computation (V4 schema)
        let receipt_input = ReceiptDigestInput::new(
            self.start.context_digest,
            *self.run_head_hash.as_bytes(),
            *output_digest.as_bytes(),
            finalization.logical_prompt_tokens,
            finalization.prefix_cached_token_count,
            finalization.billed_input_tokens,
            finalization.logical_output_tokens,
            finalization.billed_output_tokens,
        )
        .with_stop_controller(
            finalization.stop_reason_code.clone(),
            finalization.stop_reason_token_index,
            finalization.stop_policy_digest_b3.map(|h| *h.as_bytes()),
        )
        .with_kv_quota(
            finalization.tenant_kv_quota_bytes,
            finalization.tenant_kv_bytes_used,
            finalization.kv_evictions,
            finalization.kv_residency_policy_id.clone(),
            finalization.kv_quota_enforced,
        )
        .with_prefix_cache(
            finalization.prefix_kv_key_b3.map(|h| *h.as_bytes()),
            finalization.prefix_cache_hit,
            finalization.prefix_kv_bytes,
        )
        .with_model_cache_identity(
            finalization.model_cache_identity_v2_digest_b3.map(|h| *h.as_bytes()),
        );

        let receipt_digest = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V4)
            .expect("V4 schema is always supported");

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

        // PRD-01: Serialize prefix_kv_key_b3 to hex string for TEXT column
        let prefix_kv_key_hex = finalization.prefix_kv_key_b3.as_ref().map(|h| h.to_hex());

        // Serialize equipment profile fields for storage
        let equipment_profile_digest_bytes = finalization
            .equipment_profile
            .as_ref()
            .map(|ep| ep.digest.as_bytes().to_vec());
        let processor_id = finalization
            .equipment_profile
            .as_ref()
            .map(|ep| ep.processor_id.clone());
        let mlx_version = finalization
            .equipment_profile
            .as_ref()
            .map(|ep| ep.engine_version.clone());
        let ane_version = finalization
            .equipment_profile
            .as_ref()
            .and_then(|ep| ep.ane_version.clone());

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
                prefix_kv_key_b3,
                prefix_cache_hit,
                prefix_kv_bytes,
                input_digest_b3,
                equipment_profile_digest_b3,
                processor_id,
                mlx_version,
                ane_version,
                created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
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
        .bind(Option::<Vec<u8>>::None)
        .bind(finalization.attestation.as_deref())
        .bind(&finalization.stop_reason_code)
        .bind(finalization.stop_reason_token_index.map(|i| i as i64))
        .bind(stop_policy_digest_bytes.as_ref().map(|b| &b[..]))
        .bind(
            model_cache_identity_v2_digest_bytes
                .as_ref()
                .map(|b| &b[..]),
        )
        .bind(&prefix_kv_key_hex)
        .bind(finalization.prefix_cache_hit as i64)
        .bind(finalization.prefix_kv_bytes as i64)
        .bind(self.input_digest_b3.as_ref().map(|d| d.as_bytes().to_vec()))
        .bind(equipment_profile_digest_bytes.as_ref().map(|b| &b[..]))
        .bind(&processor_id)
        .bind(&mlx_version)
        .bind(&ane_version)
        .execute(self.db.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert trace receipt: {e}")))?;

        Ok(TraceReceipt {
            trace_id: self.start.trace_id.clone(),
            run_head_hash: self.run_head_hash,
            output_digest,
            receipt_digest,
            signature: None,
            attestation: finalization.attestation.clone(),
            logical_prompt_tokens: finalization.logical_prompt_tokens,
            prefix_cached_token_count: finalization.prefix_cached_token_count,
            billed_input_tokens: finalization.billed_input_tokens,
            logical_output_tokens: finalization.logical_output_tokens,
            billed_output_tokens: finalization.billed_output_tokens,
            stop_reason_code: finalization.stop_reason_code.clone(),
            stop_reason_token_index: finalization.stop_reason_token_index,
            stop_policy_digest_b3: finalization.stop_policy_digest_b3,
            model_cache_identity_v2_digest_b3: finalization.model_cache_identity_v2_digest_b3,
            input_digest_b3: self.input_digest_b3,
            equipment_profile: finalization.equipment_profile.clone(),
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

        // Extract input digest and equipment profile from stored receipt
        let input_digest_bytes: Option<Vec<u8>> =
            row.try_get("input_digest_b3").ok().flatten();
        let input_digest_b3 = match input_digest_bytes {
            Some(bytes) if bytes.len() == 32 => {
                Some(B3Hash::from_bytes(SqlTraceSink::to_digest(bytes)?))
            }
            _ => None,
        };

        let equipment_profile_digest_bytes: Option<Vec<u8>> =
            row.try_get("equipment_profile_digest_b3").ok().flatten();
        let stored_processor_id: Option<String> = row.try_get("processor_id").ok().flatten();
        let stored_mlx_version: Option<String> = row.try_get("mlx_version").ok().flatten();
        let stored_ane_version: Option<String> = row.try_get("ane_version").ok().flatten();

        let equipment_profile = match equipment_profile_digest_bytes {
            Some(bytes) if bytes.len() == 32 => Some(EquipmentProfile {
                processor_id: stored_processor_id.unwrap_or_default(),
                engine_version: stored_mlx_version.unwrap_or_default(),
                ane_version: stored_ane_version,
                digest: B3Hash::from_bytes(SqlTraceSink::to_digest(bytes)?),
            }),
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
            input_digest_b3,
            equipment_profile,
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

    // Use canonical receipt digest computation (V4 schema)
    let receipt_input = ReceiptDigestInput::new(
        context_digest,
        *run_head.as_bytes(),
        *output_digest.as_bytes(),
        logical_prompt_tokens,
        prefix_cached_token_count,
        billed_input_tokens,
        logical_output_tokens,
        billed_output_tokens,
    )
    .with_stop_controller(
        stop_reason_code.clone(),
        stop_reason_token_index,
        stop_policy_digest_b3.map(|h| *h.as_bytes()),
    )
    .with_kv_quota(
        tenant_kv_quota_bytes,
        tenant_kv_bytes_used,
        kv_evictions,
        kv_residency_policy_id.clone(),
        kv_quota_enforced,
    )
    .with_prefix_cache(
        prefix_kv_key_b3.map(|h| *h.as_bytes()),
        prefix_cache_hit,
        prefix_kv_bytes,
    )
    .with_model_cache_identity(
        model_cache_identity_v2_digest_b3.map(|h| *h.as_bytes()),
    );

    let recomputed_receipt_digest = compute_receipt_digest(&receipt_input, RECEIPT_SCHEMA_V4)
        .expect("V4 schema is always supported");

    // For recomputation, carry over input_digest and equipment_profile from stored receipt
    let (recomputed_input_digest_b3, recomputed_equipment_profile) =
        if let Some(stored) = &stored {
            (stored.input_digest_b3, stored.equipment_profile.clone())
        } else {
            (None, None)
        };

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
        input_digest_b3: recomputed_input_digest_b3,
        equipment_profile: recomputed_equipment_profile,
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

// =============================================================================
// Provenance Chain (AUDIT)
// =============================================================================

/// Provenance information for an adapter used in inference
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterProvenance {
    /// Adapter ID
    pub adapter_id: String,
    /// Gate value (Q15 format, 0-32767)
    pub gate_q15: i16,
    /// Gate value as normalized float (0.0-1.0)
    pub gate_normalized: f32,
    /// Training job ID (if known)
    pub training_job_id: Option<String>,
    /// Dataset version ID (if known)
    pub dataset_version_id: Option<String>,
}

/// Provenance information for a source document
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DocumentProvenance {
    /// Source file path
    pub source_file: String,
    /// BLAKE3 hash of the document
    pub source_hash_b3: String,
    /// Line range cited
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    /// Relevance/confidence score of this source
    pub relevance: Option<f32>,
}

/// Full provenance chain from inference to source documents
///
/// This enables the AUDIT phase of the AARA lifecycle by tracing
/// inference decisions back through adapters to their source documents.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProvenanceChain {
    /// Inference trace ID
    pub trace_id: String,
    /// Tenant that owns this trace
    pub tenant_id: String,
    /// Request ID (if available)
    pub request_id: Option<String>,
    /// When the inference occurred
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Adapters that contributed to this inference
    pub adapters_used: Vec<AdapterProvenance>,
    /// Source documents that the adapters were trained on
    pub source_documents: Vec<DocumentProvenance>,
    /// Whether full provenance could be resolved
    pub is_complete: bool,
    /// Any warnings or missing links
    pub warnings: Vec<String>,
}

impl ProvenanceChain {
    /// Create an empty provenance chain
    pub fn new(trace_id: impl Into<String>, tenant_id: impl Into<String>) -> Self {
        Self {
            trace_id: trace_id.into(),
            tenant_id: tenant_id.into(),
            request_id: None,
            created_at: None,
            adapters_used: Vec::new(),
            source_documents: Vec::new(),
            is_complete: false,
            warnings: Vec::new(),
        }
    }

    /// Add a warning message
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Get total confidence based on adapter gates
    pub fn total_confidence(&self) -> f32 {
        if self.adapters_used.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.adapters_used.iter().map(|a| a.gate_normalized).sum();
        sum / self.adapters_used.len() as f32
    }

    /// Get a human-readable summary
    pub fn summary(&self) -> String {
        format!(
            "Trace {} used {} adapter(s) from {} source document(s)",
            self.trace_id,
            self.adapters_used.len(),
            self.source_documents.len()
        )
    }
}

/// Get the provenance chain for an inference trace
///
/// This traces back from the inference through:
/// 1. Inference trace tokens → adapter IDs + gates
/// 2. Adapter → training lineage → dataset versions
/// 3. Dataset versions → training dataset rows → source documents
///
/// Returns a ProvenanceChain with as much information as can be resolved.
pub async fn get_provenance_chain(db: &Db, trace_id: &str) -> Result<ProvenanceChain> {
    // Get the trace header
    let trace_row = sqlx::query(
        r#"
        SELECT trace_id, tenant_id, request_id, created_at
        FROM inference_traces
        WHERE trace_id = ?
        "#,
    )
    .bind(trace_id)
    .fetch_optional(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to fetch trace: {e}")))?;

    let Some(row) = trace_row else {
        return Err(AosError::not_found(format!(
            "Trace not found: {}",
            trace_id
        )));
    };

    let tenant_id: String = row.get("tenant_id");
    let request_id: Option<String> = row.get("request_id");
    let created_at: Option<String> = row.get("created_at");
    let created_at = created_at
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let mut chain = ProvenanceChain::new(trace_id, &tenant_id);
    chain.request_id = request_id;
    chain.created_at = created_at;

    // Get all token-level adapter selections
    let token_rows = sqlx::query(
        r#"
        SELECT adapter_ids_blob, gates_blob
        FROM inference_trace_tokens
        WHERE trace_id = ?
        ORDER BY token_index ASC
        "#,
    )
    .bind(trace_id)
    .fetch_all(db.pool())
    .await
    .map_err(|e| AosError::Database(format!("Failed to fetch trace tokens: {e}")))?;

    // Aggregate adapter usage across all tokens
    let mut adapter_gates: std::collections::HashMap<String, Vec<i16>> =
        std::collections::HashMap::new();

    for row in token_rows {
        let adapter_ids_blob: Vec<u8> = row.get("adapter_ids_blob");
        let gates_blob: Vec<u8> = row.get("gates_blob");

        let adapter_ids = SqlTraceSink::decode_adapter_ids(&adapter_ids_blob);
        let gates = SqlTraceSink::decode_gates_q15(&gates_blob);

        for (adapter_id, gate) in adapter_ids.into_iter().zip(gates.into_iter()) {
            adapter_gates.entry(adapter_id).or_default().push(gate);
        }
    }

    // Convert to AdapterProvenance with average gates
    for (adapter_id, gates) in adapter_gates {
        if gates.is_empty() {
            continue;
        }
        let avg_gate: i32 = gates.iter().map(|&g| g as i32).sum::<i32>() / gates.len() as i32;
        let avg_gate_q15 = avg_gate as i16;
        let gate_normalized = avg_gate_q15 as f32 / 32767.0;

        // Try to get training lineage for this adapter
        let lineage = sqlx::query(
            r#"
            SELECT training_job_id, dataset_version_id
            FROM adapter_training_lineage
            WHERE adapter_id = ?
            LIMIT 1
            "#,
        )
        .bind(&adapter_id)
        .fetch_optional(db.pool())
        .await
        .ok()
        .flatten();

        let (training_job_id, dataset_version_id) = if let Some(lin) = lineage {
            (
                lin.try_get("training_job_id").ok(),
                lin.try_get("dataset_version_id").ok(),
            )
        } else {
            chain.add_warning(format!(
                "No training lineage found for adapter {}",
                adapter_id
            ));
            (None, None)
        };

        chain.adapters_used.push(AdapterProvenance {
            adapter_id,
            gate_q15: avg_gate_q15,
            gate_normalized,
            training_job_id,
            dataset_version_id,
        });
    }

    // Try to get source documents from training dataset rows
    let mut seen_sources: std::collections::HashSet<String> = std::collections::HashSet::new();

    for adapter in &chain.adapters_used {
        if let Some(ref dsv_id) = adapter.dataset_version_id {
            // Get training rows for this dataset version
            let rows = sqlx::query(
                r#"
                SELECT DISTINCT source_file, content_hash_b3
                FROM training_dataset_rows
                WHERE dataset_version_id = ?
                LIMIT 100
                "#,
            )
            .bind(dsv_id)
            .fetch_all(db.pool())
            .await
            .ok()
            .unwrap_or_default();

            for row in rows {
                let source_file: Option<String> = row.try_get("source_file").ok().flatten();
                let hash: Option<String> = row.try_get("content_hash_b3").ok().flatten();

                if let Some(sf) = source_file {
                    if seen_sources.insert(sf.clone()) {
                        chain.source_documents.push(DocumentProvenance {
                            source_file: sf,
                            source_hash_b3: hash.unwrap_or_default(),
                            line_start: None,
                            line_end: None,
                            relevance: None,
                        });
                    }
                }
            }
        }
    }

    // Mark as complete if we found adapters and sources
    chain.is_complete = !chain.adapters_used.is_empty() && !chain.source_documents.is_empty();

    if chain.adapters_used.is_empty() {
        chain.add_warning("No adapter selections found in trace".to_string());
    }

    if chain.source_documents.is_empty() && !chain.adapters_used.is_empty() {
        chain.add_warning("Could not trace adapters back to source documents".to_string());
    }

    Ok(chain)
}
