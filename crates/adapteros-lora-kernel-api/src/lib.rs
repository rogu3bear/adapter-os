//! Safe API for fused kernels

use adapteros_core::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub mod attestation;
pub mod liquid;

pub use liquid::{
    blend_and_forward_reference, LiquidAdapterRef, LiquidBlendRequest, LiquidBlendStats,
    LiquidKernel, LiquidPrecision, LiquidSlice, LiquidTensor, LIQUID_MAX_ADAPTERS,
};

/// Backend health status for monitoring and failover
#[derive(Debug, Clone)]
pub enum BackendHealth {
    /// Backend is operating normally
    Healthy,
    /// Backend is degraded but operational
    Degraded { reason: String },
    /// Backend has failed
    Failed { reason: String, recoverable: bool },
}

/// Backend performance metrics
#[derive(Debug, Clone, Default)]
pub struct BackendMetrics {
    /// Total operations executed
    pub total_operations: u64,
    /// Successful operations
    pub successful_operations: u64,
    /// Failed operations
    pub failed_operations: u64,
    /// Average latency
    pub avg_latency: Duration,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
}

/// Type alias for buffer verification result to reduce type complexity
pub type BufferVerificationResult = (u64, Vec<u8>, Vec<u8>, Vec<u8>);

/// Canonical ring buffer for router decisions (K≤8, Q15 gates)
///
/// **CRITICAL INVARIANTS** (enforced at construction):
/// - `indices.len() == gates_q15.len()` (matching lengths)
/// - `indices[i] < total_registered_adapters` (valid adapter IDs)
/// - `K ≤ 8` (enforced by fixed-size arrays)
///
/// **Violation policy:**
/// - Debug builds: `panic!` on invariant violation
/// - Release builds: `error!` log + zero-fill offending entries
///
/// [source: crates/adapteros-lora-kernel-api/src/lib.rs L22-68]
/// [source: docs/ARCHITECTURE.md#inference-flow]
#[derive(Debug, Clone)]
pub struct RouterRing {
    /// Adapter indices (fixed K=8, unused entries zero-filled)
    pub indices: [u16; 8],
    /// Q15 quantized gates (signed i16, range: -32767 to +32767)
    pub gates_q15: [i16; 8],
    /// Token position in sequence
    pub position: usize,
    /// Number of active entries (K ≤ 8)
    pub k: usize,
}

impl RouterRing {
    /// Create new RouterRing with K active entries (K ≤ 8)
    ///
    /// # Panics
    /// Panics in debug builds if `k > 8`
    pub fn new(k: usize) -> Self {
        #[cfg(debug_assertions)]
        {
            if k > 8 {
                panic!("RouterRing: K > 8 (got {})", k);
            }
        }

        #[cfg(not(debug_assertions))]
        {
            if k > 8 {
                tracing::error!(k = %k, "RouterRing: K exceeds max (8), clamping");
            }
        }

        let clamped_k = k.min(8);
        Self {
            indices: [0; 8],
            gates_q15: [0; 8],
            position: 0,
            k: clamped_k,
        }
    }

    /// Set indices and gates with invariant checking
    ///
    /// # Panics
    /// Debug builds panic if:
    /// - `indices.len() != gates.len()`
    /// - `indices.len() > 8`
    ///
    /// Release builds clamp and log errors
    pub fn set(&mut self, indices: &[u16], gates: &[i16]) {
        self.set_with_max_adapter(indices, gates, u16::MAX)
    }

    /// Set with explicit adapter count for bounds checking
    ///
    /// # Arguments
    /// * `indices` - Adapter indices (K ≤ 8)
    /// * `gates` - Q15 gates (must match indices length)
    /// * `max_adapter` - Maximum valid adapter index (exclusive)
    pub fn set_with_max_adapter(&mut self, indices: &[u16], gates: &[i16], max_adapter: u16) {
        // Invariant 1: matching lengths
        #[cfg(debug_assertions)]
        {
            if indices.len() != gates.len() {
                panic!(
                    "RouterRing: mismatched lengths (indices={}, gates={})",
                    indices.len(),
                    gates.len()
                );
            }
            if indices.len() > 8 {
                panic!("RouterRing: K > 8 (got {})", indices.len());
            }
        }

        #[cfg(not(debug_assertions))]
        {
            if indices.len() != gates.len() {
                tracing::error!(
                    indices_len = %indices.len(),
                    gates_len = %gates.len(),
                    "RouterRing: length mismatch, zero-filling"
                );
                self.indices = [0; 8];
                self.gates_q15 = [0; 8];
                self.k = 0;
                return;
            }
        }

        let k = indices.len().min(8);

        // Invariant 2: valid adapter indices
        #[cfg(debug_assertions)]
        {
            for (i, &idx) in indices.iter().enumerate() {
                if idx >= max_adapter {
                    panic!(
                        "RouterRing: invalid adapter index {} at position {} (max={})",
                        idx, i, max_adapter
                    );
                }
            }
        }

        #[cfg(not(debug_assertions))]
        {
            for (i, &idx) in indices.iter().enumerate() {
                if idx >= max_adapter {
                    tracing::error!(
                        index = %idx,
                        position = %i,
                        max = %max_adapter,
                        "RouterRing: out-of-bounds index, zero-filling"
                    );
                    self.indices = [0; 8];
                    self.gates_q15 = [0; 8];
                    self.k = 0;
                    return;
                }
            }
        }

        // Copy data
        self.indices[..k].copy_from_slice(&indices[..k]);
        self.gates_q15[..k].copy_from_slice(&gates[..k]);
        // Zero-fill unused entries
        self.indices[k..].fill(0);
        self.gates_q15[k..].fill(0);
        self.k = k;
    }

    /// Get active slice of indices (length = K)
    pub fn active_indices(&self) -> &[u16] {
        &self.indices[..self.k]
    }

    /// Get active slice of gates (length = K)
    pub fn active_gates(&self) -> &[i16] {
        &self.gates_q15[..self.k]
    }

    /// Get number of active adapters
    pub fn len(&self) -> usize {
        self.k
    }

    /// Check if ring is empty (no active adapters)
    pub fn is_empty(&self) -> bool {
        self.k == 0
    }
}

/// IO buffers for kernel execution
///
/// [source: crates/adapteros-lora-kernel-api/src/lib.rs L178-182]
/// [source: crates/adapteros-lora-worker/src/inference_pipeline.rs L45-67]
pub struct IoBuffers {
    pub input_ids: Vec<u32>,
    pub output_logits: Vec<f32>,
    pub position: usize,
}

impl IoBuffers {
    pub fn new(vocab_size: usize) -> Self {
        Self {
            input_ids: Vec::new(),
            output_logits: vec![0.0; vocab_size],
            position: 0,
        }
    }
}

