//! SQLite registry for adapters, tenants, and ACLs

use adapteros_core::{AdapterName, AosError, B3Hash, ForkType, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub mod eviction;
pub mod migrations;
pub mod models;

pub use models::{ModelRecord, ModelRegistry};

/// Registry for managing adapters, tenants, and access control
pub struct Registry {
    conn: Arc<Mutex<Connection>>,
}

impl Registry {
    /// Open or create registry at path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| AosError::Registry(format!("Failed to open database: {}", e)))?;

        // Enable WAL mode for better concurrency
        conn.execute("PRAGMA journal_mode=WAL", [])
            .map_err(|e| AosError::Registry(format!("Failed to set WAL mode: {}", e)))?;

        let registry = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        // Run migrations
        registry.migrate()?;

        Ok(registry)
    }

    /// Run database migrations
    fn migrate(&self) -> Result<()> {
        let mut conn = self.conn.lock();
        migrations::run_migrations(&mut conn)
            .map_err(|e| AosError::Registry(format!("Migration failed: {}", e)))?;
        Ok(())
    }

    /// Register a new adapter with semantic name
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

            // Check revision monotonicity
            if let Some(latest) = self.get_latest_revision(name.tenant(), name.domain(), name.purpose())? {
                if let Some(latest_name) = latest.semantic_name {
                    let latest_rev = latest_name.revision_number()?;
                    let new_rev = name.revision_number()?;

                    // Revision must be greater than latest
                    if new_rev <= latest_rev {
                        warn!(
                            event_type = "revision.monotonicity_violation",
                            adapter_name = %name,
                            latest_rev = latest_rev,
                            attempted_rev = new_rev,
                            "Revision monotonicity check failed"
                        );
                        return Err(AosError::Registry(format!(
                            "Revision r{:03} must be greater than latest r{:03} in lineage {}",
                            new_rev,
                            latest_rev,
                            name.base_path()
                        )));
                    }

                    // Cannot skip more than 5 revisions (prevents accidental gaps)
                    if new_rev > latest_rev + 5 {
                        warn!(
                            event_type = "revision.gap_too_large",
                            adapter_name = %name,
                            latest_rev = latest_rev,
                            attempted_rev = new_rev,
                            gap = new_rev - latest_rev,
                            "Revision gap exceeds limit"
                        );
                        return Err(AosError::Registry(format!(
                            "Cannot skip more than 5 revisions: r{:03} → r{:03} (gap of {})",
                            latest_rev,
                            new_rev,
                            new_rev - latest_rev
                        )));
                    }
                }
            }
        }

        // Validate parent exists if specified
        if let Some(parent) = parent_id {
            if self.get_adapter(parent)?.is_none() {
                return Err(AosError::Registry(format!(
                    "Parent adapter '{}' does not exist",
                    parent
                )));
            }

            // CRITICAL: Check for circular dependency
            // This prevents A→B→C→A cycles by checking if parent is a descendant of current adapter
            if self.is_descendant_of(parent, id)? {
                error!(
                    event_type = "circular_dependency.detected",
                    adapter_id = %id,
                    parent_id = %parent,
                    "Circular dependency detected - registration blocked"
                );
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

        // Validate fork type semantics (independent vs extension)
        if let (Some(parent), Some(child_name), Some(ft)) = (parent_id, semantic_name, fork_type) {
            // Get parent's semantic name
            if let Some(parent_record) = self.get_adapter(parent)? {
                if let Some(parent_name) = &parent_record.semantic_name {
                    // Validate fork semantics
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

        let conn = self.conn.lock();
        let acl_json = serde_json::to_string(&effective_acl)?;

        // Build SQL with optional semantic name fields
        if let Some(name) = semantic_name {
            conn.execute(
                "INSERT OR REPLACE INTO adapters
                 (id, hash, tier, rank, acl, adapter_name, tenant_namespace, domain, purpose, revision,
                  parent_id, fork_type, registered_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, datetime('now'))",
                params![
                    id,
                    hash.to_hex(),
                    tier,
                    rank,
                    acl_json,
                    name.to_string(),
                    name.tenant(),
                    name.domain(),
                    name.purpose(),
                    name.revision(),
                    parent_id,
                    fork_type.map(|ft| ft.as_str()),
                ],
            )
        } else {
            conn.execute(
                "INSERT OR REPLACE INTO adapters (id, hash, tier, rank, acl, registered_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
                params![id, hash.to_hex(), tier, rank, acl_json],
            )
        }
        .map_err(|e| AosError::Registry(format!("Failed to register adapter: {}", e)))?;

        // Log successful registration
        info!(
            event_type = "adapter.registered",
            adapter_id = %id,
            adapter_name = %semantic_name.map(|n| n.to_string()).unwrap_or_else(|| id.to_string()),
            tenant = %semantic_name.map(|n| n.tenant().to_string()).unwrap_or_default(),
            domain = %semantic_name.map(|n| n.domain().to_string()).unwrap_or_default(),
            purpose = %semantic_name.map(|n| n.purpose().to_string()).unwrap_or_default(),
            revision = %semantic_name.map(|n| n.revision().to_string()).unwrap_or_default(),
            parent_id = ?parent_id,
            fork_type = ?fork_type.map(|ft| ft.as_str()),
            tier = %tier,
            rank = rank,
            "Adapter registered successfully"
        );

        Ok(())
    }

    /// Lookup adapter by ID
    pub fn get_adapter(&self, id: &str) -> Result<Option<AdapterRecord>> {
        let conn = self.conn.lock();

        let result = conn
            .query_row(
                "SELECT id, hash, tier, rank, acl, activation_pct, registered_at,
                        adapter_name, parent_id, fork_type, fork_reason
                 FROM adapters WHERE id = ?1",
                params![id],
                |row| {
                    Ok(AdapterRecord {
                        id: row.get(0)?,
                        hash: B3Hash::from_hex(&row.get::<_, String>(1)?)
                            .expect("Operation should succeed"),
                        tier: row.get(2)?,
                        rank: row.get(3)?,
                        acl: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                        activation_pct: row.get(5)?,
                        registered_at: row.get(6)?,
                        semantic_name: row
                            .get::<_, Option<String>>(7)?
                            .and_then(|s| s.parse::<AdapterName>().ok()),
                        parent_id: row.get(8)?,
                        fork_type: row
                            .get::<_, Option<String>>(9)?
                            .and_then(|s| ForkType::from_str(&s).ok()),
                        fork_reason: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(|e| AosError::Registry(format!("Failed to get adapter: {}", e)))?;

        Ok(result)
    }

    /// Lookup adapter by semantic name
    pub fn get_adapter_by_name(&self, name: &str) -> Result<Option<AdapterRecord>> {
        // Validate name format first
        let parsed_name = AdapterName::parse(name)?;

        let conn = self.conn.lock();

        let result = conn
            .query_row(
                "SELECT id, hash, tier, rank, acl, activation_pct, registered_at,
                        adapter_name, parent_id, fork_type, fork_reason
                 FROM adapters WHERE adapter_name = ?1",
                params![parsed_name.to_string()],
                |row| {
                    Ok(AdapterRecord {
                        id: row.get(0)?,
                        hash: B3Hash::from_hex(&row.get::<_, String>(1)?)
                            .expect("Operation should succeed"),
                        tier: row.get(2)?,
                        rank: row.get(3)?,
                        acl: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                        activation_pct: row.get(5)?,
                        registered_at: row.get(6)?,
                        semantic_name: row
                            .get::<_, Option<String>>(7)?
                            .and_then(|s| s.parse::<AdapterName>().ok()),
                        parent_id: row.get(8)?,
                        fork_type: row
                            .get::<_, Option<String>>(9)?
                            .and_then(|s| ForkType::from_str(&s).ok()),
                        fork_reason: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(|e| AosError::Registry(format!("Failed to get adapter: {}", e)))?;

        Ok(result)
    }

    /// List adapters in a specific lineage (tenant/domain/purpose)
    pub fn list_adapters_in_lineage(
        &self,
        tenant: &str,
        domain: &str,
        purpose: &str,
    ) -> Result<Vec<AdapterRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, hash, tier, rank, acl, activation_pct, registered_at,
                        adapter_name, parent_id, fork_type, fork_reason
                 FROM adapters
                 WHERE tenant_namespace = ?1 AND domain = ?2 AND purpose = ?3
                 ORDER BY revision DESC",
            )
            .map_err(|e| AosError::Registry(format!("Failed to prepare statement: {}", e)))?;

        let adapters = stmt
            .query_map(params![tenant, domain, purpose], |row| {
                Ok(AdapterRecord {
                    id: row.get(0)?,
                    hash: B3Hash::from_hex(&row.get::<_, String>(1)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    tier: row.get(2)?,
                    rank: row.get(3)?,
                    acl: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    activation_pct: row.get(5)?,
                    registered_at: row.get(6)?,
                    semantic_name: row
                        .get::<_, Option<String>>(7)?
                        .and_then(|s| s.parse::<AdapterName>().ok()),
                    parent_id: row.get(8)?,
                    fork_type: row
                        .get::<_, Option<String>>(9)?
                        .and_then(|s| ForkType::from_str(&s).ok()),
                    fork_reason: row.get(10)?,
                })
            })
            .map_err(|e| AosError::Registry(format!("Failed to query adapters: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AosError::Registry(format!("Failed to collect adapters: {}", e)))?;

        debug!(
            event_type = "lineage.queried",
            tenant = %tenant,
            domain = %domain,
            purpose = %purpose,
            count = adapters.len(),
            "Lineage query completed"
        );

        Ok(adapters)
    }

    /// Get the latest revision in a lineage
    pub fn get_latest_revision(
        &self,
        tenant: &str,
        domain: &str,
        purpose: &str,
    ) -> Result<Option<AdapterRecord>> {
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

    /// Check if child_id is a descendant of potential_ancestor_id (recursively)
    ///
    /// Uses SQLite recursive CTE to traverse the parent chain and detect:
    /// - Direct parent-child relationships
    /// - Multi-level ancestry (grandparent, great-grandparent, etc.)
    /// - Circular dependencies (A→B→C→A)
    pub fn is_descendant_of(
        &self,
        child_id: &str,
        potential_ancestor_id: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock();

        let result: Option<i32> = conn
            .query_row(
                "WITH RECURSIVE ancestry AS (
                    SELECT id, parent_id FROM adapters WHERE id = ?1
                    UNION ALL
                    SELECT a.id, a.parent_id
                    FROM adapters a
                    JOIN ancestry ON a.id = ancestry.parent_id
                    WHERE ancestry.parent_id IS NOT NULL
                )
                SELECT 1 FROM ancestry WHERE id = ?2",
                params![child_id, potential_ancestor_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AosError::Registry(format!("Ancestry check failed: {}", e)))?;

        Ok(result.is_some())
    }

    /// Check if adapter is allowed for tenant (with ACL inheritance)
    ///
    /// ACL resolution follows this hierarchy:
    /// 1. If adapter has explicit ACL (non-empty), use it
    /// 2. If adapter has no ACL but has a parent, inherit from parent (recursive)
    /// 3. If no ACL anywhere in chain, allow all (global access)
    pub fn check_acl(&self, adapter_id: &str, tenant_id: &str) -> Result<bool> {
        let adapter = self
            .get_adapter(adapter_id)?
            .ok_or_else(|| AosError::Registry(format!("Adapter '{}' not found", adapter_id)))?;

        // Check direct ACL first
        if !adapter.acl.is_empty() {
            let allowed = adapter.acl.contains(&tenant_id.to_string());
            if allowed {
                debug!(
                    event_type = "acl.allowed",
                    adapter_id = %adapter_id,
                    tenant_id = %tenant_id,
                    reason = "direct_acl",
                    "ACL check passed"
                );
            } else {
                warn!(
                    event_type = "acl.denied",
                    adapter_id = %adapter_id,
                    tenant_id = %tenant_id,
                    reason = "not_in_acl",
                    "ACL check failed"
                );
            }
            return Ok(allowed);
        }

        // Empty ACL: inherit from parent if exists
        if let Some(parent_id) = &adapter.parent_id {
            debug!(
                event_type = "acl.inherited",
                adapter_id = %adapter_id,
                parent_id = %parent_id,
                tenant_id = %tenant_id,
                "Inheriting ACL from parent"
            );
            return self.check_acl(parent_id, tenant_id);
        }

        // No ACL and no parent: allow all (global access)
        debug!(
            event_type = "acl.allowed",
            adapter_id = %adapter_id,
            tenant_id = %tenant_id,
            reason = "global_access",
            "ACL check passed (global access)"
        );
        Ok(true)
    }

    /// Register a tenant
    pub fn register_tenant(&self, id: &str, uid: u32, gid: u32) -> Result<()> {
        let conn = self.conn.lock();

        conn.execute(
            "INSERT OR REPLACE INTO tenants (id, uid, gid, created_at) 
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![id, uid, gid],
        )
        .map_err(|e| AosError::Registry(format!("Failed to register tenant: {}", e)))?;

        Ok(())
    }

    /// Get tenant by ID
    pub fn get_tenant(&self, id: &str) -> Result<Option<TenantRecord>> {
        let conn = self.conn.lock();

        let result = conn
            .query_row(
                "SELECT id, uid, gid, created_at FROM tenants WHERE id = ?1",
                params![id],
                |row| {
                    Ok(TenantRecord {
                        id: row.get(0)?,
                        uid: row.get(1)?,
                        gid: row.get(2)?,
                        created_at: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|e| AosError::Registry(format!("Failed to get tenant: {}", e)))?;

        Ok(result)
    }

    /// Update adapter activation percentage
    pub fn update_activation(&self, adapter_id: &str, pct: f32) -> Result<()> {
        let conn = self.conn.lock();

        conn.execute(
            "UPDATE adapters SET activation_pct = ?1 WHERE id = ?2",
            params![pct, adapter_id],
        )
        .map_err(|e| AosError::Registry(format!("Failed to update activation: {}", e)))?;

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

    /// List all adapters
    pub fn list_adapters(&self) -> Result<Vec<AdapterRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, hash, tier, rank, acl, activation_pct, registered_at,
                        adapter_name, parent_id, fork_type, fork_reason
                 FROM adapters",
            )
            .map_err(|e| AosError::Registry(format!("Failed to prepare statement: {}", e)))?;

        let adapters = stmt
            .query_map([], |row| {
                Ok(AdapterRecord {
                    id: row.get(0)?,
                    hash: B3Hash::from_hex(&row.get::<_, String>(1)?)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
                    tier: row.get(2)?,
                    rank: row.get(3)?,
                    acl: serde_json::from_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                    activation_pct: row.get(5)?,
                    registered_at: row.get(6)?,
                    semantic_name: row
                        .get::<_, Option<String>>(7)?
                        .and_then(|s| s.parse::<AdapterName>().ok()),
                    parent_id: row.get(8)?,
                    fork_type: row
                        .get::<_, Option<String>>(9)?
                        .and_then(|s| ForkType::from_str(&s).ok()),
                    fork_reason: row.get(10)?,
                })
            })
            .map_err(|e| AosError::Registry(format!("Failed to query adapters: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AosError::Registry(format!("Failed to collect adapters: {}", e)))?;

        Ok(adapters)
    }
}

#[derive(Debug, Clone)]
pub struct AdapterRecord {
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

impl AdapterRecord {
    /// Get display name (semantic name if available, otherwise ID)
    pub fn display_name(&self) -> String {
        if let Some(ref name) = self.semantic_name {
            name.display_name()
        } else {
            self.id.clone()
        }
    }

    /// Check if adapter is in a lineage
    pub fn is_in_lineage(&self, other: &AdapterRecord) -> bool {
        match (&self.semantic_name, &other.semantic_name) {
            (Some(n1), Some(n2)) => n1.is_same_lineage(n2),
            _ => false,
        }
    }

    /// Check if this adapter is a direct child of another (parent_id match only)
    ///
    /// Note: This only checks the immediate parent, not the full ancestry chain.
    /// For recursive ancestry checking (grandparents, etc.), use `Registry::is_descendant_of()`.
    pub fn is_descendant_of(&self, potential_parent_id: &str) -> bool {
        if let Some(ref parent) = self.parent_id {
            parent == potential_parent_id
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct TenantRecord {
    pub id: String,
    pub uid: u32,
    pub gid: u32,
    pub created_at: String,
}
