//! Migration Compatibility Tests
//!
//! These tests verify that adapteros-mlx provides API compatibility
//! with common mlx-rs patterns, ensuring smooth migration for dependent crates.
//!
//! Run with: cargo test -p adapteros-mlx --test migration -- --test-threads=1

use adapteros_mlx::{Array, Dtype, Device, LayerNorm, RMSNorm, MultiHeadAttention, MLP};
use adapteros_mlx::layers::mlp::Activation;

// =============================================================================
// Array Creation and Properties
// =============================================================================

#[test]
fn test_array_from_slice() {
    // Common mlx-rs pattern: Array::from_slice
    let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    assert_eq!(arr.shape(), vec![2, 2]);
    assert_eq!(arr.ndim(), 2);
    assert_eq!(arr.size(), 4);
}

#[test]
fn test_array_zeros_ones() {
    let zeros = Array::zeros(&[3, 4]).unwrap();
    assert_eq!(zeros.shape(), vec![3, 4]);

    let ones = Array::ones(&[2, 3]).unwrap();
    assert_eq!(ones.shape(), vec![2, 3]);
}

#[test]
fn test_array_dtype() {
    let arr = Array::from_f32(&[1.0, 2.0], &[2]).unwrap();
    assert_eq!(arr.dtype(), Dtype::Float32);
}

// =============================================================================
// Array Operations
// =============================================================================

#[test]
fn test_arithmetic_ops() {
    let a = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = Array::from_f32(&[1.0, 1.0, 1.0, 1.0], &[2, 2]).unwrap();

    let sum = a.add(&b).unwrap();
    let diff = a.sub(&b).unwrap();
    let prod = a.mul(&b).unwrap();
    let quot = a.div(&b).unwrap();

    assert_eq!(sum.shape(), vec![2, 2]);
    assert_eq!(diff.shape(), vec![2, 2]);
    assert_eq!(prod.shape(), vec![2, 2]);
    assert_eq!(quot.shape(), vec![2, 2]);
}

#[test]
fn test_matmul() {
    let a = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let b = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[3, 2]).unwrap();

    let c = a.matmul(&b).unwrap();
    assert_eq!(c.shape(), vec![2, 2]);
}

#[test]
fn test_transpose() {
    let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let t = arr.transpose().unwrap();
    assert_eq!(t.shape(), vec![3, 2]);
}

#[test]
fn test_reshape() {
    let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let reshaped = arr.reshape(&[3, 2]).unwrap();
    assert_eq!(reshaped.shape(), vec![3, 2]);
}

// =============================================================================
// Reductions
// =============================================================================

#[test]
fn test_sum() {
    let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();

    // Sum all
    let total = arr.sum(None, false).unwrap();
    assert_eq!(total.shape(), Vec::<i32>::new());

    // Sum along axis
    let row_sum = arr.sum(Some(1), false).unwrap();
    assert_eq!(row_sum.shape(), vec![2]);
}

#[test]
fn test_mean() {
    let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();

    let mean = arr.mean(None, false).unwrap();
    assert_eq!(mean.shape(), Vec::<i32>::new());

    let row_mean = arr.mean(Some(1), true).unwrap();
    assert_eq!(row_mean.shape(), vec![2, 1]);
}

// =============================================================================
// Activations
// =============================================================================

#[test]
fn test_softmax() {
    let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let soft = arr.softmax(-1).unwrap();
    assert_eq!(soft.shape(), vec![2, 2]);

    // Softmax should sum to 1 along the axis
    let data = soft.to_vec_f32().unwrap();
    let row1_sum = data[0] + data[1];
    let row2_sum = data[2] + data[3];
    assert!((row1_sum - 1.0).abs() < 1e-5);
    assert!((row2_sum - 1.0).abs() < 1e-5);
}

#[test]
fn test_relu() {
    let arr = Array::from_f32(&[-1.0, 0.0, 1.0, 2.0], &[4]).unwrap();
    let activated = arr.relu().unwrap();
    let data = activated.to_vec_f32().unwrap();
    assert_eq!(data, vec![0.0, 0.0, 1.0, 2.0]);
}

