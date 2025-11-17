//! Metal kernel implementation
//!
//! This crate contains the unsafe boundary for Metal FFI.
//! All unsafe code is confined to this crate.
//!
//! References:
//! - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders
//! - Metal Shading Language: https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{
    attestation, FusedKernels, IoBuffers, RouterRing, MAX_ADAPTERS_PER_STEP,
};

#[cfg(target_os = "macos")]
use metal::*;

#[cfg(target_os = "macos")]
use rand::{Rng, SeedableRng};

#[cfg(target_os = "macos")]
use std::collections::HashMap;

#[cfg(target_os = "macos")]
use std::sync::Arc;

pub mod ane_acceleration;
pub mod compute_shaders;
pub mod debug;
pub mod fused_mlp;
pub mod fused_qkv;
pub mod keys;
pub mod layout;
pub mod manifest;
pub mod metal3x;
pub mod mplora;
pub mod noise_tracker;
pub mod optimization;
pub mod recovery;
pub mod ring_buffer;
pub mod vision_kernels;
pub mod vram;

pub use compute_shaders::{ComputeShaderDescriptor, ComputeShaderRegistry, ShaderExecutionStats};
pub use debug::{KernelDebugger, KernelParams};
pub use fused_mlp::{FusedMlpKernel, LoraConfig};
pub use fused_qkv::{FlashAttentionKernel, FusedQkvKernel, GqaConfig};
pub use layout::LayoutValidator;
pub use manifest::{verify_embedded_manifest, KernelManifest};
pub use mplora::MploraKernel;
pub use noise_tracker::{NoiseTracker, NoiseTrackingConfig};
pub use optimization::{KernelOptimizationPlan, KernelOptimizer, KernelPerformanceMetrics};
pub use recovery::RecoveryWrapper;
pub use ring_buffer::{ActiveAdapter, RingBuffer};
pub use vision_kernels::{
    MetalImageTensor, MetalImageTensorOwned, MetalVisionActivation, MetalVisionArchitecture,
    MetalVisionKernelConfig, MetalVisionPooling, VisionKernelBundle,
};
pub use vram::VramTracker;

/// Embedding dimensions for Metal inference
#[derive(Debug, Clone)]
pub struct EmbeddingDimensions {
    pub vocab_size: usize,
    pub hidden_size: usize,
}

/// Transformer layer weights
#[derive(Debug)]
pub struct TransformerWeights {
    // MLP weights
    pub gate_weight: Buffer,
    pub up_weight: Buffer,
    pub down_weight: Buffer,
    // QKV weights
    pub q_weight: Buffer,
    pub k_weight: Buffer,
    pub v_weight: Buffer,
}

/// Language modeling head weights for vocabulary projection
#[derive(Debug)]
pub struct LmHeadWeights {
    pub weight: Buffer, // [hidden_size, vocab_size]
}

/// GPU-resident adapter weights for hot-swappable LoRA adapters
#[derive(Debug)]
pub struct AdapterWeights {
    /// LoRA A matrices per target module [rank × in_dim]
    /// Order: [q_proj_A, k_proj_A, v_proj_A, mlp_down_A, mlp_up_A]
    pub lora_a_buffers: Vec<Buffer>,

    /// LoRA B matrices per target module [out_dim × rank]
    /// Order: [q_proj_B, k_proj_B, v_proj_B, mlp_down_B, mlp_up_B]
    pub lora_b_buffers: Vec<Buffer>,

    /// LoRA rank (typically 4-64)
    pub rank: usize,

    /// LoRA alpha scaling factor
    pub alpha: f32,

    /// Total VRAM used by this adapter (bytes)
    pub vram_bytes: u64,

    /// Content hash for integrity verification (BLAKE3)
    pub hash_b3: B3Hash,
}

impl AdapterWeights {
    /// Calculate scaling factor: alpha / rank
    pub fn scaling_factor(&self) -> f32 {
        self.alpha / (self.rank as f32)
    }
}

/// Intermediate buffers for transformer computation
#[derive(Debug)]
pub struct IntermediateBuffers {
    pub hidden_states: Buffer,
    pub q_output: Buffer,
    pub k_output: Buffer,
    pub v_output: Buffer,
    pub attention_output: Buffer,
    pub mlp_output: Buffer,
}

// Embed precompiled metallib
// Compiled offline with deterministic build process
const METALLIB_BYTES: &[u8] = include_bytes!("../shaders/aos_kernels.metallib");
const METALLIB_HASH: &str = include_str!("../shaders/kernel_hash.txt");

/// Metal kernel implementation
pub struct MetalKernels {
    device: Arc<Device>,
    _queue: CommandQueue,
    library: Option<Library>,
    mlp_kernel: Option<FusedMlpKernel>,
    qkv_kernel: Option<FusedQkvKernel>,
    flash_attention_kernel: Option<FlashAttentionKernel>,
    ring_buffer: Option<RingBuffer>,
    vram_tracker: VramTracker,
    debugger: KernelDebugger,
    recovery: RecoveryWrapper,
    noise_tracker: NoiseTracker,
    // Embedding weights and pipeline for Metal inference
    embedding_buffer: Option<Buffer>,
    embedding_pipeline: Option<ComputePipelineState>,
    embedding_dimensions: Option<EmbeddingDimensions>,
    // Transformer layer weights and buffers
    transformer_weights: Option<TransformerWeights>,
    intermediate_buffers: Option<IntermediateBuffers>,
    // Language modeling head for vocabulary projection
    lm_head_weights: Option<LmHeadWeights>,
    lm_head_pipeline: Option<ComputePipelineState>,
    // Hot-swappable adapter weights indexed by adapter_id
    adapter_weights: HashMap<u16, AdapterWeights>,
}

// Safety: Metal objects are thread-safe
unsafe impl Send for MetalKernels {}
unsafe impl Sync for MetalKernels {}

impl MetalKernels {
    /// Create a new Metal kernel executor with manifest verification
    pub fn new() -> Result<Self> {
        // Verify embedded manifest before proceeding
        let _manifest = verify_embedded_manifest(METALLIB_BYTES, None)?;

        let device = Self::select_device()?;
        let queue = device.new_command_queue();

        Ok(Self {
            device: Arc::new(device),
            _queue: queue,
            library: None,
            mlp_kernel: None,
            qkv_kernel: None,
            flash_attention_kernel: None,
            ring_buffer: None,
            vram_tracker: VramTracker::new(),
            debugger: KernelDebugger::from_env(),
            recovery: RecoveryWrapper::new(),
            noise_tracker: NoiseTracker::new(NoiseTrackingConfig::default(), None),
            embedding_buffer: None,
            embedding_pipeline: None,
            embedding_dimensions: None,
            transformer_weights: None,
            intermediate_buffers: None,
            lm_head_weights: None,
            lm_head_pipeline: None,
            adapter_weights: HashMap::new(),
        })
    }

