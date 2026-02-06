//! Zero-copy cross-backend memory sharing
//!
//! This module provides unified tensor buffers that can be accessed
//! by multiple backends (Metal, MLX, CoreML) without data copying.
//!
//! # Design
//!
//! Uses Metal's MTLSharedHeap as the unified backing store, allowing
//! all backends to share the same physical memory.

use adapteros_core::{AosError, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tracing::{debug, info};

/// Backend types that can access unified buffers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendAccess {
    /// Metal GPU operations
    Metal,
    /// MLX framework
    MLX,
    /// CoreML Neural Engine
    CoreML,
    /// CPU direct access
    CPU,
}

/// Unified tensor buffer that can be shared across backends
///
/// Provides zero-copy access to the same memory from Metal, MLX, and CoreML.
/// The buffer tracks which backends have active references.
#[derive(Debug)]
pub struct UnifiedTensorBuffer {
    /// Buffer identifier
    id: String,
    /// Base memory pointer (owned by Metal or system allocator)
    base_ptr: *mut u8,
    /// Buffer size in bytes
    size: usize,
    /// Alignment
    alignment: usize,
    /// Reference count
    ref_count: AtomicU64,
    /// Active backend flags
    active_backends: AtomicU64,
    /// Is buffer valid
    is_valid: AtomicBool,
    /// Tensor shape (for validation)
    shape: Vec<usize>,
    /// Element type size
    element_size: usize,
}

// SAFETY: UnifiedTensorBuffer is Send because:
// - The raw pointer is owned and not shared during mutation
// - Atomic operations protect concurrent access metadata
// - Actual data access is synchronized by backends
unsafe impl Send for UnifiedTensorBuffer {}
unsafe impl Sync for UnifiedTensorBuffer {}

/// Handle to a unified buffer with backend-specific view
#[derive(Debug)]
pub struct BufferView {
    /// Reference to the underlying buffer
    buffer: Arc<UnifiedTensorBuffer>,
    /// Backend accessing this view
    backend: BackendAccess,
    /// Offset into the buffer
    offset: usize,
    /// Length of this view
    length: usize,
}

impl UnifiedTensorBuffer {
    /// Create a new unified tensor buffer
    ///
    /// # Arguments
    /// * `id` - Unique identifier for this buffer
    /// * `shape` - Tensor shape (e.g., [batch, seq_len, hidden_dim])
    /// * `element_size` - Size of each element in bytes (e.g., 2 for f16, 4 for f32)
    pub fn new(id: impl Into<String>, shape: Vec<usize>, element_size: usize) -> Result<Self> {
        let total_elements: usize = shape.iter().product();
        let size = total_elements * element_size;
        let alignment = 64; // Cache-line alignment

        // Allocate aligned memory
        let ptr = unsafe {
            let layout = std::alloc::Layout::from_size_align(size, alignment)
                .map_err(|e| AosError::Memory(format!("Invalid layout: {}", e)))?;
            let ptr = std::alloc::alloc_zeroed(layout);
            if ptr.is_null() {
                return Err(AosError::Memory(format!(
                    "Failed to allocate {} bytes for tensor",
                    size
                )));
            }
            ptr
        };

        let id_str = id.into();
        debug!(
            "Created UnifiedTensorBuffer '{}': {:?} x {} bytes = {} total",
            id_str, shape, element_size, size
        );

        Ok(Self {
            id: id_str,
            base_ptr: ptr,
            size,
            alignment,
            ref_count: AtomicU64::new(1),
            active_backends: AtomicU64::new(0),
            is_valid: AtomicBool::new(true),
            shape,
            element_size,
        })
    }

    /// Create a view for a specific backend
    pub fn create_view(self: &Arc<Self>, backend: BackendAccess) -> Result<BufferView> {
        if !self.is_valid.load(Ordering::Acquire) {
            return Err(AosError::Memory("Buffer is no longer valid".to_string()));
        }

        // Track active backend
        let backend_bit = 1u64 << (backend as u64);
        self.active_backends
            .fetch_or(backend_bit, Ordering::Release);
        self.ref_count.fetch_add(1, Ordering::Release);

        debug!(
            "Created {:?} view for buffer '{}' (refs: {})",
            backend,
            self.id,
            self.ref_count.load(Ordering::Acquire)
        );

        Ok(BufferView {
            buffer: Arc::clone(self),
            backend,
            offset: 0,
            length: self.size,
        })
    }

