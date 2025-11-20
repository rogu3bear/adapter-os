//! Buffer pooling system for IoBuffers and tensor format conversions
//!
//! Implements buffer reuse to reduce allocation overhead across backends.
//! Supports:
//! - IoBuffer pooling with configurable max pool size
//! - Tensor format conversion cache (Metal ↔ CoreML ↔ MLX)
//! - Memory pressure-aware buffer eviction

use adapteros_core::{AosError, Result};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, warn};

/// Buffer pool configuration
#[derive(Debug, Clone)]
pub struct BufferPoolConfig {
    /// Maximum number of buffers to pool
    pub max_pool_size: usize,
    /// Maximum buffer size to pool (bytes)
    pub max_buffer_size: usize,
    /// Enable tensor format conversion cache
    pub enable_conversion_cache: bool,
    /// Maximum conversion cache entries
    pub max_conversion_cache_size: usize,
}

impl Default for BufferPoolConfig {
    fn default() -> Self {
        Self {
            max_pool_size: 64,
            max_buffer_size: 128 * 1024 * 1024, // 128 MB
            enable_conversion_cache: true,
            max_conversion_cache_size: 32,
        }
    }
}

/// Pooled buffer metadata
#[derive(Debug, Clone)]
struct PooledBuffer {
    /// Buffer data
    data: Vec<u8>,
    /// Capacity (may be larger than current size)
    capacity: usize,
    /// Last access timestamp
    last_accessed: u64,
    /// Number of times reused
    reuse_count: u32,
}

impl PooledBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
            capacity,
            last_accessed: current_timestamp(),
            reuse_count: 0,
        }
    }

    fn reset(&mut self, new_size: usize) {
        self.data.clear();
        self.data.resize(new_size, 0);
        self.last_accessed = current_timestamp();
        self.reuse_count += 1;
    }
}

/// Tensor format for conversion cache
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TensorFormat {
    /// Metal GPU format (row-major f32)
    Metal,
    /// CoreML format (channel-first f16)
    CoreML,
    /// MLX format (unified memory f32)
    Mlx,
}

/// Tensor format conversion key
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ConversionKey {
    /// Source format
    from: TensorFormat,
    /// Destination format
    to: TensorFormat,
    /// Tensor shape (height, width, channels)
    shape: (usize, usize, usize),
}

/// Cached tensor conversion
#[derive(Debug, Clone)]
struct CachedConversion {
    /// Converted data
    data: Vec<u8>,
    /// Last access timestamp
    last_accessed: u64,
    /// Size in bytes
    size_bytes: usize,
}

/// Buffer pool for IoBuffers and tensor conversions
pub struct BufferPool {
    /// Configuration
    config: BufferPoolConfig,
    /// Available buffers by size bucket
    buffers: Arc<Mutex<HashMap<usize, VecDeque<PooledBuffer>>>>,
    /// Tensor format conversion cache
    conversion_cache: Arc<Mutex<HashMap<ConversionKey, CachedConversion>>>,
    /// Total pooled bytes
    total_pooled_bytes: Arc<Mutex<usize>>,
    /// Total cache bytes
    total_cache_bytes: Arc<Mutex<usize>>,
}

impl BufferPool {
    /// Create a new buffer pool
    pub fn new(config: BufferPoolConfig) -> Self {
        Self {
            config,
            buffers: Arc::new(Mutex::new(HashMap::new())),
            conversion_cache: Arc::new(Mutex::new(HashMap::new())),
            total_pooled_bytes: Arc::new(Mutex::new(0)),
            total_cache_bytes: Arc::new(Mutex::new(0)),
        }
    }

    /// Acquire a buffer from the pool or allocate new
    pub fn acquire_buffer(&self, size: usize) -> Result<Vec<u8>> {
        if size > self.config.max_buffer_size {
            return Err(AosError::Memory(format!(
                "Requested buffer size {} exceeds max {}",
                size, self.config.max_buffer_size
            )));
        }

        let bucket = self.size_bucket(size);
        let mut buffers = self.buffers.lock().unwrap();

        if let Some(bucket_queue) = buffers.get_mut(&bucket) {
            if let Some(mut pooled) = bucket_queue.pop_front() {
                pooled.reset(size);

                let mut total_pooled = self.total_pooled_bytes.lock().unwrap();
                *total_pooled = total_pooled.saturating_sub(pooled.capacity);

                debug!(
                    size = size,
                    bucket = bucket,
                    reuse_count = pooled.reuse_count,
                    "Acquired buffer from pool"
                );

                return Ok(pooled.data);
            }
        }

        // Allocate new buffer
        let mut buffer = Vec::with_capacity(bucket);
        buffer.resize(size, 0);

        debug!(size = size, bucket = bucket, "Allocated new buffer");

        Ok(buffer)
    }

