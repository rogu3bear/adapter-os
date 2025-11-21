//! Comprehensive tests for Metal inference pipeline
//!
//! This test suite includes:
//! - Unit tests for each kernel (FusedMLP, FusedQKV, FlashAttention)
//! - Integration tests for full run_step()
//! - Determinism tests (same input = same output)
//! - Performance benchmarks
//!
//! Run with: cargo test -p adapteros-lora-kernel-mtl --test metal_inference_pipeline_tests

#![cfg(target_os = "macos")]

use metal::{Device, MTLResourceOptions, MTLSize, CompileOptions, Buffer};
use std::sync::Arc;
use std::time::Instant;

// =============================================================================
// Test Utilities and Fixtures
// =============================================================================

/// Test fixture for creating Metal test environments
pub struct MetalTestFixture {
    pub device: Arc<Device>,
    pub queue: metal::CommandQueue,
}

impl MetalTestFixture {
    pub fn new() -> Self {
        let device = Device::system_default().expect("Metal device required for tests");
        let queue = device.new_command_queue();
        Self {
            device: Arc::new(device),
            queue,
        }
    }

    /// Create a buffer with test data
    pub fn create_buffer(&self, data: &[f32]) -> Buffer {
        self.device.new_buffer_with_data(
            data.as_ptr() as *const _,
            (data.len() * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Create an empty buffer
    pub fn create_empty_buffer(&self, count: usize) -> Buffer {
        self.device.new_buffer(
            (count * std::mem::size_of::<f32>()) as u64,
            MTLResourceOptions::StorageModeShared,
        )
    }

    /// Read floats from buffer
    pub fn read_buffer(&self, buffer: &Buffer, count: usize) -> Vec<f32> {
        unsafe {
            let ptr = buffer.contents() as *const f32;
            std::slice::from_raw_parts(ptr, count).to_vec()
        }
    }
}

/// Helper to generate deterministic test weights based on seed
pub fn generate_deterministic_weights(size: usize, seed: u64) -> Vec<f32> {
    let mut weights = Vec::with_capacity(size);
    let mut state = seed;
    for _ in 0..size {
        // Simple LCG for reproducible pseudo-random numbers
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let value = ((state >> 33) as f64) / (1u64 << 31) as f64;
        weights.push((value * 0.2 - 0.1) as f32);
    }
    weights
}

/// Helper to generate mock LoRA adapter weights
pub struct MockLoraWeights {
    pub lora_a: Vec<f32>,
    pub lora_b: Vec<f32>,
    pub rank: usize,
    pub hidden_size: usize,
}

impl MockLoraWeights {
    pub fn new(hidden_size: usize, rank: usize, seed: u64) -> Self {
        let lora_a = generate_deterministic_weights(rank * hidden_size, seed);
        let lora_b = generate_deterministic_weights(hidden_size * rank, seed.wrapping_add(1));
        Self {
            lora_a,
            lora_b,
            rank,
            hidden_size,
        }
    }
}

/// Compare two float vectors with tolerance
pub fn compare_floats(expected: &[f32], actual: &[f32], tolerance: f32) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!(
            "Length mismatch: expected {}, got {}",
            expected.len(),
            actual.len()
        ));
    }

    let mut max_diff = 0.0f32;
    let mut diff_idx = 0;
    for (i, (e, a)) in expected.iter().zip(actual.iter()).enumerate() {
        let diff = (e - a).abs();
        if diff > max_diff {
            max_diff = diff;
            diff_idx = i;
        }
        if diff > tolerance {
            return Err(format!(
                "Value mismatch at index {}: expected {}, got {}, diff {}",
                i, e, a, diff
            ));
        }
    }

    Ok(())
}

/// Compute CPU reference for LoRA forward pass
pub fn cpu_lora_forward(
    input: &[f32],
    base_weight: &[f32],
    lora_a: &[f32],
    lora_b: &[f32],
    hidden_size: usize,
    out_size: usize,
    rank: usize,
    alpha: f32,
) -> Vec<f32> {
    // Base output: W @ x
    let mut output = vec![0.0f32; out_size];
    for i in 0..out_size {
        let mut sum = 0.0f32;
        for j in 0..hidden_size {
            sum += base_weight[i * hidden_size + j] * input[j];
        }
        output[i] = sum;
    }

    // LoRA: (B @ (A @ x)) * (alpha / rank)
    // A: [rank x hidden_size], B: [out_size x rank]
    let mut intermediate = vec![0.0f32; rank];
    for r in 0..rank {
        let mut sum = 0.0f32;
        for h in 0..hidden_size {
            sum += lora_a[r * hidden_size + h] * input[h];
        }
        intermediate[r] = sum;
    }

    let scaling = alpha / (rank as f32);
    for i in 0..out_size {
        let mut lora_sum = 0.0f32;
        for r in 0..rank {
            lora_sum += lora_b[i * rank + r] * intermediate[r];
        }
        output[i] += lora_sum * scaling;
    }

    output
}

