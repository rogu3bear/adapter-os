//! KV cache management with Metal buffer support
//!
//! Provides production-ready KV cache with Metal-backed buffers for efficient
//! GPU memory management. Supports per-sequence allocation, buffer zeroization,
//! and OOM handling.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// Unique identifier for a sequence
pub type SequenceId = u64;

/// Trait for zeroizable buffers
pub trait ZeroizableBuffer {
    fn zeroize(&mut self);
}

/// Allocation record for a KV cache slice
#[derive(Debug, Clone)]
struct KvAllocation {
    _sequence_id: SequenceId,
    /// Offset in bytes for K buffer
    k_offset: u64,
    /// Size in bytes for K buffer
    k_size: u64,
    /// Offset in bytes for V buffer
    v_offset: u64,
    /// Size in bytes for V buffer
    v_size: u64,
}

/// Metal-backed KV cache with slab allocator
///
/// Layout: [seq_0_k | seq_0_v | seq_1_k | seq_1_v | ...]
///
/// Each sequence gets a contiguous slice for K and V states.
/// Buffers use MTLResourceOptions::StorageModeShared for CPU/GPU access.
pub struct KvCache {
    /// Metal device reference (optional for testing)
    #[cfg(target_os = "macos")]
    _device: Option<Arc<metal::Device>>,
    /// Metal buffer for K states
    #[cfg(target_os = "macos")]
    k_buffer: Option<metal::Buffer>,
    /// Metal buffer for V states
    #[cfg(target_os = "macos")]
    v_buffer: Option<metal::Buffer>,
    /// Total capacity in bytes
    capacity_bytes: u64,
    /// Used bytes
    used_bytes: u64,
    /// Active allocations by sequence ID
    allocations: HashMap<SequenceId, KvAllocation>,
    /// Next sequence ID
    next_seq_id: SequenceId,
    /// Bytes per layer per token
    bytes_per_token: u64,
    /// Free regions (offset, size) available for reuse
    free_regions: Vec<(u64, u64)>,
}

impl KvCache {
    /// Create new KV cache without Metal (for testing/non-Mac platforms)
    pub fn new(capacity_bytes: u64) -> Self {
        Self {
            #[cfg(target_os = "macos")]
            _device: None,
            #[cfg(target_os = "macos")]
            k_buffer: None,
            #[cfg(target_os = "macos")]
            v_buffer: None,
            capacity_bytes,
            used_bytes: 0,
            allocations: HashMap::new(),
            next_seq_id: 1,
            bytes_per_token: 8192, // Default: 32 layers * 128 heads * 2 bytes (fp16)
            free_regions: Vec::new(),
        }
    }

    /// Create new KV cache with Metal device
    #[cfg(target_os = "macos")]
    pub fn new_with_metal(
        device: Arc<metal::Device>,
        capacity_bytes: u64,
        bytes_per_token: u64,
    ) -> Result<Self> {
        use metal::MTLResourceOptions;

        // Allocate Metal buffers for K and V states
        let k_buffer = device.new_buffer(capacity_bytes, MTLResourceOptions::StorageModeShared);

        let v_buffer = device.new_buffer(capacity_bytes, MTLResourceOptions::StorageModeShared);

        tracing::info!(
            "Initialized KV cache with Metal: {} MB capacity",
            capacity_bytes / (1024 * 1024)
        );

        Ok(Self {
            _device: Some(device),
            k_buffer: Some(k_buffer),
            v_buffer: Some(v_buffer),
            capacity_bytes,
            used_bytes: 0,
            allocations: HashMap::new(),
            next_seq_id: 1,
            bytes_per_token,
            free_regions: Vec::new(),
        })
    }