    /// Return a buffer to the pool
    pub fn release_buffer(&self, mut buffer: Vec<u8>) {
        let capacity = buffer.capacity();

        if capacity > self.config.max_buffer_size {
            // Too large to pool
            debug!(capacity = capacity, "Buffer too large to pool, dropping");
            return;
        }

        let bucket = self.size_bucket(capacity);
        let mut buffers = self.buffers.lock().unwrap();

        let bucket_queue = buffers.entry(bucket).or_insert_with(VecDeque::new);

        // Check if pool is full
        if bucket_queue.len() >= self.config.max_pool_size {
            // Evict oldest buffer
            if let Some(old) = bucket_queue.pop_back() {
                let mut total_pooled = self.total_pooled_bytes.lock().unwrap();
                *total_pooled = total_pooled.saturating_sub(old.capacity);

                debug!(
                    bucket = bucket,
                    evicted_capacity = old.capacity,
                    "Evicted oldest buffer from full pool"
                );
            }
        }

        // Clear buffer data for security
        buffer.clear();

        let pooled = PooledBuffer::new(capacity);
        bucket_queue.push_front(pooled);

        let mut total_pooled = self.total_pooled_bytes.lock().unwrap();
        *total_pooled += capacity;

        debug!(
            bucket = bucket,
            capacity = capacity,
            "Released buffer to pool"
        );
    }

    /// Get or convert tensor format (with caching)
    pub fn convert_tensor_format(
        &self,
        data: &[u8],
        from: TensorFormat,
        to: TensorFormat,
        shape: (usize, usize, usize),
    ) -> Result<Vec<u8>> {
        if from == to {
            return Ok(data.to_vec());
        }

        if !self.config.enable_conversion_cache {
            return self.perform_conversion(data, from, to, shape);
        }

        let key = ConversionKey { from, to, shape };

        // Check cache
        {
            let mut cache = self.conversion_cache.lock().unwrap();
            if let Some(cached) = cache.get_mut(&key) {
                cached.last_accessed = current_timestamp();
                debug!(
                    from = ?from,
                    to = ?to,
                    shape = ?shape,
                    "Tensor conversion cache hit"
                );
                return Ok(cached.data.clone());
            }
        }

        // Perform conversion
        let converted = self.perform_conversion(data, from, to, shape)?;

        // Cache result
        {
            let mut cache = self.conversion_cache.lock().unwrap();

            // Evict if cache full
            if cache.len() >= self.config.max_conversion_cache_size {
                self.evict_oldest_conversion(&mut cache);
            }

            let size_bytes = converted.len();
            cache.insert(
                key,
                CachedConversion {
                    data: converted.clone(),
                    last_accessed: current_timestamp(),
                    size_bytes,
                },
            );

            let mut total_cache = self.total_cache_bytes.lock().unwrap();
            *total_cache += size_bytes;

            debug!(
                from = ?from,
                to = ?to,
                shape = ?shape,
                size_bytes = size_bytes,
                "Cached tensor conversion"
            );
        }

        Ok(converted)
    }

    /// Perform actual tensor format conversion
    fn perform_conversion(
        &self,
        data: &[u8],
        from: TensorFormat,
        to: TensorFormat,
        shape: (usize, usize, usize),
    ) -> Result<Vec<u8>> {
        match (from, to) {
            (TensorFormat::Metal, TensorFormat::CoreML) => self.metal_to_coreml(data, shape),
            (TensorFormat::CoreML, TensorFormat::Metal) => self.coreml_to_metal(data, shape),
            (TensorFormat::Metal, TensorFormat::Mlx) => self.metal_to_mlx(data, shape),
            (TensorFormat::Mlx, TensorFormat::Metal) => self.mlx_to_metal(data, shape),
            (TensorFormat::CoreML, TensorFormat::Mlx) => self.coreml_to_mlx(data, shape),
            (TensorFormat::Mlx, TensorFormat::CoreML) => self.mlx_to_coreml(data, shape),
            _ => Err(AosError::Memory(format!(
                "Unsupported conversion: {:?} -> {:?}",
                from, to
            ))),
        }
    }

