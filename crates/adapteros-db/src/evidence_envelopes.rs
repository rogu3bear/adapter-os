//! Evidence envelope storage with chain-linked Merkle verification
//!
//! Stores and retrieves unified evidence envelopes for telemetry, policy audit,
//! and inference traces. Each envelope is chain-linked to the previous via
//! `previous_root` for tamper-evident audit trails.
//!
//! # Chain Verification
//!
//! Each envelope within a tenant+scope pair forms a chain:
//! - First envelope has `previous_root = None`
//! - Subsequent envelopes reference the prior envelope's `root`
//! - Chain breaks are detected and reported as divergence errors
//!
//! # Example
//!
//! ```no_run
//! use adapteros_db::Db;
//! use adapteros_core::{EvidenceEnvelope, EvidenceScope, BundleMetadataRef, B3Hash};
//!
//! # async fn example(db: &Db) -> anyhow::Result<()> {
//! // Create a telemetry envelope
//! let bundle_ref = BundleMetadataRef {
//!     bundle_hash: B3Hash::hash(b"bundle"),
//!     merkle_root: B3Hash::hash(b"merkle"),
//!     event_count: 100,
//!     cpid: Some("cp-001".to_string()),
//!     sequence_no: Some(1),
//! };
//!
//! let envelope = EvidenceEnvelope::new_telemetry(
//!     "tenant-1".to_string(),
//!     bundle_ref,
//!     None,
//! );
//!
//! let id = db.store_evidence_envelope(&envelope).await?;
//! # Ok(())
//! # }
//! ```

use crate::query_helpers::{db_err, FilterBuilder};
use crate::Db;
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::evidence_envelope::EvidenceEnvelope;
use adapteros_core::evidence_verifier::{
    evidence_chain_divergence, ChainVerificationResult, EVIDENCE_CHAIN_DIVERGED_CODE,
};
use adapteros_core::{AosError, B3Hash, EvidenceScope, Result};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use tracing::warn;

/// Result of verifying all evidence chains across tenants and scopes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllChainsVerificationResult {
    /// Tenant ID this result pertains to
    pub tenant_id: String,
    /// Evidence scope this result pertains to
    pub scope: EvidenceScope,
    /// Overall validity of the chain
    pub is_valid: bool,
    /// Number of envelopes checked
    pub envelopes_checked: usize,
    /// Whether divergence was detected
    pub divergence_detected: bool,
    /// Index of first invalid envelope (if any)
    pub first_invalid_index: Option<usize>,
    /// Error message if verification failed
    pub error_message: Option<String>,
    /// Duration of verification in milliseconds
    pub duration_ms: u64,
}

/// Filters for querying evidence envelopes
///
/// All filters are optional (None = no filter applied).
/// Multiple filters are combined with AND logic.
#[derive(Debug, Default, Clone)]
pub struct EvidenceEnvelopeFilter {
    /// Filter by tenant ID
    pub tenant_id: Option<String>,
    /// Filter by evidence scope
    pub scope: Option<EvidenceScope>,
    /// Filter by minimum chain sequence (inclusive)
    pub from_sequence: Option<i64>,
    /// Filter by maximum chain sequence (inclusive)
    pub to_sequence: Option<i64>,
    /// Filter by key_id (for signature verification)
    pub key_id: Option<String>,
    /// Maximum number of results
    pub limit: Option<i64>,
    /// Offset for pagination
    pub offset: Option<i64>,
}

/// Database row representation for evidence_envelopes table
#[allow(dead_code)]
#[derive(Debug, Clone, sqlx::FromRow)]
struct EvidenceEnvelopeRow {
    pub id: String,
    pub schema_version: i32,
    pub tenant_id: String,
    pub scope: String,
    pub previous_root: Option<String>,
    pub root: String,
    pub signature: String,
    pub public_key: String,
    pub key_id: String,
    pub attestation_ref: Option<String>,
    pub created_at: String,
    pub signed_at_us: i64,
    pub payload_json: String,
    pub chain_sequence: i64,
}