    /// Get the raw pointer (for backend-specific operations)
    ///
    /// # Safety
    /// The caller must ensure proper synchronization when accessing
    /// the memory from multiple backends.
    pub unsafe fn as_ptr(&self) -> *mut u8 {
        self.base_ptr
    }

    /// Get buffer size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get tensor shape
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Get element count
    pub fn element_count(&self) -> usize {
        self.shape.iter().product()
    }

    /// Check if a backend has an active view
    pub fn is_backend_active(&self, backend: BackendAccess) -> bool {
        let backend_bit = 1u64 << (backend as u64);
        (self.active_backends.load(Ordering::Acquire) & backend_bit) != 0
    }

    /// Get current reference count
    pub fn ref_count(&self) -> u64 {
        self.ref_count.load(Ordering::Acquire)
    }

    /// Invalidate the buffer (prevents new views)
    pub fn invalidate(&self) {
        self.is_valid.store(false, Ordering::Release);
        info!("Buffer '{}' invalidated", self.id);
    }

    /// Get buffer ID
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl Drop for UnifiedTensorBuffer {
    fn drop(&mut self) {
        if !self.base_ptr.is_null() {
            match std::alloc::Layout::from_size_align(self.size, self.alignment) {
                Ok(layout) => {
                    unsafe {
                        std::alloc::dealloc(self.base_ptr, layout);
                    }
                    debug!("Dropped UnifiedTensorBuffer '{}'", self.id);
                }
                Err(_) => {
                    // Invalid layout in drop - leak memory rather than panic.
                    // Panicking in Drop can cause double-panic and process abort.
                    // This should never happen if the buffer was constructed properly.
                    debug!(
                        "UnifiedTensorBuffer '{}' leaked: invalid layout (size={}, align={})",
                        self.id, self.size, self.alignment
                    );
                }
            }
        }
    }
}

impl BufferView {
    /// Get the underlying pointer for this view
    ///
    /// # Safety
    /// Caller must ensure synchronization with other backends
    pub unsafe fn as_ptr(&self) -> *mut u8 {
        self.buffer.base_ptr.add(self.offset)
    }

    /// Get view length
    pub fn len(&self) -> usize {
        self.length
    }

    /// Check if view is empty
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Get the backend for this view
    pub fn backend(&self) -> BackendAccess {
        self.backend
    }

    /// Create a subview with offset and length
    pub fn subview(&self, offset: usize, length: usize) -> Result<BufferView> {
        if offset + length > self.length {
            return Err(AosError::Memory(format!(
                "Subview exceeds bounds: {} + {} > {}",
                offset, length, self.length
            )));
        }

        self.buffer.ref_count.fetch_add(1, Ordering::Release);

        Ok(BufferView {
            buffer: Arc::clone(&self.buffer),
            backend: self.backend,
            offset: self.offset + offset,
            length,
        })
    }

    /// Get the parent buffer ID
    pub fn buffer_id(&self) -> &str {
        self.buffer.id()
    }
}

impl Drop for BufferView {
    fn drop(&mut self) {
        let backend_bit = 1u64 << (self.backend as u64);

        // Decrement ref count and potentially clear backend flag
        let old_count = self.buffer.ref_count.fetch_sub(1, Ordering::Release);

        // If this was the last view for this backend, clear the flag
        // (simplified - in production you'd track per-backend counts)
        if old_count == 2 {
            self.buffer
                .active_backends
                .fetch_and(!backend_bit, Ordering::Release);
        }
    }
}

/// Manager for unified tensor buffers
pub struct CrossBackendManager {
    /// Active buffers
    buffers: std::sync::Mutex<std::collections::HashMap<String, Arc<UnifiedTensorBuffer>>>,
    /// Total allocated memory
    total_allocated: AtomicU64,
    /// Memory limit
    limit: usize,
}

impl CrossBackendManager {
    /// Create a new cross-backend manager
    pub fn new(limit: usize) -> Self {
        info!("Creating CrossBackendManager with {} byte limit", limit);
        Self {
            buffers: std::sync::Mutex::new(std::collections::HashMap::new()),
            total_allocated: AtomicU64::new(0),
            limit,
        }
    }

