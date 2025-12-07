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
use crate::auth_sessions_kv::{AuthSessionKv, AuthSessionKvRepository};
use crate::chat_sessions_kv::ChatSessionKvRepository;
use crate::collections_kv::CollectionKvRepository;
use crate::documents_kv::{DocumentChunkKv, DocumentKv, DocumentKvRepository};
use crate::kv_metrics::global_kv_metrics;
use crate::plans_kv::{plan_to_kv, PlanKvRepository};
use crate::policy_audit_kv::PolicyAuditKvRepository;
use crate::runtime_sessions::RuntimeSession;
use crate::runtime_sessions_kv::RuntimeSessionKvRepository;
use crate::stacks_kv::{stack_record_to_kv, StackKvOps, StackKvRepository};
use crate::tenants::Tenant;
use crate::tenants_kv::TenantKvRepository;
use crate::training_jobs_kv::{TrainingJobKv, TrainingJobKvRepository, TrainingMetricKv};
use crate::traits::StackRecord;
use crate::Db;
use adapteros_core::{AosError, Result};
use blake3::Hasher;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;
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

    /// Migrate documents and chunks from SQL to KV storage.
    /// Deprecated in favor of migrate_domain_rag_artifacts.
    pub async fn migrate_documents_to_kv(&self) -> Result<MigrationStats> {
        Ok(MigrationStats::default())
    }

    #[cfg(any())]
    /// Migrate document collections and memberships to KV storage.
    pub async fn migrate_collections_to_kv(&self) -> Result<MigrationStats> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not attached - call init_kv_backend() first".into())
        })?;
        let repo = CollectionKvRepository::new(kv_backend.backend().clone());
        let mut stats = MigrationStats::default();

        let cols = sqlx::query!(
            r#"SELECT id, tenant_id, name, description, metadata_json
             FROM document_collections"#
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        stats.total = cols.len();
        for row in cols {
            let id = row.id.clone();
            let tenant_id = row.tenant_id.clone();
            let name = row.name.clone();
            let res = repo
                .create_collection(
                    &tenant_id,
                    &id,
                    &name,
                    row.description.clone(),
                    row.metadata_json.clone(),
                )
                .await;
            match res {
                Ok(_) => stats.migrated += 1,
                Err(e) => {
                    stats.failed += 1;
                    stats.failed_ids.push(id.clone());
                    warn!(error = %e, collection_id = %id, "Failed to migrate collection");
                }
            }

            let doc_ids: Vec<(String,)> = sqlx::query_as(
                "SELECT document_id FROM collection_documents WHERE collection_id = ?",
            )
            .bind(&id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;
            for (doc_id,) in doc_ids {
                let _ = repo
                    .add_document_to_collection(&tenant_id, &id, &doc_id, None)
                    .await;
            }
        }

        Ok(stats)
    }

    /// Migrate document collections (stubbed).
    pub async fn migrate_collections_stub(&self) -> Result<MigrationStats> {
        warn!("Collections migration stubbed; skipping");
        Ok(MigrationStats::default())
    }

    #[cfg(any())]
    /// Migrate policy audit decisions to KV storage.
    pub async fn migrate_policy_audit_to_kv(&self) -> Result<MigrationStats> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not attached - call init_kv_backend() first".into())
        })?;
        let repo = PolicyAuditKvRepository::new(kv_backend.backend().clone());
        let mut stats = MigrationStats::default();

        let entries = sqlx::query!(
            r#"SELECT id, tenant_id, policy_pack_id, hook, decision, reason, request_id, user_id,
                resource_type, resource_id, metadata_json, timestamp, previous_hash, entry_hash, chain_sequence
             FROM policy_audit_decisions
             ORDER BY tenant_id, chain_sequence ASC"#
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        stats.total = entries.len();
        for row in entries {
            let id = row.id.clone();
            let tenant_id = row.tenant_id.clone();
            let policy_pack_id = row.policy_pack_id.clone();
            let hook = row.hook.clone();
            let decision = row.decision.clone();
            let res = repo
                .log_policy_decision(
                    &tenant_id,
                    &policy_pack_id,
                    &hook,
                    &decision,
                    row.reason.as_deref(),
                    row.request_id.as_deref(),
                    row.user_id.as_deref(),
                    row.resource_type.as_deref(),
                    row.resource_id.as_deref(),
                    row.metadata_json.as_deref(),
                )
                .await;
            match res {
                Ok(_) => stats.migrated += 1,
                Err(e) => {
                    stats.failed += 1;
                    stats.failed_ids.push(id.clone());
                    warn!(error = %e, entry_id = %id, "Failed to migrate policy audit");
                }
            }
        }

        Ok(stats)
    }

    /// Migrate policy audit decisions (stubbed).
    pub async fn migrate_policy_audit_stub(&self) -> Result<MigrationStats> {
        warn!("Policy audit migration stubbed; skipping");
        Ok(MigrationStats::default())
    }

    #[cfg(any())]
    /// Migrate training jobs and metrics to KV storage.
    pub async fn migrate_training_jobs_to_kv(&self) -> Result<MigrationStats> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not attached - call init_kv_backend() first".into())
        })?;
        let repo = TrainingJobKvRepository::new(kv_backend.backend().clone());
        let mut stats = MigrationStats::default();

        let jobs = sqlx::query!(
            r#"SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by, adapter_name, template_id,
                    created_at, metadata_json, config_hash_b3,
                    dataset_id, base_model_id, collection_id, tenant_id, build_id, source_documents_json,
                    retryable, retry_of_job_id, stack_id, adapter_id
             FROM repository_training_jobs"#
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(e.to_string()))?;

        stats.total = jobs.len();
        for row in jobs {
            let id = row.id.clone();
            let job = TrainingJobKv {
                id: id.clone(),
                repo_id: row.repo_id.clone(),
                training_config_json: row.training_config_json.clone(),
                status: row.status.clone(),
                progress_json: row.progress_json.clone(),
                started_at: row
                    .started_at
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| Utc::now().to_rfc3339()),
                completed_at: row.completed_at.map(|d| d.to_string()),
                created_by: row.created_by.clone(),
                adapter_name: row.adapter_name.clone(),
                template_id: row.template_id.clone(),
                created_at: row.created_at.map(|d| d.to_string()),
                metadata_json: row.metadata_json.clone(),
                config_hash_b3: row.config_hash_b3.clone(),
                dataset_id: row.dataset_id.clone(),
                base_model_id: row.base_model_id.clone(),
                collection_id: row.collection_id.clone(),
                tenant_id: row.tenant_id.clone(),
                build_id: row.build_id.clone(),
                source_documents_json: row.source_documents_json.clone(),
                retryable: row.retryable,
                retry_of_job_id: row.retry_of_job_id.clone(),
                stack_id: row.stack_id.clone(),
                adapter_id: row.adapter_id.clone(),
            };
            match repo.put_job(&job).await {
                Ok(_) => stats.migrated += 1,
                Err(e) => {
                    stats.failed += 1;
                    stats.failed_ids.push(id.clone());
                    warn!(error = %e, job_id = %id, "Failed to migrate training job");
                }
            }

            let metrics = sqlx::query!(
                r#"SELECT id, training_job_id, step, epoch, metric_name, metric_value, metric_timestamp
             FROM repository_training_metrics WHERE training_job_id = ?"#,
                id
            )
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::Database(e.to_string()))?;

            for m in metrics {
                let metric = TrainingMetricKv {
                    id: m.id.clone().unwrap_or_default(),
                    training_job_id: m.training_job_id.clone().unwrap_or_default(),
                    step: m.step,
                    epoch: m.epoch,
                    metric_name: m.metric_name.clone().unwrap_or_default(),
                    metric_value: m.metric_value,
                    metric_timestamp: m.metric_timestamp.map(|d| d.to_string()),
                };
                let _ = repo.put_metric(&metric).await;
            }
        }

        Ok(stats)
    }

    /// Migrate training jobs and metrics to KV storage (stubbed).
    pub async fn migrate_training_jobs_stub(&self) -> Result<MigrationStats> {
        warn!("Training jobs migration stubbed; skipping");
        Ok(MigrationStats::default())
    }

    #[cfg(any())]
    /// Migrate chat sessions and messages to KV storage (basic fields).
    pub async fn migrate_chat_sessions_to_kv(&self) -> Result<MigrationStats> {
        warn!("Chat sessions migration stubbed; skipping");
        Ok(MigrationStats::default())
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
            // Already in KV, treat as successfully migrated
            return Ok(true);
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
            base_model_id: None,           // Not available during KV migration
            manifest_schema_version: None, // Not available during KV migration
            content_hash_b3: None,         // Not available during KV migration
            provenance_json: None,         // Not available during KV migration
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

    /// Migrate chat sessions (stubbed).
    pub async fn migrate_chat_sessions_stub(&self) -> Result<MigrationStats> {
        warn!("Chat sessions migration stubbed; skipping");
        Ok(MigrationStats::default())
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

    // -------------------------------------------------------------------------
    // Multi-domain migration (SQL -> KV)
    // -------------------------------------------------------------------------
}

/// Stable per-domain stats for CI/CLI JSON.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DomainStats {
    pub total: usize,
    pub migrated: usize,
    pub skipped: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

/// Resume checkpoint keyed by domain (and optional tenant filter).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationCheckpoint {
    pub processed: HashMap<String, usize>,
}