impl Db {
    /// Get the tail of an evidence chain (latest envelope)
    ///
    /// Returns the root hash and sequence number of the latest envelope
    /// for the given tenant and scope, or None if the chain is empty.
    pub async fn get_evidence_chain_tail(
        &self,
        tenant_id: &str,
        scope: EvidenceScope,
    ) -> Result<Option<(B3Hash, i64)>> {
        let scope_str = scope.as_str();

        let row = sqlx::query(
            r#"
            SELECT root, chain_sequence
            FROM evidence_envelopes
            WHERE tenant_id = ? AND scope = ?
            ORDER BY chain_sequence DESC
            LIMIT 1
            "#,
        )
        .bind(tenant_id)
        .bind(scope_str)
        .fetch_optional(self.pool())
        .await
        .db_err("fetch evidence chain tail")?;

        match row {
            Some(r) => {
                let root_hex: String = r.get("root");
                let seq: i64 = r.get("chain_sequence");
                let root = B3Hash::from_hex(&root_hex)?;
                Ok(Some((root, seq)))
            }
            None => Ok(None),
        }
    }

    /// Store a new evidence envelope with chain validation
    ///
    /// Validates that the envelope correctly links to the existing chain
    /// before storing. Returns the envelope ID on success.
    ///
    /// # Transaction Guarantees
    ///
    /// The chain tail read and envelope insert are wrapped in a transaction to ensure
    /// atomicity. This prevents race conditions where concurrent writes could corrupt
    /// the chain linkage (e.g., two writers both reading the same tail and inserting
    /// with the same `previous_root`).
    ///
    /// # Errors
    ///
    /// Returns `EVIDENCE_CHAIN_DIVERGED` error if:
    /// - `previous_root` doesn't match the current chain tail
    /// - Envelope claims to be first but chain already has entries
    /// - Envelope claims a previous but chain is empty
    pub async fn store_evidence_envelope(&self, envelope: &EvidenceEnvelope) -> Result<String> {
        // Validate envelope structure first (outside transaction - pure validation)
        envelope.validate()?;

        // Begin transaction for atomic chain read + insert
        let mut tx = self.begin_write_tx().await?;

        // Get current chain tail within transaction
        let tail: Option<(String, i64)> = sqlx::query_as(
            r#"
            SELECT root, chain_sequence
            FROM evidence_envelopes
            WHERE tenant_id = ? AND scope = ?
            ORDER BY chain_sequence DESC
            LIMIT 1
            "#,
        )
        .bind(&envelope.tenant_id)
        .bind(envelope.scope.as_str())
        .fetch_optional(&mut *tx)
        .await
        .db_err("fetch chain tail in transaction")?;

        let tail = tail.map(|(root, seq)| -> Result<(B3Hash, i64)> {
            Ok((B3Hash::from_hex(&root)?, seq))
        }).transpose()?;

        // =========================================================================
        // Evidence Chain Sequence Validation (1-indexed)
        //
        // SECURITY: Validates that envelope correctly links to the existing chain:
        // 1. First entry: chain_sequence == 1, previous_root must be None
        // 2. Subsequent entries: chain_sequence == last_sequence + 1, must have correct previous_root
        //
        // This prevents:
        // - Sequence gaps that could hide tampered/missing entries
        // - Forged first entries with non-zero sequences
        // - Chain injection attacks via sequence number manipulation
        // =========================================================================

        // Compute expected sequence (1-indexed: first entry = 1, second = 2, etc.)
        let expected_sequence = match &tail {
            Some((_, last_seq)) => last_seq + 1,
            None => 1, // First entry should have sequence 1
        };

        // For first entry: require previous_root.is_none()
        if expected_sequence == 1 {
            // This is the first entry in the chain
            if envelope.previous_root.is_some() {
                return Err(evidence_chain_divergence(format!(
                    "CHAIN_SEQUENCE_FIRST_ENTRY_INVALID: unexpected previous_root. First envelope in chain must have \
                     previous_root = None, but got previous_root = {}. \
                     First entries cannot reference a prior envelope.",
                    envelope
                        .previous_root
                        .as_ref()
                        .map(|h| h.to_short_hex())
                        .unwrap_or_default()
                )));
            }
        }

        // Log sequence for audit trail
        tracing::debug!(
            tenant_id = %envelope.tenant_id,
            scope = ?envelope.scope,
            expected_sequence = expected_sequence,
            previous_root = ?envelope.previous_root.as_ref().map(|h| h.to_short_hex()),
            "Storing evidence envelope with sequence continuity check (transactional)"
        );

        // Verify chain linkage (previous_root must match current tail)
        match (&envelope.previous_root, &tail) {
            (Some(prev), Some((expected_root, seq))) if prev != expected_root => {
                return Err(evidence_chain_divergence(format!(
                    "PREVIOUS_ROOT_MISMATCH: previous_root mismatch. Expected previous_root = {} (sequence {}), \
                     but envelope has previous_root = {}. Chain linkage is broken.",
                    expected_root.to_short_hex(),
                    seq,
                    prev.to_short_hex()
                )));
            }
            (None, Some((expected_root, seq))) => {
                return Err(evidence_chain_divergence(format!(
                    "PREVIOUS_ROOT_MISSING: Chain has {} entries (last sequence = {}), \
                     expected previous_root = {}, but envelope has previous_root = None. \
                     Non-first entries must reference the prior envelope's root.",
                    seq,
                    seq,
                    expected_root.to_short_hex()
                )));
            }
            (Some(prev), None) => {
                return Err(evidence_chain_divergence(format!(
                    "PREVIOUS_ROOT_UNEXPECTED: unexpected previous_root. Envelope claims previous_root = {}, but \
                     chain is empty. First envelope must have previous_root = None.",
                    prev.to_short_hex()
                )));
            }
            // Valid states:
            // - (None, None): First envelope, correctly has no previous
            // - (Some(prev), Some((expected, _))) where prev == expected: Correct linkage
            _ => {}
        }

        let id = uuid::Uuid::now_v7().to_string();
        let scope_str = envelope.scope.as_str();
        let payload_json = serde_json::to_string(envelope)?;

        sqlx::query(
            r#"
            INSERT INTO evidence_envelopes (
                id, schema_version, tenant_id, scope, previous_root, root,
                signature, public_key, key_id, attestation_ref,
                created_at, signed_at_us, payload_json, chain_sequence
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(envelope.schema_version as i32)
        .bind(&envelope.tenant_id)
        .bind(scope_str)
        .bind(envelope.previous_root.as_ref().map(|h| h.to_hex()))
        .bind(envelope.root.to_hex())
        .bind(&envelope.signature)
        .bind(&envelope.public_key)
        .bind(&envelope.key_id)
        .bind(&envelope.attestation_ref)
        .bind(&envelope.created_at)
        .bind(envelope.signed_at_us as i64)
        .bind(&payload_json)
        .bind(expected_sequence)
        .execute(&mut *tx)
        .await
        .db_err("insert evidence envelope")?;

        // Commit the transaction
        tx.commit()
            .await
            .map_err(|e| AosError::database(format!("Failed to commit evidence envelope: {e}")))?;

        Ok(id)
    }

    /// Get an evidence envelope by ID
    pub async fn get_evidence_envelope(&self, id: &str) -> Result<Option<EvidenceEnvelope>> {
        let row = sqlx::query_as::<_, EvidenceEnvelopeRow>(
            r#"
            SELECT id, schema_version, tenant_id, scope, previous_root, root,
                   signature, public_key, key_id, attestation_ref,
                   created_at, signed_at_us, payload_json, chain_sequence
            FROM evidence_envelopes
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .db_err("fetch evidence envelope by id")?;

        match row {
            Some(r) => {
                let mut envelope: EvidenceEnvelope = serde_json::from_str(&r.payload_json)?;
                envelope.root = B3Hash::from_hex(&r.root)?;
                envelope.previous_root = r
                    .previous_root
                    .as_ref()
                    .map(|h| B3Hash::from_hex(h))
                    .transpose()?;
                Ok(Some(envelope))
            }
            None => Ok(None),
        }
    }

    /// Query evidence envelopes with filters
    pub async fn query_evidence_envelopes(
        &self,
        filter: EvidenceEnvelopeFilter,
    ) -> Result<Vec<EvidenceEnvelope>> {
        let mut builder = FilterBuilder::new(
            r#"
            SELECT id, schema_version, tenant_id, scope, previous_root, root,
                   signature, public_key, key_id, attestation_ref,
                   created_at, signed_at_us, payload_json, chain_sequence
            FROM evidence_envelopes
            WHERE 1=1
            "#
            .to_string(),
        );

        builder.add_filter("tenant_id", filter.tenant_id.as_ref());
        builder.add_filter("scope", filter.scope.map(|s| s.as_str().to_string()));
        builder.add_filter("key_id", filter.key_id.as_ref());

        // Range filters need custom handling
        if let Some(from_seq) = filter.from_sequence {
            builder.push_str(" AND chain_sequence >= ?");
            builder.add_param(from_seq);
        }
        if let Some(to_seq) = filter.to_sequence {
            builder.push_str(" AND chain_sequence <= ?");
            builder.add_param(to_seq);
        }

        builder.push_str(" ORDER BY chain_sequence ASC");

        if let Some(limit) = filter.limit {
            builder.push_str(&format!(" LIMIT {}", limit));
        }
        if let Some(offset) = filter.offset {
            if filter.limit.is_none() {
                builder.push_str(" LIMIT -1");
            }
            builder.push_str(&format!(" OFFSET {}", offset));
        }

        let (sql, args) = builder.build();
        let mut query = sqlx::query_as::<_, EvidenceEnvelopeRow>(&sql);
        for arg in &args {
            query = query.bind(arg);
        }

        let rows = query
            .fetch_all(self.pool())
            .await
            .db_err("query evidence envelopes")?;

        let mut envelopes = Vec::with_capacity(rows.len());
        for row in rows {
            let mut envelope: EvidenceEnvelope = serde_json::from_str(&row.payload_json)?;
            envelope.root = B3Hash::from_hex(&row.root)?;
            envelope.previous_root = row
                .previous_root
                .as_ref()
                .map(|h| B3Hash::from_hex(h))
                .transpose()?;
            envelopes.push(envelope);
        }

        Ok(envelopes)
    }

    /// Verify evidence chain integrity for a tenant and scope
    ///
    /// Loads all envelopes for the chain and verifies:
    /// - Each envelope's root matches its computed root
    /// - Chain linkage is correct (previous_root matches prior envelope's root)
    /// - Sequence numbers are monotonically increasing
    pub async fn verify_evidence_chain(
        &self,
        tenant_id: &str,
        scope: EvidenceScope,
    ) -> Result<ChainVerificationResult> {
        let envelopes = self
            .query_evidence_envelopes(EvidenceEnvelopeFilter {
                tenant_id: Some(tenant_id.to_string()),
                scope: Some(scope),
                ..Default::default()
            })
            .await?;

        if envelopes.is_empty() {
            return Ok(ChainVerificationResult {
                is_valid: true,
                envelopes_checked: 0,
                first_invalid_index: None,
                divergence_detected: false,
                error_message: None,
            });
        }

        let verifier = adapteros_core::EvidenceVerifier::new();
        verifier.verify_chain(&envelopes)
    }

    /// Verify all evidence envelope chains across all tenants and scopes
    ///
    /// Returns a vector of verification results for each tenant+scope combination,
    /// continuing to check all chains even if some have diverged. This allows
    /// operators to see the full scope of any chain integrity issues.
    ///
    /// Checks all three evidence scopes: Telemetry, Policy, Inference
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let results = db.verify_all_evidence_chains().await?;
    /// for result in &results {
    ///     if result.divergence_detected {
    ///         eprintln!("ALERT: Tenant {} scope {:?} has divergent chain at index {}",
    ///             result.tenant_id, result.scope, result.first_invalid_index.unwrap_or(0));
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_all_evidence_chains(&self) -> Result<Vec<AllChainsVerificationResult>> {
        // Get distinct tenant IDs from evidence envelopes
        let tenants: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT tenant_id FROM evidence_envelopes ORDER BY tenant_id",
        )
        .fetch_all(self.pool())
        .await
        .db_err("fetch distinct tenant IDs for evidence envelopes")?;

        let scopes = [
            EvidenceScope::Telemetry,
            EvidenceScope::Policy,
            EvidenceScope::Inference,
        ];
        let mut results = Vec::new();

        for tenant_id in &tenants {
            for scope in &scopes {
                let start = std::time::Instant::now();
                let result = self.verify_evidence_chain(tenant_id, *scope).await?;
                let duration_ms = start.elapsed().as_millis() as u64;

                let all_result = AllChainsVerificationResult {
                    tenant_id: tenant_id.clone(),
                    scope: *scope,
                    is_valid: result.is_valid,
                    envelopes_checked: result.envelopes_checked,
                    divergence_detected: result.divergence_detected,
                    first_invalid_index: result.first_invalid_index,
                    error_message: result.error_message.clone(),
                    duration_ms,
                };

                if all_result.divergence_detected {
                    tracing::error!(
                        tenant_id = %tenant_id,
                        scope = ?scope,
                        first_invalid_index = ?result.first_invalid_index,
                        error_message = ?result.error_message,
                        "Evidence envelope chain divergence detected"
                    );
                }

                results.push(all_result);
            }
        }

        Ok(results)
    }

    /// Count evidence envelopes by tenant and scope
    pub async fn count_evidence_envelopes(
        &self,
        tenant_id: &str,
        scope: Option<EvidenceScope>,
    ) -> Result<i64> {
        let (sql, scope_val) = match scope {
            Some(s) => (
                "SELECT COUNT(*) as count FROM evidence_envelopes WHERE tenant_id = ? AND scope = ?",
                Some(s.as_str().to_string()),
            ),
            None => (
                "SELECT COUNT(*) as count FROM evidence_envelopes WHERE tenant_id = ?",
                None,
            ),
        };

        let mut query = sqlx::query(sql).bind(tenant_id);
        if let Some(ref s) = scope_val {
            query = query.bind(s);
        }

        let row = query
            .fetch_one(self.pool())
            .await
            .db_err("count evidence envelopes")?;
        let count: i64 = row.get("count");
        Ok(count)
    }

    /// Delete all evidence envelopes for a tenant (for testing/cleanup)
    ///
    /// # Warning
    ///
    /// This permanently deletes all evidence envelopes for the tenant.
    /// Use with caution - primarily for testing.
    pub async fn delete_tenant_evidence_envelopes(&self, tenant_id: &str) -> Result<u64> {
        let result = sqlx::query("DELETE FROM evidence_envelopes WHERE tenant_id = ?")
            .bind(tenant_id)
            .execute(self.pool())
            .await
            .db_err("delete tenant evidence envelopes")?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::evidence_envelope::{
        BundleMetadataRef, InferenceReceiptRef, PolicyAuditRef,
    };

    // Note: These tests require a database connection.
    // Integration tests are in crates/adapteros-db/tests/evidence_envelope_tests.rs
}
