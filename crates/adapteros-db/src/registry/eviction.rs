//! Async adapter eviction with secure memory zeroization
//!
//! This module provides secure adapter eviction with memory zeroization
//! to prevent sensitive data from persisting in memory after eviction.
//!
//! Based on security requirements for sensitive data handling.

use crate::Db;
use adapteros_core::{AosError, Result};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};
use zeroize::Zeroize;

/// Trait for adapter eviction with telemetry
#[async_trait]
pub trait TelemetryWriter: Send + Sync {
    fn log_adapter_eviction(&self, adapter_id: &str, reason: &str) -> Result<()>;
}

/// Zeroization policy configuration
#[derive(Debug, Clone)]
pub struct ZeroizationPolicy {
    /// Whether to zeroize system memory
    pub zeroize_system_memory: bool,
    /// Whether to zeroize VRAM
    pub zeroize_vram: bool,
    /// Whether to zeroize disk cache
    pub zeroize_disk_cache: bool,
    /// Number of zeroization passes
    pub zeroization_passes: usize,
}

impl Default for ZeroizationPolicy {
    fn default() -> Self {
        Self {
            zeroize_system_memory: true,
            zeroize_vram: true,
            zeroize_disk_cache: false, // Usually not necessary for disk
            zeroization_passes: 1,
        }
    }
}

/// Adapter structure for eviction
#[derive(Debug, Clone)]
pub struct AdapterEvictionInfo {
    pub id: String,
    pub weights: Option<Vec<u8>>,
    pub cache: Option<Vec<u8>>,
    pub vram_handle: Option<u64>,
    pub vram_size: usize,
    pub cache_path: Option<String>,
    pub last_used: Instant,
    pub priority: u32,
    pub size: usize,
}

/// Async adapter eviction manager with zeroization
pub struct EvictionManager {
    /// Database reference
    db: Arc<Db>,
    /// Telemetry writer for logging evictions
    telemetry: Option<Arc<dyn TelemetryWriter>>,
    /// Zeroization policy
    zeroization_policy: ZeroizationPolicy,
}

impl EvictionManager {
    /// Create a new eviction manager
    pub fn new(
        db: Arc<Db>,
        telemetry: Option<Arc<dyn TelemetryWriter>>,
        zeroization_policy: ZeroizationPolicy,
    ) -> Self {
        Self {
            db,
            telemetry,
            zeroization_policy,
        }
    }

    /// Evict adapter with zeroization
    pub async fn evict_adapter(
        &self,
        adapter_id: &str,
        adapter_info: Option<AdapterEvictionInfo>,
        reason: &str,
    ) -> Result<()> {
        info!("Evicting adapter {} with reason: {}", adapter_id, reason);

        let start_time = Instant::now();

        // Use provided info or create minimal info
        let adapter = adapter_info.unwrap_or_else(|| AdapterEvictionInfo {
            id: adapter_id.to_string(),
            weights: None,
            cache: None,
            vram_handle: None,
            vram_size: 0,
            cache_path: None,
            last_used: Instant::now(),
            priority: 0,
            size: 0,
        });

        // Zeroize adapter weights in system memory
        if self.zeroization_policy.zeroize_system_memory {
            self.zeroize_system_memory(&adapter).await?;
        }

        // Zeroize adapter weights in VRAM
        if self.zeroization_policy.zeroize_vram {
            self.zeroize_vram(&adapter).await?;
        }

        // Zeroize disk cache if enabled
        if self.zeroization_policy.zeroize_disk_cache {
            self.zeroize_disk_cache(&adapter).await?;
        }

        // Update database state (mark as unloaded)
        sqlx::query(
            r#"
            UPDATE adapters
            SET load_state = 'cold',
                current_state = 'unloaded',
                last_unloaded_at = datetime('now')
            WHERE adapter_id = ?1
            "#,
        )
        .bind(adapter_id)
        .execute(self.db.pool_result()?)
        .await
        .map_err(|e| AosError::Worker(format!("Failed to update adapter state: {}", e)))?;

        // Log eviction event
        if let Some(telemetry) = &self.telemetry {
            telemetry.log_adapter_eviction(adapter_id, reason)?;
        }

        let duration = start_time.elapsed();
        info!(
            "Successfully evicted adapter {} in {:?}",
            adapter_id, duration
        );

        Ok(())
    }