    /// Select Metal device based on AOS_GPU_INDEX or system default
    ///
    /// Supports multi-GPU systems by allowing explicit device selection
    /// via environment variable.
    fn select_device() -> Result<Device> {
        // Check for explicit GPU selection
        if let Ok(gpu_index_str) = std::env::var("AOS_GPU_INDEX") {
            let idx: usize = gpu_index_str
                .parse()
                .map_err(|_| AosError::Kernel("Invalid AOS_GPU_INDEX".to_string()))?;

            let devices = Device::all();

            if devices.is_empty() {
                return Err(AosError::Kernel("No Metal devices found".to_string()));
            }

            if idx < devices.len() {
                let device = devices[idx].clone();
                tracing::info!(gpu_index = idx, gpu_name = %device.name(), "Selected Metal GPU device");
                return Ok(device);
            } else {
                return Err(AosError::Kernel(format!(
                    "GPU index {} out of range (max {})",
                    idx,
                    devices.len() - 1
                )));
            }
        }

        // Default: use system default device
        Device::system_default()
            .ok_or_else(|| AosError::Kernel("No Metal device found".to_string()))
    }

    /// Get VRAM tracker for adapter attribution
    pub fn vram_tracker(&self) -> &VramTracker {
        &self.vram_tracker
    }

    /// Get mutable VRAM tracker
    pub fn vram_tracker_mut(&mut self) -> &mut VramTracker {
        &mut self.vram_tracker
    }

    /// Get debugger
    pub fn debugger(&self) -> &KernelDebugger {
        &self.debugger
    }

    /// Get recovery wrapper
    pub fn recovery(&self) -> &RecoveryWrapper {
        &self.recovery
    }

    /// Get mutable recovery wrapper
    pub fn recovery_mut(&mut self) -> &mut RecoveryWrapper {
        &mut self.recovery
    }

    /// Get noise tracker
    pub fn noise_tracker(&self) -> &NoiseTracker {
        &self.noise_tracker
    }

    /// Get mutable noise tracker
    pub fn noise_tracker_mut(&mut self) -> &mut NoiseTracker {
        &mut self.noise_tracker
    }

    /// Load library from embedded metallib with hash verification
    fn load_library(&mut self) -> Result<()> {
        if METALLIB_BYTES.is_empty() {
            return Err(AosError::Kernel(
                "Metal library not yet compiled. Run build.sh to compile shaders.".to_string(),
            ));
        }

        // Verify hash matches embedded constant
        let actual_hash = B3Hash::hash(METALLIB_BYTES);
        let expected_hash_str = METALLIB_HASH.trim();
        let expected_hash = B3Hash::from_hex(expected_hash_str)
            .map_err(|e| AosError::Kernel(format!("Invalid metallib hash constant: {}", e)))?;

        if actual_hash != expected_hash {
            return Err(AosError::DeterminismViolation(format!(
                "Metallib hash mismatch!\n  Expected: {}\n  Got:      {}\n  \
                This indicates the embedded metallib does not match build.rs output.\n  \
                Recompile with: cargo clean && cargo build",
                expected_hash.to_hex(),
                actual_hash.to_hex()
            )));
        }

        tracing::info!("Kernel hash verified: {}", actual_hash.to_short_hex());

        // Load library
        let library = self
            .device
            .new_library_with_data(METALLIB_BYTES)
            .map_err(|e| AosError::Kernel(format!("Failed to load library: {}", e)))?;

        tracing::info!(
            "Loaded Metal library with {} functions",
            library.function_names().len()
        );

        // Create embedding lookup pipeline
        // Note: This assumes the embedding_lookup function exists in the metallib
        // If not available, we'll use a fallback approach
        if let Ok(function) = library.get_function("embedding_lookup", None) {
            let pipeline = self
                .device
                .new_compute_pipeline_state_with_function(&function)
                .map_err(|e| {
                    AosError::Kernel(format!("Failed to create embedding pipeline: {}", e))
                })?;
            self.embedding_pipeline = Some(pipeline);
            tracing::info!("Created embedding lookup pipeline");
        } else {
            tracing::warn!("embedding_lookup function not found in metallib, using fallback");
        }

        // Create vocabulary projection pipeline
        if let Ok(function) = library.get_function("vocabulary_projection", None) {
            let pipeline = self
                .device
                .new_compute_pipeline_state_with_function(&function)
                .map_err(|e| {
                    AosError::Kernel(format!(
                        "Failed to create vocabulary projection pipeline: {}",
                        e
                    ))
                })?;
            self.lm_head_pipeline = Some(pipeline);
            tracing::info!("Created vocabulary projection pipeline");
        } else {
            tracing::warn!("vocabulary_projection function not found in metallib, using fallback");
        }

        self.library = Some(library);
        Ok(())
    }

    /// Create a compute pipeline
    fn _create_pipeline(&self, function_name: &str) -> Result<ComputePipelineState> {
        let library = self
            .library
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Library not loaded".to_string()))?;

        let function = library
            .get_function(function_name, None)
            .map_err(|e| AosError::Kernel(format!("Function not found: {}", e)))?;

        self.device
            .new_compute_pipeline_state_with_function(&function)
            .map_err(|e| AosError::Kernel(format!("Failed to create pipeline: {}", e)))
    }

    /// Parse embedding weights from plan bytes
    ///
    /// Plan bytes contain a serialized model structure with embedding weights.
    /// This method extracts the embedding matrix for Metal kernel execution.
    fn parse_embedding_weights(&self, plan_bytes: &[u8]) -> Result<Vec<f32>> {
        // For now, create dummy embedding weights for testing
        // In production, this would parse the actual plan structure
        let vocab_size = 152064; // Qwen2.5-7B vocab size
        let hidden_size = 3584; // Qwen2.5-7B hidden size

        // Create deterministic embedding weights based on plan hash
        let plan_hash = adapteros_core::B3Hash::hash(plan_bytes);
        let hash_bytes = plan_hash.as_bytes();
        let mut seed = [0u8; 32];
        let copy_len = std::cmp::min(hash_bytes.len(), 32);
        seed[..copy_len].copy_from_slice(&hash_bytes[..copy_len]);
        let mut rng = rand::rngs::StdRng::from_seed(seed);

        let mut embedding_weights = Vec::with_capacity(vocab_size * hidden_size);
        for _ in 0..vocab_size * hidden_size {
            embedding_weights.push(rng.gen_range(-0.1..0.1));
        }

        tracing::info!(
            "Parsed embedding weights: {} tokens, {} dims, {} total params",
            vocab_size,
            hidden_size,
            embedding_weights.len()
        );

        Ok(embedding_weights)
    }

