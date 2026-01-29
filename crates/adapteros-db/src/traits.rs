//! Database abstraction traits for supporting multiple backends
//!
//! This module provides traits that abstract database backends
//! differences, allowing the application to work with either backend
//! without code changes.

use adapteros_core::Result;
pub use adapteros_types::adapters::{AdapterRecord, CreateStackRequest, StackRecord};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow as SqlxFromRow, Sqlite};

/// Database-specific row for AdapterRecord
#[derive(Debug, Clone, Serialize, Deserialize, SqlxFromRow)]
pub struct AdapterRecordRow {
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
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,
    pub version: String,
    pub lifecycle_state: String,
    pub lora_strength: Option<f32>,
    pub archived_at: Option<String>,
    pub archived_by: Option<String>,
    pub archive_reason: Option<String>,
    pub purged_at: Option<String>,
}

impl From<AdapterRecordRow> for AdapterRecord {
    fn from(row: AdapterRecordRow) -> Self {
        Self {
            id: row.id,
            tenant_id: row.tenant_id,
            name: row.name,
            tier: row.tier,
            hash_b3: row.hash_b3,
            rank: row.rank,
            alpha: row.alpha,
            targets_json: row.targets_json,
            acl_json: row.acl_json,
            adapter_id: row.adapter_id,
            languages_json: row.languages_json,
            framework: row.framework,
            active: row.active != 0,
            category: row.category,
            scope: row.scope,
            framework_id: row.framework_id,
            framework_version: row.framework_version,
            repo_id: row.repo_id,
            commit_sha: row.commit_sha,
            intent: row.intent,
            current_state: row.current_state,
            pinned: row.pinned != 0,
            memory_bytes: row.memory_bytes,
            last_activated: row.last_activated,
            activation_count: row.activation_count,
            expires_at: row.expires_at,
            load_state: row.load_state,
            last_loaded_at: row.last_loaded_at,
            adapter_name: row.adapter_name,
            tenant_namespace: row.tenant_namespace,
            domain: row.domain,
            purpose: row.purpose,
            revision: row.revision,
            parent_id: row.parent_id,
            fork_type: row.fork_type,
            fork_reason: row.fork_reason,
            version: row.version,
            lifecycle_state: row.lifecycle_state,
            lora_strength: row.lora_strength,
            archived_at: row.archived_at,
            archived_by: row.archived_by,
            archive_reason: row.archive_reason,
            purged_at: row.purged_at,
        }
    }
}

/// Database-specific row for StackRecord
#[derive(Debug, Clone, Serialize, Deserialize, SqlxFromRow)]
pub struct StackRecordRow {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: Option<String>,
    pub adapter_ids_json: String,
    pub workflow_type: Option<String>,
    pub lifecycle_state: String,
    pub created_at: String,
    pub updated_at: String,
    pub created_by: Option<String>,
    pub version: String,
    pub determinism_mode: Option<String>,
    pub routing_determinism_mode: Option<String>,
    pub metadata_json: Option<String>,
}

impl From<StackRecordRow> for StackRecord {
    fn from(row: StackRecordRow) -> Self {
        Self {
            id: row.id,
            tenant_id: row.tenant_id,
            name: row.name,
            description: row.description,
            adapter_ids_json: row.adapter_ids_json,
            workflow_type: row.workflow_type,
            lifecycle_state: row.lifecycle_state,
            created_at: row.created_at,
            updated_at: row.updated_at,
            created_by: row.created_by,
            version: row.version,
            determinism_mode: row.determinism_mode,
            routing_determinism_mode: row.routing_determinism_mode,
            metadata_json: row.metadata_json,
        }
    }
}

/// Database backend abstraction trait
///
/// This trait defines operations that must be implemented by both
/// Currently supports SQLite backend. It handles differences in:
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
    /// Backend type: "sqlite"
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
}

impl DatabaseBackendType {
    /// Get the backend type from a string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "sqlite" => Some(Self::Sqlite),
            _ => None,
        }
    }

    /// Check if this is SQLite
    pub fn is_sqlite(&self) -> bool {
        matches!(self, Self::Sqlite)
    }
}

impl std::fmt::Display for DatabaseBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite => write!(f, "sqlite"),
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
    }
}
