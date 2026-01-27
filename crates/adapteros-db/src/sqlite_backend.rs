//! SQLite backend implementation for database abstraction

use super::traits::{AdapterRecordRow, CreateStackRequest, DatabaseBackend, StackRecordRow};
use adapteros_core::{AosError, Result};
use adapteros_types::adapters::{AdapterRecord, StackRecord};
use async_trait::async_trait;
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use tracing::info;

/// SQLite database backend
pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    /// Create a new SQLite backend
    ///
    /// **CRITICAL:** Enables foreign key enforcement on all connections.
    pub async fn new(path: &str) -> Result<Self> {
        info!("Connecting to SQLite database at: {}", path);

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))
            .map_err(|e| AosError::Database(format!("Invalid SQLite path: {}", e)))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .foreign_keys(true); // CRITICAL: Enable foreign key constraints

        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to SQLite: {}", e)))?;

        Ok(Self { pool })
    }

    /// Get the connection pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

#[async_trait]
impl DatabaseBackend for SqliteBackend {
    async fn insert_stack(&self, req: &CreateStackRequest) -> Result<String> {
        let id = self.generate_id();
        let adapter_ids_json =
            serde_json::to_string(&req.adapter_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type = req.workflow_type.as_deref().unwrap_or("Parallel");
        let description = req.description.as_deref().unwrap_or("");

        let row = sqlx::query(
            "INSERT INTO adapter_stacks (id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, created_at, updated_at, determinism_mode, routing_determinism_mode)
             VALUES (?, ?, ?, ?, ?, ?, 1, 'active', datetime('now'), datetime('now'), ?, ?)
             RETURNING id"
        )
        .bind(&id)
        .bind(&req.tenant_id)
        .bind(&req.name)
        .bind(description)
        .bind(&adapter_ids_json)
        .bind(workflow_type)
        .bind(&req.determinism_mode)
        .bind(&req.routing_determinism_mode)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert stack: {}", e)))?;

        Ok(row.get(0))
    }

    async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<StackRecord>> {
        let row = sqlx::query_as::<_, StackRecordRow>(
            "SELECT tenant_id, id, name, description, adapter_ids_json, workflow_type, CAST(version AS INTEGER) AS version, lifecycle_state, created_at, updated_at, created_by, determinism_mode, routing_determinism_mode, metadata_json
             FROM adapter_stacks WHERE tenant_id = ? AND id = ?"
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch stack: {}", e)))?;

        Ok(row.map(StackRecord::from))
    }

    async fn list_stacks(&self) -> Result<Vec<StackRecord>> {
        let rows = sqlx::query_as::<_, StackRecordRow>(
            r#"
            SELECT tenant_id, id, name, description, adapter_ids_json, workflow_type, CAST(version AS INTEGER) AS version, lifecycle_state, created_at, updated_at, created_by, determinism_mode, routing_determinism_mode, metadata_json
            FROM adapter_stacks
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list stacks: {}", e)))?;

        Ok(rows.into_iter().map(StackRecord::from).collect())
    }

    async fn delete_stack(&self, tenant_id: &str, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM adapter_stacks WHERE tenant_id = ? AND id = ?")
            .bind(tenant_id)
            .bind(id)
            .execute(self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete stack: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_stack(
        &self,
        _tenant_id: &str,
        id: &str,
        stack: &CreateStackRequest,
    ) -> Result<bool> {
        let adapter_ids_json =
            serde_json::to_string(&stack.adapter_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type = stack.workflow_type.as_deref().unwrap_or("parallel");
        let description = stack.description.as_deref().unwrap_or("");

        let tenant_id = &stack.tenant_id;

        // CRITICAL FIX: Use a single atomic UPDATE with conditional version increment
        // This prevents race conditions where two concurrent updates could both read
        // the same version and both increment it, resulting in lost updates.
        //
        // Strategy: Always do a conditional update based on comparing JSON values
        // directly in SQL, eliminating the SELECT-then-UPDATE race window.
        let result = sqlx::query(
            r#"
            UPDATE adapter_stacks
            SET name = ?,
                description = ?,
                adapter_ids_json = ?,
                workflow_type = ?,
                determinism_mode = ?,
                routing_determinism_mode = ?,
                version = CASE
                    WHEN adapter_ids_json != ? OR workflow_type != ?
                    THEN version + 1
                    ELSE version
                END,
                updated_at = datetime('now')
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(&stack.name)
        .bind(description)
        .bind(&adapter_ids_json)
        .bind(workflow_type)
        .bind(&stack.determinism_mode)
        .bind(&stack.routing_determinism_mode)
        .bind(&adapter_ids_json) // For comparison
        .bind(workflow_type) // For comparison
        .bind(tenant_id)
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update stack: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    fn database_type(&self) -> &str {
        "sqlite"
    }

    async fn run_migrations(&self) -> Result<()> {
        // Run workspace migrations to mirror Db::migrate behavior for tests
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| AosError::Database("Failed to locate workspace root".to_string()))?;
        let migrations_path = workspace_root.join("migrations");

        let migrator = sqlx::migrate::Migrator::new(migrations_path.clone())
            .await
            .map_err(|e| AosError::Database(format!("Failed to load migrations: {}", e)))?;

        tokio::time::timeout(crate::Db::migration_timeout(), migrator.run(&self.pool))
            .await
            .map_err(|_| {
                AosError::Database(
                    "Migration timed out while waiting for database lock. Run `aosctl db unlock` and retry."
                        .to_string(),
                )
            })?
            .map_err(|e| AosError::Database(format!("Migration failed: {}", e)))?;

        Ok(())
    }

    async fn table_exists(&self, table_name: &str) -> Result<bool> {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?",
        )
        .bind(table_name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to check table existence: {}", e)))?;

        Ok(exists > 0)
    }

    fn generate_id(&self) -> String {
        // Generate a UUID-like ID for SQLite
        uuid::Uuid::now_v7().to_string()
    }

    fn current_timestamp(&self) -> String {
        // SQLite-compatible timestamp
        chrono::Utc::now().to_rfc3339()
    }

    async fn get_adapter_by_id(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Option<AdapterRecord>> {
        let row = sqlx::query_as::<_, AdapterRecordRow>(
            r#"
            SELECT id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework, active, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, activation_count, expires_at, load_state, last_loaded_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, version, lifecycle_state, lora_strength, archived_at, archived_by, archive_reason, purged_at
            FROM adapters
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch adapter: {}", e)))?;

        Ok(row.map(AdapterRecord::from))
    }

    async fn list_stacks_for_tenant(&self, tenant_id: &str) -> Result<Vec<StackRecord>> {
        let rows = sqlx::query_as::<_, StackRecordRow>(
            r#"
            SELECT tenant_id, id, name, description, adapter_ids_json, workflow_type,
                   CAST(version AS INTEGER) AS version, lifecycle_state, created_at, updated_at, created_by, determinism_mode, routing_determinism_mode, metadata_json
            FROM adapter_stacks
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list stacks for tenant: {}", e)))?;

        Ok(rows.into_iter().map(StackRecord::from).collect())
    }
}
