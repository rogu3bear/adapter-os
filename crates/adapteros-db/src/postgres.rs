//! PostgreSQL connection pool and query methods
//!
//! Production backend for AdapterOS registry and state management.
//! Replaces SQLite for multi-node deployments with pgvector support.

use crate::{
    models::ModelRegistrationParams,
    process_monitoring::{
        AlertFilters, AlertSeverity, AlertStatus, AnomalyFilters, AnomalyStatus,
        CreateDashboardRequest, CreateReportRequest, MonitoringDashboard, ProcessAlert,
        ProcessAnomaly, ProcessMonitoringReport, ProcessMonitoringRule, RuleType,
        ThresholdOperator,
    },
    AdapterRegistrationParams,
};
use adapteros_core::{AosError, Result};
use chrono;
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx::Row;
use std::time::Duration;
use uuid;

/// Database connection pool for PostgreSQL
#[derive(Clone)]
pub struct PostgresDb {
    pool: PgPool,
}

impl PostgresDb {
    /// Connect to PostgreSQL database with connection pooling
    ///
    /// # Connection String Format
    /// `postgresql://user:password@host:port/database`
    ///
    /// # Example
    /// ```no_run
    /// use adapteros_db::postgres::PostgresDb;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let db = PostgresDb::connect("postgresql://aos:aos@localhost/adapteros").await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(20) // Connection pool size
            .min_connections(2) // Minimum idle connections
            .acquire_timeout(Duration::from_secs(5))
            .idle_timeout(Duration::from_secs(300))
            .max_lifetime(Duration::from_secs(1800))
            .connect(database_url)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to PostgreSQL: {}", e)))?;

        tracing::info!("Connected to PostgreSQL database");
        Ok(Self { pool })
    }

    /// Connect using DATABASE_URL environment variable
    ///
    /// Falls back to local PostgreSQL if not set.
    pub async fn connect_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://aos:aos@localhost/adapteros".to_string());

        tracing::info!(
            "Connecting to PostgreSQL using: {}",
            database_url.split('@').next_back().unwrap_or("unknown")
        );

        Self::connect(&database_url).await
    }

    /// Run database migrations
    ///
    /// Applies all SQL migrations from the `migrations_postgres/` directory.
    /// Migrations are idempotent and can be run multiple times safely.
    pub async fn migrate(&self) -> Result<()> {
        use std::path::Path;
        let migrations_dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations_postgres");

        let migrator = sqlx::migrate::Migrator::new(migrations_dir.as_path())
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to load migrations from {}: {}",
                    migrations_dir.display(),
                    e
                ))
            })?;

        migrator
            .run(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Migration failed: {}", e)))?;

        tracing::info!("Database migrations completed successfully");
        Ok(())
    }

    /// Health check - verify database connectivity
    ///
    /// Returns `Ok(())` if database is reachable and responsive.
    pub async fn health_check(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Health check failed: {}", e)))?;

        Ok(())
    }

    /// Get connection pool statistics
    ///
    /// Returns current pool size and idle connections.
    pub fn pool_stats(&self) -> (u32, u32) {
        (self.pool.size(), self.pool.num_idle() as u32)
    }

    /// Seed database with development data
    ///
    /// **WARNING:** Only use in development environments.
    /// Creates sample users, nodes, and tenants for testing.
    pub async fn seed_dev_data(&self) -> Result<()> {
        use argon2::{
            password_hash::{PasswordHasher, SaltString},
            Argon2,
        };
        use rand::rngs::OsRng;

        tracing::info!("Seeding development data...");

        // Create default tenant
        sqlx::query(
            "INSERT INTO tenants (id, name, org_id, isolation_mode, max_memory_gb, status, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, NOW())
             ON CONFLICT (id) DO NOTHING"
        )
        .bind("default")
        .bind("Default Tenant")
        .bind("org-001")
        .bind("process")
        .bind(64)
        .bind("active")
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;

        // Create development users
        let argon2 = Argon2::default();
        let salt = SaltString::generate(&mut OsRng);
        let dev_password_hash = argon2
            .hash_password(b"aos123", &salt)
            .map_err(|e| AosError::Database(format!("Password hashing failed: {}", e)))?
            .to_string();

        let users = vec![
            (
                "admin@adapteros.dev",
                "Admin User",
                "admin",
                &dev_password_hash,
            ),
            (
                "operator@adapteros.dev",
                "Operator User",
                "operator",
                &dev_password_hash,
            ),
            ("sre@adapteros.dev", "SRE User", "sre", &dev_password_hash),
        ];

        for (email, display_name, role, pwd_hash) in users {
            let username = email.split('@').next().unwrap_or("unknown");

            sqlx::query(
                "INSERT INTO users (id, email, display_name, pw_hash, role, disabled, created_at)
                 VALUES ($1, $2, $3, $4, $5, false, NOW())
                 ON CONFLICT (id) DO NOTHING",
            )
            .bind(format!("{}-user", username))
            .bind(email)
            .bind(display_name)
            .bind(pwd_hash)
            .bind(role)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create user {}: {}", email, e)))?;
        }

        // Create sample nodes
        let nodes = vec![
            ("node-01", "m1-max-01.local", "M1 Max", 64),
            ("node-02", "m2-ultra-01.local", "M2 Ultra", 128),
            ("node-03", "m3-max-01.local", "M3 Max", 96),
        ];

        for (id, hostname, family, memory) in nodes {
            sqlx::query(
                "INSERT INTO nodes (id, tenant_id, hostname, metal_family, memory_gb, status, last_heartbeat)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW())
                 ON CONFLICT (id) DO NOTHING"
            )
            .bind(id)
            .bind("default")
            .bind(hostname)
            .bind(family)
            .bind(memory)
            .bind("online")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create node {}: {}", id, e)))?;
        }

        tracing::info!("Development data seeded successfully");
        Ok(())
    }

    /// Get the underlying pool for custom queries
    ///
    /// Use this for complex queries not covered by the API methods.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Close the database connection pool gracefully
    ///
    /// Waits for active connections to complete before closing.
    pub async fn close(self) {
        self.pool.close().await;
        tracing::info!("PostgreSQL connection pool closed");
    }
}

