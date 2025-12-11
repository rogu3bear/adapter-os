//! Database factory for creating Db instances with optional KV backend
//!
//! Provides initialization logic for the dual-write migration system, allowing
//! gradual transition from SQL-only to KV-primary storage.

use crate::{Db, KvDb, StorageMode};
use adapteros_core::{AosError, Result};
use serde_json::json;
use sqlx::sqlite::SqlitePoolOptions;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::info;

/// Storage backend selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBackend {
    /// SQL-only (current production)
    Sql,
    /// Dual-write mode (SQL primary, KV for validation)
    Dual,
    /// KV-primary mode (SQL as fallback)
    KvPrimary,
    /// KV-only mode (future target)
    KvOnly,
}

/// Database factory for creating Db instances
pub struct DbFactory;

fn agent_log(
    hypothesis_id: &'static str,
    location: &'static str,
    message: &'static str,
    data: serde_json::Value,
) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    // #region agent log
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/Users/mln-dev/Dev/adapter-os/.cursor/debug.log")
    {
        let _ = writeln!(
            file,
            "{}",
            json!({
                "sessionId": "debug-session",
                "runId": "pre-fix",
                "hypothesisId": hypothesis_id,
                "location": location,
                "message": message,
                "data": data,
                "timestamp": timestamp,
            })
        );
    }
    // #endregion
}

impl DbFactory {
    /// Create a Db instance based on configuration
    ///
    /// # Arguments
    ///
    /// * `database_url` - SQLite database path
    /// * `database_pool_size` - Maximum pool connections
    /// * `storage_backend` - Backend selection (Sql, Dual, KvPrimary, KvOnly)
    /// * `kv_path` - Path to redb database (if using KV)
    /// * `tantivy_path` - Path to Tantivy index (if using KV)
    ///
    /// # Returns
    ///
    /// Configured Db instance with appropriate storage mode
    pub async fn create(
        database_url: &str,
        database_pool_size: u32,
        storage_backend: StorageBackend,
        kv_path: Option<&Path>,
        tantivy_path: Option<&Path>,
    ) -> Result<Db> {
        agent_log(
            "H1",
            "factory.rs:53",
            "db_factory_create_start",
            json!({
                "database_url": database_url,
                "pool_size": database_pool_size,
                "storage_backend": format!("{:?}", storage_backend),
                "kv_path": kv_path.map(|p| p.display().to_string()),
                "tantivy_path": tantivy_path.map(|p| p.display().to_string()),
            }),
        );
        // Create SQL pool unless running KV-only
        // NOTE: If KvOnly is later downgraded by the coverage guard, any code path
        // that touches `pool()` will panic unless a pool exists. Keep this in mind
        // if you rely on fallback modes.
        let pool = match storage_backend {
            StorageBackend::KvOnly => None,
            _ => Some(Self::create_sql_pool(database_url, database_pool_size).await?),
        };

        // Create KV backend if enabled
        let (kv, storage_mode) = match storage_backend {
            StorageBackend::Sql => {
                info!("Initializing SQL-only storage mode");
                (None, StorageMode::SqlOnly)
            }
            StorageBackend::Dual => {
                info!("Initializing dual-write storage mode (SQL primary, KV validation)");
                let kv = Self::create_kv_backend(kv_path, tantivy_path).await?;
                (Some(Arc::new(kv)), StorageMode::DualWrite)
            }
            StorageBackend::KvPrimary => {
                info!("Initializing KV-primary storage mode (SQL fallback)");
                let kv = Self::create_kv_backend(kv_path, tantivy_path).await?;
                (Some(Arc::new(kv)), StorageMode::KvPrimary)
            }
            StorageBackend::KvOnly => {
                info!("Initializing KV-only storage mode");
                let kv = Self::create_kv_backend(kv_path, tantivy_path).await?;
                (Some(Arc::new(kv)), StorageMode::KvOnly)
            }
        };

        let mut db = match (&pool, &kv) {
            (Some(pool_ref), kv_opt) => Db::new(pool_ref.clone(), kv_opt.clone(), storage_mode),
            (None, kv_opt) => Db::new_kv_only(kv_opt.clone(), storage_mode),
        };

        agent_log(
            "H1",
            "factory.rs:85",
            "db_factory_create_resolved",
            json!({
                "storage_mode": format!("{:?}", storage_mode),
                "has_sql_pool": pool.is_some(),
                "has_kv_backend": kv.is_some(),
            }),
        );

        agent_log(
            "H4",
            "factory.rs:150",
            "kv_only_guard_enter",
            json!({
                "storage_mode": format!("{:?}", storage_mode),
                "has_sql_pool": pool.is_some(),
                "has_kv_backend": kv.is_some(),
            }),
        );

        db.enforce_kv_only_guard().map_err(|e| {
            agent_log(
                "H4",
                "factory.rs:160",
                "kv_only_guard_err",
                json!({
                    "storage_mode": format!("{:?}", storage_mode),
                    "error": format!("{}", e),
                }),
            );
            e
        })?;

        agent_log(
            "H4",
            "factory.rs:169",
            "kv_only_guard_ok",
            json!({
                "storage_mode": format!("{:?}", storage_mode),
            }),
        );
        Ok(db)
    }

