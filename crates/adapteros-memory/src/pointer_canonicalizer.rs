//! Pointer reuse canonicalization for deterministic memory behavior
//!
//! Ensures that pointer reuse patterns are consistent across runs by:
//! - Tracking pointer allocation and deallocation order
//! - Canonicalizing pointer reuse based on logical allocation order
//! - Detecting when OS memory tricks affect pointer reuse
//! - Providing deterministic pointer mapping for replay

use crate::{MemoryWatchdogError, Result};
use adapteros_core::B3Hash;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

/// Pointer allocation record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointerAllocation {
    /// Unique allocation ID
    pub allocation_id: Uuid,
    /// Logical allocation order
    pub logical_order: u64,
    /// Actual pointer address
    pub pointer_addr: u64,
    /// Allocation size
    pub size_bytes: u64,
    /// Allocation timestamp
    pub timestamp: u128,
    /// Allocation context
    pub context: String,
    /// Whether this pointer was reused
    pub was_reused: bool,
    /// Previous allocation ID (if reused)
    pub previous_allocation_id: Option<Uuid>,
}

/// Pointer reuse pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PointerReusePattern {
    /// Pattern ID
    pub pattern_id: Uuid,
    /// Logical allocation order
    pub logical_order: u64,
    /// Canonicalized pointer address
    pub canonical_addr: u64,
    /// Reuse count
    pub reuse_count: u32,
    /// First allocation timestamp
    pub first_allocation_timestamp: u128,
    /// Last allocation timestamp
    pub last_allocation_timestamp: u128,
    /// Allocation IDs in this pattern
    pub allocation_ids: Vec<Uuid>,
}

/// Canonicalized memory layout
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalMemoryLayout {
    /// Layout ID
    pub layout_id: Uuid,
    /// Layout hash
    pub layout_hash: B3Hash,
    /// Pointer reuse patterns
    pub reuse_patterns: Vec<PointerReusePattern>,
    /// Total allocations
    pub total_allocations: usize,
    /// Total reused pointers
    pub total_reused: usize,
    /// Layout timestamp
    pub timestamp: u128,
}

/// Pointer canonicalizer
pub struct PointerCanonicalizer {
    /// Active allocations by pointer address
    active_allocations: Arc<RwLock<HashMap<u64, PointerAllocation>>>,
    /// Allocation history for reuse detection
    allocation_history: Arc<RwLock<VecDeque<PointerAllocation>>>,
    /// Reuse patterns
    reuse_patterns: Arc<RwLock<HashMap<u64, PointerReusePattern>>>,
    /// Logical allocation counter
    logical_order_counter: Arc<std::sync::atomic::AtomicU64>,
    /// Canonicalized layouts
    canonical_layouts: Arc<RwLock<Vec<CanonicalMemoryLayout>>>,
    /// Maximum history size
    max_history_size: usize,
}

impl PointerCanonicalizer {
    /// Create a new pointer canonicalizer
    pub fn new(max_history_size: usize) -> Self {
        Self {
            active_allocations: Arc::new(RwLock::new(HashMap::new())),
            allocation_history: Arc::new(RwLock::new(VecDeque::new())),
            reuse_patterns: Arc::new(RwLock::new(HashMap::new())),
            logical_order_counter: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            canonical_layouts: Arc::new(RwLock::new(Vec::new())),
            max_history_size,
        }
    }

    /// Record pointer allocation
    pub fn record_allocation(
        &self,
        pointer_addr: u64,
        size_bytes: u64,
        context: String,
    ) -> Result<Uuid> {
        let logical_order = self
            .logical_order_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let timestamp = current_timestamp();
        let allocation_id = Uuid::new_v4();

        // Check if this pointer was reused
        let was_reused = {
            let history = self.allocation_history.read();
            history
                .iter()
                .any(|alloc| alloc.pointer_addr == pointer_addr)
        };

        let previous_allocation_id = if was_reused {
            let history = self.allocation_history.read();
            history
                .iter()
                .find(|alloc| alloc.pointer_addr == pointer_addr)
                .map(|alloc| alloc.allocation_id)
        } else {
            None
        };

        let allocation = PointerAllocation {
            allocation_id,
            logical_order,
            pointer_addr,
            size_bytes,
            timestamp,
            context,
            was_reused,
            previous_allocation_id,
        };

        // Record active allocation
        {
            let mut active = self.active_allocations.write();
            active.insert(pointer_addr, allocation.clone());
        }

        // Add to history
        {
            let mut history = self.allocation_history.write();
            history.push_back(allocation.clone());

            // Trim history if too large
            while history.len() > self.max_history_size {
                history.pop_front();
            }
        }

        // Update reuse pattern if this is a reuse
        if was_reused {
            self.update_reuse_pattern(&allocation)?;
        }

        debug!(
            "Recorded pointer allocation: addr=0x{:x}, size={}, logical_order={}, reused={}",
            pointer_addr, size_bytes, logical_order, was_reused
        );

        Ok(allocation_id)
    }