    /// Allocate cache for sequence
    ///
    /// Returns sequence ID that can be used to free the allocation later.
    pub fn allocate(&mut self, seq_len: usize) -> Result<SequenceId> {
        // Round sequence length to slab sizes to improve reuse
        let rounded_len = if seq_len <= 128 {
            128
        } else if seq_len <= 256 {
            256
        } else if seq_len <= 512 {
            512
        } else {
            seq_len
        } as u64;
        let required_bytes = rounded_len * self.bytes_per_token;

        // Check capacity
        if self.used_bytes + required_bytes * 2 > self.capacity_bytes {
            return Err(AosError::MemoryPressure(format!(
                "KV cache full: {} / {} bytes used",
                self.used_bytes, self.capacity_bytes
            )));
        }

        let seq_id = self.next_seq_id;
        self.next_seq_id += 1;

        // Try to use a free region first
        let k_offset;
        let k_size = required_bytes;
        let v_offset;
        let v_size = required_bytes;
        if let Some((idx, &(off, size))) = self
            .free_regions
            .iter()
            .enumerate()
            .find(|(_, &(_off, size))| size >= required_bytes * 2)
        {
            // Use this region and adjust remaining
            k_offset = off;
            v_offset = k_offset + k_size;
            let consumed = k_size + v_size;
            let remaining = size - consumed;
            let start = off + consumed;
            self.free_regions.remove(idx);
            if remaining > 0 {
                self.free_regions.push((start, remaining));
            }
        } else {
            // Append at the end
            k_offset = self.used_bytes;
            v_offset = k_offset + k_size;
        }

        let allocation = KvAllocation {
            _sequence_id: seq_id,
            k_offset,
            k_size,
            v_offset,
            v_size,
        };

        self.allocations.insert(seq_id, allocation);
        // Update used pointer only if we appended
        let end_pos = v_offset + v_size;
        if end_pos > self.used_bytes {
            self.used_bytes = end_pos;
        }

        tracing::debug!(
            "Allocated KV cache for seq {}: {} tokens, {} bytes (K: {}+{}, V: {}+{})",
            seq_id,
            seq_len,
            k_size + v_size,
            k_offset,
            k_size,
            v_offset,
            v_size
        );

        Ok(seq_id)
    }

    /// Free cache for sequence
    pub fn free(&mut self, seq_id: SequenceId) -> Result<()> {
        if let Some(allocation) = self.allocations.remove(&seq_id) {
            let freed_bytes = allocation.k_size + allocation.v_size;
            // Mark the region as free and coalesce
            self.free_regions.push((allocation.k_offset, freed_bytes));
            self.coalesce_free_regions();

            tracing::debug!(
                "Freed KV cache for seq {}: {} bytes (offset {})",
                seq_id,
                freed_bytes,
                allocation.k_offset
            );

            Ok(())
        } else {
            Err(AosError::Worker(format!(
                "Sequence ID {} not found in KV cache",
                seq_id
            )))
        }
    }

    /// Get usage statistics
    pub fn usage(&self) -> (u64, u64) {
        (self.used_bytes, self.capacity_bytes)
    }

    /// Get usage percentage
    pub fn usage_percent(&self) -> f32 {
        if self.capacity_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f32 / self.capacity_bytes as f32) * 100.0
        }
    }

    /// Get number of active sequences
    pub fn active_sequences(&self) -> usize {
        self.allocations.len()
    }

    /// Check if sequence is allocated
    pub fn is_allocated(&self, seq_id: SequenceId) -> bool {
        self.allocations.contains_key(&seq_id)
    }

    /// Zeroize KV cache buffers for security
    ///
    /// Clears all cached K/V states by writing zeros to Metal buffers.
    /// This is important for security when deallocating sensitive sequences.
    pub fn zeroize(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if let (Some(ref k_buffer), Some(ref v_buffer)) = (&self.k_buffer, &self.v_buffer) {
                // Zero out K buffer
                let k_contents = k_buffer.contents();
                unsafe {
                    let k_slice = std::slice::from_raw_parts_mut(
                        k_contents as *mut u8,
                        k_buffer.length() as usize,
                    );
                    k_slice.fill(0);
                }

                // Zero out V buffer
                let v_contents = v_buffer.contents();
                unsafe {
                    let v_slice = std::slice::from_raw_parts_mut(
                        v_contents as *mut u8,
                        v_buffer.length() as usize,
                    );
                    v_slice.fill(0);
                }

                tracing::info!(
                    "Zeroized KV cache buffers ({} MB)",
                    self.capacity_bytes / (1024 * 1024)
                );
            }
        }

        // Clear allocations
        self.allocations.clear();
        self.used_bytes = 0;
        self.free_regions.clear();
    }

    /// Zeroize specific sequence for security
    pub fn zeroize_sequence(&mut self, seq_id: SequenceId) -> Result<()> {
        let allocation = self
            .allocations
            .get(&seq_id)
            .ok_or_else(|| AosError::Worker(format!("Sequence ID {} not found", seq_id)))?;

        #[cfg(target_os = "macos")]
        {
            if let (Some(ref k_buffer), Some(ref v_buffer)) = (&self.k_buffer, &self.v_buffer) {
                // Zero out K buffer slice
                let k_contents = k_buffer.contents();
                unsafe {
                    let k_slice = std::slice::from_raw_parts_mut(
                        k_contents.add(allocation.k_offset as usize) as *mut u8,
                        allocation.k_size as usize,
                    );
                    k_slice.fill(0);
                }

                // Zero out V buffer slice
                let v_contents = v_buffer.contents();
                unsafe {
                    let v_slice = std::slice::from_raw_parts_mut(
                        v_contents.add(allocation.v_offset as usize) as *mut u8,
                        allocation.v_size as usize,
                    );
                    v_slice.fill(0);
                }

                tracing::debug!("Zeroized KV cache for sequence {}", seq_id);
            }
        }

        // Free the allocation
        self.free(seq_id)
    }

    /// Get Metal buffer references for kernel dispatch
    #[cfg(target_os = "macos")]
    pub fn get_buffers(&self) -> Option<(&metal::Buffer, &metal::Buffer)> {
        match (&self.k_buffer, &self.v_buffer) {
            (Some(k), Some(v)) => Some((k, v)),
            _ => None,
        }
    }

    /// Get allocation info for a sequence
    pub fn get_allocation(&self, seq_id: SequenceId) -> Option<(u64, u64, u64, u64)> {
        self.allocations
            .get(&seq_id)
            .map(|alloc| (alloc.k_offset, alloc.k_size, alloc.v_offset, alloc.v_size))
    }
}

