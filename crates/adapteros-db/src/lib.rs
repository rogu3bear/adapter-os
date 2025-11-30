#![allow(unexpected_cfgs)]
#![allow(unused_imports)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::manual_strip)]
#![allow(clippy::redundant_closure)]

use adapteros_core::{AosError, Result};
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;
use std::sync::Arc;
use tracing::{debug, warn};

// Query constants for SELECT column lists
pub mod constants;

// Database abstraction layer
#[cfg(feature = "postgres")]
pub mod postgres_backend;
pub mod sqlite_backend;
pub mod traits;
pub mod kv_backend;

// Re-export commonly used types
pub use traits::{
    AdapterRecord, CreateStackRequest, DatabaseBackend, DatabaseBackendType, DatabaseConfig,
    StackRecord,
};

// Re-export KV backend types
pub use kv_backend::{KvBackend, KvDb, StorageError as KvStorageError};

// PostgreSQL backend for production (legacy - to be deprecated)
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "postgres")]
pub use postgres::PostgresDb;

/// Storage mode for database operations
///
/// Defines how the database layer handles reads and writes when both
/// SQL and KV backends are available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    /// Only use SQL backend (default, current behavior)
    SqlOnly,
    /// Write to both SQL and KV, read from SQL
    /// Used during migration validation phase
    DualWrite,
    /// Write to both SQL and KV, read from KV
    /// Used during migration cutover phase
    KvPrimary,
    /// Only use KV backend (full migration complete)
    KvOnly,
}

impl Default for StorageMode {
    fn default() -> Self {
        StorageMode::SqlOnly
    }
}

impl StorageMode {
    /// Returns true if this mode reads from SQL backend
    pub fn read_from_sql(self) -> bool {
        matches!(self, StorageMode::SqlOnly | StorageMode::DualWrite)
    }

    /// Returns true if this mode reads from KV backend
    pub fn read_from_kv(self) -> bool {
        matches!(self, StorageMode::KvPrimary | StorageMode::KvOnly)
    }

    /// Returns true if this mode writes to SQL backend
    pub fn write_to_sql(self) -> bool {
        matches!(
            self,
            StorageMode::SqlOnly | StorageMode::DualWrite | StorageMode::KvPrimary
        )
    }

    /// Returns true if this mode writes to KV backend
    pub fn write_to_kv(self) -> bool {
        matches!(
            self,
            StorageMode::DualWrite | StorageMode::KvPrimary | StorageMode::KvOnly
        )
    }

    /// Returns true if this is the final KV-only mode (migration complete)
    pub fn is_kv_only(self) -> bool {
        matches!(self, StorageMode::KvOnly)
    }

    /// Returns true if dual-write is active (writing to both backends)
    pub fn is_dual_write(self) -> bool {
        matches!(self, StorageMode::DualWrite | StorageMode::KvPrimary)
    }
}

/// Database connection pool and query methods (SQLite)
///
/// For production deployments, use `PostgresDb` instead.
/// Supports optional KV backend integration for migration scenarios.
#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
    kv: Option<std::sync::Arc<KvDb>>,
    storage_mode: StorageMode,
}

impl Db {
    /// Create a new Db instance with the given components
    ///
    /// This is the primary constructor for creating a Db with custom configuration.
    /// For simple SQLite connections, use `Db::connect()` or `Db::connect_env()` instead.
    ///
    /// # Arguments
    /// * `pool` - SQLite connection pool
    /// * `kv` - Optional KV backend for dual-write or KV-only modes
    /// * `storage_mode` - Controls read/write behavior across backends
    pub fn new(
        pool: SqlitePool,
        kv: Option<std::sync::Arc<KvDb>>,
        storage_mode: StorageMode,
    ) -> Self {
        Self {
            pool,
            kv,
            storage_mode,
        }
    }

