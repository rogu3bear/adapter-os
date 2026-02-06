//! Policy audit decision logging with Merkle chain for compliance
//!
//! All policy decisions (allow/deny) are logged with cryptographic chaining
//! for tamper-evident audit trails. Each decision links to the previous via BLAKE3 hash.
#![allow(unused_variables)]

use crate::new_id;
use crate::policy_audit_kv::PolicyAuditKvRepository;
use crate::query_helpers::{db_err, FilterBuilder};
use crate::{Db, KvBackend};
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::{AosError, Result};
use adapteros_id::IdPrefix;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;

/// Error code used when the policy audit chain diverges
pub const AUDIT_CHAIN_DIVERGED_CODE: &str = "AUDIT_CHAIN_DIVERGED";

fn audit_chain_divergence(msg: impl Into<String>) -> AosError {
    AosError::Validation(format!("{}: {}", AUDIT_CHAIN_DIVERGED_CODE, msg.into()))
}

pub fn is_audit_chain_divergence(err: &AosError) -> bool {
    matches!(err, AosError::Validation(msg) if msg.contains(AUDIT_CHAIN_DIVERGED_CODE))
}

/// Policy audit decision record
///
/// Represents a single policy decision (allow/deny) in the audit trail.
/// Each decision is cryptographically chained to the previous decision
/// via BLAKE3 hashing to create a tamper-evident log.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PolicyAuditDecision {
    pub id: String,
    pub tenant_id: String,
    pub policy_pack_id: String,
    pub hook: String,
    pub decision: String,
    pub reason: Option<String>,
    pub request_id: Option<String>,
    pub user_id: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub metadata_json: Option<String>,
    pub timestamp: String,
    pub entry_hash: String,
    pub previous_hash: Option<String>,
    pub chain_sequence: i64,
}

/// Result of chain verification
///
/// Contains detailed information about the integrity of the policy audit chain,
/// including whether the chain is valid and where any issues were detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    /// Overall validity of the chain
    pub is_valid: bool,
    /// Number of entries checked during verification
    pub entries_checked: usize,
    /// Sequence number of first invalid entry (if any)
    pub first_invalid_sequence: Option<i64>,
    /// Description of the first validation failure
    pub error_message: Option<String>,
    /// Whether divergence was detected (tampered entries)
    #[serde(default)]
    pub divergence_detected: bool,
    /// Tenant ID this result pertains to (when verifying specific tenant)
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// Duration of verification in milliseconds
    #[serde(default)]
    pub duration_ms: u64,
}

/// Filters for querying policy decisions
///
/// All filters are optional (None = no filter applied).
/// Multiple filters are combined with AND logic.
#[derive(Debug, Default, Clone)]
pub struct PolicyDecisionFilters {
    pub tenant_id: Option<String>,
    pub policy_pack_id: Option<String>,
    pub hook: Option<String>,
    pub decision: Option<String>,
    pub from_time: Option<String>,
    pub to_time: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl Db {
    fn get_policy_audit_kv_repo(&self) -> Option<PolicyAuditKvRepository> {
        if self.storage_mode().write_to_kv() || self.storage_mode().read_from_kv() {
            self.kv_backend().map(|kv| {
                let backend: Arc<dyn KvBackend> = kv.clone();
                PolicyAuditKvRepository::new(backend)
            })
        } else {
            None
        }
    }

    fn compute_policy_entry_hash(entry: &PolicyAuditDecision) -> String {
        let entry_data = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            entry.id,
            entry.timestamp,
            entry.tenant_id,
            entry.policy_pack_id,
            entry.hook,
            entry.decision,
            entry.reason.as_deref().unwrap_or(""),
            entry.request_id.as_deref().unwrap_or(""),
            entry.user_id.as_deref().unwrap_or(""),
            entry.resource_type.as_deref().unwrap_or(""),
            entry.resource_id.as_deref().unwrap_or(""),
            entry.metadata_json.as_deref().unwrap_or(""),
            entry.previous_hash.as_deref().unwrap_or(""),
        );
        adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string()
    }

