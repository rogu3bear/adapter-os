//! Quota management tests
//!
//! Tests for disk quota enforcement, space reservation, and usage calculation.

use super::new_test_tempdir;
use crate::quota::QuotaManager;
use crate::StorageConfig;
use adapteros_core::Result;

#[tokio::test]
async fn test_quota_manager_creation() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let config = StorageConfig {
        max_disk_space_bytes: 1000,
        max_files: 100,
        ..Default::default()
    };

    let quota_manager = QuotaManager::new(&config, temp_dir.path())?;
    assert!(
        quota_manager.check_space(100).await.is_ok(),
        "Should allow small allocation"
    );

    Ok(())
}

#[tokio::test]
async fn test_space_reservation() -> Result<()> {
    let temp_dir = new_test_tempdir()?;
    let config = StorageConfig {
        max_disk_space_bytes: 1000,
        max_files: 100,
        ..Default::default()
    };

    let quota_manager = QuotaManager::new(&config, temp_dir.path())?;

    // Test space reservation
    let reservation = quota_manager.reserve_space(500).await?;
    assert_eq!(reservation.size, 500);

    Ok(())
}