/// Trait for fused kernel implementations
///
/// This trait defines the interface that all ML inference backends must implement
/// to provide deterministic, fused kernel execution for LoRA routing.
///
/// ## Implementation Requirements
///
/// Implementations must be:
/// - **Thread-safe**: `Send + Sync` for concurrent access
/// - **Deterministic**: Same inputs produce identical outputs
/// - **Resource-aware**: Proper memory management and cleanup
///
/// ## Error Handling
///
/// All methods return `Result<()>` and should provide detailed error context:
/// - `AosError::Kernel`: Backend-specific kernel errors
/// - `AosError::Io`: I/O and buffer management errors
/// - `AosError::ResourceExhaustion`: Memory or resource limits
///
/// ## Usage Example
/// ```rust,ignore
/// use adapteros_lora_kernel_api::{FusedKernels, RouterRing, IoBuffers};
/// use adapteros_core::Result;
///
/// async fn example(backend: &mut impl FusedKernels) -> Result<()> {
///     // Load model plan and weights
///     let plan_bytes = load_plan_from_file("model.aos")?;
///     backend.load(&plan_bytes)?;
///
///     // Prepare router decision
///     let mut ring = RouterRing::new(4);
///     ring.set(&[0, 2, 5, 7], &[1000, 2000, 1500, 800])?;
///
///     // Prepare IO buffers
///     let mut io = IoBuffers::new(1); // batch size 1
///     io.input_ids = vec![15043, 995, 1234]; // "Hello world" tokens
///
///     // Run inference step
///     backend.run_step(&ring, &mut io)?;
///
///     // Results available in io.output_logits
///     assert!(!io.output_logits.is_empty());
///     Ok(())
/// }
/// ```
///
/// [source: crates/adapteros-lora-kernel-api/src/lib.rs L198-244]
/// [source: crates/adapteros-lora-worker/src/backend_factory.rs L30-45]
/// [source: docs/ARCHITECTURE.md#architecture-components]
pub trait FusedKernels: Send + Sync {
    /// Expose LiquidKernel support when available (default: None)
    fn as_liquid_kernel(&self) -> Option<&dyn liquid::LiquidKernel> {
        None
    }

    /// Mutable access to LiquidKernel implementation when available (default: None)
    fn as_liquid_kernel_mut(&mut self) -> Option<&mut dyn liquid::LiquidKernel> {
        None
    }

    /// Whether this backend supports liquid blending of LoRA adapters
    fn supports_liquid_blending(&self) -> bool {
        self.as_liquid_kernel()
            .map(|k| k.supports_liquid_blending())
            .unwrap_or(false)
    }

    /// Maximum adapters supported by liquid blending (0 when unsupported)
    fn liquid_max_adapters(&self) -> usize {
        self.as_liquid_kernel()
            .map(|k| k.max_liquid_adapters())
            .unwrap_or(0)
    }

    /// Load model plan and adapter weights
    ///
    /// This method initializes the backend with a compiled model plan and
    /// any associated LoRA adapter weights. The plan format is backend-specific
    /// but typically contains compiled computation graphs and weight matrices.
    ///
    /// ## Parameters
    /// * `plan_bytes`: Compiled model plan in backend-specific format
    ///
    /// ## Errors
    /// * `AosError::Kernel`: Invalid plan format or corrupted data
    /// * `AosError::Io`: Failed to read plan data
    /// * `AosError::ResourceExhaustion`: Insufficient memory for model loading
    ///
    /// ## Performance
    /// Model loading is typically expensive (seconds) and should be done once
    /// per model lifetime, not per inference request.
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()>;

    /// Execute a single token generation step with LoRA routing
    ///
    /// Runs one step of autoregressive text generation using the provided
    /// router decision to select and combine LoRA adapters. Input tokens
    /// are processed through the base model with adapter modifications,
    /// producing output logits for the next token prediction.
    ///
    /// ## Parameters
    /// * `ring`: Router decision specifying which adapters to use and their weights
    /// * `io`: Input/output buffers containing tokens and logits
    ///
    /// ## Errors
    /// * `AosError::Kernel`: GPU/kernel execution failed
    /// * `AosError::InvalidInput`: Malformed input data or router decision
    /// * `AosError::ResourceExhaustion`: Insufficient compute resources
    ///
    /// ## Performance
    /// Typical latency: 10-100ms depending on model size and hardware.
    /// Memory usage scales with batch size and model parameters.
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()>;

    /// Get device name
    fn device_name(&self) -> &str;

    /// Attest to determinism guarantees of this backend
    ///
    /// Returns a DeterminismReport containing metallib hash, RNG seeding method,
    /// floating-point mode, compiler flags, and overall deterministic attestation.
    ///
    /// This method is called during backend initialization and before serving
    /// to validate that the backend meets determinism requirements.
    fn attest_determinism(&self) -> Result<attestation::DeterminismReport>;

    /// Load adapter at runtime (hot-swap)
    ///
    /// Default implementation returns error for backends that don't support hot-swap
    fn load_adapter(&mut self, _id: u16, _weights: &[u8]) -> Result<()> {
        Err(adapteros_core::AosError::Kernel(
            "Hot-swap not supported by this backend".to_string(),
        ))
    }

    /// Attach a preloaded adapter for hot-swap backends.
    ///
    /// Default: no-op for backends that treat `load_adapter` as attach.
    fn attach_adapter(&mut self, _id: u16) -> Result<()> {
        Ok(())
    }

    /// Detach an adapter without requiring backend restart.
    ///
    /// Default: forwards to `unload_adapter` so existing implementations
    /// continue freeing backend resources.
    fn detach_adapter(&mut self, id: u16) -> Result<()> {
        self.unload_adapter(id)
    }

    /// Switch active adapter in-place (optional optimization).
    ///
    /// Default: no-op; backends can override to track an active slot.
    fn switch_adapter(&mut self, _id: u16) -> Result<()> {
        Ok(())
    }

    /// Unload adapter at runtime (hot-swap)
    ///
    /// Default implementation returns error for backends that don't support hot-swap
    fn unload_adapter(&mut self, _id: u16) -> Result<()> {
        Err(adapteros_core::AosError::Kernel(
            "Hot-swap not supported by this backend".to_string(),
        ))
    }

    /// Verify GPU adapter buffers and compute fingerprint
    ///
    /// Samples buffer contents at checkpoints (first/last/mid 4KB) and returns
    /// a fingerprint for integrity verification. This enables cross-layer validation
    /// without full GPU-to-CPU buffer readback.
    ///
    /// # Arguments
    /// * `id` - Adapter ID to verify
    ///
    /// # Returns
    /// * Buffer size in bytes
    /// * Checkpoint samples (first 4KB, last 4KB, mid 4KB)
    ///
    /// Default implementation returns error for backends without GPU verification
    fn verify_adapter_buffers(&self, _id: u16) -> Result<BufferVerificationResult> {
        Err(adapteros_core::AosError::Kernel(
            "GPU buffer verification not supported by this backend".to_string(),
        ))
    }

