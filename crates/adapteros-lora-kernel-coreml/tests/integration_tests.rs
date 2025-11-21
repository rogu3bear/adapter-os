//! Integration tests for CoreML tensor operation pipeline
//!
//! These tests exercise the full tensor operation lifecycle including:
//! - Create -> Operate -> Materialize flow
//! - Chained operations
//! - Large tensors (1M+ elements)
//! - Edge cases
//! - Swift vs ObjC++ bridge benchmarks
//! - Concurrent operations

use adapteros_lora_kernel_coreml::{ffi, MLTensor, TensorBridgeType};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

// =============================================================================
// Test helpers
// =============================================================================

/// Skip test if MLTensor is not available
macro_rules! require_mltensor {
    () => {
        if !MLTensor::is_available() {
            eprintln!("Skipping test - MLTensor not available (requires macOS 15+)");
            return;
        }
    };
}

/// Approximate float comparison
fn approx_eq(a: f32, b: f32, epsilon: f32) -> bool {
    (a - b).abs() < epsilon
}

/// Verify softmax output sums to 1
fn verify_softmax(data: &[f32], epsilon: f32) -> bool {
    let sum: f32 = data.iter().sum();
    (sum - 1.0).abs() < epsilon
}

// =============================================================================
// 1. Create -> Operate -> Materialize Flow Tests
// =============================================================================

#[test]
fn test_full_pipeline_create_operate_materialize() {
    require_mltensor!();

    // Create tensor
    let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let tensor = MLTensor::from_floats(&data, &[2, 3]).expect("Failed to create tensor");

    // Operate: scale by 2
    let scaled = tensor.scale(2.0).expect("Failed to scale tensor");

    // Materialize
    let result = scaled.to_vec().expect("Failed to materialize tensor");

    // Verify
    let expected = vec![2.0, 4.0, 6.0, 8.0, 10.0, 12.0];
    assert_eq!(result, expected, "Scale operation result mismatch");
}

#[test]
fn test_pipeline_with_softmax() {
    require_mltensor!();

    let data = vec![1.0, 2.0, 3.0, 4.0];
    let tensor = MLTensor::from_floats(&data, &[1, 4]).expect("Failed to create tensor");

    let softmax_result = tensor.softmax(-1).expect("Failed to apply softmax");
    let result = softmax_result.to_vec().expect("Failed to materialize");

    assert!(verify_softmax(&result, 1e-5), "Softmax should sum to 1");
    assert!(result.iter().all(|&x| x > 0.0), "Softmax values must be positive");
    assert!(
        result[3] > result[2] && result[2] > result[1] && result[1] > result[0],
        "Softmax should preserve ordering"
    );
}

#[test]
fn test_pipeline_with_add() {
    require_mltensor!();

    let data1 = vec![1.0, 2.0, 3.0, 4.0];
    let data2 = vec![10.0, 20.0, 30.0, 40.0];

    let t1 = MLTensor::from_floats(&data1, &[2, 2]).expect("Failed to create t1");
    let t2 = MLTensor::from_floats(&data2, &[2, 2]).expect("Failed to create t2");

    let sum = t1.add(&t2).expect("Failed to add tensors");
    let result = sum.to_vec().expect("Failed to materialize");

    assert_eq!(result, vec![11.0, 22.0, 33.0, 44.0]);
}

#[test]
fn test_pipeline_with_matmul() {
    require_mltensor!();

    // 2x3 @ 3x2 = 2x2
    let data1 = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
    let data2 = vec![7.0, 8.0, 9.0, 10.0, 11.0, 12.0];

    let t1 = MLTensor::from_floats(&data1, &[2, 3]).expect("Failed to create t1");
    let t2 = MLTensor::from_floats(&data2, &[3, 2]).expect("Failed to create t2");

    let product = t1.matmul(&t2).expect("Failed to matmul");
    let result = product.to_vec().expect("Failed to materialize");

    // [1,2,3] @ [7,8; 9,10; 11,12] = [1*7+2*9+3*11, 1*8+2*10+3*12] = [58, 64]
    // [4,5,6] @ [7,8; 9,10; 11,12] = [4*7+5*9+6*11, 4*8+5*10+6*12] = [139, 154]
    assert_eq!(result, vec![58.0, 64.0, 139.0, 154.0]);
}

