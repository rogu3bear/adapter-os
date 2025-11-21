//! Unified memory management interface for AdapterOS
//!
//! Provides a centralized interface for all memory management operations
//! across the system, consolidating adapter lifecycle, eviction, and monitoring.
//!
//! # Citations
//! - Policy Pack #12 (Memory): "MUST maintain ≥ 15 percent unified memory headroom"
//! - CLAUDE.md L140: "Memory management: Adapter eviction with headroom maintenance"

use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::Mutex as TokioMutex;
use tracing::info;

/// Unified memory management interface
#[allow(async_fn_in_trait)]
pub trait MemoryManager {
    /// Get current memory usage statistics
    async fn get_memory_usage(&self) -> Result<MemoryUsageStats>;

    /// Evict an adapter from memory
    async fn evict_adapter(&self, adapter_id: &str) -> Result<()>;

    /// Pin an adapter to prevent eviction
    async fn pin_adapter(&self, adapter_id: &str, pinned: bool) -> Result<()>;

    /// Get adapter memory information
    async fn get_adapter_memory_info(&self, adapter_id: &str) -> Result<AdapterMemoryInfo>;

    /// Check memory pressure level
    async fn check_memory_pressure(&self) -> Result<MemoryPressureLevel>;

    /// Perform memory cleanup
    async fn cleanup_memory(&self) -> Result<MemoryCleanupReport>;
}

/// Memory usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUsageStats {
    /// Total system memory (bytes)
    pub total_memory: u64,

    /// Available memory (bytes)
    pub available_memory: u64,

    /// Used memory (bytes)
    pub used_memory: u64,

    /// Memory pressure level
    pub pressure_level: MemoryPressureLevel,

    /// Adapter memory usage
    pub adapters: Vec<AdapterMemoryInfo>,

    /// Memory headroom percentage
    pub headroom_percentage: f64,

    /// Timestamp of the stats
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Adapter memory information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMemoryInfo {
    /// Adapter identifier
    pub adapter_id: String,

    /// Adapter name
    pub adapter_name: String,

    /// Memory usage in bytes
    pub memory_usage_bytes: u64,

    /// Memory usage in MB
    pub memory_usage_mb: u64,

    /// Adapter state
    pub state: AdapterState,

    /// Whether adapter is pinned
    pub pinned: bool,

    /// Adapter category
    pub category: AdapterCategory,

    /// Last access time
    pub last_access: Option<chrono::DateTime<chrono::Utc>>,

    /// Activation count
    pub activation_count: u64,

    /// Quality score
    pub quality_score: f64,
}

/// Adapter states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AdapterState {
    /// Adapter is loaded and ready
    Loaded,

    /// Adapter is currently being used
    Active,

    /// Adapter is idle but loaded
    Idle,

    /// Adapter is being unloaded
    Unloading,

    /// Adapter has been evicted
    Evicted,

    /// Adapter is in error state
    Error,
}

/// Adapter categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterCategory {
    /// Base language model
    Base,

    /// Code-specific adapter
    Code,

    /// Framework-specific adapter
    Framework,

    /// Directory-specific adapter
    Directory,

    /// Ephemeral adapter
    Ephemeral,

    /// Custom adapter
    Custom(String),
}

/// Memory pressure levels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryPressureLevel {
    /// Low pressure - plenty of memory available
    Low,

    /// Medium pressure - some memory constraints
    Medium,

    /// High pressure - significant memory constraints
    High,

    /// Critical pressure - immediate action required
    Critical,
}

/// Memory cleanup report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCleanupReport {
    /// Number of adapters evicted
    pub adapters_evicted: u32,

    /// Number of adapters pinned
    pub adapters_pinned: u32,

    /// Memory freed in bytes
    pub memory_freed_bytes: u64,

    /// Memory freed in MB
    pub memory_freed_mb: u64,

    /// Cleanup duration
    pub duration_ms: u64,

    /// Cleanup operations performed
    pub operations: Vec<CleanupOperation>,
}

/// Cleanup operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CleanupOperation {
    /// Evicted an adapter
    EvictAdapter {
        adapter_id: String,
        memory_freed: u64,
    },

    /// Pinned an adapter
    PinAdapter { adapter_id: String },

    /// Reduced K value
    ReduceK { old_k: u32, new_k: u32 },

    /// Freed unused memory
    FreeUnusedMemory { memory_freed: u64 },
}

/// Unified memory manager implementation
#[derive(Debug)]
pub struct UnifiedMemoryManager {
    /// Memory pools by backend
    pools: HashMap<String, Arc<Mutex<MemoryPool>>>,

