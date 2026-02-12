//! Memory pressure management and eviction coordination
//!
//! Implements memory pressure detection and coordinated eviction across backends.
//! Enforces 15% headroom policy and provides deterministic eviction ordering.
//! Integrates with K reduction protocol for lifecycle-aware memory management.
//!
//! # K Reduction Integration
//!
//! When memory pressure reaches a threshold requiring adapter count reduction,
//! this manager sends `KReductionRequest` messages through a tokio mpsc channel.
//! The lifecycle manager consumes these requests and coordinates the actual
//! adapter unloading.

use crate::k_reduction_integration::{KReductionRequestSender, SendError};
use crate::k_reduction_protocol::{KReductionCoordinator, KReductionRequest};
use crate::unified_tracker::{
    BackendType, EvictionStrategy, MemoryLimits, PressureLevel, UnifiedMemoryTracker,
};
use adapteros_core::{AosError, Result};
use adapteros_deterministic_exec::spawn_deterministic;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

/// Memory pressure manager
pub struct MemoryPressureManager {
    /// Unified memory tracker
    tracker: Arc<UnifiedMemoryTracker>,
    /// Pinned adapters (never evict)
    pinned_adapters: Arc<parking_lot::RwLock<HashSet<u32>>>,
    /// K reduction channel sender (for lifecycle manager integration)
    k_reduction_sender: Option<KReductionRequestSender>,
    /// K reduction coordinator (optional, for synchronous protocol)
    k_reduction_coordinator: Option<Arc<KReductionCoordinator>>,
}

impl MemoryPressureManager {
    /// Create a new memory pressure manager
    pub fn new(tracker: Arc<UnifiedMemoryTracker>) -> Self {
        Self {
            tracker,
            pinned_adapters: Arc::new(parking_lot::RwLock::new(HashSet::new())),
            k_reduction_sender: None,
            k_reduction_coordinator: None,
        }
    }

    /// Create a new memory pressure manager with K reduction channel sender
    pub fn with_channel_sender(
        tracker: Arc<UnifiedMemoryTracker>,
        sender: KReductionRequestSender,
    ) -> Self {
        Self {
            tracker,
            pinned_adapters: Arc::new(parking_lot::RwLock::new(HashSet::new())),
            k_reduction_sender: Some(sender),
            k_reduction_coordinator: None,
        }
    }

    /// Create a new memory pressure manager with K reduction coordinator
    pub fn with_coordinator(
        tracker: Arc<UnifiedMemoryTracker>,
        coordinator: Arc<KReductionCoordinator>,
    ) -> Self {
        Self {
            tracker,
            pinned_adapters: Arc::new(parking_lot::RwLock::new(HashSet::new())),
            k_reduction_sender: None,
            k_reduction_coordinator: Some(coordinator),
        }
    }

    /// Set the K reduction channel sender
    pub fn set_channel_sender(&mut self, sender: KReductionRequestSender) {
        self.k_reduction_sender = Some(sender);
    }

    /// Set the K reduction coordinator
    pub fn set_coordinator(&mut self, coordinator: Arc<KReductionCoordinator>) {
        self.k_reduction_coordinator = Some(coordinator);
    }

    /// Pin an adapter (prevent eviction)
    pub fn pin_adapter(&self, adapter_id: u32) {
        self.pinned_adapters.write().insert(adapter_id);
        info!(adapter_id = adapter_id, "Pinned adapter");
    }

    /// Unpin an adapter (allow eviction)
    pub fn unpin_adapter(&self, adapter_id: u32) {
        self.pinned_adapters.write().remove(&adapter_id);
        info!(adapter_id = adapter_id, "Unpinned adapter");
    }

    /// Check if adapter is pinned
    pub fn is_pinned(&self, adapter_id: u32) -> bool {
        self.pinned_adapters.read().contains(&adapter_id)
    }