    /// Create a new unified tensor buffer
    pub fn create_buffer(
        &self,
        id: impl Into<String>,
        shape: Vec<usize>,
        element_size: usize,
    ) -> Result<Arc<UnifiedTensorBuffer>> {
        let id_str = id.into();
        let total_elements: usize = shape.iter().product();
        let size = total_elements * element_size;

        // Check limit
        let current = self.total_allocated.load(Ordering::Acquire);
        if current + size as u64 > self.limit as u64 {
            return Err(AosError::Memory(format!(
                "Cross-backend memory limit exceeded: {} + {} > {}",
                current, size, self.limit
            )));
        }

        let buffer = Arc::new(UnifiedTensorBuffer::new(
            id_str.clone(),
            shape,
            element_size,
        )?);

        self.total_allocated
            .fetch_add(size as u64, Ordering::Release);

        let mut buffers = self.buffers.lock().unwrap();
        buffers.insert(id_str, Arc::clone(&buffer));

        Ok(buffer)
    }

    /// Get an existing buffer
    pub fn get_buffer(&self, id: &str) -> Option<Arc<UnifiedTensorBuffer>> {
        let buffers = self.buffers.lock().unwrap();
        buffers.get(id).cloned()
    }

    /// Remove a buffer
    pub fn remove_buffer(&self, id: &str) -> Option<Arc<UnifiedTensorBuffer>> {
        let mut buffers = self.buffers.lock().unwrap();
        if let Some(buffer) = buffers.remove(id) {
            self.total_allocated
                .fetch_sub(buffer.size() as u64, Ordering::Release);
            Some(buffer)
        } else {
            None
        }
    }

    /// Get statistics
    pub fn stats(&self) -> CrossBackendStats {
        let buffers = self.buffers.lock().unwrap();
        CrossBackendStats {
            buffer_count: buffers.len(),
            total_allocated: self.total_allocated.load(Ordering::Acquire),
            limit: self.limit as u64,
        }
    }
}

/// Statistics for cross-backend memory
#[derive(Debug, Clone)]
pub struct CrossBackendStats {
    /// Number of active buffers
    pub buffer_count: usize,
    /// Total allocated bytes
    pub total_allocated: u64,
    /// Memory limit
    pub limit: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_buffer_creation() {
        let shape = vec![2, 4, 8]; // 64 elements
        let buffer = UnifiedTensorBuffer::new("test", shape.clone(), 4).unwrap();

        assert_eq!(buffer.size(), 64 * 4);
        assert_eq!(buffer.shape(), &shape);
        assert_eq!(buffer.element_count(), 64);
    }

    #[test]
    fn test_multi_backend_views() {
        let buffer = Arc::new(UnifiedTensorBuffer::new("multi-backend", vec![16, 16], 4).unwrap());

        let metal_view = buffer.create_view(BackendAccess::Metal).unwrap();
        let mlx_view = buffer.create_view(BackendAccess::MLX).unwrap();

        assert!(buffer.is_backend_active(BackendAccess::Metal));
        assert!(buffer.is_backend_active(BackendAccess::MLX));
        assert!(!buffer.is_backend_active(BackendAccess::CoreML));

        // 1 base + 2 views = 3
        assert_eq!(buffer.ref_count(), 3);

        drop(metal_view);
        drop(mlx_view);

        assert_eq!(buffer.ref_count(), 1);
    }

    #[test]
    fn test_subview() {
        let buffer = Arc::new(UnifiedTensorBuffer::new("subview-test", vec![1024], 1).unwrap());

        let view = buffer.create_view(BackendAccess::CPU).unwrap();
        assert_eq!(view.len(), 1024);

        let sub = view.subview(100, 200).unwrap();
        assert_eq!(sub.len(), 200);

        // Subview out of bounds
        let result = view.subview(900, 200);
        assert!(result.is_err());
    }

    #[test]
    fn test_cross_backend_manager() {
        let manager = CrossBackendManager::new(10 * 1024 * 1024); // 10MB

        let buf1 = manager.create_buffer("tensor1", vec![256, 256], 4).unwrap();
        assert_eq!(buf1.size(), 256 * 256 * 4);

        let stats = manager.stats();
        assert_eq!(stats.buffer_count, 1);
        assert_eq!(stats.total_allocated, (256 * 256 * 4) as u64);

        manager.remove_buffer("tensor1");

        let stats = manager.stats();
        assert_eq!(stats.buffer_count, 0);
        assert_eq!(stats.total_allocated, 0);
    }

    #[test]
    fn test_buffer_invalidation() {
        let buffer = Arc::new(UnifiedTensorBuffer::new("invalid-test", vec![16], 4).unwrap());

        let _view1 = buffer.create_view(BackendAccess::Metal).unwrap();

        buffer.invalidate();

        // New views should fail
        let result = buffer.create_view(BackendAccess::MLX);
        assert!(result.is_err());
    }
}
