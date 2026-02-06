//! Audit logging for RBAC compliance and security
//!
//! All sensitive operations are logged to the audit_logs table for compliance review.
//! Audit logs are immutable and queryable for compliance officers and administrators.
#![allow(deprecated)]

use crate::query_helpers::{db_err, FilterBuilder};
use crate::Db;
use adapteros_core::error_helpers::DbErrorExt;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use crate::new_id;
use adapteros_id::IdPrefix;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditLog {
    pub id: String,
    pub timestamp: String,
    pub user_id: String,
    pub user_role: String,
    pub tenant_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub ip_address: Option<String>,
    pub metadata_json: Option<String>,
}

/// Statistics about inference abstention for AARA lifecycle monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceAbstentionStats {
    /// Total number of inference requests
    pub total_inferences: usize,
    /// Number of requests that resulted in abstention
    pub abstained_count: usize,
    /// Abstention rate (0.0 to 1.0)
    pub abstention_rate: f32,
    /// Average confidence score across all inferences
    pub avg_confidence: Option<f32>,
}

impl Db {
    /// Log an audit event
    ///
    /// # Arguments
    /// * `user_id` - User who performed the action
    /// * `user_role` - Role of the user (admin, operator, sre, compliance, viewer)
    /// * `tenant_id` - Tenant context
    /// * `action` - Action performed (e.g., "adapter.register", "training.start")
    /// * `resource_type` - Type of resource (e.g., "adapter", "policy", "tenant")
    /// * `resource_id` - ID of the resource acted upon
    /// * `status` - "success" or "failure"
    /// * `error_message` - Error details if status = "failure"
    /// * `ip_address` - Client IP address (optional)
    /// * `metadata_json` - Additional context as JSON (optional)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// db.log_audit(
    ///     "user-123",
    ///     "admin",
    ///     "tenant-a",
    ///     "adapter.delete",
    ///     "adapter",
    ///     Some("adapter-xyz"),
    ///     "success",
    ///     None,
    ///     Some("192.168.1.100"),
    ///     None,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn log_audit(
        &self,
        user_id: &str,
        user_role: &str,
        tenant_id: &str,
        action: &str,
        resource_type: &str,
        resource_id: Option<&str>,
        status: &str,
        error_message: Option<&str>,
        ip_address: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<String> {
        let id = new_id(IdPrefix::Aud);
        let timestamp = chrono::Utc::now().to_rfc3339();

        // Get the latest audit log entry to link to it (chain-of-custody)
        let latest_entry = sqlx::query_as::<_, (Option<String>, Option<i64>)>(
            "SELECT entry_hash, chain_sequence FROM audit_logs
             WHERE tenant_id = ?
             ORDER BY chain_sequence DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        let (previous_hash, chain_sequence) = match latest_entry {
            Some((hash_opt, seq_opt)) => {
                let prev_hash = hash_opt.unwrap_or_default();
                let next_seq = seq_opt.unwrap_or(0) + 1;
                (Some(prev_hash), next_seq)
            }
            None => {
                // First entry in the chain
                (None, 1)
            }
        };

        // Compute hash of this entry (deterministic)
        let entry_data = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            id,
            timestamp,
            user_id,
            user_role,
            tenant_id,
            action,
            resource_type,
            resource_id.unwrap_or(""),
            status,
            error_message.unwrap_or(""),
            ip_address.unwrap_or(""),
            previous_hash.as_deref().unwrap_or(""),
        );
        let entry_hash = adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string();

        sqlx::query(
            "INSERT INTO audit_logs
             (id, timestamp, user_id, user_role, tenant_id, action, resource_type, resource_id,
              status, error_message, ip_address, metadata_json, previous_hash, entry_hash, chain_sequence)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&timestamp)
        .bind(user_id)
        .bind(user_role)
        .bind(tenant_id)
        .bind(action)
        .bind(resource_type)
        .bind(resource_id)
        .bind(status)
        .bind(error_message)
        .bind(ip_address)
        .bind(metadata_json)
        .bind(previous_hash.as_deref())
        .bind(&entry_hash)
        .bind(chain_sequence)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(id)
    }

    // =========================================================================
    // AARA Lifecycle: ACT Phase - Inference Decision Audit
    // =========================================================================