    /// Store GPU buffer fingerprint for adapter
    ///
    /// Stores a BLAKE3 hash of GPU buffer checkpoint samples for later verification.
    /// Used after adapter load to establish baseline.
    ///
    /// # Arguments
    /// * `id` - Adapter ID
    /// * `buffer_size` - Buffer size in bytes
    /// * `checkpoint_hash_hex` - BLAKE3 hash of checkpoint samples as hex string
    ///
    /// # Returns
    /// * `Ok(())` - Fingerprint stored successfully
    /// * `Err` - Backend does not support GPU fingerprinting
    ///
    /// Default implementation returns error - backends must implement if they support fingerprinting
    fn store_gpu_fingerprint(
        &mut self,
        _id: u16,
        _buffer_size: u64,
        _checkpoint_hash_hex: &str,
    ) -> Result<()> {
        Err(adapteros_core::AosError::Kernel(
            "GPU fingerprint storage not implemented for this backend".to_string(),
        ))
    }

    /// Verify GPU buffer fingerprint matches stored baseline
    ///
    /// Compares current GPU buffer fingerprint against stored baseline.
    ///
    /// # Arguments
    /// * `id` - Adapter ID
    /// * `buffer_size` - Current buffer size
    /// * `checkpoint_hash_hex` - Current BLAKE3 hash as hex string
    ///
    /// # Returns
    /// * `Ok(true)` - Fingerprint matches baseline
    /// * `Ok(false)` - No baseline stored (first verification)
    /// * `Err` - Backend does not support GPU fingerprinting
    ///
    /// Default implementation returns error - backends must implement if they support fingerprinting
    fn verify_gpu_fingerprint(
        &self,
        _id: u16,
        _buffer_size: u64,
        _checkpoint_hash_hex: &str,
    ) -> Result<bool> {
        Err(adapteros_core::AosError::Kernel(
            "GPU fingerprint verification not implemented for this backend".to_string(),
        ))
    }

    /// Check if memory footprint is within adaptive baseline tolerance
    ///
    /// Uses 2σ tolerance with adaptive baseline learning.
    ///
    /// # Arguments
    /// * `id` - Adapter ID
    /// * `buffer_size` - Current buffer size
    ///
    /// # Returns
    /// * within_tolerance: bool - false if not implemented (no baseline available)
    /// * z_score: f64 - 0.0 if not implemented
    /// * baseline_stats: Option<(mean, stddev, sample_count)> - None if not implemented
    ///
    /// Default implementation returns (false, 0.0, None) - no baseline means cannot verify
    fn check_memory_footprint(
        &self,
        _id: u16,
        _buffer_size: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        (false, 0.0, None) // No baseline = cannot verify tolerance
    }

    /// Get backend metrics
    ///
    /// Returns performance metrics for monitoring and telemetry
    fn get_metrics(&self) -> BackendMetrics {
        BackendMetrics::default()
    }

    /// Perform health check on the backend
    ///
    /// Returns the current health status of the backend.
    ///
    /// Default implementation returns Degraded with "health check not implemented" reason.
    /// Backends should override this to perform actual health verification.
    fn health_check(&self) -> Result<BackendHealth> {
        Ok(BackendHealth::Degraded {
            reason: "Health check not implemented for this backend".to_string(),
        })
    }

    /// Get GPU fingerprints for loaded adapters
    ///
    /// Returns a map of adapter IDs to their GPU buffer fingerprints.
    /// Each fingerprint contains buffer size and checkpoint hash.
    ///
    /// Default implementation returns empty map for backends without VRAM tracking.
    fn get_gpu_fingerprints(&self) -> std::collections::HashMap<u32, GpuBufferFingerprint> {
        std::collections::HashMap::new()
    }

    /// Get comprehensive GPU memory report
    ///
    /// Returns memory pool statistics, adapter allocations, and VRAM usage.
    /// Used by capacity handlers to expose real GPU metrics instead of hardcoded values.
    ///
    /// Default implementation returns None for backends without memory tracking.
    fn memory_report(&self) -> Option<GpuMemoryReportData> {
        None
    }

    // =========================================================================
    // TEXT GENERATION METHODS (for backends that bypass run_step)
    // =========================================================================

    /// Check if this backend supports direct text generation
    ///
    /// Backends that support text generation (like MLXSubprocessBridge) return true.
    /// These backends bypass the token-by-token `run_step()` loop and instead
    /// generate text in bulk via `generate_text_full()`.
    ///
    /// Default implementation returns false - most backends use run_step().
    #[deprecated(
        since = "0.1.0",
        note = "use `supports_streaming_text_generation` instead"
    )]
    fn supports_text_generation(&self) -> bool {
        false
    }

    /// Generate text from a prompt (non-streaming)
    ///
    /// For backends that support bulk text generation (where `supports_text_generation()`
    /// returns true), this method generates text directly without using `run_step()`.
    ///
    /// # Arguments
    /// * `prompt` - Input text/prompt to continue from
    /// * `max_tokens` - Maximum tokens to generate
    /// * `temperature` - Sampling temperature (0.0-2.0, typically 0.7)
    /// * `top_p` - Nucleus sampling parameter (0.0-1.0, typically 0.9)
    ///
    /// # Returns
    /// * `TextGenerationResult` with full text, token count, and stats
    ///
    /// # Errors
    /// Default implementation returns error - only text-generation backends implement this.
    #[deprecated(since = "0.1.0", note = "use `generate_text_complete` instead")]
    fn generate_text_full(
        &self,
        _prompt: &str,
        _max_tokens: usize,
        _temperature: f32,
        _top_p: f32,
    ) -> Result<TextGenerationResult> {
        Err(adapteros_core::AosError::Kernel(
            "Text generation not supported by this backend - use run_step() instead".to_string(),
        ))
    }

    /// Check if this backend supports streaming text generation
    ///
    /// Backends that support text generation (like MLXSubprocessBridge) return true.
    /// These backends bypass the token-by-token `run_step()` loop and instead
    /// generate text via `generate_text_complete()`.
    ///
    /// Default implementation forwards to the deprecated `supports_text_generation()`.
    #[allow(deprecated)]
    fn supports_streaming_text_generation(&self) -> bool {
        self.supports_text_generation()
    }

    /// Generate text from a prompt (blocking, returns complete result)
    ///
    /// For backends that support bulk text generation (where
    /// `supports_streaming_text_generation()` returns true), this method generates
    /// text directly without using `run_step()`.
    ///
    /// # Arguments
    /// * `prompt` - Input text/prompt to continue from
    /// * `max_tokens` - Maximum tokens to generate
    /// * `temperature` - Sampling temperature (0.0-2.0, typically 0.7)
    /// * `top_p` - Nucleus sampling parameter (0.0-1.0, typically 0.9)
    ///
    /// # Returns
    /// * `TextGenerationResult` with full text, token count, and stats
    ///
    /// # Errors
    /// Default implementation forwards to the deprecated `generate_text_full()`.
    #[allow(deprecated)]
    fn generate_text_complete(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<TextGenerationResult> {
        self.generate_text_full(prompt, max_tokens, temperature, top_p)
    }

    /// Generate text with streaming (blocking call with callback)
    fn generate_text_stream(
        &self,
        _prompt: &str,
        _max_tokens: usize,
        _temperature: f32,
        _top_p: f32,
        _on_token: &mut dyn FnMut(TextToken) -> bool,
    ) -> Result<TextGenerationResult> {
        Err(adapteros_core::AosError::Kernel(
            "Streaming text generation not supported by this backend".to_string(),
        ))
    }

    /// Pre-warm experts for MoE models
    fn prewarm_experts(&self, _experts: Vec<(usize, u8)>) -> Result<usize> {
        Ok(0) // Default: no-op
    }

    /// Check if this backend is currently running a Mixture of Experts (MoE) model
    fn is_moe(&self) -> bool {
        false // Default: not MoE
    }

    /// Get the number of experts in the model (for MoE models)
    fn num_experts(&self) -> usize {
        0
    }

    /// Get the number of experts activated per token (for MoE models)
    fn experts_per_token(&self) -> usize {
        0
    }
}

