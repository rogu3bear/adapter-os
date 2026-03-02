//! KV cache management with Metal buffer support
//!
//! Provides production-ready KV cache with Metal-backed buffers for efficient
//! GPU memory management. Supports per-sequence allocation, buffer zeroization,
//! and OOM handling.
//!
//! # KV Cache Coherence
//!
//! When adapters are hot-swapped (generation counter changes), the KV cache may
//! contain stale computations from previous adapter state, leading to non-deterministic
//! behavior. This module provides automatic coherence checking:
//!
//! - `SequenceGuard`: RAII guard that captures stack generation at sequence start
//! - `ensure_cache_coherence()`: Resets KV cache on generation change
//! - Generation tracking integrated with hot-swap mechanism
//!
//! # Usage
//!
//! ```ignore
//! use adapteros_lora_worker::kvcache::KvCache;
//! use adapteros_core::constants::BYTES_PER_MB;
//!
//! let mut cache = KvCache::new(BYTES_PER_MB);
//!
//! // Set initial generation
//! cache.set_generation(1);
//!
//! // Allocate sequence with guard
//! let guard = cache.allocate_with_guard(128, 1)?;
//!
//! // On generation change, cache is automatically reset
//! cache.ensure_cache_coherence(2)?; // Resets if generation changed
//! # Ok::<(), adapteros_core::AosError>(())
//! ```

use adapteros_core::{constants::BYTES_PER_MB, AosError, Result};
use std::collections::HashMap;
use std::sync::Arc;

use crate::TenantKvQuotaManager;

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

/// RAII guard that tracks adapter stack generation for a sequence
///
/// Automatically validates cache coherence when dropped. If the stack generation
/// has changed during the sequence lifetime, this indicates the KV cache may
/// contain stale computations.
#[derive(Debug)]
pub struct SequenceGuard {
    /// Sequence ID being guarded
    pub sequence_id: SequenceId,
    /// Stack generation captured at sequence start
    pub generation: u64,
    /// Whether the guard is still active
    active: bool,
}

impl SequenceGuard {
    /// Create a new sequence guard
    pub fn new(sequence_id: SequenceId, generation: u64) -> Self {
        Self {
            sequence_id,
            generation,
            active: true,
        }
    }

    /// Mark guard as inactive (prevents drop validation)
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Check if guard is active
    pub fn is_active(&self) -> bool {
        self.active
    }
}

/// Metal-backed KV cache with slab allocator and coherence tracking
///
/// Layout: [seq_0_k | seq_0_v | seq_1_k | seq_1_v | ...]
///
/// Each sequence gets a contiguous slice for K and V states.
/// Buffers use MTLResourceOptions::StorageModeShared for CPU/GPU access.
///
/// # Coherence Guarantee
///
/// The cache tracks the adapter stack generation and automatically resets
/// when the generation changes, preventing stale KV states from affecting
/// inference with new adapter configurations.
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
    /// Current adapter stack generation
    stack_generation: u64,
    /// Active sequence guards
    active_guards: HashMap<SequenceId, u64>,
    /// Optional quota manager for per-tenant KV cache limits
    quota_manager: Option<Arc<TenantKvQuotaManager>>,
}

