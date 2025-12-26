//! MoE Prefix Cache - Extended cache for Mixture of Experts models
//!
//! This module extends the base `PrefixKvCache` with MoE-specific capabilities:
//! - Expert routing indices per token
//! - Expert heat maps for pre-warming
//! - Pre-computed "free tokens" for ultra-low-latency first tokens
//!
//! ## Free Token Concept
//!
//! Static LoRA adapters often produce predictable initial tokens based on their
//! training domain. By caching these tokens and their expert routing patterns,
//! we can:
//! 1. Pre-warm the relevant experts before inference
//! 2. Deliver the first few tokens immediately without model computation
//!
//! ## Example
//!
//! ```ignore
//! // Build cache with free tokens for a Python code adapter
//! let entry = MoEPrefixEntry::builder()
//!     .with_kv_cache(kv_entry)
//!     .with_expert_indices(routing)
//!     .with_free_tokens(vec![
//!         FreeToken::new("\n", 0.95),
//!         FreeToken::new("    ", 0.90),
//!         FreeToken::new("def ", 0.85),
//!     ])
//!     .build()?;
//! ```

use adapteros_core::{AosError, B3Hash, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::moe_types::{ExpertId, ExpertRouting, LayerIdx, SequenceExpertRouting};

// =============================================================================
// Expert Heat Map
// =============================================================================

/// Expert activation frequency for pre-warming decisions
#[derive(Debug, Clone, Default)]
pub struct ExpertHeatMap {
    /// Per-layer activation counts: layer_idx -> (expert_id -> activation_count)
    pub per_layer: Vec<HashMap<u8, u32>>,

    /// Top-K hot experts per layer (pre-computed for fast access)
    /// Dimensions: [num_layers][top_k]
    pub hot_experts: Vec<Vec<u8>>,

    /// Routing stability score (0.0-1.0)
    /// Higher values indicate more predictable routing patterns
    pub routing_stability: f32,

    /// Total tokens observed when building this heat map
    pub sample_count: u32,
}

impl ExpertHeatMap {
    /// Create a new empty heat map for the given number of layers
    pub fn new(num_layers: usize) -> Self {
        Self {
            per_layer: vec![HashMap::new(); num_layers],
            hot_experts: vec![Vec::new(); num_layers],
            routing_stability: 0.0,
            sample_count: 0,
        }
    }

    /// Record an expert activation
    pub fn record_activation(&mut self, layer_idx: usize, expert_id: u8) {
        if layer_idx < self.per_layer.len() {
            *self.per_layer[layer_idx].entry(expert_id).or_insert(0) += 1;
        }
    }

    /// Record multiple expert activations for a token
    pub fn record_token_routing(&mut self, expert_routing: &[(LayerIdx, ExpertId)]) {
        for &(layer_idx, expert_id) in expert_routing {
            self.record_activation(layer_idx, expert_id);
        }
        self.sample_count += 1;
    }

    /// Compute hot experts and stability after collecting samples
    ///
    /// # Deterministic Ordering
    ///
    /// When multiple experts have identical activation counts, they are ordered
    /// by expert_id ascending (lower IDs first). This ensures deterministic
    /// hot expert lists across runs regardless of HashMap iteration order.
    pub fn finalize(&mut self, top_k: usize) {
        // Compute hot experts per layer
        for layer_idx in 0..self.per_layer.len() {
            let layer_counts = &self.per_layer[layer_idx];
            let mut sorted: Vec<_> = layer_counts.iter().collect();
            // Primary sort: count descending (most activated first)
            // Secondary sort: expert_id ascending (deterministic tie-break)
            sorted.sort_by(|a, b| match b.1.cmp(a.1) {
                std::cmp::Ordering::Equal => a.0.cmp(b.0),
                other => other,
            });

            self.hot_experts[layer_idx] = sorted
                .into_iter()
                .take(top_k)
                .map(|(&expert_id, _)| expert_id)
                .collect();
        }

        // Compute routing stability
        // Stability = average concentration of activations (entropy-based)
        if self.sample_count > 0 {
            let mut total_concentration = 0.0f32;
            let mut layer_count = 0;

            for layer_counts in &self.per_layer {
                if layer_counts.is_empty() {
                    continue;
                }

                let total: u32 = layer_counts.values().sum();
                if total == 0 {
                    continue;
                }

                // Compute normalized entropy
                let max_count = *layer_counts.values().max().unwrap_or(&1) as f32;
                let concentration = max_count / total as f32;
                total_concentration += concentration;
                layer_count += 1;
            }

            self.routing_stability = if layer_count > 0 {
                total_concentration / layer_count as f32
            } else {
                0.0
            };
        }
    }

    /// Get the hot experts for a specific layer
    pub fn get_hot_experts(&self, layer_idx: usize) -> &[u8] {
        self.hot_experts
            .get(layer_idx)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if routing is stable enough for free token predictions
    pub fn is_stable(&self, threshold: f32) -> bool {
        self.routing_stability >= threshold && self.sample_count >= 10
    }

    /// Merge another heat map into this one
    pub fn merge(&mut self, other: &ExpertHeatMap) {
        // Extend layers if needed
        while self.per_layer.len() < other.per_layer.len() {
            self.per_layer.push(HashMap::new());
            self.hot_experts.push(Vec::new());
        }

        // Merge counts
        for (layer_idx, other_counts) in other.per_layer.iter().enumerate() {
            for (&expert_id, &count) in other_counts {
                *self.per_layer[layer_idx].entry(expert_id).or_insert(0) += count;
            }
        }

        self.sample_count += other.sample_count;
    }

    /// Get memory usage in bytes
    pub fn memory_bytes(&self) -> usize {
        let per_layer_bytes: usize = self
            .per_layer
            .iter()
            .map(|m| m.len() * (std::mem::size_of::<u8>() + std::mem::size_of::<u32>()))
            .sum();
        let hot_experts_bytes: usize = self
            .hot_experts
            .iter()
            .map(|v| v.len() * std::mem::size_of::<u8>())
            .sum();
        per_layer_bytes + hot_experts_bytes + std::mem::size_of::<Self>()
    }
}

// =============================================================================
// Free Token Types
// =============================================================================

/// Source of free token prediction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FreeTokenSource {
    /// Explicitly declared in adapter manifest
    ManifestDeclared,
    /// Derived from adapter training data patterns
    AdapterTrainingData,
    /// Learned from runtime observation
    RuntimeLearned {
        /// Number of samples used to learn this pattern
        sample_count: u32,
    },
}

/// A pre-computed "free" token that can be delivered without model computation
#[derive(Debug, Clone)]
pub struct FreeToken {
    /// Token text
    pub text: String,
    /// Token ID in vocabulary
    pub token_id: u32,
    /// Confidence that this token will match model output (0.0-1.0)
    pub confidence: f32,
    /// Pre-computed logits for this token (optional)
    pub logits: Option<Vec<f32>>,
    /// Expert routing for this token (layer_idx, expert_id)
    pub expert_routing: ExpertRouting,
}

impl FreeToken {
    /// Create a new free token with high confidence
    pub fn new(text: impl Into<String>, token_id: u32, confidence: f32) -> Self {
        Self {
            text: text.into(),
            token_id,
            confidence,
            logits: None,
            expert_routing: Vec::new(),
        }
    }

    /// Add expert routing information
    pub fn with_expert_routing(mut self, expert_routing: ExpertRouting) -> Self {
        self.expert_routing = expert_routing;
        self
    }

    /// Add pre-computed logits
    pub fn with_logits(mut self, logits: Vec<f32>) -> Self {
        self.logits = Some(logits);
        self
    }
}

/// Pre-computed continuation tokens for an adapter
#[derive(Debug, Clone)]
pub struct PrecomputedTokens {
    /// Ordered sequence of free tokens
    pub tokens: Vec<FreeToken>,
    /// Overall confidence for this sequence (product of individual confidences)
    pub sequence_confidence: f32,
    /// How these tokens were determined
    pub source: FreeTokenSource,
    /// Maximum temperature at which these predictions are valid
    pub max_temperature: f32,
    /// Context hash this prediction is valid for (None = any context)
    pub context_hash: Option<B3Hash>,
}

impl PrecomputedTokens {
    /// Create new precomputed tokens
    pub fn new(tokens: Vec<FreeToken>, source: FreeTokenSource) -> Self {
        let sequence_confidence = tokens
            .iter()
            .map(|t| t.confidence)
            .fold(1.0, |acc, c| acc * c);

        Self {
            tokens,
            sequence_confidence,
            source,
            max_temperature: 0.3, // Default: only valid for low temperature
            context_hash: None,
        }
    }

    /// Set the maximum temperature for validity
    pub fn with_max_temperature(mut self, temp: f32) -> Self {
        self.max_temperature = temp;
        self
    }

    /// Set context hash for validity
    pub fn with_context(mut self, hash: B3Hash) -> Self {
        self.context_hash = Some(hash);
        self
    }

    /// Check if these tokens are valid for the given temperature
    pub fn is_valid_for_temperature(&self, temperature: f32) -> bool {
        temperature <= self.max_temperature
    }

    /// Get total memory usage in bytes
    pub fn memory_bytes(&self) -> usize {
        self.tokens
            .iter()
            .map(|t| {
                t.text.len()
                    + t.logits.as_ref().map(|l| l.len() * 4).unwrap_or(0)
                    + t.expert_routing.len() * std::mem::size_of::<(usize, u8)>()
            })
            .sum::<usize>()
            + std::mem::size_of::<Self>()
    }
}

// =============================================================================
// MoE Prefix Entry
// =============================================================================

/// Extended prefix cache entry for MoE models
#[derive(Debug)]
pub struct MoEPrefixEntry {
    /// Per-layer key tensors
    pub keys: Vec<Vec<f32>>,
    /// Per-layer value tensors
    pub values: Vec<Vec<f32>>,
    /// Tenant that owns this entry
    pub tenant_id: String,
    /// Adapter ID this entry is for
    pub adapter_id: Option<String>,
    /// Number of prefix tokens cached
    pub prefix_cached_token_count: u32,
    /// Total bytes of KV data
    pub kv_bytes: u64,
    /// Per-token expert routing indices
    /// Dimensions: [num_tokens][active_experts_per_token] = (layer_idx, expert_id)
    pub expert_routing: SequenceExpertRouting,
    /// Aggregated expert heat map
    pub heat_map: ExpertHeatMap,
    /// Pre-computed free tokens (optional)
    pub free_tokens: Option<PrecomputedTokens>,
    /// Creation timestamp
    pub created_at: Instant,
    /// Last access timestamp
    last_access_ns: AtomicU64,
    /// Active reference count
    pub active_refcount: AtomicU32,
}

impl MoEPrefixEntry {
    /// Create a new MoE prefix entry
    pub fn new(
        keys: Vec<Vec<f32>>,
        values: Vec<Vec<f32>>,
        tenant_id: String,
        prefix_token_count: u32,
        num_layers: usize,
    ) -> Self {
        let kv_bytes = Self::compute_kv_bytes(&keys, &values);

        Self {
            keys,
            values,
            tenant_id,
            adapter_id: None,
            prefix_cached_token_count: prefix_token_count,
            kv_bytes,
            expert_routing: Vec::new(),
            heat_map: ExpertHeatMap::new(num_layers),
            free_tokens: None,
            created_at: Instant::now(),
            last_access_ns: AtomicU64::new(0),
            active_refcount: AtomicU32::new(0),
        }
    }

    /// Compute total KV bytes
    fn compute_kv_bytes(keys: &[Vec<f32>], values: &[Vec<f32>]) -> u64 {
        let key_bytes: usize = keys.iter().map(|k| k.len() * 4).sum();
        let value_bytes: usize = values.iter().map(|v| v.len() * 4).sum();
        (key_bytes + value_bytes) as u64
    }

    /// Set adapter ID
    pub fn with_adapter(mut self, adapter_id: String) -> Self {
        self.adapter_id = Some(adapter_id);
        self
    }

    /// Add expert routing data
    pub fn with_expert_routing(mut self, expert_routing: SequenceExpertRouting) -> Self {
        // Update heat map from routing
        for token_routing in &expert_routing {
            self.heat_map.record_token_routing(token_routing);
        }
        self.expert_routing = expert_routing;
        self
    }

    /// Add pre-computed free tokens
    pub fn with_free_tokens(mut self, tokens: PrecomputedTokens) -> Self {
        self.free_tokens = Some(tokens);
        self
    }

    /// Finalize the entry (compute hot experts, etc.)
    pub fn finalize(mut self, top_k_experts: usize) -> Self {
        self.heat_map.finalize(top_k_experts);
        self
    }

    /// Record an access
    pub fn record_access(&self) {
        let now_ns = self.created_at.elapsed().as_nanos() as u64;
        self.last_access_ns.store(now_ns, Ordering::Relaxed);
    }

    /// Get last access time
    pub fn last_access_ns(&self) -> u64 {
        self.last_access_ns.load(Ordering::Relaxed)
    }

    /// Acquire a reference
    pub fn acquire(&self) -> u32 {
        self.active_refcount.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Release a reference
    pub fn release(&self) -> u32 {
        self.active_refcount.fetch_sub(1, Ordering::SeqCst) - 1
    }

    /// Check if entry is in use
    pub fn is_in_use(&self) -> bool {
        self.active_refcount.load(Ordering::SeqCst) > 0
    }

    /// Get total memory usage
    pub fn total_bytes(&self) -> u64 {
        self.kv_bytes
            + self.heat_map.memory_bytes() as u64
            + self
                .free_tokens
                .as_ref()
                .map(|t| t.memory_bytes() as u64)
                .unwrap_or(0)
            + (self.expert_routing.len() * std::mem::size_of::<Vec<(usize, u8)>>()) as u64
    }

    /// Get free tokens if valid for the given temperature
    pub fn get_free_tokens(&self, temperature: f32) -> Option<&PrecomputedTokens> {
        self.free_tokens
            .as_ref()
            .filter(|t| t.is_valid_for_temperature(temperature))
    }

    /// Check if this entry has stable routing patterns
    pub fn has_stable_routing(&self, threshold: f32) -> bool {
        self.heat_map.is_stable(threshold)
    }
}

// =============================================================================
// Free Token Validation & Metrics
// =============================================================================

/// Metrics for free token optimization
pub struct FreeTokenMetrics {
    /// Total free tokens delivered
    pub tokens_delivered: AtomicU64,
    /// Free tokens that matched actual model output
    pub tokens_validated: AtomicU64,
    /// Free tokens that were rejected (model produced different)
    pub tokens_rejected: AtomicU64,
    /// Estimated latency saved in milliseconds
    pub latency_saved_ms: AtomicU64,
}

impl std::fmt::Debug for FreeTokenMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FreeTokenMetrics")
            .field(
                "tokens_delivered",
                &self.tokens_delivered.load(Ordering::Relaxed),
            )
            .field(
                "tokens_validated",
                &self.tokens_validated.load(Ordering::Relaxed),
            )
            .field(
                "tokens_rejected",
                &self.tokens_rejected.load(Ordering::Relaxed),
            )
            .field(
                "latency_saved_ms",
                &self.latency_saved_ms.load(Ordering::Relaxed),
            )
            .finish()
    }
}