/// GPU buffer fingerprint for cross-layer integrity verification
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuBufferFingerprint {
    /// Buffer size in bytes
    pub buffer_bytes: u64,
    /// BLAKE3 hash of checkpoint samples
    pub checkpoint_hash: adapteros_core::B3Hash,
}

/// GPU memory report data returned by backends that support memory tracking
///
/// This struct provides a backend-agnostic view of GPU memory usage.
/// Backends like Metal expose this through their memory pool tracking.
#[derive(Debug, Clone, Default)]
pub struct GpuMemoryReportData {
    /// Total GPU memory in bytes
    pub total_gpu_bytes: u64,
    /// Used GPU memory in bytes
    pub used_gpu_bytes: u64,
    /// Number of tracked adapters
    pub adapter_count: usize,
    /// Total VRAM used by adapters
    pub adapter_vram_total: u64,
    /// Per-adapter allocations: (adapter_id, bytes)
    pub adapter_allocations: Vec<(u32, u64)>,
    /// Pool statistics (if available)
    pub pool_stats: Option<GpuPoolStats>,
}

/// GPU memory pool statistics
#[derive(Debug, Clone, Default)]
pub struct GpuPoolStats {
    /// Total allocations made
    pub total_allocations: u64,
    /// Current active memory in bytes
    pub active_bytes: u64,
    /// Current pooled (cached) memory in bytes
    pub pooled_bytes: u64,
    /// Pool hit rate (0.0-1.0)
    pub hit_rate: f32,
    /// Peak memory usage in bytes
    pub peak_usage: u64,
}

/// Mock kernels implementation for testing
#[derive(Debug)]
pub struct MockKernels {
    device_name: String,
}

impl MockKernels {
    /// Create a new mock kernels instance
    pub fn new() -> Self {
        Self {
            device_name: "Mock Kernels (Test)".to_string(),
        }
    }
}

impl FusedKernels for MockKernels {
    fn as_liquid_kernel(&self) -> Option<&dyn liquid::LiquidKernel> {
        Some(self)
    }

    fn as_liquid_kernel_mut(&mut self) -> Option<&mut dyn liquid::LiquidKernel> {
        Some(self)
    }

    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Mock implementation - no-op
        Ok(())
    }

    fn run_step(&mut self, _ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
        // Mock implementation - generate deterministic logits for testing
        for (i, logit) in io.output_logits.iter_mut().enumerate() {
            *logit = (i as f32 * 0.001) % 1.0; // Deterministic pattern
        }

        io.position += 1;
        Ok(())
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        // Mock kernels are deterministic for testing purposes
        Ok(attestation::DeterminismReport::for_mock())
    }

    fn check_memory_footprint(
        &self,
        _id: u16,
        _buffer_size: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        (true, 0.0, None)
    }
}

impl Default for MockKernels {
    fn default() -> Self {
        Self::new()
    }
}

impl liquid::LiquidKernel for MockKernels {
    fn blend_and_forward(
        &mut self,
        request: liquid::LiquidBlendRequest<'_>,
    ) -> Result<liquid::LiquidBlendStats> {
        liquid::blend_and_forward_reference(request)
    }
}

/// Failing kernel implementation for testing strict mode behavior
///
/// This kernel always fails on `run_step()` to test that strict mode
/// properly prevents backend fallback when the primary backend fails.
#[derive(Debug)]
pub struct FailingKernel {
    device_name: String,
    fail_message: String,
}

impl FailingKernel {
    /// Create a new failing kernel that returns the specified error message
    pub fn new(fail_message: &str) -> Self {
        Self {
            device_name: "FailingKernel (Test)".to_string(),
            fail_message: fail_message.to_string(),
        }
    }
}

impl FusedKernels for FailingKernel {
    fn load(&mut self, _plan_bytes: &[u8]) -> Result<()> {
        // Load succeeds - we only fail on run_step
        Ok(())
    }

    fn run_step(&mut self, _ring: &RouterRing, _io: &mut IoBuffers) -> Result<()> {
        // Always fail to test strict mode behavior
        Err(adapteros_core::AosError::Kernel(self.fail_message.clone()))
    }

    fn device_name(&self) -> &str {
        &self.device_name
    }

    fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
        // Failing kernel is not deterministic
        let mut report = attestation::DeterminismReport::for_mock();
        report.deterministic = false;
        report.determinism_level = attestation::DeterminismLevel::None;
        Ok(report)
    }
}

impl Default for FailingKernel {
    fn default() -> Self {
        Self::new("FailingKernel: intentional failure for testing")
    }
}

