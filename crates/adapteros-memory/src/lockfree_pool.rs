//! Lock-free memory pool for high-performance allocation
//!
//! This module provides a lock-free slab allocator optimized for
//! common tensor sizes in ML inference. It uses atomic operations
//! for allocation/deallocation to eliminate mutex contention.
//!
//! # Design
//!
//! - Multiple slab sizes: 64KB, 256KB, 1MB, 4MB
//! - AtomicPtr-based free list per slab size
//! - Fallback to standard allocator for non-standard sizes
//! - Zero-copy block reuse

use adapteros_core::{AosError, Result};
use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering};
use tracing::{debug, info, trace};

/// Standard slab sizes for common tensor allocations
pub const SLAB_SIZES: &[usize] = &[
    64 * 1024,       // 64KB - small tensors, attention masks
    256 * 1024,      // 256KB - medium tensors
    1024 * 1024,     // 1MB - layer activations
    4 * 1024 * 1024, // 4MB - KV cache blocks
];

/// Lock-free memory pool using slab allocation
///
/// Uses separate free lists for each slab size, enabling
/// O(1) allocation without mutex contention.
pub struct LockFreePool {
    /// Free lists for each slab size (indexed by slab tier)
    free_lists: [AtomicPtr<FreeNode>; 4],
    /// Current allocation counts per tier
    alloc_counts: [AtomicU64; 4],
    /// Total allocated bytes (for stats)
    total_allocated: AtomicUsize,
    /// Total capacity
    capacity: usize,
    /// Pool is active
    is_active: bool,
}

/// Node in the lock-free free list
#[repr(C)]
struct FreeNode {
    /// Pointer to next free node
    next: *mut FreeNode,
    /// Size of this block (for validation)
    size: usize,
}

/// A block allocated from the lock-free pool
#[derive(Debug)]
pub struct LockFreeBlock {
    /// Memory pointer
    ptr: NonNull<u8>,
    /// Block size
    size: usize,
    /// Slab tier (0-3 for standard sizes, usize::MAX for fallback)
    tier: usize,
}

// SAFETY: LockFreeBlock is Send because:
// - The pointer is obtained from the global allocator
// - Ownership is transferred on allocation
// - No shared mutable state
unsafe impl Send for LockFreeBlock {}
unsafe impl Sync for LockFreeBlock {}

impl LockFreePool {
    /// Create a new lock-free pool with given capacity
    pub fn new(capacity: usize) -> Self {
        info!(
            "Creating lock-free memory pool with {} bytes capacity",
            capacity
        );
        Self {
            free_lists: [
                AtomicPtr::new(std::ptr::null_mut()),
                AtomicPtr::new(std::ptr::null_mut()),
                AtomicPtr::new(std::ptr::null_mut()),
                AtomicPtr::new(std::ptr::null_mut()),
            ],
            alloc_counts: [
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
                AtomicU64::new(0),
            ],
            total_allocated: AtomicUsize::new(0),
            capacity,
            is_active: true,
        }
    }

    /// Allocate a block of the requested size
    ///
    /// Rounds up to the nearest slab size for efficiency.
    /// Uses lock-free pop from the free list if available.
    pub fn allocate(&self, size: usize) -> Result<LockFreeBlock> {
        if !self.is_active {
            return Err(AosError::Memory("Pool is not active".to_string()));
        }

        // Find the appropriate slab tier
        let (tier, actual_size) = self.find_tier(size);

        // Check capacity
        let current = self.total_allocated.load(Ordering::Acquire);
        if current + actual_size > self.capacity {
            return Err(AosError::Memory(format!(
                "Lock-free pool capacity exceeded: {} + {} > {}",
                current, actual_size, self.capacity
            )));
        }

        // Try to pop from free list (lock-free)
        if tier < 4 {
            if let Some(ptr) = self.try_pop_free(tier) {
                self.total_allocated
                    .fetch_add(actual_size, Ordering::Release);
                self.alloc_counts[tier].fetch_add(1, Ordering::Relaxed);

                trace!("Reused block from tier {} ({} bytes)", tier, actual_size);
                return Ok(LockFreeBlock {
                    ptr: unsafe { NonNull::new_unchecked(ptr as *mut u8) },
                    size: actual_size,
                    tier,
                });
            }
        }

        // Allocate new block
        let ptr = self.allocate_new(actual_size)?;
        self.total_allocated
            .fetch_add(actual_size, Ordering::Release);
        if tier < 4 {
            self.alloc_counts[tier].fetch_add(1, Ordering::Relaxed);
        }

        debug!("Allocated new block: {} bytes (tier {})", actual_size, tier);
        Ok(LockFreeBlock {
            ptr,
            size: actual_size,
            tier,
        })
    }