impl KvCache {
    /// Release quota bytes for all currently tracked allocations.
    ///
    /// Called before bulk cache clears (`reset_all`/`zeroize*`) so tenant quota
    /// state remains consistent with cache contents.
    fn release_allocation_quota(&self) {
        let Some(ref quota_manager) = self.quota_manager else {
            return;
        };

        let allocated_bytes: u64 = self
            .allocations
            .values()
            .map(|a| a.k_size.saturating_add(a.v_size))
            .sum();
        if allocated_bytes == 0 {
            return;
        }

        // Guard against stale accounting: never release more than currently used.
        let used_bytes = quota_manager.usage().used_bytes;
        let release_bytes = allocated_bytes.min(used_bytes);
        if release_bytes > 0 {
            quota_manager.release(release_bytes);
        }
    }

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
            stack_generation: 0,
            active_guards: HashMap::new(),
            quota_manager: None,
        }
    }

    /// Create new KV cache with quota manager
    pub fn new_with_quota(
        capacity_bytes: u64,
        quota_manager: Option<Arc<TenantKvQuotaManager>>,
    ) -> Self {
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
            stack_generation: 0,
            active_guards: HashMap::new(),
            quota_manager,
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
            capacity_bytes / BYTES_PER_MB
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
            stack_generation: 0,
            active_guards: HashMap::new(),
            quota_manager: None,
        })
    }

    /// Create new KV cache with Metal device and quota manager
    #[cfg(target_os = "macos")]
    pub fn new_with_metal_and_quota(
        device: Arc<metal::Device>,
        capacity_bytes: u64,
        bytes_per_token: u64,
        quota_manager: Option<Arc<TenantKvQuotaManager>>,
    ) -> Result<Self> {
        use metal::MTLResourceOptions;

        // Allocate Metal buffers for K and V states
        let k_buffer = device.new_buffer(capacity_bytes, MTLResourceOptions::StorageModeShared);

        let v_buffer = device.new_buffer(capacity_bytes, MTLResourceOptions::StorageModeShared);

        tracing::info!(
            "Initialized KV cache with Metal: {} MB capacity",
            capacity_bytes / BYTES_PER_MB
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
            stack_generation: 0,
            active_guards: HashMap::new(),
            quota_manager,
        })
    }

    /// Set current stack generation
    ///
    /// Should be called when adapter stack changes to track coherence.
    pub fn set_generation(&mut self, generation: u64) {
        self.stack_generation = generation;
    }

    /// Get current stack generation
    pub fn generation(&self) -> u64 {
        self.stack_generation
    }

    /// Ensure cache coherence with current stack generation
    ///
    /// Resets the cache if the generation has changed since allocation,
    /// preventing stale KV states from being used with new adapter configurations.
    ///
    /// # Arguments
    /// * `current_generation` - Current adapter stack generation
    ///
    /// # Returns
    /// Ok(true) if cache was reset, Ok(false) if coherent
    pub fn ensure_cache_coherence(&mut self, current_generation: u64) -> Result<bool> {
        if current_generation != self.stack_generation {
            tracing::warn!(
                old_generation = self.stack_generation,
                new_generation = current_generation,
                active_sequences = self.active_sequences(),
                "Stack generation changed, resetting KV cache for coherence"
            );

            // Reset cache to ensure coherence
            self.reset_all();
            self.stack_generation = current_generation;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Allocate cache for sequence with generation guard
    ///
    /// Returns a SequenceGuard that tracks the stack generation at allocation time.
    /// This enables automatic coherence validation.
    ///
    /// # Arguments
    /// * `seq_len` - Sequence length in tokens
    /// * `generation` - Current adapter stack generation
    ///
    /// # Returns
    /// SequenceGuard that must be held for the sequence lifetime
    pub fn allocate_with_guard(
        &mut self,
        seq_len: usize,
        generation: u64,
    ) -> Result<SequenceGuard> {
        // Ensure cache is coherent with current generation
        self.ensure_cache_coherence(generation)?;

        let seq_id = self.allocate(seq_len)?;
        let guard = SequenceGuard::new(seq_id, generation);

        // Track active guard
        self.active_guards.insert(seq_id, generation);

        tracing::debug!(
            sequence_id = seq_id,
            generation = generation,
            seq_len = seq_len,
            "Allocated sequence with generation guard"
        );

        Ok(guard)
    }

    /// Check if any active sequences would be invalidated by generation change
    ///
    /// Returns true if a generation change would require draining active sequences.
    pub fn has_active_sequences_with_generation(&self, generation: u64) -> bool {
        self.active_guards
            .values()
            .any(|&guard_gen| guard_gen != generation)
    }

    /// Wait for all active sequences to complete before generation change
    ///
    /// This should be called before hot-swapping adapters to ensure no
    /// sequences are using stale KV cache state.
    ///
    /// # Returns
    /// Number of sequences that were drained
    pub fn drain_active_sequences(&mut self) -> usize {
        let count = self.active_guards.len();

        if count > 0 {
            tracing::info!(
                active_sequences = count,
                "Draining active sequences before generation change"
            );

            // Free all active sequences
            let seq_ids: Vec<SequenceId> = self.active_guards.keys().copied().collect();
            for seq_id in seq_ids {
                let _ = self.free(seq_id);
            }

            self.active_guards.clear();
        }

        count
    }

    /// Allocate cache for sequence
    ///
    /// Returns sequence ID that can be used to free the allocation later.
    ///
    /// Note: Consider using `allocate_with_guard()` for automatic coherence tracking.
    pub fn allocate(&mut self, seq_len: usize) -> Result<SequenceId> {
        // Cap sequence length to prevent overflow (max 1M tokens)
        let seq_len = seq_len.min(1 << 20);

        // Round sequence length to slab sizes to improve reuse
        let rounded_len: u64 = if seq_len <= 128 {
            128
        } else if seq_len <= 256 {
            256
        } else if seq_len <= 512 {
            512
        } else {
            seq_len as u64
        };

        let required_bytes = rounded_len
            .checked_mul(self.bytes_per_token)
            .ok_or_else(|| AosError::Validation("Sequence length overflow".into()))?;

        // Reserve quota if quota manager is present
        let reservation = if let Some(ref quota_manager) = self.quota_manager {
            match quota_manager.reserve(required_bytes * 2) {
                Ok(res) => Some(res),
                Err(AosError::MemoryPressure(msg)) if msg.contains("KV quota exceeded") => {
                    // Return specific error for quota exhaustion
                    return Err(AosError::QuotaExceeded {
                        resource: "kv_cache".to_string(),
                        failure_code: Some("KV_QUOTA_EXCEEDED".to_string()),
                    });
                }
                Err(e) => return Err(e),
            }
        } else {
            None
        };

        // Check capacity
        if self.used_bytes + required_bytes * 2 > self.capacity_bytes {
            // Rollback reservation on capacity failure
            if let Some(res) = reservation {
                if let Some(ref quota_manager) = self.quota_manager {
                    quota_manager.rollback(res);
                }
            }
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

        // Finalize quota reservation after successful allocation
        if let Some(res) = reservation {
            if let Some(ref quota_manager) = self.quota_manager {
                if let Err(e) = quota_manager.finalize(res) {
                    // If finalization fails, rollback the allocation
                    self.allocations.remove(&seq_id);
                    if end_pos == self.used_bytes {
                        self.used_bytes -= k_size + v_size;
                    }
                    return Err(e);
                }
            }
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
        // Remove guard if exists
        self.active_guards.remove(&seq_id);

        if let Some(allocation) = self.allocations.remove(&seq_id) {
            let freed_bytes = allocation.k_size + allocation.v_size;
            // Mark the region as free and coalesce
            self.free_regions.push((allocation.k_offset, freed_bytes));
            self.coalesce_free_regions();

            // Release quota if quota manager is present
            if let Some(ref quota_manager) = self.quota_manager {
                quota_manager.release(freed_bytes);
            }

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
                // SAFETY: k_contents is a valid pointer returned by Metal buffer.contents().
                // k_buffer.length() returns the exact buffer size allocated.
                // Metal StorageModeShared buffers are CPU-accessible for read/write.
                unsafe {
                    let k_slice = std::slice::from_raw_parts_mut(
                        k_contents as *mut u8,
                        k_buffer.length() as usize,
                    );
                    k_slice.fill(0);
                }

                // Zero out V buffer
                let v_contents = v_buffer.contents();
                // SAFETY: v_contents is a valid pointer returned by Metal buffer.contents().
                // v_buffer.length() returns the exact buffer size allocated.
                // Metal StorageModeShared buffers are CPU-accessible for read/write.
                unsafe {
                    let v_slice = std::slice::from_raw_parts_mut(
                        v_contents as *mut u8,
                        v_buffer.length() as usize,
                    );
                    v_slice.fill(0);
                }

                tracing::info!(
                    "Zeroized KV cache buffers ({} MB)",
                    self.capacity_bytes / BYTES_PER_MB
                );
            }
        }

        // Release quota for all active allocations, then clear cache state.
        self.release_allocation_quota();
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
                let k_offset = allocation.k_offset as usize;
                let k_size = allocation.k_size as usize;
                let v_offset = allocation.v_offset as usize;
                let v_size = allocation.v_size as usize;
                let buffer_len = k_buffer.length() as usize;

                // Validate bounds before unsafe operations to prevent buffer overflows
                let k_end = k_offset
                    .checked_add(k_size)
                    .ok_or_else(|| AosError::Worker("K buffer offset+size overflow".to_string()))?;
                let v_end = v_offset
                    .checked_add(v_size)
                    .ok_or_else(|| AosError::Worker("V buffer offset+size overflow".to_string()))?;

                if k_end > buffer_len {
                    return Err(AosError::Worker(format!(
                        "K buffer bounds exceeded: offset {} + size {} > buffer len {}",
                        k_offset, k_size, buffer_len
                    )));
                }
                if v_end > buffer_len {
                    return Err(AosError::Worker(format!(
                        "V buffer bounds exceeded: offset {} + size {} > buffer len {}",
                        v_offset, v_size, buffer_len
                    )));
                }

                // Zero out K buffer slice
                let k_contents = k_buffer.contents();
                // SAFETY: k_contents is a valid pointer from Metal buffer.contents().
                // Bounds validated above: k_offset + k_size <= buffer_len.
                // Metal StorageModeShared buffers are CPU-accessible.
                unsafe {
                    let k_slice =
                        std::slice::from_raw_parts_mut(k_contents.add(k_offset) as *mut u8, k_size);
                    k_slice.fill(0);
                }

                // Zero out V buffer slice
                let v_contents = v_buffer.contents();
                // SAFETY: v_contents is a valid pointer from Metal buffer.contents().
                // Bounds validated above: v_offset + v_size <= buffer_len.
                // Metal StorageModeShared buffers are CPU-accessible.
                unsafe {
                    let v_slice =
                        std::slice::from_raw_parts_mut(v_contents.add(v_offset) as *mut u8, v_size);
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

    /// Reset all KV cache allocations (called on adapter swap if stack changes)
    pub fn reset_all(&mut self) {
        self.release_allocation_quota();
        self.used_bytes = 0;
        self.allocations.clear();
        self.next_seq_id = 1;
        self.free_regions.clear();
        self.active_guards.clear();
        #[cfg(target_os = "macos")]
        if let (Some(k_buf), Some(v_buf)) = (&mut self.k_buffer, &mut self.v_buffer) {
            // SAFETY: Both k_buf and v_buf are valid Metal buffers allocated with
            // capacity_bytes size. contents() returns a valid CPU-accessible pointer
            // for StorageModeShared buffers. We write exactly capacity_bytes bytes.
            unsafe {
                std::ptr::write_bytes(k_buf.contents() as *mut u8, 0, self.capacity_bytes as usize);
                std::ptr::write_bytes(v_buf.contents() as *mut u8, 0, self.capacity_bytes as usize);
            }
        }
    }

    /// Zeroize all KV cache buffers and reset state
    pub fn zeroize_all(&mut self) {
        self.release_allocation_quota();
        self.allocations.clear();
        self.used_bytes = 0;
        self.free_regions = vec![(0, self.capacity_bytes)];
        self.active_guards.clear();

        #[cfg(target_os = "macos")]
        {
            if let Some(ref mut k_buffer) = &mut self.k_buffer {
                let contents = k_buffer.contents();
                let zero_slice = vec![0u8; self.capacity_bytes as usize];
                // Alignment check for safety (u8 has no alignment requirements, but validates pointer)
                assert_eq!(
                    contents as usize % std::mem::align_of::<u8>(),
                    0,
                    "K buffer misaligned"
                );
                // SAFETY: contents is a valid pointer from Metal buffer.contents() for
                // StorageModeShared buffers. Buffer was allocated with capacity_bytes size.
                // zero_slice.len() == capacity_bytes. Alignment validated above.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        zero_slice.as_ptr(),
                        contents as *mut u8,
                        zero_slice.len(),
                    );
                }
            }

            if let Some(ref mut v_buffer) = &mut self.v_buffer {
                let contents = v_buffer.contents();
                let zero_slice = vec![0u8; self.capacity_bytes as usize];
                // Alignment check for safety (u8 has no alignment requirements, but validates pointer)
                assert_eq!(
                    contents as usize % std::mem::align_of::<u8>(),
                    0,
                    "V buffer misaligned"
                );
                // SAFETY: contents is a valid pointer from Metal buffer.contents() for
                // StorageModeShared buffers. Buffer was allocated with capacity_bytes size.
                // zero_slice.len() == capacity_bytes. Alignment validated above.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        zero_slice.as_ptr(),
                        contents as *mut u8,
                        zero_slice.len(),
                    );
                }
            }
        }

        tracing::debug!(
            "KV cache zeroized: {} allocations cleared",
            self.active_sequences()
        );
    }
}

