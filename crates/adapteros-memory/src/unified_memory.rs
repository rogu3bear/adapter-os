//! Unified memory management for adapterOS
//!
//! This module provides unified memory management using Metal's MTLSharedHeap
//! for efficient tensor operations across Metal, MLX, and CoreML backends.

use adapteros_core::{AosError, Result};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, info};

/// Unified memory manager for cross-backend tensor operations
#[derive(Debug)]
pub struct UnifiedMemoryManager {
    /// Memory pools by backend
    pools: HashMap<String, Arc<Mutex<MemoryPool>>>,
    /// Total allocated memory
    total_allocated: Arc<Mutex<usize>>,
    /// Memory limit
    memory_limit: usize,
}

/// Memory pool for a specific backend
#[derive(Debug)]
pub struct MemoryPool {
    /// Pool identifier (reserved for multi-pool management)
    _id: String,
    /// Allocated blocks
    blocks: HashMap<String, MemoryBlock>,
    /// Available memory
    available: usize,
    /// Total pool size
    total_size: usize,
}

/// Memory block within a pool
///
/// # Thread Safety
///
/// `MemoryBlock` contains a raw pointer but is marked as `Send + Sync` because:
/// 1. The pointer is obtained from `aligned_alloc` which returns stable addresses
/// 2. The memory is owned by the manager and blocks are only deallocated through
///    the manager's `deallocate` method which holds proper locks
/// 3. The pointer itself is not dereferenced in this struct - it's just an address
///    that gets passed to backends for actual memory operations
///
/// The actual memory access synchronization is handled by the backends (Metal, MLX, etc.)
/// which have their own synchronization mechanisms.
#[derive(Debug, Clone)]
pub struct MemoryBlock {
    /// Block identifier
    id: String,
    /// Memory address (stable after allocation, safe to send across threads)
    ptr: *mut u8,
    /// Block size
    size: usize,
    /// Backend type
    backend: String,
    /// Allocation timestamp (reserved for LRU eviction)
    _timestamp: u64,
}

// SAFETY: MemoryBlock is Send because:
// - The raw pointer is a stable address obtained from libc::aligned_alloc
// - The pointer is not dereferenced within MemoryBlock itself
// - All memory operations go through synchronized backend code
// - The Manager's allocate/deallocate methods use proper Mutex synchronization
unsafe impl Send for MemoryBlock {}

// SAFETY: MemoryBlock is Sync because:
// - MemoryBlock is immutable after creation (no &mut methods that modify ptr)
// - Reading the pointer value itself is safe (it's just reading an address)
// - Actual memory access is synchronized by the backends
unsafe impl Sync for MemoryBlock {}

/// Memory allocation request
#[derive(Debug)]
pub struct AllocationRequest {
    /// Requested size
    pub size: usize,
    /// Backend type
    pub backend: String,
    /// Alignment requirements
    pub alignment: usize,
    /// Memory type hint
    pub memory_type: MemoryType,
}

/// Memory type hints for optimization
#[derive(Debug, Clone)]
pub enum MemoryType {
    /// GPU memory (Metal)
    GPU,
    /// Unified memory (shared between CPU/GPU)
    Unified,
    /// CPU memory
    CPU,
    /// Neural Engine memory
    NeuralEngine,
}

impl UnifiedMemoryManager {
    /// Create new unified memory manager
    pub fn new(memory_limit: usize) -> Self {
        Self {
            pools: HashMap::new(),
            total_allocated: Arc::new(Mutex::new(0)),
            memory_limit,
        }
    }

    /// Initialize memory pool for a backend
    pub fn init_pool(&mut self, backend: &str, pool_size: usize) -> Result<()> {
        if self.pools.contains_key(backend) {
            return Err(AosError::Memory(format!(
                "Pool for backend {} already exists",
                backend
            )));
        }

        let pool = MemoryPool {
            _id: backend.to_string(),
            blocks: HashMap::new(),
            available: pool_size,
            total_size: pool_size,
        };

        self.pools
            .insert(backend.to_string(), Arc::new(Mutex::new(pool)));
        info!(
            "Initialized memory pool for {}: {} bytes",
            backend, pool_size
        );

        Ok(())
    }

