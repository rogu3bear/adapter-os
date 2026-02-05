//! Forward Pass Handler
//!
//! Handles forward pass requests from workers, managing:
//! - Base model inference via MLX FFI
//! - KV cache lookup and update
//! - Hot adapter fusion

use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, warn};

use crate::adapter_cache::{AdapterCache, CachedAdapter};
use crate::kv_cache::KvCacheManager;
use adapteros_core::{AosError, Result, Q15_GATE_DENOMINATOR};

#[cfg(feature = "mlx")]
use adapteros_lora_mlx_ffi::tensor::MLXFFITensor;

/// Request for a forward pass
#[derive(Debug, Clone)]
pub struct ForwardPassRequest {
    /// Session ID for KV cache
    pub session_id: String,

    /// Input token IDs
    pub input_ids: Vec<u32>,

    /// Current position in sequence
    pub position: u32,

    /// Maximum sequence length
    pub max_seq_len: u32,

    /// Adapter IDs to apply (hot adapters)
    pub adapter_ids: Vec<u32>,

    /// Q15 gates for adapters
    pub adapter_gates_q15: Vec<i16>,

    /// Whether to include hidden states
    pub include_hidden_states: bool,

    /// Deterministic seed
    pub manifest_seed: Option<Vec<u8>>,
}

/// Response from a forward pass
#[derive(Debug, Clone)]
pub struct ForwardPassResponse {
    /// Output logits (vocab_size)
    pub logits: Vec<f32>,

    /// Updated position
    pub position: u32,

    /// Hidden states (if requested)
    pub hidden_states: Option<Vec<f32>>,

    /// Whether KV cache was hit
    pub kv_cache_hit: bool,

    /// Number of cached tokens
    pub cached_tokens: u32,

    /// Forward pass latency in milliseconds
    pub latency_ms: f64,
}

/// Forward pass executor
pub struct ForwardExecutor {
    /// KV cache manager
    kv_cache: Arc<KvCacheManager>,

    /// Adapter cache
    adapter_cache: Arc<AdapterCache>,

    /// Vocabulary size (for output logits)
    vocab_size: usize,

    /// Hidden size (for model dimensions)
    hidden_size: usize,

    /// Number of layers
    num_layers: usize,

    /// Model loaded flag
    model_loaded: bool,

    /// Estimated model memory in bytes
    model_memory_bytes: u64,

    /// The loaded MLX model (when mlx feature is enabled)
    #[cfg(feature = "mlx")]
    model: Option<adapteros_lora_mlx_ffi::MLXFFIModel>,
}

impl ForwardExecutor {
    /// Create a new forward executor
    pub fn new(
        kv_cache: Arc<KvCacheManager>,
        adapter_cache: Arc<AdapterCache>,
        vocab_size: usize,
        hidden_size: usize,
        num_layers: usize,
    ) -> Self {
        Self {
            kv_cache,
            adapter_cache,
            vocab_size,
            hidden_size,
            num_layers,
            model_loaded: false,
            model_memory_bytes: 0,
            #[cfg(feature = "mlx")]
            model: None,
        }
    }

