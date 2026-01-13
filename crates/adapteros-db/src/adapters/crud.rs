use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use sqlx::{Row, Transaction, Sqlite};
use adapteros_core::{AosError, Result};
use crate::Db;
use crate::adapters_kv::{AdapterKvOps, AdapterKvRepository};
use crate::kv_metrics::global_kv_metrics;
use adapteros_storage::repos::AdapterRepository;
use crate::adapters::types::*;
use crate::adapters::aos_parser::*;
use adapteros_normalization::extract_repo_identifier_from_metadata;
use serde_json::Value;

impl Db {
    /// Get an AdapterKvRepository if KV writes are enabled
    pub(crate) fn get_adapter_kv_repo(&self, tenant_id: &str) -> Option<AdapterKvRepository> {
        if self.storage_mode().write_to_kv() {
            self.kv_backend().map(|kv| {
                let repo = AdapterRepository::new(kv.backend().clone(), kv.index_manager().clone());
                AdapterKvRepository::new_with_locks(
                    Arc::new(repo),
                    tenant_id.to_string(),
                    kv.increment_locks().clone(),
                )
            })
        } else {
            None
        }
    }

    /// Get tenant_id for an adapter by adapter_id (external ID) with tenant verification
    ///
    /// # Security
    /// This method requires a `requesting_tenant_id` to prevent cross-tenant information
    /// disclosure. The adapter's tenant_id is only returned if it matches the requesting
    /// tenant, enforcing tenant isolation.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter's external ID
    /// * `requesting_tenant_id` - The tenant context making the request (for verification)
    ///
    /// # Returns
    /// * `Ok(Some(tenant_id))` - If adapter exists AND belongs to the requesting tenant
    /// * `Ok(None)` - If adapter doesn't exist OR belongs to a different tenant
    pub(crate) async fn get_adapter_tenant_id(
        &self,
        adapter_id: &str,
        requesting_tenant_id: &str,
    ) -> Result<Option<String>> {
        // SECURITY: Filter by requesting_tenant_id to prevent cross-tenant lookups
        let tenant_id: Option<String> = sqlx::query_scalar(
            "SELECT tenant_id FROM adapters WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(adapter_id)
        .bind(requesting_tenant_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;
        Ok(tenant_id)
    }

    /// Get adapter directly from KV without SQL tenant lookup
    ///
    /// This is used for KV-only adapters that don't exist in SQL.
    /// It queries the BY_ADAPTER_ID index to find the adapter.
    async fn get_adapter_from_kv_direct(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        use adapteros_storage::kv::indexing::adapter_indexes;

        let kv = match self.kv_backend() {
            Some(kv) => kv,
            None => return Ok(None),
        };

        // Query BY_ADAPTER_ID index to find the internal UUID
        let internal_ids = kv
            .index_manager()
            .query_index(adapter_indexes::BY_ADAPTER_ID, adapter_id)
            .await
            .map_err(|e| AosError::database(format!("Failed to query adapter index: {}", e)))?;

        let internal_id = match internal_ids.first() {
            Some(id) => id,
            None => return Ok(None),
        };

        // Load adapter by internal UUID
        let key = format!("adapter:{}", internal_id);
        let bytes = match kv
            .backend()
            .get(&key)
            .await
            .map_err(|e| AosError::database(format!("Failed to get adapter: {}", e)))?
        {
            Some(b) => b,
            None => return Ok(None),
        };

        // Deserialize and convert to Adapter
        let adapter_kv: adapteros_storage::AdapterKv = bincode::deserialize(&bytes)
            .map_err(|e| AosError::database(format!("Failed to deserialize adapter: {}", e)))?;

        Ok(Some(adapter_kv.into()))
    }

    // =========================================================================
    // AOS File Metadata Storage Operations
    // =========================================================================

    /// Store .aos file metadata for an adapter
    ///
    /// Stores extended metadata about an .aos adapter file in the `aos_adapter_metadata` table.
    /// This metadata is used for cache management, staleness detection, and integrity verification.
    ///
    /// # Validation
    ///
    /// This method validates the metadata before storing. Invalid metadata will result in an error.
    /// Use [`validate_aos_metadata`] to pre-validate metadata if needed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use adapteros_db::{Db, StoreAdapterFileMetadataParams};
    ///
    /// let params = StoreAdapterFileMetadataParams::new(
    ///     "adapter-123",
    ///     "/var/adapters/my-adapter.aos",
    ///     "b3:abc123..."
    /// )
    /// .file_size_bytes(1024 * 1024 * 50)
    /// .segment_count(3)
    /// .manifest_schema_version("1.0.0");
    ///
    /// db.store_adapter_file_metadata(params).await?;
    /// ```
    pub async fn store_adapter_file_metadata(
        &self,
        params: StoreAdapterFileMetadataParams,
    ) -> Result<()> {
        // Validate metadata before storing
        let validation = validate_aos_metadata(&params);
        if !validation.is_valid {
            return Err(AosError::validation(format!(
                "Invalid .aos metadata: {}",
                validation.errors.join("; ")
            )));
        }

        // Log warnings but continue
        for warning in &validation.warnings {
            warn!(adapter_id = %params.adapter_id, warning = %warning, "AOS metadata warning");
        }

        // Insert or update (upsert) the metadata
        sqlx::query(
            "INSERT INTO aos_adapter_metadata (
                adapter_id, aos_file_path, aos_file_hash, extracted_weights_path,
                training_data_count, lineage_version, signature_valid, file_size_bytes,
                file_modified_at, segment_count, manifest_schema_version, base_model,
                category, tier, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
            ON CONFLICT(adapter_id) DO UPDATE SET
                aos_file_path = excluded.aos_file_path,
                aos_file_hash = excluded.aos_file_hash,
                extracted_weights_path = excluded.extracted_weights_path,
                training_data_count = excluded.training_data_count,
                lineage_version = excluded.lineage_version,
                signature_valid = excluded.signature_valid,
                file_size_bytes = excluded.file_size_bytes,
                file_modified_at = excluded.file_modified_at,
                segment_count = excluded.segment_count,
                manifest_schema_version = excluded.manifest_schema_version,
                base_model = excluded.base_model,
                category = excluded.category,
                tier = excluded.tier,
                updated_at = datetime('now')",
        )
        .bind(&params.adapter_id)
        .bind(&params.aos_file_path)
        .bind(&params.aos_file_hash)
        .bind(&params.extracted_weights_path)
        .bind(params.training_data_count)
        .bind(&params.lineage_version)
        .bind(params.signature_valid)
        .bind(params.file_size_bytes)
        .bind(&params.file_modified_at)
        .bind(params.segment_count)
        .bind(&params.manifest_schema_version)
        .bind(&params.base_model)
        .bind(&params.category)
        .bind(&params.tier)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to store .aos metadata: {}", e)))?;

        info!(
            adapter_id = %params.adapter_id,
            aos_file_path = %params.aos_file_path,
            "Stored .aos file metadata"
        );

        Ok(())
    }

    /// Retrieve .aos file metadata for an adapter
    ///
    /// Returns the extended metadata for an .aos adapter file from the `aos_adapter_metadata` table.
    ///
    /// # Returns
    ///
    /// - `Ok(Some(metadata))` if metadata exists
    /// - `Ok(None)` if no metadata exists for the adapter
    /// - `Err(...)` on database error
    ///
    /// # Example
    ///
    /// ```ignore
    /// use adapteros_db::Db;
    ///
    /// if let Some(metadata) = db.get_adapter_file_metadata("adapter-123").await? {
    ///     println!("File size: {:?}", metadata.file_size_bytes);
    ///     println!("Segment count: {:?}", metadata.segment_count);
    /// }
    /// ```
    pub async fn get_adapter_file_metadata(
        &self,
        adapter_id: &str,
    ) -> Result<Option<AdapterFileMetadata>> {
        let metadata = sqlx::query_as::<_, AdapterFileMetadata>(
            "SELECT adapter_id, aos_file_path, aos_file_hash, extracted_weights_path,
                    training_data_count, lineage_version, signature_valid, file_size_bytes,
                    file_modified_at, segment_count, manifest_schema_version, base_model,
                    category, tier, created_at, updated_at
             FROM aos_adapter_metadata
             WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to get .aos metadata: {}", e)))?;

        Ok(metadata)
    }

    /// Delete .aos file metadata for an adapter
    ///
    /// Removes the extended metadata entry from the `aos_adapter_metadata` table.
    /// This is typically called during adapter purging or cleanup.
    ///
    /// # Returns
    ///
    /// `true` if metadata was deleted, `false` if no metadata existed
    pub async fn delete_adapter_file_metadata(&self, adapter_id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM aos_adapter_metadata WHERE adapter_id = ?")
            .bind(adapter_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::database(format!("Failed to delete .aos metadata: {}", e)))?;

        let deleted = result.rows_affected() > 0;
        if deleted {
            debug!(adapter_id = %adapter_id, "Deleted .aos file metadata");
        }

        Ok(deleted)
    }

    /// Update adapter metadata based on a parsed .aos manifest.
    ///
    /// Merges manifest metadata into the existing metadata_json and updates
    /// artifact fields (aos path/hash, base model, schema version, content hash).
    pub async fn update_adapter_aos_metadata(&self, update: AosMetadataUpdate) -> Result<()> {
        let adapter_id = update.adapter_id.trim();
        let tenant_id = update.tenant_id.trim();
        if adapter_id.is_empty() {
            return Err(AosError::Validation("adapter_id is required".to_string()));
        }
        if tenant_id.is_empty() {
            return Err(AosError::Validation("tenant_id is required".to_string()));
        }

        let adapter = self
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        let manifest_metadata = update
            .manifest_metadata
            .as_ref()
            .filter(|meta| !meta.is_empty());
        let merged_metadata_json = if let Some(manifest_metadata) = manifest_metadata {
            let mut metadata_map = match adapter.metadata_json.as_deref() {
                Some(raw) => match serde_json::from_str::<Value>(raw) {
                    Ok(Value::Object(map)) => map,
                    Ok(_) => {
                        warn!(
                            adapter_id = %adapter_id,
                            "Existing metadata_json is not an object; replacing with manifest metadata"
                        );
                        serde_json::Map::new()
                    }
                    Err(err) => {
                        warn!(
                            adapter_id = %adapter_id,
                            error = %err,
                            "Failed to parse metadata_json; replacing with manifest metadata"
                        );
                        serde_json::Map::new()
                    }
                },
                None => serde_json::Map::new(),
            };

            for (key, value) in manifest_metadata {
                metadata_map.insert(key.clone(), Value::String(value.clone()));
            }

            Some(
                serde_json::to_string(&Value::Object(metadata_map))
                    .map_err(AosError::Serialization)?,
            )
        } else {
            None
        };

        let aos_file_path = update
            .aos_file_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                Path::new(value)
                    .canonicalize()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| value.to_string())
            });
        let aos_file_hash = update
            .aos_file_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let provenance_json = update
            .provenance_json
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let base_model_id = update
            .base_model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let manifest_schema_version = update
            .manifest_schema_version
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        let content_hash_b3 = update
            .content_hash_b3
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        if self.storage_mode().write_to_sql() {
            let result = sqlx::query(
                "UPDATE adapters SET
                    aos_file_path = COALESCE(?, aos_file_path),
                    aos_file_hash = COALESCE(?, aos_file_hash),
                    metadata_json = COALESCE(?, metadata_json),
                    provenance_json = COALESCE(?, provenance_json),
                    base_model_id = COALESCE(?, base_model_id),
                    manifest_schema_version = COALESCE(?, manifest_schema_version),
                    content_hash_b3 = COALESCE(?, content_hash_b3),
                    updated_at = datetime('now')
                 WHERE tenant_id = ? AND adapter_id = ?",
            )
            .bind(&aos_file_path)
            .bind(&aos_file_hash)
            .bind(&merged_metadata_json)
            .bind(&provenance_json)
            .bind(&base_model_id)
            .bind(&manifest_schema_version)
            .bind(&content_hash_b3)
            .bind(tenant_id)
            .bind(adapter_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update .aos metadata: {}", e)))?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_adapter_aos_metadata".to_string(),
            ));
        }

        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo
                .update_adapter_aos_metadata_kv(
                    adapter_id,
                    aos_file_path.as_deref(),
                    aos_file_hash.as_deref(),
                    merged_metadata_json.as_deref(),
                    provenance_json.as_deref(),
                    base_model_id.as_deref(),
                    manifest_schema_version.as_deref(),
                    content_hash_b3.as_deref(),
                )
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL metadata update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "Metadata update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write",
                        "Failed to update adapter metadata in KV backend"
                    );
                }
            } else {
                debug!(
                    adapter_id = %adapter_id,
                    tenant_id = %tenant_id,
                    mode = "dual-write",
                    "Adapter metadata updated in both SQL and KV backends"
                );
            }
        }

        Ok(())
    }

    /// Update adapter hash fields (content_hash_b3 and manifest_hash)
    ///
    /// Used by hash repair commands to populate missing hashes on legacy adapters.
    /// Only updates fields if the provided value is Some and non-empty.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's internal ID (not adapter_id field)
    /// * `content_hash_b3` - Optional new content hash
    /// * `manifest_hash` - Optional new manifest hash
    pub async fn update_adapter_hashes(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        content_hash_b3: Option<&str>,
        manifest_hash: Option<&str>,
    ) -> Result<()> {
        // Only update non-empty values
        let content_hash = content_hash_b3.filter(|h| !h.trim().is_empty());
        let manifest = manifest_hash.filter(|h| !h.trim().is_empty());

        if content_hash.is_none() && manifest.is_none() {
            return Ok(()); // Nothing to update
        }

        let affected = sqlx::query(
            "UPDATE adapters
             SET content_hash_b3 = COALESCE(?, content_hash_b3),
                 manifest_hash = COALESCE(?, manifest_hash),
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(content_hash)
        .bind(manifest)
        .bind(adapter_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to update adapter hashes: {}", e)))?
        .rows_affected();

        if affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter not found: {}",
                adapter_id
            )));
        }

        info!(
            adapter_id = %adapter_id,
            content_hash = ?content_hash,
            manifest_hash = ?manifest,
            code = "ADAPTER_HASHES_UPDATED",
            "Updated adapter hashes"
        );

        // KV dual-write (tenant_id already verified via parameter)
        if let Some(verified_tenant) = self.get_adapter_tenant_id(adapter_id, tenant_id).await? {
            if let Some(repo) = self.get_adapter_kv_repo(&verified_tenant) {
                let mut patch = AdapterMetadataPatch::default();
                if let Some(hash) = content_hash {
                    patch.content_hash_b3 = Some(hash.to_string());
                }
                if let Some(hash) = manifest {
                    patch.manifest_hash = Some(hash.to_string());
                }

                if patch.has_updates() {
                    if let Err(e) = repo.update_adapter_metadata_kv(adapter_id, &patch).await {
                        warn!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %verified_tenant,
                            mode = "dual-write",
                            "Failed to update adapter hashes in KV backend"
                        );
                    } else {
                        debug!(
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write",
                            "Adapter hashes updated in both SQL and KV backends"
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Update semantic adapter alias fields with lifecycle gating.
    pub async fn update_adapter_alias(&self, adapter_id: &str, alias: Option<&str>) -> Result<()> {
        self.update_adapter_alias_with_gate(adapter_id, alias, &AliasUpdateGateConfig::default())
            .await
    }

    /// Update semantic adapter alias fields with custom gating configuration.
    pub async fn update_adapter_alias_with_gate(
        &self,
        adapter_id: &str,
        alias: Option<&str>,
        gate: &AliasUpdateGateConfig,
    ) -> Result<()> {
        #[allow(deprecated)]
        let adapter = self
            .get_adapter(adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        self.update_adapter_alias_inner(adapter, adapter_id, alias, gate)
            .await
    }

    /// Tenant-scoped alias update with lifecycle gating.
    pub async fn update_adapter_alias_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        alias: Option<&str>,
    ) -> Result<()> {
        self.update_adapter_alias_for_tenant_with_gate(
            tenant_id,
            adapter_id,
            alias,
            &AliasUpdateGateConfig::default(),
        )
        .await
    }

    /// Tenant-scoped alias update with custom gating configuration.
    pub async fn update_adapter_alias_for_tenant_with_gate(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        alias: Option<&str>,
        gate: &AliasUpdateGateConfig,
    ) -> Result<()> {
        let adapter = self
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await?
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))?;

        self.update_adapter_alias_inner(adapter, adapter_id, alias, gate)
            .await
    }

    async fn update_adapter_alias_inner(
        &self,
        adapter: Adapter,
        adapter_id: &str,
        alias: Option<&str>,
        gate: &AliasUpdateGateConfig,
    ) -> Result<()> {
        let update = AdapterAliasUpdate::from_alias(alias)?;
        if update.matches_adapter(&adapter) {
            return Ok(());
        }

        let lifecycle_state = LifecycleState::from_str(&adapter.lifecycle_state).map_err(|_| {
            AosError::Validation(format!(
                "Invalid lifecycle state '{}' for adapter {}",
                adapter.lifecycle_state, adapter_id
            ))
        })?;

        if !lifecycle_state.is_mutable() {
            match lifecycle_state {
                LifecycleState::Ready => {
                    if !gate.allow_ready {
                        return Err(AosError::PolicyViolation(format!(
                            "Alias update requires confirmation for adapter '{}' in ready state",
                            adapter_id
                        )));
                    }
                }
                LifecycleState::Active | LifecycleState::Deprecated => {
                    return Err(AosError::PolicyViolation(format!(
                        "Alias update blocked for adapter '{}' in {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
                LifecycleState::Retired | LifecycleState::Failed => {
                    return Err(AosError::PolicyViolation(format!(
                        "Alias update blocked for adapter '{}' in terminal {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
                _ => {
                    return Err(AosError::PolicyViolation(format!(
                        "Alias update not allowed for adapter '{}' in {} state",
                        adapter_id,
                        lifecycle_state.as_str()
                    )));
                }
            }
        }

        if self.storage_mode().write_to_sql() {
            let result = sqlx::query(
                "UPDATE adapters SET
                    adapter_name = ?,
                    tenant_namespace = ?,
                    domain = ?,
                    purpose = ?,
                    revision = ?,
                    updated_at = datetime('now')
                 WHERE tenant_id = ? AND adapter_id = ?",
            )
            .bind(&update.adapter_name)
            .bind(&update.tenant_namespace)
            .bind(&update.domain)
            .bind(&update.purpose)
            .bind(&update.revision)
            .bind(&adapter.tenant_id)
            .bind(adapter_id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update adapter alias: {}", e)))?;

            if result.rows_affected() == 0 {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        } else if !self.storage_mode().write_to_kv() {
            return Err(AosError::Database(
                "No backend available for update_adapter_alias".to_string(),
            ));
        }

        if let Some(repo) = self.get_adapter_kv_repo(&adapter.tenant_id) {
            if let Err(e) = repo.update_adapter_alias_kv(adapter_id, &update).await {
                self.record_kv_write_fallback("adapters.update_alias");
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %adapter.tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL alias update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::Database(format!(
                        "Alias update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %adapter.tenant_id,
                        mode = "dual-write",
                        "Failed to update adapter alias in KV backend"
                    );
                }
            } else {
                debug!(
                    adapter_id = %adapter_id,
                    tenant_id = %adapter.tenant_id,
                    mode = "dual-write",
                    "Adapter alias updated in both SQL and KV backends"
                );
            }
        }

        Ok(())
    }

    /// Register a new adapter
    ///
    /// Construct parameters using [`AdapterRegistrationBuilder`] to ensure required
    /// fields are provided and validated:
    /// ```no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let params = adapteros_db::AdapterRegistrationBuilder::new()
    ///     .adapter_id("adapter-123")
    ///     .name("My Adapter")
    ///     .hash_b3("b3:0123")
    ///     .rank(8)
    ///     .tier(2)
    ///     .build()?;
    /// db.register_adapter(params).await?;
    /// # Ok(())
    /// # }
    /// ```
    async fn store_aos_metadata_if_present(
        &self,
        adapter_internal_id: &str,
        params: &AdapterRegistrationParams,
    ) -> Result<()> {
        if !self.storage_mode().write_to_sql() {
            return Ok(());
        }

        let (aos_path, aos_hash) = match (&params.aos_file_path, &params.aos_file_hash) {
            (Some(path), Some(hash)) if !path.is_empty() && !hash.is_empty() => (path, hash),
            _ => return Ok(()),
        };

        let mut aos_path = aos_path.clone();
        if let Ok(canonical) = Path::new(&aos_path).canonicalize() {
            aos_path = canonical.to_string_lossy().into_owned();
        }

        let manifest_info = match parse_aos_manifest_metadata(Path::new(&aos_path)) {
            Ok(info) => Some(info),
            Err(err) => {
                warn!(
                    error = %err,
                    path = %aos_path,
                    "Failed to parse .aos manifest metadata"
                );
                None
            }
        };

        let manifest_schema_version = params.manifest_schema_version.clone().or_else(|| {
            manifest_info
                .as_ref()
                .and_then(|info| info.manifest_schema_version.clone())
        });
        let base_model_id = params.base_model_id.clone().or_else(|| {
            manifest_info
                .as_ref()
                .and_then(|info| info.base_model.clone())
        });
        let category = if params.category.trim().is_empty() {
            manifest_info
                .as_ref()
                .and_then(|info| info.category.clone())
        } else {
            Some(params.category.clone())
        };
        let tier = if params.tier.trim().is_empty() {
            manifest_info.as_ref().and_then(|info| info.tier.clone())
        } else {
            Some(params.tier.clone())
        };

        sqlx::query(
            "UPDATE adapters
             SET aos_file_path = ?,
                 aos_file_hash = ?,
                 manifest_schema_version = COALESCE(?, manifest_schema_version),
                 base_model_id = COALESCE(?, base_model_id),
                 metadata_json = COALESCE(?, metadata_json),
                 content_hash_b3 = ?,
                 manifest_hash = COALESCE(?, manifest_hash),
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(&aos_path)
        .bind(&aos_hash)
        .bind(manifest_schema_version.as_deref())
        .bind(base_model_id.as_deref())
        .bind(&params.metadata_json)
        .bind(&params.content_hash_b3)
        .bind(&params.manifest_hash)
        .bind(adapter_internal_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to update adapter AOS fields: {}", e)))?;

        let mut meta = StoreAdapterFileMetadataParams::new(
            adapter_internal_id.to_string(),
            aos_path.clone(),
            aos_hash.clone(),
        );

        if let Ok(fs_meta) = std::fs::metadata(&aos_path) {
            meta = meta.file_size_bytes(fs_meta.len() as i64);
            if let Ok(modified) = fs_meta.modified() {
                let modified: DateTime<Utc> = modified.into();
                meta = meta.file_modified_at(modified.to_rfc3339());
            }
        }
        match read_aos_segment_count(Path::new(&aos_path)) {
            Ok(Some(count)) => {
                meta = meta.segment_count(count);
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    error = %err,
                    path = %aos_path,
                    "Failed to read .aos segment count"
                );
            }
        }

        match read_single_file_adapter_metadata(Path::new(&aos_path)).await {
            Ok(Some(metadata)) => {
                if let Some(count) = metadata.training_data_count {
                    meta = meta.training_data_count(count);
                }
                if let Some(version) = metadata.lineage_version {
                    meta = meta.lineage_version(version);
                }
                if let Some(valid) = metadata.signature_valid {
                    meta = meta.signature_valid(valid);
                }
            }
            Ok(None) => {}
            Err(err) => {
                warn!(
                    error = %err,
                    path = %aos_path,
                    "Failed to read single-file adapter metadata"
                );
            }
        }

        if let Some(ref info) = manifest_info {
            if meta.training_data_count.is_none() {
                if let Some(count) = info.training_data_count {
                    meta = meta.training_data_count(count);
                }
            }
        }

        if let Some(ref version) = manifest_schema_version {
            meta = meta.manifest_schema_version(version.clone());
        }

        if let Some(ref base_model) = base_model_id {
            meta = meta.base_model(base_model.clone());
        }

        if let Some(ref category) = category {
            meta = meta.category(category.clone());
        }

        if let Some(ref tier) = tier {
            meta = meta.tier(tier.clone());
        }

        self.store_adapter_file_metadata(meta).await
    }

    async fn persist_adapter_metadata_from_params(
        &self,
        adapter_internal_id: &str,
        params: &AdapterRegistrationParams,
    ) -> Result<()> {
        let patch = AdapterMetadataPatch::from_params(params);
        if !patch.has_updates() {
            return Ok(());
        }

        if self.storage_mode().write_to_sql() {
            sqlx::query(
                "UPDATE adapters SET
                    aos_file_path = COALESCE(?, aos_file_path),
                    aos_file_hash = COALESCE(?, aos_file_hash),
                    base_model_id = COALESCE(?, base_model_id),
                    manifest_schema_version = COALESCE(?, manifest_schema_version),
                    content_hash_b3 = COALESCE(?, content_hash_b3),
                    metadata_json = COALESCE(?, metadata_json),
                    provenance_json = COALESCE(?, provenance_json),
                    repo_path = COALESCE(?, repo_path),
                    codebase_scope = COALESCE(?, codebase_scope),
                    dataset_version_id = COALESCE(?, dataset_version_id),
                    registration_timestamp = COALESCE(?, registration_timestamp),
                    manifest_hash = COALESCE(?, manifest_hash),
                    updated_at = datetime('now')
                 WHERE id = ?",
            )
            .bind(&patch.aos_file_path)
            .bind(&patch.aos_file_hash)
            .bind(&patch.base_model_id)
            .bind(&patch.manifest_schema_version)
            .bind(&patch.content_hash_b3)
            .bind(&patch.metadata_json)
            .bind(&patch.provenance_json)
            .bind(&patch.repo_path)
            .bind(&patch.codebase_scope)
            .bind(&patch.dataset_version_id)
            .bind(&patch.registration_timestamp)
            .bind(&patch.manifest_hash)
            .bind(adapter_internal_id)
            .execute(self.pool())
            .await
            .map_err(|e| {
                AosError::database(format!("Failed to persist adapter metadata: {}", e))
            })?;
        }

        if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
            if let Err(e) = repo
                .update_adapter_metadata_kv(&params.adapter_id, &patch)
                .await
            {
                self.record_kv_write_fallback("adapters.persist_metadata");
                warn!(
                    error = %e,
                    adapter_id = %params.adapter_id,
                    "KV metadata update failed"
                );
            }
        }

        Ok(())
    }

    async fn update_adapter_aos_fields_if_missing(
        &self,
        adapter_internal_id: &str,
        existing: &Adapter,
        params: &AdapterRegistrationParams,
    ) -> Result<()> {
        if !self.storage_mode().write_to_sql() {
            return Ok(());
        }

        let mut new_path = params
            .aos_file_path
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());
        if let Some(ref mut path) = new_path {
            if let Ok(canonical) = Path::new(path).canonicalize() {
                *path = canonical.to_string_lossy().into_owned();
            }
        }

        let new_hash = params
            .aos_file_hash
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string());

        let missing_path = existing
            .aos_file_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none();
        let missing_hash = existing
            .aos_file_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none();

        let should_update_path = missing_path && new_path.is_some();
        let should_update_hash = missing_hash && new_hash.is_some();
        if !should_update_path && !should_update_hash {
            return Ok(());
        }

        sqlx::query(
            "UPDATE adapters
             SET aos_file_path = CASE WHEN aos_file_path IS NULL OR aos_file_path = '' THEN ? ELSE aos_file_path END,
                 aos_file_hash = CASE WHEN aos_file_hash IS NULL OR aos_file_hash = '' THEN ? ELSE aos_file_hash END,
                 updated_at = datetime('now')
             WHERE id = ?",
        )
        .bind(new_path.as_deref())
        .bind(new_hash.as_deref())
        .bind(adapter_internal_id)
        .execute(self.pool())
        .await
        .map_err(|e| {
            AosError::database(format!(
                "Failed to update adapter .aos fields: {}",
                e
            ))
        })?;

        Ok(())
    }

    async fn record_adapter_session_membership(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        metadata_json: Option<&str>,
    ) -> Result<()> {
        let Some(context) = parse_session_context(metadata_json) else {
            return Ok(());
        };

        self.ensure_dataset_collection_session(
            &context.session_id,
            context.session_name.as_deref(),
            context.session_tags.as_deref(),
            Some(tenant_id),
        )
        .await?;

        self.link_adapter_to_collection_session(
            &context.session_id,
            adapter_id,
            Some("registered"),
            None,
        )
        .await?;

        Ok(())
    }

    pub async fn register_adapter(&self, params: AdapterRegistrationParams) -> Result<String> {
        self.register_adapter_extended(params).await
    }

    /// Register a new adapter with extended fields
    ///
    /// Use [`AdapterRegistrationBuilder`] to construct complex parameter sets:
    /// ```no_run
    /// use adapteros_db::adapters::AdapterRegistrationBuilder;
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) {
    /// let params = AdapterRegistrationBuilder::new()
    ///     .adapter_id("adapter-123")
    ///     .name("My Adapter")
    ///     .hash_b3("abc123...")
    ///     .rank(1)
    ///     .tier(2)
    ///     .category("classification")
    ///     .scope("general")
    ///     .build()
    ///     .expect("required fields");
    /// db.register_adapter_extended(params)
    ///     .await
    ///     .expect("registration succeeds");
    /// # }
    /// ```
    pub async fn register_adapter_extended(
        &self,
        mut params: AdapterRegistrationParams,
    ) -> Result<String> {
        if self.get_tenant(&params.tenant_id).await?.is_none() {
            return Err(AosError::validation(format!(
                "Tenant '{}' does not exist",
                params.tenant_id
            )));
        }

        let is_codebase_adapter = params
            .codebase_scope
            .as_ref()
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false)
            || params
                .repo_id
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false)
            || params
                .repo_path
                .as_ref()
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
        if is_codebase_adapter && params.tenant_id != "system" {
            return Err(AosError::validation(
                "Codebase adapters must be registered under the system tenant".to_string(),
            ));
        }

        let hash_missing = params
            .aos_file_hash
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .is_none();
        if hash_missing {
            if let Some(ref path) = params.aos_file_path {
                let computed = compute_aos_file_hash(Path::new(path))?;
                params.aos_file_hash = Some(computed);
            }
        }

        if let Some(aos_path) = params.aos_file_path.clone() {
            match read_aos_manifest_bytes(Path::new(&aos_path)) {
                Ok(Some(manifest_bytes)) => {
                    let manifest_hash = blake3::hash(&manifest_bytes).to_hex().to_string();
                    if params
                        .manifest_hash
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .is_none()
                    {
                        params.manifest_hash = Some(manifest_hash);
                    }

                    match serde_json::from_slice::<Value>(&manifest_bytes) {
                        Ok(manifest) => {
                            let manifest_schema_version = manifest
                                .get("schema_version")
                                .and_then(value_to_trimmed_string)
                                .or_else(|| {
                                    manifest
                                        .get("manifest_schema_version")
                                        .and_then(value_to_trimmed_string)
                                })
                                .or_else(|| {
                                    manifest.get("version").and_then(value_to_trimmed_string)
                                });

                            if params
                                .manifest_schema_version
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .is_none()
                            {
                                if let Some(version) = manifest_schema_version {
                                    params.manifest_schema_version = Some(version);
                                }
                            }

                            let manifest_category =
                                manifest.get("category").and_then(value_to_trimmed_string);
                            if let Some(category) = manifest_category {
                                let current = params.category.trim();
                                if current.is_empty() || current == "code" {
                                    params.category = category;
                                }
                            }

                            let manifest_tier =
                                manifest.get("tier").and_then(value_to_trimmed_string);
                            if let Some(tier) = manifest_tier {
                                let current = params.tier.trim();
                                if current.is_empty() || current == "warm" {
                                    params.tier = tier;
                                }
                            }

                            if params
                                .metadata_json
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .is_none()
                            {
                                if let Some(metadata_obj) =
                                    manifest.get("metadata").and_then(|v| v.as_object())
                                {
                                    if let Ok(json) = serde_json::to_string(metadata_obj) {
                                        params.metadata_json = Some(json);
                                    }
                                }
                            }

                            if params
                                .base_model_id
                                .as_deref()
                                .map(str::trim)
                                .filter(|value| !value.is_empty())
                                .is_none()
                            {
                                if let Some(model_id) = manifest
                                    .get("base_model_id")
                                    .and_then(value_to_trimmed_string)
                                {
                                    params.base_model_id = Some(model_id);
                                } else if let Some(model_name) =
                                    manifest.get("base_model").and_then(value_to_trimmed_string)
                                {
                                    let tenant_id = params.tenant_id.clone();
                                    match self
                                        .get_model_by_name_for_tenant(&tenant_id, &model_name)
                                        .await
                                    {
                                        Ok(Some(model)) => {
                                            params.base_model_id = Some(model.id);
                                        }
                                        Ok(None) => {
                                            warn!(
                                                base_model = %model_name,
                                                tenant_id = %tenant_id,
                                                "Manifest base model not found for tenant"
                                            );
                                        }
                                        Err(err) => {
                                            warn!(
                                                base_model = %model_name,
                                                tenant_id = %tenant_id,
                                                error = %err,
                                                "Failed to resolve manifest base model"
                                            );
                                        }
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            warn!(
                                path = %aos_path,
                                error = %err,
                                "Failed to parse .aos manifest JSON"
                            );
                        }
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    warn!(
                        path = %aos_path,
                        error = %err,
                        "Failed to read .aos manifest bytes"
                    );
                }
            }
        }

        let normalized_hash_b3 = params
            .hash_b3
            .trim()
            .trim_start_matches("b3:")
            .to_ascii_lowercase();
        let normalized_content_hash = params
            .content_hash_b3
            .trim()
            .trim_start_matches("b3:")
            .to_ascii_lowercase();
        let content_hash_needs_compute =
            normalized_content_hash.is_empty() || normalized_content_hash == normalized_hash_b3;
        let manifest_hash_missing = params
            .manifest_hash
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none();

        if (content_hash_needs_compute || manifest_hash_missing) && params.aos_file_path.is_some() {
            if let Some(aos_path) = params.aos_file_path.clone() {
                match std::fs::read(&aos_path) {
                    Ok(bytes) => match open_aos(&bytes) {
                        Ok(view) => {
                            if manifest_hash_missing {
                                params.manifest_hash =
                                    Some(B3Hash::hash(view.manifest_bytes).to_hex());
                            }

                            let scope_path = serde_json::from_slice::<Value>(view.manifest_bytes)
                                .ok()
                                .and_then(|manifest| manifest.get("metadata").cloned())
                                .and_then(|meta| meta.get("scope_path").cloned())
                                .and_then(|val| val.as_str().map(|s| s.to_string()));

                            let canonical_segment = scope_path
                                .as_deref()
                                .map(compute_scope_hash)
                                .and_then(|scope_hash| {
                                    view.segments.iter().find(|seg| {
                                        seg.backend_tag == BackendTag::Canonical
                                            && seg.scope_hash == scope_hash
                                    })
                                })
                                .or_else(|| {
                                    view.segments
                                        .iter()
                                        .find(|seg| seg.backend_tag == BackendTag::Canonical)
                                })
                                .or_else(|| view.segments.first());

                            if let Some(segment) = canonical_segment {
                                if content_hash_needs_compute {
                                    params.content_hash_b3 =
                                        B3Hash::hash_multi(&[view.manifest_bytes, segment.payload])
                                            .to_hex();
                                }

                                let actual_weights_hash = B3Hash::hash(segment.payload).to_hex();
                                if !normalized_hash_b3.is_empty()
                                    && actual_weights_hash != normalized_hash_b3
                                {
                                    warn!(
                                        path = %aos_path,
                                        expected = %normalized_hash_b3,
                                        actual = %actual_weights_hash,
                                        "Weights hash does not match canonical segment"
                                    );
                                }
                            } else if content_hash_needs_compute {
                                warn!(
                                    path = %aos_path,
                                    "No segments found in .aos bundle; content hash not computed"
                                );
                            }
                        }
                        Err(err) => {
                            warn!(
                                path = %aos_path,
                                error = %err,
                                "Failed to parse .aos file for content hash"
                            );
                        }
                    },
                    Err(err) => {
                        warn!(
                            path = %aos_path,
                            error = %err,
                            "Failed to read .aos file for content hash"
                        );
                    }
                }
            }
        }

        if params.content_hash_b3.trim().is_empty() {
            params.content_hash_b3 = params.hash_b3.clone();
        }

        // Idempotency check: if adapter with same adapter_id exists, verify hash matches
        // This prevents duplicate registrations while allowing safe retries
        if let Some(existing) = self
            .get_adapter_for_tenant(&params.tenant_id, &params.adapter_id)
            .await?
        {
            if existing.hash_b3 == params.hash_b3 {
                // Exact match - return existing ID (idempotent)
                tracing::info!(
                    adapter_id = %params.adapter_id,
                    hash_b3 = %params.hash_b3,
                    existing_id = %existing.id,
                    "Adapter already registered with identical hash - returning existing ID"
                );
                let membership_adapter_id = existing
                    .adapter_id
                    .as_deref()
                    .unwrap_or(params.adapter_id.as_str());
                if let Err(e) = self
                    .record_adapter_session_membership(
                        membership_adapter_id,
                        &params.tenant_id,
                        params.metadata_json.as_deref(),
                    )
                    .await
                {
                    warn!(
                        error = %e,
                        adapter_id = %membership_adapter_id,
                        tenant_id = %params.tenant_id,
                        "Failed to record adapter session membership"
                    );
                }
                if let Err(e) = self
                    .store_aos_metadata_if_present(&existing.id, &params)
                    .await
                {
                    warn!(
                        error = %e,
                        adapter_id = %params.adapter_id,
                        internal_id = %existing.id,
                        "Failed to store .aos metadata for existing adapter"
                    );
                }
                if let Err(e) = self
                    .persist_adapter_metadata_from_params(&existing.id, &params)
                    .await
                {
                    warn!(
                        error = %e,
                        adapter_id = %params.adapter_id,
                        internal_id = %existing.id,
                        "Failed to persist metadata for existing adapter"
                    );
                }
                if let Err(e) = self
                    .update_adapter_aos_fields_if_missing(&existing.id, &existing, &params)
                    .await
                {
                    warn!(
                        error = %e,
                        adapter_id = %params.adapter_id,
                        internal_id = %existing.id,
                        "Failed to update .aos fields for existing adapter"
                    );
                }
                return Ok(existing.id);
            } else {
                // Hash mismatch - conflict error
                return Err(AosError::validation(format!(
                    "Adapter '{}' already registered with different hash (existing: {}, new: {}). \
                     Use a new adapter_id or update the existing adapter.",
                    params.adapter_id, existing.hash_b3, params.hash_b3
                )));
            }
        }

        // Deduplication check: if adapter with same content_hash_b3 exists, return existing ID
        // This prevents duplicate adapters with identical content (unique index on content_hash_b3)
        if let Some(existing) = self
            .find_adapter_by_content_hash(&params.content_hash_b3)
            .await?
        {
            tracing::info!(
                content_hash_b3 = %params.content_hash_b3,
                existing_id = %existing.id,
                existing_adapter_id = %existing.adapter_id.as_deref().unwrap_or("N/A"),
                "Adapter with identical content_hash_b3 already exists - returning existing ID"
            );
            let membership_adapter_id = existing
                .adapter_id
                .as_deref()
                .unwrap_or(params.adapter_id.as_str());
            if let Err(e) = self
                .record_adapter_session_membership(
                    membership_adapter_id,
                    &params.tenant_id,
                    params.metadata_json.as_deref(),
                )
                .await
            {
                warn!(
                    error = %e,
                    adapter_id = %membership_adapter_id,
                    tenant_id = %params.tenant_id,
                    "Failed to record adapter session membership for duplicate content hash"
                );
            }
            if let Err(e) = self
                .store_aos_metadata_if_present(&existing.id, &params)
                .await
            {
                warn!(
                    error = %e,
                    adapter_id = %params.adapter_id,
                    internal_id = %existing.id,
                    "Failed to store .aos metadata for duplicate content hash"
                );
            }
            if let Err(e) = self
                .persist_adapter_metadata_from_params(&existing.id, &params)
                .await
            {
                warn!(
                    error = %e,
                    adapter_id = %params.adapter_id,
                    internal_id = %existing.id,
                    "Failed to persist metadata for duplicate content hash"
                );
            }
            if let Err(e) = self
                .update_adapter_aos_fields_if_missing(&existing.id, &existing, &params)
                .await
            {
                warn!(
                    error = %e,
                    adapter_id = %params.adapter_id,
                    internal_id = %existing.id,
                    "Failed to update .aos fields for duplicate content hash"
                );
            }
            return Ok(existing.id);
        }

        let id = Uuid::now_v7().to_string();
        let mut dual_write_completed = false;
        let dual_write_timer =
            if self.storage_mode().write_to_sql() && self.storage_mode().write_to_kv() {
                Some(Instant::now())
            } else {
                None
            };

        // For dual-write mode with strict atomicity, we need to:
        // 1. Start a transaction BEFORE any writes
        // 2. Execute SQL insert within transaction (don't commit yet)
        // 3. Execute KV write
        // 4. If both succeed, commit the transaction
        // 5. If KV fails, rollback the transaction (not committed yet, so this works atomically)
        let needs_dual_write = self.storage_mode().write_to_sql()
            && self.storage_mode().write_to_kv()
            && self.get_adapter_kv_repo(&params.tenant_id).is_some();

        // Write to SQL when allowed by storage mode
        if self.storage_mode().write_to_sql() {
            if let Some(pool) = self.pool_opt() {
                if needs_dual_write && self.dual_write_requires_strict() {
                    // Use transaction-based atomic dual-write for strict mode
                    let mut tx = sqlx::Acquire::begin(pool)
                        .await
                        .map_err(|e| AosError::database(e.to_string()))?;

                    // SQL insert within transaction (don't commit yet)
                    sqlx::query(
                        "INSERT INTO adapters (id, tenant_id, adapter_id, name, hash_b3, rank, alpha, lora_strength, tier, targets_json, acl_json, languages_json, framework, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, expires_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, aos_file_path, aos_file_hash, base_model_id, recommended_for_moe, manifest_schema_version, content_hash_b3, metadata_json, provenance_json, repo_path, codebase_scope, dataset_version_id, registration_timestamp, manifest_hash, training_dataset_hash_b3, version, lifecycle_state, current_state, pinned, memory_bytes, activation_count, load_state, active)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37, $38, $39, $40, $41, $42, $43, '1.0.0', 'draft', 'unloaded', 0, 0, 0, 'cold', 1)"
                    )
                    .bind(&id)
                    .bind(&params.tenant_id)
                    .bind(&params.adapter_id)
                    .bind(&params.name)
                    .bind(&params.hash_b3)
                    .bind(params.rank)
                    .bind(params.alpha)
                    .bind(&params.lora_strength)
                    .bind(&params.tier)
                    .bind(&params.targets_json)
                    .bind(&params.acl_json)
                    .bind(&params.languages_json)
                    .bind(&params.framework)
                    .bind(&params.category)
                    .bind(&params.scope)
                    .bind(&params.framework_id)
                    .bind(&params.framework_version)
                    .bind(&params.repo_id)
                    .bind(&params.commit_sha)
                    .bind(&params.intent)
                    .bind(&params.expires_at)
                    .bind(&params.adapter_name)
                    .bind(&params.tenant_namespace)
                    .bind(&params.domain)
                    .bind(&params.purpose)
                    .bind(&params.revision)
                    .bind(&params.parent_id)
                    .bind(&params.fork_type)
                    .bind(&params.fork_reason)
                    .bind(&params.aos_file_path)
                    .bind(&params.aos_file_hash)
                    .bind(&params.base_model_id)
                    .bind(params.recommended_for_moe.unwrap_or(true))
                    .bind(&params.manifest_schema_version)
                    .bind(&params.content_hash_b3)
                    .bind(&params.metadata_json)
                    .bind(&params.provenance_json)
                    .bind(&params.repo_path)
                    .bind(&params.codebase_scope)
                    .bind(&params.dataset_version_id)
                    .bind(&params.registration_timestamp)
                    .bind(&params.manifest_hash)
                    .bind(&params.training_dataset_hash_b3)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;

                    // KV write - if this fails, we can rollback SQL (not committed yet)
                    // Initialize WriteAck for tracking this dual-write operation
                    let mut write_ack = WriteAck::new("adapter", &id);
                    write_ack.sql_status = WriteStatus::Ok; // SQL insert succeeded (in transaction)

                    if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
                        match repo.register_adapter_kv_with_id(&id, params.clone()).await {
                            Ok(_) => {
                                // Both succeeded, now commit the SQL transaction
                                tx.commit()
                                    .await
                                    .map_err(|e| AosError::database(e.to_string()))?;
                                dual_write_completed = true;
                                write_ack.kv_status = WriteStatus::Ok;
                                write_ack.complete();
                                debug!(adapter_id = %id, tenant_id = %params.tenant_id, mode = "dual-write-strict", "Adapter registered atomically in both SQL and KV backends");

                                // Record successful dual-write ack
                                if let Err(ack_err) = self.store_ack(&write_ack).await {
                                    warn!(
                                        error = %ack_err,
                                        adapter_id = %id,
                                        operation_id = %write_ack.operation_id,
                                        "Failed to store WriteAck for successful dual-write"
                                    );
                                }
                            }
                            Err(e) => {
                                // KV failed, rollback SQL (not committed yet, so this works atomically)
                                write_ack.kv_status = WriteStatus::Failed {
                                    error: e.to_string(),
                                };
                                write_ack.sql_status = WriteStatus::Pending; // Rolled back
                                write_ack.mark_degraded(
                                    "KV write failed, SQL rolled back in strict mode",
                                );
                                write_ack.complete();

                                error!(
                                    error = %e,
                                    adapter_id = %id,
                                    tenant_id = %params.tenant_id,
                                    mode = "dual-write-strict",
                                    operation_id = %write_ack.operation_id,
                                    "KV write failed in strict atomic mode - rolling back uncommitted SQL transaction"
                                );
                                if let Err(rollback_err) = tx.rollback().await {
                                    error!(
                                        error = %rollback_err,
                                        adapter_id = %id,
                                        "Transaction rollback failed after KV write failure - connection may be in inconsistent state"
                                    );
                                }

                                // Record failed dual-write ack for audit trail
                                if let Err(ack_err) = self.store_ack(&write_ack).await {
                                    warn!(
                                        error = %ack_err,
                                        adapter_id = %id,
                                        operation_id = %write_ack.operation_id,
                                        "Failed to store WriteAck for failed dual-write"
                                    );
                                }

                                return Err(AosError::database(format!(
                                    "KV write failed in strict mode for adapter_id={id}: {e}"
                                )));
                            }
                        }
                    }
                } else {
                    // Non-strict mode or SQL-only: use direct execute (auto-commit)
                    sqlx::query(
                        "INSERT INTO adapters (id, tenant_id, adapter_id, name, hash_b3, rank, alpha, lora_strength, tier, targets_json, acl_json, languages_json, framework, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, expires_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, aos_file_path, aos_file_hash, base_model_id, recommended_for_moe, manifest_schema_version, content_hash_b3, metadata_json, provenance_json, repo_path, codebase_scope, dataset_version_id, registration_timestamp, manifest_hash, training_dataset_hash_b3, version, lifecycle_state, current_state, pinned, memory_bytes, activation_count, load_state, active)
                         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37, $38, $39, $40, $41, $42, $43, '1.0.0', 'draft', 'unloaded', 0, 0, 0, 'cold', 1)"
                    )
                    .bind(&id)
                    .bind(&params.tenant_id)
                    .bind(&params.adapter_id)
                    .bind(&params.name)
                    .bind(&params.hash_b3)
                    .bind(params.rank)
                    .bind(params.alpha)
                    .bind(&params.lora_strength)
                    .bind(&params.tier)
                    .bind(&params.targets_json)
                    .bind(&params.acl_json)
                    .bind(&params.languages_json)
                    .bind(&params.framework)
                    .bind(&params.category)
                    .bind(&params.scope)
                    .bind(&params.framework_id)
                    .bind(&params.framework_version)
                    .bind(&params.repo_id)
                    .bind(&params.commit_sha)
                    .bind(&params.intent)
                    .bind(&params.expires_at)
                    .bind(&params.adapter_name)
                    .bind(&params.tenant_namespace)
                    .bind(&params.domain)
                    .bind(&params.purpose)
                    .bind(&params.revision)
                    .bind(&params.parent_id)
                    .bind(&params.fork_type)
                    .bind(&params.fork_reason)
                    .bind(&params.aos_file_path)
                    .bind(&params.aos_file_hash)
                    .bind(&params.base_model_id)
                    .bind(params.recommended_for_moe.unwrap_or(true))
                    .bind(&params.manifest_schema_version)
                    .bind(&params.content_hash_b3)
                    .bind(&params.metadata_json)
                    .bind(&params.provenance_json)
                    .bind(&params.repo_path)
                    .bind(&params.codebase_scope)
                    .bind(&params.dataset_version_id)
                    .bind(&params.registration_timestamp)
                    .bind(&params.manifest_hash)
                    .bind(&params.training_dataset_hash_b3)
                    .execute(pool)
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;

                    // KV write (non-strict dual-write mode) - best effort, log on failure
                    // Initialize WriteAck for tracking this dual-write operation
                    let mut write_ack = WriteAck::new("adapter", &id);
                    write_ack.sql_status = WriteStatus::Ok; // SQL insert succeeded

                    if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
                        if let Err(e) = repo.register_adapter_kv_with_id(&id, params.clone()).await
                        {
                            write_ack.kv_status = WriteStatus::Failed {
                                error: e.to_string(),
                            };
                            write_ack.mark_degraded("KV write failed in best-effort mode");
                            write_ack.complete();
                            warn!(
                                error = %e,
                                adapter_id = %id,
                                mode = "dual-write",
                                operation_id = %write_ack.operation_id,
                                "Failed to write adapter to KV backend"
                            );

                            // Record degraded dual-write ack for repair queue
                            if let Err(ack_err) = self.store_ack(&write_ack).await {
                                warn!(
                                    error = %ack_err,
                                    adapter_id = %id,
                                    operation_id = %write_ack.operation_id,
                                    "Failed to store WriteAck for degraded dual-write"
                                );
                            }
                        } else {
                            dual_write_completed = true;
                            write_ack.kv_status = WriteStatus::Ok;
                            write_ack.complete();
                            debug!(adapter_id = %id, tenant_id = %params.tenant_id, mode = "dual-write", "Adapter registered in both SQL and KV backends");

                            // Record successful dual-write ack
                            if let Err(ack_err) = self.store_ack(&write_ack).await {
                                warn!(
                                    error = %ack_err,
                                    adapter_id = %id,
                                    operation_id = %write_ack.operation_id,
                                    "Failed to store WriteAck for successful dual-write"
                                );
                            }
                        }
                    } else {
                        // KV repo not available - mark as unavailable
                        write_ack.kv_status = WriteStatus::Unavailable;
                        write_ack.complete();
                    }
                }
            } else if !self.storage_mode().write_to_kv() {
                // No SQL pool and not writing to KV means we cannot satisfy the write
                return Err(AosError::database(
                    "SQL backend unavailable for adapter registration".to_string(),
                ));
            } else {
                // SQL pool unavailable but KV is enabled - write to KV only
                if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
                    repo.register_adapter_kv_with_id(&id, params.clone())
                        .await
                        .map_err(|e| AosError::database(e.to_string()))?;
                }
            }
        } else {
            // SQL writes disabled - write to KV only if enabled
            if let Some(repo) = self.get_adapter_kv_repo(&params.tenant_id) {
                repo.register_adapter_kv_with_id(&id, params.clone())
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;
            }
        }

        if dual_write_completed {
            if let Some(start) = dual_write_timer {
                global_kv_metrics().record_dual_write_lag(start.elapsed());
            }
        }

        if let Err(e) = self.store_aos_metadata_if_present(&id, &params).await {
            warn!(
                error = %e,
                adapter_id = %params.adapter_id,
                internal_id = %id,
                "Failed to store .aos metadata for new adapter"
            );
        }

        if let Err(e) = self
            .record_adapter_session_membership(
                &params.adapter_id,
                &params.tenant_id,
                params.metadata_json.as_deref(),
            )
            .await
        {
            warn!(
                error = %e,
                adapter_id = %params.adapter_id,
                tenant_id = %params.tenant_id,
                "Failed to record adapter session membership for new adapter"
            );
        }

        Ok(id)
    }

    /// Find all expired adapters
    pub async fn find_expired_adapters(&self) -> Result<Vec<Adapter>> {
        deny_unscoped_adapter_query("find_expired_adapters")?;
        let query = format!(
            "SELECT {} FROM adapters WHERE expires_at IS NOT NULL AND expires_at < datetime('now')",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// List all adapters (DEPRECATED - use list_adapters_for_tenant instead)
    ///
    /// WARNING: This method returns ALL adapters across ALL tenants without filtering.
    /// This breaks multi-tenant isolation and should only be used in very specific cases
    /// like system administration or migration scripts where cross-tenant access is required.
    ///
    /// For normal operations, use `list_adapters_for_tenant()` which enforces tenant isolation.
    #[deprecated(
        since = "0.3.0",
        note = "Use list_adapters_for_tenant() for tenant isolation"
    )]
    pub async fn list_adapters(&self) -> Result<Vec<Adapter>> {
        deny_unscoped_adapter_query("list_adapters")?;
        let query = format!(
            "SELECT {} FROM adapters WHERE active = 1 ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// List ALL adapters across ALL tenants for system-level operations.
    ///
    /// This method is explicitly designed for system-level operations that require
    /// cross-tenant visibility, such as:
    /// - Cleanup jobs and garbage collection
    /// - System monitoring and health checks
    /// - Lifecycle management and state recovery
    /// - Administrative dashboards
    /// - Migration scripts
    ///
    /// For normal tenant-scoped operations, use `list_adapters_for_tenant()` instead.
    ///
    /// # Returns
    /// Vector of all active adapters ordered by tier (ascending) and creation date (descending)
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

    /// List adapters for a specific tenant
    ///
    /// This is the RECOMMENDED method for listing adapters as it enforces tenant isolation.
    /// Only returns adapters belonging to the specified tenant.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID to filter by
    ///
    /// # Returns
    /// Vector of adapters belonging to the tenant, ordered by tier (ascending) and creation date (descending)
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::Db;
    ///
    /// # async fn example(db: &Db) -> anyhow::Result<()> {
    /// let adapters = db.list_adapters_for_tenant("tenant-123").await?;
    /// for adapter in adapters {
    ///     println!("Adapter: {} ({})", adapter.name, adapter.id);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    /// Adaptive Query Planner (Phase 2, Item 8)
    ///
    /// Dynamically selects optimal index paths based on tenant data distribution.
    /// Currently enforces the Migration 0210 Golden Index.
    pub fn select_adapter_query_plan(&self, _tenant_id: &str) -> &'static str {
        "idx_adapters_tenant_active_tier_created"
    }

    /// Backward-compatible listing without pagination (defaults to full set).
    pub async fn list_adapters_for_tenant(&self, tenant_id: &str) -> Result<Vec<Adapter>> {
        self.list_adapters_for_tenant_paged(tenant_id, None, None)
            .await
    }

    /// List adapters for a tenant with optional limit/offset.
    pub async fn list_adapters_for_tenant_paged(
        &self,
        tenant_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Adapter>> {
        // Phase 2: Rate Limiting
        if !self.check_rate_limit(tenant_id) {
            return Err(AosError::QuotaExceeded {
                resource: "adapter_listings".to_string(),
                failure_code: Some("RATE_LIMIT_EXCEEDED".to_string()),
            });
        }
        self.increment_rate_limit(tenant_id);

        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo
                    .list_adapters_for_tenant_kv(tenant_id, limit, offset)
                    .await
                {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(tenant_id = %tenant_id, count = adapters.len(), mode = "kv-primary", "Retrieved adapters from KV");
                        return Ok(adapters);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_for_tenant.empty");
                        debug!(tenant_id = %tenant_id, mode = "kv-fallback", "KV returned empty list, falling back to SQL");
                    }
                    Ok(adapters) => {
                        return Ok(adapters);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_for_tenant.error");
                        warn!(error = %e, tenant_id = %tenant_id, mode = "kv-fallback", "KV read failed, falling back to SQL");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read
        // Optimization: Use Migration 0210 composite index explicitly
        let mut query = format!(
            "SELECT {} FROM adapters INDEXED BY idx_adapters_tenant_active_tier_created \
             WHERE tenant_id = ? AND active = 1 \
             ORDER BY tier ASC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        if limit.is_some() {
            query.push_str(" LIMIT ?");
        }
        if offset.is_some() {
            query.push_str(" OFFSET ?");
        }

        #[cfg(test)]
        {
            let index_exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM sqlite_master WHERE type='index' AND name = ? LIMIT 1",
            )
            .bind("idx_adapters_tenant_active_tier_created")
            .fetch_optional(self.pool())
            .await?;

            if index_exists.is_none() {
                // In test environment, index might be missing if migration hasn't run.
                // Fallback to standard query to prevent test failures, but warn.
                warn!("idx_adapters_tenant_active_tier_created missing in test env");
            }
        }

        // Performance monitoring for tenant-scoped queries
        let start_time = std::time::Instant::now();

        // Phase 2: Execution Time Budgets
        let timeout_duration = self.get_query_timeout();
        let pool = self.pool().clone();
        let tenant_id_owned = tenant_id.to_string();

        let adapters_future = async move {
            let mut q = sqlx::query_as::<_, Adapter>(&query).bind(tenant_id_owned);
            if let Some(lim) = limit {
                q = q.bind(lim as i64);
            }
            if let Some(off) = offset {
                q = q.bind(off as i64);
            }
            q.fetch_all(&pool).await
        };

        let adapters = if timeout_duration.as_millis() > 0 {
            tokio::time::timeout(timeout_duration, adapters_future)
                .await
                .map_err(|_| {
                    AosError::PerformanceViolation(format!(
                        "Query timeout after {:?}",
                        timeout_duration
                    ))
                })?
                .map_err(|e| {
                    AosError::database(format!("Failed to list adapters for tenant: {}", e))
                })?
        } else {
            adapters_future.await.map_err(|e| {
                AosError::database(format!("Failed to list adapters for tenant: {}", e))
            })?
        };

        let execution_time = start_time.elapsed();

        // Record performance metrics if monitoring is enabled
        if let Some(monitor_guard) = self.performance_monitor() {
            if let Some(monitor) = monitor_guard.as_ref() {
                let mut monitor_clone = monitor.clone();
                let metrics = crate::QueryMetrics {
                    query_name: "list_adapters_for_tenant".to_string(),
                    execution_time_us: execution_time.as_micros() as u64,
                    rows_returned: Some(adapters.len() as i64),
                    used_index: true,
                    query_plan: Some("idx_adapters_tenant_active_tier_created".to_string()),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    tenant_id: Some(tenant_id.to_string()),
                };
                monitor_clone.record(metrics);

                // Update the monitor in the database
                if let Some(mut monitor_guard_mut) = self.performance_monitor_mut() {
                    if let Some(monitor_ref) = monitor_guard_mut.as_mut() {
                        *monitor_ref = monitor_clone;
                    }
                }
            }
        }

        Ok(adapters)
    }

    /// Delete an adapter by its ID
    ///
    /// This function checks if the adapter is pinned before deletion.
    /// Pinned adapters cannot be deleted until they are unpinned.
    ///
    /// **Pin Enforcement:** Uses `active_pinned_adapters` view as the single source of truth.
    /// The view automatically respects TTL (pinned_until) via SQL filtering, eliminating manual
    /// expiration checks. This ensures consistent pin enforcement across all DB operations.
    /// Implementation: crates/adapteros-db/src/pinned_adapters.rs
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 1.3
    pub async fn delete_adapter(&self, id: &str) -> Result<()> {
        // Get adapter_id and tenant_id for pinning check and KV dual-write
        let adapter_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE id = ?")
                .bind(id)
                .fetch_optional(self.pool())
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

        let (adapter_id, tenant_id) = match adapter_data {
            Some((aid, tid)) => (aid, tid),
            None => {
                // Adapter doesn't exist - nothing to delete
                return Ok(());
            }
        };

        // Check active_pinned_adapters view (single source of truth)
        // View automatically filters expired pins (pinned_until > now())
        let active_pin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM active_pinned_adapters WHERE adapter_id = ?")
                .bind(&adapter_id)
                .fetch_one(self.pool())
                .await
                .unwrap_or(0);

        if active_pin_count > 0 {
            warn!(
                id = %id,
                adapter_id = %adapter_id,
                pin_count = active_pin_count,
                "Attempted to delete adapter with active pins"
            );
            return Err(AosError::PolicyViolation(format!(
                "Cannot delete adapter '{}': adapter has {} active pin(s). Unpin first.",
                adapter_id, active_pin_count
            )));
        }

        // Not pinned - safe to delete
        let sql_start = std::time::Instant::now();
        sqlx::query("DELETE FROM adapters WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        let sql_latency = sql_start.elapsed();

        // KV write (dual-write mode)
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            let kv_start = std::time::Instant::now();
            if let Err(e) = repo.delete_adapter_kv(&adapter_id).await {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL delete committed but KV delete failed in strict mode. KV entry may be orphaned."
                    );
                    return Err(AosError::database(format!(
                        "Adapter deleted in SQL but KV delete failed (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to delete adapter from KV backend");
                }
            } else {
                let kv_latency = kv_start.elapsed();
                // Record dual-write latency lag (KV latency vs SQL latency)
                let lag = if kv_latency > sql_latency {
                    kv_latency.saturating_sub(sql_latency)
                } else {
                    std::time::Duration::ZERO
                };
                global_kv_metrics().record_dual_write_lag(lag);
                debug!(
                    adapter_id = %adapter_id,
                    tenant_id = %tenant_id,
                    mode = "dual-write",
                    sql_latency_ms = sql_latency.as_millis() as u64,
                    kv_latency_ms = kv_latency.as_millis() as u64,
                    lag_ms = lag.as_millis() as u64,
                    "Adapter deleted from both SQL and KV backends"
                );
            }
        }

        // Audit log for adapter deletion
        let metadata = serde_json::json!({
            "adapter_id": adapter_id,
            "deletion_mode": "simple",
            "id": id
        });
        if let Err(e) = self
            .log_audit(
                "system",
                "system",
                &tenant_id,
                "adapter.delete_db",
                "adapter",
                Some(&adapter_id),
                "success",
                None,
                None,
                Some(&metadata.to_string()),
            )
            .await
        {
            warn!(
                adapter_id = %adapter_id,
                error = %e,
                "Failed to log adapter deletion audit (non-fatal)"
            );
        }

        Ok(())
    }

    /// Delete an adapter and all its related entries in a transaction
    ///
    /// This ensures cascade deletion of:
    /// - Adapter record from adapters table
    /// - Any pinned_adapters entries
    /// - Any adapter_stack references (would need additional cleanup)
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 4.1
    pub async fn delete_adapter_cascade(&self, id: &str) -> Result<()> {
        use tracing::info;

        let mut tx = self.begin_write_tx().await?;

        // Get adapter_id and tenant_id for pinning check and KV dual-write
        let adapter_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

        let (adapter_id, tenant_id) = match adapter_data {
            Some((aid, tid)) => (aid, tid),
            None => {
                return Err(AosError::NotFound(format!("Adapter not found: {}", id)));
            }
        };

        // Check active_pinned_adapters view (single source of truth)
        let active_pin_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM active_pinned_adapters WHERE adapter_id = ?")
                .bind(&adapter_id)
                .fetch_one(&mut *tx)
                .await
                .unwrap_or(0);

        if active_pin_count > 0 {
            warn!(
                id = %id,
                adapter_id = %adapter_id,
                pin_count = active_pin_count,
                "Attempted to cascade delete adapter with active pins"
            );
            return Err(AosError::PolicyViolation(format!(
                "Cannot delete adapter '{}': adapter has {} active pin(s)",
                adapter_id, active_pin_count
            )));
        }

        // Delete from pinned_adapters (expired pins)
        // Use subquery to find adapter_pk from adapters.id where adapter_id matches
        sqlx::query(
            "DELETE FROM pinned_adapters WHERE adapter_pk IN
             (SELECT id FROM adapters WHERE adapter_id = ?)",
        )
        .bind(&adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        info!(id = %id, adapter_id = %adapter_id, "Deleting adapter with cascade");

        // Delete the adapter itself
        let sql_start = std::time::Instant::now();
        sqlx::query("DELETE FROM adapters WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        let sql_latency = sql_start.elapsed();

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            let kv_start = std::time::Instant::now();
            if let Err(e) = repo.delete_adapter_kv(&adapter_id).await {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL cascade delete committed but KV delete failed in strict mode. KV entry may be orphaned."
                    );
                    return Err(AosError::database(format!(
                        "Cascade delete succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to cascade delete adapter from KV backend");
                }
            } else {
                let kv_latency = kv_start.elapsed();
                // Record dual-write latency lag (KV latency vs SQL latency)
                let lag = if kv_latency > sql_latency {
                    kv_latency.saturating_sub(sql_latency)
                } else {
                    std::time::Duration::ZERO
                };
                global_kv_metrics().record_dual_write_lag(lag);
                debug!(
                    adapter_id = %adapter_id,
                    tenant_id = %tenant_id,
                    mode = "dual-write",
                    sql_latency_ms = sql_latency.as_millis() as u64,
                    kv_latency_ms = kv_latency.as_millis() as u64,
                    lag_ms = lag.as_millis() as u64,
                    "Adapter cascade deleted from both SQL and KV backends"
                );
            }
        }

        // Audit log for cascade adapter deletion
        let metadata = serde_json::json!({
            "adapter_id": adapter_id,
            "deletion_mode": "cascade",
            "id": id
        });
        if let Err(e) = self
            .log_audit(
                "system",
                "system",
                &tenant_id,
                "adapter.delete_db",
                "adapter",
                Some(&adapter_id),
                "success",
                None,
                None,
                Some(&metadata.to_string()),
            )
            .await
        {
            warn!(
                adapter_id = %adapter_id,
                error = %e,
                "Failed to log cascade adapter deletion audit (non-fatal)"
            );
        }

        Ok(())
    }

    /// Get adapter by ID (DEPRECATED - no tenant isolation)
    ///
    /// # Security Warning
    /// This method does NOT enforce tenant isolation. It can return adapters
    /// from ANY tenant, which is a security risk in multi-tenant environments.
    ///
    /// For tenant-scoped access, use [`get_adapter_for_tenant`] instead.
    ///
    /// # When to use this method
    /// - Internal system operations (migrations, garbage collection)
    /// - Test code where tenant context is not relevant
    /// - Admin-only operations with explicit authorization
    #[deprecated(
        since = "0.1.0",
        note = "Use get_adapter_for_tenant() for tenant-scoped access. This method lacks tenant isolation."
    )]
    pub async fn get_adapter(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        deny_unscoped_adapter_query("get_adapter")?;
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            // SECURITY NOTE: This deprecated function uses direct tenant lookup for KV routing.
            // This is acceptable since the entire function is deprecated and for admin-only use.
            // Use a direct query to get tenant_id for KV lookup (admin path only).
            let tenant_result: Option<String> =
                sqlx::query_scalar("SELECT tenant_id FROM adapters WHERE adapter_id = ?")
                    .bind(adapter_id)
                    .fetch_optional(self.pool())
                    .await
                    .map_err(|e| AosError::database(e.to_string()))?;

            if let Some(tenant_id) = tenant_result {
                if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
                    match repo.get_adapter_kv(adapter_id).await {
                        Ok(Some(adapter)) => {
                            debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-primary", "Retrieved adapter from KV");
                            return Ok(Some(adapter));
                        }
                        Ok(None) if self.storage_mode().sql_fallback_enabled() => {
                            debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV returned None, falling back to SQL");
                        }
                        Ok(None) => {
                            return Ok(None);
                        }
                        Err(e) if self.storage_mode().sql_fallback_enabled() => {
                            warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV read failed, falling back to SQL");
                        }
                        Err(e) => {
                            return Err(e);
                        }
                    }
                }
            } else {
                // SQL doesn't have this adapter - try direct KV lookup for KV-only data
                // This handles the case where data exists only in KV (e.g., during migration)
                if let Some(adapter) = self.get_adapter_from_kv_direct(adapter_id).await? {
                    debug!(adapter_id = %adapter_id, mode = "kv-direct", "Retrieved KV-only adapter");
                    return Ok(Some(adapter));
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE adapter_id = ?",
            ADAPTER_SELECT_FIELDS
        );
        let adapter = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapter)
    }

    /// Get adapter by ID scoped to a tenant (returns None on tenant mismatch).
    pub async fn get_adapter_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Option<Adapter>> {
        // Try KV first if enabled (tenant-scoped repo avoids cross-tenant leakage)
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.get_adapter_kv(adapter_id).await {
                    Ok(Some(adapter)) => {
                        debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-primary", "Retrieved adapter from KV (tenant-scoped)");
                        return Ok(Some(adapter));
                    }
                    Ok(None) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.get_for_tenant.none");
                        debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV returned None, falling back to SQL (tenant-scoped)");
                    }
                    Ok(None) => {
                        return Ok(None);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.get_for_tenant.error");
                        warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV read failed, falling back to SQL (tenant-scoped)");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read (tenant-scoped; supports adapter_id or internal id)
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND (adapter_id = ? OR id = ?) LIMIT 2",
            ADAPTER_SELECT_FIELDS
        );

        // Performance monitoring for tenant-scoped queries
        let start_time = std::time::Instant::now();
        let mut adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(adapter_id)
            .bind(adapter_id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        let execution_time = start_time.elapsed();

        // Record performance metrics if monitoring is enabled
        if let Some(monitor_guard) = self.performance_monitor() {
            if let Some(monitor) = monitor_guard.as_ref() {
                let mut monitor_clone = monitor.clone();
                let metrics = crate::QueryMetrics {
                    query_name: "get_adapter_for_tenant".to_string(),
                    execution_time_us: execution_time.as_micros() as u64,
                    rows_returned: Some(adapters.len() as i64),
                    used_index: true, // Should use composite index from migration 0210
                    query_plan: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    tenant_id: Some(tenant_id.to_string()),
                };
                monitor_clone.record(metrics);

                // Update the monitor in the database
                if let Some(mut monitor_guard_mut) = self.performance_monitor_mut() {
                    if let Some(monitor_ref) = monitor_guard_mut.as_mut() {
                        *monitor_ref = monitor_clone;
                    }
                }
            }
        }

        match adapters.len() {
            0 => Ok(None),
            1 => Ok(Some(adapters.remove(0))),
            _ => Err(AosError::validation(format!(
                "Ambiguous adapter_id '{}' for tenant '{}'",
                adapter_id, tenant_id
            ))),
        }
    }

    /// Find adapter by BLAKE3 hash for deduplication
    ///
    /// Returns an existing active adapter with the same hash_b3 within the specified tenant.
    ///
    /// # Security
    /// This method REQUIRES a tenant context to prevent cross-tenant hash discovery attacks.
    /// An attacker could otherwise probe for adapter existence across tenants by testing hashes.
    ///
    /// # Arguments
    /// * `hash_b3` - The BLAKE3 hash to search for
    /// * `tenant_hint` - REQUIRED tenant context for security isolation
    ///
    /// # Errors
    /// Returns an error if `tenant_hint` is None (security isolation requirement).
    pub async fn find_adapter_by_hash(
        &self,
        hash_b3: &str,
        tenant_hint: Option<&str>,
    ) -> Result<Option<Adapter>> {
        // SECURITY: Require tenant context to prevent cross-tenant hash discovery
        let tenant_id = match tenant_hint {
            Some(tid) => tid,
            None => {
                error!(
                    hash = %hash_b3,
                    "Hash lookup attempted without tenant context - rejecting for security isolation"
                );
                return Err(AosError::validation(
                    "Hash lookup requires tenant context (security isolation)".to_string(),
                ));
            }
        };

        // Delegate to tenant-scoped lookup which enforces proper isolation
        self.find_adapter_by_hash_for_tenant(tenant_id, hash_b3)
            .await
    }

    /// Find adapter by hash within a specific tenant (secure version)
    ///
    /// This is the recommended method for tenant-scoped hash lookups to prevent
    /// cross-tenant adapter discovery via hash collision.
    pub async fn find_adapter_by_hash_for_tenant(
        &self,
        tenant_id: &str,
        hash_b3: &str,
    ) -> Result<Option<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.find_adapter_by_hash_kv(hash_b3).await {
                    Ok(Some(adapter)) => {
                        debug!(tenant_id = %tenant_id, hash = %hash_b3, mode = "kv-primary", "Found adapter by hash in KV");
                        return Ok(Some(adapter));
                    }
                    Ok(None) if self.storage_mode().sql_fallback_enabled() => {
                        debug!(tenant_id = %tenant_id, hash = %hash_b3, mode = "kv-fallback", "Hash not found in KV, falling back to SQL");
                    }
                    Ok(None) => return Ok(None),
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        warn!(error = %e, tenant_id = %tenant_id, hash = %hash_b3, mode = "kv-fallback", "KV lookup failed, falling back to SQL");
                    }
                    Err(e) => return Err(AosError::database(format!("KV lookup failed: {}", e))),
                }
            }
        }

        let start_time = std::time::Instant::now();
        // Updated to use the covering index from migration 0210
        let query = format!(
            "SELECT {} FROM adapters INDEXED BY idx_adapters_tenant_hash_active_covering WHERE tenant_id = ? AND hash_b3 = ? AND active = 1 AND lifecycle_state != 'purged' LIMIT 1",
            ADAPTER_SELECT_FIELDS
        );
        let adapter: Option<Adapter> = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(hash_b3)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| {
                AosError::database(format!("Failed to find adapter by hash for tenant: {}", e))
            })?;
        let execution_time = start_time.elapsed();

        // Performance monitoring
        if let Some(monitor_guard) = self.performance_monitor() {
            if let Some(monitor) = monitor_guard.as_ref() {
                let mut monitor_clone = monitor.clone();
                let metrics = crate::QueryMetrics {
                    query_name: "find_adapter_by_hash_for_tenant".to_string(),
                    execution_time_us: execution_time.as_micros() as u64,
                    rows_returned: Some(if adapter.is_some() { 1 } else { 0 }),
                    used_index: true,
                    query_plan: None,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                    tenant_id: Some(tenant_id.to_string()),
                };
                monitor_clone.record(metrics);

                if let Some(mut monitor_guard_mut) = self.performance_monitor_mut() {
                    if let Some(monitor_ref) = monitor_guard_mut.as_mut() {
                        *monitor_ref = monitor_clone;
                    }
                }
            }
        }

        Ok(adapter)
    }

    /// Find adapter by content hash (BLAKE3 of manifest + weights)
    ///
    /// Used for deduplication during registration - if an adapter with the same
    /// content hash already exists, we return the existing adapter instead of
    /// creating a duplicate.
    pub async fn find_adapter_by_content_hash(
        &self,
        content_hash_b3: &str,
    ) -> Result<Option<Adapter>> {
        // Try KV first if enabled (global lookup - no tenant scoping)
        if self.storage_mode().read_from_kv() {
            if let Some(kv) = self.kv_backend() {
                let repo = AdapterRepository::new(kv.backend().clone(), kv.index_manager().clone());
                match repo.find_by_content_hash(content_hash_b3).await {
                    Ok(Some(adapter_kv)) => {
                        let adapter: Adapter = adapter_kv.into();
                        // Filter for active adapters only (matching SQL behavior)
                        if adapter.active == 1 {
                            debug!(content_hash_b3 = %content_hash_b3, mode = "kv-primary", "Found adapter by content hash in KV");
                            return Ok(Some(adapter));
                        }
                        // Adapter exists but not active, fall through to SQL if enabled
                        if !self.storage_mode().sql_fallback_enabled() {
                            return Ok(None);
                        }
                    }
                    Ok(None) => {
                        // Not found in KV, fall through to SQL if enabled
                        if !self.storage_mode().sql_fallback_enabled() {
                            return Ok(None);
                        }
                    }
                    Err(e) => {
                        if self.storage_mode().sql_fallback_enabled() {
                            debug!(
                                content_hash_b3 = %content_hash_b3,
                                error = %e,
                                "KV lookup failed, falling back to SQL"
                            );
                        } else {
                            return Err(AosError::Database(format!(
                                "Failed to find adapter by content hash: {}",
                                e
                            )));
                        }
                    }
                }
            }
        }

        // SQL lookup using the unique index on content_hash_b3 (from migration 0153)
        let query = format!(
            "SELECT {} FROM adapters WHERE content_hash_b3 = ? AND active = 1 LIMIT 1",
            ADAPTER_SELECT_FIELDS
        );
        let adapter = sqlx::query_as::<_, Adapter>(&query)
            .bind(content_hash_b3)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| {
                AosError::database(format!("Failed to find adapter by content hash: {}", e))
            })?;

        Ok(adapter)
    }

    /// Record adapter activation
    pub async fn record_activation(
        &self,
        adapter_id: &str,
        request_id: Option<&str>,
        gate_value: f64,
        selected: bool,
    ) -> Result<String> {
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO adapter_activations (id, adapter_id, request_id, gate_value, selected) 
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(adapter_id)
        .bind(request_id)
        .bind(gate_value)
        .bind(if selected { 1 } else { 0 })
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;
        Ok(id)
    }

    /// Get adapter activations
    pub async fn get_adapter_activations(
        &self,
        adapter_id: &str,
        limit: i64,
    ) -> Result<Vec<AdapterActivation>> {
        let activations = sqlx::query_as::<_, AdapterActivation>(
            "SELECT id, adapter_id, request_id, gate_value, selected, created_at 
             FROM adapter_activations 
             WHERE adapter_id = ? 
             ORDER BY created_at DESC 
             LIMIT ?",
        )
        .bind(adapter_id)
        .bind(limit)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;
        Ok(activations)
    }

    /// Get adapter activation stats
    pub async fn get_adapter_stats(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<(i64, i64, f64)> {
        let row = sqlx::query(
            "SELECT 
                COUNT(aa.id) as total,
                SUM(aa.selected) as selected_count,
                AVG(aa.gate_value) as avg_gate
             FROM adapter_activations aa
             JOIN adapters a ON aa.adapter_id = a.id
             WHERE a.tenant_id = ? AND (a.adapter_id = ? OR a.id = ?)",
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(adapter_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        let total: i64 = row
            .try_get("total")
            .map_err(|e| AosError::database(e.to_string()))?;
        let selected: i64 = row.try_get("selected_count").unwrap_or(0);
        let avg_gate: f64 = row.try_get("avg_gate").unwrap_or(0.0);

        Ok((total, selected, avg_gate))
    }

    /// Get adapter latency stats from performance summary table
    /// Returns (avg_latency_ms, p95_latency_ms, p99_latency_ms) or None if no data
    pub async fn get_adapter_latency_stats(
        &self,
        adapter_id: &str,
    ) -> Result<Option<(f64, f64, f64)>> {
        let row = sqlx::query(
            r#"SELECT
                COALESCE(avg_latency_us, 0) / 1000.0 as avg_ms,
                COALESCE(p95_latency_us, 0) / 1000.0 as p95_ms,
                COALESCE(p99_latency_us, 0) / 1000.0 as p99_ms
            FROM adapter_performance_summary
            WHERE adapter_id = ?
            ORDER BY window_end DESC
            LIMIT 1"#,
        )
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        match row {
            Some(r) => {
                let avg: f64 = r.try_get("avg_ms").unwrap_or(0.0);
                let p95: f64 = r.try_get("p95_ms").unwrap_or(0.0);
                let p99: f64 = r.try_get("p99_ms").unwrap_or(0.0);
                Ok(Some((avg, p95, p99)))
            }
            None => Ok(None),
        }
    }

    /// Get adapter memory usage from performance metrics (last hour average)
    /// Returns memory usage in MB or None if no data
    pub async fn get_adapter_memory_usage(&self, adapter_id: &str) -> Result<Option<f64>> {
        let row = sqlx::query_scalar::<_, f64>(
            r#"SELECT AVG(memory_used_bytes) / 1024.0 / 1024.0 as memory_mb
            FROM adapter_performance_metrics
            WHERE adapter_id = ? AND recorded_at > datetime('now', '-1 hour')"#,
        )
        .bind(adapter_id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        Ok(row)
    }

    /// Update adapter state
    pub async fn update_adapter_state(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE tenant_id = ? AND (adapter_id = ? OR id = ?)"
        )
        .bind(state)
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(adapter_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo
                .update_adapter_state_kv(adapter_id, state, reason)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL state update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::database(format!(
                        "State update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, state = %state, mode = "dual-write", "Adapter state updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    // Pin/unpin functionality moved to pinned_adapters.rs

    /// Update adapter memory usage
    pub async fn update_adapter_memory(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        memory_bytes: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now') WHERE tenant_id = ? AND (adapter_id = ? OR id = ?)"
        )
        .bind(memory_bytes)
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(adapter_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        // KV write (dual-write mode)
        if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
            if let Err(e) = repo
                .update_adapter_memory_kv(adapter_id, memory_bytes)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL memory update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::database(format!(
                        "Memory update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter memory in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, memory_bytes = %memory_bytes, mode = "dual-write", "Adapter memory updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// Update adapter state with transaction protection
    ///
    /// **Concurrency Safety:** SQLite transactions provide serialization without explicit locks.
    /// The transaction ensures atomic read-check-write, preventing lost updates in concurrent scenarios.
    /// Multiple callers are serialized by SQLite's default isolation level - no application-level
    /// mutexes or row locks required. This optimistic concurrency approach is tested under load
    /// (see tests/stability_reinforcement_tests.rs::test_concurrent_state_update_race_condition).
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 1.1
    pub(crate) async fn update_adapter_state_tx(
        &self,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        let mut tx = self.begin_write_tx().await?;

        // Lock the row and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

        let tenant_id = match row_data {
            Some((_, tid)) => tid,
            None => {
                warn!(adapter_id = %adapter_id, "Adapter not found for state update");
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        };

        // Update state with reason logged
        debug!(adapter_id = %adapter_id, state = %state, reason = %reason,
               "Updating adapter state (transactional)");

        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(state)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo
                .update_adapter_state_kv(adapter_id, state, reason)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL state update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::database(format!(
                        "State update (tx) succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, state = %state, mode = "dual-write", "Adapter state updated in both SQL and KV backends (tx)");
            }
        }

        Ok(())
    }

    /// Compare-and-swap (CAS) update of adapter state
    ///
    /// Atomically updates the adapter state only if the current state matches the expected state.
    /// This prevents TOCTOU (Time-of-Check-to-Time-of-Use) race conditions where two concurrent
    /// requests might both read the same state and try to transition, causing invalid state sequences.
    ///
    /// # Arguments
    /// * `adapter_id` - The adapter to update
    /// * `expected_state` - The state we expect the adapter to be in
    /// * `new_state` - The state to transition to
    /// * `reason` - Human-readable reason for the transition (audit trail)
    ///
    /// # Returns
    /// * `Ok(true)` - State was updated successfully
    /// * `Ok(false)` - State was not updated because current state != expected_state
    /// * `Err(AosError::NotFound)` - Adapter doesn't exist
    /// * `Err(AosError::Database)` - Database error
    ///
    /// # Example
    /// ```ignore
    /// // Only promote from cold to warm if still in cold state
    /// let updated = db.update_adapter_state_cas(
    ///     "adapter-123", "cold", "warm", "promoting for inference"
    /// ).await?;
    /// if !updated {
    ///     // Another request already changed the state - retry or handle conflict
    /// }
    /// ```
    pub(crate) async fn update_adapter_state_cas(
        &self,
        adapter_id: &str,
        expected_state: &str,
        new_state: &str,
        reason: &str,
    ) -> Result<bool> {
        let mut tx = self.begin_write_tx().await?;

        // Lock the row and verify current state
        let row_data: Option<(String, String, String)> = sqlx::query_as(
            "SELECT adapter_id, tenant_id, current_state FROM adapters WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        let (tenant_id, current_state) = match row_data {
            Some((_, tid, state)) => (tid, state),
            None => {
                warn!(adapter_id = %adapter_id, "Adapter not found for CAS state update");
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        };

        // CAS check: only update if current state matches expected
        if current_state != expected_state {
            debug!(
                adapter_id = %adapter_id,
                expected = %expected_state,
                actual = %current_state,
                "CAS state update rejected: state mismatch"
            );
            return Ok(false);
        }

        // State matches, proceed with update
        debug!(
            adapter_id = %adapter_id,
            old_state = %expected_state,
            new_state = %new_state,
            reason = %reason,
            "CAS state update: transitioning"
        );

        sqlx::query(
            "UPDATE adapters SET current_state = ?, updated_at = datetime('now') WHERE adapter_id = ? AND current_state = ?",
        )
        .bind(new_state)
        .bind(adapter_id)
        .bind(expected_state) // Double-check in WHERE clause for atomicity
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo
                .update_adapter_state_kv(adapter_id, new_state, reason)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL state update committed but KV write failed in strict mode (CAS). Use ensure_consistency() to repair."
                    );
                    return Err(AosError::database(format!(
                        "State update (CAS) succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state in KV backend (CAS)");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, new_state = %new_state, mode = "dual-write", "Adapter state updated in both SQL and KV backends (CAS)");
            }
        }

        info!(
            adapter_id = %adapter_id,
            old_state = %expected_state,
            new_state = %new_state,
            reason = %reason,
            "Adapter state CAS update successful"
        );

        Ok(true)
    }

    /// Update adapter memory usage with transaction protection
    ///
    /// **Concurrency Approach:** Optimistic concurrency via SQLite transactions.
    /// Transactions serialize updates without explicit locking. Concurrent memory updates
    /// are handled safely by SQLite's transaction isolation, eliminating the need for
    /// application-level synchronization primitives.
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 1.1
    pub async fn update_adapter_memory_tx(
        &self,
        adapter_id: &str,
        memory_bytes: i64,
    ) -> Result<()> {
        let mut tx = self.begin_write_tx().await?;

        // Verify adapter exists and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

        let tenant_id = match row_data {
            Some((_, tid)) => tid,
            None => {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        };

        debug!(adapter_id = %adapter_id, memory_bytes = %memory_bytes,
               "Updating adapter memory (transactional)");

        sqlx::query(
            "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now') WHERE adapter_id = ?"
        )
        .bind(memory_bytes)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo
                .update_adapter_memory_kv(adapter_id, memory_bytes)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL memory update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::database(format!(
                        "Memory update (tx) succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter memory in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, memory_bytes = %memory_bytes, mode = "dual-write", "Adapter memory updated in both SQL and KV backends (tx)");
            }
        }

        Ok(())
    }

    /// Atomically update both adapter state and memory in a single transaction
    ///
    /// This prevents race conditions where state and memory updates might
    /// interleave, causing inconsistent adapter records.
    ///
    /// Citation: Agent G Stability Reinforcement Plan - Patch 1.1
    pub(crate) async fn update_adapter_state_and_memory(
        &self,
        adapter_id: &str,
        state: &str,
        memory_bytes: i64,
        reason: &str,
    ) -> Result<()> {
        let mut tx = self.begin_write_tx().await?;

        // Verify adapter exists and get tenant_id for KV dual-write
        let row_data: Option<(String, String)> =
            sqlx::query_as("SELECT adapter_id, tenant_id FROM adapters WHERE adapter_id = ?")
                .bind(adapter_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| AosError::database(e.to_string()))?;

        let tenant_id = match row_data {
            Some((_, tid)) => tid,
            None => {
                return Err(AosError::NotFound(format!(
                    "Adapter not found: {}",
                    adapter_id
                )));
            }
        };

        debug!(
            adapter_id = %adapter_id,
            state = %state,
            memory_bytes = %memory_bytes,
            reason = %reason,
            "Updating adapter state and memory atomically"
        );

        // Single UPDATE for both fields - atomic at SQL level
        sqlx::query(
            "UPDATE adapters
             SET current_state = ?, memory_bytes = ?, updated_at = datetime('now')
             WHERE adapter_id = ?",
        )
        .bind(state)
        .bind(memory_bytes)
        .bind(adapter_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| AosError::database(e.to_string()))?;

        // KV write (dual-write mode) - after transaction commit
        if let Some(repo) = self.get_adapter_kv_repo(&tenant_id) {
            if let Err(e) = repo
                .update_adapter_state_and_memory_kv(adapter_id, state, memory_bytes, reason)
                .await
            {
                if self.dual_write_requires_strict() {
                    error!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        mode = "dual-write-strict",
                        "CONSISTENCY WARNING: SQL state/memory update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                    );
                    return Err(AosError::database(format!(
                        "State/memory update succeeded in SQL but failed in KV (strict mode): {e}"
                    )));
                } else {
                    warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter state/memory in KV backend");
                }
            } else {
                debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, state = %state, memory_bytes = %memory_bytes, mode = "dual-write", "Adapter state/memory updated in both SQL and KV backends");
            }
        }

        Ok(())
    }

    /// List adapters by category
    pub async fn list_adapters_by_category(
        &self,
        tenant_id: &str,
        category: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.list_adapters_by_category_kv(tenant_id, category).await {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(tenant_id = %tenant_id, category = %category, count = adapters.len(), mode = "kv-primary", "Retrieved adapters by category from KV");
                        return Ok(adapters);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_by_category.empty");
                        debug!(tenant_id = %tenant_id, category = %category, mode = "kv-fallback", "KV returned empty list, falling back to SQL");
                    }
                    Ok(adapters) => {
                        return Ok(adapters);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_by_category.error");
                        warn!(error = %e, tenant_id = %tenant_id, category = %category, mode = "kv-fallback", "KV read failed, falling back to SQL");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND active = 1 AND category = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(category)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// List adapters by scope (tenant-scoped)
    pub async fn list_adapters_by_scope(
        &self,
        tenant_id: &str,
        scope: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.list_adapters_by_scope_kv(tenant_id, scope).await {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(tenant_id = %tenant_id, scope = %scope, count = adapters.len(), mode = "kv-primary", "Retrieved adapters by scope from KV");
                        return Ok(adapters);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_by_scope.empty");
                        debug!(tenant_id = %tenant_id, scope = %scope, mode = "kv-fallback", "KV returned empty list, falling back to SQL");
                    }
                    Ok(adapters) => {
                        return Ok(adapters);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        self.record_kv_read_fallback("adapters.list_by_scope.error");
                        warn!(error = %e, tenant_id = %tenant_id, scope = %scope, mode = "kv-fallback", "KV read failed, falling back to SQL");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read (tenant-scoped)
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND active = 1 AND scope = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(scope)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// List adapters by state
    pub async fn list_adapters_by_state(
        &self,
        tenant_id: &str,
        state: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        let read_from_kv = self.storage_mode().read_from_kv();
        if read_from_kv && self.storage_mode().sql_fallback_enabled() {
            debug!(tenant_id = %tenant_id, state = %state, mode = "sql-required", "State lookup with tenant isolation, using SQL");
        }
        if read_from_kv {
            // TODO: Add tenant-scoped state index to KV backend
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE tenant_id = ? AND active = 1 AND current_state = ? ORDER BY activation_count DESC, created_at DESC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(tenant_id)
            .bind(state)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// Get adapter state summary
    pub async fn get_adapter_state_summary(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<(String, String, String, i64, i64, f64, Option<String>)>> {
        let summary = sqlx::query(
            "SELECT category, scope, current_state, COUNT(*) as count,
                    SUM(memory_bytes) as total_memory_bytes,
                    AVG(activation_count) as avg_activations,
                    MAX(last_activated) as most_recent_activation
             FROM adapters
             WHERE active = 1
               AND tenant_id = ?
             GROUP BY category, scope, current_state
             ORDER BY category, scope, current_state",
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        let mut result = Vec::new();
        for row in summary {
            let category: String = row
                .try_get("category")
                .map_err(|e| AosError::database(e.to_string()))?;
            let scope: String = row
                .try_get("scope")
                .map_err(|e| AosError::database(e.to_string()))?;
            let state: String = row
                .try_get("current_state")
                .map_err(|e| AosError::database(e.to_string()))?;
            let count: i64 = row
                .try_get("count")
                .map_err(|e| AosError::database(e.to_string()))?;
            let total_memory: i64 = row.try_get("total_memory_bytes").unwrap_or(0);
            let avg_activations: f64 = row.try_get("avg_activations").unwrap_or(0.0);
            let most_recent: Option<String> = row.try_get("most_recent_activation").ok();

            result.push((
                category,
                scope,
                state,
                count,
                total_memory,
                avg_activations,
                most_recent,
            ));
        }

        Ok(result)
    }

    // ============================================================================
    // Adapter Lineage Queries
    // ============================================================================

    /// Get full lineage tree for an adapter (ancestors and descendants)
    ///
    /// Returns all adapters in the lineage tree, including:
    /// - Ancestors (parent, grandparent, etc.)
    /// - The adapter itself
    /// - Descendants (children, grandchildren, etc.)
    ///
    /// Uses recursive CTEs to traverse parent_id relationships.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    pub async fn get_adapter_lineage(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv()
            && self
                .get_adapter_tenant_id(adapter_id, tenant_id)
                .await?
                .is_some()
        {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.get_adapter_lineage_kv(adapter_id).await {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, count = adapters.len(), mode = "kv-primary", "Retrieved lineage from KV");
                        return Ok(adapters);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV returned empty lineage, falling back to SQL");
                    }
                    Ok(adapters) => {
                        return Ok(adapters);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV lineage read failed, falling back to SQL");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "WITH RECURSIVE
             -- Get ancestors (walk up parent_id chain)
             ancestors AS (
                 SELECT {}, 1 as depth
                 FROM adapters
                 WHERE adapter_id = ?
                 UNION ALL
                 SELECT {}, anc.depth + 1
                 FROM adapters a
                 JOIN ancestors anc ON a.id = anc.parent_id
                 WHERE anc.depth < 10  -- Prevent infinite loops
             ),
             -- Get descendants (walk down parent_id references)
             descendants AS (
                 SELECT {}, 1 as depth
                 FROM adapters
                 WHERE adapter_id = ?
                 UNION ALL
                 SELECT {}, desc.depth + 1
                FROM adapters a
                JOIN descendants desc ON a.parent_id = desc.id
                 WHERE desc.depth < 10  -- Prevent infinite loops
             )
             SELECT DISTINCT {}
             FROM (
                 SELECT * FROM ancestors
                 UNION
                 SELECT * FROM descendants
             )
             ORDER BY created_at ASC",
            ADAPTER_SELECT_FIELDS,
            ADAPTER_COLUMNS_ALIAS_A,
            ADAPTER_SELECT_FIELDS,
            ADAPTER_COLUMNS_ALIAS_A,
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .bind(adapter_id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// Get direct children of an adapter
    ///
    /// Returns all adapters that have this adapter as their parent_id.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    pub async fn get_adapter_children(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Vec<Adapter>> {
        // Try KV first if enabled
        if self.storage_mode().read_from_kv()
            && self
                .get_adapter_tenant_id(adapter_id, tenant_id)
                .await?
                .is_some()
        {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                match repo.get_adapter_children_kv(adapter_id).await {
                    Ok(adapters) if !adapters.is_empty() => {
                        debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, count = adapters.len(), mode = "kv-primary", "Retrieved children from KV");
                        return Ok(adapters);
                    }
                    Ok(_) if self.storage_mode().sql_fallback_enabled() => {
                        debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV returned empty children list, falling back to SQL");
                    }
                    Ok(adapters) => {
                        return Ok(adapters);
                    }
                    Err(e) if self.storage_mode().sql_fallback_enabled() => {
                        warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "kv-fallback", "KV children read failed, falling back to SQL");
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        // SQL fallback or primary read
        let query = format!(
            "SELECT {} FROM adapters WHERE parent_id = ? AND active = 1 ORDER BY revision ASC, created_at ASC",
            ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// Get lineage path from root to adapter
    ///
    /// Returns ordered list of adapters from root ancestor to the specified adapter,
    /// tracing the parent_id chain upwards.
    pub async fn get_lineage_path(&self, adapter_id: &str) -> Result<Vec<Adapter>> {
        let query = format!(
            "WITH RECURSIVE lineage AS (
                 SELECT {}, 0 as depth
                 FROM adapters
                 WHERE adapter_id = ?
                 UNION ALL
                 SELECT a.id, a.tenant_id, a.adapter_id, a.name, a.hash_b3, a.rank, a.alpha, a.tier, a.targets_json, a.acl_json,
                        a.languages_json, a.framework, a.category, a.scope, a.framework_id, a.framework_version,
                        a.repo_id, a.commit_sha, a.intent, a.current_state, a.pinned, a.memory_bytes, a.last_activated,
                        a.activation_count, a.expires_at, a.load_state, a.last_loaded_at, a.aos_file_path, a.aos_file_hash,
                        a.adapter_name, a.tenant_namespace, a.domain, a.purpose, a.revision, a.parent_id, a.fork_type, a.fork_reason,
                        a.created_at, a.updated_at, a.active, a.version, a.lifecycle_state, lin.depth + 1
                 FROM adapters a
                 JOIN lineage lin ON a.adapter_id = lin.parent_id
                 WHERE lin.depth < 10  -- Prevent infinite loops
             )
             SELECT {}
             FROM lineage
             ORDER BY depth DESC",
            ADAPTER_SELECT_FIELDS, ADAPTER_SELECT_FIELDS
        );
        let adapters = sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_all(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?;
        Ok(adapters)
    }

    /// Find latest revision number for a given adapter family
    ///
    /// Searches for adapters with matching tenant_namespace, domain, and purpose,
    /// and returns the highest revision number found (e.g., "r042" -> 42).
    ///
    /// Returns None if no adapters found or if revisions don't follow rNNN format.
    pub async fn find_latest_revision(
        &self,
        tenant_namespace: &str,
        domain: &str,
        purpose: &str,
    ) -> Result<Option<i32>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT revision FROM adapters
             WHERE tenant_namespace = ? AND domain = ? AND purpose = ? AND active = 1
             ORDER BY revision DESC
             LIMIT 1",
        )
        .bind(tenant_namespace)
        .bind(domain)
        .bind(purpose)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        if let Some((revision_str,)) = row {
            // Parse revision string (e.g., "r042" -> 42)
            if let Some(stripped) = revision_str.strip_prefix('r') {
                if let Ok(rev_num) = stripped.parse::<i32>() {
                    return Ok(Some(rev_num));
                }
            }
        }

        Ok(None)
    }

    /// Validate revision gap constraint
    ///
    /// Ensures that the difference between the highest and lowest revision numbers
    /// in an adapter family does not exceed max_gap (default: 5).
    ///
    /// Returns Ok(()) if constraint is satisfied, Err otherwise.
    pub async fn validate_revision_gap(
        &self,
        tenant_namespace: &str,
        domain: &str,
        purpose: &str,
        max_gap: i32,
    ) -> Result<()> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT revision FROM adapters
             WHERE tenant_namespace = ? AND domain = ? AND purpose = ? AND active = 1
             ORDER BY revision ASC",
        )
        .bind(tenant_namespace)
        .bind(domain)
        .bind(purpose)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        if rows.len() < 2 {
            return Ok(()); // No gap if only 0-1 adapters
        }

        let mut revisions: Vec<i32> = Vec::new();
        for (revision_str,) in rows {
            if let Some(stripped) = revision_str.strip_prefix('r') {
                if let Ok(rev_num) = stripped.parse::<i32>() {
                    revisions.push(rev_num);
                }
            }
        }

        if revisions.is_empty() {
            return Ok(());
        }

        let min_rev = *revisions.iter().min().unwrap_or(&0);
        let max_rev = *revisions.iter().max().unwrap_or(&0);
        let gap = max_rev - min_rev;

        if gap > max_gap {
            return Err(AosError::validation(format!(
                "Revision gap ({}) exceeds maximum allowed ({}) for adapter family {}/{}/{}",
                gap, max_gap, tenant_namespace, domain, purpose
            )));
        }

        Ok(())
    }

    /// Update adapter tier
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    /// * `tier` - The new tier value
    pub async fn update_adapter_tier(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        tier: &str,
    ) -> Result<()> {
        // SECURITY: Update only within tenant scope
        sqlx::query(
            "UPDATE adapters SET tier = ?, updated_at = datetime('now') WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(tier)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to update adapter tier: {}", e)))?;

        // KV write (dual-write mode) - tenant verified via parameter
        if self
            .get_adapter_tenant_id(adapter_id, tenant_id)
            .await?
            .is_some()
        {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                if let Err(e) = repo.update_adapter_tier_kv(adapter_id, tier).await {
                    if self.dual_write_requires_strict() {
                        error!(
                            error = %e,
                            adapter_id = %adapter_id,
                            tenant_id = %tenant_id,
                            mode = "dual-write-strict",
                            "CONSISTENCY WARNING: SQL tier update committed but KV write failed in strict mode. Use ensure_consistency() to repair."
                        );
                        return Err(AosError::database(format!(
                            "Tier update succeeded in SQL but failed in KV (strict mode): {e}"
                        )));
                    } else {
                        warn!(error = %e, adapter_id = %adapter_id, tenant_id = %tenant_id, mode = "dual-write", "Failed to update adapter tier in KV backend");
                    }
                } else {
                    debug!(adapter_id = %adapter_id, tenant_id = %tenant_id, tier = %tier, mode = "dual-write", "Adapter tier updated in both SQL and KV backends");
                }
            }
        }

        Ok(())
    }

    /// Update runtime LoRA strength multiplier
    pub async fn update_adapter_strength(
        &self,
        adapter_id: &str,
        lora_strength: f32,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE adapters SET lora_strength = ?, updated_at = datetime('now') WHERE adapter_id = ?",
        )
        .bind(lora_strength)
        .bind(adapter_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(format!("Failed to update adapter strength: {}", e)))?;

        Ok(())
    }

    /// Ensure consistency between SQL and KV storage for a single adapter.
    ///
    /// Returns:
    /// - Ok(true) if adapter is consistent or was repaired
    /// - Ok(false) if adapter not found in SQL
    pub async fn ensure_consistency(&self, adapter_id: &str) -> Result<bool> {
        // Get adapter from SQL (source of truth during migration)
        let query = format!(
            "SELECT {} FROM adapters WHERE adapter_id = ?",
            ADAPTER_SELECT_FIELDS
        );
        let adapter = match sqlx::query_as::<_, Adapter>(&query)
            .bind(adapter_id)
            .fetch_optional(self.pool())
            .await
            .map_err(|e| AosError::database(e.to_string()))?
        {
            Some(a) => a,
            None => return Ok(false),
        };

        // If KV is not available, consider consistent
        let repo = match self.get_adapter_kv_repo(&adapter.tenant_id) {
            Some(r) => r,
            None => return Ok(true),
        };

        // Check KV entry
        match repo.get_adapter_kv(adapter_id).await {
            Ok(Some(kv_adapter)) => {
                let fields_match = kv_adapter.hash_b3 == adapter.hash_b3
                    && kv_adapter.tier == adapter.tier
                    && kv_adapter.current_state == adapter.current_state
                    && kv_adapter.memory_bytes == adapter.memory_bytes;

                if fields_match {
                    return Ok(true);
                }

                // Repair by re-registering from SQL data
                warn!(
                    adapter_id = %adapter_id,
                    tenant_id = %adapter.tenant_id,
                    "Inconsistency detected between SQL and KV - repairing from SQL"
                );

                let params = AdapterRegistrationParams {
                    tenant_id: adapter.tenant_id.clone(),
                    adapter_id: adapter
                        .adapter_id
                        .clone()
                        .unwrap_or_else(|| adapter_id.to_string()),
                    name: adapter.name.clone(),
                    hash_b3: adapter.hash_b3.clone(),
                    rank: adapter.rank,
                    tier: adapter.tier.clone(),
                    alpha: adapter.alpha,
                    lora_strength: adapter.lora_strength,
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
                    base_model_id: adapter.base_model_id.clone(),
                    recommended_for_moe: adapter.recommended_for_moe,
                    manifest_schema_version: adapter.manifest_schema_version.clone(),
                    // Use existing content_hash_b3 or fall back to hash_b3 for legacy adapters
                    content_hash_b3: adapter
                        .content_hash_b3
                        .clone()
                        .unwrap_or_else(|| adapter.hash_b3.clone()),
                    provenance_json: adapter.provenance_json.clone(),
                    metadata_json: adapter.metadata_json.clone(),
                    repo_path: adapter.repo_path.clone(),
                    // These fields may not exist on legacy adapters
                    codebase_scope: adapter.codebase_scope.clone(),
                    dataset_version_id: adapter.dataset_version_id.clone(),
                    registration_timestamp: adapter.registration_timestamp.clone(),
                    manifest_hash: adapter.manifest_hash.clone(),
                    // Codebase adapter type and stream binding
                    adapter_type: adapter.adapter_type.clone(),
                    base_adapter_id: adapter.base_adapter_id.clone(),
                    stream_session_id: adapter.stream_session_id.clone(),
                    versioning_threshold: adapter.versioning_threshold,
                    coreml_package_hash: adapter.coreml_package_hash.clone(),
                    training_dataset_hash_b3: adapter.training_dataset_hash_b3.clone(),
                };

                // Delete old KV entry then re-register and sync state/memory
                let _ = repo.delete_adapter_kv(adapter_id).await;
                repo.register_adapter_kv(params)
                    .await
                    .map_err(|e| AosError::database(format!("Failed to repair KV entry: {}", e)))?;
                repo.update_adapter_state_kv(
                    adapter_id,
                    &adapter.current_state,
                    "consistency_repair",
                )
                .await
                .map_err(|e| AosError::database(format!("Failed to repair KV state: {}", e)))?;
                repo.update_adapter_memory_kv(adapter_id, adapter.memory_bytes)
                    .await
                    .map_err(|e| {
                        AosError::database(format!("Failed to repair KV memory: {}", e))
                    })?;

                Ok(true)
            }
            Ok(None) => {
                // Missing in KV - create it
                warn!(
                    adapter_id = %adapter_id,
                    tenant_id = %adapter.tenant_id,
                    "Adapter missing in KV - creating from SQL"
                );

                let params = AdapterRegistrationParams {
                    tenant_id: adapter.tenant_id.clone(),
                    adapter_id: adapter
                        .adapter_id
                        .clone()
                        .unwrap_or_else(|| adapter_id.to_string()),
                    name: adapter.name.clone(),
                    hash_b3: adapter.hash_b3.clone(),
                    rank: adapter.rank,
                    tier: adapter.tier.clone(),
                    alpha: adapter.alpha,
                    lora_strength: adapter.lora_strength,
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
                    base_model_id: adapter.base_model_id.clone(),
                    recommended_for_moe: adapter.recommended_for_moe,
                    manifest_schema_version: adapter.manifest_schema_version.clone(),
                    // Use existing content_hash_b3 or fall back to hash_b3 for legacy adapters
                    content_hash_b3: adapter
                        .content_hash_b3
                        .clone()
                        .unwrap_or_else(|| adapter.hash_b3.clone()),
                    provenance_json: adapter.provenance_json.clone(),
                    metadata_json: adapter.metadata_json.clone(),
                    repo_path: adapter.repo_path.clone(),
                    // These fields may not exist on legacy adapters
                    codebase_scope: adapter.codebase_scope.clone(),
                    dataset_version_id: adapter.dataset_version_id.clone(),
                    registration_timestamp: adapter.registration_timestamp.clone(),
                    manifest_hash: adapter.manifest_hash.clone(),
                    // Codebase adapter type and stream binding
                    adapter_type: adapter.adapter_type.clone(),
                    base_adapter_id: adapter.base_adapter_id.clone(),
                    stream_session_id: adapter.stream_session_id.clone(),
                    versioning_threshold: adapter.versioning_threshold,
                    coreml_package_hash: adapter.coreml_package_hash.clone(),
                    training_dataset_hash_b3: adapter.training_dataset_hash_b3.clone(),
                };

                repo.register_adapter_kv(params).await.map_err(|e| {
                    AosError::database(format!("Failed to create adapter in KV: {}", e))
                })?;
                repo.update_adapter_state_kv(
                    adapter_id,
                    &adapter.current_state,
                    "consistency_repair",
                )
                .await
                .map_err(|e| AosError::database(format!("Failed to sync state to KV: {}", e)))?;
                repo.update_adapter_memory_kv(adapter_id, adapter.memory_bytes)
                    .await
                    .map_err(|e| {
                        AosError::database(format!("Failed to sync memory to KV: {}", e))
                    })?;

                Ok(true)
            }
            Err(e) => Err(AosError::database(format!(
                "Consistency check failed: {}",
                e
            ))),
        }
    }

    /// Batch ensure consistency for multiple adapters
    pub async fn ensure_consistency_batch(
        &self,
        adapter_ids: &[String],
    ) -> Vec<(String, Result<bool>)> {
        let mut results = Vec::new();

        for adapter_id in adapter_ids {
            let res = self.ensure_consistency(adapter_id).await;
            results.push((adapter_id.clone(), res));
        }

        results
    }

    /// Validate consistency for all adapters in a tenant
    ///
    /// Returns (consistent, inconsistent, errors)
    pub async fn validate_tenant_consistency(
        &self,
        tenant_id: &str,
        repair: bool,
    ) -> Result<(usize, usize, usize)> {
        let adapters = self.list_adapters_for_tenant(tenant_id).await?;

        let mut consistent = 0usize;
        let mut inconsistent = 0usize;
        let mut errors = 0usize;

        for adapter in adapters {
            if let Some(adapter_id) = &adapter.adapter_id {
                if repair {
                    match self.ensure_consistency(adapter_id).await {
                        Ok(true) => consistent += 1,
                        Ok(false) => {}
                        Err(_) => {
                            inconsistent += 1;
                            errors += 1;
                        }
                    }
                } else {
                    // Check-only path (no repair)
                    match self.get_adapter_kv_repo(&adapter.tenant_id) {
                        None => {
                            consistent += 1;
                        }
                        Some(repo) => match repo.get_adapter_kv(adapter_id).await {
                            Ok(Some(kv_adapter)) => {
                                let fields_match = kv_adapter.hash_b3 == adapter.hash_b3
                                    && kv_adapter.tier == adapter.tier
                                    && kv_adapter.current_state == adapter.current_state
                                    && kv_adapter.memory_bytes == adapter.memory_bytes;

                                if fields_match {
                                    consistent += 1;
                                } else {
                                    inconsistent += 1;
                                }
                            }
                            Ok(None) => {
                                inconsistent += 1;
                            }
                            Err(_) => {
                                inconsistent += 1;
                                errors += 1;
                            }
                        },
                    }
                }
            }
        }

        Ok((consistent, inconsistent, errors))
    }

    /// Clean up orphaned adapters in KV that don't exist in SQL.
    ///
    /// During dual-write mode, inconsistencies can occur where an adapter
    /// exists in KV but not in SQL (e.g., from failed rollbacks, interrupted
    /// operations, or bugs). This method finds and removes such orphans.
    ///
    /// # Algorithm
    /// 1. List all adapter IDs from SQL for the tenant
    /// 2. List all adapter IDs from KV for the tenant
    /// 3. Find KV entries that don't exist in SQL
    /// 4. Delete each orphaned KV entry
    ///
    /// # Returns
    /// Count of orphaned KV entries that were deleted
    ///
    /// # Safety
    /// This operation is safe because SQL is the source of truth during
    /// the migration period. Any adapter in KV that doesn't exist in SQL
    /// is definitionally orphaned and should be cleaned up.
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

    // =========================================================================
    // Archive & Garbage Collection Operations (from migration 0138)
    // =========================================================================

    /// Archive adapters for a tenant (cascade from tenant archival)
    ///
    /// Sets `archived_at` timestamp for all active, non-archived adapters
    /// belonging to the tenant. Does NOT delete .aos files - that's handled by GC.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant whose adapters to archive
    /// * `archived_by` - User/system initiating the archive
    /// * `reason` - Human-readable reason (e.g., "tenant_archived")
    ///
    /// # Returns
    /// Number of adapters archived
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

    /// Archive a single adapter
    ///
    /// Sets `archived_at` timestamp. Does NOT delete .aos file.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    /// * `archived_by` - Who is archiving
    /// * `reason` - Reason for archiving
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

    /// Find archived adapters eligible for garbage collection
    ///
    /// Returns adapters where:
    /// - `archived_at` is older than `min_age_days`
    /// - `purged_at` is NULL (not yet purged)
    /// - `aos_file_path` is NOT NULL (file reference exists)
    ///
    /// # Arguments
    /// * `min_age_days` - Minimum days since archival before eligible for GC
    /// * `limit` - Maximum number of adapters to return
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

    /// Find adapters with missing content_hash_b3 or manifest_hash
    ///
    /// Returns adapters that need hash repair for preflight validation.
    /// Only includes adapters with a valid `.aos` file path that can be
    /// used to recompute the missing hashes.
    ///
    /// # Hash Repair Context
    ///
    /// As of preflight hardening, adapters require both `content_hash_b3` and
    /// `manifest_hash` to pass alias swap preflight checks. Older adapters
    /// registered before these fields were mandatory may be missing one or both.
    ///
    /// This query identifies repair candidates:
    /// - `content_hash_b3` is NULL or empty
    /// - OR `manifest_hash` is NULL or empty
    /// - AND `aos_file_path` is present (needed to recompute hashes)
    /// - AND adapter is not archived/purged (active or ready adapters only)
    ///
    /// # Arguments
    /// * `tenant_id` - Optional tenant filter; if None, queries all tenants
    /// * `limit` - Maximum number of adapters to return
    ///
    /// # Returns
    /// Adapters eligible for hash repair, ordered by created_at ascending.
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

    /// Mark an adapter as purged after .aos file deletion
    ///
    /// Sets `purged_at` timestamp and clears `aos_file_path`.
    /// The record is preserved for audit purposes.
    ///
    /// # Point of No Return
    ///
    /// **WARNING: THIS IS AN IRREVERSIBLE OPERATION.**
    ///
    /// After this function executes successfully:
    /// - The adapter's `.aos` file reference is permanently cleared
    /// - `unarchive_adapter()` will fail for this adapter
    /// - The adapter can never be loaded again
    /// - Only the audit record remains in the database
    ///
    /// This boundary is enforced by:
    /// - Database trigger `prevent_purged_adapter_load` (migration 0138)
    /// - SQL WHERE clause `purged_at IS NULL` in `unarchive_adapter()`
    ///
    /// # Prerequisites
    ///
    /// CRITICAL: Call this ONLY AFTER successfully deleting the `.aos` file from disk.
    /// The `.aos` file MUST be deleted before calling this function to maintain
    /// consistency between filesystem and database state.
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Adapter is not archived (must be archived before purge)
    /// - Adapter is already purged
    /// - Database operation fails
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

    /// Check if an adapter is loadable (not archived/purged)
    ///
    /// Returns `true` if the adapter exists and is neither archived nor purged.
    /// Used by the loader to reject attempts to load unavailable adapters.
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

    /// Unarchive an adapter (restore from archived state)
    ///
    /// Restores an archived adapter to active state. This is the last opportunity
    /// to recover an adapter before the garbage collection purge makes it permanent.
    ///
    /// # Recovery Boundary
    ///
    /// This function succeeds only if `purged_at IS NULL`. Once an adapter has been
    /// purged via `mark_adapter_purged()`, recovery is impossible because:
    /// - The `.aos` file has been permanently deleted from disk
    /// - The `aos_file_path` column is NULL
    /// - The database trigger `prevent_purged_adapter_load` blocks any load attempts
    ///
    /// # State Transitions
    ///
    /// ```text
    /// Active → Archived → Active (this function)
    ///               ↓
    ///           Purged (IRREVERSIBLE - unarchive fails here)
    /// ```
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant context (required for security isolation)
    /// * `adapter_id` - The adapter's external ID
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Adapter is not archived (nothing to restore)
    /// - Adapter has been purged (point of no return crossed)
    /// - Database operation fails
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

    /// Count archived adapters for a tenant
    ///
    /// Returns the number of adapters that are archived but not yet purged.
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

    /// Count purged adapters for a tenant
    ///
    /// Returns the number of adapters that have been purged (file deleted, record kept).
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

    // =========================================================================
    // Tenant-Scoped Adapter Operations
    // These methods validate tenant ownership before performing operations.
    // =========================================================================

    /// Update adapter state with tenant validation (transactional)
    pub(crate) async fn update_adapter_state_tx_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        // Verify adapter belongs to tenant
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM adapters WHERE adapter_id = ? AND tenant_id = ?)",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        if !exists {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        self.update_adapter_state_tx(adapter_id, state, reason)
            .await
    }

    /// Update adapter state with CAS (compare-and-swap) and tenant validation
    pub(crate) async fn update_adapter_state_cas_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        expected_state: &str,
        new_state: &str,
        reason: &str,
    ) -> Result<bool> {
        // Verify adapter belongs to tenant
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM adapters WHERE adapter_id = ? AND tenant_id = ?)",
        )
        .bind(adapter_id)
        .bind(tenant_id)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?;

        if !exists {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        self.update_adapter_state_cas(adapter_id, expected_state, new_state, reason)
            .await
    }

    /// Update adapter memory with tenant validation
    pub async fn update_adapter_memory_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        memory_bytes: i64,
    ) -> Result<()> {
        // Verify adapter belongs to tenant and update atomically
        let rows_affected = sqlx::query(
            "UPDATE adapters SET memory_bytes = ?, updated_at = datetime('now')
             WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(memory_bytes)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        Ok(())
    }

    /// Update adapter tier with tenant validation
    pub async fn update_adapter_tier_for_tenant(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        tier: &str,
    ) -> Result<()> {
        // Verify adapter belongs to tenant and update atomically
        let rows_affected = sqlx::query(
            "UPDATE adapters SET tier = ?, updated_at = datetime('now')
             WHERE adapter_id = ? AND tenant_id = ?",
        )
        .bind(tier)
        .bind(adapter_id)
        .bind(tenant_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::database(e.to_string()))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        Ok(())
    }

    /// Delete adapter with tenant validation
    pub async fn delete_adapter_for_tenant(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        // Verify adapter belongs to tenant and delete atomically
        let rows_affected =
            sqlx::query("DELETE FROM adapters WHERE adapter_id = ? AND tenant_id = ?")
                .bind(adapter_id)
                .bind(tenant_id)
                .execute(self.pool())
                .await
                .map_err(|e| AosError::database(e.to_string()))?
                .rows_affected();

        if rows_affected == 0 {
            return Err(AosError::NotFound(format!(
                "Adapter {} not found for tenant {}",
                adapter_id, tenant_id
            )));
        }

        // Clean up KV if enabled
        if self.storage_mode().write_to_kv() {
            if let Some(repo) = self.get_adapter_kv_repo(tenant_id) {
                if let Err(e) = repo.delete_adapter_kv(adapter_id).await {
                    warn!(
                        error = %e,
                        adapter_id = %adapter_id,
                        tenant_id = %tenant_id,
                        "Failed to delete adapter from KV (SQL delete succeeded)"
                    );
                }
            }
        }

        Ok(())
    }

    /// Duplicate an adapter for the given tenant
    ///
    /// Creates a copy of an existing adapter with a new ID and name.
    /// The new adapter will have:
    /// - `parent_id` set to the source adapter's ID
    /// - `fork_type` set to "duplicate"
    /// - A new unique ID and hash
    /// - Initial state set to 'cold'
    ///
    /// # Arguments
    /// * `tenant_id` - The tenant ID (must match the source adapter's tenant)
    /// * `source_adapter_id` - The adapter ID to duplicate
    /// * `new_name` - Optional name for the duplicate (defaults to "{original_name} (copy)")
    ///
    /// # Returns
    /// The ID of the newly created adapter
    pub async fn duplicate_adapter_for_tenant(
        &self,
        tenant_id: &str,
        source_adapter_id: &str,
        new_name: Option<&str>,
    ) -> Result<Adapter> {
        // Fetch the source adapter with tenant validation
        let source = self
            .get_adapter_for_tenant(tenant_id, source_adapter_id)
            .await?
            .ok_or_else(|| {
                AosError::NotFound(format!(
                    "Adapter {} not found for tenant {}",
                    source_adapter_id, tenant_id
                ))
            })?;

        // Generate new identifiers
        let new_adapter_id = format!("adapter-{}", Uuid::now_v7());
        let name = new_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{} (copy)", source.name));

        // Generate a new hash for the duplicate
        let new_hash = {
            let mut hasher = blake3::Hasher::new();
            hasher.update(new_adapter_id.as_bytes());
            hasher.update(chrono::Utc::now().to_rfc3339().as_bytes());
            hasher.finalize().to_hex().to_string()
        };

        // Build registration params from source adapter
        let params = AdapterRegistrationParams {
            tenant_id: tenant_id.to_string(),
            adapter_id: new_adapter_id.clone(),
            name: name.clone(),
            hash_b3: new_hash,
            rank: source.rank,
            tier: source.tier.clone(),
            alpha: source.alpha,
            lora_strength: source.lora_strength,
            targets_json: source.targets_json.clone(),
            acl_json: source.acl_json.clone(),
            languages_json: source.languages_json.clone(),
            framework: source.framework.clone(),
            category: source.category.clone(),
            scope: source.scope.clone(),
            framework_id: source.framework_id.clone(),
            framework_version: source.framework_version.clone(),
            repo_id: source.repo_id.clone(),
            commit_sha: source.commit_sha.clone(),
            intent: source.intent.clone(),
            expires_at: None, // Don't copy expiration
            aos_file_path: source.aos_file_path.clone(),
            aos_file_hash: source.aos_file_hash.clone(),
            adapter_name: Some(name.clone()),
            tenant_namespace: source.tenant_namespace.clone(),
            domain: source.domain.clone(),
            purpose: source.purpose.clone(),
            revision: Some("1".to_string()), // Start at revision 1
            parent_id: Some(source_adapter_id.to_string()),
            fork_type: Some("duplicate".to_string()),
            fork_reason: Some("User-requested copy".to_string()),
            base_model_id: source.base_model_id.clone(),
            recommended_for_moe: source.recommended_for_moe,
            manifest_schema_version: source.manifest_schema_version.clone(),
            // Generate new content hash for duplicate (it's a distinct adapter even if weights are same)
            content_hash_b3: {
                let mut hasher = blake3::Hasher::new();
                hasher.update(b"duplicate:");
                hasher.update(new_adapter_id.as_bytes());
                hasher.update(
                    source
                        .content_hash_b3
                        .as_deref()
                        .unwrap_or(&source.hash_b3)
                        .as_bytes(),
                );
                hasher.finalize().to_hex().to_string()
            },
            provenance_json: source.provenance_json.clone(),
            metadata_json: source.metadata_json.clone(),
            repo_path: source.repo_path.clone(),
            // These fields may not exist on legacy adapters
            codebase_scope: source.codebase_scope.clone(),
            dataset_version_id: source.dataset_version_id.clone(),
            registration_timestamp: source.registration_timestamp.clone(),
            manifest_hash: source.manifest_hash.clone(),
            // Codebase adapter type and stream binding (from migration 0261)
            adapter_type: source.adapter_type.clone(),
            base_adapter_id: source.base_adapter_id.clone(),
            stream_session_id: source.stream_session_id.clone(),
            versioning_threshold: source.versioning_threshold,
            coreml_package_hash: source.coreml_package_hash.clone(),
            training_dataset_hash_b3: source.training_dataset_hash_b3.clone(),
        };

        // Register the new adapter
        let new_id = self.register_adapter_extended(params).await?;

        // Fetch and return the new adapter using tenant-scoped access
        self.get_adapter_for_tenant(tenant_id, &new_id)
            .await?
            .ok_or_else(|| AosError::database("Failed to retrieve duplicated adapter".to_string()))
    }
}