    /// Check memory pressure and execute eviction if needed
    pub fn check_and_handle_pressure(&self) -> Result<MemoryPressureReport> {
        let pressure = self.tracker.check_memory_pressure();

        match pressure.action {
            EvictionStrategy::None => Ok(MemoryPressureReport {
                pressure_level: pressure.level,
                action_taken: EvictionStrategy::None,
                adapters_evicted: vec![],
                bytes_freed: 0,
                headroom_before: pressure.headroom_pct,
                headroom_after: pressure.headroom_pct,
            }),
            EvictionStrategy::EvictLowPriority => self.evict_low_priority(pressure.bytes_to_free),
            EvictionStrategy::EvictCrossBackend => self.evict_cross_backend(pressure.bytes_to_free),
            EvictionStrategy::ReduceK => self.request_k_reduction(pressure),
            EvictionStrategy::EmergencyEvict => self.emergency_evict(pressure.bytes_to_free),
        }
    }

    /// Request K reduction through channel or coordinator (if available)
    fn request_k_reduction(
        &self,
        pressure: crate::unified_tracker::MemoryPressure,
    ) -> Result<MemoryPressureReport> {
        // Convert pressure to normalized level (0-1)
        let pressure_level = match pressure.level {
            PressureLevel::Low => 0.25,
            PressureLevel::Medium => 0.50,
            PressureLevel::High => 0.75,
            PressureLevel::Critical => 1.0,
        };

        // Derive current K from the live adapter set tracked in memory.
        let current_k = self.tracker.adapter_count();
        if current_k <= 1 {
            warn!(
                current_k = current_k,
                "K reduction requested but current K is already at minimum"
            );
            return Ok(self.create_report(
                pressure.level,
                EvictionStrategy::ReduceK,
                vec![],
                0,
                pressure.headroom_pct,
            ));
        }

        // Create K reduction request
        let request = KReductionRequest::new(
            1, // Minimal target K for emergency
            current_k,
            pressure_level,
            pressure.bytes_to_free,
            pressure.headroom_pct,
            "Memory pressure threshold exceeded".to_string(),
        );

        debug!(
            request_id = %request.request_id,
            target_k = request.target_k,
            pressure_level = pressure_level,
            bytes_to_free = pressure.bytes_to_free,
            "Initiating K reduction request"
        );

        // Try channel sender first (preferred, async)
        if let Some(sender) = &self.k_reduction_sender {
            // Spawn async task to send request with backpressure (blocking send).
            // Use send_with_timeout instead of try_send to provide backpressure and avoid dropping.
            let send_once = |sender: KReductionRequestSender, request: KReductionRequest| async move {
                // Use blocking send with timeout instead of try_send to avoid silent drops
                match sender.send_with_timeout(request.clone(), 5000).await {
                    Ok(()) => {
                        info!(
                            request_id = %request.request_id,
                            target_k = request.target_k,
                            "K reduction request sent through channel with backpressure"
                        );
                    }
                    Err(SendError::SendTimeout) => {
                        // After timeout, log error but continue - this is a backpressure signal
                        error!(
                            request_id = %request.request_id,
                            "K reduction channel send timed out after 5s (backpressure)"
                        );
                    }
                    Err(SendError::ChannelClosed) => {
                        error!(
                            request_id = %request.request_id,
                            "K reduction channel closed, lifecycle manager not available"
                        );
                    }
                    Err(SendError::ChannelFull) => {
                        // This shouldn't happen with send_with_timeout, but handle it
                        error!(
                            request_id = %request.request_id,
                            "K reduction channel unexpectedly full after timeout"
                        );
                    }
                }
            };

            let sender_clone = sender.clone();
            let request_clone = request.clone();

            // Use deterministic execution for reproducible K-reduction ordering
            if let Err(e) = spawn_deterministic(
                format!("k-reduction-send:{}", request_clone.request_id),
                send_once(sender_clone, request_clone.clone()),
            ) {
                debug!(
                    error = %e,
                    request_id = %request_clone.request_id,
                    "Deterministic executor unavailable for K reduction; falling back to Tokio"
                );
                tokio::spawn(send_once(sender.clone(), request.clone()));
            }

            return Ok(self.create_report(
                pressure.level,
                EvictionStrategy::ReduceK,
                vec![],
                0,
                pressure.headroom_pct,
            ));
        }

        // Fall back to synchronous coordinator
        if let Some(coordinator) = &self.k_reduction_coordinator {
            debug!(
                request_id = %request.request_id,
                "Using synchronous K reduction coordinator"
            );

            // Process through coordinator
            let response = coordinator.process_request(request);

            if response.approved {
                info!(
                    request_id = %response.request_id,
                    new_k = response.new_k,
                    "K reduction request approved"
                );
            } else {
                warn!(
                    request_id = %response.request_id,
                    reason = %response.reason,
                    "K reduction request rejected"
                );
            }

            return Ok(self.create_report(
                pressure.level,
                EvictionStrategy::ReduceK,
                vec![],
                0,
                pressure.headroom_pct,
            ));
        }

        // No K reduction mechanism available
        warn!(
            request_id = %request.request_id,
            "K reduction requested but no sender or coordinator available"
        );

        Ok(self.create_report(
            pressure.level,
            EvictionStrategy::ReduceK,
            vec![],
            0,
            pressure.headroom_pct,
        ))
    }

