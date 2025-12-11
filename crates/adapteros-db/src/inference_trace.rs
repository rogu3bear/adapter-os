use crate::Db;
use adapteros_core::{AosError, B3Hash, Result};
use async_trait::async_trait;
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
}

#[derive(Debug, Clone)]
pub struct TraceReceiptVerification {
    pub matches: bool,
    pub mismatched_token: Option<u32>,
    pub stored: Option<TraceReceipt>,
    pub recomputed: TraceReceipt,
}

#[async_trait]
pub trait TraceSink: Send {
    async fn record_token(&mut self, token: TraceTokenInput) -> Result<()>;
    async fn finalize(&mut self, output_tokens: &[u32]) -> Result<TraceReceipt>;
    async fn flush(&mut self) -> Result<()>;
}

struct TraceTokenRow {
    token_index: u32,
    adapter_ids_blob: Vec<u8>,
    gates_blob: Vec<u8>,
    decision_hash: B3Hash,
    policy_mask_digest: Option<[u8; 32]>,
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
            INSERT INTO inference_traces (id, tenant_id, request_id, context_digest, created_at)
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

    fn hash_decision(
        context_digest: &[u8; 32],
        token_index: u32,
        adapter_blob: &[u8],
        gates_blob: &[u8],
        policy_mask_digest: Option<[u8; 32]>,
        backend_id: Option<&str>,
        kernel_version_id: Option<&str>,
    ) -> B3Hash {
        let policy_bytes = policy_mask_digest
            .map(|d| d.to_vec())
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
            sqlx::query(
                r#"
                INSERT INTO inference_trace_tokens (
                    trace_id, token_index, selected_adapter_ids, gates_q15,
                    decision_hash, policy_mask_digest, backend_id, kernel_version_id, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                "#,
            )
            .bind(&self.start.trace_id)
            .bind(row.token_index as i64)
            .bind(row.adapter_ids_blob)
            .bind(row.gates_blob)
            .bind(row.decision_hash.as_bytes())
            .bind(row.policy_mask_digest.as_ref().map(|d| &d[..]))
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
        let decision_hash = Self::hash_decision(
            &self.start.context_digest,
            token.token_index,
            &adapter_blob,
            &gates_blob,
            token.policy_mask_digest,
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
            backend_id: token.backend_id,
            kernel_version_id: token.kernel_version_id,
        });

        if self.buffer.len() >= self.flush_every {
            self.insert_buffer().await?;
        }

        Ok(())
    }

    async fn finalize(&mut self, output_tokens: &[u32]) -> Result<TraceReceipt> {
        self.insert_buffer().await?;

        let output_digest = Self::output_digest(output_tokens);
        let receipt_digest = B3Hash::hash_multi(&[
            &self.start.context_digest,
            self.run_head_hash.as_bytes(),
            output_digest.as_bytes(),
        ]);

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO inference_trace_receipts (
                trace_id, run_head_hash, output_digest, receipt_digest, signature, attestation, created_at
            ) VALUES (?, ?, ?, ?, NULL, NULL, datetime('now'))
            "#,
        )
        .bind(&self.start.trace_id)
        .bind(self.run_head_hash.as_bytes())
        .bind(output_digest.as_bytes())
        .bind(receipt_digest.as_bytes())
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

    let context_digest: [u8; 32] = {
        let row = sqlx::query("SELECT context_digest FROM inference_traces WHERE id = ? LIMIT 1")
            .bind(trace_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to load inference_trace: {e}")))?;

        let Some(row) = row else {
            return Err(AosError::NotFound(format!("Trace {} not found", trace_id)));
        };
        let bytes: Vec<u8> = row.get("context_digest");
        SqlTraceSink::to_digest(bytes)?
    };

    let tokens = sqlx::query(
        r#"
        SELECT token_index, selected_adapter_ids, gates_q15, decision_hash,
               policy_mask_digest, backend_id, kernel_version_id
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
        let backend_id: Option<String> = row.get("backend_id");
        let kernel_version_id: Option<String> = row.get("kernel_version_id");

        let policy_mask_digest = match policy_digest {
            Some(bytes) if !bytes.is_empty() => Some(SqlTraceSink::to_digest(bytes)?),
            _ => None,
        };

        let recomputed = SqlTraceSink::hash_decision(
            &context_digest,
            token_index as u32,
            &adapter_blob,
            &gates_blob,
            policy_mask_digest,
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
        SELECT run_head_hash, output_digest, receipt_digest, signature, attestation
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

        let stored = TraceReceipt {
            trace_id: trace_id.to_string(),
            run_head_hash: B3Hash::from_bytes(stored_run_head),
            output_digest: B3Hash::from_bytes(stored_output),
            receipt_digest: B3Hash::from_bytes(stored_receipt_digest),
            signature: row.get("signature"),
            attestation: row.get("attestation"),
        };
        (stored.output_digest, Some(stored))
    } else {
        (B3Hash::zero(), None)
    };

    let recomputed_receipt_digest = B3Hash::hash_multi(&[
        &context_digest,
        run_head.as_bytes(),
        output_digest.as_bytes(),
    ]);

    let recomputed = TraceReceipt {
        trace_id: trace_id.to_string(),
        run_head_hash: run_head,
        output_digest,
        receipt_digest: recomputed_receipt_digest,
        signature: None,
        attestation: None,
    };

    let matches = stored
        .as_ref()
        .map(|s| s.receipt_digest == recomputed.receipt_digest && mismatched_token.is_none())
        .unwrap_or(false);

    Ok(TraceReceiptVerification {
        matches,
        mismatched_token,
        stored,
        recomputed,
    })
}
