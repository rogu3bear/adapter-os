//! KV cache management for efficient autoregressive generation
//!
//! This module provides memory-efficient key-value caching for transformer attention,
//! enabling efficient token-by-token generation without recomputing past attention.
//!
//! Features:
//! - Per-layer key and value Metal buffers
//! - Dynamic cache growth with configurable maximum sequence length
//! - Support for GQA (Grouped Query Attention)
//! - Integration with FlashAttentionKernel
//! - Memory-efficient storage with automatic cleanup
//!
//! References:
//! - KV Cache: https://arxiv.org/abs/2211.05102
//! - Flash Attention: https://arxiv.org/abs/2205.14135

use adapteros_core::{AosError, Result};
use metal::*;
use std::sync::Arc;
use std::time::Instant;

use super::fused_qkv::GqaConfig;
use super::kv_quota::{COLD_DEMOTION_IDLE_TIME, HOT_PROMOTION_THRESHOLD, HOT_RECENCY_WINDOW};
use super::purgeable::PurgeableBuffer;

/// KV cache residency classification for memory management
///
/// HOT entries are actively in use or frequently accessed and should be protected
/// from OS-level memory purgeing. COLD entries can be reclaimed under memory pressure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum KvResidency {
    /// Active or frequently-used entry - marked non-purgeable on supported backends
    Hot,
    /// Idle entry - can be evicted under memory pressure
    #[default]
    Cold,
}

impl std::fmt::Display for KvResidency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hot => write!(f, "HOT"),
            Self::Cold => write!(f, "COLD"),
        }
    }
}

/// Configuration for KV cache
#[derive(Debug, Clone)]
pub struct KVCacheConfig {
    /// Number of transformer layers
    pub num_layers: usize,
    /// Maximum sequence length to cache
    pub max_seq_len: usize,
    /// Number of key-value heads (for GQA)
    pub num_kv_heads: usize,
    /// Dimension per head
    pub head_dim: usize,
    /// Batch size (typically 1 for inference)
    pub batch_size: usize,
}

impl Default for KVCacheConfig {
    fn default() -> Self {
        Self {
            num_layers: 28,    // Qwen2.5-7B default
            max_seq_len: 4096, // Default context length
            num_kv_heads: 4,   // GQA: 4 KV heads
            head_dim: 128,     // Per-head dimension
            batch_size: 1,
        }
    }
}

impl KVCacheConfig {
    /// Create config from GQA config
    pub fn from_gqa_config(gqa_config: &GqaConfig, num_layers: usize, max_seq_len: usize) -> Self {
        Self {
            num_layers,
            max_seq_len,
            num_kv_heads: gqa_config.num_key_value_heads as usize,
            head_dim: gqa_config.head_dim as usize,
            batch_size: 1,
        }
    }

    /// Calculate bytes per layer for K or V cache
    pub fn bytes_per_layer(&self) -> u64 {
        (self.batch_size
            * self.num_kv_heads
            * self.max_seq_len
            * self.head_dim
            * std::mem::size_of::<f32>()) as u64
    }

    /// Calculate total bytes for entire cache (all layers, K+V)
    pub fn total_bytes(&self) -> u64 {
        self.bytes_per_layer() * 2 * self.num_layers as u64
    }
}

/// Per-layer KV cache buffers
#[derive(Debug)]
pub struct LayerKVCache {
    /// Key cache buffer [batch, num_kv_heads, seq_len, head_dim]
    pub key_cache: Buffer,
    /// Value cache buffer [batch, num_kv_heads, seq_len, head_dim]
    pub value_cache: Buffer,
    /// Current sequence position (number of cached tokens)
    pub seq_pos: usize,
    /// Maximum sequence length
    pub max_seq_len: usize,
    /// Residency classification for memory management
    residency: KvResidency,
    /// Whether purgeable state has been applied to buffers
    purgeable_state_applied: bool,
    /// Number of times this cache has been accessed
    access_count: usize,
    /// Last time this cache was accessed
    last_access_time: Instant,
}

