//! Resource recovery mechanisms for MLX backend
//!
//! Handles GPU OOM, adapter cleanup, and graceful degradation strategies

use crate::error::MlxError;
use crate::{memory, MLXFFIBackend};
use std::sync::Arc;
use parking_lot::RwLock;

/// Recovery strategies for resource exhaustion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Trigger garbage collection
    GarbageCollect,
    /// Unload least recently used adapters
    UnloadLRU,
    /// Reduce batch size (for future requests)
    ReduceBatchSize,
    /// Fallback to CPU (not implemented in MLX)
    FallbackCpu,
    /// Fail immediately
    FailImmediately,
}

/// Recovery action result
#[derive(Debug)]
pub struct RecoveryResult {
    pub strategy: RecoveryStrategy,
    pub success: bool,
    pub freed_mb: f32,
    pub message: String,
}

/// Resource recovery manager
pub struct RecoveryManager {
    /// LRU tracking for adapters
    adapter_usage: Arc<RwLock<Vec<(u16, std::time::Instant)>>>,
    /// Maximum memory threshold (MB)
    max_memory_mb: f32,
    /// Target memory after recovery (MB)
    target_memory_mb: f32,
}

impl RecoveryManager {
    /// Create new recovery manager
    pub fn new(max_memory_mb: f32) -> Self {
        Self {
            adapter_usage: Arc::new(RwLock::new(Vec::new())),
            max_memory_mb,
            target_memory_mb: max_memory_mb * 0.75, // Leave 25% headroom
        }
    }

    /// Record adapter access for LRU tracking
    pub fn record_adapter_access(&self, adapter_id: u16) {
        let mut usage = self.adapter_usage.write();

        // Update or add adapter timestamp
        if let Some(entry) = usage.iter_mut().find(|(id, _)| *id == adapter_id) {
            entry.1 = std::time::Instant::now();
        } else {
            usage.push((adapter_id, std::time::Instant::now()));
        }

        // Sort by timestamp (most recent first)
        usage.sort_by(|a, b| b.1.cmp(&a.1));
    }