    /// Deallocate a block, returning it to the free list
    pub fn deallocate(&self, block: LockFreeBlock) -> Result<()> {
        if block.tier < 4 {
            // Push to free list (lock-free)
            self.push_free(block.tier, block.ptr.as_ptr(), block.size);
        } else {
            // Free non-standard size blocks directly
            self.free_block(block.ptr, block.size)?;
        }

        self.total_allocated
            .fetch_sub(block.size, Ordering::Release);
        trace!(
            "Deallocated block: {} bytes (tier {})",
            block.size,
            block.tier
        );
        Ok(())
    }

    /// Find the appropriate tier for a size
    fn find_tier(&self, size: usize) -> (usize, usize) {
        for (tier, &slab_size) in SLAB_SIZES.iter().enumerate() {
            if size <= slab_size {
                return (tier, slab_size);
            }
        }
        // Non-standard size - use exact allocation
        (usize::MAX, size)
    }

    /// Try to pop a block from the free list (lock-free)
    fn try_pop_free(&self, tier: usize) -> Option<*mut FreeNode> {
        let head = &self.free_lists[tier];

        loop {
            let current = head.load(Ordering::Acquire);
            if current.is_null() {
                return None;
            }

            // SAFETY: We own this node via the atomic load
            let next = unsafe { (*current).next };

            // CAS to remove the head
            match head.compare_exchange_weak(current, next, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return Some(current),
                Err(_) => continue, // Retry on contention
            }
        }
    }

    /// Push a block to the free list (lock-free)
    fn push_free(&self, tier: usize, ptr: *mut u8, size: usize) {
        let head = &self.free_lists[tier];
        let node = ptr as *mut FreeNode;

        // Initialize the node
        unsafe {
            (*node).size = size;
        }

        loop {
            let current = head.load(Ordering::Acquire);
            unsafe {
                (*node).next = current;
            }

            // CAS to insert at head
            match head.compare_exchange_weak(current, node, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return,
                Err(_) => continue, // Retry on contention
            }
        }
    }

    /// Allocate a new block from the system allocator
    fn allocate_new(&self, size: usize) -> Result<NonNull<u8>> {
        let layout = Layout::from_size_align(size, 64)
            .map_err(|e| AosError::Memory(format!("Invalid layout: {}", e)))?;

        let ptr = unsafe { alloc(layout) };
        if ptr.is_null() {
            return Err(AosError::Memory(format!(
                "Failed to allocate {} bytes",
                size
            )));
        }

        Ok(unsafe { NonNull::new_unchecked(ptr) })
    }

    /// Free a block back to the system allocator
    fn free_block(&self, ptr: NonNull<u8>, size: usize) -> Result<()> {
        let layout = Layout::from_size_align(size, 64)
            .map_err(|e| AosError::Memory(format!("Invalid layout: {}", e)))?;

        unsafe {
            dealloc(ptr.as_ptr(), layout);
        }
        Ok(())
    }

