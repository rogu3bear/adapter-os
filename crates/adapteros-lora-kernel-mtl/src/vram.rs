//! VRAM attribution per adapter
//!
//! Tracks Metal buffer allocations for each adapter, including LoRA weights
//! and estimated KV cache contribution. Enables per-adapter memory profiling
//! and telemetry without exposing sensitive tensor data.
//!
//! GPU Integrity Verification:
//! - GpuBufferFingerprint: Metadata + checkpoint sampling for fast verification
//! - Adaptive baseline tracking for memory footprint anomaly detection
//! - Cross-layer hash combining lifecycle state and GPU buffer state

use adapteros_core::B3Hash;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// GPU buffer fingerprint for integrity verification
///
/// Uses metadata + checkpoint sampling (not full buffer readback) for fast verification:
/// - Buffer metadata: size, allocation timestamp
/// - Checkpoint samples: hash of first 4KB + last 4KB + midpoint
/// - Combined BLAKE3 hash for tamper detection
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuBufferFingerprint {
    /// Total buffer size in bytes
    pub buffer_bytes: u64,
    /// Unix timestamp when buffer was allocated
    pub allocated_at: u64,
    /// BLAKE3 hash of checkpoint samples
    /// Format: hash(first_4kb || last_4kb || midpoint_4kb)
    pub checkpoint_hash: B3Hash,
}

impl GpuBufferFingerprint {
    /// Create fingerprint from buffer metadata and sample data
    ///
    /// # Arguments
    /// * `buffer_bytes` - Total buffer size
    /// * `first_sample` - First 4KB (or less if buffer smaller)
    /// * `last_sample` - Last 4KB (or less if buffer smaller)
    /// * `mid_sample` - Midpoint 4KB (or less if buffer smaller)
    pub fn new(
        buffer_bytes: u64,
        first_sample: &[u8],
        last_sample: &[u8],
        mid_sample: &[u8],
    ) -> Self {
        let checkpoint_hash = B3Hash::hash_multi(&[first_sample, last_sample, mid_sample]);
        let allocated_at = adapteros_core::time::unix_timestamp_secs();

        Self {
            buffer_bytes,
            allocated_at,
            checkpoint_hash,
        }
    }

    /// Create fingerprint with explicit timestamp (for testing/replay)
    pub fn with_timestamp(
        buffer_bytes: u64,
        allocated_at: u64,
        first_sample: &[u8],
        last_sample: &[u8],
        mid_sample: &[u8],
    ) -> Self {
        let checkpoint_hash = B3Hash::hash_multi(&[first_sample, last_sample, mid_sample]);
        Self {
            buffer_bytes,
            allocated_at,
            checkpoint_hash,
        }
    }

    /// Verify another fingerprint matches this one
    ///
    /// Returns true if checkpoint hashes match and buffer sizes are identical.
    /// Timestamp is NOT checked (buffers may be reloaded at different times).
    pub fn matches(&self, other: &Self) -> bool {
        self.buffer_bytes == other.buffer_bytes && self.checkpoint_hash == other.checkpoint_hash
    }
}

/// Memory footprint baseline for adaptive anomaly detection
#[derive(Debug, Clone)]
pub struct MemoryFootprintBaseline {
    /// Adapter ID
    _adapter_id: u32,
    /// Observed footprints (up to N samples)
    samples: Vec<u64>,
    /// Maximum samples to keep
    max_samples: usize,
    /// Computed mean (cached)
    mean: Option<f64>,
    /// Computed standard deviation (cached)
    stddev: Option<f64>,
}

impl MemoryFootprintBaseline {
    /// Create new baseline tracker
    pub fn new(adapter_id: u32, max_samples: usize) -> Self {
        Self {
            _adapter_id: adapter_id,
            samples: Vec::with_capacity(max_samples),
            max_samples,
            mean: None,
            stddev: None,
        }
    }

    /// Add a footprint sample
    pub fn add_sample(&mut self, bytes: u64) {
        if self.samples.len() >= self.max_samples {
            // Rolling window: remove oldest
            self.samples.remove(0);
        }
        self.samples.push(bytes);
        // Invalidate cached statistics
        self.mean = None;
        self.stddev = None;
    }

    /// Compute mean (cached)
    fn compute_mean(&mut self) -> f64 {
        if let Some(mean) = self.mean {
            return mean;
        }

        if self.samples.is_empty() {
            return 0.0;
        }

        let sum: u64 = self.samples.iter().sum();
        let mean = sum as f64 / self.samples.len() as f64;
        self.mean = Some(mean);
        mean
    }

    /// Compute standard deviation (cached)
    fn compute_stddev(&mut self) -> f64 {
        if let Some(stddev) = self.stddev {
            return stddev;
        }

        if self.samples.len() < 2 {
            return 0.0;
        }

        let mean = self.compute_mean();
        let variance: f64 = self
            .samples
            .iter()
            .map(|&x| {
                let diff = x as f64 - mean;
                diff * diff
            })
            .sum::<f64>()
            / self.samples.len() as f64;

        let stddev = variance.sqrt();
        self.stddev = Some(stddev);
        stddev
    }

