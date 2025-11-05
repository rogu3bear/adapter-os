use adapteros_core::AosError;
use anyhow::{Context, Result};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{PgPool, SqlitePool};
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

// PostgreSQL backend for production
pub mod postgres;

pub use postgres::PostgresDb;

// Database modules
pub mod activity_events;
pub mod adapters;
pub mod artifacts;
pub mod audits;
pub mod code_policies;
pub mod commits;
pub mod contacts;
pub mod cp_pointers;
pub mod domain_adapters;
pub mod enclave_operations;
pub mod ephemeral_adapters;
pub mod git;
pub mod git_repositories;
pub mod incidents;
pub mod jobs;
pub mod key_metadata;
pub mod manifests;
pub mod messages;
pub mod migration_verify;
pub mod model_operations;
pub mod models;
pub mod nodes;
pub mod notifications;
pub mod patch_proposals;
pub mod pinned_adapters;
pub mod plans;
pub mod policies;
pub mod policy_hash;
pub mod process_monitoring;
pub mod rag_retrieval_audit;
pub mod replay_sessions;
pub mod repositories;
pub mod telemetry_bundles;
pub mod tenants;
pub mod training_datasets;
pub mod training_jobs;
pub mod tutorials;
pub mod unified_access;
pub mod users;
pub mod workers;
pub mod workspaces;

/// Database connection pool and query methods (SQLite).
///
/// A PostgreSQL backend can be reintroduced behind a feature flag without
/// changing callers that depend on `Database`.
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
        let url = if path.starts_with("sqlite://") || path.starts_with("sqlite::") {
            // Already in URL form (absolute, relative, or special memory URL)
            path.to_string()
        } else if path.starts_with('/') {
            // Absolute filesystem path
            format!("sqlite://{}", path)
        } else {
            // Relative filesystem path
            format!("sqlite://{}", path)
        };

        // Establish connection via URL (creates DB file if missing)
        let connect_opts = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .immutable(false);
        let pool = SqlitePool::connect_with(connect_opts).await?;

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
        // Use embedded migrations from the crate's migrations directory
        const MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

        tracing::info!("Running database migrations with embedded migrations");

        match MIGRATIONS.run(&self.pool).await {
            Ok(_) => {
                tracing::info!("Database migrations applied successfully");
            }
            Err(e) => {
                // In development, log warning but continue if it's just a checksum/version mismatch
                // sqlx returns MigrateError::VersionMismatch for checksum mismatches
                let err_str = format!("{:?}", e);
                if err_str.contains("VersionMismatch") || err_str.contains("checksum") {
                    tracing::warn!("Migration checksum mismatch detected (dev mode): {}", e);
                    tracing::warn!(
                        "Continuing despite checksum mismatch - this is acceptable in development"
                    );
                    tracing::info!("Database migrations check completed (with warnings)");
                } else {
                    tracing::error!("Migration error details: {:?}", e);
                    return Err(e).with_context(|| "Database migration failed".to_string());
                }
            }
        }

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
            ("admin@aos.local", "Admin", "admin", &password_hash),
            ("operator@aos.local", "Operator", "operator", &password_hash),
            ("sre@aos.local", "SRE", "sre", &password_hash),
            ("viewer@aos.local", "Viewer", "viewer", &password_hash),
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

    /// List monitoring reports with optional filters
    pub async fn list_monitoring_reports(
        &self,
        tenant_id: Option<&str>,
        report_type: Option<&str>,
    ) -> Result<Vec<process_monitoring::ProcessMonitoringReport>> {
        process_monitoring::ProcessMonitoringReport::list(self.pool(), tenant_id, report_type)
            .await
            .map_err(anyhow::Error::from)
    }

    /// Create a new monitoring report
    pub async fn create_monitoring_report(
        &self,
        req: process_monitoring::CreateReportRequest,
    ) -> Result<String> {
        process_monitoring::ProcessMonitoringReport::create(self.pool(), req)
            .await
            .map_err(anyhow::Error::from)
    }
}

// Re-export sqlx types for convenience
pub use sqlx;
pub use sqlx::Row;

