//! Kernel test utilities for Metal and CoreML backends
//!
//! This module provides shared testing utilities for Metal and CoreML kernel tests:
//! - Deterministic weight generation (SeededLcg)
//! - Mock adapter structures
//! - CPU reference implementations
//! - Tolerance-based comparison utilities
//! - Q15 quantization helpers
//! - Benchmarking utilities
//!
//! This consolidates 1,843 lines of duplicated code from:
//! - adapteros-lora-kernel-mtl/tests/test_utils.rs
//! - adapteros-lora-kernel-coreml/tests/test_utils.rs

use std::time::{Duration, Instant};

// =============================================================================
// Weight Generation (Seeded LCG for Deterministic Tests)
// =============================================================================

/// Linear Congruential Generator (LCG) for deterministic pseudo-random sequences
///
/// Uses the same constants as `rand::pcg` for compatibility with determinism tests.
/// Produces reproducible sequences across runs given the same seed.
pub struct SeededLcg {
    state: u64,
}

impl SeededLcg {
    /// Create a new LCG with the given seed
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Generate next pseudo-random u64
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }

    /// Generate next pseudo-random f32 in range [0, 1)
    pub fn next_f32(&mut self) -> f32 {
        let val = self.next_u64();
        (val >> 33) as f32 / (1u64 << 31) as f32
    }

    /// Generate next pseudo-random f32 in range [-scale, scale]
    pub fn next_f32_scaled(&mut self, scale: f32) -> f32 {
        self.next_f32() * 2.0 * scale - scale
    }
}

/// Generate deterministic pseudo-random weights using LCG
///
/// This function produces identical results given the same seed,
/// ensuring reproducible test data across runs.
pub fn deterministic_weights(size: usize, seed: u64) -> Vec<f32> {
    let mut rng = SeededLcg::new(seed);
    let mut weights = Vec::with_capacity(size);
    for _ in 0..size {
        weights.push(rng.next_f32_scaled(0.1));
    }
    weights
}

/// Generate Xavier-initialized weights (scaled normal-ish distribution)
pub fn xavier_weights(size: usize, fan_in: usize, fan_out: usize, seed: u64) -> Vec<f32> {
    let mut rng = SeededLcg::new(seed);
    let scale = (2.0 / (fan_in + fan_out) as f64).sqrt() as f32;
    let mut weights = Vec::with_capacity(size);
    for _ in 0..size {
        weights.push(rng.next_f32_scaled(scale));
    }
    weights
}

/// Generate uniform random weights in [min, max] range
pub fn uniform_weights(size: usize, min: f32, max: f32, seed: u64) -> Vec<f32> {
    let mut rng = SeededLcg::new(seed);
    let range = max - min;
    let mut weights = Vec::with_capacity(size);
    for _ in 0..size {
        weights.push(min + rng.next_f32() * range);
    }
    weights
}

// =============================================================================
// Mock Adapter Creation
// =============================================================================

/// Mock LoRA adapter weights for testing
#[derive(Debug, Clone)]
pub struct MockAdapter {
    /// Down-projection weights (A matrix): [rank x hidden]
    pub lora_a: Vec<f32>,
    /// Up-projection weights (B matrix): [hidden x rank]
    pub lora_b: Vec<f32>,
    /// LoRA rank
    pub rank: usize,
    /// Hidden dimension
    pub hidden: usize,
    /// Scaling factor alpha
    pub alpha: f32,
    /// Adapter ID
    pub id: u16,
}

impl MockAdapter {
    /// Create a new mock adapter with deterministic weights
    pub fn new(id: u16, hidden: usize, rank: usize, alpha: f32, seed: u64) -> Self {
        let lora_a = deterministic_weights(rank * hidden, seed);
        let lora_b = deterministic_weights(hidden * rank, seed.wrapping_add(1));
        Self {
            lora_a,
            lora_b,
            rank,
            hidden,
            alpha,
            id,
        }
    }

    /// Get the LoRA scaling factor (alpha / rank)
    pub fn scaling(&self) -> f32 {
        self.alpha / self.rank as f32
    }

    /// Serialize adapter weights to bytes (for testing load operations)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Simple serialization: [rank:4][hidden:4][alpha:4][lora_a...][lora_b...]
        bytes.extend(&(self.rank as u32).to_le_bytes());
        bytes.extend(&(self.hidden as u32).to_le_bytes());
        bytes.extend(&self.alpha.to_le_bytes());
        for val in &self.lora_a {
            bytes.extend(&val.to_le_bytes());
        }
        for val in &self.lora_b {
            bytes.extend(&val.to_le_bytes());
        }
        bytes
    }
}

/// Mock QKV attention weights
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

/// Mock MLP layer weights
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
    assert_eq!(a.len(), m * k, "Matrix A dimension mismatch");
    assert_eq!(b.len(), k * n, "Matrix B dimension mismatch");

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
/// output = base_weight @ input + (B @ (A @ input)) * (alpha / rank)
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

/// CPU softmax
pub fn cpu_softmax(input: &[f32]) -> Vec<f32> {
    let max_val = input.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp_sum: f32 = input.iter().map(|&x| (x - max_val).exp()).sum();
    input
        .iter()
        .map(|&x| (x - max_val).exp() / exp_sum)
        .collect()
}

