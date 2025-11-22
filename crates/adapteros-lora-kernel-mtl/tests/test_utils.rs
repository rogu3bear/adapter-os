//! Shared test utilities for Metal kernel tests
//!
//! This module provides reusable utilities for testing Metal kernels including:
//! - Mock weight generation
//! - CPU reference implementations
//! - Comparison helpers
//! - Test fixtures

#![cfg(target_os = "macos")]

use metal::{Buffer, Device, MTLResourceOptions};
use std::sync::Arc;

// =============================================================================
// Core Test Fixture
// =============================================================================

/// Test fixture for Metal kernel testing
pub struct MetalTestContext {
    pub device: Arc<Device>,
    pub queue: metal::CommandQueue,
}

impl MetalTestContext {
    pub fn new() -> Self {
        let device = Device::system_default().expect("Metal device required for tests");
        let queue = device.new_command_queue();
        Self {
            device: Arc::new(device),
            queue,
        }
    }

    /// Create buffer with f32 data
    pub fn buffer_f32(&self, data: &[f32]) -> Buffer {
        self.device.new_buffer_with_data(
            data.as_ptr() as *const _,
            (data.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Create buffer with u32 data
    pub fn buffer_u32(&self, data: &[u32]) -> Buffer {
        self.device.new_buffer_with_data(
            data.as_ptr() as *const _,
            (data.len() * std::mem::size_of::<u32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Create empty buffer for given number of f32 elements
    pub fn empty_f32(&self, count: usize) -> Buffer {
        self.device.new_buffer(
            (count * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Read f32 values from buffer
    pub fn read_f32(&self, buffer: &Buffer, count: usize) -> Vec<f32> {
        unsafe {
            let ptr = buffer.contents() as *const f32;
            std::slice::from_raw_parts(ptr, count).to_vec()
        }
    }

    /// Create a constant buffer with a single value
    pub fn constant_f32(&self, value: f32) -> Buffer {
        self.device.new_buffer_with_data(
            &value as *const f32 as *const _,
            std::mem::size_of::<f32>() as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Create a constant buffer with a single u32 value
    pub fn constant_u32(&self, value: u32) -> Buffer {
        self.device.new_buffer_with_data(
            &value as *const u32 as *const _,
            std::mem::size_of::<u32>() as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }
}

impl Default for MetalTestContext {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Weight Generation
// =============================================================================

/// Generate deterministic pseudo-random weights using LCG
pub fn deterministic_weights(size: usize, seed: u64) -> Vec<f32> {
    let mut weights = Vec::with_capacity(size);
    let mut state = seed;
    for _ in 0..size {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let normalized = ((state >> 33) as f64) / (1u64 << 31) as f64;
        weights.push((normalized * 0.2 - 0.1) as f32);
    }
    weights
}

/// Generate weights from normal distribution (Xavier initialization)
pub fn xavier_weights(size: usize, fan_in: usize, fan_out: usize, seed: u64) -> Vec<f32> {
    let mut weights = Vec::with_capacity(size);
    let mut state = seed;
    let scale = (2.0 / (fan_in + fan_out) as f64).sqrt();

    for _ in 0..size {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let normalized = ((state >> 33) as f64) / (1u64 << 31) as f64;
        weights.push((normalized * 2.0 - 1.0) as f32 * scale as f32);
    }
    weights
}

/// Mock LoRA adapter weights
pub struct MockAdapter {
    pub lora_a: Vec<f32>,
    pub lora_b: Vec<f32>,
    pub rank: usize,
    pub hidden: usize,
    pub alpha: f32,
}

impl MockAdapter {
    pub fn new(hidden: usize, rank: usize, alpha: f32, seed: u64) -> Self {
        let lora_a = deterministic_weights(rank * hidden, seed);
        let lora_b = deterministic_weights(hidden * rank, seed.wrapping_add(1));
        Self {
            lora_a,
            lora_b,
            rank,
            hidden,
            alpha,
        }
    }

    /// Get scaling factor (alpha / rank)
    pub fn scaling(&self) -> f32 {
        self.alpha / self.rank as f32
    }
}

/// Generate mock QKV weights for testing attention
pub struct MockQkvWeights {
    pub q_weight: Vec<f32>,
    pub k_weight: Vec<f32>,
    pub v_weight: Vec<f32>,
    pub hidden_size: usize,
    pub num_heads: usize,
    pub head_dim: usize,
}

impl MockQkvWeights {
    pub fn new(hidden_size: usize, num_heads: usize, head_dim: usize, seed: u64) -> Self {
        let qkv_size = hidden_size * hidden_size;
        Self {
            q_weight: deterministic_weights(qkv_size, seed),
            k_weight: deterministic_weights(qkv_size, seed.wrapping_add(1)),
            v_weight: deterministic_weights(qkv_size, seed.wrapping_add(2)),
            hidden_size,
            num_heads,
            head_dim,
        }
    }
}

/// Generate mock MLP weights
pub struct MockMlpWeights {
    pub gate_weight: Vec<f32>,
    pub up_weight: Vec<f32>,
    pub down_weight: Vec<f32>,
    pub hidden_size: usize,
    pub intermediate_size: usize,
}

impl MockMlpWeights {
    pub fn new(hidden_size: usize, intermediate_size: usize, seed: u64) -> Self {
        Self {
            gate_weight: deterministic_weights(hidden_size * intermediate_size, seed),
            up_weight: deterministic_weights(hidden_size * intermediate_size, seed.wrapping_add(1)),
            down_weight: deterministic_weights(
                intermediate_size * hidden_size,
                seed.wrapping_add(2),
            ),
            hidden_size,
            intermediate_size,
        }
    }
}

// =============================================================================
// CPU Reference Implementations
// =============================================================================

/// CPU matrix multiplication: C = A @ B
/// A: [m x k], B: [k x n] -> C: [m x n]
pub fn cpu_matmul(a: &[f32], b: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
    let mut c = vec![0.0f32; m * n];
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for p in 0..k {
                sum += a[i * k + p] * b[p * n + j];
            }
            c[i * n + j] = sum;
        }
    }
    c
}

/// CPU LoRA forward pass
pub fn cpu_lora_forward(
    input: &[f32],
    base_weight: &[f32],
    lora_a: &[f32],
    lora_b: &[f32],
    in_size: usize,
    out_size: usize,
    rank: usize,
    alpha: f32,
) -> Vec<f32> {
    // Base: output = base_weight @ input
    let mut output = vec![0.0f32; out_size];
    for i in 0..out_size {
        for j in 0..in_size {
            output[i] += base_weight[i * in_size + j] * input[j];
        }
    }

    // LoRA: intermediate = A @ input
    let mut intermediate = vec![0.0f32; rank];
    for r in 0..rank {
        for h in 0..in_size {
            intermediate[r] += lora_a[r * in_size + h] * input[h];
        }
    }

    // LoRA: output += (B @ intermediate) * (alpha / rank)
    let scaling = alpha / rank as f32;
    for i in 0..out_size {
        let mut lora_out = 0.0f32;
        for r in 0..rank {
            lora_out += lora_b[i * rank + r] * intermediate[r];
        }
        output[i] += lora_out * scaling;
    }

    output
}

/// CPU SwiGLU activation
pub fn cpu_swiglu(gate: &[f32], up: &[f32]) -> Vec<f32> {
    gate.iter()
        .zip(up.iter())
        .map(|(&g, &u)| {
            let silu = g / (1.0 + (-g).exp());
            silu * u
        })
        .collect()
}

/// CPU softmax
pub fn cpu_softmax(input: &[f32]) -> Vec<f32> {
    let max_val = input.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_sum: f32 = input.iter().map(|&x| (x - max_val).exp()).sum();
    input
        .iter()
        .map(|&x| (x - max_val).exp() / exp_sum)
        .collect()
}

/// CPU RMS normalization
pub fn cpu_rms_norm(input: &[f32], epsilon: f32) -> Vec<f32> {
    let mean_sq: f32 = input.iter().map(|x| x * x).sum::<f32>() / input.len() as f32;
    let rms = (mean_sq + epsilon).sqrt();
    input.iter().map(|&x| x / rms).collect()
}

// =============================================================================
// Comparison Utilities
// =============================================================================

/// Tolerance configuration for comparisons
#[derive(Clone)]
pub struct Tolerance {
    pub abs: f32,
    pub rel: f32,
}

impl Default for Tolerance {
    fn default() -> Self {
        Self {
            abs: 1e-6,
            rel: 1e-5,
        }
    }
}

impl Tolerance {
    pub fn strict() -> Self {
        Self {
            abs: 1e-7,
            rel: 1e-6,
        }
    }

    pub fn relaxed() -> Self {
        Self {
            abs: 1e-4,
            rel: 1e-3,
        }
    }
}

/// Comparison result with error statistics
#[derive(Debug)]
pub struct ComparisonResult {
    pub passed: bool,
    pub max_error: f32,
    pub mean_error: f32,
    pub l2_error: f32,
    pub error_indices: Vec<usize>,
}

/// Compare two vectors with specified tolerance
pub fn compare_vectors(expected: &[f32], actual: &[f32], tol: &Tolerance) -> ComparisonResult {
    if expected.len() != actual.len() {
        return ComparisonResult {
            passed: false,
            max_error: f32::INFINITY,
            mean_error: f32::INFINITY,
            l2_error: f32::INFINITY,
            error_indices: vec![],
        };
    }

    let mut max_error = 0.0f32;
    let mut sum_error = 0.0f32;
    let mut l2_sum = 0.0f32;
    let mut error_indices = Vec::new();

    for (i, (&e, &a)) in expected.iter().zip(actual.iter()).enumerate() {
        let diff = (e - a).abs();
        let allowed = tol.abs + e.abs() * tol.rel;

        if diff > max_error {
            max_error = diff;
        }
        sum_error += diff;
        l2_sum += diff * diff;

        if diff > allowed {
            error_indices.push(i);
        }
    }

    let n = expected.len() as f32;
    ComparisonResult {
        passed: error_indices.is_empty(),
        max_error,
        mean_error: sum_error / n,
        l2_error: l2_sum.sqrt(),
        error_indices,
    }
}

/// Assert vectors are equal within tolerance
pub fn assert_vectors_eq(expected: &[f32], actual: &[f32], tol: &Tolerance, msg: &str) {
    let result = compare_vectors(expected, actual, tol);
    if !result.passed {
        panic!(
            "{}: {} errors found. Max error: {}, Mean error: {}, L2 error: {}. First errors at: {:?}",
            msg,
            result.error_indices.len(),
            result.max_error,
            result.mean_error,
            result.l2_error,
            &result.error_indices[..result.error_indices.len().min(5)]
        );
    }
}

/// Check bit-exact equality (for determinism tests)
pub fn assert_bit_exact(a: &[f32], b: &[f32], msg: &str) {
    assert_eq!(a.len(), b.len(), "{}: length mismatch", msg);
    for (i, (&av, &bv)) in a.iter().zip(b.iter()).enumerate() {
        assert!(
            av.to_bits() == bv.to_bits(),
            "{}: bit mismatch at index {}: {} ({:#x}) vs {} ({:#x})",
            msg,
            i,
            av,
            av.to_bits(),
            bv,
            bv.to_bits()
        );
    }
}

// =============================================================================
// Benchmarking Utilities
// =============================================================================

use std::time::{Duration, Instant};

/// Simple benchmark timer
pub struct BenchTimer {
    start: Instant,
    iterations: usize,
}

impl BenchTimer {
    pub fn new(iterations: usize) -> Self {
        Self {
            start: Instant::now(),
            iterations,
        }
    }

    pub fn restart(&mut self) {
        self.start = Instant::now();
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn per_iteration(&self) -> Duration {
        self.start.elapsed() / self.iterations as u32
    }

    pub fn ops_per_second(&self, ops_per_iter: usize) -> f64 {
        let total_ops = ops_per_iter * self.iterations;
        total_ops as f64 / self.start.elapsed().as_secs_f64()
    }
}

/// Print benchmark results in a formatted table
pub fn print_bench_result(name: &str, time_us: f64, throughput: Option<f64>) {
    if let Some(tp) = throughput {
        println!("{:<30} {:>10.2} us {:>12.2} MB/s", name, time_us, tp);
    } else {
        println!("{:<30} {:>10.2} us", name, time_us);
    }
}

// =============================================================================
// Test Data Generators
// =============================================================================

/// Generate a test input sequence (like token embeddings)
pub fn test_input_sequence(seq_len: usize, hidden_size: usize, seed: u64) -> Vec<f32> {
    deterministic_weights(seq_len * hidden_size, seed)
}

/// Generate attention mask (1.0 for valid, 0.0 for padding)
pub fn test_attention_mask(seq_len: usize, valid_len: usize) -> Vec<f32> {
    let mut mask = vec![0.0f32; seq_len];
    for i in 0..valid_len.min(seq_len) {
        mask[i] = 1.0;
    }
    mask
}

/// Generate position IDs for RoPE
pub fn test_position_ids(seq_len: usize) -> Vec<u32> {
    (0..seq_len as u32).collect()
}

// =============================================================================
// Q15 Quantization Helpers
// =============================================================================

/// Convert float gate weight to Q15 fixed-point
pub fn float_to_q15(value: f32) -> i16 {
    (value.clamp(-1.0, 1.0) * 32767.0) as i16
}

/// Convert Q15 fixed-point to float
pub fn q15_to_float(value: i16) -> f32 {
    value as f32 / 32768.0
}

/// Generate Q15 gate weights for K adapters
pub fn test_q15_gates(k: usize, seed: u64) -> Vec<i16> {
    let mut state = seed;
    let mut gates = Vec::with_capacity(k);

    for _ in 0..k {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let normalized = ((state >> 33) as f64) / (1u64 << 31) as f64;
        let float_gate = normalized as f32;
        gates.push(float_to_q15(float_gate));
    }

    gates
}
