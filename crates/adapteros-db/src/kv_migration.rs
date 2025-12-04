//! SQL-to-KV migration utilities
//!
//! This module provides tools for migrating adapter data from SQL to KV storage,
//! including batch migration, progress tracking, consistency verification, and rollback.
//!
//! ## Features
//!
//! - **Batch Migration**: Migrate adapters in configurable batches for large datasets
//! - **Tenant-Specific Migration**: Migrate adapters for a single tenant
//! - **Progress Callbacks**: Track migration progress with custom callbacks
//! - **Rollback Support**: Delete all KV data for re-migration scenarios
//! - **Consistency Verification**: Compare SQL and KV data to detect discrepancies

use crate::adapters::{Adapter, AdapterRegistrationParams};
use crate::adapters_kv::{AdapterKvOps, AdapterKvRepository};
use crate::Db;
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Migration progress statistics
///
/// Tracks the outcome of a migration operation including successful migrations,
/// failures, and skipped adapters. Also maintains a list of failed adapter IDs
/// for troubleshooting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStats {
    /// Total number of adapters to migrate
    pub total: usize,
    /// Number of adapters successfully migrated
    pub migrated: usize,
    /// Number of adapters that failed to migrate
    pub failed: usize,
    /// Number of adapters skipped (already in KV)
    pub skipped: usize,
    /// List of adapter IDs that failed migration
    pub failed_ids: Vec<String>,
}

impl Default for MigrationStats {
    fn default() -> Self {
        Self {
            total: 0,
            migrated: 0,
            failed: 0,
            skipped: 0,
            failed_ids: Vec::new(),
        }
    }
}

impl MigrationStats {
    /// Check if migration was completely successful (no failures)
    pub fn is_success(&self) -> bool {
        self.failed == 0 && self.total > 0
    }

    /// Get success rate as percentage (migrated / total)
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.migrated as f64 / self.total as f64) * 100.0
    }
}

/// Migration progress information for callbacks
#[derive(Debug, Clone)]
pub struct MigrationProgress {
    /// Current adapter being migrated
    pub current_adapter_id: String,
    /// Number of adapters processed so far
    pub processed: usize,
    /// Total number of adapters to migrate
    pub total: usize,
    /// Current batch number (1-indexed)
    pub batch: usize,
    /// Whether this adapter succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

impl MigrationProgress {
    /// Get progress percentage
    pub fn percentage(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.processed as f64 / self.total as f64) * 100.0
    }
}

/// Represents a discrepancy between SQL and KV data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationDiscrepancy {
    /// The adapter ID with the discrepancy
    pub adapter_id: String,
    /// The field that differs
    pub field: String,
    /// The value in SQL
    pub sql_value: String,
    /// The value in KV
    pub kv_value: String,
}

