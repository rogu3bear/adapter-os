//! SQLite registry for adapters, tenants, and ACLs

use adapteros_core::{AosError, B3Hash, Result};
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use std::sync::Arc;

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
        // PRAGMA returns results, so use query instead of execute
        conn.query_row("PRAGMA journal_mode=WAL", [], |row| {
            let mode: String = row.get(0)?;
            Ok(mode)
        })
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

    /// Create a new adapter stack
    pub fn create_stack(&self, name: &str, description: Option<&str>, adapter_ids: &[String]) -> Result<()> {
        let conn = self.conn.lock();

        // Validate that all adapters exist
        for adapter_id in adapter_ids {
            if self.get_adapter(adapter_id)?.is_none() {
                return Err(AosError::Registry(format!(
                    "Adapter {} does not exist",
                    adapter_id
                )));
            }
        }

        // Insert stack
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO adapter_stacks (name, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![name, description, &now, &now],
        )
        .map_err(|e| AosError::Registry(format!("Failed to create stack: {}", e)))?;

        // Insert stack members (sorted by adapter_id for determinism)
        let mut sorted_ids = adapter_ids.to_vec();
        sorted_ids.sort();
        for (position, adapter_id) in sorted_ids.iter().enumerate() {
            conn.execute(
                "INSERT INTO stack_members (stack_name, adapter_id, position) VALUES (?1, ?2, ?3)",
                params![name, adapter_id, position as i32],
            )
            .map_err(|e| AosError::Registry(format!("Failed to add adapter to stack: {}", e)))?;
        }

        Ok(())
    }

    /// Get stack by name
    pub fn get_stack(&self, name: &str) -> Result<Option<AdapterStack>> {
        let conn = self.conn.lock();

        let result = conn
            .query_row(
                "SELECT name, description, created_at, updated_at FROM adapter_stacks WHERE name = ?1",
                params![name],
                |row| {
                    Ok(AdapterStack {
                        name: row.get(0)?,
                        description: row.get(1)?,
                        created_at: row.get(2)?,
                        updated_at: row.get(3)?,
                        adapter_ids: Vec::new(), // Will be filled below
                    })
                },
            )
            .optional()
            .map_err(|e| AosError::Registry(format!("Failed to get stack: {}", e)))?;

        if let Some(mut stack) = result {
            // Get adapter IDs for this stack (ordered by position)
            let mut stmt = conn
                .prepare("SELECT adapter_id FROM stack_members WHERE stack_name = ?1 ORDER BY position")
                .map_err(|e| AosError::Registry(format!("Failed to prepare statement: {}", e)))?;

            let adapter_ids: Vec<String> = stmt
                .query_map(params![name], |row| row.get(0))
                .map_err(|e| AosError::Registry(format!("Failed to query stack members: {}", e)))?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| AosError::Registry(format!("Failed to collect stack members: {}", e)))?;

            stack.adapter_ids = adapter_ids;
            Ok(Some(stack))
        } else {
            Ok(None)
        }
    }

    /// List all stacks
    pub fn list_stacks(&self) -> Result<Vec<AdapterStack>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare("SELECT name, description, created_at, updated_at FROM adapter_stacks ORDER BY name")
            .map_err(|e| AosError::Registry(format!("Failed to prepare statement: {}", e)))?;

        let stacks = stmt
            .query_map([], |row| {
                Ok(AdapterStack {
                    name: row.get(0)?,
                    description: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    adapter_ids: Vec::new(),
                })
            })
            .map_err(|e| AosError::Registry(format!("Failed to query stacks: {}", e)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| AosError::Registry(format!("Failed to collect stacks: {}", e)))?;

        // Populate adapter_ids for each stack
        let mut result = Vec::new();
        for mut stack in stacks {
            if let Some(full_stack) = self.get_stack(&stack.name)? {
                stack.adapter_ids = full_stack.adapter_ids;
            }
            result.push(stack);
        }

        Ok(result)
    }

    /// Update a stack's description and/or adapter list
    pub fn update_stack(&self, name: &str, description: Option<&str>, adapter_ids: Option<&[String]>) -> Result<()> {
        let conn = self.conn.lock();

        // Check stack exists
        if self.get_stack(name)?.is_none() {
            return Err(AosError::Registry(format!("Stack {} does not exist", name)));
        }

        let now = chrono::Utc::now().to_rfc3339();

        // Update description if provided
        if let Some(desc) = description {
            conn.execute(
                "UPDATE adapter_stacks SET description = ?1, updated_at = ?2 WHERE name = ?3",
                params![desc, &now, name],
            )
            .map_err(|e| AosError::Registry(format!("Failed to update stack: {}", e)))?;
        }

        // Update adapter list if provided
        if let Some(adapter_ids) = adapter_ids {
            // Validate all adapters exist
            for adapter_id in adapter_ids {
                if self.get_adapter(adapter_id)?.is_none() {
                    return Err(AosError::Registry(format!(
                        "Adapter {} does not exist",
                        adapter_id
                    )));
                }
            }

            // Remove existing members
            conn.execute("DELETE FROM stack_members WHERE stack_name = ?1", params![name])
                .map_err(|e| AosError::Registry(format!("Failed to update stack members: {}", e)))?;

            // Insert new members (sorted by adapter_id for determinism)
            let mut sorted_ids = adapter_ids.to_vec();
            sorted_ids.sort();
            for (position, adapter_id) in sorted_ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO stack_members (stack_name, adapter_id, position) VALUES (?1, ?2, ?3)",
                    params![name, adapter_id, position as i32],
                )
                .map_err(|e| AosError::Registry(format!("Failed to add adapter to stack: {}", e)))?;
            }

            // Update timestamp
            conn.execute(
                "UPDATE adapter_stacks SET updated_at = ?1 WHERE name = ?2",
                params![&now, name],
            )
            .map_err(|e| AosError::Registry(format!("Failed to update stack timestamp: {}", e)))?;
        }

        Ok(())
    }

    /// Delete a stack
    pub fn delete_stack(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock();

        // Check if this is the active stack
        if let Some(active) = self.get_active_stack()? {
            if active == name {
                return Err(AosError::Registry(
                    "Cannot delete active stack. Deactivate it first.".to_string()
                ));
            }
        }

        let rows_affected = conn.execute("DELETE FROM adapter_stacks WHERE name = ?1", params![name])
            .map_err(|e| AosError::Registry(format!("Failed to delete stack: {}", e)))?;

        if rows_affected == 0 {
            return Err(AosError::Registry(format!("Stack {} does not exist", name)));
        }

        Ok(())
    }

    /// Activate a stack (sets it as the active stack for routing)
    pub fn activate_stack(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock();

        // Verify stack exists
        if self.get_stack(name)?.is_none() {
            return Err(AosError::Registry(format!("Stack {} does not exist", name)));
        }

        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE active_stack SET stack_name = ?1, activated_at = ?2 WHERE id = 1",
            params![name, &now],
        )
        .map_err(|e| AosError::Registry(format!("Failed to activate stack: {}", e)))?;

        Ok(())
    }

    /// Deactivate the active stack (return to normal routing)
    pub fn deactivate_stack(&self) -> Result<()> {
        let conn = self.conn.lock();

        conn.execute(
            "UPDATE active_stack SET stack_name = NULL, activated_at = NULL WHERE id = 1",
            [],
        )
        .map_err(|e| AosError::Registry(format!("Failed to deactivate stack: {}", e)))?;

        Ok(())
    }

    /// Get the currently active stack name
    pub fn get_active_stack(&self) -> Result<Option<String>> {
        let conn = self.conn.lock();

        let result = conn
            .query_row(
                "SELECT stack_name FROM active_stack WHERE id = 1",
                [],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()
            .map_err(|e| AosError::Registry(format!("Failed to get active stack: {}", e)))?;

        Ok(result.flatten())
    }

    /// Get adapter IDs in the active stack (or None if no stack is active)
    pub fn get_active_stack_adapters(&self) -> Result<Option<Vec<String>>> {
        if let Some(stack_name) = self.get_active_stack()? {
            if let Some(stack) = self.get_stack(&stack_name)? {
                Ok(Some(stack.adapter_ids))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
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

#[derive(Debug, Clone)]
pub struct AdapterStack {
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}
