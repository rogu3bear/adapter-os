use adapteros_core::AosError;
use anyhow::{Context, Result};
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

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
        let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");

        let migrator = Migrator::new(migrations_dir.as_path())
            .await
            .with_context(|| {
                format!(
                    "Failed to load migrations from {}",
                    migrations_dir.display()
                )
            })?;

        migrator.run(&self.pool).await.context("Migration failed")?;

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

/// Unified database enum for both SQLite and PostgreSQL
#[derive(Clone)]
pub enum Database {
    Sqlite(Db),
    Postgres(PostgresDb),
}

// Implement delegation methods for the Database enum
impl Database {
    /// Connect to a database using DATABASE_URL.
    ///
    /// If `DATABASE_URL` begins with `postgres://` or `postgresql://`, connects to PostgreSQL.
    /// Otherwise, uses SQLite (path or sqlite URL).
    pub async fn connect_env() -> Result<Database> {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "./var/cp.db".to_string());
        let url_lc = database_url.to_lowercase();
        if url_lc.starts_with("postgres://") || url_lc.starts_with("postgresql://") {
            let pg = PostgresDb::connect_env()
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            Ok(Database::Postgres(pg))
        } else {
            let sqlite = Db::connect(&database_url).await?;
            Ok(Database::Sqlite(sqlite))
        }
    }
    pub fn as_sqlite(&self) -> Option<&Db> {
        match self {
            Database::Sqlite(db) => Some(db),
            Database::Postgres(_) => None,
        }
    }

    pub fn as_postgres(&self) -> Option<&PostgresDb> {
        match self {
            Database::Sqlite(_) => None,
            Database::Postgres(db) => Some(db),
        }
    }

    /// Get the SQLite database, panicking if this is PostgreSQL
    /// Used for legacy code that requires direct Db access
    pub fn sqlite(&self) -> &Db {
        self.as_sqlite()
            .expect("Expected SQLite database but got PostgreSQL")
    }

    /// Get the PostgreSQL database, panicking if this is SQLite
    /// Used for legacy code that requires direct PostgresDb access
    pub fn postgres(&self) -> &PostgresDb {
        self.as_postgres()
            .expect("Expected PostgreSQL database but got SQLite")
    }

    pub fn pool(&self) -> &sqlx::SqlitePool {
        match self {
            Database::Sqlite(db) => db.pool(),
            Database::Postgres(_) => panic!("Cannot get SQLite pool from PostgreSQL database"),
        }
    }

    pub fn postgres_pool(&self) -> &sqlx::postgres::PgPool {
        match self {
            Database::Sqlite(_) => panic!("Cannot get PostgreSQL pool from SQLite database"),
            Database::Postgres(db) => db.pool(),
        }
    }

    /// Run database migrations for the active backend
    pub async fn migrate(&self) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.migrate().await,
            Database::Postgres(db) => db
                .migrate()
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string())),
        }
    }

    /// Seed development data for the active backend
    pub async fn seed_dev_data(&self) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.seed_dev_data().await,
            Database::Postgres(db) => db
                .seed_dev_data()
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string())),
        }
    }

    pub async fn list_adapters(&self) -> Result<Vec<adapters::Adapter>> {
        match self {
            Database::Sqlite(db) => db.list_adapters().await,
            Database::Postgres(db) => db.list_adapters().await.map_err(Into::into),
        }
    }

    pub async fn get_adapter(&self, adapter_id: &str) -> Result<Option<adapters::Adapter>> {
        match self {
            Database::Sqlite(db) => db.get_adapter(adapter_id).await,
            Database::Postgres(db) => db.get_adapter(adapter_id).await.map_err(Into::into),
        }
    }

    pub async fn register_adapter(
        &self,
        params: crate::AdapterRegistrationParams,
    ) -> Result<String> {
        match self {
            Database::Sqlite(db) => Db::register_adapter(db, params.clone()).await,
            Database::Postgres(db) => PostgresDb::register_adapter(db, params)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn delete_adapter(&self, id: &str) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.delete_adapter(id).await,
            Database::Postgres(db) => db.delete_adapter(id).await.map_err(Into::into),
        }
    }

    pub async fn update_adapter_state(
        &self,
        adapter_id: &str,
        state: &str,
        reason: &str,
    ) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.update_adapter_state(adapter_id, state, reason).await,
            Database::Postgres(db) => db
                .update_adapter_state(adapter_id, state, reason)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<users::User>> {
        match self {
            Database::Sqlite(db) => db.get_user_by_email(email).await,
            Database::Postgres(db) => db.get_user_by_email(email).await.map_err(Into::into),
        }
    }

    pub async fn list_tenants(&self) -> Result<Vec<tenants::Tenant>> {
        match self {
            Database::Sqlite(db) => db.list_tenants().await,
            Database::Postgres(db) => db.list_tenants().await.map_err(Into::into),
        }
    }

    pub async fn create_tenant(&self, name: &str, itar_flag: bool) -> Result<String> {
        match self {
            Database::Sqlite(db) => db.create_tenant(name, itar_flag).await,
            Database::Postgres(db) => db.create_tenant(name, itar_flag).await.map_err(Into::into),
        }
    }

    pub async fn get_tenant(&self, id: &str) -> Result<Option<tenants::Tenant>> {
        match self {
            Database::Sqlite(db) => db.get_tenant(id).await,
            Database::Postgres(db) => db.get_tenant(id).await.map_err(Into::into),
        }
    }

    pub async fn list_all_workers(&self) -> Result<Vec<models::Worker>> {
        match self {
            Database::Sqlite(db) => db.list_all_workers().await,
            Database::Postgres(db) => db.list_all_workers().await.map_err(Into::into),
        }
    }

    pub async fn list_workers_by_node(&self, node_id: &str) -> Result<Vec<models::Worker>> {
        match self {
            Database::Sqlite(db) => db.list_workers_by_node(node_id).await,
            Database::Postgres(db) => db.list_workers_by_node(node_id).await.map_err(Into::into),
        }
    }

    pub async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.update_worker_status(worker_id, status).await,
            Database::Postgres(db) => db
                .update_worker_status(worker_id, status)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn list_nodes(&self) -> Result<Vec<nodes::Node>> {
        match self {
            Database::Sqlite(db) => db.list_nodes().await,
            Database::Postgres(db) => db.list_nodes().await.map_err(Into::into),
        }
    }

    pub async fn register_node(&self, hostname: &str, agent_endpoint: &str) -> Result<String> {
        match self {
            Database::Sqlite(db) => db.register_node(hostname, agent_endpoint).await,
            Database::Postgres(db) => db
                .register_node(hostname, agent_endpoint)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<nodes::Node>> {
        match self {
            Database::Sqlite(db) => db.get_node(id).await,
            Database::Postgres(db) => db.get_node(id).await.map_err(Into::into),
        }
    }

    pub async fn update_node_status(&self, id: &str, status: &str) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.update_node_status(id, status).await,
            Database::Postgres(db) => db.update_node_status(id, status).await.map_err(Into::into),
        }
    }

    pub async fn create_job(
        &self,
        kind: &str,
        tenant_id: Option<&str>,
        user_id: Option<&str>,
        payload_json: &str,
    ) -> Result<String> {
        match self {
            Database::Sqlite(db) => db.create_job(kind, tenant_id, user_id, payload_json).await,
            Database::Postgres(db) => db
                .create_job(kind, tenant_id, user_id, payload_json)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn list_replay_sessions(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<Vec<replay_sessions::ReplaySession>> {
        match self {
            Database::Sqlite(db) => db.list_replay_sessions(tenant_id).await,
            Database::Postgres(db) => db.list_replay_sessions(tenant_id).await.map_err(Into::into),
        }
    }

    pub async fn get_replay_session(
        &self,
        session_id: &str,
    ) -> Result<Option<replay_sessions::ReplaySession>> {
        match self {
            Database::Sqlite(db) => db.get_replay_session(session_id).await,
            Database::Postgres(db) => db.get_replay_session(session_id).await.map_err(Into::into),
        }
    }

    pub async fn create_replay_session(
        &self,
        session: &replay_sessions::ReplaySession,
    ) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.create_replay_session(session).await,
            Database::Postgres(db) => db.create_replay_session(session).await.map_err(Into::into),
        }
    }

    pub async fn get_training_job(
        &self,
        job_id: &str,
    ) -> Result<Option<training_jobs::TrainingJobRecord>> {
        match self {
            Database::Sqlite(db) => db.get_training_job(job_id).await,
            Database::Postgres(db) => db.get_training_job(job_id).await.map_err(Into::into),
        }
    }

    pub async fn update_training_status(&self, job_id: &str, status: &str) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.update_training_status(job_id, status).await,
            Database::Postgres(db) => db
                .update_training_status(job_id, status)
                .await
                .map_err(Into::into),
        }
    }

    // Add missing methods as needed
    pub async fn rename_tenant(&self, tenant_id: &str, new_name: &str) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.rename_tenant(tenant_id, new_name).await,
            Database::Postgres(db) => db
                .rename_tenant(tenant_id, new_name)
                .await
                .map_err(Into::into),
        }
    }

    // Git repository methods
    pub async fn create_git_repository(
        &self,
        _id: &str,
        repo_id: &str,
        path: &str,
        branch: &str,
        analysis_json: &str,
        created_by: &str,
    ) -> Result<String> {
        match self {
            Database::Sqlite(db) => {
                db.create_git_repository(_id, repo_id, path, branch, analysis_json, created_by)
                    .await
            }
            Database::Postgres(db) => db
                .create_git_repository(_id, repo_id, path, branch, analysis_json, created_by)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn get_git_repository(
        &self,
        repo_id: &str,
    ) -> Result<Option<git_repositories::GitRepository>> {
        match self {
            Database::Sqlite(db) => db.get_git_repository(repo_id).await,
            Database::Postgres(db) => db.get_git_repository(repo_id).await.map_err(Into::into),
        }
    }

    pub async fn list_git_repositories(&self) -> Result<Vec<git_repositories::GitRepository>> {
        match self {
            Database::Sqlite(db) => db.list_git_repositories().await,
            Database::Postgres(db) => db.list_git_repositories().await.map_err(Into::into),
        }
    }

    // Training job methods
    pub async fn create_training_job(
        &self,
        repo_id: &str,
        training_config_json: &str,
        created_by: &str,
    ) -> Result<String> {
        match self {
            Database::Sqlite(db) => {
                db.create_training_job(repo_id, training_config_json, created_by)
                    .await
            }
            Database::Postgres(db) => db
                .create_training_job(repo_id, training_config_json, created_by)
                .await
                .map_err(Into::into),
        }
    }

    // Model methods
    pub async fn register_model(
        &self,
        params: crate::models::ModelRegistrationParams,
    ) -> Result<String> {
        match self {
            Database::Sqlite(db) => db.register_model(params.clone()).await,
            Database::Postgres(db) => db.register_model(params).await.map_err(Into::into),
        }
    }

    pub async fn get_model(&self, id: &str) -> Result<Option<models::Model>> {
        match self {
            Database::Sqlite(db) => db.get_model(id).await,
            Database::Postgres(db) => db.get_model(id).await.map_err(Into::into),
        }
    }

    pub async fn get_base_model_status(
        &self,
        tenant_id: &str,
    ) -> Result<Option<models::BaseModelStatus>> {
        match self {
            Database::Sqlite(db) => db.get_base_model_status(tenant_id).await,
            Database::Postgres(db) => db
                .get_base_model_status(tenant_id)
                .await
                .map_err(Into::into),
        }
    }

    // Job methods
    pub async fn list_jobs(&self, tenant_id: Option<&str>) -> Result<Vec<jobs::Job>> {
        match self {
            Database::Sqlite(db) => db.list_jobs(tenant_id).await,
            Database::Postgres(db) => db.list_jobs(tenant_id).await.map_err(Into::into),
        }
    }

    // Plan methods
    pub async fn get_plan(&self, id: &str) -> Result<Option<models::Plan>> {
        match self {
            Database::Sqlite(db) => db.get_plan(id).await,
            Database::Postgres(db) => db.get_plan(id).await.map_err(Into::into),
        }
    }

    // Audit methods
    pub async fn list_all_audits(&self) -> Result<Vec<audits::Audit>> {
        match self {
            Database::Sqlite(db) => db.list_all_audits().await,
            Database::Postgres(db) => db.list_all_audits().await.map_err(Into::into),
        }
    }

    // CP Pointer methods
    pub async fn get_active_cp_pointer(
        &self,
        tenant_id: &str,
    ) -> Result<Option<models::CpPointer>> {
        match self {
            Database::Sqlite(db) => db.get_active_cp_pointer(tenant_id).await,
            Database::Postgres(db) => db
                .get_active_cp_pointer(tenant_id)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn get_cp_pointer_by_name(&self, name: &str) -> Result<Option<models::CpPointer>> {
        match self {
            Database::Sqlite(db) => db.get_cp_pointer_by_name(name).await,
            Database::Postgres(db) => db.get_cp_pointer_by_name(name).await.map_err(Into::into),
        }
    }

    // Worker methods
    pub async fn list_workers_by_tenant(&self, tenant_id: &str) -> Result<Vec<models::Worker>> {
        match self {
            Database::Sqlite(db) => db.list_workers_by_tenant(tenant_id).await,
            Database::Postgres(db) => db
                .list_workers_by_tenant(tenant_id)
                .await
                .map_err(Into::into),
        }
    }

    // Additional worker methods
    pub async fn insert_worker(&self, params: crate::workers::WorkerInsertParams) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.insert_worker(params).await,
            Database::Postgres(db) => db
                .insert_worker(params)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn update_worker_heartbeat(&self, id: &str, status: Option<&str>) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.update_worker_heartbeat(id, status).await,
            Database::Postgres(db) => db
                .update_worker_heartbeat(id, status)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    // Plan methods
    pub async fn list_plans_by_tenant(&self, tenant_id: &str) -> Result<Vec<models::Plan>> {
        match self {
            Database::Sqlite(db) => db.list_plans_by_tenant(tenant_id).await,
            Database::Postgres(db) => db
                .list_plans_by_tenant(tenant_id)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn list_all_plans(&self) -> Result<Vec<models::Plan>> {
        match self {
            Database::Sqlite(db) => db.list_all_plans().await,
            Database::Postgres(db) => db
                .list_all_plans()
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    // CP Pointer methods
    pub async fn deactivate_all_cp_pointers(&self, tenant_id: &str) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.deactivate_all_cp_pointers(tenant_id).await,
            Database::Postgres(db) => db
                .deactivate_all_cp_pointers(tenant_id)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn insert_cp_pointer(
        &self,
        id: &str,
        tenant_id: &str,
        name: &str,
        adapter_id: &str,
        active: bool,
    ) -> Result<()> {
        match self {
            Database::Sqlite(db) => {
                db.insert_cp_pointer(id, tenant_id, name, adapter_id, active)
                    .await
            }
            Database::Postgres(db) => db
                .insert_cp_pointer(id, tenant_id, name, adapter_id, active)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn list_cp_pointers_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<models::CpPointer>> {
        match self {
            Database::Sqlite(db) => db.list_cp_pointers_by_tenant(tenant_id).await,
            Database::Postgres(db) => db
                .list_cp_pointers_by_tenant(tenant_id)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn activate_cp_pointer(&self, id: &str) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.activate_cp_pointer(id).await,
            Database::Postgres(db) => db
                .activate_cp_pointer(id)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    // Adapter stats methods
    pub async fn get_adapter_stats(&self, adapter_id: &str) -> Result<(i64, i64, f64)> {
        match self {
            Database::Sqlite(db) => db.get_adapter_stats(adapter_id).await,
            Database::Postgres(db) => db
                .get_adapter_stats(adapter_id)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn update_adapter_memory(&self, adapter_id: &str, memory_bytes: i64) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.update_adapter_memory(adapter_id, memory_bytes).await,
            Database::Postgres(db) => db
                .update_adapter_memory(adapter_id, memory_bytes)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn get_adapter_activations(
        &self,
        adapter_id: &str,
        limit: i64,
    ) -> Result<Vec<adapters::AdapterActivation>> {
        match self {
            Database::Sqlite(db) => db.get_adapter_activations(adapter_id, limit).await,
            Database::Postgres(db) => db
                .get_adapter_activations(adapter_id, limit)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    // Repository methods
    pub async fn list_repositories(
        &self,
        tenant_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<repositories::Repository>> {
        match self {
            Database::Sqlite(db) => db.list_repositories(tenant_id, limit, offset).await,
            Database::Postgres(db) => db
                .list_repositories(tenant_id, limit, offset)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    // System monitoring methods
    pub async fn list_monitoring_rules(
        &self,
        tenant_id: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Vec<process_monitoring::ProcessMonitoringRule>> {
        match self {
            Database::Sqlite(db) => {
                process_monitoring::ProcessMonitoringRule::list(&db.pool, tenant_id, is_active)
                    .await
                    .map_err(|e| anyhow::anyhow!("SQLite error: {}", e))
            }
            Database::Postgres(db) => db
                .list_monitoring_rules(tenant_id, is_active)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn list_process_alerts(
        &self,
        filters: process_monitoring::AlertFilters,
    ) -> Result<Vec<process_monitoring::ProcessAlert>> {
        match self {
            Database::Sqlite(db) => process_monitoring::ProcessAlert::list(&db.pool, filters)
                .await
                .map_err(|e| anyhow::anyhow!("SQLite error: {}", e)),
            Database::Postgres(db) => db
                .list_process_alerts(filters)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn get_process_alert(
        &self,
        id: &str,
    ) -> Result<Option<process_monitoring::ProcessAlert>> {
        match self {
            Database::Sqlite(db) => process_monitoring::ProcessAlert::list(
                &db.pool,
                process_monitoring::AlertFilters {
                    tenant_id: None,
                    worker_id: None,
                    status: None,
                    severity: None,
                    start_time: None,
                    end_time: None,
                    limit: Some(500),
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("SQLite error: {}", e))
            .map(|alerts| alerts.into_iter().find(|a| a.id == id)),
            Database::Postgres(db) => db
                .get_process_alert(id)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn update_process_alert_status(
        &self,
        id: &str,
        status: process_monitoring::AlertStatus,
        user: Option<&str>,
    ) -> Result<()> {
        match self {
            Database::Sqlite(db) => {
                process_monitoring::ProcessAlert::update_status(&db.pool, id, status, user)
                    .await
                    .map_err(|e| anyhow::anyhow!("SQLite error: {}", e))
            }
            Database::Postgres(db) => db
                .update_process_alert_status(id, status, user)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn list_process_anomalies(
        &self,
        filters: process_monitoring::AnomalyFilters,
    ) -> Result<Vec<process_monitoring::ProcessAnomaly>> {
        match self {
            Database::Sqlite(db) => process_monitoring::ProcessAnomaly::list(&db.pool, filters)
                .await
                .map_err(|e| anyhow::anyhow!("SQLite error: {}", e)),
            Database::Postgres(db) => db
                .list_process_anomalies(filters)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn get_process_anomaly(
        &self,
        id: &str,
    ) -> Result<Option<process_monitoring::ProcessAnomaly>> {
        match self {
            Database::Sqlite(db) => process_monitoring::ProcessAnomaly::list(
                &db.pool,
                process_monitoring::AnomalyFilters {
                    tenant_id: None,
                    worker_id: None,
                    status: None,
                    anomaly_type: None,
                    start_time: None,
                    end_time: None,
                    limit: Some(500),
                },
            )
            .await
            .map_err(|e| anyhow::anyhow!("SQLite error: {}", e))
            .map(|anomalies| anomalies.into_iter().find(|a| a.id == id)),
            Database::Postgres(db) => db
                .get_process_anomaly(id)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn update_process_anomaly_status(
        &self,
        id: &str,
        status: process_monitoring::AnomalyStatus,
        investigated_by: Option<&str>,
        notes: Option<&str>,
    ) -> Result<()> {
        match self {
            Database::Sqlite(db) => process_monitoring::ProcessAnomaly::update_status(
                &db.pool,
                id,
                status,
                investigated_by,
                notes,
            )
            .await
            .map_err(|e| anyhow::anyhow!("SQLite error: {}", e)),
            Database::Postgres(db) => db
                .update_process_anomaly_status(id, status, investigated_by, notes)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn create_monitoring_dashboard(
        &self,
        req: process_monitoring::CreateDashboardRequest,
    ) -> Result<String> {
        match self {
            Database::Sqlite(db) => {
                process_monitoring::MonitoringDashboard::create(&db.pool, req.clone())
                    .await
                    .map_err(|e| anyhow::anyhow!("SQLite error: {}", e))
            }
            Database::Postgres(db) => db
                .create_monitoring_dashboard(req)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn list_monitoring_dashboards(
        &self,
        tenant_id: Option<&str>,
        is_shared: Option<bool>,
    ) -> Result<Vec<process_monitoring::MonitoringDashboard>> {
        match self {
            Database::Sqlite(db) => {
                process_monitoring::MonitoringDashboard::list(&db.pool, tenant_id, is_shared)
                    .await
                    .map_err(|e| anyhow::anyhow!("SQLite error: {}", e))
            }
            Database::Postgres(db) => db
                .list_monitoring_dashboards(tenant_id, is_shared)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn create_monitoring_report(
        &self,
        req: process_monitoring::CreateReportRequest,
    ) -> Result<String> {
        match self {
            Database::Sqlite(db) => {
                process_monitoring::ProcessMonitoringReport::create(&db.pool, req.clone())
                    .await
                    .map_err(|e| anyhow::anyhow!("SQLite error: {}", e))
            }
            Database::Postgres(db) => db
                .create_monitoring_report(req)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn list_monitoring_reports(
        &self,
        tenant_id: Option<&str>,
        report_type: Option<&str>,
    ) -> Result<Vec<process_monitoring::ProcessMonitoringReport>> {
        match self {
            Database::Sqlite(db) => {
                process_monitoring::ProcessMonitoringReport::list(&db.pool, tenant_id, report_type)
                    .await
                    .map_err(|e| anyhow::anyhow!("SQLite error: {}", e))
            }
            Database::Postgres(db) => db
                .list_monitoring_reports(tenant_id, report_type)
                .await
                .map_err(|e| anyhow::anyhow!("PostgreSQL error: {}", e)),
        }
    }

    pub async fn pin_adapter(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        pinned_until: Option<&str>,
        reason: &str,
        pinned_by: Option<&str>,
    ) -> Result<String> {
        match self {
            Database::Sqlite(db) => {
                db.pin_adapter(tenant_id, adapter_id, pinned_until, reason, pinned_by)
                    .await
            }
            Database::Postgres(db) => db
                .pin_adapter(tenant_id, adapter_id, pinned_until, reason, pinned_by)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn unpin_adapter(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        match self {
            Database::Sqlite(db) => db.unpin_adapter(tenant_id, adapter_id).await,
            Database::Postgres(db) => db
                .unpin_adapter(tenant_id, adapter_id)
                .await
                .map_err(Into::into),
        }
    }

    pub async fn list_pinned_adapters(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<pinned_adapters::PinnedAdapter>> {
        match self {
            Database::Sqlite(db) => db.list_pinned_adapters(tenant_id).await,
            Database::Postgres(db) => db.list_pinned_adapters(tenant_id).await.map_err(Into::into),
        }
    }

    pub async fn count_enclave_operations(&self) -> Result<i64> {
        match self {
            Database::Sqlite(db) => db.count_enclave_operations().await,
            Database::Postgres(db) => db.count_enclave_operations().await.map_err(Into::into),
        }
    }

    pub async fn get_policies(&self, tenant_id: &str) -> Result<crate::policies::TenantPolicies> {
        match self {
            Database::Sqlite(db) => db.get_policies(tenant_id).await,
            Database::Postgres(db) => db.get_policies(tenant_id).await.map_err(Into::into),
        }
    }
}

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
pub mod domain_adapters;
pub mod enclave_operations;
pub use enclave_operations::{EnclaveOperation, OperationStats};
pub mod ephemeral_adapters;
pub mod git;
pub mod git_repositories;
pub use git_repositories::GitRepository;
pub mod incidents;
pub mod jobs;
pub mod training_jobs;
pub use training_jobs::TrainingProgress;
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