impl Default for FreeTokenMetrics {
    fn default() -> Self {
        Self {
            tokens_delivered: AtomicU64::new(0),
            tokens_validated: AtomicU64::new(0),
            tokens_rejected: AtomicU64::new(0),
            latency_saved_ms: AtomicU64::new(0),
        }
    }
}

impl FreeTokenMetrics {
    /// Create new metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a delivered free token
    pub fn record_delivered(&self) {
        self.tokens_delivered.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a validated free token
    pub fn record_validated(&self, latency_saved_ms: u64) {
        self.tokens_validated.fetch_add(1, Ordering::Relaxed);
        self.latency_saved_ms
            .fetch_add(latency_saved_ms, Ordering::Relaxed);
    }

    /// Record a rejected free token
    pub fn record_rejected(&self) {
        self.tokens_rejected.fetch_add(1, Ordering::Relaxed);
    }

    /// Get accuracy rate (0.0-1.0)
    pub fn accuracy(&self) -> f32 {
        let validated = self.tokens_validated.load(Ordering::Relaxed);
        let rejected = self.tokens_rejected.load(Ordering::Relaxed);
        let total = validated + rejected;
        if total == 0 {
            1.0 // No data yet, assume perfect
        } else {
            validated as f32 / total as f32
        }
    }

    /// Get total latency saved
    pub fn total_latency_saved_ms(&self) -> u64 {
        self.latency_saved_ms.load(Ordering::Relaxed)
    }

    /// Check if free tokens should be disabled (accuracy too low)
    pub fn should_disable(&self, min_accuracy: f32, min_samples: u64) -> bool {
        let validated = self.tokens_validated.load(Ordering::Relaxed);
        let rejected = self.tokens_rejected.load(Ordering::Relaxed);
        let total = validated + rejected;

        total >= min_samples && self.accuracy() < min_accuracy
    }
}

/// Per-adapter metrics tracking
#[derive(Debug, Default)]
pub struct PerAdapterMetrics {
    metrics: RwLock<HashMap<String, Arc<FreeTokenMetrics>>>,
}

impl PerAdapterMetrics {
    /// Create new per-adapter metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create metrics for an adapter
    pub fn get_or_create(&self, adapter_id: &str) -> Arc<FreeTokenMetrics> {
        {
            let metrics = self.metrics.read();
            if let Some(m) = metrics.get(adapter_id) {
                return Arc::clone(m);
            }
        }

        let mut metrics = self.metrics.write();
        metrics
            .entry(adapter_id.to_string())
            .or_insert_with(|| Arc::new(FreeTokenMetrics::new()))
            .clone()
    }

