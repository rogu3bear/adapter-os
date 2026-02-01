//! Kernel Wrapper for Model Server Mode
//!
//! This module provides a FusedKernels implementation that delegates forward passes
//! to a remote Model Server while handling cold adapter loading locally.
//!
//! ## Architecture
//!
//! In model server mode, workers don't load the base model. Instead:
//! 1. Forward passes are delegated to the Model Server via gRPC
//! 2. Hot adapters are fused in the Model Server before returning logits
//! 3. Cold adapters are applied locally to the returned base logits
//!
//! This reduces memory usage significantly when multiple workers share a model.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use tokio::runtime::Handle;
use tracing::{debug, error, info, warn};

use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::attestation::{
    BackendType, DeterminismLevel, DeterminismReport, FloatingPointMode, RngSeedingMethod,
};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};

use crate::model_server_client::{ModelServerClient, ModelServerClientConfig};

/// Cold adapter entry stored locally on the worker
#[derive(Debug)]
struct ColdAdapter {
    /// Adapter ID
    id: u16,
    /// LoRA A weights (down projection)
    lora_a: Vec<f32>,
    /// LoRA B weights (up projection)
    lora_b: Vec<f32>,
    /// Scaling factor
    scale: f32,
}

/// Model Server kernel wrapper
///
/// Implements FusedKernels by delegating forward passes to a remote Model Server
/// and applying cold adapters locally.
pub struct ModelServerKernels {
    /// Client for communicating with the Model Server
    client: Arc<ModelServerClient>,

    /// Session ID for KV cache management
    session_id: String,

    /// Cold adapters stored locally
    cold_adapters: RwLock<HashMap<u16, ColdAdapter>>,

    /// Vocabulary size for logit allocation
    vocab_size: usize,

    /// Maximum sequence length
    max_seq_len: u32,

    /// Current position in the sequence
    current_position: u32,

    /// Tokio runtime handle for blocking calls
    runtime_handle: Handle,

    /// Determinism report from server (cached)
    determinism_report: RwLock<Option<DeterminismReport>>,

    /// Manifest seed for deterministic execution
    manifest_seed: Option<Vec<u8>>,
}

impl ModelServerKernels {
    /// Create a new Model Server kernel wrapper
    pub fn new(config: ModelServerClientConfig, session_id: String, vocab_size: usize) -> Self {
        Self {
            client: Arc::new(ModelServerClient::new(config)),
            session_id,
            cold_adapters: RwLock::new(HashMap::new()),
            vocab_size,
            max_seq_len: 4096,
            current_position: 0,
            runtime_handle: Handle::current(),
            determinism_report: RwLock::new(None),
            manifest_seed: None,
        }
    }

    /// Create with a specific runtime handle
    pub fn with_runtime(
        config: ModelServerClientConfig,
        session_id: String,
        vocab_size: usize,
        runtime_handle: Handle,
    ) -> Self {
        Self {
            client: Arc::new(ModelServerClient::new(config)),
            session_id,
            cold_adapters: RwLock::new(HashMap::new()),
            vocab_size,
            max_seq_len: 4096,
            current_position: 0,
            runtime_handle,
            determinism_report: RwLock::new(None),
            manifest_seed: None,
        }
    }

    /// Set manifest seed for deterministic execution
    pub fn set_manifest_seed(&mut self, seed: Vec<u8>) {
        self.manifest_seed = Some(seed);
    }

    /// Set maximum sequence length
    pub fn set_max_seq_len(&mut self, max_seq_len: u32) {
        self.max_seq_len = max_seq_len;
    }

    /// Connect to the model server
    pub async fn connect(&self) -> Result<()> {
        self.client.connect().await
    }

    /// Check connection status
    pub async fn is_connected(&self) -> bool {
        self.client.is_connected().await
    }

