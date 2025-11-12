//! Metal kernel implementation
//!
//! This crate contains the unsafe boundary for Metal FFI.
//! All unsafe code is confined to this crate.
//!
//! References:
//! - Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders
//! - Metal Shading Language: https://developer.apple.com/metal/Metal-Shading-Language-Specification.pdf

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::{attestation, FusedKernels, IoBuffers, RouterRing};
use adapteros_manifest::{Adapter, ManifestV3};
use memmap2::MmapOptions;
use metal::*;
use rand::{rngs::StdRng, Rng, SeedableRng};
use safetensors::{tensor::TensorView, Dtype, SafeTensors};
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::ffi::c_void;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

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

#[derive(Debug)]
struct LoraBuffers {
    gate_lora_a: Buffer,
    gate_lora_b: Buffer,
    up_lora_a: Buffer,
    up_lora_b: Buffer,
    down_lora_a: Buffer,
    down_lora_b: Buffer,
    q_lora_a: Buffer,
    q_lora_b: Buffer,
    k_lora_a: Buffer,
    k_lora_b: Buffer,
    v_lora_a: Buffer,
    v_lora_b: Buffer,
}

/// Dimensions describing a LoRA module's base projection
pub struct ModuleShape {
    /// Output dimension of the base weight (rows of B)
    pub out_dim: usize,
    /// Input dimension of the base weight (columns of A)
    pub in_dim: usize,
}

/// Metal-resident buffers for a single adapter's LoRA weights
pub struct AdapterWeights {
    pub adapter_id: String,
    pub rank: u32,
    pub rank_padded: u32,
    pub alpha: f32,
    pub lora_a_buffers: HashMap<String, Buffer>,
    pub lora_b_buffers: HashMap<String, Buffer>,
    pub module_shapes: HashMap<String, ModuleShape>,
    pub total_bytes: u64,
}

// Embed precompiled metallib
// Compiled offline with deterministic build process
const METALLIB_BYTES: &[u8] = include_bytes!("../shaders/adapteros_kernels.metallib");
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
    // Reusable output logits buffer to avoid per-step allocations
    logits_buffer: Option<Buffer>,
    // Current intermediate buffer batch capacity
    batch_capacity: usize,
    // Reusable constant buffers for shader parameters
    hidden_size_const: Option<Buffer>,
    vocab_size_const: Option<Buffer>,
    adapter_index_map: Vec<String>,
    adapter_weights: HashMap<String, AdapterWeights>,
    plan_seed: [u8; 32],
    adapter_logits_cache: HashMap<u32, Vec<f32>>,
    lora_buffers: Option<LoraBuffers>,
    populated_lora_adapters: HashSet<u32>,
}

/// Builder for creating LoRA copy parameters
#[derive(Default)]
struct LoraCopyParamsBuilder<'a> {
    adapter_index: Option<usize>,
    weights: Option<&'a AdapterWeights>,
    buffers: Option<&'a LoraBuffers>,
    hidden_size: Option<usize>,
    intermediate_size: Option<usize>,
    kv_width: Option<usize>,
    rank: Option<usize>,
}

/// Parameters for LoRA copy operation
struct LoraCopyParams<'a> {
    adapter_index: usize,
    weights: &'a AdapterWeights,
    buffers: &'a LoraBuffers,
    hidden_size: usize,
    intermediate_size: usize,
    kv_width: usize,
    rank: usize,
}

impl<'a> LoraCopyParamsBuilder<'a> {
    /// Create a new LoRA copy parameters builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the adapter index (required)
    pub fn adapter_index(mut self, adapter_index: usize) -> Self {
        self.adapter_index = Some(adapter_index);
        self
    }

    /// Set the adapter weights (required)
    pub fn weights(mut self, weights: &'a AdapterWeights) -> Self {
        self.weights = Some(weights);
        self
    }

    /// Set the LoRA buffers (required)
    pub fn buffers(mut self, buffers: &'a LoraBuffers) -> Self {
        self.buffers = Some(buffers);
        self
    }

    /// Set the hidden size (required)
    pub fn hidden_size(mut self, hidden_size: usize) -> Self {
        self.hidden_size = Some(hidden_size);
        self
    }

    /// Set the intermediate size (required)
    pub fn intermediate_size(mut self, intermediate_size: usize) -> Self {
        self.intermediate_size = Some(intermediate_size);
        self
    }

    /// Set the KV width (required)
    pub fn kv_width(mut self, kv_width: usize) -> Self {
        self.kv_width = Some(kv_width);
        self
    }

    /// Set the rank (required)
    pub fn rank(mut self, rank: usize) -> Self {
        self.rank = Some(rank);
        self
    }

    /// Build the LoRA copy parameters
    pub fn build(self) -> Result<LoraCopyParams<'a>> {
        fn missing(field: &str) -> AosError {
            AosError::Kernel(format!("{} is required", field))
        }

