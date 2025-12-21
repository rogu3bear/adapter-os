//! Unified memory tracker for Metal, CoreML, and MLX backends
//!
//! Extends VramTracker to support multiple backend memory types:
//! - Metal VRAM (GPU memory)
//! - CoreML ANE memory (Apple Neural Engine)
//! - MLX unified memory (shared CPU/GPU)
//!
//! Provides centralized memory accounting, pressure detection, and eviction coordination.

use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

/// Backend type for memory tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BackendType {
    /// Metal GPU memory
    Metal,
    /// CoreML Neural Engine memory
    CoreML,
    /// MLX unified memory (shared CPU/GPU)
    Mlx,
}

impl BackendType {
    /// Get human-readable name
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Metal => "Metal",
            Self::CoreML => "CoreML",
            Self::Mlx => "MLX",
        }
    }
}

/// Memory limits configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryLimits {
    /// Maximum VRAM (GPU memory) in bytes
    pub max_vram: u64,
    /// Maximum system RAM in bytes
    pub max_system_ram: u64,
    /// Headroom percentage to maintain (e.g., 0.15 for 15%)
    pub headroom_pct: f32,
}

impl MemoryLimits {
    /// Create new memory limits
    pub fn new(max_vram: u64, max_system_ram: u64, headroom_pct: f32) -> Self {
        Self {
            max_vram,
            max_system_ram,
            headroom_pct,
        }
    }

    /// Calculate headroom in bytes for VRAM
    pub fn vram_headroom_bytes(&self) -> u64 {
        (self.max_vram as f32 * self.headroom_pct) as u64
    }

    /// Calculate headroom in bytes for system RAM
    pub fn system_ram_headroom_bytes(&self) -> u64 {
        (self.max_system_ram as f32 * self.headroom_pct) as u64
    }

    /// Get effective VRAM limit (accounting for headroom)
    pub fn effective_vram_limit(&self) -> u64 {
        self.max_vram - self.vram_headroom_bytes()
    }

    /// Get effective system RAM limit (accounting for headroom)
    pub fn effective_system_ram_limit(&self) -> u64 {
        self.max_system_ram - self.system_ram_headroom_bytes()
    }
}

/// Memory pressure level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PressureLevel {
    /// Low pressure - plenty of memory available
    Low,
    /// Medium pressure - approaching headroom threshold
    Medium,
    /// High pressure - below headroom threshold
    High,
    /// Critical pressure - immediate eviction required
    Critical,
}

impl PressureLevel {
    /// Create pressure level from headroom percentage
    pub fn from_headroom(headroom_pct: f32, threshold: f32) -> Self {
        if headroom_pct >= threshold + 10.0 {
            Self::Low
        } else if headroom_pct >= threshold {
            Self::Medium
        } else if headroom_pct >= threshold - 5.0 {
            Self::High
        } else {
            Self::Critical
        }
    }

    /// Get priority for logging/alerting
    pub fn priority(&self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Medium => 1,
            Self::High => 2,
            Self::Critical => 3,
        }
    }
}

/// Memory pressure state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPressure {
    /// Current pressure level
    pub level: PressureLevel,
    /// Recommended eviction strategy
    pub action: EvictionStrategy,
    /// Current headroom percentage
    pub headroom_pct: f32,
    /// Bytes to free to reach target headroom
    pub bytes_to_free: u64,
}

/// Eviction strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvictionStrategy {
    /// No eviction needed
    None,
    /// Evict lowest priority unpinned adapters
    EvictLowPriority,
    /// Evict across backends (Metal before CoreML)
    EvictCrossBackend,
    /// Reduce K value to decrease active adapters
    ReduceK,
    /// Emergency eviction - evict all unpinned adapters
    EmergencyEvict,
}

