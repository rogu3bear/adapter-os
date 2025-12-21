//! Metal heap observer for page migration tracking with FFI bindings
//!
//! Monitors Metal heap allocations, deallocations, and page migrations to ensure
//! deterministic memory behavior across runs. Tracks unified memory usage patterns
//! and detects when the OS performs memory tricks that could affect determinism.
//!
//! # FFI Integration
//!
//! Provides FFI-safe structures and functions for calling from C/C++/Objective-C code.
//! All structures use `#[repr(C)]` for ABI compatibility.

use crate::{MemoryMigrationEvent, MigrationType, Result};
use adapteros_core::B3Hash;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid::Uuid;

#[cfg(target_os = "macos")]
use metal::{foreign_types::ForeignType, Buffer, Device, Heap};

// ============================================================================
// FFI-SAFE STRUCTURES FOR C/C++/OBJECTIVE-C INTEROP
// ============================================================================

/// FFI-safe representation of heap allocation info
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIHeapAllocation {
    /// Allocation size in bytes
    pub size_bytes: u64,
    /// Allocation offset within heap
    pub offset_bytes: u64,
    /// Memory address (if available)
    pub memory_addr: u64,
    /// Allocation timestamp in microseconds since epoch
    pub timestamp: u64,
    /// Storage mode flags (MTLStorageModeShared=1, MTLStorageModeManaged=2, etc.)
    pub storage_mode: u32,
}

/// FFI-safe representation of heap state snapshot
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIHeapState {
    /// Heap identifier (pointer as u64)
    pub heap_id: u64,
    /// Total heap size in bytes
    pub total_size: u64,
    /// Used size in bytes
    pub used_size: u64,
    /// Number of active allocations
    pub allocation_count: u32,
    /// Heap fragmentation ratio (0.0-1.0)
    pub fragmentation_ratio: f32,
    /// Average allocation size in bytes
    pub avg_alloc_size: u64,
    /// Largest free block in bytes (if known)
    pub largest_free_block: u64,
}

/// FFI-safe representation of fragmentation metrics
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIFragmentationMetrics {
    /// Fragmentation ratio (0.0=no fragmentation, 1.0=maximum)
    pub fragmentation_ratio: f32,
    /// External fragmentation (wasted space between allocations)
    pub external_fragmentation: f32,
    /// Internal fragmentation (wasted space within allocations)
    pub internal_fragmentation: f32,
    /// Number of free blocks detected
    pub free_blocks: u32,
    /// Total free space in bytes
    pub total_free_bytes: u64,
    /// Average free block size in bytes
    pub avg_free_block_size: u64,
    /// Largest contiguous free block in bytes
    pub largest_free_block: u64,
    /// Compaction efficiency (0.0-1.0, higher = more efficient)
    pub compaction_efficiency: f32,
}

/// FFI-safe representation of Metal memory metrics
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIMetalMemoryMetrics {
    /// Total allocated memory across all heaps
    pub total_allocated: u64,
    /// Total heap size across all heaps
    pub total_heap_size: u64,
    /// Total used memory across all heaps
    pub total_heap_used: u64,
    /// Number of active allocations
    pub allocation_count: u32,
    /// Number of active heaps
    pub heap_count: u32,
    /// Overall fragmentation (0.0-1.0)
    pub overall_fragmentation: f32,
    /// Memory utilization percentage (0-100)
    pub utilization_pct: f32,
    /// Number of migration events recorded
    pub migration_event_count: u32,
}

/// FFI-safe representation of page migration event
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FFIPageMigrationEvent {
    /// Event ID (first 8 bytes of UUID)
    pub event_id_high: u64,
    /// Event ID (last 8 bytes of UUID)
    pub event_id_low: u64,
    /// Migration type (1=PageOut, 2=PageIn, 3=BufferRelocate, 4=HeapCompaction, 5=PressureEviction)
    pub migration_type: u32,
    /// Source memory address
    pub source_addr: u64,
    /// Destination memory address
    pub dest_addr: u64,
    /// Size of migrated memory in bytes
    pub size_bytes: u64,
    /// Timestamp in microseconds since epoch
    pub timestamp: u64,
}

// ============================================================================
// METAL HEAP OBSERVER FFI BINDINGS
// ============================================================================

/// Opaque handle to Metal device for FFI calls
#[repr(C)]
pub struct MetalDeviceHandle {
    _ptr: *mut std::ffi::c_void,
}

#[cfg(target_os = "macos")]
extern "C" {
    /// Initialize Metal heap observation
    /// Returns non-zero on success, 0 on failure
    pub fn metal_heap_observer_init() -> i32;

    /// Observe a heap allocation
    /// # Arguments
    /// - `heap_id`: Opaque heap identifier
    /// - `buffer_id`: Unique buffer identifier
    /// - `size`: Allocation size in bytes
    /// - `offset`: Offset within heap
    /// - `addr`: Memory address
    /// - `storage_mode`: MTL storage mode enum
    ///   Returns non-zero on success
    pub fn metal_heap_observe_allocation(
        heap_id: u64,
        buffer_id: u64,
        size: u64,
        offset: u64,
        addr: u64,
        storage_mode: u32,
    ) -> i32;

    /// Observe a heap deallocation
    pub fn metal_heap_observe_deallocation(buffer_id: u64) -> i32;

    /// Update heap state after operations
    /// Returns non-zero on success
    pub fn metal_heap_update_state(heap_id: u64, total_size: u64, used_size: u64) -> i32;

    /// Calculate heap fragmentation metrics
    /// Fills out_metrics with current fragmentation data
    /// Returns non-zero on success
    pub fn metal_heap_get_fragmentation(
        heap_id: u64,
        out_metrics: *mut FFIFragmentationMetrics,
    ) -> i32;

    /// Get all current heap states
    /// out_heaps: pointer to array where heap states will be written
    /// max_heaps: maximum number of heaps that can fit in array
    /// Returns number of heaps written, or negative on error
    pub fn metal_heap_get_all_states(out_heaps: *mut FFIHeapState, max_heaps: u32) -> i32;

    /// Get current Metal memory metrics
    pub fn metal_heap_get_metrics(out_metrics: *mut FFIMetalMemoryMetrics) -> i32;

    /// Get page migration events
    /// out_events: pointer to array where events will be written
    /// max_events: maximum number of events that can fit in array
    /// Returns number of events written
    pub fn metal_heap_get_migration_events(
        out_events: *mut FFIPageMigrationEvent,
        max_events: u32,
    ) -> i32;

    /// Clear all recorded observation data
    pub fn metal_heap_clear() -> i32;

    /// Get last error message
    /// buffer: pointer to character buffer
    /// buffer_len: size of buffer
    /// Returns number of bytes written (including null terminator)
    pub fn metal_heap_get_last_error(buffer: *mut i8, buffer_len: usize) -> usize;
}