impl LayerKVCache {
    /// Create new layer cache with pre-allocated buffers
    fn new(device: &Device, config: &KVCacheConfig) -> Result<Self> {
        let buffer_size = config.bytes_per_layer();

        let key_cache = device.new_buffer(buffer_size, MTLResourceOptions::StorageModeShared);
        let value_cache = device.new_buffer(buffer_size, MTLResourceOptions::StorageModeShared);

        Ok(Self {
            key_cache,
            value_cache,
            seq_pos: 0,
            max_seq_len: config.max_seq_len,
            residency: KvResidency::default(),
            purgeable_state_applied: false,
            access_count: 0,
            last_access_time: Instant::now(),
        })
    }

    /// Check if cache can accept more tokens
    pub fn can_append(&self, num_tokens: usize) -> bool {
        self.seq_pos + num_tokens <= self.max_seq_len
    }

    /// Get remaining capacity in tokens
    pub fn remaining_capacity(&self) -> usize {
        self.max_seq_len - self.seq_pos
    }

    /// Reset cache position (for new generation)
    pub fn reset(&mut self) {
        self.seq_pos = 0;
    }

    /// Get current residency classification
    pub fn residency(&self) -> KvResidency {
        self.residency
    }

    /// Set residency classification and update purgeable state
    ///
    /// When promoting to HOT, marks buffers as non-purgeable to prevent OS reclamation.
    /// When demoting to COLD, marks buffers as purgeable to allow memory recovery.
    pub fn set_residency(&mut self, residency: KvResidency) {
        if self.residency == residency {
            return; // No change needed
        }

        let old_residency = self.residency;
        self.residency = residency;

        // Update purgeable state based on residency
        match residency {
            KvResidency::Hot => {
                // Promote to HOT: make buffers non-purgeable
                if let Err(e) = self.key_cache.make_non_purgeable() {
                    tracing::warn!(
                        error = %e,
                        "Failed to make key cache non-purgeable (promotion to HOT)"
                    );
                }
                if let Err(e) = self.value_cache.make_non_purgeable() {
                    tracing::warn!(
                        error = %e,
                        "Failed to make value cache non-purgeable (promotion to HOT)"
                    );
                }
                self.purgeable_state_applied = true;
                tracing::debug!(
                    old_residency = %old_residency,
                    new_residency = %residency,
                    access_count = self.access_count,
                    "Promoted KV cache to HOT (non-purgeable)"
                );
            }
            KvResidency::Cold => {
                // Demote to COLD: make buffers purgeable
                if let Err(e) = self.key_cache.make_purgeable() {
                    tracing::warn!(
                        error = %e,
                        "Failed to make key cache purgeable (demotion to COLD)"
                    );
                }
                if let Err(e) = self.value_cache.make_purgeable() {
                    tracing::warn!(
                        error = %e,
                        "Failed to make value cache purgeable (demotion to COLD)"
                    );
                }
                self.purgeable_state_applied = true;
                tracing::debug!(
                    old_residency = %old_residency,
                    new_residency = %residency,
                    access_count = self.access_count,
                    "Demoted KV cache to COLD (purgeable)"
                );
            }
        }
    }

    /// Record an access to this cache entry
    ///
    /// Updates access count and last access time, and checks if promotion
    /// to HOT is warranted based on access frequency or recency.
    pub fn record_access(&mut self) {
        self.access_count += 1;
        self.last_access_time = Instant::now();

        // Check if we should promote to HOT
        if self.residency == KvResidency::Cold {
            let should_promote = self.access_count >= HOT_PROMOTION_THRESHOLD
                || self.last_access_time.elapsed() < HOT_RECENCY_WINDOW;

            if should_promote {
                self.set_residency(KvResidency::Hot);
            }
        }
    }

    /// Check if this cache entry should be demoted to COLD
    ///
    /// HOT entries that haven't been accessed recently may be demoted
    /// to free up non-purgeable memory slots.
    pub fn should_demote(&self) -> bool {
        self.residency == KvResidency::Hot
            && self.last_access_time.elapsed() > COLD_DEMOTION_IDLE_TIME
    }