/// Backend-specific memory allocation
#[derive(Debug, Clone)]
struct BackendAllocation {
    adapter_id: u32,
    backend: BackendType,
    /// Size of LoRA weight buffers in bytes
    buffer_bytes: u64,
    /// Estimated KV cache contribution in bytes
    kv_cache_bytes: u64,
    /// GPU buffer fingerprint (if available)
    fingerprint: Option<GpuBufferFingerprint>,
    /// Allocation timestamp
    allocated_at: u64,
}

/// GPU buffer fingerprint for integrity verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GpuBufferFingerprint {
    /// Total buffer size in bytes
    pub buffer_bytes: u64,
    /// Unix timestamp when buffer was allocated
    pub allocated_at: u64,
    /// BLAKE3 hash of checkpoint samples
    pub checkpoint_hash: B3Hash,
}

impl GpuBufferFingerprint {
    /// Create fingerprint from buffer metadata and sample data
    pub fn new(
        buffer_bytes: u64,
        first_sample: &[u8],
        last_sample: &[u8],
        mid_sample: &[u8],
    ) -> Self {
        let checkpoint_hash = B3Hash::hash_multi(&[first_sample, last_sample, mid_sample]);
        let allocated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            buffer_bytes,
            allocated_at,
            checkpoint_hash,
        }
    }

    /// Verify another fingerprint matches this one
    pub fn matches(&self, other: &Self) -> bool {
        self.buffer_bytes == other.buffer_bytes && self.checkpoint_hash == other.checkpoint_hash
    }
}

/// Memory footprint baseline for adaptive anomaly detection
#[derive(Debug, Clone)]
struct MemoryFootprintBaseline {
    adapter_id: u32,
    samples: Vec<u64>,
    max_samples: usize,
    mean: Option<f64>,
    stddev: Option<f64>,
}

impl MemoryFootprintBaseline {
    fn new(adapter_id: u32, max_samples: usize) -> Self {
        Self {
            adapter_id,
            samples: Vec::with_capacity(max_samples),
            max_samples,
            mean: None,
            stddev: None,
        }
    }

    fn add_sample(&mut self, bytes: u64) {
        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }
        self.samples.push(bytes);
        self.mean = None;
        self.stddev = None;
    }

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

    fn check_footprint(&mut self, bytes: u64) -> (bool, f64) {
        if self.samples.len() < 2 {
            return (true, 0.0);
        }

        let mean = self.compute_mean();
        let stddev = self.compute_stddev();

        if stddev == 0.0 {
            return (
                bytes as f64 == mean,
                if bytes as f64 == mean { 0.0 } else { f64::MAX },
            );
        }

        let z_score = (bytes as f64 - mean).abs() / stddev;
        let within_tolerance = z_score <= 2.0; // 2σ threshold

        (within_tolerance, z_score)
    }

    fn stats(&mut self) -> (f64, f64, usize) {
        let mean = self.compute_mean();
        let stddev = self.compute_stddev();
        (mean, stddev, self.samples.len())
    }
}

/// Unified memory tracker for all backends
pub struct UnifiedMemoryTracker {
    /// Allocations by adapter ID
    allocations: Arc<RwLock<HashMap<u32, Vec<BackendAllocation>>>>,
    /// Memory limits
    limits: MemoryLimits,
    /// Memory footprint baselines
    baselines: Arc<RwLock<HashMap<u32, MemoryFootprintBaseline>>>,
}