    /// Allocate memory block
    ///
    /// Lock ordering: Always acquire total_allocated BEFORE pool to prevent deadlock
    pub fn allocate(&self, request: AllocationRequest) -> Result<MemoryBlock> {
        let pool = self
            .pools
            .get(&request.backend)
            .ok_or_else(|| AosError::Memory(format!("No pool for backend {}", request.backend)))?;

        // LOCK ORDER: 1. total_allocated first (prevent deadlock with deallocate)
        let mut total_allocated = self.total_allocated.lock().unwrap();

        // Check global memory limit before acquiring pool lock
        if *total_allocated + request.size > self.memory_limit {
            return Err(AosError::Memory(format!(
                "Global memory limit exceeded: {} + {} > {}",
                *total_allocated, request.size, self.memory_limit
            )));
        }

        // LOCK ORDER: 2. pool second
        let mut pool_guard = pool.lock().unwrap();

        // Check if we have enough memory in the pool
        if pool_guard.available < request.size {
            return Err(AosError::Memory(format!(
                "Insufficient memory in {} pool: need {}, available {}",
                request.backend, request.size, pool_guard.available
            )));
        }

        // Allocate memory block
        let block_id = format!("{}_{}", request.backend, self.generate_block_id());
        let ptr = self.allocate_memory(request.size, request.alignment, &request.memory_type)?;

        let block = MemoryBlock {
            id: block_id.clone(),
            ptr,
            size: request.size,
            backend: request.backend.clone(),
            _timestamp: self.current_timestamp(),
        };

        pool_guard.blocks.insert(block_id, block.clone());
        pool_guard.available -= request.size;
        *total_allocated += request.size;

        debug!(
            "Allocated {} bytes for {} backend (block: {})",
            request.size, request.backend, block.id
        );

        Ok(block)
    }

    /// Deallocate memory block
    ///
    /// Lock ordering: Always acquire total_allocated BEFORE pool to prevent deadlock
    pub fn deallocate(&self, block: &MemoryBlock) -> Result<()> {
        let pool = self
            .pools
            .get(&block.backend)
            .ok_or_else(|| AosError::Memory(format!("No pool for backend {}", block.backend)))?;

        // LOCK ORDER: 1. total_allocated first (consistent with allocate)
        let mut total_allocated = self.total_allocated.lock().unwrap();

        // LOCK ORDER: 2. pool second
        let mut pool_guard = pool.lock().unwrap();

        if pool_guard.blocks.remove(&block.id).is_some() {
            pool_guard.available += block.size;
            *total_allocated -= block.size;

            // Release locks before calling deallocate_memory
            drop(pool_guard);
            drop(total_allocated);

            self.deallocate_memory(block.ptr, block.size)?;

            debug!("Deallocated block {} ({} bytes)", block.id, block.size);
        }

        Ok(())
    }

    /// Get memory usage statistics
    pub fn get_stats(&self) -> MemoryStats {
        let total_allocated = *self.total_allocated.lock().unwrap();
        let mut backend_stats = HashMap::new();

        for (backend, pool) in &self.pools {
            let pool_guard = pool.lock().unwrap();
            backend_stats.insert(
                backend.clone(),
                BackendStats {
                    allocated: pool_guard.total_size - pool_guard.available,
                    available: pool_guard.available,
                    total: pool_guard.total_size,
                    block_count: pool_guard.blocks.len(),
                },
            );
        }

        MemoryStats {
            total_allocated,
            memory_limit: self.memory_limit,
            backend_stats,
        }
    }

    /// Allocate memory based on type hint
    fn allocate_memory(
        &self,
        size: usize,
        alignment: usize,
        memory_type: &MemoryType,
    ) -> Result<*mut u8> {
        match memory_type {
            MemoryType::GPU => self.allocate_gpu_memory(size, alignment),
            MemoryType::Unified => self.allocate_unified_memory(size, alignment),
            MemoryType::CPU => self.allocate_cpu_memory(size, alignment),
            MemoryType::NeuralEngine => self.allocate_neural_engine_memory(size, alignment),
        }
    }

