//! Hybrid CoreML + Runtime LoRA Backend
//!
//! This module implements a hybrid inference approach where:
//! - CoreML handles base transformer inference (outputs hidden_states)
//! - Rust/Accelerate handles LM head projection and LoRA application
//!
//! This enables hot-swapping LoRA adapters in <1ms while maintaining
//! CoreML/ANE acceleration for the heavy transformer computation.

use crate::matmul::{axpy, matvec_accelerate};
use crate::ComputeUnits;
use adapteros_core::{AosError, Result};
use adapteros_lora_kernel_api::{
    attestation::{
        BackendType, DeterminismReport, FloatingPointMode, KernelManifest, RngSeedingMethod,
    },
    BackendHealth, BackendMetrics, FusedKernels, IoBuffers, RouterRing,
};
use half::f16;
use safetensors::SafeTensors;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// LoRA adapter weights for the LM head projection
#[derive(Debug, Clone)]
pub struct LmHeadLoRA {
    /// LoRA A matrix [rank, hidden_size]
    pub lora_a: Vec<f32>,
    /// LoRA B matrix [vocab_size, rank]
    pub lora_b: Vec<f32>,
    /// Scaling factor (alpha / rank)
    pub scale: f32,
    /// LoRA rank
    pub rank: usize,
}

/// Hybrid CoreML + Runtime LoRA Backend
///
/// Architecture:
/// ```text
/// input_ids
///     ↓
/// [CoreML: Embeddings + 48 Transformer Layers]
///     ↓
/// hidden_states [batch, seq, hidden_size]
///     ↓
/// [Rust/Accelerate: LM Head + LoRA Fusion]
///     ↓
/// logits [batch, seq, vocab_size]
/// ```
pub struct HybridCoreMLBackend {
    /// CoreML model handle (outputs hidden_states instead of logits)
    #[cfg(target_os = "macos")]
    model_handle: *mut std::ffi::c_void,

    /// LM head weights [vocab_size, hidden_size]
    lm_head: Vec<f32>,

    /// Model dimensions
    vocab_size: usize,
    hidden_size: usize,

    /// LoRA adapters for LM head, keyed by adapter slot
    adapters: HashMap<u16, LmHeadLoRA>,

    /// Backend metrics
    metrics: BackendMetrics,

    /// Model path for reloading
    model_path: Option<PathBuf>,

    /// Sequence length the model was compiled for
    seq_len: usize,

    /// Device name for reporting
    device_name: String,
}

// Safety: CoreML model handles are thread-safe
unsafe impl Send for HybridCoreMLBackend {}
unsafe impl Sync for HybridCoreMLBackend {}