    /// Validate the tail of the policy audit chain for a tenant.
    ///
    /// Returns the last entry hash and sequence if present.
    async fn validate_policy_audit_tail(&self, tenant_id: &str) -> Result<Option<(String, i64)>> {
        // Fetch the last two entries to validate linkage and hashes
        let tail: Vec<PolicyAuditDecision> = sqlx::query_as::<_, PolicyAuditDecision>(
            "SELECT id, tenant_id, policy_pack_id, hook, decision, reason, request_id, user_id,
                    resource_type, resource_id, metadata_json, timestamp, entry_hash, previous_hash, chain_sequence
             FROM policy_audit_decisions
             WHERE tenant_id = ?
             ORDER BY chain_sequence DESC
             LIMIT 2",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .db_err("fetch policy audit tail")?;

        if tail.is_empty() {
            return Ok(None);
        }

        let latest = &tail[0];
        let latest_hash = Self::compute_policy_entry_hash(latest);
        if latest_hash != latest.entry_hash {
            return Err(audit_chain_divergence(format!(
                "tail hash mismatch at seq {} (expected {}, found {})",
                latest.chain_sequence, latest_hash, latest.entry_hash
            )));
        }

        if latest.chain_sequence == 1 {
            if latest.previous_hash.is_some() {
                return Err(audit_chain_divergence(
                    "first entry unexpectedly has previous_hash",
                ));
            }
            return Ok(Some((latest.entry_hash.clone(), latest.chain_sequence)));
        }

        if tail.len() < 2 {
            return Err(audit_chain_divergence(
                "missing predecessor for latest audit entry",
            ));
        }

        let prev = &tail[1];
        let prev_hash = Self::compute_policy_entry_hash(prev);
        if prev_hash != prev.entry_hash {
            return Err(audit_chain_divergence(format!(
                "previous entry hash mismatch at seq {}",
                prev.chain_sequence
            )));
        }

        match (&latest.previous_hash, Some(&prev.entry_hash)) {
            (Some(stored), Some(expected)) if stored == expected => {
                Ok(Some((latest.entry_hash.clone(), latest.chain_sequence)))
            }
            (Some(stored), Some(expected)) => Err(audit_chain_divergence(format!(
                "previous_hash mismatch at seq {} (expected {}, found {})",
                latest.chain_sequence, expected, stored
            ))),
            _ => Err(audit_chain_divergence(
                "latest entry missing previous_hash linkage",
            )),
        }
    }

    /// Forcefully corrupt the tail of the policy audit chain for testing.
    ///
    /// Only intended for E2E/testkit usage guarded by environment flags.
    pub async fn force_corrupt_policy_audit_tail(
        &self,
        tenant_id: &str,
    ) -> Result<(String, String, i64)> {
        let latest = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT id, entry_hash, chain_sequence
             FROM policy_audit_decisions
             WHERE tenant_id = ?
             ORDER BY chain_sequence DESC
             LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .db_err("fetch latest policy audit entry")?;

        let Some((entry_id, entry_hash, chain_sequence)) = latest else {
            return Err(audit_chain_divergence(
                "no audit entries available to corrupt",
            ));
        };

        // Corrupt the linkage by mutating previous_hash without updating entry_hash.
        sqlx::query(
            "UPDATE policy_audit_decisions
             SET previous_hash = 'corrupted-e2e-divergence'
             WHERE id = ?",
        )
        .bind(&entry_id)
        .execute(self.pool())
        .await
        .db_err("corrupt policy audit tail")?;

        Ok((entry_id, entry_hash, chain_sequence))
    }

    /// Log a policy decision to the audit trail
    ///
    /// Creates a new policy audit entry with cryptographic chaining to the previous entry.
    /// Each entry includes a BLAKE3 hash computed from its contents plus the previous hash,
    /// forming a tamper-evident Merkle chain.
    ///
    /// # Arguments
    /// * `tenant_id` - Tenant context for the decision
    /// * `policy_pack_id` - ID of the policy pack that made the decision
    /// * `hook` - Policy hook that was evaluated (e.g., "adapter.register", "training.start")
    /// * `decision` - Decision result ("allow" or "deny")
    /// * `reason` - Human-readable explanation for the decision
    /// * `request_id` - Optional request ID for correlation
    /// * `user_id` - User who initiated the request
    /// * `resource_type` - Type of resource being accessed
    /// * `resource_id` - ID of the resource being accessed
    /// * `metadata_json` - Additional context as JSON
    ///
    /// # Returns
    /// The ID of the created audit entry
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let id = db.log_policy_decision(
    ///     "tenant-123",
    ///     "router-policy-v1",
    ///     "adapter.load",
    ///     "allow",
    ///     Some("Adapter within memory budget"),
    ///     Some("req-456"),
    ///     Some("user-789"),
    ///     Some("adapter"),
    ///     Some("adapter-xyz"),
    ///     None,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn log_policy_decision(
        &self,
        tenant_id: &str,
        policy_pack_id: &str,
        hook: &str,
        decision: &str,
        reason: Option<&str>,
        request_id: Option<&str>,
        user_id: Option<&str>,
        resource_type: Option<&str>,
        resource_id: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<String> {
        if !self.storage_mode().write_to_sql() {
            return Err(AosError::Validation(
                "SQL write disabled - cannot log policy audit decision".to_string(),
            ));
        }

        let kv_repo = if self.storage_mode().write_to_kv() {
            Some(self.get_policy_audit_kv_repo().ok_or_else(|| {
                AosError::Validation(
                    "KV backend unavailable - cannot enforce dual-write policy audit".to_string(),
                )
            })?)
        } else {
            None
        };

        let tail_state = self.validate_policy_audit_tail(tenant_id).await?;
        let (previous_hash, previous_seq) = match tail_state {
            Some((hash, seq)) => (Some(hash), seq),
            None => (None, 0),
        };
        let chain_sequence = previous_seq + 1;

        let id = new_id(IdPrefix::Aud);
        let timestamp = chrono::Utc::now().to_rfc3339();

        // Ensure KV is aligned before writing when KV is enabled.
        if let Some(repo) = kv_repo.as_ref() {
            if let Some(kv_latest) = repo.latest_entry(tenant_id).await? {
                if kv_latest.chain_sequence != previous_seq {
                    return Err(AosError::Validation(format!(
                        "KV policy audit chain out of sync (expected seq {}, found {})",
                        previous_seq, kv_latest.chain_sequence
                    )));
                }
                if let Some(prev_hash) = &previous_hash {
                    if kv_latest.entry_hash != *prev_hash {
                        return Err(AosError::Validation(
                            "KV policy audit previous hash mismatch".to_string(),
                        ));
                    }
                }
            } else if previous_seq != 0 {
                return Err(AosError::Validation(
                    "KV policy audit missing prior entries while SQL has history".to_string(),
                ));
            }
        }

        let mut entry = PolicyAuditDecision {
            id: id.clone(),
            tenant_id: tenant_id.to_string(),
            policy_pack_id: policy_pack_id.to_string(),
            hook: hook.to_string(),
            decision: decision.to_string(),
            reason: reason.map(|s| s.to_string()),
            request_id: request_id.map(|s| s.to_string()),
            user_id: user_id.map(|s| s.to_string()),
            resource_type: resource_type.map(|s| s.to_string()),
            resource_id: resource_id.map(|s| s.to_string()),
            metadata_json: metadata_json.map(|s| s.to_string()),
            timestamp,
            entry_hash: String::new(),
            previous_hash,
            chain_sequence,
        };

        entry.entry_hash = Self::compute_policy_entry_hash(&entry);

        sqlx::query(
            "INSERT INTO policy_audit_decisions
         (id, tenant_id, policy_pack_id, hook, decision, reason, request_id, user_id,
          resource_type, resource_id, metadata_json, timestamp, previous_hash, entry_hash, chain_sequence)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&entry.id)
        .bind(&entry.tenant_id)
        .bind(&entry.policy_pack_id)
        .bind(&entry.hook)
        .bind(&entry.decision)
        .bind(&entry.reason)
        .bind(&entry.request_id)
        .bind(&entry.user_id)
        .bind(&entry.resource_type)
        .bind(&entry.resource_id)
        .bind(&entry.metadata_json)
        .bind(&entry.timestamp)
        .bind(&entry.previous_hash)
        .bind(&entry.entry_hash)
        .bind(entry.chain_sequence)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        if let Some(repo) = kv_repo.as_ref() {
            repo.put_entry(&entry).await?;

            let kv_latest = repo.latest_entry(tenant_id).await?.ok_or_else(|| {
                AosError::Validation("KV policy audit missing after write".to_string())
            })?;
            if kv_latest.chain_sequence != entry.chain_sequence
                || kv_latest.entry_hash != entry.entry_hash
            {
                return Err(AosError::Validation(
                    "KV policy audit entry does not match SQL write".to_string(),
                ));
            }
        }

        Ok(id)
    }

    /// Verify policy audit chain integrity
    ///
    /// Validates that the policy audit chain is intact by checking:
    /// 1. Each entry's hash matches its computed hash
    /// 2. Each entry's previous_hash matches the prior entry's entry_hash
    /// 3. Chain sequence numbers are monotonically increasing within each tenant
    ///
    /// # Arguments
    /// * `tenant_id` - Optional tenant ID to verify chain for specific tenant (None = all tenants)
    ///
    /// # Returns
    /// ChainVerificationResult with validation status and details
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// // Verify chain for specific tenant
    /// let result = db.verify_policy_audit_chain(Some("tenant-123")).await?;
    /// if !result.is_valid {
    ///     eprintln!("Chain integrity violation at sequence {}", result.first_invalid_sequence.unwrap());
    /// }
    ///
    /// // Verify all tenant chains
    /// let all_result = db.verify_policy_audit_chain(None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_policy_audit_chain(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<ChainVerificationResult> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_policy_audit_kv_repo() {
                let res = repo.verify_policy_audit_chain(tenant_id).await?;
                if !self.storage_mode().sql_fallback_enabled() || !res.is_valid {
                    return Ok(res);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(ChainVerificationResult {
                is_valid: true,
                entries_checked: 0,
                first_invalid_sequence: None,
                error_message: None,
                divergence_detected: false,
                tenant_id: tenant_id.map(|s| s.to_string()),
                duration_ms: 0,
            });
        }

        // Build query with optional tenant filter
        let query = if tenant_id.is_some() {
            "SELECT id, tenant_id, policy_pack_id, hook, decision, reason, request_id, user_id,
                    resource_type, resource_id, metadata_json, timestamp, entry_hash, previous_hash, chain_sequence
             FROM policy_audit_decisions
             WHERE tenant_id = ?
             ORDER BY chain_sequence ASC"
        } else {
            "SELECT id, tenant_id, policy_pack_id, hook, decision, reason, request_id, user_id,
                    resource_type, resource_id, metadata_json, timestamp, entry_hash, previous_hash, chain_sequence
             FROM policy_audit_decisions
             ORDER BY tenant_id, chain_sequence ASC"
        };

        let decisions = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, PolicyAuditDecision>(query)
                .bind(tid)
                .fetch_all(self.pool())
                .await
                .db_err("fetch policy audit decisions")?
        } else {
            sqlx::query_as::<_, PolicyAuditDecision>(query)
                .fetch_all(self.pool())
                .await
                .db_err("fetch policy audit decisions")?
        };

        if decisions.is_empty() {
            return Ok(ChainVerificationResult {
                is_valid: true,
                entries_checked: 0,
                first_invalid_sequence: None,
                error_message: None,
                divergence_detected: false,
                tenant_id: tenant_id.map(|s| s.to_string()),
                duration_ms: 0,
            });
        }

        // Group by tenant for per-tenant chain validation
        let mut per_tenant_chains: std::collections::HashMap<String, Vec<&PolicyAuditDecision>> =
            std::collections::HashMap::new();

        for decision in &decisions {
            per_tenant_chains
                .entry(decision.tenant_id.clone())
                .or_default()
                .push(decision);
        }

        // Verify each tenant's chain independently
        let mut total_checked = 0;
        for (tenant, chain) in per_tenant_chains {
            let mut prev_hash: Option<String> = None;
            let mut prev_seq = 0i64;

            for decision in chain {
                total_checked += 1;

                // Check sequence monotonicity
                if decision.chain_sequence != prev_seq + 1 {
                    tracing::error!(
                        tenant_id = %tenant,
                        decision_id = %decision.id,
                        expected_seq = prev_seq + 1,
                        actual_seq = decision.chain_sequence,
                        "Policy audit chain sequence gap detected"
                    );
                    return Ok(ChainVerificationResult {
                        is_valid: false,
                        entries_checked: total_checked,
                        first_invalid_sequence: Some(decision.chain_sequence),
                        error_message: Some(format!(
                            "Sequence gap: expected {}, got {}",
                            prev_seq + 1,
                            decision.chain_sequence
                        )),
                        divergence_detected: true,
                        tenant_id: Some(tenant.clone()),
                        duration_ms: 0,
                    });
                }

                // Check previous_hash linkage
                if let Some(ref expected_prev) = prev_hash {
                    if decision.previous_hash.as_deref() != Some(expected_prev) {
                        tracing::error!(
                            tenant_id = %tenant,
                            decision_id = %decision.id,
                            expected_prev_hash = %expected_prev,
                            actual_prev_hash = ?decision.previous_hash,
                            "Policy audit chain previous_hash mismatch"
                        );
                        return Ok(ChainVerificationResult {
                            is_valid: false,
                            entries_checked: total_checked,
                            first_invalid_sequence: Some(decision.chain_sequence),
                            error_message: Some("Previous hash mismatch".to_string()),
                            divergence_detected: true,
                            tenant_id: Some(tenant.clone()),
                            duration_ms: 0,
                        });
                    }
                } else if decision.previous_hash.is_some() {
                    tracing::error!(
                        tenant_id = %tenant,
                        decision_id = %decision.id,
                        "First policy audit decision should have NULL previous_hash"
                    );
                    return Ok(ChainVerificationResult {
                        is_valid: false,
                        entries_checked: total_checked,
                        first_invalid_sequence: Some(decision.chain_sequence),
                        error_message: Some("First entry has non-null previous_hash".to_string()),
                        divergence_detected: true,
                        tenant_id: Some(tenant.clone()),
                        duration_ms: 0,
                    });
                }

                // Recompute entry hash and verify
                let entry_data = format!(
                    "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                    decision.id,
                    decision.timestamp,
                    decision.tenant_id,
                    decision.policy_pack_id,
                    decision.hook,
                    decision.decision,
                    decision.reason.as_deref().unwrap_or(""),
                    decision.request_id.as_deref().unwrap_or(""),
                    decision.user_id.as_deref().unwrap_or(""),
                    decision.resource_type.as_deref().unwrap_or(""),
                    decision.resource_id.as_deref().unwrap_or(""),
                    decision.metadata_json.as_deref().unwrap_or(""),
                    decision.previous_hash.as_deref().unwrap_or(""),
                );
                let computed_hash = adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string();

                if computed_hash != decision.entry_hash {
                    tracing::error!(
                        tenant_id = %tenant,
                        decision_id = %decision.id,
                        computed_hash = %computed_hash,
                        stored_hash = %decision.entry_hash,
                        "Policy audit entry hash mismatch - possible tampering"
                    );
                    return Ok(ChainVerificationResult {
                        is_valid: false,
                        entries_checked: total_checked,
                        first_invalid_sequence: Some(decision.chain_sequence),
                        error_message: Some("Entry hash mismatch - possible tampering".to_string()),
                        divergence_detected: true,
                        tenant_id: Some(tenant.clone()),
                        duration_ms: 0,
                    });
                }

                // Update for next iteration
                prev_hash = Some(decision.entry_hash.clone());
                prev_seq = decision.chain_sequence;
            }
        }

        Ok(ChainVerificationResult {
            is_valid: true,
            entries_checked: total_checked,
            first_invalid_sequence: None,
            error_message: None,
            divergence_detected: false,
            tenant_id: tenant_id.map(|s| s.to_string()),
            duration_ms: 0,
        })
    }

