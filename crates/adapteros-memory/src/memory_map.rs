//! Memory map hashing for determinism verification
//!
//! Provides comprehensive memory layout hashing to ensure identical memory
//! layouts across runs. This is critical for determinism verification as
//! different memory layouts can lead to different execution paths.

use crate::{MemoryLayoutHash, MemoryWatchdogError, Result};
use adapteros_core::B3Hash;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

#[cfg(target_os = "macos")]
use metal::{foreign_types::ForeignType, Buffer, Device, Heap};

/// Memory region information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRegion {
    /// Region identifier
    pub region_id: Uuid,
    /// Region type
    pub region_type: MemoryRegionType,
    /// Base address
    pub base_addr: u64,
    /// Region size
    pub size_bytes: u64,
    /// Allocation order
    pub allocation_order: u64,
    /// Region hash
    pub region_hash: B3Hash,
    /// Additional metadata
    pub metadata: serde_json::Value,
}

/// Memory region types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MemoryRegionType {
    /// Metal heap
    MetalHeap,
    /// Metal buffer
    MetalBuffer,
    /// System memory
    SystemMemory,
    /// GPU memory
    GpuMemory,
    /// Unified memory
    UnifiedMemory,
}

/// Memory map snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMapSnapshot {
    /// Snapshot ID
    pub snapshot_id: Uuid,
    /// Snapshot timestamp
    pub timestamp: u128,
    /// Memory regions
    pub regions: Vec<MemoryRegion>,
    /// Complete memory map hash
    pub memory_map_hash: B3Hash,
    /// Region count
    pub region_count: usize,
    /// Total memory size
    pub total_memory_size: u64,
}

/// Memory map hasher
pub struct MemoryMapHasher {
    /// Device reference
    #[cfg(target_os = "macos")]
    #[allow(dead_code)]
    device: Arc<Device>,
    /// Memory regions by ID
    memory_regions: Arc<RwLock<HashMap<Uuid, MemoryRegion>>>,
    /// Memory map snapshots
    snapshots: Arc<RwLock<Vec<MemoryMapSnapshot>>>,
    /// Next allocation order
    allocation_order_counter: Arc<std::sync::atomic::AtomicU64>,
    /// Hashing enabled
    hashing_enabled: bool,
}