impl ZeroizableBuffer for KvCache {
    fn zeroize(&mut self) {
        KvCache::zeroize(self)
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
        // Need 2x capacity for K+V buffers: 128 tokens * 8192 bytes/token * 2 = 2MB
        let mut cache = KvCache::new(4 * BYTES_PER_MB); // 4 MB

        // Allocate for 128 token sequence
        let seq_id = cache
            .allocate(128)
            .expect("Test cache allocation should succeed");
        assert_eq!(cache.active_sequences(), 1);
        assert!(cache.is_allocated(seq_id));

        // Check usage
        let (used, capacity) = cache.usage();
        assert!(used > 0);
        assert_eq!(capacity, 4 * BYTES_PER_MB);

        // Free allocation
        cache.free(seq_id).expect("Test cache free should succeed");
        assert_eq!(cache.active_sequences(), 0);
        assert!(!cache.is_allocated(seq_id));
    }

    #[test]
    fn test_generation_tracking() {
        let mut cache = KvCache::new(4 * BYTES_PER_MB);

        // Initial generation
        assert_eq!(cache.generation(), 0);

        // Set generation
        cache.set_generation(1);
        assert_eq!(cache.generation(), 1);

        // Set again
        cache.set_generation(5);
        assert_eq!(cache.generation(), 5);
    }

