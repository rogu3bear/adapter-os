//! Disk quota enforcement
//!
//! Implements disk space reservation and enforcement for tenant storage.

use crate::{StorageConfig, StorageUsage};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tracing::{debug, warn};

/// Quota manager for enforcing disk space limits
#[derive(Clone)]
pub struct QuotaManager {
    config: StorageConfig,
    root_path: PathBuf,
    reservations: Arc<RwLock<HashMap<String, SpaceReservation>>>,
    usage_cache: Arc<Mutex<Option<StorageUsage>>>,
    cache_ttl: Duration,
}

/// Space reservation for a file operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceReservation {
    /// Reservation ID
    pub id: String,
    /// Reserved space in bytes
    pub size: u64,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Expiration timestamp
    pub expires_at: SystemTime,
    /// File path being reserved
    pub file_path: PathBuf,
}

impl QuotaManager {
    /// Create a new quota manager
    pub fn new(config: &StorageConfig, root_path: &Path) -> Result<Self> {
        Ok(Self {
            config: config.clone(),
            root_path: root_path.to_path_buf(),
            reservations: Arc::new(RwLock::new(HashMap::new())),
            usage_cache: Arc::new(Mutex::new(None)),
            cache_ttl: Duration::from_secs(60), // 1 minute cache
        })
    }

    /// Check if there's enough space for a file
    pub async fn check_space(&self, file_size: u64) -> Result<()> {
        let usage = self.get_current_usage()?;
        let reserved = self.get_total_reserved().await?;
        let total_needed = usage.used_bytes + reserved + file_size;

        if total_needed > self.config.max_disk_space_bytes {
            return Err(AosError::Io(format!(
                "Insufficient disk space: need {} bytes, available {} bytes (reserved: {} bytes)",
                total_needed, self.config.max_disk_space_bytes, reserved
            )));
        }

        Ok(())
    }

