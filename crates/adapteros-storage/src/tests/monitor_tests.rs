//! Storage monitor tests
//!
//! Tests for storage usage monitoring and alerting.

use crate::monitor::StorageMonitor;
use crate::{AlertThresholds, StorageConfig, StorageMonitoring};
use adapteros_core::Result;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_storage_monitor_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 1000,
        max_files: 100,
        monitoring: StorageMonitoring {
            enabled: true,
            check_interval: Duration::from_secs(1),
            alert_thresholds: AlertThresholds {
                warning_pct: 50.0,
                critical_pct: 80.0,
                emergency_pct: 95.0,
            },
        },
        ..Default::default()
    };

    let monitor = StorageMonitor::new(&config, temp_dir.path())?;
    assert!(
        monitor.get_usage().await.is_ok(),
        "Monitor should be created successfully"
    );

    Ok(())
}

#[tokio::test]
async fn test_get_usage_with_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 10000,
        max_files: 100,
        ..Default::default()
    };

    let monitor = StorageMonitor::new(&config, temp_dir.path())?;

    // Create test files
    let test_file1 = temp_dir.path().join("test1.txt");
    fs::write(&test_file1, "hello")?;

    let test_file2 = temp_dir.path().join("test2.txt");
    fs::write(&test_file2, "world")?;

    // Get usage
    let usage = monitor.get_usage().await?;

    assert!(usage.used_bytes > 0, "Should detect used space");
    assert_eq!(usage.file_count, 2, "Should count files correctly");

    Ok(())
}