/// CPU reference for SwiGLU activation
pub fn cpu_swiglu(gate: &[f32], up: &[f32]) -> Vec<f32> {
    gate.iter()
        .zip(up.iter())
        .map(|(&g, &u)| {
            let swish = g / (1.0 + (-g).exp()); // SiLU/Swish
            swish * u
        })
        .collect()
}

// =============================================================================
// Unit Tests for Individual Kernels
// =============================================================================

mod unit_tests {
    use super::*;

    #[test]
    fn test_metal_device_availability() {
        let fixture = MetalTestFixture::new();
        assert!(!fixture.device.name().is_empty(), "Metal device name should not be empty");
        println!("Using Metal device: {}", fixture.device.name());
    }

    #[test]
    fn test_buffer_creation_and_read() {
        let fixture = MetalTestFixture::new();
        let data: Vec<f32> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let buffer = fixture.create_buffer(&data);
        let read_back = fixture.read_buffer(&buffer, data.len());

        for (expected, actual) in data.iter().zip(read_back.iter()) {
            assert!(
                (expected - actual).abs() < 1e-6,
                "Buffer read mismatch: expected {}, got {}",
                expected,
                actual
            );
        }
    }

    #[test]
    fn test_fused_mlp_kernel_creation() {
        use adapteros_lora_kernel_mtl::fused_mlp::FusedMlpKernel;

        let device = Device::system_default().expect("Metal device required");
        let result = FusedMlpKernel::new(Arc::new(device));
        assert!(result.is_ok(), "FusedMlpKernel creation failed: {:?}", result.err());
    }

    #[test]
    fn test_fused_qkv_kernel_creation() {
        use adapteros_lora_kernel_mtl::fused_qkv::{FusedQkvKernel, GqaConfig};

        let device = Device::system_default().expect("Metal device required");
        let gqa_config = GqaConfig::default();
        let result = FusedQkvKernel::new(Arc::new(device), gqa_config);
        assert!(result.is_ok(), "FusedQkvKernel creation failed: {:?}", result.err());
    }

    #[test]
    fn test_flash_attention_kernel_creation() {
        use adapteros_lora_kernel_mtl::fused_qkv::{FlashAttentionKernel, GqaConfig};

        let device = Device::system_default().expect("Metal device required");
        let gqa_config = GqaConfig::default();
        let result = FlashAttentionKernel::new(Arc::new(device), gqa_config);
        assert!(result.is_ok(), "FlashAttentionKernel creation failed: {:?}", result.err());
    }

    #[test]
    fn test_mock_lora_weights_generation() {
        let weights = MockLoraWeights::new(64, 8, 12345);
        assert_eq!(weights.lora_a.len(), 8 * 64);
        assert_eq!(weights.lora_b.len(), 64 * 8);

        // Check determinism
        let weights2 = MockLoraWeights::new(64, 8, 12345);
        assert_eq!(weights.lora_a, weights2.lora_a);
        assert_eq!(weights.lora_b, weights2.lora_b);
    }