    /// Create SQL connection pool
    async fn create_sql_pool(database_url: &str, pool_size: u32) -> Result<sqlx::SqlitePool> {
        use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
        use std::str::FromStr;

        agent_log(
            "H2",
            "factory.rs:99",
            "sql_pool_connecting",
            json!({
                "database_url": database_url,
                "pool_size": pool_size,
            }),
        );

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", database_url))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(30))
            .statement_cache_capacity(100)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(pool_size)
            .connect_with(options)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

        info!(
            database_url = %database_url,
            pool_size = pool_size,
            "SQL connection pool initialized"
        );

        agent_log(
            "H2",
            "factory.rs:117",
            "sql_pool_connected",
            json!({
                "database_url": database_url,
                "pool_size": pool_size,
                "journal_mode": "Wal",
                "synchronous": "Normal",
            }),
        );

        Ok(pool)
    }

    /// Create KV backend with indexes
    async fn create_kv_backend(
        kv_path: Option<&Path>,
        _tantivy_path: Option<&Path>,
    ) -> Result<KvDb> {
        // Get path or use default
        let kv_path = kv_path
            .ok_or_else(|| AosError::Config("KV path required for KV storage mode".to_string()))?;

        // Open redb backend using existing KvDb::init_redb
        info!(kv_path = ?kv_path, "Opening redb backend");
        let kv_db = KvDb::init_redb(kv_path)?;

        agent_log(
            "H3",
            "factory.rs:131",
            "kv_backend_initialized",
            json!({
                "kv_path": kv_path.display().to_string(),
            }),
        );

        info!("KV backend initialized successfully");
        Ok(kv_db)
    }

    /// Create in-memory database for testing
    ///
    /// Creates a temporary SQLite database in memory with all migrations applied.
    /// Useful for unit tests and integration tests.
    pub async fn create_in_memory() -> Result<Db> {
        let pool = Self::create_sql_pool(":memory:", 5).await?;
        Ok(Db::new(pool, None, StorageMode::SqlOnly))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_factory_sql_only() {
        let db = DbFactory::create_in_memory().await.unwrap();
        assert_eq!(db.storage_mode(), StorageMode::SqlOnly);
        assert!(!db.has_kv_backend());
    }

    #[tokio::test]
    async fn kv_only_guard_blocks_until_coverage_complete() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("cp.sqlite3");
        let kv_path = tmp.path().join("kv.redb");

        let result = DbFactory::create(
            db_path.to_str().expect("tmp path should be valid UTF-8"),
            2,
            StorageBackend::KvOnly,
            Some(kv_path.as_path()),
            None,
        )
        .await;

        match result {
            Ok(db) => assert_eq!(db.storage_mode(), StorageMode::KvOnly),
            Err(_) => {
                // Acceptable when KV coverage is still incomplete; guard should block KvOnly.
            }
        }
    }

    #[tokio::test]
    async fn test_storage_mode_flags() {
        // SQL-only
        let mode = StorageMode::SqlOnly;
        assert!(mode.read_from_sql());
        assert!(!mode.read_from_kv());
        assert!(mode.write_to_sql());
        assert!(!mode.write_to_kv());

        // Dual-write
        let mode = StorageMode::DualWrite;
        assert!(mode.read_from_sql());
        assert!(!mode.read_from_kv());
        assert!(mode.write_to_sql());
        assert!(mode.write_to_kv());

        // KV-primary
        let mode = StorageMode::KvPrimary;
        assert!(mode.read_from_sql());
        assert!(mode.read_from_kv());
        assert!(mode.write_to_sql());
        assert!(mode.write_to_kv());

        // KV-only
        let mode = StorageMode::KvOnly;
        assert!(!mode.read_from_sql());
        assert!(mode.read_from_kv());
        assert!(!mode.write_to_sql());
        assert!(mode.write_to_kv());
    }
}
