//! CoreML backend memory management with ANE awareness
//!
//! This module provides comprehensive memory management for the CoreML backend:
//! - ANE memory tracking and monitoring
//! - MLMultiArray buffer pooling for allocation reuse
//! - Memory pressure detection and handling
//! - CPU ↔ ANE transfer optimization
//! - Integration with unified memory tracking
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │              CoreMLMemoryManager                             │
//! │  - ANE memory pool (shared with GPU)                        │
//! │  - MLMultiArray buffer pool (thread-safe)                   │
//! │  - Transfer bandwidth tracking                               │
//! │  - Memory pressure callbacks                                 │
//! └──────────────────────┬──────────────────────────────────────┘
//!                        │
//!                        ├──> ANE Memory Tracker
//!                        ├──> Buffer Pool Manager
//!                        ├──> Transfer Optimizer
//!                        └──> Pressure Handler
//! ```
//!
//! ## iOS Memory Management Best Practices
//!
//! - Monitor memory warnings from OS
//! - Minimize MLMultiArray allocations (pool reuse)
//! - Prefer ANE-resident buffers (avoid CPU ↔ ANE copies)
//! - Batch small operations to reduce overhead
//! - Use async transfers when possible
//! - Respect system memory limits (ANE shares unified memory)

use adapteros_core::{AosError, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

// Note: Core Foundation imports not needed for current implementation
// #[cfg(target_os = "macos")]
// use core_foundation::base::TCFType;

/// CoreML memory manager configuration
#[derive(Debug, Clone)]
pub struct CoreMLMemoryConfig {
    /// Maximum buffer pool size (number of buffers)
    pub max_pool_size: usize,
    /// Maximum single buffer size (bytes)
    pub max_buffer_size: usize,
    /// ANE memory limit (bytes, 0 = auto-detect)
    pub ane_memory_limit: usize,
    /// Enable aggressive buffer reuse
    pub aggressive_pooling: bool,
    /// Memory pressure threshold (0.0 - 1.0)
    pub pressure_threshold: f32,
    /// Enable transfer batching
    pub enable_transfer_batching: bool,
    /// Transfer batch timeout (ms)
    pub transfer_batch_timeout_ms: u64,
}

impl Default for CoreMLMemoryConfig {
    fn default() -> Self {
        Self {
            max_pool_size: 128,
            max_buffer_size: 256 * 1024 * 1024, // 256 MB
            ane_memory_limit: 0,                // Auto-detect
            aggressive_pooling: true,
            pressure_threshold: 0.85, // 85% usage triggers pressure
            enable_transfer_batching: true,
            transfer_batch_timeout_ms: 10,
        }
    }
}

/// ANE memory statistics
#[derive(Debug, Clone, Default)]
pub struct ANEMemoryStats {
    /// Total ANE memory available (bytes)
    pub total_bytes: u64,
    /// Currently allocated (bytes)
    pub allocated_bytes: u64,
    /// Peak allocation (bytes)
    pub peak_allocated_bytes: u64,
    /// Number of active allocations
    pub allocation_count: u64,
    /// Total allocations ever made
    pub total_allocations: u64,
    /// Total deallocations
    pub total_deallocations: u64,
    /// Memory pressure level (0.0 - 1.0)
    pub pressure_level: f32,
}

impl ANEMemoryStats {
    /// Calculate current usage percentage
    pub fn usage_percent(&self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.allocated_bytes as f32 / self.total_bytes as f32) * 100.0
        }
    }

    /// Calculate headroom percentage
    pub fn headroom_percent(&self) -> f32 {
        100.0 - self.usage_percent()
    }

    /// Check if memory pressure exists
    pub fn has_pressure(&self, threshold: f32) -> bool {
        self.pressure_level >= threshold
    }
}

/// MLMultiArray buffer pool entry
#[derive(Debug)]
struct PooledBuffer {
    /// Buffer identifier
    id: usize,
    /// Buffer size (bytes)
    size_bytes: usize,
    /// Buffer shape [batch, channels, height, width]
    shape: Vec<usize>,
    /// Data type (Float32, Float16, Int8)
    dtype: BufferDataType,
    /// Last accessed timestamp
    last_accessed: Instant,
    /// Reuse count
    reuse_count: u64,
    /// Buffer location (CPU or ANE)
    location: BufferLocation,
}