    /// Load the base model
    #[cfg(feature = "mlx")]
    pub fn load_model(&mut self, model_path: &std::path::Path) -> Result<()> {
        use adapteros_lora_mlx_ffi::{
            mlx_runtime_init_with_device, mlx_runtime_is_initialized, MLXFFIModel, MlxDeviceType,
        };

        // Initialize MLX runtime if needed
        if !mlx_runtime_is_initialized() {
            mlx_runtime_init_with_device(MlxDeviceType::Auto).map_err(|e| {
                AosError::Config(format!("Failed to initialize MLX runtime: {}", e))
            })?;
        }

        // Load the model
        let model = MLXFFIModel::load(model_path).map_err(|e| {
            AosError::Config(format!(
                "Failed to load model from '{}': {}",
                model_path.display(),
                e
            ))
        })?;

        // Update dimensions from loaded model config
        let config = model.config();
        self.vocab_size = config.vocab_size;
        self.hidden_size = config.hidden_size;
        self.num_layers = config.num_hidden_layers;

        // Estimate model memory based on parameters
        // Formula: approx 2 bytes per parameter for FP16/BF16 models
        // Params ≈ vocab * hidden + layers * (4 * hidden^2 + 3 * hidden * intermediate)
        let intermediate_size = config.intermediate_size;
        let params_embedding = self.vocab_size * self.hidden_size;
        let params_per_layer = 4 * self.hidden_size * self.hidden_size // attention QKV + O
            + 3 * self.hidden_size * intermediate_size; // FFN
        let total_params = params_embedding + self.num_layers * params_per_layer;
        // Assume 2 bytes per param (FP16) plus some overhead
        self.model_memory_bytes = (total_params * 2) as u64;

        // Store the model for inference
        self.model = Some(model);
        self.model_loaded = true;

        info!(
            model_path = %model_path.display(),
            vocab_size = self.vocab_size,
            hidden_size = self.hidden_size,
            num_layers = self.num_layers,
            estimated_memory_mb = self.model_memory_bytes / (1024 * 1024),
            "Model loaded successfully"
        );

        Ok(())
    }

    #[cfg(not(feature = "mlx"))]
    pub fn load_model(&mut self, _model_path: &std::path::Path) -> Result<()> {
        Err(AosError::Config(
            "MLX feature not enabled. Rebuild with --features mlx".to_string(),
        ))
    }

    /// Execute a forward pass
    pub fn forward(&self, request: ForwardPassRequest) -> Result<ForwardPassResponse> {
        let start = Instant::now();

        // Get or create KV cache for this session
        let cache_entry = self
            .kv_cache
            .get_or_create(&request.session_id, request.max_seq_len);

        let (kv_cache_hit, cached_tokens) = {
            let entry = cache_entry.read();
            (entry.cached_tokens > 0, entry.cached_tokens)
        };

        if kv_cache_hit {
            debug!(
                session_id = request.session_id,
                cached_tokens = cached_tokens,
                "KV cache hit"
            );
        }

        // Get hot adapters for fusion
        let adapters: Vec<Arc<CachedAdapter>> = request
            .adapter_ids
            .iter()
            .filter_map(|&id| self.adapter_cache.get(id))
            .collect();

        if !adapters.is_empty() {
            debug!(
                adapter_count = adapters.len(),
                adapter_ids = ?request.adapter_ids,
                "Fusing hot adapters"
            );
            self.adapter_cache.record_fusion();
        }

        // Execute forward pass using real MLX model when available
        #[cfg(feature = "mlx")]
        let (mut logits, hidden_states_map): (
            Vec<f32>,
            std::collections::HashMap<String, Vec<f32>>,
        ) = {
            if let Some(ref model) = self.model {
                // Real MLX forward pass
                model
                    .forward_with_hidden_states(&request.input_ids, request.position as usize)
                    .map_err(|e| AosError::Internal(format!("Forward pass failed: {e}")))?
            } else {
                // Model not loaded, use mock (shouldn't happen in production)
                warn!("MLX model not loaded, using mock forward pass");
                let mock_logits = self.mock_forward(&request, &adapters)?;
                (mock_logits, std::collections::HashMap::new())
            }
        };

        #[cfg(not(feature = "mlx"))]
        let (mut logits, hidden_states_map): (
            Vec<f32>,
            std::collections::HashMap<String, Vec<f32>>,
        ) = {
            let mock_logits = self.mock_forward(&request, &adapters)?;
            (mock_logits, std::collections::HashMap::new())
        };

        // Apply hot adapter fusion if we have adapters and hidden states
        if !adapters.is_empty() {
            // Get hidden states from last layer for LoRA fusion
            let last_layer_key = format!("layer_{}", self.num_layers.saturating_sub(1));
            if let Some(hidden) = hidden_states_map.get(&last_layer_key) {
                fuse_adapters(&mut logits, hidden, &adapters, &request.adapter_gates_q15);
            } else if let Some(hidden) = hidden_states_map.get("o_proj") {
                // Fallback to o_proj if layer key not found
                fuse_adapters(&mut logits, hidden, &adapters, &request.adapter_gates_q15);
            }
        }

        // Update KV cache
        {
            let mut entry = cache_entry.write();
            entry.cached_tokens = request.position + request.input_ids.len() as u32;
            entry.touch();
        }

        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Extract hidden states from the last layer if requested
        let hidden_states = if request.include_hidden_states {
            // Try last layer key first, then fallback to o_proj
            let last_layer_key = format!("layer_{}", self.num_layers.saturating_sub(1));
            hidden_states_map
                .get(&last_layer_key)
                .or_else(|| hidden_states_map.get("o_proj"))
                .or_else(|| hidden_states_map.get("hidden"))
                .cloned()
        } else {
            None
        };

        let response = ForwardPassResponse {
            logits,
            position: request.position + request.input_ids.len() as u32,
            hidden_states,
            kv_cache_hit,
            cached_tokens,
            latency_ms,
        };

        debug!(
            session_id = request.session_id,
            position = response.position,
            latency_ms = latency_ms,
            adapters_fused = adapters.len(),
            "Forward pass complete"
        );

        Ok(response)
    }