// Re-export types for trait usage
pub use adapters::{Adapter, AdapterRegistrationBuilder, AdapterRegistrationParams};
pub use commits::{Commit, CommitBuilder, CommitParams};
pub use contacts::{ContactUpsertBuilder, ContactUpsertParams};
pub use jobs::Job;
pub use models::{ModelRegistrationBuilder, ModelRegistrationParams, Worker};
pub use nodes::Node;
pub use patch_proposals::{PatchProposalBuilder, PatchProposalParams};
pub use replay_sessions::ReplaySession;
pub use repositories::{
    CodeGraphMetadataBuilder, CodeGraphMetadataParams, RepositoryExtendedBuilder,
    RepositoryExtendedParams,
};
pub use telemetry_bundles::{TelemetryBatchBuilder, TelemetryBatchParams, TelemetryRecord};
pub use tenants::Tenant;
pub use training_jobs::{TrainingJobBuilder, TrainingJobParams, TrainingJobRecord};
pub use workers::{WorkerInsertBuilder, WorkerInsertParams};

/// Database backend enum supporting both SQLite and PostgreSQL
#[derive(Clone)]
pub enum DatabaseBackend {
    Sqlite(Db),
    Postgres(PostgresDb),
}

// Make DatabaseBackend accessible for pattern matching
impl Database {
    /// Access the backend enum directly (for pattern matching)
    pub fn backend(&self) -> &DatabaseBackend {
        &self.0
    }
}

/// Thin wrapper over the primary database backend.
///
/// Supports both SQLite and PostgreSQL backends. Automatically selects
/// PostgreSQL when DATABASE_URL contains "postgresql://", otherwise uses SQLite.
#[derive(Clone)]
pub struct Database(DatabaseBackend);

impl Database {
    /// Wrap an existing SQLite database handle.
    pub fn new(db: Db) -> Self {
        Self(DatabaseBackend::Sqlite(db))
    }

    /// Wrap an existing PostgreSQL database handle.
    pub fn new_postgres(db: PostgresDb) -> Self {
        Self(DatabaseBackend::Postgres(db))
    }

    /// Borrow the inner SQLite handle (panics if PostgreSQL).
    pub fn inner(&self) -> &Db {
        match &self.0 {
            DatabaseBackend::Sqlite(db) => db,
            DatabaseBackend::Postgres(_) => {
                panic!("Database is PostgreSQL, not SQLite")
            }
        }
    }

    /// Consume the wrapper and return the inner SQLite handle (panics if PostgreSQL).
    pub fn into_inner(self) -> Db {
        match self.0 {
            DatabaseBackend::Sqlite(db) => db,
            DatabaseBackend::Postgres(_) => {
                panic!("Database is PostgreSQL, not SQLite")
            }
        }
    }

    /// Connect using an explicit database URL or path.
    ///
    /// Detects PostgreSQL URLs (postgresql://) and uses PostgresDb,
    /// otherwise uses SQLite.
    pub async fn connect(path: &str) -> Result<Self> {
        if path.starts_with("postgresql://") || path.starts_with("postgres://") {
            Ok(Self(DatabaseBackend::Postgres(
                PostgresDb::connect(path).await?,
            )))
        } else {
            Ok(Self(DatabaseBackend::Sqlite(Db::connect(path).await?)))
        }
    }