    /// Get least recently used adapters
    pub fn get_lru_adapters(&self, count: usize) -> Vec<u16> {
        let usage = self.adapter_usage.read();
        usage.iter()
            .rev() // Get oldest first
            .take(count)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Attempt recovery from GPU OOM
    pub fn recover_from_oom(
        &self,
        backend: Option<&MLXFFIBackend>,
        requested_mb: f32,
    ) -> Result<RecoveryResult, MlxError> {
        tracing::warn!(
            requested_mb = %requested_mb,
            current_mb = %memory::bytes_to_mb(memory::memory_usage()),
            "Attempting recovery from OOM"
        );

        // Step 1: Try garbage collection first (fast, non-disruptive)
        let result = self.try_garbage_collection(requested_mb)?;
        if result.success {
            return Ok(result);
        }

        // Step 2: If backend available, try unloading LRU adapters
        if let Some(backend) = backend {
            let result = self.try_unload_lru_adapters(backend, requested_mb)?;
            if result.success {
                return Ok(result);
            }
        }

        // Step 3: All recovery attempts failed
        Err(MlxError::GpuOomError {
            requested_mb,
            available_mb: memory::bytes_to_mb(memory::memory_usage()),
            hint: format!(
                "Recovery failed. Current usage: {:.2}MB, requested: {:.2}MB, max: {:.2}MB. \
                Suggestions: 1) Reduce model size, 2) Reduce adapter count, 3) Restart process",
                memory::bytes_to_mb(memory::memory_usage()),
                requested_mb,
                self.max_memory_mb
            ),
        })
    }

    /// Try garbage collection recovery
    fn try_garbage_collection(&self, requested_mb: f32) -> Result<RecoveryResult, MlxError> {
        let before_mb = memory::bytes_to_mb(memory::memory_usage());

        tracing::info!(
            before_mb = %before_mb,
            "Attempting garbage collection"
        );

        memory::gc_collect();

        // Give GC a moment to complete
        std::thread::sleep(std::time::Duration::from_millis(100));

        let after_mb = memory::bytes_to_mb(memory::memory_usage());
        let freed_mb = before_mb - after_mb;
        let success = after_mb + requested_mb <= self.max_memory_mb;

        tracing::info!(
            before_mb = %before_mb,
            after_mb = %after_mb,
            freed_mb = %freed_mb,
            success = %success,
            "Garbage collection complete"
        );

        Ok(RecoveryResult {
            strategy: RecoveryStrategy::GarbageCollect,
            success,
            freed_mb,
            message: format!(
                "Garbage collection freed {:.2}MB ({:.2}MB → {:.2}MB)",
                freed_mb, before_mb, after_mb
            ),
        })
    }

    /// Try unloading least recently used adapters
    fn try_unload_lru_adapters(
        &self,
        backend: &MLXFFIBackend,
        requested_mb: f32,
    ) -> Result<RecoveryResult, MlxError> {
        let before_mb = memory::bytes_to_mb(memory::memory_usage());
        let needed_mb = (before_mb + requested_mb) - self.target_memory_mb;

        if needed_mb <= 0.0 {
            return Ok(RecoveryResult {
                strategy: RecoveryStrategy::UnloadLRU,
                success: true,
                freed_mb: 0.0,
                message: "No unloading needed".to_string(),
            });
        }

        tracing::info!(
            before_mb = %before_mb,
            needed_mb = %needed_mb,
            "Attempting to unload LRU adapters"
        );

        // Start with oldest adapters
        let lru_adapters = self.get_lru_adapters(backend.adapter_count());
        let mut freed_total_mb = 0.0;
        let mut unloaded_count = 0;

        for adapter_id in lru_adapters {
            // Check adapter memory usage
            match backend.get_adapter_memory_usage(adapter_id) {
                Ok(bytes) => {
                    let adapter_mb = memory::bytes_to_mb(bytes);

                    // Try to unload
                    match backend.unload_adapter_runtime(adapter_id) {
                        Ok(_) => {
                            freed_total_mb += adapter_mb;
                            unloaded_count += 1;

                            // Remove from LRU tracking
                            self.adapter_usage.write().retain(|(id, _)| *id != adapter_id);

                            tracing::info!(
                                adapter_id = adapter_id,
                                freed_mb = %adapter_mb,
                                total_freed = %freed_total_mb,
                                "Unloaded LRU adapter"
                            );

                            // Check if we've freed enough
                            if freed_total_mb >= needed_mb {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                adapter_id = adapter_id,
                                error = %e,
                                "Failed to unload adapter"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        adapter_id = adapter_id,
                        error = %e,
                        "Failed to get adapter memory usage"
                    );
                }
            }
        }

        // Run GC after unloading
        memory::gc_collect();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let after_mb = memory::bytes_to_mb(memory::memory_usage());
        let actual_freed = before_mb - after_mb;
        let success = after_mb + requested_mb <= self.max_memory_mb;

        Ok(RecoveryResult {
            strategy: RecoveryStrategy::UnloadLRU,
            success,
            freed_mb: actual_freed,
            message: format!(
                "Unloaded {} LRU adapters, freed {:.2}MB",
                unloaded_count, actual_freed
            ),
        })
    }

    /// Check if recovery is needed before allocation
    pub fn check_and_recover(
        &self,
        backend: Option<&MLXFFIBackend>,
        required_mb: f32,
    ) -> Result<(), MlxError> {
        let current_mb = memory::bytes_to_mb(memory::memory_usage());

        if current_mb + required_mb > self.max_memory_mb {
            tracing::warn!(
                current_mb = %current_mb,
                required_mb = %required_mb,
                max_mb = %self.max_memory_mb,
                "Memory threshold exceeded, attempting recovery"
            );

            let result = self.recover_from_oom(backend, required_mb)?;

            if !result.success {
                return Err(MlxError::AllocationFailed {
                    size_mb: required_mb,
                    total_allocated_mb: current_mb,
                    hint: format!(
                        "Recovery attempt failed: {}. Current: {:.2}MB, requested: {:.2}MB, max: {:.2}MB",
                        result.message, current_mb, required_mb, self.max_memory_mb
                    ),
                });
            }

            tracing::info!(
                strategy = ?result.strategy,
                freed_mb = %result.freed_mb,
                message = %result.message,
                "Recovery successful"
            );
        }

        Ok(())
    }

    /// Get memory statistics
    pub fn memory_stats(&self) -> MemoryStats {
        let current_mb = memory::bytes_to_mb(memory::memory_usage());
        let usage_pct = (current_mb / self.max_memory_mb) * 100.0;

        MemoryStats {
            current_mb,
            max_mb: self.max_memory_mb,
            target_mb: self.target_memory_mb,
            usage_pct,
            allocation_count: memory::allocation_count(),
        }
    }
}

/// Memory statistics snapshot
#[derive(Debug, Clone)]
pub struct MemoryStats {
    pub current_mb: f32,
    pub max_mb: f32,
    pub target_mb: f32,
    pub usage_pct: f32,
    pub allocation_count: usize,
}

impl MemoryStats {
    pub fn is_healthy(&self) -> bool {
        self.usage_pct < 85.0
    }

    pub fn needs_recovery(&self) -> bool {
        self.usage_pct > 90.0
    }
}

/// Cleanup guard that runs recovery on drop
pub struct CleanupGuard<'a> {
    recovery: &'a RecoveryManager,
    backend: Option<&'a MLXFFIBackend>,
}

impl<'a> CleanupGuard<'a> {
    pub fn new(recovery: &'a RecoveryManager, backend: Option<&'a MLXFFIBackend>) -> Self {
        Self { recovery, backend }
    }
}

impl<'a> Drop for CleanupGuard<'a> {
    fn drop(&mut self) {
        // Run cleanup on drop (e.g., after failed operation)
        let stats = self.recovery.memory_stats();
        if stats.needs_recovery() {
            tracing::warn!(
                usage_pct = %stats.usage_pct,
                "Running cleanup on guard drop"
            );

            if let Err(e) = self.recovery.recover_from_oom(self.backend, 0.0) {
                tracing::error!(
                    error = %e,
                    "Cleanup failed during guard drop"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_manager_creation() {
        let manager = RecoveryManager::new(2048.0);
        assert_eq!(manager.max_memory_mb, 2048.0);
        assert_eq!(manager.target_memory_mb, 1536.0); // 75% of max
    }

    #[test]
    fn test_adapter_lru_tracking() {
        let manager = RecoveryManager::new(2048.0);

        manager.record_adapter_access(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.record_adapter_access(2);
        std::thread::sleep(std::time::Duration::from_millis(10));
        manager.record_adapter_access(3);

        let lru = manager.get_lru_adapters(2);
        assert_eq!(lru.len(), 2);
        assert_eq!(lru[0], 1); // Oldest
    }

    #[test]
    fn test_memory_stats() {
        let manager = RecoveryManager::new(2048.0);
        let stats = manager.memory_stats();

        assert!(stats.current_mb >= 0.0);
        assert_eq!(stats.max_mb, 2048.0);
        assert_eq!(stats.target_mb, 1536.0);
    }

    #[test]
    fn test_memory_stats_health() {
        let stats = MemoryStats {
            current_mb: 1024.0,
            max_mb: 2048.0,
            target_mb: 1536.0,
            usage_pct: 50.0,
            allocation_count: 10,
        };

        assert!(stats.is_healthy());
        assert!(!stats.needs_recovery());

        let high_stats = MemoryStats {
            current_mb: 1945.0,
            max_mb: 2048.0,
            target_mb: 1536.0,
            usage_pct: 95.0,
            allocation_count: 100,
        };

        assert!(!high_stats.is_healthy());
        assert!(high_stats.needs_recovery());
    }

    #[test]
    fn test_garbage_collection_recovery() {
        let manager = RecoveryManager::new(2048.0);
        memory::reset();

        let result = manager.try_garbage_collection(100.0);
        assert!(result.is_ok());

        let recovery = result.unwrap();
        assert_eq!(recovery.strategy, RecoveryStrategy::GarbageCollect);
    }
}
