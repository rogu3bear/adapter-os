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
    adapter_hotswap, inference_management::InferenceCancelRegistry, CoremlVerificationSnapshot,
    HealthMonitor, HotSwapManager, KvCache, Worker, WorkerModelRuntimeState,
};
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::FusedKernels;
use adapteros_telemetry::TelemetryWriter;
use parking_lot::RwLock;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

/// Apply a runtime model load/switch transition.
///
/// Switch failures are fail-safe: if a ready model is already active, it remains active.
pub fn apply_runtime_model_load_transition(
    state: &mut WorkerModelRuntimeState,
    model_id: &str,
    model_path: &str,
) -> Result<()> {
    let model_path_ref = std::path::Path::new(model_path);

    if state.active_model_id.as_deref() == Some(model_id) && state.status == "ready" {
        return Ok(());
    }

    state.generation = state.generation.saturating_add(1);
    state.status = "loading".to_string();
    state.last_error = None;

    if !model_path_ref.exists() {
        let err = format!("Model path does not exist: {}", model_path_ref.display());
        let keep_previous = state.active_model_id.is_some();
        state.last_error = Some(err.clone());
        if keep_previous {
            state.status = "ready".to_string();
        } else {
            state.status = "error".to_string();
        }
        return Err(AosError::NotFound(err));
    }

    let hash = blake3::hash(model_path.as_bytes()).to_hex().to_string();
    state.active_model_id = Some(model_id.to_string());
    state.active_model_hash = Some(hash);
    state.status = "ready".to_string();
    state.last_error = None;

    Ok(())
}

/// Apply a runtime model unload transition.
pub fn apply_runtime_model_unload_transition(state: &mut WorkerModelRuntimeState) {
    state.generation = state.generation.saturating_add(1);
    state.status = "unloading".to_string();
    state.active_model_id = None;
    state.active_model_hash = None;
    state.last_error = None;
    state.status = "no-model".to_string();
}

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
    pub fn get_adapter_states(&self) -> Vec<adapter_hotswap::AdapterLoadState> {
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

    /// Get cloned reference to inference cancellation registry
    pub fn inference_cancel_registry(&self) -> Arc<InferenceCancelRegistry> {
        self.inference_cancellations.clone()
    }

    /// Get current worker model runtime lifecycle state.
    pub fn model_runtime_state(&self) -> WorkerModelRuntimeState {
        self.model_runtime_state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_default()
    }

    /// Load or switch the runtime model state used by UDS model lifecycle endpoints.
    ///
    /// Switch failures are fail-safe: if a ready model is already active, it remains active.
    pub fn load_or_switch_runtime_model(
        &self,
        model_id: &str,
        model_path: &str,
    ) -> Result<WorkerModelRuntimeState> {
        let mut state = self.model_runtime_state.lock().map_err(|_| {
            AosError::Internal("Worker model runtime state lock poisoned".to_string())
        })?;
        apply_runtime_model_load_transition(&mut state, model_id, model_path)?;
        Ok(state.clone())
    }

    /// Unload the active runtime model state used by UDS model lifecycle endpoints.
    pub fn unload_runtime_model(&self) -> Result<WorkerModelRuntimeState> {
        let mut state = self.model_runtime_state.lock().map_err(|_| {
            AosError::Internal("Worker model runtime state lock poisoned".to_string())
        })?;
        apply_runtime_model_unload_transition(&mut state);
        Ok(state.clone())
    }
}