    /// Evict low priority adapters (LRU, unpinned)
    fn evict_low_priority(&self, target_bytes: u64) -> Result<MemoryPressureReport> {
        let headroom_before = self.tracker.check_memory_pressure().headroom_pct;
        let pinned: Vec<u32> = self.pinned_adapters.read().iter().copied().collect();
        let candidates = self.tracker.get_eviction_candidates(&pinned);

        let mut evicted = Vec::new();
        let mut total_freed = 0u64;

        for (adapter_id, backend, _bytes, priority) in candidates {
            if priority == f32::MAX {
                continue; // Skip pinned adapters
            }

            if let Some(freed) = self.tracker.untrack_adapter(adapter_id) {
                evicted.push(EvictedAdapter {
                    adapter_id,
                    backend,
                    bytes_freed: freed,
                });
                total_freed += freed;

                info!(
                    adapter_id = adapter_id,
                    backend = backend.as_str(),
                    bytes_freed = freed,
                    "Evicted low priority adapter"
                );

                if total_freed >= target_bytes {
                    break;
                }
            }
        }

        Ok(self.create_report(
            PressureLevel::Medium,
            EvictionStrategy::EvictLowPriority,
            evicted,
            total_freed,
            headroom_before,
        ))
    }

    /// Evict across backends (Metal before CoreML for ANE efficiency)
    fn evict_cross_backend(&self, target_bytes: u64) -> Result<MemoryPressureReport> {
        let headroom_before = self.tracker.check_memory_pressure().headroom_pct;
        let pinned: Vec<u32> = self.pinned_adapters.read().iter().copied().collect();
        let candidates = self.tracker.get_eviction_candidates(&pinned);

        let mut evicted = Vec::new();
        let mut total_freed = 0u64;

        // Evict in order: Metal → MLX → CoreML (preserve ANE resources)
        if self.try_evict_backend(
            &candidates,
            BackendType::Metal,
            target_bytes,
            &mut evicted,
            &mut total_freed,
            "cross-backend-metal",
        ) {
            return Ok(self.create_report(
                PressureLevel::High,
                EvictionStrategy::EvictCrossBackend,
                evicted,
                total_freed,
                headroom_before,
            ));
        }

        if self.try_evict_backend(
            &candidates,
            BackendType::MLX,
            target_bytes,
            &mut evicted,
            &mut total_freed,
            "cross-backend-mlx",
        ) {
            return Ok(self.create_report(
                PressureLevel::High,
                EvictionStrategy::EvictCrossBackend,
                evicted,
                total_freed,
                headroom_before,
            ));
        }

        self.try_evict_backend(
            &candidates,
            BackendType::CoreML,
            target_bytes,
            &mut evicted,
            &mut total_freed,
            "cross-backend-coreml",
        );

        Ok(self.create_report(
            PressureLevel::High,
            EvictionStrategy::EvictCrossBackend,
            evicted,
            total_freed,
            headroom_before,
        ))
    }