    /// Get accuracy for an adapter
    pub fn get_accuracy(&self, adapter_id: &str) -> Option<f32> {
        self.metrics.read().get(adapter_id).map(|m| m.accuracy())
    }

    /// Check if free tokens should be disabled for an adapter
    pub fn should_disable(&self, adapter_id: &str, min_accuracy: f32, min_samples: u64) -> bool {
        self.metrics
            .read()
            .get(adapter_id)
            .map(|m| m.should_disable(min_accuracy, min_samples))
            .unwrap_or(false)
    }
}

// =============================================================================
// MoE Prefix Cache
// =============================================================================

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct MoEPrefixCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub entry_count: u64,
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub free_tokens_delivered: u64,
    pub free_tokens_validated: u64,
    pub prewarm_count: u64,
}

/// MoE-aware prefix cache
pub struct MoEPrefixCache {
    /// Cache entries keyed by prefix hash
    entries: RwLock<HashMap<B3Hash, Arc<MoEPrefixEntry>>>,
    /// Maximum byte budget
    max_bytes: u64,
    /// Current bytes used
    used_bytes: AtomicU64,
    /// Per-adapter metrics
    adapter_metrics: PerAdapterMetrics,
    /// Global statistics
    stats: RwLock<MoEPrefixCacheStats>,
}

