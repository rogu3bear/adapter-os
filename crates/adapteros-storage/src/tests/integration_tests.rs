//! Integration tests
//!
//! End-to-end tests combining quota, cleanup, monitoring, and policy enforcement.

use crate::{
    AlertThresholds, CleanupPolicy, StorageConfig, StorageManager, StorageMonitoring,
};
use adapteros_core::Result;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn test_storage_manager_creation() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig::default();

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Basic operations should work
    let result = manager.check_space(100).await;
    assert!(result.is_ok(), "Storage manager should be created successfully");

    Ok(())
}

#[tokio::test]
async fn test_full_storage_workflow() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 10000,
        max_files: 100,
        cleanup_policy: CleanupPolicy {
            enabled: true,
            interval: Duration::from_secs(1),
            age_threshold: Duration::from_secs(0),
            usage_threshold_pct: 80.0,
            patterns: vec!["*.tmp".to_string()],
        },
        monitoring: StorageMonitoring {
            enabled: true,
            check_interval: Duration::from_secs(1),
            alert_thresholds: AlertThresholds {
                warning_pct: 70.0,
                critical_pct: 85.0,
                emergency_pct: 95.0,
            },
        },
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // 1. Check initial space
    manager.check_space(1000).await?;

    // 2. Reserve space
    let reservation = manager.reserve_space(500).await?;
    assert_eq!(reservation.size, 500);

    // 3. Create some files
    let test_file1 = temp_dir.path().join("test1.txt");
    fs::write(&test_file1, "content1")?;

    let test_file2 = temp_dir.path().join("test2.tmp");
    fs::write(&test_file2, "content2")?;

    // 4. Get usage
    let usage = manager.get_usage().await?;
    assert!(usage.used_bytes > 0);
    assert!(usage.file_count >= 2);

    // 5. Run cleanup
    manager.cleanup_if_needed().await?;

    // tmp file should be cleaned up
    assert!(!test_file2.exists());
    assert!(test_file1.exists());

    // 6. Release reservation
    manager.release_space(reservation).await?;

    Ok(())
}

#[tokio::test]
async fn test_quota_enforcement_with_cleanup() -> Result<()> {
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

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Fill up space
    let reservation = manager.reserve_space(900).await?;

    // Should fail to allocate more
    let result = manager.check_space(200).await;
    assert!(result.is_err(), "Should reject allocation exceeding quota");

    // Release space
    manager.release_space(reservation).await?;

    // Should succeed now
    let result = manager.check_space(200).await;
    assert!(result.is_ok(), "Should allow allocation after release");

    Ok(())
}

#[tokio::test]
async fn test_monitoring_with_usage_growth() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 10000,
        max_files: 100,
        monitoring: StorageMonitoring {
            enabled: true,
            check_interval: Duration::from_millis(100),
            alert_thresholds: AlertThresholds {
                warning_pct: 30.0,
                critical_pct: 60.0,
                emergency_pct: 90.0,
            },
        },
        ..Default::default()
    };

    let mut manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Start monitoring
    manager.start_monitoring().await?;

    // Create files to increase usage
    for i in 0..5 {
        let file = temp_dir.path().join(format!("file{}.txt", i));
        fs::write(&file, "a".repeat(500))?;
    }

    // Wait for monitoring to detect
    tokio::time::sleep(Duration::from_millis(300)).await;

    // Check usage
    let usage = manager.get_usage().await?;
    assert!(usage.file_count >= 5);

    // Stop monitoring
    manager.stop_monitoring().await?;

    Ok(())
}