impl MemoryMapHasher {
    /// Create a new memory map hasher
    #[cfg(target_os = "macos")]
    pub fn new(device: Arc<Device>, hashing_enabled: bool) -> Self {
        Self {
            device,
            memory_regions: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(Vec::new())),
            allocation_order_counter: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            hashing_enabled,
        }
    }

    /// Create a new memory map hasher (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn new(_device: Option<()>, hashing_enabled: bool) -> Self {
        Self {
            memory_regions: Arc::new(RwLock::new(HashMap::new())),
            snapshots: Arc::new(RwLock::new(Vec::new())),
            allocation_order_counter: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            hashing_enabled,
        }
    }

    /// Add a memory region to the map
    pub fn add_region(
        &self,
        region_type: MemoryRegionType,
        base_addr: u64,
        size_bytes: u64,
        metadata: serde_json::Value,
    ) -> Result<Uuid> {
        if !self.hashing_enabled {
            return Ok(Uuid::new_v4()); // Return dummy ID
        }

        let region_id = Uuid::new_v4();
        let allocation_order = self
            .allocation_order_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _timestamp = current_timestamp();

        let region_hash = self.calculate_region_hash(base_addr, size_bytes, &metadata);

        let region = MemoryRegion {
            region_id,
            region_type: region_type.clone(),
            base_addr,
            size_bytes,
            allocation_order,
            region_hash,
            metadata,
        };

        {
            let mut regions = self.memory_regions.write();
            regions.insert(region_id, region);
        }

        debug!(
            "Added memory region: id={}, type={:?}, addr=0x{:x}, size={}",
            region_id,
            region_type.clone(),
            base_addr,
            size_bytes
        );

        Ok(region_id)
    }

    /// Add a Metal heap to the map
    #[cfg(target_os = "macos")]
    pub fn add_metal_heap(&self, heap: &Heap) -> Result<Uuid> {
        if !self.hashing_enabled {
            return Ok(Uuid::new_v4());
        }

        let base_addr = heap.as_ptr() as u64;
        let size_bytes = heap.size();
        let metadata = serde_json::json!({
            "heap_type": "Metal",
            "used_size": heap.used_size(),
            "current_allocated_size": heap.current_allocated_size(),
        });

        self.add_region(MemoryRegionType::MetalHeap, base_addr, size_bytes, metadata)
    }

    /// Add a Metal heap to the map (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn add_metal_heap(&self, _heap: Option<()>) -> Result<Uuid> {
        if !self.hashing_enabled {
            return Ok(Uuid::new_v4());
        }

        // Simulate heap on non-macOS platforms
        let base_addr = 0x1000000;
        let size_bytes = 1024 * 1024 * 1024; // 1GB
        let metadata = serde_json::json!({
            "heap_type": "Simulated",
            "used_size": 0,
            "current_allocated_size": 0,
        });

        self.add_region(MemoryRegionType::MetalHeap, base_addr, size_bytes, metadata)
    }

    /// Add a Metal buffer to the map
    #[cfg(target_os = "macos")]
    pub fn add_metal_buffer(&self, buffer: &Buffer) -> Result<Uuid> {
        if !self.hashing_enabled {
            return Ok(Uuid::new_v4());
        }

        let base_addr = buffer.as_ptr() as u64;
        let size_bytes = buffer.length();
        let metadata = serde_json::json!({
            "buffer_type": "Metal",
            "resource_options": format!("{:?}", buffer.resource_options()),
            "storage_mode": format!("{:?}", buffer.storage_mode()),
        });

        self.add_region(
            MemoryRegionType::MetalBuffer,
            base_addr,
            size_bytes,
            metadata,
        )
    }

    /// Add a Metal buffer to the map (non-macOS)
    #[cfg(not(target_os = "macos"))]
    pub fn add_metal_buffer(&self, _buffer: Option<()>) -> Result<Uuid> {
        if !self.hashing_enabled {
            return Ok(Uuid::new_v4());
        }

        // Simulate buffer on non-macOS platforms
        let base_addr = 0x2000000;
        let size_bytes = 1024 * 1024; // 1MB
        let metadata = serde_json::json!({
            "buffer_type": "Simulated",
            "resource_options": "default",
            "storage_mode": "shared",
        });

        self.add_region(
            MemoryRegionType::MetalBuffer,
            base_addr,
            size_bytes,
            metadata,
        )
    }

    /// Remove a memory region from the map
    pub fn remove_region(&self, region_id: Uuid) -> Result<()> {
        if !self.hashing_enabled {
            return Ok(());
        }

        {
            let mut regions = self.memory_regions.write();
            regions.remove(&region_id);
        }

        debug!("Removed memory region: id={}", region_id);
        Ok(())
    }

    /// Generate a memory map snapshot
    pub fn generate_snapshot(&self) -> Result<MemoryMapSnapshot> {
        if !self.hashing_enabled {
            return Ok(MemoryMapSnapshot {
                snapshot_id: Uuid::new_v4(),
                timestamp: current_timestamp(),
                regions: Vec::new(),
                memory_map_hash: B3Hash::hash(b"empty"),
                region_count: 0,
                total_memory_size: 0,
            });
        }

        let snapshot_id = Uuid::new_v4();
        let timestamp = current_timestamp();

        let regions: Vec<MemoryRegion> = {
            let regions = self.memory_regions.read();
            regions.values().cloned().collect()
        };

        let region_count = regions.len();
        let total_memory_size: u64 = regions.iter().map(|r| r.size_bytes).sum();

        // Calculate complete memory map hash
        let memory_map_hash = self.calculate_memory_map_hash(&regions);

        let snapshot = MemoryMapSnapshot {
            snapshot_id,
            timestamp,
            regions,
            memory_map_hash,
            region_count,
            total_memory_size,
        };

        // Store snapshot
        {
            let mut snapshots = self.snapshots.write();
            snapshots.push(snapshot.clone());
        }

        info!(
            "Generated memory map snapshot: id={}, regions={}, total_size={}",
            snapshot_id, region_count, total_memory_size
        );

        Ok(snapshot)
    }

    /// Calculate region hash
    fn calculate_region_hash(
        &self,
        base_addr: u64,
        size_bytes: u64,
        metadata: &serde_json::Value,
    ) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        hasher.update(&base_addr.to_le_bytes());
        hasher.update(&size_bytes.to_le_bytes());

        // Hash metadata deterministically
        if let Ok(metadata_bytes) = serde_json::to_vec(metadata) {
            hasher.update(&metadata_bytes);
        }

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Calculate complete memory map hash
    fn calculate_memory_map_hash(&self, regions: &[MemoryRegion]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Sort regions by allocation order for deterministic hashing
        let mut sorted_regions: Vec<_> = regions.iter().collect();
        sorted_regions.sort_by_key(|r| r.allocation_order);

        for region in sorted_regions {
            hasher.update(&region.allocation_order.to_le_bytes());
            hasher.update(&region.base_addr.to_le_bytes());
            hasher.update(&region.size_bytes.to_le_bytes());
            hasher.update(region.region_hash.as_bytes());
        }

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Generate memory layout hash for determinism verification
    pub fn generate_memory_layout_hash(&self) -> Result<MemoryLayoutHash> {
        if !self.hashing_enabled {
            return Ok(MemoryLayoutHash {
                layout_hash: B3Hash::hash(b"disabled"),
                pointer_pattern_hash: B3Hash::hash(b"disabled"),
                allocation_order_hash: B3Hash::hash(b"disabled"),
                timestamp: current_timestamp(),
            });
        }

        let snapshot = self.generate_snapshot()?;
        let timestamp = current_timestamp();

        // Calculate pointer pattern hash
        let pointer_pattern_hash = self.calculate_pointer_pattern_hash(&snapshot.regions);

        // Calculate allocation order hash
        let allocation_order_hash = self.calculate_allocation_order_hash(&snapshot.regions);

        Ok(MemoryLayoutHash {
            layout_hash: snapshot.memory_map_hash,
            pointer_pattern_hash,
            allocation_order_hash,
            timestamp,
        })
    }

    /// Calculate pointer pattern hash
    fn calculate_pointer_pattern_hash(&self, regions: &[MemoryRegion]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash pointer patterns (base addresses) in deterministic order
        let mut sorted_regions: Vec<_> = regions.iter().collect();
        sorted_regions.sort_by_key(|r| r.allocation_order);

        for region in sorted_regions {
            hasher.update(&region.base_addr.to_le_bytes());
        }

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Calculate allocation order hash
    fn calculate_allocation_order_hash(&self, regions: &[MemoryRegion]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash allocation order
        let mut sorted_regions: Vec<_> = regions.iter().collect();
        sorted_regions.sort_by_key(|r| r.allocation_order);

        for region in sorted_regions {
            hasher.update(&region.allocation_order.to_le_bytes());
            hasher.update(format!("{:?}", region.region_type).as_bytes());
        }

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Verify memory layout consistency
    pub fn verify_layout_consistency(&self, expected_hash: &MemoryLayoutHash) -> Result<()> {
        if !self.hashing_enabled {
            return Ok(()); // Skip verification when disabled
        }

        let current_hash = self.generate_memory_layout_hash()?;

        if current_hash.layout_hash != expected_hash.layout_hash {
            return Err(MemoryWatchdogError::MemoryLayoutMismatch {
                expected: format!("{:?}", expected_hash.layout_hash),
                actual: format!("{:?}", current_hash.layout_hash),
            });
        }

        if current_hash.pointer_pattern_hash != expected_hash.pointer_pattern_hash {
            return Err(MemoryWatchdogError::MemoryLayoutMismatch {
                expected: format!("{:?}", expected_hash.pointer_pattern_hash),
                actual: format!("{:?}", current_hash.pointer_pattern_hash),
            });
        }

        if current_hash.allocation_order_hash != expected_hash.allocation_order_hash {
            return Err(MemoryWatchdogError::MemoryLayoutMismatch {
                expected: format!("{:?}", expected_hash.allocation_order_hash),
                actual: format!("{:?}", current_hash.allocation_order_hash),
            });
        }

        Ok(())
    }

    /// Get memory map statistics
    pub fn get_memory_stats(&self) -> MemoryMapStats {
        let regions = self.memory_regions.read();
        let snapshots = self.snapshots.read();

        let total_regions = regions.len();
        let total_size: u64 = regions.values().map(|r| r.size_bytes).sum();
        let region_types: HashMap<String, usize> = regions
            .values()
            .map(|r| (format!("{:?}", r.region_type), 1))
            .fold(HashMap::new(), |mut acc, (k, v)| {
                *acc.entry(k).or_insert(0) += v;
                acc
            });

        MemoryMapStats {
            total_regions,
            total_size,
            region_types,
            snapshot_count: snapshots.len(),
        }
    }

    /// Get memory regions
    pub fn get_memory_regions(&self) -> Vec<MemoryRegion> {
        let regions = self.memory_regions.read();
        regions.values().cloned().collect()
    }

    /// Get snapshots
    pub fn get_snapshots(&self) -> Vec<MemoryMapSnapshot> {
        let snapshots = self.snapshots.read();
        snapshots.clone()
    }

    /// Clear all recorded data
    pub fn clear(&self) {
        {
            let mut regions = self.memory_regions.write();
            regions.clear();
        }
        {
            let mut snapshots = self.snapshots.write();
            snapshots.clear();
        }

        self.allocation_order_counter
            .store(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Enable or disable hashing
    pub fn set_hashing_enabled(&mut self, enabled: bool) {
        self.hashing_enabled = enabled;
        info!(
            "Memory map hashing {}",
            if enabled { "enabled" } else { "disabled" }
        );
    }

    /// Check if hashing is enabled
    pub fn is_hashing_enabled(&self) -> bool {
        self.hashing_enabled
    }
}

/// Memory map statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMapStats {
    pub total_regions: usize,
    pub total_size: u64,
    pub region_types: HashMap<String, usize>,
    pub snapshot_count: usize,
}

/// Get current timestamp in microseconds
fn current_timestamp() -> u128 {
    adapteros_core::time::unix_timestamp_micros_u128()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: Create device for testing (reduces duplication)
    #[allow(unused)]
    fn create_test_device() -> Arc<metal::Device> {
        #[cfg(target_os = "macos")]
        {
            Arc::new(metal::Device::system_default().unwrap())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Arc::new(metal::Device::system_default().unwrap_or_else(|| {
                // Create a mock device for testing
                unsafe { std::mem::transmute(0x1usize) }
            }))
        }
    }

    #[test]
    fn test_memory_map_hasher_creation() {
        #[cfg(target_os = "macos")]
        {
            if let Some(device) = Device::system_default() {
                let hasher = MemoryMapHasher::new(Arc::new(device), true);
                assert!(hasher.is_hashing_enabled());

                let stats = hasher.get_memory_stats();
                assert_eq!(stats.total_regions, 0);
                assert_eq!(stats.total_size, 0);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            #[cfg(target_os = "macos")]
            let device = Arc::new(metal::Device::system_default().unwrap());
            #[cfg(not(target_os = "macos"))]
            let device = Arc::new(metal::Device::system_default().unwrap_or_else(|| {
                // Create a mock device for testing
                unsafe { std::mem::transmute(0x1usize) }
            }));
            let hasher = MemoryMapHasher::new(device, true);
            assert!(hasher.is_hashing_enabled());
        }
    }

    #[test]
    fn test_memory_region_addition() {
        let hasher = MemoryMapHasher::new(create_test_device(), true);

        let metadata = serde_json::json!({
            "test": "data",
            "size": 1024,
        });

        let region_id = hasher
            .add_region(MemoryRegionType::SystemMemory, 0x1000, 1024, metadata)
            .unwrap();

        assert!(!region_id.is_nil());

        let stats = hasher.get_memory_stats();
        assert_eq!(stats.total_regions, 1);
        assert_eq!(stats.total_size, 1024);
    }

    #[test]
    fn test_memory_map_snapshot() {
        let hasher = MemoryMapHasher::new(create_test_device(), true);

        // Add some regions
        let _region_id1 = hasher
            .add_region(
                MemoryRegionType::SystemMemory,
                0x1000,
                1024,
                serde_json::json!({"type": "system"}),
            )
            .unwrap();

        let _region_id2 = hasher
            .add_region(
                MemoryRegionType::GpuMemory,
                0x2000,
                2048,
                serde_json::json!({"type": "gpu"}),
            )
            .unwrap();

        let snapshot = hasher.generate_snapshot().unwrap();

        assert_eq!(snapshot.region_count, 2);
        assert_eq!(snapshot.total_memory_size, 3072);
        assert!(!snapshot.snapshot_id.is_nil());
    }

    #[test]
    fn test_memory_layout_hash_generation() {
        let hasher = MemoryMapHasher::new(create_test_device(), true);

        // Add a region
        let _region_id = hasher
            .add_region(
                MemoryRegionType::SystemMemory,
                0x1000,
                1024,
                serde_json::json!({"test": "data"}),
            )
            .unwrap();

        let layout_hash = hasher.generate_memory_layout_hash().unwrap();

        assert_ne!(
            layout_hash.layout_hash,
            adapteros_core::B3Hash::new([0u8; 32])
        );
        assert_ne!(
            layout_hash.pointer_pattern_hash,
            adapteros_core::B3Hash::new([0u8; 32])
        );
        assert_ne!(
            layout_hash.allocation_order_hash,
            adapteros_core::B3Hash::new([0u8; 32])
        );
    }

    #[test]
    fn test_layout_consistency_verification() {
        let hasher = MemoryMapHasher::new(create_test_device(), true);

        // Generate initial layout hash
        let initial_hash = hasher.generate_memory_layout_hash().unwrap();

        // Verify consistency
        hasher.verify_layout_consistency(&initial_hash).unwrap();

        // Add region and verify inconsistency
        let _region_id = hasher
            .add_region(
                MemoryRegionType::SystemMemory,
                0x1000,
                1024,
                serde_json::json!({"test": "data"}),
            )
            .unwrap();

        let result = hasher.verify_layout_consistency(&initial_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_hashing_enable_disable() {
        let mut hasher = MemoryMapHasher::new(create_test_device(), true);
        assert!(hasher.is_hashing_enabled());

        hasher.set_hashing_enabled(false);
        assert!(!hasher.is_hashing_enabled());

        hasher.set_hashing_enabled(true);
        assert!(hasher.is_hashing_enabled());
    }

    #[test]
    fn test_region_removal() {
        let hasher = MemoryMapHasher::new(create_test_device(), true);

        let region_id = hasher
            .add_region(
                MemoryRegionType::SystemMemory,
                0x1000,
                1024,
                serde_json::json!({"test": "data"}),
            )
            .unwrap();

        let stats_before = hasher.get_memory_stats();
        assert_eq!(stats_before.total_regions, 1);

        hasher.remove_region(region_id).unwrap();

        let stats_after = hasher.get_memory_stats();
        assert_eq!(stats_after.total_regions, 0);
    }
}