impl MoEPrefixCache {
    /// Create a new MoE prefix cache
    pub fn new(max_bytes: u64) -> Self {
        tracing::info!(
            max_bytes_mb = max_bytes as f64 / (1024.0 * 1024.0),
            "Creating MoE prefix cache"
        );

        Self {
            entries: RwLock::new(HashMap::new()),
            max_bytes,
            used_bytes: AtomicU64::new(0),
            adapter_metrics: PerAdapterMetrics::new(),
            stats: RwLock::new(MoEPrefixCacheStats {
                max_bytes,
                ..Default::default()
            }),
        }
    }

    /// Get an entry from the cache
    pub fn get(&self, key: &B3Hash) -> Option<Arc<MoEPrefixEntry>> {
        let entries = self.entries.read();
        if let Some(entry) = entries.get(key) {
            entry.record_access();
            let mut stats = self.stats.write();
            stats.hits += 1;
            Some(Arc::clone(entry))
        } else {
            let mut stats = self.stats.write();
            stats.misses += 1;
            None
        }
    }

    /// Insert an entry into the cache
    pub fn insert(&self, key: B3Hash, entry: MoEPrefixEntry) -> Result<Arc<MoEPrefixEntry>> {
        let entry_bytes = entry.total_bytes();

        if entry_bytes > self.max_bytes {
            return Err(AosError::Validation(format!(
                "MoE prefix entry ({} bytes) exceeds max budget ({} bytes)",
                entry_bytes, self.max_bytes
            )));
        }

        self.evict_until_fits(entry_bytes)?;

        let entry = Arc::new(entry);
        {
            let mut entries = self.entries.write();
            entries.insert(key, Arc::clone(&entry));
        }

        self.used_bytes.fetch_add(entry_bytes, Ordering::SeqCst);
        {
            let mut stats = self.stats.write();
            stats.entry_count += 1;
            stats.used_bytes = self.used_bytes.load(Ordering::SeqCst);
        }

        Ok(entry)
    }