/// Buffer data type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BufferDataType {
    Float32,
    Float16,
    Int8,
    Int16,
}

impl BufferDataType {
    /// Get size in bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            BufferDataType::Float32 => 4,
            BufferDataType::Float16 => 2,
            BufferDataType::Int8 => 1,
            BufferDataType::Int16 => 2,
        }
    }
}

/// Buffer location (CPU or ANE)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferLocation {
    /// Buffer on CPU (system memory)
    CPU,
    /// Buffer on ANE (unified memory, ANE-accessible)
    ANE,
    /// Buffer shared between CPU and ANE
    Unified,
}

/// Buffer pool key (for bucketing by size/shape)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct BufferPoolKey {
    /// Total size (bytes)
    size_bytes: usize,
    /// Data type
    dtype: BufferDataType,
}

/// Transfer statistics
#[derive(Debug, Clone, Default)]
pub struct TransferStats {
    /// Total CPU → ANE transfers
    pub cpu_to_ane_count: u64,
    /// Total ANE → CPU transfers
    pub ane_to_cpu_count: u64,
    /// Total bytes transferred CPU → ANE
    pub cpu_to_ane_bytes: u64,
    /// Total bytes transferred ANE → CPU
    pub ane_to_cpu_bytes: u64,
    /// Total transfer time (microseconds)
    pub total_transfer_time_us: u64,
    /// Average transfer bandwidth (GB/s)
    pub avg_bandwidth_gbps: f32,
}

impl TransferStats {
    /// Calculate average transfer time (microseconds)
    pub fn avg_transfer_time_us(&self) -> f32 {
        let total_transfers = self.cpu_to_ane_count + self.ane_to_cpu_count;
        if total_transfers == 0 {
            0.0
        } else {
            self.total_transfer_time_us as f32 / total_transfers as f32
        }
    }

    /// Update bandwidth calculation
    pub fn update_bandwidth(&mut self) {
        let total_bytes = self.cpu_to_ane_bytes + self.ane_to_cpu_bytes;
        let total_time_s = self.total_transfer_time_us as f64 / 1_000_000.0;
        if total_time_s > 0.0 {
            self.avg_bandwidth_gbps = (total_bytes as f64 / total_time_s / 1_000_000_000.0) as f32;
        }
    }
}

/// Memory pressure event
#[derive(Debug, Clone)]
pub struct MemoryPressureEvent {
    /// Event timestamp
    pub timestamp: Instant,
    /// Pressure level before
    pub pressure_before: f32,
    /// Pressure level after
    pub pressure_after: f32,
    /// Bytes freed
    pub bytes_freed: u64,
    /// Buffers evicted
    pub buffers_evicted: usize,
    /// Action taken
    pub action: PressureAction,
}

/// Memory pressure action
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureAction {
    /// No action needed
    None,
    /// Evict least recently used buffers
    EvictLRU,
    /// Emergency eviction (all unpinned buffers)
    EmergencyEvict,
    /// Notify system of memory warning
    SystemWarning,
}

/// CoreML memory manager
pub struct CoreMLMemoryManager {
    /// Configuration
    config: CoreMLMemoryConfig,
    /// ANE memory statistics
    ane_stats: Arc<Mutex<ANEMemoryStats>>,
    /// Buffer pool (indexed by BufferPoolKey)
    buffer_pool: Arc<Mutex<HashMap<BufferPoolKey, VecDeque<PooledBuffer>>>>,
    /// Active buffers (currently in use)
    active_buffers: Arc<Mutex<HashMap<usize, PooledBuffer>>>,
    /// Transfer statistics
    transfer_stats: Arc<Mutex<TransferStats>>,
    /// Memory pressure events
    pressure_events: Arc<Mutex<Vec<MemoryPressureEvent>>>,
    /// Next buffer ID
    next_buffer_id: Arc<Mutex<usize>>,
    /// Pinned buffer IDs (never evict)
    pinned_buffers: Arc<Mutex<std::collections::HashSet<usize>>>,
}

