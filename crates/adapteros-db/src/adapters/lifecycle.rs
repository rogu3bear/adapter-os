use tracing::{debug, info, warn};
use adapteros_core::{Result, AosError};
use crate::Db;
use crate::adapters::types::*;
use crate::adapters::ADAPTER_SELECT_FIELDS;
use crate::adapters_kv::AdapterKvOps;

impl Db {
pub async fn archive_adapter(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        archived_by: &str,
        reason: &str,
    ) -> Result<()> {
        // SECURITY: Update only within tenant scope
        let affected = sqlx::query(
            "UPDATE adapters
             SET archived_at = datetime('now'),
                 archived_by = ?,
                 archive_reason = ?,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND tenant_id = ?
               AND archived_at IS NULL",
        )
        .bind(archived_by)
        .bind(reason)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to archive adapter: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter not found or already archived: {}",
                adapter_id
            )));
        }

        info!(adapter_id = %adapter_id, archived_by = %archived_by, "Archived adapter");

        // KV write (dual-write mode) - tenant verified via parameter
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo
                .archive_adapter_kv(adapter_id, archived_by, reason)
                .await
            {
                warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to archive adapter in KV backend");
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter archived in both SQL and KV backends");
            }
        }

        Ok(())
    }
}

impl Db {
pub async fn archive_adapters_for_tenant(
        &self,
        tenant_id: &str,
        archived_by: &str,
        reason: &str,
    ) -> Result<u64> {
        // First, get the list of adapter IDs that will be affected (for KV dual-write)
        let affected_adapter_ids: Vec<String> = sqlx::query_scalar(
            "SELECT adapter_id FROM adapters
             WHERE tenant_id = ?
               AND archived_at IS NULL
               AND active = 1",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to query adapters: {}", e)))?;

        let result = sqlx::query(
            "UPDATE adapters
             SET archived_at = datetime('now'),
                 archived_by = ?,
                 archive_reason = ?,
                 updated_at = datetime('now')
             WHERE tenant_id = ?
               AND archived_at IS NULL
               AND active = 1",
        )
        .bind(archived_by)
        .bind(reason)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to archive adapters: {}", e)))?;

        info!(
            tenant_id = %tenant_id,
            archived_by = %archived_by,
            count = result.rows_affected(),
            "Archived adapters for tenant"
        );

        // KV write (dual-write mode) - archive each adapter in KV backend
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            let mut kv_success_count = 0u64;
            let mut kv_error_count = 0u64;

            for adapter_id in &affected_adapter_ids {
                match repo
                    .archive_adapter_kv(adapter_id, archived_by, reason)
                    .await
                {
                    Ok(()) => {
                        kv_success_count += 1;
                    }
                    Err(e) => {
                        kv_error_count += 1;
                        warn!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write",
                            "Failed to archive adapter in KV backend"
                        );
                    }
                }
            }

            if kv_error_count > 0 {
                warn!(
                    tenant_id = %tenant_id,
                    success_count = kv_success_count,
                    error_count = kv_error_count,
                    mode = "dual-write",
                    "Partial KV archive failure for tenant adapters"
                );
            } else if kv_success_count > 0 {
                debug!(
                    tenant_id = %tenant_id,
                    count = kv_success_count,
                    mode = "dual-write",
                    "Archived adapters in both SQL and KV backends"
                );
            }
        }

        Ok(result.rows_affected())
    }
}

impl Db {
pub async fn find_archived_adapters_for_gc(
        &self,
        min_age_days: u32,
        limit: i64,
    ) -> Result<Vec<Adapter>> {
        let query = format!(
            "SELECT {} FROM adapters
             WHERE archived_at IS NOT NULL
               AND purged_at IS NULL
               AND aos_file_path IS NOT NULL
               AND datetime(archived_at, '+{} days') <= datetime('now')
             ORDER BY archived_at ASC
             LIMIT ?",
            ADAPTER_SELECT_FIELDS, min_age_days
        );

        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(limit)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(format!("Failed to find GC candidates: {}", e)))?;

        Ok(adapters)
    }
}

impl Db {
pub async fn mark_adapter_purged(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        // Pre-check: Log that we're about to cross the point of no return
        warn!(
            adapter_id = %adapter_id,
            "POINT OF NO RETURN: About to mark adapter as purged. This is irreversible."
        );

        // SECURITY: Update only within tenant scope
        let affected = sqlx::query(
            "UPDATE adapters
             SET purged_at = datetime('now'),
                 aos_file_path = NULL,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND tenant_id = ?
               AND archived_at IS NOT NULL
               AND purged_at IS NULL",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to mark adapter purged: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::validation(format!(
                "Adapter {} is not archived or already purged. Cannot proceed with irreversible purge.",
                adapter_id
            )));
        }

        // Log completion of the irreversible operation
        info!(
            adapter_id = %adapter_id,
            "IRREVERSIBLE: Adapter marked as purged. Recovery is no longer possible."
        );

        // KV write (dual-write mode) - tenant verified via parameter
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo.mark_adapter_purged_kv(adapter_id).await {
                warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to mark adapter purged in KV backend");
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter marked purged in both SQL and KV backends");
            }
        }

        Ok(())
    }
}

impl Db {
pub async fn is_adapter_loadable(&self, adapter_id: &str) -> Result<bool> {
        let result: Option<(Option<String>, Option<String>)> =
            sqlx::query_as("SELECT archived_at, purged_at FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(self.pool())
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

        match result {
            Some((archived_at, purged_at)) => {
                // Loadable if not archived AND not purged
                Ok(archived_at.is_none() && purged_at.is_none())
            }
            None => Err(AosError::NotFound(format!(
                "Adapter not found: {}",
                adapter_id
            ))),
        }
    }
}

impl Db {
pub async fn unarchive_adapter(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        // SECURITY: Update only within tenant scope
        let affected = sqlx::query(
            "UPDATE adapters
             SET archived_at = NULL,
                 archived_by = NULL,
                 archive_reason = NULL,
                 updated_at = datetime('now')
             WHERE adapter_id = ?
               AND tenant_id = ?
               AND archived_at IS NOT NULL
               AND purged_at IS NULL",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to unarchive adapter: {}", e)))?
        .rows_affected();

        if affected == 0 {
            // This is the enforcement point - purged adapters cannot be restored
            return Err(AosError::validation(format!(
                "Adapter {} is not archived or has crossed the point of no return (purged). Recovery is not possible.",
                adapter_id
            )));
        }

        info!(adapter_id = %adapter_id, "Unarchived adapter - successfully restored before purge");

        // KV write (dual-write mode) - tenant verified via parameter
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo.unarchive_adapter_kv(adapter_id).await {
                warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to unarchive adapter in KV backend");
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Adapter unarchived in both SQL and KV backends");
            }
        }

        Ok(())
    }
}

impl Db {
pub async fn count_archived_adapters(&self, tenant_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM adapters
             WHERE tenant_id = ?
               AND archived_at IS NOT NULL
               AND purged_at IS NULL",
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to count archived adapters: {}", e)))?;

        Ok(count)
    }
}

impl Db {
pub async fn count_purged_adapters(&self, tenant_id: &str) -> Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM adapters
             WHERE tenant_id = ?
               AND purged_at IS NOT NULL",
        )
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to count purged adapters: {}", e)))?;

        Ok(count)
    }
}

