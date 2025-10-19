use adapteros_core::AosError;
use anyhow::Result;
use sqlx::SqlitePool;

// PostgreSQL backend for production
pub mod postgres;
pub use postgres::PostgresDb;

/// Database connection pool and query methods (SQLite)
///
/// For production deployments, use `PostgresDb` instead.
#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Connect to SQLite database with WAL mode
    ///
    /// Accepts either a filesystem path (e.g., "var/aos.db") or a full URL (e.g.,
    /// "sqlite://var/aos.db"). Prevents double-prefix issues in tests.
    pub async fn connect(path: &str) -> Result<Self> {
        // Normalize input to a SQLite URL understood by sqlx
        let url = if path.starts_with("sqlite://") {
            // Convert triple-slash form to single-colon absolute path style
            // e.g., sqlite:///abs/path.db -> sqlite:/abs/path.db
            format!("sqlite:{}", &path["sqlite://".len()..])
        } else if path.starts_with("sqlite:") {
            path.to_string()
        } else if path.starts_with('/') {
            // Absolute filesystem path
            format!("sqlite:{}", path)
        } else {
            // Relative filesystem path
            format!("sqlite://{}", path)
        };

        // Establish connection via URL (creates DB file if missing)
        let pool = SqlitePool::connect(&url).await?;

        // Apply recommended pragmas for concurrency and performance
        let _ = sqlx::query("PRAGMA journal_mode = WAL;")
            .execute(&pool)
            .await;
        let _ = sqlx::query("PRAGMA synchronous = NORMAL;")
            .execute(&pool)
            .await;

        Ok(Self { pool })
    }

    /// Connect to SQLite database using DATABASE_URL environment variable
    pub async fn connect_env() -> Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "./var/cp.db".to_string());
        Self::connect(&database_url).await
    }

    /// Run database migrations (SQLite)
    pub async fn migrate(&self) -> Result<()> {
        // Embed migrations at compile time for reproducibility
        sqlx::migrate!("../../migrations")
            .run(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Migration failed: {}", e)))?;

        Ok(())
    }

    /// Seed database with development data
    pub async fn seed_dev_data(&self) -> Result<()> {
        use argon2::{
            password_hash::{PasswordHasher, SaltString},
            Argon2,
        };
        use rand::rngs::OsRng;

        // Check if data already exists
        let user_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;

        if user_count > 0 {
            tracing::info!("Database already contains data, skipping seed");
            return Ok(());
        }

        tracing::info!("Seeding development data...");

        // Create default tenant
        sqlx::query(
            "INSERT INTO tenants (id, name, created_at) 
             VALUES ('default', 'default', datetime('now'))",
        )
        .execute(&self.pool)
        .await?;

        // Create demo users with hashed passwords
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let password_hash = argon2
            .hash_password("password".as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("failed to hash password: {}", e))?
            .to_string();

        let users = vec![
            ("admin@aos.local", "Admin", "Admin", &password_hash),
            ("operator@aos.local", "Operator", "Operator", &password_hash),
            ("sre@aos.local", "SRE", "SRE", &password_hash),
            ("viewer@aos.local", "Viewer", "Viewer", &password_hash),
        ];

        for (email, display_name, role, pwd_hash) in users {
            let username = email
                .split('@')
                .next()
                .ok_or_else(|| AosError::Database(format!("Invalid email format: {}", email)))?;

            sqlx::query(
                "INSERT INTO users (id, email, display_name, pw_hash, role, disabled, created_at)
                 VALUES (?, ?, ?, ?, ?, 0, datetime('now'))",
            )
            .bind(format!("{}-user", username))
            .bind(email)
            .bind(display_name)
            .bind(pwd_hash)
            .bind(role)
            .execute(&self.pool)
            .await?;
        }

        // Create sample nodes using stable schema helper
        let nodes = vec![
            ("m1-max-01.local", "http://127.0.0.1:8081"),
            ("m2-ultra-01.local", "http://127.0.0.1:8082"),
            ("m3-max-01.local", "http://127.0.0.1:8083"),
        ];

        for (hostname, agent_endpoint) in nodes {
            let _ = self.register_node(hostname, agent_endpoint).await;
        }

        tracing::info!("Development data seeded successfully");
        Ok(())
    }

    /// Get the underlying pool for custom queries
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

// Re-export sqlx types for convenience
pub use sqlx;
pub use sqlx::Row;

pub mod adapters;
pub mod artifacts;
pub mod audits;
pub mod migration_verify;
pub mod unified_access;
pub use audits::Audit;
pub mod code_policies;
pub mod commits;
pub mod contacts;
pub use contacts::{Contact, ContactStream};
pub mod cp_pointers;
pub mod enclave_operations;
pub use enclave_operations::{EnclaveOperation, OperationStats};
pub mod ephemeral_adapters;
pub mod git;
pub mod git_repositories;
pub use git_repositories::GitRepository;
pub mod incidents;
pub mod jobs;
pub use jobs::Job;
pub mod training_jobs;
pub use training_jobs::{TrainingJobRecord, TrainingProgress};
pub mod training_datasets;
pub use training_datasets::{DatasetFile, DatasetStatistics, TrainingDataset};
pub mod key_metadata;
pub use key_metadata::KeyMetadata;
pub mod manifests;
pub mod models;
pub mod nodes;
pub mod patch_proposals;
pub use patch_proposals::PatchProposal;
pub mod pinned_adapters;
pub mod plans;
pub mod policies;
pub mod policy_hash;
pub use policy_hash::PolicyHashRecord;
pub mod process_monitoring;
pub use process_monitoring::*;
pub mod rag_retrieval_audit;
pub mod replay_sessions;
pub mod repositories;
pub mod telemetry_bundles;
pub mod tenants;
pub mod users;
pub mod workers;

// Re-export unified access types
pub use unified_access::{
    ConnectionInfo, DatabaseAccess, DatabaseConfig, DatabaseStatistics, DatabaseType, HealthState,
    HealthStatus, SqlParameter, ToSql, Transaction, UnifiedDatabaseAccess, UnifiedTransaction,
};

// Re-export RAG audit types for API convenience
pub use rag_retrieval_audit::{RagRetrievalAuditRecord, RagRetrievalCount};