    /// Record pointer deallocation
    pub fn record_deallocation(&self, pointer_addr: u64) -> Result<()> {
        let allocation = {
            let mut active = self.active_allocations.write();
            active.remove(&pointer_addr)
        };

        if let Some(allocation) = allocation {
            debug!(
                "Recorded pointer deallocation: addr=0x{:x}, allocation_id={}",
                pointer_addr, allocation.allocation_id
            );
        }

        Ok(())
    }

    /// Update reuse pattern for a reused pointer
    fn update_reuse_pattern(&self, allocation: &PointerAllocation) -> Result<()> {
        let canonical_addr =
            self.canonicalize_pointer(allocation.pointer_addr, allocation.logical_order);

        let mut patterns = self.reuse_patterns.write();

        if let Some(pattern) = patterns.get_mut(&canonical_addr) {
            // Update existing pattern
            pattern.reuse_count += 1;
            pattern.last_allocation_timestamp = allocation.timestamp;
            pattern.allocation_ids.push(allocation.allocation_id);
        } else {
            // Create new pattern
            let pattern = PointerReusePattern {
                pattern_id: Uuid::new_v4(),
                logical_order: allocation.logical_order,
                canonical_addr,
                reuse_count: 1,
                first_allocation_timestamp: allocation.timestamp,
                last_allocation_timestamp: allocation.timestamp,
                allocation_ids: vec![allocation.allocation_id],
            };

            patterns.insert(canonical_addr, pattern);
        }

        Ok(())
    }

    /// Canonicalize pointer address based on logical order
    fn canonicalize_pointer(&self, actual_addr: u64, logical_order: u64) -> u64 {
        // Use logical order to create deterministic canonical address
        // This ensures the same logical allocation order produces the same canonical address
        logical_order * 0x1000 + (actual_addr & 0xFFF) // Preserve low 12 bits for alignment
    }

    /// Generate canonical memory layout
    pub fn generate_canonical_layout(&self) -> Result<CanonicalMemoryLayout> {
        let timestamp = current_timestamp();
        let layout_id = Uuid::new_v4();

        let reuse_patterns: Vec<PointerReusePattern> = {
            let patterns = self.reuse_patterns.read();
            patterns.values().cloned().collect()
        };

        let total_allocations = {
            let active = self.active_allocations.read();
            active.len()
        };

        let total_reused = reuse_patterns.iter().map(|p| p.reuse_count as usize).sum();

        // Calculate layout hash
        let layout_hash = self.calculate_layout_hash(&reuse_patterns);

        let layout = CanonicalMemoryLayout {
            layout_id,
            layout_hash,
            reuse_patterns,
            total_allocations,
            total_reused,
            timestamp,
        };

        // Store canonical layout
        {
            let mut layouts = self.canonical_layouts.write();
            layouts.push(layout.clone());
        }

        info!(
            "Generated canonical memory layout: id={}, allocations={}, reused={}",
            layout_id, total_allocations, total_reused
        );

        Ok(layout)
    }

    /// Calculate layout hash for determinism verification
    fn calculate_layout_hash(&self, patterns: &[PointerReusePattern]) -> B3Hash {
        let mut hasher = blake3::Hasher::new();

        // Hash patterns in deterministic order
        let mut sorted_patterns: Vec<_> = patterns.iter().collect();
        sorted_patterns.sort_by_key(|p| p.logical_order);

        for pattern in sorted_patterns {
            hasher.update(&pattern.logical_order.to_le_bytes());
            hasher.update(&pattern.canonical_addr.to_le_bytes());
            hasher.update(&pattern.reuse_count.to_le_bytes());
            hasher.update(&pattern.first_allocation_timestamp.to_le_bytes());
        }

        B3Hash::new(*hasher.finalize().as_bytes())
    }

    /// Get current reuse patterns
    pub fn get_reuse_patterns(&self) -> Vec<PointerReusePattern> {
        let patterns = self.reuse_patterns.read();
        patterns.values().cloned().collect()
    }

    /// Get canonical layouts
    pub fn get_canonical_layouts(&self) -> Vec<CanonicalMemoryLayout> {
        let layouts = self.canonical_layouts.read();
        layouts.clone()
    }

    /// Get allocation statistics
    pub fn get_allocation_stats(&self) -> AllocationStats {
        let active = self.active_allocations.read();
        let history = self.allocation_history.read();
        let patterns = self.reuse_patterns.read();

        let total_allocations = history.len();
        let active_allocations = active.len();
        let reused_pointers = patterns.len();
        let total_reuses: u32 = patterns.values().map(|p| p.reuse_count).sum();

        AllocationStats {
            total_allocations,
            active_allocations,
            reused_pointers,
            total_reuses,
            reuse_rate: if total_allocations > 0 {
                reused_pointers as f32 / total_allocations as f32
            } else {
                0.0
            },
        }
    }

