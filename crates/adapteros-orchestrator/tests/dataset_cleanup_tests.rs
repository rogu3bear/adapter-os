//! Tests for dataset cleanup and storage management

use adapteros_orchestrator::{CleanupConfig, StorageQuotaStatus};
use tempfile::TempDir;

// Note: Database tests requiring migrations are skipped due to migration signature verification.
// Unit tests for configuration and status structures are included below.

#[test]
fn test_cleanup_config_creation() {
    let temp_dir = TempDir::new().unwrap();

    let config = CleanupConfig {
        dataset_storage_path: temp_dir.path().to_path_buf(),
        quota_per_tenant_bytes: 1_000_000_000,
        archive_age_days: 30,
        auto_cleanup_on_startup: false,
        cleanup_interval_secs: 0,
    };

    assert_eq!(config.quota_per_tenant_bytes, 1_000_000_000);
    assert_eq!(config.archive_age_days, 30);
    assert!(!config.auto_cleanup_on_startup);
    assert_eq!(config.cleanup_interval_secs, 0);
}

#[test]
fn test_cleanup_config_defaults() {
    let config = CleanupConfig::default();

    assert_eq!(config.quota_per_tenant_bytes, 100 * 1024 * 1024 * 1024);
    assert_eq!(config.archive_age_days, 30);
    assert!(config.auto_cleanup_on_startup);
    assert_eq!(config.cleanup_interval_secs, 3600);
}

#[test]
fn test_quota_status_critical_threshold() {
    let status = StorageQuotaStatus {
        tenant_id: "test".to_string(),
        used_bytes: 900,
        quota_bytes: 1000,
        percent_used: 90.0,
        datasets_count: 1,
        is_over_quota: false,
    };

    assert!(status.is_critical());
    assert!(status.is_high());
}

#[test]
fn test_quota_status_high_threshold() {
    let status = StorageQuotaStatus {
        tenant_id: "test".to_string(),
        used_bytes: 800,
        quota_bytes: 1000,
        percent_used: 80.0,
        datasets_count: 2,
        is_over_quota: false,
    };

    assert!(!status.is_critical());
    assert!(status.is_high());
}

#[test]
fn test_quota_status_normal() {
    let status = StorageQuotaStatus {
        tenant_id: "test".to_string(),
        used_bytes: 500,
        quota_bytes: 1000,
        percent_used: 50.0,
        datasets_count: 1,
        is_over_quota: false,
    };

    assert!(!status.is_critical());
    assert!(!status.is_high());
}

#[test]
fn test_quota_status_over_quota() {
    let status = StorageQuotaStatus {
        tenant_id: "test".to_string(),
        used_bytes: 1100,
        quota_bytes: 1000,
        percent_used: 110.0,
        datasets_count: 1,
        is_over_quota: true,
    };

    assert!(status.is_over_quota);
    assert!(status.is_critical());
}

#[test]
fn test_storage_quota_status_display() {
    let status = StorageQuotaStatus {
        tenant_id: "production".to_string(),
        used_bytes: 5_368_709_120,   // 5 GB
        quota_bytes: 10_737_418_240, // 10 GB
        percent_used: 50.0,
        datasets_count: 42,
        is_over_quota: false,
    };

    assert_eq!(status.tenant_id, "production");
    assert_eq!(status.datasets_count, 42);
    assert!(!status.is_over_quota);
    assert_eq!(status.percent_used, 50.0);
}