    /// Get access statistics
    pub fn access_stats(&self) -> (usize, Instant) {
        (self.access_count, self.last_access_time)
    }
}

/// KV cache manager for all transformer layers
pub struct KVCache {
    device: Arc<Device>,
    /// Per-layer caches
    layers: Vec<LayerKVCache>,
    /// Cache configuration
    config: KVCacheConfig,
    /// Pipeline state for cache update kernel
    update_pipeline: Option<ComputePipelineState>,
    /// Pipeline state for cache read kernel
    read_pipeline: Option<ComputePipelineState>,
    /// Command queue for cache operations
    command_queue: CommandQueue,
}

impl KVCache {
    /// Create new KV cache with Metal buffers
    pub fn new(device: Arc<Device>, config: KVCacheConfig) -> Result<Self> {
        let command_queue = device.new_command_queue();

        // Pre-allocate all layer caches
        let mut layers = Vec::with_capacity(config.num_layers);
        for _ in 0..config.num_layers {
            layers.push(LayerKVCache::new(&device, &config)?);
        }

        tracing::info!(
            num_layers = config.num_layers,
            max_seq_len = config.max_seq_len,
            total_bytes = config.total_bytes(),
            "Created KV cache"
        );

        Ok(Self {
            device,
            layers,
            config,
            update_pipeline: None,
            read_pipeline: None,
            command_queue,
        })
    }

    /// Create KV cache from GQA config
    pub fn from_gqa_config(
        device: Arc<Device>,
        gqa_config: &GqaConfig,
        num_layers: usize,
        max_seq_len: usize,
    ) -> Result<Self> {
        let config = KVCacheConfig::from_gqa_config(gqa_config, num_layers, max_seq_len);
        Self::new(device, config)
    }

    /// Initialize cache update and read pipelines from metallib
    pub fn init_pipelines(&mut self, library: &Library) -> Result<()> {
        // Try to load kv_cache_update kernel
        if let Ok(function) = library.get_function("kv_cache_update", None) {
            let pipeline = self
                .device
                .new_compute_pipeline_state_with_function(&function)
                .map_err(|e| {
                    AosError::Kernel(format!("Failed to create kv_cache_update pipeline: {}", e))
                })?;
            self.update_pipeline = Some(pipeline);
            tracing::info!("Created KV cache update pipeline");
        } else {
            tracing::warn!("kv_cache_update function not found in metallib");
        }

        // Try to load kv_cache_read kernel
        if let Ok(function) = library.get_function("kv_cache_read", None) {
            let pipeline = self
                .device
                .new_compute_pipeline_state_with_function(&function)
                .map_err(|e| {
                    AosError::Kernel(format!("Failed to create kv_cache_read pipeline: {}", e))
                })?;
            self.read_pipeline = Some(pipeline);
            tracing::info!("Created KV cache read pipeline");
        } else {
            tracing::warn!("kv_cache_read function not found in metallib");
        }

        Ok(())
    }

    /// Get cache configuration
    pub fn config(&self) -> &KVCacheConfig {
        &self.config
    }

    /// Get number of layers
    pub fn num_layers(&self) -> usize {
        self.layers.len()
    }

    /// Get current sequence position (same for all layers)
    pub fn seq_pos(&self) -> usize {
        self.layers.first().map(|l| l.seq_pos).unwrap_or(0)
    }

    /// Get remaining capacity in tokens
    pub fn remaining_capacity(&self) -> usize {
        self.layers
            .first()
            .map(|l| l.remaining_capacity())
            .unwrap_or(0)
    }

    /// Check if cache can accept more tokens
    pub fn can_append(&self, num_tokens: usize) -> bool {
        self.layers
            .first()
            .map(|l| l.can_append(num_tokens))
            .unwrap_or(false)
    }

    /// Get total memory usage in bytes
    pub fn memory_usage(&self) -> u64 {
        self.config.total_bytes()
    }