    /// Log an inference decision for the AARA lifecycle audit trail
    ///
    /// This logs all inference decisions including:
    /// - Which adapters were considered and selected
    /// - Gate scores for each adapter
    /// - Whether abstention occurred
    /// - Confidence levels
    ///
    /// # Arguments
    /// * `request_id` - Unique inference request ID
    /// * `tenant_id` - Tenant context
    /// * `user_id` - User who made the request
    /// * `adapters_considered` - All adapters that were candidates
    /// * `adapters_selected` - Adapters that were actually used
    /// * `max_gate_score` - Maximum gate score observed
    /// * `abstained` - Whether the system abstained from answering
    /// * `abstention_reason` - Reason for abstention (if abstained)
    /// * `latency_ms` - Total inference latency
    pub async fn log_inference_decision(
        &self,
        request_id: &str,
        tenant_id: &str,
        user_id: &str,
        adapters_considered: &[String],
        adapters_selected: &[String],
        max_gate_score: f32,
        abstained: bool,
        abstention_reason: Option<&str>,
        latency_ms: u64,
    ) -> Result<String> {
        // Build metadata JSON for the inference decision
        let metadata = serde_json::json!({
            "adapters_considered": adapters_considered,
            "adapters_selected": adapters_selected,
            "max_gate_score": max_gate_score,
            "abstained": abstained,
            "abstention_reason": abstention_reason,
            "latency_ms": latency_ms,
            "aara_phase": "act",
        });

        let action = if abstained {
            "inference.abstain"
        } else {
            "inference.execute"
        };

        let status = if abstained { "abstained" } else { "success" };

        self.log_audit(
            user_id,
            "user", // Role not always known at inference time
            tenant_id,
            action,
            "inference",
            Some(request_id),
            status,
            abstention_reason,
            None,
            Some(&metadata.to_string()),
        )
        .await
    }

    /// Query inference decisions for audit analysis (AARA lifecycle)
    ///
    /// Returns all inference audit entries for a tenant within a time range.
    pub async fn query_inference_decisions(
        &self,
        tenant_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        abstained_only: bool,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        let limit = limit.clamp(1, 1000);

        let mut builder = FilterBuilder::new(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs
             WHERE tenant_id = ? AND resource_type = 'inference'",
        );
        builder.add_param(tenant_id);

        if abstained_only {
            builder.push_str(" AND status = 'abstained'");
        }

        if let Some(start) = start_date {
            builder.push_str(" AND timestamp >= ?");
            builder.add_param(start);
        }
        if let Some(end) = end_date {
            builder.push_str(" AND timestamp <= ?");
            builder.add_param(end);
        }

        builder.push_str(" ORDER BY timestamp DESC LIMIT ?");
        builder.add_param(limit);

        let mut q = sqlx::query_as::<_, AuditLog>(builder.query());
        for param in builder.params() {
            q = q.bind(param);
        }

        q.fetch_all(self.pool())
            .await
            .map_err(db_err("query inference decisions"))
    }

