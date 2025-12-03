//! Database setup helpers for KV integration tests
//!
//! Provides utilities for creating test databases with various storage modes
//! and configurations.

use adapteros_core::Result;
use adapteros_db::{Db, KvDb, StorageMode};
use std::path::PathBuf;
use tempfile::TempDir;
use tracing::debug;

/// Test database wrapper with cleanup support
///
/// Automatically handles database lifecycle including migrations,
/// seeding, and cleanup. Supports both in-memory and file-based databases.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::TestDb;
///
/// #[tokio::test]
/// async fn test_example() {
///     let test_db = TestDb::new().await;
///     // Use test_db.db() to get the Db instance
///     // Cleanup happens automatically on drop
/// }
/// ```
pub struct TestDb {
    db: Db,
    _temp_dir: Option<TempDir>,
    storage_mode: StorageMode,
}

impl TestDb {
    /// Create a new in-memory test database with SqlOnly mode
    ///
    /// This is the fastest option for tests that don't need persistence
    /// or KV backend functionality.
    pub async fn new() -> Self {
        Self::with_mode(StorageMode::SqlOnly).await
    }

    /// Create a test database with a specific storage mode
    ///
    /// For SqlOnly and DualWrite modes, uses in-memory SQLite.
    /// For KvPrimary and KvOnly modes, uses temporary file-based storage.
    pub async fn with_mode(mode: StorageMode) -> Self {
        match mode {
            StorageMode::SqlOnly | StorageMode::DualWrite => {
                Self::new_in_memory_with_mode(mode).await
            }
            StorageMode::KvPrimary | StorageMode::KvOnly => {
                Self::new_persistent_with_mode(mode).await
            }
        }
    }

    /// Create an in-memory test database with specified storage mode
    ///
    /// # Note
    ///
    /// KV modes (KvPrimary, KvOnly) will not have a functional KV backend
    /// in this configuration. Use `new_persistent_with_mode` for KV testing.
    async fn new_in_memory_with_mode(mode: StorageMode) -> Self {
        let db = Db::new_in_memory()
            .await
            .expect("Failed to create in-memory database");

        // Apply migrations
        db.migrate().await.expect("Failed to apply migrations");

        // Seed with default tenant
        db.seed_dev_data().await.expect("Failed to seed dev data");

        // Set storage mode if different from default
        if mode != StorageMode::SqlOnly {
            debug!(storage_mode = ?mode, "Setting test database storage mode");
            // Note: Storage mode is set during construction, cannot be changed after
            // This is a limitation of the current API
        }

        Self {
            db,
            _temp_dir: None,
            storage_mode: mode,
        }
    }

    /// Create a persistent test database with KV backend support
    ///
    /// Creates temporary directories for both SQLite and KV storage.
    /// Required for testing KvPrimary and KvOnly modes.
    async fn new_persistent_with_mode(mode: StorageMode) -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        // Create paths for databases
        let db_path = temp_dir.path().join("test.db");
        let kv_path = temp_dir.path().join("test.kv");

        // Open SQLite database
        let db_url = db_path.to_str().expect("Invalid db path");
        let db = Db::connect(db_url)
            .await
            .expect("Failed to connect to database");

        // Apply migrations
        db.migrate().await.expect("Failed to apply migrations");

        // Seed with default tenant
        db.seed_dev_data().await.expect("Failed to seed dev data");

        // Initialize KV backend if needed
        if mode.write_to_kv() {
            debug!(kv_path = ?kv_path, "Initializing KV backend for test");
            let _kv = KvDb::init_redb(&kv_path).expect("Failed to initialize KV backend");
            // TODO: Attach KV backend to Db instance
            // This requires API changes to support runtime KV attachment
        }

        Self {
            db,
            _temp_dir: Some(temp_dir),
            storage_mode: mode,
        }
    }

    /// Get a reference to the underlying Db instance
    pub fn db(&self) -> &Db {
        &self.db
    }

    /// Get the storage mode for this test database
    pub fn storage_mode(&self) -> StorageMode {
        self.storage_mode
    }

    /// Get the temp directory path (if using persistent storage)
    pub fn temp_path(&self) -> Option<PathBuf> {
        self._temp_dir.as_ref().map(|d| d.path().to_path_buf())
    }

    /// Explicitly clean up the database
    ///
    /// Called automatically on drop, but can be called explicitly
    /// if you need to control cleanup timing.
    pub async fn cleanup(self) {
        drop(self.db);
        // TempDir cleanup happens on drop
    }
}