    /// Emergency eviction - evict all unpinned adapters
    fn emergency_evict(&self, target_bytes: u64) -> Result<MemoryPressureReport> {
        let headroom_before = self.tracker.check_memory_pressure().headroom_pct;
        let pinned: Vec<u32> = self.pinned_adapters.read().iter().copied().collect();
        let candidates = self.tracker.get_eviction_candidates(&pinned);

        let mut evicted = Vec::new();
        let mut total_freed = 0u64;

        for (adapter_id, backend, _bytes, priority) in candidates {
            if priority == f32::MAX {
                warn!(adapter_id, "Cannot evict pinned adapter during emergency");
                continue;
            }

            if let Some(freed) = self.tracker.untrack_adapter(adapter_id) {
                evicted.push(EvictedAdapter {
                    adapter_id,
                    backend,
                    bytes_freed: freed,
                });
                total_freed += freed;

                warn!(
                    adapter_id,
                    backend = backend.as_str(),
                    bytes_freed = freed,
                    "Emergency eviction"
                );

                if total_freed >= target_bytes {
                    break;
                }
            }
        }

        let report = self.create_report(
            PressureLevel::Critical,
            EvictionStrategy::EmergencyEvict,
            evicted,
            total_freed,
            headroom_before,
        );

        if report.headroom_after < 15.0 {
            return Err(AosError::Memory(format!(
                "Emergency eviction failed to restore headroom: {:.2}% (target: 15%)",
                report.headroom_after
            )));
        }

        Ok(report)
    }

    /// Get current memory statistics
    pub fn get_stats(&self) -> MemoryStats {
        let pressure = self.tracker.check_memory_pressure();
        let total_memory = self.tracker.get_total_memory();
        let metal_memory = self.tracker.get_backend_memory(BackendType::Metal);
        let coreml_memory = self.tracker.get_backend_memory(BackendType::CoreML);
        let mlx_memory = self.tracker.get_backend_memory(BackendType::MLX);
        let pinned_count = self.pinned_adapters.read().len();

        MemoryStats {
            total_memory_used: total_memory,
            metal_memory_used: metal_memory,
            coreml_memory_used: coreml_memory,
            mlx_memory_used: mlx_memory,
            pressure_level: pressure.level,
            headroom_pct: pressure.headroom_pct,
            pinned_adapter_count: pinned_count,
            total_adapter_count: self.tracker.adapter_count(),
        }
    }

    /// Helper: Create memory pressure report (reduces duplication)
    fn create_report(
        &self,
        pressure_level: PressureLevel,
        action_taken: EvictionStrategy,
        evicted: Vec<EvictedAdapter>,
        total_freed: u64,
        headroom_before: f32,
    ) -> MemoryPressureReport {
        let headroom_after = self.tracker.check_memory_pressure().headroom_pct;
        MemoryPressureReport {
            pressure_level,
            action_taken,
            adapters_evicted: evicted,
            bytes_freed: total_freed,
            headroom_before,
            headroom_after,
        }
    }

    /// Helper: Try to evict adapters from specific backend (reduces duplication)
    fn try_evict_backend(
        &self,
        candidates: &[(u32, BackendType, u64, f32)],
        target_backend: BackendType,
        target_bytes: u64,
        evicted: &mut Vec<EvictedAdapter>,
        total_freed: &mut u64,
        strategy_name: &str,
    ) -> bool {
        for (adapter_id, backend, _bytes, priority) in candidates {
            if priority == &f32::MAX || *backend != target_backend {
                continue;
            }

            if let Some(freed) = self.tracker.untrack_adapter(*adapter_id) {
                evicted.push(EvictedAdapter {
                    adapter_id: *adapter_id,
                    backend: *backend,
                    bytes_freed: freed,
                });
                *total_freed += freed;

                warn!(
                    adapter_id = adapter_id,
                    backend = backend.as_str(),
                    bytes_freed = freed,
                    strategy = strategy_name,
                    "Evicted adapter"
                );

                if *total_freed >= target_bytes {
                    return true; // Target met
                }
            }
        }
        false // Target not met
    }
}

/// Memory pressure report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPressureReport {
    /// Pressure level detected
    pub pressure_level: PressureLevel,
    /// Action taken
    pub action_taken: EvictionStrategy,
    /// Adapters evicted
    pub adapters_evicted: Vec<EvictedAdapter>,
    /// Total bytes freed
    pub bytes_freed: u64,
    /// Headroom percentage before eviction
    pub headroom_before: f32,
    /// Headroom percentage after eviction
    pub headroom_after: f32,
}

