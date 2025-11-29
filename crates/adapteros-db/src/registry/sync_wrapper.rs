//! Synchronous compatibility layer for legacy Registry API
//!
//! This module provides blocking wrappers around async Db methods,
//! allowing gradual migration from adapteros-registry without
//! breaking CLI and other sync consumers.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use adapteros_db::registry::SyncRegistry;
//!
//! let registry = SyncRegistry::open("var/aos-cp.sqlite3")?;
//! let adapter = registry.get_adapter("my-adapter")?;
//! ```

use crate::Db;
use adapteros_core::{AdapterName, AosError, B3Hash, ForkType, Result};
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tracing::{debug, info, warn};

/// Legacy-compatible synchronous registry wrapper
///
/// Wraps the async `Db` with a tokio runtime to provide blocking APIs
/// for CLI commands and other sync contexts.
pub struct SyncRegistry {
    db: Arc<Db>,
    runtime: Arc<Runtime>,
}

impl SyncRegistry {
    /// Open or create registry at database path (compatibility with Registry::open)
    ///
    /// Creates a new tokio runtime for blocking operations and runs migrations.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let rt = Runtime::new()
            .map_err(|e| AosError::Registry(format!("Failed to create runtime: {}", e)))?;

        let path_str = path
            .as_ref()
            .to_str()
            .ok_or_else(|| AosError::Registry("Invalid path".to_string()))?;

        let db = rt.block_on(async {
            let db = Db::connect(path_str).await?;
            // Run migrations to ensure schema is up to date
            db.migrate().await?;
            Ok::<_, AosError>(db)
        })?;