    /// Mock forward pass for development/testing
    fn mock_forward(
        &self,
        request: &ForwardPassRequest,
        adapters: &[Arc<CachedAdapter>],
    ) -> Result<Vec<f32>> {
        // Generate mock logits
        let mut logits = vec![0.0f32; self.vocab_size];

        // Simple pattern based on input
        if let Some(&last_token) = request.input_ids.last() {
            let seed = last_token as usize;
            for (i, logit) in logits.iter_mut().enumerate() {
                *logit = ((seed + i) as f32 * 0.001) % 1.0 - 0.5;
            }
        }

        // Apply adapter modifications (mock LoRA fusion)
        for (adapter_idx, adapter) in adapters.iter().enumerate() {
            let gate = request
                .adapter_gates_q15
                .get(adapter_idx)
                .copied()
                .unwrap_or(0);
            let gate_f32 = gate as f32 / 32767.0;

            // Mock LoRA: logits += scale * gate * lora_b @ lora_a @ hidden
            // Simplified: just add a scaled offset
            let offset = adapter.scale * gate_f32 * adapter.lora_b.get(0).copied().unwrap_or(0.0);
            for logit in logits.iter_mut() {
                *logit += offset * 0.01;
            }
        }

        Ok(logits)
    }

    /// Check if model is loaded
    pub fn is_loaded(&self) -> bool {
        self.model_loaded
    }

    /// Get vocab size
    pub fn vocab_size(&self) -> usize {
        self.vocab_size
    }

    /// Get hidden size
    pub fn hidden_size(&self) -> usize {
        self.hidden_size
    }

    /// Get estimated model memory in bytes
    pub fn model_memory_bytes(&self) -> u64 {
        self.model_memory_bytes
    }
}

/// Apply LoRA adapters to base logits
///
/// output = base + sum(scale_i * gate_i * (hidden @ A_i.T @ B_i.T))
///
/// For efficiency, we compute this as:
/// output = base + sum(scale_i * gate_i * delta_i)
/// where delta_i is the LoRA contribution for adapter i
pub fn fuse_adapters(
    base_logits: &mut [f32],
    hidden_states: &[f32],
    adapters: &[Arc<CachedAdapter>],
    gates_q15: &[i16],
) {
    #[cfg(feature = "mlx")]
    {
        if let Err(err) = fuse_adapters_mlx(base_logits, hidden_states, adapters, gates_q15) {
            warn!(?err, "Falling back to CPU LoRA fusion");
            fuse_adapters_cpu(base_logits, hidden_states, adapters, gates_q15);
        }
    }

    #[cfg(not(feature = "mlx"))]
    {
        fuse_adapters_cpu(base_logits, hidden_states, adapters, gates_q15);
    }
}