    /// Evict entries until the specified bytes can fit
    fn evict_until_fits(&self, needed_bytes: u64) -> Result<()> {
        let current = self.used_bytes.load(Ordering::SeqCst);
        let available = self.max_bytes.saturating_sub(current);

        if available >= needed_bytes {
            return Ok(());
        }

        let to_free = needed_bytes - available;
        let mut freed = 0u64;
        let mut evicted_keys = Vec::new();

        {
            let entries = self.entries.read();
            let mut candidates: Vec<_> = entries
                .iter()
                .filter(|(_, e)| !e.is_in_use())
                .map(|(k, e)| (*k, e.last_access_ns(), e.total_bytes()))
                .collect();

            candidates.sort_by_key(|(_, access, _)| *access);

            for (key, _, bytes) in candidates {
                evicted_keys.push(key);
                freed += bytes;
                if freed >= to_free {
                    break;
                }
            }
        }

        if freed < to_free {
            return Err(AosError::MemoryPressure(format!(
                "Cannot evict enough MoE prefix entries: need {} bytes, can free {}",
                to_free, freed
            )));
        }

        {
            let mut entries = self.entries.write();
            let mut stats = self.stats.write();

            for key in &evicted_keys {
                if let Some(entry) = entries.remove(key) {
                    self.used_bytes
                        .fetch_sub(entry.total_bytes(), Ordering::SeqCst);
                    stats.evictions += 1;
                    stats.entry_count = stats.entry_count.saturating_sub(1);
                }
            }
            stats.used_bytes = self.used_bytes.load(Ordering::SeqCst);
        }

        Ok(())
    }

