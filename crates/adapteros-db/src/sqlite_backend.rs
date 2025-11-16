//! SQLite backend implementation for database abstraction

use super::traits::{CreateStackRequest, DatabaseBackend, StackRecord};
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use tracing::{debug, info};

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
    async fn insert_stack(&self, stack: CreateStackRequest) -> Result<String> {
        let id = self.generate_id();
        let now = self.current_timestamp();
        let adapter_ids_json =
            serde_json::to_string(&stack.adapter_ids).map_err(|e| AosError::Serialization(e))?;

        sqlx::query(
            r#"
            INSERT INTO adapter_stacks (id, name, description, adapter_ids_json, workflow_type, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&stack.name)
        .bind(&stack.description)
        .bind(&adapter_ids_json)
        .bind(&stack.workflow_type)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert stack: {}", e)))?;

        debug!("Inserted stack '{}' with ID: {}", stack.name, id);
        Ok(id)
    }

    async fn get_stack(&self, id: &str) -> Result<Option<StackRecord>> {
        let row = sqlx::query_as::<_, (
            String,           // id
            String,           // name
            Option<String>,   // description
            String,           // adapter_ids_json
            Option<String>,   // workflow_type
            String,           // created_at
            String,           // updated_at
            Option<String>,   // created_by
        )>(
            r#"
            SELECT id, name, description, adapter_ids_json, workflow_type, created_at, updated_at, created_by
            FROM adapter_stacks
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch stack: {}", e)))?;

        Ok(row.map(|r| StackRecord {
            id: r.0,
            name: r.1,
            description: r.2,
            adapter_ids_json: r.3,
            workflow_type: r.4,
            created_at: r.5,
            updated_at: r.6,
            created_by: r.7,
        }))
    }

    async fn list_stacks(&self) -> Result<Vec<StackRecord>> {
        let rows = sqlx::query_as::<_, (
            String,           // id
            String,           // name
            Option<String>,   // description
            String,           // adapter_ids_json
            Option<String>,   // workflow_type
            String,           // created_at
            String,           // updated_at
            Option<String>,   // created_by
        )>(
            r#"
            SELECT id, name, description, adapter_ids_json, workflow_type, created_at, updated_at, created_by
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
                id: r.0,
                name: r.1,
                description: r.2,
                adapter_ids_json: r.3,
                workflow_type: r.4,
                created_at: r.5,
                updated_at: r.6,
                created_by: r.7,
            })
            .collect())
    }

    async fn delete_stack(&self, id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM adapter_stacks
            WHERE id = ?
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to delete stack: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_stack(&self, id: &str, stack: CreateStackRequest) -> Result<bool> {
        let now = self.current_timestamp();
        let adapter_ids_json =
            serde_json::to_string(&stack.adapter_ids).map_err(|e| AosError::Serialization(e))?;

        let result = sqlx::query(
            r#"
            UPDATE adapter_stacks
            SET name = ?, description = ?, adapter_ids_json = ?, workflow_type = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(&stack.name)
        .bind(&stack.description)
        .bind(&adapter_ids_json)
        .bind(&stack.workflow_type)
        .bind(&now)
        .bind(id)
        .execute(&self.pool)
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
}