// Sub-modules for specific data models
pub mod adapters;
pub use adapters::AdapterRow;

// Re-export adapter types from SQLite module for compatibility
pub use crate::adapters::{Adapter, AdapterActivation};

// Compatibility methods to match SQLite Db interface
impl PostgresDb {
    /// List all adapters (compatibility method matching SQLite Db interface)
    ///
    /// Note: For M0, returns all active adapters. Future versions should accept tenant_id filter.
    pub async fn list_adapters(&self) -> Result<Vec<Adapter>> {
        let adapters = sqlx::query_as::<_, Adapter>(
            "SELECT id, adapter_id, name, hash_b3, rank,
                    CASE tier
                        WHEN 'persistent' THEN 0
                        WHEN 'warm' THEN 1
                        WHEN 'ephemeral' THEN 2
                        ELSE 0
                    END as tier,
                    languages_json, framework,
                    category, scope, framework_id, framework_version, repo_id, commit_sha, intent,
                    current_state, pinned, memory_bytes::bigint as memory_bytes,
                    last_activated, activation_count::bigint as activation_count,
                    expires_at, created_at::text as created_at, updated_at::text as updated_at, active
             FROM adapters
             WHERE active = 1
             ORDER BY tier ASC, created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list adapters: {}", e)))?;
        Ok(adapters)
    }

    /// Get adapter by adapter_id (compatibility method)
    pub async fn get_adapter(&self, adapter_id: &str) -> Result<Option<Adapter>> {
        let adapter = sqlx::query_as::<_, Adapter>(
            "SELECT id, adapter_id, name, hash_b3, rank,
                    CASE tier
                        WHEN 'persistent' THEN 0
                        WHEN 'warm' THEN 1
                        WHEN 'ephemeral' THEN 2
                        ELSE 0
                    END as tier,
                    languages_json, framework,
                    category, scope, framework_id, framework_version, repo_id, commit_sha, intent,
                    current_state, pinned, memory_bytes::bigint as memory_bytes,
                    last_activated, activation_count::bigint as activation_count,
                    expires_at, created_at::text as created_at, updated_at::text as updated_at, active
             FROM adapters
             WHERE adapter_id = $1"
        )
        .bind(adapter_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get adapter: {}", e)))?;
        Ok(adapter)
    }

    /// Delete adapter by ID (compatibility method)
    pub async fn delete_adapter(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM adapters WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to delete adapter: {}", e)))?;
        Ok(())
    }

    /// Get adapter statistics (compatibility method)
    pub async fn get_adapter_stats(&self, adapter_id: &str) -> Result<(i64, i64, f64)> {
        let row = sqlx::query(
            "SELECT
                COUNT(*) as total,
                SUM(selected) as selected_count,
                AVG(gate_value) as avg_gate
             FROM adapter_activations
             WHERE adapter_id = $1",
        )
        .bind(adapter_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get adapter stats: {}", e)))?;

        let total: i64 = row.try_get("total").unwrap_or(0);
        let selected: i64 = row.try_get("selected_count").unwrap_or(0);
        let avg_gate: f64 = row.try_get("avg_gate").unwrap_or(0.0);

        Ok((total, selected, avg_gate))
    }

    // Additional compatibility methods
    pub async fn list_all_workers(&self) -> Result<Vec<crate::models::Worker>> {
        use crate::models::Worker;
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers ORDER BY started_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list workers: {}", e)))?;
        Ok(workers)
    }

    pub async fn list_replay_sessions(
        &self,
        tenant_id: Option<&str>,
    ) -> Result<Vec<crate::replay_sessions::ReplaySession>> {
        use crate::replay_sessions::ReplaySession;
        let query = if tenant_id.is_some() {
            "SELECT * FROM replay_sessions WHERE tenant_id = $1 ORDER BY snapshot_at DESC"
        } else {
            "SELECT * FROM replay_sessions ORDER BY snapshot_at DESC"
        };

        let sessions = if let Some(tid) = tenant_id {
            sqlx::query_as::<_, ReplaySession>(query)
                .bind(tid)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query_as::<_, ReplaySession>(query)
                .fetch_all(&self.pool)
                .await?
        };
        Ok(sessions)
    }

    pub async fn get_replay_session(
        &self,
        session_id: &str,
    ) -> Result<Option<crate::replay_sessions::ReplaySession>> {
        use crate::replay_sessions::ReplaySession;
        let session =
            sqlx::query_as::<_, ReplaySession>("SELECT * FROM replay_sessions WHERE id = $1")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to get replay session: {}", e)))?;
        Ok(session)
    }

    pub async fn create_replay_session(
        &self,
        session: &crate::replay_sessions::ReplaySession,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO replay_sessions (
                id, tenant_id, cpid, plan_id, snapshot_at, seed_global_b3,
                manifest_hash_b3, policy_hash_b3, kernel_hash_b3,
                telemetry_bundle_ids_json, adapter_state_json,
                routing_decisions_json, inference_traces_json, rng_state_json, signature
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(&session.id)
        .bind(&session.tenant_id)
        .bind(&session.cpid)
        .bind(&session.plan_id)
        .bind(&session.snapshot_at)
        .bind(&session.seed_global_b3)
        .bind(&session.manifest_hash_b3)
        .bind(&session.policy_hash_b3)
        .bind(&session.kernel_hash_b3)
        .bind(&session.telemetry_bundle_ids_json)
        .bind(&session.adapter_state_json)
        .bind(&session.routing_decisions_json)
        .bind(&session.inference_traces_json)
        .bind(&session.rng_state_json)
        .bind(&session.signature)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create replay session: {}", e)))?;
        Ok(())
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<crate::users::User>> {
        use crate::users::User;
        let user = sqlx::query_as::<_, User>(
            "SELECT id, email, display_name, pw_hash, role, disabled, created_at FROM users WHERE email = $1"
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get user by email: {}", e)))?;
        Ok(user)
    }

    pub async fn list_tenants(&self) -> Result<Vec<crate::tenants::Tenant>> {
        use crate::tenants::Tenant;
        let tenants = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at FROM tenants ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list tenants: {}", e)))?;
        Ok(tenants)
    }

    pub async fn create_tenant(&self, name: &str, itar_flag: bool) -> Result<String> {
        use uuid::Uuid;
        let id = Uuid::now_v7().to_string();
        sqlx::query("INSERT INTO tenants (id, name, itar_flag) VALUES ($1, $2, $3)")
            .bind(&id)
            .bind(name)
            .bind(itar_flag)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to create tenant: {}", e)))?;
        Ok(id)
    }

    pub async fn get_tenant(&self, id: &str) -> Result<Option<crate::tenants::Tenant>> {
        use crate::tenants::Tenant;
        let tenant = sqlx::query_as::<_, Tenant>(
            "SELECT id, name, itar_flag, created_at FROM tenants WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get tenant: {}", e)))?;
        Ok(tenant)
    }

    pub async fn rename_tenant(&self, id: &str, new_name: &str) -> Result<()> {
        sqlx::query("UPDATE tenants SET name = $1 WHERE id = $2")
            .bind(new_name)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to rename tenant: {}", e)))?;
        Ok(())
    }

    pub async fn update_node_status(&self, id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE nodes SET status = $1, last_seen_at = NOW() WHERE id = $2")
            .bind(status)
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update node status: {}", e)))?;
        Ok(())
    }

    pub async fn create_job(
        &self,
        kind: &str,
        tenant_id: Option<&str>,
        user_id: Option<&str>,
        payload_json: &str,
    ) -> Result<String> {
        use uuid::Uuid;
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO jobs (id, kind, tenant_id, user_id, payload_json, status) VALUES ($1, $2, $3, $4, $5, 'queued')"
        )
        .bind(&id)
        .bind(kind)
        .bind(tenant_id)
        .bind(user_id)
        .bind(payload_json)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create job: {}", e)))?;
        Ok(id)
    }

    pub async fn list_workers_by_node(&self, node_id: &str) -> Result<Vec<crate::models::Worker>> {
        use crate::models::Worker;
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at, last_seen_at FROM workers WHERE node_id = $1 ORDER BY started_at DESC"
        )
        .bind(node_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list workers by node: {}", e)))?;
        Ok(workers)
    }

    pub async fn update_worker_status(&self, worker_id: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE workers SET status = $1, last_seen_at = NOW() WHERE id = $2")
            .bind(status)
            .bind(worker_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update worker status: {}", e)))?;
        Ok(())
    }

    pub async fn register_adapter(&self, params: AdapterRegistrationParams) -> Result<String> {
        use uuid::Uuid;
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO adapters (
                id,
                adapter_id,
                name,
                hash_b3,
                rank,
                tier,
                languages_json,
                framework,
                category,
                scope,
                framework_id,
                framework_version,
                repo_id,
                commit_sha,
                intent,
                expires_at,
                current_state,
                pinned,
                memory_bytes,
                activation_count,
                active
            )
             VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, 'unloaded', 0, 0, 0, 1
             )",
        )
        .bind(&id)
        .bind(&params.adapter_id)
        .bind(&params.name)
        .bind(&params.hash_b3)
        .bind(params.rank)
        .bind(params.tier)
        .bind(&params.languages_json)
        .bind(&params.framework)
        .bind(&params.category)
        .bind(&params.scope)
        .bind(&params.framework_id)
        .bind(&params.framework_version)
        .bind(&params.repo_id)
        .bind(&params.commit_sha)
        .bind(&params.intent)
        .bind(&params.expires_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to register adapter: {}", e)))?;
        Ok(id)
    }

    pub async fn insert_worker(&self, params: crate::workers::WorkerInsertParams) -> Result<()> {
        sqlx::query(
            "INSERT INTO workers (id, tenant_id, node_id, plan_id, uds_path, pid, status, started_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(&params.id)
        .bind(&params.tenant_id)
        .bind(&params.node_id)
        .bind(&params.plan_id)
        .bind(&params.uds_path)
        .bind(params.pid)
        .bind(&params.status)
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert worker: {}", e)))?;
        Ok(())
    }

    pub async fn update_worker_heartbeat(&self, id: &str, status: Option<&str>) -> Result<()> {
        if let Some(st) = status {
            sqlx::query("UPDATE workers SET status = $1, last_seen_at = $2 WHERE id = $3")
                .bind(st)
                .bind(chrono::Utc::now())
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to update worker heartbeat: {}", e))
                })?;
        } else {
            sqlx::query("UPDATE workers SET last_seen_at = $1 WHERE id = $2")
                .bind(chrono::Utc::now())
                .bind(id)
                .execute(&self.pool)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to update worker heartbeat: {}", e))
                })?;
        }
        Ok(())
    }

