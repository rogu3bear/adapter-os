//! Database abstraction traits for supporting multiple backends
//!
//! This module provides traits that abstract over SQLite and PostgreSQL
//! differences, allowing the application to work with either backend
//! without code changes.

use adapteros_core::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Adapter record from database
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdapterRecord {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub tier: String,
    pub hash_b3: String,
    pub rank: i32,
    pub alpha: f64,
    pub targets_json: String,
    pub acl_json: Option<String>,
    pub adapter_id: Option<String>,
    pub languages_json: Option<String>,
    pub framework: Option<String>,
    pub active: i32,
    pub category: String,
    pub scope: String,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,
    pub current_state: String,
    pub pinned: i32,
    pub memory_bytes: i64,
    pub last_activated: Option<String>,
    pub activation_count: i64,
    pub expires_at: Option<String>,
    pub load_state: String,
    pub last_loaded_at: Option<String>,
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,
}

/// Stack record from database
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StackRecord {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids_json: String,
    pub workflow_type: Option<String>,
    pub generation: i64,
    pub stack_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
}

/// Request to create a new adapter stack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateStackRequest {
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids: Vec<String>,
    pub workflow_type: Option<String>,
}

/// Database backend abstraction trait
///
/// This trait defines operations that must be implemented by both
/// SQLite and PostgreSQL backends. It handles differences in:
/// - SQL syntax (? vs $1 placeholders)
/// - Date/time functions (datetime('now') vs NOW())
/// - UUID generation (randomblob(16) vs gen_random_uuid())
#[async_trait]
pub trait DatabaseBackend: Send + Sync {
    /// Insert a new adapter stack
    async fn insert_stack(&self, req: &CreateStackRequest) -> Result<String>;

    /// Get a stack by ID
    async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<StackRecord>>;

    /// List all stacks
    async fn list_stacks(&self) -> Result<Vec<StackRecord>>;

    /// List stacks for a specific tenant
    async fn list_stacks_for_tenant(&self, tenant_id: &str) -> Result<Vec<StackRecord>>;

    /// Delete a stack by ID
    async fn delete_stack(&self, tenant_id: &str, id: &str) -> Result<bool>;

    /// Update a stack
    async fn update_stack(
        &self,
        tenant_id: &str,
        id: &str,
        stack: &CreateStackRequest,
    ) -> Result<bool>;

    /// Get adapter by ID and tenant
    async fn get_adapter_by_id(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Option<AdapterRecord>>;

    /// Get the database type (for debugging/logging)
    fn database_type(&self) -> &str;

    /// Run migrations for this backend
    async fn run_migrations(&self) -> Result<()>;

    /// Check if a table exists
    async fn table_exists(&self, table_name: &str) -> Result<bool>;

    /// Generate a new unique ID (UUID or equivalent)
    fn generate_id(&self) -> String;

    /// Format current timestamp for this database
    fn current_timestamp(&self) -> String;

    /// Update stack generation and hash (used during activation)
    async fn update_stack_generation(
        &self,
        tenant_id: &str,
        stack_id: &str,
        new_generation: u64,
        new_hash: Option<&str>,
    ) -> Result<()>;
}

/// Helper trait for converting between database-specific types
pub trait FromRow<R> {
    fn from_row(row: R) -> Result<Self>
    where
        Self: Sized;
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Backend type: "sqlite" or "postgres"
    pub backend: DatabaseBackendType,
    /// Connection URL or path
    pub url: String,
    /// Maximum connections in pool
    pub max_connections: u32,
    /// Connection timeout in seconds
    pub connect_timeout_seconds: u64,
}

/// Supported database backend types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseBackendType {
    Sqlite,
    Postgres,
}

impl DatabaseBackendType {
    /// Get the backend type from a string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sqlite" => Some(Self::Sqlite),
            "postgres" | "postgresql" => Some(Self::Postgres),
            _ => None,
        }
    }

    /// Check if this is SQLite
    pub fn is_sqlite(&self) -> bool {
        matches!(self, Self::Sqlite)
    }

    /// Check if this is PostgreSQL
    pub fn is_postgres(&self) -> bool {
        matches!(self, Self::Postgres)
    }
}

impl std::fmt::Display for DatabaseBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite => write!(f, "sqlite"),
            Self::Postgres => write!(f, "postgres"),
        }
    }
}

/// Factory function to create the appropriate database backend
pub async fn create_database_backend(config: &DatabaseConfig) -> Result<Box<dyn DatabaseBackend>> {
    match config.backend {
        DatabaseBackendType::Sqlite => {
            let backend = super::sqlite_backend::SqliteBackend::new(&config.url).await?;
            Ok(Box::new(backend))
        }
        DatabaseBackendType::Postgres => {
            let backend = super::postgres_backend::PostgresBackend::new(&config.url).await?;
            Ok(Box::new(backend))
        }
    }
}