impl UnifiedMemoryTracker {
    /// Create a new unified memory tracker
    pub fn new(limits: MemoryLimits) -> Self {
        Self {
            allocations: Arc::new(RwLock::new(HashMap::new())),
            limits,
            baselines: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Track adapter allocation on a specific backend
    pub fn track_adapter(
        &self,
        adapter_id: u32,
        backend: BackendType,
        buffer_bytes: u64,
        kv_cache_bytes: u64,
    ) {
        let mut allocations = self.allocations.write().unwrap();
        let entry = allocations.entry(adapter_id).or_insert_with(Vec::new);

        let allocation = BackendAllocation {
            adapter_id,
            backend,
            buffer_bytes,
            kv_cache_bytes,
            fingerprint: None,
            allocated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        entry.push(allocation);

        // Update baseline
        let total_bytes = buffer_bytes + kv_cache_bytes;
        self.baselines
            .write()
            .unwrap()
            .entry(adapter_id)
            .or_insert_with(|| MemoryFootprintBaseline::new(adapter_id, 100))
            .add_sample(total_bytes);

        debug!(
            adapter_id = adapter_id,
            backend = backend.as_str(),
            buffer_bytes = buffer_bytes,
            kv_cache_bytes = kv_cache_bytes,
            "Tracked adapter memory allocation"
        );
    }

    /// Untrack adapter (remove all backend allocations)
    pub fn untrack_adapter(&self, adapter_id: u32) -> Option<u64> {
        let mut allocations = self.allocations.write().unwrap();
        let removed = allocations.remove(&adapter_id);

        if let Some(allocs) = removed {
            let total_freed: u64 = allocs
                .iter()
                .map(|a| a.buffer_bytes + a.kv_cache_bytes)
                .sum();

            debug!(
                adapter_id = adapter_id,
                total_freed = total_freed,
                "Untracked adapter memory"
            );

            Some(total_freed)
        } else {
            None
        }
    }

    /// Store GPU buffer fingerprint
    pub fn store_fingerprint(&self, adapter_id: u32, fingerprint: GpuBufferFingerprint) {
        let mut allocations = self.allocations.write().unwrap();
        if let Some(allocs) = allocations.get_mut(&adapter_id) {
            // Find Metal allocation and update fingerprint
            for alloc in allocs.iter_mut() {
                if alloc.backend == BackendType::Metal {
                    alloc.fingerprint = Some(fingerprint.clone());
                    break;
                }
            }
        }
    }

    /// Verify fingerprint matches stored baseline
    pub fn verify_fingerprint(
        &self,
        adapter_id: u32,
        current: &GpuBufferFingerprint,
    ) -> Result<bool> {
        let allocations = self.allocations.read().unwrap();
        if let Some(allocs) = allocations.get(&adapter_id) {
            for alloc in allocs {
                if alloc.backend == BackendType::Metal {
                    if let Some(baseline) = &alloc.fingerprint {
                        if baseline.matches(current) {
                            return Ok(true);
                        } else {
                            return Err(AosError::Memory(format!(
                                "Fingerprint mismatch for adapter {}: expected size {} bytes, got {}",
                                adapter_id, baseline.buffer_bytes, current.buffer_bytes
                            )));
                        }
                    } else {
                        // No baseline - first load
                        return Ok(false);
                    }
                }
            }
        }
        Ok(false)
    }

    /// Check memory footprint against adaptive baseline
    pub fn check_memory_footprint(
        &self,
        adapter_id: u32,
        bytes: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        let mut baselines = self.baselines.write().unwrap();
        if let Some(baseline) = baselines.get_mut(&adapter_id) {
            let (within_tolerance, z_score) = baseline.check_footprint(bytes);
            let stats = baseline.stats();
            (within_tolerance, z_score, Some(stats))
        } else {
            let mut baseline = MemoryFootprintBaseline::new(adapter_id, 100);
            baseline.add_sample(bytes);
            let stats = baseline.stats();
            baselines.insert(adapter_id, baseline);
            (true, 0.0, Some(stats))
        }
    }

    /// Get total memory used across all backends
    pub fn get_total_memory(&self) -> u64 {
        let allocations = self.allocations.read().unwrap();
        allocations
            .values()
            .flat_map(|allocs| allocs.iter())
            .map(|a| a.buffer_bytes + a.kv_cache_bytes)
            .sum()
    }

    /// Get memory used by specific backend
    pub fn get_backend_memory(&self, backend: BackendType) -> u64 {
        let allocations = self.allocations.read().unwrap();
        allocations
            .values()
            .flat_map(|allocs| allocs.iter())
            .filter(|a| a.backend == backend)
            .map(|a| a.buffer_bytes + a.kv_cache_bytes)
            .sum()
    }

    /// Get memory used by specific adapter
    pub fn get_adapter_memory(&self, adapter_id: u32) -> u64 {
        let allocations = self.allocations.read().unwrap();
        allocations
            .get(&adapter_id)
            .map(|allocs| {
                allocs
                    .iter()
                    .map(|a| a.buffer_bytes + a.kv_cache_bytes)
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Get all allocations for an adapter (across all backends)
    pub fn get_adapter_allocations(&self, adapter_id: u32) -> Vec<(BackendType, u64)> {
        let allocations = self.allocations.read().unwrap();
        allocations
            .get(&adapter_id)
            .map(|allocs| {
                allocs
                    .iter()
                    .map(|a| (a.backend, a.buffer_bytes + a.kv_cache_bytes))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check memory pressure and get recommended action
    pub fn check_memory_pressure(&self) -> MemoryPressure {
        let total_vram = self.get_backend_memory(BackendType::Metal)
            + self.get_backend_memory(BackendType::CoreML);
        let total_unified = self.get_backend_memory(BackendType::Mlx);

        let vram_available = self.limits.max_vram.saturating_sub(total_vram);
        let vram_headroom_pct = (vram_available as f32 / self.limits.max_vram as f32) * 100.0;

        let system_available = self
            .limits
            .max_system_ram
            .saturating_sub(total_unified + total_vram);
        let system_headroom_pct =
            (system_available as f32 / self.limits.max_system_ram as f32) * 100.0;

        // Use worst-case headroom
        let headroom_pct = vram_headroom_pct.min(system_headroom_pct);
        let level = PressureLevel::from_headroom(headroom_pct, self.limits.headroom_pct * 100.0);

        let action = match level {
            PressureLevel::Low => EvictionStrategy::None,
            PressureLevel::Medium => EvictionStrategy::EvictLowPriority,
            PressureLevel::High => EvictionStrategy::EvictCrossBackend,
            PressureLevel::Critical => EvictionStrategy::EmergencyEvict,
        };

        let target_headroom_bytes = self.limits.vram_headroom_bytes();
        let bytes_to_free = if vram_available < target_headroom_bytes {
            target_headroom_bytes - vram_available
        } else {
            0
        };

        MemoryPressure {
            level,
            action,
            headroom_pct,
            bytes_to_free,
        }
    }

    /// Get candidates for eviction (sorted by priority, lowest first)
    /// Returns (adapter_id, backend, bytes, priority_score)
    ///
    /// Priority score:
    /// - Pinned adapters: f32::MAX (never evict)
    /// - Metal: 1.0
    /// - CoreML: 0.5 (evict Metal before CoreML for ANE efficiency)
    /// - MLX: 0.75
    pub fn get_eviction_candidates(
        &self,
        pinned_adapters: &[u32],
    ) -> Vec<(u32, BackendType, u64, f32)> {
        let allocations = self.allocations.read().unwrap();
        let mut candidates = Vec::new();

        for (adapter_id, allocs) in allocations.iter() {
            let is_pinned = pinned_adapters.contains(adapter_id);

            for alloc in allocs {
                let bytes = alloc.buffer_bytes + alloc.kv_cache_bytes;

                let priority_score = if is_pinned {
                    f32::MAX
                } else {
                    match alloc.backend {
                        BackendType::Metal => 1.0,
                        BackendType::Mlx => 0.75,
                        BackendType::CoreML => 0.5, // Evict Metal before CoreML
                    }
                };

                candidates.push((*adapter_id, alloc.backend, bytes, priority_score));
            }
        }

        // Sort by priority (lowest first), then by bytes (largest first)
        candidates.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap().then_with(|| b.2.cmp(&a.2)));

        candidates
    }

    /// Get number of tracked adapters
    pub fn adapter_count(&self) -> usize {
        self.allocations.read().unwrap().len()
    }

    /// Clear all allocations
    pub fn clear(&self) {
        self.allocations.write().unwrap().clear();
        self.baselines.write().unwrap().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_tracker_creation() {
        let limits = MemoryLimits::new(8 * 1024 * 1024 * 1024, 16 * 1024 * 1024 * 1024, 0.15);
        let tracker = UnifiedMemoryTracker::new(limits);
        assert_eq!(tracker.adapter_count(), 0);
        assert_eq!(tracker.get_total_memory(), 0);
    }

    #[test]
    fn test_track_multiple_backends() {
        let limits = MemoryLimits::new(8 * 1024 * 1024 * 1024, 16 * 1024 * 1024 * 1024, 0.15);
        let tracker = UnifiedMemoryTracker::new(limits);

        tracker.track_adapter(1, BackendType::Metal, 1024, 512);
        tracker.track_adapter(1, BackendType::Mlx, 2048, 1024);

        assert_eq!(tracker.adapter_count(), 1);
        assert_eq!(tracker.get_adapter_memory(1), 1024 + 512 + 2048 + 1024);
        assert_eq!(tracker.get_backend_memory(BackendType::Metal), 1024 + 512);
        assert_eq!(tracker.get_backend_memory(BackendType::Mlx), 2048 + 1024);
    }

    #[test]
    fn test_memory_pressure_detection() {
        let limits = MemoryLimits::new(1024, 2048, 0.15); // 15% headroom
        let tracker = UnifiedMemoryTracker::new(limits);

        // Low pressure
        tracker.track_adapter(1, BackendType::Metal, 100, 0);
        let pressure = tracker.check_memory_pressure();
        assert_eq!(pressure.level, PressureLevel::Low);

        // High pressure
        tracker.track_adapter(2, BackendType::Metal, 800, 0);
        let pressure = tracker.check_memory_pressure();
        assert!(pressure.level == PressureLevel::High || pressure.level == PressureLevel::Critical);
    }

    #[test]
    fn test_eviction_candidates_priority() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = UnifiedMemoryTracker::new(limits);

        tracker.track_adapter(1, BackendType::Metal, 100, 0);
        tracker.track_adapter(2, BackendType::CoreML, 200, 0);
        tracker.track_adapter(3, BackendType::Mlx, 150, 0);

        let candidates = tracker.get_eviction_candidates(&[]);

        // Should be sorted: CoreML (0.5), MLX (0.75), Metal (1.0)
        assert_eq!(candidates[0].1, BackendType::CoreML);
        assert_eq!(candidates[1].1, BackendType::Mlx);
        assert_eq!(candidates[2].1, BackendType::Metal);
    }

    #[test]
    fn test_pinned_adapters_not_evicted() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = UnifiedMemoryTracker::new(limits);

        tracker.track_adapter(1, BackendType::Metal, 100, 0);
        tracker.track_adapter(2, BackendType::Metal, 200, 0);

        let candidates = tracker.get_eviction_candidates(&[1]);

        // Adapter 1 should have MAX priority (never evict)
        let adapter1 = candidates.iter().find(|(id, _, _, _)| *id == 1).unwrap();
        assert_eq!(adapter1.3, f32::MAX);

        // Adapter 2 should have normal priority
        let adapter2 = candidates.iter().find(|(id, _, _, _)| *id == 2).unwrap();
        assert_eq!(adapter2.3, 1.0);
    }

    #[test]
    fn test_fingerprint_verification() {
        let limits = MemoryLimits::new(1024, 2048, 0.15);
        let tracker = UnifiedMemoryTracker::new(limits);

        tracker.track_adapter(1, BackendType::Metal, 1024, 0);

        let fp1 = GpuBufferFingerprint::new(1024, b"first", b"last", b"mid");
        tracker.store_fingerprint(1, fp1.clone());

        // Same fingerprint should verify
        let result = tracker.verify_fingerprint(1, &fp1);
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Different fingerprint should fail
        let fp2 = GpuBufferFingerprint::new(2048, b"first", b"last", b"mid");
        let result = tracker.verify_fingerprint(1, &fp2);
        assert!(result.is_err());
    }
}
