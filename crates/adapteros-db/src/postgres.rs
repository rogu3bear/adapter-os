//! PostgreSQL connection pool and query methods
//!
//! Production backend for AdapterOS registry and state management.
//! Replaces SQLite for multi-node deployments with pgvector support.

use adapteros_core::{AosError, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use uuid;
use chrono;

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
        sqlx::migrate!("../../migrations_postgres")
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
             WHERE adapter_id = $1"
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

    pub async fn list_replay_sessions(&self, tenant_id: Option<&str>) -> Result<Vec<crate::replay_sessions::ReplaySession>> {
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

    pub async fn get_replay_session(&self, session_id: &str) -> Result<Option<crate::replay_sessions::ReplaySession>> {
        use crate::replay_sessions::ReplaySession;
        let session = sqlx::query_as::<_, ReplaySession>("SELECT * FROM replay_sessions WHERE id = $1")
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to get replay session: {}", e)))?;
        Ok(session)
    }

    pub async fn create_replay_session(&self, session: &crate::replay_sessions::ReplaySession) -> Result<()> {
        sqlx::query(
            "INSERT INTO replay_sessions (
                id, tenant_id, cpid, plan_id, snapshot_at, seed_global_b3,
                manifest_hash_b3, policy_hash_b3, kernel_hash_b3,
                telemetry_bundle_ids_json, adapter_state_json,
                routing_decisions_json, inference_traces_json, rng_state_json, signature
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)"
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
            "SELECT id, name, itar_flag, created_at FROM tenants ORDER BY created_at DESC"
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
            "SELECT id, name, itar_flag, created_at FROM tenants WHERE id = $1"
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

    pub async fn create_job(&self, kind: &str, tenant_id: Option<&str>, user_id: Option<&str>, payload_json: &str) -> Result<String> {
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

    pub async fn register_adapter(
        &self,
        adapter_id: &str,
        name: &str,
        hash_b3: &str,
        rank: i32,
        tier: i32,
        languages_json: Option<&str>,
        framework: Option<&str>,
    ) -> Result<String> {
        use uuid::Uuid;
        let id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO adapters (id, adapter_id, name, hash_b3, rank, tier, languages_json, framework, category, scope, current_state, pinned, memory_bytes, activation_count, active)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'code', 'global', 'unloaded', 0, 0, 0, 1)"
        )
        .bind(&id)
        .bind(adapter_id)
        .bind(name)
        .bind(hash_b3)
        .bind(rank)
        .bind(tier)
        .bind(languages_json)
        .bind(framework)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to register adapter: {}", e)))?;
        Ok(id)
    }

    pub async fn update_adapter_state(&self, adapter_id: &str, state: &str, _reason: &str) -> Result<()> {
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

    pub async fn get_training_job(&self, job_id: &str) -> Result<Option<crate::training_jobs::TrainingJobRecord>> {
        use crate::training_jobs::TrainingJobRecord;
        let job = sqlx::query_as::<_, TrainingJobRecord>(
            "SELECT id, repo_id, training_config_json, status, progress_json,
                    started_at, completed_at, created_by
             FROM repository_training_jobs WHERE id = $1"
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
             WHERE id = $3"
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

    pub async fn get_git_repository(&self, repo_id: &str) -> Result<Option<crate::git_repositories::GitRepository>> {
        use crate::git_repositories::GitRepository;
        let repo = sqlx::query_as::<_, GitRepository>(
            "SELECT id, repo_id, path, branch, analysis_json, evidence_json, security_scan_json,
                    status, created_by, created_at, updated_at
             FROM git_repositories WHERE repo_id = $1"
        )
        .bind(repo_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get git repository: {}", e)))?;
        Ok(repo)
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
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
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
    pub async fn register_model(
        &self,
        name: &str,
        hash_b3: &str,
        config_hash_b3: &str,
        tokenizer_hash_b3: &str,
        tokenizer_cfg_hash_b3: &str,
        license_hash_b3: Option<&str>,
        metadata_json: Option<&str>,
    ) -> Result<String> {
        let id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO models
             (id, name, hash_b3, config_hash_b3, tokenizer_hash_b3, tokenizer_cfg_hash_b3,
              license_hash_b3, metadata_json, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
        )
        .bind(&id)
        .bind(name)
        .bind(hash_b3)
        .bind(config_hash_b3)
        .bind(tokenizer_hash_b3)
        .bind(tokenizer_cfg_hash_b3)
        .bind(license_hash_b3)
        .bind(metadata_json)
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
             FROM models WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get model: {}", e)))?;
        Ok(model)
    }

    pub async fn get_base_model_status(&self, tenant_id: &str) -> Result<Option<crate::models::BaseModelStatus>> {
        use crate::models::BaseModelStatus;
        let status = sqlx::query_as::<_, BaseModelStatus>(
            "SELECT base_model, COUNT(*) as model_count, MAX(updated_at) as last_updated
             FROM models
             WHERE tenant_id = $1 AND status = 'active'
             GROUP BY base_model
             ORDER BY last_updated DESC
             LIMIT 1"
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
                 FROM jobs WHERE tenant_id = $1 ORDER BY created_at DESC"
            )
            .bind(tenant_id)
        } else {
            sqlx::query_as::<_, Job>(
                "SELECT id, kind, tenant_id, user_id, payload_json, status,
                        created_at, updated_at, completed_at
                 FROM jobs ORDER BY created_at DESC"
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
             FROM plans WHERE id = $1"
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
             FROM audits ORDER BY created_at DESC"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list audits: {}", e)))?;
        Ok(audits)
    }

    // CP Pointer methods
    pub async fn get_active_cp_pointer(&self, tenant_id: &str) -> Result<Option<crate::models::CpPointer>> {
        let pointer = sqlx::query_as::<_, crate::models::CpPointer>(
            "SELECT id, tenant_id, name, adapter_id, active, created_at, updated_at
             FROM cp_pointers WHERE tenant_id = $1 AND active = true
             ORDER BY created_at DESC LIMIT 1"
        )
        .bind(tenant_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get active CP pointer: {}", e)))?;
        Ok(pointer)
    }

    pub async fn get_cp_pointer_by_name(&self, name: &str) -> Result<Option<crate::models::CpPointer>> {
        let pointer = sqlx::query_as::<_, crate::models::CpPointer>(
            "SELECT id, tenant_id, name, adapter_id, active, created_at, updated_at
             FROM cp_pointers WHERE name = $1"
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get CP pointer by name: {}", e)))?;
        Ok(pointer)
    }

    // Worker methods
    pub async fn list_workers_by_tenant(&self, tenant_id: &str) -> Result<Vec<crate::models::Worker>> {
        use crate::models::Worker;
        let workers = sqlx::query_as::<_, Worker>(
            "SELECT id, tenant_id, node_id, plan_id, uds_path, pid, status,
                    started_at, last_seen_at
             FROM workers WHERE tenant_id = $1 ORDER BY started_at DESC"
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list workers by tenant: {}", e)))?;
        Ok(workers)
    }
}

// Re-export sqlx types for PostgreSQL
pub use sqlx::postgres::{PgQueryResult, PgRow};
pub use sqlx::Row as PgRowTrait;

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

    #[tokio::test]
    #[ignore] // Requires PostgreSQL server
    async fn test_postgres_migration() {
        let db = PostgresDb::connect("postgresql://aos:aos@localhost/adapteros_test")
            .await
            .expect("Failed to connect");

        db.migrate().await.expect("Migration failed");
        db.close().await;
    }
}