    #[test]
    fn test_cache_coherence_reset() {
        let mut cache = KvCache::new(4 * BYTES_PER_MB);

        // Set initial generation
        cache.set_generation(1);

        // Allocate with generation 1
        let _seq_id = cache.allocate(128).expect("Allocation should succeed");
        assert_eq!(cache.active_sequences(), 1);

        // Ensure coherence with same generation - no reset
        let reset = cache
            .ensure_cache_coherence(1)
            .expect("Coherence check should succeed");
        assert!(!reset, "Cache should not reset on same generation");
        assert_eq!(cache.active_sequences(), 1);

        // Ensure coherence with different generation - should reset
        let reset = cache
            .ensure_cache_coherence(2)
            .expect("Coherence check should succeed");
        assert!(reset, "Cache should reset on generation change");
        assert_eq!(cache.active_sequences(), 0);
        assert_eq!(cache.generation(), 2);
    }

    #[test]
    fn test_allocate_with_guard() {
        let mut cache = KvCache::new(4 * BYTES_PER_MB);

        // Allocate with guard
        let guard = cache
            .allocate_with_guard(128, 1)
            .expect("Guarded allocation should succeed");

        assert_eq!(guard.generation, 1);
        assert!(guard.is_active());
        assert_eq!(cache.active_sequences(), 1);
        assert_eq!(cache.generation(), 1);

        // Check guard is tracked
        assert!(cache.active_guards.contains_key(&guard.sequence_id));
    }