    /// Update cache for a specific layer with new K/V tensors
    ///
    /// # Arguments
    /// * `layer_idx` - Layer index (0-based)
    /// * `new_keys` - New key tensor [batch, num_kv_heads, num_new_tokens, head_dim]
    /// * `new_values` - New value tensor [batch, num_kv_heads, num_new_tokens, head_dim]
    /// * `num_new_tokens` - Number of new tokens to append
    pub fn update(
        &mut self,
        layer_idx: usize,
        new_keys: &Buffer,
        new_values: &Buffer,
        num_new_tokens: usize,
    ) -> Result<()> {
        if layer_idx >= self.layers.len() {
            return Err(AosError::Validation(format!(
                "Layer index {} out of range (max {})",
                layer_idx,
                self.layers.len() - 1
            )));
        }

        let layer = &mut self.layers[layer_idx];
        if !layer.can_append(num_new_tokens) {
            return Err(AosError::Validation(format!(
                "KV cache overflow: cannot append {} tokens (capacity: {}, used: {})",
                num_new_tokens, layer.max_seq_len, layer.seq_pos
            )));
        }

        // Calculate byte offsets for append position
        let bytes_per_token = (self.config.batch_size
            * self.config.num_kv_heads
            * self.config.head_dim
            * std::mem::size_of::<f32>()) as u64;
        let offset = layer.seq_pos as u64 * bytes_per_token;
        let copy_size = num_new_tokens as u64 * bytes_per_token;

        // Use blit encoder to copy new K/V to cache
        let command_buffer = self.command_queue.new_command_buffer();
        let blit_encoder = command_buffer.new_blit_command_encoder();

        // Copy keys
        blit_encoder.copy_from_buffer(new_keys, 0, &layer.key_cache, offset, copy_size);

        // Copy values
        blit_encoder.copy_from_buffer(new_values, 0, &layer.value_cache, offset, copy_size);

        blit_encoder.end_encoding();
        command_buffer.commit();
        command_buffer.wait_until_completed();

        // Update sequence position
        layer.seq_pos += num_new_tokens;

        tracing::trace!(
            layer = layer_idx,
            num_tokens = num_new_tokens,
            seq_pos = layer.seq_pos,
            "Updated KV cache"
        );

        Ok(())
    }

    /// Get cached K/V buffers for a specific layer
    ///
    /// Returns references to the key and value cache buffers along with
    /// the current sequence position. Records access for residency tracking.
    pub fn get_layer_cache(&mut self, layer_idx: usize) -> Option<(&Buffer, &Buffer, usize)> {
        if let Some(layer) = self.layers.get_mut(layer_idx) {
            layer.record_access();
            Some((&layer.key_cache, &layer.value_cache, layer.seq_pos))
        } else {
            None
        }
    }

    /// Get mutable reference to layer cache
    pub fn get_layer_cache_mut(&mut self, layer_idx: usize) -> Option<&mut LayerKVCache> {
        self.layers.get_mut(layer_idx)
    }

    /// Reset all layer caches (for new generation)
    pub fn reset(&mut self) {
        for layer in &mut self.layers {
            layer.reset();
        }
        tracing::debug!("Reset KV cache");
    }

    /// Clear and deallocate cache (for memory recovery)
    pub fn clear(&mut self) {
        self.layers.clear();
        tracing::info!("Cleared KV cache");
    }

    /// Trim cache to specific sequence length (for context window sliding)
    ///
    /// Useful when implementing sliding window attention or context compression.
    pub fn trim_to(&mut self, new_seq_len: usize) -> Result<()> {
        for layer in &mut self.layers {
            if new_seq_len > layer.seq_pos {
                return Err(AosError::Validation(format!(
                    "Cannot trim to {} tokens, only {} cached",
                    new_seq_len, layer.seq_pos
                )));
            }
            layer.seq_pos = new_seq_len;
        }
        tracing::debug!(new_seq_len, "Trimmed KV cache");
        Ok(())
    }

    /// Get device name
    pub fn device_name(&self) -> &str {
        self.device.name()
    }