/// Macro to implement FusedKernels for Box<dyn FusedKernels> variants
/// Eliminates 138 lines of duplication between two nearly-identical impls
macro_rules! impl_fused_kernels_for_box {
    ($($bounds:tt)*) => {
        impl FusedKernels for Box<dyn FusedKernels $($bounds)*> {
            fn as_liquid_kernel(&self) -> Option<&dyn crate::liquid::LiquidKernel> {
                (**self).as_liquid_kernel()
            }

            fn as_liquid_kernel_mut(&mut self) -> Option<&mut dyn crate::liquid::LiquidKernel> {
                (**self).as_liquid_kernel_mut()
            }

            fn supports_liquid_blending(&self) -> bool {
                (**self).supports_liquid_blending()
            }

            fn liquid_max_adapters(&self) -> usize {
                (**self).liquid_max_adapters()
            }

            fn load(&mut self, plan_bytes: &[u8]) -> Result<()> {
                (**self).load(plan_bytes)
            }

            fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()> {
                (**self).run_step(ring, io)
            }

            fn device_name(&self) -> &str {
                (**self).device_name()
            }

            fn attest_determinism(&self) -> Result<attestation::DeterminismReport> {
                (**self).attest_determinism()
            }

            fn load_adapter(&mut self, id: u16, weights: &[u8]) -> Result<()> {
                (**self).load_adapter(id, weights)
            }

            fn unload_adapter(&mut self, id: u16) -> Result<()> {
                (**self).unload_adapter(id)
            }

            fn verify_adapter_buffers(&self, id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)> {
                (**self).verify_adapter_buffers(id)
            }

            fn store_gpu_fingerprint(
                &mut self,
                id: u16,
                buffer_size: u64,
                checkpoint_hash_hex: &str,
            ) -> Result<()> {
                (**self).store_gpu_fingerprint(id, buffer_size, checkpoint_hash_hex)
            }

            fn verify_gpu_fingerprint(
                &self,
                id: u16,
                buffer_size: u64,
                checkpoint_hash_hex: &str,
            ) -> Result<bool> {
                (**self).verify_gpu_fingerprint(id, buffer_size, checkpoint_hash_hex)
            }

            fn check_memory_footprint(
                &self,
                id: u16,
                buffer_size: u64,
            ) -> (bool, f64, Option<(f64, f64, usize)>) {
                (**self).check_memory_footprint(id, buffer_size)
            }

            fn get_metrics(&self) -> BackendMetrics {
                (**self).get_metrics()
            }

            fn health_check(&self) -> Result<BackendHealth> {
                (**self).health_check()
            }

            fn get_gpu_fingerprints(&self) -> std::collections::HashMap<u32, GpuBufferFingerprint> {
                (**self).get_gpu_fingerprints()
            }

            fn memory_report(&self) -> Option<GpuMemoryReportData> {
                (**self).memory_report()
            }

            #[allow(deprecated)]
            fn supports_text_generation(&self) -> bool {
                (**self).supports_text_generation()
            }

            fn supports_streaming_text_generation(&self) -> bool {
                (**self).supports_streaming_text_generation()
            }

            #[allow(deprecated)]
            fn generate_text_full(
                &self,
                prompt: &str,
                max_tokens: usize,
                temperature: f32,
                top_p: f32,
            ) -> Result<TextGenerationResult> {
                (**self).generate_text_full(prompt, max_tokens, temperature, top_p)
            }

            fn generate_text_complete(
                &self,
                prompt: &str,
                max_tokens: usize,
                temperature: f32,
                top_p: f32,
            ) -> Result<TextGenerationResult> {
                (**self).generate_text_complete(prompt, max_tokens, temperature, top_p)
            }

            fn generate_text_stream(
                &self,
                prompt: &str,
                max_tokens: usize,
                temperature: f32,
                top_p: f32,
                on_token: &mut dyn FnMut(TextToken) -> bool,
            ) -> Result<TextGenerationResult> {
                (**self).generate_text_stream(prompt, max_tokens, temperature, top_p, on_token)
            }

            fn prewarm_experts(&self, experts: Vec<(usize, u8)>) -> Result<usize> {
                (**self).prewarm_experts(experts)
            }

            fn is_moe(&self) -> bool {
                (**self).is_moe()
            }

            fn num_experts(&self) -> usize {
                (**self).num_experts()
            }

            fn experts_per_token(&self) -> usize {
                (**self).experts_per_token()
            }
        }
    };
}

// Apply macro for both Box variants
impl_fused_kernels_for_box!();
impl_fused_kernels_for_box!(+ Send + Sync);

/// Trait for adapter lookup operations
///
/// This trait abstracts adapter table operations to break circular dependencies
/// between lifecycle and worker crates. Implementations can be provided by
/// the worker crate while being consumed by the lifecycle crate.
pub trait AdapterLookup: Send + Sync {
    /// Get adapter weight bytes by ID
    fn get_adapter_weights(&self, adapter_id: &str) -> Result<Vec<u8>>;

    /// Check if adapter is loaded
    fn is_adapter_loaded(&self, adapter_id: &str) -> bool;

    /// Get adapter index for routing
    fn get_adapter_index(&self, adapter_id: &str) -> Option<u16>;
}

/// DIR configuration for kernels
/// Reference: https://openreview.net/pdf?id=jqz6Msm3AF
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MploraConfig {
    pub shared_downsample: bool,
    pub compression_ratio: f32,
    pub orthogonal_constraints: bool,
    pub similarity_threshold: f32,
    pub penalty_weight: f32,
    pub history_window: usize,
}

impl Default for MploraConfig {
    fn default() -> Self {
        Self {
            shared_downsample: false,
            compression_ratio: 0.8,
            orthogonal_constraints: false,
            similarity_threshold: 0.7,
            penalty_weight: 0.1,
            history_window: 10,
        }
    }
}

// ============================================================================
// Text Generation Kernel (for backends without logits support)
// ============================================================================

/// Result of a text generation request
#[derive(Debug, Clone)]
pub struct TextGenerationResult {
    /// Complete generated text
    pub text: String,
    /// Number of tokens generated
    pub tokens_generated: usize,
    /// Reason generation stopped (e.g., "stop", "length", "max_tokens")
    pub finish_reason: String,
    /// Optional token usage statistics (prompt_tokens, completion_tokens, total)
    pub usage_stats: Option<TextGenerationUsage>,
    /// Optional timing statistics (TTFT, total latency, tokens/sec)
    pub timing_stats: Option<TextGenerationTiming>,
    /// Protocol v3: MoE model information
    pub moe_info: Option<MoEInfo>,
    /// Protocol v3: Expert routing data for the generated sequence
    pub expert_routing: Option<SequenceExpertRouting>,
    /// Number of precomputed "free tokens" delivered without backend computation
    pub free_tokens_delivered: usize,
    /// Protocol v3: Deterministic routing hash (BLAKE3)
    pub routing_hash: Option<adapteros_core::B3Hash>,
}

/// MoE (Mixture of Experts) model information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoEInfo {
    /// Whether the model is an MoE model
    pub is_moe: bool,
    /// Number of experts in the model
    pub num_experts: usize,
    /// Number of experts activated per token
    pub experts_per_token: usize,
}

/// Per-token expert routing: which expert was selected at each layer
/// Each tuple represents (layer_index, expert_id)
pub type ExpertRouting = Vec<(usize, u8)>;

/// Expert routing for an entire sequence of tokens
pub type SequenceExpertRouting = Vec<ExpertRouting>;

/// Usage statistics for text generation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextGenerationUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// Timing statistics for text generation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TextGenerationTiming {
    /// Time to first token in milliseconds
    pub ttft_ms: f64,
    /// Total generation time in milliseconds
    pub total_ms: f64,
    /// Throughput in tokens per second
    pub tokens_per_second: f64,
}

/// A token yielded during streaming generation
#[derive(Debug, Clone)]
pub struct TextToken {
    /// Token text/string
    pub text: String,
    /// Optional token ID from vocabulary
    pub token_id: Option<usize>,
    /// Token index in the generated sequence
    pub index: usize,
    /// Protocol v3: Expert routing for this token (MoE models)
    pub expert_routing: Option<ExpertRouting>,
    /// Whether this is a "free token" (pre-computed)
    pub is_free: bool,
}

/// Trait for text generation backends that don't support token-by-token logits inference.
///
/// Some backends (like MLXSubprocessBridge using Python mlx-lm) can only perform bulk text
/// generation, not single-token inference with logits output. This trait provides an
/// alternative interface for such backends.
///
/// ## Design Rationale
///
/// The FusedKernels trait requires logits output for router-driven inference:
/// ```ignore
/// fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()>
/// // Must fill: io.output_logits: Vec<f32> (vocab-sized probability distribution)
/// ```
///
/// However, Python mlx-lm only exposes:
/// - Text generation (returns text string)
/// - Streaming generation (returns tokens as they're generated)
/// - NOT raw logits or log-probabilities
///
/// This trait allows backends to declare "I do text generation, not token-by-token inference"
/// and provides methods for the worker to use them in text generation workflows.
///
/// ## Implementation Strategy
///
/// Backends that implement TextGenerationKernel:
/// 1. Declare they don't support FusedKernels::run_step() (return error)
/// 2. Implement generate_text_full() for bulk generation
/// 3. Report text generation capability via supports_text_generation()
pub trait TextGenerationKernel: Send + Sync {
    /// Check if this backend supports text generation
    ///
    /// Returns true if the backend can perform text generation via
    /// generate_text_full().
    fn supports_text_generation(&self) -> bool {
        false
    }