    #[test]
    fn test_guard_deactivation() {
        let mut guard = SequenceGuard::new(1, 5);

        assert!(guard.is_active());
        guard.deactivate();
        assert!(!guard.is_active());
    }

    #[test]
    fn test_has_active_sequences_with_generation() {
        // Use larger cache to accommodate multiple allocations
        // Each 128-token allocation needs ~2MB (128 * 8192 bytes_per_token * 2 for K/V)
        let mut cache = KvCache::new(16 * BYTES_PER_MB);

        // No active sequences
        assert!(!cache.has_active_sequences_with_generation(1));

        // Allocate with generation 1
        let _guard1 = cache
            .allocate_with_guard(64, 1)
            .expect("Allocation should succeed");

        // Check for same generation - should be false (no mismatches)
        assert!(!cache.has_active_sequences_with_generation(1));

        // Check for different generation - should be true (mismatch detected)
        assert!(cache.has_active_sequences_with_generation(2));

        // Allocate with generation 2 - this triggers coherence reset, clearing gen 1 guards
        let _guard2 = cache
            .allocate_with_guard(64, 2)
            .expect("Allocation should succeed");

        // After generation change, only gen 2 guards exist (gen 1 was cleared for coherence)
        // Check for gen 2 - should be false (all guards match gen 2)
        assert!(!cache.has_active_sequences_with_generation(2));
        // Check for gen 1 - gen 2 guards exist, mismatch with gen 1
        assert!(cache.has_active_sequences_with_generation(1));
    }

    #[test]
    fn test_drain_active_sequences() {
        // Use larger cache to accommodate multiple allocations
        let mut cache = KvCache::new(32 * BYTES_PER_MB);

        // Allocate multiple sequences (smaller sizes to fit in cache)
        let _guard1 = cache
            .allocate_with_guard(64, 1)
            .expect("Allocation should succeed");
        let _guard2 = cache
            .allocate_with_guard(64, 1)
            .expect("Allocation should succeed");
        let _guard3 = cache
            .allocate_with_guard(64, 1)
            .expect("Allocation should succeed");

        assert_eq!(cache.active_sequences(), 3);

        // Drain all sequences
        let drained = cache.drain_active_sequences();
        assert_eq!(drained, 3);
        assert_eq!(cache.active_sequences(), 0);
        assert!(cache.active_guards.is_empty());
    }