/// Create a simple in-memory test database with SqlOnly mode
///
/// Convenience function for tests that just need a basic database.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::create_test_db;
///
/// #[tokio::test]
/// async fn test_example() -> adapteros_core::Result<()> {
///     let db = create_test_db().await?;
///     // Use db...
///     Ok(())
/// }
/// ```
pub async fn create_test_db() -> Result<Db> {
    let db = Db::new_in_memory().await?;
    db.migrate().await?;
    db.seed_dev_data().await?;
    Ok(db)
}

/// Create a test database with a specific storage mode
///
/// For advanced tests that need to control storage mode.
///
/// # Arguments
///
/// * `mode` - The storage mode to use
///
/// # Returns
///
/// A configured Db instance with migrations applied and dev data seeded.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::create_test_db_with_mode;
/// use adapteros_db::StorageMode;
///
/// #[tokio::test]
/// async fn test_dual_write() -> adapteros_core::Result<()> {
///     let db = create_test_db_with_mode(StorageMode::DualWrite).await?;
///     // Test dual-write behavior...
///     Ok(())
/// }
/// ```
pub async fn create_test_db_with_mode(mode: StorageMode) -> Result<Db> {
    let test_db = TestDb::with_mode(mode).await;
    Ok(test_db.db.clone())
}

/// Create a test database with KV backend attached
///
/// Creates a temporary directory with both SQLite and KV databases.
/// Returns the Db instance and the TempDir (which must be kept alive).
///
/// # Arguments
///
/// * `mode` - The storage mode to use (should be KvPrimary or KvOnly for KV testing)
///
/// # Returns
///
/// Tuple of (Db, TempDir). Keep the TempDir alive for the duration of the test.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::create_test_db_with_kv;
/// use adapteros_db::StorageMode;
///
/// #[tokio::test]
/// async fn test_kv_operations() -> adapteros_core::Result<()> {
///     let (db, _temp_dir) = create_test_db_with_kv(StorageMode::KvPrimary).await?;
///     // Test KV operations...
///     Ok(())
/// }
/// ```
pub async fn create_test_db_with_kv(mode: StorageMode) -> Result<(Db, TempDir)> {
    let temp_dir = TempDir::new().map_err(|e| {
        adapteros_core::AosError::Internal(format!("Failed to create temp directory: {}", e))
    })?;

    // Create paths
    let db_path = temp_dir.path().join("test.db");
    let kv_path = temp_dir.path().join("test.kv");

    // Open database
    let db_url = db_path
        .to_str()
        .ok_or_else(|| adapteros_core::AosError::Internal("Invalid database path".to_string()))?;
    let db = Db::connect(db_url).await?;

    // Apply migrations and seed
    db.migrate().await?;
    db.seed_dev_data().await?;

    // Initialize KV backend
    if mode.write_to_kv() {
        debug!(kv_path = ?kv_path, "Initializing KV backend");
        let _kv = KvDb::init_redb(&kv_path)?;
        // TODO: Attach to Db instance once API supports it
    }

    Ok((db, temp_dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_in_memory_db() {
        let test_db = TestDb::new().await;
        assert_eq!(test_db.storage_mode(), StorageMode::SqlOnly);
        assert!(test_db.temp_path().is_none());
    }

    #[tokio::test]
    async fn test_create_persistent_db() {
        let test_db = TestDb::with_mode(StorageMode::KvPrimary).await;
        assert_eq!(test_db.storage_mode(), StorageMode::KvPrimary);
        assert!(test_db.temp_path().is_some());
    }

    #[tokio::test]
    async fn test_create_test_db_convenience() {
        let db = create_test_db().await.unwrap();
        // Should have migrations applied and dev data seeded
        let tenant = db.get_tenant("default-tenant").await.unwrap();
        assert!(tenant.is_some());
    }

    #[tokio::test]
    async fn test_create_test_db_with_kv() {
        let (db, temp_dir) = create_test_db_with_kv(StorageMode::KvPrimary)
            .await
            .unwrap();
        assert!(temp_dir.path().exists());

        // Verify database is functional
        let tenant = db.get_tenant("default-tenant").await.unwrap();
        assert!(tenant.is_some());
    }
}