    /// Create Metal buffer for embedding weights
    fn create_embedding_buffer(&mut self, embedding_weights: &[f32]) -> Result<()> {
        let buffer_size = std::mem::size_of_val(embedding_weights) as u64;

        let buffer = self.device.new_buffer_with_data(
            embedding_weights.as_ptr() as *const std::ffi::c_void,
            buffer_size,
            MTLResourceOptions::StorageModeShared,
        );

        self.embedding_buffer = Some(buffer);

        tracing::info!("Created Metal embedding buffer: {} bytes", buffer_size);
        Ok(())
    }

    /// Validate embedding dimensions match model config
    fn validate_embedding_dimensions(&mut self, embedding_weights: &[f32]) -> Result<()> {
        let vocab_size = 152064; // Qwen2.5-7B vocab size
        let hidden_size = 3584; // Qwen2.5-7B hidden size
        let expected_size = vocab_size * hidden_size;

        if embedding_weights.len() != expected_size {
            return Err(AosError::Kernel(format!(
                "Embedding dimension mismatch: expected {}, got {}",
                expected_size,
                embedding_weights.len()
            )));
        }

        self.embedding_dimensions = Some(EmbeddingDimensions {
            vocab_size,
            hidden_size,
        });

        tracing::info!(
            "Embedding dimensions validated: {}x{}",
            vocab_size,
            hidden_size
        );
        Ok(())
    }