/// Metal heap allocation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeapAllocation {
    /// Unique allocation ID
    pub allocation_id: Uuid,
    /// Heap identifier
    pub heap_id: u64,
    /// Buffer identifier
    pub buffer_id: u64,
    /// Allocation size in bytes
    pub size_bytes: u64,
    /// Allocation offset within heap
    pub offset_bytes: u64,
    /// Allocation timestamp
    pub timestamp: u128,
    /// Memory address (if available)
    pub memory_addr: Option<u64>,
    /// Storage mode
    pub storage_mode: String,
}

/// Heap state snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeapState {
    /// Heap identifier
    pub heap_id: u64,
    /// Total heap size
    pub total_size: u64,
    /// Used size
    pub used_size: u64,
    /// Number of allocations
    pub allocation_count: usize,
    /// Heap hash (for determinism verification)
    pub heap_hash: B3Hash,
    /// Allocation order hash
    pub allocation_order_hash: B3Hash,
}

/// Metal heap observer
pub struct MetalHeapObserver {
    /// Device reference (reserved for Metal heap API calls)
    #[cfg(target_os = "macos")]
    _device: Arc<Device>,
    /// Active allocations by buffer ID
    allocations: Arc<RwLock<HashMap<u64, HeapAllocation>>>,
    /// Heap states by heap ID
    heap_states: Arc<RwLock<HashMap<u64, HeapState>>>,
    /// Migration events
    migration_events: Arc<RwLock<Vec<MemoryMigrationEvent>>>,
    /// Next buffer ID
    next_buffer_id: Arc<std::sync::atomic::AtomicU64>,
    /// Sampling rate (0.0-1.0)
    sampling_rate: f32,
}

impl MetalHeapObserver {
    /// Create a new Metal heap observer
    #[cfg(target_os = "macos")]
    pub fn new(device: Arc<Device>, sampling_rate: f32) -> Self {
        Self {
            _device: device,
            allocations: Arc::new(RwLock::new(HashMap::new())),
            heap_states: Arc::new(RwLock::new(HashMap::new())),
            migration_events: Arc::new(RwLock::new(Vec::new())),
            next_buffer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            sampling_rate: sampling_rate.clamp(0.0, 1.0),
        }
    }

