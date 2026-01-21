//! Tiered memory manager with automatic migration
//!
//! This module provides a tiered memory system that automatically
//! migrates data between GPU, Unified, and CPU tiers based on
//! access patterns.
//!
//! # Tiers
//!
//! - **Hot (GPU)**: Actively used tensors with recent access
//! - **Warm (Unified)**: Moderately used, shared CPU/GPU
//! - **Cold (CPU)**: Infrequently accessed, CPU-only storage
//!
//! # Migration Policy
//!
//! Blocks are promoted on access and demoted after a configurable
//! idle timeout. This maximizes GPU memory for hot data while
//! offloading cold data to cheaper storage.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

/// Memory tiers ordered by access speed
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum MemoryTier {
    /// GPU memory - fastest, most limited
    Hot = 0,
    /// Unified memory - shared CPU/GPU
    Warm = 1,
    /// CPU memory - slowest, most abundant
    Cold = 2,
}

impl MemoryTier {
    /// Get the next colder tier
    pub fn demote(&self) -> Option<MemoryTier> {
        match self {
            MemoryTier::Hot => Some(MemoryTier::Warm),
            MemoryTier::Warm => Some(MemoryTier::Cold),
            MemoryTier::Cold => None,
        }
    }

    /// Get the next hotter tier
    pub fn promote(&self) -> Option<MemoryTier> {
        match self {
            MemoryTier::Hot => None,
            MemoryTier::Warm => Some(MemoryTier::Hot),
            MemoryTier::Cold => Some(MemoryTier::Warm),
        }
    }
}

/// Configuration for tiered memory management
#[derive(Debug, Clone)]
pub struct TieredConfig {
    /// Capacity per tier (bytes)
    pub tier_capacities: [usize; 3],
    /// Time before demoting to next tier
    pub demotion_timeout: Duration,
    /// Minimum access count to stay in tier
    pub min_access_count: u64,
    /// Enable automatic migration
    pub auto_migrate: bool,
}

impl Default for TieredConfig {
    fn default() -> Self {
        Self {
            tier_capacities: [
                4 * 1024 * 1024 * 1024,  // 4GB GPU
                8 * 1024 * 1024 * 1024,  // 8GB Unified
                32 * 1024 * 1024 * 1024, // 32GB CPU
            ],
            demotion_timeout: Duration::from_secs(60),
            min_access_count: 5,
            auto_migrate: true,
        }
    }
}

/// A block in the tiered memory system
#[derive(Debug)]
pub struct TieredBlock {
    /// Block identifier
    pub id: String,
    /// Current tier
    tier: MemoryTier,
    /// Memory pointer
    ptr: *mut u8,
    /// Block size
    size: usize,
    /// Access count
    access_count: AtomicU64,
    /// Last access time
    last_access: Mutex<Instant>,
    /// Creation time
    created_at: Instant,
}

// SAFETY: TieredBlock is Send/Sync because:
// - ptr is stable after allocation
// - atomic/mutex guards protect mutable state
unsafe impl Send for TieredBlock {}
unsafe impl Sync for TieredBlock {}

impl TieredBlock {
    /// Record an access to this block
    pub fn record_access(&self) {
        self.access_count.fetch_add(1, Ordering::Relaxed);
        *self.last_access.lock().unwrap() = Instant::now();
    }

    /// Get access count
    pub fn access_count(&self) -> u64 {
        self.access_count.load(Ordering::Relaxed)
    }

    /// Get time since last access
    pub fn idle_time(&self) -> Duration {
        self.last_access.lock().unwrap().elapsed()
    }

    /// Get current tier
    pub fn tier(&self) -> MemoryTier {
        self.tier
    }

    /// Get block size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get block ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get raw pointer
    pub fn as_ptr(&self) -> *mut u8 {
        self.ptr
    }
}

/// Tiered memory manager with automatic migration
pub struct TieredMemoryManager {
    /// Blocks indexed by ID
    blocks: RwLock<HashMap<String, Arc<TieredBlock>>>,
    /// Per-tier usage tracking
    tier_usage: [AtomicU64; 3],
    /// Configuration
    config: TieredConfig,
    /// Migration statistics
    migrations: AtomicU64,
    /// Block counter for unique IDs
    block_counter: AtomicU64,
}