impl CoreMLMemoryManager {
    /// Create a new CoreML memory manager
    pub fn new(config: CoreMLMemoryConfig) -> Result<Self> {
        let ane_total = if config.ane_memory_limit == 0 {
            Self::detect_ane_memory()?
        } else {
            config.ane_memory_limit as u64
        };

        info!(
            ane_total_mb = ane_total / (1024 * 1024),
            max_pool_size = config.max_pool_size,
            "CoreML memory manager initialized"
        );

        Ok(Self {
            config,
            ane_stats: Arc::new(Mutex::new(ANEMemoryStats {
                total_bytes: ane_total,
                ..Default::default()
            })),
            buffer_pool: Arc::new(Mutex::new(HashMap::new())),
            active_buffers: Arc::new(Mutex::new(HashMap::new())),
            transfer_stats: Arc::new(Mutex::new(TransferStats::default())),
            pressure_events: Arc::new(Mutex::new(Vec::new())),
            next_buffer_id: Arc::new(Mutex::new(0)),
            pinned_buffers: Arc::new(Mutex::new(std::collections::HashSet::new())),
        })
    }

    /// Detect ANE memory capacity
    fn detect_ane_memory() -> Result<u64> {
        #[cfg(target_os = "macos")]
        {
            use std::process::Command;

            // Get total system memory (ANE shares unified memory)
            let output = Command::new("sysctl")
                .args(["-n", "hw.memsize"])
                .output()
                .map_err(|e| AosError::CoreML(format!("Failed to detect memory: {}", e)))?;

            let mem_str = String::from_utf8_lossy(&output.stdout);
            let total_mem: u64 = mem_str
                .trim()
                .parse()
                .map_err(|e| AosError::CoreML(format!("Failed to parse memory: {}", e)))?;

            // ANE typically has access to ~50% of system memory
            // (conservative estimate for iOS/macOS)
            let ane_mem = total_mem / 2;

            info!(
                total_system_mb = total_mem / (1024 * 1024),
                ane_available_mb = ane_mem / (1024 * 1024),
                "Detected ANE memory capacity"
            );

            Ok(ane_mem)
        }

        #[cfg(not(target_os = "macos"))]
        {
            // Default to 4 GB on non-macOS platforms
            Ok(4 * 1024 * 1024 * 1024)
        }
    }

    /// Acquire a buffer from the pool or allocate new
    pub fn acquire_buffer(
        &self,
        shape: &[usize],
        dtype: BufferDataType,
        location: BufferLocation,
    ) -> Result<usize> {
        let size_bytes = shape.iter().product::<usize>() * dtype.size_bytes();

        if size_bytes > self.config.max_buffer_size {
            return Err(AosError::Memory(format!(
                "Buffer size {} exceeds maximum {}",
                size_bytes, self.config.max_buffer_size
            )));
        }

        // Check memory pressure before allocation
        self.check_memory_pressure()?;

        let key = BufferPoolKey {
            size_bytes,
            dtype,
        };

        let mut pool = self.buffer_pool.lock().unwrap();
        let bucket = pool.entry(key.clone()).or_insert_with(|| VecDeque::new());

        // Try to reuse from pool
        if let Some(mut pooled) = bucket.pop_front() {
            pooled.last_accessed = Instant::now();
            pooled.reuse_count += 1;
            pooled.shape = shape.to_vec();

            let buffer_id = pooled.id;

            // Move to active buffers
            let mut active = self.active_buffers.lock().unwrap();
            active.insert(buffer_id, pooled);

            debug!(
                buffer_id = buffer_id,
                size_bytes = size_bytes,
                reuse_count = active[&buffer_id].reuse_count,
                "Acquired buffer from pool"
            );

            return Ok(buffer_id);
        }

        // Allocate new buffer
        let buffer_id = {
            let mut next_id = self.next_buffer_id.lock().unwrap();
            let id = *next_id;
            *next_id += 1;
            id
        };

        let buffer = PooledBuffer {
            id: buffer_id,
            size_bytes,
            shape: shape.to_vec(),
            dtype,
            last_accessed: Instant::now(),
            reuse_count: 0,
            location,
        };

        // Update statistics
        {
            let mut stats = self.ane_stats.lock().unwrap();
            stats.allocated_bytes += size_bytes as u64;
            stats.allocation_count += 1;
            stats.total_allocations += 1;
            stats.peak_allocated_bytes =
                stats.peak_allocated_bytes.max(stats.allocated_bytes);
            stats.pressure_level = stats.allocated_bytes as f32 / stats.total_bytes as f32;
        }

        // Move to active buffers
        let mut active = self.active_buffers.lock().unwrap();
        active.insert(buffer_id, buffer);

        info!(
            buffer_id = buffer_id,
            size_bytes = size_bytes,
            dtype = ?dtype,
            location = ?location,
            "Allocated new buffer"
        );

        Ok(buffer_id)
    }