#[test]
fn test_gelu() {
    let arr = Array::from_f32(&[-1.0, 0.0, 1.0, 2.0], &[4]).unwrap();
    let activated = arr.gelu().unwrap();
    assert_eq!(activated.shape(), vec![4]);

    // GELU(0) should be approximately 0
    let data = activated.to_vec_f32().unwrap();
    assert!(data[1].abs() < 1e-5);
}

#[test]
fn test_silu() {
    let arr = Array::from_f32(&[-1.0, 0.0, 1.0, 2.0], &[4]).unwrap();
    let activated = arr.silu().unwrap();
    assert_eq!(activated.shape(), vec![4]);

    // SiLU(0) = 0 * sigmoid(0) = 0 * 0.5 = 0
    let data = activated.to_vec_f32().unwrap();
    assert!(data[1].abs() < 1e-5);
}

// =============================================================================
// Normalization
// =============================================================================

#[test]
fn test_layernorm_api() {
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();
    let bias = Array::zeros(&[4]).unwrap();

    let normalized = x.layernorm(&weight, &bias, 1e-5).unwrap();
    assert_eq!(normalized.shape(), vec![1, 4]);
}

#[test]
fn test_rms_norm_api() {
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();

    let normalized = x.rms_norm(&weight, 1e-5).unwrap();
    assert_eq!(normalized.shape(), vec![1, 4]);
}

// =============================================================================
// Layer Abstractions
// =============================================================================

#[test]
fn test_layernorm_layer() {
    let norm = LayerNorm::new(64, 1e-5).unwrap();
    assert_eq!(norm.dim, 64);

    let x = Array::ones(&[1, 8, 64]).unwrap();
    let output = norm.forward(&x, None).unwrap();
    assert_eq!(output.shape(), vec![1, 8, 64]);
}

#[test]
fn test_rmsnorm_layer() {
    let norm = RMSNorm::new(64, 1e-5).unwrap();
    assert_eq!(norm.dim, 64);

    let x = Array::ones(&[1, 8, 64]).unwrap();
    let output = norm.forward(&x, None).unwrap();
    assert_eq!(output.shape(), vec![1, 8, 64]);
}

#[test]
fn test_attention_layer() {
    let attn = MultiHeadAttention::new(64, 4).unwrap();
    assert_eq!(attn.n_heads, 4);
    assert_eq!(attn.hidden_dim, 64);
    assert_eq!(attn.head_dim, 16);

    let x = Array::ones(&[1, 8, 64]).unwrap();
    let output = attn.forward(&x, None, None).unwrap();
    assert_eq!(output.shape(), vec![1, 8, 64]);
}

#[test]
fn test_mlp_layer() {
    // Standard FFN
    let mlp = MLP::new(64, 256, Activation::GELU, false).unwrap();
    let x = Array::ones(&[1, 8, 64]).unwrap();
    let output = mlp.forward(&x).unwrap();
    assert_eq!(output.shape(), vec![1, 8, 64]);

    // Gated FFN (LLaMA-style)
    let gated_mlp = MLP::new(64, 256, Activation::SiLU, true).unwrap();
    let gated_output = gated_mlp.forward(&x).unwrap();
    assert_eq!(gated_output.shape(), vec![1, 8, 64]);
}

// =============================================================================
// Data Access
// =============================================================================

#[test]
fn test_to_vec_f32() {
    let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let data = arr.to_vec_f32().unwrap();
    assert_eq!(data, vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn test_to_vec_i32() {
    let arr = Array::from_i32(&[1, 2, 3, 4], &[2, 2]).unwrap();
    let data = arr.to_vec_i32().unwrap();
    assert_eq!(data, vec![1, 2, 3, 4]);
}

#[test]
fn test_evaluate() {
    let arr = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    // Force evaluation (MLX uses lazy evaluation)
    arr.evaluate().unwrap();
}

// =============================================================================
// Device
// =============================================================================

#[test]
fn test_device_info() {
    let device = Device::default();
    // Just verify we can create and query device info
    let _ = device;
}

// =============================================================================
// Runtime
// =============================================================================

#[test]
fn test_runtime_init() {
    let result = adapteros_mlx::runtime_init();
    assert!(result.is_ok());
    assert!(adapteros_mlx::runtime_is_initialized());
}

#[test]
fn test_backend_info() {
    let info = adapteros_mlx::backend_info();
    assert!(info.contains("mlx-rs"));
}