impl TieredMemoryManager {
    /// Create a new tiered memory manager
    pub fn new(config: TieredConfig) -> Self {
        info!(
            "Creating TieredMemoryManager: Hot={}MB, Warm={}MB, Cold={}MB",
            config.tier_capacities[0] / (1024 * 1024),
            config.tier_capacities[1] / (1024 * 1024),
            config.tier_capacities[2] / (1024 * 1024)
        );
        Self {
            blocks: RwLock::new(HashMap::new()),
            tier_usage: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
            config,
            migrations: AtomicU64::new(0),
            block_counter: AtomicU64::new(0),
        }
    }

    /// Allocate a block in the specified tier
    pub fn allocate(&self, size: usize, preferred_tier: MemoryTier) -> Result<Arc<TieredBlock>> {
        // Try preferred tier first, then fall back to colder tiers
        let tier = self.find_available_tier(size, preferred_tier)?;

        let ptr = self.allocate_raw(size)?;
        let id = format!(
            "tiered_{}",
            self.block_counter.fetch_add(1, Ordering::SeqCst)
        );

        let block = Arc::new(TieredBlock {
            id: id.clone(),
            tier,
            ptr,
            size,
            access_count: AtomicU64::new(0),
            last_access: Mutex::new(Instant::now()),
            created_at: Instant::now(),
        });

        // Track usage
        self.tier_usage[tier as usize].fetch_add(size as u64, Ordering::Release);

        // Store block
        {
            let mut blocks = self.blocks.write().unwrap();
            blocks.insert(id.clone(), Arc::clone(&block));
        }

        debug!("Allocated {} bytes in {:?} tier (id: {})", size, tier, id);

        Ok(block)
    }

    /// Deallocate a block
    pub fn deallocate(&self, block: &Arc<TieredBlock>) -> Result<()> {
        let mut blocks = self.blocks.write().unwrap();
        if blocks.remove(&block.id).is_some() {
            self.tier_usage[block.tier as usize].fetch_sub(block.size as u64, Ordering::Release);
            self.deallocate_raw(block.ptr, block.size)?;
            debug!("Deallocated block {} ({} bytes)", block.id, block.size);
        }
        Ok(())
    }

    /// Find an available tier for allocation
    fn find_available_tier(&self, size: usize, preferred: MemoryTier) -> Result<MemoryTier> {
        let check_tier = |tier: MemoryTier| -> bool {
            let usage = self.tier_usage[tier as usize].load(Ordering::Acquire);
            let capacity = self.config.tier_capacities[tier as usize];
            (usage as usize) + size <= capacity
        };

        // Try preferred tier
        if check_tier(preferred) {
            return Ok(preferred);
        }

        // Fall back to colder tiers
        let mut tier = preferred;
        while let Some(colder) = tier.demote() {
            if check_tier(colder) {
                return Ok(colder);
            }
            tier = colder;
        }

        Err(AosError::Memory(format!(
            "No tier has {} bytes available",
            size
        )))
    }

    /// Migrate a block to a different tier
    pub fn migrate(
        &self,
        block: &Arc<TieredBlock>,
        target_tier: MemoryTier,
    ) -> Result<Arc<TieredBlock>> {
        if block.tier == target_tier {
            return Ok(Arc::clone(block));
        }

        // Check capacity in target tier
        let current_usage = self.tier_usage[target_tier as usize].load(Ordering::Acquire);
        let capacity = self.config.tier_capacities[target_tier as usize];
        if (current_usage as usize) + block.size > capacity {
            return Err(AosError::Memory(format!(
                "Not enough space in {:?} tier for migration",
                target_tier
            )));
        }

        // Allocate in new tier
        let new_ptr = self.allocate_raw(block.size)?;

        // Copy data
        unsafe {
            std::ptr::copy_nonoverlapping(block.ptr, new_ptr, block.size);
        }

        // Create new block
        let new_block = Arc::new(TieredBlock {
            id: block.id.clone(),
            tier: target_tier,
            ptr: new_ptr,
            size: block.size,
            access_count: AtomicU64::new(block.access_count()),
            last_access: Mutex::new(*block.last_access.lock().unwrap()),
            created_at: block.created_at,
        });

        // Update tracking
        self.tier_usage[block.tier as usize].fetch_sub(block.size as u64, Ordering::Release);
        self.tier_usage[target_tier as usize].fetch_add(block.size as u64, Ordering::Release);

        // Update registry
        {
            let mut blocks = self.blocks.write().unwrap();
            blocks.insert(block.id.clone(), Arc::clone(&new_block));
        }

        // Free old memory
        self.deallocate_raw(block.ptr, block.size)?;

        self.migrations.fetch_add(1, Ordering::Relaxed);
        info!(
            "Migrated block {} from {:?} to {:?} ({} bytes)",
            block.id, block.tier, target_tier, block.size
        );

        Ok(new_block)
    }