    /// Generate text from a prompt (non-streaming)
    ///
    /// Performs bulk text generation and returns the complete result
    /// including usage statistics and timing information.
    ///
    /// # Arguments
    /// * `prompt` - Input text/prompt to continue from
    /// * `max_tokens` - Maximum tokens to generate
    /// * `temperature` - Sampling temperature (0.0-2.0, typically 0.7)
    /// * `top_p` - Nucleus sampling parameter (0.0-1.0, typically 0.9)
    ///
    /// # Returns
    /// * TextGenerationResult with full text, token count, and stats
    fn generate_text_full(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        top_p: f32,
    ) -> Result<TextGenerationResult> {
        let _ = (prompt, max_tokens, temperature, top_p);
        Err(adapteros_core::AosError::Kernel(
            "Text generation not supported by this backend".to_string(),
        ))
    }

    /// Get the backend name for debugging/logging
    fn text_generation_backend_name(&self) -> &str {
        "Unknown"
    }
}

/// Extended kernel trait with DIR support
pub trait MploraKernels: FusedKernels {
    /// Execute DIR with shared downsample
    fn execute_mplora(
        &mut self,
        ring: &RouterRing,
        io: &mut IoBuffers,
        mplora_config: &MploraConfig,
    ) -> Result<()>;

    /// Apply orthogonal constraints
    fn apply_orthogonal_constraints(
        &mut self,
        adapter_indices: &[u16],
        gates: &[i16],
        config: &MploraConfig,
    ) -> Result<()>;

    /// Execute shared downsample kernel
    fn execute_shared_downsample(
        &mut self,
        input: &[f32],
        shared_a: &[f32],
        adapter_bs: &[f32],
        gates: &[i16],
        output: &mut [f32],
        config: &MploraConfig,
    ) -> Result<()>;