/// Evicted adapter information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictedAdapter {
    /// Adapter ID
    pub adapter_id: u32,
    /// Backend it was evicted from
    pub backend: BackendType,
    /// Bytes freed
    pub bytes_freed: u64,
}

/// Memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// Total memory used across all backends
    pub total_memory_used: u64,
    /// Metal memory used
    pub metal_memory_used: u64,
    /// CoreML memory used
    pub coreml_memory_used: u64,
    /// MLX memory used
    pub mlx_memory_used: u64,
    /// Current pressure level
    pub pressure_level: PressureLevel,
    /// Current headroom percentage
    pub headroom_pct: f32,
    /// Number of pinned adapters
    pub pinned_adapter_count: usize,
    /// Total number of adapters
    pub total_adapter_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pressure_manager_creation() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
        let manager = MemoryPressureManager::new(tracker);

        assert_eq!(manager.pinned_adapters.read().len(), 0);
    }

    #[test]
    fn test_pin_unpin_adapter() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
        let manager = MemoryPressureManager::new(tracker);

        manager.pin_adapter(1);
        assert!(manager.is_pinned(1));

        manager.unpin_adapter(1);
        assert!(!manager.is_pinned(1));
    }

    #[test]
    fn test_eviction_respects_pinned() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
        let manager = MemoryPressureManager::new(Arc::clone(&tracker));

        tracker.track_adapter(1, BackendType::Metal, 100, 0);
        tracker.track_adapter(2, BackendType::Metal, 200, 0);

        manager.pin_adapter(1);

        // Trigger eviction
        tracker.track_adapter(3, BackendType::Metal, 800, 0);
        let report = manager.check_and_handle_pressure().unwrap();

        // Adapter 1 should not be evicted (pinned)
        let evicted_ids: Vec<u32> = report
            .adapters_evicted
            .iter()
            .map(|e| e.adapter_id)
            .collect();
        assert!(!evicted_ids.contains(&1));
    }

    #[test]
    fn test_cross_backend_eviction_order() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
        let manager = MemoryPressureManager::new(Arc::clone(&tracker));

        tracker.track_adapter(1, BackendType::Metal, 100, 0);
        tracker.track_adapter(2, BackendType::CoreML, 100, 0);
        tracker.track_adapter(3, BackendType::MLX, 100, 0);

        // Trigger high pressure
        tracker.track_adapter(4, BackendType::Metal, 800, 0);

        let report = manager.evict_cross_backend(200).unwrap();

        // Should evict Metal first
        if !report.adapters_evicted.is_empty() {
            let first_evicted = &report.adapters_evicted[0];
            // First eviction should be Metal or MLX (not CoreML)
            assert!(matches!(
                first_evicted.backend,
                BackendType::Metal | BackendType::MLX
            ));
        }
    }

    #[test]
    fn test_emergency_eviction() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
        let manager = MemoryPressureManager::new(Arc::clone(&tracker));

        tracker.track_adapter(1, BackendType::Metal, 300, 0);
        tracker.track_adapter(2, BackendType::Metal, 300, 0);
        tracker.track_adapter(3, BackendType::Metal, 300, 0);

        let report = manager.emergency_evict(600).unwrap();

        // Should evict enough to free target bytes
        assert!(report.bytes_freed >= 600);
        assert_eq!(report.action_taken, EvictionStrategy::EmergencyEvict);
    }

    #[test]
    fn test_memory_stats() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = Arc::new(UnifiedMemoryTracker::new(limits));
        let manager = MemoryPressureManager::new(Arc::clone(&tracker));

        tracker.track_adapter(1, BackendType::Metal, 100, 50);
        tracker.track_adapter(2, BackendType::CoreML, 200, 100);
        manager.pin_adapter(1);

        let stats = manager.get_stats();

        assert_eq!(stats.metal_memory_used, 150);
        assert_eq!(stats.coreml_memory_used, 300);
        assert_eq!(stats.total_memory_used, 450);
        assert_eq!(stats.pinned_adapter_count, 1);
        assert_eq!(stats.total_adapter_count, 2);
    }
}