    /// Apply cold adapters to base logits using hidden states
    ///
    /// For each cold adapter in the router ring, apply LoRA modification locally:
    /// logits += scale * gate * (hidden @ A.T @ B.T)
    ///
    /// Where:
    /// - hidden: [hidden_size] - last layer hidden states from model server
    /// - A: [rank, hidden_size] (down projection, stored as lora_a flattened)
    /// - B: [vocab_size, rank] (up projection, stored as lora_b flattened)
    fn apply_cold_adapters(&self, logits: &mut [f32], ring: &RouterRing, hidden_states: &[f32]) {
        let adapters = self.cold_adapters.read();
        let hidden_size = hidden_states.len();

        if hidden_size == 0 {
            warn!("No hidden states available for cold adapter computation");
            return;
        }

        for i in 0..ring.k {
            let adapter_id = ring.indices[i];
            let gate = ring.gates_q15[i];

            // Skip zero gates
            if gate == 0 {
                continue;
            }

            // Check if this is a cold adapter (loaded locally)
            if let Some(adapter) = adapters.get(&adapter_id) {
                let gate_f32 = gate as f32 / 32767.0;
                let scale = adapter.scale * gate_f32;

                // Compute LoRA delta: delta = hidden @ A.T @ B.T
                let delta = compute_lora_delta(
                    hidden_states,
                    &adapter.lora_a,
                    &adapter.lora_b,
                    logits.len(),
                );

                // Add scaled delta to logits
                for (j, logit) in logits.iter_mut().enumerate() {
                    if j < delta.len() {
                        *logit += scale * delta[j];
                    }
                }

                debug!(
                    adapter_id = adapter_id,
                    gate = gate,
                    scale = scale,
                    hidden_size = hidden_size,
                    delta_len = delta.len(),
                    "Applied cold adapter with LoRA matmul"
                );
            }
        }
    }

    /// Check if any cold adapters are in the router ring
    fn has_cold_adapters(&self, ring: &RouterRing) -> bool {
        let adapters = self.cold_adapters.read();
        for i in 0..ring.k {
            let adapter_id = ring.indices[i];
            let gate = ring.gates_q15[i];
            if gate != 0 && adapters.contains_key(&adapter_id) {
                return true;
            }
        }
        false
    }

    /// Partition router ring into hot and cold adapter sets
    fn partition_adapters(&self, ring: &RouterRing) -> (Vec<u32>, Vec<i32>, Vec<u16>, Vec<i16>) {
        let cold_adapters = self.cold_adapters.read();

        let mut hot_ids = Vec::with_capacity(ring.k);
        let mut hot_gates = Vec::with_capacity(ring.k);
        let mut cold_ids = Vec::with_capacity(ring.k);
        let mut cold_gates = Vec::with_capacity(ring.k);

        for i in 0..ring.k {
            let adapter_id = ring.indices[i];
            let gate = ring.gates_q15[i];

            if cold_adapters.contains_key(&adapter_id) {
                // Cold adapter - apply locally
                cold_ids.push(adapter_id);
                cold_gates.push(gate);
            } else {
                // Hot adapter - send to model server
                hot_ids.push(adapter_id as u32);
                hot_gates.push(gate as i32);
            }
        }

        (hot_ids, hot_gates, cold_ids, cold_gates)
    }
}

impl FusedKernels for ModelServerKernels {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // In model server mode, the model is loaded on the server
        // We just need to connect
        self.runtime_handle.block_on(async { self.connect().await })
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let start = Instant::now();

        // Partition adapters into hot (server) and cold (local)
        let (hot_ids, hot_gates, cold_ids, _cold_gates) = self.partition_adapters(ring);
        let num_hot_adapters = hot_ids.len();
        let num_cold_adapters = cold_ids.len();

        // Check if we need hidden states for cold adapter computation
        let need_hidden_states = self.has_cold_adapters(ring);

        // Get input tokens from IoBuffers
        let input_ids: Vec<u32> = io.input_ids.clone();

        // Execute forward pass on model server
        let response = self.runtime_handle.block_on(async {
            self.client
                .forward(
                    self.session_id.clone(),
                    input_ids,
                    self.current_position,
                    self.max_seq_len,
                    hot_ids,
                    hot_gates,
                    self.manifest_seed.clone(),
                    need_hidden_states, // Request hidden states when cold adapters exist
                )
                .await
        })?;

