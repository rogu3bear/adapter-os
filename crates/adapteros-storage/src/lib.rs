//! Storage management and disk space enforcement
//!
//! Provides disk quota enforcement, storage cleanup policies, and monitoring
//! for AdapterOS training and adapter storage.

pub mod cleanup;
pub mod monitor;
pub mod policy;
pub mod quota;

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tracing::info;

/// Storage configuration for a tenant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Maximum disk space in bytes
    pub max_disk_space_bytes: u64,
    /// Maximum number of files
    pub max_files: u32,
    /// Cleanup policy
    pub cleanup_policy: CleanupPolicy,
    /// Monitoring configuration
    pub monitoring: StorageMonitoring,
    /// Enable encryption by default
    pub enable_encryption: bool,
    /// Key provider configuration
    pub key_provider: adapteros_crypto::KeyProviderConfig,
}

/// Cleanup policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPolicy {
    /// Enable automatic cleanup
    pub enabled: bool,
    /// Cleanup interval
    pub interval: Duration,
    /// Age threshold for cleanup
    pub age_threshold: Duration,
    /// Maximum disk usage percentage before cleanup
    pub usage_threshold_pct: f32,
    /// File patterns to clean up
    pub patterns: Vec<String>,
}

/// Storage monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMonitoring {
    /// Enable monitoring
    pub enabled: bool,
    /// Check interval
    pub check_interval: Duration,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
}

/// Alert thresholds for storage monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Warning threshold percentage
    pub warning_pct: f32,
    /// Critical threshold percentage
    pub critical_pct: f32,
    /// Emergency threshold percentage
    pub emergency_pct: f32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            max_disk_space_bytes: 10 * 1024 * 1024 * 1024, // 10GB
            max_files: 10000,
            cleanup_policy: CleanupPolicy::default(),
            monitoring: StorageMonitoring::default(),
            enable_encryption: true, // Encryption enabled by default
            key_provider: adapteros_crypto::KeyProviderConfig::default(),
        }
    }
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(3600), // 1 hour
            age_threshold: Duration::from_secs(7 * 24 * 3600), // 7 days
            usage_threshold_pct: 80.0,
            patterns: vec![
                "*.tmp".to_string(),
                "*.log".to_string(),
                "*.cache".to_string(),
            ],
        }
    }
}

impl Default for StorageMonitoring {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: Duration::from_secs(300), // 5 minutes
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            warning_pct: 70.0,
            critical_pct: 85.0,
            emergency_pct: 95.0,
        }
    }
}

/// Storage manager for a tenant
pub struct StorageManager {
    config: StorageConfig,
    tenant_id: String,
    root_path: PathBuf,
    quota_manager: quota::QuotaManager,
    cleanup_manager: cleanup::CleanupManager,
    monitor: monitor::StorageMonitor,
    key_provider: Option<Box<dyn adapteros_crypto::KeyProvider + Send + Sync>>,
}

impl StorageManager {
    /// Create a new storage manager
    pub fn new(config: StorageConfig, tenant_id: String, root_path: PathBuf) -> Result<Self> {
        let quota_manager = quota::QuotaManager::new(&config, &root_path)?;
        let cleanup_manager = cleanup::CleanupManager::new(&config, &root_path)?;
        let monitor = monitor::StorageMonitor::new(&config, &root_path)?;

        Ok(Self {
            config,
            tenant_id,
            root_path,
            quota_manager,
            cleanup_manager,
            monitor,
            key_provider: None,
        })
    }

    /// Initialize the key provider (async operation)
    pub async fn init_key_provider(&mut self) -> Result<()> {
        if self.config.enable_encryption {
            // Create the appropriate key provider based on config
            let provider: Box<dyn adapteros_crypto::KeyProvider + Send + Sync> =
                match self.config.key_provider.mode {
                    adapteros_crypto::KeyProviderMode::Keychain => Box::new(
                        adapteros_crypto::KeychainProvider::new(self.config.key_provider.clone())?,
                    ),
                    adapteros_crypto::KeyProviderMode::Kms => {
                        return Err(adapteros_core::AosError::Crypto(
                            "KMS provider not yet implemented".to_string(),
                        ));
                    }
                    adapteros_crypto::KeyProviderMode::File => {
                        return Err(adapteros_core::AosError::Crypto(
                            "File provider not allowed in production".to_string(),
                        ));
                    }
                };

            self.key_provider = Some(provider);
            info!("Initialized key provider for encrypted storage operations");
        }

        Ok(())
    }

    /// Check if there's enough space for a file
    pub async fn check_space(&self, file_size: u64) -> Result<()> {
        self.quota_manager.check_space(file_size).await
    }

    /// Reserve space for a file
    pub async fn reserve_space(&self, file_size: u64) -> Result<quota::SpaceReservation> {
        self.quota_manager.reserve_space(file_size).await
    }

    /// Release reserved space
    pub async fn release_space(&self, reservation: quota::SpaceReservation) -> Result<()> {
        self.quota_manager.release_space(reservation).await
    }

    /// Run cleanup if needed
    pub async fn cleanup_if_needed(&self) -> Result<()> {
        self.cleanup_manager.cleanup_if_needed().await
    }

    /// Get current storage usage
    pub async fn get_usage(&self) -> Result<StorageUsage> {
        self.monitor.get_usage().await
    }

    /// Start monitoring
    pub async fn start_monitoring(&mut self) -> Result<()> {
        self.monitor.start_monitoring().await
    }

    /// Stop monitoring
    pub async fn stop_monitoring(&mut self) -> Result<()> {
        self.monitor.stop_monitoring().await
    }
}

/// Current storage usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageUsage {
    /// Total disk space used in bytes
    pub used_bytes: u64,
    /// Total disk space available in bytes
    pub available_bytes: u64,
    /// Number of files
    pub file_count: u32,
    /// Usage percentage
    pub usage_pct: f32,
    /// Last updated timestamp
    pub last_updated: SystemTime,
}

impl StorageUsage {
    /// Check if usage exceeds threshold
    pub fn exceeds_threshold(&self, threshold_pct: f32) -> bool {
        self.usage_pct > threshold_pct
    }

    /// Get remaining space in bytes
    pub fn remaining_bytes(&self) -> u64 {
        self.available_bytes.saturating_sub(self.used_bytes)
    }
}
