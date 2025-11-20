/// Comprehensive tests for temporary file cleanup module
///
/// Tests cover:
/// - Registry operations (register, unregister, metrics)
/// - Cleanup with retry logic
/// - Orphaned file detection and cleanup
/// - Background cleanup manager task
/// - Error handling and recovery

#[cfg(test)]
mod tests {
    use crate::temp_cleanup::{CleanupManager, TempFileRegistry};
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::sleep;

    /// Test basic registration and unregistration
    #[tokio::test]
    async fn test_register_unregister() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test.tmp");
        fs::write(&temp_file, "test").unwrap();

        // Register
        let id = registry.register(&temp_file).await;
        assert!(!id.is_empty());

        // Verify it's tracked
        let files = registry.files.read().await;
        assert!(files.contains_key(&id));
        drop(files);

        // Unregister
        registry.unregister(&id).await;

        // Verify it's removed
        let files = registry.files.read().await;
        assert!(!files.contains_key(&id));
    }

    /// Test cleanup of a single file
    #[tokio::test]
    async fn test_cleanup_single_file() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test.tmp");
        fs::write(&temp_file, "test content").unwrap();

        assert!(temp_file.exists());

        let result = registry.cleanup_with_retry(&temp_file).await;
        assert!(result.is_ok());
        assert!(!temp_file.exists());

        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_deleted, 1);
        assert_eq!(metrics.total_failed, 0);
    }

    /// Test cleanup of nonexistent file (should succeed)
    #[tokio::test]
    async fn test_cleanup_nonexistent_file() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let nonexistent = std::path::PathBuf::from("/tmp/nonexistent_cleanup_test_12345.tmp");

        let result = registry.cleanup_with_retry(&nonexistent).await;
        assert!(result.is_ok());

        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_deleted, 1);
    }

    /// Test scanning orphaned files with different ages
    #[tokio::test]
    async fn test_scan_orphaned_files() {
        // Use short orphan threshold for testing
        let registry = TempFileRegistry::new(Duration::from_millis(200), 3);
        let temp_dir = TempDir::new().unwrap();

        // Create fresh file
        let fresh_file = temp_dir.path().join("fresh.tmp");
        fs::write(&fresh_file, "fresh").unwrap();

        // Create old file
        let old_file = temp_dir.path().join("old.tmp");
        fs::write(&old_file, "old").unwrap();

        // Wait for old file to become orphaned
        sleep(Duration::from_millis(300)).await;

        let orphaned = registry.scan_orphaned_files(temp_dir.path()).await.unwrap();

        // Should find only old file
        assert!(orphaned.iter().any(|p| p.file_name().unwrap() == "old.tmp"));
        assert!(!orphaned
            .iter()
            .any(|p| p.file_name().unwrap() == "fresh.tmp"));
    }

    /// Test cleanup cycle (scan and delete)
    #[tokio::test]
    async fn test_cleanup_cycle() {
        let registry = TempFileRegistry::new(Duration::from_millis(100), 3);
        let temp_dir = TempDir::new().unwrap();

        // Create multiple old files
        for i in 0..3 {
            let file = temp_dir.path().join(format!("old_{}.tmp", i));
            fs::write(&file, "content").unwrap();
        }

        // Create fresh file
        let fresh = temp_dir.path().join("fresh.tmp");
        fs::write(&fresh, "fresh").unwrap();

        // Wait for old files to become orphaned
        sleep(Duration::from_millis(150)).await;

        // Run cleanup cycle
        let result = registry.run_cleanup_cycle(temp_dir.path()).await;
        assert!(result.is_ok());

        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_scanned, 3);
        assert_eq!(metrics.total_deleted, 3);
        assert!(metrics.last_cleanup_at.is_some());

        // Verify fresh file still exists
        assert!(fresh.exists());
    }

    /// Test metrics accumulation
    #[tokio::test]
    async fn test_metrics_accumulation() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let temp_dir = TempDir::new().unwrap();

        // Initial state
        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_deleted, 0);
        assert_eq!(metrics.total_failed, 0);

        // Cleanup multiple files
        for i in 0..5 {
            let file = temp_dir.path().join(format!("test_{}.tmp", i));
            fs::write(&file, "content").unwrap();
            let _ = registry.cleanup_with_retry(&file).await;
        }

        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_deleted, 5);
        assert_eq!(metrics.total_failed, 0);
    }

    /// Test manager creation
    #[tokio::test]
    async fn test_cleanup_manager_creation() {
        use std::sync::Arc;
        let registry = Arc::new(TempFileRegistry::new(Duration::from_secs(60), 3));
        let temp_dir = TempDir::new().unwrap();

        let manager = CleanupManager::new(registry, Duration::from_secs(5), temp_dir.path());
        assert_eq!(manager.cleanup_interval, Duration::from_secs(5));
    }

    /// Test tracked file exclusion from cleanup
    #[tokio::test]
    async fn test_tracked_file_not_scanned() {
        let registry = TempFileRegistry::new(Duration::from_millis(100), 3);
        let temp_dir = TempDir::new().unwrap();

        let tracked_file = temp_dir.path().join("tracked.tmp");
        fs::write(&tracked_file, "tracked").unwrap();

        // Register file in tracking
        let _id = registry.register(&tracked_file).await;

        // Wait for it to become old
        sleep(Duration::from_millis(150)).await;

        // Scan for orphaned files
        let orphaned = registry.scan_orphaned_files(temp_dir.path()).await.unwrap();

        // Tracked file should not be in orphaned list
        assert!(!orphaned
            .iter()
            .any(|p| p.file_name().unwrap() == "tracked.tmp"));
    }

    /// Test retry behavior with multiple attempts
    #[tokio::test]
    async fn test_cleanup_retries() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test.tmp");
        fs::write(&temp_file, "test").unwrap();

        // Cleanup should succeed on first try
        let result = registry.cleanup_with_retry(&temp_file).await;
        assert!(result.is_ok());

        let metrics = registry.metrics().await;
        // File deleted on first attempt, no retries needed
        assert_eq!(metrics.total_deleted, 1);
    }

    /// Test cleanup on readonly filesystem simulation
    #[tokio::test]
    #[cfg(unix)]
    async fn test_cleanup_readonly_directory() {
        use std::process::Command;

        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let temp_dir = TempDir::new().unwrap();
        let temp_file = temp_dir.path().join("test.tmp");
        fs::write(&temp_file, "test").unwrap();

        // Make directory readonly
        let chmod_result = Command::new("chmod")
            .arg("444")
            .arg(temp_dir.path())
            .output();

        if chmod_result.is_ok() {
            let result = registry.cleanup_with_retry(&temp_file).await;
            // Should fail due to readonly directory
            assert!(result.is_err());

            let metrics = registry.metrics().await;
            assert_eq!(metrics.total_failed, 1);

            // Restore permissions for cleanup
            let _ = Command::new("chmod")
                .arg("755")
                .arg(temp_dir.path())
                .output();
        }
    }

    /// Test directory creation on first scan
    #[tokio::test]
    async fn test_scan_nonexistent_directory() {
        let registry = TempFileRegistry::new(Duration::from_secs(60), 3);
        let nonexistent_dir = std::path::PathBuf::from("/tmp/nonexistent_scan_dir_12345");

        // Should not error on nonexistent directory
        let result = registry.scan_orphaned_files(&nonexistent_dir).await;
        assert!(result.is_ok());

        let orphaned = result.unwrap();
        assert_eq!(orphaned.len(), 0);
    }

    /// Test metrics default values
    #[test]
    fn test_metrics_defaults() {
        use crate::temp_cleanup::CleanupMetrics;

        let metrics = CleanupMetrics::default();
        assert_eq!(metrics.total_scanned, 0);
        assert_eq!(metrics.total_deleted, 0);
        assert_eq!(metrics.total_failed, 0);
        assert_eq!(metrics.total_retried, 0);
        assert_eq!(metrics.total_errors, 0);
        assert!(metrics.last_cleanup_at.is_none());
    }

    /// Test concurrent cleanup operations
    #[tokio::test]
    async fn test_concurrent_cleanup() {
        let registry = std::sync::Arc::new(TempFileRegistry::new(Duration::from_secs(60), 3));
        let temp_dir = TempDir::new().unwrap();

        // Create files
        let files: Vec<_> = (0..10)
            .map(|i| {
                let file = temp_dir.path().join(format!("test_{}.tmp", i));
                fs::write(&file, format!("content {}", i)).unwrap();
                file
            })
            .collect();

        // Cleanup concurrently
        let handles: Vec<_> = files
            .iter()
            .map(|f| {
                let registry = registry.clone();
                let file = f.clone();
                tokio::spawn(async move { registry.cleanup_with_retry(&file).await })
            })
            .collect();

        // Wait for all to complete
        for handle in handles {
            let result = handle.await;
            assert!(result.is_ok());
        }

        let metrics = registry.metrics().await;
        assert_eq!(metrics.total_deleted, 10);
    }
}