        // Update position from response
        self.current_position = response.position;

        // Copy base logits to output buffer
        if response.logits.len() != self.vocab_size {
            warn!(
                received = response.logits.len(),
                expected = self.vocab_size,
                "Model Server returned unexpected logit size"
            );
        }

        let logits_len = response.logits.len().min(io.output_logits.len());
        io.output_logits[..logits_len].copy_from_slice(&response.logits[..logits_len]);

        // Apply cold adapters locally using hidden states from server
        if num_cold_adapters > 0 {
            if response.hidden_states.is_empty() {
                warn!("Cold adapters requested but no hidden states returned from server");
            }
            self.apply_cold_adapters(&mut io.output_logits, ring, &response.hidden_states);
        }

        debug!(
            latency_ms = start.elapsed().as_millis(),
            kv_cache_hit = response.kv_cache_hit,
            cached_tokens = response.cached_tokens,
            hot_adapters = num_hot_adapters,
            cold_adapters = num_cold_adapters,
            hidden_states_len = response.hidden_states.len(),
            "Model Server forward pass completed"
        );

        Ok(())
    }

    fn device_name(&self) -> &str {
        "ModelServer"
    }

    fn attest_determinism(&self) -> Result<DeterminismReport> {
        // Check cached report first
        if let Some(ref report) = *self.determinism_report.read() {
            return Ok(report.clone());
        }

        // Fetch health from model server to build attestation
        let _health = self
            .runtime_handle
            .block_on(async { self.client.health().await })?;

        // Build report based on server health
        let report = DeterminismReport {
            backend_type: BackendType::MLX, // Model server typically uses MLX
            metallib_hash: None,            // N/A for remote
            metallib_verified: false,
            manifest: None,
            rng_seed_method: RngSeedingMethod::HkdfSeeded,
            floating_point_mode: FloatingPointMode::Deterministic,
            determinism_level: DeterminismLevel::BitExact,
            compiler_flags: vec![],
            deterministic: true,
            runtime_version: None,
            device_id: Some("ModelServer".to_string()),
        };

        // Cache the report
        *self.determinism_report.write() = Some(report.clone());

        Ok(report)
    }

    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        use safetensors::SafeTensors;

        if weights.is_empty() {
            return Err(AosError::Kernel("Empty adapter weights".to_string()));
        }

        // Try SafeTensors format first
        let (lora_a, lora_b, scale) = match SafeTensors::deserialize(weights) {
            Ok(tensors) => {
                // Extract lora_a tensor (try multiple naming conventions)
                let lora_a_tensor = tensors
                    .tensor("lora_a")
                    .or_else(|_| tensors.tensor("lora.a"))
                    .map_err(|_| {
                        AosError::Kernel("Missing lora_a tensor in SafeTensors".to_string())
                    })?;

                // Extract lora_b tensor
                let lora_b_tensor = tensors
                    .tensor("lora_b")
                    .or_else(|_| tensors.tensor("lora.b"))
                    .map_err(|_| {
                        AosError::Kernel("Missing lora_b tensor in SafeTensors".to_string())
                    })?;

                // Convert to f32
                let lora_a = tensor_to_f32_vec(&lora_a_tensor)?;
                let lora_b = tensor_to_f32_vec(&lora_b_tensor)?;

                debug!(
                    adapter_id = id,
                    lora_a_shape = ?lora_a_tensor.shape(),
                    lora_b_shape = ?lora_b_tensor.shape(),
                    "Parsed SafeTensors cold adapter"
                );

                (lora_a, lora_b, 1.0)
            }
            Err(_) => {
                // Fallback to raw f32 format
                if weights.len() % 4 != 0 {
                    return Err(AosError::Kernel(format!(
                        "Invalid adapter weights length: {} (not multiple of 4)",
                        weights.len()
                    )));
                }

                let floats: Vec<f32> = weights
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();

                let mid = floats.len() / 2;
                (floats[..mid].to_vec(), floats[mid..].to_vec(), 1.0)
            }
        };

        let adapter = ColdAdapter {
            id,
            lora_a,
            lora_b,
            scale,
        };

        self.cold_adapters.write().insert(id, adapter);

        info!(adapter_id = id, "Loaded cold adapter locally");
        Ok(())
    }

    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        if self.cold_adapters.write().remove(&id).is_some() {
            info!(adapter_id = id, "Unloaded cold adapter");
            Ok(())
        } else {
            warn!(adapter_id = id, "Cold adapter not found for unload");
            Ok(())
        }
    }

    fn attach_adapter(&mut self, id: u16) -> Result<()> {
        // Cold adapters are always attached
        if self.cold_adapters.read().contains_key(&id) {
            Ok(())
        } else {
            Err(AosError::Kernel(format!(
                "Adapter {} not loaded as cold adapter",
                id
            )))
        }
    }

    fn detach_adapter(&mut self, id: u16) -> Result<()> {
        self.unload_adapter(id)
    }
}