    /// Connect to a database using `DATABASE_URL`.
    ///
    /// Automatically detects PostgreSQL URLs and uses PostgresDb.
    /// Falls back to SQLite if DATABASE_URL is not set or not a PostgreSQL URL.
    pub async fn connect_env() -> Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "./var/cp.db".to_string());

        if database_url.starts_with("postgresql://") || database_url.starts_with("postgres://") {
            tracing::info!("Using PostgreSQL backend");
            Ok(Self(DatabaseBackend::Postgres(
                PostgresDb::connect_env().await?,
            )))
        } else {
            tracing::info!("Using SQLite backend");
            Ok(Self(DatabaseBackend::Sqlite(Db::connect_env().await?)))
        }
    }

    /// Access the underlying SQLite pool (panics if PostgreSQL).
    ///
    /// For PostgreSQL, use methods directly on Database or match on backend.
    pub fn pool(&self) -> &SqlitePool {
        match &self.0 {
            DatabaseBackend::Sqlite(db) => db.pool(),
            DatabaseBackend::Postgres(_) => {
                panic!("Cannot get SqlitePool from PostgreSQL database. Use PostgresDb methods directly.")
            }
        }
    }

    /// Access the underlying PostgreSQL pool (panics if SQLite).
    pub fn pool_postgres(&self) -> &PgPool {
        match &self.0 {
            DatabaseBackend::Postgres(db) => db.pool(),
            DatabaseBackend::Sqlite(_) => {
                panic!("Cannot get PgPool from SQLite database. Use Db methods directly.")
            }
        }
    }

    /// Run database migrations.
    ///
    /// Automatically uses the correct migration directory based on backend.
    pub async fn migrate(&self) -> Result<()> {
        match &self.0 {
            DatabaseBackend::Sqlite(db) => db.migrate().await,
            DatabaseBackend::Postgres(db) => db.migrate().await.map_err(|e| anyhow::Error::from(e)),
        }
    }

    /// Seed the database with development data.
    pub async fn seed_dev_data(&self) -> Result<()> {
        match &self.0 {
            DatabaseBackend::Sqlite(db) => db.seed_dev_data().await,
            DatabaseBackend::Postgres(db) => {
                db.seed_dev_data().await.map_err(|e| anyhow::Error::from(e))
            }
        }
    }

    /// Insert a policy hash baseline (delegates to backend)
    pub async fn insert_policy_hash(
        &self,
        policy_pack_id: &str,
        baseline_hash: &adapteros_core::B3Hash,
        cpid: Option<&str>,
        signer_pubkey: Option<&str>,
    ) -> Result<()> {
        match &self.0 {
            DatabaseBackend::Sqlite(db) => db
                .insert_policy_hash(policy_pack_id, baseline_hash, cpid, signer_pubkey)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to insert policy hash: {}", e)),
            DatabaseBackend::Postgres(db) => db
                .insert_policy_hash(policy_pack_id, baseline_hash, cpid, signer_pubkey)
                .await
                .map_err(|e| anyhow::Error::from(e)),
        }
    }

    /// Get a policy hash record (delegates to backend)
    pub async fn get_policy_hash(
        &self,
        policy_pack_id: &str,
        cpid: Option<&str>,
    ) -> Result<Option<policy_hash::PolicyHashRecord>> {
        match &self.0 {
            DatabaseBackend::Sqlite(db) => db
                .get_policy_hash(policy_pack_id, cpid)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to get policy hash: {}", e)),
            DatabaseBackend::Postgres(db) => db
                .get_policy_hash(policy_pack_id, cpid)
                .await
                .map_err(|e| anyhow::Error::from(e)),
        }
    }

    /// List policy hashes (delegates to backend)
    pub async fn list_policy_hashes(
        &self,
        cpid: Option<&str>,
    ) -> Result<Vec<policy_hash::PolicyHashRecord>> {
        match &self.0 {
            DatabaseBackend::Sqlite(db) => db
                .list_policy_hashes(cpid)
                .await
                .map_err(|e| anyhow::anyhow!("Failed to list policy hashes: {}", e)),
            DatabaseBackend::Postgres(db) => db
                .list_policy_hashes(cpid)
                .await
                .map_err(|e| anyhow::Error::from(e)),
        }
    }
}

impl From<Db> for Database {
    fn from(db: Db) -> Self {
        Self::new(db)
    }
}

impl From<PostgresDb> for Database {
    fn from(db: PostgresDb) -> Self {
        Self::new_postgres(db)
    }
}

impl Deref for Database {
    type Target = Db;

    fn deref(&self) -> &Self::Target {
        match &self.0 {
            DatabaseBackend::Sqlite(db) => db,
            DatabaseBackend::Postgres(_) => {
                panic!("Cannot deref PostgreSQL Database to Db. Use Database methods or match on backend.")
            }
        }
    }
}

impl DerefMut for Database {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match &mut self.0 {
            DatabaseBackend::Sqlite(db) => db,
            DatabaseBackend::Postgres(_) => {
                panic!("Cannot deref_mut PostgreSQL Database to Db. Use Database methods or match on backend.")
            }
        }
    }
}

impl AsRef<Db> for Database {
    fn as_ref(&self) -> &Db {
        self.inner()
    }
}

impl std::borrow::Borrow<Db> for Database {
    fn borrow(&self) -> &Db {
        self.inner()
    }
}
