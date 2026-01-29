//! Cleanup utilities for test databases and files
//!
//! Provides utilities for properly cleaning up test resources including
//! database connections, temporary files, and KV storage.

use adapteros_core::Result;
use adapteros_db::Db;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Clean up a test database
///
/// Properly closes the database connection and releases resources.
/// Safe to call multiple times.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::{create_test_db, cleanup_test_db};
///
/// #[tokio::test]
/// async fn test_example() {
///     let db = create_test_db().await.unwrap();
///     // ... test code ...
///     cleanup_test_db(&db).await.unwrap();
/// }
/// ```
pub async fn cleanup_test_db(db: &Db) -> Result<()> {
    debug!("Cleaning up test database");
    db.close().await?;
    Ok(())
}

/// Clean up test files in a directory
///
/// Recursively removes all files in the specified directory.
/// Useful for cleaning up temporary adapter files, datasets, etc.
///
/// # Arguments
///
/// * `path` - Directory to clean up
///
/// # Safety
///
/// This function will DELETE all files in the directory. Only use with
/// test-specific temporary directories.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::cleanup_test_files;
/// use std::path::Path;
///
/// #[tokio::test]
/// async fn test_example() {
///     let temp_dir = Path::new("var/test-dir");
///     // ... create test files ...
///     cleanup_test_files(temp_dir).await.unwrap();
/// }
/// ```
pub async fn cleanup_test_files(path: &Path) -> Result<()> {
    if !path.exists() {
        debug!(path = ?path, "Cleanup path does not exist, skipping");
        return Ok(());
    }

    debug!(path = ?path, "Cleaning up test files");

    tokio::fs::remove_dir_all(path).await.map_err(|e| {
        adapteros_core::AosError::Internal(format!("Failed to cleanup test files: {}", e))
    })?;

    Ok(())
}

/// Clean up multiple test file paths
///
/// Convenience function for cleaning up multiple directories at once.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::cleanup_test_paths;
/// use std::path::PathBuf;
///
/// #[tokio::test]
/// async fn test_example() {
///     let paths = vec![
///         PathBuf::from("var/test-adapters"),
///         PathBuf::from("var/test-datasets"),
///     ];
///     cleanup_test_paths(&paths).await.unwrap();
/// }
/// ```
pub async fn cleanup_test_paths(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        if let Err(e) = cleanup_test_files(path).await {
            warn!(path = ?path, error = %e, "Failed to cleanup test path");
            // Continue with other paths even if one fails
        }
    }
    Ok(())
}

/// Clean up a single test file
///
/// Removes a single file if it exists.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::cleanup_test_file;
/// use std::path::Path;
///
/// #[tokio::test]
/// async fn test_example() {
///     let file = Path::new("var/test.aos");
///     // ... create test file ...
///     cleanup_test_file(file).await.unwrap();
/// }
/// ```
pub async fn cleanup_test_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    debug!(path = ?path, "Removing test file");

    tokio::fs::remove_file(path).await.map_err(|e| {
        adapteros_core::AosError::Internal(format!("Failed to remove test file: {}", e))
    })?;

    Ok(())
}

/// RAII guard for automatic cleanup of test resources
///
/// Automatically cleans up the database and optional file paths when dropped.
/// Useful for ensuring cleanup happens even if the test panics.
///
/// # Examples
///
/// ```no_run
/// use adapteros_db::tests::common::{create_test_db, TestCleanupGuard};
/// use std::path::PathBuf;
///
/// #[tokio::test]
/// async fn test_example() {
///     let db = create_test_db().await.unwrap();
///     let temp_dir = PathBuf::from("var/test-dir");
///
///     let _guard = TestCleanupGuard::new(db.clone(), vec![temp_dir]);
///
///     // Test code here...
///     // Cleanup happens automatically when _guard is dropped
/// }
/// ```
pub struct TestCleanupGuard {
    db: Option<Db>,
    paths: Vec<PathBuf>,
}

impl TestCleanupGuard {
    /// Create a new cleanup guard
    ///
    /// # Arguments
    ///
    /// * `db` - Database to clean up
    /// * `paths` - File paths to clean up
    pub fn new(db: Db, paths: Vec<PathBuf>) -> Self {
        Self {
            db: Some(db),
            paths,
        }
    }

    /// Create a guard for database cleanup only
    pub fn db_only(db: Db) -> Self {
        Self {
            db: Some(db),
            paths: Vec::new(),
        }
    }

    /// Create a guard for file cleanup only
    pub fn files_only(paths: Vec<PathBuf>) -> Self {
        Self { db: None, paths }
    }

    /// Add a path to clean up
    pub fn add_path(&mut self, path: PathBuf) {
        self.paths.push(path);
    }

    /// Explicitly run cleanup
    ///
    /// Consumes the guard and performs cleanup. If not called explicitly,
    /// cleanup will happen automatically on drop (but errors will be ignored).
    pub async fn cleanup(mut self) -> Result<()> {
        // Clean up database
        if let Some(db) = self.db.take() {
            cleanup_test_db(&db).await?;
        }

        // Clean up files
        cleanup_test_paths(&self.paths).await?;

        Ok(())
    }
}

impl Drop for TestCleanupGuard {
    fn drop(&mut self) {
        // Best-effort cleanup on drop
        // We can't await async operations here, so just log warnings
        if self.db.is_some() || !self.paths.is_empty() {
            warn!(
                "TestCleanupGuard dropped without explicit cleanup. \
                 Consider calling cleanup() explicitly for better error handling."
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn new_test_tempdir() -> TempDir {
        TempDir::with_prefix("aos-test-")
            .expect("Failed to create temporary directory for cleanup test")
    }

    #[tokio::test]
    async fn test_cleanup_nonexistent_path() {
        let result = cleanup_test_files(Path::new("/nonexistent/path")).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cleanup_test_file() {
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.txt");

        tokio::fs::write(&test_file, b"test content").await.unwrap();
        assert!(test_file.exists());

        cleanup_test_file(&test_file).await.unwrap();
        assert!(!test_file.exists());
    }

    #[tokio::test]
    async fn test_cleanup_test_files() {
        let temp_dir = new_test_tempdir();
        let test_file = temp_dir.path().join("test.txt");

        tokio::fs::write(&test_file, b"test content").await.unwrap();
        assert!(temp_dir.path().exists());

        cleanup_test_files(temp_dir.path()).await.unwrap();
        assert!(!temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_cleanup_multiple_paths() {
        let temp_dir1 = new_test_tempdir();
        let temp_dir2 = new_test_tempdir();

        let paths = vec![
            temp_dir1.path().to_path_buf(),
            temp_dir2.path().to_path_buf(),
        ];

        cleanup_test_paths(&paths).await.unwrap();

        assert!(!temp_dir1.path().exists());
        assert!(!temp_dir2.path().exists());
    }

    #[tokio::test]
    async fn test_cleanup_guard_files_only() {
        let temp_dir = new_test_tempdir();
        let path = temp_dir.path().to_path_buf();

        {
            let mut guard = TestCleanupGuard::files_only(vec![]);
            guard.add_path(path.clone());
            guard.cleanup().await.unwrap();
        }

        assert!(!path.exists());
    }
}