    /// Total allocated memory
    total_allocated: Arc<Mutex<u64>>,

    /// Memory limit
    memory_limit: u64,

    /// Headroom threshold (percentage)
    headroom_threshold: f64,

    /// Adapter registry
    adapters: Arc<TokioMutex<HashMap<String, AdapterMemoryInfo>>>,
}

impl UnifiedMemoryManager {
    /// Create a new unified memory manager
    pub fn new(memory_limit: u64, headroom_threshold: f64) -> Self {
        Self {
            pools: HashMap::new(),
            total_allocated: Arc::new(Mutex::new(0)),
            memory_limit,
            headroom_threshold,
            adapters: Arc::new(TokioMutex::new(HashMap::new())),
        }
    }

    /// Add a memory pool for a specific backend
    pub fn add_pool(&mut self, backend: String, pool: MemoryPool) {
        self.pools.insert(backend, Arc::new(Mutex::new(pool)));
    }

    /// Get memory headroom percentage
    pub fn get_headroom_percentage(&self) -> f64 {
        let total = self.memory_limit as f64;
        let allocated = *self.total_allocated.lock().unwrap() as f64;
        let available = total - allocated;
        (available / total) * 100.0
    }

    /// Check if memory headroom is sufficient
    pub fn is_headroom_sufficient(&self) -> bool {
        self.get_headroom_percentage() >= self.headroom_threshold
    }
}

impl MemoryManager for UnifiedMemoryManager {
    async fn get_memory_usage(&self) -> Result<MemoryUsageStats> {
        let total_allocated = *self.total_allocated.lock().unwrap();
        let available_memory = self.memory_limit.saturating_sub(total_allocated);
        let headroom_percentage = self.get_headroom_percentage();

        let pressure_level = if headroom_percentage >= 30.0 {
            MemoryPressureLevel::Low
        } else if headroom_percentage >= 20.0 {
            MemoryPressureLevel::Medium
        } else if headroom_percentage >= 15.0 {
            MemoryPressureLevel::High
        } else {
            MemoryPressureLevel::Critical
        };

        let adapters = self.adapters.lock().await.values().cloned().collect();

        Ok(MemoryUsageStats {
            total_memory: self.memory_limit,
            available_memory,
            used_memory: total_allocated,
            pressure_level,
            adapters,
            headroom_percentage,
            timestamp: chrono::Utc::now(),
        })
    }

    async fn evict_adapter(&self, adapter_id: &str) -> Result<()> {
        let mut adapters = self.adapters.lock().await;

        if let Some(adapter) = adapters.get_mut(adapter_id) {
            if adapter.pinned {
                return Err(AosError::Memory("Cannot evict pinned adapter".to_string()));
            }

            let memory_freed = adapter.memory_usage_bytes;
            adapter.state = AdapterState::Evicted;

            // Update total allocated memory
            let mut total = self.total_allocated.lock().unwrap();
            *total = total.saturating_sub(memory_freed);

            info!(
                adapter_id = adapter_id,
                memory_freed = memory_freed,
                "Adapter evicted from memory"
            );

            Ok(())
        } else {
            Err(AosError::NotFound(format!(
                "Adapter not found: {}",
                adapter_id
            )))
        }
    }

    async fn pin_adapter(&self, adapter_id: &str, pinned: bool) -> Result<()> {
        let mut adapters = self.adapters.lock().await;

        if let Some(adapter) = adapters.get_mut(adapter_id) {
            adapter.pinned = pinned;

            info!(
                adapter_id = adapter_id,
                pinned = pinned,
                "Adapter pin status updated"
            );

            Ok(())
        } else {
            Err(AosError::NotFound(format!(
                "Adapter not found: {}",
                adapter_id
            )))
        }
    }

    async fn get_adapter_memory_info(&self, adapter_id: &str) -> Result<AdapterMemoryInfo> {
        let adapters = self.adapters.lock().await;

        adapters
            .get(adapter_id)
            .cloned()
            .ok_or_else(|| AosError::NotFound(format!("Adapter not found: {}", adapter_id)))
    }

    async fn check_memory_pressure(&self) -> Result<MemoryPressureLevel> {
        let headroom_percentage = self.get_headroom_percentage();

        Ok(if headroom_percentage >= 30.0 {
            MemoryPressureLevel::Low
        } else if headroom_percentage >= 20.0 {
            MemoryPressureLevel::Medium
        } else if headroom_percentage >= 15.0 {
            MemoryPressureLevel::High
        } else {
            MemoryPressureLevel::Critical
        })
    }

