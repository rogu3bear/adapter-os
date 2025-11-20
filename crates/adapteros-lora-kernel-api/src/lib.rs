//! Safe API for fused kernels

use adapteros_core::Result;

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
                panic!("RouterRing: K exceeds max (8), got {}", indices.len());
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
pub trait FusedKernels: Send + Sync + 'static {
    /// Load plan and weights
    fn load(&mut self, plan_bytes: &[u8]) -> Result<()>;

    /// Run a single token step
    fn run_step(&mut self, ring: &RouterRing, io: &mut IoBuffers) -> Result<()>;

    /// Get device information
    fn device_info(&self) -> String;

    /// Execute compression kernel
    fn execute_compression(
        &mut self,
        input: &[f32],
        output: &mut [f32],
        config: &MploraConfig,
    ) -> Result<()>;
}

/// Configuration for MPLORA operations
#[derive(Debug, Clone)]
pub struct MploraConfig {
    pub rank: usize,
    pub alpha: f32,
    pub max_seq_len: usize,
    pub vocab_size: usize,
}