    pub async fn update_adapter_state(
        &self,
        adapter_id: &str,
        state: &str,
        _reason: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE adapters SET current_state = $1 WHERE adapter_id = $2")
            .bind(state)
            .bind(adapter_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update adapter state: {}", e)))?;
        Ok(())
    }

    pub async fn register_node(&self, hostname: &str, agent_endpoint: &str) -> Result<String> {
        use uuid::Uuid;
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO nodes (id, hostname, agent_endpoint, status) VALUES ($1, $2, $3, 'active')"
        )
        .bind(&id)
        .bind(hostname)
        .bind(agent_endpoint)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to register node: {}", e)))?;
        Ok(id)
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<crate::nodes::Node>> {
        use crate::nodes::Node;
        let node = sqlx::query_as::<_, Node>(
            "SELECT id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at FROM nodes WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get node: {}", e)))?;
        Ok(node)
    }

    pub async fn list_nodes(&self) -> Result<Vec<crate::nodes::Node>> {
        use crate::nodes::Node;
        let nodes = sqlx::query_as::<_, Node>(
            "SELECT id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at FROM nodes ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list nodes: {}", e)))?;
        Ok(nodes)
    }

    pub async fn get_training_job(
        &self,
        job_id: &str,
    ) -> Result<Option<crate::training_jobs::TrainingJobRecord>> {
        use crate::training_jobs::TrainingJobRecord;
        let job = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by
             FROM repository_training_jobs WHERE id = $1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get training job: {}", e)))?;
        Ok(job)
    }

    pub async fn update_training_status(&self, job_id: &str, status: &str) -> Result<()> {
        let completed_at = if status == "completed" || status == "failed" {
            Some(chrono::Utc::now().to_rfc3339())
        } else {
            None
        };

        sqlx::query(
            "UPDATE repository_training_jobs
             SET status = $1, completed_at = $2
             WHERE id = $3",
        )
        .bind(status)
        .bind(completed_at)
        .bind(job_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update training status: {}", e)))?;
        Ok(())
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
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO git_repositories
             (id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json, status, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(&id)
        .bind(repo_id)
        .bind(path)
        .bind(branch)
        .bind(analysis_json)
        .bind("[]") // Empty evidence JSON
        .bind("{}") // Empty security scan JSON
        .bind("registered")
        .bind(created_by)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create git repository: {}", e)))?;
        Ok(id)
    }

    pub async fn get_git_repository(
        &self,
        repo_id: &str,
    ) -> Result<Option<crate::git_repositories::GitRepository>> {
        use crate::git_repositories::GitRepository;
        let repo = sqlx::query_as::<_, GitRepository>(
            "SELECT id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json,
                    status, created_by, created_at, updated_at
             FROM git_repositories WHERE repo_id = $1",
        )
        .bind(repo_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get git repository: {}", e)))?;
        Ok(repo)
    }

    pub async fn list_git_repositories(
        &self,
    ) -> Result<Vec<crate::git_repositories::GitRepository>> {
        use crate::git_repositories::GitRepository;
        let repos = sqlx::query_as::<_, GitRepository>(
            "SELECT id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json,
                    status, created_by, created_at, updated_at
             FROM git_repositories ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list git repositories: {}", e)))?;
        Ok(repos)
    }

    // Training job methods
    pub async fn create_training_job(
        &self,
        repo_id: &str,
        training_config_json: &str,
        created_by: &str,
    ) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO repository_training_jobs
             (id, repo_id, training_config_json, status, progress_json, started_at, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&id)
        .bind(repo_id)
        .bind(training_config_json)
        .bind("pending")
        .bind("{}")
        .bind(chrono::Utc::now())
        .bind(created_by)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create training job: {}", e)))?;
        Ok(id)
    }

    // Model methods
    pub async fn register_model(&self, params: ModelRegistrationParams) -> Result<String> {
        let ModelRegistrationParams {
            name,
            hash_b3,
            config_hash_b3,
            tokenizer_hash_b3,
            tokenizer_cfg_hash_b3,
            license_hash_b3,
            metadata_json,
        } = params;
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO models
             (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
              license_hash_b3, metadata_json, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(&id)
        .bind(&name)
        .bind(&hash_b3)
        .bind(&config_hash_b3)
        .bind(&tokenizer_hash_b3)
        .bind(&tokenizer_cfg_hash_b3)
        .bind(license_hash_b3.as_deref())
        .bind(metadata_json.as_deref())
        .bind(chrono::Utc::now())
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to register model: {}", e)))?;
        Ok(id)
    }

    pub async fn get_model(&self, id: &str) -> Result<Option<crate::models::Model>> {
        use crate::models::Model;
        let model = sqlx::query_as::<_, Model>(
            "SELECT id, tenant_id, model_id, model_name, base_model, model_type, model_path,
                    status, metadata_json, created_at, updated_at
             FROM models WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get model: {}", e)))?;
        Ok(model)
    }

    pub async fn get_base_model_status(
        &self,
        tenant_id: &str,
    ) -> Result<Option<crate::models::BaseModelStatus>> {
        use crate::models::BaseModelStatus;
        let status = sqlx::query_as::<_, BaseModelStatus>(
            "SELECT base_model, COUNT(*) as model_count, MAX(updated_at) as last_updated
             FROM models
             WHERE tenant_id = $1 AND status = 'active'
             GROUP BY base_model
             ORDER BY last_updated DESC
             LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get base model status: {}", e)))?;
        Ok(status)
    }

    // Job methods
    pub async fn list_jobs(&self, tenant_id: Option<&str>) -> Result<Vec<crate::jobs::Job>> {
        use crate::jobs::Job;
        let query = if let Some(tenant_id) = tenant_id {
            sqlx::query_as::<_, Job>(
                "SELECT id, kind, tenant_id, user_id, payload_json, status,
                        created_at, updated_at, completed_at
                 FROM jobs WHERE tenant_id = $1 ORDER BY created_at DESC",
            )
            .bind(tenant_id)
        } else {
            sqlx::query_as::<_, Job>(
                "SELECT id, kind, tenant_id, user_id, payload_json, status,
                        created_at, updated_at, completed_at
                 FROM jobs ORDER BY created_at DESC",
            )
        };
        let jobs = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list jobs: {}", e)))?;
        Ok(jobs)
    }

    // Plan methods
    pub async fn get_plan(&self, id: &str) -> Result<Option<crate::models::Plan>> {
        let plan = sqlx::query_as::<_, crate::models::Plan>(
            "SELECT id, name, description, config_json, active, created_at, updated_at
             FROM plans WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get plan: {}", e)))?;
        Ok(plan)
    }

    // Audit methods
    pub async fn list_all_audits(&self) -> Result<Vec<crate::audits::Audit>> {
        use crate::audits::Audit;
        let audits = sqlx::query_as::<_, Audit>(
            "SELECT id, tenant_id, user_id, action, resource_type, resource_id,
                    details_json, ip_address, user_agent, created_at
             FROM audits ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list audits: {}", e)))?;
        Ok(audits)
    }

    // CP Pointer methods
    pub async fn get_active_cp_pointer(
        &self,
        tenant_id: &str,
    ) -> Result<Option<crate::models::CpPointer>> {
        let pointer = sqlx::query_as::<_, crate::models::CpPointer>(
            "SELECT id, tenant_id, name, adapter_id, active, created_at, updated_at
             FROM cp_pointers WHERE tenant_id = $1 AND active = true
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get active CP pointer: {}", e)))?;
        Ok(pointer)
    }

    pub async fn list_plans_by_tenant(&self, tenant_id: &str) -> Result<Vec<crate::models::Plan>> {
        let plans = sqlx::query_as::<_, crate::models::Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
             FROM plans WHERE tenant_id = $1 ORDER BY created_at DESC"
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list plans by tenant: {}", e)))?;
        Ok(plans)
    }

    pub async fn list_all_plans(&self) -> Result<Vec<crate::models::Plan>> {
        let plans = sqlx::query_as::<_, crate::models::Plan>(
            "SELECT id, tenant_id, plan_id_b3, manifest_hash_b3, kernel_hashes_json, metallib_hash_b3, created_at
             FROM plans ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list all plans: {}", e)))?;
        Ok(plans)
    }

    pub async fn deactivate_all_cp_pointers(&self, tenant_id: &str) -> Result<()> {
        sqlx::query("UPDATE cp_pointers SET active = false WHERE tenant_id = $1")
            .bind(tenant_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to deactivate CP pointers: {}", e)))?;
        Ok(())
    }

    pub async fn insert_cp_pointer(
        &self,
        id: &str,
        tenant_id: &str,
        name: &str,
        adapter_id: &str,
        active: bool,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO cp_pointers (id, tenant_id, name, adapter_id, active, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $6)"
        )
        .bind(id)
        .bind(tenant_id)
        .bind(name)
        .bind(adapter_id)
        .bind(active)
        .bind(chrono::Utc::now())
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert CP pointer: {}", e)))?;
        Ok(())
    }

    pub async fn list_cp_pointers_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<crate::models::CpPointer>> {
        let pointers = sqlx::query_as::<_, crate::models::CpPointer>(
            "SELECT id, tenant_id, name, adapter_id, active, created_at, updated_at
             FROM cp_pointers WHERE tenant_id = $1 ORDER BY created_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list CP pointers: {}", e)))?;
        Ok(pointers)
    }

    pub async fn activate_cp_pointer(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE cp_pointers SET active = true, updated_at = $1 WHERE id = $2")
            .bind(chrono::Utc::now())
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to activate CP pointer: {}", e)))?;
        Ok(())
    }

    pub async fn get_cp_pointer_by_name(
        &self,
        name: &str,
    ) -> Result<Option<crate::models::CpPointer>> {
        let pointer = sqlx::query_as::<_, crate::models::CpPointer>(
            "SELECT id, tenant_id, name, adapter_id, active, created_at, updated_at
             FROM cp_pointers WHERE name = $1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get CP pointer by name: {}", e)))?;
        Ok(pointer)
    }

    // Worker methods
    pub async fn list_workers_by_tenant(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<crate::models::Worker>> {
        use crate::models::Worker;
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at
             FROM workers WHERE tenant_id = $1 ORDER BY started_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list workers by tenant: {}", e)))?;
        Ok(workers)
    }

    pub async fn get_adapter_activations(
        &self,
        adapter_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::adapters::AdapterActivation>> {
        let activations = sqlx::query_as::<_, crate::adapters::AdapterActivation>(
            "SELECT id, adapter_id, request_id, gate_value, selected, created_at
             FROM adapter_activations
             WHERE adapter_id = $1
             ORDER BY created_at DESC
             LIMIT $2",
        )
        .bind(adapter_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get adapter activations: {}", e)))?;
        Ok(activations)
    }

    pub async fn update_adapter_memory(&self, adapter_id: &str, memory_bytes: i64) -> Result<()> {
        sqlx::query("UPDATE adapters SET memory_bytes = $1, updated_at = $2 WHERE adapter_id = $3")
            .bind(memory_bytes)
            .bind(chrono::Utc::now())
            .bind(adapter_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update adapter memory: {}", e)))?;
        Ok(())
    }

    pub async fn list_repositories(
        &self,
        tenant_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<crate::repositories::Repository>> {
        let repos = sqlx::query_as::<_, crate::repositories::Repository>(
            r#"
            SELECT id, tenant_id, repo_id, path, languages_json, default_branch,
                   latest_scan_commit, latest_scan_at, latest_graph_hash, status,
                   created_at, updated_at
            FROM repositories
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list repositories: {}", e)))?;
        Ok(repos)
    }

    /// List monitoring rules with optional filters
    pub async fn list_monitoring_rules(
        &self,
        tenant_id: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Vec<crate::process_monitoring::ProcessMonitoringRule>> {
        let mut query = "SELECT * FROM process_monitoring_rules WHERE 1=1".to_string();
        let mut bind_count = 0;

        if tenant_id.is_some() {
            bind_count += 1;
            query.push_str(&format!(" AND tenant_id = ${}", bind_count));
        }

        if is_active.is_some() {
            bind_count += 1;
            query.push_str(&format!(" AND is_active = ${}", bind_count));
        }

        query.push_str(" ORDER BY created_at DESC");

        let mut sql_query = sqlx::query(&query);

        if let Some(tenant) = tenant_id {
            sql_query = sql_query.bind(tenant);
        }

        if let Some(active) = is_active {
            sql_query = sql_query.bind(active);
        }

        let rows = sql_query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list monitoring rules: {}", e)))?;

        let mut rules = Vec::new();
        for row in rows {
            let rule_type: RuleType =
                serde_json::from_value(row.get("rule_type")).unwrap_or(RuleType::Cpu);
            let threshold_operator: ThresholdOperator =
                serde_json::from_value(row.get("threshold_operator"))
                    .unwrap_or(ThresholdOperator::Gt);
            let severity: AlertSeverity =
                serde_json::from_value(row.get("severity")).unwrap_or(AlertSeverity::Warning);

            rules.push(ProcessMonitoringRule {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                tenant_id: row.get("tenant_id"),
                rule_type,
                metric_name: row.get("metric_name"),
                threshold_value: row.get("threshold_value"),
                threshold_operator,
                severity,
                evaluation_window_seconds: row.get("evaluation_window_seconds"),
                cooldown_seconds: row.get("cooldown_seconds"),
                is_active: row.get("is_active"),
                notification_channels: row.get("notification_channels"),
                escalation_rules: row.get("escalation_rules"),
                created_by: row.get("created_by"),
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }

        Ok(rules)
    }

    pub async fn list_process_alerts(&self, filters: AlertFilters) -> Result<Vec<ProcessAlert>> {
        let mut builder = sqlx::QueryBuilder::new("SELECT * FROM process_alerts WHERE 1=1");

        if let Some(tenant) = filters.tenant_id {
            builder.push(" AND tenant_id = ").push_bind(tenant);
        }

        if let Some(worker) = filters.worker_id {
            builder.push(" AND worker_id = ").push_bind(worker);
        }

        if let Some(status) = filters.status {
            builder.push(" AND status = ").push_bind(status.to_string());
        }

        if let Some(severity) = filters.severity {
            builder
                .push(" AND severity = ")
                .push_bind(severity.to_string());
        }

        builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = filters.limit {
            builder.push(" LIMIT ").push_bind(limit);
        }

        let query = builder.build();
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list alerts: {}", e)))?;

        let mut alerts = Vec::new();
        for row in rows {
            let severity = AlertSeverity::from_string(row.get::<String, _>("severity"));
            let status = AlertStatus::from_string(row.get::<String, _>("status"));

            alerts.push(ProcessAlert {
                id: row.get("id"),
                rule_id: row.get("rule_id"),
                worker_id: row.get("worker_id"),
                tenant_id: row.get("tenant_id"),
                alert_type: row.get("alert_type"),
                severity,
                title: row.get("title"),
                message: row.get("message"),
                metric_value: row.get("metric_value"),
                threshold_value: row.get("threshold_value"),
                status,
                acknowledged_by: row.get("acknowledged_by"),
                acknowledged_at: row
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("acknowledged_at"),
                resolved_at: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at"),
                suppression_reason: row.get("suppression_reason"),
                suppression_until: row
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("suppression_until"),
                escalation_level: row.get::<i64, _>("escalation_level"),
                notification_sent: row.get("notification_sent"),
                created_at: row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                updated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            });
        }

        Ok(alerts)
    }

    pub async fn get_process_alert(&self, id: &str) -> Result<Option<ProcessAlert>> {
        let row = sqlx::query("SELECT * FROM process_alerts WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get alert: {}", e)))?;

        if let Some(row) = row {
            let severity = AlertSeverity::from_string(row.get::<String, _>("severity"));
            let status = AlertStatus::from_string(row.get::<String, _>("status"));

            Ok(Some(ProcessAlert {
                id: row.get("id"),
                rule_id: row.get("rule_id"),
                worker_id: row.get("worker_id"),
                tenant_id: row.get("tenant_id"),
                alert_type: row.get("alert_type"),
                severity,
                title: row.get("title"),
                message: row.get("message"),
                metric_value: row.get("metric_value"),
                threshold_value: row.get("threshold_value"),
                status,
                acknowledged_by: row.get("acknowledged_by"),
                acknowledged_at: row
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("acknowledged_at"),
                resolved_at: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at"),
                suppression_reason: row.get("suppression_reason"),
                suppression_until: row
                    .get::<Option<chrono::DateTime<chrono::Utc>>, _>("suppression_until"),
                escalation_level: row.get::<i64, _>("escalation_level"),
                notification_sent: row.get("notification_sent"),
                created_at: row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                updated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn update_process_alert_status(
        &self,
        id: &str,
        status: AlertStatus,
        user: Option<&str>,
    ) -> Result<()> {
        let mut builder = sqlx::QueryBuilder::new("UPDATE process_alerts SET status = ");
        builder.push_bind(status.to_string());
        builder.push(", updated_at = NOW()");

        if matches!(status, AlertStatus::Acknowledged) {
            builder.push(", acknowledged_at = NOW()");
            if let Some(user) = user {
                builder.push(", acknowledged_by = ").push_bind(user);
            }
        }

        if matches!(status, AlertStatus::Resolved) {
            builder.push(", resolved_at = NOW()");
        }

        builder.push(" WHERE id = ").push_bind(id);

        builder
            .build()
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update alert status: {}", e)))?;

        Ok(())
    }

    pub async fn list_process_anomalies(
        &self,
        filters: AnomalyFilters,
    ) -> Result<Vec<ProcessAnomaly>> {
        let mut builder = sqlx::QueryBuilder::new("SELECT * FROM process_anomalies WHERE 1=1");

        if let Some(tenant) = filters.tenant_id {
            builder.push(" AND tenant_id = ").push_bind(tenant);
        }

        if let Some(worker) = filters.worker_id {
            builder.push(" AND worker_id = ").push_bind(worker);
        }

        if let Some(status) = filters.status {
            builder.push(" AND status = ").push_bind(status.to_string());
        }

        if let Some(anomaly_type) = filters.anomaly_type {
            builder.push(" AND anomaly_type = ").push_bind(anomaly_type);
        }

        builder.push(" ORDER BY created_at DESC");

        if let Some(limit) = filters.limit {
            builder.push(" LIMIT ").push_bind(limit);
        }

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list anomalies: {}", e)))?;

        let mut anomalies = Vec::new();
        for row in rows {
            anomalies.push(ProcessAnomaly {
                id: row.get("id"),
                worker_id: row.get("worker_id"),
                tenant_id: row.get("tenant_id"),
                anomaly_type: row.get("anomaly_type"),
                metric_name: row.get("metric_name"),
                detected_value: row.get("detected_value"),
                expected_range_min: row.get("expected_range_min"),
                expected_range_max: row.get("expected_range_max"),
                confidence_score: row.get("confidence_score"),
                severity: AlertSeverity::from_string(row.get::<String, _>("severity")),
                description: row.get("description"),
                detection_method: row.get("detection_method"),
                model_version: row.get("model_version"),
                status: AnomalyStatus::from_string(row.get::<String, _>("status")),
                investigated_by: row.get("investigated_by"),
                investigation_notes: row.get("investigation_notes"),
                resolved_at: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at"),
                created_at: row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            });
        }

        Ok(anomalies)
    }

    pub async fn get_process_anomaly(&self, id: &str) -> Result<Option<ProcessAnomaly>> {
        let row = sqlx::query("SELECT * FROM process_anomalies WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get anomaly: {}", e)))?;

        if let Some(row) = row {
            Ok(Some(ProcessAnomaly {
                id: row.get("id"),
                worker_id: row.get("worker_id"),
                tenant_id: row.get("tenant_id"),
                anomaly_type: row.get("anomaly_type"),
                metric_name: row.get("metric_name"),
                detected_value: row.get("detected_value"),
                expected_range_min: row.get("expected_range_min"),
                expected_range_max: row.get("expected_range_max"),
                confidence_score: row.get("confidence_score"),
                severity: AlertSeverity::from_string(row.get::<String, _>("severity")),
                description: row.get("description"),
                detection_method: row.get("detection_method"),
                model_version: row.get("model_version"),
                status: AnomalyStatus::from_string(row.get::<String, _>("status")),
                investigated_by: row.get("investigated_by"),
                investigation_notes: row.get("investigation_notes"),
                resolved_at: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at"),
                created_at: row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn update_process_anomaly_status(
        &self,
        id: &str,
        status: AnomalyStatus,
        investigated_by: Option<&str>,
        notes: Option<&str>,
    ) -> Result<()> {
        let mut builder = sqlx::QueryBuilder::new("UPDATE process_anomalies SET status = ");
        builder.push_bind(status.to_string());

        if let Some(user) = investigated_by {
            builder.push(", investigated_by = ").push_bind(user);
        }

        if let Some(notes) = notes {
            builder.push(", investigation_notes = ").push_bind(notes);
        }

        if matches!(status, AnomalyStatus::Resolved) {
            builder.push(", resolved_at = NOW()");
        }

        builder.push(" WHERE id = ").push_bind(id);

        builder
            .build()
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to update anomaly status: {}", e)))?;

        Ok(())
    }

    pub async fn create_monitoring_dashboard(&self, req: CreateDashboardRequest) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();

        let CreateDashboardRequest {
            name,
            description,
            tenant_id,
            dashboard_config,
            is_shared,
            created_by,
        } = req;

        let description_ref = description.as_deref();
        let created_by_ref = created_by.as_deref();

        sqlx::query!(
            r#"
            INSERT INTO process_monitoring_dashboards (
                id, name, description, tenant_id, dashboard_config, is_shared, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            id,
            name,
            description_ref,
            tenant_id,
            dashboard_config,
            is_shared,
            created_by_ref
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create dashboard: {}", e)))?;

        Ok(id)
    }

    pub async fn list_monitoring_dashboards(
        &self,
        tenant_id: Option<&str>,
        is_shared: Option<bool>,
    ) -> Result<Vec<MonitoringDashboard>> {
        let mut builder =
            sqlx::QueryBuilder::new("SELECT * FROM process_monitoring_dashboards WHERE 1=1");

        if let Some(tenant) = tenant_id {
            builder.push(" AND tenant_id = ").push_bind(tenant);
        }

        if let Some(shared) = is_shared {
            builder.push(" AND is_shared = ").push_bind(shared);
        }

        builder.push(" ORDER BY created_at DESC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list dashboards: {}", e)))?;

        let mut dashboards = Vec::new();
        for row in rows {
            dashboards.push(MonitoringDashboard {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                tenant_id: row.get("tenant_id"),
                dashboard_config: row.get::<serde_json::Value, _>("dashboard_config"),
                is_shared: row.get("is_shared"),
                created_by: row.get("created_by"),
                created_at: row.get::<chrono::DateTime<chrono::Utc>, _>("created_at"),
                updated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at"),
            });
        }

        Ok(dashboards)
    }

    pub async fn create_monitoring_report(&self, req: CreateReportRequest) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();

        let CreateReportRequest {
            name,
            description,
            tenant_id,
            report_type,
            report_config,
            report_data,
            file_path,
            file_size_bytes,
            created_by,
        } = req;

        let description_ref = description.as_deref();
        let file_path_ref = file_path.as_deref();
        let created_by_ref = created_by.as_deref();

        sqlx::query!(
            r#"
            INSERT INTO process_monitoring_reports (
                id, name, description, tenant_id, report_type, report_config,
                report_data, file_path, file_size_bytes, created_by
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            id,
            name,
            description_ref,
            tenant_id,
            report_type,
            report_config,
            report_data,
            file_path_ref,
            file_size_bytes,
            created_by_ref
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to create report: {}", e)))?;

        Ok(id)
    }

    pub async fn list_monitoring_reports(
        &self,
        tenant_id: Option<&str>,
        report_type: Option<&str>,
    ) -> Result<Vec<ProcessMonitoringReport>> {
        let mut builder =
            sqlx::QueryBuilder::new("SELECT * FROM process_monitoring_reports WHERE 1=1");

        if let Some(tenant) = tenant_id {
            builder.push(" AND tenant_id = ").push_bind(tenant);
        }

        if let Some(rtype) = report_type {
            builder.push(" AND report_type = ").push_bind(rtype);
        }

        builder.push(" ORDER BY generated_at DESC");

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to list reports: {}", e)))?;

        let mut reports = Vec::new();
        for row in rows {
            reports.push(ProcessMonitoringReport {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                tenant_id: row.get("tenant_id"),
                report_type: row.get("report_type"),
                report_config: row.get("report_config"),
                generated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("generated_at"),
                report_data: row.get("report_data"),
                file_path: row.get("file_path"),
                file_size_bytes: row.get("file_size_bytes"),
                created_by: row.get("created_by"),
            });
        }

        Ok(reports)
    }

    // Pinned adapter methods
    pub async fn pin_adapter(
        &self,
        tenant_id: &str,
        adapter_id: &str,
        pinned_until: Option<&str>,
        reason: &str,
        pinned_by: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO pinned_adapters (id, tenant_id, adapter_id, pinned_until, reason, pinned_by, pinned_at)
             VALUES ($1, $2, $3, $4, $5, $6, NOW())
             ON CONFLICT(tenant_id, adapter_id) DO UPDATE SET
                pinned_until = excluded.pinned_until,
                reason = excluded.reason,
                pinned_by = excluded.pinned_by,
                pinned_at = NOW()"
        )
        .bind(&id)
        .bind(tenant_id)
        .bind(adapter_id)
        .bind(pinned_until)
        .bind(reason)
        .bind(pinned_by)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to pin adapter: {}", e)))?;
        Ok(id)
    }

    pub async fn unpin_adapter(&self, tenant_id: &str, adapter_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM pinned_adapters WHERE tenant_id = $1 AND adapter_id = $2")
            .bind(tenant_id)
            .bind(adapter_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to unpin adapter: {}", e)))?;
        Ok(())
    }

    pub async fn list_pinned_adapters(
        &self,
        tenant_id: &str,
    ) -> Result<Vec<crate::pinned_adapters::PinnedAdapter>> {
        let adapters = sqlx::query_as::<_, crate::pinned_adapters::PinnedAdapter>(
            "SELECT id, tenant_id, adapter_id, pinned_until, reason, pinned_at, pinned_by
             FROM pinned_adapters
             WHERE tenant_id = $1
             AND (pinned_until IS NULL OR pinned_until > NOW())
             ORDER BY pinned_at DESC",
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list pinned adapters: {}", e)))?;
        Ok(adapters)
    }

    pub async fn count_enclave_operations(&self) -> Result<i64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM enclave_operations")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to count enclave operations: {}", e))
            })?;
        Ok(count)
    }

    pub async fn get_policies(&self, _tenant_id: &str) -> Result<crate::policies::TenantPolicies> {
        // For now, return default policies
        // TODO: Implement database storage and retrieval of tenant-specific policies
        Ok(crate::policies::TenantPolicies::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_postgres_connection() {
        let db = PostgresDb::connect("postgresql://aos:aos@localhost/adapteros_test")
            .await
            .expect("Failed to connect to test database");

        db.health_check().await.expect("Health check failed");

        let (size, idle) = db.pool_stats();
        assert!(size > 0, "Pool should have connections");
        assert!(idle > 0, "Pool should have idle connections");

        db.close().await;
    }

    #[ignore] // Requires PostgreSQL server
    async fn test_postgres_migration() {
        let db = PostgresDb::connect("postgresql://aos:aos@localhost/adapteros_test")
            .await
            .expect("Failed to connect");

        db.migrate().await.expect("Migration failed");
        db.close().await;
    }
}
