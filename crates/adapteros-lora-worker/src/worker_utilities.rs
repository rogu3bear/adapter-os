//! Worker utilities module
//!
//! Contains Worker accessor methods and utility functions including:
//! - compute_embedding
//! - generate_plan_id
//! - get_adapter_states
//! - get_memory_usage_bytes
//! - get_memory_usage_mb
//! - hotswap
//! - kv_cache
//! - coreml_verification
//! - last_stack_hash
//! - telemetry
//! - health_monitor
//! - set_health_monitor

use crate::{
    adapter_hotswap, CoremlVerificationSnapshot, HealthMonitor, HotSwapManager, KvCache, Worker,
};
use adapteros_core::{B3Hash, Result};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_telemetry::TelemetryWriter;
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

/// Worker utility methods and accessors
impl<K: FusedKernels + crate::StrictnessControl + Send + Sync + 'static> Worker<K> {
    /// Compute embedding for text query (for RAG/similarity search)
    ///
    /// This generates averaged token embeddings for semantic search.
    /// Note: Metal kernels handle embedding lookup internally for forward pass.
    pub(crate) fn compute_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let tokens = self.tokenizer.encode(text)?;
        self.embedding_model.encode_tokens(&tokens)
    }

    /// Encode tokens to embeddings for RAG/text similarity
    ///
    /// This method is used for generating query embeddings for evidence retrieval
    /// and semantic search. It averages token embeddings and applies L2 normalization.
    ///
    /// Note: This is NOT used for the forward pass - Metal kernels perform
    /// embedding lookup directly from input_ids for inference.
    #[allow(dead_code)]
    fn _encode_text_for_rag(&self, token_ids: &[u32]) -> Result<Vec<f32>> {
        self.embedding_model.encode_tokens(token_ids)
    }

    /// Generate a deterministic plan_id from the manifest hash and request context
    ///
    /// The plan_id is derived using BLAKE3 hash of:
    /// - Base model hash from manifest (ensures reproducibility across workers)
    /// - Request cpid (ensures uniqueness per request)
    ///
    /// This provides a deterministic, traceable identifier for each inference plan.
    pub(crate) fn generate_plan_id(&self, cpid: &str) -> String {
        use adapteros_core::B3Hash;

        // Combine manifest model hash with cpid for deterministic plan identification
        let combined = format!("{}:{}", self.manifest.base.model_hash, cpid);
        let hash = B3Hash::hash(combined.as_bytes());

        // Use first 16 hex chars (64 bits) for reasonable uniqueness while keeping it readable
        format!("plan_{}", &hash.to_hex()[..16])
    }

    /// Get current adapter states
    pub fn get_adapter_states(&self) -> Vec<adapter_hotswap::AdapterState> {
        self.hotswap.table().get_active()
    }

    /// Get current memory usage in bytes
    ///
    /// Returns the memory currently used by the worker, including model weights
    /// and adapter buffers. Returns 0 if memory tracking is unavailable.
    pub fn get_memory_usage_bytes(&self) -> u64 {
        self.health_monitor.get_memory_usage().unwrap_or(0)
    }

    /// Get current memory usage in MB
    pub fn get_memory_usage_mb(&self) -> i32 {
        (self.get_memory_usage_bytes() / (1024 * 1024)) as i32
    }

    /// Get reference to the hot-swap manager
    pub fn hotswap(&self) -> &Arc<HotSwapManager<K>> {
        &self.hotswap
    }

    /// Get reference to the KV cache
    pub fn kv_cache(&self) -> &Arc<StdMutex<KvCache>> {
        &self.kv_cache
    }

    /// Return the cached CoreML verification snapshot, if available.
    pub fn coreml_verification(&self) -> Option<CoremlVerificationSnapshot> {
        self.coreml_verification.clone()
    }

    /// Get reference to the last stack hash
    pub fn last_stack_hash(&self) -> &RwLock<Option<B3Hash>> {
        &self._last_stack_hash
    }

    /// Get reference to the telemetry writer
    pub fn telemetry(&self) -> &Option<TelemetryWriter> {
        &self.telemetry
    }

    /// Get cloned reference to health monitor
    pub fn health_monitor(&self) -> Arc<HealthMonitor> {
        self.health_monitor.clone()
    }

    /// Replace the health monitor (for heartbeat alignment after CP registration)
    pub fn set_health_monitor(&mut self, monitor: Arc<HealthMonitor>) {
        self.health_monitor = monitor;
    }
}