    /// Get free tokens for a prefix if available and valid
    pub fn get_free_tokens(
        &self,
        key: &B3Hash,
        temperature: f32,
        adapter_id: Option<&str>,
    ) -> Option<Vec<FreeToken>> {
        // Check if free tokens are disabled for this adapter
        if let Some(id) = adapter_id {
            if self.adapter_metrics.should_disable(id, 0.9, 100) {
                tracing::debug!(adapter_id = id, "Free tokens disabled due to low accuracy");
                return None;
            }
        }

        let entry = self.get(key)?;
        let precomputed = entry.get_free_tokens(temperature)?;

        // Record delivery
        if let Some(id) = adapter_id {
            let metrics = self.adapter_metrics.get_or_create(id);
            for _ in &precomputed.tokens {
                metrics.record_delivered();
            }
        }

        {
            let mut stats = self.stats.write();
            stats.free_tokens_delivered += precomputed.tokens.len() as u64;
        }

        Some(precomputed.tokens.clone())
    }

    /// Record validation result for a free token
    pub fn record_validation(&self, adapter_id: &str, matched: bool, latency_saved_ms: u64) {
        let metrics = self.adapter_metrics.get_or_create(adapter_id);
        if matched {
            metrics.record_validated(latency_saved_ms);
            self.stats.write().free_tokens_validated += 1;
        } else {
            metrics.record_rejected();
        }
    }

    /// Get hot experts for pre-warming
    pub fn get_hot_experts(&self, key: &B3Hash) -> Option<Vec<(usize, Vec<u8>)>> {
        let entry = self.get(key)?;

        if !entry.has_stable_routing(0.5) {
            return None;
        }

        Some(
            entry
                .heat_map
                .hot_experts
                .iter()
                .enumerate()
                .filter(|(_, experts)| !experts.is_empty())
                .map(|(layer, experts)| (layer, experts.clone()))
                .collect(),
        )
    }

    /// Record a pre-warm operation
    pub fn record_prewarm(&self) {
        self.stats.write().prewarm_count += 1;
    }