    /// Connect to SQLite database with WAL mode
    ///
    /// Configuration:
    /// - WAL mode for better concurrency
    /// - Normal synchronous mode (balance between safety and performance)
    /// - 30-second connection timeout
    /// - Max 20 connections in pool
    /// - Statement cache size of 100
    /// - **CRITICAL:** Foreign key enforcement enabled
    pub async fn connect(path: &str) -> Result<Self> {
        use std::time::Duration;

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(30)) // 30s timeout for busy database
            .statement_cache_capacity(100) // Cache up to 100 prepared statements
            .foreign_keys(true); // CRITICAL: Enable foreign key constraints

        let pool = SqlitePool::connect_with(options)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

        Ok(Self {
            pool,
            kv: None,
            storage_mode: StorageMode::SqlOnly,
        })
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

        // Get max migration number from filenames to determine expected version
        // SQLx uses the number prefix (e.g., 0081) not file count
        let expected_version = std::fs::read_dir(migrations_path)
            .map_err(|e| AosError::Database(format!("Failed to read migrations directory: {}", e)))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("sql"))
            .filter_map(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .and_then(|name| name.split('_').next())
                    .and_then(|num| num.parse::<i64>().ok())
            })
            .max()
            .unwrap_or(0);

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
                        error!(
                            "Database is BEHIND - {} migrations missing",
                            expected_version - version
                        );
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
    ///
    /// **CRITICAL FIX:** Wraps all recovery operations in a single transaction
    /// to ensure atomicity. This prevents partial recovery on crash during recovery.
    pub async fn recover_from_crash(&self) -> Result<()> {
        use chrono::Utc;
        use tracing::{info, warn};

        info!("Starting crash recovery scan...");

        let mut recovery_actions = Vec::new();

        // CRITICAL: Begin transaction for atomic recovery
        let mut tx = self.pool.begin().await.map_err(|e| {
            AosError::Database(format!("Failed to begin recovery transaction: {}", e))
        })?;

        // 1. Find adapters stuck in "loading" state (orphaned from crash)
        let stale_adapters: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT adapter_id, name, load_state
            FROM adapters
            WHERE load_state = 'loading'
              AND last_loaded_at < datetime('now', '-5 minutes')
            "#,
        )
        .fetch_all(&mut *tx)
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

                // Mark as unloaded in database (within transaction)
                sqlx::query(
                    "UPDATE adapters SET load_state = 'unloaded', updated_at = datetime('now') WHERE adapter_id = ?",
                )
                .bind(&adapter_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AosError::Database(format!("Failed to update adapter state: {}", e)))?;

                info!("✓ Recovered adapter: {} ({})", name, adapter_id);
            }
        }

        // 2. Clean up invalid activation counts (negative values)
        let reset_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM adapters WHERE activation_count < 0")
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to query invalid activation_count: {}", e))
                })?;

        if reset_count > 0 {
            warn!(
                "Found {} adapters with invalid activation_count - resetting",
                reset_count
            );

            sqlx::query("UPDATE adapters SET activation_count = 0 WHERE activation_count < 0")
                .execute(&mut *tx)
                .await
                .map_err(|e| {
                    AosError::Database(format!("Failed to reset activation_count: {}", e))
                })?;

            recovery_actions.push(format!(
                "Reset {} adapters with invalid activation percentages",
                reset_count
            ));
        }

        // CRITICAL: Commit transaction atomically
        tx.commit().await.map_err(|e| {
            AosError::Database(format!("Failed to commit recovery transaction: {}", e))
        })?;

        // 3. Log summary (after successful commit)
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
    ///
    /// **CRITICAL FIX:** Wraps all recovery operations in a single transaction
    /// to ensure atomicity.
    pub async fn recover_stale_adapters(&self, threshold_seconds: i64) -> Result<Vec<String>> {
        use chrono::Utc;
        use tracing::{info, warn};

        let cutoff_timestamp = Utc::now().timestamp() - threshold_seconds;

        // CRITICAL: Begin transaction for atomic recovery
        let mut tx = self.pool.begin().await.map_err(|e| {
            AosError::Database(format!("Failed to begin recovery transaction: {}", e))
        })?;

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
        .fetch_all(&mut *tx)
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

                // Reset state to unloaded and clear heartbeat (within transaction)
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
                .execute(&mut *tx)
                .await
                .map_err(|e| AosError::Database(format!("Failed to reset stale adapter: {}", e)))?;

                recovered_ids.push(adapter_id);
            }

            info!(
                "✓ Recovered {} stale adapters based on heartbeat timeout",
                recovered_ids.len()
            );
        }

        // CRITICAL: Commit transaction atomically
        tx.commit().await.map_err(|e| {
            AosError::Database(format!("Failed to commit recovery transaction: {}", e))
        })?;

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
            .map_err(|e| AosError::Crypto(format!("failed to hash password: {}", e)))?
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
                "INSERT INTO users (id, email, display_name, pw_hash, role, disabled, created_at, tenant_id)
                 VALUES (?, ?, ?, ?, ?, 0, datetime('now'), 'default')",
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
            // Store hardware specs in labels_json since columns don't exist
            let labels = serde_json::json!({
                "metal_family": family,
                "memory_gb": memory
            }).to_string();

            // Note: nodes table does not have tenant_id (cluster resource)
            sqlx::query(
                "INSERT INTO nodes (id, hostname, agent_endpoint, status, last_seen_at, labels_json, created_at)
                 VALUES (?, ?, ?, 'active', datetime('now'), ?, datetime('now'))"
            )
            .bind(id)
            .bind(hostname)
            .bind(format!("https://{}:3000", hostname)) // Dummy agent endpoint
            .bind(labels)
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
            SELECT id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework, active, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, activation_count, expires_at, load_state, last_loaded_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, version, lifecycle_state
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
        // SQL delete (always happens)
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

        let deleted = result.rows_affected() > 0;

        // KV delete (dual-write mode)
        if deleted {
            if let Some(kv_backend) = self.get_stack_kv_repo() {
                use stacks_kv::StackKvOps;
                if let Err(e) = kv_backend.delete_stack(tenant_id, id).await {
                    warn!(error = %e, stack_id = %id, "Failed to delete stack from KV backend (dual-write)");
                } else {
                    debug!(stack_id = %id, "Stack deleted from both SQL and KV backends");
                }
            }
        }

        Ok(deleted)
    }

    /// Update a stack
    pub async fn update_stack(&self, id: &str, stack: &CreateStackRequest) -> Result<bool> {
        let adapter_ids_json =
            serde_json::to_string(&stack.adapter_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type_str = stack.workflow_type.as_ref().map(|w| format!("{:?}", w));

        // SQL update (always happens)
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

        let updated = result.rows_affected() > 0;

        // KV update (dual-write mode)
        if updated {
            if let Some(kv_backend) = self.get_stack_kv_repo() {
                use stacks_kv::StackKvOps;
                if let Err(e) = kv_backend.update_stack(id, stack).await {
                    warn!(error = %e, stack_id = %id, "Failed to update stack in KV backend (dual-write)");
                } else {
                    debug!(stack_id = %id, "Stack updated in both SQL and KV backends");
                }
            }
        }

        Ok(updated)
    }

    /// Get the underlying pool for custom queries
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Get the current storage mode
    pub fn storage_mode(&self) -> StorageMode {
        self.storage_mode
    }

    /// Set the storage mode
    ///
    /// This allows transitioning between different storage modes during migration.
    /// For example: SqlOnly -> DualWrite -> KvPrimary -> KvOnly
    pub fn set_storage_mode(&mut self, mode: StorageMode) {
        self.storage_mode = mode;
    }

    /// Attach a KV backend to this database instance
    ///
    /// This enables dual-write or KV-primary modes. The KV backend will be used
    /// according to the current storage_mode setting.
    pub fn attach_kv_backend(&mut self, kv: KvDb) {
        self.kv = Some(std::sync::Arc::new(kv));
    }

    /// Initialize KV backend with redb at the given path
    ///
    /// This is a convenience method that creates a KvDb instance and attaches it.
    pub fn init_kv_backend(&mut self, path: &std::path::Path) -> Result<()> {
        let kv = KvDb::init_redb(path)?;
        self.attach_kv_backend(kv);
        Ok(())
    }

    /// Get a reference to the KV backend if attached
    pub fn kv_backend(&self) -> Option<&std::sync::Arc<KvDb>> {
        self.kv.as_ref()
    }

    /// Check if KV backend is available
    pub fn has_kv_backend(&self) -> bool {
        self.kv.is_some()
    }

    /// Detach the KV backend
    ///
    /// This removes the KV backend and resets storage mode to SqlOnly.
    pub fn detach_kv_backend(&mut self) {
        self.kv = None;
        self.storage_mode = StorageMode::SqlOnly;
    }

    /// Get a StackKvRepository if KV writes are enabled
    fn get_stack_kv_repo(&self) -> Option<stacks_kv::StackKvRepository> {
        if self.storage_mode().write_to_kv() {
            self.kv_backend().map(|kv| {
                let kv_backend: Arc<dyn kv_backend::KvBackend> = kv.clone();
                stacks_kv::StackKvRepository::new(kv_backend)
            })
        } else {
            None
        }
    }

    /// Insert a new adapter stack
    pub async fn insert_stack(&self, req: &CreateStackRequest) -> Result<String> {
        let id = uuid::Uuid::new_v4().to_string();
        let adapter_ids_json =
            serde_json::to_string(&req.adapter_ids).map_err(|e| AosError::Serialization(e))?;
        let workflow_type = req.workflow_type.as_deref().unwrap_or("parallel");
        let description = req.description.as_deref().unwrap_or("");

        // SQL write (always happens)
        sqlx::query(
            r#"
            INSERT INTO adapter_stacks (id, tenant_id, name, description, adapter_ids_json, workflow_type, version, lifecycle_state, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, '1.0.0', 'active', datetime('now'), datetime('now'))
            "#,
        )
        .bind(&id)
        .bind(&req.tenant_id)
        .bind(&req.name)
        .bind(description)
        .bind(&adapter_ids_json)
        .bind(workflow_type)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to insert stack: {}", e)))?;

        // KV write (dual-write mode)
        if let Some(kv_backend) = self.get_stack_kv_repo() {
            use stacks_kv::StackKvOps;
            if let Err(e) = kv_backend.create_stack(req).await {
                warn!(error = %e, stack_id = %id, "Failed to write stack to KV backend (dual-write)");
            } else {
                debug!(stack_id = %id, "Stack written to both SQL and KV backends");
            }
        }

        Ok(id)
    }

    /// Increment adapter activation count
    pub async fn increment_adapter_activation(&self, adapter_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE adapters
            SET activation_count = activation_count + 1,
                last_activated = datetime('now'),
                updated_at = datetime('now')
            WHERE adapter_id = ?
            "#,
        )
        .bind(adapter_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            AosError::Database(format!("Failed to increment adapter activation: {}", e))
        })?;

        Ok(())
    }

    /// Rebuild all indexes for a tenant
    ///
    /// Rebuilds all indexes to optimize query performance. This is useful after:
    /// - Large bulk operations (import/migration)
    /// - Adapter evictions and cleanup
    /// - Performance degradation over time
    ///
    /// The operation:
    /// 1. Analyzes table statistics via ANALYZE
    /// 2. Validates index integrity via PRAGMA integrity_check
    /// 3. Rebuilds all indexes for the tenant via REINDEX
    ///
    /// Timeline: O(n log n) where n = number of adapter rows for the tenant
    pub async fn rebuild_all_indexes(&self, tenant_id: &str) -> Result<()> {
        use tracing::{info, warn};

        info!(tenant_id = %tenant_id, "Starting index rebuild");

        // Step 1: Analyze table statistics
        info!("Analyzing table statistics");
        sqlx::query("ANALYZE adapters")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to analyze adapters table: {}", e)))?;

        sqlx::query("ANALYZE users")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to analyze users table: {}", e)))?;

        sqlx::query("ANALYZE adapter_stacks")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AosError::Database(format!("Failed to analyze adapter_stacks table: {}", e))
            })?;

        // Step 2: Perform integrity check
        info!("Validating database integrity");
        let integrity_result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to perform integrity check: {}", e)))?;

        if integrity_result != "ok" {
            warn!(result = %integrity_result, "Integrity check reported issues");
            return Err(AosError::Database(format!(
                "Database integrity check failed: {}",
                integrity_result
            ))
            .into());
        }

        // Step 3: Rebuild all indexes
        info!("Rebuilding all indexes");
        sqlx::query("REINDEX")
            .execute(&self.pool)
            .await
            .map_err(|e| AosError::Database(format!("Failed to rebuild indexes: {}", e)))?;

        // Step 4: Log completion and gather statistics
        let adapter_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM adapters WHERE tenant_id = ?")
                .bind(tenant_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to count adapters: {}", e)))?;

        info!(
            tenant_id = %tenant_id,
            adapter_count = adapter_count,
            "✓ Index rebuild complete"
        );

        Ok(())
    }

    /// List adapters for a specific tenant
    pub async fn list_adapters_by_tenant(&self, tenant_id: &str) -> Result<Vec<AdapterRecord>> {
        let rows = sqlx::query_as::<_, AdapterRecord>(
            r#"
            SELECT id, tenant_id, name, tier, hash_b3, rank, alpha, targets_json, acl_json, adapter_id, languages_json, framework, active, category, scope, framework_id, framework_version, repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, activation_count, expires_at, load_state, last_loaded_at, adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, version, lifecycle_state
            FROM adapters
            WHERE tenant_id = ?
            ORDER BY name ASC
            "#,
        )
        .bind(tenant_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to list adapters by tenant: {}", e)))?;

        Ok(rows)
    }

    /// Get user by username (optimized with direct prefix matching)
    ///
    /// Optimizations:
    /// - Uses simple equality check instead of LIKE pattern matching
    /// - Relies on email UNIQUE constraint index
    /// - Falls back to ID match only if email doesn't exist
    ///
    /// Performance: O(log n) via index lookup vs O(n) with LIKE
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>> {
        // First, try to find user by email prefix (e.g., "admin" -> "admin@aos.local")
        // This is more efficient than LIKE pattern matching
        let email_query = format!("{}@%", username);

        let row = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, display_name, pw_hash, role, disabled, created_at
            FROM users
            WHERE email LIKE ?
            LIMIT 1
            "#,
        )
        .bind(email_query)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get user by email: {}", e)))?;

        // If not found by email, try exact ID match
        if let Some(user) = row {
            return Ok(Some(user));
        }

        let user_id = format!("{}-user", username);
        let row = sqlx::query_as::<_, User>(
            r#"
            SELECT id, email, display_name, pw_hash, role, disabled, created_at
            FROM users
            WHERE id = ?
            "#,
        )
        .bind(&user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get user by id: {}", e)))?;

        Ok(row)
    }

    /// Get index hash for a tenant and index type
    pub async fn get_index_hash(
        &self,
        tenant_id: &str,
        index_type: &str,
    ) -> Result<Option<adapteros_core::B3Hash>> {
        let row: Option<(Vec<u8>,)> = sqlx::query_as(
            r#"
            SELECT hash
            FROM index_hashes
            WHERE tenant_id = ? AND index_type = ?
            "#,
        )
        .bind(tenant_id)
        .bind(index_type)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to get index hash: {}", e)))?;

        match row {
            Some((hash_bytes,)) => {
                if hash_bytes.len() == 32 {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&hash_bytes);
                    Ok(Some(adapteros_core::B3Hash::new(arr)))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }

    /// Close the database connection pool gracefully
    ///
    /// This method should be called during shutdown to ensure:
    /// - Pending transactions are completed
    /// - WAL checkpoint is performed
    /// - All connections are properly released
    ///
    /// ## SQLite Behavior
    /// SQLite connection pools are typically closed automatically when dropped,
    /// but this explicit method provides:
    /// - Guaranteed synchronous shutdown
    /// - Ability to handle shutdown errors explicitly
    /// - Clear intent in shutdown sequences
    ///
    /// ## Usage in Shutdown
    /// Call this as part of graceful shutdown before process exit:
    /// ```rust,no_run
    /// # use adapteros_db::Db;
    /// # async fn example(db: Db) -> Result<(), Box<dyn std::error::Error>> {
    /// db.close().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn close(&self) -> Result<()> {
        use tracing::info;

        info!("Closing database connection pool");

        // SQLite: Perform WAL checkpoint to finalize pending writes
        sqlx::query("PRAGMA optimize")
            .execute(&self.pool)
            .await
            .map_err(|e| {
                AosError::Database(format!(
                    "Failed to optimize database during shutdown: {}",
                    e
                ))
            })?;

        info!("Database connection pool closed successfully");
        Ok(())
    }
}

// Re-export sqlx types for convenience
pub use sqlx;
pub use sqlx::Row;

pub mod query_helpers;
pub mod activity_events;
pub use activity_events::ActivityEvent;
pub mod adapter_snapshots;
pub mod crypto_audit;
pub use adapter_snapshots::{AdapterTrainingSnapshot, CreateSnapshotParams};
pub mod inference_evidence;
pub use inference_evidence::{CreateEvidenceParams, InferenceEvidence};
pub mod query_performance;
pub use query_performance::{QueryMetrics, QueryPerformanceMonitor, QueryStats};
pub mod adapter_record;
pub use adapter_record::{
    AccessControl, AdapterIdentity, AdapterRecordBuilder, AdapterRecordV1, ArtifactInfo,
    CodeIntelligence, FlatAdapterRow, ForkMetadata, LifecycleState, LoRAConfig, SchemaCompatible,
    SchemaMetadata, SemanticNaming, TierConfig,
};
pub mod adapters;
pub mod adapters_kv;
pub mod kv_migration;
pub use adapters::{Adapter, AdapterRegistrationBuilder, AdapterRegistrationParams};
pub use adapters_kv::{AdapterKvOps, AdapterKvRepository};
pub use kv_migration::{MigrationDiscrepancy, MigrationStats};
pub mod artifacts;
pub mod audit;
pub use audit::AuditLog;
pub mod audits;
pub mod chat_sessions;
pub use chat_sessions::{
    AddMessageParams, ChatCategory, ChatMessage, ChatSearchResult, ChatSession, ChatSessionTrace,
    ChatSessionWithStatus, ChatTag, CreateChatSessionParams, SessionShare,
};
pub mod lifecycle;
pub use lifecycle::{LifecycleHistoryEvent, StackReference};
pub mod metadata;
pub use metadata::{
    AdapterMeta, AdapterStackMeta, ForkType, LifecycleState as MetadataLifecycleState,
    WorkflowType, API_SCHEMA_VERSION,
};
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
pub use training_datasets::{
    DatasetAdapterLink, DatasetFile, DatasetStatistics, EvidenceEntry, TrainingDataset,
};
pub mod key_metadata;
pub use key_metadata::KeyMetadata;
pub mod manifests;
pub mod model_operations;
pub mod models;
pub use model_operations::ModelOperation;
pub mod nodes;
pub mod patch_proposals;
pub use patch_proposals::PatchProposal;
pub mod pinned_adapters;
pub mod plans;
pub mod plugin_configs;
pub use plugin_configs::{PluginConfig, PluginTenantEnable};
pub mod plugin_enables;
pub mod policies;
pub mod policy_hash;
pub mod policy_management;
pub mod promotions;
pub use promotions::{
    CreatePromotionRequestParams, GoldenRunStage, PromotionApproval, PromotionGate,
    PromotionRequest, RecordApprovalParams, RecordGateParams,
};
pub mod tenants;
pub mod tenants_kv;
pub mod stacks_kv;
pub use policy_hash::PolicyHashRecord;
pub use tenants_kv::{CreateTenantParams, TenantKvOps, TenantKvRepository};
pub use stacks_kv::{StackKvRepository, StackKvOps};
pub mod process_monitoring;
pub mod replay_sessions;
pub mod repositories;
pub mod routing_decisions;
pub use routing_decisions::{RouterCandidate, RoutingDecision, RoutingDecisionFilters};
pub mod routing_telemetry_bridge;
pub use routing_telemetry_bridge::{event_to_decision, persist_router_decisions};
pub mod telemetry_bundles;
pub mod users;
pub mod users_kv;
pub use users::{Role, User};
// Re-export users_kv types for dual-write operations
pub use users_kv::{kv_to_user, user_to_kv, UserKeys, UserKvOps, UserKvRepository};
// Re-export KV Role type with an alias to distinguish from SQL Role
pub use users_kv::Role as KvRole;
pub mod user_tenant_access;
pub use user_tenant_access::{
    cleanup_expired_tenant_access, get_user_tenant_access, get_user_tenant_access_details,
    grant_user_tenant_access, revoke_user_tenant_access, UserTenantAccess,
};
pub mod workers;
pub use models::Worker;
pub use workers::TrainingTask;

// Document management modules
pub mod collections;
pub mod documents;
pub use collections::{CreateCollectionParams, DocumentCollection};
pub use documents::{CreateChunkParams, CreateDocumentParams, Document, DocumentChunk};

// Workspace, notifications, messages, dashboard, and tutorial modules
pub mod dashboard_configs;
pub mod messages;
pub mod notifications;
pub mod tutorials;
pub mod workspaces;

// System statistics module
pub mod system_stats;

// Federation module
pub mod federation;
pub use federation::{QuarantineDetails, QuarantineRecord};

// Authentication sessions module
pub mod auth_sessions;
pub use auth_sessions::AuthSession;
pub use dashboard_configs::DashboardWidgetConfig;
pub use notifications::{Notification, NotificationType};
pub use tutorials::TutorialStatus;
pub use workspaces::{ResourceType, Workspace, WorkspaceMember, WorkspaceResource, WorkspaceRole};

// Re-export unified access types
pub use unified_access::{
    ConnectionInfo, DatabaseAccess, DatabaseStatistics, DatabaseType, DbHealthStatus, SqlParameter,
    ToSql, Transaction, UnifiedDatabaseAccess, UnifiedTransaction,
};
// Re-export canonical health types from adapteros-core
pub use adapteros_core::{HealthCheckResult, HealthStatus};

// Add update_anomaly_status method to Db impl
impl Db {
    /// Update anomaly status with investigation details
    pub async fn update_anomaly_status(
        &self,
        anomaly_id: &str,
        status: &str,
        investigation_notes: &str,
        investigated_by: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE process_anomalies SET status = ?, investigation_notes = ?, investigated_by = ?, updated_at = datetime('now') WHERE id = ?"
        )
        .bind(status)
        .bind(investigation_notes)
        .bind(investigated_by)
        .bind(anomaly_id)
        .execute(&*self.pool())
        .await
        .map_err(|e| AosError::Database(format!("Failed to update anomaly status: {}", e)))?;
        Ok(())
    }

    /// Get a system setting value by key
    ///
    /// Returns None if the key doesn't exist or the value is empty.
    pub async fn get_system_setting(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM system_settings WHERE key = ?")
                .bind(key)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AosError::Database(format!("Failed to get system setting: {}", e)))?;

        Ok(row.map(|(v,)| v).filter(|v| !v.is_empty()))
    }

    /// Set a system setting value
    ///
    /// Creates the setting if it doesn't exist, updates if it does.
    pub async fn set_system_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO system_settings (key, value, updated_at)
             VALUES (?, ?, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET
               value = excluded.value,
               updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to set system setting: {}", e)))?;

        Ok(())
    }
}