fn fuse_adapters_cpu(
    base_logits: &mut [f32],
    hidden_states: &[f32],
    adapters: &[Arc<CachedAdapter>],
    gates_q15: &[i16],
) {
    for (idx, adapter) in adapters.iter().enumerate() {
        let gate = gates_q15.get(idx).copied().unwrap_or(0);
        if gate == 0 {
            continue;
        }

        let gate_f32 = gate as f32 / Q15_GATE_DENOMINATOR;
        let scale = adapter.scale * gate_f32;

        let delta = compute_lora_delta(hidden_states, &adapter.lora_a, &adapter.lora_b);

        for (logit, &d) in base_logits.iter_mut().zip(delta.iter()) {
            *logit += scale * d;
        }
    }
}

#[cfg(feature = "mlx")]
fn fuse_adapters_mlx(
    base_logits: &mut [f32],
    hidden_states: &[f32],
    adapters: &[Arc<CachedAdapter>],
    gates_q15: &[i16],
) -> Result<()> {
    // Shapes inferred from flattened weights
    let hidden_size = hidden_states.len();

    for (idx, adapter) in adapters.iter().enumerate() {
        let gate = gates_q15.get(idx).copied().unwrap_or(0);
        if gate == 0 {
            continue;
        }

        // gate in Q15 → float
        let gate_f32 = gate as f32 / Q15_GATE_DENOMINATOR;
        let scale = adapter.scale * gate_f32;

        // Infer rank and vocab from flattened matrices
        if hidden_size == 0 || adapter.lora_a.is_empty() || adapter.lora_b.is_empty() {
            continue;
        }
        let rank = adapter.lora_a.len() / hidden_size;
        if rank == 0 {
            continue;
        }
        let vocab_size = adapter.lora_b.len() / rank;

        // Build tensors: hidden [1, H], A^T [H, R], B^T [R, V]
        let hidden_t = MLXFFITensor::from_data(hidden_states, vec![1, hidden_size])?;
        let a_t = MLXFFITensor::from_data(&adapter.lora_a, vec![rank, hidden_size])?.transpose()?; // [H, R]
        let b_t = MLXFFITensor::from_data(&adapter.lora_b, vec![vocab_size, rank])?.transpose()?; // [R, V]

        // hidden @ A^T -> [1, R]
        let intermediate = hidden_t.matmul(&a_t)?;
        // intermediate @ B^T -> [1, V]
        let delta = intermediate.matmul(&b_t)?;
        let mut delta_vec = delta.to_float_vec()?;

        // Add scaled delta to logits
        for (logit, d) in base_logits.iter_mut().zip(delta_vec.iter_mut()) {
            *logit += scale * *d;
        }
    }

    Ok(())
}