    /// Get cache statistics
    pub fn stats(&self) -> MoEPrefixCacheStats {
        self.stats.read().clone()
    }

    /// Get adapter accuracy
    pub fn adapter_accuracy(&self, adapter_id: &str) -> Option<f32> {
        self.adapter_metrics.get_accuracy(adapter_id)
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut entries = self.entries.write();
        entries.clear();
        self.used_bytes.store(0, Ordering::SeqCst);

        let mut stats = self.stats.write();
        stats.entry_count = 0;
        stats.used_bytes = 0;
    }

    /// Get number of entries
    pub fn len(&self) -> usize {
        self.entries.read().len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.read().is_empty()
    }
}

// Thread safety
unsafe impl Send for MoEPrefixCache {}
unsafe impl Sync for MoEPrefixCache {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expert_heat_map() {
        let mut heat_map = ExpertHeatMap::new(4);

        // Record some activations using record_token_routing which increments sample_count
        heat_map.record_token_routing(&[(0, 5)]);
        heat_map.record_token_routing(&[(0, 5), (0, 3), (1, 10)]);

        heat_map.finalize(2);

        assert_eq!(heat_map.get_hot_experts(0), &[5, 3]);
        assert_eq!(heat_map.get_hot_experts(1), &[10]);
        assert!(heat_map.routing_stability > 0.0);
    }

    #[test]
    fn test_free_token() {
        let token = FreeToken::new("hello", 1234, 0.95).with_expert_routing(vec![(0, 5), (1, 10)]);

        assert_eq!(token.text, "hello");
        assert_eq!(token.token_id, 1234);
        assert!((token.confidence - 0.95).abs() < 0.01);
        assert_eq!(token.expert_routing.len(), 2);
    }

    #[test]
    fn test_precomputed_tokens_confidence() {
        let tokens = vec![FreeToken::new("a", 1, 0.9), FreeToken::new("b", 2, 0.8)];

        let precomputed = PrecomputedTokens::new(tokens, FreeTokenSource::ManifestDeclared);

        // 0.9 * 0.8 = 0.72
        assert!((precomputed.sequence_confidence - 0.72).abs() < 0.01);
    }

    #[test]
    fn test_temperature_validity() {
        let tokens = vec![FreeToken::new("x", 1, 0.95)];
        let precomputed = PrecomputedTokens::new(tokens, FreeTokenSource::ManifestDeclared)
            .with_max_temperature(0.3);

        assert!(precomputed.is_valid_for_temperature(0.1));
        assert!(precomputed.is_valid_for_temperature(0.3));
        assert!(!precomputed.is_valid_for_temperature(0.5));
    }

    #[test]
    fn test_metrics_accuracy() {
        let metrics = FreeTokenMetrics::new();

        // Start with perfect accuracy
        assert!((metrics.accuracy() - 1.0).abs() < 0.01);

        // Add some validations
        metrics.record_validated(10);
        metrics.record_validated(10);
        metrics.record_rejected();

        // 2/3 = 0.666...
        assert!((metrics.accuracy() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_cache_insert_get() {
        let cache = MoEPrefixCache::new(1024 * 1024);
        let key = B3Hash::hash(b"test_key");

        let entry = MoEPrefixEntry::new(
            vec![vec![1.0; 128]],
            vec![vec![2.0; 128]],
            "tenant1".to_string(),
            10,
            4,
        );

        cache.insert(key, entry).unwrap();

        let retrieved = cache.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().prefix_cached_token_count, 10);
    }

    #[test]
    fn test_moe_entry_with_routing() {
        let entry = MoEPrefixEntry::new(
            vec![vec![1.0; 64]],
            vec![vec![2.0; 64]],
            "tenant1".to_string(),
            5,
            4,
        )
        .with_expert_routing(vec![
            vec![(0, 5), (1, 10)],
            vec![(0, 5), (1, 8)],
            vec![(0, 3), (1, 10)],
        ])
        .finalize(2);

        assert_eq!(entry.heat_map.sample_count, 3);
        // Expert 5 should be hot in layer 0 (appears twice)
        assert!(entry.heat_map.get_hot_experts(0).contains(&5));
    }
}
