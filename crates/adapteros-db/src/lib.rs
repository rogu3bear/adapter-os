use adapteros_core::AosError;
use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

// Database abstraction layer
#[cfg(feature = "postgres")]
pub mod postgres_backend;
pub mod sqlite_backend;
pub mod traits;

// Re-export commonly used types
pub use traits::{
    AdapterRecord, CreateStackRequest, DatabaseBackend, DatabaseBackendType, DatabaseConfig,
    StackRecord,
};

// PostgreSQL backend for production (legacy - to be deprecated)
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "postgres")]
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
    /// Configuration:
    /// - WAL mode for better concurrency
    /// - Normal synchronous mode (balance between safety and performance)
    /// - 30-second connection timeout
    /// - Max 20 connections in pool
    /// - Statement cache size of 100
    pub async fn connect(path: &str) -> Result<Self> {
        use std::time::Duration;

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(30)) // 30s timeout for busy database
            .statement_cache_capacity(100); // Cache up to 100 prepared statements

        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

        Ok(Self { pool })
    }

    /// Connect to SQLite database using DATABASE_URL environment variable
    pub async fn connect_env() -> Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "./var/cp.db".to_string());
        Self::connect(&database_url).await
    }

    /// Create in-memory database for testing
    ///
    /// This creates a temporary SQLite database in memory with all migrations applied.
    /// Useful for unit tests and integration tests.
    ///
    /// # Note
    /// This is available in both test and non-test builds for maximum flexibility.
    pub async fn new_in_memory() -> Result<Self> {
        let db = Self::connect(":memory:").await?;
        db.migrate().await?;
        Ok(db)
    }

    /// Run database migrations with signature verification
    ///
    /// Per Artifacts Ruleset #13: All migrations must be Ed25519 signed.
    /// This method:
    /// 1. Verifies all migration signatures before applying
    /// 2. Runs migrations via sqlx
    /// 3. Verifies database is at expected version after completion
    pub async fn migrate(&self) -> Result<()> {
        use tracing::info;

        // Use CARGO_MANIFEST_DIR to find migrations relative to workspace root
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent() // crates/
            .and_then(|p| p.parent()) // workspace root
            .ok_or_else(|| AosError::Database("Failed to find workspace root".to_string()))?;

        let migrations_path = workspace_root.join("migrations");

        // Verify migrations directory exists
        if !migrations_path.exists() {
            return Err(AosError::Database(format!(
                "Migrations directory not found: {}",
                migrations_path.display()
            ))
            .into());
        }

        // CRITICAL: Verify all migration signatures before applying
        info!("Verifying migration signatures...");
        let verifier = crate::migration_verify::MigrationVerifier::new(&migrations_path)?;
        verifier.verify_all()?;
        info!(
            "✓ All {} migration signatures verified (fingerprint: {})",
            verifier.signature_count(),
            verifier.public_key_fingerprint()
        );

        // Use sqlx::migrate with dynamic path (PathBuf implements MigrationSource)
        let migrator = sqlx::migrate::Migrator::new(migrations_path.clone())
            .await
            .map_err(|e| AosError::Database(format!("Failed to create migrator: {}", e)))?;

        // Run migrations
        info!("Applying database migrations...");
        migrator
            .run(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Migration failed: {}", e)))?;

        // Verify database version after migration
        self.verify_migration_version(&migrations_path).await?;

        Ok(())
    }

    /// Verify database is at the expected migration version
    ///
    /// Per PRD-05: Fail fast with clear error if schema version doesn't match expected.
    /// Prevents version drift where code expects newer schema than DB has.
    ///
    /// **Critical:** This method now FAILS if database version != expected version.
    /// Use `aosctl db reset` (dev only) to recreate database with all migrations.
    pub async fn verify_migration_version(&self, migrations_path: &std::path::Path) -> Result<()> {
        use tracing::{error, info, warn};

        // Get latest migration version from database
        let latest_db_migration: Option<(i64, String)> = sqlx::query_as(
            "SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to query migration version: {}", e)))?;

        // Count migration files to determine expected version
        let migration_files: Vec<_> = std::fs::read_dir(migrations_path)
            .map_err(|e| AosError::Database(format!("Failed to read migrations directory: {}", e)))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("sql"))
            .collect();

        let expected_version = migration_files.len() as i64;

        match latest_db_migration {
            Some((version, description)) => {
                info!(
                    "Database at migration version {} ({}) - expected version {}",
                    version, description, expected_version
                );

                // PRD-05: FAIL FAST if version mismatch
                if version != expected_version {
                    error!(
                        "❌ SCHEMA VERSION MISMATCH: Database at version {}, expected {}",
                        version, expected_version
                    );
                    error!("Migration files count: {}", expected_version);
                    error!("Database has {} migrations applied", version);

                    if version < expected_version {
                        error!("Database is BEHIND - {} migrations missing", expected_version - version);
                        error!("Run migrations: aosctl db migrate");
                    } else {
                        error!("Database is AHEAD - code expects older schema");
                        error!("This may indicate migration file removal or code rollback");
                    }

                    return Err(AosError::Database(format!(
                        "Schema version mismatch: DB version {} != expected {}. Server cannot start with mismatched schema.",
                        version, expected_version
                    )).into());
                }

                info!("✓ Schema version verified: {}", version);
            }
            None => {
                if expected_version > 0 {
                    error!(
                        "❌ Database has NO migrations applied but {} migration files exist",
                        expected_version
                    );
                    error!("Run migrations: aosctl db migrate");
                    return Err(AosError::Database(format!(
                        "Database has no migrations applied but {} migration files exist. Run migrations first.",
                        expected_version
                    )).into());
                }
                warn!("No migrations applied yet (empty database)");
            }
        }

        Ok(())
    }

    /// Recover from system crash or unexpected shutdown
    ///
    /// Scans for orphaned adapters and inconsistent state, then cleans up:
    /// 1. Marks adapters stuck in loading state as unloaded
    /// 2. Resets invalid activation percentages
    /// 3. Logs recovery actions for audit trail
    ///
    /// Should be called after migrations but before handling requests.
    pub async fn recover_from_crash(&self) -> Result<()> {
        use chrono::Utc;
        use tracing::{info, warn};

        info!("Starting crash recovery scan...");

        let mut recovery_actions = Vec::new();

        // 1. Find adapters stuck in "loading" state (orphaned from crash)
        let stale_adapters: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT adapter_id, name, load_state
            FROM adapters
            WHERE load_state = 'loading'
              AND last_loaded_at < datetime('now', '-5 minutes')
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to query stale adapters: {}", e)))?;

        if !stale_adapters.is_empty() {
            warn!(
                "Found {} orphaned adapters stuck in loading state",
                stale_adapters.len()
            );

            for (adapter_id, name, load_state) in stale_adapters {
                recovery_actions.push(format!(
                    "Adapter {} ({}) stuck in state '{}' - marking as unloaded",
                    name, adapter_id, load_state
                ));

                // Mark as unloaded in database
                sqlx::query(
                    "UPDATE adapters SET load_state = 'unloaded', updated_at = datetime('now') WHERE adapter_id = ?",
                )
                .bind(&adapter_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to update adapter state: {}", e)))?;

                info!("✓ Recovered adapter: {} ({})", name, adapter_id);
            }
        }

        // 2. Clean up invalid activation percentages
        let reset_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM adapters WHERE activation_pct > 1.0 OR activation_pct < 0.0",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to query invalid activation_pct: {}", e))
        })?;

        if reset_count > 0 {
            warn!(
                "Found {} adapters with invalid activation_pct - resetting",
                reset_count
            );

            sqlx::query(
                "UPDATE adapters SET activation_pct = 0.0 WHERE activation_pct > 1.0 OR activation_pct < 0.0",
            )
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to reset activation_pct: {}", e)))?;

            recovery_actions.push(format!(
                "Reset {} adapters with invalid activation percentages",
                reset_count
            ));
        }

        // 3. Log summary
        if recovery_actions.is_empty() {
            info!("✓ Crash recovery complete - no issues detected");
        } else {
            info!(
                "✓ Crash recovery complete - {} actions taken:",
                recovery_actions.len()
            );
            for action in &recovery_actions {
                info!("  - {}", action);
            }

            // Log to audit trail if available
            let audit_log = serde_json::json!({
                "action": "crash_recovery",
                "actions_taken": recovery_actions.len(),
                "recovery_actions": recovery_actions,
                "timestamp": Utc::now().to_rfc3339()
            });
            tracing::debug!("Crash recovery audit: {}", audit_log);
        }

        Ok(())
    }

    /// Recover stale adapters based on heartbeat timeout
    ///
    /// Finds adapters that haven't sent a heartbeat within threshold_seconds
    /// and resets their state to unloaded. This is called periodically from
    /// a background task in the server to detect crashed/frozen adapters.
    ///
    /// Per Agent G Stability Reinforcement Plan Phase 2: Heartbeat Mechanism
    pub async fn recover_stale_adapters(&self, threshold_seconds: i64) -> Result<Vec<String>> {
        use chrono::Utc;
        use tracing::{info, warn};

        let cutoff_timestamp = Utc::now().timestamp() - threshold_seconds;

        // Find adapters with stale heartbeats
        let stale_adapters: Vec<(String, String, Option<i64>)> = sqlx::query_as(
            r#"
            SELECT adapter_id, name, last_heartbeat
            FROM adapters
            WHERE last_heartbeat IS NOT NULL
              AND last_heartbeat < ?
              AND load_state != 'unloaded'
            "#,
        )
        .bind(cutoff_timestamp)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to query stale adapters: {}", e)))?;

        let mut recovered_ids = Vec::new();

        if !stale_adapters.is_empty() {
            warn!(
                "Found {} adapters with stale heartbeats (threshold: {}s)",
                stale_adapters.len(),
                threshold_seconds
            );

            for (adapter_id, name, last_heartbeat) in stale_adapters {
                let seconds_since = last_heartbeat
                    .map(|ts| Utc::now().timestamp() - ts)
                    .unwrap_or(-1);

                info!(
                    "Recovering stale adapter: {} ({}) - last heartbeat: {}s ago",
                    name, adapter_id, seconds_since
                );

                // Reset state to unloaded and clear heartbeat
                sqlx::query(
                    r#"
                    UPDATE adapters
                    SET load_state = 'unloaded',
                        last_heartbeat = NULL,
                        updated_at = datetime('now')
                    WHERE adapter_id = ?
                    "#,
                )
                .bind(&adapter_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to reset stale adapter: {}", e)))?;

                recovered_ids.push(adapter_id);
            }

            info!(
                "✓ Recovered {} stale adapters based on heartbeat timeout",
                recovered_ids.len()
            );
        }

        Ok(recovered_ids)
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

        // Create sample nodes
        let nodes = vec![
            ("node-01", "m1-max-01.local", "M1 Max", 64),
            ("node-02", "m2-ultra-01.local", "M2 Ultra", 128),
            ("node-03", "m3-max-01.local", "M3 Max", 96),
        ];

        for (id, hostname, family, memory) in nodes {
            sqlx::query(
                "INSERT INTO nodes (id, tenant_id, hostname, metal_family, memory_gb, status, last_heartbeat)
                 VALUES (?, 'default', ?, ?, ?, 'online', datetime('now'))"
            )
            .bind(id)
            .bind(hostname)
            .bind(family)
            .bind(memory)
            .execute(&self.pool)
            .await?;
        }

        tracing::info!("Development data seeded successfully");
        Ok(())
    }

    /// Get adapter by ID and tenant
    pub async fn get_adapter_by_id(
        &self,
        tenant_id: &str,
        adapter_id: &str,
    ) -> Result<Option<AdapterRecord>> {
        let row = sqlx::query_as::<_, AdapterRecord>(
            r#"
            SELECT id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework, active, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, version, lifecycle_state
            FROM adapters
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(adapter_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch adapter: {}", e)))?;

        Ok(row)
    }

    /// List stacks for a tenant
    pub async fn list_stacks_for_tenant(&self, tenant_id: &str) -> Result<Vec<StackRecord>> {
        let rows = sqlx::query_as::<_, StackRecord>(
            r#"
            SELECT id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, created_at, updated_at, created_by
            FROM adapter_stacks
            WHERE tenant_id = ?
            ORDER BY created_at DESC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list stacks: {}", e)))?;

        Ok(rows)
    }

    /// Get a stack by ID and tenant
    pub async fn get_stack(&self, tenant_id: &str, id: &str) -> Result<Option<StackRecord>> {
        let row = sqlx::query_as::<_, StackRecord>(
            r#"
            SELECT id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, created_at, updated_at, created_by
            FROM adapter_stacks
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to fetch stack: {}", e)))?;

        Ok(row)
    }

    /// Delete a stack by ID and tenant
    pub async fn delete_stack(&self, tenant_id: &str, id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            DELETE FROM adapter_stacks
            WHERE tenant_id = ? AND id = ?
            "#,
        )
        .bind(tenant_id)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to delete stack: {}", e)))?;

        Ok(result.rows_affected() > 0)
    }

    /// Update a stack
    pub async fn update_stack(&self, id: &str, stack: &CreateStackRequest) -> Result<bool> {
        let adapter_ids_json =
            serde_json::to_string(&stack.adapter_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type_str = stack.workflow_type.as_ref().map(|w| format!("{:?}", w));

        let result = sqlx::query(
            r#"
            UPDATE adapter_stacks
            SET name = ?, description = ?, adapter_ids_json = ?, workflow_type = ?, updated_at = datetime('now')
            WHERE id = ?
            "#,
        )
        .bind(&stack.name)
        .bind(&stack.description)
        .bind(&adapter_ids_json)
        .bind(&workflow_type_str)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to update stack: {}", e)))?;

        Ok(result.rows_affected() > 0)
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
pub mod audit;
pub use audit::AuditLog;
pub mod audits;
pub mod metadata;
pub use metadata::{AdapterMeta, AdapterStackMeta, LifecycleState, ForkType, WorkflowType, API_SCHEMA_VERSION};
pub mod migration_verify;
pub mod unified_access;
pub mod validation;
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
pub mod plugin_enables;
pub mod policies;
pub mod policy_hash;
pub mod tenants;
pub use policy_hash::PolicyHashRecord;
pub mod process_monitoring;
pub mod replay_sessions;
pub mod repositories;
pub mod routing_decisions;
pub use routing_decisions::{RoutingDecision, RoutingDecisionFilters, RouterCandidate};
pub mod routing_telemetry_bridge;
pub use routing_telemetry_bridge::{event_to_decision, persist_router_decisions};
pub mod telemetry_bundles;

// Re-export unified access types
pub use unified_access::{
    ConnectionInfo, DatabaseAccess, DatabaseStatistics, DatabaseType, HealthState, HealthStatus,
    SqlParameter, ToSql, Transaction, UnifiedDatabaseAccess, UnifiedTransaction,
};
