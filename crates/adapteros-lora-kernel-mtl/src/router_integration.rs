//! Router Integration Utilities for CoreML Backend
//!
//! This module provides utilities for converting router decisions into CoreML-optimized
//! execution plans, with optimizations for common routing patterns and ANE scheduling.

use adapteros_core::{AosError, B3Hash, Result};
use std::collections::HashMap;
use tracing::{debug, info};

/// Convert Q15 gate weights to CoreML-compatible float32 weights
#[inline]
pub fn q15_gates_to_weights(gates_q15: &[i16]) -> Vec<f32> {
    gates_q15.iter().map(|&g| (g as f32) / 32767.0).collect()
}

/// Map adapter indices to CoreML model handles
///
/// This maintains a mapping from adapter IDs to their corresponding CoreML
/// compiled model handles for fast lookup during inference.
pub struct AdapterModelMapper {
    /// Adapter ID -> CoreML model handle
    adapter_models: HashMap<u16, CompiledModelHandle>,
    /// Cache hits (for telemetry)
    cache_hits: u64,
    /// Cache misses (for telemetry)
    cache_misses: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct CompiledModelHandle {
    pub ptr: usize,
    pub adapter_id: u16,
}

impl CompiledModelHandle {
    pub fn new(ptr: usize, adapter_id: u16) -> Self {
        Self { ptr, adapter_id }
    }