    /// Execute compression kernel
    fn execute_compression(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        config: &MploraConfig,
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // =========================================================================
    // BackendHealth Tests
    // =========================================================================

    #[test]
    fn backend_health_healthy_variant() {
        let health = BackendHealth::Healthy;
        match health {
            BackendHealth::Healthy => (),
            _ => panic!("Expected Healthy variant"),
        }
    }

    #[test]
    fn backend_health_degraded_with_reason() {
        let reason = "High memory pressure".to_string();
        let health = BackendHealth::Degraded {
            reason: reason.clone(),
        };
        match health {
            BackendHealth::Degraded { reason: r } => assert_eq!(r, reason),
            _ => panic!("Expected Degraded variant"),
        }
    }

    #[test]
    fn backend_health_failed_recoverable() {
        let health = BackendHealth::Failed {
            reason: "GPU timeout".to_string(),
            recoverable: true,
        };
        match health {
            BackendHealth::Failed {
                reason,
                recoverable,
            } => {
                assert_eq!(reason, "GPU timeout");
                assert!(recoverable);
            }
            _ => panic!("Expected Failed variant"),
        }
    }

    #[test]
    fn backend_health_failed_unrecoverable() {
        let health = BackendHealth::Failed {
            reason: "Hardware fault".to_string(),
            recoverable: false,
        };
        match health {
            BackendHealth::Failed {
                reason,
                recoverable,
            } => {
                assert_eq!(reason, "Hardware fault");
                assert!(!recoverable);
            }
            _ => panic!("Expected Failed variant"),
        }
    }

    // =========================================================================
    // BackendMetrics Tests
    // =========================================================================

    #[test]
    fn backend_metrics_default() {
        let metrics = BackendMetrics::default();
        assert_eq!(metrics.total_operations, 0);
        assert_eq!(metrics.successful_operations, 0);
        assert_eq!(metrics.failed_operations, 0);
        assert_eq!(metrics.avg_latency, Duration::ZERO);
        assert_eq!(metrics.memory_usage_bytes, 0);
    }

    #[test]
    fn backend_metrics_with_values() {
        let metrics = BackendMetrics {
            total_operations: 100,
            successful_operations: 95,
            failed_operations: 5,
            avg_latency: Duration::from_millis(42),
            memory_usage_bytes: 1024 * 1024 * 512, // 512 MB
        };
        assert_eq!(metrics.total_operations, 100);
        assert_eq!(metrics.successful_operations, 95);
        assert_eq!(metrics.failed_operations, 5);
        assert_eq!(metrics.avg_latency, Duration::from_millis(42));
        assert_eq!(metrics.memory_usage_bytes, 512 * 1024 * 1024);
    }

    // =========================================================================
    // RouterRing Tests
    // =========================================================================

    #[test]
    fn router_ring_new_valid_k() {
        for k in 0..=8 {
            let ring = RouterRing::new(k);
            assert_eq!(ring.k, k);
            assert_eq!(ring.len(), k);
            assert_eq!(ring.position, 0);
            assert!(ring.indices.iter().all(|&i| i == 0));
            assert!(ring.gates_q15.iter().all(|&g| g == 0));
        }
    }

    #[test]
    fn router_ring_is_empty() {
        let ring = RouterRing::new(0);
        assert!(ring.is_empty());

        let ring = RouterRing::new(1);
        assert!(!ring.is_empty());
    }

    #[test]
    fn router_ring_set_valid_entries() {
        let mut ring = RouterRing::new(4);
        let indices: [u16; 4] = [1, 2, 3, 4];
        let gates: [i16; 4] = [1000, 2000, 1500, 500];

        ring.set(&indices, &gates);

        assert_eq!(ring.active_indices(), &[1, 2, 3, 4]);
        assert_eq!(ring.active_gates(), &[1000, 2000, 1500, 500]);
        assert_eq!(ring.k, 4);
    }

    #[test]
    fn router_ring_set_with_max_adapter_valid() {
        let mut ring = RouterRing::new(3);
        let indices: [u16; 3] = [0, 5, 9];
        let gates: [i16; 3] = [100, 200, 300];

        // All indices < 10, so this should succeed
        ring.set_with_max_adapter(&indices, &gates, 10);

        assert_eq!(ring.active_indices(), &[0, 5, 9]);
        assert_eq!(ring.active_gates(), &[100, 200, 300]);
    }

    #[test]
    fn router_ring_zero_fills_unused_entries() {
        let mut ring = RouterRing::new(8);
        ring.indices = [9, 9, 9, 9, 9, 9, 9, 9];
        ring.gates_q15 = [999, 999, 999, 999, 999, 999, 999, 999];

        let indices: [u16; 3] = [1, 2, 3];
        let gates: [i16; 3] = [100, 200, 300];
        ring.set(&indices, &gates);

        // Active entries should be set
        assert_eq!(ring.indices[..3], [1, 2, 3]);
        assert_eq!(ring.gates_q15[..3], [100, 200, 300]);

        // Unused entries should be zero-filled
        assert_eq!(ring.indices[3..], [0, 0, 0, 0, 0]);
        assert_eq!(ring.gates_q15[3..], [0, 0, 0, 0, 0]);
    }

    #[test]
    fn router_ring_active_slices() {
        let mut ring = RouterRing::new(2);
        ring.set(&[10, 20], &[500, 600]);

        assert_eq!(ring.active_indices(), &[10, 20]);
        assert_eq!(ring.active_gates(), &[500, 600]);
        assert_eq!(ring.len(), 2);
    }

    // =========================================================================
    // IoBuffers Tests
    // =========================================================================

    #[test]
    fn io_buffers_new() {
        let vocab_size = 32000;
        let io = IoBuffers::new(vocab_size);

        assert!(io.input_ids.is_empty());
        assert_eq!(io.output_logits.len(), vocab_size);
        assert!(io.output_logits.iter().all(|&l| l == 0.0));
        assert_eq!(io.position, 0);
    }

    #[test]
    fn io_buffers_small_vocab() {
        let io = IoBuffers::new(100);
        assert_eq!(io.output_logits.len(), 100);
    }

    // =========================================================================
    // MockKernels Tests
    // =========================================================================

    #[test]
    fn mock_kernels_new() {
        let mock = MockKernels::new();
        assert_eq!(mock.device_name(), "Mock Kernels (Test)");
    }

    #[test]
    fn mock_kernels_default() {
        let mock = MockKernels::default();
        assert_eq!(mock.device_name(), "Mock Kernels (Test)");
    }

    #[test]
    fn mock_kernels_load_succeeds() {
        let mut mock = MockKernels::new();
        assert!(mock.load(&[]).is_ok());
        assert!(mock.load(&[1, 2, 3, 4]).is_ok());
    }

    #[test]
    fn mock_kernels_run_step_deterministic() {
        let mut mock = MockKernels::new();
        let ring = RouterRing::new(2);
        let mut io1 = IoBuffers::new(100);
        let mut io2 = IoBuffers::new(100);

        mock.run_step(&ring, &mut io1).unwrap();
        mock.run_step(&ring, &mut io2).unwrap();

        // The first run_step should produce identical logits patterns
        // (but positions will differ since run_step increments position)
        for i in 0..100 {
            assert!(
                (io1.output_logits[i] - io2.output_logits[i]).abs() < 1e-9,
                "Logits should be deterministic"
            );
        }
    }

    #[test]
    fn mock_kernels_run_step_increments_position() {
        let mut mock = MockKernels::new();
        let ring = RouterRing::new(1);
        let mut io = IoBuffers::new(50);

        assert_eq!(io.position, 0);
        mock.run_step(&ring, &mut io).unwrap();
        assert_eq!(io.position, 1);
        mock.run_step(&ring, &mut io).unwrap();
        assert_eq!(io.position, 2);
    }

    #[test]
    fn mock_kernels_attest_determinism() {
        let mock = MockKernels::new();
        let report = mock.attest_determinism().unwrap();

        assert!(report.deterministic);
        assert_eq!(report.backend_type, attestation::BackendType::Mock);
    }

    #[test]
    fn mock_kernels_supports_liquid_blending() {
        let mock = MockKernels::new();
        // Use FusedKernels trait method to disambiguate
        assert!(FusedKernels::supports_liquid_blending(&mock));
        assert_eq!(FusedKernels::liquid_max_adapters(&mock), liquid::LIQUID_MAX_ADAPTERS);
    }

    #[test]
    fn mock_kernels_check_memory_footprint() {
        let mock = MockKernels::new();
        let (within_tolerance, z_score, baseline) = mock.check_memory_footprint(0, 1024);
        assert!(within_tolerance);
        assert_eq!(z_score, 0.0);
        assert!(baseline.is_none());
    }

    // =========================================================================
    // FailingKernel Tests
    // =========================================================================

    #[test]
    fn failing_kernel_new() {
        let kernel = FailingKernel::new("test failure");
        assert_eq!(kernel.device_name(), "FailingKernel (Test)");
    }

    #[test]
    fn failing_kernel_default() {
        let kernel = FailingKernel::default();
        assert!(kernel.fail_message.contains("intentional failure"));
    }

    #[test]
    fn failing_kernel_load_succeeds() {
        let mut kernel = FailingKernel::new("failure");
        assert!(kernel.load(&[]).is_ok());
    }

    #[test]
    fn failing_kernel_run_step_fails() {
        let mut kernel = FailingKernel::new("expected failure");
        let ring = RouterRing::new(1);
        let mut io = IoBuffers::new(10);

        let result = kernel.run_step(&ring, &mut io);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("expected failure"));
    }

    #[test]
    fn failing_kernel_attest_not_deterministic() {
        let kernel = FailingKernel::new("failure");
        let report = kernel.attest_determinism().unwrap();

        assert!(!report.deterministic);
        assert_eq!(report.determinism_level, attestation::DeterminismLevel::None);
    }

    // =========================================================================
    // GpuBufferFingerprint Tests
    // =========================================================================

    #[test]
    fn gpu_buffer_fingerprint_equality() {
        let hash1 = adapteros_core::B3Hash::hash(b"test data");
        let hash2 = adapteros_core::B3Hash::hash(b"test data");
        let hash3 = adapteros_core::B3Hash::hash(b"different data");

        let fp1 = GpuBufferFingerprint {
            buffer_bytes: 1024,
            checkpoint_hash: hash1,
        };
        let fp2 = GpuBufferFingerprint {
            buffer_bytes: 1024,
            checkpoint_hash: hash2,
        };
        let fp3 = GpuBufferFingerprint {
            buffer_bytes: 1024,
            checkpoint_hash: hash3,
        };
        let fp4 = GpuBufferFingerprint {
            buffer_bytes: 2048,
            checkpoint_hash: hash1,
        };

        assert_eq!(fp1, fp2);
        assert_ne!(fp1, fp3); // Different hash
        assert_ne!(fp1, fp4); // Different size
    }

    // =========================================================================
    // GpuMemoryReportData Tests
    // =========================================================================

    #[test]
    fn gpu_memory_report_data_default() {
        let report = GpuMemoryReportData::default();
        assert_eq!(report.total_gpu_bytes, 0);
        assert_eq!(report.used_gpu_bytes, 0);
        assert_eq!(report.adapter_count, 0);
        assert_eq!(report.adapter_vram_total, 0);
        assert!(report.adapter_allocations.is_empty());
        assert!(report.pool_stats.is_none());
    }