impl Db {
    /// Migrate all adapters from SQL to KV storage
    ///
    /// This method:
    /// 1. Lists all adapters in SQL
    /// 2. For each adapter, checks if it exists in KV
    /// 3. If not, migrates it to KV
    /// 4. Tracks migration progress and errors
    ///
    /// # Returns
    /// Migration statistics including total, migrated, failed, and skipped counts
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let stats = db.migrate_adapters_to_kv().await?;
    /// println!("Migrated {}/{} adapters ({} failed, {} skipped)",
    ///     stats.migrated, stats.total, stats.failed, stats.skipped);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn migrate_adapters_to_kv(&self) -> Result<MigrationStats> {
        // Ensure KV backend is available
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not attached - call init_kv_backend() first".into())
        })?;

        let mut stats = MigrationStats::default();

        info!("Starting SQL-to-KV migration for all adapters");

        // Get all adapters from SQL (system-level operation)
        #[allow(deprecated)]
        let sql_adapters = self.list_all_adapters_system().await?;
        stats.total = sql_adapters.len();

        info!(total = stats.total, "Found {} adapters in SQL", stats.total);

        // Group adapters by tenant for efficient migration
        let mut adapters_by_tenant: std::collections::HashMap<String, Vec<Adapter>> =
            std::collections::HashMap::new();
        for adapter in sql_adapters {
            adapters_by_tenant
                .entry(adapter.tenant_id.clone())
                .or_default()
                .push(adapter);
        }

        // Migrate each tenant's adapters
        for (tenant_id, adapters) in adapters_by_tenant {
            debug!(
                tenant_id = %tenant_id,
                count = adapters.len(),
                "Migrating {} adapters for tenant {}",
                adapters.len(),
                tenant_id
            );

            // Create repository for this tenant
            let repo = crate::adapters_kv::AdapterKvRepository::new(
                Arc::new(adapteros_storage::repos::AdapterRepository::new(
                    kv_backend.backend().clone(),
                    kv_backend.index_manager().clone(),
                )),
                tenant_id.clone(),
            );

            for adapter in adapters {
                let adapter_id = adapter
                    .adapter_id
                    .clone()
                    .unwrap_or_else(|| adapter.id.clone());

                match self.migrate_single_adapter(&repo, adapter).await {
                    Ok(true) => {
                        stats.migrated += 1;
                        debug!(adapter_id = %adapter_id, "Migrated adapter to KV");
                    }
                    Ok(false) => {
                        stats.skipped += 1;
                        debug!(adapter_id = %adapter_id, "Adapter already in KV, skipped");
                    }
                    Err(e) => {
                        stats.failed += 1;
                        stats.failed_ids.push(adapter_id.clone());
                        warn!(
                            adapter_id = %adapter_id,
                            error = %e,
                            "Failed to migrate adapter to KV"
                        );
                    }
                }
            }
        }

        info!(
            total = stats.total,
            migrated = stats.migrated,
            failed = stats.failed,
            skipped = stats.skipped,
            "Migration complete: {}/{} migrated, {} failed, {} skipped",
            stats.migrated,
            stats.total,
            stats.failed,
            stats.skipped
        );

        Ok(stats)
    }

    /// Migrate a single adapter from SQL to KV
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter ID to migrate
    ///
    /// # Returns
    /// * `Ok(true)` - Adapter was migrated
    /// * `Ok(false)` - Adapter already exists in KV (skipped)
    /// * `Err(_)` - Migration failed
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// match db.migrate_adapter_to_kv("adapter-123").await? {
    ///     true => println!("Adapter migrated"),
    ///     false => println!("Adapter already in KV"),
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn migrate_adapter_to_kv(&self, adapter_id: &str) -> Result<bool> {
        // Ensure KV backend is available
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not attached - call init_kv_backend() first".into())
        })?;

        // Get adapter from SQL
        let adapter = self
            .get_adapter(adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        // Create repository for this tenant
        let repo = crate::adapters_kv::AdapterKvRepository::new(
            Arc::new(adapteros_storage::repos::AdapterRepository::new(
                kv_backend.backend().clone(),
                kv_backend.index_manager().clone(),
            )),
            adapter.tenant_id.clone(),
        );

        self.migrate_single_adapter(&repo, adapter).await
    }

    /// Internal helper to migrate a single adapter
    async fn migrate_single_adapter(
        &self,
        repo: &AdapterKvRepository,
        adapter: Adapter,
    ) -> Result<bool> {
        let adapter_id = adapter.adapter_id.as_ref().unwrap_or(&adapter.id);

        // Check if adapter already exists in KV
        if let Some(_) = repo.get_adapter_kv(adapter_id).await? {
            // Already in KV, skip
            return Ok(false);
        }

        // Convert SQL adapter to registration params
        let params = AdapterRegistrationParams {
            tenant_id: adapter.tenant_id.clone(),
            adapter_id: adapter.adapter_id.clone().unwrap_or(adapter.id.clone()),
            name: adapter.name.clone(),
            hash_b3: adapter.hash_b3.clone(),
            rank: adapter.rank,
            tier: adapter.tier.clone(),
            alpha: adapter.alpha,
            targets_json: adapter.targets_json.clone(),
            acl_json: adapter.acl_json.clone(),
            languages_json: adapter.languages_json.clone(),
            framework: adapter.framework.clone(),
            category: adapter.category.clone(),
            scope: adapter.scope.clone(),
            framework_id: adapter.framework_id.clone(),
            framework_version: adapter.framework_version.clone(),
            repo_id: adapter.repo_id.clone(),
            commit_sha: adapter.commit_sha.clone(),
            intent: adapter.intent.clone(),
            expires_at: adapter.expires_at.clone(),
            aos_file_path: adapter.aos_file_path.clone(),
            aos_file_hash: adapter.aos_file_hash.clone(),
            adapter_name: adapter.adapter_name.clone(),
            tenant_namespace: adapter.tenant_namespace.clone(),
            domain: adapter.domain.clone(),
            purpose: adapter.purpose.clone(),
            revision: adapter.revision.clone(),
            parent_id: adapter.parent_id.clone(),
            fork_type: adapter.fork_type.clone(),
            fork_reason: adapter.fork_reason.clone(),
            base_model_id: None, // Not available during KV migration
            manifest_schema_version: None, // Not available during KV migration
            content_hash_b3: None, // Not available during KV migration
            provenance_json: None, // Not available during KV migration
        };

        // Register in KV
        repo.register_adapter_kv(params).await?;

        Ok(true)
    }

    /// Verify consistency between SQL and KV storage
    ///
    /// This method:
    /// 1. Lists all adapters from SQL
    /// 2. For each adapter, retrieves it from both SQL and KV
    /// 3. Compares critical fields
    /// 4. Reports any discrepancies found
    ///
    /// # Returns
    /// A list of discrepancies found. Empty list means data is consistent.
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let discrepancies = db.verify_migration_consistency().await?;
    /// if discrepancies.is_empty() {
    ///     println!("All data is consistent!");
    /// } else {
    ///     for d in &discrepancies {
    ///         println!("Discrepancy in {} field '{}': SQL='{}' KV='{}'",
    ///             d.adapter_id, d.field, d.sql_value, d.kv_value);
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_migration_consistency(&self) -> Result<Vec<MigrationDiscrepancy>> {
        // Ensure KV backend is available
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not attached - call init_kv_backend() first".into())
        })?;

        let mut discrepancies = Vec::new();

        info!("Starting SQL-to-KV consistency verification");

        // Get all adapters from SQL
        #[allow(deprecated)]
        let sql_adapters = self.list_all_adapters_system().await?;
        let total = sql_adapters.len();

        info!(total = total, "Verifying {} adapters", total);

        // Group by tenant
        let mut adapters_by_tenant: std::collections::HashMap<String, Vec<Adapter>> =
            std::collections::HashMap::new();
        for adapter in sql_adapters {
            adapters_by_tenant
                .entry(adapter.tenant_id.clone())
                .or_default()
                .push(adapter);
        }

        // Verify each tenant's adapters
        for (tenant_id, adapters) in adapters_by_tenant {
            debug!(
                tenant_id = %tenant_id,
                count = adapters.len(),
                "Verifying {} adapters for tenant {}",
                adapters.len(),
                tenant_id
            );

            // Create repository for this tenant
            let repo = crate::adapters_kv::AdapterKvRepository::new(
                Arc::new(adapteros_storage::repos::AdapterRepository::new(
                    kv_backend.backend().clone(),
                    kv_backend.index_manager().clone(),
                )),
                tenant_id.clone(),
            );

            for sql_adapter in adapters {
                let adapter_id = sql_adapter.adapter_id.as_ref().unwrap_or(&sql_adapter.id);

                // Get from KV
                let kv_adapter = match repo.get_adapter_kv(adapter_id).await {
                    Ok(Some(adapter)) => adapter,
                    Ok(None) => {
                        // Adapter exists in SQL but not KV - report as discrepancy
                        discrepancies.push(MigrationDiscrepancy {
                            adapter_id: adapter_id.clone(),
                            field: "_existence".to_string(),
                            sql_value: "exists".to_string(),
                            kv_value: "missing".to_string(),
                        });
                        continue;
                    }
                    Err(e) => {
                        warn!(
                            adapter_id = %adapter_id,
                            error = %e,
                            "Failed to get adapter from KV during verification"
                        );
                        continue;
                    }
                };

                // Compare critical fields
                self.compare_adapter_fields(
                    adapter_id,
                    &sql_adapter,
                    &kv_adapter,
                    &mut discrepancies,
                );
            }
        }

        if discrepancies.is_empty() {
            info!("✓ Verification complete: All data is consistent");
        } else {
            warn!(
                count = discrepancies.len(),
                "Verification complete: Found {} discrepancies",
                discrepancies.len()
            );
        }

        Ok(discrepancies)
    }

    /// Compare fields between SQL and KV adapters
    fn compare_adapter_fields(
        &self,
        adapter_id: &str,
        sql: &Adapter,
        kv: &Adapter,
        discrepancies: &mut Vec<MigrationDiscrepancy>,
    ) {
        // Helper macro to compare fields
        macro_rules! compare_field {
            ($field:ident) => {
                if sql.$field != kv.$field {
                    discrepancies.push(MigrationDiscrepancy {
                        adapter_id: adapter_id.to_string(),
                        field: stringify!($field).to_string(),
                        sql_value: format!("{:?}", sql.$field),
                        kv_value: format!("{:?}", kv.$field),
                    });
                }
            };
        }

        // Compare critical fields
        compare_field!(name);
        compare_field!(hash_b3);
        compare_field!(rank);
        compare_field!(alpha);
        compare_field!(tier);
        compare_field!(category);
        compare_field!(scope);
        compare_field!(current_state);
        compare_field!(load_state);
        compare_field!(lifecycle_state);
        compare_field!(version);
        compare_field!(active);
        compare_field!(memory_bytes);
        compare_field!(activation_count);

        // Compare optional fields
        if sql.framework != kv.framework {
            discrepancies.push(MigrationDiscrepancy {
                adapter_id: adapter_id.to_string(),
                field: "framework".to_string(),
                sql_value: sql.framework.as_deref().unwrap_or("None").to_string(),
                kv_value: kv.framework.as_deref().unwrap_or("None").to_string(),
            });
        }

        if sql.parent_id != kv.parent_id {
            discrepancies.push(MigrationDiscrepancy {
                adapter_id: adapter_id.to_string(),
                field: "parent_id".to_string(),
                sql_value: sql.parent_id.as_deref().unwrap_or("None").to_string(),
                kv_value: kv.parent_id.as_deref().unwrap_or("None").to_string(),
            });
        }

        if sql.expires_at != kv.expires_at {
            discrepancies.push(MigrationDiscrepancy {
                adapter_id: adapter_id.to_string(),
                field: "expires_at".to_string(),
                sql_value: sql.expires_at.as_deref().unwrap_or("None").to_string(),
                kv_value: kv.expires_at.as_deref().unwrap_or("None").to_string(),
            });
        }
    }

    /// Migrate adapters in batches with configurable batch size
    ///
    /// This method processes adapters in batches to handle large datasets efficiently.
    /// It handles errors gracefully by logging failures and continuing with the next batch.
    ///
    /// # Arguments
    /// * `batch_size` - Number of adapters to process in each batch (recommended: 50-200)
    ///
    /// # Returns
    /// Migration statistics including successful/failed counts
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let stats = db.migrate_adapters_batch(100).await?;
    /// println!("Migrated {}/{} adapters ({:.1}% success)",
    ///     stats.migrated, stats.total, stats.success_rate());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn migrate_adapters_batch(&self, batch_size: usize) -> Result<MigrationStats> {
        self.migrate_with_progress_internal(None, batch_size, |_| {})
            .await
    }

    /// Migrate adapters for a specific tenant
    ///
    /// This method migrates only adapters belonging to the specified tenant.
    /// Useful for incremental tenant-by-tenant migration.
    ///
    /// # Arguments
    /// * `tenant_id` - ID of the tenant whose adapters should be migrated
    ///
    /// # Returns
    /// Migration statistics for this tenant's adapters
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let stats = db.migrate_tenant_adapters("tenant-123").await?;
    /// println!("Migrated {} adapters for tenant-123", stats.migrated);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn migrate_tenant_adapters(&self, tenant_id: &str) -> Result<MigrationStats> {
        self.migrate_with_progress_internal(Some(tenant_id), 100, |_| {})
            .await
    }

    /// Migrate adapters with progress callback
    ///
    /// This method allows you to track migration progress via a callback function.
    /// The callback is invoked after each adapter is processed (success or failure).
    ///
    /// # Arguments
    /// * `callback` - Function called after each adapter with progress information
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let stats = db.migrate_with_progress(|progress| {
    ///     println!("Progress: {:.1}% ({}/{})",
    ///         progress.percentage(), progress.processed, progress.total);
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn migrate_with_progress<F>(&self, callback: F) -> Result<MigrationStats>
    where
        F: Fn(MigrationProgress),
    {
        self.migrate_with_progress_internal(None, 100, callback)
            .await
    }

    /// Internal migration implementation with all options
    ///
    /// # Arguments
    /// * `tenant_id` - Optional tenant filter (None = all tenants)
    /// * `batch_size` - Number of adapters per batch
    /// * `callback` - Progress callback function
    async fn migrate_with_progress_internal<F>(
        &self,
        tenant_id: Option<&str>,
        batch_size: usize,
        callback: F,
    ) -> Result<MigrationStats>
    where
        F: Fn(MigrationProgress),
    {
        // Check if KV backend is available
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config(
                "KV backend not initialized. Call init_kv_backend() first.".to_string(),
            )
        })?;

        info!(
            "Starting adapter migration to KV storage (batch_size: {})",
            batch_size
        );

        // Fetch adapters from SQL
        let adapters = if let Some(tid) = tenant_id {
            info!("Fetching adapters for tenant: {}", tid);
            self.list_adapters_for_tenant(tid).await?
        } else {
            info!("Fetching all adapters across all tenants");
            #[allow(deprecated)]
            self.list_all_adapters_system().await?
        };

        let total_count = adapters.len();
        info!("Found {} adapters to migrate", total_count);

        if total_count == 0 {
            warn!("No adapters found to migrate");
            return Ok(MigrationStats::default());
        }

        let mut stats = MigrationStats::default();
        stats.total = total_count;

        // Group adapters by tenant for efficient migration
        let mut adapters_by_tenant: std::collections::HashMap<String, Vec<Adapter>> =
            std::collections::HashMap::new();
        for adapter in adapters {
            adapters_by_tenant
                .entry(adapter.tenant_id.clone())
                .or_default()
                .push(adapter);
        }

        // Process adapters in batches
        let mut batch_num = 0;
        for (tid, tenant_adapters) in adapters_by_tenant {
            // Create repository for this tenant
            let repo = crate::adapters_kv::AdapterKvRepository::new(
                Arc::new(adapteros_storage::repos::AdapterRepository::new(
                    kv_backend.backend().clone(),
                    kv_backend.index_manager().clone(),
                )),
                tid.clone(),
            );

            for chunk in tenant_adapters.chunks(batch_size) {
                batch_num += 1;
                let batch_count = chunk.len();

                info!(
                    "Processing batch {} ({} adapters) for tenant {}",
                    batch_num, batch_count, tid
                );

                for adapter in chunk {
                    let adapter_id = adapter
                        .adapter_id
                        .clone()
                        .unwrap_or_else(|| adapter.id.clone());

                    debug!("Migrating adapter: {} ({})", adapter.name, adapter_id);

                    // Attempt to migrate this adapter
                    let migration_result =
                        self.migrate_single_adapter(&repo, adapter.clone()).await;

                    let (success, error, skip) = match migration_result {
                        Ok(true) => {
                            stats.migrated += 1;
                            (true, None, false)
                        }
                        Ok(false) => {
                            stats.skipped += 1;
                            (true, None, true)
                        }
                        Err(e) => {
                            stats.failed += 1;
                            stats.failed_ids.push(adapter_id.clone());
                            let err_msg = e.to_string();
                            error!(
                                adapter_id = %adapter_id,
                                error = %err_msg,
                                "Failed to migrate adapter"
                            );
                            (false, Some(err_msg), false)
                        }
                    };

                    if !skip {
                        // Invoke progress callback (don't call for skipped adapters)
                        callback(MigrationProgress {
                            current_adapter_id: adapter_id,
                            processed: stats.migrated + stats.failed,
                            total: total_count,
                            batch: batch_num,
                            success,
                            error,
                        });
                    }
                }

                debug!(
                    "Batch {} complete: {}/{} successful",
                    batch_num,
                    stats.migrated,
                    stats.migrated + stats.failed
                );
            }
        }

        info!(
            "Migration complete: {}/{} migrated ({:.1}% success), {} failed, {} skipped",
            stats.migrated,
            stats.total,
            stats.success_rate(),
            stats.failed,
            stats.skipped
        );

        if !stats.failed_ids.is_empty() {
            warn!(
                "Failed adapter IDs (first 10): {:?}",
                stats.failed_ids.iter().take(10).collect::<Vec<_>>()
            );
        }

        Ok(stats)
    }

    /// Rollback KV data by deleting all adapter entries
    ///
    /// **WARNING:** This is a destructive operation that deletes ALL adapter data
    /// from KV storage. Use only for re-migration scenarios or testing.
    ///
    /// This operation:
    /// 1. Scans all adapter keys in KV storage
    /// 2. Deletes each adapter entry
    /// 3. Clears related indexes
    ///
    /// SQL data is NOT affected.
    ///
    /// # Example
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// // Delete all KV adapter data to re-run migration
    /// db.rollback_kv_data().await?;
    /// println!("KV data rolled back, SQL data intact");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn rollback_kv_data(&self) -> Result<()> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not initialized. Cannot rollback.".to_string())
        })?;

        warn!("Starting KV data rollback - this will delete ALL adapter data from KV storage");

        // Scan all adapter keys (adapters are stored with prefix "adapter:")
        let adapter_keys = kv_backend
            .scan_prefix("adapter:")
            .await
            .map_err(|e| AosError::Database(format!("Failed to scan adapter keys: {}", e)))?;

        let total_keys = adapter_keys.len();
        info!("Found {} adapter keys to delete", total_keys);

        if total_keys == 0 {
            info!("No adapter data in KV storage, rollback not needed");
            return Ok(());
        }

        let mut deleted = 0;
        let mut errors = 0;

        // Delete each adapter key
        for key in &adapter_keys {
            match kv_backend.delete(key).await {
                Ok(true) => {
                    deleted += 1;
                    debug!("Deleted KV key: {}", key);
                }
                Ok(false) => {
                    warn!("Key not found during delete: {}", key);
                }
                Err(e) => {
                    errors += 1;
                    error!("Failed to delete key {}: {}", key, e);
                }
            }
        }

        info!(
            "KV rollback complete: {} deleted, {} errors out of {} total keys",
            deleted, errors, total_keys
        );

        if errors > 0 {
            warn!(
                "Rollback completed with {} errors - some keys may remain",
                errors
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_stats_default() {
        let stats = MigrationStats::default();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.migrated, 0);
        assert_eq!(stats.failed, 0);
        assert_eq!(stats.skipped, 0);
        assert_eq!(stats.failed_ids.len(), 0);
    }

    #[test]
    fn test_migration_stats_success_rate() {
        let stats = MigrationStats {
            total: 100,
            migrated: 95,
            failed: 5,
            skipped: 0,
            failed_ids: vec!["a".to_string(), "b".to_string()],
        };

        assert_eq!(stats.success_rate(), 95.0);
        assert!(!stats.is_success());
    }

    #[test]
    fn test_migration_stats_is_success() {
        let stats = MigrationStats {
            total: 100,
            migrated: 100,
            failed: 0,
            skipped: 0,
            failed_ids: vec![],
        };

        assert!(stats.is_success());
        assert_eq!(stats.success_rate(), 100.0);
    }

    #[test]
    fn test_migration_progress_percentage() {
        let progress = MigrationProgress {
            current_adapter_id: "test".to_string(),
            processed: 50,
            total: 200,
            batch: 1,
            success: true,
            error: None,
        };

        assert_eq!(progress.percentage(), 25.0);
    }

    #[test]
    fn test_migration_discrepancy_creation() {
        let discrepancy = MigrationDiscrepancy {
            adapter_id: "adapter-123".to_string(),
            field: "name".to_string(),
            sql_value: "Old Name".to_string(),
            kv_value: "New Name".to_string(),
        };

        assert_eq!(discrepancy.adapter_id, "adapter-123");
        assert_eq!(discrepancy.field, "name");
        assert_eq!(discrepancy.sql_value, "Old Name");
        assert_eq!(discrepancy.kv_value, "New Name");
    }
}