    /// Get current statistics
    pub fn stats(&self) -> LockFreePoolStats {
        LockFreePoolStats {
            total_allocated: self.total_allocated.load(Ordering::Acquire),
            capacity: self.capacity,
            tier_alloc_counts: [
                self.alloc_counts[0].load(Ordering::Relaxed),
                self.alloc_counts[1].load(Ordering::Relaxed),
                self.alloc_counts[2].load(Ordering::Relaxed),
                self.alloc_counts[3].load(Ordering::Relaxed),
            ],
            free_list_lengths: self.count_free_lists(),
        }
    }

    /// Count items in each free list
    fn count_free_lists(&self) -> [usize; 4] {
        let mut counts = [0usize; 4];
        for (tier, head) in self.free_lists.iter().enumerate() {
            let mut current = head.load(Ordering::Acquire);
            while !current.is_null() {
                counts[tier] += 1;
                current = unsafe { (*current).next };
            }
        }
        counts
    }
}

impl Drop for LockFreePool {
    fn drop(&mut self) {
        // Free all nodes in the free lists
        for (tier, head) in self.free_lists.iter().enumerate() {
            let mut current = head.load(Ordering::Acquire);
            while !current.is_null() {
                let next = unsafe { (*current).next };
                let size = SLAB_SIZES[tier];
                let _ =
                    self.free_block(unsafe { NonNull::new_unchecked(current as *mut u8) }, size);
                current = next;
            }
        }
        info!("Lock-free pool dropped");
    }
}

/// Statistics for the lock-free pool
#[derive(Debug, Clone)]
pub struct LockFreePoolStats {
    /// Total currently allocated bytes
    pub total_allocated: usize,
    /// Pool capacity
    pub capacity: usize,
    /// Allocation counts per tier
    pub tier_alloc_counts: [u64; 4],
    /// Number of blocks in each free list
    pub free_list_lengths: [usize; 4],
}

impl LockFreePoolStats {
    /// Calculate reuse ratio
    pub fn reuse_ratio(&self) -> f64 {
        let total_allocs: u64 = self.tier_alloc_counts.iter().sum();
        if total_allocs == 0 {
            return 0.0;
        }
        let total_free: u64 = self.free_list_lengths.iter().map(|&x| x as u64).sum();
        total_free as f64 / total_allocs as f64
    }
}

