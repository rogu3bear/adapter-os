//! Cleanup manager tests
//!
//! Tests for automatic storage cleanup and file pattern matching.

use crate::cleanup::CleanupManager;
use crate::{CleanupPolicy, StorageConfig};
use adapteros_core::Result;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_cleanup_manager_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 1000,
        max_files: 100,
        cleanup_policy: CleanupPolicy {
            enabled: true,
            interval: Duration::from_secs(1),
            age_threshold: Duration::from_secs(0),
            usage_threshold_pct: 50.0,
            patterns: vec!["*.tmp".to_string()],
        },
        ..Default::default()
    };

    let cleanup_manager = CleanupManager::new(&config, temp_dir.path())?;
    assert!(
        cleanup_manager.cleanup_if_needed().await.is_ok(),
        "Cleanup manager should be created successfully"
    );

    Ok(())
}

#[tokio::test]
async fn test_cleanup_tmp_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 1000,
        max_files: 100,
        cleanup_policy: CleanupPolicy {
            enabled: true,
            interval: Duration::from_secs(1),
            age_threshold: Duration::from_secs(0),
            usage_threshold_pct: 50.0,
            patterns: vec!["*.tmp".to_string()],
        },
        ..Default::default()
    };

    let cleanup_manager = CleanupManager::new(&config, temp_dir.path())?;

    // Create test files
    let test_file1 = temp_dir.path().join("test1.tmp");
    fs::write(&test_file1, "hello")?;

    let test_file2 = temp_dir.path().join("test2.tmp");
    fs::write(&test_file2, "world")?;

    // Run cleanup
    cleanup_manager.cleanup_if_needed().await?;

    // Check that tmp files were cleaned up
    assert!(!test_file1.exists(), "tmp file should be cleaned up");
    assert!(!test_file2.exists(), "tmp file should be cleaned up");

    Ok(())
}