    /// Check if a footprint is within tolerance (2σ)
    ///
    /// Returns (within_tolerance, z_score)
    /// - within_tolerance: true if within 2 standard deviations
    /// - z_score: number of standard deviations from mean
    pub fn check_footprint(&mut self, bytes: u64) -> (bool, f64) {
        if self.samples.len() < 2 {
            // Not enough samples to establish baseline
            return (true, 0.0);
        }

        let mean = self.compute_mean();
        let stddev = self.compute_stddev();

        if stddev == 0.0 {
            // All samples identical - check exact match
            return (
                bytes as f64 == mean,
                if bytes as f64 == mean { 0.0 } else { f64::MAX },
            );
        }

        let z_score = (bytes as f64 - mean).abs() / stddev;
        let within_tolerance = z_score <= 2.0; // 2σ threshold

        (within_tolerance, z_score)
    }

    /// Get baseline statistics
    pub fn stats(&mut self) -> (f64, f64, usize) {
        let mean = self.compute_mean();
        let stddev = self.compute_stddev();
        (mean, stddev, self.samples.len())
    }
}

/// Tracks VRAM allocations per adapter
pub struct VramTracker {
    allocations: HashMap<u32, VramAllocation>,
    /// GPU buffer fingerprints for integrity verification
    fingerprints: HashMap<u32, GpuBufferFingerprint>,
    /// Memory footprint baselines for anomaly detection (with interior mutability)
    baselines: Arc<RwLock<HashMap<u32, MemoryFootprintBaseline>>>,
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
            fingerprints: HashMap::new(),
            baselines: Arc::new(RwLock::new(HashMap::new())),
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
        self.fingerprints.clear();
        if let Ok(mut baselines) = self.baselines.write() {
            baselines.clear();
        }
    }

    // ===== GPU Integrity Verification Methods =====

    /// Store GPU buffer fingerprint for an adapter
    ///
    /// Call this when an adapter is loaded into GPU memory with sampled data.
    ///
    /// # Arguments
    /// * `adapter_id` - Unique adapter identifier
    /// * `fingerprint` - Precomputed fingerprint from GPU buffer samples
    pub fn store_fingerprint(&mut self, adapter_id: u32, fingerprint: GpuBufferFingerprint) {
        // Update baseline with new footprint
        let total_bytes = fingerprint.buffer_bytes;
        if let Ok(mut baselines) = self.baselines.write() {
            baselines
                .entry(adapter_id)
                .or_insert_with(|| MemoryFootprintBaseline::new(adapter_id, 10))
                .add_sample(total_bytes);
        }

        self.fingerprints.insert(adapter_id, fingerprint);
    }

    /// Get stored fingerprint for an adapter
    pub fn get_fingerprint(&self, adapter_id: u32) -> Option<&GpuBufferFingerprint> {
        self.fingerprints.get(&adapter_id)
    }

    /// Verify fingerprint matches stored baseline
    ///
    /// Returns Ok(true) if fingerprints match, Ok(false) if no baseline exists.
    /// Returns Err if fingerprints mismatch (integrity violation).
    pub fn verify_fingerprint(
        &self,
        adapter_id: u32,
        current: &GpuBufferFingerprint,
    ) -> Result<bool, String> {
        if let Some(baseline) = self.fingerprints.get(&adapter_id) {
            if baseline.matches(current) {
                Ok(true)
            } else {
                Err(format!(
                    "Fingerprint mismatch for adapter {}: expected size {} bytes, got {}; checkpoint hash mismatch",
                    adapter_id, baseline.buffer_bytes, current.buffer_bytes
                ))
            }
        } else {
            // No baseline - first load
            Ok(false)
        }
    }

    /// Check memory footprint against adaptive baseline
    ///
    /// Returns (within_tolerance, z_score, baseline_stats)
    pub fn check_memory_footprint(
        &self,
        adapter_id: u32,
        bytes: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        let Ok(mut baselines) = self.baselines.write() else {
            // Lock poisoned - return safe defaults
            return (true, 0.0, None);
        };
        if let Some(baseline) = baselines.get_mut(&adapter_id) {
            let (within_tolerance, z_score) = baseline.check_footprint(bytes);
            let stats = baseline.stats();
            (within_tolerance, z_score, Some(stats))
        } else {
            // No baseline yet - create one
            let mut baseline = MemoryFootprintBaseline::new(adapter_id, 100);
            baseline.add_sample(bytes);
            let stats = baseline.stats();
            baselines.insert(adapter_id, baseline);
            (true, 0.0, Some(stats))
        }
    }

    /// Get all fingerprints for cross-layer hashing
    ///
    /// Returns sorted list of (adapter_id, fingerprint) for deterministic hashing
    pub fn get_all_fingerprints(&self) -> Vec<(u32, &GpuBufferFingerprint)> {
        let mut fps: Vec<_> = self.fingerprints.iter().map(|(&id, fp)| (id, fp)).collect();
        fps.sort_by_key(|(id, _)| *id);
        fps
    }

    /// Remove fingerprint when adapter is unloaded
    pub fn remove_fingerprint(&mut self, adapter_id: u32) -> Option<GpuBufferFingerprint> {
        self.fingerprints.remove(&adapter_id)
    }

    /// Get baseline statistics for an adapter
    pub fn get_baseline_stats(&mut self, adapter_id: u32) -> Option<(f64, f64, usize)> {
        self.baselines
            .write()
            .ok()
            .and_then(|mut baselines| baselines.get_mut(&adapter_id).map(|b| b.stats()))
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