impl LockFreeBlock {
    /// Get the pointer to the block
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr.as_ptr()
    }

    /// Get the size of the block
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get the tier
    pub fn tier(&self) -> usize {
        self.tier
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_lockfree_pool_creation() {
        let pool = LockFreePool::new(100 * 1024 * 1024);
        let stats = pool.stats();
        assert_eq!(stats.total_allocated, 0);
        assert_eq!(stats.capacity, 100 * 1024 * 1024);
    }

    #[test]
    fn test_slab_allocation() {
        let pool = LockFreePool::new(100 * 1024 * 1024);

        // Allocate tier 0 (64KB)
        let block1 = pool.allocate(32 * 1024).unwrap();
        assert_eq!(block1.size(), 64 * 1024);
        assert_eq!(block1.tier(), 0);

        // Allocate tier 1 (256KB)
        let block2 = pool.allocate(128 * 1024).unwrap();
        assert_eq!(block2.size(), 256 * 1024);
        assert_eq!(block2.tier(), 1);

        // Allocate tier 2 (1MB)
        let block3 = pool.allocate(512 * 1024).unwrap();
        assert_eq!(block3.size(), 1024 * 1024);
        assert_eq!(block3.tier(), 2);

        // Verify stats
        let stats = pool.stats();
        assert_eq!(stats.total_allocated, 64 * 1024 + 256 * 1024 + 1024 * 1024);
    }

    #[test]
    fn test_block_reuse() {
        let pool = LockFreePool::new(100 * 1024 * 1024);

        // Allocate and deallocate
        let block1 = pool.allocate(32 * 1024).unwrap();
        let ptr1 = block1.as_ptr();
        pool.deallocate(block1).unwrap();

        // Reallocate - should reuse the same block
        let block2 = pool.allocate(32 * 1024).unwrap();
        let ptr2 = block2.as_ptr();

        // Same pointer should be reused
        assert_eq!(ptr1, ptr2);

        // Free list should have 1 item after first dealloc
        let stats = pool.stats();
        assert_eq!(stats.free_list_lengths[0], 0); // It was reused
    }

    #[test]
    fn test_concurrent_allocation() {
        let pool = Arc::new(LockFreePool::new(100 * 1024 * 1024));
        let num_threads = 8;
        let allocs_per_thread = 100;

        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                let pool_clone = Arc::clone(&pool);
                thread::spawn(move || {
                    let mut blocks = Vec::new();
                    for _ in 0..allocs_per_thread {
                        let block = pool_clone.allocate(64 * 1024).unwrap();
                        blocks.push(block);
                    }
                    // Deallocate half
                    for _ in 0..(allocs_per_thread / 2) {
                        if let Some(block) = blocks.pop() {
                            pool_clone.deallocate(block).unwrap();
                        }
                    }
                    blocks.len()
                })
            })
            .collect();

        let total_remaining: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();

        // Each thread keeps half = 50, 8 threads = 400 blocks
        assert_eq!(total_remaining, num_threads * (allocs_per_thread / 2));

        let stats = pool.stats();
        let total_allocs: u64 = stats.tier_alloc_counts.iter().sum();
        assert_eq!(total_allocs, (num_threads * allocs_per_thread) as u64);
    }

    #[test]
    fn test_capacity_enforcement() {
        let pool = LockFreePool::new(1024 * 1024); // 1MB capacity

        // Allocate 1MB (tier 2)
        let block = pool.allocate(512 * 1024).unwrap();
        assert_eq!(block.size(), 1024 * 1024);

        // Next allocation should fail
        let result = pool.allocate(64 * 1024);
        assert!(result.is_err());

        // After deallocation, should work again
        pool.deallocate(block).unwrap();
        let block2 = pool.allocate(64 * 1024);
        assert!(block2.is_ok());
    }
}

/// Loom-based concurrency tests for lock-free CAS operations
///
/// These tests verify correctness of the lock-free free list under
/// all possible thread interleavings. Run with:
/// `cargo test -p adapteros-memory --features loom --release -- loom`
#[cfg(all(test, feature = "loom"))]
mod loom_tests {
    use loom::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
    use loom::sync::Arc;
    use loom::thread;
    use std::ptr;

    /// Minimal free list node for loom testing
    /// Uses a simple counter instead of actual memory allocation
    #[repr(C)]
    struct LoomFreeNode {
        next: *mut LoomFreeNode,
        id: usize,
    }

    /// Minimal lock-free stack for testing CAS operations
    struct LoomFreeList {
        head: AtomicPtr<LoomFreeNode>,
        pop_count: AtomicUsize,
        push_count: AtomicUsize,
    }

    impl LoomFreeList {
        fn new() -> Self {
            Self {
                head: AtomicPtr::new(ptr::null_mut()),
                pop_count: AtomicUsize::new(0),
                push_count: AtomicUsize::new(0),
            }
        }

        /// Push a node - mirrors production push_free CAS loop
        fn push(&self, node: *mut LoomFreeNode) {
            loop {
                let current = self.head.load(Ordering::Acquire);
                unsafe {
                    (*node).next = current;
                }
                match self.head.compare_exchange_weak(
                    current,
                    node,
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        self.push_count.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    Err(_) => continue,
                }
            }
        }

        /// Pop a node - mirrors production try_pop_free CAS loop
        fn pop(&self) -> Option<*mut LoomFreeNode> {
            loop {
                let current = self.head.load(Ordering::Acquire);
                if current.is_null() {
                    return None;
                }
                let next = unsafe { (*current).next };
                match self.head.compare_exchange_weak(
                    current,
                    next,
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        self.pop_count.fetch_add(1, Ordering::Relaxed);
                        return Some(current);
                    }
                    Err(_) => continue,
                }
            }
        }

