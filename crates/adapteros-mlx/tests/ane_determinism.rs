//! ANE Determinism Tests
//!
//! These tests verify that operations produce bit-exact results across
//! repeated executions, which is critical for the adapterOS determinism
//! guarantee.
//!
//! Run with: cargo test -p adapteros-mlx --test ane_determinism -- --test-threads=1

use adapteros_mlx::{AneConfig, Array, LayerNorm, RMSNorm};

/// Helper to compare two f32 vectors for exact equality
fn assert_exact_equal(a: &[f32], b: &[f32], context: &str) {
    assert_eq!(a.len(), b.len(), "{}: length mismatch", context);
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        assert!(
            x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()),
            "{}: mismatch at index {}: {} vs {} (bits: {:08x} vs {:08x})",
            context,
            i,
            x,
            y,
            x.to_bits(),
            y.to_bits()
        );
    }
}

#[test]
fn test_layernorm_determinism() {
    // Run layernorm multiple times with same input
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();
    let bias = Array::zeros(&[4]).unwrap();

    let result1 = x.layernorm(&weight, &bias, 1e-5).unwrap();
    let result2 = x.layernorm(&weight, &bias, 1e-5).unwrap();
    let result3 = x.layernorm(&weight, &bias, 1e-5).unwrap();

    let data1 = result1.to_vec_f32().unwrap();
    let data2 = result2.to_vec_f32().unwrap();
    let data3 = result3.to_vec_f32().unwrap();

    assert_exact_equal(&data1, &data2, "layernorm run 1 vs 2");
    assert_exact_equal(&data2, &data3, "layernorm run 2 vs 3");
}

#[test]
fn test_rms_norm_determinism() {
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();

    let result1 = x.rms_norm(&weight, 1e-5).unwrap();
    let result2 = x.rms_norm(&weight, 1e-5).unwrap();
    let result3 = x.rms_norm(&weight, 1e-5).unwrap();

    let data1 = result1.to_vec_f32().unwrap();
    let data2 = result2.to_vec_f32().unwrap();
    let data3 = result3.to_vec_f32().unwrap();

    assert_exact_equal(&data1, &data2, "rms_norm run 1 vs 2");
    assert_exact_equal(&data2, &data3, "rms_norm run 2 vs 3");
}

#[test]
fn test_softmax_determinism() {
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();

    let result1 = x.softmax(-1).unwrap();
    let result2 = x.softmax(-1).unwrap();
    let result3 = x.softmax(-1).unwrap();

    let data1 = result1.to_vec_f32().unwrap();
    let data2 = result2.to_vec_f32().unwrap();
    let data3 = result3.to_vec_f32().unwrap();

    assert_exact_equal(&data1, &data2, "softmax run 1 vs 2");
    assert_exact_equal(&data2, &data3, "softmax run 2 vs 3");
}

#[test]
fn test_matmul_determinism() {
    let a = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[2, 3]).unwrap();
    let b = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[3, 2]).unwrap();

    let result1 = a.matmul(&b).unwrap();
    let result2 = a.matmul(&b).unwrap();
    let result3 = a.matmul(&b).unwrap();

    let data1 = result1.to_vec_f32().unwrap();
    let data2 = result2.to_vec_f32().unwrap();
    let data3 = result3.to_vec_f32().unwrap();

    assert_exact_equal(&data1, &data2, "matmul run 1 vs 2");
    assert_exact_equal(&data2, &data3, "matmul run 2 vs 3");
}

#[test]
fn test_layer_determinism() {
    let norm = LayerNorm::new(4, 1e-5).unwrap();
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();

    let result1 = norm.forward(&x, None).unwrap();
    let result2 = norm.forward(&x, None).unwrap();

    let data1 = result1.to_vec_f32().unwrap();
    let data2 = result2.to_vec_f32().unwrap();

    assert_exact_equal(&data1, &data2, "LayerNorm layer forward");
}

#[test]
fn test_rms_layer_determinism() {
    let norm = RMSNorm::new(4, 1e-5).unwrap();
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 4]).unwrap();

    let result1 = norm.forward(&x, None).unwrap();
    let result2 = norm.forward(&x, None).unwrap();

    let data1 = result1.to_vec_f32().unwrap();
    let data2 = result2.to_vec_f32().unwrap();

    assert_exact_equal(&data1, &data2, "RMSNorm layer forward");
}

#[test]
fn test_ane_attestation() {
    // Even though ANE isn't available yet, the attestation should
    // report MLX GPU as deterministic
    let config = AneConfig::production();

    // AneAccelerator::try_new returns None when ANE isn't implemented yet
    // But we can verify the config itself
    assert!(
        config.require_determinism,
        "Production config should require determinism"
    );
    assert!(config.enabled, "Production config should have ANE enabled");
}

#[test]
fn test_chained_operations_determinism() {
    // Test that a sequence of operations produces deterministic results
    let x = Array::from_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 4]).unwrap();
    let weight = Array::ones(&[4]).unwrap();
    let bias = Array::zeros(&[4]).unwrap();

    let chain = |x: &Array| -> Vec<f32> {
        let normed = x.layernorm(&weight, &bias, 1e-5).unwrap();
        let activated = normed.softmax(-1).unwrap();
        let scaled = activated.scale(2.0).unwrap();
        scaled.to_vec_f32().unwrap()
    };

    let result1 = chain(&x);
    let result2 = chain(&x);
    let result3 = chain(&x);

    assert_exact_equal(&result1, &result2, "chained ops run 1 vs 2");
    assert_exact_equal(&result2, &result3, "chained ops run 2 vs 3");
}