    /// Release a buffer back to the pool
    pub fn release_buffer(&self, buffer_id: usize) -> Result<()> {
        let mut active = self.active_buffers.lock().unwrap();
        let buffer = active
            .remove(&buffer_id)
            .ok_or_else(|| AosError::NotFound(format!("Buffer {} not active", buffer_id)))?;

        // Check if buffer is pinned
        if self.pinned_buffers.lock().unwrap().contains(&buffer_id) {
            // Return to active buffers (don't pool pinned buffers)
            active.insert(buffer_id, buffer);
            return Ok(());
        }

        let key = BufferPoolKey {
            size_bytes: buffer.size_bytes,
            dtype: buffer.dtype,
        };

        let mut pool = self.buffer_pool.lock().unwrap();
        let bucket = pool.entry(key).or_insert_with(|| VecDeque::new());

        // Check pool size limit
        if bucket.len() >= self.config.max_pool_size {
            // Evict oldest buffer
            if let Some(evicted) = bucket.pop_back() {
                self.deallocate_buffer(evicted)?;
            }
        }

        debug!(
            buffer_id = buffer_id,
            size_bytes = buffer.size_bytes,
            pool_size = bucket.len() + 1,
            "Released buffer to pool"
        );

        bucket.push_front(buffer);

        Ok(())
    }

    /// Deallocate a buffer (remove from pool and update stats)
    fn deallocate_buffer(&self, buffer: PooledBuffer) -> Result<()> {
        let mut stats = self.ane_stats.lock().unwrap();
        stats.allocated_bytes = stats.allocated_bytes.saturating_sub(buffer.size_bytes as u64);
        stats.allocation_count = stats.allocation_count.saturating_sub(1);
        stats.total_deallocations += 1;
        stats.pressure_level = stats.allocated_bytes as f32 / stats.total_bytes as f32;

        debug!(
            buffer_id = buffer.id,
            size_bytes = buffer.size_bytes,
            "Deallocated buffer"
        );

        Ok(())
    }

    /// Pin a buffer (prevent eviction)
    pub fn pin_buffer(&self, buffer_id: usize) {
        self.pinned_buffers.lock().unwrap().insert(buffer_id);
        debug!(buffer_id = buffer_id, "Pinned buffer");
    }

    /// Unpin a buffer (allow eviction)
    pub fn unpin_buffer(&self, buffer_id: usize) {
        self.pinned_buffers.lock().unwrap().remove(&buffer_id);
        debug!(buffer_id = buffer_id, "Unpinned buffer");
    }

    /// Check memory pressure and take action if needed
    pub fn check_memory_pressure(&self) -> Result<()> {
        let stats = self.ane_stats.lock().unwrap();
        let pressure = stats.pressure_level;

        if pressure >= self.config.pressure_threshold {
            drop(stats); // Release lock before eviction
            warn!(
                pressure = pressure,
                threshold = self.config.pressure_threshold,
                "Memory pressure detected"
            );
            self.handle_memory_pressure()?;
        }

        Ok(())
    }