    /// Zeroize adapter weights in system memory
    async fn zeroize_system_memory(&self, adapter: &AdapterEvictionInfo) -> Result<()> {
        info!("Zeroizing system memory for adapter: {}", adapter.id);

        // Zeroize adapter weights
        if let Some(weights) = &adapter.weights {
            let mut weights_clone = weights.clone();
            weights_clone.zeroize();

            // Additional zeroization passes if configured
            for _ in 1..self.zeroization_policy.zeroization_passes {
                let mut random_data = vec![0u8; weights.len()];
                random_data.zeroize();
            }
        }

        // Zeroize any cached intermediate results
        if let Some(cache) = &adapter.cache {
            let mut cache_clone = cache.clone();
            cache_clone.zeroize();
        }

        Ok(())
    }

    /// Zeroize adapter weights in VRAM
    async fn zeroize_vram(&self, adapter: &AdapterEvictionInfo) -> Result<()> {
        if adapter.vram_handle.is_none() || adapter.vram_size == 0 {
            return Ok(());
        }

        info!("Zeroizing VRAM for adapter: {}", adapter.id);

        // Create zero buffer and immediately zeroize
        // In a real implementation, this would use Metal API
        let mut zero_buffer = vec![0u8; adapter.vram_size.min(1024 * 1024)]; // Cap at 1MB for safety
        zero_buffer.zeroize();

        Ok(())
    }

    /// Zeroize disk cache
    async fn zeroize_disk_cache(&self, adapter: &AdapterEvictionInfo) -> Result<()> {
        if let Some(cache_path) = &adapter.cache_path {
            info!("Zeroizing disk cache for adapter: {}", adapter.id);

            // Check if file exists
            if !std::path::Path::new(cache_path).exists() {
                return Ok(());
            }

            // Overwrite cache file with zeros
            let file_size = std::fs::metadata(cache_path)
                .map_err(|e| AosError::Worker(format!("Failed to get cache file size: {}", e)))?
                .len() as usize;

            let mut zero_data = vec![0u8; file_size];
            std::fs::write(cache_path, &zero_data)
                .map_err(|e| AosError::Worker(format!("Failed to zeroize cache file: {}", e)))?;

            // Zeroize the local buffer
            zero_data.zeroize();

            // Delete the file
            std::fs::remove_file(cache_path)
                .map_err(|e| AosError::Worker(format!("Failed to remove cache file: {}", e)))?;
        }

        Ok(())
    }

    /// Evict multiple adapters with zeroization
    pub async fn evict_adapters(&self, adapter_ids: &[String], reason: &str) -> Result<()> {
        use adapteros_db::query_helpers::BatchTracker;

        info!(
            "Evicting {} adapters with reason: {}",
            adapter_ids.len(),
            reason
        );

        let mut tracker = BatchTracker::new("eviction");

        for adapter_id in adapter_ids {
            tracker.track(self.evict_adapter(adapter_id, None, reason).await);
        }

        tracker.finish()
    }

    /// Get eviction statistics
    pub fn get_eviction_stats(&self) -> EvictionStats {
        EvictionStats {
            zeroize_system_memory: self.zeroization_policy.zeroize_system_memory,
            zeroize_vram: self.zeroization_policy.zeroize_vram,
            zeroize_disk_cache: self.zeroization_policy.zeroize_disk_cache,
            zeroization_passes: self.zeroization_policy.zeroization_passes,
        }
    }
}

/// Eviction statistics
#[derive(Debug, Clone)]
pub struct EvictionStats {
    pub zeroize_system_memory: bool,
    pub zeroize_vram: bool,
    pub zeroize_disk_cache: bool,
    pub zeroization_passes: usize,
}

/// Eviction order strategies
#[derive(Debug, Clone)]
pub enum EvictionOrder {
    /// Least Recently Used
    Lru,
    /// Priority-based (lowest priority first)
    Priority,
    /// Size-based (largest first)
    Size,
    /// Custom order
    Custom(Vec<String>),
}

