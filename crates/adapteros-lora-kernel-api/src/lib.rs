//! Safe API for fused kernels

use adapteros_core::Result;
use serde::{Deserialize, Serialize};

pub mod attestation;

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
                panic!("RouterRing: K must be ≤ 8, got {}", k);
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
}

/// IO buffers for kernel execution
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
pub trait FusedKernels: Send + Sync {
    /// Load plan and weights
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()>;

    /// Run a single token step
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
    fn verify_adapter_buffers(&self, _id: u16) -> Result<(u64, Vec<u8>, Vec<u8>, Vec<u8>)> {
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
    /// Default implementation is no-op for backends without GPU tracking
    fn store_gpu_fingerprint(&mut self, _id: u16, _buffer_size: u64, _checkpoint_hash_hex: &str) {
        // No-op for backends without VRAM tracking
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
    /// * `Ok(false)` - No baseline stored yet (first verification)
    /// * `Err(msg)` - Fingerprint mismatch
    ///
    /// Default implementation returns Ok(true) for backends without GPU tracking
    fn verify_gpu_fingerprint(
        &self,
        _id: u16,
        _buffer_size: u64,
        _checkpoint_hash_hex: &str,
    ) -> Result<bool> {
        Ok(true) // No verification for non-GPU backends
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
    /// * within_tolerance: bool
    /// * z_score: f64
    /// * baseline_stats: Option<(mean, stddev, sample_count)>
    ///
    /// Default implementation returns (true, 0.0, None) for backends without tracking
    fn check_memory_footprint(
        &self,
        _id: u16,
        _buffer_size: u64,
    ) -> (bool, f64, Option<(f64, f64, usize)>) {
        (true, 0.0, None) // No anomaly detection for non-GPU backends
    }
}

/// Mock kernels implementation for testing
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
        Ok(attestation::DeterminismReport {
            backend_type: attestation::BackendType::Mock,
            metallib_hash: None,
            manifest: None,
            rng_seed_method: attestation::RngSeedingMethod::FixedSeed(0),
            floating_point_mode: attestation::FloatingPointMode::Deterministic,
            compiler_flags: vec![],
            deterministic: true,
        })
    }
}

impl Default for MockKernels {
    fn default() -> Self {
        Self::new()
    }
}

/// Impl FusedKernels for Box<dyn FusedKernels> to enable dynamic dispatch
impl FusedKernels for Box<dyn FusedKernels> {
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

    fn store_gpu_fingerprint(&mut self, id: u16, buffer_size: u64, checkpoint_hash_hex: &str) {
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
}

/// Impl FusedKernels for Box<dyn FusedKernels + Send + Sync> to enable dynamic dispatch with explicit bounds
impl FusedKernels for Box<dyn FusedKernels + Send + Sync> {
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

    fn store_gpu_fingerprint(&mut self, id: u16, buffer_size: u64, checkpoint_hash_hex: &str) {
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
}

/// MPLoRA configuration for kernels
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

/// Extended kernel trait with MPLoRA support
pub trait MploraKernels: FusedKernels {
    /// Execute MPLoRA with shared downsample
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