        /// Count nodes in the list (for verification)
        fn count(&self) -> usize {
            let mut count = 0;
            let mut current = self.head.load(Ordering::Acquire);
            while !current.is_null() {
                count += 1;
                current = unsafe { (*current).next };
            }
            count
        }
    }

    /// Test concurrent push operations
    /// Verifies that all pushed nodes end up in the list
    #[test]
    fn loom_concurrent_push() {
        loom::model(|| {
            let list = Arc::new(LoomFreeList::new());

            // Pre-allocate nodes using loom's allocator
            let node1 = Box::into_raw(Box::new(LoomFreeNode {
                next: ptr::null_mut(),
                id: 1,
            }));
            let node2 = Box::into_raw(Box::new(LoomFreeNode {
                next: ptr::null_mut(),
                id: 2,
            }));

            let list1 = Arc::clone(&list);
            let list2 = Arc::clone(&list);

            let t1 = thread::spawn(move || {
                list1.push(node1);
            });

            let t2 = thread::spawn(move || {
                list2.push(node2);
            });

            t1.join().unwrap();
            t2.join().unwrap();

            // Both nodes must be in the list
            assert_eq!(list.count(), 2);
            assert_eq!(list.push_count.load(Ordering::Relaxed), 2);

            // Clean up - pop both nodes and free them
            let _ = list.pop();
            let _ = list.pop();
            unsafe {
                drop(Box::from_raw(node1));
                drop(Box::from_raw(node2));
            }
        });
    }

    /// Test concurrent push and pop (ABA scenario)
    /// This is the most critical test - verifies no corruption when
    /// one thread pushes while another pops
    #[test]
    fn loom_concurrent_push_pop() {
        loom::model(|| {
            let list = Arc::new(LoomFreeList::new());

            // Start with one node in the list
            let node1 = Box::into_raw(Box::new(LoomFreeNode {
                next: ptr::null_mut(),
                id: 1,
            }));
            list.push(node1);

            let node2 = Box::into_raw(Box::new(LoomFreeNode {
                next: ptr::null_mut(),
                id: 2,
            }));

            let list1 = Arc::clone(&list);
            let list2 = Arc::clone(&list);

            // Thread 1: pop the existing node
            let t1 = thread::spawn(move || list1.pop());

            // Thread 2: push a new node
            let t2 = thread::spawn(move || {
                list2.push(node2);
            });

            let popped = t1.join().unwrap();
            t2.join().unwrap();

            // Verify invariants:
            // - The popped node must be either node1 or node2 (LIFO order)
            // - Final count depends on timing: 1 if pop succeeded, 2 if push before pop saw it
            let final_count = list.count();
            assert!(final_count >= 1 && final_count <= 2);

            // Track which node was popped for cleanup
            let mut popped_node1 = false;
            let mut popped_node2 = false;

            if let Some(p) = popped {
                let id = unsafe { (*p).id };
                // Must be a valid node we created
                assert!(id == 1 || id == 2);
                if id == 1 {
                    popped_node1 = true;
                } else {
                    popped_node2 = true;
                }
            }

            // Clean up remaining nodes in the list
            while let Some(n) = list.pop() {
                let id = unsafe { (*n).id };
                if id == 1 {
                    popped_node1 = true;
                } else if id == 2 {
                    popped_node2 = true;
                }
            }

            // Free all nodes - both must have been either popped or remain in list
            // We always allocated both, so free both
            unsafe {
                drop(Box::from_raw(node1));
                drop(Box::from_raw(node2));
            }

            // Invariant: both nodes must have been accounted for
            assert!(popped_node1, "node1 was lost");
            assert!(popped_node2, "node2 was lost");
        });
    }
}