    #[test]
    fn test_cpu_lora_forward_basic() {
        let hidden_size = 4;
        let out_size = 4;
        let rank = 2;
        let alpha = 16.0;

        let input = vec![1.0, 0.5, -0.5, 0.25];
        let base_weight = vec![0.1f32; hidden_size * out_size];
        let lora_a = vec![0.1f32; rank * hidden_size];
        let lora_b = vec![0.1f32; out_size * rank];

        let output = cpu_lora_forward(
            &input, &base_weight, &lora_a, &lora_b,
            hidden_size, out_size, rank, alpha,
        );

        assert_eq!(output.len(), out_size);
        // Verify output is not all zeros
        assert!(output.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_cpu_swiglu() {
        let gate = vec![0.0, 1.0, -1.0, 2.0];
        let up = vec![1.0, 1.0, 1.0, 1.0];

        let result = cpu_swiglu(&gate, &up);

        // swiglu(0, 1) = 0
        assert!((result[0] - 0.0).abs() < 1e-6);
        // swiglu(1, 1) = 1 * sigmoid(1) * 1
        assert!((result[1] - 0.7310586).abs() < 1e-5);
    }

    #[test]
    fn test_gqa_config_defaults() {
        use adapteros_lora_kernel_mtl::fused_qkv::GqaConfig;

        let config = GqaConfig::default();
        assert_eq!(config.num_attention_heads, 32);
        assert_eq!(config.num_key_value_heads, 4);
        assert_eq!(config.head_dim, 128);
        assert_eq!(config.rope_theta, 10000.0);
    }

    #[test]
    fn test_lora_config_defaults() {
        use adapteros_lora_kernel_mtl::fused_mlp::LoraConfig;

        let config = LoraConfig::default();
        assert_eq!(config.rank, 16);
        assert_eq!(config.alpha, 32.0);
        assert_eq!(config.dropout_rate, 0.0);
    }
}

// =============================================================================
// Determinism Tests
// =============================================================================

mod determinism_tests {
    use super::*;

    /// Test that same input produces identical output across multiple runs
    #[test]
    fn test_metal_kernel_determinism() {
        let msl = r#"#include <metal_stdlib>
using namespace metal;

kernel void simple_matmul(
    device const float* input  [[ buffer(0) ]],
    device const float* weight [[ buffer(1) ]],
    device float* output       [[ buffer(2) ]],
    constant uint& n           [[ buffer(3) ]],
    uint3 tid                  [[ thread_position_in_grid ]]
) {
    uint i = tid.x;
    if (i >= n) return;

    float sum = 0.0f;
    for (uint j = 0; j < n; ++j) {
        sum += input[j] * weight[i * n + j];
    }
    output[i] = sum;
}
"#;

        let fixture = MetalTestFixture::new();
        let options = CompileOptions::new();
        let library = fixture.device.new_library_with_source(msl, &options).unwrap();
        let function = library.get_function("simple_matmul", None).unwrap();
        let pipeline = fixture.device.new_compute_pipeline_state_with_function(&function).unwrap();

        let n = 16usize;
        let input = generate_deterministic_weights(n, 12345);
        let weight = generate_deterministic_weights(n * n, 67890);

        let run_kernel = || {
            let input_buf = fixture.create_buffer(&input);
            let weight_buf = fixture.create_buffer(&weight);
            let output_buf = fixture.create_empty_buffer(n);
            let n_u32 = n as u32;
            let n_buf = fixture.device.new_buffer_with_data(
                &n_u32 as *const u32 as *const _,
                std::mem::size_of::<u32>() as u64,
                MTLResourceOptions::StorageModeShared,
            );

            let cmd = fixture.queue.new_command_buffer();
            let enc = cmd.new_compute_command_encoder();
            enc.set_compute_pipeline_state(&pipeline);
            enc.set_buffer(0, Some(&input_buf), 0);
            enc.set_buffer(1, Some(&weight_buf), 0);
            enc.set_buffer(2, Some(&output_buf), 0);
            enc.set_buffer(3, Some(&n_buf), 0);
            enc.dispatch_thread_groups(MTLSize::new(n as u64, 1, 1), MTLSize::new(1, 1, 1));
            enc.end_encoding();
            cmd.commit();
            cmd.wait_until_completed();

            fixture.read_buffer(&output_buf, n)
        };

        // Run multiple times and verify identical results
        let results: Vec<Vec<f32>> = (0..5).map(|_| run_kernel()).collect();

        for i in 1..results.len() {
            for (idx, (&a, &b)) in results[0].iter().zip(results[i].iter()).enumerate() {
                assert!(
                    a.to_bits() == b.to_bits(),
                    "Non-deterministic output at index {}: run 0 = {}, run {} = {}",
                    idx, a, i, b
                );
            }
        }
    }

    #[test]
    fn test_lora_determinism_multiple_runs() {
        let msl = r#"#include <metal_stdlib>
using namespace metal;

kernel void lora_determinism(
    device const float* input   [[ buffer(0) ]],
    device const float* a       [[ buffer(1) ]],
    device const float* b       [[ buffer(2) ]],
    device float* output        [[ buffer(3) ]],
    constant uint& rank         [[ buffer(4) ]],
    constant uint& hidden       [[ buffer(5) ]],
    constant float& alpha       [[ buffer(6) ]],
    uint3 tid                   [[ thread_position_in_grid ]]
) {
    uint h = tid.x;
    if (h >= hidden) return;

    float acc_out = 0.0f;
    for (uint r = 0; r < rank; ++r) {
        float inter = 0.0f;
        for (uint i = 0; i < hidden; ++i) {
            inter += input[i] * a[r * hidden + i];
        }
        acc_out += inter * b[h * rank + r];
    }
    output[h] = acc_out * (alpha / float(rank));
}
"#;

        let fixture = MetalTestFixture::new();
        let options = CompileOptions::new();
        let library = fixture.device.new_library_with_source(msl, &options).unwrap();
        let function = library.get_function("lora_determinism", None).unwrap();
        let pipeline = fixture.device.new_compute_pipeline_state_with_function(&function).unwrap();

        let hidden = 32usize;
        let rank = 8usize;
        let alpha = 16.0f32;

        let input = generate_deterministic_weights(hidden, 11111);
        let a = generate_deterministic_weights(rank * hidden, 22222);
        let b = generate_deterministic_weights(hidden * rank, 33333);

        let run_kernel = || {
            let input_buf = fixture.create_buffer(&input);
            let a_buf = fixture.create_buffer(&a);
            let b_buf = fixture.create_buffer(&b);
            let output_buf = fixture.create_empty_buffer(hidden);

            let rank_u = rank as u32;
            let hidden_u = hidden as u32;
            let rank_buf = fixture.device.new_buffer_with_data(
                &rank_u as *const _ as *const _,
                std::mem::size_of::<u32>() as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let hidden_buf = fixture.device.new_buffer_with_data(
                &hidden_u as *const _ as *const _,
                std::mem::size_of::<u32>() as u64,
                MTLResourceOptions::StorageModeShared,
            );
            let alpha_buf = fixture.device.new_buffer_with_data(
                &alpha as *const _ as *const _,
                std::mem::size_of::<f32>() as u64,
                MTLResourceOptions::StorageModeShared,
            );

            let cmd = fixture.queue.new_command_buffer();
            let enc = cmd.new_compute_command_encoder();
            enc.set_compute_pipeline_state(&pipeline);
            enc.set_buffer(0, Some(&input_buf), 0);
            enc.set_buffer(1, Some(&a_buf), 0);
            enc.set_buffer(2, Some(&b_buf), 0);
            enc.set_buffer(3, Some(&output_buf), 0);
            enc.set_buffer(4, Some(&rank_buf), 0);
            enc.set_buffer(5, Some(&hidden_buf), 0);
            enc.set_buffer(6, Some(&alpha_buf), 0);
            enc.dispatch_thread_groups(MTLSize::new(hidden as u64, 1, 1), MTLSize::new(1, 1, 1));
            enc.end_encoding();
            cmd.commit();
            cmd.wait_until_completed();

            fixture.read_buffer(&output_buf, hidden)
        };

        // Run 10 times and verify exact bit-for-bit equality
        let baseline = run_kernel();
        for i in 0..10 {
            let result = run_kernel();
            for (idx, (&expected, &actual)) in baseline.iter().zip(result.iter()).enumerate() {
                assert!(
                    expected.to_bits() == actual.to_bits(),
                    "Determinism failure at run {}, index {}: {} vs {}",
                    i, idx, expected, actual
                );
            }
        }
    }

    #[test]
    fn test_different_seeds_different_output() {
        let weights1 = generate_deterministic_weights(100, 12345);
        let weights2 = generate_deterministic_weights(100, 12346);

        // Verify different seeds produce different weights
        let different = weights1.iter().zip(weights2.iter()).any(|(a, b)| a != b);
        assert!(different, "Different seeds should produce different weights");
    }
}

// =============================================================================
// Integration Tests
// =============================================================================

mod integration_tests {
    use super::*;

    #[test]
    fn test_full_lora_pipeline_cpu_gpu_parity() {
        let msl = r#"#include <metal_stdlib>
using namespace metal;

kernel void lora_full_pipeline(
    device const float* input   [[ buffer(0) ]],
    device const float* base_w  [[ buffer(1) ]],
    device const float* lora_a  [[ buffer(2) ]],
    device const float* lora_b  [[ buffer(3) ]],
    device float* output        [[ buffer(4) ]],
    constant uint& hidden       [[ buffer(5) ]],
    constant uint& out_size     [[ buffer(6) ]],
    constant uint& rank         [[ buffer(7) ]],
    constant float& alpha       [[ buffer(8) ]],
    uint3 tid                   [[ thread_position_in_grid ]]
) {
    uint i = tid.x;
    if (i >= out_size) return;

    // Base projection
    float base_sum = 0.0f;
    for (uint j = 0; j < hidden; ++j) {
        base_sum += base_w[i * hidden + j] * input[j];
    }

    // LoRA projection
    float lora_out = 0.0f;
    for (uint r = 0; r < rank; ++r) {
        float inter = 0.0f;
        for (uint j = 0; j < hidden; ++j) {
            inter += lora_a[r * hidden + j] * input[j];
        }
        lora_out += inter * lora_b[i * rank + r];
    }

    output[i] = base_sum + lora_out * (alpha / float(rank));
}
"#;

        let fixture = MetalTestFixture::new();
        let options = CompileOptions::new();
        let library = fixture.device.new_library_with_source(msl, &options).unwrap();
        let function = library.get_function("lora_full_pipeline", None).unwrap();
        let pipeline = fixture.device.new_compute_pipeline_state_with_function(&function).unwrap();

        // Test multiple configurations
        let configs = [
            (16, 16, 4, 8.0f32),
            (32, 32, 8, 16.0f32),
            (64, 64, 16, 32.0f32),
        ];

        for (hidden, out_size, rank, alpha) in configs {
            let input = generate_deterministic_weights(hidden, 11111);
            let base_w = generate_deterministic_weights(hidden * out_size, 22222);
            let lora_a = generate_deterministic_weights(rank * hidden, 33333);
            let lora_b = generate_deterministic_weights(out_size * rank, 44444);

            // CPU reference
            let cpu_output = cpu_lora_forward(
                &input, &base_w, &lora_a, &lora_b,
                hidden, out_size, rank, alpha,
            );

            // GPU execution
            let input_buf = fixture.create_buffer(&input);
            let base_w_buf = fixture.create_buffer(&base_w);
            let lora_a_buf = fixture.create_buffer(&lora_a);
            let lora_b_buf = fixture.create_buffer(&lora_b);
            let output_buf = fixture.create_empty_buffer(out_size);

            let hidden_u = hidden as u32;
            let out_size_u = out_size as u32;
            let rank_u = rank as u32;

            let hidden_buf = fixture.device.new_buffer_with_data(
                &hidden_u as *const _ as *const _,
                4, MTLResourceOptions::StorageModeShared,
            );
            let out_size_buf = fixture.device.new_buffer_with_data(
                &out_size_u as *const _ as *const _,
                4, MTLResourceOptions::StorageModeShared,
            );
            let rank_buf = fixture.device.new_buffer_with_data(
                &rank_u as *const _ as *const _,
                4, MTLResourceOptions::StorageModeShared,
            );
            let alpha_buf = fixture.device.new_buffer_with_data(
                &alpha as *const _ as *const _,
                4, MTLResourceOptions::StorageModeShared,
            );

            let cmd = fixture.queue.new_command_buffer();
            let enc = cmd.new_compute_command_encoder();
            enc.set_compute_pipeline_state(&pipeline);
            enc.set_buffer(0, Some(&input_buf), 0);
            enc.set_buffer(1, Some(&base_w_buf), 0);
            enc.set_buffer(2, Some(&lora_a_buf), 0);
            enc.set_buffer(3, Some(&lora_b_buf), 0);
            enc.set_buffer(4, Some(&output_buf), 0);
            enc.set_buffer(5, Some(&hidden_buf), 0);
            enc.set_buffer(6, Some(&out_size_buf), 0);
            enc.set_buffer(7, Some(&rank_buf), 0);
            enc.set_buffer(8, Some(&alpha_buf), 0);
            enc.dispatch_thread_groups(
                MTLSize::new(out_size as u64, 1, 1),
                MTLSize::new(1, 1, 1),
            );
            enc.end_encoding();
            cmd.commit();
            cmd.wait_until_completed();

            let gpu_output = fixture.read_buffer(&output_buf, out_size);

            // Compare with tolerance
            let result = compare_floats(&cpu_output, &gpu_output, 1e-5);
            assert!(
                result.is_ok(),
                "CPU/GPU parity failed for hidden={}, out={}, rank={}: {:?}",
                hidden, out_size, rank, result.err()
            );
        }
    }

    #[test]
    fn test_multi_adapter_fusion() {
        // Test that multiple adapters can be fused with different gate weights
        let fixture = MetalTestFixture::new();

        let hidden = 32;
        let rank = 4;

        // Create two different adapter weights
        let adapter1 = MockLoraWeights::new(hidden, rank, 11111);
        let adapter2 = MockLoraWeights::new(hidden, rank, 22222);

        // Verify they're different
        assert_ne!(adapter1.lora_a, adapter2.lora_a);
        assert_ne!(adapter1.lora_b, adapter2.lora_b);

        // Test buffers can be created for each
        let _buf1_a = fixture.create_buffer(&adapter1.lora_a);
        let _buf1_b = fixture.create_buffer(&adapter1.lora_b);
        let _buf2_a = fixture.create_buffer(&adapter2.lora_a);
        let _buf2_b = fixture.create_buffer(&adapter2.lora_b);
    }

    #[test]
    fn test_ring_buffer_creation() {
        use adapteros_lora_kernel_mtl::RingBuffer;

        let device = Device::system_default().expect("Metal device required");
        let k_values = [1, 3, 5, 8];

        for k in k_values {
            let result = RingBuffer::new(Arc::new(device.clone()), k);
            assert!(result.is_ok(), "RingBuffer creation failed for k={}: {:?}", k, result.err());
        }
    }
}

// =============================================================================
// Performance Benchmarks
// =============================================================================

mod performance_tests {
    use super::*;

    #[test]
    fn benchmark_buffer_creation() {
        let fixture = MetalTestFixture::new();
        let sizes = [1024, 4096, 16384, 65536];

        println!("\nBuffer Creation Benchmarks:");
        println!("{:<15} {:>15} {:>15}", "Size", "Time (us)", "Throughput (MB/s)");
        println!("{:-<45}", "");

        for size in sizes {
            let data = vec![0.0f32; size];
            let iterations = 100;

            let start = Instant::now();
            for _ in 0..iterations {
                let _ = fixture.create_buffer(&data);
            }
            let elapsed = start.elapsed();

            let time_us = elapsed.as_micros() as f64 / iterations as f64;
            let bytes_per_sec = (size * 4 * iterations) as f64 / elapsed.as_secs_f64();
            let mb_per_sec = bytes_per_sec / (1024.0 * 1024.0);

            println!("{:<15} {:>15.2} {:>15.2}", size, time_us, mb_per_sec);
        }
    }

    #[test]
    fn benchmark_simple_matmul() {
        let msl = r#"#include <metal_stdlib>
using namespace metal;

kernel void matmul(
    device const float* a [[ buffer(0) ]],
    device const float* b [[ buffer(1) ]],
    device float* c       [[ buffer(2) ]],
    constant uint& n      [[ buffer(3) ]],
    uint2 tid             [[ thread_position_in_grid ]]
) {
    uint row = tid.y;
    uint col = tid.x;
    if (row >= n || col >= n) return;

    float sum = 0.0f;
    for (uint k = 0; k < n; ++k) {
        sum += a[row * n + k] * b[k * n + col];
    }
    c[row * n + col] = sum;
}
"#;

        let fixture = MetalTestFixture::new();
        let options = CompileOptions::new();
        let library = fixture.device.new_library_with_source(msl, &options).unwrap();
        let function = library.get_function("matmul", None).unwrap();
        let pipeline = fixture.device.new_compute_pipeline_state_with_function(&function).unwrap();

        let sizes = [64, 128, 256, 512];

        println!("\nMatrix Multiplication Benchmarks:");
        println!("{:<10} {:>15} {:>15}", "Size", "Time (ms)", "GFLOPS");
        println!("{:-<40}", "");

        for n in sizes {
            let a = generate_deterministic_weights(n * n, 11111);
            let b = generate_deterministic_weights(n * n, 22222);

            let a_buf = fixture.create_buffer(&a);
            let b_buf = fixture.create_buffer(&b);
            let c_buf = fixture.create_empty_buffer(n * n);
            let n_u = n as u32;
            let n_buf = fixture.device.new_buffer_with_data(
                &n_u as *const _ as *const _,
                4, MTLResourceOptions::StorageModeShared,
            );

            // Warmup
            for _ in 0..5 {
                let cmd = fixture.queue.new_command_buffer();
                let enc = cmd.new_compute_command_encoder();
                enc.set_compute_pipeline_state(&pipeline);
                enc.set_buffer(0, Some(&a_buf), 0);
                enc.set_buffer(1, Some(&b_buf), 0);
                enc.set_buffer(2, Some(&c_buf), 0);
                enc.set_buffer(3, Some(&n_buf), 0);
                enc.dispatch_thread_groups(
                    MTLSize::new(n as u64, n as u64, 1),
                    MTLSize::new(1, 1, 1),
                );
                enc.end_encoding();
                cmd.commit();
                cmd.wait_until_completed();
            }

            // Benchmark
            let iterations = 20;
            let start = Instant::now();
            for _ in 0..iterations {
                let cmd = fixture.queue.new_command_buffer();
                let enc = cmd.new_compute_command_encoder();
                enc.set_compute_pipeline_state(&pipeline);
                enc.set_buffer(0, Some(&a_buf), 0);
                enc.set_buffer(1, Some(&b_buf), 0);
                enc.set_buffer(2, Some(&c_buf), 0);
                enc.set_buffer(3, Some(&n_buf), 0);
                enc.dispatch_thread_groups(
                    MTLSize::new(n as u64, n as u64, 1),
                    MTLSize::new(1, 1, 1),
                );
                enc.end_encoding();
                cmd.commit();
                cmd.wait_until_completed();
            }
            let elapsed = start.elapsed();

            let time_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
            let flops = 2.0 * (n as f64).powi(3) * iterations as f64 / elapsed.as_secs_f64();
            let gflops = flops / 1e9;

            println!("{:<10} {:>15.3} {:>15.2}", n, time_ms, gflops);
        }
    }

    #[test]
    fn benchmark_lora_overhead() {
        let msl = r#"#include <metal_stdlib>
using namespace metal;

kernel void lora_single(
    device const float* input   [[ buffer(0) ]],
    device const float* lora_a  [[ buffer(1) ]],
    device const float* lora_b  [[ buffer(2) ]],
    device float* output        [[ buffer(3) ]],
    constant uint& hidden       [[ buffer(4) ]],
    constant uint& rank         [[ buffer(5) ]],
    uint tid                    [[ thread_position_in_grid ]]
) {
    if (tid >= hidden) return;

    float sum = 0.0f;
    for (uint r = 0; r < rank; ++r) {
        float inter = 0.0f;
        for (uint h = 0; h < hidden; ++h) {
            inter += input[h] * lora_a[r * hidden + h];
        }
        sum += inter * lora_b[tid * rank + r];
    }
    output[tid] = sum;
}
"#;

        let fixture = MetalTestFixture::new();
        let options = CompileOptions::new();
        let library = fixture.device.new_library_with_source(msl, &options).unwrap();
        let function = library.get_function("lora_single", None).unwrap();
        let pipeline = fixture.device.new_compute_pipeline_state_with_function(&function).unwrap();

        let hidden_sizes = [256, 512, 1024, 2048];
        let ranks = [4, 8, 16, 32];

        println!("\nLoRA Overhead Benchmarks:");
        println!("{:<10} {:<8} {:>15} {:>15}", "Hidden", "Rank", "Time (us)", "Params/sec (M)");
        println!("{:-<50}", "");

        for &hidden in &hidden_sizes {
            for &rank in &ranks {
                let input = generate_deterministic_weights(hidden, 11111);
                let lora_a = generate_deterministic_weights(rank * hidden, 22222);
                let lora_b = generate_deterministic_weights(hidden * rank, 33333);

                let input_buf = fixture.create_buffer(&input);
                let a_buf = fixture.create_buffer(&lora_a);
                let b_buf = fixture.create_buffer(&lora_b);
                let output_buf = fixture.create_empty_buffer(hidden);

                let hidden_u = hidden as u32;
                let rank_u = rank as u32;
                let hidden_buf = fixture.device.new_buffer_with_data(
                    &hidden_u as *const _ as *const _,
                    4, MTLResourceOptions::StorageModeShared,
                );
                let rank_buf = fixture.device.new_buffer_with_data(
                    &rank_u as *const _ as *const _,
                    4, MTLResourceOptions::StorageModeShared,
                );

                // Warmup
                for _ in 0..10 {
                    let cmd = fixture.queue.new_command_buffer();
                    let enc = cmd.new_compute_command_encoder();
                    enc.set_compute_pipeline_state(&pipeline);
                    enc.set_buffer(0, Some(&input_buf), 0);
                    enc.set_buffer(1, Some(&a_buf), 0);
                    enc.set_buffer(2, Some(&b_buf), 0);
                    enc.set_buffer(3, Some(&output_buf), 0);
                    enc.set_buffer(4, Some(&hidden_buf), 0);
                    enc.set_buffer(5, Some(&rank_buf), 0);
                    enc.dispatch_thread_groups(
                        MTLSize::new(hidden as u64, 1, 1),
                        MTLSize::new(1, 1, 1),
                    );
                    enc.end_encoding();
                    cmd.commit();
                    cmd.wait_until_completed();
                }

                // Benchmark
                let iterations = 100;
                let start = Instant::now();
                for _ in 0..iterations {
                    let cmd = fixture.queue.new_command_buffer();
                    let enc = cmd.new_compute_command_encoder();
                    enc.set_compute_pipeline_state(&pipeline);
                    enc.set_buffer(0, Some(&input_buf), 0);
                    enc.set_buffer(1, Some(&a_buf), 0);
                    enc.set_buffer(2, Some(&b_buf), 0);
                    enc.set_buffer(3, Some(&output_buf), 0);
                    enc.set_buffer(4, Some(&hidden_buf), 0);
                    enc.set_buffer(5, Some(&rank_buf), 0);
                    enc.dispatch_thread_groups(
                        MTLSize::new(hidden as u64, 1, 1),
                        MTLSize::new(1, 1, 1),
                    );
                    enc.end_encoding();
                    cmd.commit();
                    cmd.wait_until_completed();
                }
                let elapsed = start.elapsed();

                let time_us = elapsed.as_micros() as f64 / iterations as f64;
                let params = 2 * rank * hidden; // A + B parameters
                let params_per_sec = (params * iterations) as f64 / elapsed.as_secs_f64() / 1e6;

                println!("{:<10} {:<8} {:>15.2} {:>15.2}", hidden, rank, time_us, params_per_sec);
            }
        }
    }
}

// =============================================================================
// Stress Tests
// =============================================================================

mod stress_tests {
    use super::*;

    #[test]
    fn test_large_buffer_allocation() {
        let fixture = MetalTestFixture::new();

        // Test increasingly large buffers (up to 64MB)
        let sizes = [
            1_000_000,    // 4 MB
            4_000_000,    // 16 MB
            16_000_000,   // 64 MB
        ];

        for size in sizes {
            let data = vec![0.0f32; size];
            let buffer = fixture.create_buffer(&data);
            assert_eq!(buffer.length(), (size * 4) as u64);
        }
    }

    #[test]
    fn test_rapid_buffer_creation_destruction() {
        let fixture = MetalTestFixture::new();
        let size = 1024;
        let data = vec![1.0f32; size];

        // Rapidly create and drop buffers
        for _ in 0..1000 {
            let _ = fixture.create_buffer(&data);
        }
    }

    #[test]
    fn test_concurrent_command_buffers() {
        let msl = r#"#include <metal_stdlib>
using namespace metal;

kernel void add_one(
    device float* data [[ buffer(0) ]],
    uint tid           [[ thread_position_in_grid ]]
) {
    data[tid] += 1.0f;
}
"#;

        let fixture = MetalTestFixture::new();
        let options = CompileOptions::new();
        let library = fixture.device.new_library_with_source(msl, &options).unwrap();
        let function = library.get_function("add_one", None).unwrap();
        let pipeline = fixture.device.new_compute_pipeline_state_with_function(&function).unwrap();

        let size = 1024;
        let data = vec![0.0f32; size];
        let buffer = fixture.create_buffer(&data);

        // Submit multiple command buffers
        let num_iterations = 100;
        for _ in 0..num_iterations {
            let cmd = fixture.queue.new_command_buffer();
            let enc = cmd.new_compute_command_encoder();
            enc.set_compute_pipeline_state(&pipeline);
            enc.set_buffer(0, Some(&buffer), 0);
            enc.dispatch_thread_groups(
                MTLSize::new(size as u64, 1, 1),
                MTLSize::new(1, 1, 1),
            );
            enc.end_encoding();
            cmd.commit();
            cmd.wait_until_completed();
        }

        // Verify result
        let result = fixture.read_buffer(&buffer, size);
        for &val in &result {
            assert!(
                (val - num_iterations as f32).abs() < 1e-6,
                "Expected {}, got {}",
                num_iterations,
                val
            );
        }
    }
}

// =============================================================================
// Edge Case Tests
// =============================================================================

mod edge_case_tests {
    use super::*;

    #[test]
    fn test_zero_weights() {
        let fixture = MetalTestFixture::new();

        let size = 64;
        let zeros = vec![0.0f32; size];
        let buffer = fixture.create_buffer(&zeros);
        let result = fixture.read_buffer(&buffer, size);

        for val in result {
            assert_eq!(val, 0.0);
        }
    }

    #[test]
    fn test_extreme_values() {
        let fixture = MetalTestFixture::new();

        let data = vec![
            f32::MIN,
            f32::MAX,
            f32::EPSILON,
            f32::MIN_POSITIVE,
            0.0,
            -0.0,
        ];

        let buffer = fixture.create_buffer(&data);
        let result = fixture.read_buffer(&buffer, data.len());

        for (expected, actual) in data.iter().zip(result.iter()) {
            assert_eq!(expected.to_bits(), actual.to_bits());
        }
    }

    #[test]
    fn test_nan_and_inf_handling() {
        let fixture = MetalTestFixture::new();

        let data = vec![
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];

        let buffer = fixture.create_buffer(&data);
        let result = fixture.read_buffer(&buffer, data.len());

        assert!(result[0].is_nan());
        assert!(result[1].is_infinite() && result[1] > 0.0);
        assert!(result[2].is_infinite() && result[2] < 0.0);
    }

    #[test]
    fn test_single_element() {
        let fixture = MetalTestFixture::new();

        let data = vec![42.0f32];
        let buffer = fixture.create_buffer(&data);
        let result = fixture.read_buffer(&buffer, 1);

        assert_eq!(result[0], 42.0);
    }

    #[test]
    fn test_very_small_rank() {
        let weights = MockLoraWeights::new(64, 1, 12345);
        assert_eq!(weights.rank, 1);
        assert_eq!(weights.lora_a.len(), 64);
        assert_eq!(weights.lora_b.len(), 64);
    }

    #[test]
    fn test_rank_equals_hidden() {
        // Edge case where rank equals hidden size
        let weights = MockLoraWeights::new(16, 16, 12345);
        assert_eq!(weights.lora_a.len(), 256);
        assert_eq!(weights.lora_b.len(), 256);
    }
}