    #[test]
    fn test_free_removes_guard() {
        let mut cache = KvCache::new(4 * BYTES_PER_MB);

        // Allocate with guard
        let guard = cache
            .allocate_with_guard(128, 1)
            .expect("Allocation should succeed");

        let seq_id = guard.sequence_id;
        assert!(cache.active_guards.contains_key(&seq_id));

        // Free sequence
        cache.free(seq_id).expect("Free should succeed");

        // Guard should be removed
        assert!(!cache.active_guards.contains_key(&seq_id));
    }

    #[test]
    fn test_coherence_integration() {
        let mut cache = KvCache::new(8 * BYTES_PER_MB);

        // Simulate inference with generation 1
        cache.set_generation(1);
        let guard1 = cache
            .allocate_with_guard(128, 1)
            .expect("Allocation should succeed");

        assert_eq!(cache.active_sequences(), 1);

        // Simulate adapter hot-swap to generation 2
        // This should reset cache if we call ensure_cache_coherence
        let reset = cache
            .ensure_cache_coherence(2)
            .expect("Coherence check should succeed");

        assert!(reset, "Cache should reset on generation change");
        assert_eq!(cache.active_sequences(), 0);
        assert_eq!(cache.generation(), 2);

        // Old guard's sequence should be gone
        assert!(!cache.is_allocated(guard1.sequence_id));

        // New allocation with generation 2
        let _guard2 = cache
            .allocate_with_guard(256, 2)
            .expect("Allocation should succeed");

        assert_eq!(cache.active_sequences(), 1);
        assert_eq!(cache.generation(), 2);
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
        // Need 2x capacity for K+V buffers per sequence
        let mut cache = KvCache::new(8 * BYTES_PER_MB); // 8 MB for multiple allocations

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
        let mut cache = KvCache::new(10 * BYTES_PER_MB); // 10 MB

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

    #[test]
    fn test_reset_all() {
        // seq_len 10 rounds to 128 tokens, needs 128 * 8192 * 2 = 2MB
        let mut cache = KvCache::new(4 * BYTES_PER_MB);
        let _seq1 = cache.allocate(10).unwrap();
        assert!(!cache.allocations.is_empty());
        assert!(cache.used_bytes > 0);

        cache.reset_all();

        assert!(cache.allocations.is_empty());
        assert_eq!(cache.used_bytes, 0);
        assert!(cache.free_regions.is_empty());
        assert_eq!(cache.next_seq_id, 1);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        # [test]
        fn prop_kv_allocate_no_overlap(seqs in prop::collection::vec(1usize..1000, 1..10)) {
            // Each seq can be up to 1000 tokens, rounded to 1000
            // 1000 tokens * 8192 bytes = ~8MB per seq, K+V = 16MB per seq
            // 10 seqs = 160MB max, use 200MB
            let mut cache = KvCache::new(200 * BYTES_PER_MB); // 200MB
            let mut offsets = Vec::new();
            for seq_len in seqs {
                let id = cache.allocate(seq_len).unwrap();
                let alloc = cache.allocations.get(&id).unwrap();
                prop_assert!(!offsets.contains(&alloc.k_offset));
                offsets.push(alloc.k_offset);
                prop_assert!(alloc.k_size > 0);
                prop_assert!(alloc.v_offset == alloc.k_offset + alloc.k_size);
            }
        }

        # [test]
        fn prop_kv_reset_reallocates_from_zero(seq_len in 1usize..512) {
            // seq_len up to 512 tokens, rounded to 512
            // 512 tokens * 8192 bytes = 4MB, K+V = 8MB needed, use 16MB
            let mut cache = KvCache::new(16 * BYTES_PER_MB);
            let _id1 = cache.allocate(seq_len).unwrap();
            cache.reset_all();
            let id2 = cache.allocate(seq_len).unwrap();
            let alloc2 = cache.allocations.get(&id2).unwrap();
            prop_assert_eq!(alloc2.k_offset, 0);
            prop_assert_eq!(cache.next_seq_id, 2);
            prop_assert_eq!(cache.used_bytes, alloc2.k_size + alloc2.v_size);
        }
    }
}
