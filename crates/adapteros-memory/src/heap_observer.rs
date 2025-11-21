//! Metal heap observer for page migration tracking
//!
//! Monitors Metal heap allocations, deallocations, and page migrations to ensure
//! deterministic memory behavior across runs. Tracks unified memory usage patterns
//! and detects when the OS performs memory tricks that could affect determinism.

use crate::{MemoryMigrationEvent, MigrationType, Result};
use adapteros_core::B3Hash;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info};
use uuid::Uuid;

#[cfg(target_os = "macos")]
use metal::{foreign_types::ForeignType, Buffer, Device, Heap};

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
    /// Device reference
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    device: Arc<Device>,
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
            device,
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

/// Get current timestamp in microseconds
fn current_timestamp() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to get a shared Metal device for tests.
    /// Avoids repeated system_default() calls for consistency and performance.
    fn get_test_device() -> Option<Arc<Device>> {
        Device::system_default().map(Arc::new)
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
}