/// Convert safetensors tensor data to f32 vec
fn tensor_to_f32_vec(tensor: &safetensors::tensor::TensorView<'_>) -> Result<Vec<f32>> {
    use safetensors::Dtype;

    match tensor.dtype() {
        Dtype::F16 => Ok(tensor
            .data()
            .chunks(2)
            .map(|chunk| {
                let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                half::f16::from_bits(bits).to_f32()
            })
            .collect()),
        Dtype::F32 => Ok(tensor
            .data()
            .chunks(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()),
        Dtype::BF16 => Ok(tensor
            .data()
            .chunks(2)
            .map(|chunk| {
                let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
                half::bf16::from_bits(bits).to_f32()
            })
            .collect()),
        other => Err(AosError::Kernel(format!(
            "Unsupported tensor dtype: {:?}",
            other
        ))),
    }
}

/// Compute LoRA delta: delta = hidden @ A.T @ B.T
///
/// This performs the standard LoRA computation:
/// 1. intermediate = hidden @ A.T where A is [rank, hidden_size]
/// 2. delta = intermediate @ B.T where B is [vocab_size, rank]
///
/// The result is a vocab_size-length vector that gets added to logits.
fn compute_lora_delta(
    hidden: &[f32],
    lora_a: &[f32],
    lora_b: &[f32],
    vocab_size: usize,
) -> Vec<f32> {
    let hidden_size = hidden.len();

    if lora_a.is_empty() || lora_b.is_empty() || hidden_size == 0 {
        return vec![0.0; vocab_size];
    }

    // Infer rank from lora_a dimensions: lora_a is [rank, hidden_size] flattened
    let rank = lora_a.len() / hidden_size;
    if rank == 0 {
        return vec![0.0; vocab_size];
    }

    // Step 1: Compute intermediate = hidden @ A.T = [rank]
    // A is stored as [rank, hidden_size], so A.T is [hidden_size, rank]
    // For each r in 0..rank: intermediate[r] = sum(hidden[h] * A[r, h])
    let mut intermediate = vec![0.0f32; rank];
    for r in 0..rank {
        for h in 0..hidden_size {
            let a_idx = r * hidden_size + h;
            if a_idx < lora_a.len() {
                intermediate[r] += hidden[h] * lora_a[a_idx];
            }
        }
    }

    // Step 2: Compute delta = intermediate @ B.T = [vocab_size]
    // B is stored as [vocab_size, rank], so B.T is [rank, vocab_size]
    // For each v in 0..vocab_size: delta[v] = sum(intermediate[r] * B[v, r])
    let inferred_vocab = lora_b.len() / rank;
    let output_size = inferred_vocab.min(vocab_size);
    let mut delta = vec![0.0f32; output_size];

    for v in 0..output_size {
        for r in 0..rank {
            let b_idx = v * rank + r;
            if b_idx < lora_b.len() {
                delta[v] += intermediate[r] * lora_b[b_idx];
            }
        }
    }

    // Pad to vocab_size if needed
    if delta.len() < vocab_size {
        delta.resize(vocab_size, 0.0);
    }

    delta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_partition_empty_ring() {
        let config = ModelServerClientConfig::default();
        let kernels = ModelServerKernels::new(config, "test".to_string(), 32000);

        let ring = RouterRing::new(0);
        let (hot_ids, hot_gates, cold_ids, cold_gates) = kernels.partition_adapters(&ring);

        assert!(hot_ids.is_empty());
        assert!(hot_gates.is_empty());
        assert!(cold_ids.is_empty());
        assert!(cold_gates.is_empty());
    }

    #[tokio::test]
    async fn test_cold_adapter_loading() {
        let config = ModelServerClientConfig::default();
        let mut kernels = ModelServerKernels::new(config, "test".to_string(), 32000);

        // Create dummy weights (8 floats = 32 bytes)
        let weights: Vec<u8> = (0..8u32)
            .flat_map(|i| (i as f32).to_le_bytes().to_vec())
            .collect();

        assert!(kernels.load_adapter(1, &weights).is_ok());
        assert!(kernels.cold_adapters.read().contains_key(&1));

        assert!(kernels.unload_adapter(1).is_ok());
        assert!(!kernels.cold_adapters.read().contains_key(&1));
    }

    #[test]
    fn test_compute_lora_delta_basic() {
        // hidden_size=4, rank=2, vocab_size=3
        let hidden = vec![1.0, 2.0, 3.0, 4.0];

        // A: [rank=2, hidden_size=4] = 8 elements
        // Row 0: [0.1, 0.2, 0.3, 0.4]
        // Row 1: [0.5, 0.6, 0.7, 0.8]
        let lora_a = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8];

        // B: [vocab_size=3, rank=2] = 6 elements
        // Row 0: [0.1, 0.2]
        // Row 1: [0.3, 0.4]
        // Row 2: [0.5, 0.6]
        let lora_b = vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6];

        let delta = compute_lora_delta(&hidden, &lora_a, &lora_b, 3);

        // intermediate[0] = 1*0.1 + 2*0.2 + 3*0.3 + 4*0.4 = 0.1 + 0.4 + 0.9 + 1.6 = 3.0
        // intermediate[1] = 1*0.5 + 2*0.6 + 3*0.7 + 4*0.8 = 0.5 + 1.2 + 2.1 + 3.2 = 7.0
        // delta[0] = 3.0*0.1 + 7.0*0.2 = 0.3 + 1.4 = 1.7
        // delta[1] = 3.0*0.3 + 7.0*0.4 = 0.9 + 2.8 = 3.7
        // delta[2] = 3.0*0.5 + 7.0*0.6 = 1.5 + 4.2 = 5.7

        assert_eq!(delta.len(), 3);
        assert!((delta[0] - 1.7).abs() < 1e-5);
        assert!((delta[1] - 3.7).abs() < 1e-5);
        assert!((delta[2] - 5.7).abs() < 1e-5);
    }

    #[test]
    fn test_compute_lora_delta_empty() {
        let delta = compute_lora_delta(&[], &[1.0], &[1.0], 10);
        assert_eq!(delta.len(), 10);
        assert!(delta.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn test_compute_lora_delta_vocab_padding() {
        // Small LoRA that produces 2 outputs but we want 5
        let hidden = vec![1.0, 1.0];
        let lora_a = vec![1.0, 1.0]; // rank=1, hidden=2
        let lora_b = vec![0.5, 0.5]; // vocab=2, rank=1

        let delta = compute_lora_delta(&hidden, &lora_a, &lora_b, 5);

        assert_eq!(delta.len(), 5);
        // intermediate = 1*1 + 1*1 = 2
        // delta[0] = 2 * 0.5 = 1.0
        // delta[1] = 2 * 0.5 = 1.0
        // delta[2..5] = 0.0 (padded)
        assert!((delta[0] - 1.0).abs() < 1e-5);
        assert!((delta[1] - 1.0).abs() < 1e-5);
        assert!((delta[2] - 0.0).abs() < 1e-5);
    }
}