    /// Reserve space for a file
    pub async fn reserve_space(&self, file_size: u64) -> Result<SpaceReservation> {
        // Check if we can reserve this space
        self.check_space(file_size).await?;

        let reservation_id = format!(
            "reservation_{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let now = SystemTime::now();
        let expires_at = now + Duration::from_secs(3600); // 1 hour expiration

        let reservation = SpaceReservation {
            id: reservation_id.clone(),
            size: file_size,
            created_at: now,
            expires_at,
            file_path: PathBuf::new(), // Will be set when file is created
        };

        // Store reservation
        {
            let mut reservations = self.reservations.write().await;
            reservations.insert(reservation_id.clone(), reservation.clone());
        }

        debug!("Reserved {} bytes for file operation", file_size);
        Ok(reservation)
    }

    /// Release reserved space
    pub async fn release_space(&self, reservation: SpaceReservation) -> Result<()> {
        let mut reservations = self.reservations.write().await;
        reservations.remove(&reservation.id);

        debug!(
            "Released {} bytes from reservation {}",
            reservation.size, reservation.id
        );
        Ok(())
    }

    /// Get current storage usage
    fn get_current_usage(&self) -> Result<StorageUsage> {
        // Check cache first
        {
            let cache = self.usage_cache.lock().unwrap();
            if let Some(ref usage) = *cache {
                if usage.last_updated.elapsed().unwrap_or(Duration::MAX) < self.cache_ttl {
                    return Ok(usage.clone());
                }
            }
        }

        // Calculate usage
        let usage = self.calculate_usage()?;

        // Update cache
        {
            let mut cache = self.usage_cache.lock().unwrap();
            *cache = Some(usage.clone());
        }

        Ok(usage)
    }

    /// Calculate current storage usage
    fn calculate_usage(&self) -> Result<StorageUsage> {
        let mut used_bytes = 0u64;
        let mut file_count = 0u32;

        if !self.root_path.exists() {
            return Ok(StorageUsage {
                used_bytes: 0,
                available_bytes: self.config.max_disk_space_bytes,
                file_count: 0,
                usage_pct: 0.0,
                last_updated: SystemTime::now(),
            });
        }

        // Walk directory tree
        Self::walk_directory(self.root_path.as_path(), &mut used_bytes, &mut file_count)?;

        let usage_pct = (used_bytes as f32 / self.config.max_disk_space_bytes as f32) * 100.0;

        Ok(StorageUsage {
            used_bytes,
            available_bytes: self.config.max_disk_space_bytes,
            file_count,
            usage_pct,
            last_updated: SystemTime::now(),
        })
    }

    /// Walk directory tree to calculate usage
    fn walk_directory(path: &Path, used_bytes: &mut u64, file_count: &mut u32) -> Result<()> {
        let entries = std::fs::read_dir(path).map_err(|e| {
            AosError::Io(format!(
                "Failed to read directory {}: {}",
                path.display(),
                e
            ))
        })?;

        for entry in entries {
            let entry = entry
                .map_err(|e| AosError::Io(format!("Failed to read directory entry: {}", e)))?;
            let entry_path = entry.path();

            if entry_path.is_file() {
                let metadata = entry.metadata().map_err(|e| {
                    AosError::Io(format!(
                        "Failed to read file metadata {}: {}",
                        entry_path.display(),
                        e
                    ))
                })?;

                *used_bytes += metadata.len();
                *file_count += 1;
            } else if entry_path.is_dir() {
                Self::walk_directory(entry_path.as_path(), used_bytes, file_count)?;
            }
        }

        Ok(())
    }

    /// Get total reserved space
    async fn get_total_reserved(&self) -> Result<u64> {
        let reservations = self.reservations.read().await;
        let mut total = 0u64;

        for reservation in reservations.values() {
            // Check if reservation is still valid
            if reservation.expires_at > SystemTime::now() {
                total += reservation.size;
            }
        }

        Ok(total)
    }

    /// Clean up expired reservations
    pub async fn cleanup_expired_reservations(&self) -> Result<()> {
        let mut reservations = self.reservations.write().await;
        let now = SystemTime::now();
        let mut expired_count = 0;

        reservations.retain(|_, reservation| {
            if reservation.expires_at <= now {
                expired_count += 1;
                false
            } else {
                true
            }
        });

        if expired_count > 0 {
            warn!("Cleaned up {} expired space reservations", expired_count);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_quota_manager() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = StorageConfig {
            max_disk_space_bytes: 1000,
            max_files: 100,
            ..Default::default()
        };

        let quota_manager = QuotaManager::new(&config, &temp_dir.path())?;

        // Test space reservation
        let reservation = quota_manager.reserve_space(500).await?;
        assert_eq!(reservation.size, 500);

        // Test space check
        quota_manager.check_space(400).await?; // Should pass (500 + 400 = 900 < 1000)

        // Test space limit
        let result = quota_manager.check_space(600).await; // Should fail (500 + 600 = 1100 > 1000)
        assert!(result.is_err());

        // Test reservation release
        quota_manager.release_space(reservation).await?;

        // Test space check after release
        quota_manager.check_space(600).await?; // Should pass now

        Ok(())
    }

    #[tokio::test]
    async fn test_usage_calculation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let config = StorageConfig {
            max_disk_space_bytes: 1000,
            max_files: 100,
            ..Default::default()
        };

        let quota_manager = QuotaManager::new(&config, &temp_dir.path())?;

        // Create test files
        let test_file1 = temp_dir.path().join("test1.txt");
        fs::write(&test_file1, "hello")?;

        let test_file2 = temp_dir.path().join("test2.txt");
        fs::write(&test_file2, "world")?;

        // Check usage
        let usage = quota_manager.get_current_usage()?;
        assert!(usage.used_bytes > 0);
        assert_eq!(usage.file_count, 2);
        assert!(usage.usage_pct > 0.0);

        Ok(())
    }
}