    /// Convert Metal (row-major f32) to CoreML (channel-first f16)
    fn metal_to_coreml(&self, data: &[u8], shape: (usize, usize, usize)) -> Result<Vec<u8>> {
        let (h, w, c) = shape;
        let input = bytemuck::cast_slice::<u8, f32>(data);

        // Convert row-major to channel-first and f32 to f16
        let mut output = Vec::with_capacity(h * w * c * 2); // f16 is 2 bytes

        for ch in 0..c {
            for y in 0..h {
                for x in 0..w {
                    let idx = y * w * c + x * c + ch;
                    let value = half::f16::from_f32(input[idx]);
                    output.extend_from_slice(&value.to_le_bytes());
                }
            }
        }

        Ok(output)
    }

    /// Convert CoreML (channel-first f16) to Metal (row-major f32)
    fn coreml_to_metal(&self, data: &[u8], shape: (usize, usize, usize)) -> Result<Vec<u8>> {
        let (h, w, c) = shape;
        let mut output = Vec::with_capacity(h * w * c * 4); // f32 is 4 bytes

        for y in 0..h {
            for x in 0..w {
                for ch in 0..c {
                    let idx = (ch * h * w + y * w + x) * 2;
                    let value = half::f16::from_le_bytes([data[idx], data[idx + 1]]);
                    output.extend_from_slice(&value.to_f32().to_le_bytes());
                }
            }
        }

        Ok(output)
    }

    /// Convert Metal to MLX (both f32, but different memory layout)
    fn metal_to_mlx(&self, data: &[u8], _shape: (usize, usize, usize)) -> Result<Vec<u8>> {
        // MLX uses unified memory, format is same as Metal for f32
        Ok(data.to_vec())
    }

    /// Convert MLX to Metal
    fn mlx_to_metal(&self, data: &[u8], _shape: (usize, usize, usize)) -> Result<Vec<u8>> {
        // Same format
        Ok(data.to_vec())
    }

    /// Convert CoreML to MLX
    fn coreml_to_mlx(&self, data: &[u8], shape: (usize, usize, usize)) -> Result<Vec<u8>> {
        // Convert f16 channel-first to f32 row-major
        self.coreml_to_metal(data, shape)
    }

    /// Convert MLX to CoreML
    fn mlx_to_coreml(&self, data: &[u8], shape: (usize, usize, usize)) -> Result<Vec<u8>> {
        // Convert f32 row-major to f16 channel-first
        self.metal_to_coreml(data, shape)
    }

    /// Evict oldest conversion from cache
    fn evict_oldest_conversion(&self, cache: &mut HashMap<ConversionKey, CachedConversion>) {
        if let Some((oldest_key, oldest_size)) = cache
            .iter()
            .min_by_key(|(_, v)| v.last_accessed)
            .map(|(k, v)| (k.clone(), v.size_bytes))
        {
            cache.remove(&oldest_key);

            let mut total_cache = self.total_cache_bytes.lock().unwrap();
            *total_cache = total_cache.saturating_sub(oldest_size);

            debug!(
                key = ?oldest_key,
                size = oldest_size,
                "Evicted oldest conversion from cache"
            );
        }
    }

    /// Calculate size bucket for buffer pooling
    fn size_bucket(&self, size: usize) -> usize {
        // Round up to next power of 2 for efficient bucketing
        let mut bucket = 1024; // Start at 1KB
        while bucket < size {
            bucket *= 2;
        }
        bucket.min(self.config.max_buffer_size)
    }

    /// Get pool statistics
    pub fn stats(&self) -> BufferPoolStats {
        let buffers = self.buffers.lock().unwrap();
        let cache = self.conversion_cache.lock().unwrap();

        let total_pooled = *self.total_pooled_bytes.lock().unwrap();
        let total_cache = *self.total_cache_bytes.lock().unwrap();

        let buffer_count: usize = buffers.values().map(|q| q.len()).sum();
        let cache_entries = cache.len();

        BufferPoolStats {
            buffer_count,
            total_pooled_bytes: total_pooled,
            cache_entries,
            total_cache_bytes: total_cache,
        }
    }