/// CPU softmax along rows (2D tensor)
pub fn cpu_softmax_2d(input: &[f32], rows: usize, cols: usize) -> Vec<f32> {
    assert_eq!(input.len(), rows * cols);
    let mut output = Vec::with_capacity(input.len());
    for row in input.chunks(cols) {
        output.extend(cpu_softmax(row));
    }
    output
}

/// CPU RMS normalization
pub fn cpu_rms_norm(input: &[f32], epsilon: f32) -> Vec<f32> {
    let mean_sq: f32 = input.iter().map(|x| x * x).sum::<f32>() / input.len() as f32;
    let rms = (mean_sq + epsilon).sqrt();
    input.iter().map(|&x| x / rms).collect()
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

/// CPU element-wise add
pub fn cpu_add(a: &[f32], b: &[f32]) -> Vec<f32> {
    assert_eq!(a.len(), b.len());
    a.iter().zip(b.iter()).map(|(&x, &y)| x + y).collect()
}

/// CPU element-wise scale
pub fn cpu_scale(a: &[f32], factor: f32) -> Vec<f32> {
    a.iter().map(|&x| x * factor).collect()
}

// =============================================================================
// Comparison Utilities
// =============================================================================

/// Tolerance configuration for floating-point comparisons
#[derive(Clone)]
pub struct Tolerance {
    /// Absolute tolerance
    pub abs: f32,
    /// Relative tolerance
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
    /// Strict tolerance for determinism tests
    pub fn strict() -> Self {
        Self {
            abs: 1e-7,
            rel: 1e-6,
        }
    }

    /// Relaxed tolerance for numerical stability tests
    pub fn relaxed() -> Self {
        Self {
            abs: 1e-4,
            rel: 1e-3,
        }
    }

    /// Very relaxed tolerance for large matrix operations
    pub fn very_relaxed() -> Self {
        Self {
            abs: 1e-3,
            rel: 1e-2,
        }
    }
}

/// Result of vector comparison
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

/// Check approximate equality with tolerance
pub fn approx_eq(a: f32, b: f32, epsilon: f32) -> bool {
    (a - b).abs() < epsilon
}

// =============================================================================
// Benchmarking Utilities
// =============================================================================

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

    pub fn per_iteration_us(&self) -> f64 {
        self.start.elapsed().as_micros() as f64 / self.iterations as f64
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
    let mut rng = SeededLcg::new(seed);
    let mut gates = Vec::with_capacity(k);
    for _ in 0..k {
        let float_gate = rng.next_f32();
        gates.push(float_to_q15(float_gate));
    }
    gates
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
// Mock Model Plan
// =============================================================================

/// Create a mock model plan bytes for testing load operations
pub fn mock_model_plan(seed: u64) -> Vec<u8> {
    // Simple mock plan format for testing
    let mut plan = Vec::new();
    // Magic header
    plan.extend(b"AOS_PLAN");
    // Version
    plan.extend(&1u32.to_le_bytes());
    // Seed for reproducibility
    plan.extend(&seed.to_le_bytes());
    // Some padding to simulate a real plan
    plan.extend(vec![0u8; 1024]);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seeded_lcg_reproducibility() {
        let mut rng1 = SeededLcg::new(12345);
        let mut rng2 = SeededLcg::new(12345);

        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_deterministic_weights_reproducibility() {
        let w1 = deterministic_weights(1000, 42);
        let w2 = deterministic_weights(1000, 42);
        assert_eq!(w1, w2);

        let w3 = deterministic_weights(1000, 43);
        assert_ne!(w1, w3);
    }

    #[test]
    fn test_cpu_matmul() {
        // 2x3 @ 3x2 = 2x2
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let b = vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0];
        let c = cpu_matmul(&a, &b, 2, 3, 2);
        // [1,2,3] @ [7,8; 9,10; 11,12] = [58, 64]
        // [4,5,6] @ [7,8; 9,10; 11,12] = [139, 154]
        assert_eq!(c, vec![58.0, 64.0, 139.0, 154.0]);
    }

    #[test]
    fn test_cpu_softmax() {
        let input = vec![1.0, 2.0, 3.0, 4.0];
        let output = cpu_softmax(&input);
        let sum: f32 = output.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        // Check monotonicity
        assert!(output[3] > output[2]);
        assert!(output[2] > output[1]);
        assert!(output[1] > output[0]);
    }

    #[test]
    fn test_q15_conversion() {
        assert_eq!(float_to_q15(0.0), 0);
        assert_eq!(float_to_q15(1.0), 32767);
        assert_eq!(float_to_q15(-1.0), -32767);
        assert!((q15_to_float(32767) - 0.99997).abs() < 0.001);
    }

    #[test]
    fn test_tolerance_comparison() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![1.0, 2.0, 3.0, 4.0];
        let result = compare_vectors(&a, &b, &Tolerance::default());
        assert!(result.passed);

        let c = vec![1.0, 2.001, 3.0, 4.0];
        let result = compare_vectors(&a, &c, &Tolerance::strict());
        assert!(!result.passed);
    }

    #[test]
    fn test_mock_adapter_creation() {
        let adapter = MockAdapter::new(1, 768, 16, 32.0, 12345);
        assert_eq!(adapter.rank, 16);
        assert_eq!(adapter.hidden, 768);
        assert_eq!(adapter.lora_a.len(), 16 * 768);
        assert_eq!(adapter.lora_b.len(), 768 * 16);
        assert!((adapter.scaling() - 2.0).abs() < 1e-6);
    }
}