/// Compute LoRA delta via CPU matrix multiplication
///
/// delta = hidden @ A.T @ B.T
///
/// Where:
/// - hidden: [hidden_size]
/// - A: [rank, hidden_size] (down projection)
/// - B: [vocab_size, rank] (up projection)
/// - delta: [vocab_size]
///
/// Note: This is a correct CPU implementation. For GPU acceleration,
/// the LoRA fusion could be moved to the MLX C++ layer.
fn compute_lora_delta(hidden: &[f32], lora_a: &[f32], lora_b: &[f32]) -> Vec<f32> {
    // Infer dimensions (assuming lora_a is [rank, hidden_size] flattened)
    let hidden_size = hidden.len();
    if lora_a.is_empty() || lora_b.is_empty() {
        return vec![0.0; lora_b.len()];
    }

    // Compute intermediate = hidden @ A.T = [rank]
    let rank = lora_a.len() / hidden_size.max(1);
    let mut intermediate = vec![0.0f32; rank];

    for (r, inter_r) in intermediate.iter_mut().enumerate().take(rank) {
        for (h, hidden_h) in hidden.iter().enumerate().take(hidden_size) {
            let a_idx = r * hidden_size + h;
            if a_idx < lora_a.len() {
                *inter_r += hidden_h * lora_a[a_idx];
            }
        }
    }

    // Compute output = intermediate @ B.T = [vocab_size]
    let vocab_size = lora_b.len() / rank.max(1);
    let mut output = vec![0.0f32; vocab_size];

    for (v, out_v) in output.iter_mut().enumerate().take(vocab_size) {
        for (r, inter_r) in intermediate.iter().enumerate().take(rank) {
            let b_idx = v * rank + r;
            if b_idx < lora_b.len() {
                *out_v += inter_r * lora_b[b_idx];
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_forward_request() {
        let request = ForwardPassRequest {
            session_id: "test-session".to_string(),
            input_ids: vec![1, 2, 3],
            position: 0,
            max_seq_len: 2048,
            adapter_ids: vec![],
            adapter_gates_q15: vec![],
            include_hidden_states: false,
            manifest_seed: None,
        };

        assert_eq!(request.input_ids.len(), 3);
    }

    #[test]
    fn test_forward_executor() {
        let kv_cache = Arc::new(KvCacheManager::new(1024 * 1024, 4096, 32));
        let adapter_cache = Arc::new(AdapterCache::with_defaults());

        let executor = ForwardExecutor::new(kv_cache, adapter_cache, 32000, 4096, 32);

        let request = ForwardPassRequest {
            session_id: "test".to_string(),
            input_ids: vec![1, 2, 3],
            position: 0,
            max_seq_len: 2048,
            adapter_ids: vec![],
            adapter_gates_q15: vec![],
            include_hidden_states: false,
            manifest_seed: None,
        };

        let response = executor.forward(request).unwrap();
        assert_eq!(response.logits.len(), 32000);
        assert_eq!(response.position, 3);
        assert!(!response.kv_cache_hit); // First request
    }

    #[test]
    fn test_kv_cache_hit() {
        let kv_cache = Arc::new(KvCacheManager::new(1024 * 1024, 4096, 32));
        let adapter_cache = Arc::new(AdapterCache::with_defaults());

        let executor = ForwardExecutor::new(kv_cache.clone(), adapter_cache, 32000, 4096, 32);

        // First request
        let request1 = ForwardPassRequest {
            session_id: "test".to_string(),
            input_ids: vec![1, 2, 3],
            position: 0,
            max_seq_len: 2048,
            adapter_ids: vec![],
            adapter_gates_q15: vec![],
            include_hidden_states: false,
            manifest_seed: None,
        };

        let response1 = executor.forward(request1).unwrap();
        assert!(!response1.kv_cache_hit);

        // Second request - should hit cache
        let request2 = ForwardPassRequest {
            session_id: "test".to_string(),
            input_ids: vec![4],
            position: 3,
            max_seq_len: 2048,
            adapter_ids: vec![],
            adapter_gates_q15: vec![],
            include_hidden_states: false,
            manifest_seed: None,
        };

        let response2 = executor.forward(request2).unwrap();
        assert!(response2.kv_cache_hit);
        assert_eq!(response2.cached_tokens, 3);
    }

    #[test]
    fn test_compute_lora_delta() {
        let hidden = vec![1.0, 2.0, 3.0, 4.0]; // hidden_size = 4
        let lora_a = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]; // rank=2, hidden=4
        let lora_b = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]; // vocab=3, rank=2

        let delta = compute_lora_delta(&hidden, &lora_a, &lora_b);
        assert_eq!(delta.len(), 3);
    }
}