    /// Clear all pooled buffers and cache (for memory pressure)
    pub fn clear(&self) {
        {
            let mut buffers = self.buffers.lock().unwrap();
            buffers.clear();
            let mut total_pooled = self.total_pooled_bytes.lock().unwrap();
            *total_pooled = 0;
        }

        {
            let mut cache = self.conversion_cache.lock().unwrap();
            cache.clear();
            let mut total_cache = self.total_cache_bytes.lock().unwrap();
            *total_cache = 0;
        }

        warn!("Cleared all pooled buffers and conversion cache");
    }
}

/// Buffer pool statistics
#[derive(Debug, Clone)]
pub struct BufferPoolStats {
    /// Number of pooled buffers
    pub buffer_count: usize,
    /// Total pooled memory (bytes)
    pub total_pooled_bytes: usize,
    /// Number of cached conversions
    pub cache_entries: usize,
    /// Total cache memory (bytes)
    pub total_cache_bytes: usize,
}

/// Get current timestamp (seconds since epoch)
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_pool_acquire_release() {
        let config = BufferPoolConfig::default();
        let pool = BufferPool::new(config);

        // Acquire buffer
        let buffer1 = pool.acquire_buffer(1024).unwrap();
        assert_eq!(buffer1.len(), 1024);

        // Release buffer
        pool.release_buffer(buffer1);

        // Acquire again (should reuse)
        let buffer2 = pool.acquire_buffer(1024).unwrap();
        assert_eq!(buffer2.len(), 1024);
    }

    #[test]
    fn test_size_bucketing() {
        let config = BufferPoolConfig::default();
        let pool = BufferPool::new(config);

        assert_eq!(pool.size_bucket(500), 1024);
        assert_eq!(pool.size_bucket(1024), 1024);
        assert_eq!(pool.size_bucket(1025), 2048);
        assert_eq!(pool.size_bucket(2048), 2048);
    }

    #[test]
    fn test_pool_eviction() {
        let config = BufferPoolConfig {
            max_pool_size: 2,
            ..Default::default()
        };
        let pool = BufferPool::new(config);

        // Fill pool
        let b1 = pool.acquire_buffer(1024).unwrap();
        let b2 = pool.acquire_buffer(1024).unwrap();
        pool.release_buffer(b1);
        pool.release_buffer(b2);

        let stats = pool.stats();
        assert_eq!(stats.buffer_count, 2);

        // Adding third should evict oldest
        let b3 = pool.acquire_buffer(1024).unwrap();
        pool.release_buffer(b3);

        let stats = pool.stats();
        assert_eq!(stats.buffer_count, 2);
    }

    #[test]
    fn test_tensor_format_conversion() {
        let config = BufferPoolConfig::default();
        let pool = BufferPool::new(config);

        let data = vec![0u8; 1024]; // Mock f32 data
        let shape = (8, 8, 4); // 8x8 image, 4 channels

        // Convert Metal -> CoreML
        let converted = pool
            .convert_tensor_format(&data, TensorFormat::Metal, TensorFormat::CoreML, shape)
            .unwrap();

        // Should be f16 (half size per value)
        assert!(converted.len() > 0);
    }

    #[test]
    fn test_conversion_cache() {
        let config = BufferPoolConfig::default();
        let pool = BufferPool::new(config);

        let data = vec![0u8; 1024];
        let shape = (8, 8, 4);

        // First conversion
        pool.convert_tensor_format(&data, TensorFormat::Metal, TensorFormat::CoreML, shape)
            .unwrap();

        let stats = pool.stats();
        assert_eq!(stats.cache_entries, 1);

        // Second conversion (should hit cache)
        pool.convert_tensor_format(&data, TensorFormat::Metal, TensorFormat::CoreML, shape)
            .unwrap();

        let stats = pool.stats();
        assert_eq!(stats.cache_entries, 1); // Still 1 (cache hit)
    }

    #[test]
    fn test_pool_clear() {
        let config = BufferPoolConfig::default();
        let pool = BufferPool::new(config);

        let buffer = pool.acquire_buffer(1024).unwrap();
        pool.release_buffer(buffer);

        let data = vec![0u8; 1024];
        pool.convert_tensor_format(&data, TensorFormat::Metal, TensorFormat::CoreML, (8, 8, 4))
            .unwrap();

        let stats = pool.stats();
        assert!(stats.buffer_count > 0 || stats.cache_entries > 0);

        pool.clear();

        let stats = pool.stats();
        assert_eq!(stats.buffer_count, 0);
        assert_eq!(stats.cache_entries, 0);
    }
}