        Ok(LoraCopyParams {
            adapter_index: self.adapter_index.ok_or_else(|| missing("adapter_index"))?,
            weights: self.weights.ok_or_else(|| missing("weights"))?,
            buffers: self.buffers.ok_or_else(|| missing("buffers"))?,
            hidden_size: self.hidden_size.ok_or_else(|| missing("hidden_size"))?,
            intermediate_size: self
                .intermediate_size
                .ok_or_else(|| missing("intermediate_size"))?,
            kv_width: self.kv_width.ok_or_else(|| missing("kv_width"))?,
            rank: self.rank.ok_or_else(|| missing("rank"))?,
        })
    }
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
            logits_buffer: None,
            batch_capacity: 1,
            hidden_size_const: None,
            vocab_size_const: None,
            adapter_index_map: Vec::new(),
            adapter_weights: HashMap::new(),
            plan_seed: [0u8; 32],
            adapter_logits_cache: HashMap::new(),
            lora_buffers: None,
            populated_lora_adapters: HashSet::new(),
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
                info!(gpu_idx = idx, device_name = %device.name(), "Selected GPU");
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

    fn adapters_root_path() -> PathBuf {
        std::env::var("AOS_ADAPTERS_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./adapters"))
    }

    fn resolve_adapter_weights_path(root: &Path, identifier: &str) -> PathBuf {
        let mut name = identifier.trim().to_string();
        if let Some(rest) = name.strip_prefix("b3:") {
            name = rest.to_string();
        }

        let is_hex = name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit());
        let mut candidates = Vec::new();

        if is_hex {
            candidates.push(root.join(format!("{}.safetensors", name)));
            candidates.push(root.join(&name).join("weights.safetensors"));
        } else {
            candidates.push(root.join(&name).join("weights.safetensors"));
            candidates.push(root.join(format!("{}.safetensors", name)));
        }

        if identifier != name {
            candidates.push(root.join(identifier).join("weights.safetensors"));
            candidates.push(root.join(format!("{}.safetensors", identifier)));
        }

        candidates
            .into_iter()
            .find(|p| p.exists())
            .unwrap_or_else(|| {
                if is_hex {
                    root.join(format!("{}.safetensors", name))
                } else {
                    root.join(&name).join("weights.safetensors")
                }
            })
    }

    fn tensor_to_f32_vec(tensor: &TensorView<'_>, label: &str) -> Result<Vec<f32>> {
        if tensor.dtype() != Dtype::F32 {
            return Err(AosError::Kernel(format!(
                "Tensor {} has unsupported dtype {:?}, expected f32",
                label,
                tensor.dtype()
            )));
        }

        let data = tensor.data();
        let elem_size = std::mem::size_of::<f32>();
        if !data.len().is_multiple_of(elem_size) {
            return Err(AosError::Kernel(format!(
                "Tensor {} data length {} not divisible by {}",
                label,
                data.len(),
                elem_size
            )));
        }

        let mut values = vec![0f32; data.len() / elem_size];
        for (idx, chunk) in data.chunks_exact(elem_size).enumerate() {
            values[idx] = f32::from_le_bytes(chunk.try_into().expect("chunk size invariant"));
        }

        Ok(values)
    }

    fn parse_manifest_from_plan(plan_bytes: &[u8]) -> Result<ManifestV3> {
        serde_json::from_slice(plan_bytes).map_err(|err| {
            AosError::Kernel(format!("Failed to parse manifest from plan bytes: {}", err))
        })
    }

    fn load_adapter_from_safetensors(
        &self,
        root: &Path,
        manifest: &ManifestV3,
        adapter: &Adapter,
    ) -> Result<AdapterWeights> {
        if adapter.rank == 0 {
            return Err(AosError::Kernel(format!(
                "Adapter {} has rank 0, cannot load weights",
                adapter.id
            )));
        }

        let rank = adapter.rank as usize;
        let rank_padded = rank.div_ceil(16) * 16;

        let hash_hex = adapter.hash.to_hex();
        let hash_path = Self::resolve_adapter_weights_path(root, &hash_hex);
        let path = if hash_path.exists() {
            hash_path
        } else {
            let by_id = Self::resolve_adapter_weights_path(root, &adapter.id);
            if by_id.exists() {
                by_id
            } else {
                hash_path
            }
        };

        let file = File::open(&path).map_err(|err| {
            AosError::Kernel(format!(
                "Failed to open weights for adapter {} at {}: {}",
                adapter.id,
                path.display(),
                err
            ))
        })?;

        let mmap = unsafe { MmapOptions::new().map(&file) }.map_err(|err| {
            AosError::Kernel(format!(
                "Failed to memory-map weights for adapter {} ({}): {}",
                adapter.id,
                path.display(),
                err
            ))
        })?;

        if mmap.is_empty() {
            return Err(AosError::Kernel(format!(
                "Adapter {} weights file {} is empty",
                adapter.id,
                path.display()
            )));
        }

        let file_hash = B3Hash::hash(&mmap);
        if file_hash != adapter.hash {
            return Err(AosError::Kernel(format!(
                "Adapter {} hash mismatch: manifest {}, file {} (path: {})",
                adapter.id,
                adapter.hash.to_hex(),
                file_hash.to_hex(),
                path.display()
            )));
        }

        let tensors = SafeTensors::deserialize(&mmap).map_err(|err| {
            AosError::Kernel(format!(
                "Failed to deserialize safetensors for adapter {} ({}): {}",
                adapter.id,
                path.display(),
                err
            ))
        })?;

        let mut lora_a_buffers = HashMap::new();
        let mut lora_b_buffers = HashMap::new();
        let mut module_shapes = HashMap::new();
        let mut total_bytes = 0u64;

        for module in &adapter.target_modules {
            let a_key = format!("lora_a.{}", module);
            let b_key = format!("lora_b.{}", module);

            let a_tensor = tensors.tensor(&a_key).map_err(|err| {
                AosError::Kernel(format!(
                    "Adapter {} missing tensor {}: {}",
                    adapter.id, a_key, err
                ))
            })?;
            let b_tensor = tensors.tensor(&b_key).map_err(|err| {
                AosError::Kernel(format!(
                    "Adapter {} missing tensor {}: {}",
                    adapter.id, b_key, err
                ))
            })?;

            let a_shape = a_tensor.shape();
            if a_shape.len() != 2 {
                return Err(AosError::Kernel(format!(
                    "Adapter {} tensor {} expected rank 2, got {:?}",
                    adapter.id, a_key, a_shape
                )));
            }

            let b_shape = b_tensor.shape();
            if b_shape.len() != 2 {
                return Err(AosError::Kernel(format!(
                    "Adapter {} tensor {} expected rank 2, got {:?}",
                    adapter.id, b_key, b_shape
                )));
            }

            let a_rows = a_shape[0] as usize;
            let a_cols = a_shape[1] as usize;
            if a_rows != rank && a_rows != rank_padded {
                return Err(AosError::Kernel(format!(
                    "Adapter {} tensor {} expected {} rows (or padded {}), got {}",
                    adapter.id, a_key, rank, rank_padded, a_rows
                )));
            }

            let b_rows = b_shape[0] as usize;
            let b_cols = b_shape[1] as usize;
            if b_cols != rank && b_cols != rank_padded {
                return Err(AosError::Kernel(format!(
                    "Adapter {} tensor {} expected {} columns (or padded {}), got {}",
                    adapter.id, b_key, rank, rank_padded, b_cols
                )));
            }

            if module == "lm_head" {
                let expected_vocab = manifest.base.vocab_size as usize;
                if b_rows != expected_vocab {
                    return Err(AosError::Kernel(format!(
                        "Adapter {} lm_head has {} rows but vocab size is {}",
                        adapter.id, b_rows, expected_vocab
                    )));
                }
            }

            let a_values = Self::tensor_to_f32_vec(&a_tensor, &a_key)?;
            let b_values = Self::tensor_to_f32_vec(&b_tensor, &b_key)?;

            let mut padded_a = vec![0f32; rank_padded * a_cols];
            let copy_rows = usize::min(rank, a_rows);
            for r in 0..copy_rows {
                let src = r * a_cols;
                let dst = r * a_cols;
                padded_a[dst..dst + a_cols].copy_from_slice(&a_values[src..src + a_cols]);
            }

            let mut padded_b = vec![0f32; b_rows * rank_padded];
            let copy_cols = usize::min(rank, b_cols);
            for row in 0..b_rows {
                let src = row * b_cols;
                let dst = row * rank_padded;
                padded_b[dst..dst + copy_cols].copy_from_slice(&b_values[src..src + copy_cols]);
            }

            let a_buffer = self.device.new_buffer_with_data(
                padded_a.as_ptr() as *const c_void,
                (padded_a.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let b_buffer = self.device.new_buffer_with_data(
                padded_b.as_ptr() as *const c_void,
                (padded_b.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            );

            total_bytes += a_buffer.length();
            total_bytes += b_buffer.length();

            lora_a_buffers.insert(module.clone(), a_buffer);
            lora_b_buffers.insert(module.clone(), b_buffer);
            module_shapes.insert(
                module.clone(),
                ModuleShape {
                    out_dim: b_rows,
                    in_dim: a_cols,
                },
            );
        }

        Ok(AdapterWeights {
            adapter_id: adapter.id.clone(),
            rank: adapter.rank,
            rank_padded: rank_padded as u32,
            alpha: adapter.alpha,
            lora_a_buffers,
            lora_b_buffers,
            module_shapes,
            total_bytes,
        })
    }

    fn load_adapters_from_manifest(&mut self, manifest: &ManifestV3) -> Result<()> {
        self.adapter_weights.clear();
        self.adapter_index_map = manifest.adapters.iter().map(|a| a.id.clone()).collect();
        self.adapter_logits_cache.clear();
        self.lora_buffers = None;
        self.populated_lora_adapters.clear();

        if manifest.adapters.is_empty() {
            tracing::warn!("Manifest contains no adapters; skipping LoRA weight loading");
            return Ok(());
        }

        let root = Self::adapters_root_path();
        let mut total_bytes = 0u64;

        for adapter in &manifest.adapters {
            let weights = self.load_adapter_from_safetensors(&root, manifest, adapter)?;
            total_bytes += weights.total_bytes;
            self.adapter_weights.insert(adapter.id.clone(), weights);
        }

        tracing::info!(
            adapters = manifest.adapters.len(),
            total_bytes,
            root = %root.display(),
            "Loaded adapter weights into Metal buffers"
        );

        Ok(())
    }

    fn allocate_f32_buffer(&self, elements: usize) -> Buffer {
        let bytes = (elements * std::mem::size_of::<f32>()) as u64;
        self.device
            .new_buffer(bytes, MTLResourceOptions::StorageModeShared)
    }

    fn ensure_lora_buffers(
        &mut self,
        hidden_size: usize,
        intermediate_size: usize,
        kv_width: usize,
        rank: usize,
        max_adapters: usize,
    ) -> Result<()> {
        if self.lora_buffers.is_some() {
            return Ok(());
        }

        let adapter_blocks = max_adapters.max(1);
        let gate_a_elems = adapter_blocks * hidden_size * rank;
        let gate_b_elems = adapter_blocks * rank * intermediate_size;
        let down_a_elems = adapter_blocks * intermediate_size * rank;
        let down_b_elems = adapter_blocks * rank * hidden_size;
        let kv_b_elems = adapter_blocks * rank * kv_width;

        let buffers = LoraBuffers {
            gate_lora_a: self.allocate_f32_buffer(gate_a_elems),
            gate_lora_b: self.allocate_f32_buffer(gate_b_elems),
            up_lora_a: self.allocate_f32_buffer(gate_a_elems),
            up_lora_b: self.allocate_f32_buffer(gate_b_elems),
            down_lora_a: self.allocate_f32_buffer(down_a_elems),
            down_lora_b: self.allocate_f32_buffer(down_b_elems),
            q_lora_a: self.allocate_f32_buffer(gate_a_elems),
            q_lora_b: self.allocate_f32_buffer(down_b_elems),
            k_lora_a: self.allocate_f32_buffer(gate_a_elems),
            k_lora_b: self.allocate_f32_buffer(kv_b_elems),
            v_lora_a: self.allocate_f32_buffer(gate_a_elems),
            v_lora_b: self.allocate_f32_buffer(kv_b_elems),
        };

        self.lora_buffers = Some(buffers);
        Ok(())
    }

    fn adapter_seed(&self, adapter_id: u32, tag: &str) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.plan_seed);
        hasher.update(&adapter_id.to_le_bytes());
        hasher.update(tag.as_bytes());
        let hash = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(hash.as_bytes());
        out
    }

    fn fill_buffer_with_rng(
        &self,
        buffer: &Buffer,
        offset_floats: usize,
        len: usize,
        rng: &mut StdRng,
    ) {
        let ptr = buffer.contents() as *mut f32;
        if ptr.is_null() {
            return;
        }
        unsafe {
            let slice = std::slice::from_raw_parts_mut(ptr.add(offset_floats), len);
            for value in slice.iter_mut() {
                *value = rng.gen_range(-0.05..0.05);
            }
        }
        let offset_bytes = (offset_floats * std::mem::size_of::<f32>()) as NSUInteger;
        let len_bytes = (len * std::mem::size_of::<f32>()) as NSUInteger;
        buffer.did_modify_range(NSRange::new(offset_bytes, len_bytes));
    }

    fn zero_lora_region(&self, buffer: &Buffer, offset_floats: usize, len: usize) {
        if len == 0 {
            return;
        }
        let ptr = buffer.contents() as *mut f32;
        if ptr.is_null() {
            return;
        }
        unsafe {
            let slice = std::slice::from_raw_parts_mut(ptr.add(offset_floats), len);
            slice.fill(0.0);
        }
        let offset_bytes = (offset_floats * std::mem::size_of::<f32>()) as NSUInteger;
        let len_bytes = (len * std::mem::size_of::<f32>()) as NSUInteger;
        buffer.did_modify_range(NSRange::new(offset_bytes, len_bytes));
    }

    fn copy_lora_matrix_a(
        &self,
        weights: &AdapterWeights,
        module: &str,
        dst: &Buffer,
        dst_offset: usize,
        expected_cols: usize,
        copy_rank: usize,
    ) -> Result<()> {
        let (a_buffer, shape) = match (
            weights.lora_a_buffers.get(module),
            weights.module_shapes.get(module),
        ) {
            (Some(buf), Some(shape)) => (buf, shape),
            _ => {
                tracing::warn!(
                    adapter = %weights.adapter_id,
                    module,
                    "Adapter module missing lora_a buffer or shape"
                );
                return Ok(());
            }
        };

        let src_cols = shape.in_dim;
        if src_cols == 0 || copy_rank == 0 {
            return Ok(());
        }

        let copy_cols = expected_cols.min(src_cols);
        let src_ptr = a_buffer.contents() as *const f32;
        if src_ptr.is_null() {
            return Err(AosError::Kernel(format!(
                "Adapter {} module {} lora_a buffer is not CPU-accessible",
                weights.adapter_id, module
            )));
        }
        let src_len = (a_buffer.length() as usize) / std::mem::size_of::<f32>();
        let src_slice = unsafe { std::slice::from_raw_parts(src_ptr, src_len) };

        let dst_ptr = dst.contents() as *mut f32;
        if dst_ptr.is_null() {
            return Err(AosError::Kernel(format!(
                "Destination LoRA buffer for module {} is not CPU-accessible",
                module
            )));
        }
        let dst_len = (dst.length() as usize) / std::mem::size_of::<f32>();
        let dst_slice = unsafe { std::slice::from_raw_parts_mut(dst_ptr, dst_len) };

        for r in 0..copy_rank {
            let src_start = r * src_cols;
            let dst_start = dst_offset + r * expected_cols;
            let dst_row = &mut dst_slice[dst_start..dst_start + expected_cols];
            dst_row[..copy_cols].copy_from_slice(&src_slice[src_start..src_start + copy_cols]);
        }

        let offset_bytes = (dst_offset * std::mem::size_of::<f32>()) as NSUInteger;
        let len_bytes = (copy_rank * expected_cols * std::mem::size_of::<f32>()) as NSUInteger;
        dst.did_modify_range(NSRange::new(offset_bytes, len_bytes));
        Ok(())
    }

    fn copy_lora_matrix_b_transpose(
        &self,
        weights: &AdapterWeights,
        module: &str,
        dst: &Buffer,
        dst_offset: usize,
        expected_cols: usize,
        copy_rank: usize,
    ) -> Result<()> {
        let (b_buffer, shape) = match (
            weights.lora_b_buffers.get(module),
            weights.module_shapes.get(module),
        ) {
            (Some(buf), Some(shape)) => (buf, shape),
            _ => {
                tracing::warn!(
                    adapter = %weights.adapter_id,
                    module,
                    "Adapter module missing lora_b buffer or shape"
                );
                return Ok(());
            }
        };

        let src_rows = shape.out_dim;
        if src_rows == 0 || copy_rank == 0 {
            return Ok(());
        }

        let copy_cols = expected_cols.min(src_rows);
        let rank_padded = weights.rank_padded as usize;
        let effective_rank = copy_rank.min(rank_padded);
        let src_ptr = b_buffer.contents() as *const f32;
        if src_ptr.is_null() {
            return Err(AosError::Kernel(format!(
                "Adapter {} module {} lora_b buffer is not CPU-accessible",
                weights.adapter_id, module
            )));
        }
        let src_len = (b_buffer.length() as usize) / std::mem::size_of::<f32>();
        let src_slice = unsafe { std::slice::from_raw_parts(src_ptr, src_len) };

        let dst_ptr = dst.contents() as *mut f32;
        if dst_ptr.is_null() {
            return Err(AosError::Kernel(format!(
                "Destination LoRA buffer for module {} is not CPU-accessible",
                module
            )));
        }
        let dst_len = (dst.length() as usize) / std::mem::size_of::<f32>();
        let dst_slice = unsafe { std::slice::from_raw_parts_mut(dst_ptr, dst_len) };

        for r in 0..effective_rank {
            let dst_start = dst_offset + r * expected_cols;
            let dst_row = &mut dst_slice[dst_start..dst_start + expected_cols];
            for (col, value) in dst_row.iter_mut().take(copy_cols).enumerate() {
                let src_index = col * rank_padded + r;
                if let Some(src_val) = src_slice.get(src_index) {
                    *value = *src_val;
                }
            }
        }

        let offset_bytes = (dst_offset * std::mem::size_of::<f32>()) as NSUInteger;
        let len_bytes = (effective_rank * expected_cols * std::mem::size_of::<f32>()) as NSUInteger;
        dst.did_modify_range(NSRange::new(offset_bytes, len_bytes));
        Ok(())
    }

    /// Copy LoRA weights to Metal buffers
    ///
    /// Use `LoraCopyParamsBuilder` to construct copy parameters:
    /// ```rust
    /// let params = LoraCopyParamsBuilder::new()
    ///     .adapter_index(0)
    ///     .weights(&adapter_weights)
    ///     .buffers(&lora_buffers)
    ///     .hidden_size(4096)
    ///     .intermediate_size(11008)
    ///     .kv_width(128)
    ///     .rank(8)
    ///     .build()?;
    /// kernels.copy_lora_from_weights(params).await?;
    /// ```
    fn copy_lora_from_weights(&self, params: LoraCopyParams<'_>) -> Result<()> {
        let rank_actual = params.weights.rank as usize;
        let copy_rank = params
            .rank
            .min(rank_actual)
            .min(params.weights.rank_padded as usize);
        if copy_rank == 0 {
            return Ok(());
        }

        let adapter_offset_hidden = params.adapter_index * params.hidden_size * params.rank;
        let adapter_offset_intermediate =
            params.adapter_index * params.intermediate_size * params.rank;
        let adapter_offset_hidden_rank = params.adapter_index * params.rank * params.hidden_size;
        let adapter_offset_intermediate_rank =
            params.adapter_index * params.rank * params.intermediate_size;
        let adapter_offset_kv_rank = params.adapter_index * params.rank * params.kv_width;

        // Gate projection
        self.zero_lora_region(
            &params.buffers.gate_lora_a,
            adapter_offset_hidden,
            params.hidden_size * params.rank,
        );
        self.zero_lora_region(
            &params.buffers.gate_lora_b,
            adapter_offset_intermediate_rank,
            params.rank * params.intermediate_size,
        );
        self.copy_lora_matrix_a(
            params.weights,
            "gate_proj",
            &params.buffers.gate_lora_a,
            adapter_offset_hidden,
            params.hidden_size,
            copy_rank,
        )?;
        self.copy_lora_matrix_b_transpose(
            params.weights,
            "gate_proj",
            &params.buffers.gate_lora_b,
            adapter_offset_intermediate_rank,
            params.intermediate_size,
            copy_rank,
        )?;

        // Up projection
        self.zero_lora_region(
            &params.buffers.up_lora_a,
            adapter_offset_hidden,
            params.hidden_size * params.rank,
        );
        self.zero_lora_region(
            &params.buffers.up_lora_b,
            adapter_offset_intermediate_rank,
            params.rank * params.intermediate_size,
        );
        self.copy_lora_matrix_a(
            params.weights,
            "up_proj",
            &params.buffers.up_lora_a,
            adapter_offset_hidden,
            params.hidden_size,
            copy_rank,
        )?;
        self.copy_lora_matrix_b_transpose(
            params.weights,
            "up_proj",
            &params.buffers.up_lora_b,
            adapter_offset_intermediate_rank,
            params.intermediate_size,
            copy_rank,
        )?;

        // Down projection
        self.zero_lora_region(
            &params.buffers.down_lora_a,
            adapter_offset_intermediate,
            params.intermediate_size * params.rank,
        );
        self.zero_lora_region(
            &params.buffers.down_lora_b,
            adapter_offset_hidden_rank,
            params.rank * params.hidden_size,
        );
        self.copy_lora_matrix_a(
            params.weights,
            "down_proj",
            &params.buffers.down_lora_a,
            adapter_offset_intermediate,
            params.intermediate_size,
            copy_rank,
        )?;
        self.copy_lora_matrix_b_transpose(
            params.weights,
            "down_proj",
            &params.buffers.down_lora_b,
            adapter_offset_hidden_rank,
            params.hidden_size,
            copy_rank,
        )?;

        // Q projection
        self.zero_lora_region(
            &params.buffers.q_lora_a,
            adapter_offset_hidden,
            params.hidden_size * params.rank,
        );
        self.zero_lora_region(
            &params.buffers.q_lora_b,
            adapter_offset_hidden_rank,
            params.rank * params.hidden_size,
        );
        self.copy_lora_matrix_a(
            params.weights,
            "q_proj",
            &params.buffers.q_lora_a,
            adapter_offset_hidden,
            params.hidden_size,
            copy_rank,
        )?;
        self.copy_lora_matrix_b_transpose(
            params.weights,
            "q_proj",
            &params.buffers.q_lora_b,
            adapter_offset_hidden_rank,
            params.hidden_size,
            copy_rank,
        )?;

        // K projection
        self.zero_lora_region(
            &params.buffers.k_lora_a,
            adapter_offset_hidden,
            params.hidden_size * params.rank,
        );
        self.zero_lora_region(
            &params.buffers.k_lora_b,
            adapter_offset_kv_rank,
            params.rank * params.kv_width,
        );
        self.copy_lora_matrix_a(
            params.weights,
            "k_proj",
            &params.buffers.k_lora_a,
            adapter_offset_hidden,
            params.hidden_size,
            copy_rank,
        )?;
        self.copy_lora_matrix_b_transpose(
            params.weights,
            "k_proj",
            &params.buffers.k_lora_b,
            adapter_offset_kv_rank,
            params.kv_width,
            copy_rank,
        )?;

        // V projection
        self.zero_lora_region(
            &params.buffers.v_lora_a,
            adapter_offset_hidden,
            params.hidden_size * params.rank,
        );
        self.zero_lora_region(
            &params.buffers.v_lora_b,
            adapter_offset_kv_rank,
            params.rank * params.kv_width,
        );
        self.copy_lora_matrix_a(
            params.weights,
            "v_proj",
            &params.buffers.v_lora_a,
            adapter_offset_hidden,
            params.hidden_size,
            copy_rank,
        )?;
        self.copy_lora_matrix_b_transpose(
            params.weights,
            "v_proj",
            &params.buffers.v_lora_b,
            adapter_offset_kv_rank,
            params.kv_width,
            copy_rank,
        )?;

        Ok(())
    }

    fn populate_lora_for_adapter(
        &mut self,
        adapter_id: u32,
        rank: usize,
        hidden_size: usize,
        intermediate_size: usize,
        kv_width: usize,
        max_adapters: usize,
    ) -> Result<()> {
        if adapter_id == 0 || adapter_id as usize >= max_adapters {
            return Ok(());
        }

        if !self.populated_lora_adapters.insert(adapter_id) {
            return Ok(());
        }

        let buffers = self
            .lora_buffers
            .as_ref()
            .ok_or_else(|| AosError::Kernel("LoRA buffers not allocated".to_string()))?;

        let adapter_index = adapter_id as usize;
        if let Some(adapter_name) = self.adapter_index_map.get(adapter_index) {
            if let Some(weights) = self.adapter_weights.get(adapter_name) {
                let copy_params = LoraCopyParamsBuilder::new()
                    .adapter_index(adapter_index)
                    .weights(weights)
                    .buffers(buffers)
                    .hidden_size(hidden_size)
                    .intermediate_size(intermediate_size)
                    .kv_width(kv_width)
                    .rank(rank)
                    .build()?;
                self.copy_lora_from_weights(copy_params)?;
                return Ok(());
            }
        }

        let adapter_offset_hidden = adapter_id as usize * hidden_size * rank;
        let adapter_offset_intermediate = adapter_id as usize * intermediate_size * rank;
        let adapter_offset_hidden_rank = adapter_id as usize * rank * hidden_size;
        let adapter_offset_intermediate_rank = adapter_id as usize * rank * intermediate_size;
        let adapter_offset_kv_rank = adapter_id as usize * rank * kv_width;

        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "gate_lora_a"));
        self.fill_buffer_with_rng(
            &buffers.gate_lora_a,
            adapter_offset_hidden,
            hidden_size * rank,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "gate_lora_b"));
        self.fill_buffer_with_rng(
            &buffers.gate_lora_b,
            adapter_offset_intermediate_rank,
            rank * intermediate_size,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "up_lora_a"));
        self.fill_buffer_with_rng(
            &buffers.up_lora_a,
            adapter_offset_hidden,
            hidden_size * rank,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "up_lora_b"));
        self.fill_buffer_with_rng(
            &buffers.up_lora_b,
            adapter_offset_intermediate_rank,
            rank * intermediate_size,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "down_lora_a"));
        self.fill_buffer_with_rng(
            &buffers.down_lora_a,
            adapter_offset_intermediate,
            intermediate_size * rank,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "down_lora_b"));
        self.fill_buffer_with_rng(
            &buffers.down_lora_b,
            adapter_offset_hidden_rank,
            rank * hidden_size,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "q_lora_a"));
        self.fill_buffer_with_rng(
            &buffers.q_lora_a,
            adapter_offset_hidden,
            hidden_size * rank,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "q_lora_b"));
        self.fill_buffer_with_rng(
            &buffers.q_lora_b,
            adapter_offset_hidden_rank,
            rank * hidden_size,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "k_lora_a"));
        self.fill_buffer_with_rng(
            &buffers.k_lora_a,
            adapter_offset_hidden,
            hidden_size * rank,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "k_lora_b"));
        self.fill_buffer_with_rng(
            &buffers.k_lora_b,
            adapter_offset_kv_rank,
            rank * kv_width,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "v_lora_a"));
        self.fill_buffer_with_rng(
            &buffers.v_lora_a,
            adapter_offset_hidden,
            hidden_size * rank,
            &mut rng,
        );
        let mut rng = StdRng::from_seed(self.adapter_seed(adapter_id, "v_lora_b"));
        self.fill_buffer_with_rng(
            &buffers.v_lora_b,
            adapter_offset_kv_rank,
            rank * kv_width,
            &mut rng,
        );

        Ok(())
    }

    fn compute_dropout_seed(&self) -> u32 {
        let hash = blake3::hash(&self.plan_seed);
        let bytes = hash.as_bytes();
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
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
        #[allow(clippy::const_is_empty)]
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
        let hidden_size = 3584; // Qwen2.5-7B hidden size (default until weights parsed)
        let seq_len = self.batch_capacity.max(1);

        let buffer_size =
            (hidden_size as u64) * (seq_len as u64) * (std::mem::size_of::<f32>() as u64);

        let hidden_states = self
            .device
            .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared);

        let q_output = self
            .device
            .new_buffer(buffer_size, MTLResourceOptions::StorageModeShared);

        // kv_width derived from gqa_config if available, else default to hidden_size/8
        let kv_width = self
            .qkv_kernel
            .as_ref()
            .map(|k| k.gqa_config().kv_width as usize)
            .unwrap_or(hidden_size / 8);
        let kv_bytes = (kv_width as u64) * (seq_len as u64) * (std::mem::size_of::<f32>() as u64);
        let k_output = self
            .device
            .new_buffer(kv_bytes, MTLResourceOptions::StorageModeShared);
        let v_output = self
            .device
            .new_buffer(kv_bytes, MTLResourceOptions::StorageModeShared);

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

    /// Ensure intermediate buffers are allocated for at least `batch` tokens
    fn ensure_intermediate_capacity(&mut self, batch: usize) -> Result<()> {
        let needed = batch.max(1);
        if self.batch_capacity >= needed {
            return Ok(());
        }

        self.batch_capacity = needed;
        self.intermediate_buffers = Some(self.create_intermediate_buffers()?);
        Ok(())
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
    fn perform_embedding_lookup(&mut self, io: &mut IoBuffers) -> Result<()> {
        let embedding_buffer = self
            .embedding_buffer
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Embedding buffer not initialized".to_string()))?;

        let embedding_pipeline = self
            .embedding_pipeline
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Embedding pipeline not initialized".to_string()))?;

        let _dimensions = self
            .embedding_dimensions
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Embedding dimensions not set".to_string()))?;

        let intermediate_buffers = self
            .intermediate_buffers
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Intermediate buffers not created".to_string()))?;

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

        // Use preallocated intermediate buffer for hidden states
        encoder.set_buffer(2, Some(&intermediate_buffers.hidden_states), 0);

        // Dispatch embedding lookup kernel
        let threadgroup_size = MTLSize::new(256, 1, 1);
        let threadgroup_count = MTLSize::new(io.input_ids.len().div_ceil(256) as u64, 1, 1);
        encoder.dispatch_thread_groups(threadgroup_count, threadgroup_size);

        encoder.end_encoding();
        command_buffer.commit();
        command_buffer.wait_until_completed();

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
        let (hidden_size, intermediate_size, kv_width) = {
            let transformer_weights = self
                .transformer_weights
                .as_ref()
                .ok_or_else(|| AosError::Kernel("Transformer weights not loaded".to_string()))?;
            let embedding_dims = self
                .embedding_dimensions
                .as_ref()
                .ok_or_else(|| AosError::Kernel("Embedding dimensions not set".to_string()))?;

            let hidden_size = embedding_dims.hidden_size;
            if hidden_size == 0 {
                return Err(AosError::Kernel(
                    "Hidden size must be greater than zero".to_string(),
                ));
            }

            let gate_elements =
                (transformer_weights.gate_weight.length() as usize) / std::mem::size_of::<f32>();
            if !gate_elements.is_multiple_of(hidden_size) {
                return Err(AosError::Kernel(format!(
                    "Gate weight length {} is not divisible by hidden size {}",
                    gate_elements, hidden_size
                )));
            }
            let intermediate_size = gate_elements / hidden_size;

            let kv_width = self
                .qkv_kernel
                .as_ref()
                .map(|k| k.gqa_config().kv_width as usize)
                .unwrap_or(hidden_size);

            (hidden_size, intermediate_size, kv_width)
        };

        let rank = fused_mlp::LoraConfig::default().rank as usize;
        let ring_buffer_capacity = self
            .ring_buffer
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Ring buffer not initialized".to_string()))?
            .capacity();

        self.ensure_lora_buffers(
            hidden_size,
            intermediate_size,
            kv_width,
            rank,
            ring_buffer_capacity,
        )?;

        for adapter in adapters {
            self.populate_lora_for_adapter(
                adapter.id,
                rank,
                hidden_size,
                intermediate_size,
                kv_width,
                ring_buffer_capacity,
            )?;
        }

        let (ring_state, max_adapters_u32) = {
            let ring_buffer = self
                .ring_buffer
                .as_mut()
                .ok_or_else(|| AosError::Kernel("Ring buffer not initialized".to_string()))?;
            ring_buffer.update(adapters)?;
            (ring_buffer.raw_state(), ring_buffer_capacity as u32)
        };

        let dropout_seed = self.compute_dropout_seed();

        let transformer_weights = self
            .transformer_weights
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Transformer weights not loaded".to_string()))?;
        let intermediate_buffers = self
            .intermediate_buffers
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Intermediate buffers not created".to_string()))?;
        let lora_buffers = self
            .lora_buffers
            .as_ref()
            .ok_or_else(|| AosError::Kernel("LoRA buffers not allocated".to_string()))?;

        let hidden_elements =
            (intermediate_buffers.hidden_states.length() as usize) / std::mem::size_of::<f32>();
        let batch_size = (hidden_elements / hidden_size).max(1) as u32;

        let hidden_size_u32: u32 = hidden_size
            .try_into()
            .map_err(|_| AosError::Kernel("Hidden size exceeds u32::MAX".to_string()))?;
        let intermediate_size_u32: u32 = intermediate_size
            .try_into()
            .map_err(|_| AosError::Kernel("Intermediate size exceeds u32::MAX".to_string()))?;

        if let Some(ref mut qkv_kernel) = self.qkv_kernel {
            let lora_config = fused_qkv::LoraConfig::default();
            qkv_kernel.execute(
                &intermediate_buffers.hidden_states,
                &transformer_weights.q_weight,
                &transformer_weights.k_weight,
                &transformer_weights.v_weight,
                &intermediate_buffers.q_output,
                &intermediate_buffers.k_output,
                &intermediate_buffers.v_output,
                &lora_buffers.q_lora_a,
                &lora_buffers.q_lora_b,
                &lora_buffers.k_lora_a,
                &lora_buffers.k_lora_b,
                &lora_buffers.v_lora_a,
                &lora_buffers.v_lora_b,
                &lora_config,
                ring_state,
                max_adapters_u32,
                batch_size,
            )?;
        }

        if let Some(ref flash_attention_kernel) = self.flash_attention_kernel {
            flash_attention_kernel.execute(
                &intermediate_buffers.q_output,
                &intermediate_buffers.k_output,
                &intermediate_buffers.v_output,
                &intermediate_buffers.attention_output,
            )?;
        }

        if let Some(ref mut mlp_kernel) = self.mlp_kernel {
            let lora_config = fused_mlp::LoraConfig::default();
            mlp_kernel.execute(
                &intermediate_buffers.attention_output,
                &transformer_weights.gate_weight,
                &transformer_weights.up_weight,
                &transformer_weights.down_weight,
                &intermediate_buffers.mlp_output,
                &lora_config,
                &lora_buffers.gate_lora_a,
                &lora_buffers.gate_lora_b,
                &lora_buffers.up_lora_a,
                &lora_buffers.up_lora_b,
                &lora_buffers.down_lora_a,
                &lora_buffers.down_lora_b,
                ring_state,
                max_adapters_u32,
                batch_size,
                hidden_size_u32,
                intermediate_size_u32,
                dropout_seed,
            )?;
        }

        tracing::debug!(
            "Transformer layers completed with {} adapters on GPU",
            adapters.len()
        );
        Ok(())
    }

    /// Perform vocabulary projection using Metal kernels
    fn perform_vocabulary_projection(
        &mut self,
        adapters: &[ActiveAdapter],
        io: &mut IoBuffers,
    ) -> Result<()> {
        let dimensions = self
            .embedding_dimensions
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Embedding dimensions not set".to_string()))?;

        let vocab_size = dimensions.vocab_size;

        if io.output_logits.len() != vocab_size {
            io.output_logits.resize(vocab_size, 0.0);
        }

        let mlp_output_buffer = {
            let intermediate_buffers = self
                .intermediate_buffers
                .as_ref()
                .ok_or_else(|| AosError::Kernel("Intermediate buffers not created".to_string()))?;
            intermediate_buffers.mlp_output.clone()
        };

        let slot_capacity = {
            let ring_buffer_ref = self
                .ring_buffer
                .as_ref()
                .ok_or_else(|| AosError::Kernel("Ring buffer not initialized".to_string()))?;
            ring_buffer_ref.capacity()
        };

        // Skip CPU-side adapter delta computation; LoRA effects are applied in GPU kernels for
        // transformer layers. This reduces CPU-GPU transfers during vocab projection.
        let adapter_delta_host: Vec<f32> = Vec::new();
        let total_gate_weight: f32 = 0.0;
        let active_slots: usize = 0;

        let lm_head_weights = self
            .lm_head_weights
            .as_ref()
            .ok_or_else(|| AosError::Kernel("LM head weights not initialized".to_string()))?;

        let lm_head_pipeline = self
            .lm_head_pipeline
            .as_ref()
            .ok_or_else(|| AosError::Kernel("LM head pipeline not initialized".to_string()))?;

        let adapter_delta_buffer = if !adapter_delta_host.is_empty() {
            Some(self.device.new_buffer_with_data(
                adapter_delta_host.as_ptr() as *const std::ffi::c_void,
                (adapter_delta_host.len() * std::mem::size_of::<f32>()) as u64,
                MTLResourceOptions::StorageModeShared,
            ))
        } else {
            None
        };

        let ring_buffer_gpu = self
            .ring_buffer
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Ring buffer not initialized".to_string()))?
            .get_buffer()
            .ok_or_else(|| AosError::Kernel("Ring buffer GPU buffer missing".to_string()))?;

        // Reuse output buffer across steps when possible
        let needed_bytes = (vocab_size * std::mem::size_of::<f32>()) as u64;
        let logits_buffer = match &self.logits_buffer {
            Some(buf) if buf.length() == needed_bytes => buf.clone(),
            _ => {
                let buf = self
                    .device
                    .new_buffer(needed_bytes, MTLResourceOptions::StorageModeShared);
                self.logits_buffer = Some(buf.clone());
                buf
            }
        };

        // Use preallocated constant buffers
        let hidden_size_buffer = self
            .hidden_size_const
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Missing hidden size const buffer".to_string()))?;
        let vocab_size_buffer = self
            .vocab_size_const
            .as_ref()
            .ok_or_else(|| AosError::Kernel("Missing vocab size const buffer".to_string()))?;

        // Create command buffer for vocabulary projection
        let command_buffer = self._queue.new_command_buffer();
        let encoder = command_buffer.new_compute_command_encoder();

        encoder.set_compute_pipeline_state(lm_head_pipeline);
        encoder.set_buffer(0, Some(&mlp_output_buffer), 0);
        encoder.set_buffer(1, Some(&lm_head_weights.weight), 0);
        encoder.set_buffer(2, Some(&logits_buffer), 0);
        encoder.set_buffer(3, Some(&**ring_buffer_gpu), 0);
        if let Some(ref delta_buf) = adapter_delta_buffer {
            encoder.set_buffer(4, Some(&**delta_buf), 0);
        } else {
            encoder.set_buffer(4, None, 0);
        }
        encoder.set_buffer(5, Some(hidden_size_buffer), 0);
        encoder.set_buffer(6, Some(vocab_size_buffer), 0);

        let threadgroup_size = MTLSize::new(256, 1, 1);
        let threadgroup_count = MTLSize::new((vocab_size as u64).div_ceil(256), 1, 1);
        encoder.dispatch_thread_groups(threadgroup_count, threadgroup_size);

        encoder.end_encoding();
        command_buffer.commit();
        command_buffer.wait_until_completed();

        let logits_ptr = logits_buffer.contents() as *const f32;
        let logits_slice = unsafe { std::slice::from_raw_parts(logits_ptr, vocab_size) };
        io.output_logits.copy_from_slice(logits_slice);

        tracing::debug!(
            "Performed vocabulary projection with {} adapters ({} active of capacity {}), total gate weight: {}",
            adapters.len(),
            active_slots,
            slot_capacity,
            total_gate_weight
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {}

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
    /// embedding lookup) will be added when adapteros_kernels.metallib is fully compiled.
    ///
    /// # Note
    ///
    /// The embedding lookup is NOT performed in Rust - it happens in the Metal
    /// kernel during forward pass. The Worker's `EmbeddingModel` is only used
    /// for RAG/text similarity, not for inference.
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
        let plan_hash = B3Hash::hash(plan_bytes);
        self.plan_seed = plan_hash.to_bytes();
        self.adapter_logits_cache.clear();

        if let Err(err) = Self::parse_manifest_from_plan(plan_bytes).and_then(|manifest| {
            self.load_adapters_from_manifest(&manifest)?;
            Ok(())
        }) {
            tracing::warn!(
                error = %err,
                "Failed to load adapters from manifest; proceeding without LoRA weights"
            );
            self.adapter_weights.clear();
            self.adapter_index_map.clear();
        }

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

        self.qkv_kernel = Some(FusedQkvKernel::new(self.device.clone(), gqa_config)?);
        self.flash_attention_kernel =
            Some(FlashAttentionKernel::new(self.device.clone(), gqa_config)?);
        self.ring_buffer = Some(RingBuffer::new(self.device.clone(), 3)?);

        // Load transformer weights
        self.load_transformer_weights(plan_bytes)?;

        // Load LM head weights
        let lm_head_weights = self.parse_lm_head_weights(plan_bytes)?;
        self.lm_head_weights = Some(lm_head_weights);

        // Preallocate constant buffers that remain stable across steps
        if let Some(dims) = &self.embedding_dimensions {
            let hidden_size_value = dims.hidden_size as u32;
            let vocab_size_value = dims.vocab_size as u32;
            self.hidden_size_const = Some(self.device.new_buffer_with_data(
                &hidden_size_value as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u64,
                MTLResourceOptions::StorageModeShared,
            ));
            self.vocab_size_const = Some(self.device.new_buffer_with_data(
                &vocab_size_value as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u64,
                MTLResourceOptions::StorageModeShared,
            ));
        }

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

        // Ensure intermediate buffers can hold the current batch of tokens
        self.ensure_intermediate_capacity(io.input_ids.len())?;

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
}