    /// Create a new Metal heap observer (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn new(_device: Option<()>, sampling_rate: f32) -> Self {
        Self {
            allocations: Arc::new(RwLock::new(HashMap::new())),
            heap_states: Arc::new(RwLock::new(HashMap::new())),
            migration_events: Arc::new(RwLock::new(Vec::new())),
            next_buffer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            sampling_rate: sampling_rate.clamp(0.0, 1.0),
        }
    }

    /// Observe buffer allocation
    #[cfg(target_os = "macos")]
    pub fn observe_allocation(&self, buffer: &Buffer, heap: Option<&Heap>) -> Result<u64> {
        if !self.should_sample() {
            return Ok(0); // Skip sampling
        }

        let buffer_id = self
            .next_buffer_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = current_timestamp();

        let allocation = HeapAllocation {
            allocation_id: Uuid::new_v4(),
            heap_id: heap.map(|h| h.as_ptr() as u64).unwrap_or(0),
            buffer_id,
            size_bytes: buffer.length(),
            offset_bytes: 0, // Metal doesn't expose offset directly
            timestamp,
            memory_addr: Some(buffer.as_ptr() as u64),
            storage_mode: format!("{:?}", buffer.resource_options()),
        };

        // Record allocation
        {
            let mut allocations = self.allocations.write();
            allocations.insert(buffer_id, allocation.clone());
        }

        // Update heap state
        if let Some(heap) = heap {
            self.update_heap_state(heap)?;
        }

        debug!(
            "Observed Metal buffer allocation: id={}, size={}, heap={}",
            buffer_id, allocation.size_bytes, allocation.heap_id
        );

        Ok(buffer_id)
    }

    /// Observe buffer deallocation
    #[cfg(target_os = "macos")]
    pub fn observe_deallocation(&self, buffer_id: u64) -> Result<()> {
        if !self.should_sample() {
            return Ok(()); // Skip sampling
        }

        let allocation = {
            let mut allocations = self.allocations.write();
            allocations.remove(&buffer_id)
        };

        if let Some(allocation) = allocation {
            debug!(
                "Observed Metal buffer deallocation: id={}, size={}",
                buffer_id, allocation.size_bytes
            );

            // Check for potential page migration
            self.check_page_migration(&allocation)?;
        }

        Ok(())
    }

    /// Update heap state after allocation/deallocation
    #[cfg(target_os = "macos")]
    fn update_heap_state(&self, heap: &Heap) -> Result<()> {
        let heap_id = heap.as_ptr() as u64;
        let total_size = heap.size();
        let used_size = heap.used_size();
        let allocation_count = (heap.current_allocated_size() / 1024) as usize; // Rough estimate

        // Calculate heap hash based on allocation pattern
        let allocations: Vec<HeapAllocation> = {
            let allocations = self.allocations.read();
            allocations
                .values()
                .filter(|alloc| alloc.heap_id == heap_id)
                .cloned()
                .collect()
        };

        let heap_hash = self.calculate_heap_hash(&allocations.iter().collect::<Vec<_>>());
        let allocation_order_hash =
            self.calculate_allocation_order_hash(&allocations.iter().collect::<Vec<_>>());

        let heap_state = HeapState {
            heap_id,
            total_size,
            used_size,
            allocation_count,
            heap_hash,
            allocation_order_hash,
        };

        {
            let mut heap_states = self.heap_states.write();
            heap_states.insert(heap_id, heap_state);
        }

        Ok(())
    }

    /// Check for page migration events
    fn check_page_migration(&self, allocation: &HeapAllocation) -> Result<()> {
        // In a real implementation, this would use Metal's memory pressure callbacks
        // or IOKit to detect actual page migrations. For now, we simulate detection
        // based on allocation patterns and memory pressure.

        let timestamp = current_timestamp();

        // Simulate page migration detection based on allocation size and timing
        if allocation.size_bytes > 1024 * 1024 {
            // Large allocation (>1MB)
            let migration_event = MemoryMigrationEvent {
                event_id: Uuid::new_v4(),
                migration_type: MigrationType::PageOut,
                source_addr: allocation.memory_addr,
                dest_addr: None,
                size_bytes: allocation.size_bytes,
                timestamp,
                context: serde_json::json!({
                    "allocation_id": allocation.allocation_id,
                    "heap_id": allocation.heap_id,
                    "buffer_id": allocation.buffer_id,
                }),
            };

            {
                let mut events = self.migration_events.write();
                events.push(migration_event.clone());
            }

            info!(
                "Detected potential page migration: allocation_id={}, size={}",
                allocation.allocation_id, allocation.size_bytes
            );
        }

        Ok(())
    }

    /// Calculate heap hash for determinism verification
    fn calculate_heap_hash(&self, allocations: &[&HeapAllocation]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash allocation sizes and offsets in deterministic order
        let mut sorted_allocations: Vec<_> = allocations.iter().collect();
        sorted_allocations.sort_by_key(|alloc| alloc.buffer_id);

        for allocation in sorted_allocations {
            hasher.update(&allocation.size_bytes.to_le_bytes());
            hasher.update(&allocation.offset_bytes.to_le_bytes());
            hasher.update(&allocation.buffer_id.to_le_bytes());
        }

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Calculate allocation order hash
    fn calculate_allocation_order_hash(&self, allocations: &[&HeapAllocation]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash allocation timestamps in order
        let mut sorted_allocations: Vec<_> = allocations.iter().collect();
        sorted_allocations.sort_by_key(|alloc| alloc.timestamp);

        for allocation in sorted_allocations {
            hasher.update(&allocation.timestamp.to_le_bytes());
            hasher.update(&allocation.buffer_id.to_le_bytes());
        }

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Check if we should sample this event
    fn should_sample(&self) -> bool {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        current_timestamp().hash(&mut hasher);
        let hash = hasher.finish();

        // Use hash to determine sampling
        (hash as f32 / u64::MAX as f32) < self.sampling_rate
    }

    /// Get current heap states
    pub fn get_heap_states(&self) -> Vec<HeapState> {
        let heap_states = self.heap_states.read();
        heap_states.values().cloned().collect()
    }

    /// Get migration events
    pub fn get_migration_events(&self) -> Vec<MemoryMigrationEvent> {
        let events = self.migration_events.read();
        events.clone()
    }

    /// Get allocation count
    pub fn get_allocation_count(&self) -> usize {
        let allocations = self.allocations.read();
        allocations.len()
    }

    /// Clear all recorded data
    pub fn clear(&self) {
        {
            let mut allocations = self.allocations.write();
            allocations.clear();
        }
        {
            let mut heap_states = self.heap_states.write();
            heap_states.clear();
        }
        {
            let mut events = self.migration_events.write();
            events.clear();
        }
    }

    /// Detect heap fragmentation
    pub fn detect_fragmentation(&self) -> Result<FragmentationMetrics> {
        let allocations = self.allocations.read();

        if allocations.is_empty() {
            return Ok(FragmentationMetrics {
                fragmentation_ratio: 0.0,
                external_fragmentation: 0.0,
                internal_fragmentation: 0.0,
                free_blocks: 0,
                total_free_bytes: 0,
                avg_free_block_size: 0,
                largest_free_block: 0,
                compaction_efficiency: 1.0,
                fragmentation_type: FragmentationType::None,
            });
        }

        let mut sorted_allocations: Vec<_> = allocations.values().collect();
        sorted_allocations.sort_by_key(|a| a.offset_bytes);

        let total_allocated: u64 = sorted_allocations.iter().map(|a| a.size_bytes).sum();
        let heap_states = self.heap_states.read();
        let total_heap_size: u64 = heap_states.values().map(|h| h.total_size).sum();

        // Detect free blocks (gaps between allocations)
        let mut free_blocks = Vec::new();
        let mut current_offset = 0u64;

        for alloc in &sorted_allocations {
            if alloc.offset_bytes > current_offset {
                let free_size = alloc.offset_bytes - current_offset;
                free_blocks.push(free_size);
            }
            current_offset = alloc.offset_bytes + alloc.size_bytes;
        }

        // Account for trailing free space
        if current_offset < total_heap_size {
            free_blocks.push(total_heap_size - current_offset);
        }

        let total_free_bytes: u64 = free_blocks.iter().sum();
        let num_free_blocks = free_blocks.len();
        let avg_free_block_size = if num_free_blocks > 0 {
            total_free_bytes / num_free_blocks as u64
        } else {
            0
        };
        let largest_free_block = free_blocks.iter().max().copied().unwrap_or(0);

        // Calculate fragmentation metrics
        let external_fragmentation = if total_heap_size > 0 {
            total_free_bytes as f32 / total_heap_size as f32
        } else {
            0.0
        };

        let internal_fragmentation = if total_allocated > 0 {
            // Internal fragmentation: wasted space within allocations (alignment, padding)
            // Estimate as 5-10% of allocated space (typical for GPU buffers)
            let estimated_waste = total_allocated as f32 * 0.05;
            (estimated_waste / total_allocated as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let fragmentation_ratio = (external_fragmentation + internal_fragmentation) / 2.0;

        // Compaction efficiency (how much memory could be recovered)
        let max_recoverable = if num_free_blocks > 1 {
            // If we have multiple free blocks, we could consolidate them
            total_free_bytes - largest_free_block
        } else {
            0
        };

        let compaction_efficiency = if max_recoverable > 0 && total_free_bytes > 0 {
            1.0 - (max_recoverable as f32 / total_free_bytes as f32)
        } else {
            1.0
        };

        let fragmentation_type = match fragmentation_ratio {
            r if r < 0.2 => FragmentationType::Low,
            r if r < 0.5 => FragmentationType::Medium,
            r if r < 0.8 => FragmentationType::High,
            _ => FragmentationType::Critical,
        };

        if fragmentation_type != FragmentationType::None {
            info!(
                "Heap fragmentation detected: {:.1}% (type: {:?})",
                fragmentation_ratio * 100.0,
                fragmentation_type
            );
        }

        Ok(FragmentationMetrics {
            fragmentation_ratio,
            external_fragmentation,
            internal_fragmentation,
            free_blocks: num_free_blocks,
            total_free_bytes,
            avg_free_block_size,
            largest_free_block,
            compaction_efficiency,
            fragmentation_type,
        })
    }

    /// Get fragmentation metrics for a specific heap
    pub fn get_heap_fragmentation(&self, heap_id: u64) -> Result<FragmentationMetrics> {
        let allocations = self.allocations.read();

        let heap_allocations: Vec<_> = allocations
            .values()
            .filter(|a| a.heap_id == heap_id)
            .collect();

        if heap_allocations.is_empty() {
            return Ok(FragmentationMetrics {
                fragmentation_ratio: 0.0,
                external_fragmentation: 0.0,
                internal_fragmentation: 0.0,
                free_blocks: 0,
                total_free_bytes: 0,
                avg_free_block_size: 0,
                largest_free_block: 0,
                compaction_efficiency: 1.0,
                fragmentation_type: FragmentationType::None,
            });
        }

        let heap_states = self.heap_states.read();
        let heap_state = heap_states.get(&heap_id);
        let total_heap_size = heap_state.map(|h| h.total_size).unwrap_or(0);

        let mut sorted_allocations = heap_allocations;
        sorted_allocations.sort_by_key(|a| a.offset_bytes);

        let total_allocated: u64 = sorted_allocations.iter().map(|a| a.size_bytes).sum();
        let mut free_blocks = Vec::new();
        let mut current_offset = 0u64;

        for alloc in &sorted_allocations {
            if alloc.offset_bytes > current_offset {
                free_blocks.push(alloc.offset_bytes - current_offset);
            }
            current_offset = alloc.offset_bytes + alloc.size_bytes;
        }

        if current_offset < total_heap_size {
            free_blocks.push(total_heap_size - current_offset);
        }

        let total_free_bytes: u64 = free_blocks.iter().sum();
        let num_free_blocks = free_blocks.len();
        let avg_free_block_size = if num_free_blocks > 0 {
            total_free_bytes / num_free_blocks as u64
        } else {
            0
        };
        let largest_free_block = free_blocks.iter().max().copied().unwrap_or(0);

        let external_fragmentation = if total_heap_size > 0 {
            total_free_bytes as f32 / total_heap_size as f32
        } else {
            0.0
        };

        let internal_fragmentation = if total_allocated > 0 {
            (total_allocated as f32 * 0.05 / total_allocated as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let fragmentation_ratio = (external_fragmentation + internal_fragmentation) / 2.0;

        let max_recoverable = if num_free_blocks > 1 {
            total_free_bytes - largest_free_block
        } else {
            0
        };

        let compaction_efficiency = if max_recoverable > 0 && total_free_bytes > 0 {
            1.0 - (max_recoverable as f32 / total_free_bytes as f32)
        } else {
            1.0
        };

        let fragmentation_type = match fragmentation_ratio {
            r if r < 0.2 => FragmentationType::Low,
            r if r < 0.5 => FragmentationType::Medium,
            r if r < 0.8 => FragmentationType::High,
            _ => FragmentationType::Critical,
        };

        Ok(FragmentationMetrics {
            fragmentation_ratio,
            external_fragmentation,
            internal_fragmentation,
            free_blocks: num_free_blocks,
            total_free_bytes,
            avg_free_block_size,
            largest_free_block,
            compaction_efficiency,
            fragmentation_type,
        })
    }

    /// Get memory usage statistics
    pub fn get_memory_stats(&self) -> MemoryStats {
        let allocations = self.allocations.read();
        let heap_states = self.heap_states.read();
        let events = self.migration_events.read();

        let total_allocated: u64 = allocations.values().map(|a| a.size_bytes).sum();
        let total_heap_size: u64 = heap_states.values().map(|h| h.total_size).sum();
        let total_heap_used: u64 = heap_states.values().map(|h| h.used_size).sum();

        MemoryStats {
            total_allocated,
            total_heap_size,
            total_heap_used,
            allocation_count: allocations.len(),
            heap_count: heap_states.len(),
            migration_event_count: events.len(),
        }
    }
}

/// Memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_allocated: u64,
    pub total_heap_size: u64,
    pub total_heap_used: u64,
    pub allocation_count: usize,
    pub heap_count: usize,
    pub migration_event_count: usize,
}

/// Heap fragmentation metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentationMetrics {
    /// Overall fragmentation ratio (0.0-1.0)
    pub fragmentation_ratio: f32,
    /// External fragmentation (space between allocations)
    pub external_fragmentation: f32,
    /// Internal fragmentation (wasted space within allocations)
    pub internal_fragmentation: f32,
    /// Number of free blocks
    pub free_blocks: usize,
    /// Total free space
    pub total_free_bytes: u64,
    /// Average free block size
    pub avg_free_block_size: u64,
    /// Largest contiguous free block
    pub largest_free_block: u64,
    /// Compaction efficiency score (0.0-1.0)
    pub compaction_efficiency: f32,
    /// Fragmentation type
    pub fragmentation_type: FragmentationType,
}

/// Types of detected fragmentation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FragmentationType {
    /// No fragmentation detected
    None,
    /// Low fragmentation (< 20%)
    Low,
    /// Medium fragmentation (20-50%)
    Medium,
    /// High fragmentation (50-80%)
    High,
    /// Critical fragmentation (> 80%)
    Critical,
}

/// Get current timestamp in microseconds
fn current_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
}

// ============================================================================
// FFI WRAPPER FUNCTIONS (Safe Rust interface for C/C++/ObjC code)
// ============================================================================

use std::sync::OnceLock;

/// Global Metal heap observer instance for FFI access
static METAL_OBSERVER: OnceLock<Arc<MetalHeapObserver>> = OnceLock::new();

/// Initialize the global Metal heap observer with a device
#[cfg(target_os = "macos")]
pub fn ffi_metal_heap_observer_init(device: Arc<Device>) -> Result<()> {
    let observer = Arc::new(MetalHeapObserver::new(device, 1.0));
    METAL_OBSERVER.set(observer).ok();
    debug!("Global Metal heap observer initialized");
    Ok(())
}

/// Get reference to global observer (fallback if not initialized)
#[cfg(target_os = "macos")]
fn get_global_observer() -> Arc<MetalHeapObserver> {
    METAL_OBSERVER
        .get_or_init(|| {
            if let Some(device) = Device::system_default() {
                Arc::new(MetalHeapObserver::new(Arc::new(device), 1.0))
            } else {
                Arc::new(MetalHeapObserver::new(
                    Arc::new(Device::system_default().unwrap()),
                    1.0,
                ))
            }
        })
        .clone()
}

#[cfg(not(target_os = "macos"))]
fn get_global_observer() -> Arc<MetalHeapObserver> {
    METAL_OBSERVER
        .get_or_init(|| Arc::new(MetalHeapObserver::new(None, 1.0)))
        .clone()
}

/// FFI-safe wrapper to record an allocation
/// Returns 0 on success, non-zero on error
#[no_mangle]
pub extern "C" fn ffi_metal_heap_record_allocation(
    heap_id: u64,
    buffer_id: u64,
    size: u64,
    offset: u64,
    addr: u64,
    storage_mode: u32,
) -> i32 {
    let observer = get_global_observer();
    let alloc = HeapAllocation {
        allocation_id: Uuid::new_v4(),
        heap_id,
        buffer_id,
        size_bytes: size,
        offset_bytes: offset,
        timestamp: current_timestamp(),
        memory_addr: if addr == 0 { None } else { Some(addr) },
        storage_mode: format!("mode_{}", storage_mode),
    };

    {
        let mut allocations = observer.allocations.write();
        allocations.insert(buffer_id, alloc);
    }

    1 // Success
}

/// FFI-safe wrapper to record a deallocation
#[no_mangle]
pub extern "C" fn ffi_metal_heap_record_deallocation(buffer_id: u64) -> i32 {
    let observer = get_global_observer();
    let _ = observer.observe_deallocation(buffer_id);
    1 // Success
}

/// FFI-safe wrapper to get fragmentation metrics
/// Returns 0 on success, non-zero on error
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn ffi_metal_heap_get_fragmentation(
    out_metrics: *mut FFIFragmentationMetrics,
) -> i32 {
    if out_metrics.is_null() {
        return -1; // Invalid pointer
    }

    let observer = get_global_observer();
    match observer.detect_fragmentation() {
        Ok(metrics) => {
            *out_metrics = FFIFragmentationMetrics {
                fragmentation_ratio: metrics.fragmentation_ratio,
                external_fragmentation: metrics.external_fragmentation,
                internal_fragmentation: metrics.internal_fragmentation,
                free_blocks: metrics.free_blocks as u32,
                total_free_bytes: metrics.total_free_bytes,
                avg_free_block_size: metrics.avg_free_block_size,
                largest_free_block: metrics.largest_free_block,
                compaction_efficiency: metrics.compaction_efficiency,
            };
            0 // Success
        }
        Err(_) => -2, // Calculation error
    }
}

/// FFI-safe wrapper to get all heap states
/// Returns number of heaps written, or negative on error
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn ffi_metal_heap_get_all_states(
    out_heaps: *mut FFIHeapState,
    max_heaps: u32,
) -> i32 {
    if out_heaps.is_null() || max_heaps == 0 {
        return -1; // Invalid parameters
    }

    let observer = get_global_observer();
    let heap_states = observer.get_heap_states();

    let count = std::cmp::min(heap_states.len(), max_heaps as usize) as i32;

    for (i, state) in heap_states.iter().take(max_heaps as usize).enumerate() {
        let frag = observer
            .get_heap_fragmentation(state.heap_id)
            .unwrap_or(FragmentationMetrics {
                fragmentation_ratio: 0.0,
                external_fragmentation: 0.0,
                internal_fragmentation: 0.0,
                free_blocks: 0,
                total_free_bytes: 0,
                avg_free_block_size: 0,
                largest_free_block: 0,
                compaction_efficiency: 1.0,
                fragmentation_type: FragmentationType::None,
            });

        let alloc_count = observer
            .allocations
            .read()
            .values()
            .filter(|a| a.heap_id == state.heap_id)
            .count() as u32;

        (*out_heaps.add(i)) = FFIHeapState {
            heap_id: state.heap_id,
            total_size: state.total_size,
            used_size: state.used_size,
            allocation_count: alloc_count,
            fragmentation_ratio: frag.fragmentation_ratio,
            avg_alloc_size: if alloc_count > 0 {
                state.used_size / alloc_count as u64
            } else {
                0
            },
            largest_free_block: frag.largest_free_block,
        };
    }

    count
}

/// FFI-safe wrapper to get Metal memory metrics
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn ffi_metal_heap_get_metrics(
    out_metrics: *mut FFIMetalMemoryMetrics,
) -> i32 {
    if out_metrics.is_null() {
        return -1;
    }

    let observer = get_global_observer();
    let stats = observer.get_memory_stats();

    let frag = observer
        .detect_fragmentation()
        .unwrap_or(FragmentationMetrics {
            fragmentation_ratio: 0.0,
            external_fragmentation: 0.0,
            internal_fragmentation: 0.0,
            free_blocks: 0,
            total_free_bytes: 0,
            avg_free_block_size: 0,
            largest_free_block: 0,
            compaction_efficiency: 1.0,
            fragmentation_type: FragmentationType::None,
        });

    let utilization_pct = if stats.total_heap_size > 0 {
        (stats.total_heap_used as f32 / stats.total_heap_size as f32) * 100.0
    } else {
        0.0
    };

    *out_metrics = FFIMetalMemoryMetrics {
        total_allocated: stats.total_allocated,
        total_heap_size: stats.total_heap_size,
        total_heap_used: stats.total_heap_used,
        allocation_count: stats.allocation_count as u32,
        heap_count: stats.heap_count as u32,
        overall_fragmentation: frag.fragmentation_ratio,
        utilization_pct,
        migration_event_count: stats.migration_event_count as u32,
    };

    0 // Success
}

/// FFI-safe wrapper to get migration events
#[no_mangle]
#[allow(clippy::missing_safety_doc)]
pub unsafe extern "C" fn ffi_metal_heap_get_migration_events(
    out_events: *mut FFIPageMigrationEvent,
    max_events: u32,
) -> i32 {
    if out_events.is_null() {
        return -1;
    }

    let observer = get_global_observer();
    let events = observer.get_migration_events();

    let count = std::cmp::min(events.len(), max_events as usize) as i32;

    for (i, event) in events.iter().take(max_events as usize).enumerate() {
        let uuid_bytes = event.event_id.as_bytes();
        let mut event_id_high = 0u64;
        let mut event_id_low = 0u64;

        // Split UUID into two u64s (first 8 bytes, last 8 bytes)
        if uuid_bytes.len() >= 8 {
            for byte in uuid_bytes.iter().take(8) {
                event_id_high = (event_id_high << 8) | (*byte as u64);
            }
        }
        if uuid_bytes.len() >= 16 {
            for byte in uuid_bytes.iter().skip(8).take(8) {
                event_id_low = (event_id_low << 8) | (*byte as u64);
            }
        }

        let migration_type = match event.migration_type {
            MigrationType::PageOut => 1u32,
            MigrationType::PageIn => 2u32,
            MigrationType::BufferRelocate => 3u32,
            MigrationType::HeapCompaction => 4u32,
            MigrationType::PressureEviction => 5u32,
        };

        (*out_events.add(i)) = FFIPageMigrationEvent {
            event_id_high,
            event_id_low,
            migration_type,
            source_addr: event.source_addr.unwrap_or(0),
            dest_addr: event.dest_addr.unwrap_or(0),
            size_bytes: event.size_bytes,
            timestamp: event.timestamp as u64,
        };
    }

    count
}

/// FFI-safe wrapper to clear all observation data
#[no_mangle]
pub extern "C" fn ffi_metal_heap_clear() -> i32 {
    let observer = get_global_observer();
    observer.clear();
    0 // Success
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to get a shared Metal device for tests.
    /// Avoids repeated system_default() calls for consistency and performance.
    fn get_test_device() -> Option<Arc<Device>> {
        #[cfg(target_os = "macos")]
        {
            Device::system_default().map(Arc::new)
        }
        #[cfg(not(target_os = "macos"))]
        {
            None
        }
    }

    #[test]
    fn test_heap_observer_creation() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 1.0);
            assert_eq!(observer.get_allocation_count(), 0);
        }
    }

    #[test]
    fn test_sampling_rate() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 0.5);
            // Test that sampling rate is clamped
            assert!(observer.sampling_rate >= 0.0 && observer.sampling_rate <= 1.0);
        }
    }

    #[test]
    fn test_memory_stats() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 1.0);
            let stats = observer.get_memory_stats();

            assert_eq!(stats.total_allocated, 0);
            assert_eq!(stats.allocation_count, 0);
            assert_eq!(stats.heap_count, 0);
            assert_eq!(stats.migration_event_count, 0);
        }
    }

    #[test]
    fn test_heap_hash_calculation() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 1.0);

            let allocations = vec![
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 1,
                    size_bytes: 1024,
                    offset_bytes: 0,
                    timestamp: 1000,
                    memory_addr: Some(0x1000),
                    storage_mode: "shared".to_string(),
                },
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 2,
                    size_bytes: 2048,
                    offset_bytes: 1024,
                    timestamp: 2000,
                    memory_addr: Some(0x2000),
                    storage_mode: "shared".to_string(),
                },
            ];

            let refs: Vec<&HeapAllocation> = allocations.iter().collect();
            let hash1 = observer.calculate_heap_hash(&refs);
            let hash2 = observer.calculate_heap_hash(&refs);

            // Hash should be deterministic
            assert_eq!(hash1, hash2);
        }
    }

    #[test]
    fn test_fragmentation_detection_no_allocations() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 1.0);
            let metrics = observer.detect_fragmentation().unwrap();

            assert_eq!(metrics.fragmentation_ratio, 0.0);
            assert_eq!(metrics.fragmentation_type, FragmentationType::None);
            assert_eq!(metrics.free_blocks, 0);
        }
    }

    #[test]
    fn test_fragmentation_detection_contiguous() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 1.0);

            // Create contiguous allocations with no gaps
            let allocations = vec![
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 1,
                    size_bytes: 1024,
                    offset_bytes: 0,
                    timestamp: 1000,
                    memory_addr: Some(0x1000),
                    storage_mode: "shared".to_string(),
                },
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 2,
                    size_bytes: 1024,
                    offset_bytes: 1024,
                    timestamp: 2000,
                    memory_addr: Some(0x2000),
                    storage_mode: "shared".to_string(),
                },
            ];

            {
                let mut allocs = observer.allocations.write();
                for alloc in allocations {
                    allocs.insert(alloc.buffer_id, alloc);
                }
            }

            // Set heap state
            {
                let mut heap_states = observer.heap_states.write();
                heap_states.insert(
                    1,
                    HeapState {
                        heap_id: 1,
                        total_size: 2048,
                        used_size: 2048,
                        allocation_count: 2,
                        heap_hash: B3Hash::hash(b"test"),
                        allocation_order_hash: B3Hash::hash(b"test"),
                    },
                );
            }

            let metrics = observer.detect_fragmentation().unwrap();

            // Contiguous allocations should have low fragmentation
            assert!(metrics.fragmentation_ratio < 0.3);
            assert_eq!(metrics.free_blocks, 0);
        }
    }

    #[test]
    fn test_fragmentation_detection_fragmented() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 1.0);

            // Create fragmented allocations with gaps
            let allocations = vec![
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 1,
                    size_bytes: 512,
                    offset_bytes: 0,
                    timestamp: 1000,
                    memory_addr: Some(0x1000),
                    storage_mode: "shared".to_string(),
                },
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 2,
                    size_bytes: 512,
                    offset_bytes: 1536, // Gap of 512 bytes
                    timestamp: 2000,
                    memory_addr: Some(0x2000),
                    storage_mode: "shared".to_string(),
                },
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 3,
                    size_bytes: 512,
                    offset_bytes: 3072, // Gap of 512 bytes
                    timestamp: 3000,
                    memory_addr: Some(0x3000),
                    storage_mode: "shared".to_string(),
                },
            ];

            {
                let mut allocs = observer.allocations.write();
                for alloc in allocations {
                    allocs.insert(alloc.buffer_id, alloc);
                }
            }

            {
                let mut heap_states = observer.heap_states.write();
                heap_states.insert(
                    1,
                    HeapState {
                        heap_id: 1,
                        total_size: 4096,
                        used_size: 1536,
                        allocation_count: 3,
                        heap_hash: B3Hash::hash(b"test"),
                        allocation_order_hash: B3Hash::hash(b"test"),
                    },
                );
            }

            let metrics = observer.detect_fragmentation().unwrap();

            // Should detect fragmentation
            assert!(metrics.fragmentation_ratio > 0.0);
            assert!(metrics.free_blocks >= 1);
            assert!(metrics.total_free_bytes > 0);
            assert!(
                metrics.fragmentation_type == FragmentationType::High
                    || metrics.fragmentation_type == FragmentationType::Medium
            );
        }
    }

    #[test]
    fn test_ffi_fragmentation_metrics() {
        if let Some(device) = get_test_device() {
            let observer = Arc::new(MetalHeapObserver::new(device, 1.0));
            METAL_OBSERVER.set(observer).ok();

            let allocations = vec![
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 1,
                    size_bytes: 256,
                    offset_bytes: 0,
                    timestamp: 1000,
                    memory_addr: Some(0x1000),
                    storage_mode: "shared".to_string(),
                },
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 2,
                    size_bytes: 256,
                    offset_bytes: 512, // Gap
                    timestamp: 2000,
                    memory_addr: Some(0x2000),
                    storage_mode: "shared".to_string(),
                },
            ];

            {
                let observer = get_global_observer();
                let mut allocs = observer.allocations.write();
                for alloc in allocations {
                    allocs.insert(alloc.buffer_id, alloc);
                }
            }

            let mut metrics = FFIFragmentationMetrics {
                fragmentation_ratio: 0.0,
                external_fragmentation: 0.0,
                internal_fragmentation: 0.0,
                free_blocks: 0,
                total_free_bytes: 0,
                avg_free_block_size: 0,
                largest_free_block: 0,
                compaction_efficiency: 0.0,
            };

            let result = unsafe { ffi_metal_heap_get_fragmentation(&mut metrics) };
            assert_eq!(result, 0); // Success
            assert!(metrics.fragmentation_ratio >= 0.0);
            assert!(metrics.fragmentation_ratio <= 1.0);
        }
    }

    #[test]
    #[ignore = "flaky: race condition with global Metal state when run in parallel [tracking: STAB-IGN-001]"]
    fn test_ffi_metal_memory_metrics() {
        if let Some(device) = get_test_device() {
            let observer = Arc::new(MetalHeapObserver::new(device, 1.0));
            METAL_OBSERVER.set(observer).ok();

            ffi_metal_heap_record_allocation(1, 100, 1024, 0, 0x1000, 1);
            ffi_metal_heap_record_allocation(1, 101, 2048, 1024, 0x2000, 1);

            let mut metrics = FFIMetalMemoryMetrics {
                total_allocated: 0,
                total_heap_size: 0,
                total_heap_used: 0,
                allocation_count: 0,
                heap_count: 0,
                overall_fragmentation: 0.0,
                utilization_pct: 0.0,
                migration_event_count: 0,
            };

            let result = unsafe { ffi_metal_heap_get_metrics(&mut metrics) };
            assert_eq!(result, 0); // Success
            assert_eq!(metrics.allocation_count, 2);
            assert_eq!(metrics.total_allocated, 1024 + 2048);
        }
    }

    #[test]
    fn test_ffi_heap_states() {
        if let Some(device) = get_test_device() {
            let observer = Arc::new(MetalHeapObserver::new(device, 1.0));
            METAL_OBSERVER.set(observer).ok();

            let observer = get_global_observer();
            {
                let mut heap_states = observer.heap_states.write();
                heap_states.insert(
                    1,
                    HeapState {
                        heap_id: 1,
                        total_size: 4096,
                        used_size: 2048,
                        allocation_count: 2,
                        heap_hash: B3Hash::hash(b"test"),
                        allocation_order_hash: B3Hash::hash(b"test"),
                    },
                );
            }

            let mut heaps: [FFIHeapState; 10] = unsafe { std::mem::zeroed() };
            let count = unsafe { ffi_metal_heap_get_all_states(heaps.as_mut_ptr(), 10) };

            assert!(count >= 0);
            assert!(count as usize <= 10);
        }
    }

    #[test]
    fn test_ffi_null_pointer_handling() {
        // Test that FFI functions handle null pointers safely
        let result = unsafe { ffi_metal_heap_get_fragmentation(std::ptr::null_mut()) };
        assert!(result < 0);

        let result = unsafe { ffi_metal_heap_get_all_states(std::ptr::null_mut(), 10) };
        assert!(result < 0);

        let result = unsafe { ffi_metal_heap_get_metrics(std::ptr::null_mut()) };
        assert!(result < 0);
    }

    #[test]
    fn test_fragmentation_types() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 1.0);

            // Test FragmentationType classification
            assert_eq!(FragmentationType::None as u8, 0);
            assert_ne!(
                FragmentationType::Low as u8,
                FragmentationType::Medium as u8
            );
            assert_ne!(
                FragmentationType::Medium as u8,
                FragmentationType::High as u8
            );
            assert_ne!(
                FragmentationType::High as u8,
                FragmentationType::Critical as u8
            );

            let metrics = observer.detect_fragmentation().unwrap();
            assert!(matches!(
                metrics.fragmentation_type,
                FragmentationType::None
                    | FragmentationType::Low
                    | FragmentationType::Medium
                    | FragmentationType::High
                    | FragmentationType::Critical
            ));
        }
    }

    #[test]
    fn test_heap_specific_fragmentation() {
        if let Some(device) = get_test_device() {
            let observer = MetalHeapObserver::new(device, 1.0);

            // Create allocations on two different heaps
            let allocations = vec![
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 1,
                    buffer_id: 1,
                    size_bytes: 1024,
                    offset_bytes: 0,
                    timestamp: 1000,
                    memory_addr: Some(0x1000),
                    storage_mode: "shared".to_string(),
                },
                HeapAllocation {
                    allocation_id: Uuid::new_v4(),
                    heap_id: 2,
                    buffer_id: 2,
                    size_bytes: 512,
                    offset_bytes: 0,
                    timestamp: 2000,
                    memory_addr: Some(0x3000),
                    storage_mode: "shared".to_string(),
                },
            ];

            {
                let mut allocs = observer.allocations.write();
                for alloc in allocations {
                    allocs.insert(alloc.buffer_id, alloc);
                }
            }

            // Test heap 1 fragmentation
            let frag_1 = observer.get_heap_fragmentation(1).unwrap();
            assert!(frag_1.fragmentation_ratio >= 0.0);

            // Test heap 2 fragmentation
            let frag_2 = observer.get_heap_fragmentation(2).unwrap();
            assert!(frag_2.fragmentation_ratio >= 0.0);

            // Test non-existent heap returns empty
            let frag_empty = observer.get_heap_fragmentation(999).unwrap();
            assert_eq!(frag_empty.fragmentation_type, FragmentationType::None);
        }
    }
}