    /// Allocate GPU memory using Metal
    #[cfg(target_os = "macos")]
    fn allocate_gpu_memory(&self, size: usize, alignment: usize) -> Result<*mut u8> {
        // Placeholder for Metal memory allocation
        // In a real implementation, we would use MTLDevice::newBufferWithLength
        unsafe {
            let ptr = libc::aligned_alloc(alignment.max(8), size);
            if ptr.is_null() {
                return Err(AosError::Memory(
                    "Failed to allocate GPU memory".to_string(),
                ));
            }
            Ok(ptr as *mut u8)
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn allocate_gpu_memory(&self, size: usize, alignment: usize) -> Result<*mut u8> {
        Err(AosError::Memory(
            "GPU memory allocation not supported on this platform".to_string(),
        ))
    }

    /// Allocate unified memory (shared between CPU/GPU)
    #[cfg(target_os = "macos")]
    fn allocate_unified_memory(&self, size: usize, alignment: usize) -> Result<*mut u8> {
        // Placeholder for unified memory allocation
        // In a real implementation, we would use MTLSharedHeap
        unsafe {
            let ptr = libc::aligned_alloc(alignment.max(8), size);
            if ptr.is_null() {
                return Err(AosError::Memory(
                    "Failed to allocate unified memory".to_string(),
                ));
            }
            Ok(ptr as *mut u8)
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn allocate_unified_memory(&self, size: usize, alignment: usize) -> Result<*mut u8> {
        Err(AosError::Memory(
            "Unified memory allocation not supported on this platform".to_string(),
        ))
    }

    /// Allocate CPU memory
    fn allocate_cpu_memory(&self, size: usize, alignment: usize) -> Result<*mut u8> {
        unsafe {
            let ptr = libc::aligned_alloc(alignment.max(8), size);
            if ptr.is_null() {
                return Err(AosError::Memory(
                    "Failed to allocate CPU memory".to_string(),
                ));
            }
            Ok(ptr as *mut u8)
        }
    }

    /// Allocate Neural Engine memory
    #[cfg(target_os = "macos")]
    fn allocate_neural_engine_memory(&self, size: usize, alignment: usize) -> Result<*mut u8> {
        // Placeholder for Neural Engine memory allocation
        // In a real implementation, we would use CoreML's memory management
        self.allocate_unified_memory(size, alignment)
    }

    #[cfg(not(target_os = "macos"))]
    fn allocate_neural_engine_memory(&self, size: usize, alignment: usize) -> Result<*mut u8> {
        Err(AosError::Memory(
            "Neural Engine memory allocation not supported on this platform".to_string(),
        ))
    }

    /// Deallocate memory
    fn deallocate_memory(&self, ptr: *mut u8, _size: usize) -> Result<()> {
        unsafe {
            libc::free(ptr as *mut libc::c_void);
        }
        Ok(())
    }

    /// Generate unique block ID
    fn generate_block_id(&self) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        format!("{:016x}", COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    /// Get current timestamp
    fn current_timestamp(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }
}

/// Memory usage statistics
#[derive(Debug)]
pub struct MemoryStats {
    /// Total allocated memory across all backends
    pub total_allocated: usize,
    /// Global memory limit
    pub memory_limit: usize,
    /// Per-backend statistics
    pub backend_stats: HashMap<String, BackendStats>,
}

/// Backend-specific memory statistics
#[derive(Debug, Clone)]
pub struct BackendStats {
    /// Allocated memory
    pub allocated: usize,
    /// Available memory
    pub available: usize,
    /// Total pool size
    pub total: usize,
    /// Number of allocated blocks
    pub block_count: usize,
}

impl Default for AllocationRequest {
    fn default() -> Self {
        Self {
            size: 0,
            backend: "unknown".to_string(),
            alignment: 8,
            memory_type: MemoryType::Unified,
        }
    }
}

// ============================================================================
// Phase 2: Predictive Memory Pre-allocation
// ============================================================================

/// Memory budget for model inference
///
/// Pre-computed memory requirements that allow zero dynamic allocation
/// during inference after initial pre-allocation.
#[derive(Debug, Clone)]
pub struct MemoryBudget {
    /// Budget identifier (e.g., model name)
    pub id: String,
    /// KV cache memory per layer (bytes)
    pub kv_cache_per_layer: usize,
    /// Number of layers
    pub num_layers: usize,
    /// Activation memory per layer
    pub activation_per_layer: usize,
    /// LoRA adapter memory (total for all adapters)
    pub lora_memory: usize,
    /// Scratch/workspace memory
    pub scratch_memory: usize,
    /// Alignment requirement
    pub alignment: usize,
    /// Backend to allocate on
    pub backend: String,
}

impl MemoryBudget {
    /// Create a new memory budget
    pub fn new(id: impl Into<String>, backend: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kv_cache_per_layer: 0,
            num_layers: 0,
            activation_per_layer: 0,
            lora_memory: 0,
            scratch_memory: 0,
            alignment: 64, // Default to cache-line alignment
            backend: backend.into(),
        }
    }

    /// Set KV cache requirements
    pub fn with_kv_cache(mut self, per_layer: usize, num_layers: usize) -> Self {
        self.kv_cache_per_layer = per_layer;
        self.num_layers = num_layers;
        self
    }

    /// Set activation memory requirements
    pub fn with_activations(mut self, per_layer: usize) -> Self {
        self.activation_per_layer = per_layer;
        self
    }

    /// Set LoRA adapter memory
    pub fn with_lora(mut self, total_memory: usize) -> Self {
        self.lora_memory = total_memory;
        self
    }

    /// Set scratch/workspace memory
    pub fn with_scratch(mut self, size: usize) -> Self {
        self.scratch_memory = size;
        self
    }

    /// Calculate total memory required
    pub fn total_memory(&self) -> usize {
        let kv_total = self.kv_cache_per_layer * self.num_layers;
        let activation_total = self.activation_per_layer * self.num_layers;
        kv_total + activation_total + self.lora_memory + self.scratch_memory
    }

    /// Validate budget against available memory
    pub fn validate(&self, available: usize) -> Result<()> {
        let required = self.total_memory();
        if required > available {
            return Err(AosError::Memory(format!(
                "MemoryBudget '{}' requires {} bytes but only {} available",
                self.id, required, available
            )));
        }
        Ok(())
    }
}

/// Pre-allocated memory plan
///
/// Holds all pre-allocated memory blocks for a model inference session.
/// After creation, inference should require zero additional allocations.
#[derive(Debug)]
pub struct PreAllocationPlan {
    /// Plan identifier
    pub id: String,
    /// KV cache blocks (one per layer)
    pub kv_cache_blocks: Vec<MemoryBlock>,
    /// Activation blocks (one per layer)
    pub activation_blocks: Vec<MemoryBlock>,
    /// LoRA adapter block
    pub lora_block: Option<MemoryBlock>,
    /// Scratch/workspace block
    pub scratch_block: Option<MemoryBlock>,
    /// Total pre-allocated memory
    pub total_allocated: usize,
    /// Whether the plan is active
    pub is_active: bool,
}

impl PreAllocationPlan {
    /// Check if pre-allocation is complete
    pub fn is_complete(&self) -> bool {
        self.is_active && !self.kv_cache_blocks.is_empty()
    }

    /// Get total number of blocks
    pub fn block_count(&self) -> usize {
        self.kv_cache_blocks.len()
            + self.activation_blocks.len()
            + self.lora_block.as_ref().map_or(0, |_| 1)
            + self.scratch_block.as_ref().map_or(0, |_| 1)
    }
}

impl UnifiedMemoryManager {
    /// Pre-allocate memory for a model based on a memory budget
    ///
    /// Allocates all required memory upfront to ensure zero dynamic allocations
    /// during inference. Returns a PreAllocationPlan that holds all blocks.
    ///
    /// # Example
    /// ```ignore
    /// let budget = MemoryBudget::new("llama-7b", "metal")
    ///     .with_kv_cache(4 * 1024 * 1024, 32)  // 4MB per layer, 32 layers
    ///     .with_activations(1024 * 1024)       // 1MB per layer
    ///     .with_lora(8 * 1024 * 1024)          // 8MB for LoRA
    ///     .with_scratch(16 * 1024 * 1024);     // 16MB scratch
    ///
    /// let plan = manager.pre_allocate_for_model(&budget)?;
    /// assert!(plan.is_complete());
    /// ```
    pub fn pre_allocate_for_model(&self, budget: &MemoryBudget) -> Result<PreAllocationPlan> {
        // Validate budget against available memory
        let stats = self.get_stats();
        let available = self.memory_limit.saturating_sub(stats.total_allocated);
        budget.validate(available)?;

        info!(
            "Pre-allocating {} bytes for model '{}' on backend '{}'",
            budget.total_memory(),
            budget.id,
            budget.backend
        );

        let mut plan = PreAllocationPlan {
            id: budget.id.clone(),
            kv_cache_blocks: Vec::with_capacity(budget.num_layers),
            activation_blocks: Vec::with_capacity(budget.num_layers),
            lora_block: None,
            scratch_block: None,
            total_allocated: 0,
            is_active: false,
        };

        // Pre-allocate KV cache blocks (one per layer)
        for layer_idx in 0..budget.num_layers {
            let request = AllocationRequest {
                size: budget.kv_cache_per_layer,
                backend: budget.backend.clone(),
                alignment: budget.alignment,
                memory_type: MemoryType::Unified,
            };
            let block = self.allocate(request).map_err(|e| {
                AosError::Memory(format!(
                    "Failed to pre-allocate KV cache for layer {}: {}",
                    layer_idx, e
                ))
            })?;
            plan.total_allocated += block.size;
            plan.kv_cache_blocks.push(block);
        }

        // Pre-allocate activation blocks
        if budget.activation_per_layer > 0 {
            for layer_idx in 0..budget.num_layers {
                let request = AllocationRequest {
                    size: budget.activation_per_layer,
                    backend: budget.backend.clone(),
                    alignment: budget.alignment,
                    memory_type: MemoryType::Unified,
                };
                let block = self.allocate(request).map_err(|e| {
                    AosError::Memory(format!(
                        "Failed to pre-allocate activations for layer {}: {}",
                        layer_idx, e
                    ))
                })?;
                plan.total_allocated += block.size;
                plan.activation_blocks.push(block);
            }
        }

        // Pre-allocate LoRA block
        if budget.lora_memory > 0 {
            let request = AllocationRequest {
                size: budget.lora_memory,
                backend: budget.backend.clone(),
                alignment: budget.alignment,
                memory_type: MemoryType::Unified,
            };
            let block = self.allocate(request).map_err(|e| {
                AosError::Memory(format!("Failed to pre-allocate LoRA memory: {}", e))
            })?;
            plan.total_allocated += block.size;
            plan.lora_block = Some(block);
        }

        // Pre-allocate scratch block
        if budget.scratch_memory > 0 {
            let request = AllocationRequest {
                size: budget.scratch_memory,
                backend: budget.backend.clone(),
                alignment: budget.alignment,
                memory_type: MemoryType::Unified,
            };
            let block = self.allocate(request).map_err(|e| {
                AosError::Memory(format!("Failed to pre-allocate scratch memory: {}", e))
            })?;
            plan.total_allocated += block.size;
            plan.scratch_block = Some(block);
        }

        plan.is_active = true;

        info!(
            "Pre-allocation complete for '{}': {} blocks, {} bytes total",
            budget.id,
            plan.block_count(),
            plan.total_allocated
        );

        Ok(plan)
    }

    /// Release all memory from a pre-allocation plan
    pub fn release_pre_allocation(&self, plan: &mut PreAllocationPlan) -> Result<()> {
        if !plan.is_active {
            return Ok(());
        }

        info!(
            "Releasing pre-allocation plan '{}' ({} bytes)",
            plan.id, plan.total_allocated
        );

        // Deallocate all blocks
        for block in &plan.kv_cache_blocks {
            self.deallocate(block)?;
        }
        for block in &plan.activation_blocks {
            self.deallocate(block)?;
        }
        if let Some(block) = &plan.lora_block {
            self.deallocate(block)?;
        }
        if let Some(block) = &plan.scratch_block {
            self.deallocate(block)?;
        }

        plan.kv_cache_blocks.clear();
        plan.activation_blocks.clear();
        plan.lora_block = None;
        plan.scratch_block = None;
        plan.total_allocated = 0;
        plan.is_active = false;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_manager_creation() {
        let manager = UnifiedMemoryManager::new(1024 * 1024 * 1024); // 1GB limit
        assert_eq!(manager.memory_limit, 1024 * 1024 * 1024);
    }

    #[test]
    fn test_pool_initialization() {
        let mut manager = UnifiedMemoryManager::new(1024 * 1024);
        manager.init_pool("metal", 512 * 1024).unwrap();
        manager.init_pool("mlx", 256 * 1024).unwrap();

        assert!(manager.pools.contains_key("metal"));
        assert!(manager.pools.contains_key("mlx"));
    }

    #[test]
    fn test_memory_allocation() {
        let mut manager = UnifiedMemoryManager::new(1024 * 1024);
        manager.init_pool("test", 512 * 1024).unwrap();

        let request = AllocationRequest {
            size: 1024,
            backend: "test".to_string(),
            alignment: 8,
            memory_type: MemoryType::CPU,
        };

        let block = manager.allocate(request).unwrap();
        assert_eq!(block.size, 1024);
        assert_eq!(block.backend, "test");

        manager.deallocate(&block).unwrap();
    }

    #[test]
    fn test_memory_stats() {
        let mut manager = UnifiedMemoryManager::new(1024 * 1024);
        manager.init_pool("test", 512 * 1024).unwrap();

        let stats = manager.get_stats();
        assert_eq!(stats.total_allocated, 0);
        assert_eq!(stats.memory_limit, 1024 * 1024);
        assert!(stats.backend_stats.contains_key("test"));
    }

    // =================== Phase 2: Pre-allocation Tests ===================

    #[test]
    fn test_memory_budget_creation() {
        let budget = MemoryBudget::new("test-model", "metal")
            .with_kv_cache(4 * 1024 * 1024, 32) // 4MB per layer, 32 layers
            .with_activations(1024 * 1024) // 1MB per layer
            .with_lora(8 * 1024 * 1024) // 8MB for LoRA
            .with_scratch(16 * 1024 * 1024); // 16MB scratch

        assert_eq!(budget.id, "test-model");
        assert_eq!(budget.num_layers, 32);
        assert_eq!(
            budget.total_memory(),
            (4 + 1) * 32 * 1024 * 1024 + 8 * 1024 * 1024 + 16 * 1024 * 1024
        );
    }

    #[test]
    fn test_memory_budget_validation() {
        let budget = MemoryBudget::new("large-model", "metal").with_kv_cache(1024 * 1024, 10); // 10MB total

        // Should pass with enough memory
        assert!(budget.validate(20 * 1024 * 1024).is_ok());

        // Should fail with insufficient memory
        assert!(budget.validate(5 * 1024 * 1024).is_err());
    }

    #[test]
    fn test_pre_allocate_for_model() {
        let mut manager = UnifiedMemoryManager::new(100 * 1024 * 1024); // 100MB limit
        manager.init_pool("test", 100 * 1024 * 1024).unwrap();

        let budget = MemoryBudget::new("small-model", "test")
            .with_kv_cache(1024 * 1024, 4) // 4MB KV cache total
            .with_activations(512 * 1024) // 2MB activations total
            .with_lora(2 * 1024 * 1024) // 2MB LoRA
            .with_scratch(1024 * 1024); // 1MB scratch

        let plan = manager.pre_allocate_for_model(&budget).unwrap();

        assert!(plan.is_complete());
        assert_eq!(plan.kv_cache_blocks.len(), 4);
        assert_eq!(plan.activation_blocks.len(), 4);
        assert!(plan.lora_block.is_some());
        assert!(plan.scratch_block.is_some());
        assert_eq!(plan.block_count(), 10); // 4 KV + 4 activation + 1 LoRA + 1 scratch

        // Verify total allocation matches budget
        assert_eq!(plan.total_allocated, budget.total_memory());
    }

    #[test]
    fn test_release_pre_allocation() {
        let mut manager = UnifiedMemoryManager::new(100 * 1024 * 1024);
        manager.init_pool("test", 100 * 1024 * 1024).unwrap();

        let budget = MemoryBudget::new("release-test", "test")
            .with_kv_cache(1024 * 1024, 2)
            .with_scratch(512 * 1024);

        let mut plan = manager.pre_allocate_for_model(&budget).unwrap();
        let allocated_before = plan.total_allocated;

        // Verify memory was allocated
        let stats_before = manager.get_stats();
        assert_eq!(stats_before.total_allocated, allocated_before);

        // Release the plan
        manager.release_pre_allocation(&mut plan).unwrap();

        // Verify plan is inactive
        assert!(!plan.is_active);
        assert_eq!(plan.total_allocated, 0);
        assert!(plan.kv_cache_blocks.is_empty());

        // Verify memory was freed
        let stats_after = manager.get_stats();
        assert_eq!(stats_after.total_allocated, 0);
    }

    #[test]
    fn test_pre_allocation_failure_on_insufficient_memory() {
        let mut manager = UnifiedMemoryManager::new(1024 * 1024); // 1MB limit
        manager.init_pool("test", 1024 * 1024).unwrap();

        let budget = MemoryBudget::new("too-large", "test").with_kv_cache(1024 * 1024, 10); // 10MB, exceeds limit

        let result = manager.pre_allocate_for_model(&budget);
        assert!(result.is_err());
    }
}