impl ZeroizableBuffer for KvCache {
    fn zeroize(&mut self) {
        self.zeroize()
    }
}

impl KvCache {
    /// Coalesce adjacent free regions
    fn coalesce_free_regions(&mut self) {
        if self.free_regions.is_empty() {
            return;
        }
        self.free_regions.sort_by_key(|(off, _)| *off);
        let mut merged: Vec<(u64, u64)> = Vec::with_capacity(self.free_regions.len());
        let mut current = self.free_regions[0];
        for &(off, size) in self.free_regions.iter().skip(1) {
            let (cur_off, cur_size) = current;
            if off == cur_off + cur_size {
                current = (cur_off, cur_size + size);
            } else {
                merged.push(current);
                current = (off, size);
            }
        }
        merged.push(current);
        self.free_regions = merged;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kv_cache_allocation() {
        let mut cache = KvCache::new(1024 * 1024); // 1 MB

        // Allocate for 128 token sequence
        let seq_id = cache
            .allocate(128)
            .expect("Test cache allocation should succeed");
        assert_eq!(cache.active_sequences(), 1);
        assert!(cache.is_allocated(seq_id));

        // Check usage
        let (used, capacity) = cache.usage();
        assert!(used > 0);
        assert_eq!(capacity, 1024 * 1024);

        // Free allocation
        cache.free(seq_id).expect("Test cache free should succeed");
        assert_eq!(cache.active_sequences(), 0);
        assert!(!cache.is_allocated(seq_id));
    }

    #[test]
    fn test_kv_cache_oom() {
        let mut cache = KvCache::new(1024); // Very small: 1 KB

        // This should fail due to insufficient capacity
        let result = cache.allocate(1024);
        assert!(result.is_err());
    }

    #[test]
    fn test_kv_cache_zeroize() {
        let mut cache = KvCache::new(1024 * 1024);

        // Allocate some sequences
        let _seq1 = cache
            .allocate(64)
            .expect("Test cache allocation should succeed");
        let _seq2 = cache
            .allocate(128)
            .expect("Test cache allocation should succeed");

        assert_eq!(cache.active_sequences(), 2);

        // Zeroize all
        cache.zeroize();

        assert_eq!(cache.active_sequences(), 0);
        assert_eq!(cache.used_bytes, 0);
    }

    #[test]
    fn test_kv_cache_multiple_sequences() {
        let mut cache = KvCache::new(10 * 1024 * 1024); // 10 MB

        let seq1 = cache
            .allocate(64)
            .expect("Test cache allocation should succeed");
        let seq2 = cache
            .allocate(128)
            .expect("Test cache allocation should succeed");
        let seq3 = cache
            .allocate(256)
            .expect("Test cache allocation should succeed");

        assert_eq!(cache.active_sequences(), 3);

        // Free middle sequence
        cache.free(seq2).expect("Test cache free should succeed");
        assert_eq!(cache.active_sequences(), 2);
        assert!(cache.is_allocated(seq1));
        assert!(!cache.is_allocated(seq2));
        assert!(cache.is_allocated(seq3));
    }
}
