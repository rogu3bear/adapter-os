//! Threadgroup memory planner for Metal 3.x
//!
//! Metal 3.x increased the available threadgroup memory. This planner
//! provides deterministic allocation so that kernels can pre-compute
//! offsets and avoid dynamic branching when binding buffers.

use adapteros_core::{AosError, Result};

/// Allocation record for a threadgroup memory region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadgroupAllocation {
    pub label: String,
    pub size: usize,
    pub alignment: usize,
    pub offset: usize,
}

/// Planner responsible for carving up threadgroup memory.
#[derive(Debug, Clone)]
pub struct ThreadgroupMemoryPlanner {
    capacity: usize,
    used: usize,
    allocations: Vec<ThreadgroupAllocation>,
}

impl ThreadgroupMemoryPlanner {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            used: 0,
            allocations: Vec::new(),
        }
    }

    /// Allocate memory with alignment. Returns the deterministic offset
    /// that should be used when binding the buffer.
    pub fn allocate(
        &mut self,
        label: impl Into<String>,
        size: usize,
        alignment: usize,
    ) -> Result<ThreadgroupAllocation> {
        if size == 0 {
            return Err(AosError::Kernel(
                "Threadgroup allocation size must be > 0".into(),
            ));
        }
        if alignment.count_ones() != 1 {
            return Err(AosError::Kernel(
                "Threadgroup alignment must be a power of two".into(),
            ));
        }

        let alignment = alignment.max(1);
        let aligned_offset = (self.used + alignment - 1) & !(alignment - 1);
        let end = aligned_offset
            .checked_add(size)
            .ok_or_else(|| AosError::Kernel("Threadgroup allocation overflowed usize".into()))?;

        if end > self.capacity {
            return Err(AosError::Kernel(format!(
                "Threadgroup memory capacity exceeded: requested {} bytes (used {} / {})",
                size, self.used, self.capacity
            )));
        }

        let allocation = ThreadgroupAllocation {
            label: label.into(),
            size,
            alignment,
            offset: aligned_offset,
        };
        self.used = end;
        self.allocations.push(allocation.clone());
        Ok(allocation)
    }

    /// Reset the planner so new kernels can reuse the memory layout.
    pub fn reset(&mut self) {
        self.used = 0;
        self.allocations.clear();
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn used(&self) -> usize {
        self.used
    }

    pub fn allocations(&self) -> &[ThreadgroupAllocation] {
        &self.allocations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planner_computes_aligned_offsets() {
        let mut planner = ThreadgroupMemoryPlanner::new(1024);
        let first = planner
            .allocate("scratch", 128, 64)
            .expect("should allocate");
        assert_eq!(first.offset % 64, 0);
        let second = planner
            .allocate("residual", 64, 32)
            .expect("should allocate");
        assert!(second.offset >= first.offset + first.size);
        assert_eq!(second.offset % 32, 0);
    }

    #[test]
    fn planner_enforces_capacity() {
        let mut planner = ThreadgroupMemoryPlanner::new(128);
        planner.allocate("small", 64, 16).expect("should allocate");
        let err = planner.allocate("large", 128, 16).unwrap_err();
        assert!(matches!(err, AosError::Kernel(_)));
    }
}