#[tokio::test]
async fn test_cleanup_triggered_by_threshold() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 1000,
        max_files: 100,
        cleanup_policy: CleanupPolicy {
            enabled: true,
            interval: Duration::from_secs(1),
            age_threshold: Duration::from_secs(0),
            usage_threshold_pct: 50.0, // Low threshold
            patterns: vec!["*.tmp".to_string()],
        },
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Create tmp files
    for i in 0..10 {
        let file = temp_dir.path().join(format!("file{}.tmp", i));
        fs::write(&file, "content")?;
    }

    // Trigger cleanup
    manager.cleanup_if_needed().await?;

    // tmp files should be cleaned
    for i in 0..10 {
        let file = temp_dir.path().join(format!("file{}.tmp", i));
        assert!(!file.exists(), "tmp files should be cleaned up");
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 100000,
        max_files: 1000,
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Spawn multiple concurrent space check tasks
    let mut check_handles = vec![];
    for _ in 0..5 {
        let mgr = manager.clone();
        let handle = tokio::spawn(async move { mgr.check_space(100).await });
        check_handles.push(handle);
    }

    // Wait for all checks
    for handle in check_handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent space checks should succeed");
    }

    // Spawn multiple concurrent reservation tasks
    let mut reserve_handles = vec![];
    for _ in 0..5 {
        let mgr = manager.clone();
        let handle = tokio::spawn(async move { mgr.reserve_space(100).await });
        reserve_handles.push(handle);
    }

    // Wait for all reservations
    for handle in reserve_handles {
        let result = handle.await.unwrap();
        assert!(result.is_ok(), "Concurrent reservations should succeed");
    }

    Ok(())
}

#[tokio::test]
async fn test_storage_recovery_after_cleanup() -> Result<()> {
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

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Fill up with tmp files
    for i in 0..20 {
        let file = temp_dir.path().join(format!("file{}.tmp", i));
        fs::write(&file, "a".repeat(40))?;
    }

    // Should be near quota
    let result = manager.check_space(500).await;
    assert!(result.is_err(), "Should be near quota limit");

    // Cleanup
    manager.cleanup_if_needed().await?;

    // Should have space now
    let result = manager.check_space(500).await;
    assert!(result.is_ok(), "Should have space after cleanup");

    Ok(())
}

#[tokio::test]
async fn test_monitoring_lifecycle() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 10000,
        max_files: 100,
        monitoring: StorageMonitoring {
            enabled: true,
            check_interval: Duration::from_millis(50),
            alert_thresholds: AlertThresholds::default(),
        },
        ..Default::default()
    };

    let mut manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Start and stop multiple times
    for _ in 0..3 {
        manager.start_monitoring().await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        manager.stop_monitoring().await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_usage_reporting_accuracy() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 10000,
        max_files: 100,
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Create known files
    let file1 = temp_dir.path().join("file1.txt");
    fs::write(&file1, "a".repeat(100))?;

    let file2 = temp_dir.path().join("file2.txt");
    fs::write(&file2, "b".repeat(200))?;

    // Check usage
    let usage = manager.get_usage().await?;
    assert_eq!(usage.file_count, 2);
    // Size should be approximately 300 bytes
    assert!(usage.used_bytes >= 300 && usage.used_bytes <= 310);

    Ok(())
}

#[tokio::test]
async fn test_reservation_expiration_handling() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 1000,
        max_files: 100,
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Create multiple reservations
    let res1 = manager.reserve_space(200).await?;
    let res2 = manager.reserve_space(200).await?;
    let res3 = manager.reserve_space(200).await?;

    // Total reserved: 600
    // Should fail to allocate 500 more (600 + 500 > 1000)
    let result = manager.check_space(500).await;
    assert!(result.is_err());

    // Release reservations
    manager.release_space(res1).await?;
    manager.release_space(res2).await?;
    manager.release_space(res3).await?;

    // Should succeed now
    let result = manager.check_space(500).await;
    assert!(result.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_nested_directory_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 10000,
        max_files: 100,
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Create nested structure
    let sub1 = temp_dir.path().join("sub1");
    fs::create_dir(&sub1)?;
    let sub2 = sub1.join("sub2");
    fs::create_dir(&sub2)?;

    // Create files at different levels
    fs::write(temp_dir.path().join("root.txt"), "root")?;
    fs::write(sub1.join("level1.txt"), "level1")?;
    fs::write(sub2.join("level2.txt"), "level2")?;

    // Check usage counts all files
    let usage = manager.get_usage().await?;
    assert_eq!(usage.file_count, 3);

    Ok(())
}