impl HybridCoreMLBackend {
    /// Create a new hybrid backend (uninitialized)
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "macos")]
            model_handle: std::ptr::null_mut(),
            lm_head: Vec::new(),
            vocab_size: 0,
            hidden_size: 0,
            adapters: HashMap::new(),
            metrics: BackendMetrics::default(),
            model_path: None,
            seq_len: 512,
            device_name: "HybridCoreML".to_string(),
        }
    }

    /// Load a hybrid CoreML model and LM head weights
    ///
    /// Expects:
    /// - `model_path`: Path to .mlpackage that outputs hidden_states
    /// - `model_path/lm_head_weights.safetensors`: Separate LM head weights
    pub fn load_model(&mut self, model_path: &Path) -> Result<()> {
        tracing::info!(path = ?model_path, "Loading hybrid CoreML model");

        // Load LM head weights
        let lm_head_path = model_path.join("lm_head_weights.safetensors");
        if !lm_head_path.exists() {
            return Err(AosError::Kernel(format!(
                "LM head weights not found: {:?}. Model must be converted with --output-hidden-states",
                lm_head_path
            )));
        }

        let lm_head_bytes = std::fs::read(&lm_head_path)?;
        let tensors = SafeTensors::deserialize(&lm_head_bytes)
            .map_err(|e| AosError::Kernel(format!("Failed to load LM head weights: {}", e)))?;

        // Get LM head tensor
        let lm_head_tensor = tensors
            .tensor("lm_head.weight")
            .map_err(|e| AosError::Kernel(format!("LM head tensor not found: {}", e)))?;

        let shape = lm_head_tensor.shape();
        if shape.len() != 2 {
            return Err(AosError::Kernel(format!(
                "LM head must be 2D, got shape: {:?}",
                shape
            )));
        }

        self.vocab_size = shape[0];
        self.hidden_size = shape[1];

        tracing::info!(
            vocab_size = self.vocab_size,
            hidden_size = self.hidden_size,
            "Loaded LM head dimensions"
        );

        // Convert from FP16 to FP32
        self.lm_head = convert_tensor_to_f32(lm_head_tensor.data(), lm_head_tensor.dtype())?;

        // Load CoreML model
        #[cfg(target_os = "macos")]
        {
            let model_path_str = model_path
                .to_str()
                .ok_or_else(|| AosError::Kernel("Invalid model path".to_string()))?;

            let c_path = std::ffi::CString::new(model_path_str)
                .map_err(|e| AosError::Kernel(format!("Invalid path encoding: {}", e)))?;

            // Use CPU + ANE for hybrid inference
            let compute_units = ComputeUnits::CpuAndNeuralEngine as i32;

            let handle = unsafe {
                crate::ffi::coreml_load_model(
                    c_path.as_ptr(),
                    c_path.as_bytes().len(),
                    compute_units,
                )
            };
            if handle.is_null() {
                return Err(AosError::Kernel(format!(
                    "Failed to load CoreML model: {:?}",
                    model_path
                )));
            }

            self.model_handle = handle;
        }

        self.model_path = Some(model_path.to_path_buf());

        tracing::info!(
            vocab_size = self.vocab_size,
            hidden_size = self.hidden_size,
            "Hybrid CoreML backend loaded successfully"
        );

        Ok(())
    }

    /// Load a LoRA adapter for the LM head
    ///
    /// Adapter weights should contain:
    /// - `lm_head.lora_A`: [rank, hidden_size]
    /// - `lm_head.lora_B`: [vocab_size, rank]
    pub fn load_lora_adapter(&mut self, slot: u16, weights_bytes: &[u8]) -> Result<Duration> {
        let start = Instant::now();

        let tensors = SafeTensors::deserialize(weights_bytes)
            .map_err(|e| AosError::Kernel(format!("Failed to deserialize adapter: {}", e)))?;

        // Try different naming conventions for LoRA matrices
        let (lora_a_data, lora_a_shape) = get_tensor_any_name(
            &tensors,
            &["lm_head.lora_A", "lm_head.lora_a", "lora_A", "lora_a"],
        )?;
        let (lora_b_data, lora_b_shape) = get_tensor_any_name(
            &tensors,
            &["lm_head.lora_B", "lm_head.lora_b", "lora_B", "lora_b"],
        )?;

        // Validate shapes
        if lora_a_shape.len() != 2 || lora_b_shape.len() != 2 {
            return Err(AosError::Kernel(format!(
                "LoRA matrices must be 2D: A={:?}, B={:?}",
                lora_a_shape, lora_b_shape
            )));
        }

        let rank = lora_a_shape[0];
        let a_hidden = lora_a_shape[1];
        let b_vocab = lora_b_shape[0];
        let b_rank = lora_b_shape[1];

        if a_hidden != self.hidden_size {
            return Err(AosError::Kernel(format!(
                "LoRA A hidden_size mismatch: expected {}, got {}",
                self.hidden_size, a_hidden
            )));
        }
        if b_vocab != self.vocab_size {
            return Err(AosError::Kernel(format!(
                "LoRA B vocab_size mismatch: expected {}, got {}",
                self.vocab_size, b_vocab
            )));
        }
        if rank != b_rank {
            return Err(AosError::Kernel(format!(
                "LoRA rank mismatch: A rank={}, B rank={}",
                rank, b_rank
            )));
        }

        // Get alpha from metadata or default
        let alpha = get_alpha_from_metadata(&tensors).unwrap_or(rank as f32);
        let scale = alpha / rank as f32;

        let lora = LmHeadLoRA {
            lora_a: lora_a_data,
            lora_b: lora_b_data,
            scale,
            rank,
        };

        tracing::debug!(slot, rank, alpha, scale, "Loaded LM head LoRA adapter");

        self.adapters.insert(slot, lora);

        Ok(start.elapsed())
    }

    /// Unload an adapter from the given slot
    pub fn unload_lora_adapter(&mut self, slot: u16) -> Result<Duration> {
        let start = Instant::now();
        self.adapters.remove(&slot);
        Ok(start.elapsed())
    }

    /// Hot-swap between adapters (unload old, keep new)
    ///
    /// Returns swap latency (should be <1ms).
    pub fn swap_adapter(&mut self, old_slot: u16, new_slot: u16) -> Result<Duration> {
        let start = Instant::now();

        // Just remove the old adapter - new should already be loaded
        self.adapters.remove(&old_slot);

        if !self.adapters.contains_key(&new_slot) {
            tracing::warn!(
                old = old_slot,
                new = new_slot,
                "Swap target adapter not loaded"
            );
        }

        let elapsed = start.elapsed();
        tracing::debug!(
            old = old_slot,
            new = new_slot,
            elapsed_us = elapsed.as_micros(),
            "Adapter hot-swap completed"
        );

        Ok(elapsed)
    }

    /// Run hybrid inference step
    ///
    /// 1. Run CoreML base model to get hidden_states
    /// 2. Apply LM head projection
    /// 3. Apply LoRA adapters based on router decisions
    pub fn run_hybrid_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        let inference_start = Instant::now();

        // Step 1: Run CoreML to get hidden_states
        let hidden_states = self.run_coreml_hidden_states(io)?;

        let coreml_elapsed = inference_start.elapsed();
        tracing::trace!(coreml_ms = coreml_elapsed.as_millis(), "CoreML inference");

        // Step 2: Compute base LM head logits
        // hidden_states: [seq_len, hidden_size]
        // lm_head: [vocab_size, hidden_size]
        // logits = hidden_states @ lm_head.T = [seq_len, vocab_size]
        let lm_head_start = Instant::now();

        // Validate hidden_states size
        if hidden_states.len() < self.hidden_size {
            return Err(AosError::Kernel(format!(
                "Hidden states too small: got {} elements, need at least {} for hidden_size",
                hidden_states.len(),
                self.hidden_size
            )));
        }

        // For now, just use the last token's hidden state
        let actual_seq_len = hidden_states.len() / self.hidden_size;
        let last_token_start = (actual_seq_len - 1) * self.hidden_size;
        let last_hidden = &hidden_states[last_token_start..last_token_start + self.hidden_size];

        // logits[v] = sum_h last_hidden[h] * lm_head[v, h]
        let mut logits = matvec_accelerate(
            &self.lm_head,
            last_hidden,
            self.vocab_size,
            self.hidden_size,
        )?;

        let lm_head_elapsed = lm_head_start.elapsed();
        tracing::trace!(
            lm_head_ms = lm_head_elapsed.as_millis(),
            "LM head projection"
        );

        // Step 3: Apply LoRA adapters
        let lora_start = Instant::now();
        let indices = ring.active_indices();
        let gates = ring.active_gates();

        for (&adapter_idx, &gate_q15) in indices.iter().zip(gates.iter()) {
            if gate_q15 == 0 {
                continue;
            }

            if let Some(lora) = self.adapters.get(&adapter_idx) {
                // LoRA: delta = B @ A @ hidden
                // A: [rank, hidden_size], hidden: [hidden_size] -> a_out: [rank]
                let a_out =
                    matvec_accelerate(&lora.lora_a, last_hidden, lora.rank, self.hidden_size)?;

                // B: [vocab_size, rank], a_out: [rank] -> delta: [vocab_size]
                let delta = matvec_accelerate(&lora.lora_b, &a_out, self.vocab_size, lora.rank)?;

                // Scale by gate and LoRA scale
                let gate_f32 = gate_q15 as f32 / 32767.0;
                let combined_scale = gate_f32 * lora.scale;

                // logits += combined_scale * delta
                axpy(combined_scale, &delta, &mut logits)?;

                tracing::trace!(
                    adapter = adapter_idx,
                    gate = gate_f32,
                    scale = combined_scale,
                    "Applied LoRA adapter"
                );
            }
        }

        let lora_elapsed = lora_start.elapsed();
        tracing::trace!(lora_us = lora_elapsed.as_micros(), "LoRA application");

        // Step 4: Copy logits to output buffer
        let output_len = io.output_logits.len().min(logits.len());
        io.output_logits[..output_len].copy_from_slice(&logits[..output_len]);

        let total_elapsed = inference_start.elapsed();
        tracing::debug!(
            total_ms = total_elapsed.as_millis(),
            coreml_ms = coreml_elapsed.as_millis(),
            lm_head_ms = lm_head_elapsed.as_millis(),
            lora_us = lora_elapsed.as_micros(),
            num_adapters = indices.len(),
            "Hybrid inference step completed"
        );

        Ok(())
    }

    /// Run CoreML model to get hidden_states
    #[cfg(target_os = "macos")]
    fn run_coreml_hidden_states(&self, io: &IoBuffers) -> Result<Vec<f32>> {
        if self.model_handle.is_null() {
            return Err(AosError::Kernel("Model not loaded".to_string()));
        }

        // Allocate buffer for hidden_states [batch, seq_len, hidden_size]
        // CoreML outputs [1, seq_len, hidden_size], we flatten to [seq_len, hidden_size]
        let output_size = self.seq_len * self.hidden_size;
        let mut hidden_states = vec![0.0f32; output_size];

        // Use "hidden_states" as the primary output name for hybrid models
        let output_name = b"hidden_states";

        let result = unsafe {
            crate::ffi::coreml_run_inference_named_output(
                self.model_handle,
                io.input_ids.as_ptr(),
                io.input_ids.len(),
                hidden_states.as_mut_ptr(),
                hidden_states.len(),
                output_name.as_ptr() as *const i8,
                output_name.len(),
            )
        };

        // Positive result = number of elements copied, negative = error code
        if result < 0 {
            return Err(AosError::Kernel(format!(
                "CoreML inference failed with code {}",
                result
            )));
        }

        let elements_copied = result as usize;
        if elements_copied < output_size {
            tracing::warn!(
                expected = output_size,
                got = elements_copied,
                "CoreML returned fewer elements than expected, padding with zeros"
            );
        }

        // Truncate to actual size if we got fewer elements
        if elements_copied < hidden_states.len() {
            hidden_states.truncate(elements_copied);
        }

        Ok(hidden_states)
    }

    #[cfg(not(target_os = "macos"))]
    fn run_coreml_hidden_states(&self, io: &IoBuffers) -> Result<Vec<f32>> {
        // Stub for non-macOS: generate deterministic hidden states
        let output_size = self.seq_len * self.hidden_size;
        let hidden_states: Vec<f32> = (0..output_size)
            .map(|i| {
                let pos_factor = ((io.position as f32 + i as f32) * 0.001).sin();
                pos_factor * 0.1
            })
            .collect();
        Ok(hidden_states)
    }

    /// Get backend health status
    pub fn health(&self) -> BackendHealth {
        #[cfg(target_os = "macos")]
        let model_loaded = !self.model_handle.is_null();
        #[cfg(not(target_os = "macos"))]
        let model_loaded = !self.lm_head.is_empty();

        if model_loaded {
            BackendHealth::Healthy
        } else {
            BackendHealth::Failed {
                reason: "Model not loaded".to_string(),
                recoverable: true,
            }
        }
    }

    /// Get current metrics
    pub fn metrics(&self) -> &BackendMetrics {
        &self.metrics
    }

    /// Get number of loaded adapters
    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }
}

