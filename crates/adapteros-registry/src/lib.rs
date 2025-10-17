//! SQLite registry for adapters, tenants, and ACLs

use adapteros_core::{AosError, B3Hash, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;

pub mod dependencies;
pub mod eviction;
pub mod migrations;
pub mod models;

pub use dependencies::{DependencyGraph, DependencyResolver};
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
        conn.pragma_update(None, "journal_mode", &"WAL")
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

    /// Create a dependency resolver backed by this registry instance.
    pub fn dependency_resolver(&self) -> DependencyResolver<'_> {
        dependencies::DependencyResolver::new(self)
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
        let conn = self.conn.lock();
        let acl_json = serde_json::to_string(acl)?;

        conn.execute(
            "INSERT OR REPLACE INTO adapters (id, hash, tier, rank, acl, registered_at) 
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            params![id, hash.to_hex(), tier, rank, acl_json],
        )
        .map_err(|e| AosError::Registry(format!("Failed to register adapter: {}", e)))?;

        Ok(())
    }

    /// Lookup adapter by ID
    pub fn get_adapter(&self, id: &str) -> Result<Option<AdapterRecord>> {
        let conn = self.conn.lock();

        let result = conn
            .query_row(
                "SELECT id, hash, tier, rank, acl, activation_pct, registered_at 
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
                    })
                },
            )
            .optional()
            .map_err(|e| AosError::Registry(format!("Failed to get adapter: {}", e)))?;

        Ok(result)
    }

    /// Check if adapter is allowed for tenant
    pub fn check_acl(&self, adapter_id: &str, tenant_id: &str) -> Result<bool> {
        if let Some(adapter) = self.get_adapter(adapter_id)? {
            // Empty ACL means allow all
            if adapter.acl.is_empty() {
                return Ok(true);
            }
            Ok(adapter.acl.contains(&tenant_id.to_string()))
        } else {
            Ok(false)
        }
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

    /// List all adapters
    pub fn list_adapters(&self) -> Result<Vec<AdapterRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT id, hash, tier, rank, acl, activation_pct, registered_at FROM adapters",
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
}

#[derive(Debug, Clone)]
pub struct TenantRecord {
    pub id: String,
    pub uid: u32,
    pub gid: u32,
    pub created_at: String,
}
