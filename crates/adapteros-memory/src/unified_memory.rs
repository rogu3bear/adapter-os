//! Unified memory management for AdapterOS
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
    /// Pool identifier
    id: String,
    /// Allocated blocks
    blocks: HashMap<String, MemoryBlock>,
    /// Available memory
    available: usize,
    /// Total pool size
    total_size: usize,
}

/// Memory block within a pool
#[derive(Debug, Clone)]
pub struct MemoryBlock {
    /// Block identifier
    id: String,
    /// Memory address
    ptr: *mut u8,
    /// Block size
    size: usize,
    /// Backend type
    backend: String,
    /// Allocation timestamp
    timestamp: u64,
}

/// Memory allocation request
#[derive(Debug)]
pub struct AllocationRequest {
    /// Requested size
    size: usize,
    /// Backend type
    backend: String,
    /// Alignment requirements
    alignment: usize,
    /// Memory type hint
    memory_type: MemoryType,
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
            id: backend.to_string(),
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
    pub fn allocate(&self, request: AllocationRequest) -> Result<MemoryBlock> {
        let pool = self
            .pools
            .get(&request.backend)
            .ok_or_else(|| AosError::Memory(format!("No pool for backend {}", request.backend)))?;

        let mut pool_guard = pool.lock().unwrap();

        // Check if we have enough memory
        if pool_guard.available < request.size {
            return Err(AosError::Memory(format!(
                "Insufficient memory in {} pool: need {}, available {}",
                request.backend, request.size, pool_guard.available
            )));
        }

        // Check global memory limit
        let mut total_allocated = self.total_allocated.lock().unwrap();
        if *total_allocated + request.size > self.memory_limit {
            return Err(AosError::Memory(format!(
                "Global memory limit exceeded: {} + {} > {}",
                *total_allocated, request.size, self.memory_limit
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
            timestamp: self.current_timestamp(),
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
    pub fn deallocate(&self, block: &MemoryBlock) -> Result<()> {
        let pool = self
            .pools
            .get(&block.backend)
            .ok_or_else(|| AosError::Memory(format!("No pool for backend {}", block.backend)))?;

        let mut pool_guard = pool.lock().unwrap();

        if pool_guard.blocks.remove(&block.id).is_some() {
            pool_guard.available += block.size;

            let mut total_allocated = self.total_allocated.lock().unwrap();
            *total_allocated -= block.size;

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
    fn deallocate_memory(&self, ptr: *mut u8, size: usize) -> Result<()> {
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
}