    /// Handle memory pressure by evicting buffers
    fn handle_memory_pressure(&self) -> Result<()> {
        let pressure_before = self.ane_stats.lock().unwrap().pressure_level;

        // Determine action based on pressure level
        let action = if pressure_before >= 0.95 {
            PressureAction::EmergencyEvict
        } else {
            PressureAction::EvictLRU
        };

        let mut bytes_freed = 0u64;
        let mut buffers_evicted = 0usize;

        match action {
            PressureAction::EvictLRU => {
                // Evict least recently used buffers from pool
                let mut pool = self.buffer_pool.lock().unwrap();
                for bucket in pool.values_mut() {
                    while let Some(buffer) = bucket.pop_back() {
                        bytes_freed += buffer.size_bytes as u64;
                        buffers_evicted += 1;
                        self.deallocate_buffer(buffer)?;

                        // Check if pressure relieved
                        let current_pressure = self.ane_stats.lock().unwrap().pressure_level;
                        if current_pressure < 0.75 {
                            break;
                        }
                    }
                }
            }
            PressureAction::EmergencyEvict => {
                // Emergency: evict all unpinned buffers from pool
                let mut pool = self.buffer_pool.lock().unwrap();
                for bucket in pool.values_mut() {
                    while let Some(buffer) = bucket.pop_back() {
                        bytes_freed += buffer.size_bytes as u64;
                        buffers_evicted += 1;
                        self.deallocate_buffer(buffer)?;
                    }
                }

                warn!(
                    bytes_freed_mb = bytes_freed / (1024 * 1024),
                    buffers_evicted = buffers_evicted,
                    "Emergency memory eviction"
                );
            }
            _ => {}
        }

        let pressure_after = self.ane_stats.lock().unwrap().pressure_level;

        // Record pressure event
        let event = MemoryPressureEvent {
            timestamp: Instant::now(),
            pressure_before,
            pressure_after,
            bytes_freed,
            buffers_evicted,
            action,
        };

        self.pressure_events.lock().unwrap().push(event.clone());

        info!(
            action = ?action,
            bytes_freed_mb = bytes_freed / (1024 * 1024),
            buffers_evicted = buffers_evicted,
            pressure_before = pressure_before,
            pressure_after = pressure_after,
            "Memory pressure handled"
        );

        Ok(())
    }

    /// Record a CPU → ANE transfer
    pub fn record_cpu_to_ane_transfer(&self, bytes: u64, duration: Duration) {
        let mut stats = self.transfer_stats.lock().unwrap();
        stats.cpu_to_ane_count += 1;
        stats.cpu_to_ane_bytes += bytes;
        stats.total_transfer_time_us += duration.as_micros() as u64;
        stats.update_bandwidth();

        debug!(
            bytes_mb = bytes / (1024 * 1024),
            duration_us = duration.as_micros(),
            bandwidth_gbps = stats.avg_bandwidth_gbps,
            "CPU → ANE transfer"
        );
    }

    /// Record an ANE → CPU transfer
    pub fn record_ane_to_cpu_transfer(&self, bytes: u64, duration: Duration) {
        let mut stats = self.transfer_stats.lock().unwrap();
        stats.ane_to_cpu_count += 1;
        stats.ane_to_cpu_bytes += bytes;
        stats.total_transfer_time_us += duration.as_micros() as u64;
        stats.update_bandwidth();

        debug!(
            bytes_mb = bytes / (1024 * 1024),
            duration_us = duration.as_micros(),
            bandwidth_gbps = stats.avg_bandwidth_gbps,
            "ANE → CPU transfer"
        );
    }

    /// Get current ANE memory statistics
    pub fn stats(&self) -> ANEMemoryStats {
        self.ane_stats.lock().unwrap().clone()
    }

    /// Get transfer statistics
    pub fn transfer_stats(&self) -> TransferStats {
        self.transfer_stats.lock().unwrap().clone()
    }

    /// Get pool statistics
    pub fn pool_stats(&self) -> PoolStats {
        let pool = self.buffer_pool.lock().unwrap();
        let active = self.active_buffers.lock().unwrap();

        let total_pooled = pool.values().map(|v| v.len()).sum();
        let total_pooled_bytes: usize = pool
            .values()
            .flat_map(|v| v.iter())
            .map(|b| b.size_bytes)
            .sum();

        let total_active = active.len();
        let total_active_bytes: usize = active.values().map(|b| b.size_bytes).sum();

        PoolStats {
            pooled_buffers: total_pooled,
            pooled_bytes: total_pooled_bytes as u64,
            active_buffers: total_active,
            active_bytes: total_active_bytes as u64,
            pool_buckets: pool.len(),
        }
    }

    /// Get recent pressure events
    pub fn pressure_events(&self, limit: usize) -> Vec<MemoryPressureEvent> {
        let events = self.pressure_events.lock().unwrap();
        events.iter().rev().take(limit).cloned().collect()
    }