    /// Perform residency maintenance: demote idle HOT entries to COLD
    ///
    /// This should be called periodically (e.g., every few seconds) to
    /// free up non-purgeable memory slots for more active entries.
    ///
    /// Returns the number of layers demoted from HOT to COLD.
    pub fn maintain_residency(&mut self) -> usize {
        let mut demoted_count = 0;

        for layer in &mut self.layers {
            if layer.should_demote() {
                layer.set_residency(KvResidency::Cold);
                demoted_count += 1;
            }
        }

        if demoted_count > 0 {
            tracing::debug!(
                demoted_layers = demoted_count,
                "Demoted idle HOT entries to COLD"
            );
        }

        demoted_count
    }

    /// Get residency statistics for all layers
    ///
    /// Returns (hot_count, cold_count, total_hot_bytes, total_cold_bytes)
    pub fn residency_stats(&self) -> (usize, usize, u64, u64) {
        let mut hot_count = 0;
        let mut cold_count = 0;
        let bytes_per_layer = self.config.bytes_per_layer() * 2; // K + V

        for layer in &self.layers {
            match layer.residency() {
                KvResidency::Hot => hot_count += 1,
                KvResidency::Cold => cold_count += 1,
            }
        }

        let hot_bytes = hot_count as u64 * bytes_per_layer;
        let cold_bytes = cold_count as u64 * bytes_per_layer;

        (hot_count, cold_count, hot_bytes, cold_bytes)
    }
}

/// Integration with FlashAttentionKernel for cached attention
pub struct CachedFlashAttention {
    /// The underlying flash attention kernel
    flash_kernel: super::fused_qkv::FlashAttentionKernel,
    /// KV cache
    kv_cache: KVCache,
    /// Device reference
    device: Arc<Device>,
    /// Command queue
    command_queue: CommandQueue,
}

impl CachedFlashAttention {
    /// Create new cached flash attention
    pub fn new(
        device: Arc<Device>,
        gqa_config: GqaConfig,
        num_layers: usize,
        max_seq_len: usize,
    ) -> Result<Self> {
        let flash_kernel =
            super::fused_qkv::FlashAttentionKernel::new(device.clone(), gqa_config.clone())?;
        let kv_cache =
            KVCache::from_gqa_config(device.clone(), &gqa_config, num_layers, max_seq_len)?;
        let command_queue = device.new_command_queue();

        Ok(Self {
            flash_kernel,
            kv_cache,
            device,
            command_queue,
        })
    }

    /// Execute attention with KV cache for a single layer
    ///
    /// # Arguments
    /// * `layer_idx` - Layer index
    /// * `q` - Query tensor for new tokens [batch, num_heads, num_new_tokens, head_dim]
    /// * `k` - Key tensor for new tokens [batch, num_kv_heads, num_new_tokens, head_dim]
    /// * `v` - Value tensor for new tokens [batch, num_kv_heads, num_new_tokens, head_dim]
    /// * `output` - Output buffer for attention result
    /// * `num_new_tokens` - Number of new tokens being processed
    pub fn execute(
        &mut self,
        layer_idx: usize,
        q: &Buffer,
        k: &Buffer,
        v: &Buffer,
        output: &Buffer,
        num_new_tokens: usize,
    ) -> Result<()> {
        // Update cache with new K/V
        self.kv_cache.update(layer_idx, k, v, num_new_tokens)?;

        // Get cached K/V for full sequence attention
        let (cached_k, cached_v, seq_pos) = self
            .kv_cache
            .get_layer_cache(layer_idx)
            .ok_or_else(|| AosError::Kernel(format!("Layer {} not found in cache", layer_idx)))?;

        // Execute flash attention with full cached sequence
        // Q is only for new tokens, K/V includes full cached sequence
        self.flash_kernel.execute(q, cached_k, cached_v, output)?;

        tracing::trace!(
            layer = layer_idx,
            seq_pos = seq_pos,
            num_new_tokens = num_new_tokens,
            "Executed cached flash attention"
        );

        Ok(())
    }

    /// Reset cache for new generation
    pub fn reset_cache(&mut self) {
        self.kv_cache.reset();
    }