    pub fn as_ptr(&self) -> *mut std::ffi::c_void {
        self.ptr as *mut std::ffi::c_void
    }
}

impl AdapterModelMapper {
    pub fn new() -> Self {
        Self {
            adapter_models: HashMap::new(),
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// Register an adapter with its CoreML model handle
    pub fn register(&mut self, adapter_id: u16, model_ptr: usize) {
        let handle = CompiledModelHandle::new(model_ptr, adapter_id);
        self.adapter_models.insert(adapter_id, handle);
        info!(
            adapter_id = adapter_id,
            total_adapters = self.adapter_models.len(),
            "Adapter model registered in mapper"
        );
    }

    /// Get model handle for an adapter
    pub fn get(&mut self, adapter_id: u16) -> Option<CompiledModelHandle> {
        if let Some(&handle) = self.adapter_models.get(&adapter_id) {
            self.cache_hits += 1;
            Some(handle)
        } else {
            self.cache_misses += 1;
            None
        }
    }

    /// Remove an adapter from the mapping
    pub fn remove(&mut self, adapter_id: u16) -> Option<CompiledModelHandle> {
        self.adapter_models.remove(&adapter_id)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> (u64, u64, f32) {
        let total = self.cache_hits + self.cache_misses;
        let hit_rate = if total > 0 {
            (self.cache_hits as f32) / (total as f32)
        } else {
            0.0
        };
        (self.cache_hits, self.cache_misses, hit_rate)
    }

    /// Clear all mappings
    pub fn clear(&mut self) {
        self.adapter_models.clear();
        debug!("Adapter model mapper cleared");
    }
}

impl Default for AdapterModelMapper {
    fn default() -> Self {
        Self::new()
    }
}

/// Batch router decisions for efficient ANE scheduling
///
/// Groups multiple routing decisions together to minimize ANE invocation overhead
/// and maximize parallelism.
pub struct RouterDecisionBatcher {
    /// Current batch of decisions
    batch: Vec<RouterDecisionBatch>,
    /// Maximum batch size
    max_batch_size: usize,
    /// Total decisions processed
    total_decisions: u64,
}

#[derive(Debug, Clone)]
pub struct RouterDecisionBatch {
    pub adapter_indices: Vec<u16>,
    pub gate_weights: Vec<f32>,
    pub input_token_ids: Vec<u32>,
    pub batch_id: u64,
}

impl RouterDecisionBatcher {
    pub fn new(max_batch_size: usize) -> Self {
        Self {
            batch: Vec::new(),
            max_batch_size,
            total_decisions: 0,
        }
    }

    /// Add a routing decision to the current batch
    pub fn add(
        &mut self,
        adapter_indices: Vec<u16>,
        gate_weights: Vec<f32>,
        input_token_ids: Vec<u32>,
    ) {
        let decision = RouterDecisionBatch {
            adapter_indices,
            gate_weights,
            input_token_ids,
            batch_id: self.total_decisions,
        };

        self.batch.push(decision);
        self.total_decisions += 1;

        debug!(
            batch_size = self.batch.len(),
            batch_id = self.total_decisions - 1,
            "Router decision added to batch"
        );
    }

    /// Check if batch is ready for execution
    pub fn is_ready(&self) -> bool {
        self.batch.len() >= self.max_batch_size
    }

    /// Flush the current batch
    pub fn flush(&mut self) -> Vec<RouterDecisionBatch> {
        let batch = std::mem::take(&mut self.batch);
        debug!(
            batch_size = batch.len(),
            total_decisions = self.total_decisions,
            "Router decision batch flushed"
        );
        batch
    }

    /// Get current batch size
    pub fn len(&self) -> usize {
        self.batch.len()
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.batch.is_empty()
    }
}

/// Cache for frequently used adapter combinations
///
/// Optimizes repeated routing patterns by precomputing gate weight combinations
/// and caching execution plans.
pub struct RouterPatternCache {
    /// Pattern hash -> cached gate weights
    pattern_cache: HashMap<B3Hash, Vec<f32>>,
    /// Hit count for telemetry
    cache_hits: u64,
    /// Miss count for telemetry
    cache_misses: u64,
    /// Maximum cache size (number of patterns)
    max_cache_size: usize,
}

impl RouterPatternCache {
    pub fn new(max_cache_size: usize) -> Self {
        Self {
            pattern_cache: HashMap::new(),
            cache_hits: 0,
            cache_misses: 0,
            max_cache_size,
        }
    }

    /// Compute hash for a routing pattern (indices + gates)
    pub fn pattern_hash(indices: &[u16], gates_q15: &[i16]) -> B3Hash {
        let mut data = Vec::new();
        for &idx in indices {
            data.extend_from_slice(&idx.to_le_bytes());
        }
        for &gate in gates_q15 {
            data.extend_from_slice(&gate.to_le_bytes());
        }
        B3Hash::hash(&data)
    }

    /// Get cached gate weights for a pattern
    pub fn get(&mut self, indices: &[u16], gates_q15: &[i16]) -> Option<Vec<f32>> {
        let hash = Self::pattern_hash(indices, gates_q15);

        if let Some(weights) = self.pattern_cache.get(&hash) {
            self.cache_hits += 1;
            debug!(
                pattern_hash = %hash.to_short_hex(),
                k = indices.len(),
                "Router pattern cache hit"
            );
            Some(weights.clone())
        } else {
            self.cache_misses += 1;
            None
        }
    }

    /// Store gate weights for a pattern
    pub fn put(&mut self, indices: &[u16], gates_q15: &[i16], weights: Vec<f32>) -> Result<()> {
        if self.pattern_cache.len() >= self.max_cache_size {
            // Evict oldest entry (simple FIFO policy)
            if let Some(key) = self.pattern_cache.keys().next().cloned() {
                self.pattern_cache.remove(&key);
                debug!("Router pattern cache eviction (size limit reached)");
            }
        }

        let hash = Self::pattern_hash(indices, gates_q15);
        self.pattern_cache.insert(hash, weights);

        debug!(
            pattern_hash = %hash.to_short_hex(),
            k = indices.len(),
            cache_size = self.pattern_cache.len(),
            "Router pattern cached"
        );

        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> RouterPatternCacheStats {
        let total = self.cache_hits + self.cache_misses;
        let hit_rate = if total > 0 {
            (self.cache_hits as f32) / (total as f32)
        } else {
            0.0
        };

        RouterPatternCacheStats {
            cache_hits: self.cache_hits,
            cache_misses: self.cache_misses,
            hit_rate,
            cache_size: self.pattern_cache.len(),
            max_cache_size: self.max_cache_size,
        }
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.pattern_cache.clear();
        self.cache_hits = 0;
        self.cache_misses = 0;
        debug!("Router pattern cache cleared");
    }
}

#[derive(Debug, Clone)]
pub struct RouterPatternCacheStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub hit_rate: f32,
    pub cache_size: usize,
    pub max_cache_size: usize,
}

impl Default for RouterPatternCache {
    fn default() -> Self {
        Self::new(1024) // Default cache size: 1024 patterns
    }
}

/// Optimize gate weight computation for common patterns
pub struct GateWeightOptimizer;

impl GateWeightOptimizer {
    /// Precompute gate combinations for k=2 (most common multi-adapter case)
    pub fn precompute_k2_weights(gate1_q15: i16, gate2_q15: i16) -> (f32, f32) {
        let w1 = (gate1_q15 as f32) / 32767.0;
        let w2 = (gate2_q15 as f32) / 32767.0;
        (w1, w2)
    }

    /// Precompute gate combinations for k=4 (medium complexity)
    pub fn precompute_k4_weights(gates_q15: &[i16; 4]) -> [f32; 4] {
        [
            (gates_q15[0] as f32) / 32767.0,
            (gates_q15[1] as f32) / 32767.0,
            (gates_q15[2] as f32) / 32767.0,
            (gates_q15[3] as f32) / 32767.0,
        ]
    }

    /// Precompute gate combinations for k=8 (maximum adapters)
    pub fn precompute_k8_weights(gates_q15: &[i16; 8]) -> [f32; 8] {
        [
            (gates_q15[0] as f32) / 32767.0,
            (gates_q15[1] as f32) / 32767.0,
            (gates_q15[2] as f32) / 32767.0,
            (gates_q15[3] as f32) / 32767.0,
            (gates_q15[4] as f32) / 32767.0,
            (gates_q15[5] as f32) / 32767.0,
            (gates_q15[6] as f32) / 32767.0,
            (gates_q15[7] as f32) / 32767.0,
        ]
    }

    /// Validate gate weights sum to approximately 1.0
    pub fn validate_weights(weights: &[f32]) -> Result<()> {
        let sum: f32 = weights.iter().sum();
        const TOLERANCE: f32 = 0.01; // Allow 1% deviation

        if (sum - 1.0).abs() > TOLERANCE {
            return Err(AosError::Validation(format!(
                "Gate weights sum to {}, expected ~1.0 (tolerance: {})",
                sum, TOLERANCE
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_q15_gates_to_weights() {
        let gates_q15 = vec![32767, 16384, 8192];
        let weights = q15_gates_to_weights(&gates_q15);

        assert_eq!(weights.len(), 3);
        assert!((weights[0] - 1.0).abs() < 0.001);
        assert!((weights[1] - 0.5).abs() < 0.001);
        assert!((weights[2] - 0.25).abs() < 0.001);
    }

    #[test]
    fn test_adapter_model_mapper() {
        let mut mapper = AdapterModelMapper::new();

        mapper.register(0, 0x1000);
        mapper.register(1, 0x2000);

        let handle0 = mapper.get(0).unwrap();
        assert_eq!(handle0.ptr, 0x1000);

        let handle1 = mapper.get(1).unwrap();
        assert_eq!(handle1.ptr, 0x2000);

        let (hits, misses, hit_rate) = mapper.cache_stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 0);
        assert_eq!(hit_rate, 1.0);

        assert!(mapper.get(2).is_none());
        let (hits, misses, _) = mapper.cache_stats();
        assert_eq!(hits, 2);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_router_decision_batcher() {
        let mut batcher = RouterDecisionBatcher::new(3);

        assert!(!batcher.is_ready());
        assert_eq!(batcher.len(), 0);

        batcher.add(vec![0], vec![1.0], vec![100]);
        batcher.add(vec![1], vec![1.0], vec![101]);
        assert!(!batcher.is_ready());
        assert_eq!(batcher.len(), 2);

        batcher.add(vec![2], vec![1.0], vec![102]);
        assert!(batcher.is_ready());
        assert_eq!(batcher.len(), 3);

        let batch = batcher.flush();
        assert_eq!(batch.len(), 3);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_router_pattern_cache() {
        let mut cache = RouterPatternCache::new(2);

        let indices1 = vec![0, 1];
        let gates1 = vec![16384, 16383];
        let weights1 = vec![0.5, 0.5];

        assert!(cache.get(&indices1, &gates1).is_none());
        cache.put(&indices1, &gates1, weights1.clone()).unwrap();

        let cached = cache.get(&indices1, &gates1).unwrap();
        assert_eq!(cached, weights1);

        let stats = cache.stats();
        assert_eq!(stats.cache_hits, 1);
        assert_eq!(stats.cache_misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }

    #[test]
    fn test_gate_weight_optimizer_k2() {
        let (w1, w2) = GateWeightOptimizer::precompute_k2_weights(16384, 16383);
        assert!((w1 - 0.5).abs() < 0.001);
        assert!((w2 - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_gate_weight_optimizer_k4() {
        let gates = [8192, 8192, 8192, 8191];
        let weights = GateWeightOptimizer::precompute_k4_weights(&gates);

        for i in 0..4 {
            assert!((weights[i] - 0.25).abs() < 0.001);
        }
    }

    #[test]
    fn test_gate_weight_validation() {
        let valid_weights = vec![0.5, 0.5];
        assert!(GateWeightOptimizer::validate_weights(&valid_weights).is_ok());

        let invalid_weights = vec![0.6, 0.6];
        assert!(GateWeightOptimizer::validate_weights(&invalid_weights).is_err());
    }

    #[test]
    fn test_pattern_cache_eviction() {
        let mut cache = RouterPatternCache::new(2);

        cache.put(&[0], &[32767], vec![1.0]).unwrap();
        cache.put(&[1], &[32767], vec![1.0]).unwrap();

        let stats = cache.stats();
        assert_eq!(stats.cache_size, 2);

        cache.put(&[2], &[32767], vec![1.0]).unwrap();

        let stats = cache.stats();
        assert_eq!(stats.cache_size, 2);
    }
}