    /// Verify policy audit chains for all tenants
    ///
    /// Returns a map of tenant_id -> verification result, continuing to check
    /// all tenants even if some have diverged. This allows operators to see
    /// the full scope of any chain integrity issues across the system.
    ///
    /// # Returns
    /// BTreeMap where keys are tenant IDs and values are verification results
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let results = db.verify_all_policy_audit_chains().await?;
    /// for (tenant_id, result) in &results {
    ///     if result.divergence_detected {
    ///         eprintln!("ALERT: Tenant {} has divergent audit chain at sequence {}",
    ///             tenant_id, result.first_invalid_sequence.unwrap_or(0));
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_all_policy_audit_chains(
        &self,
    ) -> Result<std::collections::BTreeMap<String, ChainVerificationResult>> {
        use std::collections::BTreeMap;

        // Get distinct tenant IDs
        let tenants: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT tenant_id FROM policy_audit_decisions ORDER BY tenant_id",
        )
        .fetch_all(self.pool())
        .await
        .db_err("fetch distinct tenant IDs for policy audit")?;

        let mut results = BTreeMap::new();

        for tenant_id in tenants {
            let start = std::time::Instant::now();
            let mut result = self.verify_policy_audit_chain(Some(&tenant_id)).await?;
            result.duration_ms = start.elapsed().as_millis() as u64;
            result.tenant_id = Some(tenant_id.clone());

            if result.divergence_detected {
                tracing::error!(
                    tenant_id = %tenant_id,
                    first_invalid_sequence = ?result.first_invalid_sequence,
                    error_message = ?result.error_message,
                    "Policy audit chain divergence detected"
                );
            }

            results.insert(tenant_id, result);
        }