    /// Run migration pass - demote cold blocks
    pub fn run_migration_pass(&self) -> MigrationStats {
        let mut demoted = 0;
        let mut promoted = 0;
        let mut bytes_moved = 0usize;

        let blocks: Vec<Arc<TieredBlock>> = {
            let blocks = self.blocks.read().unwrap();
            blocks.values().cloned().collect()
        };

        for block in blocks {
            let idle_time = block.idle_time();
            let access_count = block.access_count();

            // Check for demotion
            if idle_time > self.config.demotion_timeout
                && access_count < self.config.min_access_count
            {
                if let Some(target) = block.tier.demote() {
                    if let Ok(_new_block) = self.migrate(&block, target) {
                        demoted += 1;
                        bytes_moved += block.size;
                    }
                }
            }

            // Check for promotion (high access in cold tier)
            if block.tier == MemoryTier::Cold && access_count > self.config.min_access_count * 2 {
                if let Some(target) = block.tier.promote() {
                    if let Ok(_new_block) = self.migrate(&block, target) {
                        promoted += 1;
                        bytes_moved += block.size;
                    }
                }
            }
        }

        if demoted > 0 || promoted > 0 {
            debug!(
                "Migration pass: {} demoted, {} promoted, {} bytes moved",
                demoted, promoted, bytes_moved
            );
        }

        MigrationStats {
            demoted,
            promoted,
            bytes_moved,
        }
    }

    /// Get current statistics
    pub fn stats(&self) -> TieredStats {
        let blocks = self.blocks.read().unwrap();
        TieredStats {
            block_count: blocks.len(),
            tier_usage: [
                self.tier_usage[0].load(Ordering::Acquire),
                self.tier_usage[1].load(Ordering::Acquire),
                self.tier_usage[2].load(Ordering::Acquire),
            ],
            tier_capacities: self.config.tier_capacities.map(|c| c as u64),
            total_migrations: self.migrations.load(Ordering::Acquire),
        }
    }

    /// Get a block by ID
    pub fn get_block(&self, id: &str) -> Option<Arc<TieredBlock>> {
        let blocks = self.blocks.read().unwrap();
        blocks.get(id).cloned()
    }

    /// Allocate raw memory
    fn allocate_raw(&self, size: usize) -> Result<*mut u8> {
        let layout = std::alloc::Layout::from_size_align(size, 64)
            .map_err(|e| AosError::Memory(format!("Invalid layout: {}", e)))?;

        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err(AosError::Memory(format!(
                "Failed to allocate {} bytes",
                size
            )));
        }
        Ok(ptr)
    }

    /// Deallocate raw memory
    fn deallocate_raw(&self, ptr: *mut u8, size: usize) -> Result<()> {
        let layout = std::alloc::Layout::from_size_align(size, 64)
            .map_err(|e| AosError::Memory(format!("Invalid layout: {}", e)))?;

        unsafe {
            std::alloc::dealloc(ptr, layout);
        }
        Ok(())
    }
}

/// Statistics from a tiered memory system
#[derive(Debug, Clone)]
pub struct TieredStats {
    /// Number of active blocks
    pub block_count: usize,
    /// Usage per tier (bytes)
    pub tier_usage: [u64; 3],
    /// Capacity per tier (bytes)
    pub tier_capacities: [u64; 3],
    /// Total migrations performed
    pub total_migrations: u64,
}

