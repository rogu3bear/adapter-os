//! VRAM attribution per adapter
//!
//! Tracks Metal buffer allocations for each adapter, including LoRA weights
//! and estimated KV cache contribution. Enables per-adapter memory profiling
//! and telemetry without exposing sensitive tensor data.

use std::collections::HashMap;

/// Tracks VRAM allocations per adapter
pub struct VramTracker {
    allocations: HashMap<u32, VramAllocation>,
}

/// VRAM allocation for a single adapter
#[derive(Debug, Clone)]
struct VramAllocation {
    _adapter_id: u32,
    /// Size of LoRA weight buffers in bytes
    buffer_bytes: u64,
    /// Estimated KV cache contribution in bytes
    kv_cache_bytes: u64,
}

impl VramTracker {
    /// Create a new VRAM tracker
    pub fn new() -> Self {
        Self {
            allocations: HashMap::new(),
        }
    }

    /// Track adapter allocation
    ///
    /// Records buffer sizes for an adapter. Call this when an adapter
    /// is loaded into GPU memory.
    ///
    /// # Arguments
    /// * `adapter_id` - Unique adapter identifier
    /// * `lora_weights_bytes` - Size of LoRA weight matrices
    /// * `kv_cache_estimate_bytes` - Estimated KV cache contribution
    pub fn track_adapter(
        &mut self,
        adapter_id: u32,
        lora_weights_bytes: u64,
        kv_cache_estimate_bytes: u64,
    ) {
        self.allocations.insert(
            adapter_id,
            VramAllocation {
                _adapter_id: adapter_id,
                buffer_bytes: lora_weights_bytes,
                kv_cache_bytes: kv_cache_estimate_bytes,
            },
        );
    }

    /// Stop tracking an adapter
    ///
    /// Call this when an adapter is evicted from GPU memory
    pub fn untrack_adapter(&mut self, adapter_id: u32) -> Option<u64> {
        self.allocations
            .remove(&adapter_id)
            .map(|alloc| alloc.buffer_bytes + alloc.kv_cache_bytes)
    }

    /// Get total bytes for an adapter
    ///
    /// Returns the sum of buffer and KV cache sizes
    pub fn get_total_bytes(&self, adapter_id: u32) -> u64 {
        self.allocations
            .get(&adapter_id)
            .map(|a| a.buffer_bytes + a.kv_cache_bytes)
            .unwrap_or(0)
    }

    /// Get buffer bytes only (no KV cache)
    pub fn get_buffer_bytes(&self, adapter_id: u32) -> u64 {
        self.allocations
            .get(&adapter_id)
            .map(|a| a.buffer_bytes)
            .unwrap_or(0)
    }

    /// Get KV cache bytes only
    pub fn get_kv_cache_bytes(&self, adapter_id: u32) -> u64 {
        self.allocations
            .get(&adapter_id)
            .map(|a| a.kv_cache_bytes)
            .unwrap_or(0)
    }

    /// Get all allocations as (adapter_id, total_bytes) pairs
    pub fn get_all_allocations(&self) -> Vec<(u32, u64)> {
        self.allocations
            .iter()
            .map(|(id, alloc)| (*id, alloc.buffer_bytes + alloc.kv_cache_bytes))
            .collect()
    }

    /// Get total VRAM used across all adapters
    pub fn get_total_vram(&self) -> u64 {
        self.allocations
            .values()
            .map(|alloc| alloc.buffer_bytes + alloc.kv_cache_bytes)
            .sum()
    }

    /// Get number of tracked adapters
    pub fn adapter_count(&self) -> usize {
        self.allocations.len()
    }

    /// Check if an adapter is tracked
    pub fn is_tracked(&self, adapter_id: u32) -> bool {
        self.allocations.contains_key(&adapter_id)
    }

    /// Update KV cache estimate for an adapter
    ///
    /// Useful for updating estimates as context grows
    pub fn update_kv_cache_estimate(&mut self, adapter_id: u32, new_estimate: u64) {
        if let Some(alloc) = self.allocations.get_mut(&adapter_id) {
            alloc.kv_cache_bytes = new_estimate;
        }
    }

    /// Clear all allocations
    pub fn clear(&mut self) {
        self.allocations.clear();
    }
}

impl Default for VramTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_creation() {
        let tracker = VramTracker::new();
        assert_eq!(tracker.adapter_count(), 0);
        assert_eq!(tracker.get_total_vram(), 0);
    }

    #[test]
    fn test_track_adapter() {
        let mut tracker = VramTracker::new();

        tracker.track_adapter(1, 1024, 2048);
        assert_eq!(tracker.adapter_count(), 1);
        assert!(tracker.is_tracked(1));
        assert_eq!(tracker.get_total_bytes(1), 3072);
        assert_eq!(tracker.get_buffer_bytes(1), 1024);
        assert_eq!(tracker.get_kv_cache_bytes(1), 2048);
    }

    #[test]
    fn test_untrack_adapter() {
        let mut tracker = VramTracker::new();

        tracker.track_adapter(1, 1024, 2048);
        assert_eq!(tracker.adapter_count(), 1);

        let removed = tracker.untrack_adapter(1);
        assert_eq!(removed, Some(3072));
        assert_eq!(tracker.adapter_count(), 0);
        assert!(!tracker.is_tracked(1));
    }

    #[test]
    fn test_multiple_adapters() {
        let mut tracker = VramTracker::new();

        tracker.track_adapter(1, 1024, 512);
        tracker.track_adapter(2, 2048, 1024);
        tracker.track_adapter(3, 4096, 2048);

        assert_eq!(tracker.adapter_count(), 3);
        assert_eq!(
            tracker.get_total_vram(),
            1024 + 512 + 2048 + 1024 + 4096 + 2048
        );

        let all = tracker.get_all_allocations();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_update_kv_cache() {
        let mut tracker = VramTracker::new();

        tracker.track_adapter(1, 1024, 512);
        assert_eq!(tracker.get_kv_cache_bytes(1), 512);

        tracker.update_kv_cache_estimate(1, 1024);
        assert_eq!(tracker.get_kv_cache_bytes(1), 1024);
        assert_eq!(tracker.get_total_bytes(1), 2048); // 1024 buffer + 1024 kv
    }

    #[test]
    fn test_untracked_adapter_returns_zero() {
        let tracker = VramTracker::new();
        assert_eq!(tracker.get_total_bytes(999), 0);
        assert_eq!(tracker.get_buffer_bytes(999), 0);
        assert_eq!(tracker.get_kv_cache_bytes(999), 0);
    }

    #[test]
    fn test_clear() {
        let mut tracker = VramTracker::new();

        tracker.track_adapter(1, 1024, 512);
        tracker.track_adapter(2, 2048, 1024);
        assert_eq!(tracker.adapter_count(), 2);

        tracker.clear();
        assert_eq!(tracker.adapter_count(), 0);
        assert_eq!(tracker.get_total_vram(), 0);
    }
}