    /// Clear all recorded data
    pub fn clear(&self) {
        {
            let mut active = self.active_allocations.write();
            active.clear();
        }
        {
            let mut history = self.allocation_history.write();
            history.clear();
        }
        {
            let mut patterns = self.reuse_patterns.write();
            patterns.clear();
        }
        {
            let mut layouts = self.canonical_layouts.write();
            layouts.clear();
        }

        self.logical_order_counter
            .store(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Verify layout consistency
    pub fn verify_layout_consistency(&self, expected_layout: &CanonicalMemoryLayout) -> Result<()> {
        let current_layout = self.generate_canonical_layout()?;

        if current_layout.layout_hash != expected_layout.layout_hash {
            return Err(MemoryWatchdogError::MemoryLayoutMismatch {
                expected: format!("{:?}", expected_layout.layout_hash),
                actual: format!("{:?}", current_layout.layout_hash),
            });
        }

        Ok(())
    }
}

/// Allocation statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllocationStats {
    pub total_allocations: usize,
    pub active_allocations: usize,
    pub reused_pointers: usize,
    pub total_reuses: u32,
    pub reuse_rate: f32,
}

/// Get current timestamp in microseconds
fn current_timestamp() -> u128 {
    adapteros_core::time::unix_timestamp_micros_u128()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pointer_canonicalizer_creation() {
        let canonicalizer = PointerCanonicalizer::new(1000);
        let stats = canonicalizer.get_allocation_stats();

        assert_eq!(stats.total_allocations, 0);
        assert_eq!(stats.active_allocations, 0);
        assert_eq!(stats.reused_pointers, 0);
    }

    #[test]
    fn test_pointer_allocation_recording() {
        let canonicalizer = PointerCanonicalizer::new(1000);

        let allocation_id = canonicalizer
            .record_allocation(0x1000, 1024, "test".to_string())
            .unwrap();

        let stats = canonicalizer.get_allocation_stats();
        assert_eq!(stats.total_allocations, 1);
        assert_eq!(stats.active_allocations, 1);
        assert_eq!(stats.reused_pointers, 0);

        assert!(!allocation_id.is_nil());
    }

    #[test]
    fn test_pointer_reuse_detection() {
        let canonicalizer = PointerCanonicalizer::new(1000);

        // First allocation
        let _allocation_id1 = canonicalizer
            .record_allocation(0x1000, 1024, "test1".to_string())
            .unwrap();

        // Deallocate
        canonicalizer.record_deallocation(0x1000).unwrap();

        // Second allocation at same address (reuse)
        let _allocation_id2 = canonicalizer
            .record_allocation(0x1000, 2048, "test2".to_string())
            .unwrap();

        let stats = canonicalizer.get_allocation_stats();
        assert_eq!(stats.total_allocations, 2);
        assert_eq!(stats.reused_pointers, 1);
        assert_eq!(stats.total_reuses, 1);
    }

    #[test]
    fn test_canonical_layout_generation() {
        let canonicalizer = PointerCanonicalizer::new(1000);

        // Record some allocations
        let _allocation_id1 = canonicalizer
            .record_allocation(0x1000, 1024, "test1".to_string())
            .unwrap();

        let _allocation_id2 = canonicalizer
            .record_allocation(0x2000, 2048, "test2".to_string())
            .unwrap();

        let layout = canonicalizer.generate_canonical_layout().unwrap();

        assert_eq!(layout.total_allocations, 2);
        assert_eq!(layout.total_reused, 0);
        assert!(!layout.layout_id.is_nil());
    }

    #[test]
    fn test_layout_consistency_verification() {
        let canonicalizer = PointerCanonicalizer::new(1000);

        // Generate initial layout
        let initial_layout = canonicalizer.generate_canonical_layout().unwrap();

        // Verify consistency
        canonicalizer
            .verify_layout_consistency(&initial_layout)
            .unwrap();

        // Record allocations that will create reuse patterns
        let _allocation_id1 = canonicalizer
            .record_allocation(0x1000, 1024, "test1".to_string())
            .unwrap();

        let _allocation_id2 = canonicalizer
            .record_allocation(0x2000, 2048, "test2".to_string())
            .unwrap();

        // Record a reuse to create a pattern
        let _allocation_id3 = canonicalizer
            .record_allocation(0x1000, 1024, "test1_reuse".to_string())
            .unwrap();

        // Generate new layout after allocations
        let new_layout = canonicalizer.generate_canonical_layout().unwrap();

        // Layouts should be different due to reuse patterns
        assert_ne!(initial_layout.layout_hash, new_layout.layout_hash);

        // Verification should fail with old layout
        let result = canonicalizer.verify_layout_consistency(&initial_layout);
        assert!(result.is_err());

        // But should pass with new layout
        canonicalizer
            .verify_layout_consistency(&new_layout)
            .unwrap();
    }

    #[test]
    fn test_pointer_canonicalization() {
        let canonicalizer = PointerCanonicalizer::new(1000);

        // Test that same logical order produces same canonical address
        let canonical1 = canonicalizer.canonicalize_pointer(0x1000, 1);
        let canonical2 = canonicalizer.canonicalize_pointer(0x2000, 1);

        // Should be same for same logical order (canonicalization ignores actual address)
        assert_eq!(canonical1, canonical2);

        // But different for different logical orders
        let canonical3 = canonicalizer.canonicalize_pointer(0x3000, 2);
        assert_ne!(canonical1, canonical3);
    }
}