impl TieredStats {
    /// Calculate utilization for a tier
    pub fn tier_utilization(&self, tier: MemoryTier) -> f64 {
        let idx = tier as usize;
        if self.tier_capacities[idx] == 0 {
            return 0.0;
        }
        self.tier_usage[idx] as f64 / self.tier_capacities[idx] as f64
    }

    /// Get effective capacity (weighted by tier speed)
    pub fn effective_capacity(&self) -> u64 {
        // Hot = 3x, Warm = 2x, Cold = 1x weighting
        self.tier_capacities[0] * 3 + self.tier_capacities[1] * 2 + self.tier_capacities[2]
    }
}

/// Statistics from a migration pass
#[derive(Debug, Clone, Default)]
pub struct MigrationStats {
    /// Blocks demoted to colder tier
    pub demoted: usize,
    /// Blocks promoted to hotter tier
    pub promoted: usize,
    /// Total bytes moved
    pub bytes_moved: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> TieredConfig {
        TieredConfig {
            tier_capacities: [
                10 * 1024 * 1024,  // 10MB GPU
                20 * 1024 * 1024,  // 20MB Unified
                100 * 1024 * 1024, // 100MB CPU
            ],
            demotion_timeout: Duration::from_millis(100),
            min_access_count: 3,
            auto_migrate: true,
        }
    }

    #[test]
    fn test_tiered_allocation() {
        let manager = TieredMemoryManager::new(test_config());

        let block = manager.allocate(1024 * 1024, MemoryTier::Hot).unwrap();
        assert_eq!(block.tier(), MemoryTier::Hot);
        assert_eq!(block.size(), 1024 * 1024);

        let stats = manager.stats();
        assert_eq!(stats.block_count, 1);
        assert_eq!(stats.tier_usage[0], 1024 * 1024);
    }

    #[test]
    fn test_tier_fallback() {
        let manager = TieredMemoryManager::new(TieredConfig {
            tier_capacities: [
                512 * 1024,        // 512KB GPU (small)
                20 * 1024 * 1024,  // 20MB Unified
                100 * 1024 * 1024, // 100MB CPU
            ],
            ..Default::default()
        });

        // Request 1MB in Hot tier - should fall back to Warm
        let block = manager.allocate(1024 * 1024, MemoryTier::Hot).unwrap();
        assert_eq!(block.tier(), MemoryTier::Warm);
    }

    #[test]
    fn test_migration() {
        let manager = TieredMemoryManager::new(test_config());

        let block = manager.allocate(1024 * 1024, MemoryTier::Hot).unwrap();
        assert_eq!(block.tier(), MemoryTier::Hot);

        let new_block = manager.migrate(&block, MemoryTier::Warm).unwrap();
        assert_eq!(new_block.tier(), MemoryTier::Warm);

        let stats = manager.stats();
        assert_eq!(stats.total_migrations, 1);
        assert_eq!(stats.tier_usage[0], 0); // Hot tier empty
        assert_eq!(stats.tier_usage[1], 1024 * 1024); // Warm has the block
    }

    #[test]
    fn test_access_tracking() {
        let manager = TieredMemoryManager::new(test_config());
        let block = manager.allocate(1024, MemoryTier::Hot).unwrap();

        assert_eq!(block.access_count(), 0);

        block.record_access();
        block.record_access();
        block.record_access();

        assert_eq!(block.access_count(), 3);
    }

    #[test]
    fn test_migration_pass() {
        let manager = TieredMemoryManager::new(TieredConfig {
            demotion_timeout: Duration::from_millis(10),
            min_access_count: 100, // High threshold so blocks get demoted
            ..test_config()
        });

        // Allocate block in Hot tier
        let _block = manager.allocate(1024 * 1024, MemoryTier::Hot).unwrap();

        // Wait for timeout
        std::thread::sleep(Duration::from_millis(20));

        // Run migration - block should be demoted
        let stats = manager.run_migration_pass();
        assert_eq!(stats.demoted, 1);

        let total_stats = manager.stats();
        assert_eq!(total_stats.tier_usage[0], 0); // Hot is now empty
        assert_eq!(total_stats.tier_usage[1], 1024 * 1024); // Warm has the block
    }
}