    /// Get current sequence position
    pub fn seq_pos(&self) -> usize {
        self.kv_cache.seq_pos()
    }

    /// Get remaining capacity
    pub fn remaining_capacity(&self) -> usize {
        self.kv_cache.remaining_capacity()
    }

    /// Get memory usage
    pub fn memory_usage(&self) -> u64 {
        self.kv_cache.memory_usage()
    }

    /// Get reference to underlying KV cache
    pub fn cache(&self) -> &KVCache {
        &self.kv_cache
    }

    /// Get mutable reference to underlying KV cache
    pub fn cache_mut(&mut self) -> &mut KVCache {
        &mut self.kv_cache
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kv_cache_config_default() {
        let config = KVCacheConfig::default();
        assert_eq!(config.num_layers, 28);
        assert_eq!(config.max_seq_len, 4096);
        assert_eq!(config.num_kv_heads, 4);
        assert_eq!(config.head_dim, 128);
        assert_eq!(config.batch_size, 1);
    }

    #[test]
    fn test_kv_cache_config_bytes() {
        let config = KVCacheConfig {
            num_layers: 1,
            max_seq_len: 100,
            num_kv_heads: 4,
            head_dim: 128,
            batch_size: 1,
        };

        // 1 * 4 * 100 * 128 * 4 bytes = 204800 bytes per layer
        assert_eq!(config.bytes_per_layer(), 204800);
        // 204800 * 2 (K+V) * 1 layer = 409600 bytes total
        assert_eq!(config.total_bytes(), 409600);
    }

    #[test]
    fn test_kv_cache_config_from_gqa() {
        let gqa_config = GqaConfig::default();
        let config = KVCacheConfig::from_gqa_config(&gqa_config, 28, 2048);

        assert_eq!(config.num_layers, 28);
        assert_eq!(config.max_seq_len, 2048);
        assert_eq!(config.num_kv_heads, gqa_config.num_key_value_heads as usize);
        assert_eq!(config.head_dim, gqa_config.head_dim as usize);
    }

    #[test]
    fn test_kv_cache_creation() {
        let device = Device::system_default().expect("Metal device should be available");
        let config = KVCacheConfig {
            num_layers: 2,
            max_seq_len: 100,
            num_kv_heads: 4,
            head_dim: 128,
            batch_size: 1,
        };

        let cache =
            KVCache::new(Arc::new(device), config).expect("KV cache creation should succeed");

        assert_eq!(cache.num_layers(), 2);
        assert_eq!(cache.seq_pos(), 0);
        assert_eq!(cache.remaining_capacity(), 100);
        assert!(cache.can_append(50));
        assert!(!cache.can_append(101));
    }

    #[test]
    fn test_kv_cache_reset() {
        let device = Device::system_default().expect("Metal device should be available");
        let config = KVCacheConfig {
            num_layers: 1,
            max_seq_len: 100,
            num_kv_heads: 4,
            head_dim: 128,
            batch_size: 1,
        };

        let mut cache =
            KVCache::new(Arc::new(device), config).expect("KV cache creation should succeed");

        // Simulate updating the cache
        if let Some(layer) = cache.get_layer_cache_mut(0) {
            layer.seq_pos = 50;
        }

        assert_eq!(cache.seq_pos(), 50);

        cache.reset();
        assert_eq!(cache.seq_pos(), 0);
        assert_eq!(cache.remaining_capacity(), 100);
    }

    #[test]
    fn test_layer_cache_capacity() {
        let device = Device::system_default().expect("Metal device should be available");
        let config = KVCacheConfig {
            num_layers: 1,
            max_seq_len: 100,
            num_kv_heads: 4,
            head_dim: 128,
            batch_size: 1,
        };

        let cache =
            LayerKVCache::new(&device, &config).expect("Layer cache creation should succeed");

        assert_eq!(cache.seq_pos, 0);
        assert_eq!(cache.max_seq_len, 100);
        assert!(cache.can_append(100));
        assert!(!cache.can_append(101));
        assert_eq!(cache.remaining_capacity(), 100);
    }
}
