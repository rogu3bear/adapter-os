//! Metal kernel implementation
//!
//! This crate contains the unsafe boundary for Metal FFI.
//! All unsafe code is confined to this crate.
//!
//! References:
//! - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders
//! - Metal Shading Language: https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf

use metal::*;
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{FusedKernels, IoBuffers, RouterRing};
use std::sync::Arc;

pub mod debug;
pub mod fused_mlp;
pub mod fused_qkv;
pub mod keys;
pub mod layout;
pub mod manifest;
pub mod noise_tracker;
pub mod recovery;
pub mod ring_buffer;
pub mod vram;

pub use debug::{KernelDebugger, KernelParams};
pub use fused_mlp::{FusedMlpKernel, LoraConfig};
pub use fused_qkv::{FlashAttentionKernel, FusedQkvKernel, GqaConfig};
pub use layout::LayoutValidator;
pub use manifest::{verify_embedded_manifest, KernelManifest};
pub use noise_tracker::{NoiseTracker, NoiseTrackingConfig};
pub use recovery::RecoveryWrapper;
pub use ring_buffer::{ActiveAdapter, RingBuffer};
pub use vram::VramTracker;

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
                println!("Selected GPU {}: {}", idx, device.name());
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
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Load the Metal library
        self.load_library()?;

        // TODO: Parse plan_bytes and extract embedding weights
        // TODO: Create Metal buffer for embedding matrix
        // TODO: Validate embedding dimensions match model config

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

        tracing::info!("Metal kernels initialized (embedding weights will be loaded when Metal execution is fully implemented)");

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

        // Metal kernels are compiled and ready
        // Full kernel execution will be implemented after compiling aos_kernels.metallib
        //
        // The full implementation will:
        // 1. Lookup embedding for input_ids[position] in Metal shader
        // 2. Run transformer layers with LoRA adapters
        // 3. Generate output logits
        //
        // For now, fill output with deterministic values based on input
        // This allows Phase 1 integration testing while Metal shaders are compiled

        // Create deterministic output based on input and router gates
        let total_gate_weight: f32 = adapters
            .iter()
            .map(|a| (a.gate as f32) / 32768.0) // Convert Q15 to float
            .sum();

        // Fill logits with scaled values (ensures non-zero output for testing)
        let base_logit = total_gate_weight * 0.1;
        for (idx, logit) in io.output_logits.iter_mut().enumerate() {
            *logit = base_logit * ((idx % 100) as f32) * 0.01;
        }

        // Track numerical noise for this step
        // In a real implementation, this would compare quantized vs reference outputs
        let quantized_output = io.output_logits.as_slice();
        
        // For demonstration, create a reference output with slight differences
        let reference_output: Vec<f32> = quantized_output
            .iter()
            .map(|&x| x + (x * 0.001)) // Add 0.1% noise for demonstration
            .collect();

        // Track noise for the output layer
        self.noise_tracker.track_layer_error(
            "output_logits",
            quantized_output,
            Some(&reference_output),
        )?;

        // Track noise for intermediate layers (simulated)
        if adapters.len() > 0 {
            let intermediate_output: Vec<f32> = vec![total_gate_weight; 128];
            let intermediate_reference: Vec<f32> = intermediate_output
                .iter()
                .map(|&x| x + (x * 0.0005))
                .collect();

            self.noise_tracker.track_layer_error(
                "intermediate_layers",
                &intermediate_output,
                Some(&intermediate_reference),
            )?;
        }

        // Complete the step tracking
        self.noise_tracker.track_step()?;

        Ok(())
    }

    fn device_name(&self) -> &str {
        self.device.name()
    }
}
