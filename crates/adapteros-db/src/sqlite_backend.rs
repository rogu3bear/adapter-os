//! SQLite backend implementation for database abstraction

use super::traits::{CreateStackRequest, DatabaseBackend, StackRecord};
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use sqlx::{sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::str::FromStr;
use tracing::info;

/// SQLite database backend
pub struct SqliteBackend {
    pool: SqlitePool,
}

impl SqliteBackend {
    /// Create a new SQLite backend
    pub async fn new(path: &str) -> Result<Self> {
        info!("Connecting to SQLite database at: {}", path);

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))
            .map_err(|e| AosError::Database(format!("Invalid SQLite path: {}", e)))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

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

        // Sort and deduplicate adapter_ids per PRD 3
        let mut sorted_ids = req.adapter_ids.clone();
        sorted_ids.sort();
        sorted_ids.dedup();

        let adapter_ids_json =
            serde_json::to_string(&sorted_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type = req.workflow_type.as_deref().unwrap_or("parallel");
        let description = req.description.as_deref().unwrap_or("");

        let row = sqlx::query(
            "INSERT INTO adapter_stacks (id, tenant_id, name, description, adapter_ids_json, workflow_type, generation, stack_hash, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 0, NULL, datetime('now'), datetime('now'))
             RETURNING id"
        )
        .bind(&id)
        .bind(&req.tenant_id)
        .bind(&req.name)
        .bind(description)
        .bind(&adapter_ids_json)
        .bind(workflow_type)
        .fetch_one(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert stack: {}", e)))?;

        Ok(row.get(0))
    }

    async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<StackRecord>> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, String, Option<String>, i64, Option<String>, String, String, Option<String>)>(
            "SELECT tenant_id, id, name, description, adapter_ids_json, workflow_type, generation, stack_hash, created_at, updated_at, created_by
             FROM adapter_stacks WHERE tenant_id = ? AND id = ?"
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch stack: {}", e)))?;

        Ok(row.map(|r| StackRecord {
            tenant_id: r.0,
            id: r.1,
            name: r.2,
            description: r.3,
            adapter_ids_json: r.4,
            workflow_type: r.5,
            generation: r.6,
            stack_hash: r.7,
            created_at: r.8,
            updated_at: r.9,
            created_by: r.10,
        }))
    }

    async fn list_stacks(&self) -> Result<Vec<StackRecord>> {
        let rows = sqlx::query_as::<_, (
            String,           // tenant_id
            String,           // id
            String,           // name
            Option<String>,   // description
            String,           // adapter_ids_json
            Option<String>,   // workflow_type
            i64,              // generation
            Option<String>,   // stack_hash
            String,           // created_at
            String,           // updated_at
            Option<String>,   // created_by
        )>(
            r#"
            SELECT tenant_id, id, name, description, adapter_ids_json, workflow_type, generation, stack_hash, created_at, updated_at, created_by
            FROM adapter_stacks
            ORDER BY created_at DESC
            "#
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list stacks: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| StackRecord {
                tenant_id: r.0,
                id: r.1,
                name: r.2,
                description: r.3,
                adapter_ids_json: r.4,
                workflow_type: r.5,
                generation: r.6,
                stack_hash: r.7,
                created_at: r.8,
                updated_at: r.9,
                created_by: r.10,
            })
            .collect())
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

        let result = sqlx::query(
            "UPDATE adapter_stacks SET name = ?, description = ?, adapter_ids_json = ?, workflow_type = ?
             WHERE tenant_id = ? AND id = ?"
        )
        .bind(&stack.name)
        .bind(description)
        .bind(&adapter_ids_json)
        .bind(workflow_type)
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
        // Use the standard migration runner for SQLite
        // Note: We skip sqlx::migrate! macro here to avoid compile-time checks
        // Migrations should be run separately by the application
        info!("SQLite migrations should be run separately");
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
    ) -> Result<Option<super::traits::AdapterRecord>> {
        use super::traits::AdapterRecord;

        let row = sqlx::query_as::<_, AdapterRecord>(
            r#"
            SELECT id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework, active, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason
            FROM adapters
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch adapter: {}", e)))?;

        Ok(row)
    }

    async fn list_stacks_for_tenant(&self, tenant_id: &str) -> Result<Vec<StackRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT tenant_id, id, name, description, adapter_ids_json, workflow_type,
                   generation, stack_hash, created_at, updated_at, created_by
            FROM adapter_stacks
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list stacks for tenant: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r| StackRecord {
                tenant_id: r.get(0),
                id: r.get(1),
                name: r.get(2),
                description: r.get::<Option<String>, _>(3),
                adapter_ids_json: r.get(4),
                workflow_type: r.get::<Option<String>, _>(5),
                generation: r.get(6),
                stack_hash: r.get::<Option<String>, _>(7),
                created_at: r.get(8),
                updated_at: r.get(9),
                created_by: r.get::<Option<String>, _>(10),
            })
            .collect())
    }

    async fn update_stack_generation(
        &self,
        tenant_id: &str,
        stack_id: &str,
        new_generation: u64,
        new_hash: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE adapter_stacks
            SET generation = ?, stack_hash = ?, updated_at = datetime('now')
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(new_generation as i64)
        .bind(new_hash)
        .bind(tenant_id)
        .bind(stack_id)
        .execute(self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update stack generation: {}", e)))?;

        Ok(())
    }
}
