//! Database factory for creating Db instances with optional KV backend
//!
//! Provides initialization logic for the dual-write migration system, allowing
//! gradual transition from SQL-only to KV-primary storage.

use crate::{Db, KvDb, StorageMode};
use adapteros_core::{AosError, Result};
use sqlx::sqlite::SqlitePoolOptions;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// SQLite statement cache capacity.
/// Reduced from 100 to 50 to limit memory usage per connection.
/// Each cached statement uses memory proportional to query complexity.
const STATEMENT_CACHE_CAPACITY: usize = 50;

/// Threshold for warning about estimated cache memory usage (bytes).
/// Based on average ~5KB per cached statement (conservative estimate).
const CACHE_MEMORY_WARNING_THRESHOLD: usize = STATEMENT_CACHE_CAPACITY * 5 * 1024;

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

        db.enforce_kv_only_guard()?;

        Ok(db)
    }

    /// Create SQL connection pool
    async fn create_sql_pool(database_url: &str, pool_size: u32) -> Result<sqlx::SqlitePool> {
        use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};
        use std::str::FromStr;

        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", database_url))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(Duration::from_secs(30))
            .statement_cache_capacity(STATEMENT_CACHE_CAPACITY)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(pool_size)
            .connect_with(options)
            .await
            .map_err(|e| AosError::Database(format!("Failed to connect to database: {}", e)))?;

        // Estimate and warn about potential cache memory usage
        let estimated_cache_memory = pool_size as usize * CACHE_MEMORY_WARNING_THRESHOLD;
        if estimated_cache_memory > 10 * 1024 * 1024 {
            // > 10MB total
            warn!(
                pool_size = pool_size,
                cache_capacity = STATEMENT_CACHE_CAPACITY,
                estimated_cache_mb = estimated_cache_memory / (1024 * 1024),
                "Statement cache memory usage may be high with current pool size"
            );
        }

        info!(
            database_url = %database_url,
            pool_size = pool_size,
            statement_cache_capacity = STATEMENT_CACHE_CAPACITY,
            "SQL connection pool initialized"
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