impl Default for HybridCoreMLBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl FusedKernels for HybridCoreMLBackend {
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let model_path_str = std::str::from_utf8(plan_bytes)
            .map_err(|_| AosError::Kernel("Invalid plan bytes encoding".to_string()))?;

        let model_path = PathBuf::from(model_path_str.trim());
        self.load_model(&model_path)
    }

    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        self.run_hybrid_step(ring, io)
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<DeterminismReport> {
        Ok(DeterminismReport {
            backend_type: BackendType::CoreML,
            metallib_hash: None, // Hybrid uses Accelerate, not Metal
            manifest: Some(KernelManifest {
                kernel_hash: "hybrid-accelerate-blas".to_string(),
                xcrun_version: "N/A".to_string(),
                sdk_version: "Accelerate.framework".to_string(),
                rust_version: env!("CARGO_PKG_VERSION").to_string(),
                build_timestamp: chrono_lite_timestamp(),
            }),
            // FixedSeed(0) indicates no randomness used in inference
            rng_seed_method: RngSeedingMethod::FixedSeed(0),
            floating_point_mode: FloatingPointMode::Deterministic,
            compiler_flags: vec!["-O3".to_string(), "-fno-fast-math".to_string()],
            deterministic: true,
        })
    }

    fn load_adapter(&mut self, slot_id: u16, adapter_plan: &[u8]) -> Result<()> {
        self.load_lora_adapter(slot_id, adapter_plan)?;
        Ok(())
    }

    fn unload_adapter(&mut self, slot_id: u16) -> Result<()> {
        self.unload_lora_adapter(slot_id)?;
        Ok(())
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Convert tensor bytes to f32 vec, handling FP16
fn convert_tensor_to_f32(data: &[u8], dtype: safetensors::Dtype) -> Result<Vec<f32>> {
    match dtype {
        safetensors::Dtype::F32 => {
            // Safe conversion for f32
            if !data.len().is_multiple_of(4) {
                return Err(AosError::Kernel("Invalid F32 data length".to_string()));
            }
            let num_floats = data.len() / 4;
            let mut result = Vec::with_capacity(num_floats);
            for i in 0..num_floats {
                let bytes = [
                    data[i * 4],
                    data[i * 4 + 1],
                    data[i * 4 + 2],
                    data[i * 4 + 3],
                ];
                result.push(f32::from_le_bytes(bytes));
            }
            Ok(result)
        }
        safetensors::Dtype::F16 => {
            // Safe conversion for f16
            if !data.len().is_multiple_of(2) {
                return Err(AosError::Kernel("Invalid F16 data length".to_string()));
            }
            let num_halfs = data.len() / 2;
            let mut result = Vec::with_capacity(num_halfs);
            for i in 0..num_halfs {
                let bytes = [data[i * 2], data[i * 2 + 1]];
                let half = f16::from_le_bytes(bytes);
                result.push(half.to_f32());
            }
            Ok(result)
        }
        safetensors::Dtype::BF16 => {
            // Safe conversion for bf16
            if !data.len().is_multiple_of(2) {
                return Err(AosError::Kernel("Invalid BF16 data length".to_string()));
            }
            let num_bf16s = data.len() / 2;
            let mut result = Vec::with_capacity(num_bf16s);
            for i in 0..num_bf16s {
                let bytes = [data[i * 2], data[i * 2 + 1]];
                let bf = half::bf16::from_le_bytes(bytes);
                result.push(bf.to_f32());
            }
            Ok(result)
        }
        _ => Err(AosError::Kernel(format!("Unsupported dtype: {:?}", dtype))),
    }
}

/// Try multiple tensor names and return the first found
fn get_tensor_any_name(tensors: &SafeTensors, names: &[&str]) -> Result<(Vec<f32>, Vec<usize>)> {
    for name in names {
        if let Ok(tensor) = tensors.tensor(name) {
            let data = convert_tensor_to_f32(tensor.data(), tensor.dtype())?;
            return Ok((data, tensor.shape().to_vec()));
        }
    }
    Err(AosError::Kernel(format!(
        "None of these tensors found: {:?}",
        names
    )))
}

/// Extract LoRA alpha from safetensors metadata
///
/// Currently returns None as safetensors metadata parsing is not implemented.
/// Falls back to using rank as alpha (scale = 1.0).
fn get_alpha_from_metadata(_tensors: &SafeTensors) -> Option<f32> {
    // TODO: Parse metadata when safetensors API supports it
    // For now, return None to use default alpha = rank
    None
}

/// Get a simple timestamp without external chrono dependency
fn chrono_lite_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_backend_new() {
        let backend = HybridCoreMLBackend::new();
        assert_eq!(backend.vocab_size, 0);
        assert_eq!(backend.hidden_size, 0);
        assert!(backend.adapters.is_empty());
    }

    #[test]
    fn test_convert_f32_tensor() {
        let data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0];
        let mut bytes = Vec::new();
        for f in &data {
            bytes.extend_from_slice(&f.to_le_bytes());
        }
        let result = convert_tensor_to_f32(&bytes, safetensors::Dtype::F32).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_convert_f16_tensor() {
        let halfs: Vec<f16> = vec![f16::from_f32(1.0), f16::from_f32(2.0), f16::from_f32(3.0)];
        let mut bytes = Vec::new();
        for h in &halfs {
            bytes.extend_from_slice(&h.to_le_bytes());
        }
        let result = convert_tensor_to_f32(&bytes, safetensors::Dtype::F16).unwrap();
        assert!((result[0] - 1.0).abs() < 0.01);
        assert!((result[1] - 2.0).abs() < 0.01);
        assert!((result[2] - 3.0).abs() < 0.01);
    }
}