        Ok(results)
    }

    /// Query policy decisions with filters
    ///
    /// Returns policy audit decisions matching the provided filters.
    /// All filters are optional and combined with AND logic.
    ///
    /// # Arguments
    /// * `filters` - Query filters (tenant_id, policy_pack_id, hook, decision, time range, pagination)
    ///
    /// # Returns
    /// Vector of matching policy audit decisions, ordered by timestamp descending
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::{Db, PolicyDecisionFilters};
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let filters = PolicyDecisionFilters {
    ///     tenant_id: Some("tenant-123".to_string()),
    ///     decision: Some("deny".to_string()),
    ///     from_time: Some("2025-01-01T00:00:00Z".to_string()),
    ///     limit: Some(100),
    ///     ..Default::default()
    /// };
    ///
    /// let decisions = db.query_policy_decisions(filters).await?;
    /// for decision in decisions {
    ///     println!("Policy {} {} at {}: {}",
    ///         decision.policy_pack_id,
    ///         decision.decision,
    ///         decision.hook,
    ///         decision.reason.unwrap_or_default()
    ///     );
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn query_policy_decisions(
        &self,
        filters: PolicyDecisionFilters,
    ) -> Result<Vec<PolicyAuditDecision>> {
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_policy_audit_kv_repo() {
                let res = repo.query_policy_decisions(filters.clone()).await?;
                if !self.storage_mode().sql_fallback_enabled() || !res.is_empty() {
                    return Ok(res);
                }
            }
        }

        if !self.storage_mode().read_from_sql() {
            return Ok(Vec::new());
        }

        // Enforce maximum limit
        let limit = filters.limit.unwrap_or(100).min(1000);

        // Use FilterBuilder to construct dynamic query
        let mut builder = FilterBuilder::new(
            "SELECT id, tenant_id, policy_pack_id, hook, decision, reason, request_id, user_id,
                    resource_type, resource_id, metadata_json, timestamp, entry_hash, previous_hash, chain_sequence
             FROM policy_audit_decisions WHERE 1=1",
        );

        // Apply optional filters
        if let Some(tid) = &filters.tenant_id {
            builder.push_str(" AND tenant_id = ?");
            builder.add_param(tid);
        }

        if let Some(pid) = &filters.policy_pack_id {
            builder.push_str(" AND policy_pack_id = ?");
            builder.add_param(pid);
        }

        if let Some(hook) = &filters.hook {
            builder.push_str(" AND hook = ?");
            builder.add_param(hook);
        }

        if let Some(decision) = &filters.decision {
            builder.push_str(" AND decision = ?");
            builder.add_param(decision);
        }

        // Handle timestamp filters with custom operators
        if let Some(from) = &filters.from_time {
            builder.push_str(" AND timestamp >= ?");
            builder.add_param(from);
        }
        if let Some(to) = &filters.to_time {
            builder.push_str(" AND timestamp <= ?");
            builder.add_param(to);
        }

        builder.push_str(" ORDER BY timestamp DESC LIMIT ?");
        builder.add_param(limit);

        if let Some(offset) = filters.offset {
            builder.push_str(" OFFSET ?");
            builder.add_param(offset);
        }

        // Build and execute query
        let mut q = sqlx::query_as::<_, PolicyAuditDecision>(builder.query());
        for param in builder.params() {
            q = q.bind(param);
        }

        let decisions = q
            .fetch_all(self.pool())
            .await
            .map_err(db_err("query policy decisions"))?;
        Ok(decisions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv_backend::KvDb;
    use crate::StorageMode;

    async fn setup_test_db() -> Db {
        let mut db = Db::new_in_memory()
            .await
            .expect("Failed to create in-memory database");
        let kv = KvDb::init_in_memory().expect("Failed to init in-memory KV");
        db.attach_kv_backend(kv);
        db.set_storage_mode(StorageMode::DualWrite)
            .expect("Failed to set storage mode");
        db
    }

    #[tokio::test]
    async fn test_policy_decision_creation() {
        let db = setup_test_db().await;

        let id = db
            .log_policy_decision(
                "tenant-a",
                "router-policy-v1",
                "adapter.load",
                "allow",
                Some("Within memory budget"),
                Some("req-123"),
                Some("user-456"),
                Some("adapter"),
                Some("adapter-xyz"),
                Some(r#"{"memory_mb":512}"#),
            )
            .await
            .unwrap();

        assert!(!id.is_empty());

        // Query back
        let filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            ..Default::default()
        };
        let decisions = db.query_policy_decisions(filters).await.unwrap();

        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].policy_pack_id, "router-policy-v1");
        assert_eq!(decisions[0].hook, "adapter.load");
        assert_eq!(decisions[0].decision, "allow");
        assert_eq!(decisions[0].chain_sequence, 1);
    }

    #[tokio::test]
    async fn test_chain_linkage() {
        let db = setup_test_db().await;

        // Create multiple decisions
        let id1 = db
            .log_policy_decision(
                "tenant-a",
                "policy-1",
                "adapter.register",
                "allow",
                Some("First decision"),
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let id2 = db
            .log_policy_decision(
                "tenant-a",
                "policy-2",
                "adapter.load",
                "deny",
                Some("Second decision"),
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        // Verify chain linkage
        let filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            ..Default::default()
        };
        let decisions = db.query_policy_decisions(filters).await.unwrap();

        assert_eq!(decisions.len(), 2);

        // Find decisions by ID (query returns in DESC order by timestamp)
        let dec1 = decisions.iter().find(|d| d.id == id1).unwrap();
        let dec2 = decisions.iter().find(|d| d.id == id2).unwrap();

        assert_eq!(dec1.chain_sequence, 1);
        assert!(dec1.previous_hash.is_none());

        assert_eq!(dec2.chain_sequence, 2);
        assert_eq!(dec2.previous_hash.as_ref().unwrap(), &dec1.entry_hash);
    }

    #[tokio::test]
    async fn test_chain_verification() {
        let db = setup_test_db().await;

        // Create chain of decisions
        for i in 0..5 {
            db.log_policy_decision(
                "tenant-a",
                &format!("policy-{}", i),
                "adapter.load",
                if i % 2 == 0 { "allow" } else { "deny" },
                Some(&format!("Decision {}", i)),
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Verify chain integrity
        let result = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();

        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 5);
        assert!(result.first_invalid_sequence.is_none());
    }

    #[tokio::test]
    async fn test_multi_tenant_chains() {
        let db = setup_test_db().await;

        // Create decisions for two different tenants
        db.log_policy_decision(
            "tenant-a",
            "policy-1",
            "adapter.load",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        db.log_policy_decision(
            "tenant-b",
            "policy-1",
            "adapter.load",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        db.log_policy_decision(
            "tenant-a",
            "policy-2",
            "training.start",
            "deny",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // Each tenant should have independent chains
        let result_a = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(result_a.is_valid);
        assert_eq!(result_a.entries_checked, 2);

        let result_b = db
            .verify_policy_audit_chain(Some("tenant-b"))
            .await
            .unwrap();
        assert!(result_b.is_valid);
        assert_eq!(result_b.entries_checked, 1);

        // Verify all chains together
        let result_all = db.verify_policy_audit_chain(None).await.unwrap();
        assert!(result_all.is_valid);
        assert_eq!(result_all.entries_checked, 3);
    }

    #[tokio::test]
    async fn test_query_filters() {
        let db = setup_test_db().await;

        // Create variety of decisions
        db.log_policy_decision(
            "tenant-a",
            "router-policy",
            "adapter.load",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        db.log_policy_decision(
            "tenant-a",
            "memory-policy",
            "adapter.load",
            "deny",
            Some("Out of memory"),
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        db.log_policy_decision(
            "tenant-a",
            "router-policy",
            "training.start",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // Filter by decision type
        let deny_filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            decision: Some("deny".to_string()),
            ..Default::default()
        };
        let denies = db.query_policy_decisions(deny_filters).await.unwrap();
        assert_eq!(denies.len(), 1);
        assert_eq!(denies[0].policy_pack_id, "memory-policy");

        // Filter by hook
        let load_filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            hook: Some("adapter.load".to_string()),
            ..Default::default()
        };
        let loads = db.query_policy_decisions(load_filters).await.unwrap();
        assert_eq!(loads.len(), 2);

        // Filter by policy pack
        let router_filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            policy_pack_id: Some("router-policy".to_string()),
            ..Default::default()
        };
        let router_decisions = db.query_policy_decisions(router_filters).await.unwrap();
        assert_eq!(router_decisions.len(), 2);
    }

    #[tokio::test]
    async fn test_blake3_hash_determinism() {
        let db = setup_test_db().await;

        // Create a decision
        let id = db
            .log_policy_decision(
                "tenant-a",
                "policy-1",
                "adapter.load",
                "allow",
                Some("Test reason"),
                Some("req-123"),
                Some("user-456"),
                Some("adapter"),
                Some("adapter-xyz"),
                Some(r#"{"test":"data"}"#),
            )
            .await
            .unwrap();

        // Retrieve the decision
        let filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            ..Default::default()
        };
        let decisions = db.query_policy_decisions(filters).await.unwrap();
        assert_eq!(decisions.len(), 1);

        let decision = &decisions[0];

        // Recompute hash and verify it matches
        let recomputed = Db::compute_policy_entry_hash(decision);
        assert_eq!(recomputed, decision.entry_hash);

        // Create second decision and verify its hash includes previous hash
        let _id2 = db
            .log_policy_decision(
                "tenant-a",
                "policy-2",
                "adapter.unload",
                "deny",
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let filters2 = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            ..Default::default()
        };
        let decisions2 = db.query_policy_decisions(filters2).await.unwrap();
        assert_eq!(decisions2.len(), 2);

        // Find second decision
        let dec2 = decisions2.iter().find(|d| d.chain_sequence == 2).unwrap();
        assert_eq!(dec2.previous_hash.as_ref().unwrap(), &decision.entry_hash);

        // Verify second decision hash includes first hash in its computation
        let recomputed2 = Db::compute_policy_entry_hash(dec2);
        assert_eq!(recomputed2, dec2.entry_hash);
    }

    #[tokio::test]
    async fn test_chain_integrity_with_tampering() {
        let db = setup_test_db().await;

        // Create a chain
        for i in 0..3 {
            db.log_policy_decision(
                "tenant-a",
                &format!("policy-{}", i),
                "adapter.load",
                "allow",
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Verify chain is valid
        let result = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 3);

        // Tamper with middle entry by directly modifying database
        sqlx::query(
            "UPDATE policy_audit_decisions
             SET reason = 'tampered'
             WHERE chain_sequence = 2 AND tenant_id = ?",
        )
        .bind("tenant-a")
        .execute(db.pool())
        .await
        .unwrap();

        // Verification should now fail
        let result2 = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(!result2.is_valid);
        assert_eq!(result2.first_invalid_sequence, Some(2));
        assert!(result2
            .error_message
            .as_ref()
            .unwrap()
            .contains("tampering"));
    }

    #[tokio::test]
    async fn test_chain_linkage_break_detection() {
        let db = setup_test_db().await;

        // Create initial chain
        for i in 0..3 {
            db.log_policy_decision(
                "tenant-a",
                &format!("policy-{}", i),
                "adapter.load",
                "allow",
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Break the chain by corrupting previous_hash
        sqlx::query(
            "UPDATE policy_audit_decisions
             SET previous_hash = 'corrupted-hash'
             WHERE chain_sequence = 3 AND tenant_id = ?",
        )
        .bind("tenant-a")
        .execute(db.pool())
        .await
        .unwrap();

        // Verification should detect broken linkage
        let result = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(!result.is_valid);
        assert_eq!(result.first_invalid_sequence, Some(3));
        assert!(result
            .error_message
            .as_ref()
            .unwrap()
            .contains("Previous hash mismatch"));
    }

    #[tokio::test]
    async fn test_sequence_gap_detection() {
        let db = setup_test_db().await;

        // Create initial entries
        db.log_policy_decision(
            "tenant-a",
            "policy-1",
            "adapter.load",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        db.log_policy_decision(
            "tenant-a",
            "policy-2",
            "adapter.load",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // Manually create a gap by inserting entry with wrong sequence
        sqlx::query(
            "INSERT INTO policy_audit_decisions
             (id, tenant_id, policy_pack_id, hook, decision, timestamp, entry_hash, previous_hash, chain_sequence)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id(IdPrefix::Aud))
        .bind("tenant-a")
        .bind("policy-gap")
        .bind("adapter.load")
        .bind("allow")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind("fake-hash")
        .bind("fake-prev")
        .bind(5i64) // Gap: jumping from 2 to 5
        .execute(db.pool())
        .await
        .unwrap();

        // Verification should detect the gap
        let result = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(!result.is_valid);
        assert_eq!(result.first_invalid_sequence, Some(5));
        assert!(result
            .error_message
            .as_ref()
            .unwrap()
            .contains("Sequence gap"));
    }

    #[tokio::test]
    async fn test_first_entry_must_have_null_previous_hash() {
        let db = setup_test_db().await;

        // Create first entry normally
        db.log_policy_decision(
            "tenant-a",
            "policy-1",
            "adapter.load",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // Verify it has null previous_hash
        let filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            ..Default::default()
        };
        let decisions = db.query_policy_decisions(filters).await.unwrap();
        assert_eq!(decisions.len(), 1);
        assert!(decisions[0].previous_hash.is_none());

        // Tamper by adding a previous_hash to first entry
        sqlx::query(
            "UPDATE policy_audit_decisions
             SET previous_hash = 'should-be-null'
             WHERE chain_sequence = 1 AND tenant_id = ?",
        )
        .bind("tenant-a")
        .execute(db.pool())
        .await
        .unwrap();

        // Verification should fail
        let result = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(!result.is_valid);
        assert!(result
            .error_message
            .as_ref()
            .unwrap()
            .contains("non-null previous_hash"));
    }

    #[tokio::test]
    async fn test_hash_computation_includes_all_fields() {
        let db = setup_test_db().await;

        // Create decision with all fields populated
        db.log_policy_decision(
            "tenant-a",
            "policy-1",
            "adapter.load",
            "allow",
            Some("reason"),
            Some("req-123"),
            Some("user-456"),
            Some("adapter"),
            Some("adapter-xyz"),
            Some(r#"{"meta":"data"}"#),
        )
        .await
        .unwrap();

        let filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            ..Default::default()
        };
        let decisions = db.query_policy_decisions(filters).await.unwrap();
        let decision = &decisions[0];

        // Compute hash manually
        let entry_data = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            decision.id,
            decision.timestamp,
            decision.tenant_id,
            decision.policy_pack_id,
            decision.hook,
            decision.decision,
            decision.reason.as_deref().unwrap_or(""),
            decision.request_id.as_deref().unwrap_or(""),
            decision.user_id.as_deref().unwrap_or(""),
            decision.resource_type.as_deref().unwrap_or(""),
            decision.resource_id.as_deref().unwrap_or(""),
            decision.metadata_json.as_deref().unwrap_or(""),
            decision.previous_hash.as_deref().unwrap_or(""),
        );
        let computed = adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string();
        assert_eq!(computed, decision.entry_hash);

        // Modify any field should invalidate hash
        let mut tampered_decision = decision.clone();
        tampered_decision.reason = Some("modified".to_string());
        let tampered_hash = Db::compute_policy_entry_hash(&tampered_decision);
        assert_ne!(tampered_hash, decision.entry_hash);
    }

    #[tokio::test]
    async fn test_tenant_isolation_in_chains() {
        let db = setup_test_db().await;

        // Create chains for two tenants
        for i in 0..3 {
            db.log_policy_decision(
                "tenant-a",
                &format!("policy-a-{}", i),
                "adapter.load",
                "allow",
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        for i in 0..3 {
            db.log_policy_decision(
                "tenant-b",
                &format!("policy-b-{}", i),
                "adapter.load",
                "allow",
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Each tenant should have independent sequences starting at 1
        let filters_a = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            ..Default::default()
        };
        let decisions_a = db.query_policy_decisions(filters_a).await.unwrap();
        assert_eq!(decisions_a.len(), 3);
        assert!(decisions_a.iter().any(|d| d.chain_sequence == 1));
        assert!(decisions_a.iter().any(|d| d.chain_sequence == 2));
        assert!(decisions_a.iter().any(|d| d.chain_sequence == 3));

        let filters_b = PolicyDecisionFilters {
            tenant_id: Some("tenant-b".to_string()),
            ..Default::default()
        };
        let decisions_b = db.query_policy_decisions(filters_b).await.unwrap();
        assert_eq!(decisions_b.len(), 3);
        assert!(decisions_b.iter().any(|d| d.chain_sequence == 1));
        assert!(decisions_b.iter().any(|d| d.chain_sequence == 2));
        assert!(decisions_b.iter().any(|d| d.chain_sequence == 3));

        // Verify chains independently
        let result_a = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(result_a.is_valid);
        assert_eq!(result_a.entries_checked, 3);

        let result_b = db
            .verify_policy_audit_chain(Some("tenant-b"))
            .await
            .unwrap();
        assert!(result_b.is_valid);
        assert_eq!(result_b.entries_checked, 3);
    }

    #[tokio::test]
    async fn test_force_corrupt_tail() {
        let db = setup_test_db().await;

        // Create chain
        for i in 0..3 {
            db.log_policy_decision(
                "tenant-a",
                &format!("policy-{}", i),
                "adapter.load",
                "allow",
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Verify chain is valid
        let result = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(result.is_valid);

        // Force corrupt the tail
        let (corrupted_id, original_hash, seq) = db
            .force_corrupt_policy_audit_tail("tenant-a")
            .await
            .unwrap();
        assert_eq!(seq, 3);
        assert!(!original_hash.is_empty());
        assert!(!corrupted_id.is_empty());

        // Verification should now fail
        let result2 = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(!result2.is_valid);
    }

    #[tokio::test]
    async fn test_validate_tail_detects_corruption() {
        let db = setup_test_db().await;

        // Create chain
        for i in 0..3 {
            db.log_policy_decision(
                "tenant-a",
                &format!("policy-{}", i),
                "adapter.load",
                "allow",
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Validate tail should succeed
        let tail = db.validate_policy_audit_tail("tenant-a").await.unwrap();
        assert!(tail.is_some());
        let (hash, seq) = tail.unwrap();
        assert_eq!(seq, 3);
        assert!(!hash.is_empty());

        // Corrupt the tail
        db.force_corrupt_policy_audit_tail("tenant-a")
            .await
            .unwrap();

        // Validate tail should now fail
        let result = db.validate_policy_audit_tail("tenant-a").await;
        assert!(result.is_err());
        assert!(is_audit_chain_divergence(&result.unwrap_err()));
    }

    #[tokio::test]
    async fn test_empty_chain_verification() {
        let db = setup_test_db().await;

        // Verify empty chain should succeed
        let result = db
            .verify_policy_audit_chain(Some("tenant-empty"))
            .await
            .unwrap();
        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 0);
        assert!(result.first_invalid_sequence.is_none());
        assert!(result.error_message.is_none());
    }

    #[tokio::test]
    async fn test_single_entry_chain() {
        let db = setup_test_db().await;

        // Create single entry
        db.log_policy_decision(
            "tenant-a",
            "policy-1",
            "adapter.load",
            "allow",
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // Verify single entry chain
        let result = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 1);

        // Check it has correct properties
        let filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            ..Default::default()
        };
        let decisions = db.query_policy_decisions(filters).await.unwrap();
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].chain_sequence, 1);
        assert!(decisions[0].previous_hash.is_none());
        assert!(!decisions[0].entry_hash.is_empty());
    }

    #[tokio::test]
    async fn test_large_chain_verification() {
        let db = setup_test_db().await;

        // Create a large chain
        for i in 0..100 {
            db.log_policy_decision(
                "tenant-a",
                &format!("policy-{}", i),
                "adapter.load",
                if i % 2 == 0 { "allow" } else { "deny" },
                Some(&format!("Decision {}", i)),
                None,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Verify entire chain
        let result = db
            .verify_policy_audit_chain(Some("tenant-a"))
            .await
            .unwrap();
        assert!(result.is_valid);
        assert_eq!(result.entries_checked, 100);

        // Verify linkage is correct throughout
        let filters = PolicyDecisionFilters {
            tenant_id: Some("tenant-a".to_string()),
            limit: Some(1000),
            ..Default::default()
        };
        let decisions = db.query_policy_decisions(filters).await.unwrap();
        assert_eq!(decisions.len(), 100);

        // Check first and last entries
        let first = decisions.iter().find(|d| d.chain_sequence == 1).unwrap();
        let last = decisions.iter().find(|d| d.chain_sequence == 100).unwrap();

        assert!(first.previous_hash.is_none());
        assert!(last.previous_hash.is_some());
    }
}