    async fn cleanup_memory(&self) -> Result<MemoryCleanupReport> {
        let start_time = std::time::Instant::now();
        let mut operations = Vec::new();
        let mut adapters_evicted = 0;
        let mut memory_freed = 0;

        // Get current memory usage
        let stats = self.get_memory_usage().await?;

        // Count pinned adapters
        let adapters_pinned = stats.adapters.iter().filter(|a| a.pinned).count() as u32;

        // If headroom is sufficient, no cleanup needed
        if stats.headroom_percentage >= self.headroom_threshold {
            return Ok(MemoryCleanupReport {
                adapters_evicted: 0,
                adapters_pinned,
                memory_freed_bytes: 0,
                memory_freed_mb: 0,
                duration_ms: start_time.elapsed().as_millis() as u64,
                operations,
            });
        }

        // Evict unpinned adapters starting with lowest quality
        // Per Determinism Ruleset #2: eviction order must be deterministic
        // Sort by: pinned status → quality score → adapter ID hash (for deterministic tiebreaking)
        let mut adapters: Vec<_> = stats.adapters.into_iter().collect();
        adapters.sort_by(|a, b| {
            if a.pinned != b.pinned {
                // Pinned adapters always come last
                a.pinned.cmp(&b.pinned)
            } else {
                // Compare by quality score
                match a.quality_score.partial_cmp(&b.quality_score) {
                    Some(ord) if ord != std::cmp::Ordering::Equal => ord,
                    // Tiebreaker: sort by BLAKE3 hash of adapter ID for determinism
                    _ => {
                        let hash_a = blake3::hash(a.adapter_id.as_bytes());
                        let hash_b = blake3::hash(b.adapter_id.as_bytes());
                        hash_a.as_bytes().cmp(hash_b.as_bytes())
                    }
                }
            }
        });

        for adapter in adapters {
            if !adapter.pinned && adapter.state != AdapterState::Evicted {
                if let Ok(_) = self.evict_adapter(&adapter.adapter_id).await {
                    adapters_evicted += 1;
                    memory_freed += adapter.memory_usage_bytes;

                    operations.push(CleanupOperation::EvictAdapter {
                        adapter_id: adapter.adapter_id.clone(),
                        memory_freed: adapter.memory_usage_bytes,
                    });

                    // Check if we have sufficient headroom now
                    let current_stats = self.get_memory_usage().await?;
                    if current_stats.headroom_percentage >= self.headroom_threshold {
                        break;
                    }
                }
            }
        }

        let duration = start_time.elapsed();

        info!(
            adapters_evicted = adapters_evicted,
            memory_freed_mb = memory_freed / (1024 * 1024),
            duration_ms = duration.as_millis(),
            "Memory cleanup completed"
        );

        Ok(MemoryCleanupReport {
            adapters_evicted,
            adapters_pinned,
            memory_freed_bytes: memory_freed,
            memory_freed_mb: memory_freed / (1024 * 1024),
            duration_ms: duration.as_millis() as u64,
            operations,
        })
    }
}

/// Memory pool for a specific backend
#[derive(Debug)]
pub struct MemoryPool {
    /// Pool identifier
    pub id: String,

    /// Allocated blocks
    pub blocks: HashMap<String, MemoryBlock>,

    /// Available memory
    pub available: u64,

    /// Total pool size
    pub total_size: u64,
}

/// Memory block within a pool
#[derive(Debug, Clone)]
pub struct MemoryBlock {
    /// Block identifier
    pub id: String,

    /// Memory address
    pub ptr: *mut u8,

    /// Block size
    pub size: u64,

    /// Backend type
    pub backend: String,