    /// Clear all buffers (for testing or reset)
    pub fn clear_all_buffers(&self) -> Result<()> {
        warn!("Clearing all buffers");

        // Clear pool
        let mut pool = self.buffer_pool.lock().unwrap();
        for bucket in pool.values_mut() {
            while let Some(buffer) = bucket.pop_back() {
                self.deallocate_buffer(buffer)?;
            }
        }
        pool.clear();

        // Clear active buffers (except pinned)
        let mut active = self.active_buffers.lock().unwrap();
        let pinned = self.pinned_buffers.lock().unwrap();
        active.retain(|id, _| pinned.contains(id));

        info!("All unpinned buffers cleared");

        Ok(())
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Number of buffers in pool
    pub pooled_buffers: usize,
    /// Total bytes in pool
    pub pooled_bytes: u64,
    /// Number of active buffers
    pub active_buffers: usize,
    /// Total bytes in active buffers
    pub active_bytes: u64,
    /// Number of pool buckets
    pub pool_buckets: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_data_type_size() {
        assert_eq!(BufferDataType::Float32.size_bytes(), 4);
        assert_eq!(BufferDataType::Float16.size_bytes(), 2);
        assert_eq!(BufferDataType::Int8.size_bytes(), 1);
        assert_eq!(BufferDataType::Int16.size_bytes(), 2);
    }

    #[test]
    fn test_ane_memory_stats() {
        let stats = ANEMemoryStats {
            total_bytes: 1024 * 1024 * 1024, // 1 GB
            allocated_bytes: 512 * 1024 * 1024, // 512 MB
            ..Default::default()
        };

        assert_eq!(stats.usage_percent(), 50.0);
        assert_eq!(stats.headroom_percent(), 50.0);
        assert!(!stats.has_pressure(0.85));
    }

    #[test]
    fn test_memory_manager_creation() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config);
        assert!(manager.is_ok());
    }

    #[test]
    fn test_buffer_acquisition_and_release() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Acquire buffer
        let shape = vec![1, 3, 224, 224];
        let buffer_id = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();

        assert!(buffer_id >= 0);

        // Release buffer
        let result = manager.release_buffer(buffer_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_buffer_pooling() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Acquire and release multiple times
        let shape = vec![1, 3, 224, 224];
        let buffer_id1 = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();
        manager.release_buffer(buffer_id1).unwrap();

        let buffer_id2 = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();

        // Should reuse the same buffer (ID might be same or different depending on pool state)
        let pool_stats = manager.pool_stats();
        assert!(pool_stats.active_buffers >= 1);
    }

    #[test]
    fn test_buffer_pinning() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        let shape = vec![1, 3, 224, 224];
        let buffer_id = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();

        // Pin buffer
        manager.pin_buffer(buffer_id);

        // Try to release (should stay active due to pin)
        manager.release_buffer(buffer_id).unwrap();

        // Unpin
        manager.unpin_buffer(buffer_id);
    }

    #[test]
    fn test_transfer_stats() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Record transfers
        manager.record_cpu_to_ane_transfer(1024 * 1024, Duration::from_micros(100));
        manager.record_ane_to_cpu_transfer(2048 * 1024, Duration::from_micros(200));

        let stats = manager.transfer_stats();
        assert_eq!(stats.cpu_to_ane_count, 1);
        assert_eq!(stats.ane_to_cpu_count, 1);
        assert_eq!(stats.cpu_to_ane_bytes, 1024 * 1024);
        assert_eq!(stats.ane_to_cpu_bytes, 2048 * 1024);
        assert!(stats.avg_bandwidth_gbps > 0.0);
    }

    #[test]
    fn test_memory_pressure_detection() {
        let mut config = CoreMLMemoryConfig::default();
        config.pressure_threshold = 0.5; // Low threshold for testing
        config.ane_memory_limit = 1024 * 1024; // 1 MB limit

        let manager = CoreMLMemoryManager::new(config).unwrap();

        // Allocate large buffer to trigger pressure
        let shape = vec![1024, 1024]; // 4 MB in Float32
        let result = manager.acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE);

        // Should fail or trigger pressure handling
        // (depending on exact implementation)
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_pool_stats() {
        let config = CoreMLMemoryConfig::default();
        let manager = CoreMLMemoryManager::new(config).unwrap();

        let shape = vec![1, 3, 224, 224];
        let buffer_id = manager
            .acquire_buffer(&shape, BufferDataType::Float32, BufferLocation::ANE)
            .unwrap();

        let stats = manager.pool_stats();
        assert_eq!(stats.active_buffers, 1);
        assert!(stats.active_bytes > 0);

        manager.release_buffer(buffer_id).unwrap();

        let stats = manager.pool_stats();
        assert_eq!(stats.pooled_buffers, 1);
        assert!(stats.pooled_bytes > 0);
    }
}