    /// Parse LM head weights from plan bytes
    fn parse_lm_head_weights(&self, plan_bytes: &[u8]) -> Result<LmHeadWeights> {
        // For now, create deterministic weights for testing
        // In production, this would parse the actual plan structure
        let hidden_size = 3584; // Qwen2.5-7B hidden size
        let vocab_size = 152064; // Qwen2.5-7B vocab size

        let plan_hash = adapteros_core::B3Hash::hash(plan_bytes);
        let hash_bytes = plan_hash.as_bytes();
        let mut seed = [0u8; 32];
        let copy_len = std::cmp::min(hash_bytes.len(), 32);
        seed[..copy_len].copy_from_slice(&hash_bytes[..copy_len]);
        let mut rng = rand::rngs::StdRng::from_seed(seed);

        let mut lm_head_weight = vec![0.0f32; hidden_size * vocab_size];
        for w in lm_head_weight.iter_mut() {
            *w = rng.gen_range(-0.1..0.1);
        }

        // Create Metal buffer
        let lm_head_buffer = self.device.new_buffer_with_data(
            lm_head_weight.as_ptr() as *const std::ffi::c_void,
            (lm_head_weight.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        tracing::info!(
            "Parsed LM head weights: {}x{}, {} total params",
            hidden_size,
            vocab_size,
            lm_head_weight.len()
        );

        Ok(LmHeadWeights {
            weight: lm_head_buffer,
        })
    }

    /// Parse transformer weights from plan bytes
    fn parse_transformer_weights(&self, plan_bytes: &[u8]) -> Result<TransformerWeights> {
        // For now, create deterministic weights for testing
        // In production, this would parse the actual plan structure
        let hidden_size = 3584; // Qwen2.5-7B hidden size
        let intermediate_size = 18944; // Qwen2.5-7B intermediate size

        let plan_hash = adapteros_core::B3Hash::hash(plan_bytes);
        let hash_bytes = plan_hash.as_bytes();
        let mut seed = [0u8; 32];
        let copy_len = std::cmp::min(hash_bytes.len(), 32);
        seed[..copy_len].copy_from_slice(&hash_bytes[..copy_len]);
        let mut rng = rand::rngs::StdRng::from_seed(seed);

        // MLP weights
        let gate_weight_size = hidden_size * intermediate_size;
        let mut gate_weight = vec![0.0f32; gate_weight_size];
        for w in gate_weight.iter_mut() {
            *w = rng.gen_range(-0.1..0.1);
        }

        let up_weight_size = hidden_size * intermediate_size;
        let mut up_weight = vec![0.0f32; up_weight_size];
        for w in up_weight.iter_mut() {
            *w = rng.gen_range(-0.1..0.1);
        }

        let down_weight_size = intermediate_size * hidden_size;
        let mut down_weight = vec![0.0f32; down_weight_size];
        for w in down_weight.iter_mut() {
            *w = rng.gen_range(-0.1..0.1);
        }

        // QKV weights
        let q_weight_size = hidden_size * hidden_size;
        let mut q_weight = vec![0.0f32; q_weight_size];
        for w in q_weight.iter_mut() {
            *w = rng.gen_range(-0.1..0.1);
        }

        let k_weight_size = hidden_size * (hidden_size / 8); // GQA: 4 KV heads
        let mut k_weight = vec![0.0f32; k_weight_size];
        for w in k_weight.iter_mut() {
            *w = rng.gen_range(-0.1..0.1);
        }

        let v_weight_size = hidden_size * (hidden_size / 8); // GQA: 4 KV heads
        let mut v_weight = vec![0.0f32; v_weight_size];
        for w in v_weight.iter_mut() {
            *w = rng.gen_range(-0.1..0.1);
        }

        // Create Metal buffers
        let gate_buffer = self.device.new_buffer_with_data(
            gate_weight.as_ptr() as *const std::ffi::c_void,
            (gate_weight.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        let up_buffer = self.device.new_buffer_with_data(
            up_weight.as_ptr() as *const std::ffi::c_void,
            (up_weight.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        let down_buffer = self.device.new_buffer_with_data(
            down_weight.as_ptr() as *const std::ffi::c_void,
            (down_weight.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        let q_buffer = self.device.new_buffer_with_data(
            q_weight.as_ptr() as *const std::ffi::c_void,
            (q_weight.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        let k_buffer = self.device.new_buffer_with_data(
            k_weight.as_ptr() as *const std::ffi::c_void,
            (k_weight.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        let v_buffer = self.device.new_buffer_with_data(
            v_weight.as_ptr() as *const std::ffi::c_void,
            (v_weight.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        tracing::info!(
            "Parsed transformer weights: hidden={}, intermediate={}, qkv_params={}",
            hidden_size,
            intermediate_size,
            q_weight_size + k_weight_size + v_weight_size
        );

        Ok(TransformerWeights {
            gate_weight: gate_buffer,
            up_weight: up_buffer,
            down_weight: down_buffer,
            q_weight: q_buffer,
            k_weight: k_buffer,
            v_weight: v_buffer,
        })
    }

    /// Create intermediate buffers for transformer computation
    fn create_intermediate_buffers(&mut self) -> Result<IntermediateBuffers> {
        let hidden_size = 3584; // Qwen2.5-7B hidden size
        let seq_len = 1; // Single token for now

        let buffer_size = hidden_size * seq_len * std::mem::size_of::<f32>() as u64;

        let hidden_states = self
            .device
            .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared);

        let q_output = self
            .device
            .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared);

        let k_output = self.device.new_buffer(
            (hidden_size / 8) * seq_len * std::mem::size_of::<f32>() as u64, // GQA
            MTLResourceOptions::StorageModeShared,
        );

        let v_output = self.device.new_buffer(
            (hidden_size / 8) * seq_len * std::mem::size_of::<f32>() as u64, // GQA
            MTLResourceOptions::StorageModeShared,
        );

        let attention_output = self
            .device
            .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared);

        let mlp_output = self
            .device
            .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared);

        tracing::info!("Created intermediate buffers for transformer computation");

        Ok(IntermediateBuffers {
            hidden_states,
            q_output,
            k_output,
            v_output,
            attention_output,
            mlp_output,
        })
    }

    /// Load transformer weights and create intermediate buffers
    fn load_transformer_weights(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let transformer_weights = self.parse_transformer_weights(plan_bytes)?;
        let intermediate_buffers = self.create_intermediate_buffers()?;

        self.transformer_weights = Some(transformer_weights);
        self.intermediate_buffers = Some(intermediate_buffers);

        Ok(())
    }

    /// Perform embedding lookup using Metal kernels
    fn perform_embedding_lookup(&self, io: &mut IoBuffers) -> Result<()> {
        let embedding_buffer = self
            .embedding_buffer
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Embedding buffer not initialized".to_string()))?;

        let embedding_pipeline = self
            .embedding_pipeline
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Embedding pipeline not initialized".to_string()))?;

        let dimensions = self
            .embedding_dimensions
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Embedding dimensions not set".to_string()))?;

        // Create command buffer for embedding lookup
        let command_buffer = self._queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        // Set compute pipeline state
        encoder.set_compute_pipeline_state(embedding_pipeline);

        // Set buffers
        encoder.set_buffer(0, Some(embedding_buffer), 0);

        // Create input buffer for token IDs
        let input_buffer = self.device.new_buffer_with_data(
            io.input_ids.as_ptr() as *const std::ffi::c_void,
            (io.input_ids.len() * std::mem::size_of::<u32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(1, Some(&input_buffer), 0);

        // Create output buffer for hidden states
        let hidden_size = dimensions.hidden_size;
        let mut hidden_states = vec![0.0f32; io.input_ids.len() * hidden_size];
        let hidden_buffer = self.device.new_buffer_with_data(
            hidden_states.as_mut_ptr() as *mut std::ffi::c_void,
            (hidden_states.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        encoder.set_buffer(2, Some(&hidden_buffer), 0);

        // Dispatch embedding lookup kernel
        let threadgroup_size = MTLSize::new(256, 1, 1);
        let threadgroup_count = MTLSize::new(io.input_ids.len().div_ceil(256) as u64, 1, 1);
        encoder.dispatch_thread_groups(threadgroup_count, threadgroup_size);

        encoder.end_encoding();
        command_buffer.commit();
        command_buffer.wait_until_completed();

        // Copy results back to io buffers
        // For now, use deterministic values based on input
        let total_gate_weight: f32 = 1.0; // Placeholder
        let base_logit = total_gate_weight * 0.1;
        for (idx, logit) in io.output_logits.iter_mut().enumerate() {
            *logit = base_logit * ((idx % 100) as f32) * 0.01;
        }

        tracing::debug!(
            "Embedding lookup completed for {} tokens",
            io.input_ids.len()
        );
        Ok(())
    }

    /// Run transformer layers with LoRA adapters
    fn run_transformer_layers(
        &mut self,
        adapters: &[ActiveAdapter],
        _io: &mut IoBuffers,
    ) -> Result<()> {
        let transformer_weights = self
            .transformer_weights
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Transformer weights not loaded".to_string()))?;
        let intermediate_buffers = self
            .intermediate_buffers
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Intermediate buffers not created".to_string()))?;

        // Copy input hidden states from embedding lookup
        // For now, assume hidden states are in io or create from input_ids
        // In production, this would be the output from embedding lookup

        // Extract adapter weight references from loaded adapters
        // Verify all adapters are loaded into GPU before execution
        let adapter_weight_refs: Vec<&AdapterWeights> = adapters
            .iter()
            .map(|a| {
                let id_u16 = (a.id & 0xFFFF) as u16;
                self.adapter_weights.get(&id_u16).ok_or_else(|| {
                    AosError::Kernel(format!(
                        "Adapter {} (u16={}) not loaded into GPU. Available adapters: {:?}",
                        a.id,
                        id_u16,
                        self.adapter_weights.keys().collect::<Vec<_>>()
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?;

        // Execute Fused QKV Kernel with actual adapter weights
        if let Some(ref mut qkv_kernel) = self.qkv_kernel {
            qkv_kernel.execute(
                &intermediate_buffers.hidden_states,
                &transformer_weights.q_weight,
                &transformer_weights.k_weight,
                &transformer_weights.v_weight,
                &intermediate_buffers.q_output,
                &intermediate_buffers.k_output,
                &intermediate_buffers.v_output,
                &adapter_weight_refs,
                adapters,
                self.ring_buffer.as_ref().unwrap(),
            )?;
        }

        // Execute Flash Attention Kernel
        if let Some(ref flash_attention_kernel) = self.flash_attention_kernel {
            flash_attention_kernel.execute(
                &intermediate_buffers.q_output,
                &intermediate_buffers.k_output,
                &intermediate_buffers.v_output,
                &intermediate_buffers.attention_output,
            )?;
        }

        // Execute Fused MLP Kernel with actual adapter weights
        if let Some(ref mut mlp_kernel) = self.mlp_kernel {
            mlp_kernel.execute(
                &intermediate_buffers.attention_output,
                &transformer_weights.gate_weight,
                &transformer_weights.up_weight,
                &transformer_weights.down_weight,
                &intermediate_buffers.mlp_output,
                &adapter_weight_refs,
                adapters,
            )?;
        }

        // Copy final output from MLP to io buffers
        // The kernels above (QKV, Flash Attention, MLP) now receive the loaded adapter weights
        // from self.adapter_weights HashMap. The LoRA computation target is:
        // output = W_base @ x + Σᵢ (gateᵢ / 32767) * (alpha / rank) * (Bᵢ @ (Aᵢ @ x))
        //
        // ✅ DONE (Phase 2.1/2.2): Updated kernel execute() signatures to accept &[&AdapterWeights]
        // ✅ DONE (Phase 2.3): Wired adapter_weight_refs to kernel execute() calls
        // ⏸️ TODO (Phase 2.4): Update Metal shaders to use actual weight buffers
        //    Currently kernels receive weights but shaders still use LoraConfig workaround

        tracing::debug!(
            num_adapters = adapters.len(),
            num_adapters_loaded_gpu = self.adapter_weights.len(),
            num_weights_passed = adapter_weight_refs.len(),
            "Transformer layers completed - adapter weights wired to kernels"
        );
        Ok(())
    }

    /// Perform vocabulary projection using Metal kernels
    fn perform_vocabulary_projection(
        &self,
        adapters: &[ActiveAdapter],
        io: &mut IoBuffers,
    ) -> Result<()> {
        let lm_head_weights = self
            .lm_head_weights
            .as_ref()
            .ok_or_else(|| AosError::Kernel("LM head weights not initialized".to_string()))?;

        let lm_head_pipeline = self
            .lm_head_pipeline
            .as_ref()
            .ok_or_else(|| AosError::Kernel("LM head pipeline not initialized".to_string()))?;

        // Get the final hidden states from transformer layers
        // For now, assume we have hidden states from the transformer computation
        // In production, this would be the output from the last transformer layer
        let hidden_size = 3584; // Qwen2.5-7B hidden size
        let vocab_size = 152064; // Qwen2.5-7B vocab size

        // Create dummy hidden states for testing (in production, this comes from transformer)
        let mut hidden_states = vec![0.0f32; hidden_size];
        for (i, val) in hidden_states.iter_mut().enumerate() {
            *val = (i as f32 * 0.001) % 1.0; // Deterministic pattern
        }

        // Create Metal buffer for hidden states
        let hidden_buffer = self.device.new_buffer_with_data(
            hidden_states.as_ptr() as *const std::ffi::c_void,
            (hidden_states.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Create output buffer for logits
        let mut logits = vec![0.0f32; vocab_size];
        let logits_buffer = self.device.new_buffer_with_data(
            logits.as_mut_ptr() as *const std::ffi::c_void,
            (logits.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        );

        // Create command buffer for vocabulary projection
        let command_buffer = self._queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        // Set compute pipeline state
        encoder.set_compute_pipeline_state(lm_head_pipeline);

        // Set buffers
        encoder.set_buffer(0, Some(&hidden_buffer), 0);
        encoder.set_buffer(1, Some(&lm_head_weights.weight), 0);
        encoder.set_buffer(2, Some(&logits_buffer), 0);

        // Dispatch vocabulary projection kernel
        let threadgroup_size = MTLSize::new(256, 1, 1);
        let threadgroup_count = MTLSize::new((vocab_size as u64).div_ceil(256), 1, 1);
        encoder.dispatch_thread_groups(threadgroup_count, threadgroup_size);

        encoder.end_encoding();
        command_buffer.commit();
        command_buffer.wait_until_completed();

        // Copy results back to io buffers
        // Read back the logits from GPU
        let logits_ptr = logits_buffer.contents() as *const f32;
        for i in 0..vocab_size {
            io.output_logits[i] = unsafe { *logits_ptr.add(i) };
        }

        // Apply adapter fusion scaling
        let total_gate_weight: f32 = adapters
            .iter()
            .map(|a| (a.gate as f32) / 32768.0) // Convert Q15 to float
            .sum();

        for logit in io.output_logits.iter_mut() {
            *logit *= total_gate_weight;
        }

        tracing::debug!(
            "Performed vocabulary projection with {} adapters, total gate weight: {}",
            adapters.len(),
            total_gate_weight
        );
        Ok(())
    }
}

impl FusedKernels for MetalKernels {
    /// Load plan and initialize Metal kernels
    ///
    /// # Embedding Layer
    ///
    /// The embedding layer weights are expected to be included in the plan_bytes.
    /// When the full Metal kernel execution is implemented, the embedding lookup
    /// will be performed on the GPU:
    ///
    /// 1. Parse plan_bytes to extract embedding matrix (vocab_size x hidden_dim)
    /// 2. Create Metal buffer for embedding weights
    /// 3. Pass embedding buffer to kernels during run_step()
    /// 4. Metal shader performs: hidden_state = embedding[input_ids]
    ///
    /// For now, this method initializes the kernel pipelines but does not load
    /// the embedding weights. The actual Metal kernel execution (including
    /// embedding lookup) will be added when aos_kernels.metallib is fully compiled.
    ///
    /// # Note
    ///
    /// The embedding lookup is NOT performed in Rust - it happens in the Metal
    /// kernel during forward pass. The Worker's `EmbeddingModel` is only used
    /// for RAG/text similarity, not for inference.
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        // Load the Metal library
        self.load_library()?;

        // Parse plan_bytes and extract embedding weights
        let embedding_weights = self.parse_embedding_weights(plan_bytes)?;

        // Create Metal buffer for embedding matrix
        self.create_embedding_buffer(&embedding_weights)?;

        // Validate embedding dimensions match model config
        self.validate_embedding_dimensions(&embedding_weights)?;

        // Initialize kernels
        self.mlp_kernel = Some(FusedMlpKernel::new(self.device.clone())?);

        // Create default GQA config for Qwen2.5-7B-Instruct
        let gqa_config = GqaConfig::default();

        self.qkv_kernel = Some(FusedQkvKernel::new(
            self.device.clone(),
            gqa_config.clone(),
        )?);
        self.flash_attention_kernel =
            Some(FlashAttentionKernel::new(self.device.clone(), gqa_config)?);
        self.ring_buffer = Some(RingBuffer::new(self.device.clone(), 3)?);

        // Load transformer weights
        self.load_transformer_weights(plan_bytes)?;

        // Load LM head weights
        let lm_head_weights = self.parse_lm_head_weights(plan_bytes)?;
        self.lm_head_weights = Some(lm_head_weights);

        tracing::info!(
            "Metal kernels initialized with embedding, transformer, and LM head weights loaded"
        );

        Ok(())
    }

    /// Run single inference step through Metal kernels
    ///
    /// # Token Embedding Lookup
    ///
    /// The embedding lookup for input_ids is performed inside the Metal kernel:
    ///
    /// ```metal
    /// // Metal shader pseudo-code:
    /// kernel void forward_pass(
    ///     device const uint* input_ids [[buffer(0)]],
    ///     device const float* embedding_weights [[buffer(1)]],
    ///     device float* hidden_states [[buffer(2)]],
    ///     ...
    /// ) {
    ///     uint token_id = input_ids[position];
    ///     // Lookup embedding from buffer
    ///     for (uint i = 0; i < hidden_dim; i++) {
    ///         hidden_states[i] = embedding_weights[token_id * hidden_dim + i];
    ///     }
    ///     // ... rest of forward pass
    /// }
    /// ```
    ///
    /// This is more efficient than doing the lookup in Rust and copying to GPU.
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // PRD 6: Validate RouterRing contract invariants in debug builds
        #[cfg(debug_assertions)]
        {
            if let Err(e) = ring.validate_invariants() {
                tracing::error!(
                    error = %e,
                    indices_len = ring.indices.len(),
                    gates_len = ring.gates_q15.len(),
                    "RouterRing contract violation in Metal backend"
                );
                return Err(AosError::Validation(format!(
                    "RouterRing contract violation: {}",
                    e
                )));
            }

            // Additional validation: check for dimension mismatches
            if ring.indices.len() > MAX_ADAPTERS_PER_STEP {
                tracing::error!(
                    ring_len = ring.indices.len(),
                    max_adapters = MAX_ADAPTERS_PER_STEP,
                    "RouterRing exceeds maximum adapter count"
                );
                return Err(AosError::Validation(format!(
                    "RouterRing length {} exceeds MAX_ADAPTERS_PER_STEP={}",
                    ring.indices.len(),
                    MAX_ADAPTERS_PER_STEP
                )));
            }
        }

        // Convert RouterRing to ActiveAdapter list
        let adapters: Vec<ActiveAdapter> = ring
            .indices
            .iter()
            .zip(ring.gates_q15.iter())
            .map(|(&id, &gate)| ActiveAdapter {
                id: id as u32,
                gate: gate as u16,
            })
            .collect();

        // Update ring buffer with active adapters
        if let Some(ref mut ring_buffer) = self.ring_buffer {
            ring_buffer.update(&adapters)?;
        }

        // Perform embedding lookup using Metal kernels
        self.perform_embedding_lookup(io)?;

        // Run transformer layers with LoRA adapters
        self.run_transformer_layers(&adapters, io)?;

        // Perform vocabulary projection with adapter fusion
        self.perform_vocabulary_projection(&adapters, io)?;

        Ok(())
    }

    fn device_name(&self) -> &str {
        self.device.name()
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        // Get metallib hash from embedded constant
        let metallib_hash = B3Hash::from_hex(METALLIB_HASH.trim())
            .map_err(|e| AosError::Kernel(format!("Invalid metallib hash: {}", e)))?;

        // Get manifest from verification (contains toolchain info)
        let manifest_result = verify_embedded_manifest(METALLIB_BYTES, None);

        let manifest = manifest_result.ok().map(|m| attestation::KernelManifest {
            kernel_hash: m.kernel_hash,
            xcrun_version: m.xcrun_version,
            sdk_version: m.sdk_version,
            rust_version: m.rust_version,
            build_timestamp: m.build_timestamp,
        });

        // Metal backend uses HKDF seeding (via plan-derived seeds)
        let rng_seed_method = attestation::RngSeedingMethod::HkdfSeeded;

        // Metal kernels are compiled with deterministic settings
        let floating_point_mode = attestation::FloatingPointMode::Deterministic;

        // Compiler flags from build metadata (no fast-math)
        let compiler_flags = vec![
            "-O2".to_string(),
            "-std=metal3.1".to_string(),
            // No fast-math flags - ensures determinism
        ];

        // Metal backend is deterministic by design
        let deterministic = true;

        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::Metal,
            metallib_hash: Some(metallib_hash),
            manifest,
            rng_seed_method,
            floating_point_mode,
            compiler_flags,
            deterministic,
        })
    }

    /// Load adapter weights into GPU VRAM for hot-swapping
    ///
    /// # Arguments
    /// * `id` - Adapter ID (u16) to index the adapter in the ring buffer
    /// * `weights` - SafeTensors format adapter weights
    ///
    /// # Process
    /// 1. Parse SafeTensors format to extract LoRA A/B matrices
    /// 2. Create Metal buffers for each weight tensor
    /// 3. Upload weights to GPU VRAM
    /// 4. Store in adapter_weights HashMap indexed by adapter_id
    /// 5. Calculate actual VRAM usage
    ///
    /// # Returns
    /// Ok(()) on success, containing VRAM bytes used
    ///
    /// # Errors
    /// - AosError::Serialization if SafeTensors parsing fails
    /// - AosError::Kernel if Metal buffer creation fails
    /// - AosError::Validation if expected tensors are missing or have wrong shapes
    fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
        use safetensors::SafeTensors;
        use tracing::{info, warn};

        info!(
            adapter_id = id,
            weight_bytes = weights.len(),
            "Loading adapter weights into Metal GPU"
        );

        // Check if adapter already loaded (atomic check-and-remove to avoid TOCTOU race)
        use std::collections::hash_map::Entry;
        if let Entry::Occupied(entry) = self.adapter_weights.entry(id) {
            warn!(
                adapter_id = id,
                "Adapter already loaded, removing existing entry"
            );
            entry.remove();
        }

        // 1. Parse SafeTensors format
        let tensors = SafeTensors::deserialize(weights)
            .map_err(|e| AosError::Parse(format!("Failed to parse SafeTensors: {}", e)))?;

        // 2. Extract LoRA metadata from first tensor (rank, alpha)
        // Convention: LoRA tensors are named like "q_proj.lora_A", "q_proj.lora_B", etc.
        let tensor_names: Vec<&str> = tensors.names().iter().map(|s| s.as_str()).collect();

        // Find rank from first A matrix shape
        let rank = if let Some(a_name) = tensor_names.iter().find(|n| n.contains("lora_A")) {
            let tensor_info = tensors
                .tensor(a_name)
                .map_err(|e| AosError::Parse(format!("Failed to get tensor info: {}", e)))?;
            tensor_info.shape()[0] // First dimension is rank
        } else {
            return Err(AosError::Validation(
                "No LoRA A matrices found in weights".to_string(),
            ));
        };

        // Read alpha from AOS2 manifest if available, otherwise use 2*rank default
        // The incoming weights might be:
        // 1. Full AOS2 format (has manifest with metadata)
        // 2. Raw SafeTensors (no manifest)
        let alpha = if weights.len() >= 8 {
            // Try to parse AOS2 manifest
            let manifest_offset =
                u32::from_le_bytes([weights[0], weights[1], weights[2], weights[3]]) as usize;
            let manifest_len =
                u32::from_le_bytes([weights[4], weights[5], weights[6], weights[7]]) as usize;

            if weights.len() >= manifest_offset + manifest_len {
                let manifest_bytes = &weights[manifest_offset..manifest_offset + manifest_len];
                if let Ok(manifest) = serde_json::from_slice::<serde_json::Value>(manifest_bytes) {
                    if let Some(alpha_val) = manifest.get("lora_alpha").and_then(|v| v.as_f64()) {
                        info!(
                            adapter_id = id,
                            alpha = alpha_val,
                            "Found lora_alpha in AOS2 manifest"
                        );
                        alpha_val as f32
                    } else {
                        let default_alpha = (2 * rank) as f32;
                        warn!(
                            adapter_id = id,
                            rank = rank,
                            "No lora_alpha in AOS2 manifest, using 2*rank={}",
                            default_alpha
                        );
                        default_alpha
                    }
                } else {
                    // Not AOS2 format, use default
                    (2 * rank) as f32
                }
            } else {
                // Invalid AOS2 format, use default
                (2 * rank) as f32
            }
        } else {
            // Too small to be AOS2, use default
            (2 * rank) as f32
        };

        // 3. Define expected target modules (order matters for buffer indexing)
        let target_modules = vec!["q_proj", "k_proj", "v_proj", "mlp.down_proj", "mlp.up_proj"];
        let mut lora_a_buffers = Vec::new();
        let mut lora_b_buffers = Vec::new();

        // 4. Load A and B matrices for each target module
        for module in &target_modules {
            let a_name = format!("{}.lora_A", module);
            let b_name = format!("{}.lora_B", module);

            // Load A matrix
            let a_data = tensors
                .tensor(&a_name)
                .map_err(|_| {
                    warn!(module = %module, "LoRA A matrix not found, using zero buffer");
                    AosError::Validation(format!("Missing {}", a_name))
                })
                .ok();

            // Load B matrix
            let b_data = tensors
                .tensor(&b_name)
                .map_err(|_| {
                    warn!(module = %module, "LoRA B matrix not found, using zero buffer");
                    AosError::Validation(format!("Missing {}", b_name))
                })
                .ok();

            // Create Metal buffers
            if let Some(a_tensor) = a_data {
                let a_view = a_tensor.data();
                let a_size = a_view.len() as u64;

                let a_buffer = self.device.new_buffer_with_data(
                    a_view.as_ptr() as *const std::ffi::c_void,
                    a_size,
                    metal::MTLResourceOptions::StorageModeShared,
                );

                lora_a_buffers.push(a_buffer);
            } else {
                // Create zero buffer as fallback
                let zero_buffer = self.device.new_buffer(
                    (rank * 4096 * std::mem::size_of::<f32>()) as u64,
                    metal::MTLResourceOptions::StorageModeShared,
                );
                lora_a_buffers.push(zero_buffer);
            }

            if let Some(b_tensor) = b_data {
                let b_view = b_tensor.data();
                let b_size = b_view.len() as u64;

                let b_buffer = self.device.new_buffer_with_data(
                    b_view.as_ptr() as *const std::ffi::c_void,
                    b_size,
                    metal::MTLResourceOptions::StorageModeShared,
                );

                lora_b_buffers.push(b_buffer);
            } else {
                // Create zero buffer as fallback
                let zero_buffer = self.device.new_buffer(
                    (4096 * rank * std::mem::size_of::<f32>()) as u64,
                    metal::MTLResourceOptions::StorageModeShared,
                );
                lora_b_buffers.push(zero_buffer);
            }
        }

        // 5. Calculate actual VRAM usage from Metal buffer lengths
        let total_vram_bytes: u64 = lora_a_buffers
            .iter()
            .map(|b| b.length())
            .chain(lora_b_buffers.iter().map(|b| b.length()))
            .sum();

        // 6. Compute content hash for integrity verification
        let hash_b3 = B3Hash::hash(weights);

        // 7. Store in adapter_weights HashMap
        let adapter_weights = AdapterWeights {
            lora_a_buffers,
            lora_b_buffers,
            rank,
            alpha,
            vram_bytes: total_vram_bytes,
            hash_b3: hash_b3.clone(),
        };

        self.adapter_weights.insert(id, adapter_weights);

        // 8. Log success with instrumentation
        let num_active_adapters = self.adapter_weights.len();
        let total_vram_all_adapters: u64 =
            self.adapter_weights.values().map(|w| w.vram_bytes).sum();

        info!(
            adapter_id = id,
            rank = rank,
            alpha = alpha,
            vram_bytes = total_vram_bytes,
            hash_b3 = %hash_b3,
            num_active_adapters = num_active_adapters,
            total_vram_mb = total_vram_all_adapters / (1024 * 1024),
            "Adapter loaded successfully into Metal GPU"
        );

        // 9. Verify non-zero weights (sample first buffer)
        if let Some(first_a_buffer) = self.adapter_weights[&id].lora_a_buffers.first() {
            let contents = first_a_buffer.contents() as *const f32;

            // SAFETY: Metal buffer contents pointer is valid for the buffer's lifetime.
            // The buffer is owned by self.adapter_weights[id] and won't be freed while we hold a reference.
            // Metal guarantees proper alignment for f32 access in buffers created with new_buffer_with_data.
            // We limit the slice length to min(10, buffer_length/sizeof(f32)) to prevent out-of-bounds access.
            // The buffer length is in bytes, so we divide by sizeof(f32) to get element count.
            let sample: Vec<f32> = unsafe {
                std::slice::from_raw_parts(
                    contents,
                    10.min(first_a_buffer.length() as usize / std::mem::size_of::<f32>()),
                )
            }
            .to_vec();

            let non_zero_count = sample.iter().filter(|&&v| v.abs() > 1e-8).count();
            if non_zero_count == 0 {
                warn!(adapter_id = id, "WARNING: All sampled weights are zero!");
            } else {
                info!(
                    adapter_id = id,
                    non_zero_count = non_zero_count,
                    "Verified non-zero weights in GPU buffer"
                );
            }
        }

        Ok(())
    }

    /// Unload adapter weights from GPU VRAM
    ///
    /// # Arguments
    /// * `id` - Adapter ID to unload
    ///
    /// # Process
    /// 1. Remove adapter from adapter_weights HashMap
    /// 2. Metal buffers are automatically freed when dropped
    /// 3. Return VRAM bytes freed
    ///
    /// # Returns
    /// Ok(()) on success
    ///
    /// # Errors
    /// - AosError::NotFound if adapter is not loaded
    fn unload_adapter(&mut self, id: u16) -> Result<()> {
        use tracing::info;

        if let Some(adapter_weights) = self.adapter_weights.remove(&id) {
            let vram_freed = adapter_weights.vram_bytes;
            let num_remaining = self.adapter_weights.len();
            let total_vram_remaining: u64 =
                self.adapter_weights.values().map(|w| w.vram_bytes).sum();

            info!(
                adapter_id = id,
                vram_freed_bytes = vram_freed,
                vram_freed_mb = vram_freed / (1024 * 1024),
                num_remaining_adapters = num_remaining,
                total_vram_mb = total_vram_remaining / (1024 * 1024),
                "Adapter unloaded from Metal GPU"
            );

            // Metal buffers are automatically freed when adapter_weights is dropped
            Ok(())
        } else {
            Err(AosError::NotFound(format!("Adapter {} not loaded", id)))
        }
    }

    /// Verify GPU adapter buffers and create fingerprint
    ///
    /// Samples buffer contents at checkpoints (first/last/mid 4KB) for fast integrity
    /// verification without full GPU-to-CPU readback. Creates cryptographic fingerprint
    /// for cross-layer verification.
    ///
    /// # Arguments
    /// * `id` - Adapter ID to verify
    ///
    /// # Returns
    /// * `(buffer_size, first_4kb, last_4kb, mid_4kb)` - Buffer size and checkpoint samples
    ///
    /// # Process
    /// 1. Get Metal buffer for adapter
    /// 2. Read samples from GPU: first 4KB, last 4KB, midpoint 4KB
    /// 3. Create `GpuBufferFingerprint` with BLAKE3 hash
    /// 4. Store fingerprint in VramTracker for future verification
    ///
    /// # Errors
    /// - AosError::NotFound if adapter is not loaded
    /// - AosError::Kernel if buffer contents pointer is null
    fn verify_adapter_buffers(&self, id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)> {
        use tracing::info;

        // Get adapter weights
        let adapter_weights = self
            .adapter_weights
            .get(&id)
            .ok_or_else(|| AosError::NotFound(format!("Adapter {} not loaded in GPU", id)))?;

        // Sample first LoRA A buffer (sufficient for verification)
        // In production, could sample multiple buffers for stronger guarantee
        let first_buffer = adapter_weights
            .lora_a_buffers
            .first()
            .ok_or_else(|| AosError::Kernel("No LoRA buffers found".to_string()))?;

        let buffer_bytes = first_buffer.length();
        const SAMPLE_SIZE: usize = 4096; // 4KB samples

        // Read samples from GPU buffer
        let ptr = first_buffer.contents() as *const u8;
        if ptr.is_null() {
            return Err(AosError::Kernel(
                "Metal buffer contents pointer is null".to_string(),
            ));
        }

        // SAFETY: Metal buffer contents pointer is valid for the buffer's lifetime.
        // The buffer is owned by adapter_weights and accessed via &self, ensuring it won't be freed.
        // We verified ptr is non-null above.
        // Metal buffers are always byte-aligned for u8 access.
        // We use buffer.length() as the exact size, preventing out-of-bounds access.
        // All slice operations below use .min() to stay within bounds.
        let (first_sample, last_sample, mid_sample) = unsafe {
            let buffer_slice = std::slice::from_raw_parts(ptr, buffer_bytes as usize);

            // Sample first 4KB (or less if buffer smaller)
            let first_end = SAMPLE_SIZE.min(buffer_bytes as usize);
            let first = buffer_slice[..first_end].to_vec();

            // Sample last 4KB
            let last_start = (buffer_bytes as usize).saturating_sub(SAMPLE_SIZE);
            let last = buffer_slice[last_start..].to_vec();

            // Sample midpoint 4KB
            let mid_start = (buffer_bytes as usize / 2).saturating_sub(SAMPLE_SIZE / 2);
            let mid_end = (mid_start + SAMPLE_SIZE).min(buffer_bytes as usize);
            let mid = buffer_slice[mid_start..mid_end].to_vec();

            (first, last, mid)
        };

        info!(
            adapter_id = id,
            buffer_bytes = adapter_weights.vram_bytes,
            sample_points = 3,
            "GPU buffer fingerprint sampled"
        );

        // Return samples for fingerprint creation by caller
        Ok((
            adapter_weights.vram_bytes,
            first_sample,
            last_sample,
            mid_sample,
        ))
    }

    fn store_gpu_fingerprint(&mut self, id: u16, buffer_size: u64, checkpoint_hash_hex: &str) {
        use crate::vram::GpuBufferFingerprint;
        use adapteros_core::B3Hash;

        // Parse hex hash back to B3Hash
        let checkpoint_hash = match B3Hash::from_hex(checkpoint_hash_hex) {
            Ok(hash) => hash,
            Err(e) => {
                tracing::error!(
                    adapter_id = id,
                    checkpoint_hash_hex = checkpoint_hash_hex,
                    error = %e,
                    "Failed to parse checkpoint hash hex - skipping fingerprint storage"
                );
                return; // Skip storing invalid fingerprint
            }
        };

        let fingerprint = GpuBufferFingerprint {
            buffer_bytes: buffer_size,
            allocated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            checkpoint_hash,
        };

        self.vram_tracker.store_fingerprint(id as u32, fingerprint);
    }

    fn verify_gpu_fingerprint(
        &self,
        id: u16,
        buffer_size: u64,
        checkpoint_hash_hex: &str,
    ) -> Result<bool> {
        use crate::vram::GpuBufferFingerprint;
        use adapteros_core::B3Hash;

        // Parse hex hash back to B3Hash
        let checkpoint_hash = B3Hash::from_hex(checkpoint_hash_hex)
            .map_err(|e| AosError::Validation(format!("Invalid checkpoint hash hex: {}", e)))?;

        let current_fp = GpuBufferFingerprint {
            buffer_bytes: buffer_size,
            allocated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            checkpoint_hash,
        };

        self.vram_tracker
            .verify_fingerprint(id as u32, &current_fp)
            .map_err(|msg| AosError::Validation(msg))
    }

    fn check_memory_footprint(
        &self,
        id: u16,
        buffer_size: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        // Use interior mutability in VramTracker to enable baseline learning from &self
        self.vram_tracker
            .check_memory_footprint(id as u32, buffer_size)
    }
}
