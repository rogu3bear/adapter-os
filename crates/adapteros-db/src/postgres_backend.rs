//! PostgreSQL backend implementation for database abstraction

use super::traits::{CreateStackRequest, DatabaseBackend, StackRecord};
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use sqlx::{postgres::PgConnectOptions, PgPool};
use std::str::FromStr;
use tracing::{debug, info};

/// PostgreSQL database backend
pub struct PostgresBackend {
    pool: PgPool,
}

impl PostgresBackend {
    /// Create a new PostgreSQL backend
    pub async fn new(url: &str) -> Result<Self> {
        info!("Connecting to PostgreSQL database");

        let options = PgConnectOptions::from_str(url)
            .map_err(|e| AosError::Database(format!("Invalid PostgreSQL URL: {}", e)))?;

        let pool = PgPool::connect_with(options)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to PostgreSQL: {}", e)))?;

        Ok(Self { pool })
    }

    /// Get the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl DatabaseBackend for PostgresBackend {
    async fn insert_stack(&self, stack: CreateStackRequest) -> Result<String> {
        let id = self.generate_id();
        let adapter_ids_json =
            serde_json::to_string(&stack.adapter_ids).map_err(|e| AosError::Serialization(e))?;

        sqlx::query(
            r#"
            INSERT INTO adapter_stacks (id, name, description, adapter_ids_json, workflow_type, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
            "#,
        )
        .bind(&id)
        .bind(&stack.name)
        .bind(&stack.description)
        .bind(&adapter_ids_json)
        .bind(&stack.workflow_type)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert stack: {}", e)))?;

        debug!("Inserted stack '{}' with ID: {}", stack.name, id);
        Ok(id)
    }

    async fn get_stack(&self, id: &str) -> Result<Option<StackRecord>> {
        let row = sqlx::query_as::<
            _,
            (
                String,         // id
                String,         // name
                Option<String>, // description
                String,         // adapter_ids_json
                Option<String>, // workflow_type
                String,         // created_at
                String,         // updated_at
                Option<String>, // created_by
            ),
        >(
            r#"
            SELECT id, name, description, adapter_ids_json, workflow_type,
                   created_at::text as "created_at",
                   updated_at::text as "updated_at",
                   created_by
            FROM adapter_stacks
            WHERE id = $1
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
        let rows = sqlx::query_as::<
            _,
            (
                String,         // id
                String,         // name
                Option<String>, // description
                String,         // adapter_ids_json
                Option<String>, // workflow_type
                String,         // created_at
                String,         // updated_at
                Option<String>, // created_by
            ),
        >(
            r#"
            SELECT id, name, description, adapter_ids_json, workflow_type,
                   created_at::text as "created_at",
                   updated_at::text as "updated_at",
                   created_by
            FROM adapter_stacks
            ORDER BY created_at DESC
            "#,
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
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to delete stack: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    async fn update_stack(&self, id: &str, stack: CreateStackRequest) -> Result<bool> {
        let adapter_ids_json =
            serde_json::to_string(&stack.adapter_ids).map_err(|e| AosError::Serialization(e))?;

        let result = sqlx::query(
            r#"
            UPDATE adapter_stacks
            SET name = $1, description = $2, adapter_ids_json = $3, workflow_type = $4, updated_at = NOW()
            WHERE id = $5
            "#,
        )
        .bind(&stack.name)
        .bind(&stack.description)
        .bind(&adapter_ids_json)
        .bind(&stack.workflow_type)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update stack: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    fn database_type(&self) -> &str {
        "postgres"
    }

    async fn run_migrations(&self) -> Result<()> {
        // For PostgreSQL, we need to use PostgreSQL-specific migrations
        // Note: We skip sqlx::migrate! macro here to avoid compile-time checks
        // Migrations should be run separately by the application
        info!("PostgreSQL migrations should be run separately");
        Ok(())
    }

    async fn table_exists(&self, table_name: &str) -> Result<bool> {
        let exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (
                SELECT FROM information_schema.tables
                WHERE table_schema = 'public'
                AND table_name = $1
            )",
        )
        .bind(table_name)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to check table existence: {}", e)))?;

        Ok(exists)
    }

    fn generate_id(&self) -> String {
        // PostgreSQL can use gen_random_uuid() but we'll use Rust UUID for consistency
        uuid::Uuid::now_v7().to_string()
    }

    fn current_timestamp(&self) -> String {
        // This is for display purposes; actual DB uses NOW()
        chrono::Utc::now().to_rfc3339()
    }
}