    /// Get inference abstention statistics for a tenant (AARA lifecycle)
    pub async fn get_inference_abstention_stats(
        &self,
        tenant_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<InferenceAbstentionStats> {
        let mut builder = FilterBuilder::new(
            "SELECT
                COUNT(*) as total,
                SUM(CASE WHEN status = 'abstained' THEN 1 ELSE 0 END) as abstained,
                AVG(CASE WHEN metadata_json IS NOT NULL
                    THEN CAST(json_extract(metadata_json, '$.max_gate_score') AS REAL)
                    ELSE NULL END) as avg_confidence
             FROM audit_logs
             WHERE tenant_id = ? AND resource_type = 'inference'",
        );
        builder.add_param(tenant_id);

        if let Some(start) = start_date {
            builder.push_str(" AND timestamp >= ?");
            builder.add_param(start);
        }
        if let Some(end) = end_date {
            builder.push_str(" AND timestamp <= ?");
            builder.add_param(end);
        }

        let mut q = sqlx::query(builder.query());
        for param in builder.params() {
            q = q.bind(param);
        }

        let row = q
            .fetch_one(self.pool())
            .await
            .map_err(db_err("get inference abstention stats"))?;

        use sqlx::Row;
        let total: i64 = row.try_get("total").unwrap_or(0);
        let abstained: i64 = row.try_get("abstained").unwrap_or(0);
        let avg_confidence: Option<f64> = row.try_get("avg_confidence").ok();

        Ok(InferenceAbstentionStats {
            total_inferences: total as usize,
            abstained_count: abstained as usize,
            abstention_rate: if total > 0 {
                abstained as f32 / total as f32
            } else {
                0.0
            },
            avg_confidence: avg_confidence.map(|c| c as f32),
        })
    }

    /// Query audit logs with filters (DEPRECATED - use query_audit_logs_for_tenant instead)
    ///
    /// WARNING: This method queries audit logs across ALL tenants without filtering.
    /// This breaks multi-tenant isolation and should only be used for system administration.
    ///
    /// For normal operations, use `query_audit_logs_for_tenant()` which enforces tenant isolation.
    ///
    /// # Arguments
    /// * `user_id` - Filter by user (optional)
    /// * `action` - Filter by action (optional)
    /// * `resource_type` - Filter by resource type (optional)
    /// * `start_date` - Filter by start date in RFC3339 format (optional)
    /// * `end_date` - Filter by end date in RFC3339 format (optional)
    /// * `limit` - Maximum number of results (default: 100, max: 1000)
    #[deprecated(
        since = "0.3.0",
        note = "Use query_audit_logs_for_tenant() for tenant isolation"
    )]
    pub async fn query_audit_logs(
        &self,
        user_id: Option<&str>,
        action: Option<&str>,
        resource_type: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        // Enforce maximum limit
        let limit = limit.min(1000);

        let mut builder = FilterBuilder::new(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs WHERE 1=1",
        );
        builder.add_filter("user_id", user_id);
        builder.add_filter("action", action);
        builder.add_filter("resource_type", resource_type);

        if let Some(start) = start_date {
            builder.push_str(" AND timestamp >= ?");
            builder.add_param(start);
        }
        if let Some(end) = end_date {
            builder.push_str(" AND timestamp <= ?");
            builder.add_param(end);
        }

        builder.push_str(" ORDER BY timestamp DESC LIMIT ?");
        builder.add_param(limit);

        let mut q = sqlx::query_as::<_, AuditLog>(builder.query());
        for param in builder.params() {
            q = q.bind(param);
        }

        let logs = q
            .fetch_all(self.pool())
            .await
            .map_err(db_err("query audit logs"))?;
        Ok(logs)
    }

    /// Query audit logs with filters for a specific tenant
    ///
    /// This is the RECOMMENDED method for querying audit logs as it enforces tenant isolation.
    /// Only returns audit logs belonging to the specified tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by (REQUIRED for tenant isolation)
    /// * `user_id` - Filter by user (optional)
    /// * `action` - Filter by action (optional)
    /// * `resource_type` - Filter by resource type (optional)
    /// * `start_date` - Filter by start date in RFC3339 format (optional)
    /// * `end_date` - Filter by end date in RFC3339 format (optional)
    /// * `limit` - Maximum number of results (default: 100, max: 1000)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let logs = db.query_audit_logs_for_tenant(
    ///     "tenant-123",
    ///     Some("user-456"),
    ///     Some("adapter.load"),
    ///     Some("adapter"),
    ///     Some("2024-01-01T00:00:00Z"),
    ///     Some("2024-12-31T23:59:59Z"),
    ///     100,
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn query_audit_logs_for_tenant(
        &self,
        tenant_id: &str,
        user_id: Option<&str>,
        action: Option<&str>,
        resource_type: Option<&str>,
        start_date: Option<&str>,
        end_date: Option<&str>,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        // Enforce maximum limit
        let limit = limit.min(1000);

        // Use FilterBuilder to construct dynamic query
        let mut builder = FilterBuilder::new(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs WHERE tenant_id = ?",
        );
        builder.add_param(tenant_id);
        builder.add_filter("user_id", user_id);
        builder.add_filter("action", action);
        builder.add_filter("resource_type", resource_type);

        // Handle timestamp filters with custom operators
        if let Some(start) = start_date {
            builder.push_str(" AND timestamp >= ?");
            builder.add_param(start);
        }
        if let Some(end) = end_date {
            builder.push_str(" AND timestamp <= ?");
            builder.add_param(end);
        }

        builder.push_str(" ORDER BY timestamp DESC LIMIT ?");
        builder.add_param(limit);

        // Build and execute query
        let mut q = sqlx::query_as::<_, AuditLog>(builder.query());
        for param in builder.params() {
            q = q.bind(param);
        }

        let logs = q
            .fetch_all(self.pool())
            .await
            .map_err(db_err("query audit logs for tenant"))?;
        Ok(logs)
    }

    /// Get audit logs for a specific resource (DEPRECATED - use get_resource_audit_trail_for_tenant instead)
    ///
    /// WARNING: This method queries audit logs across ALL tenants without filtering.
    /// This breaks multi-tenant isolation and should only be used for system administration.
    ///
    /// For normal operations, use `get_resource_audit_trail_for_tenant()` which enforces tenant isolation.
    ///
    /// # Arguments
    /// * `resource_type` - Type of resource (e.g., "adapter", "policy")
    /// * `resource_id` - ID of the resource
    /// * `limit` - Maximum number of results
    #[deprecated(
        since = "0.3.0",
        note = "Use get_resource_audit_trail_for_tenant() for tenant isolation"
    )]
    pub async fn get_resource_audit_trail(
        &self,
        resource_type: &str,
        resource_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        let limit = limit.min(1000);

        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs
             WHERE resource_type = ? AND resource_id = ?
             ORDER BY timestamp DESC
             LIMIT ?",
        )
        .bind(resource_type)
        .bind(resource_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        Ok(logs)
    }

    /// Get audit logs for a specific resource within a tenant
    ///
    /// This is the RECOMMENDED method for querying resource audit trails as it enforces tenant isolation.
    /// Only returns audit logs for the specified resource within the specified tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by (REQUIRED for tenant isolation)
    /// * `resource_type` - Type of resource (e.g., "adapter", "policy")
    /// * `resource_id` - ID of the resource
    /// * `limit` - Maximum number of results (capped at 1000)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let logs = db.get_resource_audit_trail_for_tenant(
    ///     "tenant-123",
    ///     "adapter",
    ///     "adapter-xyz",
    ///     50
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_resource_audit_trail_for_tenant(
        &self,
        tenant_id: &str,
        resource_type: &str,
        resource_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditLog>> {
        let limit = limit.min(1000);

        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs
             WHERE tenant_id = ? AND resource_type = ? AND resource_id = ?
             ORDER BY timestamp DESC
             LIMIT ?",
        )
        .bind(tenant_id)
        .bind(resource_type)
        .bind(resource_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to get resource audit trail for tenant: {}",
                e
            ))
        })?;

        Ok(logs)
    }

    /// Verify audit log chain integrity (DEPRECATED - use verify_audit_chain_for_tenant instead)
    ///
    /// WARNING: This method validates audit logs across ALL tenants without filtering.
    /// This breaks multi-tenant isolation and should only be used for system administration.
    ///
    /// For normal operations, use `verify_audit_chain_for_tenant()` which enforces tenant isolation.
    ///
    /// Validates that the audit log chain is intact by checking:
    /// 1. Each entry's hash matches its computed hash
    /// 2. Each entry's previous_hash matches the prior entry's entry_hash
    /// 3. Chain sequence numbers are monotonically increasing
    ///
    /// # Returns
    /// - Ok(true) if chain is valid
    /// - Ok(false) if chain has integrity issues
    /// - Err if database query fails
    #[deprecated(
        since = "0.3.0",
        note = "Use verify_audit_chain_for_tenant() for tenant isolation"
    )]
    pub async fn verify_audit_chain(&self) -> Result<bool> {
        // Fetch all audit logs ordered by chain_sequence
        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs
             ORDER BY chain_sequence ASC",
        )
        .fetch_all(self.pool())
        .await
        .db_err("fetch audit logs")?;

        if logs.is_empty() {
            return Ok(true); // Empty chain is valid
        }

        // Fetch chain metadata (previous_hash, entry_hash, chain_sequence)
        let chain_data = sqlx::query_as::<_, (String, Option<String>, String, i64)>(
            "SELECT id, previous_hash, entry_hash, chain_sequence
             FROM audit_logs
             ORDER BY chain_sequence ASC",
        )
        .fetch_all(self.pool())
        .await
        .db_err("fetch audit chain metadata")?;

        let mut prev_hash: Option<String> = None;
        let mut prev_seq = 0i64;

        for (idx, (log_id, stored_prev_hash, stored_entry_hash, seq)) in
            chain_data.iter().enumerate()
        {
            // Check sequence monotonicity
            if *seq != prev_seq + 1 {
                tracing::error!(
                    log_id = %log_id,
                    expected_seq = prev_seq + 1,
                    actual_seq = seq,
                    "Audit chain sequence gap detected"
                );
                return Ok(false);
            }

            // Check previous_hash linkage
            if let Some(ref expected_prev) = prev_hash {
                if stored_prev_hash.as_deref() != Some(expected_prev) {
                    tracing::error!(
                        log_id = %log_id,
                        expected_prev_hash = %expected_prev,
                        actual_prev_hash = ?stored_prev_hash,
                        "Audit chain previous_hash mismatch"
                    );
                    return Ok(false);
                }
            } else if stored_prev_hash.is_some() {
                tracing::error!(
                    log_id = %log_id,
                    "First audit log should have NULL previous_hash"
                );
                return Ok(false);
            }

            // Recompute entry hash and verify
            let log = &logs[idx];
            let entry_data = format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                log.id,
                log.timestamp,
                log.user_id,
                log.user_role,
                log.tenant_id,
                log.action,
                log.resource_type,
                log.resource_id.as_deref().unwrap_or(""),
                log.status,
                log.error_message.as_deref().unwrap_or(""),
                log.ip_address.as_deref().unwrap_or(""),
                stored_prev_hash.as_deref().unwrap_or(""),
            );
            let computed_hash = adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string();

            if &computed_hash != stored_entry_hash {
                tracing::error!(
                    log_id = %log.id,
                    computed_hash = %computed_hash,
                    stored_hash = %stored_entry_hash,
                    "Audit entry hash mismatch - possible tampering"
                );
                return Ok(false);
            }

            // Update for next iteration
            prev_hash = Some(stored_entry_hash.clone());
            prev_seq = *seq;
        }

        Ok(true)
    }

    /// Verify audit log chain integrity for a specific tenant
    ///
    /// This is the RECOMMENDED method for verifying audit chain integrity as it enforces tenant isolation.
    /// Only validates the audit log chain for the specified tenant.
    ///
    /// Validates that the audit log chain is intact by checking:
    /// 1. Each entry's hash matches its computed hash
    /// 2. Each entry's previous_hash matches the prior entry's entry_hash
    /// 3. Chain sequence numbers are monotonically increasing
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by (REQUIRED for tenant isolation)
    ///
    /// # Returns
    /// - Ok(true) if chain is valid for the tenant
    /// - Ok(false) if chain has integrity issues
    /// - Err if database query fails
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let is_valid = db.verify_audit_chain_for_tenant("tenant-123").await?;
    /// if !is_valid {
    ///     eprintln!("WARNING: Audit chain integrity violation detected for tenant!");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_audit_chain_for_tenant(&self, tenant_id: &str) -> Result<bool> {
        // Fetch audit logs for tenant ordered by chain_sequence
        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs
             WHERE tenant_id = ?
             ORDER BY chain_sequence ASC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(db_err("fetch audit logs for tenant"))?;

        if logs.is_empty() {
            return Ok(true); // Empty chain is valid
        }

        // Fetch chain metadata (previous_hash, entry_hash, chain_sequence)
        let chain_data = sqlx::query_as::<_, (String, Option<String>, String, i64)>(
            "SELECT id, previous_hash, entry_hash, chain_sequence
             FROM audit_logs
             WHERE tenant_id = ?
             ORDER BY chain_sequence ASC",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| {
            AosError::Database(format!(
                "Failed to fetch audit chain metadata for tenant: {}",
                e
            ))
        })?;

        let mut prev_hash: Option<String> = None;
        let mut prev_seq = 0i64;

        for (idx, (log_id, stored_prev_hash, stored_entry_hash, seq)) in
            chain_data.iter().enumerate()
        {
            // Check sequence monotonicity
            if *seq != prev_seq + 1 {
                tracing::error!(
                    tenant_id = %tenant_id,
                    log_id = %log_id,
                    expected_seq = prev_seq + 1,
                    actual_seq = seq,
                    "Audit chain sequence gap detected"
                );
                return Ok(false);
            }

            // Check previous_hash linkage
            if let Some(ref expected_prev) = prev_hash {
                if stored_prev_hash.as_deref() != Some(expected_prev) {
                    tracing::error!(
                        tenant_id = %tenant_id,
                        log_id = %log_id,
                        expected_prev_hash = %expected_prev,
                        actual_prev_hash = ?stored_prev_hash,
                        "Audit chain previous_hash mismatch"
                    );
                    return Ok(false);
                }
            } else if stored_prev_hash.is_some() {
                tracing::error!(
                    tenant_id = %tenant_id,
                    log_id = %log_id,
                    "First audit log should have NULL previous_hash"
                );
                return Ok(false);
            }

            // Recompute entry hash and verify
            let log = &logs[idx];
            let entry_data = format!(
                "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                log.id,
                log.timestamp,
                log.user_id,
                log.user_role,
                log.tenant_id,
                log.action,
                log.resource_type,
                log.resource_id.as_deref().unwrap_or(""),
                log.status,
                log.error_message.as_deref().unwrap_or(""),
                log.ip_address.as_deref().unwrap_or(""),
                stored_prev_hash.as_deref().unwrap_or(""),
            );
            let computed_hash = adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string();

            if &computed_hash != stored_entry_hash {
                tracing::error!(
                    tenant_id = %tenant_id,
                    log_id = %log.id,
                    computed_hash = %computed_hash,
                    stored_hash = %stored_entry_hash,
                    "Audit entry hash mismatch - possible tampering"
                );
                return Ok(false);
            }

            // Update for next iteration
            prev_hash = Some(stored_entry_hash.clone());
            prev_seq = *seq;
        }

        Ok(true)
    }

    /// Get audit log count by action (for compliance dashboard)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let stats = db.get_audit_stats_by_action(
    ///     Some("2024-01-01T00:00:00Z"),
    ///     Some("2024-12-31T23:59:59Z"),
    /// ).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_audit_stats_by_action(
        &self,
        start_date: Option<&str>,
        end_date: Option<&str>,
    ) -> Result<Vec<(String, i64)>> {
        let mut builder = FilterBuilder::new(
            "SELECT action, COUNT(*) as count
             FROM audit_logs
             WHERE 1=1",
        );

        if let Some(start) = start_date {
            builder.push_str(" AND timestamp >= ?");
            builder.add_param(start);
        }
        if let Some(end) = end_date {
            builder.push_str(" AND timestamp <= ?");
            builder.add_param(end);
        }

        builder.push_str(" GROUP BY action ORDER BY count DESC");

        let mut q = sqlx::query_as::<_, (String, i64)>(builder.query());
        for param in builder.params() {
            q = q.bind(param);
        }

        let stats = q
            .fetch_all(self.pool())
            .await
            .map_err(db_err("get audit stats by action"))?;
        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_test_db() -> Db {
        Db::new_in_memory()
            .await
            .expect("Failed to create in-memory database")
    }

    #[tokio::test]
    async fn test_audit_log_creation() {
        let db = setup_test_db().await;

        let id = db
            .log_audit(
                "user-123",
                "admin",
                "tenant-a",
                "adapter.register",
                "adapter",
                Some("adapter-xyz"),
                "success",
                None,
                Some("192.168.1.100"),
                Some(r#"{"extra":"metadata"}"#),
            )
            .await
            .unwrap();

        assert!(!id.is_empty());

        // Query back
        let logs = db
            .query_audit_logs(Some("user-123"), None, None, None, None, 10)
            .await
            .unwrap();

        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].action, "adapter.register");
        assert_eq!(logs[0].user_role, "admin");
        assert_eq!(logs[0].status, "success");
    }

    #[tokio::test]
    async fn test_resource_audit_trail() {
        let db = setup_test_db().await;

        // Create multiple audit logs for same resource
        db.log_audit(
            "user-1",
            "admin",
            "tenant-a",
            "adapter.register",
            "adapter",
            Some("adapter-xyz"),
            "success",
            None,
            None,
            None,
        )
        .await
        .unwrap();

        db.log_audit(
            "user-2",
            "operator",
            "tenant-a",
            "adapter.load",
            "adapter",
            Some("adapter-xyz"),
            "success",
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let trail = db
            .get_resource_audit_trail("adapter", "adapter-xyz", 10)
            .await
            .unwrap();

        assert_eq!(trail.len(), 2);
    }

    #[tokio::test]
    async fn test_blake3_audit_chain_integrity() {
        let db = setup_test_db().await;

        // Create chain of audit logs
        for i in 0..5 {
            db.log_audit(
                &format!("user-{}", i),
                "admin",
                "tenant-a",
                "adapter.register",
                "adapter",
                Some(&format!("adapter-{}", i)),
                "success",
                None,
                Some(&format!("192.168.1.{}", i)),
                None,
            )
            .await
            .unwrap();
        }

        // Verify chain is valid
        let is_valid = db.verify_audit_chain_for_tenant("tenant-a").await.unwrap();
        assert!(is_valid);
    }

    #[tokio::test]
    async fn test_audit_chain_with_tampering() {
        let db = setup_test_db().await;

        // Create chain
        for i in 0..3 {
            db.log_audit(
                "user-1",
                "admin",
                "tenant-a",
                &format!("action-{}", i),
                "adapter",
                Some("adapter-xyz"),
                "success",
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Verify chain is valid
        assert!(db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());

        // Tamper with middle entry
        sqlx::query(
            "UPDATE audit_logs
             SET action = 'tampered-action'
             WHERE chain_sequence = 2",
        )
        .execute(db.pool())
        .await
        .unwrap();

        // Verification should now fail
        assert!(!db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());
    }

    #[tokio::test]
    async fn test_audit_chain_linkage() {
        let db = setup_test_db().await;

        // Create multiple audit logs
        let id1 = db
            .log_audit(
                "user-1",
                "admin",
                "tenant-a",
                "adapter.register",
                "adapter",
                Some("adapter-1"),
                "success",
                None,
                None,
                None,
            )
            .await
            .unwrap();

        let id2 = db
            .log_audit(
                "user-1",
                "admin",
                "tenant-a",
                "adapter.load",
                "adapter",
                Some("adapter-1"),
                "success",
                None,
                None,
                None,
            )
            .await
            .unwrap();

        // Fetch chain metadata
        let chain_data = sqlx::query_as::<_, (String, Option<String>, String, i64)>(
            "SELECT id, previous_hash, entry_hash, chain_sequence
             FROM audit_logs
             ORDER BY chain_sequence ASC",
        )
        .fetch_all(db.pool())
        .await
        .unwrap();

        assert_eq!(chain_data.len(), 2);

        // First entry should have no previous hash
        let (first_id, first_prev, first_hash, first_seq) = &chain_data[0];
        assert_eq!(first_id, &id1);
        assert!(first_prev.is_none());
        assert_eq!(*first_seq, 1);

        // Second entry should link to first
        let (second_id, second_prev, _second_hash, second_seq) = &chain_data[1];
        assert_eq!(second_id, &id2);
        assert_eq!(second_prev.as_ref().unwrap(), first_hash);
        assert_eq!(*second_seq, 2);
    }

    #[tokio::test]
    async fn test_audit_hash_determinism() {
        let db = setup_test_db().await;

        // Create an audit log with all fields
        db.log_audit(
            "user-123",
            "admin",
            "tenant-a",
            "adapter.register",
            "adapter",
            Some("adapter-xyz"),
            "success",
            None,
            Some("192.168.1.100"),
            Some(r#"{"extra":"metadata"}"#),
        )
        .await
        .unwrap();

        // Fetch the entry
        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs
             ORDER BY chain_sequence ASC",
        )
        .fetch_all(db.pool())
        .await
        .unwrap();

        assert_eq!(logs.len(), 1);
        let log = &logs[0];

        // Fetch chain metadata
        let (stored_prev_hash, stored_entry_hash) = sqlx::query_as::<_, (Option<String>, String)>(
            "SELECT previous_hash, entry_hash FROM audit_logs WHERE id = ?",
        )
        .bind(&log.id)
        .fetch_one(db.pool())
        .await
        .unwrap();

        // Recompute hash
        let entry_data = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            log.id,
            log.timestamp,
            log.user_id,
            log.user_role,
            log.tenant_id,
            log.action,
            log.resource_type,
            log.resource_id.as_deref().unwrap_or(""),
            log.status,
            log.error_message.as_deref().unwrap_or(""),
            log.ip_address.as_deref().unwrap_or(""),
            stored_prev_hash.as_deref().unwrap_or(""),
        );
        let computed_hash = adapteros_core::B3Hash::hash(entry_data.as_bytes()).to_string();

        assert_eq!(computed_hash, stored_entry_hash);
    }

    #[tokio::test]
    async fn test_audit_chain_sequence_monotonicity() {
        let db = setup_test_db().await;

        // Create multiple entries
        for i in 0..10 {
            db.log_audit(
                "user-1",
                "admin",
                "tenant-a",
                &format!("action-{}", i),
                "adapter",
                None,
                "success",
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Fetch sequences
        let sequences = sqlx::query_as::<_, (i64,)>(
            "SELECT chain_sequence FROM audit_logs ORDER BY chain_sequence ASC",
        )
        .fetch_all(db.pool())
        .await
        .unwrap();

        // Verify monotonic increase
        for (i, (seq,)) in sequences.iter().enumerate() {
            assert_eq!(*seq, (i + 1) as i64);
        }
    }

    #[tokio::test]
    async fn test_audit_chain_broken_linkage() {
        let db = setup_test_db().await;

        // Create chain
        for i in 0..3 {
            db.log_audit(
                "user-1",
                "admin",
                "tenant-a",
                &format!("action-{}", i),
                "adapter",
                None,
                "success",
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Verify chain is valid
        assert!(db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());

        // Break linkage by corrupting previous_hash
        sqlx::query(
            "UPDATE audit_logs
             SET previous_hash = 'corrupted-hash-value'
             WHERE chain_sequence = 3",
        )
        .execute(db.pool())
        .await
        .unwrap();

        // Verification should fail
        assert!(!db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());
    }

    #[tokio::test]
    async fn test_audit_chain_first_entry_validation() {
        let db = setup_test_db().await;

        // Create first entry
        db.log_audit(
            "user-1",
            "admin",
            "tenant-a",
            "adapter.register",
            "adapter",
            None,
            "success",
            None,
            None,
            None,
        )
        .await
        .unwrap();

        // Verify it has null previous_hash
        let (prev_hash, seq) = sqlx::query_as::<_, (Option<String>, i64)>(
            "SELECT previous_hash, chain_sequence FROM audit_logs WHERE chain_sequence = 1",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();

        assert!(prev_hash.is_none());
        assert_eq!(seq, 1);

        // Verify chain is valid
        assert!(db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());

        // Tamper first entry to have a previous_hash
        sqlx::query(
            "UPDATE audit_logs
             SET previous_hash = 'should-not-exist'
             WHERE chain_sequence = 1",
        )
        .execute(db.pool())
        .await
        .unwrap();

        // Verification should fail
        assert!(!db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());
    }

    #[tokio::test]
    async fn test_audit_chain_multi_tenant_isolation() {
        let db = setup_test_db().await;

        // Create chains for two tenants
        for i in 0..3 {
            db.log_audit(
                "user-a",
                "admin",
                "tenant-a",
                &format!("action-a-{}", i),
                "adapter",
                None,
                "success",
                None,
                None,
                None,
            )
            .await
            .unwrap();

            db.log_audit(
                "user-b",
                "admin",
                "tenant-b",
                &format!("action-b-{}", i),
                "adapter",
                None,
                "success",
                None,
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Each tenant should have valid independent chains
        assert!(db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());
        assert!(db.verify_audit_chain_for_tenant("tenant-b").await.unwrap());

        // Fetch tenant-a logs
        let logs_a = db
            .query_audit_logs_for_tenant("tenant-a", None, None, None, None, None, 100)
            .await
            .unwrap();
        assert_eq!(logs_a.len(), 3);
        assert!(logs_a.iter().all(|l| l.tenant_id == "tenant-a"));

        // Fetch tenant-b logs
        let logs_b = db
            .query_audit_logs_for_tenant("tenant-b", None, None, None, None, None, 100)
            .await
            .unwrap();
        assert_eq!(logs_b.len(), 3);
        assert!(logs_b.iter().all(|l| l.tenant_id == "tenant-b"));
    }

    #[tokio::test]
    async fn test_audit_chain_empty_tenant() {
        let db = setup_test_db().await;

        // Verify empty chain for non-existent tenant
        let is_valid = db
            .verify_audit_chain_for_tenant("tenant-nonexistent")
            .await
            .unwrap();
        assert!(is_valid);
    }

    #[tokio::test]
    async fn test_audit_hash_includes_all_fields() {
        let db = setup_test_db().await;

        // Create log with all fields populated
        db.log_audit(
            "user-123",
            "admin",
            "tenant-a",
            "adapter.register",
            "adapter",
            Some("adapter-xyz"),
            "success",
            Some("error message"),
            Some("192.168.1.100"),
            Some(r#"{"key":"value"}"#),
        )
        .await
        .unwrap();

        // Fetch and verify
        let logs = sqlx::query_as::<_, AuditLog>(
            "SELECT id, timestamp, user_id, user_role, tenant_id, action, resource_type,
                    resource_id, status, error_message, ip_address, metadata_json
             FROM audit_logs",
        )
        .fetch_all(db.pool())
        .await
        .unwrap();

        let log = &logs[0];

        // Verify all fields are populated
        assert_eq!(log.user_id, "user-123");
        assert_eq!(log.user_role, "admin");
        assert_eq!(log.tenant_id, "tenant-a");
        assert_eq!(log.action, "adapter.register");
        assert_eq!(log.resource_type, "adapter");
        assert_eq!(log.resource_id.as_deref(), Some("adapter-xyz"));
        assert_eq!(log.status, "success");
        assert_eq!(log.error_message.as_deref(), Some("error message"));
        assert_eq!(log.ip_address.as_deref(), Some("192.168.1.100"));
        assert_eq!(log.metadata_json.as_deref(), Some(r#"{"key":"value"}"#));
    }

    #[tokio::test]
    async fn test_audit_chain_large_volume() {
        let db = setup_test_db().await;

        // Create a large chain
        for i in 0..100 {
            db.log_audit(
                "user-1",
                "admin",
                "tenant-a",
                &format!("action-{}", i),
                "adapter",
                Some(&format!("resource-{}", i)),
                if i % 2 == 0 { "success" } else { "failure" },
                if i % 2 == 1 {
                    Some("error occurred")
                } else {
                    None
                },
                None,
                None,
            )
            .await
            .unwrap();
        }

        // Verify entire chain
        assert!(db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());

        // Query should return correct count
        let logs = db
            .query_audit_logs_for_tenant("tenant-a", None, None, None, None, None, 1000)
            .await
            .unwrap();
        assert_eq!(logs.len(), 100);
    }

    #[tokio::test]
    async fn test_audit_chain_sequence_gap_detection() {
        let db = setup_test_db().await;

        // Create initial entries
        db.log_audit(
            "user-1", "admin", "tenant-a", "action-1", "adapter", None, "success", None, None, None,
        )
        .await
        .unwrap();

        db.log_audit(
            "user-1", "admin", "tenant-a", "action-2", "adapter", None, "success", None, None, None,
        )
        .await
        .unwrap();

        // Verify chain is valid
        assert!(db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());

        // Manually create a gap by inserting entry with wrong sequence
        sqlx::query(
            "INSERT INTO audit_logs
             (id, timestamp, user_id, user_role, tenant_id, action, resource_type,
              status, previous_hash, entry_hash, chain_sequence)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id(IdPrefix::Aud))
        .bind(chrono::Utc::now().to_rfc3339())
        .bind("user-1")
        .bind("admin")
        .bind("tenant-a")
        .bind("gap-action")
        .bind("adapter")
        .bind("success")
        .bind("fake-prev")
        .bind("fake-hash")
        .bind(5i64) // Gap: jumping from 2 to 5
        .execute(db.pool())
        .await
        .unwrap();

        // Verification should detect the gap
        assert!(!db.verify_audit_chain_for_tenant("tenant-a").await.unwrap());
    }
}