#[tokio::test]
async fn test_cleanup_preserves_non_tmp_files() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 10000,
        max_files: 100,
        cleanup_policy: CleanupPolicy {
            enabled: true,
            interval: Duration::from_secs(1),
            age_threshold: Duration::from_secs(0),
            usage_threshold_pct: 10.0,
            patterns: vec!["*.tmp".to_string()],
        },
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Create mixed files
    let tmp_file = temp_dir.path().join("temp.tmp");
    fs::write(&tmp_file, "temporary")?;

    let txt_file = temp_dir.path().join("important.txt");
    fs::write(&txt_file, "important data")?;

    // Run cleanup
    manager.cleanup_if_needed().await?;

    // Check results
    assert!(!tmp_file.exists(), "tmp file should be removed");
    assert!(txt_file.exists(), "txt file should be preserved");

    Ok(())
}

#[tokio::test]
async fn test_high_usage_scenario() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 1000,
        max_files: 100,
        cleanup_policy: CleanupPolicy {
            enabled: true,
            interval: Duration::from_secs(1),
            age_threshold: Duration::from_secs(0),
            usage_threshold_pct: 80.0,
            patterns: vec!["*.tmp".to_string()],
        },
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Fill to 90% with tmp files
    let tmp_file = temp_dir.path().join("big.tmp");
    fs::write(&tmp_file, "a".repeat(900))?;

    let usage_before = manager.get_usage().await?;
    assert!(usage_before.usage_pct > 80.0);

    // Cleanup should trigger
    manager.cleanup_if_needed().await?;

    let usage_after = manager.get_usage().await?;
    assert!(usage_after.usage_pct < usage_before.usage_pct);

    Ok(())
}

#[tokio::test]
async fn test_empty_directory_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig::default();

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // All operations should work on empty directory
    manager.check_space(100).await?;
    let reservation = manager.reserve_space(100).await?;
    let usage = manager.get_usage().await?;
    manager.cleanup_if_needed().await?;

    assert_eq!(usage.used_bytes, 0);
    assert_eq!(usage.file_count, 0);

    manager.release_space(reservation).await?;

    Ok(())
}

#[tokio::test]
async fn test_storage_manager_clone_operations() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 10000,
        max_files: 100,
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Create a file
    fs::write(temp_dir.path().join("test.txt"), "content")?;

    // Clone should see the same state
    let usage1 = manager.get_usage().await?;
    let usage2 = manager.get_usage().await?;

    assert_eq!(usage1.file_count, usage2.file_count);
    assert_eq!(usage1.used_bytes, usage2.used_bytes);

    Ok(())
}

#[tokio::test]
async fn test_rapid_file_creation_and_cleanup() -> Result<()> {
    let temp_dir = TempDir::new()?;
    let config = StorageConfig {
        max_disk_space_bytes: 100000,
        max_files: 1000,
        cleanup_policy: CleanupPolicy {
            enabled: true,
            interval: Duration::from_millis(100),
            age_threshold: Duration::from_secs(0),
            usage_threshold_pct: 50.0,
            patterns: vec!["*.tmp".to_string()],
        },
        ..Default::default()
    };

    let manager = StorageManager::new(
        config,
        "test_tenant".to_string(),
        temp_dir.path().to_path_buf(),
    )?;

    // Rapidly create and cleanup files
    for _ in 0..3 {
        // Create files
        for i in 0..10 {
            let file = temp_dir.path().join(format!("rapid{}.tmp", i));
            fs::write(&file, "data")?;
        }

        // Cleanup
        manager.cleanup_if_needed().await?;
    }

    // Final usage should be low
    let usage = manager.get_usage().await?;
    assert!(usage.file_count < 10);

    Ok(())
}