// =============================================================================
// 2. Chained Operations Tests: (a + b) * c
// =============================================================================

#[test]
fn test_chained_add_then_scale() {
    require_mltensor!();

    let a = MLTensor::from_floats(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = MLTensor::from_floats(&[5.0, 6.0, 7.0, 8.0], &[2, 2]).unwrap();

    // (a + b) * 2
    let sum = a.add(&b).expect("Add failed");
    let result = sum.scale(2.0).expect("Scale failed");

    let output = result.to_vec().expect("Materialize failed");
    // (1+5)*2=12, (2+6)*2=16, (3+7)*2=20, (4+8)*2=24
    assert_eq!(output, vec![12.0, 16.0, 20.0, 24.0]);
}

#[test]
fn test_chained_scale_add_matmul() {
    require_mltensor!();

    let a = MLTensor::from_floats(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let b = MLTensor::from_floats(&[1.0, 0.0, 0.0, 1.0], &[2, 2]).unwrap(); // Identity matrix

    // Scale a by 2, add b, then matmul with original a
    let scaled_a = a.scale(2.0).expect("Scale failed");
    let sum = scaled_a.add(&b).expect("Add failed");
    let result = sum.matmul(&a).expect("Matmul failed");

    let output = result.to_vec().expect("Materialize failed");
    // scaled_a = [2, 4, 6, 8], sum = [3, 4, 6, 9]
    // [3,4] @ [1,2; 3,4] = [3*1+4*3, 3*2+4*4] = [15, 22]
    // [6,9] @ [1,2; 3,4] = [6*1+9*3, 6*2+9*4] = [33, 48]
    assert_eq!(output, vec![15.0, 22.0, 33.0, 48.0]);
}

#[test]
fn test_chained_multiple_softmax() {
    require_mltensor!();

    let data = vec![1.0, 2.0, 3.0, 4.0];
    let t = MLTensor::from_floats(&data, &[1, 4]).unwrap();

    // Apply softmax twice (result should still sum to 1)
    let s1 = t.softmax(-1).expect("First softmax failed");
    let s2 = s1.softmax(-1).expect("Second softmax failed");

    let result = s2.to_vec().expect("Materialize failed");
    assert!(verify_softmax(&result, 1e-5), "Double softmax should still sum to 1");
}

#[test]
fn test_chained_complex_expression() {
    require_mltensor!();

    // ((a * 2) + (b * 3)) * 0.5
    let a = MLTensor::from_floats(&[2.0, 4.0], &[2]).unwrap();
    let b = MLTensor::from_floats(&[1.0, 2.0], &[2]).unwrap();

    let a_scaled = a.scale(2.0).unwrap(); // [4, 8]
    let b_scaled = b.scale(3.0).unwrap(); // [3, 6]
    let sum = a_scaled.add(&b_scaled).unwrap(); // [7, 14]
    let result = sum.scale(0.5).unwrap(); // [3.5, 7]

    let output = result.to_vec().unwrap();
    assert_eq!(output, vec![3.5, 7.0]);
}

// =============================================================================
// 3. Large Tensor Tests (1M+ elements)
// =============================================================================

#[test]
fn test_large_tensor_1m_elements() {
    require_mltensor!();

    let size = 1_000_000;
    let data: Vec<f32> = (0..size).map(|i| (i % 100) as f32).collect();

    let start = Instant::now();
    let tensor = MLTensor::from_floats(&data, &[1000, 1000]).expect("Failed to create 1M tensor");
    let create_time = start.elapsed();

    let start = Instant::now();
    let scaled = tensor.scale(2.0).expect("Failed to scale 1M tensor");
    let op_time = start.elapsed();

    let start = Instant::now();
    let result = scaled.to_vec().expect("Failed to materialize 1M tensor");
    let materialize_time = start.elapsed();

    assert_eq!(result.len(), size);
    // Verify a few samples
    assert_eq!(result[0], 0.0);
    assert_eq!(result[50], 100.0); // 50 * 2
    assert_eq!(result[99], 198.0); // 99 * 2

    eprintln!(
        "1M tensor: create={:?}, scale={:?}, materialize={:?}",
        create_time, op_time, materialize_time
    );
}

#[test]
fn test_large_tensor_2m_elements() {
    require_mltensor!();

    let size = 2_000_000;
    let data: Vec<f32> = vec![1.0; size];

    let tensor = MLTensor::from_floats(&data, &[2000, 1000]).expect("Failed to create 2M tensor");
    let scaled = tensor.scale(3.0).expect("Failed to scale");
    let result = scaled.to_vec().expect("Failed to materialize");

    assert_eq!(result.len(), size);
    assert!(result.iter().all(|&x| approx_eq(x, 3.0, 1e-6)));
}

#[test]
fn test_large_tensor_add() {
    require_mltensor!();

    let size = 500_000;
    let data1: Vec<f32> = vec![1.0; size];
    let data2: Vec<f32> = vec![2.0; size];

    let t1 = MLTensor::from_floats(&data1, &[500, 1000]).expect("Create t1");
    let t2 = MLTensor::from_floats(&data2, &[500, 1000]).expect("Create t2");

    let sum = t1.add(&t2).expect("Add failed");
    let result = sum.to_vec().expect("Materialize failed");

    assert_eq!(result.len(), size);
    assert!(result.iter().all(|&x| approx_eq(x, 3.0, 1e-6)));
}

#[test]
fn test_large_tensor_matmul() {
    require_mltensor!();

    // 512x1024 @ 1024x512 = 512x512 (262k output elements)
    let m = 512;
    let k = 1024;
    let n = 512;

    let data1: Vec<f32> = vec![0.001; m * k]; // Small values to avoid overflow
    let data2: Vec<f32> = vec![0.001; k * n];

    let start = Instant::now();
    let t1 = MLTensor::from_floats(&data1, &[m, k]).expect("Create t1");
    let t2 = MLTensor::from_floats(&data2, &[k, n]).expect("Create t2");

    let product = t1.matmul(&t2).expect("Matmul failed");
    let result = product.to_vec().expect("Materialize failed");
    let elapsed = start.elapsed();

    assert_eq!(result.len(), m * n);
    // Each element should be sum of k products of 0.001 * 0.001 = k * 0.000001
    let expected = (k as f32) * 0.000001;
    assert!(
        approx_eq(result[0], expected, 1e-6),
        "Expected {}, got {}",
        expected,
        result[0]
    );

    eprintln!("Large matmul ({}x{}@{}x{}): {:?}", m, k, k, n, elapsed);
}

// =============================================================================
// 4. Edge Case Tests
// =============================================================================

#[test]
fn test_single_element_tensor() {
    require_mltensor!();

    let tensor = MLTensor::from_floats(&[42.0], &[1]).expect("Create single element");
    let scaled = tensor.scale(2.0).expect("Scale");
    let result = scaled.to_vec().expect("Materialize");

    assert_eq!(result, vec![84.0]);
}

#[test]
fn test_1d_tensor_operations() {
    require_mltensor!();

    let t1 = MLTensor::from_floats(&[1.0, 2.0, 3.0], &[3]).unwrap();
    let t2 = MLTensor::from_floats(&[4.0, 5.0, 6.0], &[3]).unwrap();

    let sum = t1.add(&t2).unwrap();
    let result = sum.to_vec().unwrap();

    assert_eq!(result, vec![5.0, 7.0, 9.0]);
}

#[test]
fn test_high_dimensional_tensor() {
    require_mltensor!();

    // 4D tensor: batch x channels x height x width
    let data: Vec<f32> = (0..120).map(|i| i as f32).collect();
    let tensor = MLTensor::from_floats(&data, &[2, 3, 4, 5]).expect("Create 4D tensor");

    assert_eq!(tensor.shape(), vec![2, 3, 4, 5]);
    assert_eq!(tensor.num_elements(), 120);

    let scaled = tensor.scale(0.1).expect("Scale");
    let result = scaled.to_vec().expect("Materialize");

    assert_eq!(result.len(), 120);
    assert!(approx_eq(result[10], 1.0, 1e-6)); // 10 * 0.1
}

#[test]
fn test_max_dimensions() {
    require_mltensor!();

    // Test with 8 dimensions (well within 16 limit)
    let data: Vec<f32> = vec![1.0; 256]; // 2^8
    let shape = vec![2, 2, 2, 2, 2, 2, 2, 2];

    let tensor = MLTensor::from_floats(&data, &shape).expect("Create 8D tensor");
    let result = tensor.to_vec().expect("Materialize");

    assert_eq!(result.len(), 256);
}

#[test]
fn test_dimension_limit_exceeded() {
    require_mltensor!();

    // 17 dimensions should fail
    let data = vec![1.0; 131072]; // 2^17
    let shape: Vec<usize> = vec![2; 17];

    let result = MLTensor::from_floats(&data, &shape);
    assert!(result.is_err(), "Should reject >16 dimensions");
}

#[test]
fn test_shape_mismatch_error() {
    require_mltensor!();

    let data = vec![1.0, 2.0, 3.0]; // 3 elements
    let result = MLTensor::from_floats(&data, &[2, 2]); // Expects 4 elements

    assert!(result.is_err(), "Should reject shape mismatch");
}

#[test]
fn test_zero_scale() {
    require_mltensor!();

    let tensor = MLTensor::from_floats(&[1.0, 2.0, 3.0, 4.0], &[4]).unwrap();
    let scaled = tensor.scale(0.0).unwrap();
    let result = scaled.to_vec().unwrap();

    assert!(result.iter().all(|&x| x == 0.0));
}

#[test]
fn test_negative_scale() {
    require_mltensor!();

    let tensor = MLTensor::from_floats(&[1.0, -2.0, 3.0, -4.0], &[4]).unwrap();
    let scaled = tensor.scale(-1.0).unwrap();
    let result = scaled.to_vec().unwrap();

    assert_eq!(result, vec![-1.0, 2.0, -3.0, 4.0]);
}

#[test]
fn test_softmax_single_element_per_row() {
    require_mltensor!();

    // Softmax of single element should be 1.0
    let tensor = MLTensor::from_floats(&[5.0], &[1, 1]).unwrap();
    let result = tensor.softmax(-1).unwrap().to_vec().unwrap();

    assert!(approx_eq(result[0], 1.0, 1e-6));
}

#[test]
fn test_very_small_values() {
    require_mltensor!();

    let data: Vec<f32> = vec![1e-30, 2e-30, 3e-30, 4e-30];
    let tensor = MLTensor::from_floats(&data, &[4]).unwrap();
    let scaled = tensor.scale(1e30).unwrap();
    let result = scaled.to_vec().unwrap();

    assert!(approx_eq(result[0], 1.0, 1e-6));
    assert!(approx_eq(result[3], 4.0, 1e-6));
}

#[test]
fn test_very_large_values() {
    require_mltensor!();

    let data: Vec<f32> = vec![1e30, 2e30, 3e30, 4e30];
    let tensor = MLTensor::from_floats(&data, &[4]).unwrap();
    let scaled = tensor.scale(1e-30).unwrap();
    let result = scaled.to_vec().unwrap();

    assert!(approx_eq(result[0], 1.0, 1e-6));
    assert!(approx_eq(result[3], 4.0, 1e-6));
}

// =============================================================================
// 5. Swift vs ObjC++ Bridge Benchmarks
// =============================================================================

#[test]
#[cfg(target_os = "macos")]
fn test_bridge_type_detection() {
    require_mltensor!();

    let tensor = MLTensor::from_floats(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();
    let bridge_type = tensor.bridge_type();

    match bridge_type {
        TensorBridgeType::Swift => eprintln!("Using Swift bridge (optimal performance)"),
        TensorBridgeType::ObjCpp => eprintln!("Using ObjC++ bridge (fallback)"),
    }

    // Just verify we got a valid bridge type
    assert!(
        matches!(bridge_type, TensorBridgeType::Swift | TensorBridgeType::ObjCpp),
        "Invalid bridge type"
    );
}

#[test]
#[cfg(target_os = "macos")]
fn benchmark_tensor_creation() {
    require_mltensor!();

    let iterations = 100;
    let size = 10000;
    let data: Vec<f32> = (0..size).map(|i| i as f32).collect();

    let start = Instant::now();
    for _ in 0..iterations {
        let tensor = MLTensor::from_floats(&data, &[100, 100]).unwrap();
        std::hint::black_box(tensor);
    }
    let elapsed = start.elapsed();

    let avg_us = elapsed.as_micros() as f64 / iterations as f64;
    eprintln!(
        "Tensor creation ({} elements): {:.2} us/iter ({} iterations)",
        size, avg_us, iterations
    );
}

#[test]
#[cfg(target_os = "macos")]
fn benchmark_tensor_operations() {
    require_mltensor!();

    let iterations = 100;
    let data: Vec<f32> = vec![1.0; 10000];

    // Benchmark scale
    let tensor = MLTensor::from_floats(&data, &[100, 100]).unwrap();
    let start = Instant::now();
    for _ in 0..iterations {
        let scaled = tensor.scale(2.0).unwrap();
        std::hint::black_box(scaled);
    }
    let scale_time = start.elapsed();

    // Benchmark add
    let t1 = MLTensor::from_floats(&data, &[100, 100]).unwrap();
    let t2 = MLTensor::from_floats(&data, &[100, 100]).unwrap();
    let start = Instant::now();
    for _ in 0..iterations {
        let sum = t1.add(&t2).unwrap();
        std::hint::black_box(sum);
    }
    let add_time = start.elapsed();

    // Benchmark softmax
    let tensor = MLTensor::from_floats(&data, &[100, 100]).unwrap();
    let start = Instant::now();
    for _ in 0..iterations {
        let softmax = tensor.softmax(-1).unwrap();
        std::hint::black_box(softmax);
    }
    let softmax_time = start.elapsed();

    eprintln!(
        "Benchmarks ({} iters, 10k elements):\n  Scale: {:.2} us/iter\n  Add: {:.2} us/iter\n  Softmax: {:.2} us/iter",
        iterations,
        scale_time.as_micros() as f64 / iterations as f64,
        add_time.as_micros() as f64 / iterations as f64,
        softmax_time.as_micros() as f64 / iterations as f64
    );
}

#[test]
#[cfg(target_os = "macos")]
fn benchmark_materialize() {
    require_mltensor!();

    let iterations = 100;
    let size = 100000;
    let data: Vec<f32> = vec![1.0; size];

    let tensor = MLTensor::from_floats(&data, &[1000, 100]).unwrap();

    let start = Instant::now();
    for _ in 0..iterations {
        let result = tensor.to_vec().unwrap();
        std::hint::black_box(result);
    }
    let elapsed = start.elapsed();

    let avg_us = elapsed.as_micros() as f64 / iterations as f64;
    let throughput = (size as f64 * iterations as f64) / elapsed.as_secs_f64() / 1e6;
    eprintln!(
        "Materialize ({} elements): {:.2} us/iter, {:.2} M elements/sec",
        size, avg_us, throughput
    );
}

#[test]
#[cfg(target_os = "macos")]
fn benchmark_full_pipeline() {
    require_mltensor!();

    let iterations = 50;
    let size = 50000;

    let start = Instant::now();
    for _ in 0..iterations {
        let data: Vec<f32> = vec![1.0; size];
        let t = MLTensor::from_floats(&data, &[250, 200]).unwrap();
        let scaled = t.scale(2.0).unwrap();
        let result = scaled.to_vec().unwrap();
        std::hint::black_box(result);
    }
    let elapsed = start.elapsed();

    let avg_ms = elapsed.as_millis() as f64 / iterations as f64;
    eprintln!(
        "Full pipeline (create->scale->materialize, {} elements): {:.2} ms/iter",
        size, avg_ms
    );
}

// =============================================================================
// 6. Concurrent Tensor Operations Tests
// =============================================================================

#[test]
fn test_concurrent_tensor_creation() {
    if !MLTensor::is_available() {
        eprintln!("Skipping concurrent test - MLTensor not available");
        return;
    }

    let num_threads = 4;
    let tensors_per_thread = 10;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            thread::spawn(move || {
                for i in 0..tensors_per_thread {
                    let data: Vec<f32> = vec![(thread_id * 100 + i) as f32; 100];
                    let tensor =
                        MLTensor::from_floats(&data, &[10, 10]).expect("Concurrent create failed");
                    let result = tensor.to_vec().expect("Concurrent materialize failed");
                    assert_eq!(result[0], (thread_id * 100 + i) as f32);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

#[test]
fn test_concurrent_operations() {
    if !MLTensor::is_available() {
        eprintln!("Skipping concurrent test - MLTensor not available");
        return;
    }

    let num_threads = 4;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            thread::spawn(move || {
                let data: Vec<f32> = vec![thread_id as f32; 1000];
                let tensor = MLTensor::from_floats(&data, &[100, 10]).expect("Create failed");

                // Perform multiple operations
                let scaled = tensor.scale(2.0).expect("Scale failed");
                let doubled = tensor.add(&scaled).expect("Add failed");
                let result = doubled.to_vec().expect("Materialize failed");

                // Verify: original + 2*original = 3*original
                let expected = 3.0 * thread_id as f32;
                assert!(
                    result.iter().all(|&x| approx_eq(x, expected, 1e-6)),
                    "Thread {} got wrong result",
                    thread_id
                );
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

#[test]
fn test_concurrent_softmax() {
    if !MLTensor::is_available() {
        eprintln!("Skipping concurrent test - MLTensor not available");
        return;
    }

    let num_threads = 8;

    let handles: Vec<_> = (0..num_threads)
        .map(|_| {
            thread::spawn(|| {
                let data: Vec<f32> = (0..100).map(|i| i as f32 * 0.01).collect();
                let tensor = MLTensor::from_floats(&data, &[10, 10]).expect("Create failed");

                let softmax_result = tensor.softmax(-1).expect("Softmax failed");
                let result = softmax_result.to_vec().expect("Materialize failed");

                // Each row should sum to ~1.0 (10 rows)
                for row in 0..10 {
                    let row_sum: f32 = result[row * 10..(row + 1) * 10].iter().sum();
                    assert!(
                        (row_sum - 1.0).abs() < 1e-4,
                        "Row {} sum was {}",
                        row,
                        row_sum
                    );
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

#[test]
fn test_concurrent_matmul() {
    if !MLTensor::is_available() {
        eprintln!("Skipping concurrent test - MLTensor not available");
        return;
    }

    let num_threads = 4;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            thread::spawn(move || {
                // Small matmul per thread
                let data1 = vec![1.0; 64];
                let data2 = vec![1.0; 64];

                let t1 = MLTensor::from_floats(&data1, &[8, 8]).expect("Create t1");
                let t2 = MLTensor::from_floats(&data2, &[8, 8]).expect("Create t2");

                let product = t1.matmul(&t2).expect("Matmul failed");
                let result = product.to_vec().expect("Materialize failed");

                // Each element should be 8.0 (sum of 8 ones)
                assert!(
                    result.iter().all(|&x| approx_eq(x, 8.0, 1e-5)),
                    "Thread {} matmul result incorrect",
                    thread_id
                );
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

#[test]
fn test_shared_tensor_concurrent_read() {
    if !MLTensor::is_available() {
        eprintln!("Skipping concurrent test - MLTensor not available");
        return;
    }

    let data: Vec<f32> = (0..1000).map(|i| i as f32).collect();
    let tensor = Arc::new(MLTensor::from_floats(&data, &[100, 10]).expect("Create failed"));

    let num_readers = 4;
    let handles: Vec<_> = (0..num_readers)
        .map(|_| {
            let tensor = Arc::clone(&tensor);
            thread::spawn(move || {
                let result = tensor.to_vec().expect("Read failed");
                assert_eq!(result.len(), 1000);
                assert_eq!(result[0], 0.0);
                assert_eq!(result[999], 999.0);
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

#[test]
fn test_concurrent_large_tensors() {
    if !MLTensor::is_available() {
        eprintln!("Skipping concurrent test - MLTensor not available");
        return;
    }

    let num_threads = 2; // Fewer threads for large tensors
    let size = 250_000;

    let start = Instant::now();
    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            thread::spawn(move || {
                let data: Vec<f32> = vec![1.0; size];
                let tensor = MLTensor::from_floats(&data, &[500, 500]).expect("Create failed");
                let scaled = tensor.scale(thread_id as f32 + 1.0).expect("Scale failed");
                let result = scaled.to_vec().expect("Materialize failed");

                assert_eq!(result.len(), size);
                let expected = thread_id as f32 + 1.0;
                assert!(
                    approx_eq(result[0], expected, 1e-6),
                    "Thread {} expected {}, got {}",
                    thread_id,
                    expected,
                    result[0]
                );
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    eprintln!(
        "Concurrent large tensors ({} threads, {} elements each): {:?}",
        num_threads,
        size,
        start.elapsed()
    );
}

// =============================================================================
// Stress Tests
// =============================================================================

#[test]
fn test_rapid_create_destroy() {
    if !MLTensor::is_available() {
        eprintln!("Skipping stress test - MLTensor not available");
        return;
    }

    let iterations = 1000;

    let start = Instant::now();
    for i in 0..iterations {
        let data = vec![i as f32; 100];
        let tensor = MLTensor::from_floats(&data, &[10, 10]).expect("Create failed");
        // Tensor drops here
        std::hint::black_box(tensor);
    }
    let elapsed = start.elapsed();

    eprintln!(
        "Rapid create/destroy ({} iterations): {:?}",
        iterations, elapsed
    );
}

#[test]
fn test_deep_operation_chain() {
    require_mltensor!();

    let mut tensor = MLTensor::from_floats(&[1.0, 2.0, 3.0, 4.0], &[2, 2]).unwrap();

    // Chain 20 operations
    for _ in 0..10 {
        tensor = tensor.scale(1.1).expect("Scale failed");
        tensor = tensor.scale(1.0 / 1.1).expect("Scale failed");
    }

    let result = tensor.to_vec().unwrap();

    // Should be approximately original values (with floating point error)
    assert!(approx_eq(result[0], 1.0, 0.01));
    assert!(approx_eq(result[3], 4.0, 0.01));
}

#[test]
fn test_alternating_operations() {
    require_mltensor!();

    let a = MLTensor::from_floats(&[1.0, 1.0, 1.0, 1.0], &[2, 2]).unwrap();
    let b = MLTensor::from_floats(&[1.0, 1.0, 1.0, 1.0], &[2, 2]).unwrap();

    // Alternate between add and scale
    let mut result = a.add(&b).unwrap(); // 2
    result = result.scale(2.0).unwrap(); // 4
    result = result.add(&b).unwrap(); // 5
    result = result.scale(0.5).unwrap(); // 2.5

    let output = result.to_vec().unwrap();
    assert!(output.iter().all(|&x| approx_eq(x, 2.5, 1e-6)));
}