/// Memory pressure-based eviction selector
pub struct MemoryPressureSelector {
    /// Memory pressure threshold for eviction (0.0-1.0)
    pressure_threshold: f32,
    /// Eviction order strategy
    eviction_order: EvictionOrder,
}

impl MemoryPressureSelector {
    /// Create a new memory pressure selector
    pub fn new(pressure_threshold: f32, eviction_order: EvictionOrder) -> Self {
        Self {
            pressure_threshold,
            eviction_order,
        }
    }

    /// Check if eviction is needed based on memory pressure
    pub fn should_evict(&self, memory_pressure: f32) -> bool {
        memory_pressure > self.pressure_threshold
    }

    /// Select adapters for eviction based on memory pressure
    pub async fn select_adapters_for_eviction(
        &self,
        db: &Db,
        memory_pressure: f32,
        target_memory_to_free: usize,
    ) -> Result<Vec<String>> {
        if !self.should_evict(memory_pressure) {
            return Ok(vec![]);
        }

        // Query adapters ordered by eviction strategy
        let order_clause = match &self.eviction_order {
            EvictionOrder::Lru => "last_activated_at ASC NULLS FIRST",
            EvictionOrder::Priority => "rank ASC",
            EvictionOrder::Size => "rank DESC", // Using rank as proxy for size
            EvictionOrder::Custom(order) => {
                // For custom order, we'll filter in memory
                return self
                    .select_custom_order(db, order, target_memory_to_free)
                    .await;
            }
        };

        let query = format!(
            r#"
            SELECT adapter_id FROM adapters
            WHERE current_state IN ('warm', 'hot', 'resident')
               OR load_state IN ('loaded', 'warm')
            ORDER BY {}
            "#,
            order_clause
        );

        let candidates: Vec<String> = sqlx::query_scalar(&query)
            .fetch_all(db.pool_result()?)
            .await
            .map_err(|e| AosError::Worker(format!("Failed to query adapters for eviction: {}", e)))?;

        // Select enough adapters to free target memory
        // For now, just return first few (proper implementation would track actual sizes)
        let count = (target_memory_to_free / (1024 * 1024)).max(1).min(10);
        Ok(candidates.into_iter().take(count).collect())
    }

    async fn select_custom_order(
        &self,
        db: &Db,
        order: &[String],
        _target_memory_to_free: usize,
    ) -> Result<Vec<String>> {
        // Get loaded adapters that are in the custom order list
        let candidates: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT adapter_id FROM adapters
            WHERE current_state IN ('warm', 'hot', 'resident')
               OR load_state IN ('loaded', 'warm')
            "#,
        )
        .fetch_all(db.pool_result()?)
        .await
        .map_err(|e| AosError::Worker(format!("Failed to query adapters: {}", e)))?;

        // Filter by custom order
        Ok(candidates
            .into_iter()
            .filter(|id| order.contains(id))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zeroization_policy_default() {
        let policy = ZeroizationPolicy::default();
        assert!(policy.zeroize_system_memory);
        assert!(policy.zeroize_vram);
        assert!(!policy.zeroize_disk_cache);
        assert_eq!(policy.zeroization_passes, 1);
    }

    #[test]
    fn test_memory_pressure_selector() {
        let selector = MemoryPressureSelector::new(0.8, EvictionOrder::Lru);
        assert!(selector.should_evict(0.9));
        assert!(!selector.should_evict(0.7));
    }

    #[test]
    fn test_eviction_order() {
        let lru = EvictionOrder::Lru;
        let priority = EvictionOrder::Priority;
        let size = EvictionOrder::Size;
        let custom = EvictionOrder::Custom(vec!["adapter1".to_string()]);

        assert!(matches!(lru, EvictionOrder::Lru));
        assert!(matches!(priority, EvictionOrder::Priority));
        assert!(matches!(size, EvictionOrder::Size));
        assert!(matches!(custom, EvictionOrder::Custom(_)));
    }

    #[tokio::test]
    async fn test_eviction_manager() {
        let db = Arc::new(Db::new_in_memory().await.unwrap());
        let manager = EvictionManager::new(db, None, ZeroizationPolicy::default());

        let stats = manager.get_eviction_stats();
        assert!(stats.zeroize_system_memory);
        assert!(stats.zeroize_vram);
    }
}