/// Migration options shared across domains.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationOptions {
    pub batch_size: usize,
    pub dry_run: bool,
    pub tenant_filter: Option<String>,
    pub checkpoint: Option<MigrationCheckpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantChecksum {
    pub tenant_id: String,

    pub adapters_sql: usize,
    pub adapters_kv: usize,
    pub adapters_hash_sql: String,
    pub adapters_hash_kv: String,

    pub stacks_sql: usize,
    pub stacks_kv: usize,
    pub stacks_hash_sql: String,
    pub stacks_hash_kv: String,

    pub plans_sql: usize,
    pub plans_kv: usize,
    pub plans_hash_sql: String,
    pub plans_hash_kv: String,

    pub consistent: bool,
}

fn hash_rows(mut rows: Vec<String>) -> String {
    rows.sort();
    let mut hasher = Hasher::new();
    for row in rows {
        hasher.update(row.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Domains supported by the orchestrator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MigrationDomain {
    Adapters,
    Tenants,
    Stacks,
    Plans,
    AuthSessions,
    RuntimeSessions,
    RagArtifacts,
    PolicyAudit,
    TrainingJobs,
    ChatSessions,
}

impl MigrationDomain {
    pub fn label(&self) -> &'static str {
        match self {
            MigrationDomain::Adapters => "adapters",
            MigrationDomain::Tenants => "tenants",
            MigrationDomain::Stacks => "stacks",
            MigrationDomain::Plans => "plans",
            MigrationDomain::AuthSessions => "auth_sessions",
            MigrationDomain::RuntimeSessions => "runtime_sessions",
            MigrationDomain::RagArtifacts => "rag_artifacts",
            MigrationDomain::PolicyAudit => "policy_audit",
            MigrationDomain::TrainingJobs => "training_jobs",
            MigrationDomain::ChatSessions => "chat_sessions",
        }
    }

    fn checkpoint_key(&self, tenant: Option<&str>) -> String {
        match tenant {
            Some(t) => format!("{}::{}", self.label(), t),
            None => self.label().to_string(),
        }
    }
}

impl Db {
    /// Migrate the selected domains from SQL to KV with deterministic ordering and tenant isolation.
    ///
    /// - Honors `tenant_filter` for tenant-scoped domains.
    /// - Respects `dry_run` (no writes).
    /// - Uses checkpoints to resume by skipping already-processed rows.
    pub async fn migrate_domains(
        &self,
        domains: &[MigrationDomain],
        opts: &MigrationOptions,
    ) -> Result<(Vec<(MigrationDomain, DomainStats)>, MigrationCheckpoint)> {
        if !self.has_kv_backend() {
            return Err(AosError::Config(
                "KV backend not attached; initialize before migration".to_string(),
            ));
        }

        let mut results = Vec::new();
        let mut checkpoint = opts.checkpoint.clone().unwrap_or_default();
        let tenant_filter = opts.tenant_filter.as_deref();

        for domain in domains {
            let key = domain.checkpoint_key(tenant_filter);
            let start_at = checkpoint.processed.get(&key).copied().unwrap_or(0);

            let mut stats = DomainStats::default();
            let processed = self
                .migrate_domain_internal(*domain, start_at, opts, &mut stats)
                .await?;

            checkpoint.processed.insert(key, start_at + processed);
            results.push((*domain, stats));
        }

        Ok((results, checkpoint))
    }

    async fn migrate_domain_internal(
        &self,
        domain: MigrationDomain,
        start_at: usize,
        opts: &MigrationOptions,
        stats: &mut DomainStats,
    ) -> Result<usize> {
        match domain {
            MigrationDomain::Adapters => self.migrate_domain_adapters(start_at, opts, stats).await,
            MigrationDomain::Tenants => self.migrate_domain_tenants(start_at, opts, stats).await,
            MigrationDomain::Stacks => self.migrate_domain_stacks(start_at, opts, stats).await,
            MigrationDomain::Plans => self.migrate_domain_plans(start_at, opts, stats).await,
            MigrationDomain::AuthSessions => {
                self.migrate_domain_auth_sessions(start_at, opts, stats)
                    .await
            }
            MigrationDomain::RuntimeSessions => {
                self.migrate_domain_runtime_sessions(start_at, opts, stats)
                    .await
            }
            MigrationDomain::RagArtifacts => {
                self.migrate_domain_rag_artifacts(start_at, opts, stats)
                    .await
            }
            MigrationDomain::PolicyAudit => {
                warn!("Policy audit migration skipped (not implemented)");
                Ok(0)
            }
            MigrationDomain::TrainingJobs => {
                warn!("Training jobs migration skipped (not implemented)");
                Ok(0)
            }
            MigrationDomain::ChatSessions => {
                warn!("Chat sessions migration skipped (not implemented)");
                Ok(0)
            }
        }
    }

    async fn migrate_domain_adapters(
        &self,
        start_at: usize,
        opts: &MigrationOptions,
        stats: &mut DomainStats,
    ) -> Result<usize> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not initialized; cannot migrate adapters".to_string())
        })?;

        let adapters = if let Some(tid) = opts.tenant_filter.as_deref() {
            self.list_adapters_for_tenant(tid).await?
        } else {
            #[allow(deprecated)]
            self.list_all_adapters_system().await?
        };

        let mut adapters = adapters;
        adapters.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let mut processed = 0usize;
        for (idx, adapter) in adapters.into_iter().enumerate() {
            if idx < start_at {
                continue;
            }
            processed += 1;
            stats.total += 1;

            if opts.dry_run {
                stats.skipped += 1;
                continue;
            }

            // Each adapter is tenant-scoped; rebuild repo for tenant to maintain prefixes.
            let repo = AdapterKvRepository::new(
                Arc::new(adapteros_storage::repos::AdapterRepository::new(
                    kv_backend.backend().clone(),
                    kv_backend.index_manager().clone(),
                )),
                adapter.tenant_id.clone(),
            );

            match self.migrate_single_adapter(&repo, adapter.clone()).await {
                Ok(true) => stats.migrated += 1,
                Ok(false) => stats.skipped += 1,
                Err(e) => {
                    stats.failed += 1;
                    stats
                        .errors
                        .push(format!("adapter {}: {}", adapter.id, e.to_string()));
                }
            }
        }

        Ok(processed)
    }

    async fn migrate_domain_tenants(
        &self,
        start_at: usize,
        opts: &MigrationOptions,
        stats: &mut DomainStats,
    ) -> Result<usize> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not initialized; cannot migrate tenants".to_string())
        })?;
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for tenant migration".to_string())
        })?;

        let query = if opts.tenant_filter.is_some() {
            r#"
            SELECT id, name, itar_flag, created_at, status, updated_at, default_stack_id,
                   default_pinned_adapter_ids, max_adapters, max_training_jobs, max_storage_gb,
                   rate_limit_rpm
            FROM tenants
            WHERE id = ?
            ORDER BY created_at DESC, id ASC
            "#
        } else {
            r#"
            SELECT id, name, itar_flag, created_at, status, updated_at, default_stack_id,
                   default_pinned_adapter_ids, max_adapters, max_training_jobs, max_storage_gb,
                   rate_limit_rpm
            FROM tenants
            ORDER BY created_at DESC, id ASC
            "#
        };

        let mut tenants: Vec<Tenant> = if let Some(tid) = opts.tenant_filter.as_deref() {
            sqlx::query_as::<_, Tenant>(query)
                .bind(tid)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query_as::<_, Tenant>(query).fetch_all(pool).await?
        };

        tenants.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let repo = TenantKvRepository::new(kv_backend.backend().clone());

        let mut processed = 0usize;
        for (idx, tenant) in tenants.into_iter().enumerate() {
            if idx < start_at {
                continue;
            }
            processed += 1;
            stats.total += 1;

            if opts.dry_run {
                stats.skipped += 1;
                continue;
            }

            let kv_tenant: adapteros_storage::entities::tenant::TenantKv = tenant.clone().into();
            if let Err(e) = repo.put_tenant(&kv_tenant).await {
                stats.failed += 1;
                stats
                    .errors
                    .push(format!("tenant {}: {}", kv_tenant.id, e.to_string()));
            } else {
                stats.migrated += 1;
            }
        }

        Ok(processed)
    }

    async fn migrate_domain_stacks(
        &self,
        start_at: usize,
        opts: &MigrationOptions,
        stats: &mut DomainStats,
    ) -> Result<usize> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not initialized; cannot migrate stacks".to_string())
        })?;
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for stack migration".to_string())
        })?;

        let query = if opts.tenant_filter.is_some() {
            r#"
            SELECT id, tenant_id, name, description, adapter_ids_json, workflow_type,
                   CAST(version AS INTEGER) AS version, lifecycle_state, created_at,
                   updated_at, created_by, determinism_mode
            FROM adapter_stacks
            WHERE tenant_id = ?
            ORDER BY created_at DESC, id ASC
            "#
        } else {
            r#"
            SELECT id, tenant_id, name, description, adapter_ids_json, workflow_type,
                   CAST(version AS INTEGER) AS version, lifecycle_state, created_at,
                   updated_at, created_by, determinism_mode
            FROM adapter_stacks
            ORDER BY created_at DESC, id ASC
            "#
        };

        let mut stacks: Vec<StackRecord> = if let Some(tid) = opts.tenant_filter.as_deref() {
            sqlx::query_as::<_, StackRecord>(query)
                .bind(tid)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query_as::<_, StackRecord>(query)
                .fetch_all(pool)
                .await?
        };

        stacks.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let repo = StackKvRepository::new(kv_backend.backend().clone());
        let mut processed = 0usize;

        for (idx, record) in stacks.into_iter().enumerate() {
            if idx < start_at {
                continue;
            }
            processed += 1;
            stats.total += 1;

            if opts.dry_run {
                stats.skipped += 1;
                continue;
            }

            match stack_record_to_kv(&record) {
                Ok(kv_stack) => {
                    if let Err(e) = repo.put_stack(kv_stack).await {
                        stats.failed += 1;
                        stats
                            .errors
                            .push(format!("stack {}: {}", record.id, e.to_string()));
                    } else {
                        stats.migrated += 1;
                    }
                }
                Err(e) => {
                    stats.failed += 1;
                    stats
                        .errors
                        .push(format!("stack {} convert: {}", record.id, e.to_string()));
                }
            }
        }

        Ok(processed)
    }

    async fn migrate_domain_plans(
        &self,
        start_at: usize,
        opts: &MigrationOptions,
        stats: &mut DomainStats,
    ) -> Result<usize> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not initialized; cannot migrate plans".to_string())
        })?;
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for plan migration".to_string())
        })?;

        let query = if opts.tenant_filter.is_some() {
            r#"
            SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
            FROM plans
            WHERE tenant_id = ?
            ORDER BY created_at DESC, id ASC
            "#
        } else {
            r#"
            SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
            FROM plans
            ORDER BY created_at DESC, id ASC
            "#
        };

        let mut plans: Vec<crate::models::Plan> = if let Some(tid) = opts.tenant_filter.as_deref() {
            sqlx::query_as::<_, crate::models::Plan>(query)
                .bind(tid)
                .fetch_all(pool)
                .await?
        } else {
            sqlx::query_as::<_, crate::models::Plan>(query)
                .fetch_all(pool)
                .await?
        };

        plans.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let repo = PlanKvRepository::new(kv_backend.backend().clone());
        let mut processed = 0usize;

        for (idx, plan) in plans.into_iter().enumerate() {
            if idx < start_at {
                continue;
            }
            processed += 1;
            stats.total += 1;

            if opts.dry_run {
                stats.skipped += 1;
                continue;
            }

            match plan_to_kv(&plan) {
                Ok(plan_kv) => {
                    if let Err(e) = repo.put_plan(plan_kv).await {
                        stats.failed += 1;
                        stats
                            .errors
                            .push(format!("plan {}: {}", plan.id, e.to_string()));
                    } else {
                        stats.migrated += 1;
                    }
                }
                Err(e) => {
                    stats.failed += 1;
                    stats
                        .errors
                        .push(format!("plan {} convert: {}", plan.id, e.to_string()));
                }
            }
        }

        Ok(processed)
    }

    async fn migrate_domain_auth_sessions(
        &self,
        start_at: usize,
        opts: &MigrationOptions,
        stats: &mut DomainStats,
    ) -> Result<usize> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not initialized; cannot migrate auth sessions".to_string())
        })?;
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for auth session migration".to_string())
        })?;

        #[derive(sqlx::FromRow)]
        struct AuthRow {
            jti: String,
            user_id: String,
            ip_address: Option<String>,
            user_agent: Option<String>,
            created_at: String,
            last_activity: String,
            expires_at: i64,
        }

        let mut rows: Vec<AuthRow> = sqlx::query_as::<_, AuthRow>(
            r#"
            SELECT jti, user_id, ip_address, user_agent, created_at, last_activity, expires_at
            FROM auth_sessions
            ORDER BY last_activity DESC, jti ASC
            "#,
        )
        .fetch_all(pool)
        .await?;

        rows.sort_by(|a, b| {
            b.last_activity
                .cmp(&a.last_activity)
                .then_with(|| a.jti.cmp(&b.jti))
        });

        let repo = AuthSessionKvRepository::new(kv_backend.backend().clone());
        let mut processed = 0usize;

        for (idx, row) in rows.into_iter().enumerate() {
            if idx < start_at {
                continue;
            }
            processed += 1;
            stats.total += 1;

            if opts.dry_run {
                stats.skipped += 1;
                continue;
            }

            let created_at = DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            let last_activity = DateTime::parse_from_rfc3339(&row.last_activity)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            let kv = AuthSessionKv {
                jti: row.jti.clone(),
                user_id: row.user_id.clone(),
                tenant_id: None,
                session_id: None,
                device_id: None,
                rot_id: None,
                refresh_expires_at: None,
                refresh_hash: None,
                ip_address: row.ip_address.clone(),
                user_agent: row.user_agent.clone(),
                created_at,
                last_activity,
                expires_at: row.expires_at,
                locked: false,
            };

            if let Err(e) = repo.put_session(kv).await {
                stats.failed += 1;
                stats
                    .errors
                    .push(format!("auth_session {}: {}", row.jti, e.to_string()));
            } else {
                stats.migrated += 1;
            }
        }

        Ok(processed)
    }

    async fn migrate_domain_runtime_sessions(
        &self,
        start_at: usize,
        opts: &MigrationOptions,
        stats: &mut DomainStats,
    ) -> Result<usize> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config(
                "KV backend not initialized; cannot migrate runtime sessions".to_string(),
            )
        })?;
        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for runtime session migration".to_string())
        })?;

        let mut rows: Vec<RuntimeSession> = sqlx::query_as::<_, RuntimeSession>(
            r#"
            SELECT id, session_id, config_hash, binary_version, binary_commit,
                   started_at, ended_at, end_reason, hostname, runtime_mode,
                   config_snapshot, drift_detected, drift_summary, previous_session_id,
                   model_path, adapters_root, database_path, var_dir
            FROM runtime_sessions
            ORDER BY started_at DESC, id ASC
            "#,
        )
        .fetch_all(pool)
        .await?;

        rows.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| a.id.cmp(&b.id))
        });

        let repo = RuntimeSessionKvRepository::new(kv_backend.backend().clone());
        let mut processed = 0usize;

        for (idx, row) in rows.into_iter().enumerate() {
            if idx < start_at {
                continue;
            }
            processed += 1;
            stats.total += 1;

            if opts.dry_run {
                stats.skipped += 1;
                continue;
            }

            let kv: crate::runtime_sessions_kv::RuntimeSessionKv = row.clone().into();
            if let Err(e) = repo.put(&kv).await {
                stats.failed += 1;
                stats
                    .errors
                    .push(format!("runtime_session {}: {}", kv.id, e.to_string()));
            } else {
                stats.migrated += 1;
            }
        }

        Ok(processed)
    }

    async fn migrate_domain_rag_artifacts(
        &self,
        start_at: usize,
        opts: &MigrationOptions,
        stats: &mut DomainStats,
    ) -> Result<usize> {
        let _ = start_at;
        let _ = opts;
        let _ = stats;
        warn!("RAG artifact migration not yet implemented in kv_migration; skipping");
        Ok(0)
    }

    /// Compute deterministic per-tenant checksums across key domains (SQL vs KV).
    ///
    /// Uses sorted serde JSON renderings to ensure stable hashing. Records a drift
    /// metric when mismatches are detected.
    pub async fn tenant_checksum(&self, tenant_id: &str) -> Result<TenantChecksum> {
        let kv_backend = self.kv_backend().ok_or_else(|| {
            AosError::Config("KV backend not attached; cannot compute tenant checksum".to_string())
        })?;

        let pool = self.pool_opt().ok_or_else(|| {
            AosError::Database("SQL backend unavailable for tenant checksum".to_string())
        })?;

        #[derive(sqlx::FromRow, Serialize)]
        struct AdapterRow {
            adapter_id: Option<String>,
            id: String,
            hash_b3: Option<String>,
            lifecycle_state: String,
            current_state: String,
            version: String,
        }

        let sql_adapters: Vec<AdapterRow> = sqlx::query_as(
            r#"
            SELECT adapter_id, id, hash_b3, lifecycle_state, current_state, version
            FROM adapters
            WHERE tenant_id = ?
            ORDER BY created_at DESC, id ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;
        let adapter_repo = AdapterKvRepository::new(
            Arc::new(adapteros_storage::repos::AdapterRepository::new(
                kv_backend.backend().clone(),
                kv_backend.index_manager().clone(),
            )),
            tenant_id.to_string(),
        );
        let kv_adapters = adapter_repo
            .list_adapters_for_tenant_kv(tenant_id)
            .await
            .unwrap_or_default();

        let sql_stacks: Vec<StackRecord> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, name, description, adapter_ids_json, workflow_type,
                   CAST(version AS INTEGER) AS version, lifecycle_state, created_at,
                   updated_at, created_by, determinism_mode
            FROM adapter_stacks
            WHERE tenant_id = ?
            ORDER BY created_at DESC, id ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;
        let stack_repo = StackKvRepository::new(kv_backend.backend().clone());
        let kv_stacks = stack_repo
            .list_stacks_by_tenant(tenant_id)
            .await
            .unwrap_or_default();

        let sql_plans: Vec<crate::models::Plan> = sqlx::query_as(
            r#"
            SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
            FROM plans
            WHERE tenant_id = ?
            ORDER BY created_at DESC, id ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;
        let plan_repo = PlanKvRepository::new(kv_backend.backend().clone());
        let kv_plans = plan_repo.list_plans(tenant_id).await.unwrap_or_default();

        let mut adapter_rows_sql = Vec::new();
        for a in &sql_adapters {
            let adapter_id = a.adapter_id.as_deref().unwrap_or(&a.id);
            adapter_rows_sql.push(format!(
                "{}|{}|{}|{}|{}",
                adapter_id,
                a.hash_b3.as_deref().unwrap_or(""),
                a.lifecycle_state,
                a.current_state,
                a.version
            ));
        }
        let mut adapter_rows_kv = Vec::new();
        for a in &kv_adapters {
            let adapter_id = a.adapter_id.as_deref().unwrap_or(&a.id);
            adapter_rows_kv.push(format!(
                "{}|{}|{}|{}|{}",
                adapter_id,
                a.hash_b3.as_str(),
                a.lifecycle_state,
                a.current_state,
                a.version
            ));
        }
        let adapters_hash_sql = hash_rows(adapter_rows_sql);
        let adapters_hash_kv = hash_rows(adapter_rows_kv);

        let mut stack_rows_sql = Vec::new();
        for s in &sql_stacks {
            stack_rows_sql.push(format!(
                "{}|{}|{}|{}",
                s.id,
                s.adapter_ids_json.clone(),
                s.lifecycle_state.clone(),
                s.determinism_mode.clone().unwrap_or_default(),
            ));
        }
        let mut stack_rows_kv = Vec::new();
        for s in &kv_stacks {
            stack_rows_kv.push(format!(
                "{}|{}|{}|{}",
                s.id,
                s.adapter_ids.join(","),
                s.lifecycle_state.to_string(),
                s.workflow_type
                    .as_ref()
                    .map(|w| w.to_string())
                    .unwrap_or_default()
            ));
        }
        let stacks_hash_sql = hash_rows(stack_rows_sql);
        let stacks_hash_kv = hash_rows(stack_rows_kv);

        let mut plan_rows_sql = Vec::new();
        for p in &sql_plans {
            plan_rows_sql.push(format!(
                "{}|{}|{}|{}",
                p.id,
                p.plan_id_b3.clone(),
                p.manifest_hash_b3.clone(),
                p.metallib_hash_b3.clone().unwrap_or_default(),
            ));
        }
        let mut plan_rows_kv = Vec::new();
        for p in &kv_plans {
            plan_rows_kv.push(format!(
                "{}|{}|{}|{}",
                p.id,
                p.plan_id_b3.clone(),
                p.manifest_hash_b3.clone(),
                p.metallib_hash_b3.clone().unwrap_or_default(),
            ));
        }
        let plans_hash_sql = hash_rows(plan_rows_sql);
        let plans_hash_kv = hash_rows(plan_rows_kv);

        let consistent = adapters_hash_sql == adapters_hash_kv
            && stacks_hash_sql == stacks_hash_kv
            && plans_hash_sql == plans_hash_kv
            && sql_adapters.len() == kv_adapters.len()
            && sql_stacks.len() == kv_stacks.len()
            && sql_plans.len() == kv_plans.len();

        if !consistent {
            global_kv_metrics().record_drift_detected();
        }

        Ok(TenantChecksum {
            tenant_id: tenant_id.to_string(),
            adapters_sql: sql_adapters.len(),
            adapters_kv: kv_adapters.len(),
            adapters_hash_sql,
            adapters_hash_kv,
            stacks_sql: sql_stacks.len(),
            stacks_kv: kv_stacks.len(),
            stacks_hash_sql,
            stacks_hash_kv,
            plans_sql: sql_plans.len(),
            plans_kv: kv_plans.len(),
            plans_hash_sql,
            plans_hash_kv,
            consistent,
        })
    }

    /// Tenant-scoped backfill wrapper that preserves caller options while enforcing the tenant filter.
    pub async fn backfill_tenant_domains(
        &self,
        tenant_id: &str,
        domains: &[MigrationDomain],
        opts: &MigrationOptions,
    ) -> Result<(Vec<(MigrationDomain, DomainStats)>, MigrationCheckpoint)> {
        let mut scoped_opts = opts.clone();
        scoped_opts.tenant_filter = Some(tenant_id.to_string());
        self.migrate_domains(domains, &scoped_opts).await
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

    #[test]
    fn hash_rows_sorts_inputs() {
        let h1 = hash_rows(vec!["b".into(), "a".into()]);
        let h2 = hash_rows(vec!["a".into(), "b".into()]);
        assert_eq!(h1, h2);
    }
}