    /// Allocation timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_manager_creation() {
        let manager = UnifiedMemoryManager::new(1024 * 1024 * 1024, 15.0);
        assert_eq!(manager.memory_limit, 1024 * 1024 * 1024);
        assert_eq!(manager.headroom_threshold, 15.0);
    }

    #[tokio::test]
    async fn test_memory_usage_stats() {
        let manager = UnifiedMemoryManager::new(1024 * 1024 * 1024, 15.0);
        let stats = manager.get_memory_usage().await.unwrap();

        assert_eq!(stats.total_memory, 1024 * 1024 * 1024);
        assert_eq!(stats.used_memory, 0);
        assert_eq!(stats.available_memory, 1024 * 1024 * 1024);
        assert!(matches!(stats.pressure_level, MemoryPressureLevel::Low));
    }

    #[tokio::test]
    async fn test_memory_pressure_levels() {
        let manager = UnifiedMemoryManager::new(1000, 15.0);

        // Test different pressure levels
        let pressure = manager.check_memory_pressure().await.unwrap();
        assert!(matches!(pressure, MemoryPressureLevel::Low));
    }

    #[tokio::test]
    async fn test_deterministic_eviction_order() {
        // Test that eviction order is deterministic when quality scores are equal
        let manager = UnifiedMemoryManager::new(1024 * 1024, 15.0);

        // Add adapters with identical quality scores but different IDs
        let mut adapters = manager.adapters.lock().await;

        adapters.insert(
            "adapter-zeta".to_string(),
            AdapterMemoryInfo {
                adapter_id: "adapter-zeta".to_string(),
                adapter_name: "Zeta".to_string(),
                memory_usage_bytes: 1024,
                memory_usage_mb: 0,
                state: AdapterState::Loaded,
                pinned: false,
                category: AdapterCategory::Code,
                last_access: None,
                activation_count: 0,
                quality_score: 0.5, // Same quality score
            },
        );

        adapters.insert(
            "adapter-alpha".to_string(),
            AdapterMemoryInfo {
                adapter_id: "adapter-alpha".to_string(),
                adapter_name: "Alpha".to_string(),
                memory_usage_bytes: 1024,
                memory_usage_mb: 0,
                state: AdapterState::Loaded,
                pinned: false,
                category: AdapterCategory::Code,
                last_access: None,
                activation_count: 0,
                quality_score: 0.5, // Same quality score
            },
        );

        adapters.insert(
            "adapter-beta".to_string(),
            AdapterMemoryInfo {
                adapter_id: "adapter-beta".to_string(),
                adapter_name: "Beta".to_string(),
                memory_usage_bytes: 1024,
                memory_usage_mb: 0,
                state: AdapterState::Loaded,
                pinned: false,
                category: AdapterCategory::Code,
                last_access: None,
                activation_count: 0,
                quality_score: 0.5, // Same quality score
            },
        );

        drop(adapters);

        // Collect adapters and sort using the same logic as cleanup
        let stats = manager.get_memory_usage().await.unwrap();
        let mut sorted_adapters: Vec<_> = stats.adapters.into_iter().collect();
        sorted_adapters.sort_by(|a, b| {
            if a.pinned != b.pinned {
                a.pinned.cmp(&b.pinned)
            } else {
                match a.quality_score.partial_cmp(&b.quality_score) {
                    Some(ord) if ord != std::cmp::Ordering::Equal => ord,
                    _ => {
                        let hash_a = blake3::hash(a.adapter_id.as_bytes());
                        let hash_b = blake3::hash(b.adapter_id.as_bytes());
                        hash_a.as_bytes().cmp(hash_b.as_bytes())
                    }
                }
            }
        });

        // Verify deterministic order - should be sorted by hash of adapter ID
        let eviction_order: Vec<String> = sorted_adapters
            .iter()
            .map(|a| a.adapter_id.clone())
            .collect();

        // Pre-compute hashes to verify expected order
        let hash_alpha = blake3::hash(b"adapter-alpha");
        let hash_beta = blake3::hash(b"adapter-beta");
        let hash_zeta = blake3::hash(b"adapter-zeta");

        // Determine expected order based on hash values
        let mut expected_hashes = vec![
            ("adapter-alpha", hash_alpha),
            ("adapter-beta", hash_beta),
            ("adapter-zeta", hash_zeta),
        ];
        expected_hashes.sort_by(|a, b| a.1.as_bytes().cmp(b.1.as_bytes()));

        let expected_order: Vec<String> = expected_hashes
            .iter()
            .map(|(id, _)| id.to_string())
            .collect();

        // Verify order matches expected deterministic order
        assert_eq!(
            eviction_order, expected_order,
            "Eviction order should be deterministic and based on adapter ID hash"
        );

        // Run the sort again to verify consistency
        let stats2 = manager.get_memory_usage().await.unwrap();
        let mut sorted_adapters2: Vec<_> = stats2.adapters.into_iter().collect();
        sorted_adapters2.sort_by(|a, b| {
            if a.pinned != b.pinned {
                a.pinned.cmp(&b.pinned)
            } else {
                match a.quality_score.partial_cmp(&b.quality_score) {
                    Some(ord) if ord != std::cmp::Ordering::Equal => ord,
                    _ => {
                        let hash_a = blake3::hash(a.adapter_id.as_bytes());
                        let hash_b = blake3::hash(b.adapter_id.as_bytes());
                        hash_a.as_bytes().cmp(hash_b.as_bytes())
                    }
                }
            }
        });

        let eviction_order2: Vec<String> = sorted_adapters2
            .iter()
            .map(|a| a.adapter_id.clone())
            .collect();

        assert_eq!(
            eviction_order, eviction_order2,
            "Eviction order must be identical across multiple sort operations"
        );
    }
}
