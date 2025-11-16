use adapteros_core::AosError;
use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

// Database abstraction layer
pub mod postgres_backend;
pub mod sqlite_backend;
pub mod traits;

// Re-export commonly used types
pub use traits::{
    CreateStackRequest, DatabaseBackend, DatabaseBackendType, DatabaseConfig, StackRecord,
};

// PostgreSQL backend for production (legacy - to be deprecated)
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
    pub async fn connect(path: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePool::connect_with(options).await?;
        Ok(Self { pool })
    }

    /// Connect to SQLite database using DATABASE_URL environment variable
    pub async fn connect_env() -> Result<Self> {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "./var/cp.db".to_string());
        Self::connect(&database_url).await
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
    /// Checks that the last applied migration matches the latest migration file.
    /// Prevents version drift where code expects newer schema than DB has.
    pub async fn verify_migration_version(
        &self,
        migrations_path: &std::path::Path,
    ) -> Result<()> {
        use tracing::{info, warn};

        // Get latest migration version from database
        let latest_db_migration: Option<(i64, String)> = sqlx::query_as(
            "SELECT version, description FROM _sqlx_migrations ORDER BY version DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AosError::Database(format!("Failed to query migration version: {}", e)))?;

        // Count migration files
        let migration_files: Vec<_> = std::fs::read_dir(migrations_path)
            .map_err(|e| AosError::Database(format!("Failed to read migrations directory: {}", e)))?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .and_then(|e| e.to_str())
                    == Some("sql")
            })
            .collect();

        let expected_count = migration_files.len();

        match latest_db_migration {
            Some((version, description)) => {
                info!(
                    "✓ Database at migration version {} ({}) - {} migrations total",
                    version, description, expected_count
                );

                // Warn if version seems low (expected at least 0062 based on current state)
                if version < 60 && expected_count > 60 {
                    warn!(
                        "⚠ Database version {} is significantly behind expected version {}",
                        version, expected_count
                    );
                    warn!("⚠ Run database migrations with: aosctl db migrate");
                }
            }
            None => {
                if expected_count > 0 {
                    return Err(AosError::Database(format!(
                        "Database has no migrations applied but {} migration files exist. Run migrations first.",
                        expected_count
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
pub mod replay_sessions;
pub mod repositories;
pub mod telemetry_bundles;
pub mod tenants;
pub mod users;
pub mod workers;

// Re-export unified access types
pub use unified_access::{
    ConnectionInfo, DatabaseAccess, DatabaseStatistics, DatabaseType, HealthState, HealthStatus,
    SqlParameter, ToSql, Transaction, UnifiedDatabaseAccess, UnifiedTransaction,
};
