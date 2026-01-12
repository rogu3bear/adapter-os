use tracing::{debug, info, warn};
use adapteros_core::{Result, AosError};
use crate::Db;
use crate::adapters::types::*;
use crate::adapters::ADAPTER_SELECT_FIELDS;
use crate::adapters_kv::AdapterKvOps;

impl Db {
pub async fn find_adapters_with_missing_hashes(
        &self,
        tenant_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Adapter>> {
        let query = match tenant_id {
            Some(_) => format!(
                "SELECT {} FROM adapters
                 WHERE tenant_id = ?
                   AND aos_file_path IS NOT NULL
                   AND aos_file_path != ''
                   AND archived_at IS NULL
                   AND purged_at IS NULL
                   AND (
                       content_hash_b3 IS NULL
                       OR content_hash_b3 = ''
                       OR manifest_hash IS NULL
                       OR manifest_hash = ''
                   )
                 ORDER BY created_at ASC
                 LIMIT ?",
                ADAPTER_SELECT_FIELDS
            ),
            None => format!(
                "SELECT {} FROM adapters
                 WHERE aos_file_path IS NOT NULL
                   AND aos_file_path != ''
                   AND archived_at IS NULL
                   AND purged_at IS NULL
                   AND (
                       content_hash_b3 IS NULL
                       OR content_hash_b3 = ''
                       OR manifest_hash IS NULL
                       OR manifest_hash = ''
                   )
                 ORDER BY created_at ASC
                 LIMIT ?",
                ADAPTER_SELECT_FIELDS
            ),
        };

        let adapters = match tenant_id {
            Some(tid) => sqlx::query_as::<_, Adapter>(&query)
                .bind(tid)
                .bind(limit)
                .fetch_all(self.pool())
                .await
                .map_err(|e| {
                    AosError::database(format!(
                        "Failed to find adapters with missing hashes: {}",
                        e
                    ))
                })?,
            None => sqlx::query_as::<_, Adapter>(&query)
                .bind(limit)
                .fetch_all(self.pool())
                .await
                .map_err(|e| {
                    AosError::database(format!(
                        "Failed to find adapters with missing hashes: {}",
                        e
                    ))
                })?,
        };

        Ok(adapters)
    }
}

impl Db {
pub async fn list_adapters_for_tenant(&self, tenant_id: &str) -> Result<Vec<Adapter>> {
        self.list_adapters_for_tenant_paged(tenant_id, None, None)
            .await
    }
}

impl Db {
pub async fn list_all_adapters_system(&self) -> Result<Vec<Adapter>> {
        if self.storage_mode().read_from_kv() {
            let mut adapters = Vec::new();

            let tenants = self.list_tenants().await?;
            for tenant in tenants {
                let tenant_adapters = self.list_adapters_for_tenant(&tenant.id).await?;
                adapters.extend(tenant_adapters);
            }

            if !adapters.is_empty() || !self.storage_mode().sql_fallback_enabled() {
                adapters.sort_by(|a, b| {
                    a.tier
                        .cmp(&b.tier)
                        .then_with(|| b.created_at.cmp(&a.created_at))
                });
                return Ok(adapters);
            }

            self.record_kv_read_fallback("adapters.list_all.system");
        }

        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(self.pool())
            .await
            .map_err(|e| {
                AosError::database(format!("Failed to list all adapters (system): {}", e))
            })?;
        Ok(adapters)
    }
}

impl Db {
pub async fn cleanup_orphaned_adapters(&self, tenant_id: &str) -> Result<u64> {
        // Get adapter IDs from SQL
        let sql_adapters = self.list_adapters_for_tenant(tenant_id).await?;
        let sql_ids: std::collections::HashSet<String> = sql_adapters
            .iter()
            .filter_map(|a| a.adapter_id.clone())
            .collect();

        // Get adapter IDs from KV
        let kv_repo = match self.get_adapter_kv_repo(tenant_id) {
            Some(repo) => repo,
            None => {
                // No KV repo configured, nothing to clean up
                return Ok(0);
            }
        };

        let kv_adapters = kv_repo
            .list_adapters_for_tenant_kv(tenant_id, None, None)
            .await?;
        let kv_ids: std::collections::HashSet<String> = kv_adapters
            .iter()
            .filter_map(|a| a.adapter_id.clone())
            .collect();

        // Find orphans: KV entries that don't exist in SQL
        let orphans: Vec<String> = kv_ids.difference(&sql_ids).cloned().collect();

        if orphans.is_empty() {
            debug!(
                tenant_id = %tenant_id,
                sql_count = sql_ids.len(),
                kv_count = kv_ids.len(),
                "No orphaned adapters found in KV"
            );
            return Ok(0);
        }

        info!(
            tenant_id = %tenant_id,
            orphan_count = orphans.len(),
            "Found orphaned adapters in KV, cleaning up"
        );

        let mut deleted = 0u64;
        for orphan_id in &orphans {
            match kv_repo.delete_adapter_kv(orphan_id).await {
                Ok(()) => {
                    deleted += 1;
                    debug!(
                        adapter_id = %orphan_id,
                        tenant_id = %tenant_id,
                        "Deleted orphaned adapter from KV"
                    );
                }
                Err(e) => {
                    warn!(
                        error = %e,
                        adapter_id = %orphan_id,
                        tenant_id = %tenant_id,
                        "Failed to delete orphaned adapter from KV"
                    );
                }
            }
        }

        info!(
            tenant_id = %tenant_id,
            deleted = deleted,
            total_orphans = orphans.len(),
            "Orphan cleanup complete"
        );

        Ok(deleted)
    }
}

