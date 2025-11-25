//! PostgreSQL backend implementation for database abstraction

use super::traits::{CreateStackRequest, DatabaseBackend, StackRecord};
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use sqlx::{postgres::PgConnectOptions, PgPool, Row};
use std::str::FromStr;
use tracing::info;

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
    async fn insert_stack(&self, req: &CreateStackRequest) -> Result<String> {
        let id = self.generate_id();
        let adapter_ids_json =
            serde_json::to_string(&req.adapter_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type = req.workflow_type.as_deref().unwrap_or("parallel");
        let description = req.description.as_deref().unwrap_or("");

        let row = sqlx::query(
            "INSERT INTO adapter_stacks (id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, '1.0.0', 'active', NOW(), NOW())
             RETURNING id"
        )
        .bind(&id)
        .bind(&req.tenant_id)
        .bind(&req.name)
        .bind(description)
        .bind(&adapter_ids_json)
        .bind(workflow_type)
        .fetch_one(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert stack: {}", e)))?;

        Ok(row.get(0))
    }

    async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<StackRecord>> {
        let row = sqlx::query_as::<_, (String, String, String, Option<String>, String, Option<String>, String, String, String, String, Option<String>)>(
            "SELECT tenant_id, id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, created_at::text, updated_at::text, created_by
             FROM adapter_stacks WHERE tenant_id = $1 AND id = $2"
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch stack: {}", e)))?;

        Ok(row.map(|r| StackRecord {
            tenant_id: r.0,
            id: r.1,
            name: r.2,
            description: r.3,
            adapter_ids_json: r.4,
            workflow_type: r.5,
            version: r.6,
            lifecycle_state: r.7,
            created_at: r.8,
            updated_at: r.9,
            created_by: r.10,
        }))
    }

    async fn list_stacks(&self) -> Result<Vec<StackRecord>> {
        let rows = sqlx::query_as::<
            _,
            (
                String,         // tenant_id
                String,         // id
                String,         // name
                Option<String>, // description
                String,         // adapter_ids_json
                Option<String>, // workflow_type
                String,         // version
                String,         // lifecycle_state
                String,         // created_at
                String,         // updated_at
                Option<String>, // created_by
                i64,            // version
            ),
        >(
            r#"
            SELECT tenant_id, id, name, description, adapter_ids_json, workflow_type,
                   version, lifecycle_state,
                   created_at::text as "created_at",
                   updated_at::text as "updated_at",
                   created_by,
                   version
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
                tenant_id: r.0,
                id: r.1,
                name: r.2,
                description: r.3,
                adapter_ids_json: r.4,
                workflow_type: r.5,
                version: r.6,
                lifecycle_state: r.7,
                created_at: r.8,
                updated_at: r.9,
                created_by: r.10,
            })
            .collect())
    }

    async fn delete_stack(&self, tenant_id: &str, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM adapter_stacks WHERE tenant_id = $1 AND id = $2")
            .bind(tenant_id)
            .bind(id)
            .execute(&*self.pool())
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

        // First, fetch the current stack to check if version should increment
        let current = self.get_stack(tenant_id, id).await?;

        let should_increment_version = if let Some(current_stack) = current {
            // Increment version if adapter_ids or workflow_type changed
            let adapter_ids_changed = current_stack.adapter_ids_json != adapter_ids_json;
            let workflow_type_changed =
                current_stack.workflow_type.as_deref() != Some(workflow_type);
            adapter_ids_changed || workflow_type_changed
        } else {
            false // Stack doesn't exist, won't update
        };

        let result = if should_increment_version {
            sqlx::query(
                "UPDATE adapter_stacks
                 SET name = $3, description = $4, adapter_ids_json = $5, workflow_type = $6,
                     version = version + 1, updated_at = NOW()
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant_id)
            .bind(id)
            .bind(&stack.name)
            .bind(description)
            .bind(&adapter_ids_json)
            .bind(workflow_type)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update stack: {}", e)))?
        } else {
            sqlx::query(
                "UPDATE adapter_stacks
                 SET name = $3, description = $4, adapter_ids_json = $5, workflow_type = $6,
                     updated_at = NOW()
                 WHERE tenant_id = $1 AND id = $2",
            )
            .bind(tenant_id)
            .bind(id)
            .bind(&stack.name)
            .bind(description)
            .bind(&adapter_ids_json)
            .bind(workflow_type)
            .execute(&*self.pool())
            .await
            .map_err(|e| AosError::Database(format!("Failed to update stack: {}", e)))?
        };

        Ok(result.rows_affected() > 0)
    }

    async fn get_adapter_by_id(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Option<super::traits::AdapterRecord>> {
        use super::traits::AdapterRecord;

        let row = sqlx::query_as::<_, AdapterRecord>(
            r#"
            SELECT id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework, active, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, activation_count, expires_at, load_state, last_loaded_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason
            FROM adapters
            WHERE tenant_id = $1 AND id = $2
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
                   version, lifecycle_state,
                   created_at::text as "created_at",
                   updated_at::text as "updated_at",
                   created_by,
                   version
            FROM adapter_stacks
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to list stacks for tenant: {}", e)))?;

        Ok(rows
            .into_iter()
            .map(|r: sqlx::postgres::PgRow| StackRecord {
                tenant_id: r.get(0),
                id: r.get(1),
                name: r.get(2),
                description: r.get::<Option<String>, _>(3),
                adapter_ids_json: r.get(4),
                workflow_type: r.get::<Option<String>, _>(5),
                version: r.get(6),
                lifecycle_state: r.get(7),
                created_at: r.get(8),
                updated_at: r.get(9),
                created_by: r.get::<Option<String>, _>(10),
            })
            .collect())
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