        Ok(Self {
            db: Arc::new(db),
            runtime: Arc::new(rt),
        })
    }

    /// Create from existing Db and Runtime (for server contexts)
    pub fn from_db(db: Arc<Db>, runtime: Arc<Runtime>) -> Self {
        Self { db, runtime }
    }

    /// Get a reference to the underlying Db
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Register a new adapter
    pub fn register_adapter(
        &self,
        id: &str,
        hash: &B3Hash,
        tier: &str,
        rank: u32,
        acl: &[String],
    ) -> Result<()> {
        self.register_adapter_with_name(id, None, hash, tier, rank, acl, None, None)
    }

    /// Register a new adapter with semantic name and lineage
    #[allow(clippy::too_many_arguments)]
    pub fn register_adapter_with_name(
        &self,
        id: &str,
        semantic_name: Option<&AdapterName>,
        hash: &B3Hash,
        tier: &str,
        rank: u32,
        acl: &[String],
        parent_id: Option<&str>,
        fork_type: Option<ForkType>,
    ) -> Result<()> {
        // Validate semantic name if provided
        if let Some(name) = semantic_name {
            name.validate()?;

            // Check if name already exists
            if self.get_adapter_by_name(&name.to_string())?.is_some() {
                return Err(AosError::Registry(format!(
                    "Adapter name '{}' already exists",
                    name
                )));
            }

            // Check revision monotonicity using lineage validator
            self.runtime.block_on(async {
                super::lineage::validate_revision_for_registration(
                    &self.db,
                    name.tenant(),
                    name.domain(),
                    name.purpose(),
                    name.revision_number()?,
                )
                .await
            })?;
        }

        // Validate parent exists if specified
        if let Some(parent) = parent_id {
            if self.get_adapter(parent)?.is_none() {
                return Err(AosError::Registry(format!(
                    "Parent adapter '{}' does not exist",
                    parent
                )));
            }

            // Check for circular dependency
            if self.runtime.block_on(async {
                super::lineage::is_descendant_of(&self.db, parent, id).await
            })? {
                return Err(AosError::Registry(format!(
                    "Circular dependency detected: '{}' cannot be parent of '{}' (creates cycle)",
                    parent, id
                )));
            }
        }

        // Validate fork_type is set if parent exists
        if parent_id.is_some() && fork_type.is_none() {
            return Err(AosError::Registry(
                "fork_type must be specified when parent_id is set".to_string(),
            ));
        }

        // Validate fork type semantics
        if let (Some(parent), Some(child_name), Some(ft)) = (parent_id, semantic_name, fork_type) {
            if let Some(parent_record) = self.get_adapter(parent)? {
                if let Some(parent_name) = &parent_record.semantic_name {
                    ft.validate_fork(parent_name, child_name)?;
                }
            }
        }

        // Inherit parent ACL if ACL is empty and parent exists
        let effective_acl = if acl.is_empty() {
            if let Some(parent) = parent_id {
                if let Some(parent_record) = self.get_adapter(parent)? {
                    parent_record.acl
                } else {
                    acl.to_vec()
                }
            } else {
                acl.to_vec()
            }
        } else {
            acl.to_vec()
        };

        // Insert into database
        let acl_json = serde_json::to_string(&effective_acl)?;

        self.runtime.block_on(async {
            let adapter_name_str = semantic_name.map(|n| n.to_string());
            let tenant = semantic_name.map(|n| n.tenant().to_string());
            let domain = semantic_name.map(|n| n.domain().to_string());
            let purpose = semantic_name.map(|n| n.purpose().to_string());
            let revision = semantic_name.map(|n| n.revision().to_string());
            let fork_type_str = fork_type.map(|ft| ft.as_str().to_string());

            sqlx::query(
                r#"
                INSERT OR REPLACE INTO adapters (
                    adapter_id, hash_b3, tier, rank, acl_json,
                    adapter_name, tenant_namespace, domain, purpose, revision,
                    parent_id, fork_type, created_at, tenant_id, name
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                    datetime('now'), COALESCE(?7, 'default'), COALESCE(?6, ?1)
                )
                "#,
            )
            .bind(id)
            .bind(hash.to_hex())
            .bind(tier)
            .bind(rank as i32)
            .bind(&acl_json)
            .bind(&adapter_name_str)
            .bind(&tenant)
            .bind(&domain)
            .bind(&purpose)
            .bind(&revision)
            .bind(parent_id)
            .bind(&fork_type_str)
            .execute(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to register adapter: {}", e)))
        })?;

        info!(
            event_type = "adapter.registered",
            adapter_id = %id,
            tier = %tier,
            rank = rank,
            "Adapter registered successfully via SyncRegistry"
        );

        Ok(())
    }

    /// Lookup adapter by ID
    pub fn get_adapter(&self, id: &str) -> Result<Option<SyncRegistryAdapterRecord>> {
        self.runtime.block_on(async {
            let row = sqlx::query_as::<_, AdapterRow>(
                r#"
                SELECT adapter_id, hash_b3, tier, rank, acl_json, activation_pct,
                       adapter_name, parent_id, fork_type, fork_reason, created_at
                FROM adapters WHERE adapter_id = ?1
                "#,
            )
            .bind(id)
            .fetch_optional(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to get adapter: {}", e)))?;

            Ok(row.map(SyncRegistryAdapterRecord::from))
        })
    }

    /// Lookup adapter by semantic name
    pub fn get_adapter_by_name(&self, name: &str) -> Result<Option<SyncRegistryAdapterRecord>> {
        let parsed_name = AdapterName::parse(name)?;

        self.runtime.block_on(async {
            let row = sqlx::query_as::<_, AdapterRow>(
                r#"
                SELECT adapter_id, hash_b3, tier, rank, acl_json, activation_pct,
                       adapter_name, parent_id, fork_type, fork_reason, created_at
                FROM adapters WHERE adapter_name = ?1
                "#,
            )
            .bind(parsed_name.to_string())
            .fetch_optional(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to get adapter: {}", e)))?;

            Ok(row.map(SyncRegistryAdapterRecord::from))
        })
    }

    /// List all adapters
    pub fn list_adapters(&self) -> Result<Vec<SyncRegistryAdapterRecord>> {
        self.runtime.block_on(async {
            let rows = sqlx::query_as::<_, AdapterRow>(
                r#"
                SELECT adapter_id, hash_b3, tier, rank, acl_json, activation_pct,
                       adapter_name, parent_id, fork_type, fork_reason, created_at
                FROM adapters
                "#,
            )
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to list adapters: {}", e)))?;

            Ok(rows.into_iter().map(SyncRegistryAdapterRecord::from).collect())
        })
    }

    /// List adapters in a specific lineage
    pub fn list_adapters_in_lineage(
        &self,
        tenant: &str,
        domain: &str,
        purpose: &str,
    ) -> Result<Vec<SyncRegistryAdapterRecord>> {
        self.runtime.block_on(async {
            let rows = sqlx::query_as::<_, AdapterRow>(
                r#"
                SELECT adapter_id, hash_b3, tier, rank, acl_json, activation_pct,
                       adapter_name, parent_id, fork_type, fork_reason, created_at
                FROM adapters
                WHERE tenant_namespace = ?1 AND domain = ?2 AND purpose = ?3
                ORDER BY revision DESC
                "#,
            )
            .bind(tenant)
            .bind(domain)
            .bind(purpose)
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to query adapters: {}", e)))?;

            debug!(
                event_type = "lineage.queried",
                tenant = %tenant,
                domain = %domain,
                purpose = %purpose,
                count = rows.len(),
                "Lineage query completed"
            );

            Ok(rows.into_iter().map(SyncRegistryAdapterRecord::from).collect())
        })
    }

    /// Get the latest revision in a lineage
    pub fn get_latest_revision(
        &self,
        tenant: &str,
        domain: &str,
        purpose: &str,
    ) -> Result<Option<SyncRegistryAdapterRecord>> {
        let adapters = self.list_adapters_in_lineage(tenant, domain, purpose)?;
        Ok(adapters.into_iter().next())
    }

    /// Generate next revision number for a lineage
    pub fn next_revision_number(&self, tenant: &str, domain: &str, purpose: &str) -> Result<u32> {
        let latest = self.get_latest_revision(tenant, domain, purpose)?;

        if let Some(adapter) = latest {
            if let Some(name) = adapter.semantic_name {
                let current = name.revision_number()?;
                return Ok(current + 1);
            }
        }

        Ok(1)
    }

    /// Check if child_id is a descendant of potential_ancestor_id
    pub fn is_descendant_of(&self, child_id: &str, potential_ancestor_id: &str) -> Result<bool> {
        self.runtime
            .block_on(super::lineage::is_descendant_of(&self.db, child_id, potential_ancestor_id))
    }

    /// Check if adapter is allowed for tenant (with ACL inheritance)
    pub fn check_acl(&self, adapter_id: &str, tenant_id: &str) -> Result<bool> {
        self.runtime
            .block_on(super::acl::check_acl(&self.db, adapter_id, tenant_id))
    }

    /// Register a tenant
    pub fn register_tenant(&self, id: &str, uid: u32, gid: u32) -> Result<()> {
        self.runtime.block_on(async {
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO tenants (id, uid, gid, created_at)
                VALUES (?1, ?2, ?3, datetime('now'))
                "#,
            )
            .bind(id)
            .bind(uid as i64)
            .bind(gid as i64)
            .execute(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to register tenant: {}", e)))
        })?;

        Ok(())
    }

    /// Get tenant by ID
    pub fn get_tenant(&self, id: &str) -> Result<Option<SyncRegistryTenantRecord>> {
        self.runtime.block_on(async {
            let row = sqlx::query_as::<_, TenantRow>(
                "SELECT id, uid, gid, created_at FROM tenants WHERE id = ?1",
            )
            .bind(id)
            .fetch_optional(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to get tenant: {}", e)))?;

            Ok(row.map(SyncRegistryTenantRecord::from))
        })
    }

    /// Update adapter activation percentage
    pub fn update_activation(&self, adapter_id: &str, pct: f32) -> Result<()> {
        self.runtime.block_on(async {
            sqlx::query("UPDATE adapters SET activation_pct = ?1 WHERE adapter_id = ?2")
                .bind(pct)
                .bind(adapter_id)
                .execute(self.db.pool())
                .await
                .map_err(|e| AosError::Registry(format!("Failed to update activation: {}", e)))
        })?;

        Ok(())
    }

    /// Validate adapter dependencies
    pub fn validate_dependencies(
        &self,
        adapter_id: &str,
        dependencies: &adapteros_manifest::AdapterDependencies,
        base_model: &str,
    ) -> Result<()> {
        // Check base model match
        if let Some(required_base) = &dependencies.base_model {
            if required_base != base_model {
                return Err(AosError::Registry(format!(
                    "Adapter {}: Base model mismatch: requires {}, got {}",
                    adapter_id, required_base, base_model
                )));
            }
        }

        // Check required adapters exist
        for required in &dependencies.requires_adapters {
            if self.get_adapter(required)?.is_none() {
                return Err(AosError::Registry(format!(
                    "Adapter {}: Missing required adapter: {}",
                    adapter_id, required
                )));
            }
        }

        // Check conflicts
        for conflict in &dependencies.conflicts_with {
            if self.get_adapter(conflict)?.is_some() {
                return Err(AosError::Registry(format!(
                    "Adapter {}: Conflicting adapter present: {}",
                    adapter_id, conflict
                )));
            }
        }

        Ok(())
    }

    // ============================================================
    // Model Registry Operations
    // ============================================================

    /// Get model by name
    pub fn get_model(&self, name: &str) -> Result<Option<SyncModelRecord>> {
        self.runtime.block_on(async {
            let row = sqlx::query_as::<_, ModelRow>(
                r#"
                SELECT id, name, hash_b3, config_hash_b3, tokenizer_hash_b3,
                       tokenizer_cfg_hash_b3, license_hash_b3, license_text,
                       weights_hash_b3, model_card_hash_b3, created_at
                FROM models WHERE name = ?1
                "#,
            )
            .bind(name)
            .fetch_optional(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to get model: {}", e)))?;

            Ok(row.map(SyncModelRecord::from))
        })
    }

    /// Register a new model with hash collision detection
    pub fn register_model(&self, model: &SyncModelRecordInput) -> Result<()> {
        // Check for hash collisions before inserting
        self.check_model_hash_collisions(model)?;

        self.runtime.block_on(async {
            sqlx::query(
                r#"
                INSERT INTO models (
                    id, name, hash_b3, config_hash_b3, tokenizer_hash_b3,
                    tokenizer_cfg_hash_b3, license_hash_b3, license_text,
                    weights_hash_b3, model_card_hash_b3, created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))
                "#,
            )
            .bind(&model.id)
            .bind(&model.name)
            .bind(model.weights_hash.to_hex())
            .bind(model.config_hash.to_hex())
            .bind(model.tokenizer_hash.to_hex())
            .bind(model.tokenizer_cfg_hash.to_hex())
            .bind(model.license_hash.map(|h| h.to_hex()))
            .bind(&model.license_text)
            .bind(model.weights_hash.to_hex())
            .bind(model.model_card_hash.map(|h| h.to_hex()))
            .execute(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to register model: {}", e)))
        })?;

        info!(
            event_type = "model.registered",
            model_name = %model.name,
            "Model registered via SyncRegistry"
        );

        Ok(())
    }

    /// Check for hash collisions with existing models
    fn check_model_hash_collisions(&self, model: &SyncModelRecordInput) -> Result<()> {
        let hashes_to_check = vec![
            ("config", model.config_hash.to_hex()),
            ("tokenizer", model.tokenizer_hash.to_hex()),
            ("tokenizer_cfg", model.tokenizer_cfg_hash.to_hex()),
            ("weights", model.weights_hash.to_hex()),
        ];

        for (hash_type, hash_value) in hashes_to_check {
            let collision = self.runtime.block_on(async {
                let column = format!("{}_hash_b3", hash_type);
                let query = format!(
                    "SELECT name FROM models WHERE {} = ?1 AND name != ?2",
                    column
                );
                sqlx::query_scalar::<_, String>(&query)
                    .bind(&hash_value)
                    .bind(&model.name)
                    .fetch_optional(self.db.pool())
                    .await
                    .map_err(|e| AosError::Registry(format!("Hash collision check failed: {}", e)))
            })?;

            if let Some(existing_name) = collision {
                warn!(
                    event_type = "model.hash_collision",
                    hash_type = %hash_type,
                    hash_value = %hash_value,
                    existing_model = %existing_name,
                    new_model = %model.name,
                    "Hash collision detected"
                );
                return Err(AosError::Registry(format!(
                    "Hash collision: {} hash {} already used by model '{}'",
                    hash_type, hash_value, existing_name
                )));
            }
        }

        Ok(())
    }

    /// List all registered models
    pub fn list_models(&self) -> Result<Vec<SyncModelRecord>> {
        self.runtime.block_on(async {
            let rows = sqlx::query_as::<_, ModelRow>(
                r#"
                SELECT id, name, hash_b3, config_hash_b3, tokenizer_hash_b3,
                       tokenizer_cfg_hash_b3, license_hash_b3, license_text,
                       weights_hash_b3, model_card_hash_b3, created_at
                FROM models
                ORDER BY created_at DESC
                "#,
            )
            .fetch_all(self.db.pool())
            .await
            .map_err(|e| AosError::Registry(format!("Failed to list models: {}", e)))?;

            Ok(rows.into_iter().map(SyncModelRecord::from).collect())
        })
    }
}

// Internal row types for SQLx queries
#[derive(sqlx::FromRow)]
struct AdapterRow {
    adapter_id: String,
    hash_b3: String,
    tier: String,
    rank: i32,
    acl_json: Option<String>,
    activation_pct: Option<f64>,
    adapter_name: Option<String>,
    parent_id: Option<String>,
    fork_type: Option<String>,
    fork_reason: Option<String>,
    created_at: String,
}

#[derive(sqlx::FromRow)]
struct TenantRow {
    id: String,
    uid: i64,
    gid: i64,
    created_at: String,
}

#[derive(sqlx::FromRow)]
struct ModelRow {
    id: String,
    name: String,
    hash_b3: String,
    config_hash_b3: String,
    tokenizer_hash_b3: String,
    tokenizer_cfg_hash_b3: String,
    license_hash_b3: Option<String>,
    license_text: Option<String>,
    weights_hash_b3: Option<String>,
    model_card_hash_b3: Option<String>,
    created_at: String,
}

/// Adapter record returned by SyncRegistry (mirrors legacy AdapterRecord)
#[derive(Debug, Clone)]
pub struct SyncRegistryAdapterRecord {
    pub id: String,
    pub hash: B3Hash,
    pub tier: String,
    pub rank: u32,
    pub acl: Vec<String>,
    pub activation_pct: f32,
    pub registered_at: String,
    pub semantic_name: Option<AdapterName>,
    pub parent_id: Option<String>,
    pub fork_type: Option<ForkType>,
    pub fork_reason: Option<String>,
}

impl From<AdapterRow> for SyncRegistryAdapterRecord {
    fn from(row: AdapterRow) -> Self {
        Self {
            id: row.adapter_id,
            hash: B3Hash::from_hex(&row.hash_b3).unwrap_or_else(|_| B3Hash::zero()),
            tier: row.tier,
            rank: row.rank as u32,
            acl: row
                .acl_json
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            activation_pct: row.activation_pct.unwrap_or(0.0) as f32,
            registered_at: row.created_at,
            semantic_name: row.adapter_name.and_then(|s| s.parse::<AdapterName>().ok()),
            parent_id: row.parent_id,
            fork_type: row.fork_type.and_then(|s| ForkType::parse_from_str(&s).ok()),
            fork_reason: row.fork_reason,
        }
    }
}

impl SyncRegistryAdapterRecord {
    /// Get display name (semantic name if available, otherwise ID)
    pub fn display_name(&self) -> String {
        if let Some(ref name) = self.semantic_name {
            name.display_name()
        } else {
            self.id.clone()
        }
    }
}

/// Tenant record returned by SyncRegistry
#[derive(Debug, Clone)]
pub struct SyncRegistryTenantRecord {
    pub id: String,
    pub uid: u32,
    pub gid: u32,
    pub created_at: String,
}

impl From<TenantRow> for SyncRegistryTenantRecord {
    fn from(row: TenantRow) -> Self {
        Self {
            id: row.id,
            uid: row.uid as u32,
            gid: row.gid as u32,
            created_at: row.created_at,
        }
    }
}

/// Model record returned by SyncRegistry (mirrors legacy ModelRecord)
#[derive(Debug, Clone)]
pub struct SyncModelRecord {
    pub id: String,
    pub name: String,
    pub config_hash: B3Hash,
    pub tokenizer_hash: B3Hash,
    pub tokenizer_cfg_hash: B3Hash,
    pub weights_hash: B3Hash,
    pub license_hash: Option<B3Hash>,
    pub license_text: Option<String>,
    pub model_card_hash: Option<B3Hash>,
    pub created_at: String,
}

impl From<ModelRow> for SyncModelRecord {
    fn from(row: ModelRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            config_hash: B3Hash::from_hex(&row.config_hash_b3).unwrap_or_else(|_| B3Hash::zero()),
            tokenizer_hash: B3Hash::from_hex(&row.tokenizer_hash_b3)
                .unwrap_or_else(|_| B3Hash::zero()),
            tokenizer_cfg_hash: B3Hash::from_hex(&row.tokenizer_cfg_hash_b3)
                .unwrap_or_else(|_| B3Hash::zero()),
            weights_hash: row
                .weights_hash_b3
                .and_then(|s| B3Hash::from_hex(&s).ok())
                .unwrap_or_else(|| B3Hash::from_hex(&row.hash_b3).unwrap_or_else(|_| B3Hash::zero())),
            license_hash: row.license_hash_b3.and_then(|s| B3Hash::from_hex(&s).ok()),
            license_text: row.license_text,
            model_card_hash: row.model_card_hash_b3.and_then(|s| B3Hash::from_hex(&s).ok()),
            created_at: row.created_at,
        }
    }
}

/// Input parameters for registering a new model
#[derive(Debug, Clone)]
pub struct SyncModelRecordInput {
    pub id: String,
    pub name: String,
    pub config_hash: B3Hash,
    pub tokenizer_hash: B3Hash,
    pub tokenizer_cfg_hash: B3Hash,
    pub weights_hash: B3Hash,
    pub license_hash: Option<B3Hash>,
    pub license_text: Option<String>,
    pub model_card_hash: Option<B3Hash>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_registry_open_and_tenant() {
        // SyncRegistry creates its own runtime, so we use a regular #[test]
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // Create registry (this creates its own runtime)
        let registry = SyncRegistry::open(&db_path).unwrap();

        // Test tenant registration
        registry.register_tenant("test-tenant", 1000, 1000).unwrap();
        let tenant = registry.get_tenant("test-tenant").unwrap();
        assert!(tenant.is_some());
        assert_eq!(tenant.unwrap().uid, 1000);
    }
}