    #[test]
    fn gpu_memory_report_data_with_values() {
        let report = GpuMemoryReportData {
            total_gpu_bytes: 8 * 1024 * 1024 * 1024, // 8 GB
            used_gpu_bytes: 4 * 1024 * 1024 * 1024,  // 4 GB
            adapter_count: 3,
            adapter_vram_total: 512 * 1024 * 1024, // 512 MB
            adapter_allocations: vec![(0, 200 * 1024 * 1024), (1, 312 * 1024 * 1024)],
            pool_stats: Some(GpuPoolStats {
                total_allocations: 100,
                active_bytes: 1024 * 1024,
                pooled_bytes: 2048 * 1024,
                hit_rate: 0.85,
                peak_usage: 5 * 1024 * 1024 * 1024,
            }),
        };

        assert_eq!(report.total_gpu_bytes, 8 * 1024 * 1024 * 1024);
        assert_eq!(report.adapter_count, 3);
        assert_eq!(report.adapter_allocations.len(), 2);
        assert!(report.pool_stats.is_some());
    }

    // =========================================================================
    // GpuPoolStats Tests
    // =========================================================================

    #[test]
    fn gpu_pool_stats_default() {
        let stats = GpuPoolStats::default();
        assert_eq!(stats.total_allocations, 0);
        assert_eq!(stats.active_bytes, 0);
        assert_eq!(stats.pooled_bytes, 0);
        assert_eq!(stats.hit_rate, 0.0);
        assert_eq!(stats.peak_usage, 0);
    }

    // =========================================================================
    // MploraConfig Tests
    // =========================================================================

    #[test]
    fn mplora_config_default() {
        let config = MploraConfig::default();
        assert!(!config.shared_downsample);
        assert!((config.compression_ratio - 0.8).abs() < 1e-6);
        assert!(!config.orthogonal_constraints);
        assert!((config.similarity_threshold - 0.7).abs() < 1e-6);
        assert!((config.penalty_weight - 0.1).abs() < 1e-6);
        assert_eq!(config.history_window, 10);
    }

    #[test]
    fn mplora_config_serialize_deserialize() {
        let config = MploraConfig {
            shared_downsample: true,
            compression_ratio: 0.5,
            orthogonal_constraints: true,
            similarity_threshold: 0.9,
            penalty_weight: 0.2,
            history_window: 20,
        };

        let json = serde_json::to_string(&config).unwrap();
        let deserialized: MploraConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.shared_downsample, deserialized.shared_downsample);
        assert!((config.compression_ratio - deserialized.compression_ratio).abs() < 1e-6);
        assert_eq!(
            config.orthogonal_constraints,
            deserialized.orthogonal_constraints
        );
        assert_eq!(config.history_window, deserialized.history_window);
    }

    // =========================================================================
    // TextGenerationResult Tests
    // =========================================================================

    #[test]
    fn text_generation_result_basic() {
        let result = TextGenerationResult {
            text: "Hello, world!".to_string(),
            tokens_generated: 3,
            finish_reason: "stop".to_string(),
            usage_stats: None,
            timing_stats: None,
            moe_info: None,
            expert_routing: None,
            free_tokens_delivered: 0,
            routing_hash: None,
        };

        assert_eq!(result.text, "Hello, world!");
        assert_eq!(result.tokens_generated, 3);
        assert_eq!(result.finish_reason, "stop");
    }

    #[test]
    fn text_generation_result_with_stats() {
        let result = TextGenerationResult {
            text: "Generated text".to_string(),
            tokens_generated: 10,
            finish_reason: "length".to_string(),
            usage_stats: Some(TextGenerationUsage {
                prompt_tokens: 5,
                completion_tokens: 10,
                total_tokens: 15,
            }),
            timing_stats: Some(TextGenerationTiming {
                ttft_ms: 50.0,
                total_ms: 200.0,
                tokens_per_second: 50.0,
            }),
            moe_info: Some(MoEInfo {
                is_moe: true,
                num_experts: 8,
                experts_per_token: 2,
            }),
            expert_routing: None,
            free_tokens_delivered: 2,
            routing_hash: None,
        };

        assert!(result.usage_stats.is_some());
        assert!(result.timing_stats.is_some());
        assert!(result.moe_info.is_some());
        assert_eq!(result.free_tokens_delivered, 2);
    }

    // =========================================================================
    // TextToken Tests
    // =========================================================================

    #[test]
    fn text_token_basic() {
        let token = TextToken {
            text: "hello".to_string(),
            token_id: Some(12345),
            index: 0,
            expert_routing: None,
            is_free: false,
        };

        assert_eq!(token.text, "hello");
        assert_eq!(token.token_id, Some(12345));
        assert_eq!(token.index, 0);
        assert!(!token.is_free);
    }

    #[test]
    fn text_token_free_token() {
        let token = TextToken {
            text: "cached".to_string(),
            token_id: Some(100),
            index: 5,
            expert_routing: None,
            is_free: true,
        };

        assert!(token.is_free);
    }

    // =========================================================================
    // FusedKernels Default Method Tests
    // =========================================================================

    #[test]
    fn fused_kernels_default_load_adapter_returns_error() {
        // FailingKernel uses the default load_adapter implementation which returns error
        let mut kernel = FailingKernel::new("test");
        let result = kernel.load_adapter(0, &[]);
        assert!(result.is_err());
    }

    #[test]
    fn fused_kernels_default_unload_adapter_returns_error() {
        let kernel = FailingKernel::new("test");
        let mut boxed: Box<dyn FusedKernels> = Box::new(kernel);
        let result = boxed.unload_adapter(0);
        assert!(result.is_err());
    }

    #[test]
    fn fused_kernels_default_verify_adapter_buffers_returns_error() {
        let mock = MockKernels::new();
        let result = mock.verify_adapter_buffers(0);
        assert!(result.is_err());
    }

    #[test]
    fn fused_kernels_default_health_check_returns_degraded() {
        let kernel = FailingKernel::new("test");
        let result = kernel.health_check().unwrap();
        match result {
            BackendHealth::Degraded { reason } => {
                assert!(reason.contains("not implemented"));
            }
            _ => panic!("Expected Degraded health status"),
        }
    }

    #[test]
    fn fused_kernels_default_get_metrics() {
        let kernel = FailingKernel::new("test");
        let metrics = kernel.get_metrics();
        assert_eq!(metrics.total_operations, 0);
    }

    #[test]
    fn fused_kernels_default_supports_text_generation() {
        let kernel = FailingKernel::new("test");
        #[allow(deprecated)]
        {
            assert!(!kernel.supports_text_generation());
        }
        assert!(!kernel.supports_streaming_text_generation());
    }

    #[test]
    fn fused_kernels_default_generate_text_returns_error() {
        let kernel = FailingKernel::new("test");
        let result = kernel.generate_text_complete("prompt", 10, 0.7, 0.9);
        assert!(result.is_err());
    }

    #[test]
    fn fused_kernels_default_prewarm_experts() {
        let kernel = FailingKernel::new("test");
        let result = kernel.prewarm_experts(vec![(0, 1), (1, 2)]).unwrap();
        assert_eq!(result, 0); // Default returns 0 (no-op)
    }

    #[test]
    fn fused_kernels_default_moe_methods() {
        let kernel = FailingKernel::new("test");
        assert!(!kernel.is_moe());
        assert_eq!(kernel.num_experts(), 0);
        assert_eq!(kernel.experts_per_token(), 0);
    }
}
